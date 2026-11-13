//! Discord's server tag popout, and the popout a role mention opens.

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, popups, text};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

/// A chip's click, with the pointing hand over it.
pub fn hit(ui: &mut Ui, r: Rect, id: impl std::hash::Hash + std::fmt::Debug) -> bool {
    let resp = ui.interact(r, ui.id().with(id), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    tag(app, ctx);
    role(app, ctx);
}

fn tag(app: &mut App, ctx: &egui::Context) {
    let Some((gid, at)) = app.tag_pop else { return };
    if !app.guild_profiles.contains_key(&gid) {
        app.guild_profiles.insert(gid, serde_json::Value::Null);
        app.rest("GET", format!("/guilds/{gid}/profile"), None, Tag::GuildProfile(gid));
    }
    let p = app.guild_profiles.get(&gid).filter(|p| !p.is_null()).cloned();
    let mut go = None;
    super::motion::slide(ctx, egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tag_pop")), vec2(0.0, 1.0));
    let area = egui::Area::new(egui::Id::new("tag_pop")).order(egui::Order::Tooltip).fixed_pos(at).constrain(true).show(ctx, |ui| match &p {
        Some(p) => go = super::chatinvite::popout(ui, &app.store, gid, p),
        None => {
            Frame::new().fill(pal().bg_secondary).corner_radius(16).stroke(Stroke::new(1.0, pal().border_subtle)).show(ui, |ui| {
                ui.set_width(298.0);
                ui.set_height(120.0);
                ui.centered_and_justified(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
            });
        }
    });
    match go {
        Some(super::chatinvite::Go::Open(g)) => {
            app.tag_pop = None;
            app.select_guild(Some(g));
        }
        Some(_) => {
            app.tag_pop = None;
            let manual = p.is_some_and(|p| p["features"].as_array().is_some_and(|f| f.iter().any(|x| x == "MEMBER_VERIFICATION_MANUAL_APPROVAL")));
            let done = if manual { Tag::ok("Application sent.") } else { Tag::reason() };
            app.rest("PUT", format!("/guilds/{gid}/members/@me"), Some(json!({})), done);
        }
        None if popups::dismissed(ctx, &area.response) => app.tag_pop = None,
        None => {}
    }
}

fn role(app: &mut App, ctx: &egui::Context) {
    let Some((gid, rid, at)) = app.role_pop else { return };
    let store = &app.store;
    let Some(g) = store.guilds.get(&gid) else {
        app.role_pop = None;
        return;
    };
    let Some(role) = g.roles.get(&rid) else {
        app.role_pop = None;
        return;
    };
    let mut people: Vec<(Id, String, Option<u32>)> = store
        .members
        .get(&gid)
        .into_iter()
        .flatten()
        .filter(|(_, m)| rid == gid || m.roles.contains(&rid))
        .filter(|(u, _)| store.user(**u).is_some())
        .map(|(u, _)| (*u, store.name_in(gid, *u), store.role_color(gid, *u)))
        .collect();
    people.sort_by_key(|p| p.1.to_lowercase());
    let total = if rid == gid { g.member_count as usize } else { people.len() };
    let (name, color) = (role.name.clone(), role.color);
    let mut open = None;
    super::motion::slide(ctx, egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("role_pop")), vec2(0.0, 1.0));
    let area = egui::Area::new(egui::Id::new("role_pop")).order(egui::Order::Tooltip).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(8).show(ui, |ui| {
            ui.set_width(240.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (h, _) = ui.allocate_exact_size(vec2(240.0, 28.0), Sense::hover());
            let col = if color == 0 { Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(color) };
            ui.painter().circle_filled(pos2(h.left() + 10.0, h.center().y), 5.0, col);
            let n = text::paint(ui.painter(), pos2(h.left() + 22.0, h.center().y), Align2::LEFT_CENTER, &name, semi(12.0), pal().header_primary, 150.0);
            ui.painter().text(pos2(n.right() + 8.0, h.center().y), Align2::LEFT_CENTER, format!("\u{2014} {total}"), medium(12.0), pal().header_secondary);
            egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                for (u, nm, c) in &people {
                    let (r, resp) = ui.allocate_exact_size(vec2(240.0, 36.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if let Some(user) = store.user(*u) {
                        avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0)), &user.avatar_url());
                    }
                    let ink = c.map_or(pal().header_primary, role_color);
                    text::paint(ui.painter(), pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, nm, medium(14.0), ink, 190.0);
                    if resp.clicked() {
                        open = Some((*u, r.right_top() + vec2(8.0, 0.0)));
                    }
                }
                if people.is_empty() {
                    ui.label(egui::RichText::new("No members loaded").font(reg(14.0)).color(pal().header_secondary));
                }
            });
        });
    });
    if let Some((u, p)) = open {
        app.role_pop = None;
        app.open_profile(u, gid, p);
    } else if popups::dismissed(ctx, &area.response) {
        app.role_pop = None;
    }
}
