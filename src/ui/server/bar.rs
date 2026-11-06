use egui::{Align, Align2, CornerRadius, Layout, RichText, Sense, Ui, pos2, vec2};

use crate::theme::*;

/// The floating "unsaved changes" bar over the bottom of a page: (reset, save).
pub fn unsaved(ui: &mut Ui, saving: bool) -> (bool, bool) {
    let v = ui.clip_rect();
    let (left, width) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(egui::Id::new("notice"))).unwrap_or((v.left(), v.width()));
    let r = egui::Rect::from_min_size(pos2(left, v.bottom() - crate::ui::sidebar_view::NOTICE_PAD - 52.0), vec2(width, 52.0));
    ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
    ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful \u{2014} you have unsaved changes!", semi(16.0), pal().header_primary);
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 7.0))).layout(Layout::right_to_left(Align::Center)));
    let save = crate::ui::button(&mut b, if saving { "\u{2026}" } else { "Save Changes" }, GREEN, 120.0);
    b.add_space(12.0);
    let reset = b.add(egui::Label::new(RichText::new("Reset").font(semi(14.0)).color(pal().header_primary)).sense(Sense::click())).clicked();
    (reset, save.clicked() && !saving)
}
