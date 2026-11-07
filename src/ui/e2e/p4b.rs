//! Steps 5000 to 5049: P4b. Account settings that have to stick (clicked, then clicked back, and with
//! RUSTCORD_P4B=keep / verify across a restart: `python e2e/restart.py`), the chat column at narrow
//! widths, and `@game`. `python e2e/from_step.py 5000` runs just this group.

use std::time::{Duration, Instant};

use egui::{Event, Id, Modifiers, ViewportCommand, vec2};

use super::Run;
use crate::cache::{Animate, Density, Settings, VideoBg};
use crate::ui::App;
use crate::ui::settings::Page;

type Change = (Page, &'static str, fn(&Settings) -> bool, &'static str);

const CHANGES: [Change; 8] = [
    (Page::Accessibility, "toggle:Show image descriptions", |s| s.image_descriptions, "toggle:Show image descriptions"),
    (Page::Accessibility, "radio:Never animate", |s| s.stickers == Animate::Never, "radio:Always animate"),
    (Page::Accessibility, "radio:Spacious", |s| s.density == Density::Spacious, "radio:Default"),
    (Page::Appearance, "radio:Searches across all my DMs", |s| s.search_all_dms, "radio:Searches only in the selected DM"),
    (Page::Voice, "toggle:Always preview video", |s| s.preview_video, "toggle:Always preview video"),
    (Page::Voice, "camera:bg:blur", |s| s.video_bg == VideoBg::Blur, "camera:bg:none"),
    (Page::Language, "radio:24-hour", |s| s.hour_cycle == 2, "radio:Auto"),
    (Page::Notifications, "toggle:Friends come online", |s| !s.friend_online, "toggle:Friends come online"),
];

impl Run {
    /// Scrolls the mark into the window and clicks it; true once the click is queued.
    fn scroll_click(&mut self, ctx: &egui::Context, key: &str) -> bool {
        let window = ctx.content_rect().shrink2(vec2(0.0, 150.0));
        let Some(r) = Self::at(ctx, key) else { return false };
        let seen = Id::new(("p4b_seen", key));
        let still = ctx.data(|d| d.get_temp::<egui::Rect>(seen)) == Some(r);
        ctx.data_mut(|d| d.insert_temp(seen, r));
        if window.contains_rect(r) {
            if still {
                self.click(r.center());
            }
            return still;
        }
        self.queue.push_back(vec![Event::PointerMoved(window.center())]);
        let dy = if r.top() > window.top() { -200.0 } else { 200.0 };
        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, dy), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
        self.wait = Instant::now() + Duration::from_millis(250);
        false
    }

    fn record(&mut self, app: &App, tag: &str) {
        for k in ["header:name", "search", "composer", "composer:buttons"] {
            if let Some(r) = app.rects.get(k) {
                self.rects.insert(format!("p4b_{tag}_{k}"), serde_json::json!([r.left(), r.top(), r.width(), r.height()]));
            }
        }
    }

    pub(super) fn p4b_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(12);
        let mode = std::env::var("RUSTCORD_P4B").unwrap_or_default();
        match self.step {
            5000 => {
                if app.conn != crate::ui::Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                ctx.data_mut(|d| d.insert_temp(Id::new("p4b_k"), 0usize));
                self.next(500);
            }
            5001 | 5003 => {
                let restore = self.step == 5003;
                let k = ctx.data(|d| d.get_temp::<usize>(Id::new("p4b_k"))).unwrap_or(0);
                let Some((page, key, got, undo)) = CHANGES.get(k / 2) else {
                    ctx.data_mut(|d| d.insert_temp(Id::new("p4b_k"), 0usize));
                    self.mark(if restore { "p4b_restored" } else { "p4b_changed" });
                    self.next(300);
                    return;
                };
                let advance = |ctx: &egui::Context| ctx.data_mut(|d| d.insert_temp(Id::new("p4b_k"), k + 1));
                if k % 2 == 0 {
                    app.settings_page = Some(*page);
                    let key = if restore { undo } else { key };
                    if Self::at(ctx, key).is_none() {
                        if stuck {
                            self.fail(format!("{key} never showed on its settings page"));
                        }
                        return;
                    }
                    if self.scroll_click(ctx, key) {
                        advance(ctx);
                        self.step_at = Instant::now();
                        self.wait = Instant::now() + Duration::from_millis(400);
                    }
                } else {
                    if got(&app.settings) == restore {
                        if stuck && !ctx.data(|d| d.get_temp::<bool>(Id::new("p4b_shot"))).unwrap_or(false) {
                            ctx.data_mut(|d| d.insert_temp(Id::new("p4b_shot"), true));
                            self.shot(ctx, "p4b_stuck.png");
                            self.wait = Instant::now() + Duration::from_millis(1500);
                            return;
                        }
                        if stuck {
                            self.fail(format!("clicking {key} did not {} the setting", if restore { "put back" } else { "change" }));
                        }
                        return;
                    }
                    advance(ctx);
                    self.step_at = Instant::now();
                }
            }
            5002 => {
                app.settings_page = None;
                if mode == "keep" {
                    self.step = 9999;
                    return;
                }
                self.next(300);
            }
            5004 => {
                app.settings_page = None;
                self.next(300);
            }
            5005 => {
                app.settings.zoom = 1.5;
                app.save_settings(ctx);
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                app.select_guild(Some(self.target.0));
                app.open(self.target.1);
                self.next(1500);
            }
            5006 => {
                self.shot(ctx, "chat_940x500_150_plain.png");
                self.record(app, "plain");
                self.next(400);
            }
            5007 => {
                crate::ui::themes::open_themes(app, ctx, None);
                self.next(1000);
            }
            5008 => {
                if app.themes.panel != Some(crate::ui::themes::Panel::Themes) {
                    return;
                }
                self.shot(ctx, "chat_940x500_150_themes.png");
                self.record(app, "themes");
                let Some(r) = Self::at(ctx, "themes:Exit Preview") else { return };
                self.click(r.center());
                self.next(700);
            }
            5009 => {
                app.settings_page = None;
                app.start_search(self.target.1, "hello".into());
                self.next(1200);
            }
            5010 => {
                self.shot(ctx, "chat_940x500_150_search.png");
                self.record(app, "search");
                self.next(400);
            }
            5011 => {
                app.search = None;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings.zoom = 1.0;
                app.save_settings(ctx);
                self.next(1800);
            }
            5012 => {
                self.shot(ctx, "chat_1280x720_wide.png");
                self.record(app, "wide");
                self.next(400);
            }
            5013 => {
                app.start_search(self.target.1, "hello".into());
                self.next(1200);
            }
            5014 => {
                self.shot(ctx, "chat_1280x720_search.png");
                self.record(app, "wide_search");
                self.mark("p4b_chat_column");
                self.next(400);
            }
            5015 => {
                app.search = None;
                self.step = 5020;
            }
            5030 => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
            5040 => {
                if app.conn != crate::ui::Conn::Online || app.synced.is_none() {
                    return;
                }
                if let Some(i) = CHANGES.iter().position(|c| !(c.2)(&app.settings)) {
                    self.fail(format!("after a restart {} had gone back", CHANGES[i].1));
                    return;
                }
                self.mark("p4b_stuck");
                self.step = 9999;
            }
            _ => self.step = 5030,
        }
    }
}
