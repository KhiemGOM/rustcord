use egui::{Align2, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::privacy::{NOTIF, PRIVACY, STATUS, blob, related, write};
use super::wire::{self, Patch};
use super::{Page, caption, input, note, radio, section, toggle};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::{App, avatar};
use egui::Color32;
use serde_json::json;

const ALL: u64 = 1;
const SMALL: u64 = 2;
const NONE: u64 = 3;

fn mode(b: &[u8]) -> u64 {
    wire::num(b, PRIVACY, 29).filter(|v| *v != 0).or(wire::num(b, PRIVACY, 14).map(|v| v + 1)).unwrap_or(ALL)
}

pub fn privacy(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "What Activity I Share");
    let Some(b) = blob(app, ui) else { return false };
    let mut on = wire::flag(&b, STATUS, 3).unwrap_or(true);
    if toggle(ui, &mut on, "Share my activity", "Share activity information from games and connected apps I use, including when and how I engage.") {
        app.store.hide_game = !on;
        write(app, Patch::new().flag(STATUS, 3, on));
    }
    let mut online = wire::flag(&b, NOTIF, 25).unwrap_or(true);
    if toggle(ui, &mut online, "Share when I come online", "Allow friends to receive a push notification when you come online.") {
        write(app, Patch::new().flag(NOTIF, 25, online));
    }
    let mut live = wire::flag(&b, NOTIF, 26).unwrap_or(true);
    if toggle(ui, &mut live, "Share when I start streaming", "Allow members that you interact with the most to get a notification when you start streaming in small servers.") {
        write(app, Patch::new().flag(NOTIF, 26, live));
    }

    section(ui, "Where I Share Activity");
    caption(ui, "Automatically share my activity when joining a server", "Your activity is always shared with your friends.");
    super::end(ui, 16.0);
    let guilds: Vec<(Id, String, u32, Option<String>)> =
        app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g).map(|x| (*g, x.name.clone(), x.member_count, x.icon_url()))).collect();
    let all: Vec<u64> = guilds.iter().map(|g| g.0 as u64).collect();
    let off = wire::ids(&b, PRIVACY, 13);
    let mut m = mode(&b);
    let opts = [(ALL, "Share in all servers", ""), (SMALL, "Share in servers with 200 or fewer members", ""), (NONE, "Do not share in any servers", "")];
    if radio(ui, &mut m, &opts) {
        let ids: Vec<u64> = match m {
            ALL => Vec::new(),
            SMALL => guilds.iter().filter(|g| g.2 > 200).map(|g| g.0 as u64).collect(),
            _ => all.clone(),
        };
        write(app, Patch::new().num(PRIVACY, 14, m - 1).num(PRIVACY, 29, m).ids(PRIVACY, 13, &ids));
    }
    super::lead(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Server settings").font(medium(16.0)).color(pal().header_primary));
        if !on || m == NONE {
            ui.label(RichText::new("ignored when not sharing").font(reg(14.0)).color(pal().header_secondary));
        }
    });
    ui.add_space(12.0);
    let mut query = app.sform.act_search.clone();
    ui.horizontal(|ui| {
        let w = ui.available_width() - 340.0;
        ui.allocate_ui(vec2(w, 40.0), |ui| {
            let r = input(ui, &mut query, "Search my servers", false);
            super::mark(ui, "activity_search", r.rect);
        });
        ui.add_space(12.0);
        let by_name = app.sform.act_sort == 1;
        let mut pick = None;
        super::dropdown(ui, 140.0, None, if by_name { "Alphabetical" } else { "Server Order" }, |ui| {
            for (i, l) in ["Server Order", "Alphabetical"].iter().enumerate() {
                if ui.selectable_label(app.sform.act_sort == i as u8, *l).clicked() {
                    pick = Some(i as u8);
                }
            }
        });
        if let Some(p) = pick {
            app.sform.act_sort = p;
        }
        ui.add_space(12.0);
        let every = !all.is_empty() && all.iter().all(|g| !off.contains(g));
        let b2 = super::secondary(ui, if every { "Toggle All Off" } else { "Toggle All On" });
        super::mark(ui, "toggle_all", b2.rect);
        if b2.clicked() {
            write(app, Patch::new().ids(PRIVACY, 13, &if every { all.clone() } else { Vec::new() }));
        }
    });
    app.sform.act_search = query.clone();
    ui.add_space(12.0);
    let mut shown: Vec<&(Id, String, u32, Option<String>)> = guilds.iter().filter(|g| g.1.to_lowercase().contains(&query.to_lowercase())).collect();
    if app.sform.act_sort == 1 {
        shown.sort_by_key(|g| g.1.to_lowercase());
    }
    ui.label(RichText::new(format!("{} servers", shown.len())).font(medium(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let clip = ui.clip_rect();
    let mut flip = None;
    for (g, name, members, icon) in shown {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        if !clip.intersects(r) {
            continue;
        }
        let ic = Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(40.0, 40.0));
        match icon {
            Some(u) => avatar(ui, ic, u),
            None => {
                ui.painter().circle_filled(ic.center(), 20.0, pal().bg_secondary);
                ui.painter().text(ic.center(), Align2::CENTER_CENTER, name.chars().next().unwrap_or('?'), semi(16.0), pal().header_primary);
            }
        }
        crate::ui::text::paint(ui.painter(), pos2(r.left() + 56.0, r.center().y - 9.0), Align2::LEFT_CENTER, name, medium(16.0), pal().header_primary, 420.0);
        ui.painter().text(pos2(r.left() + 56.0, r.center().y + 11.0), Align2::LEFT_CENTER, format!("{members} Members"), reg(14.0), pal().header_secondary);
        let sw = Rect::from_min_size(pos2(r.right() - 48.0, r.center().y - 12.0), vec2(48.0, 24.0));
        let resp = ui.interact(sw, egui::Id::new(("activity_guild", *g)), Sense::click());
        let enabled = !off.contains(&(*g as u64));
        super::paint_switch(ui, sw, &resp, enabled);
        super::mark(ui, &format!("guild:{name}"), sw);
        if resp.clicked() {
            flip = Some((*g as u64, enabled));
        }
    }
    if let Some((g, enabled)) = flip {
        let ids: Vec<u64> = if enabled { off.iter().copied().chain([g]).collect() } else { off.iter().copied().filter(|x| *x != g).collect() };
        write(app, Patch::new().ids(PRIVACY, 13, &ids));
    }
    section(ui, "Related Settings");
    related(app, ui, "Data & Privacy", "Control who can see your profile info — like your bio and connected accounts.", Page::Privacy);

    section(ui, "Who Can Join My Games");
    note(ui, "Some games allow users to join the game you're playing when viewing your activity.");
    let mut friends = wire::flag(&b, PRIVACY, 1).unwrap_or(true);
    if toggle(ui, &mut friends, "Allow friends to join my game", "Allow friends to join my game without sending a request.") {
        write(app, Patch::new().flag(PRIVACY, 1, friends));
    }
    let mut vc = wire::flag(&b, PRIVACY, 2).unwrap_or(true);
    if toggle(ui, &mut vc, "Allow voice channel participants to join my game", "Allow anyone in a shared voice channel to join my game without sending a request. This feature only works in non-Community enabled servers.") {
        write(app, Patch::new().flag(PRIVACY, 2, vc));
    }
    super::lead(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let t = |ui: &mut Ui, s: &str| ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
        t(ui, "Looking for per-server controls? Navigate to the server, select the dropdown menu next to the server name, then select");
        ui.label(RichText::new("Privacy Settings").font(semi(14.0)).color(pal().header_secondary));
        t(ui, ". From there, you can toggle on or off Activity Joining for just that server.");
    });
    false
}

// house ids as the api numbers them, with the user flag bit each one sets
const HOUSES: [(u64, u64, &str, &str, Color32); 3] = [
    (1, 1 << 6, "Bravery", "Bravery is the courage to do what's right, even when it's hard.", Color32::from_rgb(0x9c, 0x84, 0xef)),
    (2, 1 << 7, "Brilliance", "Brilliance is about creativity and thinking outside the box.", Color32::from_rgb(0xf4, 0x7b, 0x67)),
    (3, 1 << 8, "Balance", "Balance is finding harmony in everything you do.", Color32::from_rgb(0x45, 0xdd, 0xc0)),
];

pub(super) fn hypesquad(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "HypeSquad");
    let flags = app.store.me_raw["flags"].as_u64().or(app.store.me_raw["public_flags"].as_u64()).unwrap_or(0);
    let current = HOUSES.iter().find(|h| flags & h.1 != 0).map(|h| h.0);
    note(ui, "Join a HypeSquad house to get a badge on your profile.");
    let mut pick = None;
    let mut leave = false;
    super::lead(ui);
    ui.columns(3, |cols| {
        for (i, (id, _, name, blurb, color)) in HOUSES.iter().enumerate() {
            let ui = &mut cols[i];
            Frame::new().fill(pal().bg_primary).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61))).corner_radius(8).inner_margin(16).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 30.0, *color);
                crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Gem, egui::Rect::from_center_size(r.center(), vec2(28.0, 28.0)), Color32::WHITE);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(*name).font(semi(18.0)).color(pal().header_primary));
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
