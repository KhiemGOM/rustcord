use egui::{Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::plugins;
use crate::state::{Id, Status, Store};
use crate::theme::*;

pub const NAMES: [&str; 3] = ["Desktop", "Mobile", "Web"];

/// The clients a user is on, desktop, mobile and web, each with the status it shows there.
pub fn of(store: &Store, uid: Id) -> Vec<(usize, Status)> {
    let Some(c) = store.clients.get(&uid) else { return Vec::new() };
    c.iter().enumerate().filter(|(_, s)| **s != Status::Offline).map(|(i, s)| (i, *s)).collect()
}

/// One marker in a square `rect`, tinted by the status. The phone keeps its colour only with
/// colorMobileIndicator.
pub fn draw(p: &Painter, kind: usize, rect: Rect, status: Status) {
    let col = if kind == 1 && !plugins::flag("PlatformIndicators", "colorMobileIndicator") { pal().header_secondary } else { status_color(status) };
    let st = Stroke::new(rect.width() / 9.0, col);
    let u = rect.width() / 24.0;
    let c = rect.center();
    match kind {
        0 => {
            let screen = Rect::from_center_size(c - vec2(0.0, 2.0 * u), vec2(20.0 * u, 13.0 * u));
            p.rect_stroke(screen, 2.0 * u, st, egui::StrokeKind::Inside);
            p.line_segment([pos2(c.x, c.y + 4.5 * u), pos2(c.x, c.y + 8.0 * u)], st);
            p.line_segment([pos2(c.x - 5.0 * u, c.y + 8.5 * u), pos2(c.x + 5.0 * u, c.y + 8.5 * u)], st);
        }
        1 => {
            let body = Rect::from_center_size(c, vec2(12.0 * u, 20.0 * u));
            p.rect_stroke(body, 3.0 * u, st, egui::StrokeKind::Inside);
            p.circle_filled(pos2(c.x, c.y + 7.5 * u), 1.2 * u, col);
        }
        _ => {
            let r = 9.0 * u;
            p.circle_stroke(c, r, st);
            p.line_segment([pos2(c.x - r, c.y), pos2(c.x + r, c.y)], st);
            p.line_segment([pos2(c.x, c.y - r), pos2(c.x, c.y + r)], st);
            p.add(egui::epaint::EllipseShape::stroke(c, vec2(r * 0.45, r), st));
        }
    }
}

/// A user's markers in a row growing leftwards from `right`, 14 apiece. Returns the room used.
pub fn row(ui: &egui::Ui, store: &Store, uid: Id, right: Pos2, hover_id: egui::Id) -> f32 {
    let mut x = right.x;
    for (kind, st) in of(store, uid) {
        let r = Rect::from_center_size(pos2(x - 7.0, right.y), vec2(14.0, 14.0));
        draw(ui.painter(), kind, r, st);
        ui.interact(r, hover_id.with(kind), egui::Sense::hover()).on_hover_text(NAMES[kind]);
        x -= 16.0;
    }
    right.x - x
}
