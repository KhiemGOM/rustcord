//! The Shop's Browse tab the way Discord's filter store drives it: item types, colours, themes,
//! Orbs and collabs become a `/shop/search` query, the results page under it, and the filter bar
//! sits beside the grid on wide windows or drops from the Hide Filters button on narrow ones.

use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use super::{RADIUS_SM, SPACE_8, SPACE_16, SPACE_24, text_width};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::mark;
use crate::ui::{App, Trail, directory::urlencode, menu_row};

pub const CATALOG_MAX: f32 = 1124.0;
pub const CATALOG_PAD: f32 = 20.0;
pub const BAR: f32 = 240.0;
pub const BAR_PAD: f32 = SPACE_8;
const SMALL_SCREEN: f32 = 1400.0;
const CONTROL: f32 = 40.0;
const SORT_WIDTH: f32 = 190.0;

pub const TYPES: [(&str, &str); 5] = [
    ("Avatar Decorations", "AVATAR_DECORATION"),
    ("Nameplates", "NAMEPLATE"),
    ("Profile Effects", "PROFILE_EFFECT"),
    ("Profile Frames", "PROFILE_FRAME"),
    ("Bundles", "BUNDLE"),
];

const COLORS: [(&str, &str, u32); 10] = [
    ("Purple", "PURPLE", 0x9b59b6),
    ("Blue", "BLUE", 0x3498db),
    ("Green", "GREEN", 0x2ecc71),
    ("Brown", "BROWN", 0xa0522d),
    ("Yellow", "YELLOW", 0xf1c40f),
    ("Orange", "ORANGE", 0xe67e22),
    ("Red", "RED", 0xe74c3c),
    ("Pink", "PINK", 0xec407a),
    ("White", "WHITE", 0xffffff),
    ("Black", "BLACK", 0x262626),
];

macro_rules! theme {
    ($label:literal, $name:literal, $file:literal) => {
        ($label, $name, concat!("bytes://icon/theme_", $file, ".svg"), include_bytes!(concat!("../../../assets/icons/theme_", $file, ".svg")).as_slice())
    };
}

const THEMES: [(&str, &str, &str, &[u8]); 10] = [
    theme!("Anime", "ANIME", "anime"),
    theme!("Gaming", "GAMING", "gaming"),
    theme!("Cute & Cozy", "CUTE_COZY", "cute"),
    theme!("Sci-Fi", "SCI_FI", "scifi"),
    theme!("Food & Drinks", "FOOD_DRINKS", "food"),
    theme!("Fantasy", "FANTASY", "fantasy"),
    theme!("Animals & Pets", "ANIMALS_PETS", "animals"),
    theme!("Nature", "NATURE", "nature"),
    theme!("Movies & TV", "MOVIES_TV_SHOWS", "movies"),
    theme!("Dark & Moody", "DARK_MOODY", "dark"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Recent,
    PriceAsc,
    PriceDesc,
    Popular,
    Relevance,
}

impl Sort {
    const ALL: [Sort; 5] = [Sort::Recent, Sort::PriceAsc, Sort::PriceDesc, Sort::Popular, Sort::Relevance];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Recent => "Recently Added",
            Sort::PriceAsc => "Price: Low to High",
            Sort::PriceDesc => "Price: High to Low",
            Sort::Popular => "Popular",
            Sort::Relevance => "Relevance",
        }
    }

    fn params(self) -> (&'static str, &'static str) {
        match self {
            Sort::Recent => ("recency", "desc"),
            Sort::PriceAsc => ("price", "asc"),
            Sort::PriceDesc => ("price", "desc"),
            Sort::Popular => ("popularity", "desc"),
            Sort::Relevance => ("relevance", "desc"),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Filters {
    pub types: [bool; 5],
    pub colors: [bool; 10],
    pub themes: [bool; 10],
    pub orbs: bool,
    pub collabs: bool,
    pub picked: Option<Sort>,
    pub open: bool,
    pub page: usize,
}

impl Filters {
    pub fn new() -> Filters {
        Filters { types: [false; 5], colors: [false; 10], themes: [false; 10], orbs: false, collabs: false, picked: None, open: true, page: 0 }
    }

    pub fn any(&self) -> bool {
        self.types.iter().chain(&self.colors).chain(&self.themes).any(|b| *b) || self.orbs || self.collabs
    }

    fn relevance(&self, query: &str) -> bool {
        self.colors.iter().chain(&self.themes).any(|b| *b) || !query.trim().is_empty()
    }

    pub fn sort(&self, query: &str) -> Sort {
        self.picked.unwrap_or(if self.relevance(query) {
            Sort::Relevance
        } else if self.types.iter().any(|b| *b) {
            Sort::Popular
        } else {
            Sort::Recent
        })
    }

    pub fn plain(&self, query: &str) -> bool {
        !self.any() && query.trim().is_empty() && self.sort(query) == Sort::Recent
    }

    pub fn dirty(&self, query: &str) -> bool {
        self.any() || !query.trim().is_empty()
    }

    pub fn only(&mut self, i: usize) {
        *self = Filters { open: self.open, ..Filters::new() };
        self.types[i] = true;
    }

    pub fn only_collabs(&mut self) {
        *self = Filters { open: self.open, ..Filters::new() };
        self.collabs = true;
    }

    pub fn reset(&mut self) {
        *self = Filters { open: self.open, ..Filters::new() };
    }

    pub fn path(&self, query: &str, limit: usize) -> String {
        let mut q = String::new();
        let mut add = |k: &str, v: &str| {
            q.push_str(if q.is_empty() { "?" } else { "&" });
            q.push_str(k);
            q.push('=');
            q.push_str(v);
        };
        for (i, on) in self.types.iter().enumerate() {
            if *on {
                add("item_types", TYPES[i].1);
            }
        }
        for (i, on) in self.colors.iter().enumerate() {
            if *on {
                add("colors", &format!("COLLECTIBLES_COLOR_{}", COLORS[i].1));
            }
        }
        for (i, on) in self.themes.iter().enumerate() {
            if *on {
                add("themes", &format!("COLLECTIBLES_THEME_{}", THEMES[i].1));
            }
        }
        if self.orbs {
            add("orbs_eligible", "true");
        }
        if self.collabs {
            add("is_first_party", "false");
        }
        add("offset", &(self.page * limit).to_string());
        add("limit", &limit.to_string());
        let (kind, dir) = self.sort(query).params();
        add("sort_type", kind);
        add("sort_direction", dir);
        if !query.trim().is_empty() {
            add("search", &urlencode(query.trim()));
        }
        format!("/shop/search{q}")
    }
}

pub struct Hits {
    pub skus: Vec<String>,
    pub total: usize,
}

pub fn hits(app: &mut App, path: &str) -> Option<Result<Hits, String>> {
    if !app.keyed.contains_key(path) {
        app.keyed.insert(path.to_string(), None);
        app.rest("GET", path.to_string(), None, Tag::Keyed(path.to_string()));
    }
    match app.keyed.get(path)?.as_ref()? {
        Ok(v) => Some(Ok(Hits {
            skus: v["skus"].as_array().into_iter().flatten().filter_map(|s| s.as_str().or(s["id"].as_str()).map(str::to_string)).collect(),
            total: v["pagination"]["total"].as_u64().unwrap_or(0) as usize,
        })),
        Err(e) => Some(Err(e.clone())),
    }
}

pub struct Plan {
    pub wide: bool,
    pub col_w: f32,
    pub gw: f32,
    pub cols: usize,
}

pub fn plan(ctx: &egui::Context, w: f32, filters_open: bool) -> Plan {
    let wide = ctx.content_rect().width() >= SMALL_SCREEN;
    let col_w = if wide && filters_open { w - BAR - 2.0 * BAR_PAD } else { w };
    let gw = (col_w.min(CATALOG_MAX) - 2.0 * CATALOG_PAD).max(super::CARD_MIN);
    Plan { wide, col_w, gw, cols: super::columns(gw, super::CARD_MIN, SPACE_24) }
}

pub fn heading(f: &Filters, query: &str, total: Option<usize>) -> String {
    if !f.dirty(query) {
        return String::new();
    }
    let Some(n) = total else { return "Loading...".into() };
    let q = query.trim();
    if !q.is_empty() {
        let shown = if q.chars().count() > 40 { format!("{}...", q.chars().take(40).collect::<String>()) } else { q.to_string() };
        return format!("Search Results for \"{shown}\" ({n})");
    }
    let one = f.types.iter().filter(|b| **b).count() == 1 && !f.orbs && !f.collabs;
    if one && let Some(i) = f.types.iter().position(|b| *b) {
        return format!("{} ({n})", TYPES[i].0);
    }
    if f.collabs && !f.types.iter().any(|b| *b) && !f.orbs {
        return format!("Collabs ({n})");
    }
    format!("Results ({n})")
}

fn pointer(ui: &Ui, hot: bool) {
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn secondary(hover: bool) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, if hover { 51 } else { 31 })
}

fn button_box(ui: &mut Ui, r: Rect, hover: bool) {
    ui.painter().rect(r, CornerRadius::same(RADIUS_SM), secondary(hover), Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
}

/// Sort by, the select, and the Hide Filters button, right-aligned in `row`, with the result count
/// to their left or above them when the row is too narrow for both. Hands back the height used and
/// the toggle button's rect.
pub fn controls(ui: &mut Ui, row: Rect, f: &mut Filters, query: &str, title: &str) -> (f32, Rect) {
    let ink = pal().header_primary;
    let toggle_label = if f.open { "Hide Filters" } else { "Show Filters" };
    let label_w = text_width(ui, "Sort by", semi(16.0));
    let toggle_w = text_width(ui, toggle_label, medium(16.0)) + 2.0 * SPACE_16 + 8.0 + 20.0;
    let strip = label_w + SPACE_8 + SORT_WIDTH + SPACE_16 + toggle_w;
    let title_w = if title.is_empty() { 0.0 } else { text_width(ui, title, semi(20.0)) };
    let stacked = title_w + SPACE_16 + strip > row.width();
    let mut y = row.top();
    if stacked && !title.is_empty() {
        ui.painter().text(pos2(row.left(), y + 14.0), Align2::LEFT_CENTER, title, semi(20.0), ink);
        y += 28.0 + SPACE_8;
    } else if !title.is_empty() {
        ui.painter().text(pos2(row.left(), y + CONTROL / 2.0), Align2::LEFT_CENTER, title, semi(20.0), ink);
    }
    let toggle = Rect::from_min_size(pos2(row.right() - toggle_w, y), vec2(toggle_w, CONTROL));
    let resp = ui.interact(toggle, Id::new("shop_filters_toggle"), Sense::click());
    button_box(ui, toggle, resp.hovered());
    ui.painter().text(pos2(toggle.left() + SPACE_16, toggle.center().y), Align2::LEFT_CENTER, toggle_label, medium(16.0), ink);
    icons::draw(ui.painter(), Icon::Filter, Rect::from_center_size(pos2(toggle.right() - SPACE_16 - 10.0, toggle.center().y), vec2(20.0, 20.0)), ink);
    mark(ui, "shop:filters_toggle", toggle);
    pointer(ui, resp.hovered());
    if resp.clicked() {
        f.open = !f.open;
    }
    let select_r = Rect::from_min_size(pos2(toggle.left() - SPACE_16 - SORT_WIDTH, y), vec2(SORT_WIDTH, CONTROL));
    let current = f.sort(query);
    let relevance = f.relevance(query);
    let options: Vec<Sort> = Sort::ALL.into_iter().filter(|s| *s != Sort::Relevance || relevance).collect();
    let labels: Vec<&str> = options.iter().map(|s| s.label()).collect();
    if let Some(i) = select(ui, select_r, "sort", current.label(), &labels) {
        f.picked = Some(options[i]);
        f.page = 0;
    }
    (y + CONTROL - row.top(), toggle)
}

/// A select box in `r` showing `current`, with `options` in a menu under it. Hands back the
/// index picked.
pub fn select(ui: &mut Ui, r: Rect, key: &str, current: &str, options: &[&str]) -> Option<usize> {
    let ink = pal().header_primary;
    let resp = ui.interact(r, Id::new(("shop_select", key)), Sense::click());
    button_box(ui, r, resp.hovered());
    ui.painter().text(pos2(r.left() + SPACE_16, r.center().y), Align2::LEFT_CENTER, current, reg(16.0), ink);
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - SPACE_16 - 4.0, r.center().y), vec2(20.0, 20.0)), ink);
    ui.painter().text(pos2(r.left() - SPACE_8, r.center().y), Align2::RIGHT_CENTER, "Sort by", semi(16.0), pal().text_normal);
    mark(ui, &format!("shop:{key}"), r);
    pointer(ui, resp.hovered());
    let mut picked = None;
    egui::Popup::menu(&resp).gap(4.0).show(|ui| {
        ui.set_min_width(r.width());
        for (i, label) in options.iter().enumerate() {
            if menu_row(ui, None, label, None, false, Trail::Radio(*label == current)).clicked() {
                picked = Some(i);
            }
        }
    });
    picked
}

fn check(ui: &mut Ui, o: Pos2, y: f32, label: &str, on: bool) -> bool {
    let row = Rect::from_min_size(o + vec2(0.0, y - 8.0), vec2(BAR, 36.0));
    let resp = ui.interact(row, Id::new(("shop_check", label)), Sense::click());
    let boxr = Rect::from_min_size(o + vec2(0.0, y), vec2(20.0, 20.0));
    if on {
        ui.painter().rect_filled(boxr, CornerRadius::same(4), BLURPLE);
        icons::draw(ui.painter(), Icon::Check, boxr.shrink(2.0), Color32::WHITE);
    } else {
        ui.painter().rect(boxr, CornerRadius::same(4), Color32::from_black_alpha(20), Stroke::new(1.0, pal().header_secondary.gamma_multiply(0.7)), StrokeKind::Inside);
    }
    ui.painter().text(o + vec2(32.0, y + 10.0), Align2::LEFT_CENTER, label, reg(16.0), pal().text_normal);
    mark(ui, &format!("shop:only:{label}"), row);
    pointer(ui, resp.hovered());
    resp.clicked()
}

fn swatch(ui: &mut Ui, c: Pos2, name: &str, rgb: u32, on: bool) -> bool {
    let r = Rect::from_center_size(c, vec2(36.0, 36.0));
    let resp = ui.interact(r, Id::new(("shop_color", name)), Sense::click());
    let fill = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    let p = ui.painter();
    let ring = if on { 4.0 } else { 0.0 };
    if on {
        p.circle_stroke(c, 17.0, Stroke::new(2.0, pal().header_primary));
    }
    let d = 18.0 - ring;
    let mut m = egui::Mesh::default();
    let n = 32;
    m.colored_vertex(c, fill);
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let pt = c + vec2(a.cos(), a.sin()) * d;
        let t = ((pt.y - (c.y - d)) / (2.0 * d)).clamp(0.0, 1.0);
        m.colored_vertex(pt, fill.lerp_to_gamma(Color32::BLACK, 0.3 * t));
    }
    for i in 0..n {
        m.add_triangle(0, i + 1, i + 2);
    }
    p.add(egui::Shape::mesh(m));
    if on {
        let light = (0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32) > 150.0;
        icons::draw(p, Icon::Check, Rect::from_center_size(c, vec2(16.0, 16.0)), if light { Color32::from_gray(0x1e) } else { Color32::WHITE });
    }
    mark(ui, &format!("shop:color:{name}"), r);
    pointer(ui, resp.hovered());
    resp.clicked()
}

fn theme_icon(ui: &Ui, i: usize, r: Rect, tint: Color32) {
    let (_, _, uri, bytes) = THEMES[i];
    let src = egui::ImageSource::Bytes { uri: uri.into(), bytes: egui::load::Bytes::Static(bytes) };
    if let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(src).load_for_size(ui.ctx(), r.size()) {
        ui.painter().image(texture.id, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}

/// The filter bar's contents from the cursor down: Show only, Color, Themes, and Clear all
/// filters once anything is set. True when Clear all filters is clicked.
pub fn bar(ui: &mut Ui, f: &mut Filters, query: &str) -> bool {
    let o = ui.cursor().min;
    let ink = pal().text_normal;
    let before = f.clone();
    ui.painter().text(o + vec2(0.0, 10.0), Align2::LEFT_CENTER, "Show only", semi(16.0), ink);
    let mut y = 38.0;
    for (i, (name, _)) in TYPES.iter().enumerate() {
        if check(ui, o, y, name, f.types[i]) {
            f.types[i] = !f.types[i];
        }
        y += 36.0;
    }
    if check(ui, o, y, "Orbs Eligible", f.orbs) {
        f.orbs = !f.orbs;
    }
    y += 36.0;
    if check(ui, o, y, "Collabs", f.collabs) {
        f.collabs = !f.collabs;
    }
    y += 20.0 + SPACE_24;
    ui.painter().text(o + vec2(0.0, y + 10.0), Align2::LEFT_CENTER, "Color", semi(16.0), ink);
    y += 30.0;
    for (i, (name, _, rgb)) in COLORS.iter().enumerate() {
        let c = o + vec2(18.0 + (i % 5) as f32 * 44.0, y + 18.0 + (i / 5) as f32 * 44.0);
        if swatch(ui, c, name, *rgb, f.colors[i]) {
            f.colors[i] = !f.colors[i];
        }
    }
    y += 2.0 * 44.0 - 8.0 + SPACE_24;
    ui.painter().text(o + vec2(0.0, y + 10.0), Align2::LEFT_CENTER, "Themes", semi(16.0), ink);
    y += 30.0;
    let (mut x, chip_h) = (0.0, 38.0);
    for (i, (name, ..)) in THEMES.iter().enumerate() {
        let w = 12.0 + 14.0 + 5.0 + text_width(ui, name, medium(16.0)) + 8.0;
        if x > 0.0 && x + w > BAR {
            x = 0.0;
            y += chip_h + SPACE_8;
        }
        let r = Rect::from_min_size(o + vec2(x, y), vec2(w, chip_h));
        let resp = ui.interact(r, Id::new(("shop_theme", i)), Sense::click());
        let on = f.themes[i];
        if on {
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SM), BLURPLE);
        } else {
            button_box(ui, r, resp.hovered());
        }
        theme_icon(ui, i, Rect::from_center_size(pos2(r.left() + 12.0 + 7.0, r.center().y), vec2(14.0, 14.0)), Color32::WHITE);
        ui.painter().text(pos2(r.left() + 31.0, r.center().y), Align2::LEFT_CENTER, name, medium(16.0), Color32::WHITE);
        mark(ui, &format!("shop:theme:{name}"), r);
        pointer(ui, resp.hovered());
        if resp.clicked() {
            f.themes[i] = !on;
        }
        x += w + SPACE_8;
    }
    y += chip_h;
    let mut cleared = false;
    if before.dirty(query) {
        y += SPACE_24;
        let r = Rect::from_min_size(o + vec2(0.0, y), vec2(BAR, CONTROL));
        let resp = ui.interact(r, Id::new("shop_clear"), Sense::click());
        button_box(ui, r, resp.hovered());
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "Clear all filters", medium(16.0), pal().header_primary);
        mark(ui, "shop:clear", r);
        pointer(ui, resp.hovered());
        cleared = resp.clicked();
        y += CONTROL;
    }
    ui.allocate_exact_size(vec2(BAR, y), Sense::hover());
    if *f != before {
        f.page = 0;
    }
    cleared
}

/// The bar dropped under `anchor` on narrow windows. Hands back its rect (for the outside-click
/// test) and whether Clear all filters was clicked.
pub fn popover(ctx: &egui::Context, anchor: Rect, f: &mut Filters, query: &str) -> (Rect, bool) {
    let mut cleared = false;
    let width = BAR + 2.0 * SPACE_16;
    let top = anchor.bottom() + SPACE_8;
    let left = (anchor.right() - width).max(ctx.content_rect().left() + SPACE_8);
    let area = egui::Area::new(Id::new("shop_filter_popover")).order(egui::Order::Foreground).fixed_pos(pos2(left, top)).show(ctx, |ui| {
        Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(RADIUS_SM).inner_margin(Margin::same(SPACE_16 as i8)).show(ui, |ui| {
            ui.set_width(BAR);
            ScrollArea::vertical().max_height((ctx.content_rect().bottom() - top - 2.0 * SPACE_16 - 8.0).max(120.0)).auto_shrink([true, true]).show(ui, |ui| {
                cleared = bar(ui, f, query);
            });
        });
    });
    (area.response.rect, cleared)
}

/// Previous, numbered pages and Next under the grid. Hands back the page picked.
pub fn pager(ui: &mut Ui, area: Rect, y: f32, page: usize, pages: usize) -> Option<usize> {
    let mut shown: Vec<Option<usize>> = Vec::new();
    for n in 0..pages {
        if n == 0 || n + 1 == pages || n.abs_diff(page) <= 1 {
            shown.push(Some(n));
        } else if shown.last() != Some(&None) {
            shown.push(None);
        }
    }
    let total = (shown.len() + 2) as f32 * 32.0 + (shown.len() + 1) as f32 * 4.0;
    let mut x = area.center().x - total / 2.0;
    let mut out = None;
    let mut gaps = 0;
    let mut cell = |ui: &mut Ui, label: Option<&str>, icon: Option<Icon>, on: bool, enabled: bool, key: String| -> bool {
        let r = Rect::from_min_size(pos2(x, y), vec2(32.0, 32.0));
        x += 36.0;
        let resp = ui.interact(r, Id::new(("shop_pager", &key)), Sense::click());
        if on {
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SM), secondary(true));
        } else if resp.hovered() && enabled {
            ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SM), secondary(false));
        }
        let ink = if enabled { pal().header_primary } else { pal().text_muted.gamma_multiply(0.5) };
        if let Some(l) = label {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, l, if on { semi(16.0) } else { medium(16.0) }, ink);
        }
        if let Some(i) = icon {
            icons::draw(ui.painter(), i, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), ink);
        }
        mark(ui, &format!("shop:page:{key}"), r);
        pointer(ui, resp.hovered() && enabled);
        enabled && resp.clicked()
    };
    if cell(ui, None, Some(Icon::ArrowLeft), false, page > 0, "prev".into()) {
        out = Some(page - 1);
    }
    for s in &shown {
        match s {
            Some(n) => {
                if cell(ui, Some(&(n + 1).to_string()), None, *n == page, true, (n + 1).to_string()) {
                    out = Some(*n);
                }
            }
            None => {
                gaps += 1;
                cell(ui, Some("…"), None, false, false, format!("gap{gaps}"));
            }
        }
    }
    if cell(ui, None, Some(Icon::ArrowRight), false, page + 1 < pages, "next".into()) {
        out = Some(page + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_until_a_filter_or_search() {
        let mut f = Filters::new();
        assert!(f.plain(""));
        f.types[1] = true;
        assert!(!f.plain(""));
        assert_eq!(f.sort(""), Sort::Popular);
        f.colors[0] = true;
        assert_eq!(f.sort(""), Sort::Relevance);
        f.picked = Some(Sort::PriceAsc);
        assert_eq!(f.sort("cat"), Sort::PriceAsc);
    }

    #[test]
    fn search_path_follows_the_store() {
        let mut f = Filters::new();
        assert_eq!(f.path("cat", 15), "/shop/search?offset=0&limit=15&sort_type=relevance&sort_direction=desc&search=cat");
        f.types[0] = true;
        f.types[4] = true;
        f.orbs = true;
        f.collabs = true;
        f.themes[3] = true;
        f.page = 2;
        assert_eq!(
            f.path("", 12),
            "/shop/search?item_types=AVATAR_DECORATION&item_types=BUNDLE&themes=COLLECTIBLES_THEME_SCI_FI&orbs_eligible=true&is_first_party=false&offset=24&limit=12&sort_type=relevance&sort_direction=desc"
        );
    }

    #[test]
    fn headings_match_the_count_wording() {
        let mut f = Filters::new();
        assert_eq!(heading(&f, "", Some(3)), "");
        assert_eq!(heading(&f, "cat", Some(345)), "Search Results for \"cat\" (345)");
        f.types[1] = true;
        assert_eq!(heading(&f, "", Some(7)), "Nameplates (7)");
        f.types[2] = true;
        assert_eq!(heading(&f, "", Some(7)), "Results (7)");
        f.reset();
        f.collabs = true;
        assert_eq!(heading(&f, "", Some(2)), "Collabs (2)");
        assert_eq!(heading(&f, "", None), "Loading...");
    }
}
