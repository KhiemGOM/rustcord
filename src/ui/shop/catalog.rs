use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2};
use serde_json::{Value, json};

use super::product::{self, Hit, kind, sku};
use super::{CARD_MIN, NIGHT, PLATE, RADIUS_LG, RADIUS_SM, RADIUS_XXL, SPACE_12, SPACE_16, SPACE_20, SPACE_24, SPACE_32, SPACE_8};
use super::{button, cell, columns, get, list, pic, round_button, shade, text_width, want};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;

const HOME: &str = "/collectibles-shop?tab=home&include_bundles=true&include_dynamic_blocks=true&variants_return_style=2&skip_num_categories=0";
const CATS: &str = "/collectibles-categories/v2?include_bundles=true&variants_return_style=2&skip_num_categories=0";
const TABS: [&str; 3] = ["Featured", "Browse", "Orbs Exclusives"];
const FILTERS: [&str; 5] = ["Avatar Decorations", "Nameplates", "Profile Effects", "Bundles", "Orbs Eligible"];

const GRID_GAP: f32 = SPACE_24;
const FEED_MAX: f32 = 1080.0;
const FEED_PAD: f32 = SPACE_32;
const BLOCK_PAD: f32 = 30.0;
const CATALOG_MAX: f32 = 1124.0;
const CATALOG_PAD: f32 = SPACE_20;
const HERO_PAD: f32 = SPACE_16;
const HERO_BAND: f32 = 0.3391473;
const HERO_BAND_MIN: f32 = 200.0;
const HERO_HEADER_PAD: f32 = SPACE_20;
const LOGO_HEIGHT: f32 = 60.0;
const LOGO_MAX: f32 = 540.0;
const TILE_HEIGHT: f32 = 236.0;
const BANNER_HEIGHT: f32 = 180.0;

enum Act {
    Tab(u8),
    Open(String),
    Close,
    Wish(String),
    Buy(String),
    Only(usize),
    Collection(Option<String>),
}

struct Ctx<'a> {
    member: bool,
    avatar: &'a str,
    wish: &'a [String],
}

fn products(app: &App) -> Vec<&Value> {
    let src = get(app, "shop_cats").or(get(app, "shop_home"));
    src.and_then(|v| v["categories"].as_array()).into_iter().flatten().flat_map(|c| c["products"].as_array().into_iter().flatten()).collect()
}

fn pass(p: &Value, only: &[bool; 5]) -> bool {
    if !only.iter().any(|o| *o) {
        return true;
    }
    let k = kind(p);
    (only[0] && k == 0) || (only[1] && k == 2) || (only[2] && k == 1) || (only[3] && k == 1000) || (only[4] && product::orbs_only(p))
}

fn chip(ui: &mut Ui, r: Rect, label: &str, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("shop_chip", key)), Sense::click());
    ui.painter().rect(r, CornerRadius::same(RADIUS_SM), Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 }), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(14.0), pal().header_primary);
    mark(ui, &format!("shop:{key}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn grid(ui: &mut Ui, left: f32, y: &mut f32, w: f32, salt: &str, items: &[&Value], look: &Ctx, acts: &mut Vec<Act>) {
    let cols = columns(w, CARD_MIN, GRID_GAP);
    let cw = cell(w, cols, GRID_GAP);
    let view = ui.clip_rect().expand(CARD_MIN);
    for row in items.chunks(cols) {
        if *y + CARD_MIN >= view.top() && *y <= view.bottom() {
            for (i, p) in row.iter().enumerate() {
                let r = Rect::from_min_size(pos2(left + i as f32 * (cw + GRID_GAP), *y), vec2(cw, CARD_MIN));
                card(ui, r, salt, p, look, acts);
            }
        }
        *y += CARD_MIN + GRID_GAP;
    }
}

fn card(ui: &mut Ui, r: Rect, salt: &str, p: &Value, look: &Ctx, acts: &mut Vec<Act>) {
    let id = sku(p).to_string();
    match product::card(ui, r, salt, p, look.member, look.wish.contains(&id), look.avatar) {
        Some(Hit::Open) => acts.push(Act::Open(id)),
        Some(Hit::Wish) => acts.push(Act::Wish(id)),
        None => {}
    }
}

fn strip(ui: &mut Ui, r: Rect, salt: &str, items: &[&Value], look: &Ctx, acts: &mut Vec<Act>) {
    ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
        ScrollArea::horizontal().id_salt(("shop_strip", salt)).auto_shrink([false, false]).show(ui, |ui| {
            let origin = ui.cursor().min;
            for (i, p) in items.iter().enumerate() {
                let c = Rect::from_min_size(pos2(origin.x + i as f32 * (CARD_MIN + GRID_GAP), origin.y), vec2(CARD_MIN, CARD_MIN));
                card(ui, c, salt, p, look, acts);
            }
            ui.allocate_exact_size(vec2(items.len() as f32 * (CARD_MIN + GRID_GAP) - GRID_GAP, CARD_MIN), Sense::hover());
        });
    });
}

fn find<'a>(all: &[&'a Value], id: &str) -> Option<&'a Value> {
    all.iter().find(|p| sku(p) == id).copied()
}

fn picked<'a>(all: &[&'a Value], b: &Value) -> Vec<&'a Value> {
    b["ranked_sku_ids"].as_array().into_iter().flatten().filter_map(|s| find(all, s.as_str().unwrap_or_default())).collect()
}

fn hero(ui: &mut Ui, all: &[&Value], b: &Value, left: f32, y: &mut f32, cw: f32, look: &Ctx, acts: &mut Vec<Act>) {
    let inner_w = cw - 2.0 * HERO_PAD - 2.0;
    let band = (HERO_BAND * inner_w).max(HERO_BAND_MIN);
    let items = picked(all, b);
    let head = SPACE_20 + 28.0 + SPACE_12;
    let h = HERO_PAD + band + head + CARD_MIN + HERO_PAD + 2.0;
    let r = Rect::from_min_size(pos2(left, *y), vec2(cw, h));
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_LG), PLATE);
    if let Some(u) = b["hero_banner_url"].as_str() {
        pic(ui, u, r, RADIUS_LG, true);
    }
    shade(ui, Rect::from_min_max(pos2(r.left() + 1.0, r.top() + band * 0.4), r.max - vec2(1.0, 1.0)), Color32::BLACK, 0, 220);
    let inside = r.shrink(HERO_PAD + 1.0);
    let text_x = inside.left() + HERO_HEADER_PAD;
    let mut ty = inside.top() + (band - LOGO_HEIGHT - SPACE_20 - 70.0).max(SPACE_8) / 2.0;
    if let Some(u) = b["hero_logo_url"].as_str() {
        pic(ui, u, Rect::from_min_size(pos2(text_x, ty), vec2(LOGO_MAX.min(inside.width() - 2.0 * HERO_HEADER_PAD), LOGO_HEIGHT)), 0, false);
    }
    ty += LOGO_HEIGHT + SPACE_20;
    let name = b["name"].as_str().unwrap_or_default();
    let summary = b["summary"].as_str().unwrap_or_default().trim();
    let title = ui.painter().text(pos2(text_x, ty), Align2::LEFT_TOP, if summary.is_empty() { name } else { summary }, semi(32.0), Color32::WHITE);
    if !summary.is_empty() {
        ui.painter().text(pos2(text_x, title.bottom() + SPACE_8), Align2::LEFT_TOP, name, reg(16.0), Color32::from_gray(0xe0));
    }
    let row_y = inside.top() + band + SPACE_20;
    ui.painter().text(pos2(inside.left(), row_y + 14.0), Align2::LEFT_CENTER, "Eligible items", semi(20.0), Color32::WHITE);
    let id = b["category_sku_id"].as_str().map(str::to_string);
    let label = "Shop the Collection";
    let bw = text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16;
    if button(ui, Rect::from_min_size(pos2(inside.right() - bw, row_y - 4.0), vec2(bw, 36.0)), label, Color32::WHITE, Color32::BLACK, "collection") {
        acts.push(Act::Collection(id));
        acts.push(Act::Tab(1));
    }
    let strip_r = Rect::from_min_size(pos2(inside.left(), row_y + 28.0 + SPACE_12), vec2(inside.width(), CARD_MIN));
    strip(ui, strip_r, "hero", &items, look, acts);
    *y += h;
}

fn tiles(ui: &mut Ui, b: &Value, left: f32, y: &mut f32, cw: f32, acts: &mut Vec<Act>) {
    let t = b["subblocks"].as_array().map_or(&[][..], |a| a.as_slice());
    let cols = columns(cw, CARD_MIN, GRID_GAP).min(t.len().max(1));
    let tw = cell(cw, cols, GRID_GAP);
    for row in t.chunks(cols) {
        for (i, t) in row.iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * (tw + GRID_GAP), *y), vec2(tw, TILE_HEIGHT));
            let resp = ui.interact(r, Id::new(("shop_tile", t["category_sku_id"].as_str(), i)), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS_LG), PLATE);
            if let Some(u) = t["asset_url"].as_str() {
                pic(ui, u, r, RADIUS_LG, true);
            }
            let name = t["name"].as_str().unwrap_or_default();
            ui.painter().text(r.left_bottom() + vec2(SPACE_24, -SPACE_24), Align2::LEFT_BOTTOM, name, bold(20.0), Color32::WHITE);
            mark(ui, &format!("shop:tile:{name}"), r);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                acts.push(Act::Collection(t["category_sku_id"].as_str().map(str::to_string)));
                acts.push(Act::Tab(1));
            }
        }
        *y += TILE_HEIGHT + GRID_GAP;
    }
    *y -= GRID_GAP;
}

fn feed(ui: &mut Ui, all: &[&Value], blocks: &[Value], left: f32, y: &mut f32, cw: f32, look: &Ctx, acts: &mut Vec<Act>) {
    let mut first = true;
    for b in blocks {
        let ty = b["type"].as_u64();
        if !matches!(ty, Some(0 | 1 | 4)) || (ty == Some(4) && picked(all, b).is_empty()) {
            continue;
        }
        if !first {
            *y += 2.0 * BLOCK_PAD;
        }
        first = false;
        match ty {
            Some(0) => hero(ui, all, b, left, y, cw, look, acts),
            Some(1) => tiles(ui, b, left, y, cw, acts),
            _ => {
                let items = picked(all, b);
                ui.painter().text(pos2(left, *y + 16.0), Align2::LEFT_CENTER, b["name"].as_str().unwrap_or_default(), semi(24.0), Color32::WHITE);
                if b["show_button"].as_bool() == Some(true) {
                    let label = b["button_text"].as_str().unwrap_or("Shop all");
                    let bw = text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16;
                    if button(ui, Rect::from_min_size(pos2(left + cw - bw, *y), vec2(bw, 36.0)), label, Color32::WHITE, Color32::BLACK, "block_button") {
                        acts.push(Act::Collection(b["category_sku_id"].as_str().map(str::to_string)));
                        acts.push(Act::Tab(1));
                    }
                }
                *y += 36.0 + SPACE_12;
                grid(ui, left, y, cw, "block", &items, look, acts);
                *y -= GRID_GAP;
            }
        }
    }
}

fn banner(ui: &mut Ui, left: f32, y: f32, w: f32, cat: &Value) {
    let r = Rect::from_min_size(pos2(left, y), vec2(w, BANNER_HEIGHT));
    if !r.intersects(ui.clip_rect().expand(BANNER_HEIGHT)) {
        return;
    }
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_XXL), PLATE);
    if let Some(u) = cat["catalog_banner_url"].as_str().or(cat["hero_banner_url"].as_str()) {
        pic(ui, u, r, RADIUS_XXL, true);
    }
    if let Some(u) = cat["logo_url"].as_str().or(cat["hero_logo_url"].as_str()) {
        pic(ui, u, Rect::from_min_size(r.min + vec2(SPACE_24, SPACE_20), vec2(LOGO_MAX.min(w / 2.0), LOGO_HEIGHT)), 0, false);
    }
    ui.painter().text(r.left_bottom() + vec2(SPACE_24, -SPACE_20), Align2::LEFT_BOTTOM, cat["name"].as_str().unwrap_or_default(), bold(20.0), Color32::WHITE);
}

fn header(app: &App, ui: &mut Ui, tab: u8, search: &mut String, acts: &mut Vec<Act>, wishlist: &mut bool) {
    let orbs = get(app, "shop_orbs").and_then(|v| v["balance"].as_u64());
    egui::Panel::top("shop_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(NIGHT).inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            icons::draw(ui.painter(), Icon::Shop, Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(24.0, 24.0)), pal().channel_default);
            let mut x = r.left() + 56.0;
            let tabs = if tab == 3 { &["Featured", "Browse", "Orbs Exclusives", "Wishlist"][..] } else { &TABS[..] };
            for (i, name) in tabs.iter().enumerate() {
                let g = ui.painter().layout_no_wrap(name.to_string(), medium(16.0), pal().header_primary);
                let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x, 48.0));
                let resp = ui.interact(tr, ui.id().with(("shop_tab", i)), Sense::click());
                mark(ui, &format!("shop:tab:{name}"), tr);
                let on = tab as usize == i && search.is_empty();
                let col = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                ui.painter().galley(pos2(x, tr.center().y - g.size().y / 2.0), g, col);
                if on {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(x, tr.bottom() - 2.0), vec2(tr.width(), 2.0)), 1, pal().header_primary);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    acts.push(Act::Tab(i as u8));
                    search.clear();
                }
                x = tr.right() + SPACE_24;
            }
            let mut right = r.right();
            if let Some(n) = orbs {
                let g = ui.painter().layout_no_wrap(n.to_string(), semi(16.0), pal().header_primary);
                let chip = Rect::from_min_size(pos2(right - g.size().x - 52.0, r.center().y - 16.0), vec2(g.size().x + 52.0, 32.0));
                ui.painter().rect(chip, RADIUS_SM, Color32::from_rgba_unmultiplied(151, 151, 159, 31), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(chip.left() + 20.0, chip.center().y), vec2(20.0, 20.0)), pal().header_primary);
                ui.painter().galley(pos2(chip.left() + 38.0, chip.center().y - g.size().y / 2.0), g, pal().header_primary);
                mark(ui, "shop:orbs", chip);
                right = chip.left() - SPACE_8;
            }
            let heart = Rect::from_min_size(pos2(right - 32.0, r.center().y - 16.0), vec2(32.0, 32.0));
            if round_button(ui, heart, Icon::Heart, pal().header_primary, "wishlist") {
                *wishlist = true;
            }
            let room = heart.left() - SPACE_8 - x - SPACE_16;
            if room >= 96.0 {
                let wide = room.min(CARD_MIN * 0.9);
                let field = Rect::from_min_size(pos2(heart.left() - SPACE_8 - wide, r.center().y - 16.0), vec2(wide, 32.0));
                ui.painter().rect(field, RADIUS_SM, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(16.0, 16.0)), pal().text_muted);
                let inner = Rect::from_min_max(pos2(field.left() + 30.0, field.top() + 2.0), pos2(field.right() - SPACE_8, field.bottom() - 2.0));
                let edit = TextEdit::singleline(search).frame(Frame::NONE).hint_text("Search the Shop").font(reg(14.0)).desired_width(inner.width()).vertical_align(egui::Align::Center);
                ui.put(inner, edit);
                mark(ui, "shop:search", field);
            }
        });
}

fn filter_width(ui: &Ui) -> f32 {
    FILTERS.iter().map(|n| text_width(ui, n, reg(16.0))).fold(text_width(ui, "Show only", semi(18.0)), f32::max) + 36.0
}

fn filters(ui: &mut Ui, r: Rect, only: &[bool; 5], acts: &mut Vec<Act>) {
    ui.painter().text(r.min + vec2(0.0, 12.0), Align2::LEFT_CENTER, "Show only", semi(18.0), pal().header_primary);
    for (i, name) in FILTERS.iter().enumerate() {
        let row = Rect::from_min_size(r.min + vec2(0.0, SPACE_32 + 4.0 + i as f32 * 42.0), vec2(r.width(), 36.0));
        let resp = ui.interact(row, ui.id().with(("shop_only", i)), Sense::click());
        let box_ = Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(24.0, 24.0));
        ui.painter().rect(box_, 6, if only[i] { BLURPLE } else { Color32::TRANSPARENT }, Stroke::new(2.0, if only[i] { BLURPLE } else { pal().text_muted }), egui::StrokeKind::Inside);
        if only[i] {
            icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(box_.center(), vec2(16.0, 16.0)), Color32::WHITE);
        }
        ui.painter().text(pos2(row.left() + 36.0, row.center().y), Align2::LEFT_CENTER, *name, reg(16.0), pal().header_primary);
        mark(ui, &format!("shop:only:{name}"), row);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            acts.push(Act::Only(i));
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    want(app, "shop_home", HOME);
    want(app, "shop_cats", CATS);
    want(app, "shop_orbs", "/users/@me/virtual-currency/balance");
    let Some(s) = app.shop.as_ref() else { return };
    let (tab, only, collection, open, wish) = (s.tab, s.only, s.collection.clone(), s.product.clone(), s.wish.clone());
    let mut search = s.search.clone();
    let avatar = app.store.user(app.store.me).map(|u| u.avatar_url()).unwrap_or_default();
    let look = Ctx { member: app.store.nitro(), avatar: &avatar, wish: &wish };
    let all = products(app);
    let blocks = get(app, "shop_home").and_then(|v| v["shop_blocks"].as_array()).map_or(&[][..], |a| a.as_slice());
    let categories = get(app, "shop_cats").map_or(&[][..], |v| list(Some(&v["categories"])));
    let ready = get(app, "shop_home").is_some() || get(app, "shop_cats").is_some();
    let mut acts: Vec<Act> = Vec::new();
    let mut wishlist = false;
    header(app, ui, tab, &mut search, &mut acts, &mut wishlist);
    if wishlist {
        acts.push(Act::Tab(3));
        search.clear();
    }
    let query = search.trim().to_lowercase();
    egui::CentralPanel::default().frame(Frame::NONE.fill(NIGHT)).show(ui, |ui| {
        let side_w = filter_width(ui);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let top = ui.cursor().min;
            let mut y = top.y + SPACE_24;
            if !ready {
                ui.add_space(SPACE_32 * 4.0);
                ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                return;
            }
            let feed_w = (w - 2.0 * FEED_PAD).min(FEED_MAX);
            let feed_left = top.x + (w - feed_w) / 2.0;
            let catalog = |avail: f32| (avail.min(CATALOG_MAX) - 2.0 * CATALOG_PAD).max(CARD_MIN);
            let browsing = !query.is_empty() || tab == 1;
            let side = browsing && query.is_empty() && catalog(w - side_w - SPACE_16) >= 2.0 * CARD_MIN + GRID_GAP;
            let col_w = if side { w - side_w - SPACE_16 } else { w };
            let gw = catalog(col_w);
            let gl = top.x + (col_w - gw) / 2.0;
            let heading = |ui: &Ui, y: &mut f32, text: &str| {
                ui.painter().text(pos2(gl, *y + 16.0), Align2::LEFT_CENTER, text, semi(24.0), Color32::WHITE);
                *y += SPACE_32 + SPACE_16;
            };
            let empty = |ui: &Ui, y: &mut f32, text: &str| {
                ui.painter().text(pos2(gl + gw / 2.0, *y + 60.0), Align2::CENTER_CENTER, text, semi(20.0), pal().header_secondary);
                *y += 140.0;
            };
            if browsing {
                if side {
                    filters(ui, Rect::from_min_size(pos2(top.x + col_w + SPACE_8, y), vec2(side_w, 300.0)), &only, &mut acts);
                } else if query.is_empty() {
                    let mut x = gl;
                    for (i, name) in FILTERS.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(x, y), vec2(text_width(ui, name, medium(14.0)) + 2.0 * SPACE_12 + 4.0, 32.0));
                        if x + r.width() > gl + gw {
                            break;
                        }
                        if chip(ui, r, name, &format!("only:{name}")) {
                            acts.push(Act::Only(i));
                        }
                        x = r.right() + SPACE_8;
                    }
                    y += SPACE_32 + SPACE_16;
                }
                if let Some(c) = collection.as_deref().filter(|_| query.is_empty()) {
                    let name = categories.iter().find(|x| x["sku_id"] == c).and_then(|x| x["name"].as_str()).unwrap_or("Collection");
                    let label = format!("{name}  ✕");
                    if chip(ui, Rect::from_min_size(pos2(gl, y), vec2(text_width(ui, &label, medium(14.0)) + 2.0 * SPACE_12 + 4.0, 32.0)), &label, "collection_clear") {
                        acts.push(Act::Collection(None));
                    }
                    y += SPACE_32 + SPACE_16;
                }
                if !query.is_empty() {
                    heading(ui, &mut y, &format!("Results for “{}”", search.trim()));
                    let hits: Vec<&Value> = all.iter().copied().filter(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase().contains(&query))).collect();
                    if hits.is_empty() {
                        empty(ui, &mut y, "No results");
                    }
                    grid(ui, gl, &mut y, gw, "search", &hits, &look, &mut acts);
                } else {
                    let mut shown = 0;
                    for c in categories.iter().filter(|c| collection.as_deref().is_none_or(|k| c["sku_id"] == k)) {
                        let prods: Vec<&Value> = c["products"].as_array().into_iter().flatten().filter(|p| pass(p, &only)).collect();
                        if prods.is_empty() {
                            continue;
                        }
                        shown += prods.len();
                        banner(ui, gl, y, gw, c);
                        y += BANNER_HEIGHT + SPACE_24;
                        grid(ui, gl, &mut y, gw, "browse", &prods, &look, &mut acts);
                    }
                    if shown == 0 {
                        empty(ui, &mut y, "Nothing matches these filters");
                    }
                }
            } else if tab == 0 {
                if blocks.is_empty() {
                    empty(ui, &mut y, "The Shop has nothing featured right now");
                }
                y += SPACE_24;
                feed(ui, &all, blocks, feed_left, &mut y, feed_w, &look, &mut acts);
                y += BLOCK_PAD;
            } else if tab == 2 {
                heading(ui, &mut y, "Orbs Exclusives");
                let items: Vec<&Value> = all.iter().copied().filter(|p| product::orbs_only(p)).collect();
                if items.is_empty() {
                    empty(ui, &mut y, "No Orbs exclusives right now");
                }
                grid(ui, gl, &mut y, gw, "orbs", &items, &look, &mut acts);
            } else {
                heading(ui, &mut y, "Your Wishlist");
                let items: Vec<&Value> = wish.iter().filter_map(|id| find(&all, id)).collect();
                if items.is_empty() {
                    empty(ui, &mut y, "Heart an item to save it here");
                }
                grid(ui, gl, &mut y, gw, "wish", &items, &look, &mut acts);
            }
            ui.allocate_exact_size(vec2(w, y - top.y + SPACE_32), Sense::hover());
        });
    });
    if let Some(p) = open.as_deref().and_then(|id| find(&all, id))
        && let Some(a) = product::detail(ui.ctx(), p, look.member, wish.iter().any(|w| w == sku(p)), &avatar)
    {
        acts.push(match a {
            product::Act::Close => Act::Close,
            product::Act::Buy => Act::Buy(sku(p).to_string()),
            product::Act::Wish => Act::Wish(sku(p).to_string()),
        });
    }
    let found = open.as_deref().is_none_or(|id| find(&all, id).is_some());
    if let Some(s) = &mut app.shop {
        s.search = search;
        if !found {
            s.product = None;
        }
    }
    apply(app, acts);
}

fn apply(app: &mut App, acts: Vec<Act>) {
    for a in acts {
        match a {
            Act::Tab(t) => {
                if let Some(s) = &mut app.shop {
                    s.tab = t;
                    if t != 1 {
                        s.collection = None;
                    }
                    s.search.clear();
                }
            }
            Act::Open(id) => {
                if let Some(s) = &mut app.shop {
                    s.product = Some(id);
                }
            }
            Act::Close => {
                if let Some(s) = &mut app.shop {
                    s.product = None;
                }
            }
            Act::Only(i) => {
                if let Some(s) = &mut app.shop {
                    s.only[i] = !s.only[i];
                }
            }
            Act::Collection(c) => {
                if let Some(s) = &mut app.shop {
                    s.collection = c;
                }
            }
            Act::Wish(id) => wish(app, id),
            Act::Buy(id) => buy(app, id),
        }
    }
}

fn wish(app: &mut App, id: String) {
    let Some(s) = &mut app.shop else { return };
    if let Some(i) = s.wish.iter().position(|w| *w == id) {
        s.wish.remove(i);
        if let Some(list) = get(app, "shop_wish").and_then(|v| v["id"].as_str()).map(str::to_string) {
            app.rest("DELETE", format!("/users/@me/wishlists/{list}/items/{id}"), None, Tag::Quiet);
        }
    } else {
        s.wish.push(id.clone());
        app.remote.insert("shop_wish", None);
        app.rest("POST", "/users/@me/wishlist/items".into(), Some(json!({ "sku_id": id })), Tag::Remote("shop_wish"));
    }
}

fn buy(app: &mut App, id: String) {
    if let Some(p) = products(app).into_iter().find(|p| sku(p) == id).cloned() {
        app.open_buy(crate::ui::buy::What::Shop(p));
    }
}
