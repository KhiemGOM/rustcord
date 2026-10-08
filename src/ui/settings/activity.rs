use egui::{Color32, Frame, RichText, Sense, Ui, vec2};
use serde_json::json;

use super::{h5, note, remote, title, toggle};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;

pub fn privacy(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Activity Privacy");
    h5(ui, "Activity status");
    match remote(app, "legacy", "/users/@me/settings").cloned() {
        None => {
            ui.add(egui::Spinner::new().color(pal().text_muted));
        }
        Some(Err(_)) => note(ui, "Discord didn't hand this setting to Fastcord. You can change it on discord.com."),
        Some(Ok(s)) => {
            let mut on = s["show_current_game"].as_bool().unwrap_or(true);
            if toggle(
                ui,
                &mut on,
                "Share your detected activities with others",
                "Lets people see what you're playing or listening to. Fastcord doesn't detect games itself, but this applies to your other Discord apps too.",
            ) {
                if let Some(Some(Ok(cur))) = app.remote.get_mut("legacy") {
                    cur["show_current_game"] = json!(on);
                }
                app.rest("PATCH", "/users/@me/settings".into(), Some(json!({ "show_current_game": on })), Tag::Remote("action"));
            }
        }
    }
    false
}

// house ids as the api numbers them, with the user flag bit each one sets
const HOUSES: [(u64, u64, &str, &str, Color32); 3] = [
    (1, 1 << 6, "Bravery", "Bravery is the courage to do what's right, even when it's hard.", Color32::from_rgb(0x9c, 0x84, 0xef)),
    (2, 1 << 7, "Brilliance", "Brilliance is about creativity and thinking outside the box.", Color32::from_rgb(0xf4, 0x7b, 0x67)),
    (3, 1 << 8, "Balance", "Balance is finding harmony in everything you do.", Color32::from_rgb(0x45, 0xdd, 0xc0)),
];

pub fn hypesquad(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "HypeSquad");
    let flags = app.store.me_raw["flags"].as_u64().or(app.store.me_raw["public_flags"].as_u64()).unwrap_or(0);
    let current = HOUSES.iter().find(|h| flags & h.1 != 0).map(|h| h.0);
    note(ui, "Join a HypeSquad house to get a badge on your profile.");
    ui.add_space(12.0);
    let mut pick = None;
    let mut leave = false;
    ui.columns(3, |cols| {
        for (i, (id, _, name, blurb, color)) in HOUSES.iter().enumerate() {
            let ui = &mut cols[i];
            Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 30.0, *color);
                crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Gem, egui::Rect::from_center_size(r.center(), vec2(28.0, 28.0)), Color32::WHITE);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(*name).font(bold(18.0)).color(pal().header_primary));
                });
                ui.label(RichText::new(*blurb).font(reg(13.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                if current == Some(*id) {
                    ui.label(RichText::new("Your house").font(semi(14.0)).color(*color));
                } else {
                    let r = crate::ui::button(ui, "Join", *color, ui.available_width());
                    super::mark(ui, &format!("house:{name}"), r.rect);
                    if r.clicked() {
                        pick = Some(*id);
                    }
                }
            });
        }
    });
    if current.is_some() {
        ui.add_space(12.0);
        leave = super::danger(ui, "Leave HypeSquad", false).clicked();
    }
    let all: u64 = HOUSES.iter().map(|h| h.1).sum();
    if let Some(id) = pick {
        app.rest("POST", "/hypesquad/online".into(), Some(json!({ "house_id": id })), Tag::Remote("action"));
        let bit = HOUSES.iter().find(|h| h.0 == id).map(|h| h.1).unwrap_or(0);
        app.store.me_raw["flags"] = json!((flags & !all) | bit);
    }
    if leave {
        app.rest("DELETE", "/hypesquad/online".into(), None, Tag::Remote("action"));
        app.store.me_raw["flags"] = json!(flags & !all);
    }
    false
}
