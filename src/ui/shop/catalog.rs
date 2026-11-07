use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2};
use serde_json::Value;

use super::browse::{self, Hits};
use super::product::{self, Hit, sku};
use super::{CARD_MIN, NIGHT, PLATE, RADIUS_LG, RADIUS_SM, RADIUS_XXL, SPACE_12, SPACE_16, SPACE_20, SPACE_24, SPACE_32, SPACE_8};
use super::{button, cell, columns, get, list, pic, round_button, shade, text_width, want, wish};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;
use crate::ui::{App, menu_row};

const HOME: &str = "/collectibles-shop?tab=home&include_bundles=true&include_dynamic_blocks=true&variants_return_style=2&skip_num_categories=0";
const ORBS_HOME: &str = "/collectibles-shop?tab=orbs&include_bundles=true&include_dynamic_blocks=true&variants_return_style=2&skip_num_categories=0";
const CATS: &str = "/collectibles-categories/v2?include_bundles=true&variants_return_style=2&skip_num_categories=0";
const TABS: [&str; 3] = ["Featured", "Browse", "Orbs Exclusives"];
const FEED_SORTS: [&str; 3] = ["Popular", "Recently Added", "Price: Low to High"];
const FEED_STEP: usize = 12;
const FEED_MAX: usize = 36;

const GRID_GAP: f32 = SPACE_24;
const FEED_WIDTH: f32 = 1080.0;
const FEED_PAD: f32 = SPACE_32;
const BLOCK_PAD: f32 = 30.0;
const HERO_PAD: f32 = SPACE_16;
const HERO_BAND: f32 = 0.3391473;
const HERO_BAND_MIN: f32 = 200.0;
const HERO_HEADER_PAD: f32 = SPACE_20;
const LOGO_HEIGHT: f32 = 60.0;
const LOGO_MAX: f32 = 540.0;
const TILE_HEIGHT: f32 = 236.0;
const BANNER_HEIGHT: f32 = 180.0;
const STRIP_ARROW: f32 = 40.0;

enum Act {
    Go(u8),
    Open(String),
    Close,
    Wish(String),
    Buy(String),
    Collection(Option<String>),
    Pick(usize),
    Heart,
    Only(usize),
    Collabs,
    FeedSort(usize),
    Shuffle,
    More,
    Quests,
}

struct Ctx<'a> {
    member: bool,
    avatar: &'a str,
    wish: &'a [String],
    orbs: bool,
}

fn products(app: &App) -> Vec<&Value> {
    let src = get(app, "shop_cats").or(get(app, "shop_home"));
    src.and_then(|v| v["categories"].as_array()).into_iter().flatten().flat_map(|c| c["products"].as_array().into_iter().flatten()).collect()
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

fn skeleton(ui: &mut Ui, left: f32, y: &mut f32, w: f32, count: usize) {
    let cols = columns(w, CARD_MIN, GRID_GAP);
    let cw = cell(w, cols, GRID_GAP);
    for row in 0..count.div_ceil(cols) {
        for i in 0..cols.min(count - row * cols) {
            let r = Rect::from_min_size(pos2(left + i as f32 * (cw + GRID_GAP), *y), vec2(cw, CARD_MIN));
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SM), PLATE);
        }
        *y += CARD_MIN + GRID_GAP;
    }
}

fn card(ui: &mut Ui, r: Rect, salt: &str, p: &Value, look: &Ctx, acts: &mut Vec<Act>) {
    let id = sku(p).to_string();
    match product::card(ui, r, salt, p, look.member, look.wish.contains(&id), look.avatar, look.orbs) {
        Some(Hit::Open) => acts.push(Act::Open(id)),
        Some(Hit::Wish) => acts.push(Act::Wish(id)),
        None => {}
    }
}

fn strip(ui: &mut Ui, r: Rect, salt: &str, items: &[&Value], look: &Ctx, acts: &mut Vec<Act>) {
    let key = Id::new(("shop_strip_to", salt));
    let to: Option<f32> = ui.data_mut(|d| d.remove_temp(key));
    let content = items.len() as f32 * (CARD_MIN + GRID_GAP) - GRID_GAP;
    let mut offset = 0.0;
    ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
        let mut area = ScrollArea::horizontal().id_salt(("shop_strip", salt)).auto_shrink([false, false]);
        if let Some(x) = to {
            area = area.horizontal_scroll_offset(x);
        }
        let out = area.show(ui, |ui| {
            let origin = ui.cursor().min;
            for (i, p) in items.iter().enumerate() {
                let c = Rect::from_min_size(pos2(origin.x + i as f32 * (CARD_MIN + GRID_GAP), origin.y), vec2(CARD_MIN, CARD_MIN));
                card(ui, c, salt, p, look, acts);
            }
            ui.allocate_exact_size(vec2(content, CARD_MIN), Sense::hover());
        });
        offset = out.state.offset.x;
    });
    if offset + r.width() + 1.0 < content {
        let b = Rect::from_center_size(pos2(r.right() - STRIP_ARROW / 2.0 - SPACE_8, r.center().y), vec2(STRIP_ARROW, STRIP_ARROW));
        if round_button(ui, b, Icon::ArrowRight, Color32::WHITE, &format!("scroll_right:{salt}")) {
            ui.data_mut(|d| d.insert_temp(key, (offset + r.width() * 0.8).min(content - r.width())));
        }
    }
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
            }
        }
        *y += TILE_HEIGHT + GRID_GAP;
    }
    *y -= GRID_GAP;
}

struct FeedState {
    sort: usize,
    shown: usize,
    shuffle: u32,
}

fn shuffled<'a>(mut items: Vec<&'a Value>, seed: u32) -> Vec<&'a Value> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    for i in (1..items.len()).rev() {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        items.swap(i, (x >> 8) as usize % (i + 1));
    }
    items
}

fn feed_order<'a>(all: &[&'a Value], b: &Value, st: &FeedState, member: bool, orbs: bool) -> Vec<&'a Value> {
    let pool: Vec<&Value> = all.iter().copied().filter(|p| !orbs || product::orbs_only(p)).collect();
    if st.shuffle > 0 && st.sort == 3 {
        return shuffled(pool, st.shuffle);
    }
    match st.sort {
        1 => pool,
        2 => {
            let mut v = pool;
            v.sort_by_key(|p| product::amount(p, member));
            v
        }
        _ => {
            let ids = b["sorted_sku_ids"]["popular"].as_array().or(b["ranked_sku_ids"].as_array());
            let mut first: Vec<&Value> = ids.into_iter().flatten().filter_map(|s| find(&pool, s.as_str().unwrap_or_default())).collect();
            for p in &pool {
                if !first.iter().any(|f| sku(f) == sku(p)) {
                    first.push(p);
                }
            }
            first
        }
    }
}

fn feed_block(ui: &mut Ui, all: &[&Value], b: &Value, title: &str, left: f32, y: &mut f32, cw: f32, st: &FeedState, orbs: bool, look: &Ctx, acts: &mut Vec<Act>) {
    ui.painter().text(pos2(left, *y + 20.0), Align2::LEFT_CENTER, title, semi(24.0), Color32::WHITE);
    let shuffle_w = text_width(ui, "Shuffle!", medium(16.0)) + 2.0 * SPACE_16;
    let sh = Rect::from_min_size(pos2(left + cw - shuffle_w, *y), vec2(shuffle_w, 40.0));
    if button(ui, sh, "Shuffle!", Color32::from_rgba_unmultiplied(151, 151, 159, 40), pal().header_primary, "shuffle") {
        acts.push(Act::Shuffle);
    }
    let select = Rect::from_min_size(pos2(sh.left() - SPACE_16 - 190.0, *y), vec2(190.0, 40.0));
    let current = if st.sort == 3 { "Shuffled" } else { FEED_SORTS[st.sort] };
    if let Some(i) = browse::select(ui, select, "feed_sort", current, &FEED_SORTS) {
        acts.push(Act::FeedSort(i));
    }
    *y += 40.0 + SPACE_20;
    let order = feed_order(all, b, st, look.member, orbs);
    let shown: Vec<&Value> = order.iter().copied().take(st.shown).collect();
    grid(ui, left, y, cw, "feed", &shown, look, acts);
    *y -= GRID_GAP;
    if st.shown >= FEED_MAX || st.shown >= order.len() {
        *y += SPACE_32;
        let label = "Shop All";
        let r = Rect::from_min_size(pos2(left, *y), vec2(cw, 40.0));
        if button(ui, r, label, BLURPLE, Color32::WHITE, "shop_all") {
            acts.push(Act::Go(1));
        }
        *y += 40.0;
    }
}

fn wide_banner(ui: &mut Ui, b: &Value, left: f32, y: &mut f32, cw: f32, acts: &mut Vec<Act>) {
    let h = (cw * 0.22).clamp(180.0, 280.0);
    let r = Rect::from_min_size(pos2(left, *y), vec2(cw, h));
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_LG), PLATE);
    if let Some(u) = b["wide_banner_url"].as_str() {
        pic(ui, u, r, RADIUS_LG, true);
    }
    shade(ui, r, Color32::BLACK, 120, 0);
    let ink = Color32::WHITE;
    let galley = ui.painter().layout(b["title"].as_str().unwrap_or_default().to_string(), bold(32.0), ink, r.width() * 0.6);
    let gh = galley.size().y;
    let body = ui.painter().layout(b["body"].as_str().unwrap_or_default().to_string(), reg(16.0), Color32::from_gray(0xc8), r.width() * 0.6);
    let total = gh + SPACE_8 + body.size().y;
    let top = r.center().y - total / 2.0;
    ui.painter().galley(pos2(r.left() + SPACE_32, top), galley, ink);
    ui.painter().galley(pos2(r.left() + SPACE_32, top + gh + SPACE_8), body, Color32::from_gray(0xc8));
    if let Some(label) = b["cta_text"].as_str().filter(|_| b["disable_cta"].as_bool() != Some(true)) {
        let bw = text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16;
        if button(ui, Rect::from_center_size(pos2(r.right() - SPACE_32 - bw / 2.0, r.center().y), vec2(bw, 40.0)), label, Color32::WHITE, Color32::BLACK, "wide_cta") {
            acts.push(Act::Quests);
        }
    }
    *y += h;
}

fn feed(ui: &mut Ui, all: &[&Value], blocks: &[Value], left: f32, y: &mut f32, cw: f32, st: &FeedState, look: &Ctx, acts: &mut Vec<Act>) {
    let mut first = true;
    for b in blocks {
        let ty = b["type"].as_u64();
        if !matches!(ty, Some(0 | 1 | 2 | 3 | 4)) || (ty == Some(4) && picked(all, b).is_empty()) {
            continue;
        }
        if !first {
            *y += 2.0 * BLOCK_PAD;
        }
        first = false;
        match ty {
            Some(0) => hero(ui, all, b, left, y, cw, look, acts),
            Some(1) => tiles(ui, b, left, y, cw, acts),
            Some(2) => feed_block(ui, all, b, "Find your style", left, y, cw, st, false, look, acts),
            Some(3) => wide_banner(ui, b, left, y, cw, acts),
            _ => {
                let items = picked(all, b);
                ui.painter().text(pos2(left, *y + 16.0), Align2::LEFT_CENTER, b["name"].as_str().unwrap_or_default(), semi(24.0), Color32::WHITE);
                if b["show_button"].as_bool() == Some(true) {
                    let label = b["button_text"].as_str().unwrap_or("Shop all");
                    let bw = text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16;
                    if button(ui, Rect::from_min_size(pos2(left + cw - bw, *y), vec2(bw, 36.0)), label, Color32::WHITE, Color32::BLACK, "block_button") {
                        acts.push(Act::Collection(b["category_sku_id"].as_str().map(str::to_string)));
                    }
                }
                *y += 36.0 + SPACE_12;
                grid(ui, left, y, cw, "block", &items, look, acts);
                *y -= GRID_GAP;
            }
        }
    }
}

fn banner(ui: &mut Ui, left: f32, y: f32, w: f32, cat: &Value, acts: &mut Vec<Act>) {
    let r = Rect::from_min_size(pos2(left, y), vec2(w, BANNER_HEIGHT));
    if !r.intersects(ui.clip_rect().expand(BANNER_HEIGHT)) {
        return;
    }
    let id = cat["sku_id"].as_str().unwrap_or_default();
    let resp = ui.interact(r, Id::new(("shop_banner", id)), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(RADIUS_XXL), PLATE);
    if let Some(u) = cat["catalog_banner_url"].as_str().or(cat["hero_banner_url"].as_str()) {
        pic(ui, u, r, RADIUS_XXL, true);
    }
    if let Some(u) = cat["logo_url"].as_str().or(cat["hero_logo_url"].as_str()) {
        pic(ui, u, Rect::from_min_size(r.min + vec2(SPACE_24, SPACE_20), vec2(LOGO_MAX.min(w / 2.0), LOGO_HEIGHT)), 0, false);
    }
    ui.painter().text(r.left_bottom() + vec2(SPACE_24, -SPACE_20), Align2::LEFT_BOTTOM, cat["name"].as_str().unwrap_or_default(), bold(20.0), Color32::WHITE);
    mark(ui, &format!("shop:banner:{id}"), r);
    if ui.rect_contains_pointer(r) {
        let arrow = Rect::from_center_size(pos2(r.right() - SPACE_32, r.center().y), vec2(32.0, 32.0));
        ui.painter().rect_filled(arrow, CornerRadius::same(RADIUS_LG), Color32::from_black_alpha(170));
        icons::draw(ui.painter(), Icon::ArrowRight, Rect::from_center_size(arrow.center(), vec2(16.0, 16.0)), Color32::WHITE);
        mark(ui, &format!("shop:view_collection:{id}"), arrow);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.interact(arrow, Id::new(("shop_view_collection", id)), Sense::hover()).on_hover_text("View Collection");
    }
    if resp.clicked() {
        acts.push(Act::Collection(Some(id.to_string())));
    }
}

fn header(app: &App, ui: &mut Ui, tab: u8, search: &mut String, acts: &mut Vec<Act>) {
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
            let searching = !search.trim().is_empty();
            for (i, name) in TABS.iter().enumerate() {
                let g = ui.painter().layout_no_wrap(name.to_string(), medium(16.0), pal().header_primary);
                let chev = if i == 1 { 24.0 } else { 0.0 };
                let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x + chev, 48.0));
                let label = Rect::from_min_size(tr.min, vec2(g.size().x, 48.0));
                let resp = ui.interact(label, ui.id().with(("shop_tab", i)), Sense::click());
                mark(ui, &format!("shop:tab:{name}"), label);
                let on = tab as usize == i || (searching && i == 1);
                let hover = resp.hovered() || (i == 1 && ui.rect_contains_pointer(tr));
                let col = if on || hover { pal().header_primary } else { pal().header_secondary };
                ui.painter().galley(pos2(x, tr.center().y - g.size().y / 2.0), g, col);
                if i == 1 {
                    let c = Rect::from_min_size(pos2(label.right(), tr.top()), vec2(chev, 48.0));
                    let cr = ui.interact(c, ui.id().with("shop_browse_menu"), Sense::click());
                    crate::ui::motion::chevron(ui, ui.id().with("shop_browse_chev"), true, Rect::from_center_size(pos2(c.left() + 12.0, c.center().y), vec2(20.0, 20.0)), col);
                    mark(ui, "shop:browse_menu", c);
                    if cr.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    egui::Popup::menu(&cr).gap(0.0).show(|ui| {
                        for (n, (label, _)) in browse::TYPES.iter().enumerate() {
                            if menu_row(ui, None, label, None, false, crate::ui::Trail::None).clicked() {
                                acts.push(Act::Only(n));
                            }
                        }
                        if menu_row(ui, None, "Collabs", None, false, crate::ui::Trail::None).clicked() {
                            acts.push(Act::Collabs);
                        }
                        if menu_row(ui, None, "Shop All", None, false, crate::ui::Trail::None).clicked() {
                            acts.push(Act::Go(1));
                        }
                    });
                }
                if on {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(x, tr.bottom() - 2.0), vec2(label.width(), 2.0)), 1, pal().header_primary);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    acts.push(Act::Go(i as u8));
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
                acts.push(Act::Heart);
            }
            let room = heart.left() - SPACE_8 - x - SPACE_16;
            if room >= 96.0 {
                let wide = room.min(CARD_MIN * 0.9);
                let field = Rect::from_min_size(pos2(heart.left() - SPACE_8 - wide, r.center().y - 16.0), vec2(wide, 32.0));
                ui.painter().rect(field, RADIUS_SM, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(16.0, 16.0)), pal().text_muted);
                let clear = if search.is_empty() { 0.0 } else { 24.0 };
                let inner = Rect::from_min_max(pos2(field.left() + 30.0, field.top() + 2.0), pos2(field.right() - SPACE_8 - clear, field.bottom() - 2.0));
                let edit = TextEdit::singleline(search).frame(Frame::NONE).hint_text("Search the Shop").font(reg(14.0)).desired_width(inner.width()).vertical_align(egui::Align::Center);
                ui.put(inner, edit);
                mark(ui, "shop:search", field);
                if clear > 0.0 {
                    let x = Rect::from_center_size(pos2(field.right() - 16.0, field.center().y), vec2(20.0, 20.0));
                    let resp = ui.interact(x, ui.id().with("shop_search_clear"), Sense::click());
                    icons::draw(ui.painter(), Icon::Close, x.shrink(3.0), if resp.hovered() { pal().header_primary } else { pal().text_muted });
                    mark(ui, "shop:search_clear", x);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        search.clear();
                    }
                }
            }
        });
}

fn collection_page(ui: &mut Ui, cat: Option<&Value>, left: f32, y: &mut f32, w: f32, gw: f32, gl: f32, acts: &mut Vec<Act>, items: &[&Value], look: &Ctx) {
    let h = (w * 0.25).clamp(240.0, 460.0);
    let top = *y;
    let r = Rect::from_min_size(pos2(left, top), vec2(w, h));
    ui.painter().rect_filled(r, 0, PLATE);
    if let Some(u) = cat.and_then(|c| c["hero_banner_url"].as_str().or(c["catalog_banner_url"].as_str())) {
        pic(ui, u, r, 0, true);
    }
    shade(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - h * 0.4), r.max), NIGHT, 0, 255);
    let name = cat.and_then(|c| c["name"].as_str()).unwrap_or("Collection");
    let all_w = text_width(ui, "Shop All", medium(16.0));
    let name_w = text_width(ui, name, medium(16.0));
    let crumb = Rect::from_min_size(pos2(gl, top + SPACE_12), vec2(SPACE_12 + all_w + SPACE_12 + 8.0 + SPACE_12 + name_w + SPACE_12, 36.0));
    ui.painter().rect(crumb, CornerRadius::same(RADIUS_SM), Color32::from_black_alpha(100), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let back = Rect::from_min_size(crumb.min, vec2(SPACE_12 + all_w + SPACE_12, 36.0));
    let resp = ui.interact(back, Id::new("shop_crumb_all"), Sense::click());
    ui.painter().text(pos2(crumb.left() + SPACE_12, crumb.center().y), Align2::LEFT_CENTER, "Shop All", medium(16.0), if resp.hovered() { pal().header_primary } else { pal().header_secondary });
    ui.painter().text(pos2(back.right(), crumb.center().y), Align2::LEFT_CENTER, "/", medium(16.0), pal().header_secondary);
    ui.painter().text(pos2(back.right() + 8.0 + SPACE_12, crumb.center().y), Align2::LEFT_CENTER, name, medium(16.0), pal().header_primary);
    mark(ui, "shop:crumb_all", back);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        acts.push(Act::Go(1));
    }
    let mut ty = top + (h - LOGO_HEIGHT - SPACE_20 - 40.0).max(2.0 * SPACE_32) / 2.0;
    if let Some(u) = cat.and_then(|c| c["hero_logo_url"].as_str().or(c["logo_url"].as_str())) {
        pic(ui, u, Rect::from_min_size(pos2(gl, ty), vec2(LOGO_MAX.min(gw / 2.0), LOGO_HEIGHT * 1.5)), 0, false);
        ty += LOGO_HEIGHT * 1.5 + SPACE_20;
    }
    if let Some(s) = cat.and_then(|c| c["summary"].as_str()).map(str::trim).filter(|s| !s.is_empty()) {
        let g = ui.painter().layout(s.to_string(), reg(16.0), Color32::WHITE, (gw * 0.45).max(CARD_MIN));
        ui.painter().galley(pos2(gl, ty), g, Color32::WHITE);
    }
    *y = top + h + SPACE_24;
    let mut y2 = *y;
    grid(ui, gl, &mut y2, gw, "collection", items, look, acts);
    *y = y2;
}

fn orbs_hero(ui: &mut Ui, blocks: &[Value], left: f32, y: &mut f32, w: f32, acts: &mut Vec<Act>) {
    let h = (w * 0.28).clamp(240.0, 420.0);
    let r = Rect::from_min_size(pos2(left, *y), vec2(w, h));
    let wide = blocks.iter().find(|b| b["type"].as_u64() == Some(3));
    ui.painter().rect_filled(r, 0, NIGHT);
    shade(ui, r, Color32::from_rgb(0x27, 0x1e, 0xad), 90, 0);
    if let Some(u) = wide.and_then(|b| b["wide_banner_url"].as_str()) {
        pic(ui, u, r, 0, true);
    }
    shade(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - h * 0.4), r.max), NIGHT, 0, 255);
    let title = wide.and_then(|b| b["title"].as_str()).unwrap_or("Introducing Discord Orbs");
    let body = wide.and_then(|b| b["body"].as_str()).unwrap_or("Reward Your Play. Earn through Quests. Spend in the Shop.");
    let label = wide.and_then(|b| b["cta_text"].as_str()).unwrap_or("Go to Quest Home");
    let x = left + (w - (w.min(browse::CATALOG_MAX) - 2.0 * browse::CATALOG_PAD)) / 2.0;
    let t = ui.painter().text(pos2(x, r.top() + h * 0.28), Align2::LEFT_TOP, title, bold(40.0), Color32::WHITE);
    ui.painter().text(pos2(x, t.bottom() + SPACE_12), Align2::LEFT_TOP, body, reg(18.0), Color32::from_gray(0xc8));
    let bw = text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16;
    if button(ui, Rect::from_min_size(pos2(r.right() - bw - w * 0.1, r.top() + h * 0.3), vec2(bw, 40.0)), label, Color32::WHITE, Color32::BLACK, "quest_home") {
        acts.push(Act::Quests);
    }
    *y += h - SPACE_32;
}

pub fn show(app: &mut App, ui: &mut Ui) {
    want(app, "shop_home", HOME);
    want(app, "shop_cats", CATS);
    want(app, "shop_orbs", "/users/@me/virtual-currency/balance");
    wish::sync(app);
    let Some(s) = app.shop.as_ref() else { return };
    let (tab, filters, collection, open, wishing, slide) = (s.tab, s.browse.clone(), s.collection.clone(), s.product.clone(), s.wish.clone(), s.slide);
    let feed_state = FeedState { sort: s.feed_sort, shown: s.feed_shown, shuffle: s.shuffle };
    let mut search = s.search.clone();
    let query = search.trim().to_string();
    if tab == 2 {
        want(app, "shop_orbs_home", ORBS_HOME);
    }
    let browsing = (tab == 1 || !query.is_empty()) && collection.is_none();
    let plan = browse::plan(ui.ctx(), ui.available_width(), filters.open && browsing);
    let limit = plan.cols * 5;
    let search_path = filters.path(&query, limit);
    let hits: Option<Result<Hits, String>> = if browsing && !filters.plain(&query) { browse::hits(app, &search_path) } else { None };
    let avatar = app.store.user(app.store.me).map(|u| u.avatar_url()).unwrap_or_default();
    let member = app.store.nitro();
    let all = products(app);
    let blocks = get(app, "shop_home").and_then(|v| v["shop_blocks"].as_array()).map_or(&[][..], |a| a.as_slice());
    let orbs_blocks = get(app, "shop_orbs_home").and_then(|v| v["shop_blocks"].as_array()).map_or(&[][..], |a| a.as_slice());
    let categories = get(app, "shop_cats").map_or(&[][..], |v| list(Some(&v["categories"])));
    let ready = get(app, "shop_home").is_some() || get(app, "shop_cats").is_some();
    let mut acts: Vec<Act> = Vec::new();
    let mut filters = filters;
    let look = Ctx { member, avatar: &avatar, wish: &wishing, orbs: tab == 2 && !browsing };
    header(app, ui, tab, &mut search, &mut acts);
    let query = search.trim().to_string();
    let small_filters = browsing && !plan.wide && filters.open;
    let mut toggle_at = Rect::NOTHING;
    let mut cleared = false;
    if browsing && plan.wide && filters.open {
        let side_w = browse::BAR + 2.0 * browse::BAR_PAD;
        egui::Panel::right("shop_filter_bar").exact_size(side_w).resizable(false).show_separator_line(false).frame(Frame::new().fill(NIGHT).inner_margin(Margin { left: browse::BAR_PAD as i8, right: browse::BAR_PAD as i8, top: 8, bottom: 8 })).show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().vline(r.left() - browse::BAR_PAD, r.y_range().expand(8.0), Stroke::new(1.0, pal().border_subtle));
            ScrollArea::vertical().id_salt("shop_filter_scroll").auto_shrink([false, false]).show(ui, |ui| {
                cleared = browse::bar(ui, &mut filters, &query);
            });
        });
    }
    let mut scroll_state = (0.0, 0.0, 0.0);
    egui::CentralPanel::default().frame(Frame::NONE.fill(NIGHT)).show(ui, |ui| {
        let out = ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let top = ui.cursor().min;
            let mut y = top.y + SPACE_24;
            if !ready {
                ui.add_space(SPACE_32 * 4.0);
                ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                return;
            }
            let feed_w = (w - 2.0 * FEED_PAD).min(FEED_WIDTH);
            let feed_left = top.x + (w - feed_w) / 2.0;
            let gw = plan.gw;
            let gl = top.x + (plan.col_w - gw) / 2.0;
            let empty = |ui: &Ui, y: &mut f32, title: &str, sub: &str| {
                ui.painter().text(pos2(gl + gw / 2.0, *y + 40.0), Align2::CENTER_CENTER, title, semi(20.0), pal().header_primary);
                ui.painter().text(pos2(gl + gw / 2.0, *y + 72.0), Align2::CENTER_CENTER, sub, medium(16.0), pal().header_secondary);
                *y += 140.0;
            };
            if let Some(c) = collection.as_deref() {
                let cat = categories.iter().find(|x| x["sku_id"] == c);
                let items: Vec<&Value> = cat.map(|c| c["products"].as_array().into_iter().flatten().collect()).unwrap_or_default();
                y = top.y;
                collection_page(ui, cat, top.x, &mut y, w, gw, top.x + (w - gw) / 2.0, &mut acts, &items, &look);
            } else if browsing {
                let row = Rect::from_min_size(pos2(gl, y - SPACE_24 + 5.0), vec2(gw, 40.0));
                let total = hits.as_ref().and_then(|h| h.as_ref().ok()).map(|h| h.total);
                let title = browse::heading(&filters, &query, total);
                let (used, toggle) = browse::controls(ui, row, &mut filters, &query, &title);
                toggle_at = toggle;
                y = row.top() + used + SPACE_24 + 1.0;
                if !filters.dirty(&query) && filters.plain(&query) {
                    let mut shown = 0;
                    for c in categories.iter() {
                        let prods: Vec<&Value> = c["products"].as_array().into_iter().flatten().collect();
                        if prods.is_empty() {
                            continue;
                        }
                        shown += prods.len();
                        banner(ui, gl, y, gw, c, &mut acts);
                        y += BANNER_HEIGHT + SPACE_24;
                        grid(ui, gl, &mut y, gw, "browse", &prods, &look, &mut acts);
                    }
                    if shown == 0 {
                        empty(ui, &mut y, "No results found", "Try searching for something else");
                    }
                } else {
                    match &hits {
                        None => skeleton(ui, gl, &mut y, gw, limit),
                        Some(Err(e)) => empty(ui, &mut y, "Something went wrong", e),
                        Some(Ok(h)) => {
                            let items: Vec<&Value> = h.skus.iter().filter_map(|id| find(&all, id)).collect();
                            if items.is_empty() {
                                empty(ui, &mut y, "No results found", "Try searching for something else");
                            }
                            grid(ui, gl, &mut y, gw, "search", &items, &look, &mut acts);
                            if h.total > limit {
                                y += SPACE_8;
                                let pages = h.total.div_ceil(limit);
                                if let Some(n) = browse::pager(ui, Rect::from_min_size(pos2(gl, y), vec2(gw, 32.0)), y, filters.page, pages) {
                                    filters.page = n;
                                }
                                y += 32.0;
                            }
                        }
                    }
                }
            } else if tab == 0 {
                if blocks.is_empty() {
                    empty(ui, &mut y, "The Shop has nothing featured right now", "");
                }
                y += SPACE_24;
                feed(ui, &all, blocks, feed_left, &mut y, feed_w, &feed_state, &look, &mut acts);
                y += BLOCK_PAD;
            } else {
                y = top.y;
                orbs_hero(ui, orbs_blocks, top.x, &mut y, w, &mut acts);
                let feed_block_b = orbs_blocks.iter().find(|b| b["type"].as_u64() == Some(2)).cloned().unwrap_or(Value::Null);
                let title = "Orbs-Worthy Popular Picks";
                feed_block(ui, &all, &feed_block_b, title, gl, &mut y, gw, &feed_state, true, &look, &mut acts);
            }
            ui.allocate_exact_size(vec2(w, y - top.y + SPACE_32), Sense::hover());
        });
        scroll_state = (out.state.offset.y, out.content_size.y, out.inner_rect.height());
    });
    if small_filters && toggle_at != Rect::NOTHING {
        let (area, clr) = browse::popover(ui.ctx(), toggle_at, &mut filters, &query);
        cleared |= clr;
        let pressed = ui.ctx().input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten());
        if let Some(p) = pressed
            && !area.contains(p)
            && !toggle_at.contains(p)
        {
            filters.open = false;
        }
    }
    if cleared {
        filters.reset();
        search.clear();
    }
    if scroll_state.1 - scroll_state.0 - scroll_state.2 <= 200.0 && (tab == 0 || tab == 2) && feed_state.shown < FEED_MAX && scroll_state.1 > 0.0 {
        acts.push(Act::More);
    }
    if let Some(p) = open.as_deref().and_then(|id| find(&all, id)) {
        let cat = categories.iter().find(|c| c["sku_id"] == p["category_sku_id"]).and_then(|c| c["name"].as_str());
        if let Some(a) = product::detail(ui.ctx(), p, member, wishing.iter().any(|w| w == sku(p)), &avatar, cat, slide) {
            acts.push(match a {
                product::Act::Close => Act::Close,
                product::Act::Buy => Act::Buy(sku(p).to_string()),
                product::Act::Wish => Act::Wish(sku(p).to_string()),
                product::Act::Collection => Act::Collection(p["category_sku_id"].as_str().map(str::to_string)),
                product::Act::Pick(i) => Act::Pick(i),
            });
        }
    }
    let found = open.as_deref().is_none_or(|id| find(&all, id).is_some());
    if let Some(s) = &mut app.shop {
        let typed = s.search != search;
        s.search = search;
        s.browse = filters;
        if typed && !s.search.trim().is_empty() {
            s.tab = 1;
            s.collection = None;
        }
        if !found {
            s.product = None;
        }
    }
    apply(app, acts);
}

fn apply(app: &mut App, acts: Vec<Act>) {
    for a in acts {
        match a {
            Act::Go(t) => {
                if let Some(s) = &mut app.shop {
                    s.tab = t;
                    s.collection = None;
                    s.search.clear();
                    s.browse.reset();
                }
            }
            Act::Open(id) => {
                if let Some(s) = &mut app.shop {
                    s.product = Some(id);
                    s.slide = 0;
                }
            }
            Act::Close => {
                if let Some(s) = &mut app.shop {
                    s.product = None;
                }
            }
            Act::Only(i) => {
                if let Some(s) = &mut app.shop {
                    s.tab = 1;
                    s.collection = None;
                    s.search.clear();
                    s.browse.only(i);
                }
            }
            Act::Collabs => {
                if let Some(s) = &mut app.shop {
                    s.tab = 1;
                    s.collection = None;
                    s.search.clear();
                    s.browse.only_collabs();
                }
            }
            Act::Collection(c) => {
                if let Some(s) = &mut app.shop {
                    s.tab = 1;
                    s.product = None;
                    s.search.clear();
                    s.browse.reset();
                    s.collection = c;
                }
            }
            Act::Pick(i) => {
                if let Some(s) = &mut app.shop {
                    s.slide = i;
                }
            }
            Act::Heart => wish::open(app),
            Act::FeedSort(i) => {
                if let Some(s) = &mut app.shop {
                    s.feed_sort = i;
                }
            }
            Act::Shuffle => {
                if let Some(s) = &mut app.shop {
                    s.feed_sort = 3;
                    s.shuffle += 1;
                }
            }
            Act::More => {
                if let Some(s) = &mut app.shop {
                    s.feed_shown = (s.feed_shown + FEED_STEP).min(FEED_MAX);
                }
            }
            Act::Quests => app.open_quests(),
            Act::Wish(id) => wish::toggle(app, id),
            Act::Buy(id) => buy(app, id),
        }
    }
}

fn buy(app: &mut App, id: String) {
    if let Some(p) = products(app).into_iter().find(|p| sku(p) == id).cloned() {
        app.open_buy(crate::ui::buy::What::Shop(p));
    }
}
