//! Modify Attachment: the name, the alt text and the spoiler mark of a file waiting in the
//! composer, from the pencil on its card or E while the card has the keyboard.

use egui::{Color32, Frame, Key, Margin, RichText, Stroke};

use super::{App, button, grey_button, icon_button, settings};
use super::icons::Icon;
use crate::theme::*;

pub struct Modify {
    index: usize,
    name: String,
    description: String,
    spoiler: bool,
    image: bool,
}

pub fn open(app: &mut App, index: usize) {
    if let Some(f) = app.files.get(index) {
        app.modify = Some(Modify { index, name: f.name.clone(), description: f.description.clone(), spoiler: f.spoiler, image: f.is_image() });
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(m) = &mut app.modify else { return };
    let (mut save, mut close) = (false, false);
    let modal = egui::Modal::new("modify_attachment".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Modify Attachment").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Filename").font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let r = settings::input(ui, &mut m.name, "", false);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "modify_name")), r.rect));
            save |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if m.image {
                ui.add_space(16.0);
                ui.label(RichText::new("Alt Text").font(medium(14.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut m.description)
                            .frame(Frame::NONE)
                            .margin(Margin::ZERO)
                            .desired_rows(3)
                            .char_limit(1024)
                            .font(reg(16.0))
                            .hint_text(RichText::new("Describe the image for people who can't see it").color(pal().text_muted))
                            .desired_width(ui.available_width()),
                    );
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mark as spoiler").font(reg(16.0)).color(pal().text_normal));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if settings::switch(ui, m.spoiler).clicked() {
                            m.spoiler = !m.spoiler;
                        }
                    });
                });
            }
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= grey_button(ui, "Cancel", 192.0).clicked();
                let b = button(ui, "Save", BLURPLE, 192.0);
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "modify_save")), b.rect));
                save |= b.clicked();
            });
        });
    save |= super::enter_submits(ctx, false);
    if save && let Some(m) = app.modify.take() {
        if let Some(f) = app.files.get_mut(m.index) {
            let name: String = m.name.trim().chars().filter(|c| !matches!(c, '/' | '\\')).collect();
            if !name.is_empty() {
                f.name = name;
            }
            f.description = m.description;
            f.spoiler = m.spoiler;
        }
    } else if close || modal.should_close() {
        app.modify = None;
    }
}
