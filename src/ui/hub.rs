//! Server Hub: the page a server's admins build out of widgets, its row over the channel list,
//! and the switch that hides it.

use egui::{Align2, Color32, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, menu_item};
use crate::net::Tag;
use crate::state::{Id, MANAGE_GUILD, Store};
use crate::theme::*;

pub struct Hub {
    pub guild: Id,
    pub editing: bool,
}

fn manages(store: &Store, gid: Id) -> bool {
    store.my_perms(gid) & MANAGE_GUILD != 0
}

pub fn in_experiment(store: &Store, gid: Id) -> bool {
    store.guild_experiments.get(&gid).is_some_and(|e| e.contains(&super::favorites::hashed("2026-09-guild-spaces")))
}

/// Discord's rule for the Server Hub row: the server is in the experiment, the hub isn't switched
/// off, and you either manage the server or the hub is published.
pub fn offered(store: &Store, gid: Id) -> bool {
    let Some(g) = store.guilds.get(&gid) else { return false };
    in_experiment(store, gid) && g.hub_enabled != Some(false) && (manages(store, gid) || (g.hub_enabled == Some(true) && g.hub_published))
}

impl App {
    pub fn open_hub(&mut self, gid: Id) {
        self.hub = Some(Hub { guild: gid, editing: false });
        self.boosts = None;
        self.home = None;
        self.server_shop = None;
        self.keyed.remove(&format!("hub:{gid}"));
    }
}

/// A house, drawn here because the app carries no icon for it.
pub fn house(p: &egui::Painter, r: Rect, c: Color32) {
    let (cx, w) = (r.center().x, r.width());
    let roof = vec![pos2(cx, r.top() + w * 0.12), pos2(r.right() - w * 0.08, r.top() + w * 0.5), pos2(r.left() + w * 0.08, r.top() + w * 0.5)];
    p.add(egui::Shape::convex_polygon(roof, c, Stroke::NONE));
    let body = Rect::from_min_max(pos2(r.left() + w * 0.2, r.top() + w * 0.46), pos2(r.right() - w * 0.2, r.bottom() - w * 0.1));
    p.rect_filled(body, 2, c);
}

fn key(gid: Id) -> String {
    format!("hub:{gid}")
}

fn fetch(app: &mut App, gid: Id) {
    for (k, path) in [(key(gid), format!("/guilds/{gid}/space")), (format!("hub_cat:{gid}"), format!("/guilds/{gid}/space/widgets/catalog"))] {
        if !app.keyed.contains_key(&k) {
            app.keyed.insert(k.clone(), None);
            app.rest("GET", path, None, Tag::Keyed(k));
        }
    }
}

fn widgets(space: Option<&Value>) -> Vec<Value> {
    let mut list: Vec<Value> = space.and_then(|s| s["widgets"].as_array()).cloned().unwrap_or_default();
    list.sort_by_key(|w| (w["position"]["column"].as_u64().unwrap_or(0), w["position"]["order"].as_u64().unwrap_or(0)));
    list
}

/// What discord's save sends: every widget with its place, ids only for the ones it already has.
pub fn save_body(list: &[Value]) -> Value {
    let mut order = [0u64; 2];
    let out: Vec<Value> = list
        .iter()
        .map(|w| {
            let col = w["position"]["column"].as_u64().unwrap_or(0).min(1);
            let mut o = json!({ "type": w["type"], "position": { "column": col, "order": order[col as usize] }, "config": w["config"] });
            order[col as usize] += 1;
            if let Some(id) = w["id"].as_str() {
                o["id"] = json!(id);
            }
            o
        })
        .collect();
    json!({ "header": {}, "widgets": out })
}

fn fresh(kind: &str, column: u64) -> Value {
    let config = if kind == "image_text" { json!({ "type": kind, "title": null, "image_hash": null, "text": null }) } else { json!({ "type": kind }) };
    json!({ "type": kind, "position": { "column": column, "order": 0 }, "config": config })
}

fn pill(ui: &mut Ui, text: &str, fill: Option<Color32>) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), if fill.is_some() { Color32::WHITE } else { pal().header_primary });
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 32.0), Sense::click());
    match fill {
        Some(c) => ui.painter().rect_filled(r, 8, if resp.hovered() { c.gamma_multiply(0.85) } else { c }),
        None => ui.painter().rect(r, 8, if resp.hovered() { pal().control_hover } else { pal().control }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside),
    };
    let ink = if fill.is_some() { Color32::WHITE } else { pal().header_primary };
    ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, ink);
    super::settings::mark(ui, &format!("hub:{text}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

enum Act {
    Edit(bool),
    Add(String, u64),
    Remove(usize),
    Publish,
}

fn column(ui: &mut Ui, col: u64, list: &[Value], catalog: &[Value], editing: bool, act: &mut Option<Act>) {
    let name = |kind: &str| catalog.iter().find(|c| c["type"] == kind).and_then(|c| c["name"].as_str()).unwrap_or(kind).to_string();
    let mut any = false;
    for (i, w) in list.iter().enumerate().filter(|(_, w)| w["position"]["column"].as_u64().unwrap_or(0).min(1) == col) {
        any = true;
        let kind = w["type"].as_str().unwrap_or("");
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(12).inner_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let title = w["config"]["title"].as_str().filter(|t| !t.is_empty()).map(str::to_string).unwrap_or_else(|| name(kind));
                ui.label(RichText::new(title).font(semi(16.0)).color(pal().header_primary));
                if editing {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        icons::draw(ui.painter(), Icon::Trash, r.shrink(4.0), if resp.hovered() { pal().critical } else { pal().header_secondary });
                        super::settings::mark(ui, &format!("hub:remove:{i}"), r);
                        if resp.on_hover_text(format!("Remove {}", name(kind))).clicked() {
                            *act = Some(Act::Remove(i));
                        }
                    });
                }
            });
            ui.add_space(8.0);
            let body = w["config"]["text"].as_str().or_else(|| catalog.iter().find(|c| c["type"] == kind).and_then(|c| c["description"].as_str())).unwrap_or("");
            ui.label(RichText::new(body).font(reg(14.0)).color(pal().text_normal));
        });
        ui.add_space(16.0);
    }
    if !editing {
        return;
    }
    if !any {
        ui.label(RichText::new("Add widgets to this column.").font(reg(14.0)).color(pal().text_muted));
        ui.add_space(8.0);
    }
    let add = pill(ui, "Add Widget", None);
    super::settings::mark(ui, &format!("hub:add:{col}"), add.rect);
    egui::Popup::menu(&add).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for c in catalog {
            let kind = c["type"].as_str().unwrap_or("");
            let have = list.iter().filter(|w| w["type"] == kind).count() as u64;
            let room = have < c["max_instances"].as_u64().unwrap_or(1) && c["locked"] != true;
            if ui.add_enabled_ui(room, |ui| menu_item(ui, None, c["name"].as_str().unwrap_or(kind), false)).inner {
                *act = Some(Act::Add(kind.to_string(), col));
            }
        }
    });
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    fetch(app, gid);
    if !app.settings.hub_seen {
        app.settings.hub_seen = true;
        app.save_settings(ui.ctx());
    }
    let manager = manages(&app.store, gid);
    let editing = app.hub.as_ref().is_some_and(|h| h.editing) && manager;
    let published = app.store.guilds.get(&gid).is_some_and(|g| g.hub_published);
    let space = match app.keyed.get(&key(gid)) {
        Some(Some(Ok(v))) => Some(v.clone()),
        _ => None,
    };
    let loading = matches!(app.keyed.get(&key(gid)), Some(None));
    let catalog: Vec<Value> = match app.keyed.get(&format!("hub_cat:{gid}")) {
        Some(Some(Ok(v))) => v["widgets"].as_array().cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    let list = widgets(space.as_ref());
    let mut act = None;
    egui::Panel::top("hub_head").exact_size(49.0).resizable(false).show_separator_line(false).frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 20, right: 16, top: 0, bottom: 1 })).show(ui, |ui| {
        let r = ui.max_rect();
        ui.painter().hline(r.left() - 20.0..=r.right() + 16.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
        house(ui.painter(), Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(20.0, 20.0)), pal().channel_default);
        ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, "Server Hub", medium(16.0), pal().header_primary);
        if manager {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !published && !list.is_empty() && pill(ui, "Publish", Some(pal().blurple)).clicked() {
                    act = Some(Act::Publish);
                }
                if pill(ui, if editing { "Done" } else { "Edit Server Hub" }, None).clicked() {
                    act = Some(Act::Edit(!editing));
                }
            });
        }
    });
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        if loading {
            ui.centered_and_justified(|ui| crate::ui::motion::spinner(ui, 32.0, pal().text_muted));
            return;
        }
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(32, 24)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                if manager && !published && !list.is_empty() {
                    Frame::new().fill(pal().control).corner_radius(8).inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("This Server Hub is a draft. Only server admins can see it until it's published.").font(reg(14.0)).color(pal().text_normal));
                    });
                    ui.add_space(16.0);
                }
                if list.is_empty() && !editing {
                    let r = Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(12).inner_margin(24).show(ui, |ui| {
                        ui.set_width(ui.available_width().min(520.0));
                        ui.label(RichText::new("Welcome to Server Hub").font(semi(20.0)).color(pal().header_primary));
                        ui.add_space(8.0);
                        ui.label(RichText::new("You can customize the layout and content to create a unique Server Hub for your community.").font(reg(16.0)).color(pal().header_secondary));
                        ui.add_space(16.0);
                        for (head, body) in [("Customizable", "Add the widgets your server will actually use."), ("Always changing", "Widgets refresh automatically with the latest activity.")] {
                            ui.label(RichText::new(head).font(medium(16.0)).color(pal().header_primary));
                            ui.add_space(4.0);
                            ui.label(RichText::new(body).font(reg(14.0)).color(pal().header_secondary));
                            ui.add_space(16.0);
                        }
                    });
                    super::settings::mark(ui, "hub:welcome", r.response.rect);
                    return;
                }
                let w = ui.available_width();
                let side = (w * 0.34).clamp(220.0, 340.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.vertical(|ui| {
                        ui.set_width(w - side - 16.0);
                        column(ui, 0, &list, &catalog, editing, &mut act);
                    });
                    ui.vertical(|ui| {
                        ui.set_width(side);
                        column(ui, 1, &list, &catalog, editing, &mut act);
                    });
                });
            });
        });
    });
    let put = |app: &mut App, list: &[Value]| {
        app.keyed.insert(key(gid), None);
        app.rest("PUT", format!("/guilds/{gid}/space"), Some(save_body(list)), Tag::Keyed(key(gid)));
    };
    match act {
        Some(Act::Edit(on)) => {
            if let Some(h) = &mut app.hub {
                h.editing = on;
            }
        }
        Some(Act::Add(kind, col)) => {
            let mut next = list.clone();
            next.push(fresh(&kind, col));
            put(app, &next);
        }
        Some(Act::Remove(i)) => {
            let mut next = list.clone();
            next.remove(i);
            put(app, &next);
        }
        Some(Act::Publish) => {
            if let Some(g) = app.store.guilds.get_mut(&gid) {
                g.hub_published = true;
                g.hub_enabled = Some(true);
            }
            app.rest("PATCH", format!("/guilds/{gid}/space/settings"), Some(json!({ "publish_status": 1 })), Tag::fail(super::SAVE_FAILED));
            app.toast("Server Hub published. Everyone in the server can see it now.");
        }
        None => {}
    }
}

/// Server settings > Engagement: whether the Server Hub row shows in the channel list.
pub fn set_enabled(app: &mut App, gid: Id, on: bool) {
    if let Some(g) = app.store.guilds.get_mut(&gid) {
        g.hub_enabled = Some(on);
    }
    app.rest("PATCH", format!("/guilds/{gid}/space/settings"), Some(json!({ "enabled": on })), Tag::fail(super::SAVE_FAILED));
}
