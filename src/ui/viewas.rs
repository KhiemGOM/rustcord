//! View Server as Role: pick a role in Server Settings and the server's channels and permissions
//! are worked out as that role has them, under a bar that goes back to the settings, changes the
//! roles or turns it off.

use egui::{Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::state::{Id, MANAGE_GUILD, MANAGE_ROLES};
use crate::theme::*;

/// Owners and admins who can manage roles, for a role at or below their highest.
pub fn can(app: &App, gid: Id, role: Id) -> bool {
    let Some(g) = app.store.guilds.get(&gid) else { return false };
    if role == gid {
        return false;
    }
    if g.owner == app.store.me {
        return true;
    }
    let p = app.store.my_perms(gid);
    let top = g.my_roles.iter().filter_map(|r| g.roles.get(r)).map(|r| r.position).max().unwrap_or(0);
    p & MANAGE_GUILD != 0 && p & MANAGE_ROLES != 0 && g.roles.get(&role).is_some_and(|r| r.position <= top)
}

pub fn start(app: &mut App, gid: Id, role: Id) {
    app.server = None;
    app.store.viewing = Some((gid, vec![role]));
    app.select_guild(Some(gid));
}

fn outlined(ui: &mut Ui, label: &str, back: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), Color32::WHITE);
    let w = g.size().x + 24.0 + if back { 20.0 } else { 0.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    let fill = if resp.hovered() { Color32::from_white_alpha(31) } else { Color32::TRANSPARENT };
    ui.painter().rect(r, CornerRadius::same(4), fill, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
    let mut x = r.left() + 12.0;
    if back {
        icons::draw(ui.painter(), Icon::ArrowLeft, Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(16.0, 16.0)), Color32::WHITE);
        x += 20.0;
    }
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn bar(app: &mut App, ui: &mut Ui) {
    let Some((gid, picked)) = app.store.viewing.clone() else { return };
    let mut back = false;
    let mut off = false;
    let mut toggle = None;
    let me_top = app.store.guilds.get(&gid).map_or(0, |g| g.my_roles.iter().filter_map(|r| g.roles.get(r)).map(|r| r.position).max().unwrap_or(0));
    let owner = app.store.guilds.get(&gid).is_some_and(|g| g.owner == app.store.me);
    let mut roles: Vec<(Id, String)> = app.store.guilds.get(&gid).map(|g| g.roles.values().filter(|r| r.id != gid && !r.managed && (owner || r.position <= me_top || picked.contains(&r.id))).map(|r| (r.id, r.name.clone())).collect()).unwrap_or_default();
    roles.sort_by_key(|r| std::cmp::Reverse(app.store.guilds.get(&gid).and_then(|g| g.roles.get(&r.0)).map_or(0, |r| r.position)));
    egui::Panel::top("viewing_bar")
        .exact_size(40.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BLURPLE).inner_margin(egui::Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let b = outlined(ui, "Back to Roles Settings", true);
                app.rects.insert("viewas:back".into(), b.rect);
                back = b.clicked();
                let n = picked.len();
                let text = format!("You are viewing this server as {}.", match n { 0 => "@everyone".to_string(), 1 => "1 role".into(), n => format!("{n} roles") });
                ui.label(egui::RichText::new(text).font(medium(14.0)).color(Color32::WHITE));
                let sel = outlined(ui, "Select Roles", false);
                app.rects.insert("viewas:select".into(), sel.rect);
                egui::Popup::menu(&sel).show(|ui| {
                    super::menu_body(ui, |ui| {
                        for (id, name) in &roles {
                            let on = picked.contains(id);
                            if super::menu_item(ui, if on { Some(Icon::Check) } else { None }, name, false) {
                                toggle = Some(*id);
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let d = outlined(ui, "Disable", false);
                    app.rects.insert("viewas:disable".into(), d.rect);
                    off = d.clicked();
                });
            });
        });
    if let Some(id) = toggle {
        if let Some(v) = &mut app.store.viewing {
            if v.1.contains(&id) {
                v.1.retain(|r| *r != id);
            } else {
                v.1.push(id);
            }
        }
    }
    if back {
        app.store.viewing = None;
        app.open_server_settings(gid);
        if let Some(s) = &mut app.server {
            s.page = super::server::Page::Roles;
        }
    } else if off {
        app.store.viewing = None;
    }
}
