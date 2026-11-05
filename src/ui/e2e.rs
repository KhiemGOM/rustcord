//! Scripted run used by the end-to-end harness (RUSTCORD_E2E=<artifact dir>).
//! Everything goes through egui's normal input path, so a click here is a click.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, ViewportCommand, pos2, vec2};
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod login;

use super::{App, side};
use crate::net::Conn;
use crate::state::Id;
use crate::voice::{State, audio};


// sidebar entries that sit low enough to need a scroll down to reach
const PAGES_BELOW: [&str; 10] = [
    "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time", "Activity Privacy", "Connected Apps", "Developer",
    "Streamer Mode", "Log Out",
];

// the settings sidebar top to bottom
const SIDEBAR: [&str; 18] = [
    "Account", "Data & Privacy", "Messaging Permissions", "Notifications", "Nitro", "Server Boost", "Subscriptions", "Gift Inventory",
    "Billing", "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time", "Activity Privacy", "Connected Apps",
    "Developer", "Log Out",
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
    // RUSTCORD_BENCH: nothing scripted, the frame counts written down for e2e/bench.py
    bench: Option<Instant>,
    spin: bool,
    causes: std::collections::HashMap<String, u32>,
    cache_guilds: Option<usize>,
    slow: Vec<serde_json::Value>,
    blur: bool,
    probes: Vec<(String, Vec<(String, Probe)>)>,
    tries: u32,
    // links the app asked to open in the browser
    pub opened: Vec<String>,
    // channels whose messages raised a notification
    pub pinged: Vec<Id>,
    // the connection popout: 0 not opened yet, 1 open and waiting on pings, 2 checked
    rtc: u8,
    // the last click into the share picker, so a click the dialog missed gets another go
    share_at: Option<Instant>,
    // pings so far and dm 3000's last message when do not disturb went on
    dnd: Option<(usize, Id)>,
    // the toasts those would have been, and the message each opens
    pub pings: Vec<(String, (Id, Id))>,
    // toasts so far and the title's count when the window was minimized for a ping
    hidden: Option<(usize, i32)>,
    // what Speak Message would have read out
    pub said: Vec<String>,
    // stream dispatches and voice errors, for the live check
    pub heard: Vec<serde_json::Value>,
    // the chat's scroll offset when a step began, for steps that scroll it
    y0: f32,
    // shift and alt held down, as the script would have them
    shift: bool,
    alt: bool,
    // RUSTCORD_LIVE: the real voice channel to go live in, when it began, and a sample a second
    live: Option<Id>,
    live_at: Option<Instant>,
    samples: Vec<serde_json::Value>,
}

/// A spot to read back from a screenshot: one pixel, or the brightest pixel in a rect (text colour).
enum Probe {
    At(Pos2),
    Bright(egui::Rect),
}

impl Run {
    pub fn from_env(t0: Instant) -> Option<Run> {
        let tour = std::env::var_os("RUSTCORD_TOUR").is_some();
        let bench = std::env::var_os("RUSTCORD_BENCH").is_some();
        let dir = PathBuf::from(std::env::var_os("RUSTCORD_E2E").or_else(|| std::env::var_os("RUSTCORD_TOUR")).or_else(|| std::env::var_os("RUSTCORD_BENCH"))?);
        let _ = std::fs::create_dir_all(&dir);
        crate::gif::trace();
        Some(Run {
            dir,
            t0,
            marks: Vec::new(),
            step: if std::env::var_os("RUSTCORD_E2E_LOGIN").is_some() { 1500 } else { 0 },
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
            bench: bench.then(Instant::now),
            spin: false,
            causes: Default::default(),
            cache_guilds: None,
            slow: Vec::new(),
            blur: false,
            probes: Vec::new(),
            tries: 0,
            opened: Vec::new(),
            pinged: Vec::new(),
            dnd: None,
            hidden: None,
            rtc: 0,
            share_at: None,
            pings: Vec::new(),
            said: Vec::new(),
            heard: Vec::new(),
            y0: 0.0,
            shift: false,
            alt: false,
            live: std::env::var("RUSTCORD_LIVE").ok().and_then(|v| v.parse().ok()),
            live_at: None,
            samples: Vec::new(),
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

    /// Called from App::logic while the window is minimized, when step() isn't.
    pub fn hidden(&mut self, app: &App, ctx: &egui::Context) {
        if self.step == 94 && !crate::winsys::shown() {
            if !self.marks.iter().any(|(n, _)| n == "tray_close") {
                self.mark("tray_close");
                crate::tray::send("open");
            }
            return;
        }
        let Some((pings, title)) = self.hidden else { return };
        let n = app.title_count.unwrap_or(0) as i32;
        let left = Duration::from_secs(8).saturating_sub(self.step_at.elapsed());
        if self.pings.len() > pings && n > title && crate::winsys::badge() == n {
            self.mark("minimized_ping");
            if crate::tray::current().unread {
                self.mark("tray_unread");
            }
        } else if !left.is_zero() {
            ctx.request_repaint_after(left);
            return;
        }
        self.hidden = None;
        crate::winsys::show_quietly(false);
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
        // only the script types and points; the real keyboard and mouse would wander in when
        // whoever's at the machine keeps working while the window is up
        raw.events.retain(|e| {
            !matches!(
                e,
                Event::Key { .. }
                    | Event::Text(_)
                    | Event::Ime(_)
                    | Event::Paste(_)
                    | Event::PointerMoved(_)
                    | Event::MouseMoved(_)
                    | Event::PointerButton { .. }
                    | Event::PointerGone
                    | Event::MouseWheel { .. }
                    | Event::ModifiersChanged(_)
                    | Event::Zoom(_)
            )
        });
        if self.bench.is_none() {
            raw.events.push(Event::ModifiersChanged(Modifiers { shift: self.shift, alt: self.alt, ..Default::default() }));
        }
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

    fn rclick(&mut self, at: Pos2) {
        let b = |pressed| Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE };
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
        // where the script is, for when a run never gets as far as writing its results
        let _ = std::fs::write(self.dir.join("progress"), format!("{} at {:.0}ms", self.step, self.ms()));
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
        if let Some(at) = self.bench {
            for c in ctx.repaint_causes() {
                *self.causes.entry(format!("{}:{} {}", c.file.rsplit(['/', '\\']).next().unwrap_or(c.file), c.line, c.reason)).or_default() += 1;
            }
            if self.spin {
                ctx.request_repaint();
            }
            if at.elapsed() > Duration::from_millis(100) {
                self.bench = Some(Instant::now());
                self.spin = self.dir.join("spin").exists();
                let marks: serde_json::Map<String, serde_json::Value> = self.marks.iter().map(|(k, v)| (k.clone(), json!(v.round()))).collect();
                let out = json!({ "frames": self.frames.len(), "ui_ms": self.frames.iter().sum::<f32>(), "marks_ms": marks, "slow_frames": self.slow, "causes": self.causes });
                let _ = std::fs::write(self.dir.join("bench.json"), out.to_string());
            }
            return;
        }
        // something asked the window to close mid-script; say where instead of vanishing
        if ctx.input(|i| i.viewport().close_requested()) {
            let at = self.step;
            self.fail(format!("window closed at step {at}"));
            self.finish(ctx);
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
            // whatever was open, which is what usually holds a step up
            let open: Vec<&str> = [
                ("viewer", app.viewer.is_some()), ("profile", app.profile.is_some()), ("picker", app.picker.is_some()), ("stickers", app.stickers.is_some()),
                ("gifs", app.gifs.is_some()), ("menu", app.menu.is_some()), ("user_menu", app.user_menu.is_some()), ("confirm", app.confirm.is_some()),
                ("forward", app.forward.is_some()), ("switcher", app.switcher.is_some()), ("pins", app.pins.is_some()), ("inbox", app.inbox.is_some()),
                ("emoji_pop", app.emoji_pop.is_some()), ("leaving", app.leaving.is_some()), ("new_thread", app.new_thread.is_some()), ("combos", app.combos.is_some()),
                ("settings", app.settings_page.is_some()), ("egui_popup", egui::Popup::is_any_open(ctx)), ("reply", app.reply.is_some()),
                ("editing", app.editing.is_some()), ("focus_msg", app.focus_msg.is_some()), ("files", !app.files.is_empty()),
            ]
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(k, _)| k)
            .collect();
            // where the chat was scrolled to, for steps that scroll looking for something
            let scroll = app.cur().map(|c| {
                let e = app.store.msgs.get(&c);
                format!(
                    " offset={:.0} msgs={} at_start={} loading={}",
                    app.offsets.get(&c).copied().unwrap_or(-1.0),
                    e.map_or(0, |e| e.list.len()),
                    e.is_some_and(|e| e.at_start),
                    e.is_some_and(|e| e.loading)
                )
            });
            let files: Vec<&str> = app.files.iter().map(|f| f.name.as_str()).collect();
            let shown = ["msg:last", "composer", "msg:other"].into_iter().filter(|k| app.rects.contains_key(*k)).collect::<Vec<_>>();
            self.shot(ctx, "timed_out.png");
            self.fail(format!("timed out at step {} in #{here}{}{} open={open:?} files={files:?} shown={shown:?}", self.step, jump.unwrap_or_default(), scroll.unwrap_or_default()));
        }
        let msgs = |app: &App, ch: Id| app.store.msgs.get(&ch).map(|e| (e.list.len(), e.fresh && !e.loading)).unwrap_or((0, false));
        GUILDS.store(app.store.guilds.len(), std::sync::atomic::Ordering::Relaxed);
        if let Some(ch) = self.live.filter(|_| self.step != 99) {
            return self.live_steps(app, ctx, ch);
        }
        if self.tour && self.step != 99 {
            return self.tour_steps(app, ctx);
        }
        let me = app.store.me;
        let heard = app.voice.speaking.iter().any(|u| *u != me);
        let sh = &app.voice.shared;
        let n = |c: &std::sync::atomic::AtomicU64| c.load(std::sync::atomic::Ordering::Relaxed);
        let mic = sh.mic.load(std::sync::atomic::Ordering::Relaxed);
        let fake_video = std::env::var_os("RUSTCORD_FAKE_VIDEO").is_some();
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
                if app.rects.contains_key("pins_dot") {
                    self.mark("pins_dot");
                }
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
                if !app.rects.contains_key("pins_dot") {
                    self.mark("pins_dot_gone");
                }
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
                        // the middle of the message list; a tall composer (an upload still attached) makes "above the composer" unreliable
                        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(c.center().x, (c.top() + 90.0) / 2.0))]);
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
                // the start of the line: the hover toolbar covers the message's top right
                self.click(r.left_center() + vec2(40.0_f32.min(r.width() / 2.0), 0.0));
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
                        // the middle of the message list; a tall composer (an upload still attached) makes "above the composer" unreliable
                        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(c.center().x, (c.top() + 90.0) / 2.0))]);
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
                // the wheel scroll glides on, and can carry the image back out below the fold
                let Some(r) = app.rects.get("image").copied() else {
                    if let Some(c) = app.rects.get("composer").copied() {
                        // the middle of the message list; a tall composer (an upload still attached) makes "above the composer" unreliable
                        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(c.center().x, (c.top() + 90.0) / 2.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, -60.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                        self.wait = Instant::now() + Duration::from_millis(300);
                    }
                    return;
                };
                if self.steady != Some(r) {
                    self.steady = Some(r);
                    return;
                }
                self.steady = None;
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
                // the muted dm got its message but neither pinged nor popped up in the rail
                if app.store.mentions(3002) > 0 && !app.rects.contains_key("dmbubble:3002") && !self.pinged.contains(&3002) && self.pinged.contains(&3000) {
                    self.mark("muted_dm");
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
                } else if let Some(r) = app.rects.remove("gifcat:trending") {
                    // discord opens on its search terms; trending is the first tile
                    self.click(r.center());
                    self.wait = Instant::now() + Duration::from_millis(800);
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
                // reply through the hover toolbar with the ping turned off; your own messages have no switch
                let Some(r) = app.rects.get("msg:other").copied() else { return };
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
                let Some(vc) = side::tree(&app.store, self.target.0, None).into_iter().flat_map(|(_, k)| k).find(|c| c.is_voice()).map(|c| c.id) else {
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
            57 if heard && self.rtc == 1 => {
                // the heartbeat runs every second against the mock, so the graph fills quickly
                if app.rects.contains_key("rtc:popout") && app.voice.pings.len() >= 2 && app.voice.host().is_some() {
                    self.shot(ctx, "rtc_popout.png");
                    self.mark("rtc_popout");
                    self.rtc = 2;
                    if let Some(r) = app.rects.get("voice:status").copied() {
                        self.click(r.center());
                    }
                    self.next(300);
                }
            }
            57 => {
                if heard {
                    self.shot(ctx, "voice.png");
                    // op 43's answer: our call's running time with the prompt under it, and the
                    // other channel's status on its own second line
                    let tall = |k: &str| app.rects.get(k).is_some_and(|r| (r.height() - 48.0).abs() < 0.5);
                    if app.rects.contains_key("chan_timer:2021") && tall("chan_status:2021") && tall("chan_status:2022") && app.rects.contains_key(&format!("voice:tile:{me}")) {
                        self.mark("voice_channel_info");
                    }
                    // then discord's connection popout off the "Voice Connected" line
                    if self.rtc == 0
                        && let Some(r) = app.rects.get("voice:status").copied()
                    {
                        self.click(r.center());
                        self.rtc = 1;
                        return;
                    }
                    self.next(300);
                }
            }
            58 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            59 => {
                if !self.open_page(app, "Voice & Video") {
                    return;
                }
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
                // the button opens discord's picker: Entire Screen, then the first screen in it
                // a click at most every 600ms: a freshly opened dialog can miss one, and a flood
                // would land on the call once the picker shuts
                if let Some(p) = &app.share {
                    self.mark("share_picker");
                    if self.share_at.is_none_or(|t| t.elapsed() > Duration::from_millis(600)) {
                        let key = if p.tab == 1 { "share:item:0" } else { "share:tab:1" };
                        if let Some(r) = app.rects.get(key).copied() {
                            if p.tab == 1 {
                                self.shot(ctx, "share_picker.png");
                            }
                            self.click(r.center());
                            self.share_at = Some(Instant::now());
                        }
                    }
                    return;
                }
                if app.voice.stream.as_ref().is_some_and(|l| l.mine && l.state == State::Connected && l.frame.is_some()) {
                    self.mark("stream_out");
                    // RUSTCORD_E2E_HOLD_STREAM=ms keeps the stream up, to watch it over time
                    let hold = std::env::var("RUSTCORD_E2E_HOLD_STREAM").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
                    self.next(hold);
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
            // the soundboard popout over the guild call controls: open it, play a sound, close it
            77 => {
                let Some(r) = app.rects.get("voice:soundboard").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            78 => {
                let loaded = app.soundboard.as_ref().and_then(|(_, s)| s.as_ref()).is_some_and(|v| !v.is_empty());
                if loaded && app.rects.contains_key("sound:5001") {
                    self.shot(ctx, "soundboard.png");
                    let Some(r) = app.rects.get("sound:5001").copied() else { return };
                    self.click(r.center());
                    self.mark("soundboard");
                    self.next(400);
                }
            }
            79 => {
                if let Some(r) = app.rects.get("voice:soundboard").copied() {
                    self.click(r.center());
                }
                self.next(300);
            }
            80 => {
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            81 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.mark("voice_left");
                    self.next(200);
                }
            }
            82 => {
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.click(r.center());
                self.next(1200);
            }
            83 => {
                // the dm with alice, which this step has always meant; the muted one came later
                let Some(dm) = app.store.dms.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.kind == 1) && !app.store.dm_muted(*c)) else {
                    return self.fail("no dm".into());
                };
                let Some(r) = app.rects.get(&format!("chan:{dm}")).copied() else { return };
                self.call = dm;
                self.click(r.center());
                self.next(800);
            }
            84 => {
                let Some(r) = app.rects.get("dm:call").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            85 => {
                if app.voice.state == State::Connected && app.voice.chan == Some(self.call) && heard {
                    self.mark("dm_call");
                    if crate::tray::current().connected {
                        self.mark("tray_voice");
                    }
                    self.next(500);
                }
            }
            86 => {
                if heard {
                    self.shot(ctx, "dm_call.png");
                    self.next(300);
                }
            }
            87 => {
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            88 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.mark("dm_left");
                    self.next(300);
                }
            }
            // alice's dm as a toast: her name over the text, her avatar, silent; clicking it opens the
            // message. The taskbar carries discord's badge for the mentions waiting.
            89 => {
                let Some((xml, at)) = self.pings.iter().find(|(_, at)| at.0 == 3000).cloned() else {
                    return self.fail("alice's dm made no toast".into());
                };
                if xml.contains("<text>Alice</text>") && xml.contains("got a sec?</text>") && xml.contains(r#"<audio silent="true"/>"#) {
                    self.mark("toast");
                }
                self.loaded = at.1 as usize;
                let n = app.title_count.unwrap_or(0) as i32;
                if n > 0 && crate::winsys::badge() == n {
                    self.mark("taskbar_badge");
                }
                app.select_guild(Some(self.target.0));
                app.open_ping(ctx, at.0, at.1);
                self.next(800);
            }
            90 => {
                if app.cur() == Some(3000) && app.store.msgs.get(&3000).is_some_and(|e| e.list.iter().any(|m| m.id as usize == self.loaded)) {
                    self.mark("toast_click");
                    self.loaded = 0;
                    self.next(0);
                }
            }
            // minimized, a dm still raises its toast and moves the taskbar badge and the title's
            // count; step() doesn't run meanwhile, so hidden() watches and brings the window back
            91 => {
                // minimizing takes focus away, which the injected input otherwise always claims
                self.blur = true;
                self.hidden = Some((self.pings.len(), app.title_count.unwrap_or(0) as i32));
                app.rest("POST", "/mock/dm".into(), Some(json!({ "channel": 3000, "from": 101, "text": "still there?", "after_ms": 1500 })), crate::net::Tag::Quiet);
                crate::winsys::show_quietly(true);
                self.next(0);
            }
            92 => {
                if self.hidden.is_none() && !ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
                    self.blur = false;
                    self.next(300);
                }
            }
            // closing the window the way Alt+F4 does, with Minimize to Tray on, puts it away instead
            93 => {
                crate::winsys::send_close();
                self.next(800);
            }
            94 => {
                if crate::winsys::shown() {
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
            350..=369 => self.sync_steps(app, ctx),
            370..=449 => self.emoji_steps(app, ctx),
            450..=519 => self.qol_steps(app, ctx),
            520..=559 => self.home_steps(app, ctx),
            1500..=1599 => self.login_steps(app, ctx),
            _ => {
                if self.pending_shots == 0 {
                    self.finish(ctx);
                }
            }
        }
    }

    /// Click a settings sidebar entry, scrolling the sidebar first if it's below the fold.
    fn open_page(&mut self, app: &App, label: &str) -> bool {
        let key = if label == "Profiles" { "Edit Profiles" } else { label };
        if let Some(r) = app.rects.get(&format!("settings:{key}")).copied() {
            // only aim once the sidebar has stopped scrolling
            if self.steady == Some(r) {
                self.steady = None;
                self.click(r.center());
                return true;
            }
            self.steady = Some(r);
            return false;
        }
        // scroll toward the label: down when it's below everything on screen
        let order = |l: &str| SIDEBAR.iter().position(|x| *x == l);
        let lowest = app.rects.keys().filter_map(|k| k.strip_prefix("settings:")).filter_map(order).max();
        let below = match (order(label), lowest) {
            (Some(i), Some(l)) => i > l,
            _ => PAGES_BELOW.contains(&label),
        };
        let dir = if below { -200.0 } else { 200.0 };
        // over the sidebar's list, just under the profile card, whatever the window's size
        let at = app.rects.get("settings:Edit Profiles").map_or(egui::pos2(280.0, 500.0), |r| egui::pos2(r.center().x, r.bottom() + 120.0));
        self.queue.push_back(vec![Event::PointerMoved(at)]);
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
        const PAGES: [&str; 18] = [
            "Account", "Profiles", "Data & Privacy", "Messaging Permissions", "Notifications", "Nitro", "Server Boost", "Subscriptions",
            "Gift Inventory", "Billing", "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time",
            "Activity Privacy", "Connected Apps", "Developer",
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
                    self.step = 142;
                    self.next(500);
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
                if !self.open_page(app, "Messages") {
                    return;
                }
                self.next(800);
            }
            114 => {
                let Some(r) = Self::at(ctx, "radio:Compact") else { return };
                // the sidebar's Messages row must have brought it into view
                if !ctx.content_rect().contains(r.center()) {
                    return;
                }
                self.mark("settings_scrolled_to");
                self.click(r.center());
                self.next(500);
            }
            115 => {
                if app.settings.compact {
                    self.mark("compact");
                    self.key(Key::Escape);
                    self.next(1000);
                }
            }
            116 => {
                self.shot(ctx, "chat_compact.png");
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            117 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(600);
            }
            118 => {
                if !self.open_page(app, "Messages") {
                    return;
                }
                self.next(800);
            }
            119 => {
                let Some(r) = Self::at(ctx, "radio:Cozy") else { return };
                self.click(r.center());
                self.next(300);
            }
            120 => {
                if !self.open_page(app, "Streamer Mode") {
                    return;
                }
                self.next(800);
            }
            121 => {
                let Some(r) = Self::at(ctx, "toggle:Enable Streamer Mode") else { return };
                self.click(r.center());
                self.next(600);
            }
            122 => {
                if app.settings.streamer {
                    self.mark("streamer");
                    self.shot(ctx, "settings_streamer_on.png");
                    self.step = 141;
                    self.next(400);
                }
            }
            123 => {
                if !self.open_page(app, "Messaging Permissions") {
                    return;
                }
                self.next(900);
            }
            124 => {
                let Some(r) = Self::at(ctx, "radio:Keep me safe") else { return };
                self.click(r.center());
                self.next(500);
            }
            125 => {
                if !self.open_page(app, "Language & Time") {
                    return;
                }
                self.next(600);
            }
            126 => {
                let Some(r) = Self::at(ctx, "lang:de") else { return };
                self.click(r.center());
                self.next(400);
            }
            127 => {
                if !self.open_page(app, "Account") {
                    return;
                }
                self.next(600);
            }
            128 => {
                if !self.open_page(app, "Family Center") {
                    return;
                }
                self.next(800);
            }
            129 => {
                let Some(r) = Self::at(ctx, "house:Bravery") else { return };
                self.shot(ctx, "settings_account_family.png");
                self.click(r.center());
                self.next(600);
            }
            130 => {
                self.mark("hypesquad");
                if !self.open_page(app, "Language & Time") {
                    return;
                }
                self.next(600);
            }
            131 => {
                let Some(r) = Self::at(ctx, "lang:en-US") else { return };
                self.click(r.center());
                self.next(300);
            }
            132 => {
                if !self.open_page(app, "Account") {
                    return;
                }
                self.next(600);
            }
            133 => {
                if !self.open_page(app, "Account Info") {
                    return;
                }
                self.next(800);
            }
            134 => {
                let Some(r) = Self::at(ctx, "edit:Display Name") else { return };
                self.click(r.center());
                self.next(500);
            }
            135 => {
                let Some(r) = Self::at(ctx, "modal_input") else { return };
                self.click(r.right_center() - vec2(4.0, 0.0));
                self.queue.push_back(vec![Event::Text("!".into())]);
                self.next(400);
            }
            136 => {
                let Some(r) = Self::at(ctx, "modal_done") else { return };
                self.shot(ctx, "settings_modal.png");
                self.click(r.center());
                self.next(0);
            }
            137 => {
                if app.sform.modal.is_none() && app.store.user(app.store.me).and_then(|u| u.global_name.as_deref()) == Some("You!") {
                    self.mark("display_name");
                    self.next(300);
                }
            }
            138 => {
                if !self.open_page(app, "Password & Security") {
                    return;
                }
                self.next(800);
            }
            139 => {
                let Some(r) = Self::at(ctx, "nav:Logged-in Devices") else { return };
                self.click(r.center());
                self.next(900);
            }
            140 => {
                self.shot(ctx, "settings_devices.png");
                self.next(400);
            }
            141 => {
                self.key(Key::Escape);
                self.step = 350;
            }
            142 => {
                let Some(r) = Self::at(ctx, "toggle:Enable Streamer Mode") else { return };
                self.click(r.center());
                self.step = 122;
                self.next(400);
            }
            143 => {
                self.key(Key::Escape);
                self.step = 109;
                self.next(900);
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
                // a booster gets the gem after their name, nobody else does
                if rect(app, "boost:102").is_none() || rect(app, "boost:103").is_some() {
                    return self.fail("member list boost gem missing for 102 or shown for 103".into());
                }
                // the activity section: three cards from people here, the outsider and the ended game left out
                if rect(app, "activity:4").is_none() {
                    return;
                }
                let cards: Vec<&str> = ["101", "102", "103", "105", "999"].into_iter().filter(|u| rect(app, &format!("activity:card:{u}")).is_some()).collect();
                if cards != ["101", "102", "103"] {
                    return self.fail(format!("activity cards were {cards:?}, wanted alice, bob and carol"));
                }
                if rect(app, "member_grad:103").is_some() {
                    self.mark("member_grad");
                }
                self.mark("activity");
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
        let side_w = |app: &App| if app.ui.side_w > 0.0 { app.ui.side_w } else { 302.0 };
        let members_w = |app: &App| if app.ui.members_w > 0.0 { app.ui.members_w } else { 264.0 };
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
                // narrower: the warm run starts where the cold one left it, which can be discord's 359 max
                drag(self, pos2(72.0 + side_w(app), 400.0), -50.0);
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
                self.probes.push((shot.into(), vec![("lock".into(), Probe::At(icon.min + icon.size() * vec2(0.86, 0.23)))]));
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
                // the code message sits a few above the newest; older than the viewport after the earlier steps sent more
                if rect(app, "codeblock").is_none() {
                    if self.tries < 20 {
                        self.tries += 1;
                        self.queue.push_back(vec![Event::PointerMoved(egui::pos2(640.0, 400.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, 150.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                        self.wait = Instant::now() + Duration::from_millis(250);
                    }
                    return;
                }
                // off the message, so the hover wash doesn't tint the probes
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(30.0, 300.0))]);
                self.next(500);
            }
            342 => {
                let Some(code) = rect(app, "codeblock") else { return };
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
            _ => self.step = 370,
        }
    }

    /// Going live for real (RUSTCORD_LIVE=<voice channel>, RUSTCORD_TOUR=<artifacts>): into the
    /// channel, the screen button, Entire Screen and its first screen, as a person would; held
    /// for RUSTCORD_LIVE_SECS while what went out, and what discord's server says it got, is
    /// written down each second; then the stream and the call are ended.
    fn live_steps(&mut self, app: &mut App, ctx: &egui::Context, ch: Id) {
        let sh = app.voice.shared.clone();
        let n = |c: &std::sync::atomic::AtomicU64| c.load(std::sync::atomic::Ordering::Relaxed);
        let secs: u64 = std::env::var("RUSTCORD_LIVE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
        if self.step == 0 {
            self.step = 600;
        }
        match self.step {
            600 => {
                let Some(c) = app.store.channels.get(&ch).filter(|_| app.conn == Conn::Online) else { return };
                if !c.is_voice() {
                    return self.fail(format!("{ch} isn't a voice channel"));
                }
                self.mark("ready");
                let g = c.guild;
                app.select_guild((g != 0).then_some(g));
                app.open(ch);
                app.join_voice(ch, false);
                self.next(0);
            }
            601 => {
                if app.voice.state == State::Connected && app.voice.chan == Some(ch) {
                    self.mark("voice_connected");
                    self.next(1500);
                }
            }
            602 => {
                if let Some(p) = &app.share {
                    if self.share_at.is_none_or(|t| t.elapsed() > Duration::from_millis(600)) {
                        let key = if p.tab == 1 { "share:item:0" } else { "share:tab:1" };
                        if let Some(r) = app.rects.get(key).copied() {
                            if p.tab == 1 {
                                self.shot(ctx, "live_picker.png");
                            }
                            self.click(r.center());
                            self.share_at = Some(Instant::now());
                        }
                    }
                    return;
                }
                if app.voice.stream.as_ref().is_some_and(|l| l.mine && l.state == State::Connected && l.frame.is_some()) {
                    self.mark("live");
                    self.live_at = Some(Instant::now());
                    self.loaded = n(&sh.video_out) as usize;
                    self.next(1000);
                    return;
                }
                if self.share_at.is_none()
                    && let Some(r) = app.rects.get("voice:screen").copied()
                {
                    self.click(r.center());
                    self.share_at = Some(Instant::now());
                }
            }
            603 => {
                let out = n(&sh.video_out);
                let at = self.live_at.map_or(0.0, |t| t.elapsed().as_secs_f64());
                let stream = app.voice.stream.as_ref();
                self.samples.push(json!({
                    "t": (at * 10.0).round() / 10.0,
                    "frames_out": out,
                    "fps": out as i64 - self.loaded as i64,
                    "stream": stream.map(|l| format!("{:?}", l.state)),
                    "server_has_seq": n(&sh.video_acked),
                    "server_lost_pct": (sh.video_lost.load(std::sync::atomic::Ordering::Relaxed) as f64 * 100.0 / 256.0).round(),
                    "ping_ms": app.voice.pings.back().map(|(_, p)| *p),
                    "audio_packets": n(&sh.sent),
                }));
                self.loaded = out as usize;
                self.rects.insert("live_samples".into(), json!(self.samples));
                self.rects.insert("live_heard".into(), json!(self.heard));
                if let Some((t, _)) = &app.toast {
                    self.rects.insert("live_toast".into(), json!(t));
                }
                if self.samples.len() as u64 == secs / 2 {
                    self.shot(ctx, "live.png");
                }
                if stream.is_none_or(|l| l.state != State::Connected) {
                    app.voice.leave(&app.net);
                    return self.fail(format!("the stream dropped after {at:.0}s"));
                }
                if self.samples.len() as u64 >= secs {
                    self.mark("held");
                    app.voice.end_stream(&app.net);
                    self.next(1500);
                } else {
                    // the same step again in a second
                    self.step_at = Instant::now();
                    self.wait = Instant::now() + Duration::from_millis(1000);
                }
            }
            604 => {
                app.voice.leave(&app.net);
                self.next(0);
            }
            605 => {
                if app.voice.state == State::Idle && app.voice.chan.is_none() {
                    self.mark("left");
                    self.step = 99;
                }
            }
            _ => self.step = 99,
        }
    }

    /// Read-only walk through a real account (RUSTCORD_TOUR + RUSTCORD_READONLY):
    /// look at things, screenshot them, change nothing.
    fn tour_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const PAGES: [&str; 18] = [
            "Account", "Profiles", "Data & Privacy", "Messaging Permissions", "Notifications", "Nitro", "Server Boost", "Subscriptions",
            "Gift Inventory", "Billing", "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time",
            "Activity Privacy", "Connected Apps", "Developer",
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
                self.loaded = 0;
                self.step = 340;
                self.step_at = Instant::now();
                self.wait = Instant::now() + Duration::from_millis(300);
            }
            // discord's menus and popouts, each opened, shot and closed again
            // back to the server the rest of the tour expects, a frame after the last shot
            354 => {
                app.select_guild(Some(self.target.0));
                self.step = 304;
                if std::env::var_os("RUSTCORD_TOUR_CHANNEL").is_some() {
                    self.step = 360;
                    self.loaded = 0;
                }
                self.step_at = Instant::now();
                self.wait = Instant::now() + Duration::from_millis(800);
            }
            // side by side with captures of discord: a busy channel, its history, the switcher and
            // pins there, a forum and a post in it, then home. RUSTCORD_TOUR_CHANNEL and
            // RUSTCORD_TOUR_FORUM name them as guild/channel.
            360..=368 => {
                let pick = |k: &str| {
                    let v = std::env::var(k).ok()?;
                    let (g, c) = v.split_once('/')?;
                    Some((g.parse::<Id>().ok()?, c.parse::<Id>().ok()?))
                };
                let chat = pick("RUSTCORD_TOUR_CHANNEL");
                let forum = pick("RUSTCORD_TOUR_FORUM");
                let middle = |app: &App| app.rects.get("composer").map(|c| egui::pos2(c.center().x, (c.top() + 90.0) / 2.0));
                let i = self.step - 360;
                if self.loaded == 0 {
                    match i {
                        0 => {
                            if let Some((g, c)) = chat {
                                app.ui.guild = Some(g);
                                app.open(c);
                            }
                        }
                        1 => {
                            if let Some(at) = middle(app) {
                                self.queue.push_back(vec![Event::PointerMoved(at)]);
                                for _ in 0..8 {
                                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 150.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                                }
                            }
                        }
                        2 => {
                            let k = |pressed| Event::Key { key: Key::K, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND };
                            self.queue.push_back(vec![k(true)]);
                            self.queue.push_back(vec![k(false)]);
                        }
                        3 => {
                            if let Some(r) = app.rects.get("pins_button").copied() {
                                self.click(r.center());
                            }
                        }
                        4 => {
                            if let Some((g, c)) = forum {
                                app.ui.guild = Some(g);
                                app.open(c);
                            }
                        }
                        5 => {
                            if let Some(r) = app.rects.iter().filter(|(k, _)| k.starts_with("post:")).map(|(_, r)| *r).min_by(|a, b| a.top().total_cmp(&b.top())) {
                                self.click(r.center());
                            }
                        }
                        6 => app.select_guild(None),
                        _ => {}
                    }
                    self.loaded = 1;
                    self.wait = Instant::now() + Duration::from_millis(if matches!(i, 0 | 4 | 5) { 5000 } else { 1800 });
                } else {
                    const SHOTS: [&str; 7] = ["30_chat", "31_chat_up", "32_switcher", "33_pins", "34_forum", "35_forum_post", "36_home"];
                    if let Some(name) = SHOTS.get(i as usize) {
                        self.shot(ctx, &format!("{name}.png"));
                    }
                    if matches!(i, 2 | 3) {
                        self.key(Key::Escape);
                    }
                    self.loaded = 0;
                    self.step += 1;
                    // one idle step first, so the last shot is saved before the restore
                    if self.step == 368 {
                        app.select_guild(Some(self.target.0));
                        self.step = 304;
                    }
                    self.step_at = Instant::now();
                    self.wait = Instant::now() + Duration::from_millis(600);
                }
            }
            340..=353 => {
                const SHOTS: [&str; 14] = [
                    "11_menu_message", "12_menu_channel", "13_menu_guild", "14_server_dropdown", "15_menu_user",
                    "16_account", "17_bell", "18_threads", "19_inbox", "20_search_options", "21_stickers", "22_forward",
                    "23_dm_profile", "24_banner",
                ];
                let i = (self.step - 340) as usize;
                if self.loaded == 0 {
                    let cur = app.cur().unwrap_or(0);
                    let me = format!(":{}", app.store.me);
                    let find = |p: &str| app.rects.iter().find(|(k, _)| k.starts_with(p) && !k.ends_with(&me)).map(|(_, r)| *r);
                    let (r, right) = match i {
                        0 => (find("stamp:"), true),
                        1 => (app.rects.get(&format!("chan:{cur}")).copied(), true),
                        2 => (app.rects.get(&format!("guild:{}", self.target.0)).copied(), true),
                        3 => (app.rects.get("guildmenu").copied(), false),
                        4 => (find("member:"), true),
                        5 => (app.rects.get("userpanel").map(|r| egui::Rect::from_center_size(egui::pos2(r.left() + 40.0, r.center().y), egui::vec2(4.0, 4.0))), false),
                        6 => (app.rects.get("bell").copied(), false),
                        7 => (app.rects.get("threads_button").copied(), false),
                        8 => (app.rects.get("title:inbox").copied(), false),
                        9 => {
                            // typed, so the dropdown shows its suggestions
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new("search_q"), "the".to_string()));
                            (app.rects.get("search").copied(), false)
                        }
                        10 => (app.rects.get("sticker_button").copied(), false),
                        11 => {
                            let last = app.store.msgs.get(&cur).and_then(|e| e.list.last()).map(|m| m.id);
                            if let Some(m) = last {
                                app.forward = Some(crate::ui::popups::Forward::new(cur, m));
                            }
                            (None, false)
                        }
                        12 => {
                            let dm = app.store.dms.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.kind == 1));
                            if let Some(c) = dm {
                                app.ui.guild = None;
                                app.open(c);
                            }
                            (None, false)
                        }
                        _ => {
                            let g = app.store.guild_order.iter().copied().find(|g| app.store.guilds.get(g).is_some_and(|g| g.banner.is_some()));
                            if let Some(g) = g {
                                app.select_guild(Some(g));
                            }
                            (None, false)
                        }
                    };
                    match r {
                        Some(r) if right => self.rclick(r.center()),
                        Some(r) => self.click(r.center()),
                        None => {}
                    }
                    self.loaded = 1;
                    self.wait = Instant::now() + Duration::from_millis(1500);
                } else {
                    self.shot(ctx, &format!("{}.png", SHOTS[i]));
                    self.key(Key::Escape);
                    self.loaded = 0;
                    self.step += 1;
                    self.step_at = Instant::now();
                    self.wait = Instant::now() + Duration::from_millis(500);
                }
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
                // with a folder left open the rail scrolls, and home can sit under the title bar
                let bar = rect(app, "bar").map_or(0.0, |b| b.bottom());
                if r.top() < bar {
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(r.center().x, bar + 200.0))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 400.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    return;
                }
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

    /// Custom emoji: `:` autocomplete across servers, typed :name: on send, nitro rules.
    fn emoji_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::net::Tag;
        let general = self.target.1;
        let say = |me: &mut Self, parts: &[&str]| {
            for p in parts {
                me.queue.push_back(vec![Event::Text(p.to_string())]);
            }
            me.key(Key::Enter);
            me.next(500);
        };
        match self.step {
            370 => {
                app.open(general);
                self.next(700);
            }
            371 => {
                // the mock account has no nitro yet: omgParty is animated, omgMods wants a role, omgBruh lives elsewhere
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text(":omg".into())]);
                self.next(500);
            }
            372 => {
                if app.ac_open {
                    self.shot(ctx, "emoji_locked.png");
                    self.key(Key::Tab);
                    self.mark("emoji_locked_ac");
                    say(self, &["locked"]);
                }
            }
            373 => say(self, &[":ferris: static"]),
            374 => say(self, &[":smile", ":", " hi"]),
            375 => say(self, &[":nosuchemoji: `:ferris:` `:smile:`"]),
            376 => say(self, &[":omgBruh: far"]),
            377 => {
                app.rest("POST", "/mock/premium".into(), Some(json!({ "type": 2 })), Tag::Quiet);
                self.next(0);
            }
            378 => {
                if !app.store.nitro() {
                    return;
                }
                self.queue.push_back(vec![Event::Text(":omg".into())]);
                self.next(500);
            }
            379 => {
                if app.ac_open {
                    self.shot(ctx, "emoji_nitro.png");
                    self.key(Key::Tab);
                    say(self, &["anim"]);
                }
            }
            380 => {
                self.queue.push_back(vec![Event::Text(":omgB".into())]);
                self.next(500);
            }
            381 => {
                if app.ac_open {
                    self.key(Key::Tab);
                    say(self, &["picked"]);
                }
            }
            382 => say(self, &[":omgBruh: typed"]),
            383 => say(self, &[":omgMods: :omgLapsed: no"]),
            384 => {
                self.mark("emoji");
                self.next(800);
            }
            // the right-click menus, the server dropdown and the search filters each open
            385 | 387 | 389 | 391 => {
                // Server Boost opens discord's boosts page in the app: its levels and perks drawn
                // from the catalogue, shot, then left so the search box comes back
                if self.step == 389 && app.boosts.is_some() {
                    if !app.rects.contains_key("boosts:level:3") || !app.rects.contains_key("boosts:perk:Server Tag Badge Packs") {
                        return;
                    }
                    if self.loaded != 389 {
                        self.shot(ctx, "boosts.png");
                        self.loaded = 389;
                    } else {
                        self.mark("boosts");
                        app.boosts = None;
                        self.loaded = 0;
                    }
                    self.wait = Instant::now() + Duration::from_millis(400);
                    return;
                }
                if self.step == 389 && !self.marks.iter().any(|(m, _)| m == "boosts") {
                    return self.fail("Server Boost in the server dropdown didn't open the boosts page".into());
                }
                // the filters drop down from an empty search box; an earlier step left a query in it
                ctx.data_mut(|d| d.remove::<String>(egui::Id::new("search_q")));
                app.search = None;
                let member = app.rects.keys().find(|k| k.starts_with("member:") && **k != format!("member:{}", app.store.me)).cloned();
                let (key, right) = match self.step {
                    385 => (format!("chan:{general}"), true),
                    387 => ("guildmenu".to_string(), false),
                    389 => ("search".to_string(), false),
                    _ => (member.unwrap_or_default(), true),
                };
                let Some(r) = app.rects.get(&key).copied() else { return self.fail(format!("nothing to open a menu on: {key}")) };
                if right {
                    self.rclick(r.center());
                } else {
                    self.click(r.center());
                }
                self.next(700);
            }
            386 | 388 | 390 | 392 => {
                let (open, name) = match self.step {
                    386 => (egui::Popup::is_any_open(ctx), "menu_channel"),
                    388 => (egui::Popup::is_any_open(ctx), "server_dropdown"),
                    390 => (app.rects.contains_key("search_options"), "search_options"),
                    _ => (app.user_menu.is_some(), "menu_user"),
                };
                if !open {
                    return self.fail(format!("{name} didn't open"));
                }
                self.shot(ctx, &format!("{name}.png"));
                // a dropdown item that hands off to the browser has to actually ask for it
                match ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", "Server Boost")))) {
                    Some(r) if self.step == 388 => self.click(r.center()),
                    _ => self.key(Key::Escape),
                }
                if self.step == 392 {
                    self.mark("menus");
                }
                self.next(500);
            }
            // Gamers United only lists what onboarding picked: lobby, not lfg, until it's added back
            393 => {
                // a warm start comes back with the cold run's pick cached and lfg as the server's
                // last channel, which the list always keeps; start from lobby with lfg unpicked
                app.store.opted.remove(&2101);
                app.ui.guild = Some(1001);
                app.open(2100);
                self.next(800);
            }
            394 => {
                if !app.rects.contains_key("chan:2100") {
                    return;
                }
                if app.rects.contains_key("chan:2101") {
                    return self.fail("lfg is listed though onboarding never picked it".into());
                }
                app.open(2101);
                self.next(800);
            }
            395 => {
                let Some(r) = app.rects.get("add_to_list").copied() else { return };
                self.shot(ctx, "unlisted.png");
                self.click(r.center());
                self.next(800);
            }
            396 => {
                if !app.store.channels.get(&2101).is_some_and(|c| app.store.listed(c)) {
                    return;
                }
                self.mark("opt_in");
                app.select_guild(Some(self.target.0));
                self.next(500);
            }
            // the message menu's quick reactions: right-click the last slot, search, Enter
            397 => {
                app.open(general);
                self.next(800);
            }
            398 => {
                let Some(id) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| !m.pending)).map(|m| m.id) else { return };
                app.menu = Some((general, id, egui::pos2(500.0, 300.0)));
                self.next(600);
            }
            399 => {
                let Some(r) = app.rects.get("quick:3").copied() else { return };
                self.rclick(r.center());
                self.next(600);
            }
            400 => {
                if !matches!(app.picker, Some((super::Pick::Quick(3), _))) {
                    return self.fail("right-clicking a quick reaction didn't open the picker for it".into());
                }
                self.queue.push_back(vec![Event::Text("skull".into())]);
                self.queue.push_back(vec![]);
                self.key(Key::Enter);
                self.next(800);
            }
            401 => {
                if app.picker.is_some() || app.settings.quick_reactions.get(3).is_none_or(|q| q.name != "💀") {
                    return;
                }
                app.menu = Some((general, app.store.msgs.get(&general).and_then(|e| e.list.last()).map_or(0, |m| m.id), egui::pos2(500.0, 300.0)));
                self.next(600);
            }
            402 => {
                self.shot(ctx, "quick_reactions.png");
                self.key(Key::Escape);
                self.mark("quick_reactions");
                self.next(400);
            }
            // a few messages back marked unread brings the "new messages" bar; a wheel down at the
            // bottom reads them and takes it away
            403 => {
                let Some(id) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().nth(3)).map(|m| m.id) else { return };
                app.mark_unread(general, id);
                self.next(600);
            }
            404 => {
                let Some(c) = app.rects.get("composer").copied() else { return };
                if !app.marker_bar {
                    return self.fail("marking unread didn't bring the new messages bar".into());
                }
                let at = egui::pos2(c.center().x, c.top() - 120.0);
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                let wheel = Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -120.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE };
                self.queue.push_back(vec![wheel]);
                self.next(600);
            }
            405 => {
                if app.marker_bar {
                    return;
                }
                self.mark("bar_read");
                self.next(300);
            }
            // middle-click autoscroll: a click turns it on, moving up scrolls up, a click ends it
            406 => {
                let Some(c) = app.rects.get("composer").copied() else { return };
                let at = egui::pos2(c.center().x, c.top() - 200.0);
                let b = |pressed| Event::PointerButton { pos: at, button: PointerButton::Middle, pressed, modifiers: Modifiers::NONE };
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                self.queue.push_back(vec![b(true)]);
                self.queue.push_back(vec![b(false)]);
                self.next(300);
            }
            407 => {
                if app.autoscroll.is_none() {
                    return self.fail("a middle click didn't start autoscroll".into());
                }
                let Some(c) = app.rects.get("composer").copied() else { return };
                self.y0 = app.offsets.get(&general).copied().unwrap_or(0.0);
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(c.center().x, c.top() - 340.0))]);
                self.next(700);
            }
            408 => {
                if app.offsets.get(&general).copied().unwrap_or(0.0) > self.y0 - 50.0 {
                    return;
                }
                self.shot(ctx, "autoscroll.png");
                let Some(c) = app.rects.get("composer").copied() else { return };
                self.click(egui::pos2(c.center().x, c.top() - 340.0));
                self.next(300);
            }
            409 => {
                if app.autoscroll.is_some() {
                    return self.fail("a click didn't end autoscroll".into());
                }
                self.mark("autoscroll");
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
                self.next(800);
            }
            // F11 over a maximised window: fullscreen fills the whole monitor, not the area above
            // the taskbar, and leaving it gives the maximised window back
            410 => {
                let (full, max) = ctx.input(|i| (i.viewport().fullscreen.unwrap_or(false), i.viewport().maximized.unwrap_or(false)));
                if !full {
                    if max && self.loaded != 410 {
                        self.loaded = 410;
                        self.key(Key::F11);
                    }
                    return;
                }
                let (inner, monitor) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().monitor_size));
                if let (Some(r), Some(m)) = (inner, monitor)
                    && (r.height() - m.y).abs() > 2.0
                {
                    if self.step_at.elapsed() < Duration::from_secs(3) {
                        return;
                    }
                    return self.fail(format!("fullscreen is {:.0} tall on a {:.0} monitor", r.height(), m.y));
                }
                self.loaded = 0;
                self.shot(ctx, "fullscreen.png");
                self.key(Key::F11);
                self.next(800);
            }
            411 => {
                let (full, max) = ctx.input(|i| (i.viewport().fullscreen.unwrap_or(false), i.viewport().maximized.unwrap_or(false)));
                if full || !max {
                    return;
                }
                self.mark("fullscreen");
                ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                self.next(800);
            }
            // ctrl+0 to 100%, a ctrl+wheel notch up to the next level, ctrl+0 back
            412 | 414 => {
                let k = |pressed| Event::Key { key: Key::Num0, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND };
                if self.step == 414 {
                    if (ctx.zoom_factor() - 1.1).abs() > 0.01 {
                        return;
                    }
                    self.shot(ctx, "zoomed.png");
                }
                self.queue.push_back(vec![k(true)]);
                self.queue.push_back(vec![k(false)]);
                self.next(500);
            }
            413 => {
                if (ctx.zoom_factor() - 1.0).abs() > 0.01 {
                    return;
                }
                let wheel = Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, 1.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::COMMAND };
                self.queue.push_back(vec![wheel]);
                self.next(500);
            }
            415 => {
                if (ctx.zoom_factor() - 1.0).abs() > 0.01 {
                    return;
                }
                self.mark("zoom");
                self.next(300);
            }
            // a member's server profile: the avatar switches to the main one, "..." opens their menu
            416 => {
                let me = app.store.me;
                let other = app.store.guilds.get(&self.target.0).and_then(|g| {
                    g.list.iter().find_map(|i| match i {
                        crate::state::ListItem::Member(u) if *u != me => Some(*u),
                        _ => None,
                    })
                });
                let Some(r) = other.and_then(|u| app.rects.get(&format!("member:{u}")).copied()) else { return };
                self.click(r.center());
                self.next(800);
            }
            417 => {
                let Some(head) = app.rects.get("profile_head").copied().filter(|r| r.is_positive()) else { return };
                if !app.profile.is_some_and(|(_, g, _)| g != 0) {
                    return self.fail("the member list didn't open a server profile".into());
                }
                self.click(egui::pos2(head.left() + 56.0, head.top() + 101.0));
                self.next(600);
            }
            418 => {
                if !app.profile.is_some_and(|(_, g, _)| g == 0) {
                    return self.fail("the avatar didn't switch to the main profile".into());
                }
                let Some(head) = app.rects.get("profile_head").copied() else { return };
                // the "..." button, 12 in from the banner's corner
                self.click(egui::pos2(head.right() - 28.0, head.top() + 24.0));
                self.next(600);
            }
            // carol's nitro name font (Medieval): fetched, and her name asks for it
            420 => {
                if crate::fonts::cached_name(7).is_none() || app.store.user(103).is_none_or(|u| u.font != 7) {
                    return;
                }
                if crate::theme::name_font(7, 16.0).family != egui::FontFamily::Name("name7".into()) {
                    return self.fail("carol's name font isn't the Medieval face".into());
                }
                self.mark("name_font");
                self.next(300);
            }
            // the server menu's own modals and pages: Create Channel all the way to a new channel,
            // All Active Threads, the app directory and server settings
            421 => {
                app.select_guild(Some(1000));
                app.create = Some(super::create::Form::new(super::create::Kind::Channel, 1000, None));
                self.next(600);
            }
            422 => {
                let Some(r) = Self::at(ctx, "field:new-channel") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("E2E room".into())]);
                self.next(400);
            }
            423 => {
                // discord lowercases a text channel's name and swaps spaces for dashes as you type
                if app.create.as_ref().is_none_or(|f| f.name != "e2e-room") {
                    return self.fail(format!("create channel name became {:?}", app.create.as_ref().map(|f| f.name.clone())));
                }
                self.shot(ctx, "create_channel.png");
                let Some(r) = Self::at(ctx, "plate:Create Channel") else { return };
                self.click(r.center());
                self.next(0);
            }
            424 => {
                if app.create.is_none() && app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.name == "e2e-room") {
                    self.mark("create_channel");
                    app.active_threads = Some(1000);
                    self.next(700);
                }
            }
            425 => {
                let Some(r) = app.rects.get("thread_card:2050").copied() else { return };
                self.shot(ctx, "active_threads.png");
                self.click(r.center());
                self.next(600);
            }
            426 => {
                if app.cur() == Some(2050) && app.active_threads.is_none() {
                    self.mark("active_threads");
                    app.open_directory();
                    self.next(1200);
                }
            }
            427 => {
                if app.directory.as_ref().is_none_or(|d| d.data.get("collections").and_then(|v| v.as_array()).is_none_or(|a| a.is_empty())) {
                    return;
                }
                self.shot(ctx, "directory.png");
                self.mark("directory");
                self.next(400);
            }
            428 => {
                app.directory = None;
                app.open_server_settings(1000);
                if let Some(s) = &mut app.server {
                    s.page = super::server::Page::Roles;
                }
                self.next(1000);
            }
            429 => {
                if app.server.as_ref().is_none_or(|s| s.data.get("role_counts").is_none_or(|v| v.is_null())) {
                    return;
                }
                self.shot(ctx, "server_roles.png");
                self.mark("server_settings");
                app.server = None;
                app.open_events(1000);
                self.next(800);
            }
            // the Events window: the mock's one event, and Interested sent as discord does
            430 => {
                if app.events.as_ref().is_none_or(|e| e.list.as_ref().is_none_or(|l| l.is_empty())) {
                    return;
                }
                self.shot(ctx, "events.png");
                let Some(r) = Self::at(ctx, "event:Interested") else { return };
                self.click(r.center());
                self.next(500);
            }
            431 => {
                if app.events.as_ref().is_some_and(|e| e.rsvp.contains(&6100)) {
                    self.mark("events");
                    app.events = None;
                    self.next(400);
                }
            }
            // a reply open with the composer not focused: escape still drops it
            432 => {
                let Some(m) = app.cur().and_then(|c| app.store.msgs.get(&c)).and_then(|e| e.list.last()).map(|m| m.id) else { return };
                app.reply = Some(m);
                ctx.memory_mut(|mem| mem.surrender_focus(egui::Id::new("composer")));
                self.next(300);
            }
            433 => {
                self.key(Key::Escape);
                self.next(300);
            }
            434 => {
                if app.reply.is_some() {
                    return self.fail("escape left the reply open".into());
                }
                self.mark("reply_escape");
                self.next(200);
            }
            // the mock lists its own exe as a game: discord's panel over the account card, then
            // "Not Sharing" (and an empty presence) once activity sharing is off
            435 => {
                let (Some(p), Some(i)) = (app.rects.get("game:panel").copied(), app.rects.get("game:icon").copied()) else { return };
                if (p.height() - 56.0).abs() > 0.5 || (i.width() - 32.0).abs() > 0.5 || app.rects.contains_key("game:not_sharing") {
                    return self.fail(format!("game panel {p:?}, icon {i:?}, not sharing {}", app.rects.contains_key("game:not_sharing")));
                }
                self.shot(ctx, "game_panel.png");
                self.mark("game_panel");
                app.store.hide_game = true;
                self.next(500);
            }
            436 => {
                if !app.rects.contains_key("game:not_sharing") {
                    return;
                }
                self.shot(ctx, "game_hidden.png");
                self.mark("game_hidden");
                app.store.hide_game = false;
                self.next(500);
            }
            // Invite People on a server none of our friends are known to be in: the dms and
            // friends list under the search, and Invite on a friend with no dm makes one first
            437 => {
                app.invite_people(2400);
                self.next(800);
            }
            438 => {
                let Some(r) = app.rects.iter().find(|(k, _)| k.starts_with("invite:friend:")).map(|(_, r)| *r) else { return };
                if app.made_invite.as_ref().is_none_or(|i| i.code.is_none()) || !app.rects.contains_key("invite:chan:3000") {
                    return;
                }
                self.shot(ctx, "invite.png");
                // sending the invite sets the mock's dms off; under do not disturb they mustn't ping
                let me = app.store.me;
                app.store.presence.insert(me, crate::state::Status::Dnd);
                self.dnd = Some((self.pinged.len(), app.store.channels.get(&3000).map_or(0, |c| c.last_msg)));
                self.click(r.center());
                self.next(800);
            }
            439 => {
                if app.store.dms.contains(&3010) {
                    self.mark("invite_friends");
                    app.made_invite = None;
                    self.next(300);
                }
            }
            440 => {
                let Some((pings, before)) = self.dnd else { return };
                if app.store.channels.get(&3000).map_or(0, |c| c.last_msg) > before {
                    if self.pinged.len() == pings {
                        self.mark("dnd_quiet");
                    }
                    let me = app.store.me;
                    app.store.presence.insert(me, crate::state::Status::Online);
                    self.next(300);
                }
            }
            // resting on a server with people in voice: the tooltip gets discord's speaker row
            441 => {
                let Some(r) = app.rects.get("guildicon:1008").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(1200);
            }
            442 => {
                if app.rects.get("guildtip_voice").is_some_and(|r| r.height() >= 24.0) {
                    self.shot(ctx, "guild_tip.png");
                    self.mark("guild_tip_voice");
                    self.step = 450;
                }
            }
            419 => {
                if app.profile.is_none() || app.user_menu.is_none() {
                    return self.fail(format!("\"...\" on the profile left profile={} menu={}", app.profile.is_some(), app.user_menu.is_some()));
                }
                self.shot(ctx, "profile_more.png");
                self.key(Key::Escape);
                self.mark("profile_more");
                self.next(500);
            }
            _ => self.step = 99,
        }
    }

    /// Discord's little behaviours (ref/QOL.md): composer tricks, message keys, shift skipping
    /// dialogs, the link warning, the shortcut sheet and its easter egg, bookmarks, threads.
    fn qol_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let mine = |app: &App, ch: Id, text: &str| app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.author == app.store.me && !m.pending && m.content == text)).map(|m| m.id);
        let chord = |me: &mut Self, key: Key, modifiers: Modifiers| {
            let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
            me.queue.push_back(vec![k(true)]);
            me.queue.push_back(vec![k(false)]);
        };
        let say = |me: &mut Self, app: &App, text: &str| {
            if let Some(r) = app.rects.get("composer").copied() {
                me.click(r.center());
            }
            me.queue.push_back(vec![Event::Text(text.into())]);
            me.key(Key::Enter);
        };
        let menu_row = |label: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))));
        match self.step {
            450 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(800);
            }
            // +:fire: reacts to the newest message instead of being sent
            451 => {
                say(self, app, "+:fire:");
                self.next(800);
            }
            452 => {
                let last = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| !m.pending));
                if last.is_some_and(|m| m.reactions.iter().any(|r| r.me && r.emoji.name == "🔥")) {
                    self.mark("qol_react");
                    say(self, app, "typo here");
                    self.next(800);
                }
            }
            // s/old/new edits your newest message
            453 => {
                if mine(app, general, "typo here").is_some() {
                    say(self, app, "s/typo/fixed");
                    self.next(800);
                }
            }
            454 => {
                if mine(app, general, "fixed here").is_some() {
                    self.mark("qol_sub");
                    say(self, app, "/shrug hi");
                    self.next(800);
                }
            }
            455 => {
                if mine(app, general, "hi ¯\\_(ツ)_/¯").is_some() {
                    self.mark("qol_shrug");
                    // a paste past the limit goes up as message.txt
                    let Some(r) = app.rects.get("composer").copied() else { return };
                    self.click(r.center());
                    self.queue.push_back(vec![Event::Paste("x".repeat(4100))]);
                    self.next(500);
                }
            }
            456 => {
                if app.files.iter().any(|f| f.name == "message.txt") && app.drafts.get(&general).is_none_or(|d| d.is_empty()) {
                    self.mark("qol_paste_txt");
                    self.shot(ctx, "qol_message_txt.png");
                    self.key(Key::Escape);
                    self.next(400);
                }
            }
            457 => {
                if !app.files.is_empty() {
                    return;
                }
                self.mark("qol_esc_uploads");
                // ctrl+b wraps the selection
                self.queue.push_back(vec![Event::Text("make bold".into())]);
                chord(self, Key::A, Modifiers::COMMAND);
                self.queue.push_back(vec![]);
                chord(self, Key::B, Modifiers::COMMAND);
                self.next(400);
            }
            458 => {
                if app.drafts.get(&general).is_some_and(|d| d == "**make bold**") {
                    self.mark("qol_ctrl_b");
                    self.shot(ctx, "qol_format_bar.png");
                    app.drafts.remove(&general);
                    // Enter inside an open code block is a new line
                    self.queue.push_back(vec![Event::Text("```rs".into())]);
                    self.key(Key::Enter);
                    self.next(400);
                }
            }
            459 => {
                if app.drafts.get(&general).is_some_and(|d| d.starts_with("```rs\n")) {
                    self.mark("qol_code_enter");
                    app.drafts.remove(&general);
                    // shift+tab onto the newest message, then R replies to it
                    chord(self, Key::Tab, Modifiers::SHIFT);
                    self.next(300);
                }
            }
            460 => {
                if app.focus_msg.is_none() {
                    return;
                }
                self.loaded = app.focus_msg.unwrap_or(0) as usize;
                self.shot(ctx, "qol_msg_focus.png");
                self.queue.push_back(vec![Event::Text("r".into())]);
                self.next(300);
            }
            461 => {
                if app.reply == Some(self.loaded as Id) && app.focus_msg.is_none() {
                    self.mark("qol_msg_reply");
                    self.key(Key::Escape);
                    self.next(300);
                }
            }
            // holding shift over a message: discord's long toolbar, and its delete doesn't ask
            462 => {
                if app.reply.is_some() {
                    return;
                }
                let Some(id) = mine(app, general, r"hi ¯\_(ツ)_/¯") else { return };
                let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("row", id)))) else { return };
                self.shift = true;
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(400);
            }
            463 => {
                let Some(r) = app.rects.get("bar_delete").copied() else { return };
                self.shot(ctx, "qol_shift_bar.png");
                self.mark("qol_shift_bar");
                self.loaded = app.bar.map_or(0, |(id, _)| id) as usize;
                self.click(r.center());
                self.next(500);
            }
            464 => {
                let gone = !app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.id as usize == self.loaded));
                if gone && app.confirm.is_none() {
                    self.mark("qol_shift_delete");
                    self.shift = false;
                    say(self, app, "[docs](https://example.com/guide)");
                    self.next(800);
                }
            }
            // a masked link asks before leaving; trusting the site stops it asking
            465 => {
                let Some(r) = app.rects.get("link").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            466 => {
                if app.leaving.is_none() {
                    return;
                }
                self.shot(ctx, "qol_leaving.png");
                let Some(r) = app.rects.get("leaving:trust").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            467 => {
                if !app.leaving.as_ref().is_some_and(|(_, t)| *t) {
                    return;
                }
                let Some(r) = app.rects.get("leaving:visit").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            468 => {
                if app.leaving.is_none() && self.opened.iter().any(|u| u == "https://example.com/guide") && app.settings.trusted.iter().any(|t| t == "example.com") {
                    self.mark("qol_leaving");
                    let Some(r) = app.rects.get("link").copied() else { return };
                    self.loaded = self.opened.len();
                    self.click(r.center());
                    self.next(400);
                }
            }
            469 => {
                if app.leaving.is_none() && self.opened.len() > self.loaded {
                    self.mark("qol_trusted");
                }
                // the next run asks again
                app.settings.trusted.retain(|t| t != "example.com");
                crate::cache::set_settings(&app.settings);
                // pinning in a dm: P on a message asks discord's question first
                app.ui.hide_dm_profile = false;
                app.select_guild(None);
                app.open(3000);
                self.next(800);
            }
            470 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                chord(self, Key::Tab, Modifiers::SHIFT);
                self.queue.push_back(vec![]);
                self.queue.push_back(vec![Event::Text("p".into())]);
                self.next(400);
            }
            471 => {
                if !app.confirm.as_ref().is_some_and(|c| c.title == "Pin It. Pin It Good.") {
                    return;
                }
                self.shot(ctx, "qol_pin_confirm.png");
                self.loaded = app.focus_msg.unwrap_or(0) as usize;
                self.next(300);
            }
            472 => {
                self.key(Key::Enter);
                self.next(400);
            }
            473 => {
                if app.confirm.is_none() && app.store.find_msg(3000, self.loaded as Id).is_some_and(|m| m.pinned) {
                    self.mark("qol_pin");
                    // S reads the message out
                    self.queue.push_back(vec![Event::Text("s".into())]);
                    self.next(300);
                }
            }
            474 => {
                if !self.said.is_empty() {
                    self.mark("qol_speak");
                    self.key(Key::Escape);
                    self.next(300);
                }
            }
            // bookmarks: from the menu, then listed in the inbox's tab
            475 => {
                let Some(r) = app.rects.get("msg:last").copied() else { return };
                self.rclick(r.center());
                self.next(400);
            }
            476 => {
                let Some(r) = menu_row("Bookmark Message").filter(|_| app.menu.is_some()) else { return };
                self.click(r.center());
                self.next(400);
            }
            477 => {
                if app.bookmarks.is_empty() {
                    return;
                }
                self.mark("qol_bookmark");
                chord(self, Key::I, Modifiers::COMMAND);
                self.next(500);
            }
            478 => {
                let Some(r) = app.rects.get("inbox:bookmarks").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            479 => {
                if app.inbox.as_ref().is_some_and(|i| i.saved.as_ref().is_some_and(|l| !l.is_empty())) {
                    self.shot(ctx, "qol_bookmarks.png");
                    self.mark("qol_bookmarks_tab");
                    let Some(r) = app.rects.get("inbox:unreads").copied() else { return };
                    self.click(r.center());
                    self.next(400);
                }
            }
            // Mark All as Read asks first
            480 => {
                let Some(r) = app.rects.get("inbox:read_all").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            481 => {
                if app.confirm.as_ref().is_some_and(|c| c.title == "Mark all as read?") {
                    self.shot(ctx, "qol_read_all.png");
                    self.key(Key::Enter);
                    self.next(400);
                }
            }
            482 => {
                if app.confirm.is_none() {
                    self.mark("qol_read_all");
                    app.inbox = None;
                    // a call ringing: its popup, and Escape declines
                    app.voice.ringing = Some(3000);
                    self.next(600);
                }
            }
            483 => {
                if !app.rects.contains_key("call:decline") {
                    return;
                }
                self.shot(ctx, "qol_incoming_call.png");
                self.key(Key::Escape);
                self.next(400);
            }
            484 => {
                if app.voice.ringing.is_none() {
                    self.mark("qol_decline");
                    // the note you keep on alice, from her dm's profile panel
                    let Some(r) = app.rects.get("note").copied() else { return };
                    self.click(r.center());
                    self.queue.push_back(vec![Event::Text("likes rust".into())]);
                    self.key(Key::Enter);
                    self.next(500);
                }
            }
            485 => {
                if app.notes.get(&101).is_some_and(|n| n == "likes rust") {
                    self.mark("qol_note");
                }
                // ctrl+alt+→ from the dms goes back to the server left for them
                chord(self, Key::ArrowRight, Modifiers::COMMAND | Modifiers::ALT);
                self.next(500);
            }
            486 => {
                if app.ui.guild != Some(self.target.0) {
                    return;
                }
                chord(self, Key::ArrowDown, Modifiers::COMMAND | Modifiers::ALT);
                self.next(500);
            }
            487 => {
                if app.ui.guild.is_some_and(|g| g != self.target.0) {
                    self.mark("qol_server_keys");
                    app.select_guild(Some(self.target.0));
                    app.open(general);
                    // ! in the quick switcher finds voice channels
                    chord(self, Key::K, Modifiers::COMMAND);
                    self.queue.push_back(vec![]);
                    self.queue.push_back(vec![Event::Text("!lou".into())]);
                    self.next(400);
                }
            }
            488 => {
                if app.switcher.is_none() {
                    return;
                }
                self.key(Key::Enter);
                self.next(500);
            }
            489 => {
                if app.cur() == Some(2021) {
                    self.mark("qol_switcher_voice");
                    app.open(general);
                    self.next(800);
                }
            }
            // a thread from a message's menu
            490 => {
                let Some(r) = app.rects.get("msg:last").copied() else { return };
                self.rclick(r.center());
                self.next(400);
            }
            491 => {
                let Some(r) = menu_row("Create Thread").filter(|_| app.menu.is_some()) else { return };
                self.click(r.center());
                self.next(400);
            }
            492 => {
                if app.new_thread.is_none() {
                    return;
                }
                self.shot(ctx, "qol_new_thread.png");
                self.queue.push_back(vec![Event::Text("e2e thread".into())]);
                self.key(Key::Enter);
                self.next(800);
            }
            493 => {
                if app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.is_thread() && c.name == "e2e thread") {
                    self.mark("qol_thread");
                    app.open(general);
                    // the emoji picker: alt+click stars one, and it shows under Favorites
                    chord(self, Key::E, Modifiers::COMMAND);
                    self.next(600);
                }
            }
            494 => {
                let Some(r) = Self::at(ctx, "emoji0").filter(|_| app.picker.is_some()) else { return };
                self.loaded = app.settings.fav_emoji.len();
                self.alt = true;
                self.click(r.center());
                self.next(400);
            }
            495 => {
                self.alt = false;
                if app.settings.fav_emoji.len() != self.loaded && app.picker.is_some() {
                    self.mark("qol_fav_emoji");
                    self.shot(ctx, "qol_emoji_favorite.png");
                    app.settings.fav_emoji.clear();
                    crate::cache::set_settings(&app.settings);
                    app.picker = None;
                    // ctrl+/ shows the sheet; its arrows light up
                    chord(self, Key::Slash, Modifiers::COMMAND);
                    self.next(500);
                }
            }
            496 => {
                if app.combos.is_none() {
                    return;
                }
                self.key(Key::ArrowLeft);
                self.next(60);
            }
            497 => {
                if ctx.data(|d| d.get_temp::<(bool, egui::Rect)>(egui::Id::new(("e2e", "ddr", 0usize)))).is_some_and(|(lit, _)| lit) {
                    self.mark("qol_combos");
                    self.shot(ctx, "qol_combos.png");
                    // H H → N K: the Raging Demon
                    for k in [Key::H, Key::H, Key::ArrowRight, Key::N, Key::K] {
                        self.key(k);
                    }
                    self.next(5600);
                }
            }
            498 => {
                if app.demon.is_some() {
                    self.shot(ctx, "qol_demon.png");
                    self.next(4000);
                }
            }
            499 => {
                if app.demon.is_none() && app.combos.is_none() {
                    self.mark("qol_demon");
                    // escape well back in the history jumps to the present
                    let Some(r) = app.rects.get("composer").copied() else { return };
                    self.click(r.center());
                    for _ in 0..6 {
                        self.key(Key::PageUp);
                    }
                    self.next(800);
                }
            }
            500 => {
                if app.far_up != Some(general) {
                    return;
                }
                self.shot(ctx, "qol_far_up.png");
                self.key(Key::Escape);
                self.next(1000);
            }
            501 => {
                if app.far_up.is_none() && app.pinned.contains(&general) {
                    self.mark("qol_esc_present");
                    self.step = 520;
                    self.step_at = Instant::now();
                }
            }
            _ => self.step = 99,
        }
    }

    /// The rows over a server's channels and the pages behind them, a voice channel's chat and
    /// settings, the voice rows' mute icons, perk cards, the profile card's box, the viewer's zoom.
    fn home_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const GAMERS: Id = 1001;
        const HANGOUT: Id = 2102;
        let general = self.target.1;
        let hover = |me: &mut Self, at: Pos2| me.queue.push_back(vec![Event::PointerMoved(at)]);
        let mine = |app: &App, ch: Id, text: &str| app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| m.author == app.store.me && m.content == text));
        match self.step {
            520 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1200);
            }
            // carol muted and deafened herself, alice was muted by the server
            521 => {
                let seen = ["voice_state:103:Muted", "voice_state:103:Deafened", "voice_state:101:Server Muted"].iter().all(|k| Self::at(ctx, k).is_some());
                if seen {
                    self.mark("home_voice_states");
                    self.shot(ctx, "home_voice_states.png");
                    app.open_boosts(self.target.0);
                    self.next(1000);
                }
            }
            522 => {
                let Some(r) = app.rects.get("boosts:perk:Server Tag").copied() else { return };
                hover(self, r.center());
                self.next(700);
            }
            523 => {
                if Self::at(ctx, "perk_btn").is_some() {
                    self.mark("home_perk_hover");
                    self.shot(ctx, "home_perk_hover.png");
                    self.next(600);
                }
            }
            524 => {
                // the shot above is taken from the frame after it's asked for, so leave a beat first
                if app.ui.guild != Some(GAMERS) {
                    app.boosts = None;
                    app.select_guild(Some(GAMERS));
                    self.wait = Instant::now() + Duration::from_millis(1200);
                    return;
                }
                let rows = ["Events", "Channels & Roles", "Members", "Server Boosts"];
                if rows.iter().all(|r| Self::at(ctx, &format!("home_row:{r}")).is_some()) && app.ui.guild == Some(GAMERS) {
                    self.mark("home_rows");
                    self.shot(ctx, "home_rows.png");
                    if let Some(r) = Self::at(ctx, "home_row:Members") {
                        self.click(r.center());
                    }
                    self.next(1200);
                }
            }
            525 => {
                let rows = app.rects.keys().filter(|k| k.starts_with("members:row:")).count();
                if rows < 3 || app.home.as_ref().is_none_or(|h| h.page != super::community::Page::Members) {
                    return;
                }
                self.shot(ctx, "home_members.png");
                let Some(r) = app.rects.get("members:search").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("ali".into())]);
                self.next(1500);
            }
            526 => {
                if app.home.as_ref().and_then(|h| h.members.list.as_ref()).is_some_and(|l| l.len() == 1) {
                    self.mark("home_members_search");
                    if let Some(r) = Self::at(ctx, "home_row:Channels & Roles") {
                        self.click(r.center());
                    }
                    self.next(1000);
                }
            }
            527 => {
                let Some(r) = app.rects.get("home:option:Puzzles").copied() else { return };
                self.shot(ctx, "home_customize.png");
                self.click(r.center());
                self.next(600);
            }
            528 => {
                let picked = app.home.as_ref().and_then(|h| h.onboarding.as_ref()).is_some_and(|o| o["responses"].as_array().is_some_and(|r| r.iter().any(|v| v == "7102")));
                if picked {
                    self.mark("home_onboarding");
                    // the unlisted step earlier added lfg back; start the browser with it off the list
                    app.store.opted.remove(&2101);
                    if let Some(r) = app.rects.get("home:tab:1").copied() {
                        self.click(r.center());
                    }
                    self.next(800);
                }
            }
            529 => {
                let Some(r) = app.rects.get("browse:chan:2101").copied() else { return };
                self.shot(ctx, "home_browse.png");
                self.click(r.center());
                self.next(600);
            }
            530 => {
                if app.store.opted.contains(&2101) {
                    self.mark_once("home_browse");
                    let Some(r) = app.rects.get(&format!("chan:{HANGOUT}")).copied() else { return };
                    hover(self, r.center());
                    self.next(500);
                }
            }
            // a voice channel's own chat, beside the call, without joining it
            531 => {
                let Some(r) = Self::at(ctx, &format!("row_btn:Open Chat:{HANGOUT}")) else { return };
                self.click(r.center());
                self.next(1000);
            }
            532 => {
                if !app.voice_chat || app.cur() != Some(HANGOUT) || !app.rects.contains_key("voice_chat") || app.voice.chan == Some(HANGOUT) {
                    return;
                }
                self.shot(ctx, "home_voice_chat.png");
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("hello vc".into())]);
                self.key(Key::Enter);
                self.next(1000);
            }
            533 => {
                if mine(app, HANGOUT, "hello vc") {
                    self.mark("home_voice_chat");
                    let Some(r) = app.rects.get("voice_chat:close").copied() else { return };
                    self.click(r.center());
                    self.next(500);
                }
            }
            534 => {
                if app.voice_chat {
                    return;
                }
                let Some(r) = app.rects.get(&format!("chan:{HANGOUT}")).copied() else { return };
                hover(self, r.center());
                self.next(500);
            }
            535 => {
                let Some(r) = Self::at(ctx, &format!("row_btn:Edit Channel:{HANGOUT}")) else { return };
                self.click(r.center());
                self.next(1200);
            }
            // its settings: a new name saved, then View Channel denied to everyone
            536 => {
                let Some(r) = Self::at(ctx, "chanset:name").filter(|_| app.chan_settings.as_ref().is_some_and(|s| !s.draft.is_null())) else { return };
                self.shot(ctx, "home_chanset.png");
                self.click(pos2(r.right() - 20.0, r.center().y));
                self.key(Key::End);
                self.queue.push_back(vec![Event::Text(" 2".into())]);
                self.next(600);
            }
            537 => {
                let Some(r) = app.rects.get("chanset:save").copied() else { return };
                self.shot(ctx, "home_chanset_unsaved.png");
                self.click(r.center());
                self.next(1000);
            }
            538 => {
                if app.chan_settings.as_ref().is_some_and(|s| !s.saving && s.base["name"] == "Hangout 2") && app.store.channels.get(&HANGOUT).is_some_and(|c| c.name == "Hangout 2") {
                    self.mark("home_chanset_save");
                    let Some(r) = app.rects.get("chanset:Permissions").copied() else { return };
                    self.click(r.center());
                    self.next(800);
                }
            }
            539 => {
                let Some(r) = Self::at(ctx, "perm:View Channel:-1") else { return };
                self.click(r.center());
                self.next(500);
            }
            540 => {
                let Some(r) = app.rects.get("chanset:save").copied() else { return };
                self.shot(ctx, "home_chanset_perms.png");
                self.click(r.center());
                self.next(1000);
            }
            541 => {
                let denied = app.store.channels.get(&HANGOUT).is_some_and(|c| c.overwrites.iter().any(|o| o.id == GAMERS && o.deny & crate::state::VIEW_CHANNEL != 0));
                if denied && app.chan_settings.as_ref().is_some_and(|s| !s.saving) {
                    self.mark("home_chanset_perms");
                    let Some(r) = app.rects.get("chanset:delete").copied() else { return };
                    self.click(r.center());
                    self.next(600);
                }
            }
            542 => {
                if app.confirm.is_none() {
                    return;
                }
                self.shot(ctx, "home_chanset_delete.png");
                self.key(Key::Enter);
                self.next(1200);
            }
            543 => {
                if app.store.channels.contains_key(&HANGOUT) || app.chan_settings.is_some() {
                    return;
                }
                self.mark("home_chanset_delete");
                // a profile card opened low down is pushed up to fit; a click high in it stays in it
                app.select_guild(Some(self.target.0));
                app.open(general);
                let s = ctx.content_rect();
                app.open_profile(101, self.target.0, pos2(s.center().x, s.bottom() - 40.0));
                self.next(1000);
            }
            544 => {
                let Some(h) = app.rects.get("profile_head").copied().filter(|r| r.is_positive() && app.profile.is_some()) else { return };
                self.click(pos2(h.left() + 150.0, h.top() + 30.0));
                self.next(500);
            }
            545 => {
                if app.profile.is_none() {
                    return self.fail("a click inside the profile card closed it".into());
                }
                self.mark("home_profile_stays");
                self.shot(ctx, "home_profile_box.png");
                let Some(r) = app.rects.get("profile_msg").copied() else { return };
                self.click(pos2(r.left() + 60.0, r.center().y));
                self.queue.push_back(vec![Event::Text("hey from profile".into())]);
                self.key(Key::Enter);
                self.next(1200);
            }
            546 => {
                if app.profile.is_none() && app.cur() == Some(3000) && mine(app, 3000, "hey from profile") {
                    self.mark("home_profile_dm");
                    app.select_guild(Some(self.target.0));
                    app.open(general);
                    self.next(1500);
                }
            }
            // a click on the picture zooms it, another zooms back out
            547 => {
                let url = format!("{}/attachments/1/2/e2e.png", crate::api::cdn());
                app.viewer = Some((url.clone(), url, 800, 450));
                self.next(1000);
            }
            548 => {
                let Some(r) = app.rects.get("viewer_img").copied().filter(|_| app.viewer.is_some()) else { return };
                self.loaded = r.width() as usize;
                self.click(r.center());
                self.next(800);
            }
            549 => {
                let Some(z) = app.rects.get("viewer_zoomed").copied() else { return };
                if z.width() < self.loaded as f32 * 1.5 {
                    return self.fail(format!("the zoomed picture is {:.0} wide against {} fitted", z.width(), self.loaded));
                }
                self.mark("home_zoom");
                self.shot(ctx, "home_viewer_zoom.png");
                app.rects.remove("viewer_zoomed");
                self.click(ctx.content_rect().center());
                self.next(600);
            }
            550 => {
                if app.rects.contains_key("viewer_zoomed") || app.viewer.is_none() {
                    return;
                }
                self.mark("home_unzoom");
                self.key(Key::Escape);
                self.next(600);
            }
            // a poll in #general: scroll to it, pick an answer, press Vote
            551 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1000);
            }
            552 => {
                // scroll up until the whole poll card, down to its Vote button, is on screen
                if !app.rects.contains_key("poll_vote") || !app.rects.contains_key("poll_opt:2") {
                    if let Some(c) = app.rects.get("composer").copied() {
                        self.queue.push_back(vec![Event::PointerMoved(pos2(c.center().x, (c.top() + 90.0) / 2.0))]);
                        self.queue.push_back(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, 100.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: Modifiers::NONE,
                        }]);
                    }
                    return;
                }
                self.next(600);
            }
            553 => {
                let Some(r) = app.rects.get("poll_opt:2").copied() else { return };
                self.click(r.center());
                self.next(300);
            }
            554 => {
                let Some(r) = app.rects.get("poll_vote").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            555 => {
                let voted = app
                    .store
                    .msgs
                    .get(&general)
                    .and_then(|e| e.list.iter().find_map(|m| m.poll.as_ref()))
                    .is_some_and(|p| p.answers.iter().any(|a| a.id == 2 && a.me));
                if voted {
                    self.mark("home_poll");
                    self.shot(ctx, "home_poll.png");
                    self.next(300);
                }
            }
            _ => {
                if app.viewer.is_none() {
                    self.step = 99;
                }
            }
        }
    }

    /// Settings that follow the account: theme and friends, from user_settings_proto.
    fn sync_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::net::Tag;
        use crate::theme::Theme;
        let push = |app: &App, theme: u32, avatars: bool| {
            let body = json!({ "theme": theme, "compact_avatars": avatars });
            app.rest("POST", "/mock/settings-proto".into(), Some(body), Tag::Quiet);
        };
        match self.step {
            350 => {
                // the mock's READY says dark (Ash) with compact avatars on, which is not our default
                if !app.settings.compact_avatars {
                    return;
                }
                self.mark("sync_login");
                // a local edit of a synced setting, as the settings page would make it
                app.settings.compact_avatars = false;
                app.save_settings(ctx);
                push(app, 4, true);
                self.next(0);
            }
            351 => {
                if app.settings.theme != Theme::Onyx {
                    return;
                }
                self.mark("sync_live_theme");
                // the account still says true, same as at login, so the local false stays
                if !app.settings.compact_avatars {
                    self.mark("sync_local_kept");
                }
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(30.0, 300.0))]);
                self.next(500);
            }
            352 => {
                let Some(row) = app.rects.get("msg:last").copied() else { return };
                let shot = "sync_onyx.png".to_string();
                self.probes.push((shot.clone(), vec![("rail".into(), Probe::At(egui::pos2(2.0, row.center().y)))]));
                self.shot(ctx, &shot);
                self.next(400);
            }
            353 => {
                app.settings.theme = Theme::Dark;
                app.save_settings(ctx);
                push(app, 4, true);
                self.next(1000);
            }
            354 => {
                // account repeated Onyx: not a change, so the local Dark stays
                if app.settings.theme == Theme::Dark {
                    self.mark("sync_override_kept");
                }
                push(app, 1, true);
                self.next(0);
            }
            355 => {
                if app.settings.theme != Theme::Ash {
                    return;
                }
                self.mark("sync_back");
                self.step = 280;
            }
            _ => self.step = 280,
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
                if prop.as_deref() == Some("ggsans-400") || self.step_at.elapsed() > Duration::from_secs(12) {
                    self.rects.insert("look:font".into(), json!(prop));
                    self.rects.insert("look:font_medium".into(), json!(first(egui::FontFamily::Name("medium".into()))));
                    self.rects.insert("look:font_italic".into(), json!(first(egui::FontFamily::Name("bold-italic".into()))));
                    self.rects.insert("look:font_mono".into(), json!(first(egui::FontFamily::Monospace)));
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
