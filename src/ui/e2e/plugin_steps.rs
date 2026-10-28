//! The Vencord host: the run's userplugin (e2e/fce2e.tsx) shows on the Plugins page, its switch
//! starts it, and then its send listener, command, menu item, chat bar button and flux handler
//! each do their part, and its React UI is drawn and used like Fastcord's own.

use egui::Key;
use std::time::Instant;

use super::Run;
use crate::ui::App;
use crate::ui::react::{published, text};

impl Run {
    pub(super) fn plugin_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let toast = |app: &App, s: &str| app.toast.as_ref().is_some_and(|(t, _)| t == s);
        match self.step {
            560 => {
                if app.vc.host.is_none() {
                    return;
                }
                self.mark("vc_loaded");
                app.vc_call("setOption", serde_json::json!({ "name": "FcE2e", "key": "clicked", "value": false }));
                app.vc_call("setOption", serde_json::json!({ "name": "FcE2e", "key": "loud", "value": false }));
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            561 => {
                if self.open_page(app, "Plugins") {
                    app.sform.plugin_search = "FcE2e".into();
                    self.next(800);
                }
            }
            562 => {
                let Some(r) = Self::at(ctx, "plugin:FcE2e") else { return };
                self.shot(ctx, "vencord_plugins.png");
                // the warm run starts with it on from the cold one
                let on = app.vc.list.as_ref().and_then(|l| l.as_array()?.iter().find(|p| p["name"] == "FcE2e").cloned()).is_some_and(|p| p["started"] == true);
                if !on {
                    self.click(r.center());
                }
                self.next(500);
            }
            563 => {
                let on = app.vc.list.as_ref().and_then(|l| l.as_array()?.iter().find(|p| p["name"] == "FcE2e").cloned()).is_some_and(|p| p["started"] == true);
                if !on {
                    return;
                }
                self.mark("vc_enabled");
                app.sform.plugin_search.clear();
                self.key(Key::Escape);
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(1000);
            }
            564 => {
                app.drafts.insert(general, "vc:hello from a plugin".into());
                app.send_message(general);
                if let Some(chan) = app.store.channels.get(&general).cloned() {
                    app.drafts.insert(general, "/fce2e".into());
                    if !app.vc_command(&chan) {
                        return self.fail("the plugin's /fce2e wasn't a command".into());
                    }
                }
                self.next(1500);
            }
            565 => {
                if !toast(app, "flux saw it") {
                    return;
                }
                self.mark("vc_flux");
                let Some(m) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.author != app.store.me)).map(|m| m.id) else { return };
                app.menu = Some((general, m, egui::pos2(500.0, 300.0)));
                self.next(600);
            }
            566 => {
                let items = app.vc.menu.as_ref().map(|(_, v)| v.clone()).unwrap_or_default();
                let Some(action) = items.as_array().and_then(|a| a.iter().find(|i| i["label"] == "E2E Quote")).and_then(|i| i["action"].as_str()).map(str::to_string) else { return };
                self.shot(ctx, "vencord_menu.png");
                app.menu = None;
                app.vc_act(&action, false);
                self.next(400);
            }
            567 => {
                if !app.toast.as_ref().is_some_and(|(t, _)| t.starts_with("quoted ")) {
                    return;
                }
                self.mark("vc_menu");
                let Some(r) = app.rects.get("chatbar:FcE2e").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            568 => {
                let clicked = std::fs::read(crate::cache::dir().join("vencord.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .is_some_and(|v| v["plugins"]["FcE2e"]["clicked"] == true);
                if !clicked {
                    return;
                }
                self.mark("vc_chatbar");
                self.shot(ctx, "vencord_chat.png");
                self.next(300);
            }
            // React UI drawn natively: the accessory under the "vc:" message, its button bumping
            // the plugin's own state, and the decoration beside the name
            569 => {
                let me = app.store.me;
                let Some(m) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.author == me && m.content.starts_with("vc:hello"))).map(|m| m.id) else { return };
                if self.loaded != m as usize {
                    // the warm run's copy can sit up in the history
                    app.jump_to(general, m);
                }
                self.loaded = m as usize;
                let acc = published(ctx).trees.get(&format!("acc:{m}")).map(text).unwrap_or_default();
                let Some(r) = Self::at(ctx, &format!("vc:acc:{m}:Bump")) else { return };
                if !acc.contains("accessory 0") {
                    return;
                }
                self.click(r.center());
                self.next(500);
            }
            570 => {
                let m = self.loaded;
                let p = published(ctx);
                let acc = p.trees.get(&format!("acc:{m}")).map(text).unwrap_or_default();
                // decorations sit on a group's name line, so any of ours that starts one
                let dec = p.trees.iter().any(|(k, v)| k.starts_with("dec:") && text(v) == "VC");
                if !acc.contains("accessory 1") || !dec {
                    return;
                }
                self.mark("vc_accessory");
                self.shot(ctx, "vencord_accessory.png");
                self.loaded = 0;
                app.drafts.insert(general, "/fcmodal".into());
                if let Some(chan) = app.store.channels.get(&general).cloned() {
                    app.vc_command(&chan);
                }
                self.next(600);
            }
            571 => {
                let Some(modal) = app.vc.modals.first().cloned() else { return };
                let Some(r) = Self::at(ctx, &format!("vc:{modal}:Done")) else { return };
                self.shot(ctx, "vencord_modal.png");
                self.click(r.center());
                self.next(500);
            }
            572 => {
                if !app.vc.modals.is_empty() || !toast(app, "modal done") {
                    return;
                }
                self.mark("vc_modal");
                let me = app.store.me;
                let Some(u) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.author != me && !app.store.user(m.author).is_some_and(|u| u.bot))).map(|m| m.author) else { return };
                self.loaded = u as usize;
                app.profile = Some((u, self.target.0, egui::pos2(500.0, 200.0)));
                self.next(800);
            }
            573 => {
                let badges = published(ctx).trees.get(&format!("badges:{}", self.loaded)).cloned().unwrap_or_default();
                if !badges.as_array().is_some_and(|a| a.iter().any(|b| b["tooltip"] == "E2E badge")) {
                    return;
                }
                self.mark("vc_badge");
                self.shot(ctx, "vencord_badge.png");
                app.profile = None;
                self.loaded = 0;
                self.next(400);
            }
            // the member list's decorator, then the channel menu's item
            574 => {
                if !published(ctx).trees.iter().any(|(k, v)| k.starts_with("mem:guild:") && text(v) == "★") {
                    return;
                }
                self.mark("vc_member");
                let Some(r) = app.rects.get(&format!("chan:{general}")).copied() else { return };
                self.rclick(r.center());
                self.next(600);
            }
            575 => {
                let items = published(ctx).trees.get(&format!("menu:channel:{general}")).cloned().unwrap_or_default();
                if !items.as_array().is_some_and(|a| a.iter().any(|i| i["label"] == "E2E Channel")) {
                    return;
                }
                self.mark("vc_channel_menu");
                self.shot(ctx, "vencord_channel_menu.png");
                self.key(Key::Escape);
                self.next(400);
            }
            // a COMPONENT setting: the plugin's own switch on the Plugins page
            576 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            577 => {
                if self.open_page(app, "Plugins") {
                    app.sform.plugin_search = "FcE2e".into();
                    app.sform.plugin_open.insert("FcE2e".into());
                    self.next(800);
                }
            }
            578 => {
                let Some(r) = Self::at(ctx, "vc:setting:FcE2e:loud:Loud") else { return };
                self.shot(ctx, "vencord_setting.png");
                self.click(r.center());
                self.next(500);
            }
            579 => {
                let loud = std::fs::read(crate::cache::dir().join("vencord.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .is_some_and(|v| v["plugins"]["FcE2e"]["loud"] == true);
                if !loud {
                    return;
                }
                self.mark("vc_setting");
                app.sform.plugin_search.clear();
                app.sform.plugin_open.clear();
                self.next(300);
            }
            // Vencord's own section: its page, the notification the modal sent (a card, and in
            // the log), and QuickCSS colouring Fastcord
            580 => {
                if self.open_page(app, "Vencord") {
                    self.next(800);
                }
            }
            581 => {
                if Self::at(ctx, "action:Notification Log").is_none() {
                    return;
                }
                self.shot(ctx, "vencord_section.png");
                app.vc.log_open = true;
                app.vc_call("log", serde_json::json!({}));
                self.next(600);
            }
            582 => {
                let logged = app.vc.log.as_ref().and_then(|l| l.as_array()).is_some_and(|l| l.iter().any(|n| n["title"] == "E2E notice"));
                if !logged {
                    return;
                }
                self.mark("vc_notify_log");
                self.shot(ctx, "vencord_log.png");
                app.vc.log_open = false;
                crate::ui::vencord::set_quick_css(":root { --background-primary: #203040; }");
                app.vc_call("setPath", serde_json::json!({ "path": ["useQuickCss"], "value": true }));
                self.next(600);
            }
            583 => {
                if crate::theme::pal().bg_primary != egui::Color32::from_rgb(0x20, 0x30, 0x40) {
                    return;
                }
                self.mark("vc_quickcss");
                self.shot(ctx, "vencord_quickcss.png");
                crate::ui::vencord::set_quick_css("");
                app.vc.restyle = true;
                self.key(Key::Escape);
                self.step = 99;
                self.step_at = Instant::now();
            }
            _ => self.step = 99,
        }
    }
}
