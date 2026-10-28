//! Vencord's Plugins page, filled from the plugins the host loaded: Vencord's own and the
//! userplugins folders'. Each card says what of the plugin runs in Fastcord.

use egui::{Align2, Color32, Id, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{dropdown, input, note, section, slider};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

const SHOW: [&str; 4] = ["Show All", "Show Enabled", "Show Disabled", "Show Working in Fastcord"];

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Plugins");
    if app.vc.host.is_none() {
        let dir = crate::cache::dir().join("userplugins");
        note(ui, &format!("No Vencord here to load plugins from. Install Vencord (vencord.dev), or put plugin folders in {}.", dir.display()));
        return false;
    }
    if app.vc.list.is_none() {
        let paths = app.vc.host.as_ref().map(|h| h.plugins()).unwrap_or_default();
        app.vc.list = Some(app.vc_call("list", json!({ "paths": paths })));
    }
    let list: Vec<Value> = app.vc.list.as_ref().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let list: Vec<Value> = list.into_iter().filter(|p| p["hidden"] != true).collect();
    let on = list.iter().filter(|p| p["enabled"] == true).count();
    note(ui, &format!("{} plugins from Vencord and your userplugins, {on} on. Parts that patch Discord's own client don't run here; everything built on Vencord's APIs does.", list.len()));
    super::lead(ui);
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.allocate_ui(vec2(w - 232.0, 40.0), |ui| {
            let r = input(ui, &mut app.sform.plugin_search, "Search for a plugin...", false).rect;
            super::mark(ui, "plugins:search", r);
        });
        dropdown(ui, 220.0, None, SHOW[app.sform.plugin_show as usize], |ui| {
            for (i, l) in SHOW.iter().enumerate() {
                if ui.selectable_label(app.sform.plugin_show as usize == i, *l).clicked() {
                    app.sform.plugin_show = i as u8;
                }
            }
        });
    });
    super::end(ui, 24.0);
    let q = app.sform.plugin_search.to_lowercase();
    for p in &list {
        let enabled = p["enabled"] == true;
        let works = p["parts"].as_array().is_some_and(|a| !a.is_empty());
        let shown = match app.sform.plugin_show {
            1 => enabled,
            2 => !enabled,
            3 => works,
            _ => true,
        };
        let name = p["name"].as_str().unwrap_or("");
        let desc = p["description"].as_str().unwrap_or("");
        if shown && (q.is_empty() || name.to_lowercase().contains(&q) || desc.to_lowercase().contains(&q)) {
            card(app, ui, p);
        }
    }
    false
}

fn status(p: &Value) -> Option<(String, Color32)> {
    if let Some(e) = p["error"].as_str() {
        return Some((format!("Couldn't load: {}", e.lines().next().unwrap_or(e)), RED));
    }
    let parts: Vec<&str> = p["parts"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
    let patches = p["patches"].as_u64().unwrap_or(0);
    let names: Vec<&str> = parts
        .iter()
        .map(|k| match *k {
            "start" => "its startup code",
            "flux" => "event listeners",
            "commands" => "slash commands",
            "contextMenus" => "menu items",
            "chatBarButton" => "a chat bar button",
            "messagePopoverButton" => "a message button",
            "onBeforeMessageSend" | "onBeforeMessageEdit" => "message rewriting",
            "onMessageClick" => "message clicks",
            other => other,
        })
        .collect();
    match (names.is_empty(), patches) {
        (true, 0) => None,
        (true, n) => Some((format!("All {n} of its changes patch Discord's own client, so it does nothing in Fastcord."), pal().text_muted)),
        (false, 0) => Some((format!("Runs here: {}.", names.join(", ")), pal().text_muted)),
        (false, n) => Some((format!("Runs here: {}. Its {n} patches to Discord's own client don't.", names.join(", ")), pal().text_muted)),
    }
}

fn card(app: &mut App, ui: &mut Ui, p: &Value) {
    let name = p["name"].as_str().unwrap_or("").to_string();
    let enabled = p["enabled"] == true;
    let options: Vec<&Value> = p["options"].as_array().into_iter().flatten().filter(|o| o["hidden"] != true).collect();
    let open = app.sform.plugin_open.contains(&name);
    super::lead(ui);
    egui::Frame::new()
        .fill(pal().bg_secondary)
        .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.set_width(w);
            let desc = ui.painter().layout(p["description"].as_str().unwrap_or("").into(), reg(14.0), pal().header_secondary, w - 100.0);
            let st = status(p).map(|(s, c)| ui.painter().layout(s, reg(12.0), c, w - 100.0));
            let h = 24.0 + desc.size().y + st.as_ref().map_or(0.0, |g| 6.0 + g.size().y);
            let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            ui.painter().text(r.left_top(), Align2::LEFT_TOP, &name, semi(16.0), pal().header_primary);
            let dy = desc.size().y;
            ui.painter().galley(r.left_top() + vec2(0.0, 24.0), desc, pal().header_secondary);
            if let Some(g) = st {
                ui.painter().galley(r.left_top() + vec2(0.0, 30.0 + dy), g, pal().text_muted);
            }
            let sw = Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0));
            let sresp = ui.interact(sw, Id::new(("vc_switch", &name)), Sense::click());
            super::paint_switch(ui, sw, &sresp, enabled);
            super::mark(ui, &format!("plugin:{name}"), sw);
            if sresp.clicked() && p["error"].is_null() {
                app.vc_call("setEnabled", json!({ "name": name, "on": !enabled }));
            }
            if !options.is_empty() {
                let cog = Rect::from_center_size(pos2(sw.left() - 22.0, sw.center().y), vec2(24.0, 24.0));
                let cresp = ui.interact(cog, Id::new(("vc_cog", &name)), Sense::click());
                icons::draw(ui.painter(), Icon::Gear, Rect::from_center_size(cog.center(), vec2(20.0, 20.0)), if open || cresp.hovered() { pal().header_primary } else { pal().text_muted });
                if cresp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if cresp.clicked() || (resp.clicked() && !sresp.hovered()) {
                    if open {
                        app.sform.plugin_open.remove(&name);
                    } else {
                        app.sform.plugin_open.insert(name.clone());
                    }
                }
            }
            if open && !options.is_empty() {
                ui.add_space(16.0);
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 31)));
                ui.add_space(16.0);
                for o in options {
                    option(app, ui, &name, o);
                }
            }
        });
    super::end(ui, 8.0);
}

/// One option, drawn by Vencord's OptionType: string, number, bigint, boolean, select, slider.
/// Component and custom options are React UI of the plugin's own.
fn option(app: &mut App, ui: &mut Ui, plugin: &str, o: &Value) {
    let key = o["key"].as_str().unwrap_or("");
    let desc = o["description"].as_str().filter(|d| !d.is_empty()).unwrap_or(key);
    let set = |app: &mut App, v: Value| {
        app.vc_call("setOption", json!({ "name": plugin, "key": key, "value": v }));
    };
    match o["type"].as_u64().unwrap_or(99) {
        3 => {
            let mut v = o["value"] == true;
            if super::toggle(ui, &mut v, desc, "") {
                set(app, v.into());
            }
        }
        t @ (0..=2) => {
            super::lead(ui);
            ui.label(RichText::new(desc).font(medium(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let id = Id::new(("vc_text", plugin, key));
            let shown = match &o["value"] {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                v => v.to_string(),
            };
            let mut s = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or(shown);
            if input(ui, &mut s, key, false).changed() {
                let v = match t {
                    1 => s.trim().parse::<f64>().ok().map(Value::from),
                    _ => Some(Value::from(s.clone())),
                };
                if let Some(v) = v {
                    set(app, v);
                }
            }
            ui.data_mut(|d| d.insert_temp(id, s));
            super::end(ui, 20.0);
        }
        4 => {
            super::lead(ui);
            ui.label(RichText::new(desc).font(medium(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let choices: Vec<(String, Value)> = o["choices"].as_array().into_iter().flatten().map(|c| (c["label"].as_str().unwrap_or("").to_string(), c["value"].clone())).collect();
            let shown = choices.iter().find(|(_, v)| *v == o["value"]).map_or("", |(l, _)| l.as_str()).to_string();
            let mut pick = None;
            dropdown(ui, ui.available_width(), None, &shown, |ui| {
                for (l, v) in &choices {
                    if ui.selectable_label(*v == o["value"], l).clicked() {
                        pick = Some(v.clone());
                    }
                }
            });
            if let Some(v) = pick {
                set(app, v);
            }
            super::end(ui, 20.0);
        }
        5 => {
            super::lead(ui);
            let (lo, hi) = (o["min"].as_f64().unwrap_or(0.0) as f32, o["max"].as_f64().unwrap_or(1.0) as f32);
            let mut v = o["value"].as_f64().unwrap_or(lo as f64) as f32;
            ui.label(RichText::new(format!("{desc}: {}", (v * 100.0).round() / 100.0)).font(medium(14.0)).color(pal().header_primary));
            ui.add_space(12.0);
            if slider(ui, ui.available_width(), &mut v, lo, hi) {
                let markers: Vec<f32> = o["markers"].as_array().into_iter().flatten().filter_map(|m| m.as_f64()).map(|m| m as f32).collect();
                if o["stickToMarkers"] == true
                    && let Some(m) = markers.iter().copied().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
                {
                    v = m;
                }
                set(app, ((v * 100.0).round() / 100.0).into());
            }
            super::end(ui, 20.0);
        }
        _ => note(ui, &format!("{desc}: set in Vencord; this option is the plugin's own UI.")),
    }
}
