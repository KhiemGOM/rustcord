use egui::{Align, Align2, Color32, Layout, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{card, input, loading, refetch, remote, section, toggle};
use crate::net::Tag;
use crate::state::sid;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, avatar, text};

const PROVIDERS: [(&str, &str, u32); 21] = [
    ("riotgames", "Riot Games", 0xd32936),
    ("battlenet", "Battle.net", 0x00aeff),
    ("bluesky", "Bluesky", 0x1185fe),
    ("paypal", "PayPal", 0x003087),
    ("reddit", "Reddit", 0xff4500),
    ("steam", "Steam", 0x171a21),
    ("twitter", "X", 0x000000),
    ("ebay", "eBay", 0xe53238),
    ("crunchyroll", "Crunchyroll", 0xf47521),
    ("playstation", "PlayStation Network", 0x003791),
    ("spotify", "Spotify", 0x1db954),
    ("xbox", "Xbox", 0x107c10),
    ("amazon-music", "Amazon Music", 0x25d1da),
    ("bungie", "Bungie.net", 0x3a3f47),
    ("domain", "Domain", 0x5865f2),
    ("epicgames", "Epic Games", 0x2a2a2a),
    ("facebook", "Facebook", 0x1877f2),
    ("roblox", "Roblox", 0x393b3d),
    ("github", "GitHub", 0x24292f),
    ("twitch", "Twitch", 0x9146ff),
    ("youtube", "YouTube", 0xff0000),
];

fn provider(kind: &str) -> (&'static str, Color32) {
    PROVIDERS.iter().find(|p| p.0 == kind).map_or(("Account", BLURPLE), |p| (p.1, role_color(p.2)))
}

fn badge(ui: &Ui, r: Rect, name: &str, fill: Color32) {
    ui.painter().rect_filled(r, 8, fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, name.chars().next().unwrap_or('?'), semi(r.height() * 0.45), Color32::WHITE);
}

const SCOPES: [(&str, &str); 12] = [
    ("identify", "Access your username, avatar, and banner"),
    ("email", "Access your email address"),
    ("guilds", "Know what servers you're in"),
    ("guilds.join", "Join servers for you"),
    ("guilds.members.read", "Read your member info in servers"),
    ("connections", "Access your connected accounts"),
    ("applications.commands", "Create commands"),
    ("dm_channels.messages.write", "Send you direct messages"),
    ("dm_channels.read", "See your direct messages"),
    ("bot", "Add a bot to a server"),
    ("webhook.incoming", "Create a webhook in a server"),
    ("activities.write", "Update your activity"),
];

fn scope_text(s: &str) -> String {
    SCOPES.iter().find(|x| x.0 == s).map_or_else(|| s.to_string(), |x| x.1.to_string())
}

fn date(id: &str) -> String {
    let ms = (id.parse::<u64>().unwrap_or(0) >> 22) + 1_420_070_400_000;
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.format("%-m/%-d/%Y").to_string()).unwrap_or_default()
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    connections(app, ui);
    authorized(app, ui);
    false
}

fn connections(app: &mut App, ui: &mut Ui) {
    section(ui, "Connections");
    if app.settings.hiding_personal() {
        super::note(ui, "Hidden while Streamer Mode is on.");
        return;
    }
    super::note(ui, "These are accounts from other platforms. Connecting them here can unlock new features on Discord, like the ability to share what you're currently listening to, get paid safely and securely, and more. Add a new connection");
    super::lead(ui);
    let per = ((ui.available_width() + 8.0) / 52.0) as usize;
    let mut add = None;
    for row in PROVIDERS.chunks(per.max(1)) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (kind, name, c) in row {
                let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                ui.painter().rect(r, 8, Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 77 } else { 31 }), egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
                badge(ui, r.shrink(8.0), name, role_color(*c));
                super::mark(ui, &format!("provider:{kind}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.on_hover_text(*name).clicked() {
                    add = Some(*kind);
                }
            }
        });
        ui.add_space(8.0);
    }
    super::end(ui, 24.0);
    if let Some(k) = add {
        app.rest("GET", format!("/connections/{k}/authorize"), None, Tag::Remote("conn_auth"));
    }
    let Some(v) = loading(ui, remote(app, "connections", "/users/@me/connections")) else { return };
    let list = v.as_array().cloned().unwrap_or_default();
    super::lead(ui);
    ui.label(RichText::new(format!("{} connection{}", list.len(), if list.len() == 1 { "" } else { "s" })).font(semi(16.0)).color(pal().text_normal));
    ui.add_space(16.0);
    let mut patch: Option<(String, Value)> = None;
    let mut remove = None;
    for c in &list {
        let kind = c["type"].as_str().unwrap_or("").to_string();
        let id = c["id"].as_str().unwrap_or("").to_string();
        let (label, color) = provider(&kind);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                badge(ui, r, label, color);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    text::label(ui, c["name"].as_str().unwrap_or(""), medium(16.0), pal().header_primary, Sense::hover());
                    ui.label(RichText::new(label).font(reg(12.0)).color(pal().header_secondary));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let x = crate::ui::icon_button(ui, Icon::Close, 24.0, "Disconnect");
                    super::mark(ui, &format!("disconnect:{kind}"), x.rect);
                    if x.clicked() {
                        remove = Some(format!("{kind}/{id}"));
                    }
                });
            });
            ui.add_space(16.0);
            let mut vis = c["visibility"].as_u64() == Some(1);
            if toggle(ui, &mut vis, "Display on profile", "") {
                patch = Some((format!("{kind}/{id}"), json!({ "visibility": vis as u8 })));
            }
            if kind == "spotify" {
                let mut act = c["show_activity"].as_bool().unwrap_or(true);
                if toggle(ui, &mut act, "Display Spotify as your status", "") {
                    patch = Some((format!("{kind}/{id}"), json!({ "show_activity": act })));
                }
            }
        });
    }
    if let Some((p, body)) = patch {
        app.rest("PATCH", format!("/users/@me/connections/{p}"), Some(body), Tag::Remote("action"));
        refetch(app, "connections");
    }
    if let Some(p) = remove {
        app.rest("DELETE", format!("/users/@me/connections/{p}"), None, Tag::Remote("action"));
        refetch(app, "connections");
    }
}

pub fn authorize_done(app: &mut App, res: Result<Value, String>) {
    match res {
        Ok(v) => match v["url"].as_str() {
            Some(u) => {
                let _ = open::that_detached(u);
            }
            None => app.toast("Discord didn't give a sign-in address for that service."),
        },
        Err(e) => app.toast(e),
    }
}

fn authorized(app: &mut App, ui: &mut Ui) {
    section(ui, "Authorized Apps");
    super::note(ui, "These include third-party services you've given permission to access your Discord account. You can review what each app can do and revoke access anytime.");
    let mut q = app.sform.app_search.clone();
    super::lead(ui);
    let r = input(ui, &mut q, "Search installed apps", false);
    super::mark(ui, "app_search", r.rect);
    super::end(ui, 16.0);
    app.sform.app_search = q.clone();
    let Some(v) = loading(ui, remote(app, "oauth", "/oauth2/tokens")) else { return };
    let list: Vec<Value> = v.as_array().cloned().unwrap_or_default().into_iter().filter(|t| t["application"]["name"].as_str().unwrap_or("").to_lowercase().contains(&q.to_lowercase())).collect();
    if list.is_empty() {
        super::note(ui, if q.is_empty() { "You have no authorized apps." } else { "No apps match your search." });
    }
    let mut revoke = None;
    for t in &list {
        let a = &t["application"];
        let tid = t["id"].as_str().unwrap_or("").to_string();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                match a["icon"].as_str() {
                    Some(h) => avatar(ui, r, &format!("{}/app-icons/{}/{h}.png?size=80", crate::api::cdn(), sid(&a["id"]))),
                    None => {
                        ui.painter().circle_filled(r.center(), 20.0, BLURPLE);
                    }
                }
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    text::label(ui, a["name"].as_str().unwrap_or("Unknown app"), semi(16.0), pal().header_primary, Sense::hover());
                    ui.label(RichText::new(format!("Authorized on {}", date(&tid))).font(reg(14.0)).color(pal().header_secondary));
                });
            });
            ui.add_space(12.0);
            let scopes: Vec<&str> = t["scopes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let open = app.sform.app_open.contains(&tid);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            ui.painter().text(r.left_top(), Align2::LEFT_TOP, "Permissions", medium(14.0), pal().header_primary);
            ui.painter().text(r.left_top() + vec2(0.0, 20.0), Align2::LEFT_TOP, format!("{} Permissions enabled", scopes.len()), reg(14.0), pal().header_secondary);
            icons::draw(ui.painter(), if open { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
            super::mark(ui, &format!("permissions:{}", a["name"].as_str().unwrap_or("")), r);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                if open {
                    app.sform.app_open.retain(|x| *x != tid);
                } else {
                    app.sform.app_open.push(tid.clone());
                }
            }
            if open {
                for s in &scopes {
                    ui.label(RichText::new(format!("•  {}", scope_text(s))).font(reg(14.0)).color(pal().text_normal));
                }
            }
            ui.add_space(12.0);
            let b = super::danger(ui, "Deauthorize", false);
            super::mark(ui, &format!("deauthorize:{}", a["name"].as_str().unwrap_or("")), b.rect);
            if b.clicked() {
                revoke = Some(tid.clone());
            }
        });
    }
    if let Some(id) = revoke {
        app.rest("DELETE", format!("/oauth2/tokens/{id}"), None, Tag::Remote("action"));
        refetch(app, "oauth");
    }
}
