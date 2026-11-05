use egui::{Align, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;
use std::time::Duration;

use super::icons::{self, Icon};
use super::App;
use crate::net::Tag;
use crate::state::{BLOCKED, Channel, Id, MANAGE_THREADS, MANAGE_WEBHOOKS};
use crate::theme::*;

pub const NO_PERM: &str = "You do not have permission to send messages in this channel.";
const ARCHIVED: &str = "This thread is archived. You can send a message to unarchive it.";

pub enum Gate {
    Open,
    Off(&'static str),
    Bar(Bar),
}

pub enum Bar {
    Blocked(Id),
    Follow,
}

pub fn check(app: &App, chan: &Channel) -> Gate {
    let s = &app.store;
    if chan.kind == 1
        && let Some(&u) = chan.recipients.first()
        && s.relationships.get(&u) == Some(&BLOCKED)
    {
        return Gate::Bar(Bar::Blocked(u));
    }
    if !s.can_send(chan) {
        let follow = chan.kind == 5 && s.timeout_left(chan.guild) == 0 && s.guild_order.iter().any(|g| s.my_perms(*g) & MANAGE_WEBHOOKS != 0);
        return if follow { Gate::Bar(Bar::Follow) } else { Gate::Off(NO_PERM) };
    }
    if chan.is_thread() && chan.locked && s.perms(chan) & MANAGE_THREADS == 0 {
        return Gate::Off(NO_PERM);
    }
    Gate::Open
}

impl Gate {
    pub fn off(&self) -> bool {
        matches!(self, Gate::Off(_))
    }
}

pub fn placeholder(chan: &Channel, gate: &Gate) -> Option<&'static str> {
    match gate {
        Gate::Off(p) => Some(p),
        _ if chan.is_thread() && chan.archived && !chan.locked => Some(ARCHIVED),
        _ => None,
    }
}

pub fn button(ui: &mut Ui, label: &str) -> egui::Response {
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), pal().header_primary);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().galley(pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Discord's bar where the composer would be: what's wrong and the one thing that fixes it.
pub fn bar(app: &mut App, ui: &mut Ui, bar: &Bar) {
    let (msg, act) = match bar {
        Bar::Blocked(_) => ("You cannot send messages to a user you have blocked.", "Unblock"),
        Bar::Follow => ("Follow to get this channel's updates in your own server.", "Follow"),
    };
    let mut hit = false;
    let r = Frame::new().fill(pal().bg_input).stroke(Stroke::new(1.0, pal().border_muted)).corner_radius(8).inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(54.0);
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new(msg).font(semi(16.0)).color(pal().header_primary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = button(ui, act);
                app.rects.insert("gate:button".into(), b.rect);
                hit = b.clicked();
            });
        });
    });
    app.rects.insert("composer".into(), r.response.rect);
    ui.add_space(8.0);
    if hit {
        match bar {
            Bar::Blocked(u) => app.rest("DELETE", format!("/users/@me/relationships/{u}"), None, Tag::Quiet),
            Bar::Follow => app.follow = app.cur().map(super::follow::Follow::new),
        }
    }
}

/// The strip over the composer in a thread that's locked or closed, with the way back for those who can.
pub fn banner(app: &mut App, ui: &mut Ui, chan: &Channel) {
    if !chan.is_thread() || !chan.locked {
        return;
    }
    let can = app.store.perms(chan) & MANAGE_THREADS != 0;
    let post = app.store.channels.get(&chan.parent).is_some_and(|p| p.is_forum());
    let (msg, act) = if chan.archived {
        (if post { "This post has been closed and locked. Only moderators can reopen it." } else { "This thread has been closed and locked. Only moderators can reopen it." }, "Open")
    } else {
        (if post { "This post has been locked. Only moderators can send messages." } else { "This thread has been locked. Only moderators can send messages." }, "Unlock")
    };
    let mut hit = false;
    Frame::new().fill(pal().bg_secondary).corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(msg).font(reg(14.0)).color(pal().text_normal));
            if can {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = button(ui, act);
                    app.rects.insert("gate:banner".into(), b.rect);
                    hit = b.clicked();
                });
            }
        });
    });
    if hit {
        let body = if chan.archived { json!({ "archived": false, "locked": false }) } else { json!({ "locked": false }) };
        app.rest("PATCH", format!("/channels/{}", chan.id), Some(body), Tag::Quiet);
        if let Some(c) = app.store.channels.get_mut(&chan.id) {
            c.locked = false;
            c.archived = false;
        }
    }
}

/// What Discord says, for you alone, when a send it refused comes back.
pub fn notice(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let Some((c, msg)) = &app.notice else { return };
    if *c != chan.id {
        return;
    }
    let msg = msg.clone();
    let mut dismiss = false;
    Frame::new().fill(pal().bg_secondary).corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new(&msg).font(reg(14.0)).color(pal().text_normal));
            ui.label(RichText::new("\u{2022} Only you can see this \u{2022}").font(reg(12.0)).color(pal().text_muted));
            let r = ui.add(egui::Label::new(RichText::new("Dismiss this message").font(reg(12.0)).color(pal().link)).sense(Sense::click()));
            app.rects.insert("notice:dismiss".into(), r.rect);
            dismiss = r.clicked();
        });
    });
    if dismiss {
        app.notice = None;
    }
}

fn clock(ms: i64) -> String {
    let s = (ms + 999) / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn units(secs: i64) -> String {
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    [(d, "d"), (h, "h"), (m, "m"), (s, "s")].iter().filter(|(n, _)| *n > 0).map(|(n, u)| format!("{n}{u}")).collect::<Vec<_>>().join(" ")
}

/// Slowmode's countdown, or a timeout's, at the composer's top right with Discord's timer icon.
pub fn cooldown(app: &mut App, ui: &mut Ui, chan: &Channel, composer: Rect) {
    let timeout = app.store.timeout_left(chan.guild);
    let slow = app.store.slow_left(chan);
    let immune = chan.rate > 0 && app.store.perms(chan) & crate::state::BYPASS_SLOWMODE != 0 && chan.guild != 0 && app.store.can_send(chan);
    let (label, left) = if timeout > 0 {
        (format!("Timed out for {}", units(timeout)), timeout * 1000)
    } else if immune {
        ("Slowmode Immune".to_string(), 0)
    } else if chan.rate > 0 && app.store.can_send(chan) {
        (if slow > 0 { clock(slow) } else { "Slowmode is enabled.".to_string() }, slow)
    } else {
        return;
    };
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("cooldown")));
    let g = p.layout_no_wrap(label, medium(12.0), pal().text_muted);
    let y = composer.top() - 12.0;
    let right = composer.right() - 16.0;
    let icon = Rect::from_center_size(pos2(right - 8.0, y), vec2(16.0, 16.0));
    icons::draw(&p, Icon::Timer, icon, pal().text_muted);
    let at = pos2(icon.left() - 4.0 - g.size().x, y - g.size().y / 2.0);
    app.rects.insert("cooldown".into(), Rect::from_min_size(at, g.size()));
    p.galley(at, g, pal().text_muted);
    if left > 0 {
        ui.ctx().request_repaint_after(Duration::from_millis((left % 1000 + 5) as u64));
    }
}
