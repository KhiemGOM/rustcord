use egui::{RichText, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, members, rule, title};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::icons::Icon;
use crate::ui::settings;

#[derive(Default)]
pub struct Safety {
    open_level: bool,
    open_filter: bool,
    sub: u8,
}

impl Safety {
    pub fn deep(&self) -> bool {
        self.sub != 0
    }
}

const LEVELS: [(u8, &str, &str); 5] = [
    (0, "None", "Unrestricted"),
    (1, "Low", "Must have a verified email on their Discord account."),
    (2, "Medium", "Must also be registered on Discord for longer than 5 minutes."),
    (3, "High", "Must also be a member of this server for longer than 10 minutes."),
    (4, "Highest", "Must have a verified phone on their Discord account."),
];

const FILTERS: [(u8, &str, &str); 3] = [
    (0, "Do not filter", "Messages will not be filtered for sensitive image-based media."),
    (2, "Filter messages from all members", "All messages will be filtered for sensitive image-based media."),
    (1, "Filter messages from server members without roles", "Messages from server members without a role will be filtered for sensitive image-based media."),
];

const ALERTS_OFF: &str = "RAID_ALERTS_DISABLED";
const ALERTS_ON: &str = "NON_COMMUNITY_RAID_ALERTS";

fn alerts_on(g: &crate::state::Guild) -> bool {
    if g.features.iter().any(|f| f == "COMMUNITY") { !g.features.iter().any(|f| f == ALERTS_OFF) } else { g.features.iter().any(|f| f == ALERTS_ON) }
}
const PRUNE_ADMIN: &str = "PRUNE_REQUIRES_ADMIN";

fn choose(ui: &mut Ui, open: &mut bool, value: &mut u8, options: &[(u8, &str, &str)]) -> bool {
    let mut changed = false;
    let cur = options.iter().find(|o| o.0 == *value).copied().unwrap_or(options[0]);
    if !*open {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(cur.1).font(medium(16.0)).color(pal().header_primary));
                ui.label(RichText::new(cur.2).font(reg(14.0)).color(pal().header_secondary));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if settings::secondary(ui, "Change").clicked() {
                    *open = true;
                }
            });
        });
    } else {
        let list: Vec<(u8, &str, &str)> = options.to_vec();
        let mut v = *value;
        if settings::radio(ui, &mut v, &list) {
            *value = v;
            *open = false;
            changed = true;
        }
    }
    changed
}

fn patch(app: &mut App, gid: Id, body: Value) {
    app.rest("PATCH", format!("/guilds/{gid}"), Some(body), Tag::Quiet);
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let community = g.features.iter().any(|f| f == "COMMUNITY");
    let sub = app.server.as_ref().map_or(0, |s| s.pages.safety.sub);
    if community && sub != 0 {
        let name = ["", "Raid Protection and CAPTCHA", "DM and Spam Protection", "Permissions"][sub as usize];
        let back = ui.add(egui::Label::new(RichText::new("\u{2190} Back").font(semi(14.0)).color(pal().header_secondary)).sense(egui::Sense::click())).clicked();
        ui.add_space(12.0);
        title(ui, name, "");
        if back && let Some(s) = &mut app.server {
            s.pages.safety.sub = 0;
        }
    } else {
        title(ui, "Safety Setup", "");
    }
    let info = app.server_data("guild", format!("/guilds/{gid}")).cloned().unwrap_or(Value::Null);
    if info.is_null() {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    }
    let owner = g.owner == app.store.me;
    if !community {
        if members::list_toggle(app, ui, gid, "Show Members In Channel List") {
            ui.add_space(8.0);
            ui.label(RichText::new("Becoming a community will automatically enable this experience for you.").font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(24.0);
        }
        level_block(app, ui, gid, &info, false);
        rule(ui);
        mfa_prune(app, ui, gid, &g, &info, owner);
        rule(ui);
        filter_block(app, ui, gid, &info);
        rule(ui);
        ui.label(RichText::new("Activity Alerts").font(semi(24.0)).color(pal().header_primary));
        ui.add_space(8.0);
        alerts(app, ui, gid, &g, &info);
        return;
    }
    match sub {
        0 => overview(app, ui, gid, &g, &info),
        1 => {
            alerts(app, ui, gid, &g, &info);
            ui.add_space(24.0);
            status(ui, "CAPTCHA suspicious accounts before they are able to join", "");
            status(ui, "CAPTCHA all accounts before they can join during a suspected raid", "");
        }
        2 => {
            level_block(app, ui, gid, &info, true);
            ui.add_space(24.0);
            status(ui, "Hide DMs from suspicious users", "Send suspicious DMs to a separate Spam Inbox. Members can send a scam report or move it back to Inbox.");
            status(ui, "Filter DMs from unknown users", "Filter DMs from unknown users to Message Requests Inbox. Members must approve the request before they are able to respond.");
            status(ui, "Warn members before they visit outbound links", "");
            status(ui, "Hide all messages from and delete suspected spammers", "");
            rule(ui);
            filter_block(app, ui, gid, &info);
        }
        _ => {
            mfa_prune(app, ui, gid, &g, &info, owner);
            ui.add_space(24.0);
            risky(app, ui, gid);
        }
    }
}

fn status(ui: &mut Ui, head: &str, desc: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), if desc.is_empty() { 32.0 } else { 56.0 }), egui::Sense::hover());
    crate::ui::icons::draw(ui.painter(), Icon::CircleCheck, egui::Rect::from_center_size(pos2(r.left() + 10.0, r.top() + 16.0), vec2(20.0, 20.0)), GREEN);
    ui.painter().text(pos2(r.left() + 32.0, r.top() + 16.0), egui::Align2::LEFT_CENTER, head, medium(16.0), pal().header_primary);
    if !desc.is_empty() {
        crate::ui::text::paint(ui.painter(), pos2(r.left() + 32.0, r.top() + 38.0), egui::Align2::LEFT_CENTER, desc, reg(14.0), pal().header_secondary, r.width() - 40.0);
    }
}

fn overview(app: &mut App, ui: &mut Ui, gid: Id, g: &crate::state::Guild, info: &Value) {
    let off = !alerts_on(g);
    let mfa = info["mfa_level"].as_u64() == Some(1);
    let prune = g.features.iter().any(|f| f == PRUNE_ADMIN);
    let everyone = g.roles.get(&gid).map_or(0, |r| r.perms);
    let safe = everyone & RISKY == 0;
    let rules = app.server_data("automod", format!("/guilds/{gid}/auto-moderation/rules")).and_then(|v| v.as_array()).map(|a| (a.iter().filter(|r| r["enabled"].as_bool() == Some(true)).count(), a.len()));
    let cards: [(&str, String, u8); 4] = [
        ("Raid Protection and CAPTCHA", format!("{} of 3 enabled", 2 + !off as u8), 1),
        ("DM and Spam Protection", "5 of 5 enabled".into(), 2),
        ("AutoMod", rules.map_or("Not set up".into(), |(on, all)| format!("{on} of {all} enabled")), 4),
        ("Permissions", format!("{} of 3 enabled", mfa as u8 + prune as u8 + safe as u8), 3),
    ];
    let mut go = None;
    for (head, sub, page) in cards {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), egui::Sense::click());
        ui.painter().rect(r, egui::CornerRadius::same(8), crate::ui::motion::mix(pal().bg_secondary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)), egui::Stroke::new(1.0, super::faint(51)), egui::StrokeKind::Inside);
        ui.painter().text(pos2(r.left() + 20.0, r.center().y - 10.0), egui::Align2::LEFT_CENTER, head, semi(16.0), pal().header_primary);
        ui.painter().text(pos2(r.left() + 20.0, r.center().y + 12.0), egui::Align2::LEFT_CENTER, sub, medium(12.0), pal().header_secondary);
        ui.painter().text(pos2(r.right() - 20.0, r.center().y), egui::Align2::RIGHT_CENTER, "Edit", medium(14.0), pal().link);
        if resp.clicked() {
            go = Some(page);
        }
        ui.add_space(8.0);
    }
    match go {
        Some(4) => {
            if let Some(s) = &mut app.server {
                s.page = super::Page::AutoMod;
            }
        }
        Some(p) => {
            if let Some(s) = &mut app.server {
                s.pages.safety.sub = p;
            }
        }
        None => {}
    }
}

const RISKY: u64 = (1 << 3) | (1 << 5) | (1 << 4) | (1 << 28) | (1 << 1) | (1 << 2) | (1 << 13) | (1 << 17);

fn risky(app: &mut App, ui: &mut Ui, gid: Id) {
    let everyone = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&gid)).map_or(0, |r| r.perms);
    let left = everyone & RISKY;
    settings::sub(ui, "Disable risky permissions for @everyone");
    ui.label(RichText::new("This will remove the following permissions from the @everyone role: Administrator, Manage Server, Manage Channels, Manage Roles, Kick Members, Ban Members, Manage Messages, and Mention @everyone, @here, and All Roles.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(12.0);
    if ui.add_enabled_ui(left != 0, |ui| settings::primary(ui, "Disable")).inner.clicked() {
        let perms = everyone & !RISKY;
        app.rest("PATCH", format!("/guilds/{gid}/roles/{gid}"), Some(json!({ "permissions": perms.to_string() })), Tag::Quiet);
        if let Some(r) = app.store.guilds.get_mut(&gid).and_then(|g| g.roles.get_mut(&gid)) {
            r.perms = perms;
        }
    }
}

fn level_block(app: &mut App, ui: &mut Ui, gid: Id, info: &Value, community: bool) {
    let mut open = app.server.as_ref().is_some_and(|s| s.pages.safety.open_level);
    settings::sub(ui, "Verification Level");
    ui.label(RichText::new("Members of the server must meet the following criteria before they can send messages in text channels or initiate a direct message conversation. If a member has an assigned role and server onboarding is not enabled, this does not apply.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    if !community {
        ui.label(RichText::new("We recommend setting a verification level for a Community Server.").font(bold(14.0)).color(pal().header_secondary));
        ui.add_space(8.0);
    }
    ui.add_space(8.0);
    let mut level = info["verification_level"].as_u64().unwrap_or(0) as u8;
    if choose(ui, &mut open, &mut level, &LEVELS) {
        patch(app, gid, json!({ "verification_level": level }));
        set_info(app, "verification_level", json!(level));
    }
    if let Some(s) = &mut app.server {
        s.pages.safety.open_level = open;
    }
}

fn mfa_prune(app: &mut App, ui: &mut Ui, gid: Id, g: &crate::state::Guild, info: &Value, owner: bool) {
    let mut mfa = info["mfa_level"].as_u64().unwrap_or(0) == 1;
    ui.add_enabled_ui(owner, |ui| {
        if settings::toggle(ui, &mut mfa, "Require 2FA for moderator actions", "Moderators must have two-factor authentication enabled to ban, kick, or timeout members and delete messages. Only the server owner can change this setting if they have 2FA enabled.") {
            settings::guarded(app, "POST", &format!("/guilds/{gid}/mfa"), Some(json!({ "level": mfa as u8 })));
            set_info(app, "mfa_level", json!(mfa as u8));
        }
    });
    ui.add_space(24.0);
    let mut prune = g.features.iter().any(|f| f == PRUNE_ADMIN);
    if settings::toggle(ui, &mut prune, "Restrict member prune to admins", "Only administrators and the server owner can prune inactive members when this setting is enabled.") {
        features(app, gid, PRUNE_ADMIN, prune);
    }
}

fn filter_block(app: &mut App, ui: &mut Ui, gid: Id, info: &Value) {
    let mut open = app.server.as_ref().is_some_and(|s| s.pages.safety.open_filter);
    settings::sub(ui, "Sensitive content filters");
    ui.label(RichText::new("Choose if server members can share image-based media detected by Discord\u{2019}s sensitive content filters. This setting will apply to channels that are not age-restricted.").font(reg(14.0)).color(pal().header_secondary));
    if settings::link(ui, "Learn more").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/115000068672"));
    }
    ui.add_space(16.0);
    let mut filter = info["explicit_content_filter"].as_u64().unwrap_or(0) as u8;
    if choose(ui, &mut open, &mut filter, &FILTERS) {
        patch(app, gid, json!({ "explicit_content_filter": filter }));
        set_info(app, "explicit_content_filter", json!(filter));
    }
    if let Some(s) = &mut app.server {
        s.pages.safety.open_filter = open;
    }
}

fn alerts(app: &mut App, ui: &mut Ui, gid: Id, g: &crate::state::Guild, info: &Value) {
    let mut on = alerts_on(g);
    if settings::toggle(ui, &mut on, "Receive notifications for DM or join activity that exceeds usual numbers for your server.", "Each notification will contain information about the activity, including time period and approximate number of joins or DMs.") {
        if g.features.iter().any(|f| f == "COMMUNITY") {
            features(app, gid, ALERTS_OFF, !on);
        } else {
            features(app, gid, ALERTS_ON, on);
        }
    }
    ui.add_space(24.0);
    settings::h5(ui, "Safety Notifications Channel");
    let chans: Vec<(Id, String)> = g.channels.iter().filter_map(|c| app.store.channels.get(c)).filter(|c| c.kind == 0).map(|c| (c.id, c.name.clone())).collect();
    let cur = sid(&info["safety_alerts_channel_id"]);
    let shown = chans.iter().find(|c| c.0 == cur).map(|c| c.1.clone()).unwrap_or_else(|| "Select a Safety Notifications channel".into());
    let mut pick = None;
    settings::dropdown(ui, ui.available_width().min(480.0), Some(Icon::Hash), &shown, |ui| {
        crate::ui::menu_body(ui, |ui| {
            for (c, n) in &chans {
                if crate::ui::menu_item(ui, (*c == cur).then_some(Icon::Check), n, false) {
                    pick = Some(*c);
                }
            }
        });
    });
    ui.add_space(8.0);
    ui.label(RichText::new("Anyone with access to this text channel will be able to see the notifications.").font(reg(14.0)).color(pal().header_secondary));
    if let Some(c) = pick {
        patch(app, gid, json!({ "safety_alerts_channel_id": c.to_string() }));
        set_info(app, "safety_alerts_channel_id", json!(c.to_string()));
    }
}

fn set_info(app: &mut App, key: &str, v: Value) {
    if let Some(Value::Object(o)) = app.server.as_mut().and_then(|s| s.data.get_mut("guild")) {
        o.insert(key.into(), v);
    }
}

fn features(app: &mut App, gid: Id, flag: &str, on: bool) {
    let mut feats = app.store.guilds.get(&gid).map(|g| g.features.clone()).unwrap_or_default();
    feats.retain(|f| f != flag);
    if on {
        feats.push(flag.into());
    }
    if let Some(g) = app.store.guilds.get_mut(&gid) {
        g.features = feats.clone();
    }
    patch(app, gid, json!({ "features": feats }));
}
