//! Engagement: system messages, the activity feed, default notifications, the inactive channel
//! and the server widget.

use egui::{Ui, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::settings::{self, toggle};
use super::form;
use super::title;
use crate::net::Tag;
use crate::state::Id as Snow;

const TIMEOUTS: [(u64, &str); 5] = [(60, "1 minute"), (300, "5 minutes"), (900, "15 minutes"), (1800, "30 minutes"), (3600, "1 hour")];

const ON: &str = "ACTIVITY_FEED_ENABLED_BY_USER";
const OFF: &str = "ACTIVITY_FEED_DISABLED_BY_USER";

fn norm(g: &Value) -> Value {
    let mut features: Vec<String> = g["features"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect();
    features.sort();
    json!({
        "system_channel_id": g["system_channel_id"],
        "system_channel_flags": g["system_channel_flags"].as_u64().unwrap_or(0),
        "default_message_notifications": g["default_message_notifications"].as_u64().unwrap_or(0),
        "afk_channel_id": g["afk_channel_id"],
        "afk_timeout": g["afk_timeout"].as_u64().unwrap_or(300),
        "features": features,
    })
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    title(ui, "Engagement", "Manage settings that help keep your server active.");
    let Some(raw) = app.server_data("guild", format!("/guilds/{gid}")).cloned() else {
        ui.add(egui::Spinner::new().color(crate::theme::pal().text_muted));
        return;
    };
    let widget = app.server_data("widget", format!("/guilds/{gid}/widget")).cloned().unwrap_or(Value::Null);
    let orig = norm(&raw);
    let mut d = form::take(app, "guild", &orig);
    ui.set_max_width(660.0);

    let flags = d["system_channel_flags"].as_u64().unwrap_or(0);
    form::head(ui, true, "System Messages", "Configure system event messages sent to your server.");
    let mut set = flags;
    for (bit, label, needs_welcome) in [(1u64, "Send a random welcome message when someone joins this server.", false), (8, "Prompt members to reply to welcome messages with a sticker.", true), (2, "Send a message when someone boosts this server.", false), (4, "Send helpful tips for server setup.", false)] {
        let mut on = flags & bit == 0;
        let off = needs_welcome && flags & 1 != 0;
        ui.add_enabled_ui(!off, |ui| {
            if toggle(ui, &mut on, label, "") {
                set = if on { set & !bit } else { set | bit };
            }
        });
    }
    d["system_channel_flags"] = json!(set);
    ui.add_space(16.0);
    let cur = d["system_channel_id"].as_str().and_then(|s| s.parse::<Snow>().ok());
    row(ui, "System Messages Channel", "This is the channel we send system event messages to.", |ui| {
        if let Some(p) = form::channel(ui, app, gid, 268.0, cur, false, Some("No System Messages")) {
            d["system_channel_id"] = p.map_or(Value::Null, |c| json!(c.to_string()));
        }
    });

    form::head(ui, false, "Activity Feed Settings", "Shows a feed of activity from games and connected apps in this server.");
    let mut features: Vec<String> = d["features"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect();
    let mut on = features.iter().any(|f| f == ON) || !features.iter().any(|f| f == OFF);
    if toggle(ui, &mut on, "Display Activity Feed in this server", "") {
        features.retain(|f| f != ON && f != OFF);
        features.push((if on { ON } else { OFF }).into());
        features.sort();
        d["features"] = json!(features);
    }

    form::line(ui);
    settings::h5(ui, "Default Notification Settings");
    form::small(ui, "This will determine whether members who have not explicitly set their notification settings receive a notification for every message sent in this server or not.");
    ui.add_space(12.0);
    let mut n = d["default_message_notifications"].as_u64().unwrap_or(0) as u8;
    if settings::radio(ui, &mut n, &[(0u8, "All Messages", ""), (1, "Only @mentions", "")]) {
        d["default_message_notifications"] = json!(n);
    }
    form::small(ui, "We highly recommend setting this to only @mentions for a Community Server.");

    form::line(ui);
    let afk = d["afk_channel_id"].as_str().and_then(|s| s.parse::<Snow>().ok());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.vertical(|ui| {
            settings::h5(ui, "Inactive Channel");
            if let Some(p) = form::channel(ui, app, gid, 318.0, afk, true, Some("No Inactive Channel")) {
                d["afk_channel_id"] = p.map_or(Value::Null, |c| json!(c.to_string()));
            }
        });
        ui.vertical(|ui| {
            ui.add_enabled_ui(afk.is_some(), |ui| {
                settings::h5(ui, "Inactive Timeout");
                let secs = d["afk_timeout"].as_u64().unwrap_or(300);
                let shown = TIMEOUTS.iter().find(|t| t.0 == secs).map_or("5 minutes", |t| t.1);
                form::dropdown(ui, 318.0, shown, None, None, |ui| {
                    for (v, label) in TIMEOUTS {
                        if ui.selectable_label(v == secs, label).clicked() {
                            d["afk_timeout"] = json!(v);
                            ui.close();
                        }
                    }
                });
            });
        });
    });
    ui.add_space(12.0);
    form::small(ui, "Automatically move members to this channel and mute them when they have been idle for longer than the inactive timeout. This does not affect browsers.");

    form::head(ui, false, "Server Widget", "Embed an HTML widget on your website to display your online members, voice channels, and invite link.");
    let mut enabled = widget["enabled"].as_bool().unwrap_or(false);
    let wch = widget["channel_id"].as_str().and_then(|s| s.parse::<Snow>().ok());
    if toggle(ui, &mut enabled, "Enable Server Widget", "") {
        app.rest("PATCH", format!("/guilds/{gid}/widget"), Some(json!({ "enabled": enabled, "channel_id": widget["channel_id"] })), Tag::Server("widget"));
        if let Some(s) = &mut app.server {
            s.data.insert("widget", json!({ "enabled": enabled, "channel_id": widget["channel_id"] }));
        }
    }
    if enabled {
        ui.add_space(8.0);
        settings::h5(ui, "Invite Channel");
        if let Some(p) = form::channel(ui, app, gid, 318.0, wch, false, Some("No Invite")) {
            let v = p.map_or(Value::Null, |c| json!(c.to_string()));
            app.rest("PATCH", format!("/guilds/{gid}/widget"), Some(json!({ "enabled": true, "channel_id": v })), Tag::Server("widget"));
            if let Some(s) = &mut app.server {
                s.data.insert("widget", json!({ "enabled": true, "channel_id": v }));
            }
        }
    }
    ui.add_space(16.0);
    form::small(ui, "By enabling the widget, your server profile will be visible to others outside of your server members. You can control Profile privacy in Server Settings > Profile.");

    form::put(app, "guild", d);
    let ctx = ui.ctx().clone();
    form::bar(app, &ctx, "guild", "guild", &orig, format!("/guilds/{gid}"), Value::Object);
}

/// A label and a blurb on the left, a control of its own width on the right.
fn row(ui: &mut Ui, head: &str, desc: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 292.0);
            ui.label(egui::RichText::new(head).font(crate::theme::medium(16.0)).color(crate::theme::pal().header_primary));
            ui.add_space(4.0);
            form::small(ui, desc);
        });
        ui.allocate_ui(vec2(268.0, 40.0), add);
    });
}
