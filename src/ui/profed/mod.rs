//! Discord's profile editor: "Your full profile" with the styles panel on the left, the profile
//! itself in the middle as the thing you edit, and the widgets board on the right.

pub mod board;
mod card;
mod draft;
mod pick;
mod style;
mod suggest;

pub use draft::{Pic, Slot, widgets_of};
pub use pick::gif_arrived;
pub use style::PRESETS;

use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::Value;

use super::cosmo::{self, Item, Kind};
use super::icons::{self, Icon};
use super::settings::{mark, refetch, remote};
use super::{App, avatar, text};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use draft::Draft;

const PANEL_W: f32 = 242.0;
const REGION_W: f32 = 960.0;
const DIALOG_H: f32 = 800.0;
const CARD_W: f32 = 400.0;
const PAD: f32 = 48.0;
// all.css, the dbba93 module: the column beside the form is gone at this viewport width and below
const ONE_COLUMN: f32 = 1024.0;

pub(self) fn one_column(ctx: &egui::Context) -> bool {
    ctx.content_rect().width() <= ONE_COLUMN
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Board,
    Activity,
    Wishlist,
}

pub enum Dlg {
    None,
    Avatar,
    Pick(Kind, Option<Item>),
    Style(style::Edit),
    Colour(Swatch, [u8; 3]),
    Widget(board::Edit),
    Browse,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Swatch {
    Primary,
    Accent,
    Name(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    Profile,
    Banner,
    Tag,
}

pub struct Editor {
    pub guild: Option<Id>,
    pub draft: Draft,
    pub base: Draft,
    pub raw: Value,
    pub ready: bool,
    pub hide: bool,
    pub tab: Tab,
    pub since: f64,
    pub dlg: Dlg,
    pub menu: Option<Menu>,
    pub saving: bool,
    pub prompts: [usize; 2],
    pub tag_q: String,
    pub anchor: Rect,
    pub eyedrop: Option<std::sync::Arc<egui::ColorImage>>,
    pub style_back: Option<style::Edit>,
    pub to_shop: bool,
    opened: usize,
}

pub const BIO_PROMPTS: [&str; 10] = [
    "Describe yourself in a haiku",
    "Describe yourself like a game character",
    "Tell us a bit about you",
    "Share a fun fact about yourself",
    "Describe yourself in three emoji",
    "Your life in one sentence",
    "What’s your villain origin story?",
    "Write a brief intro",
    "Your vibe — chaotic, calm, or somewhere in between?",
    "Your vibe in five words",
];

pub const STATUS_PROMPTS: [&str; 27] = [
    "What have you been playing?",
    "What have you been listening to?",
    "What have you been watching?",
    "What are you currently obsessed with?",
    "Shower thought?",
    "Today I learned...",
    "What’s your hot take?",
    "Best dad joke?",
    "Best emoji to describe your day?",
    "What’s the most useless talent you have?",
    "What video game item would you be?",
    "What have you been reading?",
    "Last song stuck in your head?",
    "Most used emoji lately?",
    "Best thing you ate this week?",
    "What fictional world would you live in?",
    "If your life had a theme song...",
    "Favorite collectible?",
    "Most satisfying game mechanic?",
    "Favorite NPC companion?",
    "Food you’re craving now?",
    "Pick a mythical creature as a pet",
    "Latest hobby?",
    "Favorite anime lately?",
    "Just ranked up in...",
    "Choose your character class",
    "Hit a high score in...",
];

pub fn open(app: &mut App, guild: Option<Id>) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as usize);
    app.profed = Some(Editor {
        guild,
        draft: Draft::default(),
        base: Draft::default(),
        raw: Value::Null,
        ready: false,
        hide: false,
        tab: Tab::Board,
        since: 0.0,
        dlg: Dlg::None,
        menu: None,
        saving: false,
        prompts: [t % BIO_PROMPTS.len(), (t / 7) % STATUS_PROMPTS.len()],
        tag_q: String::new(),
        anchor: Rect::NOTHING,
        eyedrop: None,
        style_back: None,
        to_shop: false,
        opened: t,
    });
    for k in ["me_profile", "owned", "avatars"] {
        refetch(app, k);
    }
    app.cosmo.owned = None;
}

pub fn dirty(ed: &Editor) -> bool {
    ed.ready && ed.draft != ed.base
}

pub(self) fn tile(ui: &mut Ui, r: Rect, id: &str, on: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("tile", id)), Sense::click());
    let hot = super::motion::hot(ui.ctx(), &resp);
    let fill = super::motion::mix(faint(31), faint(51), hot);
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    let stroke = if on { Stroke::new(2.0, Color32::WHITE) } else { Stroke::new(1.0, faint(41)) };
    ui.painter().rect_stroke(r, CornerRadius::same(8), stroke, egui::StrokeKind::Inside);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(self) fn nitro(app: &mut App, what: &str) -> bool {
    let on = app.store.nitro();
    if !on {
        app.toast(format!("{what} are exclusive to Nitro."));
    }
    on
}

pub(self) fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub(self) fn round_icon(ui: &mut Ui, r: Rect, icon: Icon, tip: &str, overlay: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("round", tip)), Sense::click());
    let fill = if overlay { Color32::from_black_alpha(143) } else { Color32::TRANSPARENT };
    let hot = resp.hovered();
    ui.painter().rect_filled(r, CornerRadius::same(8), if hot && !overlay { faint(31) } else { fill });
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), if overlay { Color32::WHITE } else { pal().header_secondary });
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

pub(self) fn rgb_of(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

pub(self) fn pack(c: [u8; 3]) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

pub(self) fn hex(c: u32) -> String {
    format!("#{:06x}", c & 0xffffff)
}

/// A theme colour as the profile shows it: dimmed in dark themes.
pub(self) fn dim(c: u32) -> Color32 {
    let k = if pal().dark { 0.4 } else { 1.0 };
    let ch = |s: u32| (((c >> s) & 255) as f32 * k) as u8;
    Color32::from_rgb(ch(16), ch(8), ch(0))
}

pub(self) fn gradient(ui: &Ui, r: Rect, top: Color32, bottom: Color32, radius: u8) {
    ui.painter().add(gradient_shape(ui, r, top, bottom, radius));
}

pub(self) fn gradient_shape(ui: &Ui, r: Rect, top: Color32, bottom: Color32, radius: u8) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    let size = ui.ctx().fonts(|f| f.font_image_size());
    let mut tess = egui::epaint::Tessellator::new(ui.ctx().pixels_per_point(), Default::default(), size, Vec::new());
    tess.tessellate_rect(&egui::epaint::RectShape::filled(r, radius, Color32::WHITE), &mut mesh);
    for v in &mut mesh.vertices {
        let t = ((v.pos.y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        v.color = Color32::from_rgb(m(top.r(), bottom.r()), m(top.g(), bottom.g()), m(top.b(), bottom.b()));
    }
    egui::Shape::mesh(mesh)
}

pub fn theme_pair(d: &Draft) -> Option<(Color32, Color32)> {
    match d.theme {
        [Some(a), Some(b)] => Some((dim(a), dim(b))),
        [Some(a), None] | [None, Some(a)] => Some((dim(a), dim(a))),
        _ => None,
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut ed) = app.profed.take() else { return };
    let me = app.store.me;
    let user = app.store.user(me).cloned().unwrap_or_default();
    if !ed.ready {
        let path = match ed.guild {
            Some(g) => format!("/users/{me}/profile?type=modal&with_mutual_guilds=false&guild_id={g}"),
            None => format!("/users/{me}/profile?type=modal&with_mutual_guilds=false"),
        };
        if let Some(Ok(v)) = remote(app, "me_profile", &path).cloned() {
            ed.draft = draft::from_profile(&v, &user, ed.guild);
            ed.base = ed.draft.clone();
            ed.raw = v;
            ed.ready = true;
            ed.since = ctx.input(|i| i.time);
        }
    }
    let screen = ctx.content_rect();
    let h = DIALOG_H.min(screen.height() - 80.0).max(200.0);
    let w = (PANEL_W + REGION_W).min(screen.width() - 80.0).max(300.0);
    let mut close = false;
    let frame = Frame::new().fill(Color32::BLACK).corner_radius(16).stroke(Stroke::new(1.0, faint(51)));
    let modal = super::motion::Modal::new("profed".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_min_size(vec2(w, h));
        ui.set_max_size(vec2(w, h));
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect();
        let mut x = area.left();
        if !ed.hide {
            let panel = Rect::from_min_size(area.min, vec2(PANEL_W.min(area.width() * 0.5), area.height()));
            panel_ui(app, &mut ed, ui, panel, &user);
            x = panel.right();
        } else {
            let b = Rect::from_min_size(pos2(area.left(), area.top() + 16.0), vec2(36.0, 36.0));
            let resp = ui.interact(b, ui.id().with("show_styles"), Sense::click());
            ui.painter().rect_filled(b, CornerRadius { nw: 0, ne: 8, sw: 0, se: 8 }, faint(41));
            icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(b.center(), vec2(16.0, 16.0)), pal().header_primary);
            ed.hide &= !resp.on_hover_text("Show Styles").clicked();
        }
        let region = Rect::from_min_max(pos2(x, area.top()), area.max);
        region_ui(app, &mut ed, ui, region, &user);
        let c = Rect::from_min_size(pos2(area.right() - 56.0, area.top() + 12.0), vec2(32.0, 32.0));
        let resp = ui.interact(c, ui.id().with("profed_close"), Sense::click());
        ui.painter().circle_filled(c.center(), 16.0, faint(if resp.hovered() { 61 } else { 31 }));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(c.center(), vec2(16.0, 16.0)), pal().header_primary);
        mark(ui, "profed_close", c);
        if resp.on_hover_text("Close").clicked() {
            close = true;
        }
    });
    let nested = !matches!(ed.dlg, Dlg::None) || ed.menu.is_some();
    pick::dialogs(app, &mut ed, ctx, &user);
    if ed.to_shop {
        app.settings_page = None;
        return app.open_shop(super::shop::Page::Shop);
    }
    if close || (modal.should_close() && !nested) {
        return;
    }
    app.profed = Some(ed);
}

fn panel_ui(app: &mut App, ed: &mut Editor, ui: &mut Ui, panel: Rect, user: &crate::state::User) {
    ui.painter().rect_filled(panel, CornerRadius { nw: 16, ne: 0, sw: 16, se: 0 }, pal().bg_secondary);
    let head = Rect::from_min_size(panel.min, vec2(panel.width(), 64.0));
    let guilds: Vec<(Id, String)> = app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g).map(|x| (*g, x.name.clone()))).collect();
    let shown = ed.guild.and_then(|g| guilds.iter().find(|x| x.0 == g)).map_or("Main Profile".to_string(), |g| g.1.clone());
    let sel = Rect::from_min_size(head.min + vec2(10.0, 10.0), vec2(head.width() - 64.0, 44.0));
    let resp = ui.interact(sel, ui.id().with("select_profile"), Sense::click());
    mark(ui, "profile_select", sel);
    if resp.hovered() {
        ui.painter().rect_filled(sel, CornerRadius::same(8), faint(20));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let r = text::paint(ui.painter(), pos2(sel.left() + 8.0, sel.center().y), Align2::LEFT_CENTER, &shown, medium(16.0), pal().header_primary, sel.width() - 40.0);
    icons::draw(ui.painter(), if ed.menu == Some(Menu::Profile) { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() + 14.0, sel.center().y), vec2(18.0, 18.0)), pal().header_primary);
    if resp.clicked() {
        ed.menu = if ed.menu == Some(Menu::Profile) { None } else { Some(Menu::Profile) };
    }
    let hb = Rect::from_min_size(pos2(panel.right() - 42.0, panel.top() + 16.0), vec2(32.0, 32.0));
    if round_icon(ui, hb, Icon::ArrowLeft, "Hide Styles", false).clicked() {
        ed.hide = true;
    }
    mark(ui, "hide_styles", hb);
    ui.painter().hline(panel.x_range(), head.bottom() - 0.5, Stroke::new(1.0, faint(31)));
    if ed.menu == Some(Menu::Profile)
        && let Some(g) = profile_menu(ui, sel, ed.guild, &guilds)
    {
        ed.menu = None;
        if g != ed.guild {
            ed.guild = g;
            ed.ready = false;
            refetch(app, "me_profile");
        }
    }
    let list = Rect::from_min_max(pos2(panel.left(), head.bottom()), panel.max);
    mark(ui, "profed_panel", list);
    let mut scroll = ui.new_child(UiBuilder::new().max_rect(list));
    egui::ScrollArea::vertical().id_salt(("profed_panel", ed.opened)).auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let x = list.left() + 18.0;
        let w = list.width() - 36.0;
        let mut y = ui.cursor().top() + 16.0;
        let head = |ui: &mut Ui, y: &mut f32, s: &str, nitro: bool| {
            let r = ui.painter().text(pos2(x, *y + 9.0), Align2::LEFT_CENTER, s, medium(14.0), pal().header_primary);
            if nitro {
                icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(r.right() + 12.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
            }
            *y += 26.0;
        };
        if ed.ready {
            if ed.guild.is_some() && !app.settings.profed_notice {
                notice(app, ui, x, w, &mut y);
            }
            panel_tiles(app, ed, ui, x, w, &mut y, user, &head);
        }
        ui.allocate_rect(Rect::from_min_max(pos2(x, ui.cursor().top()), pos2(x + w, y + 24.0)), Sense::hover());
    });
}

fn notice(app: &mut App, ui: &mut Ui, x: f32, w: f32, y: &mut f32) {
    let g = ui.painter().layout("You can customize styles, display name, pronouns, and bio for each server.".into(), reg(14.0), pal().text_normal, w - 52.0);
    let r = Rect::from_min_size(pos2(x, *y), vec2(w, g.size().y + 20.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(88, 101, 242, 12));
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, BLURPLE), egui::StrokeKind::Inside);
    ui.painter().galley(r.min + vec2(10.0, 10.0), g, pal().text_normal);
    let b = Rect::from_center_size(pos2(r.right() - 20.0, r.top() + 20.0), vec2(20.0, 20.0));
    let resp = ui.interact(b, ui.id().with("profed_notice_x"), Sense::click());
    icons::draw(ui.painter(), Icon::Close, b, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
    mark(ui, "profed_notice_x", b);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.on_hover_text("Dismiss notice").clicked() {
        app.settings.profed_notice = true;
        crate::cache::set_settings(&app.settings);
    }
    *y += r.height() + 16.0;
}

fn profile_menu(ui: &mut Ui, under: Rect, cur: Option<Id>, guilds: &[(Id, String)]) -> Option<Option<Id>> {
    let mut pick = None;
    let area = egui::Area::new("profed_profile_menu".into()).order(egui::Order::Tooltip).fixed_pos(under.left_bottom() + vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
        Frame::popup(ui.style()).fill(pal().bg_floating).corner_radius(8).show(ui, |ui| {
            ui.set_width(under.width() + 24.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "Main Profile").clicked() {
                    pick = Some(None);
                }
                for (g, n) in guilds {
                    if ui.selectable_label(cur == Some(*g), n).clicked() {
                        pick = Some(Some(*g));
                    }
                }
            });
        });
    });
    if pick.is_none() && area.response.clicked_elsewhere() {
        pick = Some(cur);
    }
    pick
}

type Head<'a> = &'a dyn Fn(&mut Ui, &mut f32, &str, bool);

#[allow(clippy::too_many_arguments)]
fn panel_tiles(app: &mut App, ed: &mut Editor, ui: &mut Ui, x: f32, w: f32, y: &mut f32, user: &crate::state::User, head: Head) {
    let me = app.store.me;
    let bar = vec2(w.min(208.0), 42.0);
    let sq = 100.0f32.min((w - 8.0) / 2.0);
    let guild = ed.guild.is_some();
    head(ui, y, "Nameplate", false);
    let r = Rect::from_min_size(pos2(x, *y), bar);
    let resp = tile(ui, r, "nameplate", false);
    mark(ui, "tile:nameplate", r);
    let d = &ed.draft;
    match &d.nameplate {
        Some(n) => text::image(&ui.painter().with_clip_rect(r.shrink(1.0)), &cosmo::plate(&n.asset), r),
        None => {
            avatar(ui, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
            let nm = if d.name.is_empty() { user.display() } else { &d.name };
            text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y), Align2::LEFT_CENTER, nm, medium(16.0), pal().header_primary, bar.x - 90.0);
        }
    }
    overlay_icon(ui, r, resp.hovered(), d.nameplate.is_some(), true);
    let tip = match d.nameplate.as_ref().map(|n| app.product(&n.sku).map_or_else(|| "Set".to_string(), |p| p.name)) {
        Some(n) => format!("Nameplate: {n}"),
        None => "Nameplate: Not set".into(),
    };
    if resp.on_hover_text(tip).clicked() {
        ed.dlg = Dlg::Pick(Kind::Nameplate, ed.draft.nameplate.clone());
    }
    *y += bar.y + 24.0;
    head(ui, y, "Avatar & Decoration", false);
    let ra = Rect::from_min_size(pos2(x, *y), vec2(sq, sq));
    let rd = Rect::from_min_size(pos2(x + sq + 8.0, *y), vec2(sq, sq));
    let resp = tile(ui, ra, "avatar", false);
    mark(ui, "tile:avatar", ra);
    let av = Rect::from_center_size(ra.center(), vec2(sq * 0.72, sq * 0.72));
    card::paint_avatar(ui, av, d, &user.avatar_url());
    let rm = Rect::from_min_size(pos2(ra.right() - 38.0, ra.top() + 4.0), vec2(32.0, 32.0));
    let removable = !matches!(d.avatar, Pic::Gone) && (user.avatar.is_some() || !matches!(d.avatar, Pic::Keep));
    let mut gone = false;
    if removable {
        let r2 = round_icon(ui, rm, Icon::Trash, "Remove Avatar", true);
        mark(ui, "remove_avatar", rm);
        gone = r2.clicked();
    }
    if resp.on_hover_text("Avatar: Current avatar image").clicked() && !gone {
        ed.dlg = Dlg::Avatar;
    }
    if gone {
        ed.draft.avatar = Pic::Gone;
    }
    let resp = tile(ui, rd, "decoration", false);
    mark(ui, "tile:decoration", rd);
    let d = &ed.draft;
    let c = Rect::from_center_size(rd.center(), vec2(sq * 0.72, sq * 0.72));
    ui.painter().circle_stroke(c.center(), c.width() / 2.0, Stroke::new(1.0, faint(61)));
    if let Some(dec) = &d.decoration {
        avatar(ui, c, &user.avatar_url());
        text::image(ui.painter(), &cosmo::deco(&dec.asset, 128), Rect::from_center_size(c.center(), c.size() * 1.2));
    }
    overlay_icon(ui, rd, resp.hovered(), d.decoration.is_some(), d.decoration.is_none());
    let tip = match d.decoration.as_ref().map(|n| app.product(&n.sku).map_or_else(|| "Set".to_string(), |p| p.name)) {
        Some(n) => format!("Avatar Decoration: {n}"),
        None => "Avatar Decoration: Not set".into(),
    };
    if resp.on_hover_text(tip).clicked() {
        ed.dlg = Dlg::Pick(Kind::Decoration, ed.draft.decoration.clone());
    }
    *y += sq + 24.0;
    let _ = me;
    if !guild {
        head(ui, y, "Display Name Style", true);
        let r = Rect::from_min_size(pos2(x, *y), bar);
        let resp = tile(ui, r, "dns", false);
        mark(ui, "tile:dns", r);
        let nm = if ed.draft.name.is_empty() { user.display().to_string() } else { ed.draft.name.clone() };
        style::paint_name(ui, pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &nm, ed.draft.style.as_ref(), 24.0, r.width() - 56.0, true);
        overlay_icon(ui, r, resp.hovered(), ed.draft.style.is_some(), ed.draft.style.is_none());
        if resp.on_hover_text(if ed.draft.style.is_some() { "Display name style: Set" } else { "Display name style: Not set" }).clicked() && nitro(app, "Display name styles") {
            ed.dlg = Dlg::Style(style::Edit::from(ed.draft.style.as_ref()));
        }
        *y += bar.y + 24.0;
    }
    head(ui, y, "Theme & Banner", true);
    let rt = Rect::from_min_size(pos2(x, *y), vec2(sq, sq));
    let rb = Rect::from_min_size(pos2(x + sq + 8.0, *y), vec2(sq, sq));
    let (a, b) = (ed.draft.theme[0], ed.draft.theme[1]);
    let paint = ui.painter();
    paint.rect_filled(rt, CornerRadius::same(8), Color32::from_gray(30));
    let top = a.map_or(Color32::from_rgb(0x58, 0x65, 0xf2), |c| role_color(c));
    let bot = b.map_or(top, |c| role_color(c));
    gradient(ui, rt, top, bot, 8);
    ui.painter().rect_stroke(rt, CornerRadius::same(8), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
    for (i, (r, col, which, tip)) in [
        (Rect::from_center_size(pos2(rt.center().x, rt.top() + 28.0), vec2(24.0, 24.0)), a, Swatch::Primary, "Theme Primary Color"),
        (Rect::from_center_size(pos2(rt.center().x, rt.bottom() - 28.0), vec2(24.0, 24.0)), b, Swatch::Accent, "Theme Accent Color"),
    ]
    .into_iter()
    .enumerate()
    {
        let resp = ui.interact(r.expand(6.0), ui.id().with(("swatch", i)), Sense::click());
        let fill = col.map_or(Color32::from_gray(60), role_color);
        ui.painter().rect(r, CornerRadius::same(6), fill, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
        mark(ui, if i == 0 { "swatch:primary" } else { "swatch:accent" }, r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(format!("{tip}: {}", col.map_or("Not set".into(), hex))).clicked() && nitro(app, "Theme colors") {
            ed.dlg = Dlg::Colour(which, rgb_of(col.unwrap_or(0x5865f2)));
        }
    }
    let resp = tile(ui, rb, "banner", false);
    mark(ui, "tile:banner", rb);
    let cr = CornerRadius::same(8);
    match (&ed.draft.banner, &ed.draft.banner_url) {
        (Pic::New(u), _) => card::paint_data_image(ui, u, rb, cr),
        (Pic::Keep, Some(url)) => crate::gif::paint_still(ui, url, rb, cr, false),
        _ => {}
    }
    let has = matches!(&ed.draft.banner, Pic::New(_)) || (matches!(&ed.draft.banner, Pic::Keep) && ed.draft.banner_url.is_some());
    let mut gone = false;
    if has {
        let rm = Rect::from_min_size(pos2(rb.right() - 38.0, rb.top() + 4.0), vec2(32.0, 32.0));
        gone = round_icon(ui, rm, Icon::Trash, "Remove Banner", true).clicked();
        mark(ui, "remove_banner", rm);
    }
    overlay_icon(ui, rb, resp.hovered(), has, !has);
    if resp.on_hover_text("Banner image: Current banner image").clicked() && !gone {
        card::change_banner(app, ed);
    }
    if gone {
        ed.draft.banner = Pic::Gone;
    }
    *y += sq + 24.0;
    head(ui, y, "Profile Effect & Frame", false);
    for (i, kind) in [Kind::Effect, Kind::Frame].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(x + i as f32 * (sq + 8.0), *y), vec2(sq, sq));
        let resp = tile(ui, r, if i == 0 { "effect" } else { "frame" }, false);
        mark(ui, if i == 0 { "tile:effect" } else { "tile:frame" }, r);
        let it = if i == 0 { &ed.draft.effect } else { &ed.draft.frame };
        let prev = Rect::from_center_size(r.center(), vec2(60.0, if i == 0 { 88.0 } else { 60.0 }).min(r.size() - vec2(8.0, 8.0)));
        ui.painter().rect_filled(prev, CornerRadius::same(4), Color32::from_black_alpha(120));
        if let Some(p) = it.as_ref().and_then(|it| app.product(&it.sku)) {
            text::image(&ui.painter().with_clip_rect(prev), &p.thumb, prev);
        }
        overlay_icon(ui, r, resp.hovered(), it.is_some(), it.is_none());
        if resp.on_hover_text(match (i, it.is_some()) { (0, true) => "Profile Effect: Set", (0, false) => "Profile Effect: Not set", (_, true) => "Profile Frame: Set", _ => "Profile Frame: Not set" }).clicked() {
            ed.dlg = Dlg::Pick(kind, it.clone());
        }
    }
    *y += sq + 24.0;
}

fn overlay_icon(ui: &Ui, r: Rect, hot: bool, set: bool, always: bool) {
    if !hot && !always {
        return;
    }
    let c = Rect::from_center_size(r.center(), vec2(24.0, 24.0));
    let c = if r.width() > r.height() * 2.0 { Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(24.0, 24.0)) } else { c };
    let ink = Color32::WHITE;
    ui.painter().circle_filled(c.center(), 12.0, Color32::from_black_alpha(if hot { 200 } else { 140 }));
    icons::draw(ui.painter(), if set { Icon::Pencil } else { Icon::Plus }, Rect::from_center_size(c.center(), vec2(14.0, 14.0)), ink);
}

fn region_ui(app: &mut App, ed: &mut Editor, ui: &mut Ui, region: Rect, user: &crate::state::User) {
    let cr = CornerRadius { nw: 0, ne: 16, sw: 0, se: 16 };
    ui.painter().rect_filled(region, cr, Color32::BLACK);
    if let Some((a, b)) = theme_pair(&ed.draft) {
        gradient(ui, region.shrink(1.0), a.gamma_multiply(0.7), b.gamma_multiply(0.7), 15);
    }
    if !ed.ready {
        super::motion::spinner(&mut ui.new_child(UiBuilder::new().max_rect(Rect::from_center_size(region.center(), vec2(32.0, 32.0)))), 32.0, pal().text_muted);
        return;
    }
    mark(ui, "profed_region", region);
    let pad = PAD.min(region.width() * 0.08);
    let inner = region.shrink2(vec2(pad, pad));
    let card_w = CARD_W.min(inner.width());
    let side_by_side = !one_column(ui.ctx()) && inner.width() >= CARD_W + 32.0 + 2.0 * board::CELL_MIN + 16.0;
    let mut scroll = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(pos2(region.left(), region.top() + pad), pos2(region.right(), region.bottom() - 8.0))));
    let out = egui::ScrollArea::vertical().id_salt(("profed_region", ed.opened)).auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let origin = pos2(inner.left(), ui.cursor().top());
        let card_rect = Rect::from_min_size(origin, vec2(card_w, 0.0));
        let ch = card::show(app, ed, ui, card_rect, user);
        let board_rect = if side_by_side {
            Rect::from_min_max(pos2(origin.x + card_w + 32.0, origin.y), pos2(inner.right(), origin.y + 600.0))
        } else {
            Rect::from_min_size(pos2(origin.x, origin.y + ch + 32.0), vec2(inner.width(), 600.0))
        };
        let bh = board::show(app, ed, ui, board_rect);
        let bottom = if side_by_side { origin.y + ch.max(bh) } else { board_rect.top() + bh };
        ui.allocate_rect(Rect::from_min_max(pos2(origin.x, ui.cursor().top()), pos2(inner.right(), bottom + 24.0)), Sense::hover());
    });
    crate::ui::e2e::scrolled(ui.ctx(), &out);
    if dirty(ed) {
        save_bar(app, ed, ui, region);
    }
}

fn save_bar(app: &mut App, ed: &mut Editor, ui: &mut Ui, region: Rect) {
    let r = Rect::from_center_size(pos2(region.center().x, region.bottom() - 40.0), vec2((region.width() - 40.0).min(700.0), 52.0));
    ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
    ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful \u{2014} you have unsaved changes!", semi(16.0), pal().header_primary);
    let mut b = ui.new_child(UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 7.0))).layout(Layout::right_to_left(Align::Center)));
    let save_r = super::button(&mut b, if ed.saving { "\u{2026}" } else { "Save Changes" }, GREEN, 120.0);
    mark(ui, "save_changes", save_r.rect);
    let save = save_r.clicked();
    b.add_space(12.0);
    let reset = b.add(egui::Label::new(RichText::new("Reset").font(semi(14.0)).color(pal().header_primary)).sense(Sense::click())).clicked();
    mark(ui, "reset_changes", Rect::from_center_size(pos2(r.right() - 150.0, r.center().y), vec2(50.0, 24.0)));
    if reset {
        ed.draft = ed.base.clone();
    }
    if save && !ed.saving {
        ed.saving = true;
        save_all(app, ed);
    }
}

fn save_all(app: &mut App, ed: &Editor) {
    let w = draft::writes(&ed.draft, &ed.base, ed.guild);
    match ed.guild {
        Some(g) => {
            app.rest("PATCH", format!("/guilds/{g}/profile/@me"), Some(w.profile), Tag::Remote("action"));
            app.rest("PATCH", format!("/guilds/{g}/members/@me"), Some(w.account), Tag::Remote("profile_save"));
        }
        None => {
            app.rest("PATCH", "/users/@me/profile".into(), Some(w.profile), Tag::Remote("action"));
            app.rest("PATCH", "/users/@me".into(), Some(w.account), Tag::Remote("profile_save"));
        }
    }
    if ed.draft.widgets != ed.base.widgets {
        app.rest("PUT", "/users/@me/widgets".into(), Some(draft::widgets_body(&ed.draft.widgets)), Tag::Remote("widgets_save"));
    }
}

pub fn saved(app: &mut App, res: Result<Value, String>) {
    let Some(ed) = &mut app.profed else { return };
    ed.saving = false;
    match res {
        Err(e) => app.toast(e),
        Ok(v) => {
            let global = ed.guild.is_none();
            ed.ready = false;
            if v["id"].is_string() && global {
                app.store.apply("USER_UPDATE", &v);
            }
            refetch(app, "me_profile");
            app.profiles.remove(&app.store.me);
            app.toast("Your changes have been saved.");
        }
    }
}

pub fn widgets_saved(app: &mut App, res: Result<Value, String>) {
    if let Err(e) = res {
        app.toast(e);
    }
}
