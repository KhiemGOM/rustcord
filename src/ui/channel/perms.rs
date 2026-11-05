use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::{App, State, faint, heading, rule, same_perms};
use crate::state::{CONNECT, Channel, Id, Overwrite, Store, VIEW_CHANNEL};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;
use crate::ui::{bolded, menu_body};

type Perm = (u64, &'static str, &'static str);

const SEND: u64 = 1 << 11;
const SEND_THREADS: u64 = 1 << 38;
const MANAGE_CHANNELS: u64 = 1 << 4;
const MANAGE_THREADS: u64 = 1 << 34;

// discord's sections and wording for a channel's permissions
const GENERAL: [Perm; 4] = [
    (VIEW_CHANNEL, "View Channel", "Allows members to view this channel by default. Disabling this for @everyone will make this channel private."),
    (MANAGE_CHANNELS, "Manage Channel", "Allows members to change this channel’s name, description, and text settings. They can also delete the channel."),
    (1 << 28, "Manage Permissions", "Allows members to change this channel’s permissions."),
    (1 << 29, "Manage Webhooks", "Allows members to create, edit, or delete webhooks in this channel, which can post messages from other apps or sites."),
];
const MEMBERSHIP: [Perm; 1] = [(1, "Create Invite", "Allows members to invite new people to this server via a direct invite link to this channel.")];
const TEXT: [Perm; 18] = [
    (SEND, "Send Messages", "Allows members to send messages in this channel."),
    (SEND_THREADS, "Send Messages in Threads", "Allow members to send messages in threads under this channel."),
    (1 << 35, "Create Public Threads", "Allow members to create threads that everyone in this channel can view."),
    (1 << 36, "Create Private Threads", "Allow members to create invite-only threads in this channel."),
    (1 << 14, "Embed Links", "Allows links that members share to show embedded content in this channel."),
    (1 << 15, "Attach Files", "Allows members to upload files or media in this channel."),
    (1 << 6, "Add Reactions", "Allows members to add new emoji reactions to a message in this channel. If this permission is disabled, members can still react using any existing reactions on a message."),
    (1 << 18, "Use External Emoji", "Allows members to use emoji from other servers in this channel, if they’re a Discord Nitro member."),
    (1 << 37, "Use External Stickers", "Allows members to use stickers from other servers in this channel, if they’re a Discord Nitro member."),
    (1 << 17, "Mention @everyone, @here, and All Roles", "Allows members to use @everyone or @here (only online members) in this channel. They can also @mention all roles in this channel, even if the role’s “Allow anyone to mention this role” permission is disabled."),
    (1 << 13, "Manage Messages", "Allows members to delete or remove embeds from messages by other members in this channel."),
    (1 << 51, "Pin Messages", "Allows members to pin or unpin any message in this channel."),
    (1 << 52, "Bypass Slowmode", "Allows members to send messages in this channel without being affected by slowmode."),
    (MANAGE_THREADS, "Manage Threads", "Allows members to rename, delete, close, and turn on slow mode for threads in this channel. They can also view private threads."),
    (1 << 16, "Read Message History", "Allows members to read previous messages sent in this channel. If this permission is disabled, members only see messages sent when they are online."),
    (1 << 12, "Send Text-to-Speech Messages", "Allows members to send text-to-speech messages in this channel by starting a message with /tts. These messages can be heard by anyone focused on this channel."),
    (1 << 46, "Send Voice Messages", "Allows members to send voice messages in this channel. Requires the \"Attach Files\" permission as well."),
    (1 << 49, "Create Polls", "Allows members to create polls in this channel."),
];
const VOICE: [Perm; 11] = [
    (CONNECT, "Connect", "Allows members to join this voice channel and hear others. Disabling this and the View Channel permissions for @everyone will make this channel private."),
    (1 << 21, "Speak", "Allows members to talk in this voice channel. If this permission is disabled, members are default muted until somebody with the “Mute Members” permission un-mutes them."),
    (1 << 9, "Video", "Allows members to share their video, screen share, or stream a game in this voice channel."),
    (1 << 42, "Use Soundboard", "Allows members to send sounds from server soundboard in this voice channel."),
    (1 << 45, "Use External Sounds", "Allows members to use sounds from other servers, if they're a Discord Nitro member."),
    (1 << 25, "Use Voice Activity", "Allows members to speak in this voice channel by simply talking. If this permission is disabled, members are required to use Push-to-talk. Good for controlling background noise or noisy members."),
    (1 << 8, "Priority Speaker", "Allows members to be more easily heard in this voice channel. When activated, the volume of others without this permission will be automatically lowered. Priority Speaker is activated by using the Push to Talk (Priority) keybind."),
    (1 << 22, "Mute Members", "Allows members to mute other members in this voice channel for everyone."),
    (1 << 23, "Deafen Members", "Allows members to deafen other members in this voice channel, which means they won’t be able to speak or hear others."),
    (1 << 24, "Move Members", "Allows members to disconnect other members from this channel. They will also be able to move somebody into other channels that they have this permission in."),
    (1 << 48, "Set Voice Channel Status", "Allows members to create and edit voice channel status."),
];
const STAGE: [Perm; 5] = [
    (CONNECT, "Connect", "Allow members to join this Stage channel as the audience. Disabling this and the View Channel permissions for @everyone will make this channel private."),
    (1 << 9, "Video", "Allows speakers to share their video, screen share, or stream a game in this Stage channel."),
    (1 << 22, "Mute Members", "Allows members to mute other members in this Stage channel for everyone."),
    (1 << 24, "Move Members", "Allows members to disconnect other members from this channel. They will also be able to move somebody into other channels that they have this permission in."),
    (1 << 32, "Request to Speak", "Allow requests to speak in this Stage channel. Stage moderators manually approve or deny each request."),
];
const APPS: [Perm; 3] = [
    (1 << 31, "Use Application Commands", "Allows members to use commands from applications in this channel, including slash commands and context menu commands."),
    (1 << 39, "Use Activities", "Allows members to use Activities in this channel."),
    (1 << 50, "Use External Apps", "Allows apps that members have added to their account to post messages in this channel. When disabled, the messages will be private."),
];
const EVENTS: [Perm; 2] = [
    (1 << 44, "Create Events", "Allows members to create events in this channel."),
    (1 << 33, "Manage Events", "Allows members to edit and cancel events in this channel."),
];

// where a category, or a forum, says it differently from a text channel; written for "these channels"
const PLURAL: [Perm; 11] = [
    (VIEW_CHANNEL, "View Channels", "Allows members to view these channels by default. Disabling this for @everyone will make text channels private. Disabling this and the Connect permission for @everyone will make voice channels private."),
    (MANAGE_CHANNELS, "Manage Channels", "Allows members to change these channels’ name, description, and text / voice settings. They can also delete these channels."),
    (SEND, "Send Messages and Create Posts", "Allow members to send messages and create posts in these channels."),
    (SEND_THREADS, "Send Messages in Threads and Posts", "Allow members to send messages in threads and posts under these channels."),
    (MANAGE_THREADS, "Manage Threads and Posts", "Allows members to rename, delete, close, and turn on slow mode for threads and posts in these channels. They can also view private threads."),
    (1 << 12, "Send Text-to-Speech Messages", "Allows members to send text-to-speech messages in these channels by starting a message with /tts. These messages can be heard by anyone focused on that channel."),
    (CONNECT, "Connect", "Allows members to join these voice channels and hear others. Disabling this and the View Channel permissions for @everyone will make these channels private."),
    (1 << 42, "Use Soundboard", "Allows members to send sounds from server soundboard in these voice channels."),
    (1 << 8, "Priority Speaker", "Allows members to be more easily heard in these voice channels. When activated, the volume of others without this permission will be automatically lowered. Priority Speaker is activated by using the Push to Talk (Priority) keybind."),
    (1 << 24, "Move Members", "Allows members to disconnect other members from these channels. They will also be able to move somebody into other channels that they have this permission in."),
    (1 << 22, "Mute Members", "Allows members to mute other members in these voice channels for everyone."),
];

fn adapt(kind: u8, p: &Perm) -> (String, String) {
    let (bit, name, desc) = *p;
    let plural = PLURAL.iter().find(|o| o.0 == bit);
    match kind {
        4 => match plural {
            Some(o) => (o.1.into(), o.2.into()),
            None => (name.into(), desc.replace("this channel’s", "these channels’").replace("this voice channel", "these voice channels").replace("this channel", "these channels")),
        },
        15 | 16 if matches!(bit, SEND | SEND_THREADS | MANAGE_THREADS) => {
            let o = plural.unwrap_or(p);
            (o.1.into(), o.2.replace("these channels", "this channel"))
        }
        13 if bit == MANAGE_CHANNELS => (name.into(), "(Required for Stage moderator) Allow members to set a topic to start the Stage and enable or disable requests to speak. They can also delete the Stage channel.".into()),
        _ => (name.into(), desc.into()),
    }
}

fn sections(kind: u8) -> Vec<(&'static str, &'static [Perm])> {
    let general = if kind == 4 { "General Category Permissions" } else { "General Channel Permissions" };
    let text = match kind {
        15 => "Forum Channel Permissions",
        16 => "Media Channel Permissions",
        _ => "Text Channel Permissions",
    };
    let mut v: Vec<(&str, &[Perm])> = vec![(general, &GENERAL), ("Membership Permissions", &MEMBERSHIP)];
    match kind {
        2 => v.push(("Voice Channel Permissions", &VOICE)),
        13 => v.push(("Stage Channel Permissions", &STAGE)),
        _ => {}
    }
    v.push((text, &TEXT));
    if kind == 4 {
        v.push(("Voice Channel Permissions", &VOICE));
    }
    v.push(("Apps Permissions", &APPS));
    if matches!(kind, 2 | 4 | 13) {
        v.push(("Events Permissions", &EVENTS));
    }
    v
}

type Role = (Id, String, u32, i32);

fn panel(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(Margin::symmetric(20, 18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

/// Sets one permission in someone's overwrite: -1 deny, 0 neither, 1 allow.
fn set(perms: &mut Vec<Overwrite>, id: Id, kind: u8, bit: u64, v: i8) {
    let i = match perms.iter().position(|o| o.id == id) {
        Some(i) => i,
        None => {
            perms.push(Overwrite { id, kind, allow: 0, deny: 0 });
            perms.len() - 1
        }
    };
    let o = &mut perms[i];
    o.allow &= !bit;
    o.deny &= !bit;
    match v {
        1 => o.allow |= bit,
        -1 => o.deny |= bit,
        _ => {}
    }
}

/// The roles and members a picker offers, from the ones not in the list yet. Hands back a pick.
fn picker(resp: &egui::Response, s: &mut State, store: &Store, gid: Id, roles: &[Role], title: &str, search: &mut Option<String>) -> Option<(Id, u8)> {
    let mut out = None;
    egui::Popup::menu(resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).width(300.0).show(|ui| {
        ui.label(RichText::new(title).font(semi(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        settings::input(ui, &mut s.find, "Search roles or members", false);
        ui.add_space(8.0);
        let q = s.find.to_lowercase();
        let q = q.trim();
        if !q.is_empty() && s.data.get("msearch_q").and_then(|v| v.as_str()) != Some(q) {
            s.data.insert("msearch_q", Value::String(q.to_string()));
            *search = Some(q.to_string());
        }
        let held = |id: Id| s.perms.iter().any(|o| o.id == id);
        let mut left: Vec<&Role> = roles.iter().filter(|r| r.0 != gid && !held(r.0) && r.1.to_lowercase().contains(q)).collect();
        left.sort_by_key(|r| std::cmp::Reverse(r.3));
        let mut people: Vec<(Id, String)> = Vec::new();
        for m in s.data.get("msearch").and_then(|v| v.as_array()).into_iter().flatten().filter(|_| !q.is_empty()) {
            let u = &m["user"];
            let id = crate::state::sid(&u["id"]);
            let name = m["nick"].as_str().or(u["global_name"].as_str()).or(u["username"].as_str()).unwrap_or("Unknown User");
            people.push((id, name.to_string()));
        }
        for id in store.members.get(&gid).into_iter().flat_map(|m| m.keys()) {
            let name = store.name_in(gid, *id);
            if name.to_lowercase().contains(q) && !people.iter().any(|p| p.0 == *id) && people.len() < 30 {
                people.push((*id, name));
            }
        }
        people.retain(|p| !held(p.0));
        menu_body(ui, |ui| {
            ui.set_max_height(280.0);
            for (head, n) in [("Roles", left.len()), ("Members", people.len())] {
                if n == 0 {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(head).font(semi(12.0)).color(pal().header_secondary));
                for i in 0..n {
                    let (name, id, kind) = if head == "Roles" { (left[i].1.clone(), left[i].0, 0u8) } else { (people[i].1.clone(), people[i].0, 1u8) };
                    let (r, rr) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    if rr.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    crate::ui::text::paint(ui.painter(), pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &name, medium(14.0), pal().header_primary, r.width() - 16.0);
                    settings::mark(ui, &format!("chanset:pick:{name}"), r);
                    if rr.clicked() {
                        out = Some((id, kind));
                    }
                }
            }
        });
    });
    if out.is_some() {
        egui::Popup::close_all(&resp.ctx);
        s.find.clear();
    }
    out
}

/// A permission with discord's ✕ / ⁄ / ✓ switch on the right. Hands back what was clicked.
fn perm_row(ui: &mut Ui, name: &str, desc: &str, cur: i8) -> Option<i8> {
    let w = ui.available_width();
    let g = ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, w - 160.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 30.0 + g.size().y), Sense::hover());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, name, medium(16.0), pal().header_primary);
    ui.painter().galley(r.left_top() + vec2(0.0, 26.0), g, pal().header_secondary);
    let group = egui::Rect::from_min_size(pos2(r.right() - 96.0, r.top()), vec2(96.0, 28.0));
    let mut out = None;
    for (k, v) in [-1i8, 0, 1].into_iter().enumerate() {
        let cell = egui::Rect::from_min_size(pos2(group.left() + k as f32 * 32.0, group.top()), vec2(32.0, 28.0));
        let resp = ui.interact(cell, ui.id().with(("perm", name, k)), Sense::click());
        let on = cur == v;
        let round = match k {
            0 => CornerRadius { nw: 4, sw: 4, ..Default::default() },
            2 => CornerRadius { ne: 4, se: 4, ..Default::default() },
            _ => CornerRadius::ZERO,
        };
        let fill = match (on, v) {
            (true, -1) => Color32::from_rgb(0xd1, 0x2e, 0x38),
            (true, 1) => Color32::from_rgb(0x00, 0x85, 0x45),
            (true, _) => faint(61),
            (false, _) if resp.hovered() => faint(31),
            _ => pal().bg_primary,
        };
        ui.painter().rect_filled(cell, round, fill);
        let ink = match (on, v) {
            (true, 0) => Color32::from_rgb(0xdb, 0xdb, 0xde),
            (true, _) => Color32::WHITE,
            (false, -1) => pal().critical,
            (false, 1) => pal().positive,
            _ => pal().header_secondary,
        };
        let c = cell.center();
        let p = ui.painter();
        match v {
            -1 => {
                p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], Stroke::new(2.0, ink));
                p.line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], Stroke::new(2.0, ink));
            }
            0 => {
                p.line_segment([c + vec2(3.0, -6.0), c + vec2(-3.0, 6.0)], Stroke::new(2.0, ink));
            }
            _ => {
                p.add(egui::Shape::line(vec![c + vec2(-6.0, 0.0), c + vec2(-2.0, 4.0), c + vec2(6.0, -4.0)], Stroke::new(2.0, ink)));
            }
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        settings::mark(ui, &format!("perm:{name}:{v}"), cell);
        if resp.clicked() && !on {
            out = Some(v);
        }
    }
    ui.painter().rect_stroke(group, CornerRadius::same(4), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
    ui.add_space(20.0);
    let (line, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0, faint(31));
    ui.add_space(20.0);
    out
}

pub fn permissions(app: &mut App, ui: &mut Ui, c: &Channel) {
    let cat = c.kind == 4;
    heading(ui, if cat { "Category Settings" } else { "Channel Permissions" }, if cat { "Use permissions to customize who can do what in this category." } else { "Use permissions to customize who can do what in this channel." });
    let gid = c.guild;
    let parent = app.store.channels.get(&c.parent).filter(|_| !cat).cloned();
    let admin = app.store.perms(c) == u64::MAX;
    let me = app.store.me;
    let roles: Vec<Role> = app.store.guilds.get(&gid).map(|g| g.roles.values().map(|r| (r.id, r.name.clone(), r.color, r.position)).collect()).unwrap_or_default();
    let store = &app.store;
    let Some(s) = app.chan_settings.as_mut() else { return };
    let role_pos = |id: Id| roles.iter().find(|r| r.0 == id).map_or(-1, |r| r.3);
    let name_of = |id: Id, kind: u8| if id == gid { "@everyone".to_string() } else if kind == 1 { store.name_in(gid, id) } else { roles.iter().find(|r| r.0 == id).map_or_else(|| "Unknown Role".into(), |r| r.1.clone()) };
    let mut search = None;
    let mut remove = None;
    if let Some(p) = &parent {
        let synced = same_perms(&s.perms, &p.overwrites);
        let mut sync = false;
        panel(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Refresh, r, if synced { Color32::from_rgb(0xf0, 0xb2, 0x32) } else { pal().header_secondary });
                ui.add_space(4.0);
                let text = format!("Permissions {} with category: **{}**", if synced { "synced" } else { "not synced" }, p.name);
                ui.label(bolded(&text, 16.0, pal().text_normal, ui.available_width() - 120.0));
                if !synced {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| sync = settings::primary(ui, "Sync Now").clicked());
                }
            });
        });
        if sync {
            s.perms = p.overwrites.clone();
        }
        ui.add_space(20.0);
    }
    let bits = VIEW_CHANNEL | if c.is_voice() { CONNECT } else { 0 };
    let everyone = s.perms.iter().find(|o| o.id == gid).map_or(0, |o| o.deny);
    let mut private = everyone & VIEW_CHANNEL != 0;
    panel(ui, |ui| {
        let on = private;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 24.0), Sense::hover());
            icons::draw(ui.painter(), Icon::Lock, egui::Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
            ui.add_space(4.0);
            ui.label(RichText::new(if cat { "Private Category" } else { "Private Channel" }).font(semi(16.0)).color(pal().header_primary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let sw = settings::switch(ui, on);
                settings::mark(ui, "toggle:private", sw.rect);
                if sw.clicked() {
                    private = !on;
                }
            });
        });
        ui.add_space(6.0);
        let text = if cat {
            "By making a category private, only select members and roles will be able to view this category. Linked channels in this category will automatically match to this setting."
        } else if c.kind == 2 {
            "By making a channel private, only select members and roles will have access to view or connect to this channel."
        } else {
            "By making a channel private, only select members and roles will be able to view this channel."
        };
        ui.label(RichText::new(text).font(reg(12.0)).line_height(Some(18.0)).color(pal().text_normal));
    });
    if private != (everyone & VIEW_CHANNEL != 0) {
        if private {
            set(&mut s.perms, gid, 0, bits, -1);
            if !admin {
                set(&mut s.perms, me, 1, bits, 1);
            }
        } else {
            set(&mut s.perms, gid, 0, bits, 0);
        }
    }
    if private {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(if cat { "Who can access this category?" } else { "Who can access this channel?" }).font(semi(14.0)).color(pal().header_primary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let add = settings::primary(ui, "Add members or roles");
                settings::mark(ui, "chanset:add_access", add.rect);
                if let Some((id, kind)) = picker(&add, s, store, gid, &roles, "Add:", &mut search) {
                    set(&mut s.perms, id, kind, bits, 1);
                }
            });
        });
        ui.add_space(8.0);
        let mut who: Vec<&Overwrite> = s.perms.iter().filter(|o| o.id != gid && o.allow & VIEW_CHANNEL != 0).collect();
        who.sort_by_key(|o| (o.kind, std::cmp::Reverse(role_pos(o.id))));
        for o in who {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            let name = name_of(o.id, o.kind);
            crate::ui::text::paint(ui.painter(), pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &name, medium(14.0), pal().header_primary, r.width() - 160.0);
            ui.painter().text(pos2(r.right() - 40.0, r.center().y), Align2::RIGHT_CENTER, if o.kind == 1 { "Member" } else { "Role" }, reg(12.0), pal().header_secondary);
            let x = egui::Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(24.0, 24.0));
            let xr = ui.interact(x, ui.id().with(("revoke", o.id)), Sense::click());
            icons::draw(ui.painter(), Icon::Close, egui::Rect::from_center_size(x.center(), vec2(16.0, 16.0)), if xr.hovered() { pal().critical } else { pal().header_secondary });
            settings::mark(ui, &format!("chanset:revoke:{name}"), x);
            if xr.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if xr.clicked() {
                remove = Some((o.id, name));
            }
            ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, faint(31)));
        }
    }
    ui.add_space(32.0);
    rule(ui);
    ui.add_space(32.0);
    let head = ui.horizontal(|ui| {
        ui.label(RichText::new("Advanced permissions").font(semi(20.0)).color(pal().header_primary));
        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        crate::ui::motion::chevron(ui, resp.id, s.advanced, egui::Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
    });
    let hr = ui.interact(head.response.rect, ui.id().with("adv_head"), Sense::click());
    settings::mark(ui, "chanset:advanced", hr.rect);
    if hr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if hr.clicked() {
        s.advanced = !s.advanced;
    }
    if s.advanced {
        ui.add_space(24.0);
        advanced(ui, s, c, store, &roles, &mut search);
    }
    if let Some(q) = search {
        app.rest("GET", format!("/guilds/{gid}/members/search?query={}&limit=10", crate::ui::directory::urlencode(&q)), None, crate::net::Tag::Chan("msearch"));
    }
    if let Some((id, name)) = remove {
        let body = format!("Are you sure you want to delete **{name}** permissions? This action cannot be undone.");
        app.confirm = Some(crate::ui::Confirm::new("Delete Permission Settings", body, "Delete", move |app: &mut App| {
            if let Some(s) = &mut app.chan_settings {
                s.perms.retain(|o| o.id != id);
            }
        }));
    }
}

fn advanced(ui: &mut Ui, s: &mut State, c: &Channel, store: &Store, roles: &[Role], search: &mut Option<String>) {
    let gid = c.guild;
    // who has an overwrite: @everyone first, then roles high to low, then members
    let mut who: Vec<(Id, u8)> = s.perms.iter().map(|o| (o.id, o.kind)).collect();
    if !who.iter().any(|(id, _)| *id == gid) {
        who.push((gid, 0));
    }
    let pos = |id: Id| roles.iter().find(|r| r.0 == id).map_or(-1, |r| r.3);
    who.sort_by_key(|(id, kind)| (*kind, *id != gid, std::cmp::Reverse(pos(*id))));
    if !who.iter().any(|(id, _)| *id == s.who) {
        s.who = gid;
    }
    let mut add = None;
    let mut drop = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.allocate_ui_with_layout(vec2(232.0, 400.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(232.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let (h, _) = ui.allocate_exact_size(vec2(232.0, 24.0), Sense::hover());
            ui.painter().text(pos2(h.left() + 12.0, h.center().y), Align2::LEFT_CENTER, "ROLES/MEMBERS", bold(12.0), pal().header_secondary);
            let plus = egui::Rect::from_center_size(pos2(h.right() - 20.0, h.center().y), vec2(16.0, 16.0));
            let pr = ui.interact(plus.expand(2.0), ui.id().with("perm_add"), Sense::click());
            icons::draw(ui.painter(), Icon::CirclePlus, plus, if pr.hovered() { pal().header_primary } else { pal().header_secondary });
            settings::mark(ui, "chanset:perm_add", pr.rect);
            if let Some(a) = picker(&pr, s, store, gid, roles, "Add:", search) {
                add = Some(a);
            }
            for (id, kind) in &who {
                let name = if *id == gid { "@everyone".to_string() } else if *kind == 1 { store.name_in(gid, *id) } else { roles.iter().find(|r| r.0 == *id).map_or_else(|| "Unknown Role".into(), |r| r.1.clone()) };
                let (r, resp) = ui.allocate_exact_size(vec2(232.0, 36.0), Sense::click());
                if s.who == *id {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                }
                crate::ui::text::paint(ui.painter(), pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary, r.width() - 48.0);
                settings::mark(ui, &format!("chanset:who:{name}"), r);
                let x = egui::Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0));
                if *id != gid && resp.hovered() {
                    let xr = ui.interact(x, ui.id().with(("who_x", *id)), Sense::click());
                    icons::draw(ui.painter(), Icon::Close, x.shrink(3.0), if xr.hovered() { pal().critical } else { pal().header_secondary });
                    if xr.clicked() {
                        drop = Some(*id);
                    }
                }
                if resp.clicked() && drop.is_none() {
                    s.who = *id;
                }
            }
            ui.add_space(14.0);
            let (line, _) = ui.allocate_exact_size(vec2(220.0, 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, faint(31));
            ui.add_space(14.0);
            if settings::link(ui, "Need help with permissions?").clicked() {
                let _ = open::that_detached("https://support.discord.com/hc/en-us/articles/206029707");
            }
        });
        ui.vertical(|ui| {
            let target = s.who;
            let kind = who.iter().find(|(id, _)| *id == target).map_or(0, |(_, k)| *k);
            let (allow, deny) = s.perms.iter().find(|o| o.id == target).map_or((0, 0), |o| (o.allow, o.deny));
            for (head, list) in sections(c.kind) {
                ui.add_space(8.0);
                ui.label(RichText::new(head).font(semi(20.0)).color(pal().header_primary));
                ui.add_space(12.0);
                for p in list.iter() {
                    let bit = p.0;
                    let (name, desc) = adapt(c.kind, p);
                    let cur = if deny & bit != 0 { -1 } else if allow & bit != 0 { 1 } else { 0 };
                    if let Some(v) = perm_row(ui, &name, &desc, cur) {
                        set(&mut s.perms, target, kind, bit, v);
                    }
                }
            }
        });
    });
    if let Some(id) = drop {
        s.perms.retain(|o| o.id != id);
        s.who = gid;
    }
    if let Some((id, kind)) = add {
        s.perms.push(Overwrite { id, kind, allow: 0, deny: 0 });
        s.who = id;
    }
}
