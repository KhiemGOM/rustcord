//! Scripted run used by the end-to-end harness (FASTCORD_E2E=<artifact dir>).
//! Everything goes through egui's normal input path, so a click here is a click.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, ViewportCommand, pos2, vec2};
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{App, side};
use crate::net::Conn;
use crate::state::Id;
use crate::voice::{State, audio};


// sidebar entries that sit low enough to need a scroll down to reach
const PAGES_BELOW: [&str; 12] = [
    "Appearance", "Accessibility", "Voice & Video", "Chat", "Notifications", "Keybinds", "Language",
    "Windows Settings", "Streamer Mode", "Advanced", "Activity Privacy", "HypeSquad",
];

pub struct Run {
    dir: PathBuf,
    t0: Instant,
    marks: Vec<(String, f64)>,
    step: u32,
    step_at: Instant,
    wait: Instant,
    queue: VecDeque<Vec<Event>>,
    frames: Vec<f32>,
    shots: Vec<String>,
    pending_shots: u32,
    voice: serde_json::Value,
    target: (Id, Id),
    call: Id,
    loaded: usize,
    text: String,
    error: Option<String>,
    done: bool,
    rects: serde_json::Map<String, serde_json::Value>,
    steady: Option<egui::Rect>,
    pub tour: bool,
    cache_guilds: Option<usize>,
    slow: Vec<serde_json::Value>,
    blur: bool,
    probes: Vec<(String, Vec<(String, Probe)>)>,
    tries: u32,
}

/// A spot to read back from a screenshot: one pixel, or the brightest pixel in a rect (text colour).
enum Probe {
    At(Pos2),
    Bright(egui::Rect),
}

impl Run {
    pub fn from_env(t0: Instant) -> Option<Run> {
        let tour = std::env::var_os("FASTCORD_TOUR").is_some();
        let dir = PathBuf::from(std::env::var_os("FASTCORD_E2E").or_else(|| std::env::var_os("FASTCORD_TOUR"))?);
        let _ = std::fs::create_dir_all(&dir);
        crate::gif::trace();
        Some(Run {
            dir,
            t0,
            marks: Vec::new(),
            step: 0,
            step_at: Instant::now(),
            wait: Instant::now(),
            queue: VecDeque::new(),
            frames: Vec::new(),
            shots: Vec::new(),
            pending_shots: 0,
            voice: serde_json::Value::Null,
            target: (0, 0),
            call: 0,
            loaded: 0,
            text: format!("hello from the e2e run **bold** 🎉 #{}", std::process::id()),
            error: None,
            done: false,
            rects: Default::default(),
            steady: None,
            tour,
            cache_guilds: None,
            slow: Vec::new(),
            blur: false,
            probes: Vec::new(),
            tries: 0,
        })
    }

    fn ms(&self) -> f64 {
        self.t0.elapsed().as_secs_f64() * 1000.0
    }

    /// How long one expensive thing took on the UI thread, for the report.
    pub fn cost(&mut self, name: &str, d: Duration) {
        self.rects.insert(format!("cost_{name}_ms"), json!((d.as_secs_f64() * 10000.0).round() / 10.0));
    }

    pub fn mark(&mut self, name: &str) {
        let t = self.ms();
        self.marks.push((name.to_string(), t));
        if name == "first_frame" {
            let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            let _ = std::fs::write(self.dir.join("first_frame_epoch_ms"), epoch.as_millis().to_string());
        }
    }

    pub fn mark_once(&mut self, name: &str) {
        if !self.marks.iter().any(|(n, _)| n == name) {
            self.mark(name);
        }
    }

    pub fn frame_time(&mut self, d: Duration) {
        let ms = d.as_secs_f32() * 1000.0;
        if ms > 40.0 {
            let at = self.ms();
            let step = self.step;
            self.slow.push(json!({ "at_ms": at.round(), "ms": ms.round(), "step": step }));
        }
        self.frames.push(ms);
    }

    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        // the harness launches us in the background; a user would be looking at the window
        raw.focused = !self.blur;
        if let Some(batch) = self.queue.pop_front() {
            raw.events.extend(batch);
        }
    }

    pub fn screenshots(&mut self, ctx: &egui::Context) {
        let shots: Vec<(String, std::sync::Arc<egui::ColorImage>)> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    Event::Screenshot { user_data, image, .. } => {
                        let name = user_data.data.as_ref()?.downcast_ref::<String>()?.clone();
                        Some((name, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        let ppp = ctx.pixels_per_point();
        for (name, img) in shots {
            if let Some(i) = self.probes.iter().position(|(n, _)| *n == name) {
                let (_, list) = self.probes.remove(i);
                let px = |p: Pos2| img.pixels.get((p.y * ppp) as usize * img.size[0] + (p.x * ppp) as usize).copied().unwrap_or_default();
                for (key, probe) in list {
                    let c = match probe {
                        Probe::At(p) => px(p),
                        Probe::Bright(r) => {
                            let (mut best, mut at) = (0u32, Pos2::ZERO);
                            for y in (r.top() * ppp) as usize..(r.bottom() * ppp) as usize {
                                for x in (r.left() * ppp) as usize..(r.right() * ppp) as usize {
                                    let c = img.pixels.get(y * img.size[0] + x).copied().unwrap_or_default();
                                    let lum = c.r() as u32 + c.g() as u32 + c.b() as u32;
                                    if lum > best {
                                        (best, at) = (lum, egui::pos2(x as f32 / ppp, y as f32 / ppp));
                                    }
                                }
                            }
                            px(at)
                        }
                    };
                    let hex = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
                    self.rects.insert(format!("look:{}:{key}", name.trim_end_matches(".png")), json!(hex));
                }
            }
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(self.dir.join(&name));
            }
            self.shots.push(name);
            self.pending_shots = self.pending_shots.saturating_sub(1);
        }
    }

    fn shot(&mut self, ctx: &egui::Context, name: &str) {
        self.pending_shots += 1;
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::new(name.to_string())));
    }

    fn click(&mut self, at: Pos2) {
        let b = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        self.queue.push_back(vec![Event::PointerMoved(at)]);
        self.queue.push_back(vec![b(true)]);
        self.queue.push_back(vec![b(false)]);
    }

    fn key(&mut self, key: Key) {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        self.queue.push_back(vec![k(true)]);
        self.queue.push_back(vec![k(false)]);
    }

    fn next(&mut self, pause_ms: u64) {
        self.step += 1;
        self.step_at = Instant::now();
        self.wait = Instant::now() + Duration::from_millis(pause_ms);
    }

    fn fail(&mut self, why: String) {
        self.error = Some(why);
        self.step = 99;
    }

    pub fn step(&mut self, app: &mut App, ctx: &egui::Context) {
        if self.done {
            return;
        }
        ctx.request_repaint();
        // layout snapshot for alignment checks against discord's metrics
        for k in ["composer", "composer_text", "gear", "search", "friends"] {
            if let Some(r) = app.rects.get(k) {
                self.rects.insert(k.into(), json!([r.min.x, r.min.y, r.width(), r.height()]));
            }
        }
        if !self.queue.is_empty() || Instant::now() < self.wait {
            return;
        }
        if self.step != 99 && self.step_at.elapsed() > Duration::from_secs(20) {
            let jump = app.scroll_to.map(|(c, id)| {
                let have = app.store.msgs.get(&c).is_some_and(|e| e.list.iter().any(|m| m.id == id));
                format!(" scroll_to={id} loaded={have} tries={}", app.scroll_tries)
            });
            let here = app.cur().and_then(|c| app.store.channels.get(&c)).map(|c| c.name.clone()).unwrap_or_default();
            self.fail(format!("timed out at step {} in #{here}{}", self.step, jump.unwrap_or_default()));
        }
        let msgs = |app: &App, ch: Id| app.store.msgs.get(&ch).map(|e| (e.list.len(), e.fresh && !e.loading)).unwrap_or((0, false));
        GUILDS.store(app.store.guilds.len(), std::sync::atomic::Ordering::Relaxed);
        if self.tour && self.step != 99 {
            return self.tour_steps(app, ctx);
        }
        let me = app.store.me;
        let heard = app.voice.speaking.iter().any(|u| *u != me);
        let sh = &app.voice.shared;
        let n = |c: &std::sync::atomic::AtomicU64| c.load(std::sync::atomic::Ordering::Relaxed);
        let mic = sh.mic.load(std::sync::atomic::Ordering::Relaxed);
        let fake_video = std::env::var_os("FASTCORD_FAKE_VIDEO").is_some();
        let mic_name = ["unknown", "none", "device", "fake"].get(mic as usize).copied().unwrap_or("?");
        self.voice = json!({
            "mic": mic_name,
            "packets_sent": n(&sh.sent), "voiced_sent": n(&sh.voiced), "packets_recv": n(&sh.recv), "frames_decoded": n(&sh.decoded),
            "e2ee_out": n(&sh.e2ee_out), "e2ee_in": n(&sh.e2ee_in),
            "video_frames_in": n(&sh.video_in), "video_frames_out": n(&sh.video_out),
        });
        match self.step {
            0 => {
                if app.conn == Conn::Online && !app.store.guilds.is_empty() {
                    let g = app.store.guild_order[0];
                    let Some(ch) = side::first_channel(&app.store, g) else {
                        return self.fail("first guild has no text channel".into());
                    };
                    self.target = (g, ch);
                    self.next(100);
                }
            }
            1 => {
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(250);
            }
            2 => {
                let Some(r) = app.rects.get(&format!("chan:{}", self.target.1)).copied() else { return };
                self.click(r.center());
                self.next(50);
            }
            3 => {
                let (n, ready) = msgs(app, self.target.1);
                if app.cur() == Some(self.target.1) && ready && n > 0 {
                    self.mark("messages");
                    self.loaded = n;
                    self.next(1500);
                }
            }
            4 => {
                self.shot(ctx, "channel.png");
                self.next(400);
            }
            5 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text(self.text.clone())]);
                self.key(Key::Enter);
                self.mark("send_start");
                self.next(0);
            }
            6 => {
                let ch = self.target.1;
                let echoed = app
                    .store
                    .msgs
                    .get(&ch)
                    .is_some_and(|e| e.list.iter().any(|m| !m.pending && m.content == self.text));
                if echoed {
                    self.mark("send_echo");
                    self.next(300);
                }
            }
            7 => {
                self.shot(ctx, "sent.png");
                let Some(r) = app.rects.get("composer").copied() else { return };
                let at = r.center() - vec2(0.0, 250.0);
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                for _ in 0..40 {
                    self.queue.push_back(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, 400.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    }]);
                }
                self.next(0);
            }
            8 => {
                let (n, ready) = msgs(app, self.target.1);
                if n > self.loaded && ready {
                    self.mark("history");
                    self.next(1000);
                } else if self.queue.is_empty() {
                    self.queue.push_back(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, 400.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    }]);
                }
            }
            9 => {
                self.shot(ctx, "history.png");
                self.next(300);
            }
            10 => {
                // dm list, so the home view is covered too
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.click(r.center());
                self.next(1200);
            }
            11 => {
                self.shot(ctx, "dms.png");
                let Some(r) = app.rects.get("friends").copied() else { return };
                self.click(r.center());
                self.next(800);
            }
            12 => {
                self.shot(ctx, "friends.png");
                let k = |pressed| Event::Key { key: Key::K, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND };
                self.queue.push_back(vec![k(true)]);
                self.queue.push_back(vec![k(false)]);
                self.queue.push_back(vec![]);
                self.queue.push_back(vec![Event::Text("help".into())]);
                self.queue.push_back(vec![]);
                self.key(Key::Enter);
                self.next(0);
            }
            13 => {
                let help = app.store.guilds.get(&self.target.0).and_then(|g| {
                    g.channels.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.name == "help"))
                });
                if help.is_some() && app.cur() == help {
                    self.mark("switcher");
                    self.next(300);
                }
            }
            14 => {
                let g = self.target.0;
                let forum = app.store.channels.values().find(|c| c.guild == g && c.is_forum()).map(|c| c.id);
                let Some(r) = forum.and_then(|f| app.rects.get(&format!("chan:{f}")).copied()) else {
                    return self.fail("no forum channel in the sidebar".into());
                };
                self.click(r.center());
                self.next(0);
            }
            15 => {
                let Some(f) = app.cur() else { return };
                if app.store.forum_posts.get(&f).is_some_and(|p| !p.is_empty()) {
                    self.mark("forum");
                    self.next(1200);
                }
            }
            16 => {
                self.shot(ctx, "forum.png");
                let g = self.target.0;
                let thread = app.store.channels.values().find(|c| c.guild == g && c.is_thread() && c.joined).map(|c| c.id);
                let Some(r) = thread.and_then(|t| app.rects.get(&format!("chan:{t}")).copied()) else {
                    return self.fail("no joined thread in the sidebar".into());
                };
                self.click(r.center());
                self.next(300);
            }
            17 => {
                let Some(t) = app.cur() else { return };
                let (n, ready) = msgs(app, t);
                if app.store.channels.get(&t).is_some_and(|c| c.is_thread()) && ready && n > 0 {
                    self.mark("thread");
                    self.next(1000);
                }
            }
            18 => {
                self.shot(ctx, "thread.png");
                let me = app.store.me;
                let other = app.store.guilds.get(&self.target.0).and_then(|g| {
                    g.list.iter().find_map(|i| match i {
                        crate::state::ListItem::Member(u) if *u != me => Some(*u),
                        _ => None,
                    })
                });
                let Some(r) = other.and_then(|u| app.rects.get(&format!("member:{u}")).copied()) else { return };
                self.loaded = other.unwrap_or(0) as usize;
                self.click(r.center());
                self.next(0);
            }
            19 => {
                if app.profiles.contains_key(&(self.loaded as Id)) {
                    self.mark("profile");
                    self.next(800);
                }
            }
            20 => {
                self.shot(ctx, "profile.png");
                self.key(Key::Escape);
                let Some(r) = app.rects.get("search").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("coffee".into())]);
                self.key(Key::Enter);
                self.next(0);
            }
            21 => {
                if app.search.as_ref().is_some_and(|s| s.results.is_some()) {
                    self.mark("search");
                    self.next(800);
                }
            }
            22 => {
                self.shot(ctx, "search.png");
                let Some(r) = app.rects.get("pins_button").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            23 => {
                if app.pins.as_ref().is_some_and(|(_, l)| l.is_some()) {
                    self.mark("pins");
                    self.next(800);
                }
            }
            24 => {
                self.shot(ctx, "pins.png");
                self.key(Key::Escape);
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            25 => {
                if app.settings_page.is_some() {
                    self.mark("settings");
                    self.shot(ctx, "settings.png");
                    self.next(300);
                }
            }
            26 => {
                self.key(Key::Escape);
                let general = app.store.guilds.get(&self.target.0).and_then(|g| {
                    g.channels.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.name == "general"))
                });
                let Some(r) = general.and_then(|c| app.rects.get(&format!("chan:{c}")).copied()) else { return };
                self.click(r.center());
                self.next(800);
            }
            27 => {
                // @-mention through the autocomplete popup
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("@al".into())]);
                self.next(400);
            }
            28 => {
                if app.ac_open {
                    self.shot(ctx, "mention_autocomplete.png");
                    self.key(Key::Tab);
                    self.queue.push_back(vec![Event::Text("ping from autocomplete".into())]);
                    self.key(Key::Enter);
                    self.mark("mention_ac");
                    self.next(600);
                }
            }
            29 => {
                self.queue.push_back(vec![Event::Text("/rem".into())]);
                self.next(600);
            }
            30 => {
                if app.ac_open {
                    self.shot(ctx, "slash_autocomplete.png");
                    self.key(Key::Tab);
                    self.queue.push_back(vec![Event::Text("water the plants".into())]);
                    self.key(Key::Enter);
                    self.next(0);
                }
            }
            31 => {
                let ch = app.cur().unwrap_or(0);
                let replied = app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| m.content.contains("Reminder set")));
                if replied {
                    self.mark("slash");
                    self.next(800);
                }
            }
            32 => {
                self.shot(ctx, "slash_reply.png");
                let Some(r) = app.rects.get("button:snooze").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            33 => {
                let ch = app.cur().unwrap_or(0);
                if app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| m.content.contains("Snoozed"))) {
                    self.mark("component");
                    // what a screenshot tool would leave on the clipboard
                    let img = image::RgbaImage::from_fn(160, 90, |x, y| image::Rgba([(x * 255 / 160) as u8, (y * 255 / 90) as u8, 200, 255]));
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_image(arboard::ImageData { width: 160, height: 90, bytes: img.into_raw().into() });
                    }
                    app.paste_files();
                    self.next(1200);
                }
            }
            34 => {
                if !app.files.is_empty() {
                    self.shot(ctx, "upload.png");
                    let Some(r) = app.rects.get("composer").copied() else { return };
                    self.click(r.center() + vec2(0.0, r.height() / 2.0 - 22.0));
                    self.queue.push_back(vec![Event::Text("pasted screenshot".into())]);
                    self.key(Key::Enter);
                    self.mark("paste");
                    self.next(1000);
                }
            }
            35 => {
                if !app.rects.contains_key("reply") {
                    // scroll up a bit at a time until a reply preview is on screen
                    if let Some(c) = app.rects.get("composer").copied() {
                        self.queue.push_back(vec![Event::PointerMoved(c.center() - vec2(0.0, 200.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, 150.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                    }
                    return;
                }
                // let the smooth scroll settle before aiming
                self.next(600);
            }
            36 => {
                let Some(r) = app.rects.get("reply").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            37 => {
                if app.flash.is_some() {
                    self.mark("jump");
                    self.next(300);
                }
            }
            38 => {
                self.shot(ctx, "jump.png");
                self.next(200);
            }
            39 => {
                if !app.rects.contains_key("image") {
                    if let Some(c) = app.rects.get("composer").copied() {
                        self.queue.push_back(vec![Event::PointerMoved(c.center() - vec2(0.0, 200.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, 150.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                    }
                    return;
                }
                self.next(600);
            }
            40 => {
                let Some(r) = app.rects.get("image").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            41 => {
                if app.viewer.is_some() {
                    self.mark("viewer");
                    self.next(1200);
                }
            }
            42 => {
                self.shot(ctx, "viewer.png");
                self.key(Key::Escape);
                if app.rects.keys().any(|k| k.starts_with("dmbubble:")) {
                    self.mark("dm_bubble");
                }
                self.loaded = app.cur().unwrap_or(0) as usize;
                let k = |pressed| Event::Key { key: Key::ArrowDown, physical_key: None, pressed, repeat: false, modifiers: Modifiers::ALT };
                self.queue.push_back(vec![k(true)]);
                self.queue.push_back(vec![k(false)]);
                self.next(300);
            }
            43 => {
                if app.cur().is_some_and(|c| c as usize != self.loaded) {
                    self.mark("alt_nav");
                    let k = |pressed| Event::Key { key: Key::ArrowUp, physical_key: None, pressed, repeat: false, modifiers: Modifiers::ALT };
                    self.queue.push_back(vec![k(true)]);
                    self.queue.push_back(vec![k(false)]);
                    self.next(600);
                }
            }
            44 => {
                if app.cur() != Some(self.loaded as Id) {
                    return;
                }
                let Some(r) = app.rects.get("gif_button").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            45 => {
                if app.gifs.as_ref().is_some_and(|g| g.results.as_ref().is_some_and(|l| !l.is_empty())) {
                    self.next(1000);
                }
            }
            46 => {
                self.shot(ctx, "gifs.png");
                let Some(r) = app.rects.get("gif:0").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            47 => {
                let ch = app.cur().unwrap_or(0);
                if app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| !m.pending && m.content.contains("tenor.com/view/mock"))) {
                    self.mark("gif");
                    self.next(600);
                }
            }
            48 => {
                // reply through the hover toolbar with the ping turned off
                let Some(r) = app.rects.get("msg:last").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(400);
            }
            49 => {
                let Some(r) = app.rects.get("bar_reply").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            50 => {
                let Some(r) = app.rects.get("ping_toggle").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            51 => {
                if app.reply_ping {
                    return;
                }
                self.shot(ctx, "reply_ping_off.png");
                self.queue.push_back(vec![Event::Text("quiet reply".into())]);
                let se = |pressed| Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::SHIFT };
                self.queue.push_back(vec![se(true)]);
                self.queue.push_back(vec![se(false)]);
                self.queue.push_back(vec![Event::Text("second line".into())]);
                self.key(Key::Enter);
                self.mark("reply_quiet");
                self.next(0);
            }
            52 => {
                let ch = app.cur().unwrap_or(0);
                if app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| !m.pending && m.content.contains("second line"))) {
                    self.next(500);
                }
            }
            53 => {
                self.shot(ctx, "multiline.png");
                self.next(300);
            }
            54 => {
                let Some(vc) = side::tree(&app.store, self.target.0).into_iter().flat_map(|(_, k)| k).find(|c| c.is_voice()).map(|c| c.id) else {
                    return self.fail("no voice channel".into());
                };
                let Some(r) = app.rects.get(&format!("chan:{vc}")).copied() else { return };
                self.call = vc;
                self.click(r.center());
                self.mark("voice_click");
                self.next(0);
            }
            55 => {
                if app.voice.state == State::Connected && app.voice.chan == Some(self.call) {
                    self.mark("voice_connected");
                    self.next(0);
                }
            }
            56 => {
                // with the fake mic we also want to have sent real speech, not just the join silence
                let fake = mic == audio::MIC_FAKE;
                if heard && n(&sh.decoded) > 10 && (!fake || n(&sh.voiced) > 10) {
                    self.mark("voice_heard");
                    self.next(0);
                }
            }
            57 => {
                if heard {
                    self.shot(ctx, "voice.png");
                    self.next(300);
                }
            }
            58 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            59 => {
                let Some(r) = app.rects.get("settings:Voice & Video").copied() else { return };
                self.click(r.center());
                self.next(1500);
            }
            60 => {
                self.shot(ctx, "settings_voice.png");
                self.next(300);
            }
            61 => {
                self.key(Key::Escape);
                self.next(400);
            }
            62 => {
                if app.voice.video.keys().any(|u| *u != me) {
                    self.mark("video_in");
                    self.next(0);
                }
            }
            63 => {
                let Some(r) = app.rects.get("voice:camera").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            64 => {
                if app.voice.camera && app.voice.video.contains_key(&me) && n(&sh.video_out) > 10 {
                    self.mark("camera_on");
                    self.next(0);
                }
            }
            65 => {
                let Some(u) = app.voice.live.iter().copied().find(|u| *u != me) else { return };
                let Some(r) = app.rects.get(&format!("voice:watch:{u}")).copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            66 => {
                if app.voice.stream.as_ref().is_some_and(|l| !l.mine && l.state == State::Connected && l.frame.is_some()) {
                    self.mark("stream_in");
                    self.next(500);
                }
            }
            67 => {
                // real captures (warm run) stay out of the screenshots
                if fake_video {
                    self.shot(ctx, "video.png");
                }
                self.next(300);
            }
            68 => {
                let Some(r) = app.rects.get("voice:stopstream").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            69 => {
                if app.voice.stream.is_some() {
                    return;
                }
                let Some(r) = app.rects.get("voice:screen").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            70 => {
                if app.voice.stream.as_ref().is_some_and(|l| l.mine && l.state == State::Connected && l.frame.is_some()) {
                    self.mark("stream_out");
                    self.next(1500);
                }
            }
            71 => {
                if fake_video {
                    self.shot(ctx, "golive.png");
                }
                self.next(300);
            }
            72 => {
                let Some(r) = app.rects.get("voice:stopstream").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            73 => {
                if app.voice.stream.is_some() {
                    return;
                }
                let Some(r) = app.rects.get("voice:camera").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            74 => {
                if !app.voice.camera {
                    self.next(0);
                }
            }
            75 => {
                let Some(r) = app.rects.get("voice:mute").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            76 => {
                if !app.voice.mute {
                    return self.fail("mute button did nothing".into());
                }
                self.shot(ctx, "voice_muted.png");
                let Some(r) = app.rects.get("voice:mute").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            77 => {
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            78 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.mark("voice_left");
                    self.next(200);
                }
            }
            79 => {
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.click(r.center());
                self.next(1200);
            }
            80 => {
                let Some(dm) = app.store.dms.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.kind == 1)) else {
                    return self.fail("no dm".into());
                };
                let Some(r) = app.rects.get(&format!("chan:{dm}")).copied() else { return };
                self.call = dm;
                self.click(r.center());
                self.next(800);
            }
            81 => {
                let Some(r) = app.rects.get("dm:call").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            82 => {
                if app.voice.state == State::Connected && app.voice.chan == Some(self.call) && heard {
                    self.mark("dm_call");
                    self.next(500);
                }
            }
            83 => {
                if heard {
                    self.shot(ctx, "dm_call.png");
                    self.next(300);
                }
            }
            84 => {
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            85 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.mark("dm_left");
                    self.loaded = 0;
                    self.step = 100;
                }
            }
            100..=199 => self.settings_steps(app, ctx),
            200..=219 => self.media_steps(app, ctx),
            220..=239 => self.side_steps(app, ctx),
            240..=259 => self.layout_steps(app, ctx),
            260..=279 => self.title_steps(app, ctx),
            280..=299 => self.look_steps(app, ctx),
            330..=349 => self.polish_steps(app, ctx),
            _ => {
                if self.pending_shots == 0 {
                    self.finish(ctx);
                }
            }
        }
    }

    /// Click a settings sidebar entry, scrolling the sidebar first if it's below the fold.
    fn open_page(&mut self, app: &App, label: &str) -> bool {
        if let Some(r) = app.rects.get(&format!("settings:{label}")).copied() {
            // only aim once the sidebar has stopped scrolling
            if self.steady == Some(r) {
                self.steady = None;
                self.click(r.center());
                return true;
            }
            self.steady = Some(r);
            return false;
        }
        let dir = if PAGES_BELOW.contains(&label) { -200.0 } else { 200.0 };
        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(150.0, 400.0))]);
        self.queue.push_back(vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, dir),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        }]);
        false
    }

    fn at(ctx: &egui::Context, key: &str) -> Option<egui::Rect> {
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("e2e", key))))
    }

    /// Settings: a screenshot of every page, then a few real round trips.
    fn settings_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const PAGES: [&str; 24] = [
            "My Account", "Profiles", "Content & Social", "Data & Privacy", "Family Center", "Authorized Apps", "Devices",
            "Connections", "Nitro", "Subscriptions", "Gift Inventory", "Billing", "Appearance", "Accessibility",
            "Voice & Video", "Chat", "Notifications", "Keybinds", "Language", "Windows Settings", "Streamer Mode",
            "Advanced", "Activity Privacy", "HypeSquad",
        ];
        match self.step {
            100 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            101 => {
                let Some(label) = PAGES.get(self.loaded) else {
                    self.mark("settings_tour");
                    self.step = 103;
                    return;
                };
                if !self.open_page(app, label) {
                    return;
                }
                self.next(900);
            }
            102 => {
                if app.settings_page.map(crate::ui::settings::label) != Some(PAGES[self.loaded]) {
                    return self.fail(format!("clicked {} but another page is open", PAGES[self.loaded]));
                }
                let slug: String = PAGES[self.loaded].to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                self.shot(ctx, &format!("settings_{slug}.png"));
                self.loaded += 1;
                self.step = 101;
                self.wait = Instant::now() + Duration::from_millis(200);
            }
            103 => {
                if !self.open_page(app, "Profiles") {
                    return;
                }
                self.next(900);
            }
            104 => {
                let Some(r) = Self::at(ctx, "profile:pronouns") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("they/them".into())]);
                self.next(500);
            }
            105 => {
                let Some(r) = Self::at(ctx, "save_changes") else { return };
                self.shot(ctx, "settings_unsaved.png");
                self.click(r.center());
                self.next(0);
            }
            106 => {
                let saved = app.sform.profile_base.as_ref().is_some_and(|_| app.sform.profile == app.sform.profile_base) && !app.sform.saving;
                if saved {
                    self.mark("profile_saved");
                    self.next(300);
                }
            }
            107 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(600);
            }
            108 => {
                let Some(r) = app.rects.get("theme:Light").copied() else { return };
                self.click(r.center());
                self.next(700);
            }
            109 => {
                if app.settings.theme == crate::theme::Theme::Light {
                    self.mark("theme_light");
                    self.shot(ctx, "settings_light.png");
                    self.key(Key::Escape);
                    self.next(900);
                }
            }
            110 => {
                self.shot(ctx, "chat_light.png");
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            111 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(600);
            }
            112 => {
                let Some(r) = app.rects.get("theme:Ash").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            113 => {
                let Some(r) = Self::at(ctx, "radio:Compact") else { return };
                self.click(r.center());
                self.next(500);
            }
            114 => {
                if app.settings.compact {
                    self.mark("compact");
                    self.key(Key::Escape);
                    self.next(1000);
                }
            }
            115 => {
                self.shot(ctx, "chat_compact.png");
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            116 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(600);
            }
            117 => {
                let Some(r) = Self::at(ctx, "radio:Cozy") else { return };
                self.click(r.center());
                self.next(300);
            }
            118 => {
                if !self.open_page(app, "Streamer Mode") {
                    return;
                }
                self.next(600);
            }
            119 => {
                let Some(r) = Self::at(ctx, "toggle:Enable Streamer Mode") else { return };
                self.click(r.center());
                self.next(600);
            }
            120 => {
                if app.settings.streamer {
                    self.mark("streamer");
                    self.shot(ctx, "settings_streamer_on.png");
                    let Some(r) = Self::at(ctx, "toggle:Enable Streamer Mode") else { return };
                    self.click(r.center());
                    self.next(400);
                }
            }
            121 => {
                if !self.open_page(app, "Content & Social") {
                    return;
                }
                self.next(900);
            }
            122 => {
                let Some(r) = Self::at(ctx, "radio:Keep me safe") else { return };
                self.click(r.center());
                self.next(500);
            }
            123 => {
                if !self.open_page(app, "Language") {
                    return;
                }
                self.next(600);
            }
            124 => {
                let Some(r) = Self::at(ctx, "lang:de") else { return };
                self.click(r.center());
                self.next(400);
            }
            125 => {
                if !self.open_page(app, "HypeSquad") {
                    return;
                }
                self.next(600);
            }
            126 => {
                let Some(r) = Self::at(ctx, "house:Bravery") else { return };
                self.click(r.center());
                self.next(600);
            }
            127 => {
                self.mark("hypesquad");
                if !self.open_page(app, "Language") {
                    return;
                }
                self.next(600);
            }
            128 => {
                let Some(r) = Self::at(ctx, "lang:en-US") else { return };
                self.click(r.center());
                self.next(300);
            }
            129 => {
                if !self.open_page(app, "My Account") {
                    return;
                }
                self.next(600);
            }
            130 => {
                let Some(r) = Self::at(ctx, "edit:DISPLAY NAME") else { return };
                self.click(r.center());
                self.next(500);
            }
            131 => {
                let Some(r) = Self::at(ctx, "modal_input") else { return };
                self.click(r.right_center() - vec2(4.0, 0.0));
                self.queue.push_back(vec![Event::Text("!".into())]);
                self.next(400);
            }
            132 => {
                let Some(r) = Self::at(ctx, "modal_done") else { return };
                self.shot(ctx, "settings_modal.png");
                self.click(r.center());
                self.next(0);
            }
            133 => {
                if app.sform.modal.is_none() && app.store.user(app.store.me).and_then(|u| u.global_name.as_deref()) == Some("You!") {
                    self.mark("display_name");
                    self.key(Key::Escape);
                    self.step = 280;
                }
            }
            _ => self.step = 99,
        }
    }

    /// How many pixels inside `r` differ between two screenshots.
    fn changed(&self, a: &str, b: &str, r: egui::Rect, ppp: f32) -> usize {
        let (Ok(a), Ok(b)) = (image::open(self.dir.join(a)), image::open(self.dir.join(b))) else { return 0 };
        let (a, b) = (a.to_rgba8(), b.to_rgba8());
        let (x0, y0, x1, y1) = ((r.left() * ppp) as u32, (r.top() * ppp) as u32, (r.right() * ppp) as u32, (r.bottom() * ppp) as u32);
        let mut n = 0;
        for y in y0..y1.min(a.height()).min(b.height()) {
            for x in x0..x1.min(a.width()).min(b.width()) {
                n += (a.get_pixel(x, y) != b.get_pixel(x, y)) as usize;
            }
        }
        n
    }

    /// Animated emoji and gifs, media embeds.
    fn media_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const MEDIA: Id = 2060;
        const EMOJI: &str = "emojis/5100";
        // every url shape that used to draw as a still: query string, mp4, plain gif
        const SOURCES: [&str; 4] = [EMOJI, "spin.mp4", "wave.gif", "dance.gif"];
        let gif_rect = |app: &App, k: &str| app.rects.iter().find(|(n, _)| n.starts_with(k)).map(|(_, r)| *r);
        match self.step {
            200 => {
                // the dm call left us in the home view
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            201 => {
                let Some(r) = app.rects.get(&format!("chan:{MEDIA}")).copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            202 => {
                let ready = app.store.msgs.get(&MEDIA).is_some_and(|e| e.list.len() >= 5 && e.fresh && !e.loading);
                if app.cur() == Some(MEDIA) && ready {
                    self.mark("media_channel");
                    self.next(300);
                }
            }
            203 => {
                let n: Vec<usize> = SOURCES.iter().map(|s| crate::gif::shown(s)).collect();
                self.rects.insert("media_frames_seen".into(), json!(n));
                if n.iter().all(|f| *f >= 3) {
                    self.mark("media_animates");
                    self.next(0);
                } else if self.step_at.elapsed() > Duration::from_secs(15) {
                    self.fail(format!("animations never advanced, distinct frames shown per source {SOURCES:?}: {n:?}"));
                }
            }
            204 => {
                let (Some(g), Some(i)) = (gif_rect(app, "gifv:"), app.rects.get("image").copied()) else {
                    return self.fail("gifv embed or gif attachment was not drawn".into());
                };
                self.rects.insert("media_gifv".into(), json!([g.min.x, g.min.y, g.width(), g.height()]));
                self.rects.insert("media_gif_attachment".into(), json!([i.min.x, i.min.y, i.width(), i.height()]));
                self.shot(ctx, "media_t0.png");
                self.next(0);
            }
            205 => {
                if self.pending_shots == 0 {
                    self.next(150);
                }
            }
            206 => {
                self.shot(ctx, "media_t1.png");
                self.next(0);
            }
            207 => {
                if self.pending_shots > 0 {
                    return;
                }
                let (Some(g), Some(i)) = (gif_rect(app, "gifv:"), app.rects.get("image").copied()) else { return };
                let ppp = ctx.pixels_per_point();
                let (a, b) = (self.changed("media_t0.png", "media_t1.png", g, ppp), self.changed("media_t0.png", "media_t1.png", i, ppp));
                self.rects.insert("media_pixels_changed".into(), json!({ "gifv": a, "gif_attachment": b }));
                if a > 0 && b > 0 {
                    self.mark("media_pixels_change");
                }
                self.shot(ctx, "media.png");
                self.blur = true;
                self.next(700);
            }
            208 => {
                let still: Vec<usize> = SOURCES.iter().map(|s| crate::gif::current(s)).collect();
                if still.iter().any(|f| *f != 0) {
                    return self.fail(format!("animations kept running in an unfocused window: {still:?}"));
                }
                self.mark("media_pauses_unfocused");
                self.blur = false;
                self.next(0);
            }
            209 => {
                if crate::gif::current(EMOJI) != 0 && crate::gif::current("spin.mp4") != 0 {
                    self.mark("media_resumes_focused");
                    app.settings.gif_autoplay = false;
                    self.next(800);
                } else if self.step_at.elapsed() > Duration::from_secs(3) {
                    self.fail("animations did not resume when the window got focus back".into());
                }
            }
            210 => {
                // autoplay off: gifs and clips hold their first frame, emoji have their own switch
                let (g, v) = (crate::gif::current("dance.gif"), crate::gif::current("spin.mp4"));
                if g != 0 || v != 0 {
                    return self.fail(format!("gif autoplay off but dance.gif is on frame {g} and spin.mp4 on {v}"));
                }
                self.mark("media_autoplay_off");
                let Some(i) = app.rects.get("image").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(i.center())]);
                self.next(0);
            }
            211 => {
                if crate::gif::current("dance.gif") != 0 {
                    self.mark("media_hover_plays");
                    app.settings.gif_autoplay = true;
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(2.0, 2.0))]);
                    self.next(300);
                } else if self.step_at.elapsed() > Duration::from_secs(3) {
                    self.fail("hovering a gif with autoplay off did not play it".into());
                }
            }
            _ => self.step = 220,
        }
    }

    /// Server rail, channel list, member list, user panel.
    fn side_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let rect = |app: &App, k: &str| app.rects.get(k).copied();
        match self.step {
            220 => {
                if app.ui.open_folders.contains(&78) {
                    return self.next(0);
                }
                let Some(r) = rect(app, "folder:78") else { return };
                self.click(r.center());
                self.next(400);
            }
            221 => {
                let Some(r) = rect(app, "guild:1003") else { return };
                self.click(r.center());
                self.next(600);
            }
            222 => {
                if app.ui.guild != Some(1003) || rect(app, "chan:2303").is_none() {
                    return;
                }
                self.snapshot_side(app);
                for g in 1003..=1008 {
                    let (unread, _) = app.store.guild_badge(g);
                    self.rects.insert(format!("side_unread_{g}"), json!(unread));
                }
                for c in [2302, 2303, 2309, 2310] {
                    if let Some(ch) = app.store.channels.get(&c) {
                        self.rects.insert(format!("side_locked_{c}"), json!(app.store.locked(ch)));
                    }
                }
                self.shot(ctx, "side_channels.png");
                self.next(300);
            }
            223 => {
                let Some(r) = rect(app, &format!("guild:{}", self.target.0)) else { return };
                self.click(r.center());
                self.next(600);
            }
            224 => {
                // the member list is hidden while a voice channel is open
                if app.ui.guild != Some(self.target.0) {
                    return;
                }
                let Some(r) = rect(app, &format!("chan:{}", self.target.1)) else { return };
                self.click(r.center());
                self.next(600);
            }
            225 => {
                if rect(app, "member:101").is_none() {
                    return;
                }
                self.snapshot_side(app);
                self.shot(ctx, "side_members.png");
                self.next(300);
            }
            _ => {
                self.mark("side");
                self.step = 240;
            }
        }
    }

    fn snapshot_side(&mut self, app: &App) {
        const KEYS: [&str; 8] = ["guildicon:", "voicebadge:", "folderbg:", "folder:", "chanemoji:", "chanicon:", "chan:", "member:"];
        for (k, r) in &app.rects {
            if KEYS.iter().any(|p| k.starts_with(p)) || k == "userpanel" {
                self.rects.insert(k.clone(), json!([r.min.x, r.min.y, r.width(), r.height()]));
            }
        }
    }

    /// Dragging panel edges.
    fn layout_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let chat_w = |app: &App| app.rects.get("composer").map(|r| r.width());
        let side_w = |app: &App| if app.ui.side_w > 0.0 { app.ui.side_w } else { 240.0 };
        let members_w = |app: &App| if app.ui.members_w > 0.0 { app.ui.members_w } else { 240.0 };
        let drag = |run: &mut Self, from: Pos2, dx: f32| {
            run.queue.push_back(vec![Event::PointerMoved(from)]);
            run.queue.push_back(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
            for i in 1..=6 {
                run.queue.push_back(vec![Event::PointerMoved(from + vec2(dx * i as f32 / 6.0, 0.0))]);
            }
            let to = from + vec2(dx, 0.0);
            run.queue.push_back(vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
        };
        match self.step {
            240 => {
                app.settings_page = None;
                app.friends = false;
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            241 => {
                let Some(r) = app.rects.get(&format!("chan:{}", self.target.1)).copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            242 => {
                let Some(w) = chat_w(app) else { return };
                if app.cur() != Some(self.target.1) {
                    return;
                }
                self.rects.insert("layout_side_start".into(), json!(side_w(app)));
                self.rects.insert("layout_members_start".into(), json!(members_w(app)));
                self.rects.insert("layout_chat_start".into(), json!(w));
                self.shot(ctx, "layout_before.png");
                self.next(300);
            }
            243 => {
                drag(self, pos2(72.0 + side_w(app), 400.0), 60.0);
                self.next(400);
            }
            244 => {
                self.rects.insert("layout_side_dragged".into(), json!(side_w(app)));
                self.rects.insert("layout_chat_after_side".into(), json!(chat_w(app).unwrap_or(0.0)));
                // a few px inside the panel: the message list's scrollbar sits on the other side of the edge and would take the press
                let edge = ctx.content_rect().right() - members_w(app) + 3.0;
                drag(self, pos2(edge, 400.0), -40.0);
                self.next(400);
            }
            245 => {
                self.rects.insert("layout_members_dragged".into(), json!(members_w(app)));
                self.rects.insert("layout_chat_after_members".into(), json!(chat_w(app).unwrap_or(0.0)));
                self.shot(ctx, "layout_after.png");
                self.next(300);
            }
            246 => {
                // drag the side edge far past its max; the chat column has to survive it
                drag(self, pos2(72.0 + side_w(app), 400.0), 2000.0);
                self.next(400);
            }
            247 => {
                self.rects.insert("layout_side_max".into(), json!(side_w(app)));
                self.rects.insert("layout_chat_min".into(), json!(chat_w(app).unwrap_or(0.0)));
                self.shot(ctx, "layout_max.png");
                let back = app.ui.side_w;
                drag(self, pos2(72.0 + back, 400.0), self.rects["layout_side_dragged"].as_f64().unwrap_or(0.0) as f32 - back);
                self.next(400);
            }
            248 => {
                self.rects.insert("layout_side_end".into(), json!(side_w(app)));
                // the save is debounced by a second
                self.next(2000);
            }
            _ => self.step = 330,
        }
    }

    /// Leftovers: channel header marks, voice panel, palette sweep.
    fn polish_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let rect = |app: &App, k: &str| app.rects.get(k).copied();
        let save = |me: &mut Self, app: &App, k: &str| {
            if let Some(r) = rect(app, k) {
                me.rects.insert(format!("polish:{k}"), json!([r.min.x, r.min.y, r.width(), r.height()]));
            }
        };
        match self.step {
            330 => {
                let Some(r) = rect(app, "guild:1003") else { return };
                self.click(r.center());
                self.next(600);
            }
            331 | 333 => {
                let chan = if self.step == 331 { 2302 } else { 2303 };
                if app.ui.guild != Some(1003) {
                    return;
                }
                let Some(r) = rect(app, &format!("chan:{chan}")) else { return };
                self.click(r.center());
                self.next(800);
            }
            332 | 334 => {
                let want = if self.step == 332 { 2302 } else { 2303 };
                let Some(icon) = rect(app, "header:icon").filter(|_| app.cur() == Some(want)) else { return };
                // the corner of the glyph where the lock sits when a channel is private
                let shot = if want == 2302 { "polish_private.png" } else { "polish_public.png" };
                self.probes.push((shot.into(), vec![("lock".into(), Probe::At(icon.min + vec2(22.2, 9.0)))]));
                save(self, app, "header:icon");
                self.shot(ctx, shot);
                self.next(400);
            }
335 => {
                let Some(r) = rect(app, "chan:2310") else { return };
                self.click(r.center());
                self.next(600);
            }
336 => {
                let (Some(cam), Some(_)) = (rect(app, "voice:panel_camera"), rect(app, "voice:panel_screen")) else { return };
                if app.voice.state != State::Connected {
                    return;
                }
                for k in ["voice:panel_camera", "voice:panel_screen", "voice:disconnect", "userpanel"] {
                    save(self, app, k);
                }
                let probes = vec![
                    ("gap".into(), Probe::At(cam.left_top() + vec2(10.0, -6.0))),
                    ("button".into(), Probe::At(cam.left_top() + vec2(6.0, 16.0))),
                ];
                self.probes.push(("polish_voice.png".into(), probes));
                self.shot(ctx, "polish_voice.png");
                self.next(400);
            }
337 => {
                let Some(r) = rect(app, "voice:disconnect") else { return };
                self.click(r.center());
                self.next(0);
            }
338 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.next(200);
                }
            }
339 => {
                let Some(r) = rect(app, &format!("guild:{}", self.target.0)) else { return };
                self.click(r.center());
                self.next(600);
            }
340 => {
                let Some(r) = rect(app, &format!("chan:{}", self.target.1)) else { return };
                self.click(r.center());
                self.next(1200);
            }
341 => {
                // the code message sits a few above the newest; the scroll from look_steps may not have settled
                let Some(code) = rect(app, "codeblock") else {
                    if self.tries < 20 {
                        self.tries += 1;
                        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(640.0, 400.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, -300.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                        self.wait = Instant::now() + Duration::from_millis(250);
                    }
                    return;
                };
                let probes = vec![
                    ("fill".into(), Probe::At(code.min + vec2(4.0, 4.0))),
                    ("edge".into(), Probe::At(code.left_center() + vec2(0.4, 0.0))),
                    ("chat".into(), Probe::At(code.left_center() - vec2(4.0, 0.0))),
                ];
                self.probes.push(("polish_code.png".into(), probes));
                save(self, app, "codeblock");
                self.shot(ctx, "polish_code.png");
                self.mark("polish");
                self.next(400);
            }
            _ => self.step = 99,
        }
    }

    /// Read-only walk through a real account (FASTCORD_TOUR + FASTCORD_READONLY):
    /// look at things, screenshot them, change nothing.
    fn tour_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const PAGES: [&str; 24] = [
            "My Account", "Profiles", "Content & Social", "Data & Privacy", "Family Center", "Authorized Apps", "Devices",
            "Connections", "Nitro", "Subscriptions", "Gift Inventory", "Billing", "Appearance", "Accessibility",
            "Voice & Video", "Chat", "Notifications", "Keybinds", "Language", "Windows Settings", "Streamer Mode",
            "Advanced", "Activity Privacy", "HypeSquad",
        ];
        if self.cache_guilds.is_none() {
            // the snapshot written by the previous build, before READY replaces it
            self.cache_guilds = Some(app.store.guilds.len());
            self.mark("cache_checked");
        }
        let cur_msgs = |app: &App| app.cur().and_then(|c| app.store.msgs.get(&c)).map(|e| (e.list.len(), e.fresh && !e.loading)).unwrap_or((0, false));
        let skip_after = |s: &Self, secs: u64| s.step_at.elapsed() > Duration::from_secs(secs);
        if self.step == 0 {
            self.step = 300;
        }
        match self.step {
            300 => {
                if app.conn == Conn::Online && !app.store.guilds.is_empty() {
                    self.mark("ready");
                    self.next(500);
                }
            }
            301 => {
                let pick = app.store.guild_order.iter().copied().find(|g| side::first_channel(&app.store, *g).is_some());
                let Some(g) = pick else { return self.fail("no server with a readable text channel".into()) };
                self.target = (g, 0);
                match app.rects.get(&format!("guild:{g}")).copied() {
                    Some(r) => self.click(r.center()),
                    None => app.select_guild(Some(g)),
                }
                self.next(500);
            }
            302 => {
                // whatever was open last time may be a forum or voice channel; tour a text one
                let text = app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.is_text() && !c.is_thread());
                if !text {
                    let first = side::first_channel(&app.store, self.target.0);
                    if let Some(r) = first.and_then(|c| app.rects.get(&format!("chan:{c}")).copied()) {
                        self.click(r.center());
                        self.wait = Instant::now() + Duration::from_millis(500);
                    }
                    return;
                }
                let (n, ready) = cur_msgs(app);
                if ready && n > 0 {
                    self.mark("channel_loaded");
                    self.next(3000);
                }
            }
            303 => {
                self.shot(ctx, "01_server_channel.png");
                self.next(300);
            }
            304 => {
                let me = app.store.me;
                let member = app.rects.keys().find_map(|k| k.strip_prefix("member:").and_then(|u| u.parse::<Id>().ok()).filter(|u| *u != me));
                match member.and_then(|u| app.rects.get(&format!("member:{u}")).copied().map(|r| (u, r))) {
                    Some((u, r)) => {
                        self.loaded = u as usize;
                        self.click(r.center());
                        self.next(0);
                    }
                    None if skip_after(self, 6) => self.step = 307,
                    None => {}
                }
            }
            305 => {
                if app.profiles.contains_key(&(self.loaded as Id)) || skip_after(self, 6) {
                    self.next(1500);
                }
            }
            306 => {
                self.shot(ctx, "02_profile_popout.png");
                self.key(Key::Escape);
                self.next(400);
            }
            307 => {
                let k = |pressed| Event::Key { key: Key::K, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND };
                self.queue.push_back(vec![k(true)]);
                self.queue.push_back(vec![k(false)]);
                self.next(700);
            }
            308 => {
                self.shot(ctx, "03_quick_switcher.png");
                self.key(Key::Escape);
                self.next(400);
            }
            309 => {
                let k = |pressed| Event::Key { key: Key::E, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND };
                self.queue.push_back(vec![k(true)]);
                self.queue.push_back(vec![k(false)]);
                self.next(2000);
            }
            310 => {
                self.shot(ctx, "04_emoji_picker.png");
                self.key(Key::Escape);
                self.next(400);
            }
            311 => {
                let Some(r) = app.rects.get("gif_button").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            312 => {
                if app.gifs.as_ref().is_some_and(|g| g.results.is_some()) || skip_after(self, 8) {
                    self.next(2500);
                }
            }
            313 => {
                self.shot(ctx, "05_gif_picker.png");
                self.key(Key::Escape);
                self.next(400);
            }
            314 => {
                let Some(r) = app.rects.get("pins_button").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            315 => {
                if app.pins.as_ref().is_some_and(|(_, l)| l.is_some()) || skip_after(self, 8) {
                    self.next(1500);
                }
            }
            316 => {
                self.shot(ctx, "06_pins.png");
                self.key(Key::Escape);
                self.next(400);
            }
            317 => {
                let Some(r) = app.rects.get("search").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("the".into())]);
                self.key(Key::Enter);
                self.next(0);
            }
            318 => {
                if app.search.as_ref().is_some_and(|s| s.results.is_some()) || skip_after(self, 10) {
                    self.next(2000);
                }
            }
            319 => {
                self.shot(ctx, "07_search.png");
                self.next(600);
            }
            320 => {
                app.search = None;
                // a forum and a joined thread, if this server has them
                let g = self.target.0;
                let forum = app.store.channels.values().find(|c| c.guild == g && c.is_forum() && app.store.can_view(c)).map(|c| c.id);
                match forum.and_then(|f| app.rects.get(&format!("chan:{f}")).copied()) {
                    Some(r) => {
                        self.click(r.center());
                        self.next(3000);
                    }
                    None => self.step = 322,
                }
            }
            321 => {
                self.shot(ctx, "08_forum.png");
                self.next(300);
            }
            322 => {
                let g = self.target.0;
                let thread = app.store.channels.values().find(|c| c.guild == g && c.is_thread() && c.joined && !c.archived).map(|c| c.id);
                match thread.and_then(|t| app.rects.get(&format!("chan:{t}")).copied()) {
                    Some(r) => {
                        self.click(r.center());
                        self.next(3000);
                    }
                    None => self.step = 324,
                }
            }
            323 => {
                self.shot(ctx, "09_thread.png");
                self.next(300);
            }
            324 => {
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            325 => {
                let Some(r) = app.rects.get("friends").copied() else { return };
                self.click(r.center());
                self.next(2000);
            }
            326 => {
                self.shot(ctx, "10_friends.png");
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.loaded = 0;
                self.next(800);
            }
            327 => {
                let Some(label) = PAGES.get(self.loaded) else {
                    self.mark("settings_tour");
                    self.step = 329;
                    return;
                };
                if self.open_page(app, label) {
                    self.next(2500);
                }
            }
            328 => {
                if app.settings_page.map(crate::ui::settings::label) != Some(PAGES[self.loaded]) {
                    return self.fail(format!("clicked {} but another page is open", PAGES[self.loaded]));
                }
                let slug: String = PAGES[self.loaded].to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                self.shot(ctx, &format!("settings_{:02}_{slug}.png", self.loaded + 1));
                self.loaded += 1;
                self.step = 327;
                self.wait = Instant::now() + Duration::from_millis(200);
            }
            329 => {
                self.key(Key::Escape);
                self.step = 99;
            }
            _ => self.step = 99,
        }
    }

    fn finish(&mut self, ctx: &egui::Context) {
        self.done = true;
        let mut f = self.frames.clone();
        f.sort_by(|a, b| a.total_cmp(b));
        let pct = |p: f64| f.get(((f.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
        let marks: serde_json::Map<String, serde_json::Value> =
            self.marks.iter().map(|(k, v)| (k.clone(), json!((v * 10.0).round() / 10.0))).collect();
        let out = json!({
            "ok": self.error.is_none(),
            "error": self.error,
            "marks_ms": marks,
            "frame_cpu_ms": { "count": f.len(), "p50": pct(0.5), "p95": pct(0.95), "p99": pct(0.99), "max": pct(1.0) },
            "screenshots": self.shots,
            "voice": self.voice,
            "rects": self.rects,
            "cache_guilds": self.cache_guilds,
            "slow_frames": self.slow,
            "guilds_after_ready": app_guilds(),
            "blocked_by_readonly": crate::net::BLOCKED.load(std::sync::atomic::Ordering::Relaxed),
        });
        let _ = std::fs::write(self.dir.join("results.json"), serde_json::to_string_pretty(&out).unwrap_or_default());
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Title bar: back/forward, inbox, window buttons.
    fn title_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let help = app.store.channels.values().find(|c| c.guild == self.target.0 && c.name == "help").map(|c| c.id).unwrap_or(0);
        let here = app.cur();
        let rect = |app: &App, k: &str| app.rects.get(&format!("title:{k}")).copied();
        let press = |at: Pos2, button| {
            [true, false].map(|pressed| vec![Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE }])
        };
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        match self.step {
            260 => {
                if app.settings_page.is_some() {
                    return;
                }
                let names = ["bar", "back", "forward", "name", "inbox", "help", "min", "max", "close"];
                if let Some(k) = names.iter().find(|k| rect(app, k).is_none()) {
                    return self.fail(format!("title bar has no {k}"));
                }
                let bar = rect(app, "bar").unwrap_or(egui::Rect::NOTHING);
                self.rects.insert("title_bar".into(), json!([bar.min.x, bar.min.y, bar.width(), bar.height()]));
                self.rects.insert("edge_hits".into(), json!(crate::winsys::probe_edges()));
                self.mark("title_bar");
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(250);
            }
            261 => {
                let Some(r) = app.rects.get(&format!("chan:{general}")).copied() else { return };
                self.click(r.center());
                self.next(100);
            }
            262 => {
                if here != Some(general) {
                    return;
                }
                let Some(r) = app.rects.get(&format!("chan:{help}")).copied() else { return };
                self.click(r.center());
                self.next(100);
            }
            263 => {
                if here != Some(help) {
                    return;
                }
                let Some(r) = rect(app, "back") else { return };
                self.click(r.center());
                self.next(100);
            }
            264 => {
                if here != Some(general) {
                    return;
                }
                let Some(r) = rect(app, "forward") else { return };
                self.click(r.center());
                self.next(100);
            }
            265 => {
                if here != Some(help) {
                    return;
                }
                self.mark("nav_buttons");
                let key = |pressed| Event::Key { key: Key::ArrowLeft, physical_key: None, pressed, repeat: false, modifiers: Modifiers::ALT };
                self.queue.push_back(vec![key(true)]);
                self.queue.push_back(vec![key(false)]);
                self.next(100);
            }
            266 => {
                if here != Some(general) {
                    return;
                }
                // the forward button on the side of a mouse
                let at = rect(app, "bar").map(|r| r.center()).unwrap_or_default();
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                self.queue.extend(press(at, PointerButton::Extra2));
                self.next(100);
            }
            267 => {
                if here != Some(help) {
                    return;
                }
                self.mark("nav_keys");
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.click(r.center());
                self.next(800);
            }
            268 => {
                let Some(r) = app.rects.get("friends").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            269 => {
                if !app.friends {
                    return;
                }
                let Some(r) = rect(app, "back") else { return };
                self.click(r.center());
                self.next(200);
            }
            270 => {
                if app.friends || app.ui.guild.is_some() {
                    return;
                }
                let Some(r) = rect(app, "forward") else { return };
                self.click(r.center());
                self.next(200);
            }
            271 => {
                if !app.friends {
                    return;
                }
                self.mark("nav_friends");
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            272 => {
                if here != Some(help) || app.friends {
                    return;
                }
                let Some(r) = rect(app, "inbox") else { return };
                self.click(r.center());
                self.next(700);
            }
            273 => {
                let listed = app.rects.keys().filter(|k| k.starts_with("inbox:mention:")).count();
                if app.inbox.as_ref().is_none_or(|i| i.mentions.is_none()) || listed < 2 {
                    return;
                }
                self.mark("inbox_listed");
                self.shot(ctx, "inbox.png");
                let Some(r) = app.rects.get("inbox:unreads").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            274 => {
                self.shot(ctx, "inbox_unreads.png");
                let Some(r) = app.rects.get("inbox:mentions").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            275 => {
                // the oldest one, which isn't anything an earlier step sent
                let id = |k: &str| k.trim_start_matches("inbox:mention:").parse::<u64>().unwrap_or(u64::MAX);
                let oldest = app.rects.iter().filter(|(k, _)| k.starts_with("inbox:mention:")).min_by_key(|(k, _)| id(k));
                let Some((_, r)) = oldest else { return };
                self.click(r.center());
                self.next(0);
            }
            276 => {
                if here != Some(general) || app.inbox.is_some() || app.scroll_to.is_some() {
                    return;
                }
                self.mark("inbox_jump");
                self.shot(ctx, "titlebar.png");
                let Some(r) = rect(app, "close") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(800);
            }
            277 => {
                self.shot(ctx, "titlebar_close.png");
                self.next(400);
            }
            278 => {
                if self.loaded != 278 {
                    let Some(bar) = rect(app, "bar") else { return };
                    let empty = bar.left_center() + vec2(220.0, 0.0);
                    self.queue.push_back(vec![Event::PointerMoved(empty)]);
                    // double click on the empty bar
                    self.queue.extend(press(empty, PointerButton::Primary));
                    self.queue.extend(press(empty, PointerButton::Primary));
                    self.loaded = 278;
                    return;
                }
                if !maximized {
                    return;
                }
                self.mark("maximize");
                self.shot(ctx, "maximized.png");
                self.next(600);
            }
            279 => {
                let Some(r) = rect(app, "max") else { return };
                if maximized {
                    self.click(r.center());
                    self.next(600);
                    self.step = 279;
                } else {
                    self.mark("restore");
                    self.step = 200;
                }
            }
            _ => self.step = 200,
        }
    }

    /// Theme colours, fonts, message headers.
    fn look_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::theme::Theme;
        // ash goes last so everything after this runs on the default theme
        const THEMES: [(&str, Theme); 4] = [("Onyx", Theme::Onyx), ("Dark", Theme::Dark), ("Light", Theme::Light), ("Ash", Theme::Ash)];
        let (name, theme) = THEMES[self.loaded.min(3)];
        match self.step {
            280 => {
                self.loaded = 0;
                let Some(r) = app.rects.get(&format!("guild:{}", self.target.0)).copied() else { return };
                self.click(r.center());
                self.next(250);
            }
            281 => {
                let Some(r) = app.rects.get(&format!("chan:{}", self.target.1)).copied() else { return };
                self.click(r.center());
                self.next(50);
            }
            282 => {
                let ready = app.store.msgs.get(&self.target.1).is_some_and(|e| e.fresh && !e.loading && !e.list.is_empty());
                if app.cur() == Some(self.target.1) && ready {
                    self.next(1500);
                }
            }
            283 => {
                // the download lands in the background on a cold start, so give it a moment
                let first = |fam: egui::FontFamily| ctx.fonts(|f| f.definitions().families.get(&fam).and_then(|v| v.first().cloned()));
                let prop = first(egui::FontFamily::Proportional);
                if prop.as_deref() == Some("gg400") || self.step_at.elapsed() > Duration::from_secs(12) {
                    self.rects.insert("look:font".into(), json!(prop));
                    self.rects.insert("look:font_medium".into(), json!(first(egui::FontFamily::Name("medium".into()))));
                    self.next(0);
                }
            }
            284 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            285 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(500);
            }
            286 => {
                let Some(r) = app.rects.get(&format!("theme:{name}")).copied() else { return };
                self.click(r.center());
                self.next(700);
            }
            287 => {
                if app.settings.theme == theme {
                    self.key(Key::Escape);
                    self.next(900);
                }
            }
            288 => {
                // the seeded gradient authors sit above everything the earlier steps sent
                let seen = |w: u64| app.rects.contains_key(&format!("name:{w}"));
                if !(seen(103) && seen(105) && seen(106)) && self.tries < 14 {
                    self.tries += 1;
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(640.0, 400.0))]);
                    self.queue.push_back(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, 150.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    }]);
                    self.wait = Instant::now() + Duration::from_millis(250);
                    return;
                }
                let Some(r) = app.rects.get("msg:last").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.left_center() + vec2(6.0, 0.0))]);
                self.next(500);
            }
            289 => {
                let Some(r) = app.rects.get("msg:last").copied() else { return };
                let shot = format!("look_{name}_hover.png");
                self.probes.push((shot.clone(), vec![("msg_hover".into(), Probe::At(r.left_center() + vec2(6.0, 0.0)))]));
                self.shot(ctx, &shot);
                self.next(400);
            }
            290 => {
                // off every message, so nothing is hovered or animating in the next shot
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(30.0, 300.0))]);
                self.next(500);
            }
            291 => {
                let (Some(row), Some(composer), Some(search)) =
                    (app.rects.get("msg:last").copied(), app.rects.get("composer").copied(), app.rects.get("search").copied())
                else {
                    return;
                };
                let screen = ctx.content_rect();
                let y = row.center().y;
                let mut probes = vec![
                    ("rail".to_string(), Probe::At(egui::pos2(2.0, y))),
                    ("side".into(), Probe::At(egui::pos2(row.left() - 4.0, y))),
                    ("chat".into(), Probe::At(egui::pos2(row.left() + 8.0, y))),
                    ("members".into(), Probe::At(egui::pos2(screen.right() - 2.0, y))),
                    ("header".into(), Probe::At(egui::pos2(row.left() + 6.0, search.center().y))),
                    ("input".into(), Probe::At(egui::pos2(composer.left() + 3.0, composer.center().y))),
                ];
                // role gradients, display styles and role icons, by author id (see the mock's roles)
                for who in [103u64, 105, 106] {
                    if let Some(n) = app.rects.get(&format!("name:{who}")).copied() {
                        let part = n.width() * 0.3;
                        probes.push((format!("name{who}_l"), Probe::Bright(egui::Rect::from_min_size(n.min, vec2(part, n.height())))));
                        probes.push((format!("name{who}_r"), Probe::Bright(egui::Rect::from_min_size(n.right_top() - vec2(part, 0.0), vec2(part, n.height())))));
                        probes.push((format!("halo{who}_near"), Probe::At(n.left_center() - vec2(3.0, 0.0))));
                        probes.push((format!("halo{who}_far"), Probe::At(n.left_center() - vec2(10.0, 0.0))));
                    }
                    if let Some(i) = app.rects.get(&format!("role_icon:{who}")).copied() {
                        probes.push((format!("icon{who}"), Probe::Bright(i)));
                    }
                }
                for (k, r) in app.rects.iter().filter(|(k, _)| ["name:", "stamp:", "avatar:", "role_icon:"].iter().any(|p| k.starts_with(p))) {
                    self.rects.insert(format!("look:{name}:{k}"), json!([r.min.x, r.min.y, r.width(), r.height()]));
                }
                let shot = format!("look_{name}.png");
                self.probes.push((shot.clone(), probes));
                self.shot(ctx, &shot);
                self.next(400);
            }
            292 => {
                self.loaded += 1;
                self.step = if self.loaded < THEMES.len() { 284 } else { 293 };
                self.wait = Instant::now() + Duration::from_millis(200);
            }
            _ => {
                // back to the newest messages for whoever runs next
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(640.0, 400.0))]);
                for _ in 0..40 {
                    self.queue.push_back(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, -400.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    }]);
                }
                self.mark("look");
                self.step = 260;
            }
        }
    }
}

static GUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn app_guilds() -> usize {
    GUILDS.load(std::sync::atomic::Ordering::Relaxed)
}
