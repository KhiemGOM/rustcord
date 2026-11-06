//! Nitro Home and the Shop, the two pages under Friends in the Home sidebar, drawn from the calls
//! Discord's own pages make. Browsing never leaves the client; only a purchase opens the checkout
//! window.

mod catalog;
mod nitro;
mod product;

use egui::load::TexturePoll;
use egui::{Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use serde_json::Value;

use super::App;
use super::icons::{self, Icon};
use super::settings::{mark, refetch, remote};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Nitro,
    Shop,
}

pub struct Shop {
    pub page: Page,
    // 0 Featured, 1 Browse, 2 Orbs Exclusives, 3 Wishlist
    pub tab: u8,
    pub search: String,
    pub product: Option<String>,
    // Browse's Show only: avatar decorations, nameplates, profile effects, bundles, Orbs eligible
    pub only: [bool; 5],
    pub collection: Option<String>,
    // the Nitro page's favourites tabs
    pub perks: u8,
    pub wish: Vec<String>,
    pub bought: Option<String>,
}

const KEYS: [&str; 7] = ["shop_home", "shop_cats", "shop_owned", "shop_orbs", "nitro_subs", "nitro_plans", "nitro_basic"];

pub(super) const NIGHT: Color32 = Color32::from_rgb(0x0b, 0x0a, 0x12);
pub(super) const PLATE: Color32 = Color32::from_rgb(0x12, 0x12, 0x16);

impl App {
    pub fn open_shop(&mut self, page: Page) {
        for k in KEYS {
            refetch(self, k);
        }
        self.shop = Some(Shop { page, tab: 0, search: String::new(), product: None, only: [false; 5], collection: None, perks: 0, wish: Vec::new(), bought: None });
        self.ui.guild = None;
        self.directory = None;
        self.friends = false;
        self.quests = None;
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    match app.shop.as_ref().map(|s| s.page) {
        Some(Page::Nitro) => nitro::show(app, ui),
        Some(Page::Shop) => catalog::show(app, ui),
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
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    crate::ui::text::paint(ui.painter(), r.center(), egui::Align2::CENTER_CENTER, label, medium(16.0), ink, r.width() - 16.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("shop:{key}"), r);
    resp.clicked()
}

pub(super) fn round_button(ui: &mut Ui, r: Rect, icon: Icon, tint: Color32, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("shop_round", key)), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(16), Color32::from_black_alpha(if resp.hovered() { 170 } else { 143 }));
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), tint);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("shop:{key}"), r);
    resp.clicked()
}
