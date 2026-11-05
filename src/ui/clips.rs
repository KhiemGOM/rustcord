//! Clips on the app side: the save keybind, the toasts and sounds around a save, the consent
//! flags sent to the call, the clip button on the account panel and the badge in the call.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, menu_item};
use crate::clips::{self, Clip, Context, Outcome};
use crate::state::Id;
use crate::theme::*;
use crate::voice::{self, Ctl};

#[derive(Default)]
pub struct State {
    live: bool,
    streaming: bool,
    sent: Option<u32>,
    warned: Option<Id>,
    session: (String, u32),
    pub list: Option<Vec<Clip>>,
    pub gallery: Option<super::clips_gallery::Gallery>,
}

pub const CALL_WARNING: &str = "A participant in the call you joined has clipping enabled, your voice may be recorded.";

impl App {
    pub fn sync_hotkeys(&self) {
        let wanted = self.settings.clips && clips::active();
        let binds: Vec<_> = self.settings.keybinds.iter().filter(|(a, _)| a != "save_clip" || wanted).cloned().collect();
        crate::hotkeys::set(&binds);
    }

    pub fn clips_tick(&mut self, ctx: &egui::Context) {
        let live = clips::active();
        if live != self.clips.live {
            self.clips.live = live;
            self.sync_hotkeys();
        }
        if self.settings.clips && !self.settings.keybinds.iter().any(|(a, _)| a == "save_clip") {
            self.settings.keybinds.push(("save_clip".into(), clips::DEFAULT_KEYS.to_vec()));
            self.save_settings(ctx);
        }
        let streaming = clips::streaming();
        if self.clips.streaming && !streaming {
            let (app, n) = std::mem::take(&mut self.clips.session);
            if n > 0 && self.settings.clips_reminders {
                self.toast(format!("You captured clips during your recent {app} stream. Check them out in your clips library!"));
            }
        }
        self.clips.streaming = streaming;
        self.clip_flags();
        for o in clips::outcomes() {
            match o {
                Outcome::Saved(c) => {
                    self.clips.session = (c.app.clone(), self.clips.session.1 + 1);
                    if let Some(l) = &mut self.clips.list {
                        l.insert(0, c);
                    }
                    self.toast("You recently captured a clip!");
                }
                Outcome::Failed(_) => {
                    crate::sound::cue("clip_error");
                    self.toast("Unable to save clip");
                }
            }
        }
    }

    fn clip_flags(&mut self) {
        let connected = self.voice.state == voice::State::Connected && self.voice.chan.is_some();
        if !connected {
            self.clips.sent = None;
            return;
        }
        let f = if self.settings.clips { clips::CLIPS_ENABLED } else { 0 } | if self.settings.clips_voice { clips::ALLOW_VOICE_RECORDING } else { 0 };
        if self.clips.sent != Some(f) {
            self.clips.sent = Some(f);
            self.net.send(crate::net::Cmd::Voice(voice::Cmd::Ctl(Ctl::Flags(f))));
        }
        if self.clips.warned.is_some() && self.clips.warned != self.voice.chan {
            self.clips.warned = None;
        }
        if self.clips.warned.is_none() && self.settings.clips_voice && clips::others_clipping(self.store.me) {
            self.clips.warned = self.voice.chan;
            self.toast(CALL_WARNING);
        }
    }

    pub fn save_clip(&mut self, ctx: &egui::Context) {
        if !self.settings.clips {
            return;
        }
        let users = self.voice.chan.map(|c| self.voice.members(&self.store, c)).unwrap_or_default();
        let at = Context { users, guild: self.voice.guild, channel: self.voice.chan.unwrap_or(0) };
        if clips::save(ctx, clips::folder(&self.settings.clips_dir), at).is_ok() {
            crate::sound::cue("clip_save");
            self.toast("Saving clip");
        }
    }

    pub fn clip_keys(&self) -> Option<String> {
        let (_, k) = self.settings.keybinds.iter().find(|(a, _)| a == "save_clip")?;
        (!k.is_empty()).then(|| super::settings::combo(k))
    }

    /// Discord's tooltip for the clip button, in the order its hook checks.
    pub fn clip_tip(&self) -> String {
        if !self.settings.clips {
            "Clipping is disabled".into()
        } else if !clips::streaming() {
            "Stream to Start Clipping".into()
        } else if let Some(k) = self.clip_keys() {
            format!("Save Clip ({k})")
        } else {
            "Save Clip".into()
        }
    }

    pub fn open_clips(&mut self) {
        self.clips.list = None;
        self.clips.gallery = Some(Default::default());
    }
}

/// The clip button beside the account panel's mic and headphones.
pub fn panel_button(app: &mut App, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let ready = clips::streaming() && clips::saving() < clips::MAX_SAVES;
    if resp.hovered() && ready {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal().control_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = if !ready { pal().channel_muted } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), Icon::Clips, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), ink);
    app.rects.insert("clips:button".into(), rect);
    let tip = app.clip_tip();
    let resp = resp.on_hover_text(tip);
    if resp.clicked() && ready {
        let ctx = ui.ctx().clone();
        app.save_clip(&ctx);
    }
    resp.context_menu(|ui| {
        if menu_item(ui, Some(Icon::Clips), "View Clips", false) {
            app.open_clips();
            ui.close();
        }
        if menu_item(ui, Some(Icon::Gear), "Clips Settings", false) {
            app.settings_page = Some(super::settings::Page::Clips);
            ui.close();
        }
    });
}

/// "Enabled" beside a clapper in the call: you're clipping, or someone in it is.
pub fn badge(app: &mut App, ui: &mut Ui, at: egui::Pos2) {
    if !(app.settings.clips && clips::streaming() || clips::others_clipping(app.store.me)) {
        return;
    }
    let ink = Color32::from_rgb(0x4e, 0x50, 0x58);
    let g = ui.painter().layout_no_wrap("Enabled".into(), medium(12.0), Color32::WHITE);
    let r = Rect::from_min_size(at, vec2(24.0 + g.size().x + 12.0, 24.0));
    ui.painter().rect_filled(r, CornerRadius::same(12), ink);
    icons::draw(ui.painter(), Icon::Clips, Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(14.0, 14.0)), Color32::WHITE);
    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, "Enabled", medium(12.0), Color32::WHITE);
    app.rects.insert("clips:badge".into(), r);
    ui.interact(r, egui::Id::new("clips_badge"), Sense::hover()).on_hover_text("Participants in this call have clips enabled.");
}
