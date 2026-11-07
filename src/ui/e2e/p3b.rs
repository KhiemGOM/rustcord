//! Steps 4600 to 4649: what P3's settings rows really do. The shop's own pointer, the legacy chat
//! input, billing address editing, and the profile's leftovers. `python e2e/from_step.py 4600` runs
//! just this group.

use std::time::Duration;

use egui::{Event, Key, ViewportCommand, pos2, vec2};

use super::Run;
use crate::cursor;
use crate::ui::App;
use crate::ui::settings::Page as Settings;
use crate::ui::shop::Page;

impl Run {
    pub(super) fn p3b_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |k: &str| Self::at(ctx, k);
        let stuck = self.step_at.elapsed() > Duration::from_secs(12);
        match self.step {
            4600 => {
                if app.conn != crate::ui::Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings_page = None;
                app.profed = None;
                app.settings.shop_cursor = true;
                app.select_guild(None);
                app.open_shop(Page::Shop);
                self.next(1200);
            }
            4601 => {
                let Some(r) = at("shop:tab:Browse") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(500);
            }
            4602 => {
                if at("shop:tab:Browse").is_none() {
                    return;
                }
                if cursor::mode() != 2 {
                    if stuck {
                        self.fail(format!("the pointer over a shop tab was mode {}, not the hand", cursor::mode()));
                    }
                    return;
                }
                self.mark("shop_cursor_hand");
                if cursor::probe() == cursor::handle(cursor::POINTER) {
                    self.mark("shop_cursor_hand_applied");
                }
                self.queue.push_back(vec![Event::PointerMoved(pos2(ctx.content_rect().center().x + 200.0, 16.0))]);
                self.next(500);
            }
            4603 => {
                if cursor::mode() != 1 {
                    if stuck {
                        self.fail(format!("the pointer over bare shop was mode {}, not the arrow", cursor::mode()));
                    }
                    return;
                }
                self.mark("shop_cursor_arrow");
                if cursor::probe() == cursor::handle(cursor::ARROW) {
                    self.mark("shop_cursor_arrow_applied");
                }
                app.settings.shop_cursor = false;
                self.next(500);
            }
            4604 => {
                if cursor::mode() != 0 {
                    return;
                }
                if cursor::probe() != cursor::handle(cursor::ARROW) && cursor::probe() != cursor::handle(cursor::POINTER) {
                    self.mark("shop_cursor_off");
                }
                app.settings.shop_cursor = true;
                app.open_shop(Page::Nitro);
                self.next(500);
            }
            4605 => {
                app.shop = None;
                self.next(200);
            }
            4606 => {
                app.settings_page = Some(Settings::Billing);
                self.next(1500);
            }
            4607 => {
                let Some(r) = at("billing:Edit") else { return };
                self.click(r.center());
                self.next(600);
            }
            4608 => {
                let Some(r) = at("billing:addr:name") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text(" Lovelace".into())]);
                self.next(300);
            }
            4609 => {
                let Some(r) = at("billing:addr:line_2") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("Apt 4".into())]);
                self.next(300);
            }
            4610 => {
                let Some(r) = at("billing:addr:state") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("HCM".into())]);
                self.next(300);
            }
            4611 => {
                let Some(r) = at("billing:addr:name") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -400.0), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE }]);
                self.next(500);
            }
            4612 => {
                let Some(r) = at("billing:addr:month") else { return };
                self.click(r.center());
                self.key(Key::Backspace);
                self.queue.push_back(vec![Event::Text("10".into())]);
                if let Some(d) = at("billing:make_default") {
                    self.click(d.center());
                }
                self.next(300);
            }
            4613 => {
                let Some(r) = at("billing:save") else { return };
                self.shot(ctx, "billing_edit_address.png");
                self.click(r.center());
                self.next(900);
            }
            4614 => {
                ctx.data_mut(|d| d.remove::<(egui::Rect, u64)>(egui::Id::new(("e2e", "billing:save"))));
                self.next(500);
            }
            4615 => {
                if at("billing:save").is_some() {
                    if stuck {
                        self.fail(format!("the edit dialog's Save was still drawn at {:?} after {:?}", at("billing:save"), self.step_at.elapsed()));
                    }
                    return;
                }
                self.mark("billing_address_saved");
                self.key(Key::Escape);
                self.next(500);
            }
            4616 => {
                app.settings_page = None;
                app.select_guild(Some(self.target.0));
                self.next(900);
            }
            4617 => {
                let general = app.store.guilds.get(&self.target.0).and_then(|g| g.channels.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.name == "general")));
                let Some(r) = general.and_then(|c| app.rects.get(&format!("chan:{c}")).copied()) else { return };
                self.click(r.center());
                self.next(800);
            }
            4618 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                app.settings.legacy_input = true;
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("/rem".into())]);
                self.next(700);
            }
            4619 => {
                if !app.ac_open {
                    self.mark("legacy_no_app_commands");
                }
                self.shot(ctx, "legacy_input.png");
                let ch = app.cur().unwrap_or(0);
                app.drafts.insert(ch, String::new());
                app.settings.legacy_input = false;
                self.queue.push_back(vec![Event::Text("/rem".into())]);
                self.next(700);
            }
            4620 => {
                if app.ac_open {
                    self.mark("legacy_off_app_commands");
                }
                self.key(Key::Escape);
                let ch = app.cur().unwrap_or(0);
                app.drafts.insert(ch, String::new());
                self.next(300);
            }
            4621 => {
                app.store.me_raw["premium_type"] = serde_json::json!(2);
                crate::ui::profed::open(app, None);
                self.next(1500);
            }
            4622 => {
                let Some(r) = at("tile:avatar") else { return };
                self.click(r.center());
                self.next(700);
            }
            4623 => {
                let Some(r) = at("avatar:gif") else { return };
                self.shot(ctx, "avatar_dialog.png");
                self.click(r.center());
                self.next(900);
            }
            4624 => {
                let Some(r) = app.rects.get("gifcat:trending").copied() else { return };
                self.shot(ctx, "avatar_gif_categories.png");
                self.click(r.center());
                self.next(1200);
            }
            4625 => {
                let Some(r) = app.rects.get("gif:0").copied() else { return };
                self.shot(ctx, "avatar_gif_results.png");
                self.click(r.center());
                self.next(1500);
            }
            4626 => {
                if app.crop.is_none() {
                    if stuck {
                        self.fail("picking a GIF for the avatar never opened the cropper".into());
                    }
                    return;
                }
                self.mark("avatar_gif_to_crop");
                self.shot(ctx, "avatar_gif_crop.png");
                app.crop = None;
                app.profed = None;
                self.next(300);
            }
            4627 => {
                app.status_text = Some(crate::ui::cstatus::State::from(&app.store));
                self.next(700);
            }
            4628 => {
                let Some(r) = at("cstatus_emoji") else { return };
                self.shot(ctx, "status_dialog_empty.png");
                self.click(r.center());
                self.next(800);
            }
            4629 => {
                let Some(r) = at("emoji0").filter(|_| matches!(app.picker, Some((crate::ui::Pick::Status, _)))) else { return };
                self.shot(ctx, "status_emoji_picker.png");
                self.click(r.center());
                self.next(700);
            }
            4630 => {
                if app.status_text.as_ref().is_none_or(|s| s.emoji.is_none()) {
                    return;
                }
                self.mark("status_emoji_picked");
                self.queue.push_back(vec![Event::Text("emoji status".into())]);
                self.next(500);
            }
            4631 => {
                let Some(r) = at("cstatus_save") else { return };
                self.shot(ctx, "status_dialog_filled.png");
                self.click(r.center());
                self.next(800);
            }
            4632 => {
                if app.status_text.is_some() {
                    return;
                }
                if app.store.custom_emoji.is_some() && app.store.custom_status.as_deref() == Some("emoji status") {
                    self.mark("status_emoji_saved");
                }
                app.store.custom_status = None;
                app.store.custom_emoji = None;
                self.next(300);
            }
            4633 => {
                app.store.me_raw["premium_type"] = serde_json::json!(0);
                app.settings.zoom = 1.5;
                app.save_settings(ctx);
                self.next(600);
            }
            4634 => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                app.settings_page = Some(Settings::Appearance);
                self.next(1800);
            }
            4635 => {
                let Some(r) = at("button:Preview Theme") else { return };
                self.click(r.center());
                self.next(900);
            }
            4636 => {
                if app.themes.panel != Some(crate::ui::themes::Panel::Themes) {
                    return;
                }
                self.shot(ctx, "themes_panel_940x500_150.png");
                let Some(r) = at("themes:Exit Preview") else { return };
                self.click(r.center());
                self.next(700);
            }
            4637 => {
                let Some(r) = self.scroll_to(ctx, at("button:Try it out"), false) else { return };
                self.click(r.center());
                self.next(900);
            }
            4638 => {
                if app.themes.panel != Some(crate::ui::themes::Panel::Editor) {
                    return;
                }
                self.shot(ctx, "themes_editor_940x500_150.png");
                let Some(r) = at("themes:Back to Settings") else { return };
                self.click(r.center());
                self.next(700);
            }
            4639 => {
                let Some(r) = self.scroll_to(ctx, at("button:Preview Icon"), false) else { return };
                self.click(r.center());
                self.next(900);
            }
            4640 => {
                if app.themes.panel != Some(crate::ui::themes::Panel::Icons) {
                    return;
                }
                self.shot(ctx, "themes_icons_940x500_150.png");
                self.mark("themes_panels_small");
                if let Some(r) = at("themes:Exit Preview") {
                    self.click(r.center());
                }
                self.next(500);
            }
            4641 => {
                app.settings.zoom = 1.0;
                app.save_settings(ctx);
                app.settings_page = None;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                self.next(500);
            }
            4642 => {
                app.select_guild(Some(self.target.0));
                self.next(900);
            }
            4643 => {
                let Some(vc) = crate::ui::side::tree(&app.store, self.target.0, None).into_iter().flat_map(|(_, k)| k).find(|c| c.is_voice()).map(|c| c.id) else {
                    return self.fail("no voice channel".into());
                };
                let Some(r) = app.rects.get(&format!("chan:{vc}")).copied() else { return };
                self.call = vc;
                self.click(r.center());
                self.next(700);
            }
            4644 => {
                let me = app.store.me;
                if app.voice.state != crate::voice::State::Connected || app.voice.chan != Some(self.call) {
                    return;
                }
                let Some(mut r) = app.rects.get(&format!("voice:tile:{me}")).copied() else { return };
                if let Some((_, other)) = app.rects.iter().find(|(k, _)| k.starts_with("voice:tile:") && !k.ends_with(&me.to_string()) && app.voice.video.contains_key(&k["voice:tile:".len()..].parse().unwrap_or(0))) {
                    r = *other;
                }
                app.settings.dev = true;
                app.settings.stream_info = true;
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(700);
            }
            4645 => {
                if at("stream_info").is_none() {
                    if stuck {
                        self.fail("hovering a call tile with Stream Info Overlay on showed no overlay".into());
                    }
                    return;
                }
                self.mark("stream_info_overlay");
                self.shot(ctx, "stream_info_overlay.png");
                self.next(700);
            }
            4646 => {
                app.settings.dev = false;
                app.settings.stream_info = false;
                app.voice.leave(&app.net);
                self.next(600);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5000 },
        }
    }
}
