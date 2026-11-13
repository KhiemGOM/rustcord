//! Discover > Servers: the home banner and its featured servers, a tab per category and search,
//! every result a card that opens the server's preview. Pages come 24 at a time as you scroll.

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
const CARD_H: f32 = 322.0;
const GAP: f32 = 16.0;

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
    feeds: HashMap<String, Feed>,
}

pub struct Out {
    pub fetch: Option<(String, String)>,
    pub pick: Option<Id>,
}

impl App {
    pub fn open_servers(&mut self) {
        self.directory = Some(Dir { servers: true, ..Dir::default() });
        self.rest("GET", "/discovery/categories?locale=en-US&primary_only=false".into(), None, Tag::Directory("sv:cats".into()));
    }
}

pub fn loaded(d: &mut Dir, key: &str, v: Value) {
    if key == "sv:cats" {
        d.sv.cats = [1, 2, 3, 5, 6]
            .into_iter()
            .filter_map(|id| v.as_array()?.iter().find(|c| c["id"].as_u64() == Some(id)).map(|c| (id, c["name"].as_str().unwrap_or_default().to_string())))
            .collect();
        return;
    }
    let Some((feed, _)) = key.strip_prefix("sv:").and_then(|k| k.rsplit_once('@')) else { return };
    let f = d.sv.feeds.entry(feed.to_string()).or_default();
    f.loading = false;
    f.fetched = true;
    let seen: Vec<String> = f.guilds.iter().map(|g| g["id"].as_str().unwrap_or_default().to_string()).collect();
    let page: Vec<Value> = v["guilds"].as_array().into_iter().flatten().filter(|g| !seen.contains(&g["id"].as_str().unwrap_or_default().to_string())).cloned().collect();
    f.total = if page.is_empty() { f.guilds.len() } else { v["total_count"].as_u64().or(v["total"].as_u64()).unwrap_or(0) as usize };
    f.guilds.extend(page);
}


fn path(feed: &str, offset: usize) -> String {
    if feed == "home" {
        return "/discoverable-guilds?offset=0&limit=30".into();
    }
    let (cat, q) = if let Some(q) = feed.strip_prefix("q:") { (String::new(), q) } else { (feed.trim_start_matches("c:").to_string(), "") };
    format!("/discoverable-guilds/search?query={}&category_id={cat}&offset={offset}&limit={PAGE}&language_code=en-US&with_counts=true", super::directory::urlencode(q))
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

pub fn main(d: &mut Dir, ui: &mut Ui, app_rects: &mut crate::state::Map<String, Rect>) -> Out {
    let cat = d.tab.checked_sub(1).and_then(|i| d.sv.cats.get(i)).cloned();
    let searching = !d.searched.is_empty();
    let feed = if searching {
        format!("q:{}", d.searched)
    } else if let Some((id, _)) = &cat {
        format!("c:{id}")
    } else {
        "home".into()
    };
    let mut out = Out { fetch: None, pick: None };
    let f = d.sv.feeds.entry(feed.clone()).or_default();
    let mut ask = |f: &mut Feed| {
        if !f.loading && !f.fetched {
            f.loading = true;
            out.fetch = Some((format!("sv:{feed}@{}", f.guilds.len()), path(&feed, f.guilds.len())));
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
        if !searching {
            let title = cat.as_ref().map_or("Find your community on Discord".to_string(), |c| c.1.clone());
            let sub = match cat.as_ref().map(|c| c.0) {
                None => "From gaming, to music, to learning, there\u{2019}s a place for you.",
                Some(1) => "Get updates, connect with developers, and chat with other players.",
                Some(2) => "Get updates, connect with artists, and chat with other music fans.",
                Some(3) => "Connect with your favorite creators and their fans.",
                Some(5) => "Connect with developers, aficionados, and technology brands.",
                _ => "Connect with students, teachers, and learning centers.",
            };
            hero(ui, &title, sub);
        }
        let head = match (&cat, searching) {
            (_, true) => format!("Search results for \u{201c}{}\u{201d}", d.searched),
            (None, _) => "Featured Servers".into(),
            (Some(c), _) => format!("Popular {} Servers", c.1),
        };
        let (hr, _) = ui.allocate_exact_size(vec2(w, if searching { 72.0 } else { 48.0 }), Sense::hover());
        ui.painter().text(pos2(hr.left() + 44.0, hr.bottom() - 12.0), Align2::LEFT_BOTTOM, head, semi(20.0), pal().header_primary);
        ui.add_space(16.0);
        // discord's grid stops a scrollbar's width short on the right
        egui::Frame::new().inner_margin(Margin { left: 32, right: 40, top: 0, bottom: 0 }).show(ui, |ui| {
            if guilds.is_empty() {
                let msg = if first && loading { None } else { Some(if searching { "No servers found." } else { "Nothing here yet." }) };
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
        out.fetch = Some((format!("sv:{feed}@{}", f.guilds.len()), path(&feed, f.guilds.len())));
    }
    out
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
        ui.painter().text(pos2(r.left() + 44.0, y), Align2::LEFT_CENTER, line.trim_end(), display(44.0), pal().header_primary);
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
    for (n, label, dot) in [(g["approximate_presence_count"].as_u64(), "Online", pal().green), (g["approximate_member_count"].as_u64(), "Members", Color32::from_rgb(201, 202, 206))] {
        let Some(n) = n else { continue };
        ui.painter().circle_filled(pos2(x + 4.0, cy), 4.0, dot);
        let t = ui.painter().layout_no_wrap(format!("{} {label}", commas(n)), reg(12.0), pal().text_normal);
        let w = t.size().x;
        ui.painter().galley(pos2(x + 12.0, cy - t.size().y / 2.0), t, pal().text_normal);
        x += 12.0 + w + 16.0;
    }
    let edge = crate::ui::motion::mix(faint(51), faint(110), crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect_stroke(r, radius, Stroke::new(1.0, edge), StrokeKind::Inside);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
