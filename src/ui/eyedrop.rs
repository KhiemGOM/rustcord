//! "Pick a color from the page": the next screenshot is the page, and a click on it picks a pixel.

use egui::{Color32, Context, CornerRadius, Rect, Sense, Stroke, vec2};
use std::sync::Arc;

use crate::theme::*;

pub fn start(ctx: &Context, key: &'static str) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(key.to_string())));
}

/// Takes in the screenshot `start` asked for and, while one is held, lets the pointer pick from it.
/// Returns the picked colour; Escape lets go of the screenshot.
pub fn pick(ctx: &Context, key: &'static str, held: &mut Option<Arc<egui::ColorImage>>) -> Option<Color32> {
    let shot = ctx.input(|i| {
        i.raw.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { user_data, image, .. } if user_data.data.as_ref().and_then(|d| d.downcast_ref::<String>()).is_some_and(|n| n == key) => Some(image.clone()),
            _ => None,
        })
    });
    if shot.is_some() {
        *held = shot;
    }
    let img = held.clone()?;
    let ppp = ctx.pixels_per_point();
    let at = |p: egui::Pos2| img.pixels.get((p.y * ppp) as usize * img.size[0] + (p.x * ppp) as usize).copied().unwrap_or_default();
    let screen = ctx.content_rect();
    let mut picked = None;
    egui::Area::new(key.into()).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ctx, |ui| {
        let (_, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if let Some(p) = ctx.pointer_hover_pos() {
            let c = at(p);
            let r = Rect::from_center_size(p + vec2(28.0, -28.0), vec2(40.0, 40.0));
            ui.painter().rect(r, CornerRadius::same(20), c, Stroke::new(3.0, Color32::WHITE), egui::StrokeKind::Outside);
            ui.painter().text(r.center_bottom() + vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()), medium(12.0), Color32::WHITE);
        }
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            picked = Some(at(p));
        }
    });
    if picked.is_some() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *held = None;
    }
    picked
}
