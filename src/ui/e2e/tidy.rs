//! Steps 4700 to 4799: what the web view of a video embed does beyond following its poster. A page
//! asking for fullscreen through a real click in it (the mock's player has a button), the window
//! then covering the whole client area and giving it back; and the message toolbar, which egui
//! draws, showing over the embed's native window. `python e2e/from_step.py 4700` runs this group.

use egui::{Event, vec2};

use super::Run;
use super::embed::{frame_rects, msg_of, near, playing, poster, side};
use crate::ui::{App, embedplay};

const GUILD: u64 = 1000;
const EMBEDS: u64 = 2062;
const BUTTON: (i32, i32) = (30, 24);

impl Run {
    pub(super) fn tidy_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        if self.step == 4700 {
            if app.conn != crate::net::Conn::Online || !app.store.channels.contains_key(&EMBEDS) {
                return;
            }
            app.select_guild(Some(GUILD));
            app.open(EMBEDS);
            if let Some(z) = std::env::var("RUSTCORD_E2E_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()) {
                app.settings.zoom = z / 100.0;
                app.save_settings(ctx);
            }
            return self.next(800);
        }
        let Some(b) = msg_of(app, "mock2") else { return };
        let probe = |app: &App| app.embed.as_ref().map(|p| p.frame.probe());
        match self.step {
            4701 => {
                if app.cur() != Some(EMBEDS) {
                    return;
                }
                let Some(r) = poster(app, b) else {
                    let Some(chat) = app.wheel.chat else { return };
                    self.scroll_by(side(chat), if self.step_at.elapsed() < std::time::Duration::from_secs(6) { -200.0 } else { 200.0 });
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
                    self.next(500);
                    self.step -= 1;
                    return;
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
                if app.embed.as_ref().is_some_and(|p| p.frame.fullscreen()) && shown && win.iter().zip(all).all(|(x, y)| near(*x, y)) && bounds[2] == win[2] && bounds[3] == win[3] {
                    self.mark("embed_fullscreen_covers_window");
                }
                self.window_shot("embed_fullscreen.png");
                if let Some(p) = &app.embed {
                    p.frame.press(BUTTON.0, BUTTON.1);
                }
                self.next(1200);
            }
            4706 => {
                let Some((full, clip)) = frame_rects(app, b) else { return };
                let (Some((shown, win, bounds)), Some(p)) = (probe(app), &app.embed) else { return };
                let want = embedplay::px(full.intersect(clip), ppp);
                if !p.frame.fullscreen() && shown && win.iter().zip(want).all(|(x, y)| near(*x, y)) && bounds[2] == win[2] {
                    self.mark("embed_fullscreen_exit_returns");
                }
                self.next(0);
            }
            4707 => {
                let Some((full, clip)) = frame_rects(app, b) else { return };
                let at = egui::pos2(full.right().min(clip.right() - 40.0) - 60.0, full.bottom() + 28.0);
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                self.next(500);
            }
            4708 => {
                let Some((full, _)) = frame_rects(app, b) else { return };
                let (Some((_, bar)), Some(p)) = (app.bar, &app.embed) else { return };
                let over = bar.intersect(full);
                if !over.is_positive() {
                    self.shot(ctx, "embed_toolbar_apart.png");
                    return self.fail(format!("the message toolbar {bar:?} never reached the embed {full:?}"));
                }
                let (_, win, _) = p.frame.probe();
                let hole = embedplay::px(over, ppp);
                let local = [hole[0] - win[0], hole[1] - win[1], hole[2], hole[3]];
                if p.frame.hole().is_some_and(|h| h.iter().zip(local).all(|(x, y)| near(*x, y))) {
                    self.mark("embed_window_leaves_toolbar_hole");
                }
                self.window_shot("embed_toolbar.png");
                self.queue.push_back(vec![Event::PointerMoved(egui::pos2(10.0, 10.0))]);
                self.next(400);
            }
            4709 => {
                if app.bar.is_none() && app.embed.as_ref().is_some_and(|p| p.frame.hole().is_none()) {
                    self.mark("embed_window_whole_again");
                }
                app.rects.retain(|k, _| !k.starts_with("member:"));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(1200);
            }
            4710 => {
                if app.cur() != Some(EMBEDS) {
                    return;
                }
                self.shot(ctx, "members_940.png");
                if app.rects.keys().any(|k| k.starts_with("member:")) && ctx.content_rect().width() < 1000.0 {
                    self.mark("member_list_at_940");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 };
            }
            _ => self.step = 3110,
        }
    }
}
