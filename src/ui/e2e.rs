//! Scripted run used by the end-to-end harness (RUSTCORD_E2E=<artifact dir>).
//! Everything goes through egui's normal input path, so a click here is a click.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, ViewportCommand, pos2, vec2};
use serde_json::json;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod activity;
mod bots;
mod embed;
mod call;
mod web;
mod buy;
mod shop;
mod stage;
mod clips;
mod composer;
mod fixes;
mod motion;
mod members;
mod tour;
pub use tour::scrolled;
mod login;
mod camera;
mod gallery;
mod watchdog;
pub(in crate::ui) mod stress;
mod regressions;
mod profcheck;
mod replay;

pub use watchdog::beat;
mod chatx;
mod forum;
mod stickers;
mod chatcheck;
mod p1b;

use super::{App, side};
use crate::net::Conn;
use crate::state::Id;
use crate::voice::{State, audio};

mod menus;
mod more;
mod players;
mod mathfile;
mod perms;
mod p3b;
mod p4b;
mod p4c;
mod p5;
mod p5b;
mod themes;
mod serverx;
mod shopx;
mod tidy;
mod refusals;
mod live;
mod local;


// sidebar entries that sit low enough to need a scroll down to reach
const PAGES_BELOW: [&str; 10] = [
    "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time", "Activity Privacy", "Connected Apps", "Developer",
    "Streamer Mode", "Log Out",
];

// the settings sidebar top to bottom
const SIDEBAR: [&str; 19] = [
    "Account", "Data & Privacy", "Messaging Permissions", "Notifications", "Clips", "Nitro", "Server Boost", "Subscriptions", "Gift Inventory",
    "Billing", "Voice & Video", "Appearance", "Accessibility", "System", "Language & Time", "Activity Privacy", "Connected Apps",
    "Developer", "Log Out",
];

mod profed;
mod spell_discover;
mod system;

pub struct Run {
    dir: PathBuf,
    t0: Instant,
    marks: Vec<(String, f64)>,
    step: u32,
    step_at: Instant,
    sub: u8,
    picking: bool,
    wait: Instant,
    queue: VecDeque<Vec<Event>>,
    frames: Vec<f32>,
    shots: Vec<String>,
    pending_shots: u32,
    voice: serde_json::Value,
    target: (Id, Id),
    call: Id,
    cues: usize,
    loaded: usize,
    text: String,
    error: Option<String>,
    done: bool,
    rects: serde_json::Map<String, serde_json::Value>,
    steady: Option<egui::Rect>,
    pub tour: bool,
    // RUSTCORD_BENCH: nothing scripted, the frame counts written down for e2e/bench.py
    bench: Option<Instant>,
    bench_io: Option<std::sync::Arc<BenchIo>>,
    spin: bool,
    scroll: bool,
    wheel_at: Instant,
    notches: u32,
    costs: Vec<f32>,
    chat_open: bool,
    travel: f32,
    last_off: f32,
    // the channels a "browse" file has us open one after another, the next one, and when
    browse: Option<(Vec<Id>, usize, Instant)>,
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
    pub call_toasts: Vec<String>,
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
    local: local::State,
    fx: live::State,
    sweep: tour::State,
    replay: replay::State,
    local_log: Option<String>,
}

/// A spot to read back from a screenshot: one pixel, or the brightest pixel in a rect (text colour).
enum Probe {
    At(Pos2),
    Bright(egui::Rect),
}

/// bench.py's flag files and the stats file, checked and written off the UI thread: done in the
/// frame, three file checks and a write cost 18% of an idle window's CPU, and bench.json counted it.
#[derive(Default)]
struct BenchIo {
    spin: std::sync::atomic::AtomicBool,
    scroll: std::sync::atomic::AtomicBool,
    browse: std::sync::atomic::AtomicBool,
    out: std::sync::Mutex<Option<String>>,
}

impl BenchIo {
    fn start(dir: PathBuf) -> std::sync::Arc<BenchIo> {
        use std::sync::atomic::Ordering::Relaxed;
        let io = std::sync::Arc::new(BenchIo::default());
        let me = io.clone();
        let _ = std::thread::Builder::new().name("bench-io".into()).spawn(move || {
            loop {
                me.spin.store(dir.join("spin").exists(), Relaxed);
                me.scroll.store(dir.join("scroll").exists(), Relaxed);
                me.browse.store(dir.join("browse").exists(), Relaxed);
                let out = me.out.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(s) = out {
                    let _ = std::fs::write(dir.join("bench.json"), s);
                }
                // bench.py waits a second after touching a flag; polling faster cost ~0.5% of a core
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        io
    }
}

impl Run {
    pub fn from_env(t0: Instant) -> Option<Run> {
        let tour = std::env::var_os("RUSTCORD_TOUR").is_some();
        let bench = std::env::var_os("RUSTCORD_BENCH").is_some();
        let dir = PathBuf::from(std::env::var_os("RUSTCORD_E2E").or_else(|| std::env::var_os("RUSTCORD_TOUR")).or_else(|| std::env::var_os("RUSTCORD_BENCH"))?);
        let _ = std::fs::create_dir_all(&dir);
        crate::gif::trace();
        watchdog::start(dir.clone());
        let bench_io = bench.then(|| BenchIo::start(dir.clone()));
        Some(Run {
            dir,
            t0,
            marks: Vec::new(),
            step: if std::env::var_os("RUSTCORD_E2E_LOGIN").is_some() { 1500 } else { 0 },
            step_at: Instant::now(),
            sub: 0,
            picking: false,
            wait: Instant::now(),
            queue: VecDeque::new(),
            frames: Vec::new(),
            shots: Vec::new(),
            pending_shots: 0,
            voice: serde_json::Value::Null,
            target: (0, 0),
            call: 0,
            cues: 0,
            loaded: 0,
            text: format!("hello from the e2e run **bold** 🎉 #{}", std::process::id()),
            error: None,
            done: false,
            rects: Default::default(),
            steady: None,
            tour,
            bench: bench.then(Instant::now),
            bench_io,
            spin: false,
            scroll: false,
            wheel_at: Instant::now(),
            notches: 0,
            costs: Vec::new(),
            chat_open: false,
            travel: 0.0,
            last_off: 0.0,
            browse: None,
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
            call_toasts: Vec::new(),
            said: Vec::new(),
            heard: Vec::new(),
            y0: 0.0,
            shift: false,
            alt: false,
            live: std::env::var("RUSTCORD_LIVE").ok().and_then(|v| v.parse().ok()),
            live_at: None,
            samples: Vec::new(),
            local: Default::default(),
            fx: Default::default(),
            sweep: Default::default(),
            replay: Default::default(),
            local_log: None,
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
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("marks.log")) {
            let _ = writeln!(f, "{name}\t{t:.1}");
        }
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
        let Some((pings, title)) = self.hidden else {
            // minimized from outside, by something at the machine: step() would never run again
            if ctx.input(|i| i.viewport().minimized == Some(true)) {
                self.rects.insert("minimized_from_outside".into(), json!(self.step));
                crate::winsys::show_quietly(false);
            }
            return;
        };
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
        if self.scroll {
            self.costs.push(ms);
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
        if self.bench.is_some() && self.scroll {
            if let Some(r) = raw.screen_rect {
                raw.events.push(Event::PointerMoved(r.center()));
            }
            if Instant::now() >= self.wheel_at {
                self.wheel_at = Instant::now() + Duration::from_millis(90);
                self.notches += 1;
                let dy = if (self.notches / 12) % 2 == 0 { 1.0 } else { -1.0 };
                raw.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, dy), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE });
            }
        }
        // an event every frame is a frame drawn every vsync, which a measurement mustn't have
        if self.bench.is_none() && std::env::var_os("RUSTCORD_E2E_HOLD").is_none() {
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
        if self.step == 4327 {
            let n = ctx.repaint_causes().iter().filter(|c| c.file.ends_with("gif.rs") || c.file.ends_with("lottie.rs")).count();
            let k = self.rects.get("lottie_idle_wakes").and_then(|v| v.as_u64()).unwrap_or(0) + n as u64;
            self.rects.insert("lottie_idle_wakes".into(), json!(k));
        }
        if let Some(at) = self.bench {
            for c in ctx.repaint_causes() {
                *self.causes.entry(format!("{}:{} {}", c.file.rsplit(['/', '\\']).next().unwrap_or(c.file), c.line, c.reason)).or_default() += 1;
            }
            if self.spin {
                ctx.request_repaint();
            }
            if self.scroll
                && !self.chat_open
                && let Some(&g) = app.store.guild_order.first()
                && let Some(ch) = side::first_channel(&app.store, g)
            {
                self.chat_open = true;
                app.select_guild(Some(g));
                app.open(ch);
            }
            if self.scroll
                && let Some(off) = app.cur().and_then(|c| app.offsets.get(&c).copied())
            {
                self.travel += (off - self.last_off).abs();
                self.last_off = off;
            }
            if let Some((list, i, when)) = &mut self.browse
                && when.elapsed() > Duration::from_secs(3)
            {
                if let Some(ch) = list.get(*i).copied() {
                    let g = app.store.channels.get(&ch).map_or(0, |c| c.guild);
                    app.ui.guild = (g != 0).then_some(g);
                    app.open(ch);
                }
                *i += 1;
                *when = Instant::now();
            }
            if self.browse.is_some() {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
            if at.elapsed() > Duration::from_millis(100)
                && let Some(io) = self.bench_io.clone()
            {
                use std::sync::atomic::Ordering::Relaxed;
                if self.browse.is_none() && io.browse.load(Relaxed) {
                    self.browse = Some((app.ui.recent.clone(), 0, Instant::now() - Duration::from_secs(3)));
                }
                self.bench = Some(Instant::now());
                self.spin = io.spin.load(Relaxed);
                self.scroll = io.scroll.load(Relaxed);
                let marks: serde_json::Map<String, serde_json::Value> = self.marks.iter().map(|(k, v)| (k.clone(), json!(v.round()))).collect();
                let loaders = ctx.loaders();
                let mut images = serde_json::Map::new();
                for (id, n) in loaders.bytes.lock().iter().map(|l| (l.id().to_string(), l.byte_size()))
                    .chain(loaders.image.lock().iter().map(|l| (l.id().to_string(), l.byte_size())))
                    .chain(loaders.texture.lock().iter().map(|l| (l.id().to_string(), l.byte_size())))
                {
                    images.insert(id.rsplit("::").next().unwrap_or(&id).to_string(), json!(n >> 20));
                }
                let out = json!({ "frames": self.frames.len(), "ui_ms": self.frames.iter().sum::<f32>(), "marks_ms": marks, "slow_frames": self.slow, "causes": self.causes, "images_mb": images, "browsed": self.browse.as_ref().map(|b| b.1), "costs": self.costs, "travel": self.travel });
                *io.out.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.to_string());
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
        // holding still to be measured, only what the app itself asks for gets drawn, and who asked is written down
        if std::env::var_os("RUSTCORD_E2E_HOLD").is_some() {
            for c in ctx.repaint_causes() {
                *self.causes.entry(format!("{}:{}", c.file.rsplit(['/', '\\']).next().unwrap_or(c.file), c.line)).or_default() += 1;
            }
            let mut top: Vec<_> = self.causes.iter().collect();
            top.sort_by_key(|c| std::cmp::Reverse(*c.1));
            top.truncate(14);
            let _ = std::fs::write(self.dir.join("hold"), format!("{} {:?}", self.frames.len(), top));
        }
        match self.wait.checked_duration_since(Instant::now()).filter(|_| std::env::var_os("RUSTCORD_E2E_HOLD").is_some()) {
            Some(left) => ctx.request_repaint_after(left),
            None => ctx.request_repaint(),
        }
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
                ("source_pop", app.source_pop.is_some()), ("leaving", app.leaving.is_some()), ("new_thread", app.new_thread.is_some()), ("combos", app.combos.is_some()),
                ("settings", app.settings_page.is_some()), ("egui_popup", egui::Popup::is_any_open(ctx)), ("reply", app.reply.is_some()), ("timeout", app.timeout.is_some()),
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
            if std::env::var_os("RUSTCORD_REPLAY").is_some() {
                return self.replay_steps(app, ctx);
            }
            if std::env::var_os("RUSTCORD_TOUR_PROFILE").is_some() {
                return self.profile_check(app, ctx);
            }
            return self.tour_steps(app, ctx);
        }
        if (9000..9900).contains(&self.step) {
            return self.stress_steps(app, ctx);
        }
        if (8800..9000).contains(&self.step) {
            return self.regression_steps(app, ctx);
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
                let head = app.rects.get("guildmenu").map_or(0.0, |h| h.bottom());
                if r.top() < head {
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(r.center().x, head + 200.0))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 300.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    self.wait = Instant::now() + Duration::from_millis(400);
                    return;
                }
                self.click(r.center());
                self.next(50);
            }
            3 => {
                let (n, ready) = msgs(app, self.target.1);
                if app.cur() == Some(self.target.1) && ready && n > 0 {
                    self.mark("messages");
                    self.loaded = n;
                    self.next(1500);
                    if let Some(from) = std::env::var("RUSTCORD_E2E_FROM").ok().and_then(|v| v.parse().ok()) {
                        self.step = from;
                        // the step that sends a message is skipped, and alice's reply to it is what the toast steps read
                        if from > 5 {
                            app.rest("POST", "/mock/dm".into(), Some(json!({ "channel": 3000, "from": 101, "text": "hey, saw your message — got a sec?", "after_ms": 500 })), crate::net::Tag::Quiet);
                        }
                    }
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
                let thread = app.store.channels.values().find(|c| c.guild == g && c.is_thread() && c.name == "release planning").map(|c| c.id);
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
                    let s = ctx.content_rect();
                    self.click(pos2(s.left() + 36.0, s.bottom() - 36.0));
                    self.next(300);
                }
            }
            26 => {
                if app.settings_page.is_some() {
                    return self.fail("clicking outside settings didn't close it".into());
                }
                self.mark("settings_backdrop");
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
                // the glide can carry it off again: go back and find it
                let Some(r) = app.rects.get("reply").copied() else {
                    self.step = 35;
                    return;
                };
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
                let covered = ["game:panel", "userpanel"].iter().filter_map(|k| app.rects.get(*k)).map(|p| p.top()).fold(ctx.content_rect().bottom(), f32::min);
                if r.bottom() > covered {
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(r.center().x, covered - 100.0))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -300.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    self.wait = Instant::now() + Duration::from_millis(400);
                    return;
                }
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
            60 => self.step = 2800,
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
                // measuring a stream on its own: no camera going out beside it
                if std::env::var_os("RUSTCORD_E2E_HOLD").is_some() {
                    self.step = 65;
                    return;
                }
                let Some(r) = app.rects.get("voice:camera").copied() else { return };
                self.click(r.center());
                self.step = 2819;
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
                    self.next(std::env::var("RUSTCORD_E2E_HOLD").ok().and_then(|v| v.parse().ok()).unwrap_or(500));
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
                    if self.share_at.is_none() {
                        self.share_at = Some(Instant::now());
                    }
                    if self.share_at.is_some_and(|t| t.elapsed() > Duration::from_millis(600)) {
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
                    self.step = 699;
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
                    self.step = 900;
                }
            }
            900..=919 => self.web_steps(),
            3200..=3299 => self.shop_steps(app, ctx),
            4200..=4299 => self.buy_steps(app, ctx),
            2200..=2299 => self.activity_steps(app, ctx),
            3500..=3599 => self.bot_steps(app, ctx),
            3300..=3399 => self.embed_steps(app, ctx),
            100..=199 => self.settings_steps(app, ctx),
            700..=759 => self.stage_steps(app, ctx),
            2100..=2199 => self.call_steps(app, ctx),
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
            600..=699 => self.server_steps(app, ctx),
            2000..=2059 => self.account_steps(app, ctx),
            2700..=2799 => self.system_steps(app, ctx),
            560..=599 => self.composer_steps(app, ctx),
            800..=839 => self.messages_steps(app, ctx),
            1200..=1219 => self.people_steps(app, ctx),
            1300..=1399 => self.item_steps(app, ctx),
            1400..=1419 => self.scroll_steps(app, ctx),
            1700..=1719 => self.spell_steps(app, ctx),
            2800..=2829 => self.camera_steps(app, ctx),
            2900..=2959 => self.clips_steps(app, ctx),
            4100..=4199 => self.gallery_steps(app, ctx),
            4300..=4319 => self.forum_steps(app, ctx),
            4320..=4339 => self.sticker_steps(app, ctx),
            4340..=4364 => self.chat_check_steps(app, ctx),
            4365..=4399 => self.p1b_steps(app, ctx),
            4400..=4499 => self.perms_steps(app, ctx),
            4500..=4599 => self.themes_steps(app, ctx),
            4600..=4649 => self.p3b_steps(app, ctx),
            4650..=4699 => self.shopx_steps(app, ctx),
            5000..=5049 => self.p4b_steps(app, ctx),
            4700..=4799 => self.tidy_steps(app, ctx),
            5050..=5099 => self.p4c_steps(app, ctx),
            5100..=5119 => self.p5_steps(app, ctx),
            5120..=5199 => self.p5b_steps(app, ctx),
            4800..=4899 => self.serverx_steps(app, ctx),
            3600..=3699 => self.gateway_steps(app, ctx),
            3000..=3099 => self.fixes_steps(app, ctx),
            3110..=3189 => self.members_steps(app, ctx),
            3700..=3799 => self.chatx_steps(app, ctx),
            3900..=4099 => self.profed_steps(app, ctx),
            1720..=1739 => self.discover_steps(app, ctx),
            1500..=1599 => self.login_steps(app, ctx),
            1800..=1899 => self.player_steps(app, ctx),
            2300..=2319 => self.sound_steps(),
            1900..=1999 => self.more_steps(app, ctx),
            2400..=2499 => self.voice_steps(app, ctx),
            2500..=2549 => self.person_steps(app, ctx),
            1600..=1699 => self.local_steps(app, ctx),
            2600..=2619 => self.motion_steps(app, ctx),
            3400..=3449 => self.refusal_steps(app, ctx),
            3800..=3899 => self.menu_steps(app, ctx),
            _ => {
                if self.pending_shots == 0 {
                    if !self.rects.contains_key("layout_side_final") {
                        self.rects.insert("layout_side_final".into(), json!(if app.ui.side_w > 0.0 { app.ui.side_w } else { 302.0 }));
                        self.rects.insert("layout_members_final".into(), json!(if app.ui.members_w > 0.0 { app.ui.members_w } else { 264.0 }));
                        if std::env::var_os("RUSTCORD_E2E_MEMBERS").is_some() {
                            app.rest("POST", "/mock/account-reset".into(), Some(json!({})), crate::net::Tag::Quiet);
                        }
                        self.wait = Instant::now() + Duration::from_millis(1000);
                        return;
                    }
                    self.finish(ctx);
                }
            }
        }
    }

    /// Click a settings sidebar entry, scrolling the sidebar first if it's below the fold.
    /// Marks are kept for rows scrolled out of the settings window, where a click would miss.
    fn click_mark(&mut self, ctx: &egui::Context, key: &str, pause: u64) {
        let window = ctx.content_rect().shrink2(egui::vec2(0.0, 80.0));
        let Some(r) = Self::at(ctx, key) else { return };
        if !window.contains_rect(r) {
            self.queue.push_back(vec![Event::PointerMoved(window.center())]);
            self.queue.push_back(vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, if r.top() > window.top() { -200.0 } else { 200.0 }),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }]);
            self.wait = Instant::now() + Duration::from_millis(250);
            return;
        }
        self.click(r.center());
        self.next(pause);
    }

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

    /// Opens a dropdown and clicks one of its items; true once the item is clicked.
    fn pick(&mut self, ctx: &egui::Context, head: &str, item: &str) -> bool {
        if !self.picking {
            let Some(r) = Self::at(ctx, head) else { return false };
            ctx.data_mut(|d| d.remove::<(egui::Rect, u64)>(egui::Id::new(("e2e", item))));
            self.click(r.center());
            self.picking = true;
            self.wait = Instant::now() + Duration::from_millis(500);
            return false;
        }
        let Some(r) = Self::at(ctx, item) else { return false };
        self.click(r.center());
        self.picking = false;
        true
    }

    fn at(ctx: &egui::Context, key: &str) -> Option<egui::Rect> {
        let (r, pass) = ctx.data(|d| d.get_temp::<(egui::Rect, u64)>(egui::Id::new(("e2e", key))))?;
        (pass == ctx.cumulative_pass_nr()).then_some(r)
    }

    fn said(ctx: &egui::Context, key: &str) -> Option<String> {
        let (text, pass) = ctx.data(|d| d.get_temp::<(String, u64)>(egui::Id::new(("e2e_note", key))))?;
        (pass == ctx.cumulative_pass_nr()).then_some(text)
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
                let editor = PAGES[self.loaded] == "Profiles";
                if (editor && app.profed.is_none()) || (!editor && app.settings_page.map(crate::ui::settings::label) != Some(PAGES[self.loaded])) {
                    return self.fail(format!("clicked {} but another page is open", PAGES[self.loaded]));
                }
                let slug: String = PAGES[self.loaded].to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                self.shot(ctx, &format!("settings_{slug}.png"));
                app.profed = None;
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
                let saved = app.profed.as_ref().is_some_and(|e| e.ready && !super::profed::dirty(e) && !e.saving);
                if saved {
                    self.mark("profile_saved");
                    app.profed = None;
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
                let label = if app.rects.contains_key("settings:Visual Density") { "Visual Density" } else { "Accessibility" };
                if self.open_page(app, label) && label == "Visual Density" {
                    self.next(800);
                }
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
                let label = if app.rects.contains_key("settings:Visual Density") { "Visual Density" } else { "Accessibility" };
                if self.open_page(app, label) && label == "Visual Density" {
                    self.next(800);
                }
            }
            119 => {
                let Some(r) = Self::at(ctx, "radio:Cozy") else { return };
                self.click(r.center());
                self.next(300);
            }
            120 => {
                let label = if app.settings_page == Some(crate::ui::settings::Page::Appearance) { "Streamer Mode" } else { "Appearance" };
                if self.open_page(app, label) && label == "Streamer Mode" {
                    self.next(800);
                }
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
                self.step = 124;
                self.next(900);
            }
            125 => {
                if !self.open_page(app, "Language & Time") {
                    return;
                }
                self.next(600);
            }
            126 => {
                if !self.pick(ctx, "lang", "lang:de") {
                    return;
                }
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
                self.shot(ctx, "settings_account_family.png");
                self.click_mark(ctx, "house:Bravery", 600);
            }
            130 => {
                self.mark("hypesquad");
                if !self.open_page(app, "Language & Time") {
                    return;
                }
                self.next(600);
            }
            131 => {
                if !self.pick(ctx, "lang", "lang:en-US") {
                    return;
                }
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
                let Some(r) = Self::at(ctx, "edit:Username") else { return };
                self.click(r.center());
                self.next(500);
            }
            135 => {
                let (Some(r), Some(p)) = (Self::at(ctx, "modal_input"), Self::at(ctx, "modal_password")) else { return };
                if self.sub == 0 {
                    self.click(r.right_center() - vec2(4.0, 0.0));
                    self.queue.push_back(vec![Event::Text("_".into())]);
                    self.sub = 1;
                    self.wait = Instant::now() + Duration::from_millis(800);
                    return;
                }
                self.sub = 0;
                self.click(p.center());
                self.queue.push_back(vec![Event::Text("hunter2".into())]);
                self.next(400);
            }
            136 => {
                let Some(r) = Self::at(ctx, "modal_done") else { return };
                self.shot(ctx, "settings_modal.png");
                self.click(r.center());
                self.next(0);
            }
            137 => {
                if app.sform.modal_closed() && app.store.user(app.store.me).map(|u| u.username.as_str()) == Some("you_") {
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
                self.step = 1999;
                self.next(400);
            }
            141 => {
                self.step = 143;
                self.next(0);
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
            144 => {
                if !self.open_page(app, "Server Boost") {
                    return;
                }
                self.next(900);
            }
            145 => {
                if Self::at(ctx, "billing:boost_menu").is_none() || Self::at(ctx, "billing:faq8").is_none() {
                    return;
                }
                self.mark("boost_page");
                let Some(r) = Self::at(ctx, "billing:boost_banner") else { return };
                self.click(r.center());
                self.next(500);
            }
            146 => {
                if Self::at(ctx, "billing:Boost").is_none() {
                    return;
                }
                self.mark("boost_picker");
                self.key(Key::Escape);
                self.next(400);
            }
            147 => {
                if !self.open_page(app, "Gift Inventory") {
                    return;
                }
                self.next(900);
            }
            148 => {
                let Some(r) = Self::at(ctx, "gift_input") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("WUMP-AAAAA-BBBBB-CCCCC".into())]);
                self.next(400);
            }
            149 => {
                let Some(r) = Self::at(ctx, "billing:Redeem") else { return };
                self.click(r.center());
                self.next(800);
            }
            150 => {
                if Self::at(ctx, "gift_ok").is_none() {
                    return;
                }
                self.mark("gift_redeemed");
                self.next(0);
            }
            151 => {
                if !self.open_page(app, "Billing") {
                    return;
                }
                self.next(900);
            }
            152 => {
                let Some(r) = Self::at(ctx, "billing:Edit") else { return };
                self.click(r.center());
                self.next(600);
            }
            153 => {
                let Some(r) = Self::at(ctx, "billing:delete_source") else { return };
                self.mark("billing_edit");
                if let Some(d) = Self::at(ctx, "billing:make_default") {
                    self.click(d.center());
                    self.mark("billing_manage");
                    self.step = 155;
                    return self.next(600);
                }
                self.click(r.center());
                self.next(500);
            }
            154 => {
                if Self::at(ctx, "billing_confirm").is_none() {
                    return;
                }
                self.mark("billing_manage");
                self.key(Key::Escape);
                self.next(400);
            }
            155 => {
                self.key(Key::Escape);
                self.next(400);
            }
            156 => {
                if Self::at(ctx, "billing:Next").is_some() && Self::at(ctx, "billing:Previous").is_some() {
                    self.mark("billing_tx");
                }
                self.key(Key::Escape);
                self.step = 2699;
                self.next(400);
            }
            _ => self.step = 99,
        }
    }

    /// Account settings that talk to the second factor and the privacy blob: turn on the
    /// authenticator, log a device out through the ticket, then flip a few switches and read
    /// the blob back.
    fn account_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let mfa_on = app.store.me_raw["mfa_enabled"].as_bool() == Some(true);
        let click_at = |this: &mut Self, key: &str, pause: u64| this.click_mark(ctx, key, pause);
        let typed = |this: &mut Self, key: &str, text: &str| {
            let Some(r) = Self::at(ctx, key) else { return };
            this.click(r.center());
            this.queue.push_back(vec![Event::Text(text.into())]);
            this.next(300);
        };
        match self.step {
            2000 => {
                app.rest("POST", "/mock/account-reset".into(), Some(json!({})), crate::net::Tag::Quiet);
                app.remote.clear();
                self.step = 2009;
                self.next(400);
            }
            2010 => click_at(self, "crumb", 500),
            2011 => click_at(self, "nav:Multi-Factor Authentication", 700),
            2012 => click_at(self, "action:Enable Authenticator App", 600),
            2013 => typed(self, "modal_input", "123456"),
            2014 => click_at(self, "modal_done", 800),
            2015 => typed(self, "verify_input", "hunter2"),
            2016 => click_at(self, "verify_done", 0),
            2017 => {
                if mfa_on {
                    self.mark("mfa_on");
                    self.next(300);
                }
            }
            2018 => click_at(self, "modal_cancel", 500),
            2019 => click_at(self, "crumb", 500),
            2020 => click_at(self, "nav:Logged-in Devices", 900),
            2021 => click_at(self, "logout:bbb", 800),
            2022 => typed(self, "verify_input", "123456"),
            2023 => click_at(self, "verify_done", 0),
            2024 => {
                let left = match app.remote.get("sessions") {
                    Some(Some(Ok(v))) => v["user_sessions"].as_array().map(Vec::len),
                    _ => None,
                };
                if left == Some(2) {
                    self.mark("sessions_logout");
                    self.next(300);
                }
            }
            2025 => {
                if self.open_page(app, "Messaging Permissions") {
                    self.next(900);
                }
            }
            2026 if crate::ui::settings::proto_num(app, 6, 27) != Some(3) => {
                click_at(self, "radio:Filter all spam", 600);
                self.step = 2026;
            }
            2026 => click_at(self, "toggle:Message requests", 600),
            2027 => {
                if crate::ui::settings::proto_ids(app, 8, 16) > 0 && crate::ui::settings::proto_num(app, 6, 27) == Some(3) {
                    self.mark("messaging_privacy");
                    self.next(200);
                }
            }
            2028 => {
                if self.open_page(app, "Activity Privacy") {
                    self.next(900);
                }
            }
            2029 => click_at(self, "toggle_all", 600),
            2030 => {
                if crate::ui::settings::proto_ids(app, 8, 13) == app.store.guilds.len() {
                    self.mark("activity_servers");
                    self.next(200);
                }
            }
            2031 => click_at(self, "toggle:Allow friends to join my game", 600),
            2032 => {
                if crate::ui::settings::proto_flag(app, 8, 1) == Some(false) {
                    self.mark("activity_join");
                    self.next(200);
                }
            }
            2033 => {
                if self.open_page(app, "Data & Privacy") {
                    self.next(900);
                }
            }
            2034 => click_at(self, "action:Request all of my Data", 600),
            2035 => {
                if self.open_page(app, "Connected Apps") {
                    self.next(900);
                }
            }
            2036 => click_at(self, "provider:github", 500),
            2037 => click_at(self, "deauthorize:Dyno", 500),
            2038 => click_at(self, "disconnect:github", 600),
            2039 => {
                self.mark("connected_apps");
                click_at(self, "join:ghostie's graveyard", 400);
            }
            2040 => {
                if self.open_page(app, "Profiles") {
                    self.next(1200);
                }
            }
            2041 => click_at(self, "tile:decoration", 500),
            2042 => click_at(self, "collectible:7001", 300),
            2043 => click_at(self, "pick:apply", 300),
            2044 => {
                let Some(r) = Self::at(ctx, "save_changes") else { return };
                self.click(r.center());
                self.next(0);
            }
            2045 => {
                if app.store.user(app.store.me).and_then(|u| u.decoration.as_deref()) == Some("a_deco7001") {
                    self.mark("profile_decoration");
                    app.profed = None;
                    self.next(300);
                }
            }
            2046 => {
                if self.open_page(app, "Account") {
                    self.next(800);
                }
            }
            2047 => click_at(self, "action:Disable your account", 500),
            2048 => {
                if app.sform.modal_open() {
                    self.mark("owner_modal");
                    self.next(100);
                }
            }
            2049 => click_at(self, "modal_done", 400),
            2050 => {
                self.step = 140;
                self.next(200);
            }
            _ => self.step = 2050,
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
            226 => {
                app.select_guild(Some(1003));
                self.next(400);
            }
            227 => {
                if rect(app, "chan:2302").is_none() {
                    return;
                }
                app.chan_flag(2302, super::PINNED, true);
                self.next(400);
            }
            228 => {
                let (Some(head), Some(ch), Some(cat)) = (rect(app, "side:pinned"), rect(app, "chan:2302"), app.rects.iter().filter(|(k, _)| k.starts_with("cat:")).map(|(_, r)| r.top()).reduce(f32::min)) else { return };
                if head.bottom() <= ch.top() && ch.bottom() <= cat {
                    self.mark("side_pinned");
                }
                app.chan_flag(2302, super::PINNED, false);
                self.next(400);
            }
            229 => {
                if rect(app, "side:pinned").is_some() {
                    return;
                }
                self.mark("side_unpinned");
                super::lurk::start(app, 9000, 9001);
                self.next(400);
            }
            230 => {
                let Some(r) = rect(app, "lurk:join") else { return };
                if app.ui.guild != Some(9000) {
                    return;
                }
                self.mark("lurk_preview");
                self.shot(ctx, "lurk_preview.png");
                self.click(r.center());
                self.next(500);
            }
            231 => {
                if app.store.lurking.contains(&9000) {
                    return;
                }
                self.mark("lurk_join");
                app.store.apply("GUILD_DELETE", &json!({ "id": "9000" }));
                app.select_guild(Some(self.target.0));
                app.open(self.target.1);
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
        let drawn = |ctx: &egui::Context| egui::containers::panel::PanelState::load(ctx, egui::Id::new("members")).map_or(0.0, |s| s.outer_rect.width());
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
                if !self.rects.contains_key("layout_side_boot") {
                    self.rects.insert("layout_side_boot".into(), json!(side_w(app)));
                    self.rects.insert("layout_members_boot".into(), json!(members_w(app)));
                }
                if side_w(app) < 250.0 {
                    drag(self, pos2(72.0 + side_w(app), 400.0), 120.0);
                    self.wait = Instant::now() + Duration::from_millis(700);
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
            249 => {
                self.rects.insert("layout_window_w".into(), json!(ctx.content_rect().width()));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(940.0, ctx.content_rect().height())));
                self.next(600);
            }
            250 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.rects.insert("layout_members_squeezed".into(), json!(drawn(ctx)));
                self.click(r.center());
                self.next(300);
            }
            251 => {
                let w = self.rects["layout_window_w"].as_f64().unwrap_or(1280.0) as f32;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(w, ctx.content_rect().height())));
                self.next(600);
            }
            252 => {
                self.rects.insert("layout_members_back".into(), json!(drawn(ctx)));
                self.rects.insert("layout_members_kept".into(), json!(members_w(app)));
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 330 };
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
                save(self, app, "codeblock_tinted");
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
        if self.cache_guilds.is_none() {
            // the snapshot written by the previous build, before READY replaces it
            self.cache_guilds = Some(app.store.guilds.len());
            self.mark("cache_checked");
        }
        let cur_msgs = |app: &App| app.cur().and_then(|c| app.store.msgs.get(&c)).map(|e| (e.list.len(), e.fresh && !e.loading)).unwrap_or((0, false));
        let skip_after = |s: &Self, secs: u64| s.step_at.elapsed() > Duration::from_secs(secs);
        if self.step == 0 {
            self.tour_setup(app, ctx);
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
                self.step = 3100;
                self.wait = Instant::now() + Duration::from_millis(800);
            }
            3100..=3109 => self.sweep_steps(app, ctx),
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
                if let Some(tip) = app.rects.get("inbox:tip").copied().filter(|_| !app.settings.inbox_tip) {
                    self.shot(ctx, "inbox_unreads.png");
                    self.click(tip.center());
                    self.wait = Instant::now() + Duration::from_millis(500);
                    return;
                }
                if app.settings.inbox_tip {
                    self.mark("inbox_tip_dismissed");
                }
                self.shot(ctx, "inbox_unreads_after_tip.png");
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
                match Self::at(ctx, &format!("menu_row:{}", "Server Boost")) {
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
                self.shot(ctx, "quick_before.png");
                self.rclick(r.center());
                self.next(600);
            }
            400 => {
                if !matches!(app.picker, Some((super::Pick::Quick(3), _))) {
                    self.shot(ctx, "quick_after.png");
                    let at = app.rects.get("quick:3").copied();
                    return self.fail(format!("right-clicking a quick reaction didn't open the picker for it: menu={} picker={} tile={at:?}", app.menu.is_some(), app.picker.is_some()));
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
                if let Err(error) = self.fullscreen_edges() {
                    if self.step_at.elapsed() > Duration::from_secs(3) {
                        return self.fail(error);
                    }
                    return;
                }
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
                if std::env::var_os("RUSTCORD_E2E_TO").is_some() {
                    self.step = 9999;
                }
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
                if self.sub == 0 {
                    self.shot(ctx, "server_roles.png");
                    self.mark("server_settings");
                    self.sub = 1;
                }
                match super::server::probe(app, self.sub - 1) {
                    super::server::Probe::Wait => return,
                    super::server::Probe::Shot(name) => {
                        self.shot(ctx, name);
                        self.sub += 1;
                        self.step_at = Instant::now();
                        return;
                    }
                    super::server::Probe::Done => self.mark("server_pages"),
                }
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
                app.invite_people(2400, "Guild Context Menu");
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
                self.step = 1200;
            }
            _ => self.step = 99,
        }
    }

    /// The profile family: Active Now's cards, the popout's Mod View, role add and remove, the
    /// full profile and its tabs, a server tag's popout and a role mention's.
    fn people_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        match self.step {
            1200 => {
                use crate::state::{BAN_MEMBERS, KICK_MEMBERS, MANAGE_ROLES, MODERATE_MEMBERS};
                if let Some(r) = app.store.guilds.get_mut(&self.target.0).and_then(|g| g.roles.get_mut(&1101)) {
                    r.perms |= MANAGE_ROLES | KICK_MEMBERS | BAN_MEMBERS | MODERATE_MEMBERS;
                }
                let now = chrono::Utc::now().timestamp_millis();
                app.store.apply("PRESENCE_UPDATE", &json!({ "user": { "id": "101" }, "status": "online", "activities": [
                    { "type": 2, "name": "Spotify", "details": "Farewell", "state": "marQ", "sync_id": "abc", "timestamps": { "start": now - 99000, "end": now + 14000 }, "assets": { "large_image": "spotify:xyz" } },
                    { "type": 0, "name": "Rust Simulator", "application_id": "900", "details": "In the lab", "state": "Solo", "timestamps": { "start": now - 1000000 } },
                ] }));
                app.store.apply("VOICE_STATE_UPDATE", &json!({ "guild_id": self.target.0.to_string(), "user_id": "101", "channel_id": "2021" }));
                app.select_guild(None);
                app.friends = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                self.next(1200);
            }
            1201 => {
                let (Some(card), Some(bodies)) = (Self::at(ctx, "now_card:101"), Self::said(ctx, "now_rows:101").and_then(|n| n.parse::<usize>().ok())) else { return };
                if bodies < 3 || card.height() < 68.0 + 3.0 * 68.0 {
                    return self.fail(format!("alice's Active Now card had {bodies} rows over {}px", card.height()));
                }
                self.shot(ctx, "active_now.png");
                self.mark("active_now_cards");
                self.next(400);
                self.step = 1211;
            }
            1211 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
                app.friends = false;
                app.select_guild(Some(self.target.0));
                app.open_profile(101, self.target.0, pos2(300.0, 160.0));
                self.step = 1201;
                self.next(900);
            }
            1202 => {
                if app.profiles.get(&101).is_none_or(|p| p.is_null()) {
                    return;
                }
                let Some(b) = Self::at(ctx, "banner:Open in Mod View") else { return };
                self.shot(ctx, "profile_popout_extras.png");
                self.click(b.center());
                self.next(700);
            }
            1203 => {
                if !app.mod_view.as_ref().is_some_and(|m| m.msgs.is_some()) {
                    return;
                }
                let Some(b) = Self::at(ctx, "role_add") else { return };
                self.shot(ctx, "modview.png");
                self.mark("modview");
                self.click(b.center());
                self.next(500);
            }
            1204 => {
                let Some(r) = Self::at(ctx, "role_pick") else { return };
                self.shot(ctx, "role_picker.png");
                self.click(r.center());
                self.next(600);
            }
            1205 => {
                if !app.store.members.get(&self.target.0).and_then(|m| m.get(&101)).is_some_and(|m| m.roles.contains(&1102)) {
                    return;
                }
                self.mark("role_added");
                let Some(x) = Self::at(ctx, "role_x:1102") else { return };
                self.click(x.center());
                self.next(600);
            }
            1206 => {
                if app.store.members.get(&self.target.0).and_then(|m| m.get(&101)).is_some_and(|m| m.roles.contains(&1102)) {
                    return;
                }
                self.mark("role_removed");
                app.mod_view = None;
                app.open_full(101, self.target.0, super::profile::Tab::Activity);
                self.next(900);
            }
            1207 => {
                if app.profiles.get(&101).is_none_or(|p| p.is_null()) || app.outbox.get(&101).is_none_or(|o| o.is_empty()) || app.mod_view.is_some() {
                    return;
                }
                let Some(t) = Self::at(ctx, &format!("fp_tab:{}", super::profile::Tab::Friends as u8)) else { return };
                self.shot(ctx, "full_profile.png");
                self.mark("full_profile");
                self.click(t.center());
                self.next(500);
            }
            1208 => {
                if !app.full_profile.as_ref().is_some_and(|f| f.tab == super::profile::Tab::Friends) {
                    return self.fail("the Mutual Friends tab didn't open".into());
                }
                self.shot(ctx, "full_profile_friends.png");
                self.mark("full_profile_tabs");
                self.key(Key::Escape);
                app.tag_pop = Some((1001, pos2(400.0, 300.0)));
                self.next(900);
            }
            1209 => {
                if app.guild_profiles.get(&1001).is_none_or(|p| p.is_null()) {
                    return;
                }
                self.shot(ctx, "tag_popout.png");
                self.mark("tag_popout");
                app.tag_pop = None;
                app.role_pop = Some((self.target.0, 1100, pos2(400.0, 300.0)));
                self.next(600);
            }
            1210 => {
                self.shot(ctx, "role_popout.png");
                self.mark("role_popout");
                app.role_pop = None;
                if let Some(r) = app.store.guilds.get_mut(&self.target.0).and_then(|g| g.roles.get_mut(&1101)) {
                    r.perms = 0;
                }
                app.store.apply("PRESENCE_UPDATE", &json!({ "user": { "id": "101" }, "status": "online", "activities": [] }));
                app.store.apply("VOICE_STATE_UPDATE", &json!({ "guild_id": self.target.0.to_string(), "user_id": "101", "channel_id": null }));
                self.step = 420;
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
        let menu_row = |label: &str| Self::at(ctx, &format!("menu_row:{}", label));
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
                    // the note you keep on alice, from her full profile
                    if app.full_profile.is_none() {
                        app.open_full(101, 0, super::profile::Tab::Activity);
                    }
                    let Some(r) = app.rects.get("full_note").copied().filter(|r| *r != egui::Rect::NOTHING) else { return };
                    self.click(r.center());
                    self.queue.push_back(vec![Event::Text("likes rust".into())]);
                    self.key(Key::Enter);
                    self.next(500);
                }
            }
            485 => {
                if app.notes.get(&101).is_some_and(|n| n == "likes rust") {
                    self.mark("qol_note");
                    app.full_profile = None;
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
                if Self::at(ctx, "ddr_lit:0").is_some() {
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
                // alice only sits in the lounge once an earlier step has put her there, server muted, and the stage steps took carol's own mute off
                for d in [
                    json!({ "guild_id": self.target.0.to_string(), "user_id": "101", "channel_id": "2021", "mute": true }),
                    json!({ "guild_id": self.target.0.to_string(), "user_id": "103", "channel_id": "2021", "self_mute": true, "self_deaf": true }),
                ] {
                    app.voice.dispatch(&app.net, app.store.me, "VOICE_STATE_UPDATE", &d);
                    app.store.apply("VOICE_STATE_UPDATE", &d);
                }
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1200);
            }
            // carol muted and deafened herself, alice was muted by the server
            521 => {
                let seen = ["voice_state:103:Muted", "voice_state:103:Deafened", "voice_state:101:Server Muted"].iter().all(|k| Self::at(ctx, k).is_some());
                if !seen && self.step_at.elapsed() > Duration::from_secs(2) && Instant::now() >= self.wait {
                    // the voice rows sit under the text channels: bring the sidebar down to them
                    self.queue.push_back(vec![Event::PointerMoved(pos2(150.0, 400.0))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -300.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    self.wait = Instant::now() + Duration::from_millis(400);
                }
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
                if let Some(s) = &mut app.chan_settings {
                    s.advanced = true;
                }
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
                    .is_some_and(|e| e.list.iter().filter_map(|m| m.poll.as_ref()).any(|p| p.answers.iter().any(|a| a.id == 2 && a.me)));
                if voted {
                    self.mark("home_poll");
                    self.shot(ctx, "home_poll.png");
                    self.next(300);
                }
            }
            _ => {
                if app.viewer.is_none() {
                    self.step = 600;
                }
            }
        }
    }

    fn type_end(&mut self, text: &str) {
        self.key(Key::End);
        self.queue.push_back(vec![Event::Text(text.into())]);
    }

    /// Server settings pages on the mock: edit, see the bar, save, and what went to discord.
    fn server_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use super::server::Page;
        let page = |app: &mut App, p: Page| {
            if let Some(s) = &mut app.server {
                s.page = p;
            }
        };
        let loaded = |app: &App, k: &str| app.server.as_ref().is_some_and(|s| s.data.get(k).is_some_and(|v| !v.is_null()));
        match self.step {
            600 => {
                app.open_server_settings(1001);
                self.next(800);
            }
            601 => {
                if !loaded(app, "profile") {
                    return;
                }
                let Some(r) = Self::at(ctx, "field:server_name") else { return };
                self.click(r.center());
                self.type_end(" X");
                self.next(800);
            }
            602 => {
                let Some(r) = Self::at(ctx, "bar:Save Changes") else { return };
                let name = app.server.as_ref().and_then(|s| s.draft.get("profile")).and_then(|d| d["name"].as_str().map(str::to_string)).unwrap_or_default();
                if !name.ends_with(" X") {
                    return self.fail(format!("server profile name draft is {name:?}"));
                }
                self.shot(ctx, "server_profile_dirty.png");
                self.mark("server_bar");
                self.click(r.center());
                self.next(600);
            }
            603 => {
                if app.server.as_ref().is_some_and(|s| s.dirty) || !app.server.as_ref().and_then(|s| s.data.get("profile")).is_some_and(|p| p["name"].as_str().is_some_and(|n| n.ends_with(" X"))) {
                    return;
                }
                self.mark("server_profile_saved");
                page(app, Page::Tag);
                self.next(500);
            }
            604 => {
                let Some(r) = Self::at(ctx, "field:tag") else { return };
                self.click(r.center());
                self.type_end("RUST");
                self.next(300);
            }
            605 => {
                let Some(r) = Self::at(ctx, "badge:sword") else { return };
                self.click(r.center());
                self.next(700);
            }
            606 => {
                let Some(r) = Self::at(ctx, "bar:Save Changes") else { return };
                self.shot(ctx, "server_tag.png");
                self.click(r.center());
                self.next(600);
            }
            607 => {
                if app.server.as_ref().is_some_and(|s| s.dirty) {
                    return;
                }
                self.mark("server_tag");
                page(app, Page::Engagement);
                self.next(600);
            }
            608 => {
                if !loaded(app, "guild") {
                    return;
                }
                let Some(r) = Self::at(ctx, "toggle:Send helpful tips for server setup.") else { return };
                self.click(r.center());
                self.next(700);
            }
            609 => {
                let Some(r) = Self::at(ctx, "bar:Save Changes") else { return };
                self.shot(ctx, "server_engagement.png");
                self.click(r.center());
                self.next(600);
            }
            610 => {
                if app.server.as_ref().is_some_and(|s| s.dirty) {
                    return;
                }
                self.mark("server_engagement");
                page(app, Page::Perks);
                self.next(500);
            }
            611 => {
                let Some(r) = Self::at(ctx, "toggle:Show Boost progress bar") else { return };
                self.shot(ctx, "server_perks.png");
                self.click(r.center());
                self.next(400);
            }
            612 => {
                self.mark("server_perks");
                page(app, Page::Soundboard);
                self.next(600);
            }
            613 => {
                let Some(r) = Self::at(ctx, "sound:airhorn") else { return };
                self.shot(ctx, "server_soundboard.png");
                self.click(egui::pos2(r.center().x, r.center().y));
                self.next(400);
            }
            614 => {
                if app.server.as_ref().is_none_or(|s| s.sound.is_none()) {
                    return;
                }
                let Some(r) = Self::at(ctx, "field:Name your sound") else { return };
                self.shot(ctx, "server_sound_edit.png");
                self.click(r.center());
                self.type_end("2");
                self.next(300);
            }
            615 => {
                let Some(r) = Self::at(ctx, "plate:Save") else { return };
                self.click(r.center());
                self.next(600);
            }
            616 => {
                let renamed = app.server.as_ref().and_then(|s| s.data.get("sounds")).is_some_and(|v| v["items"].as_array().is_some_and(|l| l.iter().any(|s| s["name"] == "airhorn2")));
                if !renamed {
                    return;
                }
                self.mark("server_soundboard");
                page(app, Page::Access);
                self.next(600);
            }
            617 => {
                if !loaded(app, "verification") || !loaded(app, "guild") {
                    return;
                }
                let Some(r) = Self::at(ctx, "join:Apply to Join") else { return };
                self.click(r.center());
                self.next(300);
            }
            618 => {
                let Some(r) = Self::at(ctx, "button:Add a question") else { return };
                self.click(r.center());
                self.next(300);
            }
            619 => {
                let Some(r) = Self::at(ctx, "question:0") else { return };
                self.click(r.center());
                self.type_end("Why do you want to join?");
                self.next(700);
            }
            620 => {
                let Some(r) = Self::at(ctx, "bar:Save Changes") else { return };
                self.shot(ctx, "server_access.png");
                self.click(r.center());
                self.next(700);
            }
            621 => {
                if app.server.as_ref().is_some_and(|s| s.dirty) {
                    return;
                }
                self.mark("server_access");
                app.open_server_settings(1002);
                page(app, Page::Community);
                self.next(600);
            }
            622 => {
                let Some(r) = Self::at(ctx, "button:Enable Community") else { return };
                self.shot(ctx, "server_enable_community.png");
                self.click(r.center());
                self.next(500);
            }
            623 => {
                let Some(r) = Self::at(ctx, "check:Verified email required.") else { return };
                self.click(r.center());
                self.next(200);
            }
            624 => {
                let Some(r) = Self::at(ctx, "check:Scan media content from all members.") else { return };
                self.click(r.center());
                self.next(300);
            }
            625 => {
                let Some(r) = Self::at(ctx, "plate:Next") else { return };
                self.shot(ctx, "server_community_setup.png");
                self.click(r.center());
                self.next(400);
            }
            626 => {
                if let Some(s) = &mut app.server {
                    if let Some(d) = s.draft.get_mut("community") {
                        d["rules"] = json!("2200");
                        d["updates"] = json!("2201");
                    }
                }
                let Some(r) = Self::at(ctx, "plate:Next") else { return };
                self.click(r.center());
                self.next(400);
            }
            627 => {
                let Some(r) = Self::at(ctx, "check:I agree to follow Discord's Community Guidelines.") else { return };
                self.click(r.center());
                self.next(300);
            }
            628 => {
                let Some(r) = Self::at(ctx, "plate:Finish Setup") else { return };
                self.click(r.center());
                self.next(700);
            }
            629 => {
                if app.server.as_ref().is_none_or(|s| s.sub != 4) {
                    return;
                }
                self.mark("server_community");
                if let Some(s) = &mut app.server {
                    s.sub = 0;
                    s.page = Page::Template;
                }
                self.next(500);
            }
            630 => {
                if !loaded(app, "template") {
                    return;
                }
                let Some(r) = Self::at(ctx, "field:template_title") else { return };
                self.click(r.center());
                self.type_end("Design fans");
                self.next(300);
            }
            631 => {
                let Some(r) = Self::at(ctx, "button:Generate Template") else { return };
                let me = app.store.me;
                if let Some(g) = app.store.guilds.get_mut(&1002) {
                    g.owner = me;
                }
                self.shot(ctx, "server_template.png");
                self.click(r.center());
                self.next(600);
            }
            632 => {
                if !app.server.as_ref().and_then(|s| s.data.get("template")).is_some_and(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
                    return;
                }
                self.mark("server_template");
                let Some(r) = Self::at(ctx, "side:Delete Server") else { return };
                if r.bottom() > ctx.content_rect().bottom() - 10.0 {
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(r.center().x, 300.0))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -300.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    return;
                }
                self.click(r.center());
                self.next(400);
            }
            633 => {
                if app.confirm.is_none() {
                    return;
                }
                self.shot(ctx, "server_delete.png");
                self.mark("server_delete");
                self.next(500);
            }
            634 => {
                app.confirm = None;
                app.server = None;
                if let Some(g) = app.store.guilds.get_mut(&1002) {
                    g.owner = 102;
                }
                self.step = 560;
                self.step_at = Instant::now();
            }
            _ => {}
        }
    }

    /// Messages: every kind discord draws specially, who reacted, each message menu, a failed send, Report
    /// Message, the search sort menu and the inbox's filter and friend requests.
    fn messages_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        const SYSTEM: Id = 2070;
        const LABELS: [&str; 22] = [
            "Add Reaction", "View Reactions", "Edit Message", "Reply", "Forward", "Create Thread", "Copy Text", "Pin Message", "Bookmark Message", "Create Reminder", "Apps",
            "Resend message", "Mark Unread", "Configure", "Copy Message Link", "Speak Message", "View Interaction Info", "Remove Reactions", "Remove All Reactions",
            "Delete Message", "Report Message", "Save File",
        ];
        let guild = self.target.0;
        let list = |app: &App| app.store.msgs.get(&SYSTEM).map(|e| e.list.clone()).unwrap_or_default();
        let row = |ctx: &egui::Context, label: &str| Self::at(ctx, &format!("menu_row:{}", label));
        let case = |app: &App, i: usize| -> Option<Id> {
            let l = list(app);
            let found = match i {
                0 => l.iter().find(|m| !m.reactions.is_empty()),
                1 => l.iter().find(|m| m.kind == 7 && m.author == 102),
                2 => l.iter().find(|m| m.kind == 20),
                3 => l.iter().find(|m| m.flags & 8192 != 0),
                4 => l.iter().find(|m| m.kind == 6),
                _ => None,
            };
            found.map(|m| m.id)
        };
        let open_menu = |me: &mut Self, app: &mut App, id: Id| {
            app.menu = Some((SYSTEM, id, egui::pos2(300.0, 100.0)));
            me.next(400);
        };
        let labels = |ctx: &egui::Context| LABELS.iter().filter(|l| row(ctx, l).is_some()).collect::<Vec<_>>();
        match self.step {
            800 => {
                app.select_guild(Some(guild));
                app.open(SYSTEM);
                self.next(1200);
            }
            801 => {
                let ready = app.store.msgs.get(&SYSTEM).is_some_and(|e| e.list.len() >= 29 && e.fresh && !e.loading);
                if app.cur() == Some(SYSTEM) && ready {
                    let mut texts = serde_json::Map::new();
                    for m in list(app).iter().filter(|m| !matches!(m.kind, 0 | 19 | 20 | 23)) {
                        let t = super::system::describe(&app.store, guild, m).map(|s| s.plain()).unwrap_or_default();
                        texts.insert(format!("{}:{}", m.kind, m.author), json!(t));
                    }
                    self.rects.insert("system_texts".into(), json!(texts));
                    self.mark("msg_system_texts");
                    self.shot(ctx, "msg_system.png");
                    self.next(300);
                }
            }
            802 => {
                let Some(id) = case(app, 0) else { return };
                let m = app.store.find_msg(SYSTEM, id).cloned().unwrap_or_default();
                for r in &m.reactions {
                    app.ask_reactors(SYSTEM, id, &r.emoji, false, r.count);
                }
                self.next(600);
            }
            803 => {
                let Some(id) = case(app, 0) else { return };
                let m = app.store.find_msg(SYSTEM, id).cloned().unwrap_or_default();
                let loaded = m.reactions.iter().all(|r| app.store.reactors.get(&(id, r.emoji.api_name(), false)).is_some_and(|e| e.1.is_some()));
                if loaded {
                    let tips: Vec<String> = m.reactions.iter().map(|r| super::reactions::tooltip(&app.store, guild, &m, r, false)).collect();
                    self.rects.insert("reaction_tooltips".into(), json!(tips));
                    self.mark("msg_reaction_tooltip");
                    app.reactors = Some(super::reactions::Reactors::new(SYSTEM, id));
                    self.next(800);
                }
            }
            804 => {
                if app.reactors.is_some() {
                    self.mark("msg_reactors_modal");
                    self.shot(ctx, "msg_reactors.png");
                    app.reactors = None;
                    self.loaded = 0;
                    self.next(300);
                }
            }
            805 => {
                let Some(id) = case(app, self.loaded) else {
                    self.step = 808;
                    return;
                };
                open_menu(self, app, id);
            }
            806 => {
                self.rects.insert(format!("menu:{}", self.loaded), json!(labels(ctx)));
                if self.loaded == 0 {
                    self.shot(ctx, "msg_menu.png");
                }
                app.menu = None;
                self.loaded += 1;
                self.step = 805;
                self.wait = Instant::now() + Duration::from_millis(250);
            }
            808 => {
                app.send_text(SYSTEM, "e2e-fail-once".into());
                self.next(900);
            }
            809 => {
                let Some(m) = list(app).into_iter().find(|m| m.failed && m.content == "e2e-fail-once") else { return };
                self.mark("msg_failed");
                self.shot(ctx, "msg_failed.png");
                open_menu(self, app, m.id);
            }
            810 => {
                self.rects.insert("menu:failed".into(), json!(labels(ctx)));
                app.menu = None;
                let Some(m) = list(app).into_iter().find(|m| m.failed) else { return };
                app.resend(SYSTEM, m.id);
                self.next(900);
            }
            811 => {
                let me = app.store.me;
                if list(app).iter().any(|m| m.content == "e2e-fail-once" && m.author == me && !m.failed && !m.pending) {
                    self.mark("msg_resent");
                    self.next(200);
                }
            }
            812 => {
                let Some(id) = list(app).iter().find(|m| m.kind == 0 && m.author == 102).map(|m| m.id) else { return };
                app.open_report(SYSTEM, id);
                self.next(800);
            }
            813 | 814 | 815 | 817 => {
                let key = ["report_child0", "report_check0", "report_button", "", "report_button"][(self.step - 813) as usize];
                let Some(r) = Self::at(ctx, key) else { return };
                self.click(r.center());
                self.next(600);
            }
            816 => {
                if app.report.as_ref().is_some_and(|r| r.done()) {
                    self.mark("msg_report_sent");
                    self.next(300);
                }
            }
            818 => {
                if app.report.is_none() {
                    self.mark("msg_report_closed");
                    self.next(200);
                }
            }
            819 => {
                app.start_search(SYSTEM, "release".into());
                self.next(900);
            }
            820 => {
                let Some(r) = Self::at(ctx, "search_sort") else { return };
                self.click(r.center());
                self.next(500);
            }
            821 => {
                let Some(r) = row(ctx, "Oldest") else { return };
                self.click(r.center());
                self.next(800);
            }
            822 => {
                if app.search.as_ref().is_some_and(|s| s.sort == 1 && s.results.is_some()) {
                    self.mark("msg_search_sort");
                    self.shot(ctx, "msg_search_sort.png");
                    app.search = None;
                    self.next(200);
                }
            }
            823 => {
                app.toggle_inbox();
                self.next(900);
            }
            824 => {
                let Some(r) = app.rects.get("inbox:filter").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            825 => {
                let Some(r) = row(ctx, "Show Role Mentions") else { return };
                self.click(r.center());
                self.next(600);
            }
            826 => {
                if app.settings.inbox_roles {
                    return;
                }
                self.mark("msg_inbox_filter");
                app.settings.inbox_roles = true;
                let Some(r) = app.rects.get("inbox:friends").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            827 => {
                if app.inbox.as_ref().is_some_and(|i| i.friends) {
                    self.mark("msg_inbox_friends");
                    self.shot(ctx, "msg_inbox_friends.png");
                    app.inbox = None;
                    self.next(200);
                }
            }
            828 => {
                app.configure_joins(guild);
                if app.server.as_ref().is_some_and(|s| matches!(s.page, super::server::Page::Engagement)) {
                    self.mark("msg_configure");
                }
                app.server = None;
                self.step = 1300;
            }
            _ => self.step = 1300,
        }
    }

    /// Threads in split view and their menu, events managed, following an announcement channel and
    /// the quests hub, each against the mock's own server (1001 is ours).
    fn item_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let click_at = |this: &mut Self, key: &str| -> bool {
            let Some(r) = Self::at(ctx, key) else { return false };
            this.click(r.center());
            true
        };
        match self.step {
            1300 => {
                if app.conn != Conn::Online || app.store.guilds.is_empty() {
                    return;
                }
                app.select_guild(Some(1001));
                app.open(2100);
                self.next(700);
            }
            // shift on a channel-list thread opens it beside its parent, with who is in it
            1301 => {
                if !app.store.channels.contains_key(&2160) {
                    return;
                }
                app.open_thread(2160, true, true);
                self.next(700);
            }
            1302 => {
                let loaded = app.store.msgs.get(&2160).is_some_and(|e| !e.list.is_empty());
                if app.side_thread != Some(2160) || !loaded || app.store.thread_members.get(&2160).is_none_or(|l| l.len() != 3) {
                    return;
                }
                self.mark("thread_split");
                self.mark("thread_members");
                self.shot(ctx, "thread_split.png");
                if click_at(self, "side_thread:more") {
                    self.next(400);
                }
            }
            1303 => {
                if click_at(self, "thread_menu:Join Thread") {
                    self.next(600);
                }
            }
            1304 => {
                if !app.store.channels.get(&2160).is_some_and(|c| c.joined) {
                    return;
                }
                self.mark("thread_join");
                if click_at(self, "side_thread:more") {
                    self.next(400);
                }
            }
            1305 => {
                if click_at(self, "thread_menu:Close Thread") {
                    self.next(600);
                }
            }
            1306 => {
                if !app.store.channels.get(&2160).is_some_and(|c| c.archived) {
                    return;
                }
                self.mark("thread_close");
                if click_at(self, "side_thread:more") {
                    self.next(400);
                }
            }
            1307 => {
                if click_at(self, "thread_menu:Lock Thread") {
                    self.next(600);
                }
            }
            1308 => {
                if !app.store.channels.get(&2160).is_some_and(|c| c.locked) {
                    return;
                }
                self.mark("thread_lock");
                app.side_thread = None;
                app.open_events(1001);
                self.next(700);
            }
            // the Events window of a server we run: both events, a card's details with who's interested
            1309 => {
                if app.events.as_ref().is_none_or(|e| e.list.as_ref().is_none_or(|l| l.len() != 2)) {
                    return;
                }
                self.shot(ctx, "events_manage.png");
                let Some(r) = Self::at(ctx, "event_card:6200") else { return };
                self.click(r.min + vec2(120.0, 24.0));
                self.next(700);
            }
            1310 => {
                let users = app.events.as_ref().and_then(|e| e.users.get(&6200)).is_some_and(|u| u.as_ref().is_some_and(|l| l.len() == 2));
                if app.events.as_ref().is_none_or(|e| e.detail != Some(6200)) || !users {
                    return;
                }
                self.mark("event_detail");
                self.shot(ctx, "event_detail.png");
                self.key(Key::Escape);
                self.next(400);
            }
            1311 => {
                if app.events.as_ref().is_some_and(|e| e.detail.is_some()) {
                    return;
                }
                if click_at(self, "event_more:6201") {
                    self.next(400);
                }
            }
            1312 => {
                if click_at(self, "event_menu:Start Event") {
                    self.next(500);
                }
            }
            1313 => {
                if app.confirm.as_ref().is_some_and(|c| c.title == "Start Event") {
                    self.key(Key::Enter);
                    self.next(700);
                }
            }
            1314 => {
                if app.confirm.is_some() {
                    return;
                }
                self.mark("event_start");
                if click_at(self, "event_more:6200") {
                    self.next(400);
                }
            }
            1315 => {
                if click_at(self, "event_menu:Cancel Event") {
                    self.next(500);
                }
            }
            1316 => {
                if app.confirm.as_ref().is_some_and(|c| c.title == "Cancel Event?") {
                    self.key(Key::Enter);
                    self.next(900);
                }
            }
            1317 => {
                if app.confirm.is_some() || app.events.as_ref().is_none_or(|e| e.list.as_ref().is_none_or(|l| l.len() != 1)) {
                    return;
                }
                self.mark("event_cancel");
                if click_at(self, "event_more:6201") {
                    self.next(400);
                }
            }
            // Edit Event fills the wizard from the event
            1318 => {
                if click_at(self, "event_menu:Edit Event") {
                    self.next(500);
                }
            }
            1319 => {
                if app.create.as_ref().is_some_and(|f| f.edit == Some(6201) && f.name == "Map review" && !f.voice && f.location == "the community hall") {
                    self.mark("event_edit");
                    self.shot(ctx, "event_edit.png");
                    app.create = None;
                    self.next(300);
                }
            }
            // the Events row's menu mutes new events
            1320 => {
                let Some(r) = Self::at(ctx, "home_row:Events") else { return };
                self.rclick(r.center());
                self.next(400);
            }
            1321 => {
                if click_at(self, "events_menu:mute") {
                    self.next(600);
                }
            }
            1322 => {
                if !app.store.events_muted.contains(&1001) {
                    return;
                }
                self.mark("events_mute");
                app.select_guild(Some(1000));
                app.open(2002);
                self.next(700);
            }
            // an announcement channel: Follow in its header opens the dialog; Follow sends it
            1323 => {
                if app.cur() != Some(2002) {
                    return;
                }
                let Some(r) = app.rects.get("follow").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            1324 => {
                let Some(f) = &mut app.follow else { return };
                f.guild = Some(1001);
                f.target = Some(2100);
                self.next(300);
            }
            1325 => {
                self.shot(ctx, "follow.png");
                if click_at(self, "follow:Follow") {
                    self.next(700);
                }
            }
            1326 => {
                if app.follow.is_some() {
                    return;
                }
                self.mark("follow");
                app.publish_msg(2002, 1);
                self.next(400);
            }
            // the quests hub: accept one, claim the finished one
            1327 => {
                app.open_quests();
                self.next(800);
            }
            1328 => {
                let Some(q) = &app.quests else { return };
                if q.offered.as_ref().is_none_or(|l| l.len() != 3) || q.orbs != Some(1500) {
                    return;
                }
                self.mark("quests");
                self.shot(ctx, "quests.png");
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(800.0, 500.0))]);
                self.queue.push_back(vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -600.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                }]);
                self.next(900);
            }
            1329 => {
                if click_at(self, "quest:Accept Quest") {
                    self.next(900);
                }
            }
            1330 => {
                let enrolled = app.quests.as_ref().and_then(|q| q.offered.as_ref()).is_some_and(|l| l.iter().any(|q| q["id"] == "9002" && q["user_status"]["enrolled_at"].is_string()));
                if !enrolled {
                    return;
                }
                self.mark("quest_accept");
                if click_at(self, "quest:Claim Reward") {
                    self.next(900);
                }
            }
            1331 => {
                let Some(q) = &app.quests else { return };
                if q.claimed.as_ref().is_none_or(|l| l.len() != 1) || q.orbs != Some(2340) {
                    return;
                }
                self.mark("quest_claim");
                app.quests = None;
                self.step = 1400;
            }
            _ => self.step = 1400,
        }
    }

    /// The wheel and the paging keys move the chat as far as Chromium would.
    fn scroll_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let offset = |app: &App| app.offsets.get(&general).copied().unwrap_or(0.0);
        match self.step {
            1400 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1200);
            }
            1401 => {
                let Some(c) = app.rects.get("composer").copied() else { return };
                self.click(c.center());
                self.queue.push_back(vec![Event::PointerMoved(pos2(c.center().x, c.top() - 200.0))]);
                self.next(500);
            }
            // up and back once first, so the rows it crosses are measured and no height fix shifts the second pass
            1402 | 1403 | 1404 => {
                let dy = if self.step == 1403 { -1.0 } else { 1.0 };
                if self.step == 1404 {
                    self.y0 = offset(app);
                }
                self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, dy), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                self.next(700);
            }
            1405 => {
                let want = super::wheel::notch(ctx.zoom_factor(), ctx.content_rect().height());
                if (self.y0 - offset(app) - want).abs() < 2.0 {
                    self.mark("feel_wheel_notch");
                }
                self.y0 = offset(app);
                self.key(Key::PageUp);
                self.next(700);
            }
            _ => {
                let Some(c) = app.rects.get("composer").copied() else { return };
                let moved = self.y0 - offset(app);
                if moved > c.top() * 0.5 && moved < c.top() {
                    self.mark("feel_page_key");
                }
                self.step = 1700;
                if std::env::var_os("RUSTCORD_E2E_TO").is_some() {
                    self.step = 9999;
                }
            }
        }
    }

    /// Voice & Video: the mic test, input profiles, settings reaching the engine, a switch and
    /// a sound toggle clicked, and what goes to the account.
    fn voice_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::cache::{InputProfile, Noise};
        use crate::ui::settings::Page;
        use std::sync::atomic::Ordering::Relaxed;
        let sh = app.voice.shared.clone();
        let late = self.step_at.elapsed() > Duration::from_secs(3);
        let goto = |ctx: &egui::Context, name: &str| ctx.data_mut(|d| d.insert_temp(egui::Id::new("settings_goto"), name.to_string()));
        match self.step {
            2400 => {
                app.settings_page = Some(Page::Voice);
                self.next(900);
            }
            2401 => {
                let Some(r) = Self::at(ctx, "mic_test") else { return };
                self.click(r.center());
                self.next(600);
            }
            2402 => {
                if crate::voice::probe::testing() {
                    self.mark("voice_test_on");
                }
                let Some(r) = Self::at(ctx, "mic_test") else { return };
                self.click(r.center());
                self.next(600);
            }
            2403 => {
                if !crate::voice::probe::testing() {
                    self.mark("voice_test_off");
                }
                app.settings.input_profile = InputProfile::Studio;
                app.save_settings(ctx);
                self.next(300);
            }
            2404 => {
                if sh.open_mic.load(Relaxed) && !sh.echo.load(Relaxed) && sh.noise.load(Relaxed) == 0 {
                    self.mark("voice_studio");
                }
                app.settings.input_profile = InputProfile::Custom;
                app.save_settings(ctx);
                self.next(300);
            }
            2405 => {
                if !sh.open_mic.load(Relaxed) && sh.echo.load(Relaxed) {
                    self.mark("voice_custom");
                }
                let gen_before = sh.dev_gen.load(Relaxed);
                app.settings.attenuation = 40;
                app.settings.atten_self = true;
                app.settings.agc = false;
                app.settings.echo_cancel = false;
                app.settings.noise = Noise::Standard;
                app.settings.mic = Some("e2e microphone".into());
                app.save_settings(ctx);
                let applied = sh.atten.load(Relaxed) == 40 && sh.atten_self.load(Relaxed) && !sh.agc.load(Relaxed) && !sh.echo.load(Relaxed) && sh.noise.load(Relaxed) == 2;
                if applied {
                    self.mark("voice_engine_flags");
                }
                if sh.dev_gen.load(Relaxed) != gen_before {
                    self.mark("voice_device_live");
                }
                app.settings.reset_voice();
                app.save_settings(ctx);
                if app.settings.mic.is_none() && sh.atten.load(Relaxed) == 0 && sh.agc.load(Relaxed) && sh.echo.load(Relaxed) && sh.noise.load(Relaxed) == 1 {
                    self.mark("voice_reset");
                }
                goto(ctx, "Streaming");
                self.next(800);
            }
            2406 => {
                match Self::at(ctx, "toggle:Show Stream Previews") {
                    Some(r) => self.click(r.center()),
                    None if !late => return,
                    None => {}
                }
                self.next(800);
            }
            2407 => {
                if !app.settings.stream_previews {
                    self.mark("voice_previews_off");
                }
                goto(ctx, "Sounds");
                self.next(800);
            }
            2408 => {
                match Self::at(ctx, "sound:mute") {
                    Some(r) => self.click(r.center()),
                    None if !late => return,
                    None => {}
                }
                self.next(500);
            }
            2409 => {
                if app.settings.muted_sounds.iter().any(|m| m == "mute") {
                    self.mark("voice_sound_off");
                }
                app.settings.stream_previews = true;
                app.settings.muted_sounds.clear();
                app.save_settings(ctx);
                app.settings_page = None;
                self.step = 2500;
            }
            _ => self.step = 99,
        }
    }

    /// The person menu from a voice row, a friend row and a dm row, a user's volume reaching the
    /// mixer, Pin through the menu, switching accounts and the custom status.
    fn person_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::ui::usermenu::Origin;
        let row = |ctx: &egui::Context, label: &str| Self::at(ctx, &format!("menu_row:{label}"));
        let at = egui::pos2(400.0, 200.0);
        const VOICE: [&str; 8] = ["Profile", "Start a Call", "User Volume", "Mute", "Mute Soundboard", "Invite to Server", "Ignore", "Block"];
        const FRIEND: [&str; 5] = ["Profile", "Message", "Start a Call", "Add Friend Nickname", "Remove Friend"];
        const DM: [&str; 6] = ["Mark As Read", "Pin", "Profile", "Close DM", "Ignore", "Block"];
        match self.step {
            2500 => {
                app.friends = false;
                app.select_guild(Some(self.target.0));
                app.open(self.target.1);
                self.next(800);
            }
            2501 => {
                // the only server we can invite from is ours, and the activity steps met Alice there
                if let Some(den) = app.store.members.get_mut(&1008) {
                    den.remove(&101);
                }
                app.user_menu = Some((101, self.target.0, None, at));
                app.menu_from = Origin::Voice(2021);
                self.next(600);
            }
            2502 => {
                if let Some(l) = VOICE.into_iter().find(|l| row(ctx, l).is_none()) {
                    return self.fail(format!("a voice user's menu has no {l}"));
                }
                self.shot(ctx, "menu_voice_user.png");
                self.next(300);
            }
            2503 => {
                app.set_peer(101, crate::cache::Peer { volume: 70.0, ..Default::default() }, false);
                let gain = app.voice.shared.peers.lock().ok().and_then(|g| g.get(&101).copied());
                if gain != Some(0.7) {
                    return self.fail(format!("a user's volume never reached the mixer: {gain:?}"));
                }
                app.set_peer(101, Default::default(), false);
                app.user_menu = None;
                app.menu_from = Origin::Plain;
                self.mark("person_menu");
                self.next(300);
            }
            2504 => {
                app.select_guild(None);
                app.friends = true;
                app.user_menu = Some((102, 0, None, at));
                app.menu_from = Origin::Friends;
                self.next(600);
            }
            2505 => {
                if let Some(l) = FRIEND.into_iter().find(|l| row(ctx, l).is_none()) {
                    return self.fail(format!("a friend's menu has no {l}"));
                }
                self.shot(ctx, "menu_friend.png");
                self.next(300);
            }
            2506 => {
                app.user_menu = None;
                app.menu_from = Origin::Plain;
                self.mark("friend_menu");
                let Some(dm) = app.store.dms.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.kind == 1)) else { return self.fail("no dm to open the menu on".into()) };
                self.loaded = dm as usize;
                app.friends = false;
                let who = app.store.channels[&dm].recipients[0];
                app.user_menu = Some((who, 0, Some(dm), at));
                self.next(600);
            }
            2507 => {
                if let Some(l) = DM.into_iter().find(|l| row(ctx, l).is_none()) {
                    return self.fail(format!("a dm's menu has no {l}"));
                }
                self.shot(ctx, "menu_dm.png");
                let Some(r) = row(ctx, "Pin") else { return };
                self.click(r.center());
                self.next(400);
            }
            2508 => {
                if !app.store.pinned.contains(&(self.loaded as Id)) {
                    return;
                }
                self.mark("pin_dm");
                app.set_custom_status("mock status", None, 2);
                crate::cache::remember_account(&crate::cache::SavedAccount { id: 4242, name: "Second".into(), avatar: None }, "mock-token");
                app.accounts = crate::cache::accounts();
                app.switch_account(4242);
                self.next(600);
            }
            2509 => {
                if app.store.me == 0 || app.token.is_none() {
                    return;
                }
                if app.accounts.len() < 2 {
                    return self.fail("signing in didn't keep the account for Switch Accounts".into());
                }
                crate::cache::forget_account(4242);
                self.mark("accounts");
                self.next(300);
            }
            _ => self.step = 2600,
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
                if !(seen(103) && seen(105) && seen(106)) {
                    if self.step_at.elapsed() > Duration::from_secs(17) {
                        return self.fail("the seeded gradient authors never scrolled into view".into());
                    }
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(640.0, 400.0))]);
                    self.queue.push_back(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, 250.0),
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

impl Run {
    /// Discord's sounds: fetched from the mock, decoded, and queued for a play that
    /// RUSTCORD_SILENT keeps off the speakers. Without ffmpeg the mock has no files to serve, so
    /// the marks are left off instead of failing the run.
    fn sound_steps(&mut self) {
        let late = self.step_at.elapsed() > Duration::from_secs(8);
        match self.step {
            2300 => {
                if crate::cache::dir().join("sounds").join("message1.mp3").exists() || late {
                    if crate::sound::stats().0 > 0 {
                        self.mark("sound_fetched");
                    }
                    self.rects.insert("sound_queued_before".into(), json!(crate::sound::stats().2));
                    crate::sound::play("message");
                    self.next(0);
                }
            }
            2301 => {
                let (_, decoded, queued) = crate::sound::stats();
                if decoded > 0 && queued as u64 > self.rects.get("sound_queued_before").and_then(|v| v.as_u64()).unwrap_or(0) {
                    self.mark("sound_decoded");
                    self.mark("sound_queued");
                    self.next(0);
                } else if late {
                    self.next(0);
                }
            }
            _ => self.step = 2400,
        }
    }
}

static GUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn app_guilds() -> usize {
    GUILDS.load(std::sync::atomic::Ordering::Relaxed)
}
