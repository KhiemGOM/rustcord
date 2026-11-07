//! The Stickers page's boost frame: the Get Boosted banner, and the timeline of Free Slots and the
//! three levels below it, each level with the slots it adds and a Buy Level button.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::faint;
use crate::theme::*;
use crate::ui::icons::{self, Icon};

const PINK: Color32 = Color32::from_rgb(0xff, 0x73, 0xfa);

pub fn banner(ui: &mut Ui, level: u32) -> (bool, bool) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 218.0), Sense::hover());
    let mut m = egui::Mesh::default();
    let (a, b) = (Color32::from_rgb(0x4f, 0x4c, 0xe0), Color32::from_rgb(0x9a, 0x3f, 0xe6));
    m.colored_vertex(r.left_top(), a);
    m.colored_vertex(r.right_top(), b);
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), a);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    let p = ui.painter();
    for (at, size) in [(pos2(r.left() + 36.0, r.top() + 80.0), 44.0), (pos2(r.right() - 90.0, r.top() + 58.0), 36.0), (pos2(r.left() + 90.0, r.bottom() - 40.0), 28.0)] {
        icons::draw(p, Icon::Gem, Rect::from_center_size(at, vec2(size, size)), PINK);
    }
    p.text(pos2(r.center().x, r.top() + 36.0), Align2::CENTER_CENTER, "Get Boosted", semi(24.0), Color32::WHITE);
    let g = p.layout(format!("Enjoy more stickers and other perks by boosting your server to Level {level}. Each Level unlocks more sticker slots and new benefits for everyone."), medium(16.0), Color32::WHITE, (r.width() - 180.0).min(480.0));
    let gh = g.size().y;
    p.galley(pos2(r.center().x - g.size().x / 2.0, r.top() + 64.0), g, Color32::WHITE);
    let y = r.top() + 64.0 + gh + 24.0;
    let mut hit = (false, false);
    let widths = [89.0 + 40.0, 79.0 + 40.0];
    let mut x = r.center().x - (widths[0] + widths[1] + 10.0) / 2.0;
    for (i, label) in ["Boost Server", "Learn More"].into_iter().enumerate() {
        let b = Rect::from_min_size(pos2(x, y), vec2(widths[i], 36.0));
        let resp = ui.interact(b, ui.id().with(("stickline_banner", i)), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(4), if resp.hovered() { Color32::from_rgb(0xf2, 0xf3, 0xf5) } else { Color32::WHITE });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, label, medium(16.0), Color32::BLACK);
        super::super::settings::mark(ui, &format!("stickers:{label}"), b);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            if i == 0 { hit.0 = true } else { hit.1 = true }
        }
        x += widths[i] + 10.0;
    }
    hit
}

pub struct Node {
    pub y: f32,
    pub reached: bool,
}

/// The line and nodes down the left of the timeline, painted into `slot` once the content has said where its nodes are.
pub fn rail(ui: &mut Ui, slot: egui::layers::ShapeIdx, x: f32, nodes: &[Node]) {
    let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else { return };
    let mut shapes = vec![Shape::line_segment([pos2(x, first.y), pos2(x, last.y)], Stroke::new(4.0, pal().bg_accent))];
    let reached_to = nodes.iter().filter(|n| n.reached).map(|n| n.y).fold(first.y, f32::max);
    shapes.push(Shape::line_segment([pos2(x, first.y), pos2(x, reached_to)], Stroke::new(4.0, PINK)));
    for n in nodes {
        shapes.push(Shape::circle_filled(pos2(x, n.y), 8.0, if n.reached { PINK } else { pal().bg_accent }));
    }
    ui.painter().set(slot, Shape::Vec(shapes));
}

pub fn check(ui: &Ui, at: egui::Pos2) {
    icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(at, vec2(12.0, 12.0)), Color32::WHITE);
}

/// One locked level: its strip with the boosts it takes, the slots it adds, and Buy Level. Hands back the node's y and whether Buy Level was clicked.
pub fn level(ui: &mut Ui, lvl: u32, boosts: u32, adds: String, on: bool) -> (f32, bool) {
    let w = ui.available_width();
    let (head, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
    let body_h = if on { 96.0 } else { 160.0 };
    let (body, _) = ui.allocate_exact_size(vec2(w, body_h), Sense::hover());
    let card = Rect::from_min_max(head.min, body.max);
    let p = ui.painter();
    p.rect_filled(card, CornerRadius::same(8), pal().bg_primary);
    p.rect_filled(head, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, pal().bg_secondary_alt);
    p.rect_stroke(card, CornerRadius::same(8), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
    icons::draw(p, Icon::GemOutline, Rect::from_center_size(pos2(head.left() + 28.0, head.center().y), vec2(24.0, 24.0)), pal().header_secondary);
    p.text(pos2(head.left() + 56.0, head.center().y), Align2::LEFT_CENTER, format!("Level {lvl}"), semi(16.0), pal().header_primary);
    if on {
        icons::draw(p, Icon::CircleCheck, Rect::from_center_size(pos2(head.right() - 24.0, head.center().y), vec2(20.0, 20.0)), pal().positive);
    } else {
        icons::draw(p, Icon::Lock, Rect::from_center_size(pos2(head.right() - 24.0, head.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    }
    p.text(pos2(head.right() - 48.0, head.center().y), Align2::RIGHT_CENTER, format!("{boosts} Boosts"), reg(16.0), pal().text_normal);
    p.text(pos2(body.center().x, body.top() + if on { 48.0 } else { 74.0 }), Align2::CENTER_CENTER, adds, reg(16.0), pal().text_normal);
    let mut buy = false;
    if !on {
        let b = Rect::from_center_size(pos2(body.center().x, body.bottom() - 36.0), vec2(104.0, 36.0));
        let resp = ui.interact(b, ui.id().with(("stickline_buy", lvl)), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(4), crate::ui::motion::mix(Color32::from_rgb(0x24, 0x80, 0x46), Color32::from_rgb(0x1a, 0x6b, 0x38), crate::ui::motion::hot(ui.ctx(), &resp)));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Buy Level", medium(16.0), Color32::WHITE);
        super::super::settings::mark(ui, &format!("stickers:buy:{lvl}"), b);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        buy = resp.clicked();
    }
    ui.add_space(8.0);
    (head.center().y, buy)
}
