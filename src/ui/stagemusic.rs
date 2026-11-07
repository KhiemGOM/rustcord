//! Mute Music: the looping stage_waiting sound plays while you sit in a stage that hasn't started
//! and nobody on it is unmuted, unless it's muted (kept on this device) or you're deafened. In a
//! live stage the same button reads Unmute Music and plays it on request, until you leave.

use egui::{Color32, Rect, Ui};

use super::App;
use super::icons::{self, Icon};
use super::stagebar::{Look, button};
use crate::state::{Channel, Id};
use crate::theme::pal;

impl App {
    pub fn stage_waiting(&self) -> Option<Id> {
        let ch = self.voice.chan.filter(|_| self.voice.stage_room)?;
        if self.voice.stage.live.contains_key(&ch) {
            return None;
        }
        let members = self.voice.members(&self.store, ch);
        let (speakers, _) = self.voice.stage.split(&members);
        let talking = speakers.iter().any(|u| !self.voice.muted.get(u).is_some_and(|m| m[0] || m[2]));
        (!talking).then_some(ch)
    }

    pub fn stage_music(&self) -> bool {
        let Some(ch) = self.voice.chan.filter(|_| self.voice.stage_room && !self.voice.deaf) else { return false };
        if self.call.music_forced == Some(ch) {
            return true;
        }
        !self.settings.stage_music_muted && self.stage_waiting() == Some(ch)
    }
}

fn playing(app: &App, ch: Id) -> bool {
    if app.stage_waiting() == Some(ch) { !app.settings.stage_music_muted } else { app.call.music_forced == Some(ch) }
}

pub fn tray(ui: &mut Ui, app: &mut App, chan: &Channel, at: Rect) {
    let on = playing(app, chan.id);
    let look = if on { Look::Active } else { Look::Plain };
    let (icon, label) = if on { (Icon::Music, "Mute Music") } else { (Icon::MusicOff, "Unmute Music") };
    if button(ui, app, at, "music", icon, label, look, Color32::WHITE).clicked() {
        press(app, chan.id);
    }
}

pub fn press(app: &mut App, ch: Id) {
    if app.stage_waiting() == Some(ch) {
        app.settings.stage_music_muted = !app.settings.stage_music_muted;
        app.call.music_forced = None;
        crate::cache::set_settings(&app.settings);
    } else {
        app.call.music_forced = if app.call.music_forced == Some(ch) { None } else { Some(ch) };
    }
}

pub fn panel(app: &mut App, ui: &mut Ui, center: egui::Pos2) -> bool {
    let me = app.store.me;
    let Some(ch) = app.stage_waiting() else { return false };
    if !app.voice.stage.seats.get(&me).is_some_and(|s| s.audience) {
        return false;
    }
    let r = Rect::from_center_size(center, egui::vec2(32.0, 32.0));
    let resp = ui.interact(r, egui::Id::new("stage_music_btn"), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, egui::CornerRadius::same(4), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let on = !app.settings.stage_music_muted;
    let ink = if resp.hovered() { pal().header_primary } else { pal().header_secondary };
    icons::draw(ui.painter(), if on { Icon::Music } else { Icon::MusicOff }, Rect::from_center_size(center, egui::vec2(20.0, 20.0)), ink);
    app.rects.insert("voice:music".into(), r);
    if resp.on_hover_text(if on { "Mute Music" } else { "Unmute Music" }).clicked() {
        press(app, ch);
    }
    true
}
