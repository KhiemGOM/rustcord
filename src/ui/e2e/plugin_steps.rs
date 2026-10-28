//! The Vencord host: the run's userplugin (e2e/fce2e.tsx) shows on the Plugins page, its switch
//! starts it, and then its send listener, command, menu item, chat bar button and flux handler
//! each do their part.

use egui::Key;
use std::time::Instant;

use super::Run;
use crate::ui::App;

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
                self.step = 99;
                self.step_at = Instant::now();
            }
            _ => self.step = 99,
        }
    }
}
