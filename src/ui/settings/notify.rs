use egui::{Align, Align2, Id, Layout, Sense, Ui, UiBuilder, pos2, vec2};
use serde_json::json;

use super::privacy::write;
use super::wire::Patch;
use super::{Look, action, dropdown, loading, remote, section, sub, switch, toggle};
use crate::cache::Settings;
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::Icon;

const EMAILS: [(&str, &str, &str); 5] = [
    ("communication", "Communication Emails", "Receive emails for missed calls, messages, and message digests."),
    ("social", "Social Emails", "Receive emails for friend requests, new friend suggestions, and events in your server."),
    ("updates_and_announcements", "Announcements and Update Emails", "Receive emails about product updates, our newest features, improvements and bug fixes."),
    ("tips", "Tip Emails", "Receive emails with helpful advice on how to use Rustcord and information on lesser known features."),
    ("recommendations_and_events", "Recommendations Emails", "Receive emails with recommended servers and suggested events."),
];

const MARKETING: [&str; 3] = ["tips", "recommendations_and_events", "updates_and_announcements"];

fn account_toggle(app: &mut App, ui: &mut Ui, label: &str, field: fn(&mut Settings) -> &mut bool, g: u64, f: u64) {
    let mut on = *field(&mut app.settings);
    if toggle(ui, &mut on, label, "") {
        *field(&mut app.settings) = on;
        write(app, Patch::new().flag(g, f, on));
    }
}

fn pick(ui: &mut Ui, label: &str, desc: &str, shown: &str, menu: impl FnOnce(&mut Ui)) {
    super::lead(ui);
    let w = ui.available_width();
    let body = (!desc.is_empty()).then(|| ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 320.0));
    let h = (24.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y)).max(40.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let ty = r.center().y - (24.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y)) / 2.0;
    ui.painter().text(pos2(r.left(), ty), Align2::LEFT_TOP, label, medium(16.0), pal().header_primary);
    if let Some(g) = body {
        ui.painter().galley(pos2(r.left(), ty + 28.0), g, pal().header_secondary);
    }
    let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
    let resp = dropdown(&mut c, 290.0, None, shown, menu);
    super::mark(ui, &format!("pick:{label}"), resp.rect);
    super::end(ui, 24.0);
}

fn email(app: &mut App, ui: &mut Ui) {
    let Some(v) = loading(ui, remote(app, "email", "/users/@me/email-settings")) else { return };
    let cat = |k: &str| v["categories"][k].as_bool().unwrap_or(false);
    for (key, label, desc) in EMAILS {
        let mut on = cat(key);
        if toggle(ui, &mut on, label, desc) {
            if let Some(Some(Ok(cur))) = app.remote.get_mut("email") {
                cur["categories"][key] = json!(on);
            }
            app.rest("PATCH", "/users/@me/email-settings".into(), Some(json!({ "settings": { "categories": { key: on } } })), Tag::Remote("action"));
        }
    }
    let all_off = MARKETING.iter().all(|k| !cat(k));
    let mut unsub = false;
    ui.add_enabled_ui(!all_off, |ui| {
        unsub = action(
            ui,
            "Unsubscribe from all marketing emails",
            "This includes any emails about product updates, new features, tips on using Rustcord, and recommendations on new servers, events or stage channels.",
            "Unsubscribe",
            Look::Danger,
        );
    });
    if unsub {
        let off: serde_json::Map<_, _> = MARKETING.iter().map(|k| (k.to_string(), json!(false))).collect();
        if let Some(Some(Ok(cur))) = app.remote.get_mut("email") {
            for k in MARKETING {
                cur["categories"][k] = json!(false);
            }
        }
        app.rest("PATCH", "/users/@me/email-settings".into(), Some(json!({ "settings": { "categories": off } })), Tag::Remote("action"));
    }
}

fn sounds(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    let mut preview = None;
    let on = app.settings.sounds;
    for (key, label) in [("message", "New Message"), ("selected", "New Message in the channel I’m currently reading"), ("ring", "Incoming Ring")] {
        super::lead(ui);
        let s = &mut app.settings;
        let mut enabled = if key == "selected" { s.notify_selected } else { !s.muted_sounds.iter().any(|m| m == key) };
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, medium(16.0), if on { pal().header_primary } else { pal().text_muted });
        let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        let sw = switch(&mut c, enabled && on);
        super::mark(ui, &format!("toggle:{label}"), sw.rect);
        if sw.clicked() && on {
            enabled = !enabled;
            if key == "selected" {
                s.notify_selected = enabled;
            } else {
                s.muted_sounds.retain(|m| m != key);
                if !enabled {
                    s.muted_sounds.push(key.to_string());
                }
            }
            changed = true;
        }
        c.add_space(16.0);
        if crate::ui::icon_button(&mut c, Icon::Play, 24.0, "Preview Sound").clicked() {
            preview = Some(if key == "selected" { "message" } else { key });
        }
        super::end(ui, 32.0);
    }
    if let Some(k) = preview {
        crate::sound::play(k);
    }
    let mut off = !on;
    if toggle(
        ui,
        &mut off,
        "Disable All Notification Sounds",
        "Disables all notification sounds, including voice & video sound effects. Your individual sound preferences will be saved and restored if you turn this on.",
    ) {
        app.settings.sounds = !off;
        changed = true;
    }
    sub(ui, "Related Settings");
    super::lead(ui);
    let g = ui.painter().layout("Enable/disable sounds that play when you’re in a call, like Mute, Unmute, Deafen, and more.".into(), reg(14.0), pal().header_secondary, ui.available_width() - 64.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0 + g.size().y), Sense::click());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, "Voice & Video", medium(16.0), pal().link);
    ui.painter().galley(r.left_top() + vec2(0.0, 24.0), g, pal().header_secondary);
    crate::ui::icons::draw(ui.painter(), Icon::ChevronRight, egui::Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.settings_page = Some(super::Page::Voice);
    }
    super::end(ui, 24.0);
    changed
}

fn advanced(app: &mut App, ui: &mut Ui) -> bool {
    let id = Id::new("notify_advanced");
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    super::lead(ui);
    let sub_text = "Mobile notification delay, allow playback and usage of /tts command, and more";
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), if open { 24.0 } else { 48.0 }), Sense::click());
    let title = if open { "Hide Advanced Notification Settings" } else { "Show Advanced Notification Settings" };
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, title, medium(16.0), pal().header_primary);
    if !open {
        ui.painter().text(r.left_top() + vec2(0.0, 24.0), Align2::LEFT_TOP, sub_text, reg(14.0), pal().header_secondary);
    }
    crate::ui::icons::draw(
        ui.painter(),
        if open { Icon::ChevronUp } else { Icon::ChevronDown },
        egui::Rect::from_center_size(pos2(r.right() - 12.0, r.top() + 12.0), vec2(20.0, 20.0)),
        pal().header_primary,
    );
    super::mark(ui, "notify:advanced", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
    super::end(ui, 24.0);
    let mut changed = false;
    if !open {
        return false;
    }
    let delay = app.settings.push_delay;
    let shown = format!("{} Minute{}", delay / 60, if delay / 60 == 1 { "" } else { "s" });
    pick(
        ui,
        "Mobile Notification Delay",
        "Rustcord won’t send push notifications to your mobile device while you’re active on your computer. Use this setting to choose how long you need to be inactive on desktop before mobile notifications start again.",
        &shown,
        |ui| {
            for m in 1..=10u32 {
                let label = format!("{m} Minute{}", if m == 1 { "" } else { "s" });
                if ui.selectable_label(delay == m * 60, label).clicked() {
                    app.settings.push_delay = m * 60;
                    write(app, Patch::new().sub(5, 6, &[(1, m as u64 * 60)]));
                }
            }
        },
    );
    let mut on = app.settings.tts_command;
    if toggle(ui, &mut on, "Allow playback and usage of /tts command", "Change Text-to-Speech speed in Accessibility Settings.") {
        app.settings.tts_command = on;
        write(app, Patch::new().flag(6, 16, on));
    }
    sub(ui, "Speak all messages out loud");
    let mut tts = app.settings.tts_notify;
    if super::radio(ui, &mut tts, &[(2, "For all channels", ""), (1, "Only for currently selected channel", ""), (0, "Never", "")]) {
        app.settings.tts_notify = tts;
        changed = true;
    }
    changed |= toggle(ui, &mut app.settings.notify_all_dms, "Notify me about every direct message", "Otherwise only direct messages that mention you notify.");
    changed
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Overview");
    let mut changed = toggle(
        ui,
        &mut app.settings.notify,
        "Enable Desktop Notifications",
        "If you're looking for per-channel or per-server notifications, right-click the desired server icon and select Notification Settings.",
    );
    changed |= toggle(ui, &mut app.settings.flash, "Enable Taskbar Flashing", "Flashes the app in your taskbar when you have new notifications.");
    sub(ui, "Notify me when...");
    account_toggle(app, ui, "People I know start streaming in small servers", |s| &mut s.stream_notify, 5, 7);
    account_toggle(app, ui, "A friend and I reach a friendship anniversary", |s| &mut s.friend_anniv, 7, 14);
    account_toggle(app, ui, "Friends come online", |s| &mut s.friend_online, 7, 12);
    account_toggle(app, ui, "A server has an upcoming event", |s| &mut s.server_events, 7, 22);
    account_toggle(app, ui, "Friends update their profile", |s| &mut s.profile_updates, 7, 16);
    let reactions = ["All Messages", "Only Direct Messages", "Never"];
    let cur = app.settings.reaction_notify as usize;
    pick(ui, "Someone reacts to my messages", "", reactions[cur.min(2)], |ui| {
        for (i, label) in reactions.iter().enumerate() {
            if ui.selectable_label(cur == i, *label).clicked() {
                app.settings.reaction_notify = i as u8;
                write(app, Patch::new().num(7, 7, i as u64));
            }
        }
    });

    section(ui, "Sounds");
    changed |= sounds(app, ui);

    section(ui, "Badges");
    changed |= toggle(ui, &mut app.settings.badge, "Enable Unread Message Badge", "Shows a red badge on the app icon when you have unread messages.");

    section(ui, "Email");
    email(app, ui);

    section(ui, "Advanced");
    changed |= advanced(app, ui);
    changed
}
