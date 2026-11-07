//! "Suggested items for you" under the editor's wishlist: Discord asks `/wishlist/gift-recommendations`
//! for products and shows each with a heart that adds it.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use super::faint;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;
use crate::ui::{App, text};

const KEY: &str = "wish_suggested";
const COLS: usize = 4;
const GAP: f32 = 12.0;

fn ids(app: &App) -> Vec<String> {
    match app.keyed.get(KEY) {
        Some(Some(Ok(v))) => v["skus"].as_array().into_iter().flatten().filter_map(|s| s["id"].as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

pub fn path(me: Id) -> String {
    format!("/wishlist/gift-recommendations?user_ids={me}&max_recommendations=15&localize=true")
}

/// The card from `top` in `r`, as tall as it comes out (nothing while there are no suggestions).
pub fn show(app: &mut App, ui: &mut Ui, r: Rect, top: f32, wished: &[String]) -> f32 {
    if !app.keyed.contains_key(KEY) {
        app.keyed.insert(KEY.into(), None);
        let p = path(app.store.me);
        app.rest("GET", p, None, Tag::Keyed(KEY.into()));
    }
    let list: Vec<String> = ids(app).into_iter().filter(|s| !wished.contains(s)).collect();
    let items: Vec<_> = list.iter().filter_map(|s| app.product(s).map(|p| (s.clone(), p))).take(COLS * 2).collect();
    if items.is_empty() {
        return 0.0;
    }
    let pad = 16.0;
    let cell = (r.width() - 2.0 * pad - GAP * (COLS as f32 - 1.0)) / COLS as f32;
    let rows = items.len().div_ceil(COLS);
    let card = Rect::from_min_size(pos2(r.left(), top), vec2(r.width(), pad + 20.0 + rows as f32 * (cell + GAP) - GAP + pad));
    ui.painter().rect_filled(card, CornerRadius::same(16), faint(21));
    ui.painter().text(pos2(card.left() + pad, card.top() + pad + 8.0), Align2::LEFT_CENTER, "Suggested items for you", medium(12.0), Color32::WHITE.gamma_multiply(0.88));
    let mut add = None;
    for (i, (sku, p)) in items.iter().enumerate() {
        let c = Rect::from_min_size(pos2(card.left() + pad + (i % COLS) as f32 * (cell + GAP), card.top() + pad + 20.0 + (i / COLS) as f32 * (cell + GAP)), vec2(cell, cell));
        ui.painter().rect_filled(c, CornerRadius::same(8), faint(31));
        text::image(&ui.painter().with_clip_rect(c), &p.still(), c.shrink(6.0));
        mark(ui, &format!("wish:suggest:{i}"), c);
        if ui.rect_contains_pointer(c) {
            let h = Rect::from_center_size(pos2(c.right() - 16.0, c.top() + 16.0), vec2(24.0, 24.0));
            let resp = ui.interact(h, ui.id().with(("wish_suggest_heart", sku)), Sense::click());
            ui.painter().circle_filled(h.center(), 12.0, Color32::BLACK.gamma_multiply(if resp.hovered() { 0.7 } else { 0.5 }));
            icons::draw(ui.painter(), Icon::Heart, Rect::from_center_size(h.center(), vec2(14.0, 14.0)), Color32::WHITE);
            mark(ui, &format!("wish:suggest_heart:{i}"), h);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.on_hover_text(format!("Add to wishlist: {}", p.name)).clicked() {
                add = Some(sku.clone());
            }
        }
    }
    if let Some(sku) = add {
        app.keyed.insert("profed_wish".into(), None);
        app.rest("POST", "/users/@me/wishlist/items".into(), Some(json!({ "sku_id": sku })), Tag::Keyed("profed_wish".into()));
    }
    card.height()
}

/// Once an add has answered, the wishlist and the suggestions are fetched again.
pub fn settle(app: &mut App) {
    if matches!(app.keyed.get("profed_wish"), Some(Some(_))) {
        app.keyed.remove("profed_wish");
        app.keyed.retain(|k, _| !k.starts_with("wishlist:") && k != KEY && !k.starts_with("shop_wish") && !k.starts_with("profile_wish:"));
    }
}
