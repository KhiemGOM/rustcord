//! Notifications, System, Language & Time and Developer settings, in steps 2700 and up.

use super::*;

impl Run {
    pub(super) fn system_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let goto = |section: &str| ctx.data_mut(|d| d.insert_temp(egui::Id::new("settings_goto"), section.to_string()));
        match self.step {
            2700 => {
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            2701 => {
                app.sform.search = "tip emails".into();
                self.next(300);
            }
            2702 => {
                if app.rects.contains_key("settings:Notifications") && !app.rects.contains_key("settings:Nitro") {
                    self.mark("settings_search");
                }
                app.sform.search.clear();
                self.next(300);
            }
            2703 => {
                if !self.open_page(app, "Notifications") {
                    return;
                }
                self.next(900);
            }
            2704 => {
                let Some(r) = Self::at(ctx, "toggle:Friends come online") else { return };
                self.click(r.center());
                self.next(400);
            }
            2705 => {
                goto("Email");
                self.next(900);
            }
            2706 => {
                let Some(r) = Self::at(ctx, "toggle:Social Emails") else { return };
                self.shot(ctx, "settings_notifications_email.png");
                self.click(r.center());
                self.next(500);
            }
            2707 => {
                goto("Advanced");
                self.next(900);
            }
            2708 => {
                let Some(r) = Self::at(ctx, "notify:advanced") else { return };
                self.click(r.center());
                self.next(600);
            }
            2709 => {
                let Some(r) = Self::at(ctx, "toggle:Allow playback and usage of /tts command") else { return };
                self.shot(ctx, "settings_notifications_advanced.png");
                self.click(r.center());
                self.next(500);
            }
            2710 => {
                if !self.open_page(app, "System") {
                    return;
                }
                self.next(900);
            }
            2711 => {
                let Some(r) = Self::at(ctx, "toggle:Enable Hardware Acceleration") else { return };
                self.shot(ctx, "settings_system_general.png");
                self.click(r.center());
                self.next(400);
            }
            2712 => {
                if !app.settings.hw_accel {
                    self.mark("hw_accel_off");
                }
                if !self.open_page(app, "Language & Time") {
                    return;
                }
                self.next(900);
            }
            2713 => {
                let Some(r) = Self::at(ctx, "radio:24-hour") else { return };
                self.click(r.center());
                self.next(500);
            }
            2714 => {
                if !self.open_page(app, "Developer") {
                    return;
                }
                self.next(900);
            }
            2715 => {
                let Some(r) = Self::at(ctx, "toggle:Developer Mode") else { return };
                self.click(r.center());
                self.next(500);
            }
            2716 => {
                self.mark("system_pages");
                self.key(Key::Escape);
                self.step = 349;
                self.next(400);
            }
            _ => self.step = 99,
        }
    }
}
