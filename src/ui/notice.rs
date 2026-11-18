//! Discord's notice strip: one at a time, right of the rail over the sidebar and chat, with its
//! text, an outlined button and a close button.

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use crate::theme::*;

pub enum Hit {
    Button,
    Close,
}

/// `hover` is the button's text over its white hover fill, discord's --custom-notice-button-hover.
pub fn strip(ui: &mut Ui, fill: Color32, text: &str, button: &str, hover: Color32) -> Option<Hit> {
    let mut hit = None;
    egui::Panel::top("notice").exact_size(36.0).resizable(false).show_separator_line(false).frame(egui::Frame::NONE).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let corner = CornerRadius { nw: 8, ..CornerRadius::ZERO };
        p.rect_filled(r, corner, fill);
        p.with_clip_rect(r).rect_stroke(Rect::from_min_max(r.min, r.max + vec2(4.0, 4.0)), corner, Stroke::new(1.0, pal().frame_border), egui::StrokeKind::Inside);
        let msg = p.layout_no_wrap(text.into(), medium(14.0), Color32::WHITE);
        let label = p.layout_no_wrap(button.into(), medium(14.0), Color32::WHITE);
        let bw = label.size().x + 20.0;
        let x = (r.left() + 4.0 + r.right() - 28.0) / 2.0 - (msg.size().x + 10.0 + bw) / 2.0;
        let br = Rect::from_min_size(pos2(x + msg.size().x + 10.0, r.top() + 6.0), vec2(bw, 24.0));
        p.galley(pos2(x, r.center().y - msg.size().y / 2.0), msg, Color32::WHITE);
        let b = ui.interact(br, egui::Id::new("notice_button"), Sense::click());
        let hot = crate::ui::motion::hot(ui.ctx(), &b);
        p.rect(br, 3, Color32::WHITE.gamma_multiply(hot), Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
        p.galley_with_override_text_color(pos2(br.left() + 10.0, br.center().y - label.size().y / 2.0), label, crate::ui::motion::mix(Color32::WHITE, hover, hot));
        let cr = Rect::from_min_size(pos2(r.right() - 36.0, r.top()), vec2(36.0, 36.0));
        let c = ui.interact(cr, egui::Id::new("notice_close"), Sense::click());
        icons::draw(&p, Icon::Close, Rect::from_center_size(cr.center(), vec2(18.0, 18.0)), Color32::WHITE);
        if b.hovered() || c.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        crate::ui::settings::mark(ui, "notice:bar", r);
        crate::ui::settings::mark(ui, "notice:button", br);
        crate::ui::settings::mark(ui, "notice:close", cr);
        crate::ui::settings::said(ui.ctx(), "notice:text", text);
        hit = if b.clicked() {
            Some(Hit::Button)
        } else if c.clicked() {
            Some(Hit::Close)
        } else {
            None
        };
    });
    hit
}
