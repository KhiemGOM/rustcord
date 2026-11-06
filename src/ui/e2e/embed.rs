//! Video embeds in the mock's #embeds: a provider's page opening over its poster when clicked, the
//! web view following the chat as it scrolls and being clipped to it, hiding under dialogs, one at
//! a time, gone with its channel; and the proxied file playing in the inline player.

use egui::{Event, Key, Modifiers, Pos2, Rect, pos2, vec2};
use std::time::Duration;

use super::Run;
use crate::media::{self, Phase};
use crate::state::Id;
use crate::ui::{App, embedplay};

const GUILD: Id = 1000;
const EMBEDS: Id = 2062;
const GENERAL: Id = 2011;

fn msg_of(app: &App, needle: &str) -> Option<Id> {
    app.store.msgs.get(&EMBEDS)?.list.iter().find(|m| m.content.contains(needle)).map(|m| m.id)
}

fn poster(app: &App, msg: Id) -> Option<Rect> {
    app.rects.get(&format!("embed_poster:{msg}")).copied().filter(|r| r.is_positive() && r.height() > 100.0)
}

fn player(app: &App, msg: Id) -> Option<Rect> {
    app.rects.get(&format!("embed_player:{msg}")).copied().filter(|r| r.is_positive() && r.height() > 100.0)
}

fn frame_rects(app: &App, msg: Id) -> Option<(Rect, Rect)> {
    Some((app.rects.get(&format!("embed_frame:{msg}")).copied()?, app.rects.get("embed_clip").copied()?))
}

fn playing(app: &App, msg: Id) -> bool {
    app.embed.as_ref().is_some_and(|p| p.key.0 == msg && matches!(p.frame.status(), crate::webwin::child::Status::Up))
}

fn near(a: i32, b: i32) -> bool {
    (a - b).abs() <= 2
}

fn side(clip: Rect) -> Pos2 {
    pos2(clip.left() + 16.0, clip.center().y)
}

impl Run {
    fn scroll_by(&mut self, at: Pos2, dy: f32) {
        self.queue.push_back(vec![Event::PointerMoved(at)]);
        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, dy), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
    }

    fn window_shot(&self, name: &str) {
        use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos};
        let w = crate::winsys::WINDOW.load(std::sync::atomic::Ordering::Relaxed) as HWND;
        let (mut r, mut o): (RECT, POINT) = unsafe { (std::mem::zeroed(), POINT { x: 0, y: 0 }) };
        let lift = |z| unsafe { SetWindowPos(w, z, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
        lift(HWND_TOPMOST);
        std::thread::sleep(Duration::from_millis(250));
        unsafe {
            GetClientRect(w, &mut r);
            ClientToScreen(w, &mut o);
        }
        let img = crate::shareable::screen(o.x, o.y, (r.right - r.left).max(1) as usize, (r.bottom - r.top).max(1) as usize);
        lift(HWND_NOTOPMOST);
        let Some(img) = img else { return };
        let [iw, ih] = img.size;
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(buf) = image::RgbaImage::from_raw(iw as u32, ih as u32, bytes) {
            let _ = buf.save(self.dir.join(name));
        }
    }

    fn exits(&mut self) {
        self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 99 };
    }

    pub(super) fn embed_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        if self.step == 3300 {
            if app.conn != crate::net::Conn::Online || !app.store.channels.contains_key(&EMBEDS) {
                return;
            }
            app.select_guild(Some(GUILD));
            app.open(EMBEDS);
            return self.next(800);
        }
        let (Some(a), Some(b), Some(file)) = (msg_of(app, "mock1"), msg_of(app, "mock2"), msg_of(app, "medal.tv")) else { return };
        let probe = |app: &App| app.embed.as_ref().map(|p| p.frame.probe());
        match self.step {
            3301 => {
                if app.cur() != Some(EMBEDS) {
                    return;
                }
                let Some(r) = poster(app, b) else { return };
                if poster(app, file).is_some() || r.height() > 150.0 {
                    self.mark("embed_poster");
                    self.shot(ctx, "embed_poster.png");
                }
                self.click(r.center() - vec2(30.0, 0.0));
                self.next(0);
            }
            3302 => {
                if !playing(app, b) || !probe(app).is_some_and(|p| p.0) {
                    if self.step_at.elapsed() > std::time::Duration::from_secs(8) {
                        let at = app.embed.as_ref().map(|p| (p.key.0 == b, matches!(p.frame.status(), crate::webwin::child::Status::Up), p.frame.probe()));
                        self.fail(format!("the embed never came up: {at:?}"));
                    }
                    return;
                }
                self.mark("embed_frame_starts");
                self.next(3000);
            }
            3303 => {
                let Some((full, clip)) = frame_rects(app, b) else { return };
                let target = clip.top() + 30.0;
                if (full.top() - target).abs() > 6.0 {
                    self.scroll_by(side(clip), target - full.top());
                    self.next(500);
                    self.step -= 1;
                    return;
                }
                self.next(0);
            }
            3304 => {
                self.window_shot("embed_playing.png");
                let (Some((full, clip)), Some((shown, win, bounds))) = (frame_rects(app, b), probe(app)) else { return };
                let want = embedplay::px(full.intersect(clip), ppp);
                if shown && win.iter().zip(want).all(|(x, y)| near(*x, y)) && bounds[2] == win[2] && bounds[3] == win[3] {
                    self.mark("embed_frame_follows");
                }
                self.scroll_by(side(clip), clip.bottom() - 90.0 - full.top());
                self.next(700);
            }
            3305 => {
                let (Some((full, clip)), Some((shown, win, bounds))) = (frame_rects(app, b), probe(app)) else { return };
                let target = clip.bottom() - 90.0;
                if (full.top() - target).abs() > 6.0 {
                    self.scroll_by(side(clip), target - full.top());
                    self.next(500);
                    self.step -= 1;
                    return;
                }
                let (c, bot) = (embedplay::px(clip, ppp), win[1] + win[3]);
                if shown && win[3] < bounds[3] && win[3] > 40 && bot <= c[1] + c[3] + 2 && bounds[2] == win[2] {
                    self.mark("embed_frame_clipped_bottom");
                }
                self.window_shot("embed_clipped_bottom.png");
                self.next(0);
            }
            3306 => {
                let Some((_, clip)) = frame_rects(app, b) else { return };
                let Some(r) = poster(app, a) else {
                    self.scroll_by(side(clip), 160.0);
                    self.next(350);
                    self.step -= 1;
                    return;
                };
                self.click(r.center() - vec2(30.0, 0.0));
                self.next(0);
            }
            3307 => {
                if !playing(app, a) || !probe(app).is_some_and(|p| p.0) {
                    return;
                }
                if app.embed.as_ref().is_some_and(|p| p.key.0 == a) {
                    self.mark("embed_one_at_a_time");
                }
                self.next(2000);
            }
            3308 => {
                self.window_shot("embed_second.png");
                let Some((full, clip)) = frame_rects(app, a) else { return };
                self.scroll_by(side(clip), clip.top() - full.height() / 2.0 - full.top());
                self.next(700);
            }
            3309 => {
                let (Some((full, clip)), Some((shown, win, bounds))) = (frame_rects(app, a), probe(app)) else { return };
                let target = clip.top() - full.height() / 2.0;
                if (full.top() - target).abs() > 6.0 {
                    self.scroll_by(side(clip), target - full.top());
                    self.next(500);
                    self.step -= 1;
                    return;
                }
                let c = embedplay::px(clip, ppp);
                if shown && win[3] < bounds[3] && win[3] > 40 && win[1] >= c[1] - 2 && bounds[2] == win[2] {
                    self.mark("embed_frame_clipped_top");
                }
                self.window_shot("embed_clipped_top.png");
                self.scroll_by(side(clip), clip.bottom() + 120.0 - full.top());
                self.next(700);
            }
            3310 => {
                let Some((full, clip)) = frame_rects(app, a) else { return };
                if (full.top() - (clip.bottom() + 120.0)).abs() > 6.0 {
                    self.scroll_by(side(clip), clip.bottom() + 120.0 - full.top());
                    self.next(500);
                    self.step -= 1;
                    return;
                }
                if app.embed.is_some() && probe(app).is_some_and(|p| !p.0) {
                    self.mark("embed_frame_hidden_off_screen");
                }
                self.scroll_by(side(clip), 3000.0);
                self.next(900);
            }
            3311 => {
                if app.embed.is_some() {
                    return;
                }
                self.mark("embed_frame_gone_scrolled");
                let Some(clip) = app.rects.get("embed_clip").copied().or(app.wheel.chat) else { return };
                self.scroll_by(side(clip), -9000.0);
                self.next(900);
            }
            3312 => {
                let Some(r) = poster(app, b) else { return };
                self.click(r.center() - vec2(30.0, 0.0));
                self.next(0);
            }
            3313 => {
                if !playing(app, b) || !probe(app).is_some_and(|p| p.0) {
                    return;
                }
                app.switcher = Some(String::new());
                self.next(600);
            }
            3314 => {
                if probe(app).is_some_and(|p| !p.0) {
                    self.mark("embed_frame_hides_for_dialog");
                }
                self.key(Key::Escape);
                self.next(600);
            }
            3315 => {
                if app.switcher.is_some() || !probe(app).is_some_and(|p| p.0) {
                    return;
                }
                self.mark("embed_frame_returns_after_dialog");
                self.next(0);
            }
            3316 => {
                app.open(GENERAL);
                self.next(0);
            }
            3317 => {
                if app.cur() != Some(GENERAL) || app.embed.is_some() {
                    return;
                }
                self.mark("embed_frame_gone_channel");
                app.open(EMBEDS);
                self.next(600);
            }
            3318 => {
                let Some(r) = poster(app, b) else { return };
                self.click(r.center() - vec2(30.0, 0.0));
                self.next(0);
            }
            3319 => {
                if !playing(app, b) {
                    return;
                }
                let Some((_, clip)) = frame_rects(app, b) else { return };
                if player(app, file).is_none() {
                    self.scroll_by(side(clip), 120.0);
                    self.next(350);
                    self.step -= 1;
                    return;
                }
                self.mark("embed_direct_player");
                self.next(0);
            }
            3320 => {
                let Some(r) = player(app, file) else { return };
                self.click(r.center());
                self.next(1200);
            }
            3321 => {
                let ready = app.store.msgs.get(&EMBEDS).and_then(|e| e.list.iter().find(|m| m.id == file)).and_then(|m| m.embeds.first()?.video.as_ref()).and_then(|v| media::current(&v.url)).is_some_and(|s| matches!(s.view().phase, Phase::Ready));
                if !ready {
                    return;
                }
                self.mark("embed_direct_plays");
                if app.embed.is_none() {
                    self.mark("embed_inline_stops_frame");
                }
                self.next(300);
            }
            _ => self.exits(),
        }
    }
}
