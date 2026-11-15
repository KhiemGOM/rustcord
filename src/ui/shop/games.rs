//! Game Shops: the Shop header's dropdown of the games that sell through Discord, and a game's
//! storefront with its pages, sections and item cards.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::catalog::{self, Act};
use super::{NIGHT, PLATE, get, want};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::settings::mark;
use crate::ui::{App, menu_row_at};

const APPS: &str = "game_shop_apps";

fn fronts(app: &App) -> Vec<String> {
    let cfg = get(app, "shop_fronts");
    cfg.and_then(|c| c["storefronts"].as_array()).into_iter().flatten().filter(|s| s["collectibles_shop_navigation_enabled"] == true).filter_map(|s| s["application_id"].as_str().map(str::to_string)).collect()
}

pub fn fetch(app: &mut App) {
    want(app, "shop_fronts", "/partner-sdk/storefront-config");
    let ids = fronts(app);
    if !ids.is_empty() && !app.keyed.contains_key(APPS) {
        app.keyed.insert(APPS.into(), None);
        let q: Vec<String> = ids.iter().map(|i| format!("application_ids={i}")).collect();
        app.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::Keyed(APPS.into()));
    }
}

/// The games with a shop here, in the config's order: id, name, icon.
pub fn shops(app: &App) -> Vec<(String, String, Option<String>)> {
    let Some(Some(Ok(apps))) = app.keyed.get(APPS) else { return Vec::new() };
    let apps = apps.as_array().map_or(&[][..], |a| a.as_slice());
    fronts(app)
        .into_iter()
        .filter_map(|id| {
            let a = apps.iter().find(|a| a["id"] == id.as_str())?;
            let icon = a["icon"].as_str().map(|h| format!("{}/app-icons/{id}/{h}.png?size=32", crate::api::cdn()));
            Some((id, a["name"].as_str()?.to_string(), icon))
        })
        .collect()
}

/// The header's Game Shops tab at `x`; hands back where the next tab starts.
pub fn tab(app: &App, ui: &mut Ui, x: f32, r: Rect, on: bool, acts: &mut Vec<Act>) -> f32 {
    let list = shops(app);
    if list.is_empty() {
        return x;
    }
    let g = ui.painter().layout_no_wrap("Game Shops".into(), medium(16.0), pal().header_primary);
    let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x + 22.0, 48.0));
    let resp = ui.interact(tr, ui.id().with("shop_game_shops"), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let col = if on || open || resp.hovered() { pal().header_primary } else { pal().header_secondary };
    let w = g.size().x;
    ui.painter().galley(pos2(x, tr.center().y - g.size().y / 2.0), g, col);
    crate::ui::motion::chevron(ui, ui.id().with("shop_games_chev"), !open, Rect::from_center_size(pos2(tr.right() - 10.0, tr.center().y), vec2(20.0, 20.0)), col);
    if on {
        ui.painter().rect_filled(Rect::from_min_size(pos2(x, tr.bottom() - 2.0), vec2(w, 2.0)), 1, pal().header_primary);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, "shop:tab:Game Shops", tr);
    egui::Popup::menu(&resp).gap(0.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (id, name, icon) in &list {
            let (row, lead) = menu_row_at(ui, true, name, None, false, crate::ui::Trail::None);
            match icon {
                Some(u) => egui::Image::from_uri(u).corner_radius(4).show_loading_spinner(false).paint_at(ui, lead),
                None => {
                    ui.painter().rect_filled(lead, 4, pal().blurple);
                }
            }
            mark(ui, &format!("shop:game:{id}"), row.rect);
            if row.clicked() {
                acts.push(Act::Game(id.clone()));
            }
        }
    });
    tr.right() + super::SPACE_24
}

fn asset(app_id: &str, id: &Value, size: u32) -> Option<String> {
    id.as_str().map(|a| format!("{}/app-assets/{app_id}/{a}.png?size={size}", crate::api::cdn()))
}

fn price(sku: &Value) -> String {
    let p = &sku["price"];
    match (p["amount"].as_i64(), p["currency"].as_str()) {
        (Some(0), _) => "Free".into(),
        (Some(a), Some(c)) => crate::ui::settings::billing::money(a, c),
        _ => String::new(),
    }
}

fn card(ui: &mut Ui, r: Rect, app_id: &str, l: &Value) {
    let sku = &l["sku"];
    let meta = &sku["tenant_metadata"]["social_layer"];
    let resp = ui.interact(r, ui.id().with(("game_item", sku["id"].as_str().unwrap_or(""))), Sense::hover());
    ui.painter().rect(r, 12, PLATE, Stroke::new(1.0, if resp.hovered() { pal().border_normal } else { pal().border_subtle }), egui::StrokeKind::Inside);
    let art = Rect::from_min_size(r.min, vec2(r.width(), r.width()));
    let top = CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 };
    if let Some(bg) = asset(app_id, &meta["card_background_image_asset_id"], 256) {
        egui::Image::from_uri(bg).corner_radius(top).show_loading_spinner(false).paint_at(ui, art);
    }
    if let Some(pic) = asset(app_id, &meta["card_image_asset_id"], 256).or_else(|| asset(app_id, &l["thumbnail_asset_id"], 256)) {
        egui::Image::from_uri(pic).show_loading_spinner(false).paint_at(ui, art.shrink(12.0));
    }
    if let Some(tag) = meta["label"].as_str().filter(|t| !t.is_empty()) {
        let g = ui.painter().layout_no_wrap(tag.to_uppercase(), semi(10.0), Color32::WHITE);
        let chip = Rect::from_min_size(art.min + vec2(8.0, 8.0), g.size() + vec2(12.0, 6.0));
        ui.painter().rect_filled(chip, 4, Color32::from_black_alpha(160));
        ui.painter().galley(chip.min + vec2(6.0, 3.0), g, Color32::WHITE);
    }
    let name = sku["name"].as_str().unwrap_or("");
    crate::ui::text::paint(ui.painter(), pos2(r.left() + 12.0, art.bottom() + 18.0), Align2::LEFT_CENTER, name, medium(14.0), pal().header_primary, r.width() - 24.0);
    ui.painter().text(pos2(r.left() + 12.0, art.bottom() + 40.0), Align2::LEFT_CENTER, price(sku), reg(14.0), pal().text_normal);
    let about = sku["description"].as_str().or(l["summary"].as_str()).unwrap_or("");
    if !about.is_empty() {
        resp.on_hover_text(about);
    }
}

fn section(ui: &mut Ui, title: &str, ids: &[Value], listings: &[Value], app_id: &str) {
    let items: Vec<&Value> = ids.iter().filter_map(|id| listings.iter().find(|l| l["sku"]["id"] == *id)).collect();
    if items.is_empty() {
        return;
    }
    if !title.is_empty() {
        ui.label(egui::RichText::new(title).font(semi(20.0)).color(pal().header_primary));
        ui.add_space(12.0);
    }
    let w = ui.available_width();
    let cols = ((w + 16.0) / 196.0).floor().max(1.0) as usize;
    let cw = (w - 16.0 * (cols as f32 - 1.0)) / cols as f32;
    let ch = cw + 60.0;
    let rows = items.len().div_ceil(cols);
    let (grid, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (ch + 16.0) - 16.0), Sense::hover());
    for (i, l) in items.iter().enumerate() {
        let at = pos2(grid.left() + (i % cols) as f32 * (cw + 16.0), grid.top() + (i / cols) as f32 * (ch + 16.0));
        let r = Rect::from_min_size(at, vec2(cw, ch));
        if ui.is_rect_visible(r) {
            card(ui, r, app_id, l);
        }
    }
    ui.add_space(32.0);
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some((id, page)) = app.shop.as_ref().and_then(|s| s.game.clone().map(|g| (g, s.game_page))) else { return };
    let key = format!("game_shop:{id}");
    if !app.keyed.contains_key(&key) {
        app.keyed.insert(key.clone(), None);
        app.rest("GET", format!("/partner-sdk/applications/{id}/application-storefront"), None, Tag::Keyed(key.clone()));
    }
    let mut acts = Vec::new();
    let mut search = String::new();
    catalog::header(app, ui, 3, &mut search, &mut acts);
    let mut turn = None;
    egui::CentralPanel::default().frame(Frame::new().fill(NIGHT)).show(ui, |ui| match app.keyed.get(&key) {
        Some(Some(Ok(front))) => {
            let pages = front["pages"].as_array().map_or(&[][..], |a| a.as_slice());
            let listings = front["store_listings"].as_array().map_or(&[][..], |a| a.as_slice());
            let shown = pages.get(page.min(pages.len().saturating_sub(1)));
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(32, 24)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if let Some(logo) = asset(&id, &front["logo_asset_id"], 128) {
                            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                            egui::Image::from_uri(logo).show_loading_spinner(false).paint_at(ui, r);
                        }
                        let title = front["title"].as_str().or(front["application"]["name"].as_str()).unwrap_or("Game Shop");
                        ui.label(egui::RichText::new(title).font(display(24.0)).color(pal().header_primary));
                    });
                    ui.add_space(16.0);
                    if pages.len() > 1 {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for (i, p) in pages.iter().enumerate() {
                                let on = i == page;
                                let g = ui.painter().layout_no_wrap(p["title"].as_str().unwrap_or("").to_string(), medium(14.0), pal().header_primary);
                                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click());
                                ui.painter().rect_filled(r, 16, if on { pal().control_hover } else if resp.hovered() { pal().control } else { Color32::TRANSPARENT });
                                let ink = if on { pal().header_primary } else { pal().header_secondary };
                                ui.painter().galley(pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, ink);
                                mark(ui, &format!("game_shop:page:{i}"), r);
                                if resp.clicked() {
                                    turn = Some(i);
                                }
                            }
                        });
                        ui.add_space(24.0);
                    }
                    if let Some(p) = shown {
                        section(ui, "", p["sku_ids"].as_array().map_or(&[][..], |a| a.as_slice()), listings, &id);
                        for s in p["sections"].as_array().into_iter().flatten() {
                            section(ui, s["title"].as_str().unwrap_or(""), s["sku_ids"].as_array().map_or(&[][..], |a| a.as_slice()), listings, &id);
                        }
                    }
                });
            });
        }
        Some(Some(Err(_))) => {
            ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Something went wrong. Please try again.").font(reg(16.0)).color(pal().text_muted)));
        }
        _ => {
            ui.centered_and_justified(|ui| crate::ui::motion::spinner(ui, 32.0, pal().text_muted));
        }
    });
    if let (Some(i), Some(s)) = (turn, &mut app.shop) {
        s.game_page = i;
    }
    catalog::apply(app, acts);
}
