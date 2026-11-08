use egui::{Align2, Rect, RichText, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::{App, Page, faint, row, search, title};
use crate::net::Tag;
use crate::state::{Id, sid, snowflake_ms};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

#[derive(Default)]
pub struct Log {
    user: Id,
    action: u64,
    open: HashSet<String>,
    asked: Option<(Id, u64)>,
}

pub fn bans(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Server Ban List", "");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Bans by default are by account and IP. A user can circumvent an IP ban by using a proxy. Ban circumvention can be made very hard by enabling phone verification in ").font(medium(14.0)).color(pal().text_normal));
        if settings::link(ui, "Safety Setup").clicked()
            && let Some(s) = &mut app.server
        {
            s.page = Page::Safety;
        }
        ui.label(RichText::new(".").font(medium(14.0)).color(pal().text_normal));
    });
    ui.add_space(24.0);
    let q = search(ui, app, "Search Bans by User Id or Username");
    let list: Vec<Value> = app.server_data("bans", format!("/guilds/{gid}/bans?limit=1000")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if list.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No Bans").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new("You haven't banned anybody... but if and when you must, do not hesitate!").font(reg(14.0)).color(pal().header_secondary));
        });
        return;
    }
    let mut pick = None;
    let mut shown = 0;
    for b in list.iter().filter(|b| q.is_empty() || b["user"]["username"].as_str().is_some_and(|n| n.to_lowercase().contains(&q)) || b["user"]["global_name"].as_str().is_some_and(|n| n.to_lowercase().contains(&q)) || b["user"]["id"].as_str() == Some(q.as_str())) {
        shown += 1;
        let u = &b["user"];
        let id = sid(&u["id"]);
        let name = u["global_name"].as_str().or(u["username"].as_str()).unwrap_or_default().to_string();
        let avatar = u["avatar"].as_str().map(|h| format!("{}/avatars/{id}/{h}.png?size=64", crate::api::cdn()));
        let reason = b["reason"].as_str().unwrap_or("No reason provided.").to_string();
        let resp = row(ui, 56.0, |ui, r, hot| {
            if let Some(a) = &avatar {
                crate::ui::avatar(ui, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 16.0), vec2(32.0, 32.0)), a);
            }
            ui.painter().text(pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary);
            ui.painter().text(pos2(r.left() + 52.0, r.center().y + 11.0), Align2::LEFT_CENTER, id.to_string(), reg(12.0), pal().header_secondary);
            crate::ui::text::paint(ui.painter(), pos2(r.left() + r.width() * 0.4, r.center().y), Align2::LEFT_CENTER, &reason, reg(14.0), pal().text_normal, r.width() * 0.4);
            if hot {
                ui.painter().text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, "Revoke Ban", medium(14.0), pal().red);
            }
        });
        if resp.clicked() {
            pick = Some((id, name, reason));
        }
    }
    if shown == 0 {
        ui.label(RichText::new("No bans match your search.").font(reg(14.0)).color(pal().header_secondary));
    }
    if let Some((id, name, reason)) = pick {
        app.confirm = Some(crate::ui::Confirm::new(format!("Revoke ban for {name}"), format!("Ban reason: {reason}"), "Revoke Ban", move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/bans/{id}"), None, Tag::rollback(crate::ui::failed::stale("bans")));
            if let Some(s) = &mut app.server {
                if let Some(Value::Array(a)) = s.data.get_mut("bans") {
                    a.retain(|b| sid(&b["user"]["id"]) != id);
                }
            }
        }));
    }
}

const ACTIONS: &[(u64, &str)] = &[
    (1, "Server Updated"),
    (10, "Channel Created"),
    (11, "Channel Updated"),
    (12, "Channel Deleted"),
    (13, "Channel Overwrite Created"),
    (14, "Channel Overwrite Updated"),
    (15, "Channel Overwrite Deleted"),
    (20, "Member Kicked"),
    (21, "Members Pruned"),
    (22, "Member Banned"),
    (23, "Member Unbanned"),
    (24, "Member Updated"),
    (25, "Member Role Updated"),
    (26, "Member Moved"),
    (27, "Member Disconnected"),
    (28, "Bot Added"),
    (30, "Role Created"),
    (31, "Role Updated"),
    (32, "Role Deleted"),
    (40, "Invite Created"),
    (41, "Invite Updated"),
    (42, "Invite Deleted"),
    (50, "Webhook Created"),
    (51, "Webhook Updated"),
    (52, "Webhook Deleted"),
    (60, "Emoji Created"),
    (61, "Emoji Updated"),
    (62, "Emoji Deleted"),
    (72, "Message Deleted"),
    (73, "Messages Bulk Deleted"),
    (74, "Message Pinned"),
    (75, "Message Unpinned"),
    (80, "Integration Created"),
    (81, "Integration Updated"),
    (82, "Integration Deleted"),
    (83, "Stage Created"),
    (84, "Stage Updated"),
    (85, "Stage Deleted"),
    (90, "Sticker Created"),
    (91, "Sticker Updated"),
    (92, "Sticker Deleted"),
    (100, "Event Created"),
    (101, "Event Updated"),
    (102, "Event Deleted"),
    (110, "Thread Created"),
    (111, "Thread Updated"),
    (112, "Thread Deleted"),
    (121, "Application Command Permissions Updated"),
    (130, "Soundboard Sound Created"),
    (131, "Soundboard Sound Updated"),
    (132, "Soundboard Sound Deleted"),
    (140, "AutoMod Rule Created"),
    (141, "AutoMod Rule Updated"),
    (142, "AutoMod Rule Deleted"),
    (143, "Message Blocked by AutoMod"),
    (144, "Message Flagged by AutoMod"),
    (145, "Member Timed Out by AutoMod"),
];

fn verb(n: u64) -> &'static str {
    match n {
        1 => "updated the server",
        10 => "created channel",
        11 => "updated channel",
        12 => "deleted channel",
        13 => "created a permission override for",
        14 => "updated a permission override for",
        15 => "deleted a permission override for",
        20 => "kicked",
        21 => "pruned members",
        22 => "banned",
        23 => "unbanned",
        24 => "updated",
        25 => "updated roles for",
        26 => "moved",
        27 => "disconnected",
        28 => "added the app",
        30 => "created role",
        31 => "updated role",
        32 => "deleted role",
        40 => "created invite",
        41 => "updated invite",
        42 => "deleted invite",
        50 => "created webhook",
        51 => "updated webhook",
        52 => "deleted webhook",
        60 => "created emoji",
        61 => "updated emoji",
        62 => "deleted emoji",
        72 => "deleted a message from",
        73 => "bulk deleted messages in",
        74 => "pinned a message in",
        75 => "unpinned a message in",
        80 => "added the integration",
        81 => "updated the integration",
        82 => "removed the integration",
        83 => "created a Stage in",
        84 => "updated a Stage in",
        85 => "ended a Stage in",
        90 => "created sticker",
        91 => "updated sticker",
        92 => "deleted sticker",
        100 => "created event",
        101 => "updated event",
        102 => "deleted event",
        110 => "created thread",
        111 => "updated thread",
        112 => "deleted thread",
        121 => "updated command permissions for",
        130 => "created sound",
        131 => "updated sound",
        132 => "deleted sound",
        140 => "created AutoMod rule",
        141 => "updated AutoMod rule",
        142 => "deleted AutoMod rule",
        143 => "blocked a message from",
        144 => "flagged a message from",
        145 => "timed out",
        _ => "made a change",
    }
}

fn icon(n: u64) -> Icon {
    match n {
        10 | 30 | 40 | 50 | 60 | 90 | 100 | 110 | 83 | 80 | 130 | 140 => Icon::CirclePlus,
        12 | 32 | 42 | 52 | 62 | 72 | 73 | 85 | 82 | 92 | 102 | 112 | 132 | 142 | 15 => Icon::Trash,
        20 | 21 | 22 | 27 => Icon::Leave,
        23 | 28 => Icon::PersonAdd,
        74 | 75 => Icon::Pin,
        143..=145 => Icon::Shield,
        _ => Icon::Pencil,
    }
}

fn nice(key: &str) -> String {
    let mut s = key.replace('_', " ");
    if let Some(c) = s.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    s
}

fn short(v: &Value) -> String {
    match v {
        Value::Null => "none".into(),
        Value::String(s) if s.len() > 60 => format!("{}\u{2026}", s.chars().take(60).collect::<String>()),
        Value::String(s) => s.clone(),
        Value::Array(a) => format!("{} item{}", a.len(), if a.len() == 1 { "" } else { "s" }),
        Value::Object(_) => "\u{2026}".into(),
        v => v.to_string(),
    }
}

pub fn more(app: &mut App, v: Value) {
    let Some(s) = &mut app.server else { return };
    let Some(cur) = s.data.get_mut("audit") else { return };
    for k in ["audit_log_entries", "users"] {
        if let (Some(a), Some(b)) = (cur[k].as_array_mut(), v[k].as_array()) {
            a.extend(b.iter().cloned());
        }
    }
    if let (Some(a), Some(b)) = (cur["webhooks"].as_array_mut(), v["webhooks"].as_array()) {
        a.extend(b.iter().cloned());
    }
    s.data.insert("audit_more", json!(v["audit_log_entries"].as_array().map_or(0, |a| a.len())));
}

fn audit_path(gid: Id, (user, action): (Id, u64)) -> String {
    let mut path = format!("/guilds/{gid}/audit-logs?limit=50");
    if user != 0 {
        path += &format!("&user_id={user}");
    }
    if action != 0 {
        path += &format!("&action_type={action}");
    }
    path
}

pub fn changed(app: &mut App, gid: Id) {
    let Some(s) = app.server.as_ref().filter(|s| s.guild == gid && s.data.contains_key("audit")) else { return };
    let path = audit_path(gid, (s.pages.log.user, s.pages.log.action));
    app.rest("GET", path, None, Tag::Server("audit"));
}

pub fn audit(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Audit Log", "");
    let Some(s) = app.server.as_mut() else { return };
    let want = (s.pages.log.user, s.pages.log.action);
    if s.pages.log.asked != Some(want) {
        s.pages.log.asked = Some(want);
        s.data.remove("audit");
        s.data.remove("audit_more");
    }
    let path = audit_path(gid, want);
    let log = app.server_data("audit", path).cloned().unwrap_or(Value::Null);
    let users: HashMap<String, (String, String)> = log["users"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| (u["id"].as_str().unwrap_or_default().to_string(), (u["global_name"].as_str().or(u["username"].as_str()).unwrap_or_default().to_string(), u["avatar"].as_str().map(|h| format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), u["id"].as_str().unwrap_or_default())).unwrap_or_default())))
        .collect();
    let mut actors: Vec<(Id, String)> = users.iter().map(|(k, v)| (k.parse().unwrap_or(0), v.0.clone())).collect();
    actors.sort_by_key(|(_, n)| n.to_lowercase());
    let (mut user, mut action) = want;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        let w = (ui.available_width() - 16.0) / 2.0;
        ui.vertical(|ui| {
            ui.label(RichText::new("FILTER BY USER").font(bold(12.0)).color(pal().header_secondary));
            ui.add_space(4.0);
            let shown = if user == 0 { "All".to_string() } else { users.get(&user.to_string()).map(|u| u.0.clone()).unwrap_or_else(|| user.to_string()) };
            settings::dropdown(ui, w, None, &shown, |ui| {
                crate::ui::menu_body(ui, |ui| {
                    if crate::ui::menu_item(ui, (user == 0).then_some(Icon::Check), "All", false) {
                        user = 0;
                    }
                    for (id, n) in &actors {
                        if crate::ui::menu_item(ui, (user == *id).then_some(Icon::Check), n, false) {
                            user = *id;
                        }
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.label(RichText::new("FILTER BY ACTION").font(bold(12.0)).color(pal().header_secondary));
            ui.add_space(4.0);
            let shown = ACTIONS.iter().find(|a| a.0 == action).map_or("All Actions", |a| a.1);
            settings::dropdown(ui, w, None, shown, |ui| {
                crate::ui::menu_body(ui, |ui| {
                    if crate::ui::menu_item(ui, (action == 0).then_some(Icon::Check), "All Actions", false) {
                        action = 0;
                    }
                    for (n, l) in ACTIONS {
                        if crate::ui::menu_item(ui, (action == *n).then_some(Icon::Check), l, false) {
                            action = *n;
                        }
                    }
                });
            });
        });
    });
    ui.add_space(24.0);
    if let Some(s) = &mut app.server {
        s.pages.log.user = user;
        s.pages.log.action = action;
    }
    if log.is_null() {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    }
    let entries = log["audit_log_entries"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            let filtered = user != 0 || action != 0;
            ui.label(RichText::new(if filtered { "No logs found" } else { "No Logs Yet" }).font(semi(20.0)).color(pal().header_primary));
            ui.label(RichText::new(if filtered { "There are no audit log entries that match your filters." } else { "Once moderators begin moderating, you can moderate the moderation here." }).font(reg(14.0)).color(pal().header_secondary));
        });
        return;
    }
    let mut toggle = None;
    for e in &entries {
        let eid = e["id"].as_str().unwrap_or_default().to_string();
        let open = app.server.as_ref().is_some_and(|s| s.pages.log.open.contains(&eid));
        let who = users.get(e["user_id"].as_str().unwrap_or_default()).map_or("Someone".to_string(), |u| u.0.clone());
        let n = e["action_type"].as_u64().unwrap_or(0);
        let target = e["target_id"].as_str().and_then(|t| users.get(t)).map(|u| u.0.clone()).or_else(|| e["options"]["channel_id"].as_str().and_then(|c| c.parse::<Id>().ok()).and_then(|c| app.store.channels.get(&c)).map(|c| format!("#{}", c.name))).or_else(|| e["changes"].as_array().and_then(|c| c.iter().find(|c| c["key"] == "name")).and_then(|c| c["new_value"].as_str().or(c["old_value"].as_str())).map(str::to_string)).unwrap_or_default();
        let when = chrono::DateTime::from_timestamp_millis(snowflake_ms(sid(&e["id"]))).map(|t| t.with_timezone(&chrono::Local).format("%-m/%-d/%y, %-I:%M %p").to_string()).unwrap_or_default();
        let changes: Vec<Value> = e["changes"].as_array().cloned().unwrap_or_default();
        let resp = row(ui, 60.0, |ui, r, _| {
            let p = ui.painter();
            icons::draw(p, icon(n), Rect::from_min_size(pos2(r.left() + 12.0, r.center().y - 10.0), vec2(20.0, 20.0)), pal().header_secondary);
            let t = p.text(pos2(r.left() + 48.0, r.center().y - 9.0), Align2::LEFT_CENTER, &who, semi(16.0), pal().header_primary);
            let t = p.text(pos2(t.right() + 4.0, r.center().y - 9.0), Align2::LEFT_CENTER, verb(n), reg(16.0), pal().text_normal);
            p.text(pos2(t.right() + 4.0, r.center().y - 9.0), Align2::LEFT_CENTER, &target, semi(16.0), pal().header_primary);
            p.text(pos2(r.left() + 48.0, r.center().y + 12.0), Align2::LEFT_CENTER, &when, reg(12.0), pal().header_secondary);
            icons::draw(p, if open { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
        });
        if resp.clicked() {
            toggle = Some(eid.clone());
        }
        if open {
            egui::Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(r) = e["reason"].as_str() {
                    ui.label(RichText::new(format!("Reason: {r}")).font(medium(14.0)).color(pal().text_normal));
                    ui.add_space(6.0);
                }
                if changes.is_empty() && e["reason"].is_null() {
                    ui.label(RichText::new("No details for this entry.").font(reg(14.0)).color(pal().header_secondary));
                }
                for c in &changes {
                    let key = c["key"].as_str().unwrap_or_default();
                    let (old, new) = (short(&c["old_value"]), short(&c["new_value"]));
                    let line = match (c["old_value"].is_null(), c["new_value"].is_null()) {
                        (true, _) => format!("{}: {new}", nice(key)),
                        (_, true) => format!("{}: {old} \u{2192} none", nice(key)),
                        _ => format!("{}: {old} \u{2192} {new}", nice(key)),
                    };
                    ui.label(RichText::new(line).font(reg(14.0)).color(pal().text_normal));
                }
                if let Some(t) = e["target_id"].as_str() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("Target ID: {t}")).font(reg(12.0)).color(pal().header_secondary));
                }
            });
            ui.add_space(8.0);
        }
    }
    if let Some(id) = toggle
        && let Some(s) = &mut app.server
        && !s.pages.log.open.remove(&id)
    {
        s.pages.log.open.insert(id);
    }
    ui.add_space(16.0);
    let exhausted = app.server.as_ref().and_then(|s| s.data.get("audit_more")).and_then(|v| v.as_u64()).is_some_and(|n| n < 50);
    if !exhausted && entries.len() >= 50 && settings::secondary(ui, "Load More").clicked() {
        let before = entries.last().and_then(|e| e["id"].as_str()).unwrap_or_default().to_string();
        let mut path = format!("/guilds/{gid}/audit-logs?limit=50&before={before}");
        if user != 0 {
            path += &format!("&user_id={user}");
        }
        if action != 0 {
            path += &format!("&action_type={action}");
        }
        app.rest("GET", path, None, Tag::Server("audit_more"));
    }
}
