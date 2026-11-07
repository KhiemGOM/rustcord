use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::role_edit::{self, Edit};
use super::{App, faint, title};
use crate::net::Tag;
use crate::state::{Id, Role, sid};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

#[derive(Default)]
pub struct Roles {
    pub query: String,
    pub edit: Option<Edit>,
    drag: Option<Id>,
}

impl Roles {
    pub fn deep(&self) -> bool {
        self.edit.is_some()
    }
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    if app.server.as_ref().is_some_and(|s| s.pages.roles.edit.is_some()) {
        return role_edit::page(app, ui, gid);
    }
    list(app, ui, gid);
}

pub fn open(app: &mut App, gid: Id, id: Id, tab: u8) {
    let Some(role) = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&id)).cloned() else { return };
    if let Some(s) = &mut app.server {
        s.data.remove("role_members");
        s.pages.roles.edit = Some(Edit::new(&role, tab));
    }
    if id == gid {
        if let Some(s) = &mut app.server {
            s.data.insert("role_members", json!([]));
        }
    } else {
        app.rest("GET", format!("/guilds/{gid}/roles/{id}/member-ids"), None, Tag::Server("role_members"));
    }
}

pub fn saved(app: &mut App, key: &'static str, v: Value) {
    let Some(gid) = app.server.as_ref().map(|s| s.guild) else { return };
    if v["id"].is_null() {
        return;
    }
    let ev = if key == "role_new" { "GUILD_ROLE_CREATE" } else { "GUILD_ROLE_UPDATE" };
    app.store.apply(ev, &json!({ "guild_id": gid.to_string(), "role": v }));
    let id = sid(&v["id"]);
    if key == "role_new" {
        open(app, gid, id, 0);
        if let Some(e) = app.server.as_mut().and_then(|s| s.pages.roles.edit.as_mut()) {
            e.focus_name = true;
        }
    } else if let Some(role) = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&id)).cloned()
        && let Some(e) = app.server.as_mut().and_then(|s| s.pages.roles.edit.as_mut()).filter(|e| e.id == id)
    {
        e.rebase(&role);
    }
}

fn list(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Roles", "Use roles to group your server members and assign permissions.");
    let counts: HashMap<String, u64> = app
        .server_data("role_counts", format!("/guilds/{gid}/roles/member-counts"))
        .and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0))).collect())
        .unwrap_or_default();
    let mut act: Option<(&str, Id, u8)> = None;
    let r = Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(51))).corner_radius(8).inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
        let c = pos2(r.left() + 20.0, r.center().y);
        ui.painter().circle_filled(c, 20.0, Color32::from_black_alpha(80));
        icons::draw(ui.painter(), Icon::People, Rect::from_center_size(c, vec2(20.0, 20.0)), pal().header_primary);
        ui.painter().text(pos2(r.left() + 60.0, r.center().y - 9.0), Align2::LEFT_CENTER, "Default Permissions", bold(16.0), pal().header_primary);
        ui.painter().text(pos2(r.left() + 60.0, r.center().y + 11.0), Align2::LEFT_CENTER, "@everyone \u{2022} applies to all server members", reg(12.0), pal().header_secondary);
        icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp.clicked()
    });
    if r.inner {
        act = Some(("edit", gid, 1));
    }
    ui.add_space(24.0);
    let mut create = false;
    ui.horizontal(|ui| {
        let w = ui.available_width() - 132.0;
        ui.allocate_ui(vec2(w, 40.0), |ui| {
            let q = &mut app.server.as_mut().unwrap().pages.roles.query;
            settings::input(ui, q, "Search Roles", false);
        });
        create = settings::primary(ui, "Create Role").clicked();
    });
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Members use the color of the highest role they have on this list. Drag roles to reorder them. ").font(reg(14.0)).color(pal().text_normal));
        if settings::link(ui, "Need help with permissions?").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/206029707"));
        }
    });
    ui.add_space(24.0);
    let q = app.server.as_ref().map(|s| s.pages.roles.query.trim().to_lowercase()).unwrap_or_default();
    let Some(g) = app.store.guilds.get(&gid) else { return };
    let mut all: Vec<&Role> = g.roles.values().filter(|r| r.id != gid).collect();
    all.sort_by_key(|r| std::cmp::Reverse(r.position));
    let ids: Vec<Id> = all.iter().map(|r| r.id).collect();
    let shown: Vec<&Role> = all.iter().copied().filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q)).collect();
    let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().text(pos2(hr.left(), hr.center().y), Align2::LEFT_CENTER, format!("ROLES \u{2013} {}", all.len()), bold(12.0), pal().header_secondary);
    ui.painter().text(pos2(hr.left() + hr.width() * 0.55, hr.center().y), Align2::LEFT_CENTER, "MEMBERS", bold(12.0), pal().header_secondary);
    ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, faint(51)));
    let mut rects: Vec<(Id, Rect)> = Vec::new();
    let mut drag_over = None;
    let mut drop = None;
    let dragging = app.server.as_ref().and_then(|s| s.pages.roles.drag);
    for role in &shown {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click_and_drag());
        rects.push((role.id, r));
        if resp.hovered() || dragging == Some(role.id) {
            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
        }
        ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(31)));
        let c = if role.color != 0 { role_color(role.color) } else { pal().header_secondary };
        let ic = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0));
        match role.icon_url(48) {
            Some(u) => crate::ui::text::image(ui.painter(), &u, ic),
            None => icons::draw(ui.painter(), Icon::Shield, ic, c),
        }
        crate::ui::text::paint_em(ui.painter(), pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, &role.name, medium(16.0), pal().header_primary, r.width() * 0.55 - 60.0, 1.0);
        let n = counts.get(&role.id.to_string()).copied().unwrap_or(0);
        let cr = Rect::from_min_size(pos2(r.left() + r.width() * 0.55, r.center().y - 14.0), vec2(80.0, 28.0));
        let cresp = ui.interact(cr, ui.id().with(("role_count", role.id)), Sense::click());
        ui.painter().text(pos2(cr.left(), cr.center().y), Align2::LEFT_CENTER, n.to_string(), medium(16.0), pal().header_secondary);
        icons::draw(ui.painter(), Icon::Person, Rect::from_center_size(pos2(cr.left() + 30.0, cr.center().y), vec2(16.0, 16.0)), pal().header_secondary);
        if cresp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if cresp.on_hover_text("View members").clicked() {
            act = Some(("edit", role.id, 3));
        }
        let mut x = r.right() - 20.0;
        for (i, (icon, tip)) in [(Icon::More, "More"), (Icon::Pencil, "Edit")].into_iter().enumerate() {
            let b = Rect::from_center_size(pos2(x, r.center().y), vec2(36.0, 36.0));
            x -= 44.0;
            let br = ui.interact(b, ui.id().with(("role_btn", role.id, i)), Sense::click());
            app.rects.insert(format!("role:{}:{}", if i == 0 { "more" } else { "edit" }, role.id), b);
            ui.painter().rect(b, CornerRadius::same(8), crate::ui::motion::mix(faint(31), faint(51), crate::ui::motion::hot(ui.ctx(), &br)), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
            icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(18.0, 18.0)), pal().header_primary);
            if br.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if i == 1 {
                if br.on_hover_text(tip).clicked() || resp.clicked() {
                    act = Some(("edit", role.id, 0));
                }
            } else {
                let rid = role.id;
                egui::Popup::menu(&br).show(|ui| {
                    crate::ui::menu_body(ui, |ui| {
                        if crate::ui::viewas::can(app, gid, rid) && crate::ui::menu_item(ui, Some(Icon::Eye), "View Server as Role", false) {
                            act = Some(("view", rid, 0));
                        }
                        if crate::ui::menu_item(ui, Some(Icon::Copy), "Duplicate Role", false) {
                            act = Some(("dup", rid, 0));
                        }
                        if crate::ui::menu_item(ui, Some(Icon::Id), "Copy Role ID", false) {
                            act = Some(("id", rid, 0));
                        }
                        if crate::ui::menu_item(ui, Some(Icon::Trash), "Delete Role", true) {
                            act = Some(("delete", rid, 0));
                        }
                    });
                });
            }
        }
        if resp.dragged() && q.is_empty() {
            drag_over = Some(role.id);
        }
        if resp.drag_stopped() && q.is_empty() {
            drop = Some(role.id);
        }
    }
    if shown.is_empty() {
        ui.add_space(24.0);
        ui.label(RichText::new("No roles found.").font(reg(14.0)).color(pal().header_secondary));
    }
    if let Some(id) = drag_over.or(drop) {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            let to = rects.iter().filter(|(_, r)| r.center().y < p.y).count();
            let from = rects.iter().position(|(r, _)| *r == id).unwrap_or(0);
            let y = rects.get(to).map_or_else(|| rects.last().map_or(0.0, |(_, r)| r.bottom()), |(_, r)| r.top());
            if let Some((_, r0)) = rects.first() {
                ui.painter().hline(r0.x_range(), y, Stroke::new(2.0, BLURPLE));
            }
            if drop.is_some() && to != from && to != from + 1 {
                let mut order = ids.clone();
                let at = order.iter().position(|i| *i == id).unwrap_or(0);
                let moved = order.remove(at);
                let target = if to > from { to - 1 } else { to };
                order.insert(target.min(order.len()), moved);
                reorder(app, gid, order);
            }
        }
    }
    if let Some(s) = &mut app.server {
        s.pages.roles.drag = drag_over;
    }
    if create {
        app.rest("POST", format!("/guilds/{gid}/roles"), Some(json!({ "name": "new role" })), Tag::Server("role_new"));
    }
    match act {
        Some(("edit", id, tab)) => open(app, gid, id, tab),
        Some(("id", id, _)) => ui.ctx().copy_text(id.to_string()),
        Some(("view", id, _)) => crate::ui::viewas::start(app, gid, id),
        Some(("dup", id, _)) => duplicate(app, gid, id),
        Some(("delete", id, _)) => delete(app, gid, id),
        _ => {}
    }
}

fn reorder(app: &mut App, gid: Id, order: Vec<Id>) {
    let n = order.len() as i32;
    let mut body = Vec::new();
    let before: Vec<(Id, i32)> = app.store.guilds.get(&gid).map(|g| g.roles.values().map(|r| (r.id, r.position)).collect()).unwrap_or_default();
    if let Some(g) = app.store.guilds.get_mut(&gid) {
        for (i, id) in order.iter().enumerate() {
            let pos = n - i as i32;
            if let Some(r) = g.roles.get_mut(id).filter(|r| r.position != pos) {
                r.position = pos;
                body.push(json!({ "id": id.to_string(), "position": pos }));
            }
        }
    }
    if !body.is_empty() {
        app.rest("PATCH", format!("/guilds/{gid}/roles"), Some(Value::Array(body)), Tag::rollback(move |app: &mut App| {
            if let Some(g) = app.store.guilds.get_mut(&gid) {
                for (id, position) in &before {
                    if let Some(r) = g.roles.get_mut(id) {
                        r.position = *position;
                    }
                }
            }
        }));
    }
}

fn duplicate(app: &mut App, gid: Id, id: Id) {
    let Some(r) = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&id)).cloned() else { return };
    let body = json!({ "name": format!("{} copy", r.name), "color": r.color, "hoist": r.hoist, "mentionable": r.mentionable, "permissions": r.perms.to_string() });
    app.rest("POST", format!("/guilds/{gid}/roles"), Some(body), Tag::Server("role_new"));
}

fn delete(app: &mut App, gid: Id, id: Id) {
    let Some(name) = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&id)).map(|r| r.name.clone()) else { return };
    app.confirm = Some(crate::ui::Confirm::new("Delete Role", format!("Are you sure you want to delete **{name}**? This cannot be undone."), "Delete", move |app: &mut App| {
        let kept = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&id)).cloned();
        app.rest("DELETE", format!("/guilds/{gid}/roles/{id}"), None, Tag::rollback(move |app: &mut App| {
            if let (Some(r), Some(g)) = (&kept, app.store.guilds.get_mut(&gid)) {
                g.roles.insert(id, r.clone());
            }
        }));
        app.store.apply("GUILD_ROLE_DELETE", &json!({ "guild_id": gid.to_string(), "role_id": id.to_string() }));
        if let Some(s) = &mut app.server {
            s.pages.roles.edit = None;
            s.data.remove("role_counts");
        }
    }));
}
