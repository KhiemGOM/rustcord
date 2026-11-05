use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, Frame, Margin, Painter, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashSet;

use super::perms::SECTIONS;
use super::{App, bar, faint, rule};
use crate::net::Tag;
use crate::state::{Id, Role, sid};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

const HOLO: [u32; 3] = [11127295, 16759788, 16761760];
const PRESETS: [u32; 20] = [
    0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c, 0x95a5a6, 0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300, 0x992d22, 0x979c9f, 0x546e7a,
];
const PLATFORMS: [(&str, &str); 18] = [
    ("twitch", "Twitch"),
    ("youtube", "YouTube"),
    ("steam", "Steam"),
    ("github", "GitHub"),
    ("reddit", "Reddit"),
    ("spotify", "Spotify"),
    ("twitter", "X"),
    ("xbox", "Xbox"),
    ("battlenet", "Battle.net"),
    ("tiktok", "TikTok"),
    ("paypal", "PayPal"),
    ("ebay", "eBay"),
    ("bluesky", "Bluesky"),
    ("crunchyroll", "Crunchyroll"),
    ("epicgames", "Epic Games"),
    ("playstation", "PlayStation Network"),
    ("facebook", "Facebook"),
    ("instagram", "Instagram"),
];

#[derive(Clone, PartialEq)]
enum IconEdit {
    Keep,
    Clear,
    Image(String),
    Emoji(String),
}

#[derive(Clone, PartialEq)]
struct Draft {
    name: String,
    color: u32,
    grad: Vec<u32>,
    hoist: bool,
    mentionable: bool,
    perms: u64,
    icon: IconEdit,
}

impl Draft {
    fn of(r: &Role) -> Draft {
        Draft { name: r.name.clone(), color: r.color, grad: r.grad.clone(), hoist: r.hoist, mentionable: r.mentionable, perms: r.perms, icon: IconEdit::Keep }
    }

    fn style(&self) -> u8 {
        match self.grad.as_slice() {
            [] => 0,
            g if g == HOLO => 2,
            _ => 1,
        }
    }
}

pub struct Edit {
    pub id: Id,
    tab: u8,
    base: Draft,
    draft: Draft,
    pq: String,
    mq: String,
    saving: bool,
    pub focus_name: bool,
    add: Option<(String, HashSet<Id>)>,
    emoji: Option<String>,
    links: Option<Vec<String>>,
    links_all: bool,
}

impl Edit {
    pub fn new(r: &Role, tab: u8) -> Edit {
        let d = Draft::of(r);
        Edit { id: r.id, tab, base: d.clone(), draft: d, pq: String::new(), mq: String::new(), saving: false, focus_name: false, add: None, emoji: None, links: None, links_all: true }
    }

    pub fn adding(&self) -> bool {
        self.add.is_some()
    }

    pub fn rebase(&mut self, r: &Role) {
        self.base = Draft::of(r);
        self.draft = self.base.clone();
        self.saving = false;
    }
}

fn gradient_text(p: &Painter, pos: egui::Pos2, text: &str, font: egui::FontId, cols: &[u32]) -> Rect {
    let n = text.chars().count().max(2) - 1;
    let mut job = LayoutJob::default();
    for (i, ch) in text.chars().enumerate() {
        let t = i as f32 / n as f32 * (cols.len() - 1) as f32;
        let (a, b) = (role_color(cols[t.floor() as usize]), role_color(cols[(t.floor() as usize + 1).min(cols.len() - 1)]));
        let c = Color32::from_rgb(
            egui::lerp(a.r() as f32..=b.r() as f32, t.fract()) as u8,
            egui::lerp(a.g() as f32..=b.g() as f32, t.fract()) as u8,
            egui::lerp(a.b() as f32..=b.b() as f32, t.fract()) as u8,
        );
        job.append(&ch.to_string(), 0.0, TextFormat { font_id: font.clone(), color: c, ..Default::default() });
    }
    let g = p.layout_job(job);
    let r = Rect::from_min_size(pos, g.size());
    p.galley(pos, g, Color32::WHITE);
    r
}

fn name_preview(p: &Painter, pos: egui::Pos2, name: &str, d: &Draft, ink: Color32) -> Rect {
    match d.grad.len() {
        0 => p.text(pos, Align2::LEFT_TOP, name, medium(16.0), if d.color != 0 { role_color(d.color) } else { ink }),
        _ => gradient_text(p, pos, name, medium(16.0), &d.grad),
    }
}

fn tabs(ui: &mut Ui, e: &mut Edit, everyone: bool, members: usize) {
    let items: Vec<(u8, String)> = if everyone {
        vec![(1, "Permissions".into())]
    } else {
        vec![(0, "Display".into()), (1, "Permissions".into()), (2, "Links".into()), (3, format!("Manage Members ({members})"))]
    };
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(51)));
    let mut x = r.left();
    for (t, label) in items {
        let g = ui.painter().layout_no_wrap(label, medium(16.0), pal().header_primary);
        let b = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x + 8.0, r.height()));
        let resp = ui.interact(b, ui.id().with(("role_tab", t)), Sense::click());
        let on = e.tab == t;
        let ink = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
        ui.painter().text(pos2(b.left() + 4.0, b.center().y), Align2::LEFT_CENTER, g.text(), medium(16.0), ink);
        if on {
            ui.painter().rect_filled(Rect::from_min_size(pos2(b.left(), b.bottom() - 2.0), vec2(b.width(), 2.0)), 0, BLURPLE);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            e.tab = t;
        }
        x = b.right() + 24.0;
    }
    ui.add_space(24.0);
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(mut e) = app.server.as_mut().and_then(|s| s.pages.roles.edit.take()) else { return };
    let Some(role) = app.store.guilds.get(&gid).and_then(|g| g.roles.get(&e.id)).cloned() else { return };
    let everyone = e.id == gid;
    if everyone {
        e.tab = 1;
    }
    let mut close = false;
    let back = ui.horizontal(|ui| {
        let r = ui.add(egui::Label::new(RichText::new("\u{2190} Back").font(semi(14.0)).color(pal().header_secondary)).sense(Sense::click()));
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        r.clicked()
    });
    close |= back.inner;
    ui.add_space(8.0);
    let head = if everyone { "DEFAULT PERMISSIONS".to_string() } else { format!("EDIT ROLE \u{2014} {}", e.base.name.to_uppercase()) };
    ui.label(RichText::new(head).font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(12.0);
    let members: Vec<Id> = app.server.as_ref().and_then(|s| s.data.get("role_members")).and_then(|v| v.as_array()).map(|a| a.iter().map(sid).collect()).unwrap_or_default();
    let n = app
        .server_data("role_counts", format!("/guilds/{gid}/roles/member-counts"))
        .and_then(|v| v[e.id.to_string()].as_u64())
        .map_or(members.len(), |n| n as usize);
    tabs(ui, &mut e, everyone, n);
    match e.tab {
        0 => {
            if display(app, ui, gid, &mut e, &role) {
                app.server = None;
                app.ui.guild = Some(gid);
                app.open_boosts(gid);
                return;
            }
        }
        1 => permissions(app, ui, &mut e, &role),
        2 => links(app, ui, gid, &mut e, &role, n),
        _ => manage(app, ui, gid, &mut e, &members, &role),
    }
    if e.draft != e.base {
        ui.add_space(80.0);
        let (reset, save) = bar::unsaved(ui, e.saving);
        if reset {
            e.draft = e.base.clone();
        }
        if save {
            save_role(app, gid, &mut e);
        }
    }
    if !close && app.store.guilds.get(&gid).is_some_and(|g| g.roles.contains_key(&e.id)) {
        if let Some(s) = &mut app.server {
            s.pages.roles.edit = Some(e);
        }
    }
}

fn save_role(app: &mut App, gid: Id, e: &mut Edit) {
    let (b, d) = (&e.base, &e.draft);
    let mut body = json!({});
    if b.name != d.name {
        body["name"] = json!(d.name.trim());
    }
    if b.color != d.color || b.grad != d.grad {
        body["color"] = json!(d.color);
        body["colors"] = match d.grad.as_slice() {
            [] => json!({ "primary_color": d.color, "secondary_color": null, "tertiary_color": null }),
            g => json!({ "primary_color": g[0], "secondary_color": g.get(1), "tertiary_color": g.get(2) }),
        };
    }
    if b.hoist != d.hoist {
        body["hoist"] = json!(d.hoist);
    }
    if b.mentionable != d.mentionable {
        body["mentionable"] = json!(d.mentionable);
    }
    if b.perms != d.perms {
        body["permissions"] = json!(d.perms.to_string());
    }
    match &d.icon {
        IconEdit::Keep => {}
        IconEdit::Clear => {
            body["icon"] = Value::Null;
            body["unicode_emoji"] = Value::Null;
        }
        IconEdit::Image(i) => {
            body["icon"] = json!(i);
            body["unicode_emoji"] = Value::Null;
        }
        IconEdit::Emoji(x) => {
            body["unicode_emoji"] = json!(x);
            body["icon"] = Value::Null;
        }
    }
    e.saving = true;
    app.rest("PATCH", format!("/guilds/{gid}/roles/{}", e.id), Some(body), Tag::Server("role_saved"));
}

fn swatch(ui: &mut Ui, r: Rect, c: u32, on: bool, salt: impl std::hash::Hash + std::fmt::Debug) -> egui::Response {
    let id = ui.id().with(salt);
    let resp = ui.interact(r, id, Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(8), role_color(c));
    if on {
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), Color32::WHITE);
    }
    if resp.hovered() {
        ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(10), Stroke::new(2.0, pal().header_primary), egui::StrokeKind::Outside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn hex(c: u32) -> String {
    format!("#{c:06X}")
}

fn pick_color(ui: &mut Ui, salt: &str, value: &mut u32) -> bool {
    let mut changed = false;
    let default = 0x99aab5;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (r, _) = ui.allocate_exact_size(vec2(60.0, 52.0), Sense::hover());
        let on = *value == 0;
        if swatch(ui, r, default, on, (salt, "default")).on_hover_text("Default").clicked() {
            *value = 0;
            changed = true;
        }
        let (r, _) = ui.allocate_exact_size(vec2(60.0, 52.0), Sense::hover());
        let custom = *value != 0 && !PRESETS.contains(value);
        let cresp = swatch(ui, r, if custom { *value } else { 0x2b2d31 }, custom, (salt, "custom"));
        if !custom {
            icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), Color32::WHITE);
        }
        egui::Popup::menu(&cresp.on_hover_text("Custom color")).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(240.0);
            let mut c = role_color(*value);
            if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                *value = (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32;
                changed = true;
            }
            ui.add_space(8.0);
            let mut h = hex(*value);
            if settings::input(ui, &mut h, "#RRGGBB", false).changed()
                && let Ok(v) = u32::from_str_radix(h.trim_start_matches('#'), 16)
                && h.trim_start_matches('#').len() == 6
            {
                *value = v;
                changed = true;
            }
        });
    });
    ui.add_space(8.0);
    let mut x = 0;
    while x < PRESETS.len() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for c in &PRESETS[x..x + 10] {
                let (r, _) = ui.allocate_exact_size(vec2(36.0, 30.0), Sense::hover());
                if swatch(ui, r, *c, *value == *c, (salt, *c)).on_hover_text(hex(*c)).clicked() {
                    *value = *c;
                    changed = true;
                }
            }
        });
        ui.add_space(4.0);
        x += 10;
    }
    changed
}

fn field(ui: &mut Ui, label: &str, desc: &str) {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary));
    }
    ui.add_space(8.0);
}

fn image_data(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    let mime = if bytes.starts_with(b"GIF") { "image/gif" } else if bytes.starts_with(&[0x89, b'P']) { "image/png" } else { "image/jpeg" };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn display(app: &mut App, ui: &mut Ui, gid: Id, e: &mut Edit, role: &Role) -> bool {
    let mut boost = false;
    let feats = app.store.guilds.get(&gid).map(|g| g.features.clone()).unwrap_or_default();
    let enhanced = feats.iter().any(|f| f == "ENHANCED_ROLE_COLORS");
    let icons_on = feats.iter().any(|f| f == "ROLE_ICONS");
    field(ui, "Role name *", "");
    let focus = std::mem::take(&mut e.focus_name);
    let r = settings::input(ui, &mut e.draft.name, "", false);
    if focus {
        r.request_focus();
    }
    e.draft.name.truncate(100);
    rule(ui);
    field(ui, "Role Style", "");
    let style = e.draft.style();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for (i, label) in ["Solid", "Gradient", "Holographic"].into_iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(((ui.available_width() - 24.0) / 3.0).min(240.0), 96.0), Sense::click());
            let off = i > 0 && !enhanced;
            ui.painter().rect(r, CornerRadius::same(8), pal().bg_secondary, Stroke::new(2.0, if style == i as u8 { BLURPLE } else { faint(51) }), egui::StrokeKind::Inside);
            let mut d = e.draft.clone();
            match i {
                0 => d.grad.clear(),
                1 => d.grad = if style == 1 { d.grad.clone() } else { vec![if d.color != 0 { d.color } else { 0x3498db }, 0xe91e63] },
                _ => d.grad = HOLO.to_vec(),
            }
            let name = if e.draft.name.is_empty() { "role" } else { e.draft.name.as_str() };
            name_preview(&ui.painter().with_clip_rect(r), pos2(r.left() + 14.0, r.top() + 16.0), name, &d, pal().header_primary);
            ui.painter().text(pos2(r.left() + 14.0, r.bottom() - 22.0), Align2::LEFT_CENTER, label, medium(14.0), pal().header_secondary);
            if off {
                ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(90));
            } else if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !off {
                e.draft.grad = d.grad;
                if i == 0 && style != 0 {
                    e.draft.color = role.color;
                }
            }
        }
    });
    if !enhanced {
        ui.add_space(12.0);
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(51))).corner_radius(8).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Make your role stand out with gradient and holographic styles").font(semi(14.0)).color(pal().header_primary));
                    ui.label(RichText::new("Unlock more role styles with Server Boosts.").font(medium(12.0)).color(pal().header_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    boost = settings::primary(ui, "Unlock with Boosting").clicked();
                });
            });
        });
    }
    ui.add_space(24.0);
    if style == 2 {
        ui.label(RichText::new("Holographic roles use fixed colors that shift as members see them.").font(medium(12.0)).color(pal().header_secondary));
    } else {
        field(ui, "Role color *", "Members use the color of the highest role they have on the roles list.");
        if style == 1 {
            ui.label(RichText::new("Start").font(medium(12.0)).color(pal().header_secondary));
            let mut a = e.draft.grad[0];
            if pick_color(ui, "g0", &mut a) {
                e.draft.grad[0] = a;
                e.draft.color = a;
            }
            ui.label(RichText::new("End").font(medium(12.0)).color(pal().header_secondary));
            let mut b = *e.draft.grad.get(1).unwrap_or(&0xe91e63);
            if pick_color(ui, "g1", &mut b) {
                if e.draft.grad.len() < 2 {
                    e.draft.grad.push(b);
                } else {
                    e.draft.grad[1] = b;
                }
            }
        } else {
            pick_color(ui, "solid", &mut e.draft.color);
        }
    }
    ui.add_space(24.0);
    field(ui, "Role icon", "Upload an image under 256kb or pick a custom emoji from this server. We recommend at least 64x64 pixels. Members will see the icon for their highest role if they have multiple roles.");
    let mut choose = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (r, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_secondary);
        match &e.draft.icon {
            IconEdit::Emoji(x) => {
                ui.painter().text(r.center(), Align2::CENTER_CENTER, x, medium(24.0), pal().header_primary);
            }
            IconEdit::Image(_) => icons::draw(ui.painter(), Icon::Image, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), pal().header_primary),
            IconEdit::Keep => match role.icon_url(96) {
                Some(u) => crate::ui::text::image(ui.painter(), &u, r.shrink(8.0)),
                None => icons::draw(ui.painter(), Icon::Shield, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), pal().header_secondary),
            },
            IconEdit::Clear => icons::draw(ui.painter(), Icon::Shield, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), pal().header_secondary),
        }
        choose = settings::primary(ui, "Choose Image").clicked();
        let emoji = settings::secondary(ui, "Choose Emoji");
        egui::Popup::menu(&emoji).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(220.0);
            let t = e.emoji.get_or_insert_with(String::new);
            ui.label(RichText::new("Type or paste an emoji").font(medium(12.0)).color(pal().header_secondary));
            settings::input(ui, t, "\u{1f600}", false);
            if settings::primary(ui, "Use Emoji").clicked() && !t.trim().is_empty() {
                e.draft.icon = IconEdit::Emoji(t.trim().to_string());
            }
        });
        let has = !matches!(e.draft.icon, IconEdit::Clear) && (role.icon.is_some() || role.emoji.is_some() || e.draft.icon != IconEdit::Keep);
        if has && settings::danger(ui, "Remove Icon", false).clicked() {
            e.draft.icon = IconEdit::Clear;
        }
    });
    if !icons_on {
        ui.add_space(8.0);
        ui.label(RichText::new("Custom role icons are unlocked by Server Boosts (Level 2).").font(medium(12.0)).color(pal().header_secondary));
    }
    if choose && icons_on {
        if let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
            && let Some(d) = image_data(&p)
        {
            e.draft.icon = IconEdit::Image(d);
        }
    }
    ui.add_space(24.0);
    ui.label(RichText::new("Preview").font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let name = if e.draft.name.is_empty() { "role".to_string() } else { e.draft.name.clone() };
    for (i, t) in [1usize, 2, 3, 0].into_iter().enumerate() {
        let p = palette(t);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(if i == 0 { 8 } else { 0 }), p.bg_primary);
        ui.painter().circle_filled(pos2(r.left() + 36.0, r.center().y), 20.0, p.bg_accent);
        let n = name_preview(ui.painter(), pos2(r.left() + 68.0, r.top() + 8.0), &name, &e.draft, p.header_primary);
        ui.painter().text(pos2(n.right() + 8.0, n.center().y), Align2::LEFT_CENTER, "Today at 12:00 PM", reg(12.0), p.header_secondary);
        ui.painter().text(pos2(r.left() + 68.0, r.top() + 36.0), Align2::LEFT_CENTER, "rocks are really old", reg(16.0), p.text_normal);
    }
    ui.add_space(8.0);
    ui.label(RichText::new("In light mode, gradient roles are automatically darkened for accessibility.").font(medium(12.0)).color(pal().text_muted));
    rule(ui);
    settings::toggle(ui, &mut e.draft.hoist, "Display role members separately from online members", "");
    ui.add_space(16.0);
    settings::toggle(ui, &mut e.draft.mentionable, "Allow anyone to @mention this role", "Note: members with the \u{201c}Mention @everyone, @here, and All Roles\u{201d} permission will always be able to ping this role.");
    boost
}

fn permissions(_app: &mut App, ui: &mut Ui, e: &mut Edit, role: &Role) {
    let q = e.pq.trim().to_lowercase();
    ui.horizontal(|ui| {
        let w = ui.available_width() - 168.0;
        ui.allocate_ui(vec2(w, 40.0), |ui| {
            settings::input(ui, &mut e.pq, "Search permissions", false);
        });
        if settings::secondary(ui, "Clear permissions").clicked() {
            e.draft.perms = 0;
        }
    });
    ui.add_space(24.0);
    let _ = role;
    let mut any = false;
    for (head, perms) in SECTIONS {
        let rows: Vec<_> = perms.iter().filter(|p| q.is_empty() || p.1.to_lowercase().contains(&q) || p.2.to_lowercase().contains(&q)).collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        ui.label(RichText::new(*head).font(bold(12.0)).color(pal().header_secondary));
        ui.add_space(12.0);
        for (bit, name, desc) in rows {
            let mut on = e.draft.perms & bit != 0;
            if settings::toggle(ui, &mut on, name, desc) {
                if on {
                    e.draft.perms |= bit;
                } else {
                    e.draft.perms &= !bit;
                }
            }
            ui.add_space(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(r, 0, faint(31));
            ui.add_space(16.0);
        }
        ui.add_space(8.0);
    }
    if !any {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No permissions found").font(medium(14.0)).color(pal().header_secondary));
        });
    }
}

fn links(app: &mut App, ui: &mut Ui, gid: Id, e: &mut Edit, role: &Role, members: usize) {
    let key = "role_links";
    let path = format!("/guilds/{gid}/roles/{}/connection-configurations", e.id);
    if app.server.as_ref().is_some_and(|s| !s.data.contains_key(key)) {
        app.server_data(key, path.clone());
    }
    if e.links.is_none()
        && let Some(v) = app.server.as_ref().and_then(|s| s.data.get(key)).filter(|v| !v.is_null())
    {
        let list: Vec<String> = v.as_array().into_iter().flatten().flat_map(|g| g.as_array().into_iter().flatten().chain(std::iter::once(g)).filter_map(|c| c["connection_type"].as_str())).map(str::to_string).collect();
        e.links = Some(list);
    }
    let _ = role;
    let Some(list) = e.links.clone() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    ui.label(RichText::new("CREATE LINKED ROLE").font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    ui.label(RichText::new("Members who connect the accounts below to their Discord profile will be given this role automatically.").font(reg(14.0)).color(pal().text_normal));
    ui.add_space(16.0);
    if members > 0 {
        ui.label(RichText::new("Adding connection or app requirements to an existing role is only possible if the role has no members. Consider creating a new role.").font(reg(14.0)).color(pal().header_secondary));
        ui.add_space(16.0);
    }
    let mut next = list.clone();
    for (i, t) in list.iter().enumerate() {
        let name = PLATFORMS.iter().find(|p| p.0 == t).map_or(t.as_str(), |p| p.1);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
        ui.painter().rect(r, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, format!("Must connect {name}"), medium(16.0), pal().header_primary);
        let x = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(24.0, 24.0));
        icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), crate::ui::motion::mix(pal().header_secondary, RED, crate::ui::motion::hot(ui.ctx(), &resp)));
        if resp.clicked() && members == 0 {
            next.remove(i);
            break;
        }
        ui.add_space(8.0);
    }
    if list.len() > 1 {
        ui.add_space(8.0);
        let mut all = e.links_all;
        settings::toggle(ui, &mut all, "Must connect all", "Members need every connection above. Turn off to give the role to members who connect at least one.");
        e.links_all = all;
        ui.add_space(8.0);
    }
    let add = settings::secondary(ui, "Add requirement");
    egui::Popup::menu(&add).show(|ui| {
        crate::ui::menu_body(ui, |ui| {
            for (id, label) in PLATFORMS {
                if !list.iter().any(|t| t == id) && members == 0 && crate::ui::menu_item(ui, None, label, false) {
                    next.push(id.to_string());
                }
            }
        });
    });
    if next != list {
        let groups: Vec<Value> = if e.links_all || next.len() < 2 {
            vec![Value::Array(next.iter().map(|t| json!({ "connection_type": t })).collect())]
        } else {
            next.iter().map(|t| json!([{ "connection_type": t }])).collect()
        };
        let body = if next.is_empty() { json!([]) } else { Value::Array(groups) };
        app.rest("PUT", path, Some(body), Tag::Quiet);
        e.links = Some(next);
    }
}

fn manage(app: &mut App, ui: &mut Ui, gid: Id, e: &mut Edit, members: &[Id], role: &Role) {
    let rid = e.id;
    let mut add_open = false;
    ui.horizontal(|ui| {
        let w = ui.available_width() - 172.0;
        ui.allocate_ui(vec2(w, 40.0), |ui| {
            settings::input(ui, &mut e.mq, "Search Members", false);
        });
        add_open = ui.add_enabled_ui(!role.managed, |ui| settings::primary(ui, "Add Members")).inner.clicked();
    });
    ui.add_space(16.0);
    if add_open {
        e.add = Some((String::new(), HashSet::new()));
    }
    let q = e.mq.trim().to_lowercase();
    let mut shown: Vec<(Id, String)> = members.iter().map(|u| (*u, app.store.name_in(gid, *u))).filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)).collect();
    shown.sort_by_key(|(_, n)| n.to_lowercase());
    if shown.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No members found. Add members by clicking Add Members.").font(medium(14.0)).color(pal().header_secondary));
        });
    }
    let mut remove = None;
    for (u, name) in &shown {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
        let user = app.store.user(*u).cloned();
        if let Some(us) = &user {
            crate::ui::avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0)), &us.avatar_url());
        }
        let n = ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, name, semi(14.0), pal().header_primary);
        if let Some(us) = &user {
            ui.painter().text(pos2(n.right() + 8.0, r.center().y), Align2::LEFT_CENTER, &us.username, reg(14.0), pal().header_secondary);
        }
        let x = Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(28.0, 28.0));
        let xr = ui.interact(x, ui.id().with(("role_rm", *u)), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x.shrink(6.0), crate::ui::motion::mix(pal().header_secondary, RED, crate::ui::motion::hot(ui.ctx(), &xr)));
        if xr.on_hover_text("Remove member").clicked() {
            remove = Some((*u, name.clone()));
        }
    }
    if let Some((u, name)) = remove {
        let role_name = role.name.clone();
        let go = move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/members/{u}/roles/{rid}"), None, Tag::Quiet);
            if let Some(m) = app.store.members.get_mut(&gid).and_then(|m| m.get_mut(&u)) {
                m.roles.retain(|r| *r != rid);
            }
            if let Some(s) = &mut app.server {
                if let Some(Value::Array(a)) = s.data.get_mut("role_members") {
                    a.retain(|v| sid(v) != u);
                }
                s.data.remove("role_counts");
            }
        };
        if ui.input(|i| i.modifiers.shift) {
            go(app);
        } else {
            app.confirm = Some(crate::ui::Confirm::new("Remove member", format!("Are you sure you want to remove **{name}** from the role **{role_name}**? Hold shift when removing members to skip this modal."), "Remove", go));
        }
    }
    add_modal(app, ui, gid, e, members, role);
}

fn add_modal(app: &mut App, ui: &mut Ui, gid: Id, e: &mut Edit, have: &[Id], role: &Role) {
    let Some((q, picked)) = e.add.as_mut() else { return };
    let mut done = false;
    let mut submit = false;
    let m = crate::ui::motion::Modal::new(egui::Id::new("role_add_members")).show(ui.ctx(), |ui| {
        ui.set_width(440.0);
        ui.label(RichText::new("Add Members").font(bold(20.0)).color(pal().header_primary));
        ui.label(RichText::new(role.name.clone()).font(reg(14.0)).color(pal().header_secondary));
        ui.add_space(16.0);
        settings::input(ui, q, "Search members", false);
        ui.add_space(8.0);
        let ql = q.trim().to_lowercase();
        let mut list: Vec<(Id, String)> = app.store.members.get(&gid).map(|m| m.keys().copied().collect::<Vec<_>>()).unwrap_or_default().into_iter().filter(|u| !have.contains(u)).map(|u| (u, app.store.name_in(gid, u))).filter(|(_, n)| ql.is_empty() || n.to_lowercase().contains(&ql)).collect();
        list.sort_by_key(|(_, n)| n.to_lowercase());
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (u, name) in list.into_iter().take(200) {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
                let on = picked.contains(&u);
                let cb = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(20.0, 20.0));
                if on {
                    ui.painter().rect_filled(cb, 4, BLURPLE);
                    icons::draw(ui.painter(), Icon::Check, cb.shrink(2.0), Color32::WHITE);
                } else {
                    ui.painter().rect_stroke(cb, 4, Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                }
                if let Some(us) = app.store.user(u) {
                    crate::ui::avatar(ui, Rect::from_center_size(pos2(r.left() + 56.0, r.center().y), vec2(24.0, 24.0)), &us.avatar_url());
                }
                ui.painter().text(pos2(r.left() + 76.0, r.center().y), Align2::LEFT_CENTER, name, medium(14.0), pal().header_primary);
                if resp.clicked() && !picked.remove(&u) {
                    picked.insert(u);
                }
            }
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if settings::primary(ui, &format!("Add ({})", picked.len())).clicked() && !picked.is_empty() {
                submit = true;
                done = true;
            }
            if settings::secondary(ui, "Cancel").clicked() {
                done = true;
            }
        });
    });
    if m.should_close() || ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {

        done = true;
    }
    if submit {
        let ids: Vec<Id> = picked.iter().copied().collect();
        let rid = e.id;
        let body = json!({ "member_ids": ids.iter().map(|u| u.to_string()).collect::<Vec<_>>() });
        app.rest("PATCH", format!("/guilds/{gid}/roles/{rid}/members"), Some(body), Tag::Quiet);
        for u in &ids {
            if let Some(m) = app.store.members.get_mut(&gid).and_then(|m| m.get_mut(u)) {
                m.roles.push(rid);
            }
        }
        if let Some(s) = &mut app.server {
            if let Some(Value::Array(a)) = s.data.get_mut("role_members") {
                a.extend(ids.iter().map(|u| json!(u.to_string())));
            }
            s.data.remove("role_counts");
        }
    }
    if done {
        e.add = None;
    }
}

/// The scripted run's way through the editor: true once `tab` is showing and, on Display, the edit it made was saved.
pub fn probe(app: &mut App, gid: Id, tab: u8) -> bool {
    let open = app.server.as_ref().and_then(|s| s.pages.roles.edit.as_ref()).map(|e| e.id);
    if open != Some(1100) {
        super::roles::open(app, gid, 1100, tab);
        return false;
    }
    let Some(mut e) = app.server.as_mut().and_then(|s| s.pages.roles.edit.take()) else { return false };
    let mut ready = false;
    if e.tab != tab {
        e.tab = tab;
    } else if tab == 0 && e.base.name != "Admins" && e.draft == e.base {
        e.draft.name = "Admins".into();
        e.draft.hoist = !e.base.hoist;
        e.draft.perms ^= 1 << 4;
    } else if e.draft != e.base && !e.saving {
        save_role(app, gid, &mut e);
    } else {
        let members = app.server.as_ref().is_some_and(|s| s.data.get("role_members").is_some_and(|v| !v.is_null()));
        ready = e.draft == e.base && !e.saving && (tab != 0 || e.base.name == "Admins") && members;
    }
    if let Some(s) = &mut app.server {
        s.pages.roles.edit = Some(e);
    }
    ready
}
