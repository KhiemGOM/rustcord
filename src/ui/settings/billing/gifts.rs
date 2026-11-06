use egui::{Align, Align2, Color32, Key, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
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
        let fill = if ready { BLURPLE } else { BLURPLE.gamma_multiply(0.5) };
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
        "ok" => ("Gift code redeemed!", GREEN),
        "err" => ("This gift code may be expired or you might have the wrong code.", RED),
        _ => ("", RED),
    };
    if !text.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(text).font(reg(14.0)).color(col));
        mark(ui, if msg == "ok" { "gift_ok" } else { "gift_err" }, ui.min_rect());
    }
}

fn badge(app: &mut App, ui: &mut Ui) {
    let count = match remote(app, "badge", "/users/%40me/badges/17") {
        Some(Ok(v)) => gifted(v),
        _ => 0,
    };
    let top = TIERS.iter().rev().find(|t| count >= t.1);
    let max = TIERS.iter().find(|t| count < t.1).map_or(20, |t| t.1);
    lead(ui);
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 340.0), Sense::hover());
    ui.painter().rect_filled(r, 12, pal().bg_secondary);
    let p = ui.painter().clone();
    let icon = Rect::from_center_size(pos2(r.left() + 64.0, r.top() + 64.0), vec2(56.0, 56.0));
    icons::draw(&p, Icon::Gift, icon, top.map_or(pal().text_muted, |t| t.2));
    let title = match top {
        Some(t) => format!("You unlocked {}!", t.0),
        None => "Send a gift to unlock Patron!".to_string(),
    };
    p.text(pos2(r.left() + 112.0, r.top() + 44.0), Align2::LEFT_CENTER, title, medium(16.0), pal().header_primary);
    let bar = Rect::from_min_size(pos2(r.left() + 112.0, r.top() + 62.0), vec2(w - 144.0, 8.0));
    p.rect_filled(bar, 4, faint(31));
    let frac = (count as f32 / max as f32).clamp(0.0, 1.0);
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, 8.0)), 4, BLURPLE);
    p.text(pos2(bar.right(), bar.bottom() + 12.0), Align2::RIGHT_CENTER, format!("{count}/{max} gifts"), reg(12.0), pal().header_secondary);
    p.hline((r.left() + 32.0)..=(r.right() - 32.0), r.top() + 112.0, Stroke::new(1.0, faint(51)));
    let col = (w - 64.0) / 6.0;
    for (i, (name, need, tint)) in TIERS.iter().enumerate() {
        let tile = Rect::from_min_size(pos2(r.left() + 32.0 + col * i as f32, r.top() + 132.0), vec2(col, 108.0));
        if top.is_some_and(|t| t.0 == *name) {
            p.rect_filled(tile.shrink2(vec2(4.0, 0.0)), 8, faint(31));
        }
        let lit = count >= *need;
        icons::draw(&p, Icon::Gift, Rect::from_center_size(tile.center_top() + vec2(0.0, 32.0), vec2(40.0, 40.0)), if lit { *tint } else { pal().text_muted });
        p.text(tile.center_top() + vec2(0.0, 68.0), Align2::CENTER_CENTER, *name, semi(14.0), pal().header_primary);
        p.text(tile.center_top() + vec2(0.0, 88.0), Align2::CENTER_CENTER, format!("Gifted {need}x"), reg(12.0), pal().header_secondary);
    }
    p.text(pos2(r.center().x, r.top() + 262.0), Align2::CENTER_CENTER, "Your badge progresses as you gift. Exceptions include when a gift is refunded or when fraud is detected.", reg(12.0), pal().header_secondary);
    if solid(ui, Rect::from_center_size(pos2(r.center().x, r.top() + 304.0), vec2(140.0, 40.0)), "Send a Gift", BLURPLE, Color32::WHITE) {
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
