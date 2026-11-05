//! Server Settings, the full-window layer discord opens from the server menu: the profile, emoji,
//! stickers, members, roles, invites, bans and audit log. Pages rustcord doesn't draw yet say so
//! and offer discord.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

mod apps;
mod automod;
mod bar;
mod expression;
mod invites;
mod members;
mod moderation;
mod perms;
mod role_edit;
mod roles;
mod safety;

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, snowflake_ms};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Profile,
    Tag,
    Engagement,
    Perks,
    Emoji,
    Stickers,
    Soundboard,
    Members,
    Roles,
    Invites,
    Access,
    Integrations,
    Directory,
    Safety,
    Audit,
    Bans,
    AutoMod,
    Community,
}

const SIDE: &[(&str, &[(Page, &str)])] = &[
    ("", &[(Page::Profile, "Server Profile"), (Page::Tag, "Server Tag"), (Page::Engagement, "Engagement"), (Page::Perks, "Boost Perks")]),
    ("EXPRESSION", &[(Page::Emoji, "Emoji"), (Page::Stickers, "Stickers"), (Page::Soundboard, "Soundboard")]),
    ("PEOPLE", &[(Page::Members, "Members"), (Page::Roles, "Roles"), (Page::Invites, "Invites"), (Page::Access, "Access")]),
    ("APPS", &[(Page::Integrations, "Integrations"), (Page::Directory, "App Directory")]),
    ("MODERATION", &[(Page::Safety, "Safety Setup"), (Page::Audit, "Audit Log"), (Page::Bans, "Bans"), (Page::AutoMod, "AutoMod")]),
];

pub struct State {
    pub guild: Id,
    pub page: Page,
    pub name: String,
    pub search: String,
    pub data: HashMap<&'static str, Value>,
    pub pages: Pages,
}

#[derive(Default)]
pub struct Pages {
    pub roles: roles::Roles,
    pub members: super::community::Members,
    pub emoji: expression::Expression,
    pub apps: apps::Apps,
    pub safety: safety::Safety,
    pub automod: automod::AutoMod,
    pub log: moderation::Log,
}

impl App {
    pub fn open_server_settings(&mut self, gid: Id) {
        let name = self.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
        self.server = Some(State { guild: gid, page: Page::Profile, name, search: String::new(), data: HashMap::new(), pages: Pages::default() });
    }

    /// A page's data, asked for the first time it's drawn.
    fn server_data(&mut self, key: &'static str, path: String) -> Option<&Value> {
        let s = self.server.as_mut()?;
        if !s.data.contains_key(key) {
            s.data.insert(key, Value::Null);
            self.rest("GET", path, None, Tag::Server(key));
        }
        self.server.as_ref()?.data.get(key).filter(|v| !v.is_null())
    }
}

pub fn loaded(app: &mut App, key: &'static str, v: Value) {
    match key {
        "role_new" | "role_saved" => return roles::saved(app, key, v),
        "members" => return members::loaded(app, v),
        "audit_more" => return moderation::more(app, v),
        "automod_saved" => return automod::saved(app, v),
        "webhook_new" => return apps::created(app, v),
        _ => {}
    }
    if let Some(s) = &mut app.server {
        s.data.insert(key, v);
    }
}

/// Files dropped on the window go to the Emoji page when it's the one open.
pub fn dropped(app: &mut App, path: &std::path::Path) -> bool {
    match &mut app.server {
        Some(s) if s.page == Page::Emoji => {
            s.pages.emoji.dropped.push(path.to_path_buf());
            true
        }
        _ => false,
    }
}

pub enum Probe {
    Wait,
    Shot(&'static str),
    Done,
}

/// One step of the scripted run through the pages: the role editor, then each list page once its data is in.
pub fn probe(app: &mut App, n: u8) -> Probe {
    let Some(gid) = app.server.as_ref().map(|s| s.guild) else { return Probe::Done };
    let tour: [(Page, &[&str], &str); 8] = [
        (Page::Emoji, &["emojis"], "server_emoji.png"),
        (Page::Stickers, &["stickers"], "server_stickers.png"),
        (Page::Invites, &["invites"], "server_invites.png"),
        (Page::Integrations, &["integrations", "webhooks"], "server_integrations.png"),
        (Page::Safety, &["guild"], "server_safety.png"),
        (Page::Audit, &["audit"], "server_audit.png"),
        (Page::Bans, &["bans"], "server_bans.png"),
        (Page::AutoMod, &["automod"], "server_automod.png"),
    ];
    match n {
        0 | 1 => {
            if app.server.as_ref().is_some_and(|s| s.page != Page::Roles) {
                app.server.as_mut().unwrap().page = Page::Roles;
                return Probe::Wait;
            }
            if role_edit::probe(app, gid, n) { Probe::Shot(if n == 0 { "server_role_display.png" } else { "server_role_permissions.png" }) } else { Probe::Wait }
        }
        2 => {
            let s = app.server.as_mut().unwrap();
            if s.page != Page::Members {
                s.page = Page::Members;
                return Probe::Wait;
            }
            if s.pages.members.list.is_some() { Probe::Shot("server_members.png") } else { Probe::Wait }
        }
        3..=10 => {
            let (page, keys, shot) = tour[n as usize - 3];
            let s = app.server.as_mut().unwrap();
            if s.page != page {
                s.page = page;
                return Probe::Wait;
            }
            if keys.iter().all(|k| s.data.get(k).is_some_and(|v| !v.is_null())) { Probe::Shot(shot) } else { Probe::Wait }
        }
        _ => Probe::Done,
    }
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(st) = app.server.as_ref() else { return };
    let gid = st.guild;
    let page = st.page;
    let modal = st.pages.roles.edit.as_ref().is_some_and(|e| e.adding()) || st.pages.emoji.editing();
    if !modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) && app.confirm.is_none() {
        app.server = None;
        return;
    }
    let screen = ctx.content_rect();
    let area = Rect::from_min_max(pos2(screen.left(), screen.top() + 32.0), screen.max);
    let mut goto = None;
    let mut close = false;
    egui::Area::new("server_settings".into()).order(egui::Order::Foreground).fixed_pos(area.min).show(ctx, |ui| {
        ui.set_min_size(area.size());
        ui.painter().rect_filled(area, 0, pal().bg_primary);
        // discord's layer: the sidebar ends a little left of the middle, the page starts just after it
        let side_right = area.center().x - 280.0;
        let content_left = area.center().x - 228.0;
        let side = Rect::from_min_max(pos2(area.left(), area.top()), pos2(side_right + 6.0, area.bottom()));
        ui.painter().rect_filled(side, 0, pal().bg_secondary_alt);
        let mut sc = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(side_right - 239.0, area.top()), pos2(side_right, area.bottom()))));
        egui::ScrollArea::vertical().id_salt("server_side").auto_shrink([false, false]).show(&mut sc, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(46.0);
            let gname = app.store.guilds.get(&gid).map(|g| g.name.to_uppercase()).unwrap_or_default();
            for (i, (head, items)) in SIDE.iter().enumerate() {
                if i > 0 {
                    ui.add_space(8.0);
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
                    ui.add_space(8.0);
                }
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                let h = if head.is_empty() { gname.as_str() } else { *head };
                super::text::paint(ui.painter(), pos2(r.left() + 10.0, r.center().y + 4.0), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary, r.width() - 20.0);
                for (p, label) in items.iter() {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    let on = *p == page;
                    if on {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                    }
                    ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, *label, medium(16.0), if on || resp.hovered() { pal().header_primary } else { pal().header_secondary });
                    if *p == Page::Directory {
                        icons::draw(ui.painter(), Icon::ArrowRight, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        goto = Some(*p);
                    }
                }
            }
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
            ui.add_space(8.0);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            if page == Page::Community {
                ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
            }
            ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "Enable Community", medium(16.0), pal().header_secondary);
            if resp.clicked() {
                goto = Some(Page::Community);
            }
            ui.add_space(60.0);
        });
        // the round ESC button off to the right
        let esc = Rect::from_center_size(pos2(area.right() - 56.0, area.top() + 78.0 - 32.0 + 18.0), vec2(36.0, 36.0));
        let er = ui.interact(esc, ui.id().with("esc"), Sense::click());
        ui.painter().circle_stroke(esc.center(), 17.0, Stroke::new(2.0, if er.hovered() { pal().header_primary } else { pal().header_secondary }));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(esc.center(), vec2(18.0, 18.0)), if er.hovered() { pal().header_primary } else { pal().header_secondary });
        ui.painter().text(pos2(esc.center().x, esc.bottom() + 14.0), Align2::CENTER_CENTER, "ESC", semi(13.0), pal().header_secondary);
        close = er.clicked();
        let body = Rect::from_min_max(pos2(content_left, area.top()), pos2((content_left + 900.0).min(esc.left() - 40.0), area.bottom()));
        let mut bc = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical().id_salt(("server_page", page as u8)).auto_shrink([false, false]).show(&mut bc, |ui| {
            Frame::new().inner_margin(Margin { left: 0, right: 16, top: 46, bottom: 80 }).show(ui, |ui| match page {
                Page::Profile => profile(app, ui, gid),
                Page::Emoji => expression::emoji(app, ui, gid),
                Page::Stickers => expression::stickers(app, ui, gid),
                Page::Members => members::page(app, ui, gid),
                Page::Roles => roles::page(app, ui, gid),
                Page::Invites => invites::page(app, ui, gid),
                Page::Integrations => apps::page(app, ui, gid),
                Page::Safety => safety::page(app, ui, gid),
                Page::Bans => moderation::bans(app, ui, gid),
                Page::Audit => moderation::audit(app, ui, gid),
                Page::AutoMod => automod::page(app, ui, gid),
                _ => elsewhere(ui, gid, page),
            });
        });
    });
    if close {
        app.server = None;
    }
    match goto {
        Some(Page::Perks) => {
            app.server = None;
            app.ui.guild = Some(gid);
            app.open_boosts(gid);
        }
        Some(Page::Directory) => {
            app.server = None;
            app.open_directory();
        }
        Some(p) => {
            if let Some(s) = &mut app.server {
                s.page = p;
                s.search.clear();
            }
        }
        None => {}
    }
}

fn title(ui: &mut Ui, t: &str, desc: &str) {
    ui.label(RichText::new(t).font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(desc).font(medium(14.0)).line_height(Some(18.0)).color(pal().text_normal));
    }
    ui.add_space(24.0);
}

fn rule(ui: &mut Ui) {
    ui.add_space(24.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    ui.add_space(24.0);
}

/// A row of a list page: a hairline under it, hover wash, whatever `draw` puts in it.
fn row(ui: &mut Ui, h: f32, draw: impl FnOnce(&Ui, Rect, bool)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
    }
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(31)));
    draw(ui, r, resp.hovered());
    resp
}

fn profile(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    title(ui, "Server Profile", "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages");
    let mut save = None;
    let mut pick = false;
    let mut remove = false;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 40.0;
        let w = (ui.available_width() - 340.0).max(320.0);
        ui.allocate_ui(vec2(w, 600.0), |ui| {
            ui.set_width(w);
            ui.vertical(|ui| {
                super::settings::h5(ui, "Name");
                let s = app.server.as_mut().unwrap();
                super::settings::input(ui, &mut s.name, "", false);
                if s.name.trim() != g.name && !s.name.trim().is_empty() {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if super::settings::primary(ui, "Save Changes").clicked() {
                            save = Some(s.name.trim().to_string());
                        }
                        if super::settings::link(ui, "Reset").clicked() {
                            s.name = g.name.clone();
                        }
                    });
                }
                rule(ui);
                super::settings::h5(ui, "Icon");
                ui.label(RichText::new("We recommend an image of at least 512x512.").font(reg(14.0)).color(pal().header_secondary));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    pick = super::settings::primary(ui, "Change Server Icon").clicked();
                    if g.icon.is_some() {
                        remove = super::settings::danger(ui, "Remove Icon", false).clicked();
                    }
                });
            });
        });
        // the invite card discord previews on the right
        Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51))).show(ui, |ui| {
            ui.set_width(300.0);
            let (top, _) = ui.allocate_exact_size(vec2(300.0, 120.0), Sense::hover());
            let mut m = egui::Mesh::default();
            m.colored_vertex(top.left_top(), Color32::from_rgb(0x1c, 0x1c, 0x1f));
            m.colored_vertex(top.right_top(), Color32::from_rgb(0x1c, 0x1c, 0x1f));
            m.colored_vertex(top.right_bottom(), Color32::from_rgb(0x48, 0x48, 0x4c));
            m.colored_vertex(top.left_bottom(), Color32::from_rgb(0x48, 0x48, 0x4c));
            m.add_triangle(0, 1, 2);
            m.add_triangle(0, 2, 3);
            ui.painter().add(m);
            Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                let ir = Rect::from_min_size(pos2(ui.max_rect().left(), top.bottom() - 40.0), vec2(80.0, 80.0));
                ui.painter().rect_filled(ir.expand(4.0), CornerRadius::same(24), pal().bg_secondary);
                match g.icon_url() {
                    Some(u) => {
                        egui::Image::from_uri(u.replace("size=96", "size=160")).corner_radius(CornerRadius::same(20)).paint_at(ui, ir);
                    }
                    None => {
                        ui.painter().rect_filled(ir, CornerRadius::same(20), pal().bg_accent);
                    }
                }
                ui.add_space(48.0);
                ui.label(RichText::new(&g.name).font(bold(20.0)).color(pal().header_primary));
                ui.add_space(4.0);
                let online = g.list.iter().filter(|i| matches!(i, crate::state::ListItem::Member(_))).count();
                ui.label(RichText::new(format!("\u{25cf} {online} Online   \u{25cf} {} Members", g.member_count)).font(reg(14.0)).color(pal().header_secondary));
                let est = chrono::DateTime::from_timestamp_millis(snowflake_ms(gid)).map(|t| t.format("Est. %b %Y").to_string()).unwrap_or_default();
                ui.label(RichText::new(est).font(reg(14.0)).color(pal().header_secondary));
            });
        });
    });
    if let Some(n) = save {
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "name": n })), Tag::Quiet);
    }
    if remove {
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "icon": null })), Tag::Quiet);
    }
    if pick && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
        if let Ok(bytes) = std::fs::read(&path) {
            use base64::Engine;
            let mime = match bytes.get(..4) {
                Some([0x89, b'P', ..]) => "image/png",
                Some([b'G', b'I', ..]) => "image/gif",
                Some([b'R', b'I', ..]) => "image/webp",
                _ => "image/jpeg",
            };
            let icon = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
            app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "icon": icon })), Tag::Quiet);
        }
    }
}

fn search(ui: &mut Ui, app: &mut App, hint: &str) -> String {
    let s = app.server.as_mut().unwrap();
    super::settings::input(ui, &mut s.search, hint, false);
    ui.add_space(16.0);
    s.search.trim().to_lowercase()
}

fn elsewhere(ui: &mut Ui, gid: Id, page: Page) {
    let name = SIDE.iter().flat_map(|(_, i)| i.iter()).find(|(p, _)| *p == page).map(|(_, n)| *n).unwrap_or("Enable Community");
    title(ui, name, "");
    ui.label(RichText::new("Rustcord doesn't have this page yet. Discord's own client can change it.").font(reg(16.0)).color(pal().text_normal));
    ui.add_space(16.0);
    if super::settings::secondary(ui, "Open in Discord").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{gid}")));
    }
}
