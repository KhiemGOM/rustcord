//! Vencord's plugins page: search, a filter, a card per plugin with its switch, and its
//! options unfolded under it.

use egui::{Align2, Color32, Id, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::{dropdown, input, note, section, slider};
use crate::plugins::{self, Kind, Plugin};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Show {
    #[default]
    All,
    Enabled,
    Disabled,
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Plugins");
    let on = plugins::ALL.iter().filter(|p| plugins::on(p.name)).count();
    note(ui, &format!("{} plugins, {on} enabled. They act the moment they're switched on; nothing needs a restart.", plugins::ALL.len()));
    if plugins::vencord_settings().exists() && super::action(ui, "Import from Vencord", "Turn on the plugins your Vencord install has on, with their options.", "Import", super::Look::Secondary) {
        let n = plugins::import_vencord();
        app.toast(format!("Imported {n} enabled plugins from Vencord"));
    }
    super::lead(ui);
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.allocate_ui(vec2(w - 192.0, 40.0), |ui| {
            let r = input(ui, &mut app.sform.plugin_search, "Search for a plugin...", false).rect;
            super::mark(ui, "plugins:search", r);
        });
        let label = match app.sform.plugin_show {
            Show::All => "Show All",
            Show::Enabled => "Show Enabled",
            Show::Disabled => "Show Disabled",
        };
        dropdown(ui, 180.0, None, label, |ui| {
            for (s, l) in [(Show::All, "Show All"), (Show::Enabled, "Show Enabled"), (Show::Disabled, "Show Disabled")] {
                if ui.selectable_label(app.sform.plugin_show == s, l).clicked() {
                    app.sform.plugin_show = s;
                }
            }
        });
    });
    super::end(ui, 24.0);

    let q = app.sform.plugin_search.to_lowercase();
    for p in plugins::ALL {
        let enabled = plugins::on(p.name);
        let shown = match app.sform.plugin_show {
            Show::All => true,
            Show::Enabled => enabled,
            Show::Disabled => !enabled,
        };
        if !shown || !(q.is_empty() || p.name.to_lowercase().contains(&q) || p.desc.to_lowercase().contains(&q)) {
            continue;
        }
        card(app, ui, p, enabled);
    }
    false
}

fn card(app: &mut App, ui: &mut Ui, p: &'static Plugin, enabled: bool) {
    super::lead(ui);
    let open = app.sform.plugin_open.contains(p.name);
    let has_opts = !p.opts.is_empty() || custom(p.name);
    egui::Frame::new()
        .fill(pal().bg_secondary)
        .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.set_width(w);
            let desc = ui.painter().layout(p.desc.into(), reg(14.0), pal().header_secondary, w - 100.0);
            let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0 + desc.size().y), Sense::click());
            ui.painter().text(r.left_top(), Align2::LEFT_TOP, p.name, semi(16.0), pal().header_primary);
            ui.painter().galley(r.left_top() + vec2(0.0, 24.0), desc, pal().header_secondary);
            let sw = Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0));
            let sresp = ui.interact(sw, Id::new(("plugin_switch", p.name)), Sense::click());
            super::paint_switch(ui, sw, &sresp, enabled);
            super::mark(ui, &format!("plugin:{}", p.name), sw);
            if sresp.clicked() {
                plugins::enable(p.name, !enabled);
                app.heights.clear();
            }
            if has_opts {
                let cog = Rect::from_center_size(pos2(sw.left() - 22.0, sw.center().y), vec2(24.0, 24.0));
                let cresp = ui.interact(cog, Id::new(("plugin_cog", p.name)), Sense::click());
                icons::draw(ui.painter(), Icon::Gear, Rect::from_center_size(cog.center(), vec2(20.0, 20.0)), if open || cresp.hovered() { pal().header_primary } else { pal().text_muted });
                super::mark(ui, &format!("plugin_cog:{}", p.name), cog);
                if cresp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if cresp.clicked() || (resp.clicked() && !sresp.hovered()) {
                    if open {
                        app.sform.plugin_open.remove(p.name);
                    } else {
                        app.sform.plugin_open.insert(p.name);
                    }
                }
            }
            if open && has_opts {
                ui.add_space(16.0);
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 31)));
                ui.add_space(16.0);
                options(app, ui, p);
            }
        });
    super::end(ui, 8.0);
}

/// Plugins whose settings are more than a list of options.
fn custom(name: &str) -> bool {
    matches!(name, "TextReplace")
}

fn options(app: &mut App, ui: &mut Ui, p: &'static Plugin) {
    for o in p.opts {
        let label = o.key;
        match &o.kind {
            Kind::Bool(_) => {
                let mut v = plugins::get(p.name, o.key).as_bool().unwrap_or(false);
                if super::toggle(ui, &mut v, o.desc, "") {
                    plugins::set(p.name, o.key, v.into());
                    app.heights.clear();
                }
            }
            Kind::Num(_) | Kind::Text(_) => {
                super::lead(ui);
                ui.label(RichText::new(o.desc).font(medium(14.0)).color(pal().header_primary));
                ui.add_space(8.0);
                let id = Id::new(("plugin_text", p.name, o.key));
                let mut s = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| plugins::text(p.name, o.key));
                let resp = input(ui, &mut s, label, false);
                if resp.changed() {
                    let v = match o.kind {
                        Kind::Num(_) => s.trim().parse::<f64>().ok().map(Value::from),
                        _ => Some(Value::from(s.clone())),
                    };
                    if let Some(v) = v {
                        plugins::set(p.name, o.key, v);
                        app.heights.clear();
                    }
                }
                ui.data_mut(|d| d.insert_temp(id, s));
                super::end(ui, 20.0);
            }
            Kind::Pick(choices) => {
                super::lead(ui);
                ui.label(RichText::new(o.desc).font(medium(14.0)).color(pal().header_primary));
                ui.add_space(8.0);
                let cur = plugins::text(p.name, o.key);
                let shown = choices.iter().find(|(v, _)| *v == cur).map_or(choices[0].1, |(_, l)| *l);
                dropdown(ui, ui.available_width(), None, shown, |ui| {
                    for (v, l) in choices.iter() {
                        if ui.selectable_label(*v == cur, *l).clicked() {
                            plugins::set(p.name, o.key, (*v).into());
                            app.heights.clear();
                        }
                    }
                });
                super::end(ui, 20.0);
            }
            Kind::Slider(lo, hi, _) => {
                super::lead(ui);
                let mut v = plugins::num(p.name, o.key) as f32;
                let r = ui.label(RichText::new(format!("{}: {}", o.desc, trim(v))).font(medium(14.0)).color(pal().header_primary)).rect;
                ui.add_space(12.0);
                if slider(ui, ui.available_width(), &mut v, *lo as f32, *hi as f32) {
                    let step = if hi - lo > 10.0 { 1.0 } else { 0.05 };
                    plugins::set(p.name, o.key, ((v / step).round() * step).into());
                }
                super::mark(ui, &format!("plugin_slider:{}:{}", p.name, o.key), r);
                super::end(ui, 20.0);
            }
        }
    }
    if p.name == "TextReplace" {
        text_rules(ui);
    }
}

fn trim(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// TextReplace's string rules: find, replace, and an optional word the message must contain.
/// The empty row at the bottom is how a rule gets added.
fn text_rules(ui: &mut Ui) {
    let mut rules: Vec<Value> = plugins::get("TextReplace", "stringRules").as_array().cloned().unwrap_or_default();
    rules.retain(|r| !r["find"].as_str().unwrap_or("").is_empty());
    rules.push(serde_json::json!({ "find": "", "replace": "", "onlyIfIncludes": "" }));
    super::sub(ui, "String Replacements");
    let mut changed = false;
    let mut drop = None;
    for (i, r) in rules.iter_mut().enumerate() {
        super::lead(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let w = (ui.available_width() - 48.0) / 3.0 - 8.0;
            for (k, hint) in [("find", "Find"), ("replace", "Replace"), ("onlyIfIncludes", "Only if includes")] {
                let mut s = r[k].as_str().unwrap_or("").to_string();
                ui.allocate_ui(vec2(w, 40.0), |ui| {
                    if input(ui, &mut s, hint, false).changed() {
                        r[k] = s.into();
                        changed = true;
                    }
                });
            }
            if !r["find"].as_str().unwrap_or("").is_empty() {
                let (x, resp) = ui.allocate_exact_size(vec2(32.0, 40.0), Sense::click());
                icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), if resp.hovered() { RED } else { pal().text_muted });
                if resp.clicked() {
                    drop = Some(i);
                }
            }
        });
        super::end(ui, 8.0);
    }
    if let Some(i) = drop {
        rules.remove(i);
        changed = true;
    }
    if changed {
        rules.retain(|r| !r["find"].as_str().unwrap_or("").is_empty());
        plugins::set("TextReplace", "stringRules", rules.into());
    }
}
