//! Nitro Home and the Shop, the two pages under Friends in the Home sidebar, drawn from the calls
//! Discord's own pages make and sized by its CSS rules (container widths, grids that fill and
//! wrap, min and max sizes), never by a fixed window size. Browsing never leaves the client; only
//! a purchase opens the checkout window.

mod browse;
mod catalog;
mod cursor;
mod nitro;
pub mod product;
pub mod wish;

use egui::load::TexturePoll;
use egui::{Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use serde_json::Value;

use super::App;
use super::icons::{self, Icon};
use super::settings::{mark, refetch, remote};
use crate::theme::*;

pub(super) const SPACE_8: f32 = 8.0;
pub(super) const SPACE_12: f32 = 12.0;
pub(super) const SPACE_16: f32 = 16.0;
pub(super) const SPACE_20: f32 = 20.0;
pub(super) const SPACE_24: f32 = 24.0;
pub(super) const SPACE_32: f32 = 32.0;
pub(super) const RADIUS_SM: u8 = 8;
pub(super) const RADIUS_LG: u8 = 16;
pub(super) const RADIUS_XXL: u8 = 32;
pub(super) const CARD_MIN: f32 = 246.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Nitro,
    Shop,
}

pub struct Shop {
    pub page: Page,
    // 0 Featured, 1 Browse, 2 Orbs Exclusives
    pub tab: u8,
    pub search: String,
    pub product: Option<String>,
    pub slide: usize,
    pub browse: browse::Filters,
    pub collection: Option<String>,
    // the Nitro page's favourites tabs
    pub perks: u8,
    pub wish: Vec<String>,
    pub wish_id: Option<String>,
    pub wish_load: u8,
    pub feed_sort: usize,
    pub feed_shown: usize,
    pub shuffle: u32,
}

const KEYS: [&str; 8] = ["shop_home", "shop_cats", "shop_owned", "shop_orbs", "shop_orbs_home", "nitro_subs", "nitro_plans", "nitro_basic"];

pub(super) const NIGHT: Color32 = Color32::from_rgb(0x0b, 0x0a, 0x12);
pub(super) const PLATE: Color32 = Color32::from_rgb(0x12, 0x12, 0x16);

impl App {
    pub fn open_shop(&mut self, page: Page) {
        for k in KEYS {
            refetch(self, k);
        }
        self.keyed.retain(|k, _| !k.starts_with("/shop/search"));
        self.shop = Some(Shop {
            page,
            tab: 0,
            search: String::new(),
            product: None,
            slide: 0,
            browse: browse::Filters::new(),
            collection: None,
            perks: 0,
            wish: Vec::new(),
            wish_id: None,
            wish_load: 0,
            feed_sort: 0,
            feed_shown: 12,
            shuffle: 0,
        });
        wish::reload(self);
        self.ui.guild = None;
        self.directory = None;
        self.friends = false;
        self.quests = None;
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    match app.shop.as_ref().map(|s| s.page) {
        Some(Page::Nitro) => nitro::show(app, ui),
        Some(Page::Shop) => {
            let area = ui.max_rect();
            catalog::show(app, ui);
            cursor::shop(ui, area, app.settings.shop_cursor);
        }
        None => {}
    }
}

pub(super) fn want(app: &mut App, key: &'static str, path: &str) {
    let _ = remote(app, key, path);
}

pub(super) fn get<'a>(app: &'a App, key: &str) -> Option<&'a Value> {
    match app.remote.get(key)? {
        Some(Ok(v)) => Some(v),
        _ => None,
    }
}

pub(super) fn list<'a>(v: Option<&'a Value>) -> &'a [Value] {
    v.and_then(|v| v.as_array()).map_or(&[], |a| a.as_slice())
}

/// `repeat(auto-fill, minmax(min, 1fr))` with `gap` between: how many columns fit.
pub(super) fn columns(width: f32, min: f32, gap: f32) -> usize {
    (((width + gap) / (min + gap)).floor() as usize).max(1)
}

pub(super) fn cell(width: f32, cols: usize, gap: f32) -> f32 {
    (width - (cols - 1) as f32 * gap) / cols as f32
}

pub(super) fn pic(ui: &Ui, url: &str, r: Rect, radius: u8, cover: bool) {
    let src = crate::gif::uri(ui.ctx(), url, ui.is_rect_visible(r));
    let Ok(TexturePoll::Ready { texture }) = egui::Image::new(src.clone()).load_for_size(ui.ctx(), r.size()) else { return };
    let s = texture.size;
    if s.x < 1.0 || s.y < 1.0 {
        return;
    }
    let (draw, uv) = if cover {
        let k = (r.width() / s.x).max(r.height() / s.y);
        (r, Rect::from_center_size(pos2(0.5, 0.5), vec2(r.width() / (s.x * k), r.height() / (s.y * k))))
    } else {
        let k = (r.width() / s.x).min(r.height() / s.y);
        (Rect::from_center_size(r.center(), s * k), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)))
    };
    egui::Image::new(src).uv(uv).corner_radius(CornerRadius::same(radius)).show_loading_spinner(false).paint_at(ui, draw);
}

pub(super) fn button(ui: &mut Ui, r: Rect, label: &str, fill: Color32, ink: Color32, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("shop_button", key)), Sense::click());
    let fill = if resp.hovered() { fill.lerp_to_gamma(Color32::BLACK, 0.12) } else { fill };
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SM), fill);
    crate::ui::text::paint(ui.painter(), r.center(), egui::Align2::CENTER_CENTER, label, medium(16.0), ink, r.width() - SPACE_16);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("shop:{key}"), r);
    resp.clicked()
}

pub(super) fn shade(ui: &Ui, r: Rect, base: Color32, from: u8, to: u8) {
    let mut m = egui::Mesh::default();
    let (a, b) = (Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), from), Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), to));
    for (p, c) in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].into_iter().zip([a, a, b, b]) {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(m));
}

pub(super) fn text_width(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x
}

pub(super) fn round_button(ui: &mut Ui, r: Rect, icon: Icon, tint: Color32, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("shop_round", key)), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_LG), Color32::from_black_alpha(if resp.hovered() { 170 } else { 143 }));
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), tint);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("shop:{key}"), r);
    resp.clicked()
}
