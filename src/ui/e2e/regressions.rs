use egui::ViewportCommand;
use serde_json::json;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn regression_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        match self.step {
            8800 => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
                app.select_guild(Some(1003));
                self.next(700);
            }
            8801 => {
                let Some(r) = app.rects.get("chan:2302").copied() else { return };
                self.rclick(r.center());
                self.next(300);
            }
            8802 => {
                let Some(r) = Self::at(ctx, "menu_row:Pin Channel to Top") else { return };
                self.shot(ctx, "pin_menu.png");
                self.click(r.center());
                self.next(800);
            }
            8803 => {
                if !app.rects.contains_key("side:pinned") {
                    return;
                }
                self.mark("channel_pinned_by_click");
                self.shot(ctx, "channel_pinned.png");
                app.rest("POST", "/mock/event".into(), Some(json!({ "t": "USER_GUILD_SETTINGS_UPDATE", "d": {
                    "guild_id": "1003", "channel_overrides": [{ "channel_id": "2302", "muted": false }]
                }})), crate::net::Tag::Quiet);
                self.next(800);
            }
            8804 => {
                if app.store.chan_flags.get(&2302).is_none_or(|f| f & crate::ui::PINNED == 0) {
                    self.shot(ctx, "pin_lost_after_update.png");
                    return self.fail("a notification-only settings update unpinned the channel".into());
                }
                self.mark("channel_pin_survives_update");
                let Some(r) = app.rects.get("chan:2302").copied() else { return };
                self.rclick(r.center());
                self.next(300);
            }
            8805 => {
                let Some(r) = Self::at(ctx, "menu_row:Unpin Channel") else { return };
                self.click(r.center());
                self.next(800);
            }
            8806 => {
                if app.rects.contains_key("side:pinned") {
                    return;
                }
                self.mark("channel_unpinned_by_click");
                self.step = 8820;
            }
            8820 => {
                app.select_guild(Some(self.target.0));
                app.open(self.target.1);
                self.next(800);
            }
            8821 => {
                let ch = self.target.1;
                app.scroll_fix = Some((ch, 1400.0));
                app.pinned.remove(&ch);
                self.next(600);
            }
            8822 => {
                let ch = self.target.1;
                let Some(row) = app.rects.get("msg:last").copied() else { return };
                let Some(entry) = app.store.msgs.get(&ch) else { return };
                let found = entry.list.iter().find(|m| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("row", m.id)))) == Some(row));
                let Some(m) = found else { return };
                self.rects.insert("read_anchor".into(), json!([m.id, row.top()]));
                self.loaded = entry.list.len();
                let Some(first) = entry.list.first() else { return };
                app.net.send(crate::net::Cmd::Messages { channel: ch, page: crate::state::Page::Before(first.id) });
                self.next(1000);
            }
            8823 => {
                let ch = self.target.1;
                if app.store.msgs.get(&ch).is_none_or(|e| e.list.len() <= self.loaded) {
                    return;
                }
                let anchor = &self.rects["read_anchor"];
                let id = anchor[0].as_u64().unwrap();
                let y = anchor[1].as_f64().unwrap() as f32;
                let row = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("row", id))));
                self.shot(ctx, "read_anchor_after_history.png");
                if row.is_none_or(|r| (r.top() - y).abs() > 2.0) {
                    return self.fail(format!("history moved the reading anchor from {y} to {row:?}"));
                }
                self.mark("history_kept_reading_anchor");
                self.step = 8850;
            }
            8850 => {
                app.open_shop(crate::ui::shop::Page::Shop);
                self.next(800);
            }
            8851 => {
                let Some(r) = Self::at(ctx, "shop:tab:Orbs Exclusives") else { return };
                self.click(r.center());
                self.next(800);
            }
            8852 => {
                let Some(r) = Self::at(ctx, "shop:card:5203") else {
                    self.scroll_by(ctx.content_rect().center(), -200.0);
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(200);
                    return;
                };
                if r.bottom() > ctx.content_rect().bottom() - 8.0 || r.top() < 100.0 {
                    self.scroll_by(ctx.content_rect().center(), if r.top() < 100.0 { 200.0 } else { -200.0 });
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(200);
                    return;
                }
                self.shot(ctx, "orb_decoration_card.png");
                self.click(r.center());
                self.next(800);
            }
            8853 => {
                let Some(r) = Self::at(ctx, "shop:slide:1") else { return };
                self.click(r.center());
                self.next(800);
            }
            8854 => {
                if app.shop.as_ref().is_none_or(|s| s.slide != 1) {
                    return;
                }
                self.shot(ctx, "orb_decoration_variant.png");
                self.mark("orb_decoration_variant_selected");
                self.step = 99;
            }
            _ => self.step = 99,
        }
    }
}
