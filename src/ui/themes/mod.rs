//! Colour themes outside the settings window: the right-hand panel Discord opens to preview a
//! theme (or app icon) and the custom theme editor, what the window shows while one is open, and
//! saving the choice to the account.

mod editor;
pub mod icon;

use base64::Engine;
use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::json;

use super::settings::Page;
use super::{App, SAVE_FAILED, icons::{self, Icon}, motion};
use crate::net::Tag;
use crate::theme::*;

pub const WIDTH: f32 = 290.0;
const LEFT: f32 = 34.0;
const BODY: f32 = 228.0;
pub const PROMO: &str = "assets/content/568728735ebc2ee3eb3a7ab29d37c1f698f413d29cee50f9a19ed2d2eefea27f.svg";

#[derive(Clone, Copy, PartialEq)]
pub enum Panel {
    Themes,
    Editor,
    Icons,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Back {
    #[default]
    Settings,
    Themes,
}

#[derive(Default)]
pub struct State {
    pub panel: Option<Panel>,
    pub preview: Option<Fill>,
    pub back: Back,
    pub icon: Option<usize>,
    edit: editor::Draft,
}

impl State {
    /// The theme the editor is showing.
    pub fn draft(&self) -> &Custom {
        &self.edit.now
    }
}

/// The theme the account saved, from /users/@me/custom-themes.
fn saved(v: &serde_json::Value) -> Option<Custom> {
    let t = v.as_array()?.first()?;
    let colors: Vec<u32> = t["colors"].as_array()?.iter().filter_map(|c| u32::from_str_radix(c.as_str()?.trim_start_matches('#'), 16).ok()).collect();
    (!colors.is_empty()).then(|| Custom { colors, angle: t["gradient_angle"].as_i64().unwrap_or(0) as i32, mix: t["base_mix"].as_i64().unwrap_or(74) as i32, light: t["base_theme"].as_u64() == Some(2) })
}

pub fn unlocked(app: &App) -> bool {
    app.store.me_raw["premium_type"].as_u64() == Some(2)
}

fn mine(app: &App) -> Option<Fill> {
    let s = &app.settings;
    match (&s.own_theme, s.custom_theme) {
        (Some(c), _) => Some(Fill::Custom(c.clone())),
        (None, Some(i)) => Some(Fill::Preset(i)),
        _ => None,
    }
}

fn server(app: &App) -> Option<Fill> {
    if !app.settings.server_themes {
        return None;
    }
    let g = app.store.guilds.get(&app.ui.guild?)?;
    if !g.features.iter().any(|f| f == "GUILD_THEME") {
        return None;
    }
    match g.theme.clone()? {
        Fill::Custom(mut c) => {
            c.light = app.settings.theme == Theme::Light;
            Some(Fill::Custom(c))
        }
        preset => Some(preset),
    }
}

/// What sits behind the window now: the theme being previewed, else the open server's own theme,
/// else the one the user picked.
pub fn shown(app: &App) -> Option<Fill> {
    if app.themes.panel.is_some()
        && let Some(f) = &app.themes.preview
    {
        return Some(f.clone());
    }
    server(app).or_else(|| mine(app))
}

pub fn open_themes(app: &mut App, ctx: &egui::Context, preview: Option<Fill>) {
    app.settings_page = None;
    app.themes.panel = Some(Panel::Themes);
    app.themes.preview = preview;
    app.themes.back = Back::Settings;
    app.apply_look(ctx);
}

pub fn open_editor(app: &mut App, ctx: &egui::Context, back: Back) {
    app.settings_page = None;
    app.themes.edit = editor::Draft::open(app.settings.own_theme.clone(), app.settings.theme == Theme::Light);
    app.themes.panel = Some(Panel::Editor);
    app.themes.preview = Some(Fill::Custom(app.themes.edit.now.clone()));
    app.themes.back = back;
    app.apply_look(ctx);
}

pub fn close(app: &mut App, ctx: &egui::Context, settings: bool) {
    app.themes.panel = None;
    app.themes.preview = None;
    app.themes.icon = None;
    if settings {
        app.settings_page = Some(Page::Appearance);
    }
    app.apply_look(ctx);
}

pub fn apply(app: &mut App, ctx: &egui::Context, fill: Option<Fill>) {
    match fill {
        Some(Fill::Preset(i)) => (app.settings.custom_theme, app.settings.own_theme) = (Some(i), None),
        Some(Fill::Custom(c)) => (app.settings.custom_theme, app.settings.own_theme) = (None, Some(c)),
        None => (app.settings.custom_theme, app.settings.own_theme) = (None, None),
    }
    app.save_settings(ctx);
    push(app);
}

/// Sends the theme to the account when this device's theme follows it.
pub fn push(app: &mut App) {
    if !app.settings.sync_theme {
        return;
    }
    let fill = mine(app);
    let body = crate::proto::appearance(&app.settings.appearance_raw, app.settings.theme, fill.as_ref(), app.settings.server_themes);
    let settings = base64::engine::general_purpose::STANDARD.encode(crate::proto::appearance_patch(&body));
    app.settings.appearance_raw = body;
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(json!({ "settings": settings })), Tag::fail(SAVE_FAILED));
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(panel) = app.themes.panel else { return };
    egui::Panel::right("client_theme")
        .exact_size(WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_secondary))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(6.0, r.height())), 0, pal().bg_tertiary);
            let inner = Rect::from_min_max(pos2(r.left() + LEFT, r.top() + 16.0), pos2(r.left() + LEFT + BODY, r.bottom() - 16.0));
            match panel {
                Panel::Themes => themes(app, ui, inner),
                Panel::Editor => editor::show(app, ui, inner),
                Panel::Icons => icon::panel(app, ui, inner),
            }
        });
}

pub(super) fn child(ui: &mut Ui, r: Rect) -> Ui {
    ui.new_child(UiBuilder::new().max_rect(r))
}

/// The panel's title row: a close button and the heading, with the close button at the right
/// end when the heading is the editor's smaller one.
pub(super) fn header(app: &mut App, ui: &mut Ui, r: Rect, title: &str, key: &str, done: impl FnOnce(&mut App)) {
    let end = key == "editor";
    let x = if end { Rect::from_min_size(pos2(r.right() - 24.0, r.center().y - 12.0), vec2(24.0, 24.0)) } else { Rect::from_min_size(pos2(r.left(), r.center().y - 12.0), vec2(24.0, 24.0)) };
    let resp = ui.interact(x, egui::Id::new(("themes_close", key)), Sense::click());
    let col = motion::mix(pal().header_secondary, pal().header_primary, motion::hot(ui.ctx(), &resp));
    icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    super::settings::mark(ui, &format!("themes:close:{key}"), x);
    if end {
        ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, title, bold(16.0), pal().header_primary);
    } else {
        ui.painter().text(pos2(x.right() + 8.0, r.center().y), Align2::LEFT_CENTER, title, bold(20.0), pal().header_primary);
    }
    if resp.clicked() {
        done(app);
    }
}

pub(super) enum Look {
    Primary,
    Secondary,
    Premium,
}

/// A full-width button of the panel's footer.
pub(super) fn wide(ui: &mut Ui, text: &str, look: Look, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let hot = if on { motion::hot(ui.ctx(), &resp) } else { 0.0 };
    match look {
        Look::Primary => {
            ui.painter().rect_filled(r, CornerRadius::same(8), motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), hot).gamma_multiply(if on { 1.0 } else { 0.5 }));
        }
        Look::Secondary => {
            let faint = |a: u8| Color32::from_rgba_unmultiplied(151, 151, 159, a);
            ui.painter().rect(r, CornerRadius::same(8), motion::mix(faint(31), faint(51), hot), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        }
        Look::Premium => {
            let mut mesh = egui::Mesh::default();
            let (a, b) = (Color32::from_rgb(0x85, 0x47, 0xc6), Color32::from_rgb(0xab, 0x5d, 0x8a));
            let (a, b) = (motion::mix(a, Color32::WHITE, hot * 0.1), motion::mix(b, Color32::WHITE, hot * 0.1));
            for (p, c) in [(r.left_top(), a), (r.right_top(), b), (r.right_bottom(), b), (r.left_bottom(), a)] {
                mesh.colored_vertex(p, c);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(egui::Shape::mesh(mesh));
        }
    }
    let ink = if matches!(look, Look::Secondary) { pal().header_primary } else { Color32::WHITE };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(16.0), ink);
    if resp.hovered() && on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    super::settings::mark(ui, &format!("themes:{text}"), r);
    resp
}

/// The swatch for a preset: its gradient, the ring, and a tick when chosen.
pub(super) fn swatch(ui: &mut Ui, r: Rect, preset: &Preset, on: bool, id: egui::Id) -> egui::Response {
    ui.painter().add(egui::Shape::mesh(rounded_gradient(preset, r, 8.0)));
    let resp = ui.interact(r, id, Sense::click());
    let faint = Color32::from_rgba_unmultiplied(151, 151, 159, 61);
    let ring = if on { pal().blurple } else { motion::mix(faint, pal().bg_accent, motion::hot(ui.ctx(), &resp)) };
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(if on { 2.0 } else { 1.0 }, ring), egui::StrokeKind::Inside);
    if on {
        let tick = pos2(r.right() - 3.0, r.top() + 3.0);
        ui.painter().circle_filled(tick, 8.0, pal().blurple);
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(tick, vec2(10.0, 10.0)), Color32::WHITE);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(preset.name)
}

/// The laptop-and-colour-wheel picture of the "make your own theme" cards, fetched from the CDN.
pub fn promo(ui: &Ui, r: Rect) {
    let url = format!("{}/{PROMO}", crate::api::cdn());
    egui::Image::from_uri(crate::gif::uri(ui.ctx(), &url, true)).show_loading_spinner(false).paint_at(ui, r);
}

fn banner(ui: &mut Ui) -> bool {
    let w = ui.available_width();
    let texts = [("Create your own theme", semi(16.0), pal().header_primary), ("Style your space with endless color combinations.", medium(14.0), pal().header_secondary)];
    let galleys: Vec<_> = texts.iter().map(|(t, f, c)| ui.painter().layout(t.to_string(), f.clone(), *c, w - 32.0 - 64.0 - 12.0)).collect();
    let text_h = galleys.iter().map(|g| g.size().y).sum::<f32>() + 4.0;
    let row = text_h.max(38.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 16.0 + row + 16.0 + 40.0 + 16.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_tertiary);
    promo(ui, Rect::from_min_size(r.min + vec2(16.0, 16.0), vec2(64.0, 38.0)));
    let mut y = r.top() + 16.0;
    for g in galleys {
        let h = g.size().y;
        ui.painter().galley(pos2(r.left() + 16.0 + 64.0 + 12.0, y), g, pal().header_primary);
        y += h + 4.0;
    }
    let mut c = child(ui, Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 16.0 + row + 16.0), vec2(w - 32.0, 40.0)));
    wide(&mut c, "Try It Out", Look::Secondary, true).clicked()
}

fn themes(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let unlocked = unlocked(app);
    let footer_h = if unlocked { 40.0 } else { 92.0 };
    let head = Rect::from_min_size(r.min, vec2(BODY, 32.0));
    header(app, ui, head, "Preview Theme", "themes", |app| close(app, &ctx, false));
    let footer = Rect::from_min_size(pos2(r.left(), r.bottom() - footer_h), vec2(BODY, footer_h));
    let body = Rect::from_min_max(pos2(r.left(), head.bottom() + 16.0), pos2(r.right(), footer.top() - 16.0));
    let mut go_editor = false;
    let mut pick = None;
    let at = app.themes.preview.clone().or_else(|| mine(app));
    let mut scroll = child(ui, body);
    egui::ScrollArea::vertical().id_salt("themes_body").auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        go_editor = banner(ui);
        ui.add_space(16.0);
        let (grid, _) = ui.allocate_exact_size(vec2(BODY, 8.0 * 84.0 - 24.0), Sense::hover());
        for (i, preset) in PRESETS.iter().enumerate() {
            let sw = Rect::from_min_size(grid.min + vec2((i % 3) as f32 * 84.0, (i / 3) as f32 * 84.0), vec2(60.0, 60.0));
            let on = at == Some(Fill::Preset(i as u8));
            if swatch(ui, sw, preset, on, egui::Id::new(("theme_panel_swatch", i))).clicked() {
                pick = Some(i as u8);
            }
            super::settings::mark(ui, &format!("themes:swatch:{}", preset.name), sw);
        }
    });
    let mut foot = child(ui, footer);
    foot.spacing_mut().item_spacing = vec2(0.0, 12.0);
    let mut leave = false;
    let mut nitro = false;
    if !unlocked {
        nitro = wide(&mut foot, "Unlock with Nitro", Look::Premium, true).clicked();
    }
    leave |= wide(&mut foot, if unlocked { "Back to Settings" } else { "Exit Preview" }, Look::Secondary, true).clicked();
    if let Some(i) = pick {
        if unlocked {
            apply(app, &ctx, Some(Fill::Preset(i)));
            app.themes.preview = None;
        } else {
            app.themes.preview = Some(Fill::Preset(i));
            app.apply_look(&ctx);
        }
    }
    if go_editor {
        open_editor(app, &ctx, Back::Themes);
    } else if nitro {
        close(app, &ctx, false);
        app.settings_page = Some(Page::Nitro);
    } else if leave {
        close(app, &ctx, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saved_theme_is_the_first_one() {
        let v = json!([{ "colors": ["#ff0066", "00ff99"], "gradient_angle": 90, "base_mix": 40, "base_theme": 2 }, { "colors": ["#000000"] }]);
        let c = saved(&v).unwrap();
        assert_eq!((c.colors, c.angle, c.mix, c.light), (vec![0xff0066, 0x00ff99], 90, 40, true));
        assert!(saved(&json!([])).is_none());
    }

    #[test]
    fn the_appearance_message_round_trips() {
        let c = Custom { colors: vec![0xff0066, 0x00ff99], angle: 90, mix: 40, light: false };
        let body = crate::proto::appearance(&[], Theme::Onyx, Some(&Fill::Custom(c.clone())), false);
        let blob = crate::proto::appearance_patch(&body);
        let a = crate::proto::account(&blob, true);
        assert_eq!(a.own_theme, Some(Some(c)));
        assert_eq!(a.theme, Some(Theme::Onyx));
        assert_eq!(a.server_themes, Some(false));
        let body = crate::proto::appearance(&body, Theme::Ash, Some(&Fill::Preset(19)), true);
        let a = crate::proto::account(&crate::proto::appearance_patch(&body), true);
        assert_eq!((a.custom_theme, a.own_theme, a.server_themes), (Some(Some(19)), Some(None), Some(true)));
    }
}
