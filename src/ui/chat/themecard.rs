use super::*;

/// A custom theme someone shared: its colours, who made it, and Preview to try it on.
pub(super) fn theme_card(ui: &mut Ui, d: &mut D, m: &Message) {
    let t = &m.extra["shared_client_theme"];
    let Some(colors) = t["colors"].as_array() else { return };
    let colors: Vec<u32> = colors.iter().take(5).filter_map(|c| u32::from_str_radix(c.as_str()?.trim_start_matches('#'), 16).ok()).collect();
    if colors.is_empty() {
        return;
    }
    let custom = crate::theme::Custom { colors, angle: t["gradient_angle"].as_i64().unwrap_or(0) as i32, mix: t["base_mix"].as_i64().unwrap_or(0) as i32, light: t["base_theme"].as_u64() == Some(2) };
    let (r, _) = ui.allocate_exact_size(vec2(320.0_f32.min(ui.available_width()), 124.0), Sense::hover());
    ui.painter().rect(r, 8, pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let swatch = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(48.0, 48.0));
    ui.painter().add(crate::theme::rounded_custom_gradient(&custom, swatch, 8.0));
    let x = swatch.right() + 12.0;
    let title = ui.painter().text(pos2(x, swatch.top() + 13.0), Align2::LEFT_CENTER, "Custom Theme", semi(16.0), pal().header_primary);
    icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(title.right() + 14.0, title.center().y), vec2(16.0, 16.0)), pal().header_secondary);
    let by = ui.painter().text(pos2(x, swatch.top() + 36.0), Align2::LEFT_CENTER, "Created by", medium(14.0), pal().header_secondary);
    let face = Rect::from_center_size(pos2(by.right() + 16.0, by.center().y), vec2(20.0, 20.0));
    if let Some(u) = d.store.user(m.author) {
        avatar(ui, face, &u.avatar_url());
    }
    text::paint(ui.painter(), pos2(face.right() + 4.0, by.center().y), Align2::LEFT_CENTER, &d.store.name_in(d.guild, m.author), medium(14.0), pal().header_secondary, r.right() - face.right() - 16.0);
    let b = Rect::from_min_max(pos2(r.left() + 12.0, r.bottom() - 52.0), pos2(r.right() - 12.0, r.bottom() - 12.0));
    let resp = ui.interact(b, ui.id().with(("theme_preview", m.id)), Sense::click());
    ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Preview", medium(16.0), Color32::WHITE);
    d.rects.push((format!("theme_card:{}", m.id), b));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        d.acts.push(Act::TryTheme(custom));
    }
}
