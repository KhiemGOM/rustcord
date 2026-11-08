//! The in-app icon: Discord's 23 icons (the pictures come from discord.com/assets at runtime), the
//! grid in Appearance, the preview panel, and putting the chosen one on the window and taskbar.

use egui::load::BytesPoll;
use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{BODY, Look, child, close, header, unlocked, wide};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;

pub const ICONS: [(&str, &str, &str); 23] = [
    ("AppIcon", "Blurple (Default)", "5926a37cee0904ce"),
    ("BrandDarkIcon", "Goth", "e8396e79419904ca"),
    ("MangaIcon", "WHAM", "2d62663c4b493f8e"),
    ("MatteDarkIcon", "Charcoal", "28f89b5f78dd6f80"),
    ("MatteLightIcon", "Ceramic", "96f9a2942150ba17"),
    ("PastelIcon", "Pastel", "22bb7108ad1c466f"),
    ("PirateIcon", "Matey", "9af4b996c897420f"),
    ("CamoIcon", "Tactical", "9ad8ec824f71bd05"),
    ("SunsetIcon", "Sunset Ave", "5bf3279039efa66c"),
    ("GalaxyIcon", "Galactic Chrome", "3a2d57c0bde9b218"),
    ("Y2KIcon", "Holo", "181e40b5ea872cdb"),
    ("CherryBlossomIcon", "Sakura", "1e208c512641758d"),
    ("BeanieIcon", "Sherbet Dreamsicle", "e14578cc054054c0"),
    ("GamingIcon", "Gaming", "5e5cc9b058e63987"),
    ("CircuitIcon", "Mainframe", "82c7a89d15622af4"),
    ("HoloWavesIcon", "Prismatic Waves", "441d164362b711b1"),
    ("BlushIcon", "uwu~", "e5f2a55421a6dc7e"),
    ("AngryIcon", "Fuming", "9b50f7ba00ee941f"),
    ("ControllerIcon", "D64", "a629a2a3fe1ccbcc"),
    ("BlurpleTwilightIcon", "Blurple Twilight", "685b2900a9423534"),
    ("InRainbowsIcon", "In Rainbows", "10cd104567438715"),
    ("MidnightPrismIcon", "Midnight Prism", "7ebbc1790864207a"),
    ("ColorWaveIcon", "Color Wave", "7ce5a942a5cc7cba"),
];

static DONE: std::sync::Mutex<usize> = std::sync::Mutex::new(usize::MAX);

/// The icon the window was last given.
pub fn applied() -> usize {
    *DONE.lock().unwrap()
}

pub fn url(i: usize) -> String {
    format!("{}/assets/{}.webp", crate::api::asset(), ICONS[i].2)
}

pub fn index(id: &str) -> usize {
    ICONS.iter().position(|i| i.0 == id).unwrap_or(0)
}

fn draw(ui: &Ui, r: Rect, i: usize) {
    let u = crate::gif::uri(ui.ctx(), &url(i), true);
    egui::Image::from_uri(u).corner_radius(CornerRadius::same((r.width() / 6.0) as u8)).show_loading_spinner(false).paint_at(ui, r);
}

fn ring(ui: &mut Ui, r: Rect, on: bool, id: egui::Id) -> egui::Response {
    let resp = ui.interact(r, id, Sense::click());
    if on {
        ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(8), Stroke::new(2.0, pal().blurple), egui::StrokeKind::Outside);
        let tick = pos2(r.right() - 2.0, r.top() + 2.0);
        ui.painter().circle_filled(tick, 8.0, pal().blurple);
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(tick, vec2(10.0, 10.0)), Color32::WHITE);
    } else if resp.hovered() {
        ui.painter().rect_stroke(r.expand(1.0), CornerRadius::same(8), Stroke::new(1.0, pal().bg_accent), egui::StrokeKind::Outside);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The icon grid; the index that was clicked.
pub fn grid(ui: &mut Ui, size: f32, chosen: usize, live: bool) -> Option<usize> {
    let per_row = ((ui.available_width() + 8.0) / (size + 8.0)).floor().max(1.0) as usize;
    let rows = ICONS.len().div_ceil(per_row);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * (size + 8.0) - 8.0), Sense::hover());
    let mut clicked = None;
    for i in 0..ICONS.len() {
        let r = Rect::from_min_size(area.min + vec2((i % per_row) as f32 * (size + 8.0), (i / per_row) as f32 * (size + 8.0)), vec2(size, size));
        draw(ui, r, i);
        let resp = ring(ui, r, i == chosen, egui::Id::new(("app_icon", i)));
        mark(ui, &format!("icon:{}", ICONS[i].1), r);
        if resp.on_hover_text(ICONS[i].1).clicked() && live {
            clicked = Some(i);
        }
    }
    clicked
}

pub fn choose(app: &mut App, ctx: &egui::Context, i: usize) {
    app.settings.app_icon = ICONS[i].0.to_string();
    app.save_settings(ctx);
}

/// Hands the window and taskbar the chosen icon once its picture has arrived.
pub fn put(ctx: &egui::Context, i: usize) {
    if *DONE.lock().unwrap() == i {
        return;
    }
    let Ok(BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(&url(i)) else {
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
        return;
    };
    *DONE.lock().unwrap() = i;
    if let Ok(img) = image::load_from_memory(&bytes) {
        let img = img.resize(256, 256, image::imageops::FilterType::Lanczos3).into_rgba8();
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }))));
    }
}

pub fn panel(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let unlocked = unlocked(app);
    let head = Rect::from_min_size(r.min, vec2(BODY, 32.0));
    header(app, ui, head, "App Icon", "icons", |app| close(app, &ctx, false));
    let footer_h = if unlocked { 40.0 } else { 92.0 };
    let footer = Rect::from_min_size(pos2(r.left(), r.bottom() - footer_h), vec2(BODY, footer_h));
    let body = Rect::from_min_max(pos2(r.left(), head.bottom() + 16.0), pos2(r.right(), footer.top() - 16.0));
    let saved = index(&app.settings.app_icon);
    let shown = app.themes.icon.unwrap_or(saved);
    let mut pick = None;
    let mut scroll = child(ui, body);
    egui::ScrollArea::vertical().id_salt("themes_icons").auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (big, _) = ui.allocate_exact_size(vec2(BODY, 160.0), Sense::hover());
        ui.painter().rect_filled(big, CornerRadius::same(8), pal().bg_tertiary);
        draw(ui, Rect::from_center_size(big.center() - vec2(0.0, 10.0), vec2(96.0, 96.0)), shown);
        ui.painter().text(pos2(big.center().x, big.bottom() - 18.0), Align2::CENTER_CENTER, ICONS[shown].1, medium(14.0), pal().header_primary);
        ui.add_space(16.0);
        pick = grid(ui, 48.0, shown, true);
    });
    let mut foot = child(ui, footer);
    foot.spacing_mut().item_spacing = vec2(0.0, 12.0);
    let mut nitro = false;
    if !unlocked {
        nitro = wide(&mut foot, "Unlock with Nitro", Look::Premium, true).clicked();
    }
    let leave = wide(&mut foot, if unlocked { "Back to Settings" } else { "Exit Preview" }, Look::Secondary, true).clicked();
    if let Some(i) = pick {
        if unlocked {
            choose(app, &ctx, i);
            app.themes.icon = None;
        } else {
            app.themes.icon = Some(i);
        }
    }
    if nitro {
        close(app, &ctx, false);
        app.settings_page = Some(crate::ui::settings::Page::Nitro);
    } else if leave {
        close(app, &ctx, true);
    }
}
