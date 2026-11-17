//! Steps 5300 to 5309: how message text is laid out. `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5300`
//! runs just this group.

use std::time::Duration;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn mdtext_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        match self.step {
            5300 => {
                let id = ((chrono::Utc::now().timestamp_millis() - 1_420_070_400_000) as u64) << 22;
                self.text = id.to_string();
                super::live::event(app, "MESSAGE_CREATE", serde_json::json!({
                    "id": id.to_string(), "channel_id": general.to_string(), "guild_id": self.target.0.to_string(), "type": 0,
                    "content": "lists\n- one\n- two\n  - inside\nthen\n9. nine\n1. ten",
                    "author": { "id": "101", "username": "alice" }, "timestamp": chrono::Utc::now().to_rfc3339(), "embeds": [], "mentions": [], "attachments": [],
                }));
                app.select_guild(Some(self.target.0));
                app.open(general);
                app.jump_to_present(general);
                self.next(1000);
            }
            5301 => {
                let item = |marker: &str| Self::at(ctx, &format!("li:{marker}"));
                let (Some(disc), Some(circle), Some(nine), Some(ten)) = (item("Disc"), item("Circle"), item("Number(9)"), item("Number(10)")) else {
                    if self.step_at.elapsed() > Duration::from_secs(10) {
                        self.shot(ctx, "text_list_missing.png");
                        self.step = 5302;
                    }
                    return;
                };
                let near = |a: f32, b: f32| (a - b).abs() < 0.6;
                if near(disc.width(), 16.0) && near(circle.width(), 16.0) && near(circle.left(), disc.right()) {
                    self.mark("text_list_bullets");
                }
                if near(nine.width(), 25.6) && near(nine.left(), disc.left()) && near(ten.top() - nine.top(), 26.0) {
                    self.mark("text_list_numbers");
                }
                self.shot(ctx, "text_list.png");
                self.next(300);
            }
            5302 => {
                super::live::event(app, "MESSAGE_DELETE", serde_json::json!({ "id": self.text, "channel_id": general.to_string(), "guild_id": self.target.0.to_string() }));
                let id = ((chrono::Utc::now().timestamp_millis() - 1_420_070_400_000) as u64) << 22;
                self.text = id.to_string();
                super::live::event(app, "MESSAGE_CREATE", serde_json::json!({
                    "id": id.to_string(), "channel_id": general.to_string(), "guild_id": self.target.0.to_string(), "type": 0,
                    "content": "run `cargo test` then ||one|| and ||two||\n> quoted",
                    "author": { "id": "101", "username": "alice" }, "timestamp": chrono::Utc::now().to_rfc3339(), "embeds": [], "mentions": [], "attachments": [],
                }));
                app.jump_to_present(general);
                self.next(1000);
            }
            5303 => {
                let at = |key: &str| Self::at(ctx, key);
                let (Some(code), Some(one), Some(_), Some(quote), Some(room)) = (at("box:code"), at("box:covered1"), at("box:covered2"), at("quote"), at("quote_room")) else {
                    if self.step_at.elapsed() > Duration::from_secs(10) {
                        self.shot(ctx, "text_boxes_missing.png");
                        self.step = 5305;
                    }
                    return;
                };
                let near = |a: f32, b: f32| (a - b).abs() < 0.6;
                if near(code.height(), 20.0) && near(one.height(), 21.0) {
                    self.mark("text_boxes");
                }
                if near(quote.left(), room.left()) && near(quote.width(), room.width() * 0.9 - 4.0) {
                    self.mark("text_quote_width");
                }
                self.shot(ctx, "text_boxes.png");
                self.click(one.center());
                self.next(600);
            }
            5304 => {
                // an older message's spoiler may still hold the covered mark: only this row counts
                let on_row = |key: &str, y: f32| Self::at(ctx, key).is_some_and(|r| (r.top() - y).abs() < 0.6);
                if Self::at(ctx, "box:open1").is_some_and(|o| !on_row("box:covered1", o.top()) && on_row("box:covered2", o.top())) {
                    self.mark("text_spoiler_one");
                    self.shot(ctx, "text_spoiler_one.png");
                } else if self.step_at.elapsed() < Duration::from_secs(10) {
                    return;
                } else {
                    self.shot(ctx, "text_spoiler_stuck.png");
                }
                self.next(200);
            }
            _ => {
                super::live::event(app, "MESSAGE_DELETE", serde_json::json!({ "id": self.text, "channel_id": general.to_string(), "guild_id": self.target.0.to_string() }));
                self.text.clear();
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 };
            }
        }
    }
}
