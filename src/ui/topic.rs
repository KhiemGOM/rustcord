use egui::{Align, Frame, Layout, RichText, Stroke};

use super::icons::Icon;
use super::{App, icon_button};
use crate::theme::*;

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.topic else { return };
    let Some(chan) = app.store.channels.get(&id) else {
        app.topic = None;
        return;
    };
    let (name, topic) = (chan.name.clone(), chan.topic.clone().unwrap_or_default());
    let mut close = false;
    let modal = crate::ui::motion::Modal::new("topic".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("#{name}")).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(16.0);
            egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&topic).font(reg(16.0)).color(pal().text_normal)).selectable(true));
            });
        });
    if close || modal.should_close() {
        app.topic = None;
    }
}
