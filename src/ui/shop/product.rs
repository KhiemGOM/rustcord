use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::{CARD_MIN, PLATE, RADIUS_LG, RADIUS_SM, SPACE_12, SPACE_16, SPACE_20, SPACE_24, SPACE_32, SPACE_8, button, pic, shade};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{billing, mark};

pub struct Price {
    pub money: String,
    pub orbs: Option<u64>,
    pub off: Option<u32>,
    pub was: Option<String>,
}

pub fn sku(p: &Value) -> &str {
    p["sku_id"].as_str().unwrap_or_default()
}

pub fn kind(p: &Value) -> u64 {
    p["type"].as_u64().unwrap_or(0)
}

pub fn label(p: &Value) -> &'static str {
    match kind(p) {
        0 => "Avatar Decoration",
        1 => "Profile Effect",
        2 => "Nameplate",
        1000 => "Bundle",
        3000 => "Nitro Credit",
        _ => "Avatar Decoration Variant",
    }
}

fn amounts(p: &Value, key: &str) -> (Option<(i64, String)>, Option<u64>) {
    let list = p["prices"][key]["country_prices"]["prices"].as_array().map_or(&[][..], |a| a.as_slice());
    let money = list.iter().find(|x| x["currency"] != "discord_orb").map(|x| (x["amount"].as_i64().unwrap_or(0), x["currency"].as_str().unwrap_or("usd").to_string()));
    let orbs = list.iter().find(|x| x["currency"] == "discord_orb").and_then(|x| x["amount"].as_u64());
    (money, orbs)
}

pub fn price(p: &Value, member: bool) -> Price {
    let (base, base_orbs) = amounts(p, "0");
    let (mine, mine_orbs) = if member { amounts(p, "4") } else { (None, None) };
    let shown = mine.clone().or(base.clone());
    let off = match (&base, &mine) {
        (Some((a, _)), Some((b, _))) if b < a && *a > 0 => Some(((a - b) as f64 / *a as f64 * 100.0).round() as u32),
        _ => None,
    };
    let was = off.and(base.as_ref()).map(|(a, c)| billing::money(*a, c));
    Price { money: shown.map_or(String::new(), |(a, c)| billing::money(a, &c)), orbs: mine_orbs.or(base_orbs), off, was }
}

pub fn orbs_only(p: &Value) -> bool {
    amounts(p, "0").1.is_some()
}

pub fn summary(p: &Value) -> String {
    let joined = p["bundled_products"].as_array().map(|a| a.iter().filter_map(|b| b["name"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    p["summary"].as_str().unwrap_or_default().replace("{joinedItems}", &joined).trim().to_string()
}

fn hex(c: &Value, at: usize, fallback: Color32) -> Color32 {
    c.get(at).and_then(|v| v.as_u64()).map_or(fallback, |n| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

pub fn backdrop(p: &Value) -> (Color32, Color32) {
    let c = &p["styles"]["background_colors"];
    (hex(c, 0, Color32::from_rgb(0x18, 0x19, 0x1c)), hex(c, 1, Color32::from_rgb(0x1e, 0x1f, 0x22)))
}

pub fn accent(p: &Value) -> Color32 {
    hex(&p["styles"]["button_colors"], 0, BLURPLE)
}

fn cdn_asset(path: &str) -> String {
    format!("{}/assets/collectibles/{path}", crate::api::cdn())
}

fn ring(ui: &Ui, c: Pos2, avatar: &str, d: f32) {
    let face = Rect::from_center_size(c, vec2(d, d));
    ui.painter().circle_filled(c, d / 2.0, Color32::from_rgb(0x2b, 0x2d, 0x31));
    if !avatar.is_empty() {
        crate::ui::avatar(ui, face, avatar);
    }
}

pub fn preview(ui: &Ui, r: Rect, p: &Value, avatar: &str) {
    let item = &p["items"][0];
    let c = r.center();
    let k = r.width().min(r.height());
    match kind(p) {
        0 => {
            let d = k * 0.5;
            let at = c - vec2(0.0, k * 0.04);
            ring(ui, at, avatar, d);
            if let Some(a) = item["asset"].as_str() {
                pic(ui, &format!("{}/avatar-decoration-presets/{a}.png?size=240&passthrough=false", crate::api::cdn()), Rect::from_center_size(at, vec2(d * 1.2, d * 1.2)), 0, false);
            }
        }
        2 => {
            let w = r.width() * 0.86;
            let h = w * 0.17;
            let row = Rect::from_center_size(c, vec2(w, h));
            for dy in [-1.4 * h, 1.4 * h] {
                let ghost = row.translate(vec2(0.0, dy));
                ui.painter().circle_filled(pos2(ghost.left() + h * 0.5, ghost.center().y), h * 0.38, Color32::from_white_alpha(14));
                ui.painter().rect_filled(Rect::from_min_size(pos2(ghost.left() + h * 1.2, ghost.center().y - h * 0.12), vec2(w * 0.5, h * 0.24)), CornerRadius::same((h * 0.12) as u8), Color32::from_white_alpha(14));
            }
            if let Some(a) = item["asset"].as_str() {
                pic(ui, &cdn_asset(&format!("{a}static.png")), row, RADIUS_SM, true);
            }
            ring(ui, pos2(row.left() + h * 0.55, row.center().y), avatar, h * 0.66);
            ui.painter().rect_filled(Rect::from_min_size(pos2(row.left() + h * 1.2, row.center().y - h * 0.1), vec2(w * 0.4, h * 0.2)), CornerRadius::same((h * 0.1) as u8), Color32::from_white_alpha(60));
        }
        1 => {
            let card = Rect::from_center_size(c, vec2(k * 0.62, k * 0.78));
            ui.painter().rect_filled(card, CornerRadius::same(RADIUS_SM), Color32::from_black_alpha(120));
            ring(ui, pos2(card.left() + card.width() * 0.26, card.top() + card.height() * 0.24), avatar, card.width() * 0.3);
            for (i, f) in [0.7, 0.45].into_iter().enumerate() {
                ui.painter().rect_filled(Rect::from_min_size(pos2(card.left() + card.width() * 0.09, card.top() + card.height() * (0.5 + 0.08 * i as f32)), vec2(card.width() * f, card.height() * 0.04)), CornerRadius::same(4), Color32::from_white_alpha(28));
            }
            if let Some(u) = item["thumbnailPreviewSrc"].as_str().filter(|u| !u.is_empty()) {
                pic(ui, u, r, 0, false);
            }
        }
        1000 => {
            let a = &p["preview_assets"];
            if let Some(u) = a["bg_static"].as_str() {
                pic(ui, u, r, 0, true);
            }
            if let Some(u) = a["fg_static"].as_str() {
                pic(ui, u, r, 0, false);
            }
        }
        _ => icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(c, vec2(k * 0.3, k * 0.3)), Color32::from_gray(0xdd)),
    }
}

fn price_row(ui: &Ui, at: Pos2, p: &Price, size: f32) {
    let painter = ui.painter();
    icons::draw(painter, Icon::Nitro, Rect::from_center_size(pos2(at.x + 8.0, at.y), vec2(16.0, 16.0)), Color32::WHITE);
    let text = if p.money.is_empty() { p.orbs.map_or(String::new(), |o| format!("{o} Orbs")) } else { p.money.clone() };
    let t = painter.text(pos2(at.x + 24.0, at.y), Align2::LEFT_CENTER, text, bold(size), Color32::WHITE);
    if let Some(off) = p.off {
        painter.text(pos2(t.right() + 6.0, at.y), Align2::LEFT_CENTER, format!("(-{off}%)"), bold(size), GREEN);
    }
}

pub enum Hit {
    Open,
    Wish,
}

const BORDER: f32 = 2.0;
const FOOTER_LIFT: f32 = 56.0;
const FOOTER_SHADE: f32 = 146.0;
const BUNDLE_SHADE: f32 = 100.0;

pub fn card(ui: &mut Ui, r: Rect, salt: &str, p: &Value, member: bool, wished: bool, avatar: &str) -> Option<Hit> {
    let id = sku(p).to_string();
    let resp = ui.interact(r, Id::new(("shop_card", salt, &id)), Sense::click());
    let (a, b) = backdrop(p);
    let clip = ui.painter().with_clip_rect(r.intersect(ui.clip_rect()));
    clip.rect_filled(r, CornerRadius::same(RADIUS_SM), PLATE);
    clip.rect_filled(r, CornerRadius::same(RADIUS_SM), a.lerp_to_gamma(b, 0.5).gamma_multiply(0.35));
    let inner = r.shrink(BORDER);
    let bundle = kind(p) == 1000;
    let area = if bundle { inner } else { Rect::from_min_max(pos2(inner.left() + SPACE_8, inner.top()), pos2(inner.right() - SPACE_8, inner.bottom() - FOOTER_LIFT)) };
    preview(ui, area, p, avatar);
    let tall = if bundle { BUNDLE_SHADE } else { FOOTER_SHADE }.min(inner.height());
    shade(ui, Rect::from_min_max(pos2(inner.left(), inner.bottom() - tall), inner.max), Color32::BLACK, 0, 150);
    let price_y = inner.bottom() - SPACE_12 - 10.0;
    clip.text(pos2(inner.left() + SPACE_12, price_y - 26.0), Align2::LEFT_CENTER, p["name"].as_str().unwrap_or_default(), medium(16.0), Color32::WHITE);
    price_row(ui, pos2(inner.left() + SPACE_12, price_y), &price(p, member), 16.0);
    let edge = if resp.hovered() { Color32::from_white_alpha(46) } else { Color32::from_white_alpha(22) };
    ui.painter().rect_stroke(r, CornerRadius::same(RADIUS_SM), Stroke::new(BORDER, edge), egui::StrokeKind::Inside);
    mark(ui, &format!("shop:card:{id}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let heart = Rect::from_min_size(pos2(inner.right() - SPACE_12 - 32.0, inner.top() + SPACE_12), vec2(32.0, 32.0));
    if ui.rect_contains_pointer(r) || wished {
        let hr = ui.interact(heart, Id::new(("shop_heart", salt, &id)), Sense::click());
        ui.painter().rect_filled(heart, CornerRadius::same(RADIUS_LG), Color32::from_black_alpha(if hr.hovered() { 170 } else { 130 }));
        icons::draw(ui.painter(), Icon::Heart, Rect::from_center_size(heart.center(), vec2(16.0, 16.0)), if wished { RED } else { Color32::WHITE });
        mark(ui, &format!("shop:heart:{id}"), heart);
        if hr.clicked() {
            return Some(Hit::Wish);
        }
        if hr.hovered() {
            return None;
        }
    }
    resp.clicked().then_some(Hit::Open)
}

pub enum Act {
    Close,
    Buy,
    Wish,
}

pub fn detail(ctx: &egui::Context, p: &Value, member: bool, wished: bool, avatar: &str) -> Option<Act> {
    let mut out = None;
    let screen = ctx.content_rect();
    let side = (0.455 * screen.width()).clamp(300.0, 400.0);
    let avail = screen.width() - 2.0 * SPACE_32;
    let wide = avail >= side + SPACE_32 + CARD_MIN + 2.0 * SPACE_24;
    let w = if wide { (2.0 * side + SPACE_32 + 2.0 * SPACE_24).min(avail) } else { avail.min(side + 2.0 * SPACE_24) };
    let shown = crate::ui::motion::Modal::new(Id::new("shop_detail")).frame(egui::Frame::new().fill(pal().bg_secondary).corner_radius(RADIUS_LG)).show(ctx, |ui| {
        ui.set_width(w);
        let pad = SPACE_24;
        let pv = if wide { vec2(side, side) } else { vec2(w - 2.0 * pad, (w - 2.0 * pad).min(screen.height() * 0.4)) };
        let (box_, _) = ui.allocate_exact_size(vec2(w, pv.y + 2.0 * pad), Sense::hover());
        let left = Rect::from_min_size(box_.min + vec2(pad, pad), pv);
        let (a, b) = backdrop(p);
        ui.painter().rect_filled(left, CornerRadius::same(RADIUS_LG), a.lerp_to_gamma(b, 0.5));
        preview(ui, left, p, avatar);
        let x = if wide { left.right() + SPACE_32 } else { left.left() };
        let mut y = if wide { left.top() } else { left.bottom() + SPACE_16 };
        let tw = box_.right() - pad - x;
        let painter = ui.painter().clone();
        painter.text(pos2(x, y), Align2::LEFT_TOP, label(p), medium(12.0), pal().text_muted);
        y += SPACE_24 - 2.0;
        painter.text(pos2(x, y), Align2::LEFT_TOP, p["name"].as_str().unwrap_or_default(), semi(24.0), pal().header_primary);
        y += SPACE_32 + 6.0;
        let g = painter.layout(summary(p), reg(14.0), pal().header_secondary, tw);
        let gh = g.size().y;
        painter.galley(pos2(x, y), g, pal().header_secondary);
        y += gh + SPACE_12;
        if let Some(items) = p["bundled_products"].as_array() {
            for it in items.iter().take(6) {
                painter.text(pos2(x, y), Align2::LEFT_TOP, format!("•  {}  ·  {}", it["name"].as_str().unwrap_or_default(), label(it)), reg(14.0), pal().header_secondary);
                y += SPACE_20;
            }
        }
        let pr = price(p, member);
        let foot = 40.0 + SPACE_8 + 20.0 + SPACE_12;
        let bottom = if wide { left.bottom() } else { y + SPACE_16 + foot };
        price_row(ui, pos2(x, bottom - foot - 8.0), &pr, 20.0);
        if let Some(was) = &pr.was {
            painter.text(pos2(x + 24.0, bottom - foot + 14.0), Align2::LEFT_CENTER, format!("was {was}"), reg(12.0), pal().text_muted);
        }
        if let Some(o) = pr.orbs.filter(|_| !pr.money.is_empty()) {
            painter.text(pos2(x + 24.0, bottom - foot + 30.0), Align2::LEFT_CENTER, format!("or {o} Orbs"), reg(12.0), pal().text_muted);
        }
        let buy = Rect::from_min_size(pos2(x, bottom - 40.0), vec2((tw - 40.0 - SPACE_8).max(96.0), 40.0));
        if button(ui, buy, "Buy", accent(p), Color32::WHITE, "buy") {
            out = Some(Act::Buy);
        }
        let heart = Rect::from_min_size(pos2(buy.right() + SPACE_8, buy.top()), vec2(40.0, 40.0));
        let hr = ui.interact(heart, Id::new("shop_detail_heart"), Sense::click());
        painter.rect_filled(heart, CornerRadius::same(RADIUS_SM), Color32::from_rgba_unmultiplied(151, 151, 159, if hr.hovered() { 51 } else { 31 }));
        icons::draw(&painter, Icon::Heart, Rect::from_center_size(heart.center(), vec2(18.0, 18.0)), if wished { RED } else { pal().header_primary });
        mark(ui, "shop:detail_heart", heart);
        if hr.clicked() {
            out = Some(Act::Wish);
        }
        let h = (bottom + pad - box_.top()).max(box_.height());
        ui.allocate_space(vec2(0.0, h - box_.height()));
    });
    if shown.should_close() {
        out = Some(Act::Close);
    }
    out
}
