//! Steps 4700 to 4799: what the web view of a video embed does beyond following its poster. A page
//! asking for fullscreen through a real click in it (the mock's player has a button), the window
//! then covering the whole client area and giving it back, all of it again at 150% zoom where the
//! window is placed in physical pixels; and the member list at 940 wide.
//! `python e2e/from_step.py 4700` runs this group.

use egui::vec2;
use serde_json::json;

use super::Run;
use super::embed::{frame_rects, msg_of, near, playing, side};
use crate::ui::{App, embedplay};

const GUILD: u64 = 1000;
const EMBEDS: u64 = 2062;
const BUTTON: (i32, i32) = (30, 24);

fn poster(app: &App, msg: u64) -> Option<egui::Rect> {
    app.rects.get(&format!("embed_poster:{msg}")).copied().filter(|r| r.is_positive() && r.height() > 40.0)
}

impl Run {
    pub(super) fn tidy_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        if self.step == 4700 {
            if app.conn != crate::net::Conn::Online || !app.store.channels.contains_key(&EMBEDS) {
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
            app.select_guild(Some(GUILD));
            app.open(EMBEDS);
            return self.next(1500);
        }
        let Some(b) = msg_of(app, "mock2") else { return };
        let probe = |app: &App| app.embed.as_ref().map(|p| p.frame.probe());
        match self.step {
            4701 => {
                if app.cur() != Some(EMBEDS) {
                    return;
                }
                if playing(app, b) {
                    return self.next(0);
                }
                let Some(r) = poster(app, b) else {
                    let Some(chat) = app.wheel.chat else { return };
                    self.scroll_by(side(chat), if self.step_at.elapsed().as_secs() / 7 % 2 == 0 { -200.0 } else { 200.0 });
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(400);
                    return;
                };
                self.click(r.center() - vec2(30.0, 0.0));
                self.next(0);
            }
            4702 => {
                if !playing(app, b) || !probe(app).is_some_and(|p| p.0) {
                    return;
                }
                self.next(1500);
            }
            4703 => {
                let Some((full, clip)) = frame_rects(app, b) else { return };
                let target = clip.top() + 30.0;
                if (full.top() - target).abs() > 6.0 {
                    self.scroll_by(side(clip), target - full.top());
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(500);
                    return;
                }
                let want = embedplay::px(full.intersect(clip), ppp);
                if ppp > 1.4 && probe(app).is_some_and(|(shown, win, bounds)| shown && win.iter().zip(want).all(|(x, y)| near(*x, y)) && bounds[2] == win[2]) {
                    self.mark("embed_frame_follows_150");
                }
                self.next(0);
            }
            4704 => {
                if let Some(p) = &app.embed {
                    p.frame.press(BUTTON.0, BUTTON.1);
                }
                self.next(1200);
            }
            4705 => {
                let all = embedplay::px(ctx.content_rect(), ppp);
                let Some((shown, win, bounds)) = probe(app) else { return };
                let entered = app.embed.as_ref().is_some_and(|p| p.frame.fullscreen());
                self.rects.insert("embed_entered_fullscreen".into(), json!(entered));
                if entered && shown && win.iter().zip(all).all(|(x, y)| near(*x, y)) && bounds[2] == win[2] && bounds[3] == win[3] {
                    self.mark(if ppp > 1.4 { "embed_fullscreen_covers_window_150" } else { "embed_fullscreen_covers_window" });
                }
                self.window_shot(if ppp > 1.4 { "embed_fullscreen_150.png" } else { "embed_fullscreen.png" });
                if let Some(p) = &app.embed {
                    p.frame.press(BUTTON.0, BUTTON.1);
                }
                self.next(1200);
            }
            4706 => {
                let Some((full, clip)) = frame_rects(app, b) else { return };
                let (Some((shown, win, bounds)), Some(p)) = (probe(app), &app.embed) else { return };
                let want = embedplay::px(full.intersect(clip), ppp);
                let entered = self.rects.get("embed_entered_fullscreen").and_then(|v| v.as_bool()).unwrap_or(false);
                if entered && !p.frame.fullscreen() && shown && win.iter().zip(want).all(|(x, y)| near(*x, y)) && bounds[2] == win[2] {
                    self.mark(if ppp > 1.4 { "embed_fullscreen_exit_returns_150" } else { "embed_fullscreen_exit_returns" });
                }
                self.next(0);
                self.step = if ppp > 1.4 { 4710 } else { 4709 };
            }
            4709 => {
                app.settings.zoom = 1.5;
                app.save_settings(ctx);
                self.next(2500);
                self.step = 4730;
            }
            4730 => {
                app.open(EMBEDS);
                self.next(1000);
                self.step = 4701;
            }
            4710 => {
                app.settings.zoom = 1.0;
                app.save_settings(ctx);
                self.next(800);
                self.step = 4720;
            }
            4720 => {
                app.rects.retain(|k, _| !k.starts_with("member:"));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(1500);
            }
            4721 => {
                if app.cur() != Some(EMBEDS) {
                    return;
                }
                self.shot(ctx, "members_940.png");
                if app.rects.keys().any(|k| k.starts_with("member:")) && ctx.content_rect().width() < 1000.0 {
                    self.mark("member_list_at_940");
                }
                self.next(0);
            }
            4722 => {
                if self.pending_shots > 0 {
                    return;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 4600 };
            }
            _ => self.step = 4600,
        }
    }
}
