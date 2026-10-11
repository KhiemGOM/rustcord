//! Channel Settings, from Edit Channel: discord's overview, permissions, invites and webhooks,
//! with its unsaved-changes bar, and Delete Channel at the bottom of the sidebar.

use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::App;
use super::icons::{self, Icon};
use super::settings;
use crate::net::Tag;
use crate::state::{Id, Overwrite, sid};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Overview,
    Permissions,
    Invites,
    Integrations,
}

pub struct State {
    pub chan: Id,
    pub page: Page,
    // the channel as discord last sent it, and as edited
    pub base: Value,
    pub draft: Value,
    pub perms: Vec<Overwrite>,
    pub saving: bool,
    // whose overwrite the advanced permissions show
    pub who: Id,
    pub data: HashMap<&'static str, Value>,
}

const SLOWMODE: [(u64, &str); 14] = [
    (0, "Off"),
    (5, "5s"),
    (10, "10s"),
    (15, "15s"),
    (30, "30s"),
    (60, "1m"),
    (120, "2m"),
    (300, "5m"),
    (600, "10m"),
    (900, "15m"),
    (1800, "30m"),
    (3600, "1h"),
    (7200, "2h"),
    (21600, "6h"),
];

const ARCHIVE: [(u64, &str); 4] = [(60, "1 Hour"), (1440, "24 Hours"), (4320, "3 Days"), (10080, "1 Week")];

// the fields the overview edits, and so the ones a save sends if they changed
const FIELDS: [&str; 9] = ["name", "topic", "rate_limit_per_user", "nsfw", "flags", "default_auto_archive_duration", "bitrate", "user_limit", "video_quality_mode"];

const SPOILER: u64 = 1 << 21;

type Perm = (u64, &'static str, &'static str);

// discord's sections and wording for a channel's permissions
const GENERAL: [Perm; 4] = [
    (1 << 10, "View Channel", "Allows members to view this channel by default. Disabling this for @everyone will make this channel private."),
    (1 << 4, "Manage Channel", "Allows members to change this channel’s name, description, and text settings. They can also delete the channel."),
    (1 << 28, "Manage Permissions", "Allows members to change this channel’s permissions."),
    (1 << 29, "Manage Webhooks", "Allows members to create, edit, or delete webhooks in this channel, which can post messages from other apps or sites."),
];
const MEMBERSHIP: [Perm; 1] = [(1, "Create Invite", "Allows members to invite new people to this server via a direct invite link to this channel.")];
const TEXT: [Perm; 19] = [
    (1 << 11, "Send Messages", "Allows members to send messages in this channel."),
    (1 << 38, "Send Messages in Threads", "Allow members to send messages in threads under this channel."),
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
    (1 << 34, "Manage Threads", "Allows members to rename, delete, close, and turn on slow mode for threads in this channel. They can also view private threads."),
    (1 << 16, "Read Message History", "Allows members to read previous messages sent in this channel. If this permission is disabled, members only see messages sent when they are online."),
    (1 << 12, "Send Text-to-Speech Messages", "Allows members to send text-to-speech messages in this channel by starting a message with /tts. These messages can be heard by anyone focused on this channel."),
    (1 << 46, "Send Voice Messages", "Allows members to send voice messages in this channel. Requires the \"Attach Files\" permission as well."),
    (1 << 49, "Create Polls", "Allows members to create polls in this channel."),
    (1 << 31, "Use Application Commands", "Allows members to use commands from applications in this channel, including slash commands and context menu commands."),
];
const VOICE: [Perm; 11] = [
    (1 << 20, "Connect", "Allows members to join this voice channel and hear others."),
    (1 << 21, "Speak", "Allows members to talk in this voice channel. If this permission is disabled, members are default muted until somebody with the “Mute Members” permission un-mutes them."),
    (1 << 9, "Video", "Allows members to share their video, screen share, or stream a game in this voice channel."),
    (1 << 42, "Use Soundboard", "Allows members to send sounds from server soundboard."),
    (1 << 45, "Use External Sounds", "Allows members to use sounds from other servers in this channel, if they’re a Discord Nitro member."),
    (1 << 25, "Use Voice Activity", "Allows members to speak in this voice channel by simply talking. If this permission is disabled, members are required to use Push-to-talk. Good for controlling background noise or noisy members."),
    (1 << 8, "Priority Speaker", "Users with this permission have the ability to be more easily heard when talking. When activated, the volume of others without this permission will be automatically lowered."),
    (1 << 22, "Mute Members", "Allows members to mute other members in this voice channel for everyone."),
    (1 << 23, "Deafen Members", "Allows members to deafen other members in this voice channel, which means they won’t be able to speak or hear others."),
    (1 << 24, "Move Members", "Allows members to drag other members out of this channel. They can only move members between channels both they and the member they are moving have access to."),
    (1 << 48, "Set Voice Channel Status", "Allows members to create, edit, and delete a voice channel status."),
];
const APPS: [Perm; 2] = [
    (1 << 39, "Use Activities", "Allows members to use Activities in this channel."),
    (1 << 50, "Use External Apps", "Allows apps that members have added to their account to post messages in this channel. When disabled, the messages will be private."),
];
const EVENTS: [Perm; 2] = [
    (1 << 44, "Create Events", "Allows members to create events in this channel."),
    (1 << 33, "Manage Events", "Allows members to edit and cancel events in this channel."),
];

impl App {
    pub fn open_chan_settings(&mut self, ch: Id) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        self.chan_settings = Some(State { chan: ch, page: Page::Overview, base: Value::Null, draft: Value::Null, perms: c.overwrites.clone(), saving: false, who: c.guild, data: HashMap::new() });
        self.rest("GET", format!("/channels/{ch}"), None, Tag::Chan("channel"));
    }

    fn chan_data(&mut self, key: &'static str, path: String) -> Option<Value> {
        let s = self.chan_settings.as_mut()?;
        if !s.data.contains_key(key) {
            s.data.insert(key, Value::Null);
            self.rest("GET", path, None, Tag::Chan(key));
        }
        self.chan_settings.as_ref()?.data.get(key).filter(|v| !v.is_null()).cloned()
    }
}

pub fn loaded(app: &mut App, key: &'static str, v: Value) {
    let Some(s) = &mut app.chan_settings else { return };
    match key {
        "channel" | "saved" => {
            s.perms = parse_overwrites(&v);
            s.base = v.clone();
            s.draft = v;
            s.saving = false;
            if key == "saved" {
                app.store.apply("CHANNEL_UPDATE", &s.base);
            }
        }
        "hook_made" | "hook_gone" => {
            s.data.remove("webhooks");
        }
        _ => {
            s.data.insert(key, v);
        }
    }
}

fn parse_overwrites(v: &Value) -> Vec<Overwrite> {
    let num = |v: &Value| v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64()).unwrap_or(0);
    v["permission_overwrites"].as_array().into_iter().flatten().map(|o| Overwrite { id: sid(&o["id"]), kind: o["type"].as_u64().unwrap_or(0) as u8, allow: num(&o["allow"]), deny: num(&o["deny"]) }).collect()
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(st) = app.chan_settings.as_ref() else { return };
    let ch = st.chan;
    let page = st.page;
    let Some(c) = app.store.channels.get(&ch).cloned() else {
        app.chan_settings = None;
        return;
    };
    // like discord, escape leaves even with changes unsaved, and drops them
    if app.confirm.is_none() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.chan_settings = None;
        return;
    }
    let screen = ctx.content_rect();
    let area = Rect::from_min_max(pos2(screen.left(), screen.top() + 32.0), screen.max);
    let (mut goto, mut close, mut delete) = (None, false, false);
    egui::Area::new("chan_settings".into()).order(egui::Order::Foreground).fixed_pos(area.min).show(ctx, |ui| {
        ui.set_min_size(area.size());
        ui.painter().rect_filled(area, 0, pal().bg_primary);
        let side_right = area.center().x - 280.0;
        let content_left = area.center().x - 228.0;
        ui.painter().rect_filled(Rect::from_min_max(area.min, pos2(side_right + 6.0, area.bottom())), 0, pal().bg_secondary_alt);
        let mut sc = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(side_right - 239.0, area.top()), pos2(side_right, area.bottom()))));
        sc.spacing_mut().item_spacing.y = 2.0;
        sc.add_space(46.0);
        // "# GENERAL  TEXT CHANNELS": the channel muted, its category bright
        let (r, _) = sc.allocate_exact_size(vec2(sc.available_width(), 28.0), Sense::hover());
        let y = r.center().y + 4.0;
        super::side::chan_icon(&app.store, sc.painter(), &c, Rect::from_center_size(pos2(r.left() + 16.0, y), vec2(12.0, 12.0)), pal().header_secondary);
        let n = sc.painter().text(pos2(r.left() + 24.0, y), Align2::LEFT_CENTER, c.name.to_uppercase(), bold(12.0), pal().header_secondary);
        if let Some(cat) = app.store.channels.get(&c.parent) {
            super::text::paint(sc.painter(), pos2(n.right() + 6.0, y), Align2::LEFT_CENTER, &cat.name.to_uppercase(), bold(12.0), pal().header_primary, r.right() - n.right() - 10.0);
        }
        for (p, label) in [(Page::Overview, "Overview"), (Page::Permissions, "Permissions"), (Page::Invites, "Invites"), (Page::Integrations, "Integrations")] {
            let (r, resp) = sc.allocate_exact_size(vec2(sc.available_width(), 32.0), Sense::click());
            let on = p == page;
            if on {
                sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
            } else if resp.hovered() {
                sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
            }
            sc.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, medium(16.0), if on || resp.hovered() { pal().header_primary } else { pal().header_secondary });
            app.rects.insert(format!("chanset:{label}"), r);
            if resp.hovered() {
                sc.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                goto = Some(p);
            }
        }
        sc.add_space(8.0);
        let (r, _) = sc.allocate_exact_size(vec2(sc.available_width(), 1.0), Sense::hover());
        sc.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
        sc.add_space(8.0);
        let (r, resp) = sc.allocate_exact_size(vec2(sc.available_width(), 32.0), Sense::click());
        if resp.hovered() {
            sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
        }
        let what = if c.kind == 4 { "Delete Category" } else { "Delete Channel" };
        sc.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, what, medium(16.0), pal().critical);
        icons::draw(sc.painter(), Icon::Trash, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), pal().critical);
        app.rects.insert("chanset:delete".into(), r);
        delete = resp.clicked();

        let esc = Rect::from_center_size(pos2(area.right() - 56.0, area.top() + 64.0), vec2(36.0, 36.0));
        let er = ui.interact(esc, ui.id().with("chan_esc"), Sense::click());
        let ink = if er.hovered() { pal().header_primary } else { pal().header_secondary };
        ui.painter().circle_stroke(esc.center(), 17.0, Stroke::new(2.0, ink));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(esc.center(), vec2(18.0, 18.0)), ink);
        ui.painter().text(pos2(esc.center().x, esc.bottom() + 14.0), Align2::CENTER_CENTER, "ESC", semi(13.0), pal().header_secondary);
        close = er.clicked();

        let body = Rect::from_min_max(pos2(content_left, area.top()), pos2((content_left + 660.0).min(esc.left() - 40.0), area.bottom()));
        let mut bc = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical().id_salt(("chan_page", page as u8)).auto_shrink([false, false]).show(&mut bc, |ui| {
            ui.data_mut(|d| d.remove_temp::<f32>(egui::Id::new("settings_gap")));
            Frame::new().inner_margin(Margin { left: 0, right: 16, top: 46, bottom: 120 }).show(ui, |ui| match page {
                Page::Overview => overview(app, ui, &c),
                Page::Permissions => permissions(app, ui, &c),
                Page::Invites => invites(app, ui, &c),
                Page::Integrations => integrations(app, ui, &c),
            });
        });
        save_bar(app, ui, body);
    });
    if close {
        app.chan_settings = None;
    } else if let Some(p) = goto
        && let Some(s) = &mut app.chan_settings
    {
        s.page = p;
    }
    if delete {
        super::side::delete_channel(app, ch);
    }
}

fn heading(ui: &mut Ui, t: &str, desc: &str) {
    ui.label(RichText::new(t).font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(desc).font(reg(14.0)).line_height(Some(18.0)).color(pal().text_normal));
    }
    ui.add_space(24.0);
}

fn small(ui: &mut Ui, s: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(s).font(reg(12.0)).line_height(Some(16.0)).color(pal().header_secondary));
}

fn overview(app: &mut App, ui: &mut Ui, c: &crate::state::Channel) {
    heading(ui, "Overview", "");
    let tier = app.store.guilds.get(&c.guild).map_or(0, |g| g.tier);
    let regions = if c.is_voice() { app.chan_data("regions", format!("/guilds/{}/regions", c.guild)) } else { None };
    let Some(s) = app.chan_settings.as_mut() else { return };
    if s.draft.is_null() {
        ui.add_space(40.0);
        ui.spinner();
        return;
    }
    let d = &mut s.draft;
    let voice = c.is_voice();
    settings::h5(ui, if c.kind == 4 { "Category Name" } else { "Channel Name" });
    let mut name = d["name"].as_str().unwrap_or_default().to_string();
    let r = settings::input(ui, &mut name, "", false);
    settings::mark(ui, "chanset:name", r.rect);
    if r.changed() {
        // discord's text channel names are lowercase with dashes for spaces
        if !voice && c.kind != 4 {
            name = name.to_lowercase().replace(' ', "-");
        }
        d["name"] = json!(name);
    }
    if c.kind == 4 {
        return;
    }
    if !voice {
        ui.add_space(24.0);
        settings::h5(ui, "Channel Topic");
        let mut topic = d["topic"].as_str().unwrap_or_default().to_string();
        let t = Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
            let r = ui.add(
                egui::TextEdit::multiline(&mut topic)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .hint_text(RichText::new("Let everyone know how to use this channel!").color(pal().text_muted))
                    .desired_rows(8)
                    .desired_width(ui.available_width()),
            );
            if topic.chars().count() > 1024 {
                topic = topic.chars().take(1024).collect();
            }
            r
        });
        let left = 1024 - topic.chars().count() as i64;
        ui.painter().text(t.response.rect.right_bottom() - vec2(16.0, 10.0), Align2::RIGHT_BOTTOM, left.to_string(), bold(14.0), pal().text_normal);
        if t.inner.changed() {
            d["topic"] = json!(topic);
        }
    }
    ui.add_space(24.0);
    settings::h5(ui, "Slowmode");
    let slow = d["rate_limit_per_user"].as_u64().unwrap_or(0);
    let shown = SLOWMODE.iter().find(|(v, _)| *v == slow).map_or_else(|| format!("{slow}s"), |(_, l)| l.to_string());
    let w = ui.available_width();
    let mut pick = None;
    let r = settings::dropdown(ui, w, None, &shown, |ui| {
        for (v, l) in SLOWMODE {
            if super::menu_item(ui, None, l, false) {
                pick = Some(v);
            }
        }
    });
    settings::mark(ui, "chanset:slowmode", r.rect);
    if let Some(v) = pick {
        d["rate_limit_per_user"] = json!(v);
    }
    small(ui, "Members will be restricted to sending one message and creating one thread per this interval, unless they have the Bypass Slowmode permission.");
    ui.add_space(40.0);
    settings::h5(ui, "Content Visibility");
    let flags = d["flags"].as_u64().unwrap_or(0);
    let mut vis = if d["nsfw"].as_bool() == Some(true) { 2 } else if flags & SPOILER != 0 { 1 } else { 0 };
    let options = [
        (0, "Default", "Channel content is always visible."),
        (1, "Spoiler Channel", "Mark this channel as containing spoilers, so that plot twists, heavy topics, or other sensitive discussions are hidden until users choose to view."),
        (2, "Age-Restricted Channel", "Users will need to confirm they are of over the legal age to view in the content in this channel. Age-restricted channels are exempt from the explicit content filter."),
    ];
    if visibility(ui, &mut vis, &options) {
        d["nsfw"] = json!(vis == 2);
        d["flags"] = json!(if vis == 1 { flags | SPOILER } else { flags & !SPOILER });
    }
    if !voice {
        ui.add_space(24.0);
        settings::h5(ui, "Hide After Inactivity");
        let arch = d["default_auto_archive_duration"].as_u64().unwrap_or(4320);
        let shown = ARCHIVE.iter().find(|(v, _)| *v == arch).map_or("3 Days", |(_, l)| l);
        let mut pick = None;
        settings::dropdown(ui, w, None, shown, |ui| {
            for (v, l) in ARCHIVE {
                if super::menu_item(ui, None, l, false) {
                    pick = Some(v);
                }
            }
        });
        if let Some(v) = pick {
            d["default_auto_archive_duration"] = json!(v);
        }
        small(ui, "New threads will not show in the channel list after being inactive for the specified duration.");
        return;
    }
    settings::divider(ui);
    // the boost level raises the bitrate's ceiling
    let max = [96, 128, 256, 384][tier.min(3) as usize] as f32;
    settings::h5(ui, "Bitrate");
    let mut kbps = (d["bitrate"].as_u64().unwrap_or(64000) / 1000) as f32;
    if ticked_slider(ui, &mut kbps, 8.0, max, &[(8.0, "8kbps".into()), (64.0, "64kbps".into()), (max, format!("{max}kbps"))], "chanset:bitrate") {
        d["bitrate"] = json!(kbps.round() as u64 * 1000);
    }
    small(ui, if kbps > 64.0 { "ALL THE BITS! Going above 64 kbps may adversely affect people on poor connections." } else { "Going above 64 kbps may adversely affect people on poor connections." });
    ui.add_space(24.0);
    settings::h5(ui, "Video Quality");
    let mut q = d["video_quality_mode"].as_u64().unwrap_or(1);
    if settings::radio(ui, &mut q, &[(1, "Auto", ""), (2, "720p", "")]) {
        d["video_quality_mode"] = json!(q);
    }
    ui.label(super::bolded("Sets camera video quality for all channel participants. Choose **Auto** for optimal performance.", 12.0, pal().header_secondary, ui.available_width()));
    ui.add_space(24.0);
    settings::h5(ui, "User Limit");
    let mut limit = d["user_limit"].as_u64().unwrap_or(0) as f32;
    if ticked_slider(ui, &mut limit, 0.0, 99.0, &[(0.0, "\u{221e}".into()), (99.0, "99".into())], "chanset:limit") {
        d["user_limit"] = json!(limit.round() as u64);
    }
    ui.add_space(8.0);
    ui.label(super::bolded("Limits the number of users that can connect to this voice channel. Users with the **Move Members** permission ignore this limit and can move other users into the channel.", 12.0, pal().header_secondary, ui.available_width()));
    ui.add_space(24.0);
    settings::h5(ui, "Region Override");
    ui.label(RichText::new("Anyone in this channel will attempt to connect to the region you set, regardless of where they live. Regions affect voice and video quality. Leave it on Automatic and Discord will figure out what works best.").font(reg(16.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let region = d["rtc_region"].as_str().map(str::to_string);
    let list: Vec<(String, String)> = regions.as_ref().and_then(|v| v.as_array()).into_iter().flatten().filter(|r| r["deprecated"].as_bool() != Some(true)).map(|r| (r["id"].as_str().unwrap_or_default().to_string(), r["name"].as_str().unwrap_or_default().to_string())).collect();
    let shown = region.as_ref().map_or("Automatic".to_string(), |id| list.iter().find(|(i, _)| i == id).map_or(id.clone(), |(_, n)| n.clone()));
    let mut pick: Option<Option<String>> = None;
    settings::dropdown(ui, w, None, &shown, |ui| {
        if super::menu_item(ui, None, "Automatic", false) {
            pick = Some(None);
        }
        for (id, n) in &list {
            if super::menu_item(ui, None, n, false) {
                pick = Some(Some(id.clone()));
            }
        }
    });
    if let Some(p) = pick {
        d["rtc_region"] = p.map_or(Value::Null, |s| json!(s));
    }
}

/// The content visibility choices: discord's radios, each with its paragraph under it.
fn visibility(ui: &mut Ui, value: &mut i32, options: &[(i32, &str, &str)]) -> bool {
    let mut changed = false;
    let w = ui.available_width();
    for (v, label, desc) in options {
        let g = ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, w - 38.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0 + g.size().y), Sense::click());
        let c = pos2(r.left() + 12.0, r.top() + 12.0);
        if *value == *v {
            ui.painter().circle_filled(c, 12.0, BLURPLE);
            ui.painter().circle_filled(c, 5.0, Color32::WHITE);
        } else {
            ui.painter().circle_stroke(c, 11.0, Stroke::new(1.5, pal().header_secondary));
        }
        ui.painter().text(pos2(r.left() + 38.0, c.y), Align2::LEFT_CENTER, *label, reg(16.0), pal().text_normal);
        ui.painter().galley(pos2(r.left() + 38.0, r.top() + 26.0), g, pal().header_secondary);
        settings::mark(ui, &format!("chanset:vis:{label}"), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && *value != *v {
            *value = *v;
            changed = true;
        }
        ui.add_space(16.0);
    }
    changed
}

/// A slider with its labels over it where their values fall, and tick marks down to the rail.
fn ticked_slider(ui: &mut Ui, value: &mut f32, lo: f32, hi: f32, ticks: &[(f32, String)], key: &str) -> bool {
    let w = ui.available_width();
    let (top, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
    let x_of = |v: f32| top.left() + 8.0 + (v - lo) / (hi - lo) * (w - 16.0);
    for (v, l) in ticks {
        let g = ui.painter().layout_no_wrap(l.clone(), medium(12.0), pal().header_secondary);
        let left = (x_of(*v) - g.size().x / 2.0).clamp(top.left(), top.right() - g.size().x);
        ui.painter().galley(pos2(left, top.top()), g, pal().header_secondary);
    }
    ui.add_space(6.0);
    let r = ui.cursor().min;
    for (v, _) in ticks {
        ui.painter().vline(x_of(*v), r.y - 4.0..=r.y + 20.0, Stroke::new(2.0, settings::TRACK));
    }
    let moved = settings::slider(ui, w, value, lo, hi);
    settings::mark(ui, key, Rect::from_min_size(r, vec2(w, 16.0)));
    if moved {
        *value = value.round();
    }
    moved
}

fn permissions(app: &mut App, ui: &mut Ui, c: &crate::state::Channel) {
    heading(ui, "Channel Permissions", "Use permissions to customize who can do what in this channel.");
    let gid = c.guild;
    let parent = app.store.channels.get(&c.parent).cloned();
    let roles: Vec<(Id, String, u32, i32)> = app.store.guilds.get(&gid).map(|g| g.roles.values().map(|r| (r.id, r.name.clone(), r.color, r.position)).collect()).unwrap_or_default();
    let Some(s) = app.chan_settings.as_mut() else { return };
    // synced while it holds exactly its category's overwrites
    if let Some(p) = &parent {
        let mut a: Vec<_> = s.perms.iter().map(|o| (o.id, o.allow, o.deny)).collect();
        let mut b: Vec<_> = p.overwrites.iter().map(|o| (o.id, o.allow, o.deny)).collect();
        a.sort();
        b.sort();
        let synced = a == b;
        let mut sync = false;
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(Margin::symmetric(20, 18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Refresh, r, if synced { Color32::from_rgb(0xf0, 0xb2, 0x32) } else { pal().header_secondary });
                ui.add_space(4.0);
                let text = format!("Permissions {} with category: **{}**", if synced { "synced" } else { "not synced" }, p.name);
                ui.label(super::bolded(&text, 16.0, pal().text_normal, ui.available_width() - 120.0));
                if !synced {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| sync = settings::secondary(ui, "Sync Now").clicked());
                }
            });
        });
        if sync {
            s.perms = p.overwrites.clone();
        }
        ui.add_space(20.0);
    }
    // private: @everyone can't view it
    let everyone = s.perms.iter().find(|o| o.id == gid).map_or(0, |o| o.deny);
    let mut private = everyone & crate::state::VIEW_CHANNEL != 0;
    Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(Margin::symmetric(20, 18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let label = if c.kind == 4 { "Private Category" } else { "Private Channel" };
        if settings::toggle(ui, &mut private, label, "By making a channel private, only select members and roles will be able to view this channel.") {
            set(&mut s.perms, gid, 0, crate::state::VIEW_CHANNEL, if private { -1 } else { 0 });
        }
    });
    ui.add_space(32.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    ui.add_space(32.0);
    ui.label(RichText::new("Advanced permissions").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(24.0);
    // who has an overwrite: @everyone first, then roles high to low, then members
    let mut who: Vec<(Id, u8)> = s.perms.iter().map(|o| (o.id, o.kind)).collect();
    if !who.iter().any(|(id, _)| *id == gid) {
        who.push((gid, 0));
    }
    let pos = |id: Id| roles.iter().find(|r| r.0 == id).map_or(-1, |r| r.3);
    who.sort_by_key(|(id, kind)| (*kind, *id != gid, std::cmp::Reverse(pos(*id))));
    let mut add = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.allocate_ui_with_layout(vec2(232.0, 400.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(232.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let (h, _) = ui.allocate_exact_size(vec2(232.0, 24.0), Sense::hover());
            ui.painter().text(pos2(h.left() + 12.0, h.center().y), Align2::LEFT_CENTER, "ROLES/MEMBERS", bold(12.0), pal().header_secondary);
            let plus = Rect::from_center_size(pos2(h.right() - 20.0, h.center().y), vec2(16.0, 16.0));
            let pr = ui.interact(plus.expand(2.0), ui.id().with("perm_add"), Sense::click());
            icons::draw(ui.painter(), Icon::CirclePlus, plus, if pr.hovered() { pal().header_primary } else { pal().header_secondary });
            egui::Popup::menu(&pr).show(|ui| {
                super::menu_body(ui, |ui| {
                    let mut left: Vec<&(Id, String, u32, i32)> = roles.iter().filter(|r| !who.iter().any(|(id, _)| *id == r.0)).collect();
                    left.sort_by_key(|r| std::cmp::Reverse(r.3));
                    for r in left {
                        if super::menu_item(ui, None, &r.1, false) {
                            add = Some(r.0);
                        }
                    }
                });
            });
            for (id, kind) in &who {
                let name = if *id == gid { "@everyone".to_string() } else if *kind == 1 { app.store.name_in(gid, *id) } else { roles.iter().find(|r| r.0 == *id).map_or_else(|| "Unknown Role".into(), |r| r.1.clone()) };
                let (r, resp) = ui.allocate_exact_size(vec2(232.0, 36.0), Sense::click());
                if s.who == *id {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                }
                super::text::paint(ui.painter(), pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary, r.width() - 24.0);
                if resp.clicked() {
                    s.who = *id;
                }
            }
        });
        ui.vertical(|ui| {
            let target = s.who;
            let kind = who.iter().find(|(id, _)| *id == target).map_or(0, |(_, k)| *k);
            let (allow, deny) = s.perms.iter().find(|o| o.id == target).map_or((0, 0), |o| (o.allow, o.deny));
            let mut sections: Vec<(&str, &[Perm])> = vec![("General Channel Permissions", &GENERAL), ("Membership Permissions", &MEMBERSHIP)];
            if c.is_voice() {
                sections.push(("Voice Channel Permissions", &VOICE));
            }
            if c.kind != 2 || c.is_voice() {
                sections.push(("Text Channel Permissions", &TEXT));
            }
            sections.push(("Apps Permissions", &APPS));
            if c.is_voice() {
                sections.push(("Events Permissions", &EVENTS));
            }
            for (head, list) in sections {
                ui.add_space(8.0);
                ui.label(RichText::new(head).font(semi(20.0)).color(pal().header_primary));
                ui.add_space(12.0);
                for (bit, name, desc) in list.iter() {
                    let cur = if deny & bit != 0 { -1 } else if allow & bit != 0 { 1 } else { 0 };
                    if let Some(v) = perm_row(ui, name, desc, cur) {
                        set(&mut s.perms, target, kind, *bit, v);
                    }
                }
            }
        });
    });
    if let Some(id) = add {
        s.perms.push(Overwrite { id, kind: 0, allow: 0, deny: 0 });
        s.who = id;
    }
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

/// A permission with discord's ✕ / ✓ switch on the right. Hands back what was clicked.
fn perm_row(ui: &mut Ui, name: &str, desc: &str, cur: i8) -> Option<i8> {
    let w = ui.available_width();
    let g = ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, w - 160.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 30.0 + g.size().y), Sense::hover());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, name, medium(16.0), pal().header_primary);
    ui.painter().galley(r.left_top() + vec2(0.0, 26.0), g, pal().header_secondary);
    let group = Rect::from_min_size(pos2(r.right() - 96.0, r.top()), vec2(96.0, 28.0));
    let mut out = None;
    for (k, v) in [-1i8, 0, 1].into_iter().enumerate() {
        let cell = Rect::from_min_size(pos2(group.left() + k as f32 * 32.0, group.top()), vec2(32.0, 28.0));
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

fn invites(app: &mut App, ui: &mut Ui, c: &crate::state::Channel) {
    heading(ui, "Invites", "");
    let ch = c.id;
    let list: Vec<Value> = app.chan_data("invites", format!("/channels/{ch}/invites")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let mut create = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Here's a list of all active invite links. You can revoke any one or ").font(reg(14.0)).color(pal().header_secondary));
        create = settings::link(ui, "create one").clicked();
        ui.label(RichText::new(".").font(reg(14.0)).color(pal().header_secondary));
    });
    ui.add_space(20.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    if !list.is_empty() {
        ui.add_space(16.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        for (x, h) in [(0.0, "INVITER"), (0.45, "INVITE CODE"), (0.7, "USES"), (0.85, "EXPIRES")] {
            ui.painter().text(pos2(r.left() + r.width() * x, r.center().y), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary);
        }
    }
    let mut revoke = None;
    for inv in &list {
        let code = inv["code"].as_str().unwrap_or_default().to_string();
        let who = inv["inviter"]["global_name"].as_str().or(inv["inviter"]["username"].as_str()).unwrap_or("Unknown").to_string();
        let uses = match inv["max_uses"].as_u64().unwrap_or(0) {
            0 => inv["uses"].as_u64().unwrap_or(0).to_string(),
            m => format!("{}/{m}", inv["uses"].as_u64().unwrap_or(0)),
        };
        let expires = inv["expires_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or("\u{221e}".to_string(), |t| {
            let s = (t.timestamp() - chrono::Utc::now().timestamp()).max(0);
            format!("{:02}:{:02}:{:02}:{:02}", s / 86400, s / 3600 % 24, s / 60 % 60, s % 60)
        });
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
        }
        ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(31)));
        let p = ui.painter();
        p.text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, &who, medium(16.0), pal().header_primary);
        p.text(pos2(r.left() + r.width() * 0.45, r.center().y), Align2::LEFT_CENTER, &code, medium(16.0), pal().text_normal);
        p.text(pos2(r.left() + r.width() * 0.7, r.center().y), Align2::LEFT_CENTER, &uses, medium(16.0), pal().text_normal);
        p.text(pos2(r.left() + r.width() * 0.85, r.center().y), Align2::LEFT_CENTER, &expires, medium(16.0), pal().text_normal);
        if resp.hovered() {
            let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
            p.circle_filled(x.center(), 10.0, RED);
            icons::draw(p, Icon::Close, x.shrink(4.0), Color32::WHITE);
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| q.x > r.right() - 32.0) {
                revoke = Some(code.clone());
            }
        }
    }
    if let Some(code) = revoke {
        app.rest("DELETE", format!("/invites/{code}"), None, Tag::Quiet);
        if let Some(s) = &mut app.chan_settings {
            s.data.remove("invites");
        }
    }
    if create {
        app.invite_people(ch);
    }
}

fn integrations(app: &mut App, ui: &mut Ui, c: &crate::state::Channel) {
    heading(ui, "Integrations", "Customize your server with integrations. Manage webhooks, followed channels, and apps, as well as Twitch and YouTube settings for creators.");
    let ch = c.id;
    let hooks: Vec<Value> = app.chan_data("webhooks", format!("/channels/{ch}/webhooks")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let mut make = false;
    let mut delete = None;
    Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 20.0, pal().bg_accent);
            icons::draw(ui.painter(), Icon::Link, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
            ui.vertical(|ui| {
                ui.label(RichText::new("Webhooks").font(semi(16.0)).color(pal().header_primary));
                let n = hooks.len();
                ui.label(RichText::new(format!("{n} webhook{}", if n == 1 { "" } else { "s" })).font(reg(14.0)).color(pal().header_secondary));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| make = settings::secondary(ui, "Create Webhook").clicked());
        });
        for h in &hooks {
            ui.add_space(12.0);
            let name = h["name"].as_str().unwrap_or("Webhook").to_string();
            let id = sid(&h["id"]);
            let url = h["token"].as_str().map(|t| format!("https://discord.com/api/webhooks/{id}/{t}"));
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).font(medium(16.0)).color(pal().text_normal));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if settings::danger(ui, "Delete Webhook", false).clicked() {
                        delete = Some(id);
                    }
                    if let Some(u) = &url
                        && settings::secondary(ui, "Copy Webhook URL").clicked()
                    {
                        ui.ctx().copy_text(u.clone());
                    }
                });
            });
        }
    });
    if make {
        // discord's new webhooks are named for one of its own characters
        app.rest("POST", format!("/channels/{ch}/webhooks"), Some(json!({ "name": "Captain Hook" })), Tag::Chan("hook_made"));
    }
    if let Some(id) = delete {
        app.rest("DELETE", format!("/webhooks/{id}"), None, Tag::Chan("hook_gone"));
    }
}

/// Discord's bar along the bottom while an edit is unsaved: Reset, or Save Changes.
fn save_bar(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(s) = app.chan_settings.as_mut() else { return };
    if s.base.is_null() {
        return;
    }
    let changed: serde_json::Map<String, Value> = FIELDS.iter().filter(|k| s.draft[**k] != s.base[**k]).map(|k| (k.to_string(), s.draft[*k].clone())).collect();
    let mut a: Vec<_> = s.perms.iter().map(|o| (o.id, o.allow, o.deny)).collect();
    let mut b: Vec<_> = parse_overwrites(&s.base).iter().map(|o| (o.id, o.allow, o.deny)).collect();
    a.sort();
    b.sort();
    let region = s.draft["rtc_region"] != s.base["rtc_region"];
    if changed.is_empty() && a == b && !region {
        return;
    }
    let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 40.0), vec2(area.width() - 20.0, 52.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_floating);
    ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful — you have unsaved changes!", semi(16.0), pal().header_primary);
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 7.0))).layout(Layout::right_to_left(Align::Center)));
    let save = super::button(&mut bar, if s.saving { "…" } else { "Save Changes" }, GREEN, 120.0);
    app.rects.insert("chanset:save".into(), save.rect);
    bar.add_space(12.0);
    let reset = bar.add(egui::Label::new(RichText::new("Reset").font(semi(14.0)).color(pal().header_primary)).sense(Sense::click()));
    if reset.clicked() {
        s.draft = s.base.clone();
        s.perms = parse_overwrites(&s.base);
        return;
    }
    if save.clicked() && !s.saving {
        s.saving = true;
        let mut body = Value::Object(changed);
        if region {
            body["rtc_region"] = s.draft["rtc_region"].clone();
        }
        if a != b {
            body["permission_overwrites"] = s.perms.iter().map(|o| json!({ "id": o.id.to_string(), "type": o.kind, "allow": o.allow.to_string(), "deny": o.deny.to_string() })).collect();
        }
        let ch = s.chan;
        app.rest("PATCH", format!("/channels/{ch}"), Some(body), Tag::Chan("saved"));
    }
}
