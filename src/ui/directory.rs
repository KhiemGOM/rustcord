//! Discord's Discover page on its Apps tab: the featured collections, a tab per category, search,
//! and an app's card. Adding an app is discord's own authorise flow, so that one step goes to
//! the browser.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;
use std::collections::HashMap;

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::theme::*;

#[derive(Default)]
pub struct Dir {
    // 0 is Featured, then the categories in discord's order
    pub tab: usize,
    pub query: String,
    pub searched: String,
    pub data: HashMap<String, Value>,
    pub card: Option<Value>,
    pub servers: bool,
    pub sv: super::discover::Servers,
}

impl App {
    pub fn open_directory(&mut self) {
        let mut d = Dir::default();
        d.data.clear();
        self.directory = Some(d);
        self.fetch_dir("collections", "/application-directory-static/collections?surface=1&active_state=1&platform=4&locale=en-US".into());
        self.fetch_dir("categories", "/application-directory-static/categories?locale=en-US".into());
    }

    fn fetch_dir(&mut self, key: &str, path: String) {
        if let Some(d) = &mut self.directory
            && !d.data.contains_key(key)
        {
            d.data.insert(key.to_string(), Value::Null);
            self.rest("GET", path, None, Tag::Directory(key.to_string()));
        }
    }
}

pub fn loaded(app: &mut App, key: String, v: Value) {
    if let Some(d) = &mut app.directory {
        if key.starts_with("sv:") {
            super::discover::loaded(d, &key, v);
        } else {
            d.data.insert(key, v);
        }
    }
}

pub fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let side_w = if app.ui.side_w > 0.0 { app.ui.side_w } else { 302.0 };
    let servers = app.directory.as_ref().is_some_and(|d| d.servers);
    let mut goto: Option<Option<bool>> = None;
    egui::Panel::left("discover_side").exact_size(side_w).resizable(false).show_separator_line(false).frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        let r = ui.max_rect();
        ui.painter().text(pos2(r.left() + 20.0, r.top() + 29.0), Align2::LEFT_CENTER, "Discover", semi(20.0), pal().header_primary);
        ui.painter().hline(r.x_range(), r.top() + 57.5, Stroke::new(1.0, pal().border_subtle));
        ui.add_space(66.0);
        for (icon, name, page) in [(Icon::Apps, "Apps", Some(false)), (Icon::Discover, "Servers", Some(true)), (Icon::Quests, "Quests", None)] {
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
            let inner = row.shrink2(vec2(8.0, 2.0));
            let on = page == Some(servers);
            if on {
                ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
            }
            let col = if on || resp.hovered() { pal().header_primary } else { pal().channel_default };
            icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(inner.left() + 22.0, inner.center().y), vec2(24.0, 24.0)), col);
            ui.painter().text(pos2(inner.left() + 50.0, inner.center().y), Align2::LEFT_CENTER, name, medium(18.0), col);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !on {
                goto = Some(page);
            }
        }
    });
    match goto {
        Some(Some(true)) => return app.open_servers(),
        Some(Some(false)) => return app.open_directory(),
        Some(None) => return app.open_quests(),
        None => {}
    }

    let Some(d) = app.directory.as_mut() else { return };
    let cats: Vec<(u64, String)> = if d.servers { d.sv.cats.clone() } else { d.data.get("categories").and_then(|v| v.as_array()).into_iter().flatten().map(|c| (c["id"].as_u64().unwrap_or(0), c["name"].as_str().unwrap_or_default().to_string())).collect() };
    let mut fetch: Option<(String, String)> = None;
    egui::Panel::top("discover_tabs").exact_size(49.0).resizable(false).show_separator_line(false).frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 20, right: 8, top: 0, bottom: 1 })).show(ui, |ui| {
        let r = ui.max_rect();
        ui.painter().hline(r.left() - 20.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
        ui.horizontal_centered(|ui| {
            let (ir, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            icons::draw(ui.painter(), if d.servers { Icon::Discover } else { Icon::Apps }, ir, pal().header_primary);
            ui.add_space(20.0);
            let names: Vec<String> = std::iter::once(if d.servers { "Home" } else { "Featured" }.to_string()).chain(cats.iter().map(|c| c.1.clone())).collect();
            for (i, n) in names.iter().enumerate() {
                let g = ui.painter().layout_no_wrap(n.clone(), medium(16.0), Color32::WHITE);
                let (tr, resp) = ui.allocate_exact_size(vec2(g.size().x, 48.0), Sense::click());
                app.rects.insert(format!("discover:tab:{i}"), tr);
                let on = d.tab == i && d.searched.is_empty();
                let col = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                ui.painter().galley(pos2(tr.left(), tr.center().y - g.size().y / 2.0), g, col);
                if on {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(tr.left(), tr.bottom() - 2.0), tr.max), CornerRadius::same(1), pal().header_primary);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    d.tab = i;
                    d.searched.clear();
                    d.query.clear();
                }
                ui.add_space(28.0);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = Frame::new()
                    .fill(Color32::from_black_alpha(31))
                    .stroke(Stroke::new(1.0, faint(51)))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.set_width(256.0);
                        ui.horizontal(|ui| {
                            let (sr, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::Search, sr, pal().header_secondary);
                            ui.add_space(8.0);
                            ui.add(egui::TextEdit::singleline(&mut d.query).frame(Frame::NONE).font(reg(16.0)).hint_text(RichText::new("Search").color(pal().text_muted)).desired_width(ui.available_width()))
                        })
                        .inner
                    })
                    .inner;
                app.rects.insert("discover:search".into(), r.rect);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    d.searched = d.query.trim().to_string();
                }
            });
        });
    });
    if d.servers {
        let mut out = None;
        egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| out = Some(super::discover::main(d, ui, &mut app.rects)));
        if let Some(o) = out {
            if let Some((k, p)) = o.fetch {
                app.rest("GET", p, None, Tag::Directory(k));
            }
            if let Some(id) = o.pick {
                super::lurk::start(app, id, 0);

            }
        }
        return;
    }
    // what the main area shows: the featured collections, a category, or a search
    let key = if !d.searched.is_empty() {
        format!("search:{}", d.searched)
    } else if d.tab == 0 {
        "collections".to_string()
    } else {
        format!("category:{}", cats.get(d.tab - 1).map_or(0, |c| c.0))
    };
    if !d.data.contains_key(&key) {
        let path = if let Some(q) = key.strip_prefix("search:") {
            format!("/application-directory-static/search?query={}&page=1&page_size=24&locale=en-US", urlencode(q))
        } else {
            format!("/application-directory-static/search?query=&page=1&page_size=24&category_id={}&locale=en-US", key.trim_start_matches("category:"))
        };
        fetch = Some((key.clone(), path));
    }
    let cat_name = |ids: &Value| ids.as_array().into_iter().flatten().filter_map(|c| c.as_u64()).find_map(|c| cats.iter().find(|x| x.0 == c).map(|x| x.1.clone()));
    let mut pick: Option<Value> = None;
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("discover").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let data = d.data.get(&key);
            if key == "collections" {
                hero(ui);
            }
            Frame::new().inner_margin(Margin { left: 52, right: 52, top: 32, bottom: 48 }).show(ui, |ui| {
                match data {
                    None | Some(Value::Null) => {
                        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                    }
                    Some(v) if key == "collections" => {
                        let mut cols: Vec<&Value> = v.as_array().into_iter().flatten().collect();
                        cols.sort_by_key(|c| c["position"].as_i64().unwrap_or(0));
                        for c in cols {
                            let apps: Vec<(Value, bool)> = c["application_directory_collection_items"].as_array().into_iter().flatten().map(|i| (i["application"].clone(), c["title"].as_str().is_some_and(|t| t.starts_with("Promoted")))).collect();
                            ui.label(RichText::new(c["title"].as_str().unwrap_or_default()).font(semi(20.0)).color(pal().header_primary));
                            ui.add_space(24.0);
                            grid(ui, &apps, &cat_name, &mut pick);
                            ui.add_space(40.0);
                        }
                    }
                    Some(v) => {
                        let apps: Vec<(Value, bool)> = v["results"].as_array().into_iter().flatten().map(|r| (r["data"].clone(), false)).collect();
                        let title = if d.searched.is_empty() { cats.get(d.tab.saturating_sub(1)).map(|c| c.1.clone()).unwrap_or_default() } else { format!("Results for \u{201c}{}\u{201d}", d.searched) };
                        ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                        ui.add_space(24.0);
                        if apps.is_empty() {
                            ui.label(RichText::new("No apps found.").font(reg(16.0)).color(pal().header_secondary));
                        }
                        grid(ui, &apps, &cat_name, &mut pick);
                    }
                }
            });
        });
    });
    if let Some(p) = pick {
        d.card = Some(p);
    }
    let card = d.card.clone();
    if let Some((k, path)) = fetch {
        app.fetch_dir(&k, path);
    }
    if let Some(c) = card {
        let ctx = ui.ctx().clone();
        let mut close = false;
        let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
        let m = crate::ui::motion::Modal::new("app_card".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(&ctx, |ui| {
            ui.set_width(560.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let id = c["id"].as_str().unwrap_or_default();
            let (top, _) = ui.allocate_exact_size(vec2(560.0, 200.0), Sense::hover());
            if let Some(cov) = c["cover_image"].as_str() {
                egui::Image::from_uri(format!("{}/app-icons/{id}/{cov}.png?size=1024", crate::api::cdn()))
                    .corner_radius(CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 })
                    .show_loading_spinner(false)
                    .paint_at(ui, top);
            }
            Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                if let Some(ic) = c["icon"].as_str() {
                    let r = Rect::from_min_size(pos2(ui.max_rect().left(), ui.max_rect().top() - 56.0), vec2(80.0, 80.0));
                    ui.painter().rect_filled(r.expand(4.0), CornerRadius::same(20), pal().bg_secondary);
                    egui::Image::from_uri(format!("{}/app-icons/{id}/{ic}.png?size=160", crate::api::cdn())).corner_radius(CornerRadius::same(16)).paint_at(ui, r);
                    ui.add_space(32.0);
                }
                ui.label(RichText::new(c["name"].as_str().unwrap_or_default()).font(bold(24.0)).color(pal().header_primary));
                ui.add_space(8.0);
                if let Some(cn) = cat_name(&c["categories"]).or_else(|| cat_name(&c["category_ids"])) {
                    ui.label(RichText::new(cn).font(semi(12.0)).color(pal().header_secondary));
                    ui.add_space(8.0);
                }
                ui.label(RichText::new(c["description"].as_str().unwrap_or_default()).font(reg(16.0)).color(pal().text_normal));
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if super::settings::primary(ui, "Add App").clicked() {
                        let url = match c["custom_install_url"].as_str() {
                            Some(u) => u.to_string(),
                            None => {
                                let scopes = c["install_params"]["scopes"].as_array().map(|s| s.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("%20")).unwrap_or_else(|| "applications.commands".into());
                                let perms = c["install_params"]["permissions"].as_str().unwrap_or("0");
                                format!("https://discord.com/oauth2/authorize?client_id={id}&scope={scopes}&permissions={perms}")
                            }
                        };
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                    if super::settings::secondary(ui, "Close").clicked() {
                        close = true;
                    }
                });
            });
        });
        if close || m.should_close() {
            if let Some(d) = &mut app.directory {
                d.card = None;
            }
        }
    }
}

pub fn urlencode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The blue banner over the featured collections.
fn hero(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 350.0), Sense::hover());
    let mut m = egui::Mesh::default();
    let (a, b) = (Color32::from_rgb(0x2b, 0x2f, 0x8c), Color32::from_rgb(0x13, 0x16, 0x4d));
    m.colored_vertex(r.left_top(), a);
    m.colored_vertex(r.right_top(), b);
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), a.lerp_to_gamma(b, 0.6));
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    let p = ui.painter();
    p.text(pos2(r.left() + 52.0, r.top() + 160.0), Align2::LEFT_BOTTOM, "FIND THINGS TO", bold(52.0), Color32::WHITE);
    p.text(pos2(r.left() + 52.0, r.top() + 216.0), Align2::LEFT_BOTTOM, "DO TOGETHER", bold(52.0), Color32::WHITE);
    p.text(pos2(r.left() + 52.0, r.top() + 248.0), Align2::LEFT_CENTER, "Bring the fun to your hangouts with apps for games, music, creative tools, and more.", reg(18.0), Color32::from_white_alpha(200));
}

/// Four cards to a row: the cover, the icon over its corner, the name, its category, two lines
/// of description.
fn grid(ui: &mut Ui, apps: &[(Value, bool)], cat_name: &dyn Fn(&Value) -> Option<String>, pick: &mut Option<Value>) {
    let cols = ((ui.available_width() + 24.0) / 300.0).floor().clamp(2.0, 4.0) as usize;
    let w = (ui.available_width() - 24.0 * (cols as f32 - 1.0)) / cols as f32;
    let cover_h = (w * 9.0 / 16.0).round();
    for chunk in apps.chunks(cols) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), cover_h + 160.0), Sense::hover());
        for (i, (a, promoted)) in chunk.iter().enumerate() {
            let r = Rect::from_min_size(pos2(row.left() + i as f32 * (w + 24.0), row.top()), vec2(w, cover_h + 160.0));
            let resp = ui.interact(r, ui.id().with(("app", a["id"].as_str().unwrap_or_default())), Sense::click());
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(pal().bg_secondary, pal().bg_floating, crate::ui::motion::hot(ui.ctx(), &resp)));
            p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
            let id = a["id"].as_str().unwrap_or_default();
            let cover = Rect::from_min_size(r.min, vec2(w, cover_h));
            if let Some(c) = a["cover_image"].as_str() {
                egui::Image::from_uri(format!("{}/app-icons/{id}/{c}.png?size=512", crate::api::cdn()))
                    .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
                    .show_loading_spinner(false)
                    .paint_at(ui, cover);
            }
            let icon = Rect::from_min_size(pos2(r.left() + 20.0, cover.bottom() - 28.0), vec2(56.0, 56.0));
            if let Some(ic) = a["icon"].as_str() {
                ui.painter().rect_filled(icon.expand(3.0), CornerRadius::same(15), pal().bg_secondary);
                egui::Image::from_uri(format!("{}/app-icons/{id}/{ic}.png?size=128", crate::api::cdn())).corner_radius(CornerRadius::same(12)).show_loading_spinner(false).paint_at(ui, icon);
            }
            let p = ui.painter();
            let x = r.left() + 20.0;
            super::text::paint(p, pos2(x, icon.bottom() + 20.0), Align2::LEFT_CENTER, a["name"].as_str().unwrap_or_default(), semi(18.0), pal().header_primary, w - 40.0);
            let mut line = cat_name(&a["categories"]).or_else(|| cat_name(&a["category_ids"])).unwrap_or_default();
            if *promoted {
                line = if line.is_empty() { "Promoted".into() } else { format!("{line} \u{2022} Promoted") };
            }
            p.text(pos2(x, icon.bottom() + 46.0), Align2::LEFT_CENTER, line, semi(12.0), pal().header_secondary);
            let mut job = egui::text::LayoutJob::simple(a["description"].as_str().unwrap_or_default().replace('\n', " "), reg(14.0), pal().text_normal, w - 40.0);
            job.wrap.max_rows = 2;
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter().galley(pos2(x, icon.bottom() + 62.0), g, pal().text_normal);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                *pick = Some(a.clone());
            }
        }
        ui.add_space(24.0);
    }
}
