//! A server's own shop, `/channels/{id}/shop`: its published products as cards. Discord's
//! client reads them from `/guilds/{id}/products/listings`; a card opens the same checkout
//! dialog as everything else.

use egui::{Align2, CornerRadius, Frame, Margin, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::Value;

use super::What;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{billing, mark};

const GAP: f32 = 16.0;
const CARD_MIN: f32 = 246.0;
const CARD_H: f32 = 200.0;
const MAX_W: f32 = 1124.0;

pub struct ServerShop {
    pub guild: Id,
    pub listings: Option<Result<Vec<Value>, String>>,
}

impl App {
    pub fn open_server_shop(&mut self, gid: Id) {
        self.boosts = None;
        self.home = None;
        self.server_shop = Some(ServerShop { guild: gid, listings: None });
        self.rest("GET", format!("/guilds/{gid}/products/listings"), None, Tag::Buy("listings", gid));
    }
}

pub fn loaded(app: &mut App, gid: Id, res: Result<Value, String>) {
    let Some(s) = app.server_shop.as_mut().filter(|s| s.guild == gid) else { return };
    s.listings = Some(res.map(|v| v["listings"].as_array().cloned().unwrap_or_default().into_iter().filter(|l| l["published_at"].is_string()).collect()));
}

fn price(l: &Value) -> String {
    l["price_tier"].as_i64().map_or(String::new(), |p| billing::money(p, "usd"))
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    egui::Panel::top("server_shop_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.left() - 16.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            icons::draw(ui.painter(), Icon::Shop, Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(20.0, 20.0)), pal().channel_default);
            ui.painter().text(pos2(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, "Server Shop", medium(16.0), pal().header_primary);
        });
    let mut buy: Option<Value> = None;
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        ScrollArea::vertical().id_salt(("server_shop", gid)).auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                let w = ui.available_width().min(MAX_W);
                ui.set_width(w);
                match app.server_shop.as_ref().and_then(|s| s.listings.as_ref()) {
                    None => {
                        crate::ui::motion::spinner(ui, 24.0, pal().header_secondary);
                    }
                    Some(Err(e)) => {
                        ui.label(egui::RichText::new(e).font(reg(14.0)).color(pal().critical));
                    }
                    Some(Ok(list)) if list.is_empty() => {
                        ui.label(egui::RichText::new("This server has nothing for sale yet.").font(reg(16.0)).color(pal().header_secondary));
                        mark(ui, "server_shop:empty", ui.min_rect());
                    }
                    Some(Ok(list)) => {
                        let cols = (((w + GAP) / (CARD_MIN + GAP)).floor() as usize).max(1);
                        let cw = (w - (cols - 1) as f32 * GAP) / cols as f32;
                        for row in list.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = GAP;
                                for l in row {
                                    if card(ui, cw, l) {
                                        buy = Some(l.clone());
                                    }
                                }
                            });
                            ui.add_space(GAP);
                        }
                    }
                }
            });
        });
    });
    if let Some(l) = buy {
        app.open_buy(What::Listing(l));
    }
}

fn card(ui: &mut Ui, w: f32, l: &Value) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, CARD_H), Sense::CLICK);
    let p = ui.painter();
    p.rect(r, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
    icons::draw(p, Icon::Shop, Rect::from_center_size(pos2(r.center().x, r.top() + 56.0), vec2(40.0, 40.0)), pal().header_secondary);
    let name = l["name"].as_str().unwrap_or_default();
    crate::ui::text::paint(p, pos2(r.left() + 16.0, r.top() + 116.0), Align2::LEFT_CENTER, name, semi(16.0), pal().header_primary, w - 32.0);
    let g = p.layout(l["description"].as_str().unwrap_or_default().to_string(), reg(12.0), pal().header_secondary, w - 32.0);
    let h = g.size().y.min(32.0);
    p.with_clip_rect(Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 132.0), vec2(w - 32.0, h))).galley(pos2(r.left() + 16.0, r.top() + 132.0), g, pal().header_secondary);
    p.text(pos2(r.left() + 16.0, r.bottom() - 20.0), Align2::LEFT_CENTER, price(l), semi(16.0), pal().header_primary);
    mark(ui, &format!("server_shop:card:{}", l["id"].as_str().unwrap_or_default()), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
