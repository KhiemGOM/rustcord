use egui::{Align, Align2, Color32, CornerRadius, Key, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::{Value, json};

use super::{Look, KEYS, act, card, confirm, end, faint, fetch, get, input, lead, mark, pill, put, refetch, remote, section, settle, small, solid};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

const TIERS: [(&str, u32, Color32); 6] = [
    ("Patron", 1, Color32::from_rgb(0x4d, 0xa3, 0xf5)),
    ("Champion", 2, Color32::from_rgb(0x3b, 0xc9, 0x9b)),
    ("Luminary", 3, Color32::from_rgb(0xd0, 0x6c, 0xe0)),
    ("Icon", 6, Color32::from_rgb(0x8e, 0x5c, 0xf0)),
    ("Hero", 10, Color32::from_rgb(0x5c, 0xd1, 0xb8)),
    ("Legend", 20, Color32::from_rgb(0xf2, 0xb2, 0x32)),
];

fn gifted(v: &Value) -> u32 {
    let p = &v["progress"];
    [&p["current"], &p["current_value"], &p["value"], &p["count"], p, &v["current"], &v["value"]]
        .iter()
        .find_map(|x| x.as_u64())
        .unwrap_or(0) as u32
}

fn clean(code: &str) -> String {
    let c = code.trim().rsplit('/').next().unwrap_or("");
    c.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect()
}

fn name_of(v: &Value) -> String {
    [&v["store_listing"]["sku"]["name"], &v["sku"]["name"], &v["subscription_plan"]["name"], &v["store_listing"]["sku"]["summary"]]
        .iter()
        .find_map(|n| n.as_str())
        .unwrap_or("Gift")
        .to_string()
}

fn sku_of(v: &Value) -> String {
    v["sku_id"].as_str().or(v["sku"]["id"].as_str()).unwrap_or("").to_string()
}

fn redeem(app: &mut App, ui: &mut Ui) {
    super::super::caption(ui, "Received a code for Nitro or a game?", "That’s exciting! Enter it below.");
    lead(ui);
    let mut code = std::mem::take(&mut app.sform.gift_code);
    let before = code.clone();
    let mut go = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let w = ui.available_width() - 110.0;
        let r = ui.allocate_ui(vec2(w, 40.0), |ui| input(ui, &mut code, "WUMP-AAAAA-BBBBB-CCCCC", false)).inner;
        mark(ui, "gift_input", r.rect);
        let (b, _) = ui.allocate_exact_size(vec2(100.0, 40.0), Sense::hover());
        let ready = !clean(&code).is_empty();
        let fill = if ready { pal().blurple } else { pal().blurple.gamma_multiply(0.5) };
        go = solid(ui, b, "Redeem", fill, Color32::WHITE) || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
    });
    if code != before {
        put(ui, "redeem_msg", String::new());
    }
    let clean = clean(&code);
    if go && !clean.is_empty() && !app.remote.contains_key("bill_redeem") {
        app.remote.insert("bill_redeem", None);
        app.rest("POST", format!("/entitlements/gift-codes/{clean}/redeem"), Some(json!({ "channel_id": null, "payment_source_id": null })), Tag::Remote("bill_redeem"));
        put(ui, "redeem_msg", String::new());
    }
    app.sform.gift_code = code;
    if let Some(Some(res)) = app.remote.get("bill_redeem").cloned() {
        app.remote.remove("bill_redeem");
        if res.is_ok() {
            KEYS.iter().for_each(|k| refetch(app, k));
            app.sform.gift_code.clear();
        }
        put(ui, "redeem_msg", if res.is_ok() { "ok" } else { "err" }.to_string());
    }
    let msg: String = get(ui, "redeem_msg");
    let (text, col) = match msg.as_str() {
        "ok" => ("Gift code redeemed!", pal().green),
        "err" => ("This gift code may be expired or you might have the wrong code.", pal().red),
        _ => ("", pal().red),
    };
    if !text.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(text).font(reg(14.0)).color(col));
        mark(ui, if msg == "ok" { "gift_ok" } else { "gift_err" }, ui.min_rect());
    }
}

fn tier_icons(v: &Value) -> Vec<Option<String>> {
    let tiers = v["tiers"].as_array().cloned().unwrap_or_default();
    (0..TIERS.len())
        .map(|i| tiers.get(i).and_then(|t| t["complex_icon_static_url"].as_str().or(t["simple_icon_url"].as_str())).map(str::to_string))
        .collect()
}

fn tier_art(p: &egui::Painter, url: Option<&String>, rect: Rect, fallback: Color32) {
    if url.is_some_and(|u| crate::ui::webart::draw_url(p, u, rect)) {
        return;
    }
    icons::draw(p, Icon::Gift, rect.shrink(rect.width() * 0.1), fallback);
}

fn badge(app: &mut App, ui: &mut Ui) {
    let (count, art) = match remote(app, "badge", "/users/%40me/badges/17") {
        Some(Ok(v)) => (gifted(v), tier_icons(v)),
        _ => (0, vec![None; TIERS.len()]),
    };
    let at = TIERS.iter().rposition(|t| count >= t.1);
    let next = TIERS.iter().position(|t| count < t.1);
    lead(ui);
    let w = ui.available_width();
    let pad = 32.0;
    let inner = w - 2.0 * pad;
    let cols = (((inner + 8.0) / 104.0) as usize).clamp(1, TIERS.len());
    let rows = TIERS.len().div_ceil(cols);
    let grid_h = rows as f32 * 106.0 + (rows - 1) as f32 * 8.0;
    let top_h = if count > 0 { 80.0 + 16.0 + 1.0 + 16.0 } else { 0.0 };
    let body_h = pad + top_h + grid_h + 16.0 + 16.0 + pad;
    let (r, _) = ui.allocate_exact_size(vec2(w, body_h + 88.0), Sense::hover());
    let body = Rect::from_min_size(r.min, vec2(w, body_h));
    let foot = Rect::from_min_max(pos2(r.left(), body.bottom()), r.max);
    let p = ui.painter().clone();
    p.rect_filled(body, CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, faint(20));
    p.rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 }, pal().bg_secondary);
    let mut y = body.top() + pad;
    if count > 0 {
        let row = Rect::from_min_size(pos2(body.left() + pad, y), vec2(inner, 80.0));
        if let Some(i) = at {
            tier_art(&p, art[i].as_ref(), Rect::from_center_size(pos2(row.left() + 40.0, row.center().y), vec2(48.0, 48.0)), TIERS[i].2);
        }
        let text_x = row.left() + 88.0;
        let right = if next.is_some() { row.right() - 88.0 } else { row.right() };
        let title = match (at, next) {
            (_, Some(n)) => {
                let more = TIERS[n].1 - count;
                format!("Send {more} more gift{} to unlock {}!", if more == 1 { "" } else { "s" }, TIERS[n].0)
            }
            (Some(i), None) => format!("You unlocked {}!", TIERS[i].0),
            _ => String::new(),
        };
        p.text(pos2(text_x, row.center().y - 22.0), Align2::LEFT_CENTER, title, medium(16.0), pal().header_primary);
        let bar = Rect::from_min_size(pos2(text_x, row.center().y - 3.0), vec2(right - text_x, 6.0));
        p.rect_filled(bar, 3, faint(51));
        let max = next.map_or(TIERS[TIERS.len() - 1].1, |n| TIERS[n].1);
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * (count as f32 / max as f32).clamp(0.0, 1.0), 6.0)), 3, pal().blurple);
        p.text(pos2(bar.right(), bar.bottom() + 14.0), Align2::RIGHT_CENTER, format!("{}/{max} gifts", count.min(max)), reg(12.0), pal().text_muted);
        if let Some(n) = next {
            tier_art(&p, art[n].as_ref(), Rect::from_center_size(pos2(row.right() - 40.0, row.center().y), vec2(48.0, 48.0)), TIERS[n].2);
        }
        y = row.bottom() + 16.0;
        p.hline(body.left() + pad..=body.right() - pad, y, Stroke::new(1.0, faint(51)));
        y += 17.0;
    }
    for (i, (name, need, tint)) in TIERS.iter().enumerate() {
        let (row, col) = (i / cols, i % cols);
        let in_row = (TIERS.len() - row * cols).min(cols);
        let x0 = body.center().x - (in_row as f32 * 104.0 - 8.0) / 2.0;
        let tile = Rect::from_min_size(pos2(x0 + col as f32 * 104.0, y + row as f32 * 114.0), vec2(96.0, 106.0));
        if at == Some(i) {
            p.rect(tile, 12, faint(20), Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
        }
        let ic = Rect::from_center_size(pos2(tile.center().x, tile.top() + 8.0 + 22.0), vec2(44.0, 44.0));
        tier_art(&p, art[i].as_ref(), ic, if count >= *need { *tint } else { pal().text_muted });
        p.text(pos2(tile.center().x, tile.top() + 8.0 + 44.0 + 8.0 + 9.0), Align2::CENTER_CENTER, *name, semi(14.0), pal().text_muted);
        p.text(pos2(tile.center().x, tile.top() + 8.0 + 44.0 + 8.0 + 18.0 + 2.0 + 8.0), Align2::CENTER_CENTER, format!("Gifted {need}x"), reg(12.0), pal().text_muted);
    }
    y += grid_h + 16.0;
    p.text(pos2(body.center().x, y + 8.0), Align2::CENTER_CENTER, "Your badge progresses as you gift. Exceptions include when a gift is refunded or when fraud is detected.", reg(12.0), pal().text_muted);
    let btn = Rect::from_min_size(pos2(foot.right() - 24.0 - 132.0, foot.top() + 24.0), vec2(132.0, 40.0));
    let resp = ui.interact(btn, egui::Id::new("gift_badge_send"), Sense::click());
    p.rect_filled(btn, 8, if resp.hovered() { pal().blurple.gamma_multiply(0.85) } else { pal().blurple });
    icons::draw(&p, Icon::Gift, Rect::from_center_size(pos2(btn.left() + 26.0, btn.center().y), vec2(20.0, 20.0)), Color32::WHITE);
    p.text(pos2(btn.left() + 44.0, btn.center().y), Align2::LEFT_CENTER, "Send a Gift", medium(16.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, "billing:Send a Gift", btn);
    if resp.clicked() {
        app.open_buy(crate::ui::buy::What::NitroGift);
    }
    end(ui, 24.0);
}

enum Entry {
    Code(Value),
    Giftable(Value),
}

pub fn gifts(app: &mut App, ui: &mut Ui) -> bool {
    settle(app, ui);
    section(ui, "Redeem Codes");
    redeem(app, ui);
    section(ui, "The Gifting Badge");
    badge(app, ui);
    section(ui, "Gifts I Purchased");
    let Some(owned) = fetch(app, ui, "gifts", "/users/@me/entitlements/gifts") else { return false };
    let codes = match remote(app, "gift_codes", "/users/@me/entitlements/gift-codes") {
        Some(Ok(v)) => v.as_array().cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut groups: Vec<(String, String, Vec<Entry>)> = Vec::new();
    let mut put_in = |sku: String, name: String, e: Entry| match groups.iter_mut().find(|g| g.0 == sku) {
        Some(g) => g.2.push(e),
        None => groups.push((sku, name, vec![e])),
    };
    for c in codes {
        put_in(sku_of(&c), name_of(&c), Entry::Code(c));
    }
    for g in owned.into_iter().filter(|g| g["gifter_id"].is_null() || g.get("gifter_id").is_none()) {
        put_in(sku_of(&g), name_of(&g), Entry::Giftable(g));
    }
    if groups.is_empty() {
        lead(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new("There are no gifts.").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let g = ui.painter().layout_no_wrap("Feeling generous? Give someone the gift of Nitro and make their day!".into(), reg(14.0), pal().header_secondary).size().x;
                ui.add_space(((ui.available_width() - g) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("Feeling generous? Give someone the gift of").font(reg(14.0)).color(pal().header_secondary));
                if super::link(ui, "Nitro").clicked() {
                    app.open_buy(crate::ui::buy::What::NitroGift);
                }
                ui.label(RichText::new("and make their day!").font(reg(14.0)).color(pal().header_secondary));
            });
        });
        return false;
    }
    let mut revoke: Option<String> = get(ui, "revoke");
    let streamer = app.settings.streamer;
    let mut steps = Vec::new();
    for (_, name, entries) in &groups {
        card(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
            ui.painter().text(head.left_center(), Align2::LEFT_CENTER, name, semi(16.0), pal().header_primary);
            ui.painter().text(head.right_center(), Align2::RIGHT_CENTER, format!("{}x", entries.len()), medium(14.0), pal().header_secondary);
            for e in entries {
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top() + 4.0, Stroke::new(1.0, faint(51)));
                ui.add_space(8.0);
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Gift, Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(24.0, 24.0)), pal().header_secondary);
                let mut c = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)));
                c.spacing_mut().item_spacing.x = 8.0;
                match e {
                    Entry::Code(v) => {
                        let code = v["code"].as_str().unwrap_or("");
                        let used = v["uses"].as_u64().unwrap_or(0) >= v["max_uses"].as_u64().unwrap_or(1).max(1);
                        let shown = if streamer { "Gift code hidden. Use copy button.".to_string() } else { format!("discord.gift/{code}") };
                        ui.painter().text(pos2(row.left() + 32.0, row.center().y), Align2::LEFT_CENTER, shown, reg(14.0), pal().text_normal);
                        if used {
                            let (r, _) = c.allocate_exact_size(vec2(72.0, 16.0), Sense::hover());
                            pill(c.painter(), r.min, "REDEEMED", Color32::from_rgb(0x4e, 0x50, 0x58));
                        } else {
                            if small(&mut c, "Revoke", Look::Danger).clicked() {
                                revoke = Some(code.to_string());
                            }
                            if small(&mut c, "Copy", Look::Secondary).clicked() {
                                c.ctx().copy_text(format!("https://discord.gift/{code}"));
                                app.toast("Copied!");
                            }
                        }
                    }
                    Entry::Giftable(v) => {
                        ui.painter().text(pos2(row.left() + 32.0, row.center().y), Align2::LEFT_CENTER, "No code yet", reg(14.0), pal().text_muted);
                        if small(&mut c, "Generate a Code", Look::Primary).clicked() {
                            let plan = v["subscription_plan_id"].as_str().or(v["subscription_plan"]["id"].as_str());
                            steps.push(("POST", "/users/@me/entitlements/gift-codes".to_string(), Some(json!({ "sku_id": sku_of(v), "subscription_plan_id": plan, "gift_style": null }))));
                        }
                    }
                }
            }
        });
    }
    if !steps.is_empty() {
        steps.truncate(1);
        act(app, ui, steps);
    }
    put(ui, "revoke", revoke.clone());
    if let Some(code) = revoke {
        let body = "People who haven't redeemed this code yet won't be able to use it.";
        match confirm(ui.ctx(), "revoke_gift", "Revoke gift code?", body, "Revoke", true) {
            Some(true) => {
                put(ui, "revoke", None::<String>);
                act(app, ui, vec![("DELETE", format!("/users/@me/entitlements/gift-codes/{}", clean(&code)), None)]);
            }
            Some(false) => put(ui, "revoke", None::<String>),
            None => {}
        }
    }
    false
}
