//! A discoverable server's listing details: its category and the tags people search it by, saved
//! as discord's discovery metadata.

use egui::{Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    let Some(meta) = app.server_data("discovery_meta", format!("/guilds/{gid}/discovery-metadata")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let cats: Vec<(u64, String)> = app
        .server_data("discovery_categories", "/discovery/categories?locale=en-US&primary_only=false".into())
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter(|c| c["is_primary"].as_bool() != Some(false)).filter_map(|c| Some((c["id"].as_u64()?, c["name"].as_str().or_else(|| c["name"]["default"].as_str())?.to_string()))).collect())
        .unwrap_or_default();
    let Some(s) = &mut app.server else { return };
    let d = s.draft.entry("discovery_meta").or_insert_with(|| json!({ "primary_category_id": meta["primary_category_id"], "keywords": meta["keywords"] }));
    let mut primary = d["primary_category_id"].as_u64();
    let mut keywords: Vec<String> = d["keywords"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(str::to_string)).collect();
    super::super::settings::h5(ui, "Primary Category");
    form::small(ui, "Categories help Discord users discover communities that interest them. If you\u{2019}ve already set one that you\u{2019}re happy with, there\u{2019}s no need to change it.");
    ui.add_space(8.0);
    let shown = cats.iter().find(|c| Some(c.0) == primary).map_or("Select a category", |c| c.1.as_str());
    super::super::settings::dropdown(ui, 360.0, None, shown, |ui| {
        for (id, name) in &cats {
            if crate::ui::menu_item(ui, None, name, false) {
                primary = Some(*id);
            }
        }
    });
    ui.add_space(24.0);
    super::super::settings::h5(ui, "Tags");
    form::small(ui, "In Server Discovery, you can search by name, description, and tags. You can add up to 10.");
    ui.add_space(8.0);
    let mut add = String::new();
    let r = super::super::settings::input(ui, &mut add, "Add a tag", false);
    super::super::settings::mark(ui, "field:discovery_tag", r.rect);
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let t = add.trim();
        if !t.is_empty() && t.chars().count() <= 32 && keywords.len() < 10 && !keywords.iter().any(|k| k.eq_ignore_ascii_case(t)) {
            keywords.push(t.to_string());
        }
    }
    ui.add_space(8.0);
    let mut drop = None;
    ui.horizontal_wrapped(|ui| {
        for (i, k) in keywords.iter().enumerate() {
            let g = ui.painter().layout_no_wrap(k.clone(), medium(14.0), pal().header_primary);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 40.0, 28.0), Sense::click());
            ui.painter().rect_filled(r, 14, pal().bg_mod_selected);
            ui.painter().galley(pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(12.0, 12.0)), pal().header_secondary);
            if resp.on_hover_text("Remove tag").clicked() {
                drop = Some(i);
            }
        }
    });
    if let Some(i) = drop {
        keywords.remove(i);
    }
    d["primary_category_id"] = json!(primary);
    d["keywords"] = json!(keywords);
    ui.add_space(16.0);
    let saved: Vec<String> = meta["keywords"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(str::to_string)).collect();
    let dirty = primary != meta["primary_category_id"].as_u64() || saved != keywords;
    let body = d.clone();
    if ui.add_enabled_ui(dirty, |ui| form::button(ui, "Save Changes", true)).inner.clicked() {
        s.draft.remove("discovery_meta");
        app.rest("PATCH", format!("/guilds/{gid}/discovery-metadata"), Some(body), Tag::Server("discovery_meta"));
    }
    ui.add_space(24.0);
}
