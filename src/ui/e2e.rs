//! Scripted run used by the end-to-end harness (FASTCORD_E2E=<artifact dir>).
//! Everything goes through egui's normal input path, so a click here is a click.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, ViewportCommand, vec2};
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{App, side};
use crate::net::Conn;
use crate::state::Id;


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
    target: (Id, Id),
    loaded: usize,
    text: String,
    error: Option<String>,
    done: bool,
}

impl Run {
    pub fn from_env(t0: Instant) -> Option<Run> {
        let dir = PathBuf::from(std::env::var_os("FASTCORD_E2E")?);
        let _ = std::fs::create_dir_all(&dir);
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
            target: (0, 0),
            loaded: 0,
            text: format!("hello from the e2e run **bold** 🎉 #{}", std::process::id()),
            error: None,
            done: false,
        })
    }

    fn ms(&self) -> f64 {
        self.t0.elapsed().as_secs_f64() * 1000.0
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
        self.frames.push(d.as_secs_f32() * 1000.0);
    }

    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        // the harness launches us in the background; a user would be looking at the window
        raw.focused = true;
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
        for (name, img) in shots {
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
        if !self.queue.is_empty() || Instant::now() < self.wait {
            return;
        }
        if self.step < 99 && self.step_at.elapsed() > Duration::from_secs(20) {
            self.fail(format!("timed out at step {}", self.step));
        }
        let msgs = |app: &App, ch: Id| app.store.msgs.get(&ch).map(|e| (e.list.len(), e.fresh && !e.loading)).unwrap_or((0, false));
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
                self.step = 99;
            }
            _ => {
                if self.pending_shots == 0 {
                    self.finish(ctx);
                }
            }
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
        });
        let _ = std::fs::write(self.dir.join("results.json"), serde_json::to_string_pretty(&out).unwrap_or_default());
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}
