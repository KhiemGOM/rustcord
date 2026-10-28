//! The plugins page: search it, switch MessageLogger on with its switch, then an edit and a
//! delete arrive for someone's message in #general, which stays with its old text.

use egui::Key;
use serde_json::json;
use std::time::Instant;

use super::Run;
use crate::plugins;
use crate::ui::App;

impl Run {
    pub(super) fn plugin_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        match self.step {
            560 => {
                // the warm run starts with whatever the cold one left on
                plugins::enable("MessageLogger", false);
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            561 => {
                if self.open_page(app, "Plugins") {
                    app.sform.plugin_search = "MessageLogger".into();
                    self.next(800);
                }
            }
            562 => {
                let Some(r) = Self::at(ctx, "plugin:MessageLogger") else { return };
                self.shot(ctx, "plugins_page.png");
                self.click(r.center());
                self.next(400);
            }
            563 => {
                if !plugins::on("MessageLogger") {
                    return;
                }
                self.mark("plugin_switch");
                app.sform.plugin_search.clear();
                self.key(Key::Escape);
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1000);
            }
            564 => {
                let me = app.store.me;
                let Some(m) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.author != me && !m.content.is_empty() && !app.store.user(m.author).is_some_and(|u| u.bot))).cloned() else { return };
                let edited = json!({ "id": m.id.to_string(), "channel_id": general.to_string(), "content": "edited by the logger test", "edited_timestamp": chrono::Utc::now().to_rfc3339() });
                app.store.apply("MESSAGE_UPDATE", &edited);
                app.store.apply("MESSAGE_DELETE", &json!({ "id": m.id.to_string(), "channel_id": general.to_string() }));
                self.loaded = m.id as usize;
                self.next(800);
            }
            565 => {
                let kept = app.store.msgs.get(&general).and_then(|e| e.list.iter().find(|m| m.id as usize == self.loaded));
                if kept.is_some_and(|m| m.deleted && m.edits.len() == 1 && m.content == "edited by the logger test") {
                    self.mark("plugin_logger");
                    self.shot(ctx, "plugins_logger.png");
                } else {
                    self.fail("MessageLogger dropped the deleted message or its edit".into());
                    return;
                }
                app.store.forget_history(general, self.loaded as u64);
                plugins::enable("MessageLogger", false);
                self.loaded = 0;
                self.step = 99;
                self.step_at = Instant::now();
            }
            _ => self.step = 99,
        }
    }
}
