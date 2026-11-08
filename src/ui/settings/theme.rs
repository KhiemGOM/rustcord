//! Appearance's Theme and App Icon sections: the default themes, the make-your-own banner, the
//! colour themes, where servers' themes apply, and the in-app icon.

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Ui, pos2, vec2};

use super::app::{choice, related, tile};
use super::{Page, mark, note, section_with, sub, toggle};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::themes::{self, Back, icon};

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let mut changed = false;
    if section_with(ui, "Theme", Some("Preview Theme")) {
        themes::open_themes(app, &ctx, None);
    }
    let mut sync = app.settings.theme == Theme::Sync;
    if toggle(ui, &mut sync, "Same as Device Theme", "Match your device's light or dark mode.") {
        app.settings.theme = if sync { Theme::Sync } else { Theme::Dark };
        themes::apply(app, &ctx, None);
        changed = true;
    }
    super::end(ui, 32.0);
    sub(ui, "Default Themes");
    let defaults = [
        (Theme::Light, "Light", Color32::WHITE),
        (Theme::Ash, "Ash", Color32::from_rgb(50, 51, 57)),
        (Theme::Dark, "Dark", Color32::from_rgb(29, 29, 33)),
        (Theme::Onyx, "Onyx", Color32::BLACK),
        (Theme::Sync, "Sync with computer", Color32::WHITE),
    ];
    super::lead(ui);
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    for (i, (t, name, fill)) in defaults.into_iter().enumerate() {
        let r = Rect::from_min_size(strip.min + vec2(56.0 * i as f32, 0.0), vec2(48.0, 48.0));
        let on = app.settings.theme == t && app.settings.custom_theme.is_none() && app.settings.own_theme.is_none();
        ui.painter().rect_filled(r, CornerRadius::same(8), fill);
        if t == Theme::Sync {
            let half = Rect::from_min_max(pos2(r.center().x, r.top()), r.max);
            ui.painter().rect_filled(half, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, Color32::from_rgb(29, 29, 33));
        }
        if tile(app, ui, name, r, on) && !on {
            app.settings.theme = t;
            themes::apply(app, &ctx, None);
            changed = true;
        }
    }

    if app.settings.theme != Theme::Sync {
        ui.add_space(32.0);
        if banner(ui) {
            themes::open_editor(app, &ctx, Back::Settings);
        }
        ui.add_space(34.0);
    } else {
        ui.add_space(40.0);
    }
    super::lead(ui);
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let title = ui.painter().text(head.left_center(), Align2::LEFT_CENTER, "Color Themes", semi(16.0), pal().text_normal);
    icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(title.right() + 12.0, head.center().y), vec2(16.0, 16.0)), pal().header_primary);
    ui.add_space(4.0);
    ui.add(egui::Label::new(RichText::new("Make Discord yours. Only with Nitro.").font(reg(14.0)).color(pal().header_secondary)).selectable(false));
    ui.add_space(20.0);
    let per_row = ((ui.available_width() + 8.0) / 56.0).floor().max(1.0) as usize;
    let rows = (PRESETS.len() + 1).div_ceil(per_row);
    let (grid, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * 56.0 - 8.0), Sense::hover());
    let at = |i: usize| Rect::from_min_size(grid.min + vec2((i % per_row) as f32 * 56.0, (i / per_row) as f32 * 56.0), vec2(48.0, 48.0));
    let own = app.settings.own_theme.clone();
    let r = at(0);
    match &own {
        Some(c) => ui.painter().add(egui::Shape::mesh(rounded_gradient(Fill::Custom(c.clone()).resolve(), r, 8.0))),
        None => ui.painter().add(egui::Shape::from(egui::epaint::RectShape::filled(r, CornerRadius::same(8), Color32::from_rgb(0, 78, 65)))),
    };
    icons::draw(ui.painter(), Icon::SideAppearance, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), Color32::WHITE);
    if tile(app, ui, "Custom Theme", r, own.is_some()) {
        themes::open_editor(app, &ctx, Back::Settings);
    }
    for (i, preset) in PRESETS.iter().enumerate() {
        let r = at(i + 1);
        ui.painter().add(egui::Shape::mesh(rounded_gradient(preset, r, 8.0)));
        let on = app.settings.own_theme.is_none() && app.settings.custom_theme == Some(i as u8);
        if tile(app, ui, preset.name, r, on) && !on {
            if themes::unlocked(app) {
                themes::apply(app, &ctx, Some(Fill::Preset(i as u8)));
                changed = true;
            } else {
                themes::open_themes(app, &ctx, Some(Fill::Preset(i as u8)));
            }
        }
    }
    super::end(ui, 32.0);
    if toggle(ui, &mut app.settings.sync_theme, "Sync theme across my devices", "") {
        if app.settings.sync_theme {
            themes::push(app);
        }
        changed = true;
    }
    changed |= toggle(ui, &mut app.settings.own_theme_on_profiles, "Apply theme to other users' profiles", "");
    let shown = if app.settings.server_themes { "Use the server's theme" } else { "Use my theme" };
    let mut mine = !app.settings.server_themes;
    choice(ui, "Default theme in servers", shown, |ui| {
        for (v, name) in [(false, "Use the server's theme"), (true, "Use my theme")] {
            let row = ui.selectable_label(mine == v, name);
            mark(ui, &format!("option:{name}"), row.rect);
            if row.clicked() {
                mine = v;
            }
        }
    });
    if mine == app.settings.server_themes {
        app.settings.server_themes = !mine;
        themes::push(app);
        changed = true;
    }
    related(app, ui, Page::Accessibility, "Accessibility", "Change saturation of colors, enable high-contrast mode, and more");

    let ctx = ui.ctx().clone();
    if section_with(ui, "App Icon", Some("Preview Icon")) {
        app.settings_page = None;
        app.themes.panel = Some(themes::Panel::Icons);
        app.themes.icon = None;
    }
    note(ui, "Change the Discord in-app icon, only with Nitro.");
    super::lead(ui);
    let live = themes::unlocked(app);
    if let Some(i) = icon::grid(ui, 48.0, icon::index(&app.settings.app_icon), live) {
        icon::choose(app, &ctx, i);
    }
    ui.add_space(8.0);
    changed
}

/// "Make your Discord look *juuusst* right": the card that opens the custom theme editor.
fn banner(ui: &mut Ui) -> bool {
    super::lead(ui);
    let w = ui.available_width();
    let button = ui.painter().layout_no_wrap("Try it out".into(), medium(16.0), Color32::WHITE).size().x.max(68.0) + 32.0;
    let texts = w - 50.0 - 117.0 - 16.0 - 16.0 - button - 16.0;
    let title = ui.painter().layout("Make your Discord look *juuusst* right".into(), semi(20.0), pal().header_primary, texts);
    let body = ui.painter().layout("Create your own custom theme to make Discord uniquely yours. Only with Nitro.".into(), reg(14.0), pal().text_normal, texts);
    let h = 69.0f32.max(title.size().y + 4.0 + body.size().y);
    let (r, _) = ui.allocate_exact_size(vec2(w, 32.0 + h), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_tertiary);
    themes::promo(ui, Rect::from_min_size(pos2(r.left() + 25.0, r.center().y - 34.5), vec2(117.0, 69.0)));
    let x = r.left() + 25.0 + 117.0 + 16.0;
    let top = r.center().y - (title.size().y + 4.0 + body.size().y) / 2.0;
    let th = title.size().y;
    ui.painter().galley(pos2(x, top), title, pal().header_primary);
    ui.painter().galley(pos2(x, top + th + 4.0), body, pal().text_normal);
    let b = Rect::from_min_size(pos2(r.right() - 25.0 - button, r.center().y - 20.0), vec2(button, 40.0));
    let resp = ui.interact(b, egui::Id::new("theme_try_it_out"), Sense::click());
    let fill = crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(b, CornerRadius::same(8), fill, egui::Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Try it out", medium(16.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, "button:Try it out", b);
    resp.clicked()
}
