use egui::{Align, Frame, Layout, RichText, Sense, Stroke, vec2};

use super::icons::{self, Icon};
use super::{App, icon_button};
use crate::theme::*;

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.feed_about {
        return;
    }
    let mut close = false;
    let mut privacy = false;
    let modal = crate::ui::motion::Modal::new("feed_about".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("About Recent Activity").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Keep up with what your friends are doing and easily start a conversation or join them!").font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            ui.label(RichText::new("What you can do").font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let notes = [
                (Icon::People, "See the activity of people you talk to most"),
                (Icon::Bubble, "Spark a conversation with a reply or emoji"),
                (Icon::Clock, "View and delete your past activity from your profile"),
            ];
            for (icon, text) in notes {
                note(ui, icon, |ui| {
                    ui.label(RichText::new(text).font(medium(14.0)).color(pal().text_muted));
                });
            }
            note(ui, Icon::Gear, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("Manage your ").font(medium(14.0)).color(pal().text_muted));
                    privacy = super::settings::link(ui, "Activity Privacy").clicked();
                });
            });
        });
    if privacy {
        app.settings_page = Some(super::settings::Page::Activity);
    }
    if close || privacy || modal.should_close() {
        app.feed_about = false;
    }
}

fn note(ui: &mut egui::Ui, icon: Icon, text: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        icons::draw(ui.painter(), icon, r, pal().text_muted);
        text(ui);
    });
    ui.add_space(8.0);
}
