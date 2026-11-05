//! Open in Mod View: a member's card down the right edge, with what a moderator wants at hand.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Sense, pos2, vec2};

use super::{App, avatar, button, grey_button, icon_button};
use super::icons::Icon;
use crate::net::Tag;
use crate::state::{BAN_MEMBERS, Id, KICK_MEMBERS, MODERATE_MEMBERS};
use crate::theme::*;

impl App {
    pub fn open_mod_view(&mut self, gid: Id, uid: Id) {
        self.mod_view = Some((gid, uid));
        self.profiles.remove(&uid);
        self.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=false&guild_id={gid}"), None, Tag::Profile(uid));
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((gid, uid)) = app.mod_view else { return };
    let Some(user) = app.store.user(uid).cloned() else {
        app.mod_view = None;
        return;
    };
    let store = &app.store;
    let name = store.name_in(gid, uid);
    let perms = store.my_perms(gid);
    let joined = |t: i64| chrono::DateTime::from_timestamp_millis(t).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
    let created = joined(crate::state::snowflake_ms(uid));
    let server = app
        .profiles
        .get(&uid)
        .and_then(|p| p["guild_member"]["joined_at"].as_str().or(p["guild_member_profile"]["joined_at"].as_str()))
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| joined(t.timestamp_millis()));
    let roles: Vec<(String, u32)> = store
        .members
        .get(&gid)
        .and_then(|m| m.get(&uid))
        .zip(store.guilds.get(&gid))
        .map(|(m, g)| {
            let mut r: Vec<_> = m.roles.iter().filter_map(|r| g.roles.get(r)).collect();
            r.sort_by_key(|r| std::cmp::Reverse(r.position));
            r.into_iter().map(|r| (r.name.clone(), r.color)).collect()
        })
        .unwrap_or_default();
    let (mut close, mut act) = (false, None);
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("mod_view"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(screen.right() - 340.0, screen.top() + 48.0))
        .show(ctx, |ui| {
            Frame::new().fill(pal().bg_secondary).stroke(egui::Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(300.0);
                ui.set_min_height(screen.height() - 64.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mod View").font(semi(16.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        close = icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                    });
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    avatar(ui, r, &user.avatar_url());
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new(&name).font(semi(16.0)).color(pal().header_primary));
                        ui.label(RichText::new(&user.username).font(reg(14.0)).color(pal().header_secondary));
                    });
                });
                for (label, value) in [("Joined Discord", Some(created)), ("Joined Server", server)] {
                    ui.add_space(16.0);
                    ui.label(RichText::new(label).font(semi(12.0)).color(pal().header_primary));
                    ui.add_space(4.0);
                    ui.label(RichText::new(value.unwrap_or_else(|| "Loading…".into())).font(reg(14.0)).color(pal().text_normal));
                }
                if !roles.is_empty() {
                    ui.add_space(16.0);
                    ui.label(RichText::new("Roles").font(semi(12.0)).color(pal().header_primary));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for (r, c) in &roles {
                            let g = ui.painter().layout_no_wrap(r.clone(), reg(12.0), pal().header_primary);
                            let (pill, _) = ui.allocate_exact_size(vec2(32.0 + g.size().x, 24.0), Sense::hover());
                            ui.painter().rect_stroke(pill, CornerRadius::same(8), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                            let col = if *c == 0 { egui::Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(*c) };
                            ui.painter().circle_filled(pos2(pill.left() + 12.0, pill.center().y), 6.0, col);
                            ui.painter().galley(pos2(pill.left() + 22.0, pill.center().y - g.size().y / 2.0), g, pal().header_primary);
                        }
                    });
                }
                if uid != store.me {
                    ui.add_space(24.0);
                    for (label, need, danger, what) in [
                        ("Timeout", MODERATE_MEMBERS, false, 0),
                        ("Kick", KICK_MEMBERS, true, 1),
                        ("Ban", BAN_MEMBERS, true, 2),
                    ] {
                        if perms & need == 0 {
                            continue;
                        }
                        let clicked = if danger { button(ui, &format!("{label} {name}"), super::DANGER, 300.0).clicked() } else { grey_button(ui, &format!("{label} {name}"), 300.0).clicked() };
                        if clicked {
                            act = Some(what);
                        }
                        ui.add_space(8.0);
                    }
                }
            });
        });
    if let Some(what) = act {
        let (kind, name) = (what, name.clone());
        let server = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
        match kind {
            0 => app.timeout = Some(super::popups::Timeout::new(gid, uid)),
            1 => {
                let body = format!("Are you sure you want to kick @{name} from the server? They will be able to rejoin again with a new invite.");
                app.confirm = Some(super::Confirm::new(format!("Kick '{name}' from {server}"), body, "Kick", move |app: &mut App| {
                    app.rest("DELETE", format!("/guilds/{gid}/members/{uid}"), None, Tag::Quiet)
                }));
            }
            _ => {
                let body = format!("Are you sure you want to ban @{name} from {server}?");
                app.confirm = Some(super::Confirm::new(format!("Ban '{name}'"), body, "Ban", move |app: &mut App| {
                    app.rest("PUT", format!("/guilds/{gid}/bans/{uid}"), Some(serde_json::json!({ "delete_message_seconds": 0 })), Tag::Quiet)
                }));
            }
        }
    }
    if close {
        app.mod_view = None;
    }
}
