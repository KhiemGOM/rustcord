//! Discord's notice strip: one at a time, right of the rail over the sidebar and chat, with its
//! text, an outlined button and a close button.

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use crate::theme::*;

pub enum Hit {
    Code,
    Button,
    Close,
}

/// `ink` is the notice's text, outline and close colour; `hover` the button's text over its hover
/// fill, discord's --custom-notice-button-hover. `code` is an "Error: …" after the text.
pub fn strip(ui: &mut Ui, fill: Color32, ink: Color32, text: &str, code: Option<&str>, button: &str, hover: Color32) -> Option<Hit> {
    let mut hit = None;
    egui::Panel::top("notice").exact_size(36.0).resizable(false).show_separator_line(false).frame(egui::Frame::NONE).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let corner = CornerRadius { nw: 8, ..CornerRadius::ZERO };
        p.rect_filled(r, corner, fill);
        p.with_clip_rect(r).rect_stroke(Rect::from_min_max(r.min, r.max + vec2(4.0, 4.0)), corner, Stroke::new(1.0, pal().frame_border), egui::StrokeKind::Inside);
        let msg = p.layout_no_wrap(text.into(), medium(14.0), ink);
        let err = code.map(|c| p.layout_no_wrap(c.into(), semi(14.0), ink));
        let gap = err.as_ref().map_or(0.0, |e| 4.0 + e.size().x);
        let label = p.layout_no_wrap(button.into(), medium(14.0), ink);
        let bw = label.size().x + 20.0;
        let x = (r.left() + 4.0 + r.right() - 28.0) / 2.0 - (msg.size().x + gap + 10.0 + bw) / 2.0;
        let br = Rect::from_min_size(pos2(x + msg.size().x + gap + 10.0, r.top() + 6.0), vec2(bw, 24.0));
        let er = Rect::from_min_size(pos2(x + msg.size().x + 4.0, r.top()), vec2(gap - 4.0, 36.0));
        p.galley(pos2(x, r.center().y - msg.size().y / 2.0), msg, ink);
        let e = err.map(|g| {
            p.galley(pos2(er.left(), r.center().y - g.size().y / 2.0), g, ink);
            ui.interact(er, egui::Id::new("notice_code"), Sense::click())
        });
        let b = ui.interact(br, egui::Id::new("notice_button"), Sense::click());
        let hot = crate::ui::motion::hot(ui.ctx(), &b);
        p.rect(br, 3, ink.gamma_multiply(hot), Stroke::new(1.0, ink), egui::StrokeKind::Inside);
        p.galley_with_override_text_color(pos2(br.left() + 10.0, br.center().y - label.size().y / 2.0), label, crate::ui::motion::mix(ink, hover, hot));
        let cr = Rect::from_min_size(pos2(r.right() - 36.0, r.top()), vec2(36.0, 36.0));
        let c = ui.interact(cr, egui::Id::new("notice_close"), Sense::click());
        icons::draw(&p, Icon::Close, Rect::from_center_size(cr.center(), vec2(18.0, 18.0)), ink);
        if b.hovered() || c.hovered() || e.as_ref().is_some_and(|e| e.hovered()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        crate::ui::settings::mark(ui, "notice:bar", r);
        crate::ui::settings::mark(ui, "notice:button", br);
        crate::ui::settings::mark(ui, "notice:close", cr);
        crate::ui::settings::said(ui.ctx(), "notice:text", text);
        hit = if e.is_some_and(|e| e.clicked()) {
            Some(Hit::Code)
        } else if b.clicked() {
            Some(Hit::Button)
        } else if c.clicked() {
            Some(Hit::Close)
        } else {
            None
        };
    });
    hit
}
