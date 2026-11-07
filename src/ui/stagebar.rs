//! The tray along the bottom of a stage room, as Discord draws it: 40px icon buttons in sections
//! (labels are tooltips), the music button first, and a Disconnect button of its own. Mute Music
//! and the rules for when the waiting music plays are in `stagemusic`.

use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::callbar::{audio_popup, frame as popup_frame, more_popup};
use super::icons::{self, Icon};
use super::stage::is_mod;
use super::{App, DANGER, menu_item};
use crate::state::{Channel, Id};
use crate::theme::*;

const SECTION: Color32 = Color32::from_rgb(19, 20, 22);
const SECTION_LINE: Color32 = Color32::from_rgba_premultiplied(6, 6, 7, 15);
const ACTIVE: Color32 = Color32::from_rgba_premultiplied(0, 16, 8, 31);
const ACTIVE_INK: Color32 = Color32::from_rgb(115, 196, 139);
const ACTIVE_HOVER: Color32 = Color32::from_rgb(36, 128, 70);
const REQUEST_TO_SPEAK: u64 = 1 << 32;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Look {
    Plain,
    Active,
    Off,
}

fn section(ui: &Ui, left: f32, cy: f32, n: usize, extra: f32) -> Rect {
    let r = Rect::from_min_size(pos2(left, cy - 25.0), vec2(n as f32 * 40.0 + (n.max(1) - 1) as f32 * 8.0 + 10.0 + extra, 50.0));
    ui.painter().rect(r, CornerRadius::same(12), SECTION, Stroke::new(1.0, SECTION_LINE), StrokeKind::Inside);
    r
}

pub(super) fn button(ui: &mut Ui, app: &mut App, at: Rect, id: &str, icon: Icon, label: &str, look: Look, ink: Color32) -> Response {
    let resp = ui.interact(at, egui::Id::new(("stage_tray", id)), if look == Look::Off { Sense::hover() } else { Sense::click() });
    let hot = look != Look::Off && resp.hovered();
    if hot || look == Look::Active {
        let fill = match (look, hot) {
            (Look::Active, true) => ACTIVE_HOVER,
            (Look::Active, false) => ACTIVE,
            _ => pal().bg_mod_hover,
        };
        ui.painter().rect_filled(at, CornerRadius::same(8), fill);
    }
    let ink = match look {
        Look::Active if hot => Color32::WHITE,
        Look::Active => ACTIVE_INK,
        _ => ink,
    };
    icons::draw(ui.painter(), icon, Rect::from_center_size(at.center(), vec2(20.0, 20.0)), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(if look == Look::Off { egui::CursorIcon::NotAllowed } else { egui::CursorIcon::PointingHand });
    }
    app.rects.insert(format!("stage:ctl:{label}"), at);
    resp.on_hover_text(label)
}

fn slot(s: Rect, i: usize) -> Rect {
    Rect::from_min_size(pos2(s.left() + 5.0 + i as f32 * 48.0, s.top() + 5.0), vec2(40.0, 40.0))
}

pub fn controls(app: &mut App, ui: &mut Ui, chan: &Channel, bar: Rect) {
    let cy = bar.bottom() - 33.0;
    let me = app.store.me;
    let audience = app.voice.stage.seats.get(&me).is_some_and(|s| s.audience);
    let hand = app.voice.stage.seats.get(&me).is_some_and(|s| s.hand.is_some());
    let mods = is_mod(app, chan);
    let live = app.voice.stage.live.contains_key(&chan.id);
    let can_ask = app.store.perms(chan) & REQUEST_TO_SPEAK != 0;
    let speak_here = audience && mods;
    let mic_w = 40.0 + 8.0 + 20.0;
    let a = if audience { 3 + speak_here as usize } else { 4 };
    let widths: Vec<f32> = if audience { vec![a as f32 * 48.0 + 2.0] } else { vec![mic_w + 10.0, a as f32 * 48.0 + 2.0] };
    let end_w = if mods && live { 66.0 + 24.0 } else { 66.0 };
    let total = widths.iter().map(|w| w + 12.0).sum::<f32>() + end_w;
    let mut x = bar.center().x - total / 2.0;
    if !audience {
        let s = section(ui, x, cy, 1, 28.0);
        let mute = app.voice.mute || app.voice.deaf;
        let mic = Rect::from_min_size(pos2(s.left() + 5.0, s.top() + 5.0), vec2(40.0, 40.0));
        let mic = button(ui, app, mic, "mic", if mute { Icon::MicOff } else { Icon::Mic }, if mute { "Unmute" } else { "Mute" }, Look::Plain, if mute { DANGER } else { Color32::WHITE });
        if mic.clicked() {
            app.voice.set_mute(&app.net, !mute);
        }
        let caret = Rect::from_min_size(pos2(s.left() + 5.0 + 48.0, s.top() + 5.0), vec2(20.0, 40.0));
        let more = ui.interact(caret, egui::Id::new(("stage_tray", "mic_more")), Sense::click());
        if more.hovered() {
            ui.painter().rect_filled(caret, CornerRadius::same(8), pal().bg_mod_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(caret.center(), vec2(16.0, 16.0)), Color32::WHITE);
        app.rects.insert("voice:mic_more".into(), caret);
        audio_popup(app, &more.on_hover_text("More Microphone Options"), false);
        x += s.width() + 12.0;
    }
    let s = section(ui, x, cy, a, 0.0);
    let mut i = 0;
    super::stagemusic::tray(ui, app, chan, slot(s, i));
    i += 1;
    let (label, look, icon) = match (hand, can_ask) {
        (true, _) => ("Cancel Request", Look::Active, Icon::Hand),
        (false, true) => ("Request to Speak", Look::Plain, Icon::Hand),
        (false, false) => ("Raise Hand Disabled", Look::Off, Icon::Hand),
    };
    if button(ui, app, slot(s, i), "hand", icon, label, look, Color32::WHITE).clicked() {
        app.request_to_speak(chan, !hand);
    }
    i += 1;
    if speak_here {
        if button(ui, app, slot(s, i), "speak", Icon::Mic, "Speak on Stage", Look::Plain, Color32::WHITE).clicked() {
            app.set_speaker(chan, me, true);
        }
        i += 1;
    }
    if !audience {
        if button(ui, app, slot(s, i), "audience", Icon::Leave, "Leave the Stage", Look::Plain, Color32::WHITE).clicked() {
            app.set_speaker(chan, me, false);
        }
        i += 1;
    }
    let more = button(ui, app, slot(s, i), "more", Icon::More, "More Options", Look::Plain, Color32::WHITE);
    more_popup(app, &more);
    x += s.width() + 12.0;
    disconnect(app, ui, chan, x, cy, mods && live);
}

fn disconnect(app: &mut App, ui: &mut Ui, chan: &Channel, x: f32, cy: f32, can_end: bool) {
    let end = Rect::from_min_size(pos2(x, cy - 25.0), vec2(66.0, 50.0));
    let r = ui.interact(end, egui::Id::new(("stage_tray", "leave")), Sense::click());
    let shape = if can_end { CornerRadius { nw: 12, sw: 12, ne: 0, se: 0 } } else { CornerRadius::same(12) };
    ui.painter().rect_filled(end, shape, if r.hovered() { DANGER.gamma_multiply(0.85) } else { DANGER });
    icons::draw(ui.painter(), Icon::HangUp, Rect::from_center_size(end.center(), vec2(20.0, 20.0)), Color32::WHITE);
    app.rects.insert("voice:leave_big".into(), end);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.on_hover_text("Disconnect Quietly").clicked() {
        app.voice.leave(&app.net);
    }
    if !can_end {
        return;
    }
    let caret = Rect::from_min_size(pos2(end.right(), end.top()), vec2(24.0, 50.0));
    let c = ui.interact(caret, egui::Id::new(("stage_tray", "end_more")), Sense::click());
    ui.painter().rect_filled(caret, CornerRadius { nw: 0, sw: 0, ne: 12, se: 12 }, if c.hovered() { DANGER.gamma_multiply(0.85) } else { DANGER });
    icons::draw(ui.painter(), Icon::ChevronUp, Rect::from_center_size(caret.center(), vec2(16.0, 16.0)), Color32::WHITE);
    app.rects.insert("stage:ctl:End Stage".into(), caret);
    if c.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ch: Id = chan.id;
    egui::Popup::menu(&c).align(egui::RectAlign::TOP).gap(8.0).frame(popup_frame()).show(|ui| {
        ui.set_min_width(222.0);
        if menu_item(ui, Some(Icon::HangUp), "End Stage for Everyone", true) {
            app.confirm = Some(super::Confirm::new("End Stage for Everyone", "Are you sure you want to end the Stage for everyone?\n\nThe Stage will end immediately.", "End Stage", move |app: &mut App| app.end_stage(ch)));
            ui.close();
        }
    });
}
