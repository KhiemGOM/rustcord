//! Discover > Servers: the home banner and its featured servers, a tab per category, and the
//! search page with the categories that matched. Every result is a card that opens the server's
//! preview; pages come as you scroll.

use egui::{Align2, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::Value;
use std::collections::HashMap;

use super::directory::{Dir, faint};
use super::icons::{self, Icon};
use super::{App, text};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

const PAGE: usize = 24;
const SEARCH_PAGE: usize = 12;
const CARD_H: f32 = 322.0;
const RESULT_H: f32 = 320.0;
const GAP: f32 = 16.0;
const SEP: char = '\u{1f}';

#[derive(Default)]
pub struct Feed {
    guilds: Vec<Value>,
    total: usize,
    loading: bool,
    fetched: bool,
}

#[derive(Default)]
pub struct Servers {
    pub cats: Vec<(u64, String)>,
    names: HashMap<u64, String>,
    feeds: HashMap<String, Feed>,
    /// A search's category (None is All), its language, and how many servers matched in each
    /// category of the query and language named by `counted`.
    pub cat: Option<u64>,
    pub lang: Option<&'static str>,
    counts: Vec<(u64, u64)>,
    counted: String,
}

pub struct Out {
    pub fetch: Option<(String, String)>,
    pub pick: Option<Id>,
    pub join: bool,
}

impl App {
    pub fn open_servers(&mut self) {
        self.directory = Some(Dir { servers: true, ..Dir::default() });
        self.rest("GET", "/discovery/categories?locale=en-US&primary_only=false".into(), None, Tag::Directory("sv:cats".into()));
    }
}

pub fn loaded(d: &mut Dir, key: &str, v: Value) {
    if key == "sv:cats" {
        d.sv.names = v.as_array().into_iter().flatten().filter_map(|c| Some((c["id"].as_u64()?, c["name"].as_str()?.to_string()))).collect();
        d.sv.cats = [1, 2, 3, 5, 6].into_iter().filter_map(|id| d.sv.names.get(&id).map(|n| (id, n.clone()))).collect();
        return;
    }
    let Some((feed, _)) = key.strip_prefix("sv:").and_then(|k| k.rsplit_once('@')) else { return };
    if feed.starts_with("q:")
        && let Some(counts) = v["categories"].as_array().filter(|c| !c.is_empty())
    {
        d.sv.counts = counts.iter().take(8).filter_map(|c| Some((c["id"].as_u64().or_else(|| c["id"].as_str()?.parse().ok())?, c["count"].as_u64()?))).collect();
    }
    let f = d.sv.feeds.entry(feed.to_string()).or_default();
    f.loading = false;
    f.fetched = true;
    let seen: Vec<String> = f.guilds.iter().map(|g| g["id"].as_str().unwrap_or_default().to_string()).collect();
    let page: Vec<Value> = v["guilds"].as_array().into_iter().flatten().filter(|g| !seen.contains(&g["id"].as_str().unwrap_or_default().to_string())).cloned().collect();
    f.total = if page.is_empty() { f.guilds.len() } else { v["total_count"].as_u64().or(v["total"].as_u64()).unwrap_or(0) as usize };
    f.guilds.extend(page);
}


/// The feed a search fills: its words, its category (none for All) and its language.
pub fn search_key(q: &str, cat: Option<u64>, lang: Option<&str>) -> String {
    format!("q:{q}{SEP}{}{SEP}{}", cat.map_or(String::new(), |c| c.to_string()), lang.unwrap_or("en-US"))
}

fn path(feed: &str, offset: usize, counts: bool) -> String {
    if feed == "home" {
        return "/discoverable-guilds?offset=0&limit=30".into();
    }
    if let Some(rest) = feed.strip_prefix("q:") {
        let mut part = rest.split(SEP);
        let (q, cat, lang) = (part.next().unwrap_or(""), part.next().unwrap_or(""), part.next().unwrap_or("en-US"));
        let cat = if cat.is_empty() { String::new() } else { format!("&category_id={cat}") };
        return format!("/discoverable-guilds/search?query={}{cat}&offset={offset}&limit={SEARCH_PAGE}&language_code={lang}&with_counts={counts}", super::directory::urlencode(q));
    }
    let cat = feed.trim_start_matches("c:");
    format!("/discoverable-guilds/search?query=&category_id={cat}&offset={offset}&limit={PAGE}&language_code=en-US&with_counts=true")
}

/// Discord's column count for search results, by the page's width and whether the categories
/// sit beside the grid.
pub fn result_columns(width: f32, beside: bool) -> usize {
    let mut left = width - if beside { 450.0 } else { 0.0 } - 280.0;
    let mut n = 1;
    while left > 0.0 {
        left -= 264.0;
        n += 1;
    }
    n
}

fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn main(d: &mut Dir, ui: &mut Ui, app_rects: &mut crate::state::Map<String, Rect>, hubs: &[Value]) -> Out {
    if !d.searched.is_empty() {
        return results(d, ui, app_rects);
    }
    let cat = d.tab.checked_sub(1).and_then(|i| d.sv.cats.get(i)).cloned();
    if !d.sv.cats.is_empty() && d.tab == d.sv.cats.len() + 1 {
        return student_hubs(ui, app_rects, hubs);
    }
    let feed = match &cat {
        Some((id, _)) => format!("c:{id}"),
        None => "home".into(),
    };
    let mut out = Out { fetch: None, pick: None, join: false };
    let f = d.sv.feeds.entry(feed.clone()).or_default();
    let mut ask = |f: &mut Feed| {
        if !f.loading && !f.fetched {
            f.loading = true;
            out.fetch = Some((format!("sv:{feed}@{}", f.guilds.len()), path(&feed, f.guilds.len(), false)));
        }
    };
    ask(f);
    let first = f.guilds.is_empty();
    let loading = f.loading;
    let guilds = f.guilds.clone();
    let total = f.total;
    let scroll = egui::ScrollArea::vertical().id_salt("discover_servers").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let w = ui.available_width();
        let title = cat.as_ref().map_or("Find your community on Discord".to_string(), |c| c.1.clone());
        let sub = match cat.as_ref().map(|c| c.0) {
            None => "From gaming, to music, to learning, there's a place for you.",
            Some(1) => "Get updates, connect with developers, and chat with other players.",
            Some(2) => "Get updates, connect with artists, and chat with other music fans.",
            Some(3) => "Connect with your favorite creators and their fans.",
            Some(5) => "Connect with developers, aficionados, and technology brands.",
            _ => "Connect with students, teachers, and learning centers.",
        };
        hero(ui, &title, sub);
        let head = match &cat {
            None => "Featured Servers".into(),
            Some(c) => format!("Popular {} Servers", c.1),
        };
        let (hr, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
        ui.painter().text(pos2(hr.left() + 44.0, hr.bottom() - 12.0), Align2::LEFT_BOTTOM, head, semi(20.0), pal().header_primary);
        ui.add_space(16.0);
        // discord's grid stops a scrollbar's width short on the right
        egui::Frame::new().inner_margin(Margin { left: 32, right: 40, top: 0, bottom: 0 }).show(ui, |ui| {
            if guilds.is_empty() {
                let msg = if first && loading { None } else { Some("Nothing here yet.") };
                match msg {
                    None => {
                        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                    }
                    Some(m) => {
                        ui.label(RichText::new(m).font(reg(16.0)).color(pal().header_secondary));
                    }
                }
                return;
            }
            let cols = (((ui.available_width() + GAP) / (280.0 + GAP)).floor() as usize).clamp(1, 6);
            let cw = (ui.available_width() - GAP * (cols as f32 - 1.0)) / cols as f32;
            for row in guilds.chunks(cols) {
                let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), CARD_H), Sense::hover());
                if ui.is_rect_visible(rr) {
                    for (i, g) in row.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(rr.left() + i as f32 * (cw + GAP), rr.top()), vec2(cw, CARD_H));
                        let id: Id = g["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                        app_rects.insert(format!("discover:card:{id}"), r);
                        if card(ui, r, g) {
                            out.pick = Some(id);
                        }
                    }
                }
                ui.add_space(GAP);
            }
            if let Some(r) = app_rects.get(&format!("discover:card:{}", guilds[0]["id"].as_str().unwrap_or_default())).copied() {
                app_rects.insert(format!("discover:feed:{feed}"), r);
            }
            if loading || guilds.len() < total {
                ui.add_space(8.0);
                crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
            }
            ui.add_space(48.0);
        });
    });
    let (offset, view, content) = (scroll.state.offset.y, scroll.inner_rect.height(), scroll.content_size.y);
    if offset + view > content - 700.0
        && feed != "home"
        && let Some(f) = d.sv.feeds.get_mut(&feed)
        && f.guilds.len() < f.total
        && !f.loading
    {
        f.loading = true;
        out.fetch = Some((format!("sv:{feed}@{}", f.guilds.len()), path(&feed, f.guilds.len(), false)));
    }
    out
}

/// The rows of a search's category list: All, each category discord counted, and the one being
/// shown when it has no count.
fn category_rows(sv: &Servers, all: Option<u64>, here: usize) -> Vec<(Option<u64>, String, Option<u64>)> {
    let mut rows = vec![(None, "All".to_string(), all)];
    rows.extend(sv.counts.iter().filter_map(|(id, n)| Some((Some(*id), sv.names.get(id)?.clone(), Some(*n)))));
    if let Some(c) = sv.cat
        && !rows.iter().any(|r| r.0 == Some(c))
        && let Some(name) = sv.names.get(&c)
    {
        rows.push((Some(c), name.clone(), Some(here as u64)));
    }
    rows
}

fn language(ui: &mut Ui, w: f32, now: Option<&'static str>, pick: &mut Option<&'static str>) {
    let named = |code: &str, native: &'static str| if code == "en-US" { "English" } else { native };
    let locales = crate::ui::settings::system::LOCALES.iter().filter(|l| l.0 != "en-GB");
    let shown = locales.clone().find(|l| l.0 == now.unwrap_or("en-US")).map_or("English", |l| named(l.0, l.1));
    crate::ui::settings::dropdown(ui, w, None, shown, |ui| {
        for l in locales {
            if ui.selectable_label(l.0 == now.unwrap_or("en-US"), named(l.0, l.1)).clicked() {
                *pick = Some(l.0);
            }
        }
    });
}

fn category_row(ui: &mut Ui, name: &str, count: Option<u64>, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    if on {
        ui.painter().rect_filled(r, 4, pal().bg_mod_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 4, pal().bg_mod_hover);
    }
    let mut right = r.right() - 8.0;
    if let Some(n) = count {
        let c = ui.painter().text(pos2(right, r.center().y), Align2::RIGHT_CENTER, commas(n), reg(14.0), pal().text_muted);
        right = c.left() - 8.0;
    }
    text::paint(ui.painter(), pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, name, medium(14.0), pal().header_primary, right - r.left() - 8.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A search's results: discord's grid, with the categories that matched and the language beside
/// it, or over it when the page is under 1024 wide.
fn results(d: &mut Dir, ui: &mut Ui, app_rects: &mut crate::state::Map<String, Rect>) -> Out {
    let mut out = Out { fetch: None, pick: None, join: false };
    let (cat, lang) = (d.sv.cat, d.sv.lang);
    let feed = search_key(&d.searched, cat, lang);
    let counted = format!("{}{SEP}{}", d.searched, lang.unwrap_or("en-US"));
    let f = d.sv.feeds.entry(feed.clone()).or_default();
    if !f.loading && !f.fetched {
        f.loading = true;
        let counts = d.sv.counted != counted;
        out.fetch = Some((format!("sv:{feed}@0"), path(&feed, 0, counts)));
        if counts {
            d.sv.counted = counted;
            d.sv.counts.clear();
        }
    }
    let f = &d.sv.feeds[&feed];
    let (guilds, total, loading) = (f.guilds.clone(), f.total, f.loading);
    let all = d.sv.feeds.get(&search_key(&d.searched, None, lang)).filter(|f| f.fetched).map(|f| f.total as u64);
    let rows = category_rows(&d.sv, all, total);
    let page = ui.max_rect();
    let beside = page.width() >= 1024.0;
    let (mut to_cat, mut to_lang) = (None, None);
    let scroll = egui::ScrollArea::vertical().id_salt("discover_results").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if beside {
            ui.add_space(16.0);
        } else {
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.set_height(44.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(32.0);
                ui.label(RichText::new("Show").font(medium(14.0)).color(pal().header_secondary));
                let shown = rows.iter().find(|r| r.0 == cat).map_or("All", |r| r.1.as_str());
                crate::ui::settings::dropdown(ui, 200.0, None, shown, |ui| {
                    for (id, name, _) in &rows {
                        let row = ui.selectable_label(*id == cat, name.as_str());
                        crate::ui::settings::mark(ui, &format!("option:{name}"), row.rect);
                        if row.clicked() {
                            to_cat = Some(*id);
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(32.0);
                    language(ui, 180.0, lang, &mut to_lang);
                });
            });
        }
        let left = ui.max_rect().left() + 32.0;
        let width = ui.available_width() - 32.0 - if beside { 202.0 } else { 32.0 };
        if guilds.is_empty() {
            if loading {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                let mut at = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(left + width / 2.0, r.center().y), vec2(24.0, 24.0))));
                crate::ui::motion::spinner(&mut at, 24.0, pal().text_muted);
                return;
            }
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 448.0), Sense::hover());
            let mid = pos2(left + width / 2.0, r.center().y);
            let title = match rows.iter().find(|r| r.0 == cat && cat.is_some()) {
                Some(r) => format!("No results found in {}", r.1),
                None => "No results found".to_string(),
            };
            ui.painter().text(mid - vec2(0.0, 16.0), Align2::CENTER_CENTER, title, semi(24.0), pal().header_primary);
            if cat.is_none() {
                ui.painter().text(mid + vec2(0.0, 16.0), Align2::CENTER_CENTER, "Try searching for something else.", reg(16.0), pal().text_normal);
                return;
            }
            let lead = ui.painter().layout_no_wrap("Try searching for something else or ".into(), reg(16.0), pal().text_normal);
            let link = ui.painter().layout_no_wrap("see results from all categories".into(), reg(16.0), pal().link);
            let x = mid.x - (lead.size().x + link.size().x) / 2.0;
            let y = mid.y + 16.0 - lead.size().y / 2.0;
            let hit = Rect::from_min_size(pos2(x + lead.size().x, y), link.size());
            let end = hit.right_top();
            ui.painter().galley(pos2(x, y), lead, pal().text_normal);
            ui.painter().galley(hit.min, link, pal().link);
            ui.painter().text(end, Align2::LEFT_TOP, ".", reg(16.0), pal().text_normal);
            let resp = ui.interact(hit, ui.id().with("all_categories"), Sense::click());
            app_rects.insert("discover:all_categories".into(), hit);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                to_cat = Some(None);
            }
            return;
        }
        let cols = result_columns(page.width(), beside);
        let cw = (width - GAP * (cols as f32 - 1.0)) / cols as f32;
        for row in guilds.chunks(cols) {
            let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), RESULT_H), Sense::hover());
            if ui.is_rect_visible(rr) {
                for (i, g) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(left + i as f32 * (cw + GAP), rr.top()), vec2(cw, RESULT_H));
                    let id: Id = g["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                    app_rects.insert(format!("discover:card:{id}"), r);
                    if card(ui, r, g) {
                        out.pick = Some(id);
                    }
                }
            }
            ui.add_space(GAP);
        }
        if let Some(r) = app_rects.get(&format!("discover:card:{}", guilds[0]["id"].as_str().unwrap_or_default())).copied() {
            app_rects.insert(format!("discover:feed:q:{}", d.searched), r);
        }
        if loading || guilds.len() < total {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
            let mut at = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(left + width / 2.0, r.center().y), vec2(24.0, 24.0))));
            crate::ui::motion::spinner(&mut at, 24.0, pal().text_muted);
        }
    });
    if beside {
        let side = Rect::from_min_max(pos2(page.right() - 170.0, page.top()), pos2(page.right() - 8.0, page.bottom()));
        let mut s = ui.new_child(egui::UiBuilder::new().max_rect(side).layout(egui::Layout::top_down(egui::Align::Min)));
        s.spacing_mut().item_spacing = vec2(0.0, 0.0);
        s.add_space(5.0);
        language(&mut s, side.width(), lang, &mut to_lang);
        s.add_space(23.0);
        for (id, name, count) in &rows {
            let resp = category_row(&mut s, name, *count, *id == cat);
            app_rects.insert(format!("discover:cat:{name}"), resp.rect);
            if resp.clicked() {
                to_cat = Some(*id);
            }
            s.add_space(2.0);
        }
    }
    if let Some(c) = to_cat {
        d.sv.cat = c;
    }
    if let Some(l) = to_lang {
        d.sv.lang = Some(l);
    }
    let (offset, view, content) = (scroll.state.offset.y, scroll.inner_rect.height(), scroll.content_size.y);
    if offset + view > content - 240.0
        && let Some(f) = d.sv.feeds.get_mut(&feed)
        && f.fetched
        && f.guilds.len() < f.total
        && !f.loading
    {
        f.loading = true;
        out.fetch = Some((format!("sv:{feed}@{}", f.guilds.len()), path(&feed, f.guilds.len(), false)));
    }
    out
}

/// The Student Hubs tab: the hubs you're in and the card that joins one.
fn student_hubs(ui: &mut Ui, app_rects: &mut crate::state::Map<String, Rect>, hubs: &[Value]) -> Out {
    let mut out = Out { fetch: None, pick: None, join: false };
    egui::ScrollArea::vertical().id_salt("discover_hubs").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let w = ui.available_width();
        hero(ui, "Join the Student Hub for your school", "Meet classmates from your school, discover communities, and share your servers.");
        if hubs.is_empty() {
            ui.add_space(32.0);
        } else {
            let (hr, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
            ui.painter().text(pos2(hr.left() + 44.0, hr.bottom() - 12.0), Align2::LEFT_BOTTOM, "Your Hubs", semi(20.0), pal().header_primary);
            ui.add_space(16.0);
        }
        egui::Frame::new().inner_margin(Margin { left: 32, right: 40, top: 0, bottom: 0 }).show(ui, |ui| {
            let cols = (((ui.available_width() + GAP) / (280.0 + GAP)).floor() as usize).clamp(1, 6);
            let cw = (ui.available_width() - GAP * (cols as f32 - 1.0)) / cols as f32;
            let cells: Vec<Option<&Value>> = hubs.iter().map(Some).chain([None]).collect();
            for row in cells.chunks(cols) {
                let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), CARD_H), Sense::hover());
                for (i, g) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(rr.left() + i as f32 * (cw + GAP), rr.top()), vec2(cw, CARD_H));
                    match g {
                        Some(g) => {
                            if card(ui, r, g) {
                                out.pick = g["id"].as_str().and_then(|s| s.parse().ok());
                            }
                        }
                        None => out.join = join_card(ui, r, !hubs.is_empty(), app_rects),
                    }
                }
                ui.add_space(GAP);
            }
            ui.add_space(16.0);
            let y = ui.cursor().top();
            ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, pal().border_subtle));
            ui.add_space(24.0);
            for line in ["Hubs are not affiliated with or managed by schools.", "Servers listed in the school\u{2019}s Student Hub are run by students. Users do not need to be a member of a Student Hub to be invited to join a server listed there."] {
                ui.add(egui::Label::new(RichText::new(line).font(medium(14.0)).color(pal().text_normal)).wrap());
            }
            ui.add_space(48.0);
        });
    });
    out
}

fn join_card(ui: &mut Ui, r: Rect, has_hub: bool, app_rects: &mut crate::state::Map<String, Rect>) -> bool {
    ui.painter().rect(r, CornerRadius::same(8), pal().bg_primary, Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
    let inner = r.shrink(16.0);
    let dot = Rect::from_min_size(inner.min, vec2(48.0, 48.0));
    ui.painter().circle_filled(dot.center(), 24.0, pal().green);
    icons::draw(ui.painter(), Icon::People, Rect::from_center_size(dot.center(), vec2(24.0, 24.0)), Color32::WHITE);
    let (title, sub, button) = if has_hub {
        ("Attend multiple schools?", "Join the Student Hub for any school you\u{2019}re attending.", "Join Another Hub")
    } else {
        ("Find study groups, clubs, and friends in the Student Hub for your school!", "Meet classmates from your school, discover communities, and share your servers, all in one place.", "Join Hub")
    };
    let head = ui.painter().layout(title.into(), semi(18.0), pal().header_primary, inner.width());
    let body = ui.painter().layout(sub.into(), reg(14.0), pal().text_normal, inner.width());
    let y = dot.bottom() + 16.0;
    let h = head.size().y;
    ui.painter().galley(pos2(inner.left(), y), head, pal().header_primary);
    ui.painter().galley(pos2(inner.left(), y + h + 16.0), body, pal().text_normal);
    let slot = Rect::from_min_max(pos2(inner.left(), inner.bottom() - 40.0), inner.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(slot));
    let go = super::modal_button(&mut child, button, (!has_hub).then_some(pal().blurple), slot.width());
    app_rects.insert("discover:hub_join".into(), go.rect);
    go.clicked()
}

fn hero_paint(ui: &Ui, r: Rect) {
    let mut m = egui::Mesh::default();
    let (a, b) = (Color32::from_rgb(0x2b, 0x2f, 0x8c), Color32::from_rgb(0x08, 0x0a, 0x24));
    m.colored_vertex(r.left_top(), a);
    m.colored_vertex(r.right_top(), a.lerp_to_gamma(b, 0.2));
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), b);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
}

fn hero(ui: &mut Ui, title: &str, sub: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
    hero_paint(ui, r);
    let title = title.to_uppercase();
    let g = ui.painter().layout(title, display(44.0), pal().header_primary, (r.width() - 88.0).min(700.0));
    let mut y = r.top() + 115.0;
    for row in &g.rows {
        let line: String = row.row.glyphs.iter().map(|c| c.chr).collect();
        ui.painter().text(pos2(r.left() + 44.0, y + 5.3), Align2::LEFT_CENTER, line.trim_end(), display(44.0), pal().header_primary);
        y += 44.0;
    }
    ui.painter().text(pos2(r.left() + 44.0, y - 22.0 + 25.5), Align2::LEFT_CENTER, sub, medium(16.0), pal().text_muted);
}

fn card(ui: &mut Ui, r: Rect, g: &Value) -> bool {
    let id = g["id"].as_str().unwrap_or_default();
    let resp = ui.interact(r, ui.id().with(("server_card", id)), Sense::click());
    let radius = CornerRadius::same(8);
    ui.painter().rect_filled(r, radius, pal().bg_primary);
    let banner = Rect::from_min_size(r.min + vec2(1.0, 1.0), vec2(r.width() - 2.0, 146.0));
    let cdn = crate::api::cdn();
    let art = [("discovery_splash", "discovery-splashes"), ("banner", "banners"), ("splash", "splashes")].into_iter().find_map(|(k, dir)| g[k].as_str().map(|h| format!("{cdn}/{dir}/{id}/{h}.png?size=480")));
    ui.painter().rect_filled(banner, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, pal().bg_tertiary);
    if let Some(u) = art {
        egui::Image::from_uri(u).corner_radius(CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }).show_loading_spinner(false).paint_at(ui, banner);
    }
    let icon = Rect::from_min_size(r.min + vec2(16.0, 118.0), vec2(48.0, 48.0));
    ui.painter().rect_filled(icon.expand(4.0), CornerRadius::same(18), pal().bg_primary);
    match g["icon"].as_str() {
        Some(h) => egui::Image::from_uri(format!("{cdn}/icons/{id}/{h}.png?size=96")).corner_radius(CornerRadius::same(14)).show_loading_spinner(false).paint_at(ui, icon),
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(14), pal().bg_accent);
            let name = g["name"].as_str().unwrap_or_default();
            let ac: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect();
            ui.painter().text(icon.center(), Align2::CENTER_CENTER, ac, semi(16.0), pal().header_primary);
        }
    }
    let feats: Vec<&str> = g["features"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect();
    let badge = if feats.contains(&"VERIFIED") {
        Some((Icon::Verified, pal().green))
    } else if feats.contains(&"PARTNERED") {
        Some((Icon::BadgePartner, pal().header_primary))
    } else {
        None
    };
    let mut x = r.left() + 16.0;
    let y = r.top() + 185.0;
    if let Some((i, c)) = badge {
        icons::draw(ui.painter(), i, Rect::from_center_size(pos2(x + 8.0, y), vec2(16.0, 16.0)), c);
        x += 18.0;
    }
    text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, g["name"].as_str().unwrap_or_default(), semi(16.0), pal().header_primary, r.right() - 16.0 - x);
    let mut job = egui::text::LayoutJob::simple(g["description"].as_str().unwrap_or_default().replace('\n', " "), reg(14.0), pal().text_normal, r.width() - 32.0);
    job.wrap.max_rows = 3;
    let gal = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(pos2(r.left() + 16.0, r.top() + 199.0), gal, pal().text_normal);
    let mut x = r.left() + 16.0;
    let cy = r.top() + 297.0;
    let counts: Vec<(String, f32, Color32)> = [(g["approximate_presence_count"].as_u64(), "Online", pal().green), (g["approximate_member_count"].as_u64(), "Members", Color32::from_rgb(201, 202, 206))]
        .into_iter()
        .filter_map(|(n, label, dot)| {
            let text = format!("{} {label}", commas(n?));
            let w = ui.painter().layout_no_wrap(text.clone(), reg(12.0), pal().text_normal).size().x;
            Some((text, w, dot))
        })
        .collect();
    let (wanted, room) = (counts.iter().map(|c| 12.0 + c.1).sum::<f32>(), r.width() - 32.0 - 16.0 * counts.len().saturating_sub(1) as f32);
    let squeeze = (room / wanted).min(1.0);
    for (label, w, dot) in counts {
        let w = (12.0 + w) * squeeze - 12.0;
        ui.painter().circle_filled(pos2(x + 4.0, cy), 4.0, dot);
        text::paint(ui.painter(), pos2(x + 12.0, cy), Align2::LEFT_CENTER, &label, reg(12.0), pal().text_normal, w);
        x += 12.0 + w + 16.0;
    }
    let edge = crate::ui::motion::mix(faint(51), faint(110), crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect_stroke(r, radius, Stroke::new(1.0, edge), StrokeKind::Inside);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_asks_for_its_category_and_language() {
        assert_eq!(path(&search_key("rust", Some(1), None), 0, true), "/discoverable-guilds/search?query=rust&category_id=1&offset=0&limit=12&language_code=en-US&with_counts=true");
        assert_eq!(path(&search_key("a b", None, Some("fr")), 12, false), "/discoverable-guilds/search?query=a%20b&offset=12&limit=12&language_code=fr&with_counts=false");
        assert_eq!(path("c:5", 24, false), "/discoverable-guilds/search?query=&category_id=5&offset=24&limit=24&language_code=en-US&with_counts=true");
    }

    #[test]
    fn result_columns_follow_discords_widths() {
        assert_eq!(result_columns(904.0, false), 4);
        assert_eq!(result_columns(1024.0, true), 3);
        assert_eq!(result_columns(1600.0, true), 5);
    }

    #[test]
    fn the_category_list_is_all_then_what_discord_counted() {
        let mut sv = Servers { counts: vec![(1, 4), (9, 2)], cat: Some(2), ..Servers::default() };
        sv.names = [(1, "Gaming".to_string()), (2, "Music".to_string())].into();
        let rows = category_rows(&sv, Some(7), 3);
        assert_eq!(rows, vec![(None, "All".to_string(), Some(7)), (Some(1), "Gaming".to_string(), Some(4)), (Some(2), "Music".to_string(), Some(3))]);
    }
}
