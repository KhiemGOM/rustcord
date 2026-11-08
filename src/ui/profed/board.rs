//! The right-hand side of the editor: Board, Activity and Wishlist tabs, and the widgets you can
//! put on your profile.

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::Value;

use super::draft::{Game, Slot, Widget};
use super::{Dlg, Editor, Tab, faint, round_icon};
use crate::state::sid;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{mark, remote};
use crate::ui::{App, motion, text};

pub struct Edit {
    pub slot: Slot,
    pub games: Vec<Game>,
    pub q: String,
}

pub const CELL_MIN: f32 = 150.0;
const SUGGESTED: &str = "/users/@me/widgets/suggested-games";

fn game_art(app: &App, id: &str) -> Option<String> {
    let a = id.parse::<u64>().ok()?;
    let hash = app.store.app_icons.get(&a)?.as_ref()?;
    Some(format!("{}/app-icons/{a}/{hash}.png?size=128", crate::api::cdn()))
}

fn cover(ui: &Ui, app: &App, r: Rect, id: Option<&str>) {
    let fill = faint(21);
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    if let Some(u) = id.and_then(|i| game_art(app, i)) {
        text::image(&ui.painter().with_clip_rect(r), &u, r);
    }
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, faint(31)), egui::StrokeKind::Inside);
}

fn want_icons(app: &mut App, ids: &[String]) {
    let want: Vec<u64> = ids.iter().filter_map(|i| i.parse().ok()).filter(|a| !app.store.app_icons.contains_key(a)).collect();
    if want.is_empty() {
        return;
    }
    for a in &want {
        app.store.app_icons.insert(*a, None);
    }
    let q: Vec<String> = want.iter().map(|a| format!("application_ids={a}")).collect();
    app.rest("GET", format!("/applications/public?{}", q.join("&")), None, crate::net::Tag::AppIcons(want));
}

pub fn tabs(ui: &mut Ui, ed: &mut Editor, x: f32, y: f32, w: f32) {
    let mut tx = x;
    for (t, label) in [(Tab::Board, "Board"), (Tab::Activity, "Activity"), (Tab::Wishlist, "Wishlist")] {
        let on = ed.tab == t;
        let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), if on { pal().header_primary } else { pal().header_secondary });
        let r = Rect::from_min_size(pos2(tx, y), vec2(g.size().x, 32.0));
        let resp = ui.interact(r, ui.id().with(("profed_tab", label)), Sense::click());
        let ink = if on || resp.hovered() { Color32::WHITE } else { Color32::WHITE.gamma_multiply(0.76) };
        ui.painter().text(pos2(tx, y + 14.0), Align2::LEFT_CENTER, label, medium(14.0), ink);
        if on {
            ui.painter().rect_filled(Rect::from_min_size(pos2(tx, y + 30.0), vec2(r.width(), 2.0)), 1, Color32::WHITE);
        }
        mark(ui, &format!("profed_tab:{label}"), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            ed.tab = t;
        }
        tx += r.width() + 24.0;
    }
    ui.painter().hline(x..=x + w, y + 31.5, Stroke::new(1.0, faint(31)));
}

/// Draws the right-hand column in `rect` from its top; hands back how tall it came out.
pub fn show(app: &mut App, ed: &mut Editor, ui: &mut Ui, rect: Rect) -> f32 {
    tabs(ui, ed, rect.left(), rect.top(), rect.width());
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + 48.0), rect.right_bottom());
    match ed.tab {
        Tab::Board => board(app, ed, ui, body) + 48.0,
        Tab::Activity => activity(app, ui, body) + 48.0,
        Tab::Wishlist => {
            super::suggest::settle(app);
            let w = wish(app, app.store.me);
            let (h, go) = paint_wishlist(ui, body, &w, Color32::WHITE);
            ed.to_shop |= go;
            let wished = w.skus.clone();
            let more = super::suggest::show(app, ui, body, body.top() + h + 24.0, &wished);
            h + 48.0 + if more > 0.0 { more + 24.0 } else { 0.0 }
        }
    }
}

fn activity(app: &mut App, ui: &mut Ui, r: Rect) -> f32 {
    let me = app.store.me;
    let live: Vec<crate::state::Activity> = app.store.activities.get(&me).map(|l| crate::ui::presence::shown(l).cloned().collect()).unwrap_or_default();
    let mut y = r.top();
    ui.painter().text(pos2(r.left(), y + 8.0), Align2::LEFT_CENTER, "Current activity", medium(12.0), Color32::WHITE.gamma_multiply(0.88));
    y += 24.0;
    if live.is_empty() {
        ui.painter().text(pos2(r.left(), y + 10.0), Align2::LEFT_CENTER, "Nothing to show right now.", reg(14.0), Color32::WHITE.gamma_multiply(0.76));
        y += 24.0;
    }
    let mut c = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), 400.0))));
    for a in &live {
        crate::ui::presence::card(&mut c, a, r.width(), &None, false, "", false);
    }
    y += c.min_rect().height();
    y - r.top()
}

pub struct Wish {
    pub loading: bool,
    pub rows: Vec<(String, String)>,
    pub skus: Vec<String>,
}

/// What's on someone's wishlist: its id is in their profile, and each product's picture comes with it.
pub fn wish(app: &mut App, uid: crate::state::Id) -> Wish {
    let Some(list) = crate::ui::shop::wish::id_of(app, uid) else { return Wish { loading: true, rows: Vec::new(), skus: Vec::new() } };
    let Some(list) = list else { return Wish { loading: false, rows: Vec::new(), skus: Vec::new() } };
    let key = format!("wishlist:{list}");
    if !app.keyed.contains_key(&key) {
        app.keyed.insert(key.clone(), None);
        app.rest("GET", format!("/wishlists/{list}?source=user_profile"), None, crate::net::Tag::Keyed(key.clone()));
    }
    let got = app.keyed.get(&key).cloned().flatten();
    let skus: Vec<String> = match &got {
        Some(Ok(v)) => crate::ui::shop::wish::skus(v),
        _ => Vec::new(),
    };
    let rows = skus.iter().filter_map(|s| app.product(s)).map(|p| (p.name.clone(), p.still())).collect();
    Wish { loading: got.is_none(), rows, skus }
}

/// A wishlist in `r` from its top, as tall as it comes out, and whether Explore the Shop was clicked.
pub fn paint_wishlist(ui: &mut Ui, r: Rect, w: &Wish, ink: Color32) -> (f32, bool) {
    if w.loading {
        motion::spinner(&mut ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.center().x - 12.0, r.top() + 24.0), vec2(24.0, 24.0)))), 24.0, ink);
        return (80.0, false);
    }
    if w.rows.is_empty() {
        let c = r.center().x;
        text::paint(ui.painter(), pos2(c, r.top() + 72.0), Align2::CENTER_CENTER, "No hearts yet \u{1F494}", semi(16.0), ink, r.width());
        let g = ui.painter().layout("Browse the Shop and heart the items you love to add them here.".into(), reg(14.0), ink, (r.width() - 32.0).min(380.0));
        let gh = g.size().y;
        ui.painter().galley(pos2(c - g.size().x / 2.0, r.top() + 96.0), g.clone(), ink);
        let label = "Explore the Shop";
        let lw = ui.painter().layout_no_wrap(label.into(), medium(14.0), Color32::WHITE).size().x;
        let b = Rect::from_center_size(pos2(c, r.top() + 96.0 + gh + 36.0), vec2(lw + 56.0, 36.0));
        let resp = ui.interact(b, ui.id().with("wish_explore"), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(8), if resp.hovered() { crate::theme::pal().green.gamma_multiply(0.85) } else { crate::theme::pal().green });
        icons::draw(ui.painter(), Icon::Shop, Rect::from_center_size(pos2(b.left() + 22.0, b.center().y), vec2(16.0, 16.0)), Color32::WHITE);
        ui.painter().text(pos2(b.left() + 38.0, b.center().y), Align2::LEFT_CENTER, label, medium(14.0), Color32::WHITE);
        mark(ui, "wish:explore", b);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        return (96.0 + gh + 72.0, resp.clicked());
    }
    let cw = ((r.width() - 12.0) / 2.0).max(120.0);
    for (i, (name, still)) in w.rows.iter().enumerate() {
        let c = Rect::from_min_size(pos2(r.left() + (i % 2) as f32 * (cw + 12.0), r.top() + (i / 2) as f32 * 112.0), vec2(cw, 100.0));
        ui.painter().rect_filled(c, CornerRadius::same(8), faint(31));
        text::image(&ui.painter().with_clip_rect(c), still, Rect::from_min_size(c.min + vec2(8.0, 8.0), vec2(84.0, 84.0)));
        text::paint(ui.painter(), pos2(c.left() + 100.0, c.top() + 24.0), Align2::LEFT_CENTER, name, medium(14.0), ink, cw - 108.0);
        mark(ui, &format!("wish:{i}"), c);
    }
    (w.rows.len().div_ceil(2) as f32 * 112.0, false)
}

/// Explore the Shop: out of the profile and into the Shop.
pub fn explore(app: &mut App) {
    app.profed = None;
    app.full_profile = None;
    app.open_shop(crate::ui::shop::Page::Shop);
}

pub fn game_ids(ws: &[Widget]) -> Vec<String> {
    ws.iter().flat_map(|w| w.games.iter().map(|g| g.id.clone())).collect()
}

pub fn want_game_icons(app: &mut App, ws: &[Widget]) {
    want_icons(app, &game_ids(ws));
}

/// A profile's widgets as cards: each with its title and the games' covers and names.
pub fn paint_widgets(ui: &mut Ui, store: &crate::state::Store, r: Rect, ws: &[Widget], ink: Color32) -> f32 {
    let mut y = r.top();
    for w in ws {
        ui.painter().text(pos2(r.left(), y + 8.0), Align2::LEFT_CENTER, w.slot.title(), medium(12.0), ink.gamma_multiply(0.88));
        y += 24.0;
        let per = (((r.width() + 8.0) / 64.0) as usize).max(1);
        for (i, g) in w.games.iter().enumerate() {
            let cr = Rect::from_min_size(pos2(r.left() + (i % per) as f32 * 64.0, y + (i / per) as f32 * 110.0), vec2(56.0, 75.0));
            ui.painter().rect_filled(cr, CornerRadius::same(8), faint(21));
            let url = g.id.parse::<u64>().ok().and_then(|a| store.app_icons.get(&a).cloned().flatten().map(|h| format!("{}/app-icons/{a}/{h}.png?size=128", crate::api::cdn())));
            if let Some(u) = url {
                text::image(&ui.painter().with_clip_rect(cr), &u, cr);
            }
            ui.painter().rect_stroke(cr, CornerRadius::same(8), Stroke::new(1.0, faint(31)), egui::StrokeKind::Inside);
            text::paint(ui.painter(), pos2(cr.left(), cr.bottom() + 10.0), Align2::LEFT_CENTER, &g.name, reg(12.0), ink, 56.0);
            mark(ui, &format!("profile_game:{}", g.id), cr);
        }
        y += w.games.len().div_ceil(per) as f32 * 110.0 + 12.0;
    }
    y - r.top()
}

fn widget_of<'a>(list: &'a [Widget], s: Slot) -> Option<&'a Widget> {
    list.iter().find(|w| w.slot == s)
}

fn board(app: &mut App, ed: &mut Editor, ui: &mut Ui, r: Rect) -> f32 {
    let mut y = r.top() + 24.0;
    ui.painter().text(pos2(r.center().x, y + 10.0), Align2::CENTER_CENTER, "Customize your profile with Widgets", medium(16.0), Color32::WHITE);
    ui.painter().text(pos2(r.center().x, y + 38.0), Align2::CENTER_CENTER, "Choose from our library of Widgets to share more\nabout yourself and your interests", reg(14.0), Color32::WHITE.gamma_multiply(0.96));
    y += 76.0;
    let featured = remote(app, "featured", "/widget-configs/featured").and_then(|r| r.as_ref().ok()).cloned().unwrap_or(Value::Null);
    let ids: Vec<String> = ed.draft.widgets.iter().flat_map(|w| w.games.iter().map(|g| g.id.clone())).collect();
    want_icons(app, &ids);
    let order = [Some(Slot::Favorite), Some(Slot::Played), None, Some(Slot::Current), Some(Slot::Want), None];
    let apps: Vec<Value> = featured.as_array().cloned().unwrap_or_default();
    let me = app.store.me;
    let idents = remote(app, "idents", &format!("/users/{me}/application-identities?with_profiles=true")).and_then(|r| r.as_ref().ok()).cloned().unwrap_or(Value::Null);
    let linked_ids: Vec<String> = idents.as_array().into_iter().flatten().filter_map(|i| i["application_id"].as_str().map(str::to_string)).collect();
    let mut linked = apps.iter();
    let cw = ((r.width() - 16.0) / 2.0).max(CELL_MIN);
    let cols = if r.width() >= 2.0 * CELL_MIN + 16.0 { 2 } else { 1 };
    let cw = if cols == 2 { cw } else { r.width() };
    let ch = 99.0;
    for (i, s) in order.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(r.left() + (i % cols) as f32 * (cw + 16.0), y + (i / cols) as f32 * (ch + 16.0)), vec2(cw, ch));
        match s {
            Some(slot) => slot_tile(app, ed, ui, cell, *slot),
            None => match linked.next() {
                Some(a) => app_tile(app, ui, cell, a, linked_ids.iter().any(|l| Some(l.as_str()) == a["application_id"].as_str())),
                None => {}
            },
        }
    }
    let rows = order.len().div_ceil(cols);
    y += rows as f32 * (ch + 16.0) + 8.0;
    let b = Rect::from_center_size(pos2(r.center().x, y + 16.0), vec2(140.0, 32.0));
    let resp = ui.interact(b, ui.id().with("browse_widgets"), Sense::click());
    ui.painter().rect_filled(b, CornerRadius::same(8), Color32::WHITE.gamma_multiply(if resp.hovered() { 0.14 } else { 0.08 }));
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Browse All Widgets", medium(14.0), Color32::WHITE);
    mark(ui, "browse_widgets", b);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ed.dlg = Dlg::Browse;
    }
    y + 48.0 - r.top()
}

fn tile_frame(ui: &mut Ui, cell: Rect, id: &str) -> egui::Response {
    let resp = ui.interact(cell, ui.id().with(("widget_tile", id)), Sense::click());
    ui.painter().rect_filled(cell, CornerRadius::same(8), faint(if resp.hovered() { 51 } else { 31 }));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn plus(ui: &Ui, c: egui::Pos2, set: bool) {
    ui.painter().circle_filled(c, 12.0, Color32::from_black_alpha(150));
    icons::draw(ui.painter(), if set { Icon::Pencil } else { Icon::Plus }, Rect::from_center_size(c, vec2(14.0, 14.0)), Color32::WHITE);
}

fn slot_tile(app: &mut App, ed: &mut Editor, ui: &mut Ui, cell: Rect, slot: Slot) {
    let resp = tile_frame(ui, cell, slot.title());
    let w = widget_of(&ed.draft.widgets, slot);
    let n = match slot {
        Slot::Favorite | Slot::Current => 1,
        _ => 3,
    };
    let (cwid, chgt) = (56.0, 75.0);
    let pad = 12.0;
    let total = n as f32 * cwid + (n as f32 - 1.0) * 8.0;
    let left = if n == 1 { cell.left() + pad } else { cell.center().x - total / 2.0 };
    for k in 0..n {
        let cr = Rect::from_min_size(pos2(left + k as f32 * (cwid + 8.0), cell.top() + pad), vec2(cwid, chgt));
        cover(ui, app, cr, w.and_then(|w| w.games.get(k)).map(|g| g.id.as_str()));
    }
    if n == 1 {
        match w.and_then(|w| w.games.first()) {
            Some(g) => {
                text::paint(ui.painter(), pos2(left + cwid + 12.0, cell.top() + 36.0), Align2::LEFT_CENTER, &g.name, medium(14.0), Color32::WHITE, cell.width() - cwid - 36.0);
            }
            None => {
                for (k, bw) in [72.0f32, 114.0].into_iter().enumerate() {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(left + cwid + 8.0, cell.top() + 34.0 + k as f32 * 20.0), vec2(bw.min(cell.width() - cwid - 36.0), 12.0)), CornerRadius::same(6), faint(31));
                }
            }
        }
    }
    let label_y = cell.top() + 84.0 - 14.0;
    let label = if w.is_some() { format!("{} ({})", slot.title(), w.map_or(0, |w| w.games.len())) } else { slot.title().to_string() };
    ui.painter().text(pos2(cell.center().x, label_y + 18.0), Align2::CENTER_CENTER, label, medium(16.0), Color32::WHITE);
    plus(ui, pos2(cell.center().x, cell.top() + 40.0), w.is_some());
    mark(ui, &format!("widget:{}", slot.wire()), cell);
    if resp.clicked() {
        let games = w.map(|w| w.games.clone()).unwrap_or_default();
        ed.dlg = Dlg::Widget(Edit { slot, games, q: String::new() });
    }
}

fn app_tile(app: &mut App, ui: &mut Ui, cell: Rect, a: &Value, linked: bool) {
    let name = a["name"].as_str().or(a["application"]["name"].as_str()).unwrap_or("Game");
    let id = a["application_id"].as_str().or(a["id"].as_str()).unwrap_or("");
    let resp = tile_frame(ui, cell, id);
    for (k, bw) in [45.0f32, 90.0].into_iter().enumerate() {
        ui.painter().rect_filled(Rect::from_min_size(pos2(cell.left() + 16.0, cell.top() + 16.0 + k as f32 * 20.0), vec2(bw, if k == 0 { 12.0 } else { 1.0 })), CornerRadius::same(6), faint(31));
    }
    for k in 0..6 {
        ui.painter().rect_filled(Rect::from_min_size(pos2(cell.left() + 16.0 + (k % 3) as f32 * 33.0, cell.top() + 45.0 + (k / 3) as f32 * 23.0), vec2(25.0, 15.0)), CornerRadius::same(4), faint(31));
    }
    if let Some(u) = id.parse::<u64>().ok().and_then(|_| game_art(app, id)) {
        text::image(&ui.painter().with_clip_rect(cell), &u, Rect::from_min_size(pos2(cell.right() - 81.0, cell.top()), vec2(81.0, 81.0)));
    }
    icons::draw(ui.painter(), Icon::Link, Rect::from_center_size(pos2(cell.center().x, cell.top() + 34.0), vec2(24.0, 24.0)), Color32::WHITE);
    ui.painter().text(pos2(cell.center().x, cell.top() + 66.0), Align2::CENTER_CENTER, name, medium(16.0), Color32::WHITE);
    mark(ui, &format!("widget_app:{id}"), cell);
    if linked {
        if resp.on_hover_text(format!("Add {name} Widget")).clicked() {
            app.toast(format!("{name} is linked. Its widget shows up on your board once it has stats."));
        }
    } else if resp.on_hover_text(format!("Link your {name} account to add widget")).clicked() {
        app.settings_page = Some(crate::ui::settings::Page::Apps);
        app.profed = None;
    }
}

fn frame() -> Frame {
    Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)))
}

pub fn dialog(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    let sugg = remote(app, "suggested", SUGGESTED).and_then(|r| r.as_ref().ok()).cloned().unwrap_or(Value::Null);
    let Dlg::Widget(e) = &mut ed.dlg else { return };
    let rows: Vec<(String, String)> = sugg.as_array().into_iter().flatten().map(|g| (g["game_id"].as_str().or(g["application_id"].as_str()).or(g["id"].as_str()).unwrap_or("").to_string(), g["name"].as_str().unwrap_or("").to_string())).collect();
    let ids: Vec<String> = rows.iter().map(|r| r.0.clone()).chain(e.games.iter().map(|g| g.id.clone())).collect();
    want_icons(app, &ids);
    let (mut save, mut close) = (false, false);
    let max = e.slot.max();
    let screen = ctx.content_rect();
    let w = 520.0f32.min(screen.width() - 48.0);
    let m = motion::Modal::new("profed_widget".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame().inner_margin(24)).show(ctx, |ui| {
        ui.set_width(w - 48.0);
        let area = ui.max_rect();
        ui.painter().text(pos2(area.left(), area.top() + 12.0), Align2::LEFT_CENTER, e.slot.title(), semi(20.0), pal().header_primary);
        ui.painter().text(pos2(area.left(), area.top() + 40.0), Align2::LEFT_CENTER, format!("{} of {max}", e.games.len()), reg(14.0), pal().header_secondary);
        let cb = Rect::from_min_size(pos2(area.right() - 32.0, area.top()), vec2(32.0, 32.0));
        close |= round_icon(ui, cb, Icon::Close, "Close", false).clicked();
        ui.add_space(60.0);
        let r = ui.add(egui::TextEdit::singleline(&mut e.q).hint_text("Search games").desired_width(f32::INFINITY).font(reg(16.0)));
        mark(ui, "widget:search", r.rect);
        ui.add_space(8.0);
        let q = e.q.to_lowercase();
        egui::ScrollArea::vertical().max_height((screen.height() - 360.0).clamp(120.0, 340.0)).show(ui, |ui| {
            let mut all: Vec<(String, String)> = e.games.iter().map(|g| (g.id.clone(), g.name.clone())).collect();
            for r in &rows {
                if !all.iter().any(|a| a.0 == r.0) {
                    all.push(r.clone());
                }
            }
            for (id, name) in all.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)) {
                let on = e.games.iter().any(|g| &g.id == id);
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                if resp.hovered() || on {
                    ui.painter().rect_filled(r, CornerRadius::same(8), faint(if on { 41 } else { 21 }));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                cover(ui, app, Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(30.0, 40.0)), Some(id));
                text::paint(ui.painter(), pos2(r.left() + 50.0, r.center().y), Align2::LEFT_CENTER, name, medium(16.0), pal().header_primary, r.width() - 110.0);
                icons::draw(ui.painter(), if on { Icon::Check } else { Icon::Plus }, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(18.0, 18.0)), pal().header_primary);
                mark(ui, &format!("widget_game:{id}"), r);
                if resp.clicked() {
                    if on {
                        e.games.retain(|g| &g.id != id);
                    } else if e.games.len() < max {
                        e.games.push(Game { id: id.clone(), name: name.clone(), comment: None, tags: Vec::new() });
                    } else if max == 1 {
                        e.games = vec![Game { id: id.clone(), name: name.clone(), comment: None, tags: Vec::new() }];
                    }
                }
            }
            if all.is_empty() {
                ui.label(egui::RichText::new("No games to suggest yet. Play something and it will show up here.").font(reg(14.0)).color(pal().header_secondary));
            }
        });
        ui.add_space(16.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = crate::ui::button(ui, "Save", pal().blurple, 100.0);
            mark(ui, "widget:save", b.rect);
            save = b.clicked();
            close |= crate::ui::grey_button(ui, "Cancel", 100.0).clicked();
        });
    });
    if save {
        let (slot, games) = (e.slot, e.games.clone());
        let list = &mut ed.draft.widgets;
        match list.iter().position(|w| w.slot == slot) {
            Some(i) if games.is_empty() => {
                list.remove(i);
            }
            Some(i) => list[i].games = games,
            None if !games.is_empty() => list.push(Widget { id: None, slot, games }),
            None => {}
        }
        ed.dlg = Dlg::None;
    } else if close || m.should_close() {
        ed.dlg = Dlg::None;
    }
}

pub fn browse(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    let dev = remote(app, "dev_widgets", "/widget-configs/developer").and_then(|r| r.as_ref().ok()).cloned().unwrap_or(Value::Null);
    let featured = remote(app, "featured", "/widget-configs/featured").and_then(|r| r.as_ref().ok()).cloned().unwrap_or(Value::Null);
    let (mut close, mut pick) = (false, None);
    let screen = ctx.content_rect();
    let w = 560.0f32.min(screen.width() - 48.0);
    let m = motion::Modal::new("profed_browse".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame().inner_margin(24)).show(ctx, |ui| {
        ui.set_width(w - 48.0);
        let area = ui.max_rect();
        ui.painter().text(pos2(area.left(), area.top() + 12.0), Align2::LEFT_CENTER, "Browse All Widgets", semi(20.0), pal().header_primary);
        let cb = Rect::from_min_size(pos2(area.right() - 32.0, area.top()), vec2(32.0, 32.0));
        close |= round_icon(ui, cb, Icon::Close, "Close", false).clicked();
        ui.add_space(48.0);
        egui::ScrollArea::vertical().max_height((screen.height() - 220.0).clamp(160.0, 420.0)).show(ui, |ui| {
            ui.label(egui::RichText::new("Games").font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            for s in Slot::ALL {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(8), faint(31));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, s.title(), medium(16.0), pal().header_primary);
                mark(ui, &format!("browse:{}", s.wire()), r);
                if resp.clicked() {
                    pick = Some(s);
                }
            }
            for (title, list) in [("Linked accounts", &featured), ("From developers", &dev)] {
                let rows: Vec<&Value> = list.as_array().into_iter().flatten().collect();
                if rows.is_empty() {
                    continue;
                }
                ui.add_space(12.0);
                ui.label(egui::RichText::new(title).font(medium(14.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                for a in rows {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                    ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, a["name"].as_str().unwrap_or("Widget"), reg(16.0), pal().header_primary);
                }
            }
        });
    });
    if let Some(s) = pick {
        let games = widget_of(&ed.draft.widgets, s).map(|w| w.games.clone()).unwrap_or_default();
        ed.dlg = Dlg::Widget(Edit { slot: s, games, q: String::new() });
    } else if close || m.should_close() {
        ed.dlg = Dlg::None;
    }
    let _ = sid;
}
