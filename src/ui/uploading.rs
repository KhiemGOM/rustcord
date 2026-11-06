//! The card a message shows while its files are still going up: how far, and a way to stop.

use std::sync::atomic::Ordering::Relaxed;

use egui::{Align2, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::text;
use crate::theme::*;
use crate::upload::{Job, size_text};

pub struct Card {
    pub cancel: bool,
    pub rect: Rect,
    pub bar: Rect,
    pub x: Rect,
}

pub fn card(ui: &mut Ui, job: &Job) -> Card {
    let (sent, total) = (job.sent.load(Relaxed).min(job.total()), job.total().max(1));
    let name = match job.files.len() {
        0 | 1 => job.files.first().map(|f| f.0.clone()).unwrap_or_default(),
        n => format!("{} and {} more", job.files[0].0, n - 1),
    };
    let mut cancel = false;
    let mut bar = Rect::NOTHING;
    let mut cancel_at = Rect::NOTHING;
    let r = Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(12, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width().min(432.0) - 24.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        icons::draw(ui.painter(), Icon::File, Rect::from_min_size(row.min, vec2(30.0, 40.0)), pal().header_secondary);
        let x = row.left() + 40.0;
        text::paint(ui.painter(), pos2(x, row.top() + 10.0), Align2::LEFT_CENTER, &name, medium(16.0), pal().link, row.right() - x - 32.0);
        let done = job.files.is_empty() || sent >= total;
        let line = if done { "Finishing up...".to_string() } else { format!("{} of {}", size_text(sent), size_text(total)) };
        ui.painter().text(pos2(x, row.top() + 30.0), Align2::LEFT_CENTER, line, reg(12.0), pal().text_muted);
        let x_btn = Rect::from_center_size(pos2(row.right() - 10.0, row.top() + 10.0), vec2(20.0, 20.0));
        cancel_at = x_btn;
        let resp = ui.interact(x_btn, ui.id().with("cancel_upload"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x_btn, super::motion::mix(pal().header_secondary, pal().critical, super::motion::hot(ui.ctx(), &resp)));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        cancel = resp.on_hover_text("Cancel Upload").clicked();
        ui.add_space(8.0);
        let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
        ui.painter().rect_filled(track, CornerRadius::same(2), pal().bg_accent);
        bar = Rect::from_min_size(track.min, vec2(track.width() * sent as f32 / total as f32, 4.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), BLURPLE);
        ui.ctx().request_repaint_after_secs(0.1);
    });
    Card { cancel, rect: r.response.rect, bar, x: cancel_at }
}
