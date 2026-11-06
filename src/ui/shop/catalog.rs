use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Mesh, Rect, ScrollArea, Sense, Shape, Stroke, TextEdit, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::product::{self, Hit, kind, sku};
use super::{NIGHT, PLATE, button, get, list, pic, round_button, want};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;

const HOME: &str = "/collectibles-shop?tab=home&include_bundles=true&include_dynamic_blocks=true&variants_return_style=2&skip_num_categories=0";
const CATS: &str = "/collectibles-categories/v2?include_bundles=true&variants_return_style=2&skip_num_categories=0";
const TABS: [&str; 3] = ["Featured", "Browse", "Orbs Exclusives"];
const FILTERS: [&str; 5] = ["Avatar Decorations", "Nameplates", "Profile Effects", "Bundles", "Orbs Eligible"];
const GAP: f32 = 16.0;

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

fn fade(ui: &Ui, r: Rect, from: u8, to: u8, left_to_right: bool) {
    let mut m = Mesh::default();
    let (a, b) = (Color32::from_rgba_unmultiplied(11, 10, 18, from), Color32::from_rgba_unmultiplied(11, 10, 18, to));
    let colors = if left_to_right { [a, b, b, a] } else { [a, a, b, b] };
    for (p, c) in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].into_iter().zip(colors) {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(m));
}

fn chip(ui: &mut Ui, r: Rect, label: &str, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("shop_chip", key)), Sense::click());
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 }), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(14.0), pal().header_primary);
    mark(ui, &format!("shop:{key}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn cards(ui: &mut Ui, left: f32, y: &mut f32, w: f32, min: f32, h: f32, items: &[&Value], look: &Ctx, acts: &mut Vec<Act>) {
    let cols = (((w + GAP) / (min + GAP)).floor() as usize).clamp(1, 4);
    let cw = (w - (cols - 1) as f32 * GAP) / cols as f32;
    let view = ui.clip_rect().expand(40.0);
    for row in items.chunks(cols) {
        if *y + h >= view.top() && *y <= view.bottom() {
            for (i, p) in row.iter().enumerate() {
                let r = Rect::from_min_size(pos2(left + i as f32 * (cw + GAP), *y), vec2(cw, h));
                let id = sku(p).to_string();
                match product::card(ui, r, p, look.member, look.wish.contains(&id), look.avatar) {
                    Some(Hit::Open) => acts.push(Act::Open(id)),
                    Some(Hit::Wish) => acts.push(Act::Wish(id)),
                    None => {}
                }
            }
        }
        *y += h + GAP;
    }
}

fn find<'a>(all: &[&'a Value], id: &str) -> Option<&'a Value> {
    all.iter().find(|p| sku(p) == id).copied()
}

fn featured(ui: &mut Ui, app_all: &[&Value], blocks: &[Value], left: f32, y: &mut f32, cw: f32, look: &Ctx, acts: &mut Vec<Act>) {
    for b in blocks {
        match b["type"].as_u64() {
            Some(0) => {
                let skus: Vec<&Value> = b["ranked_sku_ids"].as_array().into_iter().flatten().filter_map(|s| find(app_all, s.as_str().unwrap_or_default())).collect();
                let h = if cw > 700.0 { 640.0 } else { 520.0 };
                let r = Rect::from_min_size(pos2(left, *y), vec2(cw, h));
                ui.painter().rect_filled(r, CornerRadius::same(24), PLATE);
                if let Some(u) = b["hero_banner_url"].as_str() {
                    pic(ui, u, r, 24, true);
                }
                fade(ui, Rect::from_min_max(pos2(r.left() + 1.0, r.bottom() - 330.0), r.max - vec2(1.0, 1.0)), 0, 235, false);
                if let Some(u) = b["hero_logo_url"].as_str() {
                    pic(ui, u, Rect::from_min_size(r.min + vec2(40.0, 36.0), vec2(300.0, 76.0)), 0, false);
                }
                let name = b["name"].as_str().unwrap_or_default();
                let summary = b["summary"].as_str().unwrap_or_default().trim();
                ui.painter().text(r.min + vec2(40.0, 136.0), Align2::LEFT_TOP, name, semi(36.0), Color32::WHITE);
                if !summary.is_empty() {
                    ui.painter().text(r.min + vec2(40.0, 186.0), Align2::LEFT_TOP, summary, reg(16.0), Color32::from_gray(0xe0));
                }
                let head = r.bottom() - 330.0 + 18.0;
                ui.painter().text(pos2(r.left() + 28.0, head + 20.0), Align2::LEFT_CENTER, "Eligible items", semi(20.0), Color32::WHITE);
                let id = b["category_sku_id"].as_str().map(str::to_string);
                if button(ui, Rect::from_min_size(pos2(r.right() - 28.0 - 168.0, head + 2.0), vec2(168.0, 36.0)), "Shop the Collection", Color32::WHITE, Color32::BLACK, "collection") {
                    acts.push(Act::Collection(id));
                    acts.push(Act::Tab(1));
                }
                let mut cy = head + 52.0;
                cards(ui, r.left() + 28.0, &mut cy, cw - 56.0, 190.0, 240.0, &skus.iter().take(4).copied().collect::<Vec<_>>(), look, acts);
                *y += h + 32.0;
            }
            Some(1) => {
                let tiles = b["subblocks"].as_array().map_or(&[][..], |a| a.as_slice());
                let cols = if cw > 700.0 { 2 } else { 1 };
                let tw = (cw - (cols - 1) as f32 * GAP) / cols as f32;
                for row in tiles.chunks(cols) {
                    for (i, t) in row.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(left + i as f32 * (tw + GAP), *y), vec2(tw, 200.0));
                        let resp = ui.interact(r, Id::new(("shop_tile", t["category_sku_id"].as_str(), r.min.y as i32, i)), Sense::click());
                        ui.painter().rect_filled(r, CornerRadius::same(16), PLATE);
                        if let Some(u) = t["asset_url"].as_str() {
                            pic(ui, u, r, 16, true);
                        }
                        ui.painter().text(r.left_bottom() + vec2(20.0, -20.0), Align2::LEFT_BOTTOM, t["name"].as_str().unwrap_or_default(), bold(20.0), Color32::WHITE);
                        mark(ui, &format!("shop:tile:{}", t["name"].as_str().unwrap_or_default()), r);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            acts.push(Act::Collection(t["category_sku_id"].as_str().map(str::to_string)));
                            acts.push(Act::Tab(1));
                        }
                    }
                    *y += 200.0 + GAP;
                }
                *y += 16.0;
            }
            Some(4) => {
                let skus: Vec<&Value> = b["ranked_sku_ids"].as_array().into_iter().flatten().filter_map(|s| find(app_all, s.as_str().unwrap_or_default())).collect();
                if skus.is_empty() {
                    continue;
                }
                ui.painter().text(pos2(left, *y + 16.0), Align2::LEFT_CENTER, b["name"].as_str().unwrap_or_default(), semi(24.0), Color32::WHITE);
                if b["show_button"].as_bool() == Some(true) {
                    let label = b["button_text"].as_str().unwrap_or("Shop all");
                    if button(ui, Rect::from_min_size(pos2(left + cw - 150.0, *y), vec2(150.0, 36.0)), label, Color32::WHITE, Color32::BLACK, "block_button") {
                        acts.push(Act::Collection(b["category_sku_id"].as_str().map(str::to_string)));
                        acts.push(Act::Tab(1));
                    }
                }
                *y += 52.0;
                cards(ui, left, y, cw, 260.0, 250.0, &skus.iter().take(8).copied().collect::<Vec<_>>(), look, acts);
                *y += 16.0;
            }
            _ => {}
        }
    }
}

fn banner(ui: &mut Ui, left: f32, y: f32, w: f32, cat: &Value) {
    let r = Rect::from_min_size(pos2(left, y), vec2(w, 180.0));
    if !r.intersects(ui.clip_rect().expand(40.0)) {
        return;
    }
    ui.painter().rect_filled(r, CornerRadius::same(16), PLATE);
    if let Some(u) = cat["catalog_banner_url"].as_str().or(cat["hero_banner_url"].as_str()) {
        pic(ui, u, r, 16, true);
    }
    if let Some(u) = cat["logo_url"].as_str().or(cat["hero_logo_url"].as_str()) {
        pic(ui, u, Rect::from_min_size(r.min + vec2(24.0, 20.0), vec2(220.0, 60.0)), 0, false);
    }
    ui.painter().text(r.left_bottom() + vec2(24.0, -18.0), Align2::LEFT_BOTTOM, cat["name"].as_str().unwrap_or_default(), bold(20.0), Color32::WHITE);
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
                x = tr.right() + 24.0;
            }
            let mut right = r.right();
            if let Some(n) = orbs {
                let g = ui.painter().layout_no_wrap(n.to_string(), semi(16.0), pal().header_primary);
                let chip = Rect::from_min_size(pos2(right - g.size().x - 52.0, r.center().y - 16.0), vec2(g.size().x + 52.0, 32.0));
                ui.painter().rect(chip, 8, Color32::from_rgba_unmultiplied(151, 151, 159, 31), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(chip.left() + 20.0, chip.center().y), vec2(20.0, 20.0)), pal().header_primary);
                ui.painter().galley(pos2(chip.left() + 38.0, chip.center().y - g.size().y / 2.0), g, pal().header_primary);
                mark(ui, "shop:orbs", chip);
                right = chip.left() - 8.0;
            }
            let heart = Rect::from_min_size(pos2(right - 32.0, r.center().y - 16.0), vec2(32.0, 32.0));
            if round_button(ui, heart, Icon::Heart, pal().header_primary, "wishlist") {
                *wishlist = true;
            }
            let room = (heart.left() - 8.0 - x - 16.0).min(218.0);
            if room >= 96.0 {
                let field = Rect::from_min_size(pos2(heart.left() - 8.0 - room, r.center().y - 16.0), vec2(room, 32.0));
                ui.painter().rect(field, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(16.0, 16.0)), pal().text_muted);
                let inner = Rect::from_min_max(pos2(field.left() + 30.0, field.top() + 2.0), pos2(field.right() - 8.0, field.bottom() - 2.0));
                let edit = TextEdit::singleline(search).frame(Frame::NONE).hint_text("Search the Shop").font(reg(14.0)).desired_width(inner.width()).vertical_align(egui::Align::Center);
                ui.put(inner, edit);
                mark(ui, "shop:search", field);
            }
        });
}

fn filters(ui: &mut Ui, r: Rect, only: &[bool; 5], acts: &mut Vec<Act>) {
    ui.painter().text(r.min + vec2(0.0, 12.0), Align2::LEFT_CENTER, "Show only", semi(18.0), pal().header_primary);
    for (i, name) in FILTERS.iter().enumerate() {
        let row = Rect::from_min_size(r.min + vec2(0.0, 36.0 + i as f32 * 42.0), vec2(r.width(), 36.0));
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
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let top = ui.cursor().min;
            let cw = (w - 64.0).clamp(280.0, 1080.0);
            let left = top.x + (w - cw) / 2.0;
            let mut y = top.y + 28.0;
            if !ready {
                ui.add_space(120.0);
                ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                return;
            }
            let heading = |ui: &Ui, y: &mut f32, text: &str| {
                ui.painter().text(pos2(left, *y + 16.0), Align2::LEFT_CENTER, text, semi(24.0), Color32::WHITE);
                *y += 48.0;
            };
            let empty = |ui: &Ui, y: &mut f32, text: &str| {
                ui.painter().text(pos2(left + cw / 2.0, *y + 60.0), Align2::CENTER_CENTER, text, semi(20.0), pal().header_secondary);
                *y += 140.0;
            };
            if !query.is_empty() || tab == 1 {
                let side = cw >= 760.0 && query.is_empty();
                let gw = if side { cw - 300.0 } else { cw };
                if side {
                    filters(ui, Rect::from_min_size(pos2(left + gw + 20.0, y), vec2(260.0, 300.0)), &only, &mut acts);
                } else if query.is_empty() {
                    let mut x = left;
                    for (i, name) in FILTERS.iter().enumerate() {
                        let g = ui.painter().layout_no_wrap(name.to_string(), medium(14.0), Color32::WHITE);
                        let r = Rect::from_min_size(pos2(x, y), vec2(g.size().x + 28.0, 32.0));
                        if x + r.width() > left + cw {
                            break;
                        }
                        if chip(ui, r, name, &format!("only:{name}")) {
                            acts.push(Act::Only(i));
                        }
                        x = r.right() + 8.0;
                    }
                    y += 48.0;
                }
                if let Some(c) = collection.as_deref().filter(|_| query.is_empty()) {
                    let name = categories.iter().find(|x| x["sku_id"] == c).and_then(|x| x["name"].as_str()).unwrap_or("Collection");
                    if chip(ui, Rect::from_min_size(pos2(left, y), vec2(name.chars().count() as f32 * 8.0 + 56.0, 32.0)), &format!("{name}  ✕"), "collection_clear") {
                        acts.push(Act::Collection(None));
                    }
                    y += 48.0;
                }
                if !query.is_empty() {
                    heading(ui, &mut y, &format!("Results for “{}”", search.trim()));
                    let hits: Vec<&Value> = all.iter().copied().filter(|p| p["name"].as_str().is_some_and(|n| n.to_lowercase().contains(&query))).collect();
                    if hits.is_empty() {
                        empty(ui, &mut y, "No results");
                    }
                    cards(ui, left, &mut y, gw, 240.0, 245.0, &hits, &look, &mut acts);
                } else {
                    let mut shown = 0;
                    for c in categories.iter().filter(|c| collection.as_deref().is_none_or(|k| c["sku_id"] == k)) {
                        let prods: Vec<&Value> = c["products"].as_array().into_iter().flatten().filter(|p| pass(p, &only)).collect();
                        if prods.is_empty() {
                            continue;
                        }
                        shown += prods.len();
                        banner(ui, left, y, gw, c);
                        y += 180.0 + GAP;
                        cards(ui, left, &mut y, gw, 240.0, 245.0, &prods, &look, &mut acts);
                        y += 16.0;
                    }
                    if shown == 0 {
                        empty(ui, &mut y, "Nothing matches these filters");
                    }
                }
            } else if tab == 0 {
                if blocks.is_empty() {
                    empty(ui, &mut y, "The Shop has nothing featured right now");
                }
                featured(ui, &all, blocks, left, &mut y, cw, &look, &mut acts);
            } else if tab == 2 {
                heading(ui, &mut y, "Orbs Exclusives");
                let items: Vec<&Value> = all.iter().copied().filter(|p| product::orbs_only(p)).collect();
                if items.is_empty() {
                    empty(ui, &mut y, "No Orbs exclusives right now");
                }
                cards(ui, left, &mut y, cw, 240.0, 245.0, &items, &look, &mut acts);
            } else {
                heading(ui, &mut y, "Your Wishlist");
                let items: Vec<&Value> = wish.iter().filter_map(|id| find(&all, id)).collect();
                if items.is_empty() {
                    empty(ui, &mut y, "Heart an item to save it here");
                }
                cards(ui, left, &mut y, cw, 240.0, 245.0, &items, &look, &mut acts);
            }
            ui.allocate_exact_size(vec2(w, y - top.y + 40.0), Sense::hover());
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
    let url = format!("/shop#itemSkuId={id}");
    if std::env::var_os("RUSTCORD_E2E").is_none() {
        crate::ui::settings::open_web(&url);
    }
    if let Some(s) = &mut app.shop {
        s.bought = Some(url);
    }
}
