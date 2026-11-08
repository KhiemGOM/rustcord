//! Discord's Mod View: one member as a moderator sees them. What the member list knows, their
//! roles with the controls to change them, the permissions those add up to, what they've said
//! lately, and the actions you can take.

use egui::{Align2, Color32, CornerRadius, Frame, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::profile::{self, RoleAct};
use super::{App, avatar, text};
use crate::net::Tag;
use crate::state::{ADMINISTRATOR, BAN_MEMBERS, Id, KICK_MEMBERS, MANAGE_GUILD, MANAGE_MESSAGES, MANAGE_ROLES, MODERATE_MEMBERS, Message, Store};
use crate::theme::*;

pub struct State {
    pub uid: Id,
    pub gid: Id,
    pub msgs: Option<Vec<Message>>,
}

const KEY_PERMS: [(u64, &str); 12] = [
    (ADMINISTRATOR, "Administrator"),
    (MANAGE_GUILD, "Manage Server"),
    (MANAGE_ROLES, "Manage Roles"),
    (1 << 4, "Manage Channels"),
    (KICK_MEMBERS, "Kick Members"),
    (BAN_MEMBERS, "Ban Members"),
    (MODERATE_MEMBERS, "Timeout Members"),
    (MANAGE_MESSAGES, "Manage Messages"),
    (1 << 17, "Mention Everyone"),
    (1 << 27, "Manage Nicknames"),
    (1 << 29, "Manage Webhooks"),
    (1 << 7, "View Audit Log"),
];

/// Whether you can moderate in this server at all: Mod View's button only shows then.
pub fn allowed(store: &Store, gid: Id) -> bool {
    gid != 0 && store.my_perms(gid) & (KICK_MEMBERS | BAN_MEMBERS | MODERATE_MEMBERS | MANAGE_GUILD | MANAGE_ROLES | MANAGE_MESSAGES) != 0
}

impl App {
    pub fn open_mod_view(&mut self, gid: Id, uid: Id) {
        self.profile = None;
        self.mod_view = Some(State { uid, gid, msgs: None });
        if !self.profiles.contains_key(&uid) || self.profiles[&uid].is_null() {
            self.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=true&with_mutual_friends=true&guild_id={gid}"), None, Tag::Profile(uid));
        }
        self.rest("GET", format!("/guilds/{gid}/messages/search?author_id={uid}"), None, Tag::ModView(uid));
    }
}

pub fn loaded(app: &mut App, uid: Id, v: &Value) {
    let list: Vec<Message> = v["messages"].as_array().into_iter().flatten().filter_map(|g| g.as_array().and_then(|g| g.first())).map(|m| app.store.parse_message(m)).collect();
    if let Some(m) = app.mod_view.as_mut().filter(|m| m.uid == uid) {
        m.msgs = Some(list);
    }
}

fn day(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default()
}

fn iso(v: &Value) -> i64 {
    v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map_or(0, |d| d.timestamp_millis())
}

fn perms_of(store: &Store, gid: Id, roles: &[Id]) -> u64 {
    let Some(g) = store.guilds.get(&gid) else { return 0 };
    let mut p = g.roles.get(&gid).map_or(0, |r| r.perms);
    for r in roles {
        p |= g.roles.get(r).map_or(0, |r| r.perms);
    }
    p
}

enum Do {
    Close,
    Role(RoleAct),
    Timeout,
    Untimeout,
    Kick,
    Ban,
    Jump(Id, Id),
    Profile,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(State { uid, gid, .. }) = app.mod_view else { return };
    let Some(user) = app.store.user(uid).cloned() else {
        app.mod_view = None;
        return;
    };
    let me = app.store.me;
    let store = &app.store;
    let p = app.profiles.get(&uid).filter(|p| !p.is_null());
    let name = store.name_in(gid, uid);
    let roles = profile::member_roles(store, p, gid, uid);
    let owner = store.guilds.get(&gid).is_some_and(|g| g.owner == uid);
    let perms = perms_of(store, gid, &roles);
    let gm = p.map(|p| &p["guild_member"]);
    let joined = gm.map_or(0, |m| iso(&m["joined_at"]));
    let boosting = gm.map_or(0, |m| iso(&m["premium_since"]));
    let until = gm.map_or(0, |m| iso(&m["communication_disabled_until"]));
    let timed_out = until > chrono::Utc::now().timestamp_millis();
    let pending = gm.is_some_and(|m| m["pending"] == true);
    let acts = uid != me && !owner && allowed(store, gid);
    let mine = store.my_perms(gid);
    let (can_timeout, can_kick, can_ban) = (acts && mine & MODERATE_MEMBERS != 0, acts && mine & KICK_MEMBERS != 0, acts && mine & BAN_MEMBERS != 0);
    let msgs = app.mod_view.as_ref().and_then(|m| m.msgs.as_ref());
    let screen = ctx.content_rect();
    let h = (screen.height() - 100.0).clamp(420.0, 720.0);
    let mut act = None;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = crate::ui::motion::Modal::new("mod_view".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.set_height(h);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let top = ui.max_rect();
        let a = egui::Rect::from_min_size(pos2(top.left() + 24.0, top.top() + 20.0), vec2(40.0, 40.0));
        avatar(ui, a, &user.avatar_url());
        let n = text::paint(ui.painter(), pos2(a.right() + 12.0, a.top() + 11.0), Align2::LEFT_CENTER, &name, semi(18.0), pal().header_primary, 300.0);
        ui.painter().text(pos2(a.right() + 12.0, a.top() + 30.0), Align2::LEFT_CENTER, &user.username, reg(12.0), pal().header_secondary);
        let chip = egui::Rect::from_min_size(pos2(n.right() + 10.0, a.top() + 3.0), vec2(62.0, 16.0));
        ui.painter().rect_filled(chip, CornerRadius::same(4), pal().blurple);
        ui.painter().text(chip.center(), Align2::CENTER_CENTER, "Mod View", semi(11.0), Color32::WHITE);
        let x = egui::Rect::from_center_size(pos2(top.right() - 32.0, a.center().y), vec2(32.0, 32.0));
        let xr = ui.interact(x, ui.id().with("mv_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, egui::Rect::from_center_size(x.center(), vec2(18.0, 18.0)), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &xr)));
        if xr.clicked() {
            act = Some(Do::Close);
        }
        let hl = top.top() + 76.0;
        ui.painter().hline(top.x_range(), hl, Stroke::new(1.0, pal().border_subtle));
        let body_rect = egui::Rect::from_min_max(pos2(top.left(), hl + 1.0), pos2(top.right(), top.bottom() - if can_timeout || can_kick || can_ban { 64.0 } else { 0.0 }));
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut body, |ui| {
            Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 16, bottom: 16 }).show(ui, |ui| {
                ui.set_width(472.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let head = |ui: &mut Ui, s: &str| {
                    ui.add_space(20.0);
                    ui.label(RichText::new(s).font(semi(12.0)).color(pal().header_secondary));
                    ui.add_space(8.0);
                };
                ui.label(RichText::new("About").font(semi(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                let row = |ui: &mut Ui, k: &str, v: String, warn: bool| {
                    let (r, _) = ui.allocate_exact_size(vec2(472.0, 24.0), Sense::hover());
                    ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, k, reg(14.0), pal().header_secondary);
                    ui.painter().text(pos2(r.left() + 160.0, r.center().y), Align2::LEFT_CENTER, v, medium(14.0), if warn { pal().red } else { pal().header_primary });
                };
                row(ui, "Account created", day(crate::state::snowflake_ms(uid)), false);
                if joined > 0 {
                    row(ui, "Joined server", day(joined), false);
                }
                if boosting > 0 {
                    row(ui, "Boosting since", day(boosting), false);
                }
                if timed_out {
                    row(ui, "Timed out until", day(until), true);
                }
                if pending {
                    row(ui, "Membership screening", "Pending".into(), false);
                }
                if owner {
                    row(ui, "Role", "Server Owner".into(), false);
                }
                head(ui, "Roles");
                if let Some(r) = profile::roles_row(ui, store, gid, uid, &roles, 472.0, 0) {
                    act = Some(Do::Role(r));
                } else if roles.is_empty() {
                    ui.label(RichText::new("No roles").font(reg(14.0)).color(pal().header_secondary));
                }
                head(ui, "Key permissions");
                let names: Vec<&str> = if perms & ADMINISTRATOR != 0 || owner {
                    vec![if owner { "Server Owner" } else { "Administrator" }]
                } else {
                    KEY_PERMS.iter().filter(|(b, _)| perms & b != 0).map(|(_, n)| *n).collect()
                };
                if names.is_empty() {
                    ui.label(RichText::new("None").font(reg(14.0)).color(pal().header_secondary));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for n in names {
                        let g = ui.painter().layout_no_wrap(n.to_string(), reg(12.0), pal().header_primary);
                        let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 24.0), Sense::hover());
                        ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                        ui.painter().galley(pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
                    }
                });
                head(ui, "Recent messages");
                match msgs {
                    None => {
                        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                    }
                    Some(l) if l.is_empty() => {
                        ui.label(RichText::new("No recent messages").font(reg(14.0)).color(pal().header_secondary));
                    }
                    Some(l) => {
                        for msg in l.iter().take(10) {
                            let (r, resp) = ui.allocate_exact_size(vec2(472.0, 52.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_hover);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let ch = store.channels.get(&msg.channel).map_or("unknown".to_string(), |c| c.name.clone());
                            let when = chrono::DateTime::from_timestamp_millis(msg.ts()).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %-I:%M %p").to_string()).unwrap_or_default();
                            let t = text::paint(ui.painter(), pos2(r.left() + 8.0, r.top() + 14.0), Align2::LEFT_CENTER, &format!("#{ch}"), semi(12.0), pal().header_secondary, 250.0);
                            ui.painter().text(pos2(t.right() + 8.0, r.top() + 14.0), Align2::LEFT_CENTER, when, reg(12.0), pal().text_muted);
                            let body = crate::markdown::plain(&msg.content, store, gid);
                            text::paint(ui.painter(), pos2(r.left() + 8.0, r.top() + 34.0), Align2::LEFT_CENTER, if body.is_empty() { "(attachment)" } else { &body }, reg(14.0), pal().header_primary, 450.0);
                            if resp.clicked() {
                                act = Some(Do::Jump(msg.channel, msg.id));
                            }
                        }
                    }
                }
            });
        });
        if can_timeout || can_kick || can_ban {
            let bar = egui::Rect::from_min_max(pos2(top.left(), top.bottom() - 64.0), top.max);
            ui.painter().hline(top.x_range(), bar.top(), Stroke::new(1.0, pal().border_subtle));
            let mut bx = top.left() + 24.0;
            let mut button = |ui: &mut Ui, label: &str, danger: bool| {
                let g = ui.painter().layout_no_wrap(label.to_string(), medium(14.0), Color32::WHITE);
                let r = egui::Rect::from_min_size(pos2(bx, bar.top() + 16.0), vec2(g.size().x + 32.0, 32.0));
                bx = r.right() + 8.0;
                let resp = ui.interact(r, ui.id().with(("mv_btn", label)), Sense::click());
                let fill = if danger { crate::theme::pal().danger } else { Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 61 } else { 41 }) };
                ui.painter().rect_filled(r, CornerRadius::same(8), fill.gamma_multiply(if danger && resp.hovered() { 0.85 } else { 1.0 }));
                ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                resp.clicked()
            };
            if can_timeout {
                if timed_out {
                    if button(ui, "Remove Timeout", false) {
                        act = Some(Do::Untimeout);
                    }
                } else if button(ui, "Timeout", false) {
                    act = Some(Do::Timeout);
                }
            }
            if can_kick && button(ui, "Kick", true) {
                act = Some(Do::Kick);
            }
            if can_ban && button(ui, "Ban", true) {
                act = Some(Do::Ban);
            }
            if button(ui, "Full Profile", false) {
                act = Some(Do::Profile);
            }
        }
    });
    if m.should_close() {
        app.mod_view = None;
    }
    match act {
        Some(Do::Close) => app.mod_view = None,
        Some(Do::Role(r)) => profile::do_role(app, gid, uid, r),
        Some(Do::Timeout) => app.open_timeout(gid, uid),
        Some(Do::Untimeout) => app.rest("PATCH", format!("/guilds/{gid}/members/{uid}"), Some(json!({ "communication_disabled_until": null })), Tag::reason()),
        Some(Do::Kick) => app.kick(gid, uid),
        Some(Do::Ban) => app.ban(gid, uid),
        Some(Do::Jump(c, mid)) => {
            app.mod_view = None;
            app.jump_to(c, mid);
        }
        Some(Do::Profile) => {
            app.mod_view = None;
            app.open_full(uid, gid, profile::Tab::Activity);
        }
        None => {}
    }
}

