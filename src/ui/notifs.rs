//! A server's Notification Settings dialog: mute, the level, community alerts, the suppress
//! switches, mobile push and the per-channel overrides.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::create::{modal, plate};
use super::settings::{mark, switch};
use super::{App, icons};
use crate::net::Tag;
use crate::state::{Channel, Id, MANAGE_GUILD};
use crate::theme::*;

const HIGHLIGHTS: &str = "https://support.discord.com/hc/en-us/articles/5304469213079";

fn rule(ui: &mut Ui) {
    ui.add_space(16.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().border_subtle));
    ui.add_space(16.0);
}

fn heading(ui: &mut Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, text, medium(16.0), pal().header_primary);
}

fn about(ui: &mut Ui, text: &str, wrap: f32) {
    let g = ui.painter().layout(text.into(), reg(14.0), pal().header_secondary, wrap);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 4.0), Sense::hover());
    ui.painter().galley(r.left_top() + vec2(0.0, 2.0), g, pal().header_secondary);
}

fn bold_label(ui: &Ui, parts: &[(&str, bool)]) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    for (s, strong) in parts {
        job.append(s, 0.0, egui::TextFormat::simple(if *strong { semi(16.0) } else { medium(16.0) }, pal().header_primary));
    }
    ui.painter().layout_job(job)
}

fn switch_row(ui: &mut Ui, key: &str, label: std::sync::Arc<egui::Galley>, desc: &str, on: bool) -> bool {
    let w = ui.available_width();
    let body = (!desc.is_empty()).then(|| ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 72.0));
    let h = 24.0_f32.max(label.size().y) + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y);
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    ui.painter().galley(pos2(r.left(), r.top() + 2.0), label, pal().header_primary);
    if let Some(g) = body {
        ui.painter().galley(pos2(r.left(), r.top() + 28.0), g, pal().header_secondary);
    }
    let at = Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(at));
    let resp = switch(&mut c, on);
    mark(ui, key, at);
    resp.clicked()
}

fn plain(ui: &Ui, text: &str) -> std::sync::Arc<egui::Galley> {
    bold_label(ui, &[(text, false)])
}

fn radio(ui: &mut Ui, key: &str, on: bool, parts: &[(&str, bool)]) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    mark(ui, key, r);
    let c = pos2(r.left() + 10.0, r.center().y);
    ring(ui, c, on);
    let mut job = egui::text::LayoutJob::default();
    for (s, strong) in parts {
        job.append(s, 0.0, egui::TextFormat::simple(if *strong { semi(16.0) } else { reg(16.0) }, pal().text_normal));
    }
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, pal().text_normal);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked() && !on
}

fn ring(ui: &Ui, c: egui::Pos2, on: bool) {
    if on {
        ui.painter().circle_filled(c, 10.0, BLURPLE);
        ui.painter().circle_filled(c, 4.0, egui::Color32::WHITE);
    } else {
        ui.painter().circle_stroke(c, 9.0, Stroke::new(2.0, pal().header_secondary));
    }
}

fn patch(app: &mut App, gid: Id, body: Value) {
    app.guild_settings(gid, body, Tag::Quiet);
}

fn overrides(app: &App, gid: Id, added: &[Id]) -> Vec<Id> {
    let mut v: Vec<Id> = app
        .store
        .channels
        .values()
        .filter(|c| c.guild == gid && (c.kind == 4 || !c.is_thread()) && (app.store.levels.contains_key(&c.id) || app.store.muted.contains(&c.id) || added.contains(&c.id)))
        .map(|c| c.id)
        .collect();
    v.sort_by_key(|id| app.store.channels.get(id).map_or((0, 0), |c| (c.position, c.id)));
    v
}

fn candidates(app: &App, gid: Id, shown: &[Id]) -> Vec<Channel> {
    let mut v: Vec<Channel> = app.store.channels.values().filter(|c| c.guild == gid && !c.is_thread() && !shown.contains(&c.id) && app.store.can_view(c)).cloned().collect();
    v.sort_by_key(|c| (c.position, c.id));
    v
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(gid) = app.guild_notifs else { return };
    let Some(g) = app.store.guilds.get(&gid) else {
        app.guild_notifs = None;
        return;
    };
    let name = g.name.clone();
    let community = g.features.iter().any(|f| f == "COMMUNITY") && app.store.my_perms(gid) & MANAGE_GUILD != 0;
    let level = app.store.levels.get(&gid).copied().unwrap_or(g.default_notif);
    let muted = app.store.muted.contains(&gid);
    let events_muted = app.store.events_muted.contains(&gid);
    let prefs = app.store.gprefs.get(&gid).cloned().unwrap_or_default();
    let alerts = app.settings.alerts_off.get(&gid).copied().unwrap_or_default();
    let added_id = egui::Id::new(("notif_added", gid));
    let mut added: Vec<Id> = ctx.data(|d| d.get_temp(added_id)).unwrap_or_default();
    let rows = overrides(app, gid, &added);
    let options = candidates(app, gid, &rows);
    let max_h = (ctx.content_rect().height() - 96.0 - 190.0).max(120.0);
    let mut act: Vec<Box<dyn FnOnce(&mut App)>> = Vec::new();
    let mut done = false;
    let mut pick = None;
    let mut link = false;
    let store = &app.store;
    let close = modal(ctx, "guild_notifs", Some("Notification Settings"), |ui| {
        egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let w = ui.available_width();
            ui.set_width(w - 12.0);
            let label = plain(ui, &format!("Mute {name}"));
            if switch_row(ui, "notifs:mute", label, "Muting a server prevents unread indicators and notifications from appearing unless you are mentioned.", muted) {
                act.push(Box::new(move |a: &mut App| a.set_muted(gid, true, !muted)));
            }
            rule(ui);
            heading(ui, "Server Notification Settings");
            ui.add_space(12.0);
            for (v, key, parts) in [(0, "notifs:all", vec![("All Messages", false)]), (1, "notifs:mentions", vec![("Only ", false), ("@mentions", true)]), (2, "notifs:nothing", vec![("Nothing", false)])] {
                if radio(ui, key, level == v, &parts) {
                    act.push(Box::new(move |a: &mut App| a.set_level(gid, Some(v))));
                }
            }
            rule(ui);
            if community {
                heading(ui, "Community Activity Alerts");
                ui.add_space(4.0);
                about(ui, "Receive notifications for DM or join activity that exceeds usual numbers for your server.", w - 12.0);
                ui.add_space(12.0);
                let label = plain(ui, "In-app alerts");
                if switch_row(ui, "notifs:inapp", label, "A global bar that appears across the top of Discord when you are using it, regardless of what channel or server you're in at the time.", !alerts[0]) {
                    act.push(Box::new(move |a: &mut App| a.settings.alerts_off.entry(gid).or_default()[0] = alerts[0] ^ true));
                }
                ui.add_space(24.0);
                let label = plain(ui, "Push notifications");
                if switch_row(ui, "notifs:push", label, "Sends to mobile or desktop devices when you are not using Discord.", !alerts[1]) {
                    act.push(Box::new(move |a: &mut App| a.settings.alerts_off.entry(gid).or_default()[1] = alerts[1] ^ true));
                }
                rule(ui);
            }
            let label = bold_label(ui, &[("Suppress ", false), ("@everyone", true), (" and ", false), ("@here", true)]);
            if switch_row(ui, "notifs:everyone", label, "", prefs.suppress_everyone) {
                act.push(Box::new(move |a: &mut App| {
                    a.store.gprefs.entry(gid).or_default().suppress_everyone ^= true;
                    patch(a, gid, json!({ "suppress_everyone": !prefs.suppress_everyone }));
                }));
            }
            rule(ui);
            let label = plain(ui, "Suppress All Role @mentions");
            if switch_row(ui, "notifs:roles", label, "", prefs.suppress_roles) {
                act.push(Box::new(move |a: &mut App| {
                    a.store.gprefs.entry(gid).or_default().suppress_roles ^= true;
                    patch(a, gid, json!({ "suppress_roles": !prefs.suppress_roles }));
                }));
            }
            rule(ui);
            let label = plain(ui, "Suppress Highlights");
            if switch_row(ui, "notifs:highlights", label, "Highlights provide occasional updates when your friends are chatting in busy servers, and more.", prefs.highlights_off) {
                act.push(Box::new(move |a: &mut App| {
                    a.store.gprefs.entry(gid).or_default().highlights_off ^= true;
                    patch(a, gid, json!({ "notify_highlights": if prefs.highlights_off { 1 } else { 2 } }));
                }));
            }
            let (r, resp) = ui.allocate_exact_size(vec2(w - 12.0, 22.0), Sense::click());
            let ink = if resp.hovered() { pal().link } else { pal().link.gamma_multiply(0.9) };
            let t = ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "Learn more about Highlights", reg(14.0), ink);
            if resp.hovered() {
                ui.painter().hline(t.x_range(), t.bottom(), Stroke::new(1.0, ink));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            link = resp.clicked();
            rule(ui);
            let label = plain(ui, "Mute New Events");
            if switch_row(ui, "notifs:events", label, "", events_muted) {
                act.push(Box::new(move |a: &mut App| {
                    if events_muted {
                        a.store.events_muted.remove(&gid);
                    } else {
                        a.store.events_muted.insert(gid);
                    }
                    patch(a, gid, json!({ "mute_scheduled_events": !events_muted }));
                }));
            }
            rule(ui);
            let label = plain(ui, "Mobile Push Notifications");
            if switch_row(ui, "notifs:mobile", label, "", prefs.mobile_push) {
                act.push(Box::new(move |a: &mut App| {
                    a.store.gprefs.entry(gid).or_default().mobile_push ^= true;
                    patch(a, gid, json!({ "mobile_push": !prefs.mobile_push }));
                }));
            }
            ui.add_space(32.0);
            heading(ui, "Select a channel or category…");
            ui.add_space(4.0);
            about(ui, "Add a channel to override its default notification settings", w - 12.0);
            ui.add_space(8.0);
            let (r, resp) = ui.allocate_exact_size(vec2(w - 12.0, 40.0), Sense::click());
            mark(ui, "notifs:add", r);
            ui.painter().rect(r, 8, egui::Color32::from_black_alpha(31), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "Select a channel or category…", reg(16.0), pal().text_muted);
            icons::draw(ui.painter(), icons::Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), pal().header_secondary);
            let list = egui::Popup::menu(&resp).width(r.width()).close_behavior(egui::PopupCloseBehavior::CloseOnClick);
            list.show(|ui| {
                super::menu_body(ui, |ui| {
                    if options.is_empty() {
                        ui.label(egui::RichText::new("No channels left to add.").font(reg(14.0)).color(pal().text_muted));
                    }
                    for c in &options {
                        let tag = if c.kind == 4 { c.name.to_uppercase() } else { c.name.clone() };
                        if super::menu_row(ui, None, &tag, None, false, super::Trail::None).clicked() {
                            pick = Some(c.id);
                        }
                    }
                });
            });
            ui.add_space(16.0);
            let head = ui.allocate_exact_size(vec2(w - 12.0, 24.0), Sense::hover()).0;
            let cols = [head.right() - 190.0, head.right() - 135.0, head.right() - 70.0, head.right() - 20.0];
            for (x, t) in cols.iter().zip(["All", "Mentions", "Nothing", "Mute"]) {
                ui.painter().text(pos2(*x, head.center().y), Align2::CENTER_CENTER, t, semi(12.0), pal().header_secondary);
            }
            ui.painter().text(head.left_center(), Align2::LEFT_CENTER, "CHANNEL OR CATEGORY", semi(12.0), pal().header_secondary);
            for id in &rows {
                let Some(c) = store.channels.get(id) else { continue };
                let (r, _) = ui.allocate_exact_size(vec2(w - 12.0, 40.0), Sense::hover());
                let inherit = if c.parent != 0 { store.levels.get(&c.parent).copied() } else { None };
                let at = store.levels.get(id).copied().or(inherit).unwrap_or(level);
                ui.painter().text(pos2(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, &c.name, medium(14.0), pal().header_primary);
                for (i, x) in cols.iter().take(3).enumerate() {
                    let c0 = pos2(*x, r.center().y);
                    let cell = Rect::from_center_size(c0, vec2(28.0, 28.0));
                    let resp = ui.interact(cell, ui.id().with(("override", id, i)), Sense::click());
                    mark(ui, &format!("notifs:o{id}:{i}"), cell);
                    ring(ui, c0, at as usize == i);
                    if resp.clicked() {
                        let (id, i) = (*id, i as u8);
                        act.push(Box::new(move |a: &mut App| a.set_level(id, Some(i))));
                    }
                }
                let cell = Rect::from_center_size(pos2(cols[3], r.center().y), vec2(28.0, 28.0));
                let resp = ui.interact(cell, ui.id().with(("override_mute", id)), Sense::click());
                mark(ui, &format!("notifs:m{id}"), cell);
                let on = store.muted.contains(id);
                let b = Rect::from_center_size(cell.center(), vec2(20.0, 20.0));
                if on {
                    ui.painter().rect_filled(b, 4, BLURPLE);
                    icons::draw(ui.painter(), icons::Icon::Check, b.shrink(3.0), egui::Color32::WHITE);
                } else {
                    ui.painter().rect_stroke(b, 4, Stroke::new(2.0, pal().header_secondary), egui::StrokeKind::Inside);
                }
                if resp.clicked() {
                    let (id, on) = (*id, on);
                    act.push(Box::new(move |a: &mut App| a.set_muted(id, false, !on)));
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
        ui.add_space(16.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
        done = plate(ui, r, "Done", true, true);
    });
    if let Some(id) = pick {
        added.push(id);
    }
    ctx.data_mut(|d| d.insert_temp(added_id, added));
    if link {
        app.open_link(ctx, HIGHLIGHTS, false);
    }
    for f in act {
        f(app);
    }
    if close || done {
        ctx.data_mut(|d| d.remove_temp::<Vec<Id>>(added_id));
        app.guild_notifs = None;
    }
}
