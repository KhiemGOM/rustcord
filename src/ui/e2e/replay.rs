//! RUSTCORD_REPLAY=<journeys.json>, with RUSTCORD_TOUR=<dir> and RUSTCORD_READONLY: inputs recorded
//! in Discord's own client, repeated here on the same account. After each one, everything drawn is
//! written to <id>.<n>.scene.json beside a screenshot; e2e/discord_pair.py makes the journeys and
//! sets the scenes against what Discord showed.

use std::time::{Duration, Instant};

use egui::{Color32, Event, Key, Modifiers, MouseWheelUnit, Pos2, Rect, Shape, TouchPhase, pos2, vec2};
use serde::Deserialize;
use serde_json::{Value, json};

use super::Run;
use crate::net::Conn;
use crate::ui::App;

#[derive(Deserialize)]
struct Journey {
    id: String,
    guild: Option<String>,
    channel: Option<String>,
    steps: Vec<Step>,
}

#[derive(Clone, Deserialize)]
struct Step {
    r#do: String,
    at: Option<[f32; 2]>,
    rect: Option<String>,
    text: Option<String>,
    key: Option<String>,
    value: Option<String>,
    delta: Option<f32>,
}

#[derive(Default)]
pub(super) struct State {
    journeys: Vec<Journey>,
    at: usize,
    step: usize,
    phase: u8,
    how: Vec<Value>,
    wheeled: u8,
}

fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

fn xywh(r: Rect) -> Value {
    json!([r.left(), r.top(), r.width(), r.height()])
}

fn collect(shape: &Shape, clip: Rect, z: usize, texts: &mut Vec<Value>, boxes: &mut Vec<Value>) {
    let seen = |r: Rect| r.intersects(clip) && r.width() >= 1.0 && r.height() >= 1.0;
    match shape {
        Shape::Vec(list) => list.iter().for_each(|s| collect(s, clip, z, texts, boxes)),
        Shape::Text(t) => {
            let r = t.galley.rect.translate(t.pos.to_vec2());
            let text = t.galley.text();
            if !seen(r) || text.trim().is_empty() {
                return;
            }
            let fmt = t.galley.job.sections.first().map(|s| &s.format);
            let col = t.override_text_color.or(fmt.map(|f| f.color).filter(|c| *c != Color32::PLACEHOLDER)).unwrap_or(t.fallback_color);
            let mixed = t.galley.job.sections.windows(2).any(|w| w[0].format.font_id != w[1].format.font_id || w[0].format.color != w[1].format.color);
            texts.push(json!({
                "t": text, "b": xywh(r), "clip": xywh(clip), "z": z, "rows": t.galley.rows.len(), "mixed": mixed,
                "size": fmt.map(|f| f.font_id.size), "font": fmt.map(|f| f.font_id.family.to_string()), "col": hex(col.gamma_multiply(t.opacity_factor)),
            }));
        }
        Shape::Rect(r) if seen(r.rect) && (r.fill.a() > 0 || r.stroke.width > 0.0 || r.brush.is_some()) => {
            boxes.push(json!({
                "b": xywh(r.rect), "clip": xywh(clip), "z": z, "fill": hex(r.fill), "radius": r.corner_radius.nw, "img": r.brush.is_some(),
                "stroke": (r.stroke.width > 0.0).then(|| json!([r.stroke.width, hex(r.stroke.color)])),
            }));
        }
        // avatars, emoji and icons are textured meshes; the font atlas is egui's own texture
        Shape::Mesh(m) if m.texture_id != egui::TextureId::default() && seen(m.calc_bounds()) => {
            boxes.push(json!({ "b": xywh(m.calc_bounds()), "clip": xywh(clip), "z": z, "fill": "#ffffffff", "img": true }));
        }
        Shape::Circle(c) if c.fill.a() > 0 || c.stroke.width > 0.0 => {
            let r = Rect::from_center_size(c.center, vec2(c.radius * 2.0, c.radius * 2.0));
            if seen(r) {
                boxes.push(json!({ "b": xywh(r), "clip": xywh(clip), "z": z, "fill": hex(c.fill), "round": true }));
            }
        }
        _ => {}
    }
}

fn scene(app: &App, ctx: &egui::Context) -> Value {
    let screen = ctx.content_rect();
    let (mut texts, mut boxes) = (Vec::new(), Vec::new());
    let layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    ctx.graphics(|g| {
        for (z, layer) in layers.iter().enumerate() {
            for c in g.get(*layer).into_iter().flat_map(|list| list.all_entries()) {
                collect(&c.shape, c.clip_rect.intersect(screen), z, &mut texts, &mut boxes);
            }
        }
    });
    let marks: serde_json::Map<String, Value> = app.rects.iter().filter(|(_, r)| screen.intersects(**r)).map(|(k, r)| (k.clone(), xywh(*r))).collect();
    json!({
        "screen": [screen.width(), screen.height()], "ppp": ctx.pixels_per_point(),
        "guild": app.ui.guild.map(|g| g.to_string()), "channel": app.cur().map(|c| c.to_string()),
        "layered": app.layered(ctx), "modal": app.modal_open(),
        "texts": texts, "boxes": boxes, "marks": marks,
    })
}

/// What a window just opened shows: nothing over it, the first tab. Discord's side of each journey
/// starts from a page load, and Escape alone leaves whatever the last journey switched.
fn fresh(app: &mut App, ctx: &egui::Context) {
    macro_rules! shut {
        ($($field:ident),*) => { $(app.$field = None;)* };
    }
    shut!(
        viewer, wizard, guild_modal, profile, picker, stickers, gifs, menu, user_menu, user_dialog, confirm, forward, invite, made_invite, status_edit, share,
        switcher, pins, inbox, guild_notifs, new_thread, poll, launcher, follow, modify, group_pick, topic, crop, status_text, leaving, source_pop, create,
        active_threads, server, chan_settings, events, combos, breathe, demon, timeout, kickban, prune, settings_page, search, reactors, report, full_profile,
        tag_pop, role_pop
    );
    egui::Popup::close_all(ctx);
    app.friends_tab = 0;
}

/// A replayed click can land on something that opens a native picker, which holds the UI thread
/// until it is answered. Nobody is there to, so every dialog this process owns is closed.
fn close_native_dialogs() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
    unsafe extern "system" fn each(h: HWND, _: LPARAM) -> i32 {
        let (mut pid, mut class) = (0u32, [0u16; 16]);
        unsafe {
            GetWindowThreadProcessId(h, &mut pid);
            let n = GetClassNameW(h, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            if pid == std::process::id() && String::from_utf16_lossy(&class[..n]) == "#32770" {
                PostMessageW(h, WM_CLOSE, 0, 0);
            }
        }
        1
    }
    let _ = std::thread::Builder::new().name("replay-dialogs".into()).spawn(|| {
        loop {
            unsafe { EnumWindows(Some(each), 0) };
            std::thread::sleep(Duration::from_millis(300));
        }
    });
}

fn keys(combo: &str) -> Option<(Key, Modifiers)> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in combo.split('+') {
        match part {
            "Control" => mods.ctrl = true,
            "Shift" => mods.shift = true,
            "Alt" => mods.alt = true,
            name => key = Key::from_name(name).or_else(|| Key::from_name(&name.to_uppercase())),
        }
    }
    mods.command = mods.ctrl;
    Some((key?, mods))
}

impl Run {
    fn spot(&self, app: &App, ctx: &egui::Context, step: &Step) -> Option<(Pos2, &'static str)> {
        let screen = ctx.content_rect();
        let named = step.rect.as_ref().and_then(|k| match k.strip_prefix("msg:").and_then(|id| id.parse::<u64>().ok()) {
            Some(id) => ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("row", id)))),
            None => app.rects.get(k).copied(),
        });
        // a message's row scrolled under the header or the composer isn't there to be clicked
        let message = step.rect.as_deref().is_some_and(|k| k.starts_with("msg:"));
        let shown = if message { screen.shrink2(vec2(0.0, crate::ui::titlebar::HEIGHT + 48.0)) } else { screen };
        if let Some(r) = named.filter(|r| shown.contains(r.center())) {
            return Some((r.center(), "rect"));
        }
        let at = step.at.map(|[x, y]| pos2(x, y));
        if let Some(want) = step.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let drawn = scene(app, ctx);
            let near = |b: &Value| {
                let n = |i: usize| b[i].as_f64().unwrap_or(0.0) as f32;
                let c = pos2(n(0) + n(2) / 2.0, n(1) + n(3) / 2.0);
                (c, at.map_or(0.0, |a| a.distance(c)))
            };
            let hit = drawn["texts"].as_array().into_iter().flatten().filter(|t| t["t"].as_str().is_some_and(|s| s.trim() == want)).map(|t| near(&t["b"])).min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((c, _)) = hit {
                return Some((c, "text"));
            }
        }
        at.map(|a| (a, "position"))
    }

    fn act(&mut self, app: &App, ctx: &egui::Context, step: &Step) -> Value {
        let place = self.spot(app, ctx, step);
        let said = |how: &str, at: Option<Pos2>| json!({ "do": step.r#do, "how": how, "at": at.map(|p| [p.x, p.y]) });
        match (step.r#do.as_str(), place) {
            ("click", Some((at, how))) => {
                self.click(at);
                said(how, Some(at))
            }
            ("rclick", Some((at, how))) => {
                self.rclick(at);
                said(how, Some(at))
            }
            ("hover", Some((at, how))) => {
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                said(how, Some(at))
            }
            ("wheel", Some((at, how))) => {
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                self.queue.push_back(vec![Event::MouseWheel { unit: MouseWheelUnit::Point, delta: vec2(0.0, -step.delta.unwrap_or(0.0)), phase: TouchPhase::Move, modifiers: Modifiers::NONE }]);
                said(how, Some(at))
            }
            ("key", _) => match step.key.as_deref().and_then(keys) {
                Some((key, modifiers)) => {
                    let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
                    self.queue.push_back(vec![k(true)]);
                    self.queue.push_back(vec![k(false)]);
                    said("key", None)
                }
                None => said("unknown key", None),
            },
            ("text", _) => {
                self.queue.push_back(vec![Event::Text(step.value.clone().unwrap_or_default())]);
                said("text", None)
            }
            _ => said("nowhere to aim", None),
        }
    }

    fn scene_shot(&mut self, app: &App, ctx: &egui::Context, n: usize) {
        let name = format!("{}.{n}", self.replay.journeys[self.replay.at].id);
        let _ = std::fs::write(self.dir.join(format!("{name}.scene.json")), scene(app, ctx).to_string());
        self.shot(ctx, &format!("{name}.png"));
    }

    pub(super) fn replay_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        if std::env::var_os("RUSTCORD_READONLY").is_none() {
            return self.fail("replay requires RUSTCORD_READONLY".into());
        }
        self.step_at = Instant::now();
        let pause = |s: &mut Self, ms: u64| s.wait = Instant::now() + Duration::from_millis(ms);
        if self.step == 0 {
            let raw = std::env::var_os("RUSTCORD_REPLAY").and_then(|p| std::fs::read(p).ok());
            match raw.as_deref().map(serde_json::from_slice::<Vec<Journey>>) {
                Some(Ok(list)) => self.replay.journeys = list,
                _ => return self.fail("replay: journeys file missing or malformed".into()),
            }
            self.tour_setup(app, ctx);
            close_native_dialogs();
            self.step = 1;
        }
        if app.conn != Conn::Online || app.store.guilds.is_empty() || self.pending_shots > 0 {
            return;
        }
        let Some(j) = self.replay.journeys.get(self.replay.at) else {
            self.step = 99;
            return;
        };
        match self.replay.phase {
            0 => {
                fresh(app, ctx);
                let id = |s: &Option<String>| s.as_ref().and_then(|s| s.parse::<u64>().ok());
                let (guild, channel) = (id(&j.guild), id(&j.channel));
                if channel.is_some_and(|c| !app.store.channels.contains_key(&c)) || guild.is_some_and(|g| !app.store.guilds.contains_key(&g)) {
                    let line = json!({ "id": j.id, "error": "route is not in the store" });
                    self.replay.how.push(line);
                    self.replay.at += 1;
                    return;
                }
                self.queue.push_back(vec![Event::PointerGone]);
                app.select_guild(guild);
                match channel {
                    Some(c) => app.open(c),
                    None if guild.is_none() => {
                        app.friends = true;
                        app.quests = None;
                        app.shop = None;
                    }
                    None => {}
                }
                self.replay.step = 0;
                self.replay.phase = 1;
                self.loaded = 0;
                pause(self, 300);
            }
            1 => {
                let ready = app.cur().and_then(|c| app.store.msgs.get(&c)).is_none_or(|e| e.fresh && !e.loading);
                self.loaded += 1;
                if ready || self.loaded > 20 {
                    self.replay.phase = 2;
                    return pause(self, 1500);
                }
                pause(self, 300);
            }
            2 => {
                self.scene_shot(app, ctx, 0);
                self.replay.how.push(json!({ "id": self.replay.journeys[self.replay.at].id, "steps": [] }));
                self.replay.phase = 3;
                pause(self, 100);
            }
            3 => {
                let Some(step) = j.steps.get(self.replay.step).cloned() else {
                    let _ = std::fs::write(self.dir.join("replay.json"), Value::from(self.replay.how.clone()).to_string());
                    self.replay.at += 1;
                    self.replay.phase = 0;
                    return;
                };
                // the channel list keeps its own scroll, so the row may have to be brought into view
                let row = step.rect.as_deref().filter(|k| (k.starts_with("chan:") || k.starts_with("cat:")) && app.rects.contains_key(*k));
                if let Some(key) = row
                    && self.replay.wheeled < 8
                    && self.reach_row(app, key).is_none()
                {
                    self.replay.wheeled += 1;
                    return;
                }
                self.replay.wheeled = 0;
                let hover = step.r#do == "hover";
                let said = self.act(app, ctx, &step);
                if let Some(Value::Array(steps)) = self.replay.how.last_mut().map(|l| &mut l["steps"]) {
                    steps.push(said);
                }
                self.replay.phase = 4;
                pause(self, if hover { 900 } else { 1100 });
            }
            _ => {
                self.replay.step += 1;
                self.scene_shot(app, ctx, self.replay.step);
                self.replay.phase = 3;
                pause(self, 100);
            }
        }
    }
}
