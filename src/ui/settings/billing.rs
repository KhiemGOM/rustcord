use egui::{Color32, Frame, RichText, Ui, vec2};
use serde_json::{Value, json};

use super::{Look, action, card, input, link, loading, note, open_web, primary, refetch, remote, section};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;

pub fn nitro(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Nitro");
    let tier = match app.store.me_raw["premium_type"].as_u64().unwrap_or(0) {
        1 => Some("Nitro Classic"),
        2 => Some("Nitro"),
        3 => Some("Nitro Basic"),
        _ => None,
    };
    Frame::new().fill(Color32::from_rgb(0x8e, 0x4d, 0xd8)).corner_radius(8).inner_margin(24).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match tier {
            Some(t) => {
                ui.label(RichText::new(format!("You have {t}")).font(semi(24.0)).color(Color32::WHITE));
                ui.label(RichText::new("Thanks for supporting Discord.").font(reg(16.0)).color(Color32::WHITE));
            }
            None => {
                ui.label(RichText::new("Unleash more fun with Nitro").font(semi(24.0)).color(Color32::WHITE));
                ui.label(RichText::new("Bigger uploads, HD streaming, custom emoji anywhere and more.").font(reg(16.0)).color(Color32::WHITE));
            }
        }
        ui.add_space(12.0);
        if crate::ui::button(ui, if tier.is_some() { "Manage" } else { "Subscribe" }, Color32::WHITE.gamma_multiply(0.25), 120.0).clicked() {
            open_web("/settings/premium");
        }
    });
    false
}

pub fn boost(ui: &mut Ui) -> bool {
    section(ui, "Server Boost");
    if action(ui, "Boost your servers", "Server Boosts unlock perks like better audio, more emoji slots and larger uploads for everyone in a server. Boosts are managed on discord.com.", "Open", Look::Primary) {
        open_web("/settings/premium-guild-subscriptions");
    }
    false
}

fn read_only_list(app: &mut App, ui: &mut Ui, key: &'static str, path: &str, empty: &str, row: impl Fn(&Value) -> (String, String)) {
    let Some(v) = loading(ui, remote(app, key, path)) else { return };
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        note(ui, empty);
    }
    for x in &list {
        let (a, b) = row(x);
        card(ui, |ui| {
            ui.label(RichText::new(a).font(medium(16.0)).color(pal().header_primary));
            if !b.is_empty() {
                ui.label(RichText::new(b).font(reg(14.0)).color(pal().header_secondary));
            }
        });
    }
    super::lead(ui);
    if link(ui, "Manage on discord.com").clicked() {
        open_web("/settings/billing");
    }
}

pub fn subscriptions(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Subscriptions");
    read_only_list(app, ui, "subs", "/users/@me/billing/subscriptions", "You don't have any subscriptions.", |s| {
        let ends = s["current_period_end"].as_str().unwrap_or("").get(..10).unwrap_or("").to_string();
        let status = match s["status"].as_u64() {
            Some(1) => "Active",
            Some(2) => "Past due",
            Some(3) => "Canceled",
            _ => "Pending",
        };
        (format!("Subscription {}", s["id"].as_str().unwrap_or("")), format!("{status} · renews {ends}"))
    });
    false
}

pub fn gifts(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Redeem Codes");
    super::caption(ui, "Received a code for Nitro or a game?", "That's exciting! Enter it below.");
    super::lead(ui);
    let mut code = std::mem::take(&mut app.sform.gift_code);
    let mut redeem = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let w = ui.available_width() - 110.0;
        ui.allocate_ui(vec2(w, 40.0), |ui| input(ui, &mut code, "WUMP-AAAAA-BBBBB-CCCCC", false));
        redeem = primary(ui, "Redeem").clicked();
    });
    if redeem && !code.trim().is_empty() {
        app.rest("POST", format!("/entitlements/gift-codes/{}/redeem", code.trim()), Some(json!({})), Tag::Remote("action"));
        refetch(app, "gifts");
        code.clear();
    }
    app.sform.gift_code = code;

    section(ui, "The Gifting Badge");
    if action(ui, "Gift Nitro, earn a badge", "The more gifts you send, the higher your Gifting badge climbs.", "Send a Gift", Look::Primary) {
        open_web("/settings/premium");
    }

    section(ui, "Gifts I Purchased");
    read_only_list(app, ui, "gifts", "/users/@me/entitlements/gifts", "You don't have any gifts. Gifts you buy for others show up here.", |g| {
        (g["sku"]["name"].as_str().unwrap_or("Gift").to_string(), String::new())
    });
    false
}

pub fn billing(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Payment Methods");
    read_only_list(app, ui, "sources", "/users/@me/billing/payment-sources", "You don't have any payment methods saved.", |p| {
        let brand = p["brand"].as_str().unwrap_or(if p["type"].as_u64() == Some(2) { "PayPal" } else { "Card" });
        let last4 = p["last_4"].as_str().map(|l| format!(" ending in {l}")).unwrap_or_default();
        let exp = match (p["expires_month"].as_u64(), p["expires_year"].as_u64()) {
            (Some(m), Some(y)) => format!("Expires {m:02}/{y}"),
            _ => String::new(),
        };
        (format!("{brand}{last4}"), exp)
    });
    section(ui, "Transaction History");
    read_only_list(app, ui, "payments", "/users/@me/billing/payments?limit=20", "No transactions yet.", |p| {
        let amount = p["amount"].as_u64().unwrap_or(0) as f64 / 100.0;
        let cur = p["currency"].as_str().unwrap_or("usd").to_uppercase();
        let when = p["created_at"].as_str().unwrap_or("").get(..10).unwrap_or("").to_string();
        (format!("{} — {amount:.2} {cur}", p["description"].as_str().unwrap_or("Payment")), when)
    });
    false
}
