//! Discord's server tag popout, and the popout a role mention opens.

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, avatar, popups, text};
use crate::net::Tag;
use crate::state::{Id, sid};
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
        app.rest("GET", format!("/guild-profile/{gid}"), None, Tag::GuildProfile(gid));
    }
    let Some(p) = app.guild_profiles.get(&gid).filter(|p| !p.is_null()).cloned() else {
        let area = egui::Area::new(egui::Id::new("tag_pop")).order(egui::Order::Tooltip).fixed_pos(at).constrain(true).show(ctx, |ui| {
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).show(ui, |ui| {
                ui.set_width(300.0);
                ui.set_height(120.0);
                ui.centered_and_justified(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
            });
        });
        if popups::dismissed(ctx, &area.response) {
            app.tag_pop = None;
        }
        return;
    };
    let member = app.store.guilds.contains_key(&gid);
    let apps: Vec<Id> = p["game_application_ids"].as_array().into_iter().flatten().map(sid).take(6).collect();
    let want: Vec<Id> = apps.iter().copied().filter(|a| !app.store.app_icons.contains_key(a)).collect();
    if !want.is_empty() {
        for a in &want {
            app.store.app_icons.insert(*a, None);
        }
        let q: Vec<String> = want.iter().map(|a| format!("application_ids={a}")).collect();
        app.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::AppIcons(want));
    }
    let name = p["name"].as_str().unwrap_or("").to_string();
    let desc = p["description"].as_str().unwrap_or("").to_string();
    let tag = p["tag"].as_str().unwrap_or("").to_string();
    let badge = p["badge_hash"].as_str().or(p["badge"].as_str()).filter(|b| !b.is_empty()).map(|b| format!("{}/guild-tag-badges/{gid}/{b}.png?size=32", crate::api::cdn()));
    let icon = p["icon_hash"].as_str().or(p["icon"].as_str()).map(|h| format!("{}/icons/{gid}/{h}.png?size=128", crate::api::cdn()));
    let brand = p["brand_color_primary"].as_str().and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok()).map(role_color).unwrap_or(BLURPLE);
    let (online, members) = (p["online_count"].as_u64().unwrap_or(0), p["member_count"].as_u64().unwrap_or(0));
    let (tier, boosts) = (p["premium_tier"].as_u64().unwrap_or(0), p["premium_subscription_count"].as_u64().unwrap_or(0));
    let gated = p["features"].as_array().is_some_and(|f| f.iter().any(|x| x == "MEMBER_VERIFICATION_GATE_ENABLED"));
    let icons_by_app = app.store.app_icons.clone();
    let mut go = false;
    let area = egui::Area::new(egui::Id::new("tag_pop")).order(egui::Order::Tooltip).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
            ui.set_width(300.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (band, _) = ui.allocate_exact_size(vec2(300.0, 60.0), Sense::hover());
            ui.painter().rect_filled(band, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, brand);
            let ic = Rect::from_min_size(pos2(band.left() + 16.0, band.bottom() - 35.0), vec2(70.0, 70.0));
            ui.painter().rect_filled(ic.expand(4.0), CornerRadius::same(24), pal().bg_secondary);
            ui.painter().rect_filled(ic, CornerRadius::same(20), pal().bg_accent);
            match &icon {
                Some(u) => crate::gif::paint_still(ui, u, ic, CornerRadius::same(20), false),
                None => {
                    ui.painter().text(ic.center(), Align2::CENTER_CENTER, name.chars().next().map(|c| c.to_string()).unwrap_or_default(), semi(24.0), pal().header_primary);
                }
            }
            ui.add_space(44.0);
            Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                ui.set_width(268.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(egui::RichText::new(&name).font(medium(20.0)).color(pal().header_primary));
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(268.0, 16.0), Sense::hover());
                let mut x = r.left();
                if !tag.is_empty() {
                    let g = ui.painter().layout_no_wrap(tag.clone(), semi(12.0), pal().header_primary);
                    let w = 8.0 + if badge.is_some() { 16.0 } else { 0.0 } + g.size().x;
                    let chip = Rect::from_min_size(r.min, vec2(w, 16.0));
                    ui.painter().rect_filled(chip, CornerRadius::same(4), Color32::from_rgba_unmultiplied(151, 151, 159, 61));
                    let mut tx = chip.left() + 4.0;
                    if let Some(b) = &badge {
                        text::image_tint(ui.painter(), b, Rect::from_min_size(pos2(tx, chip.top() + 1.0), vec2(14.0, 14.0)), Color32::WHITE);
                        tx += 16.0;
                    }
                    ui.painter().galley(pos2(tx, chip.center().y - g.size().y / 2.0), g, pal().header_primary);
                    x = chip.right() + 12.0;
                }
                if members > 0 {
                    ui.painter().circle_filled(pos2(x + 4.0, r.center().y), 4.0, status_color(crate::state::Status::Online));
                    let a = ui.painter().text(pos2(x + 14.0, r.center().y), Align2::LEFT_CENTER, format!("{online} Online"), reg(12.0), pal().header_secondary);
                    ui.painter().circle_filled(pos2(a.right() + 12.0, r.center().y), 4.0, pal().header_secondary);
                    ui.painter().text(pos2(a.right() + 22.0, r.center().y), Align2::LEFT_CENTER, format!("{members} Members"), reg(12.0), pal().header_secondary);
                }
                if !desc.is_empty() {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(&desc).font(reg(14.0)).color(pal().header_primary));
                }
                if tier > 0 || boosts > 0 {
                    ui.add_space(12.0);
                    let (r, _) = ui.allocate_exact_size(vec2(268.0, 18.0), Sense::hover());
                    icons::draw(ui.painter(), Icon::BoostGem, Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, format!("Level {tier} \u{2022} {boosts} Boost{}", if boosts == 1 { "" } else { "s" }), medium(12.0), pal().header_secondary);
                }
                if !apps.is_empty() {
                    ui.add_space(12.0);
                    let (r, _) = ui.allocate_exact_size(vec2(268.0, 24.0), Sense::hover());
                    for (i, a) in apps.iter().enumerate() {
                        let cell = Rect::from_min_size(pos2(r.left() + i as f32 * 28.0, r.top()), vec2(24.0, 24.0));
                        ui.painter().rect_filled(cell, CornerRadius::same(6), pal().bg_accent);
                        if let Some(h) = icons_by_app.get(a).cloned().flatten() {
                            text::image(ui.painter(), &format!("{}/app-icons/{a}/{h}.png?size=48", crate::api::cdn()), cell);
                        }
                    }
                }
                ui.add_space(16.0);
                let (b, resp) = ui.allocate_exact_size(vec2(268.0, 32.0), Sense::click());
                let label = if member { "Go to Server" } else if gated { "Apply to Join" } else { "Join Server" };
                ui.painter().rect_filled(b, CornerRadius::same(8), if resp.hovered() { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE });
                ui.painter().text(b.center(), Align2::CENTER_CENTER, label, medium(14.0), Color32::WHITE);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                go = resp.clicked();
            });
        });
    });
    if go {
        app.tag_pop = None;
        if member {
            app.ui.guild = Some(gid);
        } else if p["features"].as_array().is_some_and(|f| f.iter().any(|x| x == "MEMBER_VERIFICATION_GATE_ENABLED")) {
            app.rest("PUT", format!("/guilds/{gid}/requests/@me"), Some(json!({ "form_fields": [] })), Tag::Quiet);
            app.toast("Application sent.");
        } else {
            app.rest("PUT", format!("/guilds/{gid}/members/@me"), Some(json!({})), Tag::Quiet);
        }
    } else if popups::dismissed(ctx, &area.response) {
        app.tag_pop = None;
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
