//! Create Reminder's Custom... dialog: a time in words, read the way the composer's @time reads it.

use chrono::{DateTime, Local};
use egui::{Align, Frame, Layout, RichText, Stroke, vec2};

use super::{App, button, icon_button};
use super::icons::Icon;
use crate::state::Id;
use crate::theme::*;

pub struct Custom {
    ch: Id,
    msg: Id,
    text: String,
    fresh: bool,
}

impl Custom {
    pub fn new(ch: Id, msg: Id) -> Self {
        Custom { ch, msg, text: String::new(), fresh: true }
    }

    fn when(&self) -> Option<DateTime<Local>> {
        super::when::parse(self.text.trim(), Local::now()).filter(|t| *t > Local::now())
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(c) = &mut app.remind_custom else { return };
    let (mut set, mut close) = (false, false);
    let when = c.when();
    let modal = crate::ui::motion::Modal::new("remind_custom".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Remind Me").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(432.0, 40.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 8, pal().input_bg);
            let inner = r.shrink2(vec2(12.0, 10.0));
            let e = ui.put(inner, egui::TextEdit::singleline(&mut c.text).hint_text(RichText::new("5pm, tomorrow 9am, in 2 hours, friday").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(egui::Margin::ZERO));
            super::settings::mark(ui, "remind_custom:input", r);
            ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if e.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
            ui.advance_cursor_after_rect(r);
            if std::mem::take(&mut c.fresh) {
                e.request_focus();
            }
            if e.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                set = when.is_some();
            }
            ui.add_space(8.0);
            let line = match (&when, c.text.trim().is_empty()) {
                (Some(t), _) => t.format("%A, %B %-d at %-I:%M %p").to_string(),
                (None, true) => String::new(),
                (None, false) => "That isn't a time in the future".to_string(),
            };
            ui.label(RichText::new(line).font(reg(14.0)).color(if when.is_some() { pal().text_normal } else { pal().text_muted }));
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = ui.add_enabled_ui(when.is_some(), |ui| button(ui, "Set Reminder", pal().blurple, 130.0)).inner;
                super::settings::mark(ui, "remind_custom:set", b.rect);
                set |= b.clicked();
            });
        });
    close |= modal.should_close();
    if set && let Some(t) = when {
        let (ch, msg) = (c.ch, c.msg);
        app.remind_custom = None;
        app.remind(ch, msg, t.with_timezone(&chrono::Utc));
    } else if close {
        app.remind_custom = None;
    }
}
