//! Server Settings, the full-window layer discord opens from the server menu: the profile, emoji,
//! stickers, members, roles, invites, bans and audit log. Pages rustcord doesn't draw yet say so
//! and offer discord.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, sid, snowflake_ms};
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
}

impl App {
    pub fn open_server_settings(&mut self, gid: Id) {
        let name = self.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
        self.server = Some(State { guild: gid, page: Page::Profile, name, search: String::new(), data: HashMap::new() });
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
    if let Some(s) = &mut app.server {
        s.data.insert(key, v);
    }
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(st) = app.server.as_ref() else { return };
    let gid = st.guild;
    let page = st.page;
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) && app.confirm.is_none() {
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
                Page::Emoji => emoji(app, ui, gid),
                Page::Stickers => stickers(app, ui, gid),
                Page::Members => members(app, ui, gid),
                Page::Roles => roles(app, ui, gid),
                Page::Invites => invites(app, ui, gid),
                Page::Bans => bans(app, ui, gid),
                Page::Audit => audit(app, ui, gid),
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
            super::crop::open(app, bytes, 1.0, move |app: &mut App, icon: String| {
                app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "icon": icon })), Tag::Quiet);
            });
        }
    }
}

fn emoji(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let slots: usize = match g.tier {
        1 => 100,
        2 => 150,
        3 => 250,
        _ => 50,
    };
    title(ui, "Emoji", &format!("Add up to {slots} custom emoji that anyone can use in this server. Animated GIF emoji may be used by members with Discord Nitro."));
    let upload = super::settings::primary(ui, "Upload Emoji").clicked();
    rule(ui);
    let (stat, anim): (Vec<_>, Vec<_>) = g.emojis.iter().partition(|e| !e.animated);
    let mut delete = None;
    for (head, list) in [("Emoji", stat), ("Animated Emoji", anim)] {
        ui.label(RichText::new(format!("{head} \u{2014} {} slots available", slots.saturating_sub(list.len()))).font(bold(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        for e in list {
            let resp = row(ui, 48.0, |ui, r, hot| {
                let ir = Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 16.0), vec2(32.0, 32.0));
                egui::Image::from_uri(crate::markdown::emoji_url(e.id, e.animated)).show_loading_spinner(false).paint_at(ui, ir);
                ui.painter().text(pos2(r.left() + 56.0, r.center().y), Align2::LEFT_CENTER, format!(":{}:", e.name), medium(16.0), pal().text_normal);
                if hot {
                    icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), RED);
                }
            });
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x > resp.rect.right() - 40.0) {
                delete = Some((e.id, e.name.clone()));
            }
        }
        ui.add_space(24.0);
    }
    if let Some((id, name)) = delete {
        app.confirm = Some(super::Confirm::new("Delete Emoji", format!("Are you sure you want to delete :{name}:?"), "Delete", move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/emojis/{id}"), None, Tag::Quiet)
        }));
    }
    if upload && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
        if let Ok(bytes) = std::fs::read(&path) {
            use base64::Engine;
            let mime = if bytes.starts_with(b"GIF") { "image/gif" } else if bytes.starts_with(&[0x89, b'P']) { "image/png" } else { "image/jpeg" };
            // discord names a fresh emoji after its file, letters, digits and underscores only
            let name: String = path.file_stem().map(|s| s.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()).unwrap_or_else(|| "emoji".into());
            let image = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
            app.rest("POST", format!("/guilds/{gid}/emojis"), Some(json!({ "name": name, "image": image })), Tag::Quiet);
        }
    }
}

fn stickers(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Stickers", "Stickers can be used by anyone in this server.");
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let w = ui.available_width();
    let per = ((w + 16.0) / 136.0).floor().max(1.0) as usize;
    for chunk in g.stickers.chunks(per) {
        let (r, _) = ui.allocate_exact_size(vec2(w, 150.0), Sense::hover());
        for (i, (id, name, fmt)) in chunk.iter().enumerate() {
            let t = Rect::from_min_size(pos2(r.left() + i as f32 * 136.0, r.top()), vec2(120.0, 140.0));
            ui.painter().rect_filled(t, CornerRadius::same(8), pal().bg_secondary);
            if *fmt != 3 {
                let ext = if *fmt == 4 { "gif" } else { "png" };
                egui::Image::from_uri(format!("https://media.discordapp.net/stickers/{id}.{ext}?size=160")).show_loading_spinner(false).paint_at(ui, Rect::from_min_size(t.min + vec2(20.0, 12.0), vec2(80.0, 80.0)));
            }
            super::text::paint(ui.painter(), pos2(t.center().x, t.bottom() - 22.0), Align2::CENTER_CENTER, name, semi(14.0), pal().header_primary, 108.0);
        }
    }
    if g.stickers.is_empty() {
        ui.label(RichText::new("No stickers yet.").font(reg(14.0)).color(pal().header_secondary));
    }
}

fn search(ui: &mut Ui, app: &mut App, hint: &str) -> String {
    let s = app.server.as_mut().unwrap();
    super::settings::input(ui, &mut s.search, hint, false);
    ui.add_space(16.0);
    s.search.trim().to_lowercase()
}

fn members(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Server Members", "");
    let q = search(ui, app, "Search by username or ID");
    let s = &app.store;
    let mut list: Vec<Id> = s.members.get(&gid).map(|m| m.keys().copied().collect()).unwrap_or_default();
    list.sort_by_key(|u| s.name_in(gid, *u).to_lowercase());
    let g = s.guilds.get(&gid);
    for uid in list.into_iter().filter(|u| q.is_empty() || s.name_in(gid, *u).to_lowercase().contains(&q) || u.to_string() == q).take(500) {
        let name = s.name_in(gid, uid);
        let user = s.user(uid).cloned();
        let roles: Vec<(String, Color32)> = s
            .members
            .get(&gid)
            .and_then(|m| m.get(&uid))
            .map(|m| m.roles.iter().filter_map(|r| g.and_then(|g| g.roles.get(r))).map(|r| (r.name.clone(), if r.color != 0 { role_color(r.color) } else { pal().header_secondary })).collect())
            .unwrap_or_default();
        row(ui, 56.0, |ui, r, _| {
            if let Some(u) = &user {
                super::avatar(ui, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 16.0), vec2(32.0, 32.0)), &u.avatar_url());
            }
            let n = super::text::paint_em(ui.painter(), pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, 260.0, 1.0);
            if let Some(u) = &user {
                ui.painter().text(pos2(r.left() + 52.0, r.center().y + 11.0), Align2::LEFT_CENTER, &u.username, reg(12.0), pal().header_secondary);
            }
            let mut x = n.right().max(r.left() + 320.0);
            for (rn, c) in roles.iter().take(3) {
                let g = ui.painter().layout_no_wrap(rn.clone(), medium(12.0), pal().text_normal);
                let chip = Rect::from_min_size(pos2(x, r.center().y - 11.0), vec2(g.size().x + 26.0, 22.0));
                if chip.right() > r.right() {
                    break;
                }
                ui.painter().rect_filled(chip, CornerRadius::same(4), faint(31));
                ui.painter().circle_filled(pos2(chip.left() + 10.0, chip.center().y), 5.0, *c);
                ui.painter().galley(pos2(chip.left() + 20.0, chip.center().y - g.size().y / 2.0), g, pal().text_normal);
                x = chip.right() + 4.0;
            }
        });
    }
}

fn roles(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Roles", "Use roles to group your server members and assign permissions.");
    let counts: HashMap<String, u64> = app
        .server_data("role_counts", format!("/guilds/{gid}/roles/member-counts"))
        .and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0))).collect())
        .unwrap_or_default();
    let Some(g) = app.store.guilds.get(&gid) else { return };
    let mut roles: Vec<&crate::state::Role> = g.roles.values().filter(|r| r.id != gid).collect();
    roles.sort_by_key(|r| std::cmp::Reverse(r.position));
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, format!("ROLES \u{2014} {}", roles.len()), bold(12.0), pal().header_secondary);
    ui.painter().text(pos2(r.right() - 120.0, r.center().y), Align2::LEFT_CENTER, "MEMBERS", bold(12.0), pal().header_secondary);
    for role in roles {
        let n = counts.get(&role.id.to_string()).copied().unwrap_or(0);
        row(ui, 48.0, |ui, r, _| {
            let c = if role.color != 0 { role_color(role.color) } else { pal().header_secondary };
            let mut x = r.left() + 8.0;
            if let Some(u) = role.icon_url(32) {
                super::text::image(ui.painter(), &u, Rect::from_min_size(pos2(x, r.center().y - 10.0), vec2(20.0, 20.0)));
            } else {
                icons::draw(ui.painter(), Icon::Shield, Rect::from_min_size(pos2(x, r.center().y - 10.0), vec2(20.0, 20.0)), c);
            }
            x += 28.0;
            super::text::paint_em(ui.painter(), pos2(x, r.center().y), Align2::LEFT_CENTER, &role.name, medium(16.0), c, r.width() - 200.0, 1.0);
            ui.painter().text(pos2(r.right() - 120.0, r.center().y), Align2::LEFT_CENTER, n.to_string(), medium(16.0), pal().header_secondary);
            icons::draw(ui.painter(), Icon::People, Rect::from_min_size(pos2(r.right() - 120.0 + 24.0, r.center().y - 8.0), vec2(16.0, 16.0)), pal().header_secondary);
        });
    }
}

fn invites(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Invites", "Here's a list of all active invite links. You can revoke any one or create one.");
    let list: Vec<Value> = app.server_data("invites", format!("/guilds/{gid}/invites")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    for (x, h) in [(8.0, "INVITER"), (0.45, "INVITE CODE"), (0.7, "USES"), (0.85, "EXPIRES")] {
        let px = if x > 1.0 { r.left() + x } else { r.left() + r.width() * x };
        ui.painter().text(pos2(px, r.center().y), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary);
    }
    let mut revoke = None;
    for inv in &list {
        let code = inv["code"].as_str().unwrap_or_default().to_string();
        let who = inv["inviter"]["global_name"].as_str().or(inv["inviter"]["username"].as_str()).unwrap_or("Unknown").to_string();
        let avatar = inv["inviter"]["avatar"].as_str().map(|h| format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), inv["inviter"]["id"].as_str().unwrap_or_default()));
        let uses = match inv["max_uses"].as_u64().unwrap_or(0) {
            0 => inv["uses"].as_u64().unwrap_or(0).to_string(),
            m => format!("{}/{m}", inv["uses"].as_u64().unwrap_or(0)),
        };
        let expires = inv["expires_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| {
            let s = (t.timestamp() - chrono::Utc::now().timestamp()).max(0);
            format!("{:02}:{:02}:{:02}:{:02}", s / 86400, s / 3600 % 24, s / 60 % 60, s % 60)
        });
        let resp = row(ui, 52.0, |ui, r, hot| {
            if let Some(a) = &avatar {
                super::avatar(ui, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 12.0), vec2(24.0, 24.0)), a);
            }
            ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &who, medium(16.0), pal().header_primary);
            ui.painter().text(pos2(r.left() + r.width() * 0.45, r.center().y), Align2::LEFT_CENTER, &code, medium(16.0), pal().text_normal);
            ui.painter().text(pos2(r.left() + r.width() * 0.7, r.center().y), Align2::LEFT_CENTER, &uses, medium(16.0), pal().text_normal);
            ui.painter().text(pos2(r.left() + r.width() * 0.85, r.center().y), Align2::LEFT_CENTER, expires.clone().unwrap_or_else(|| "\u{221e}".into()), medium(16.0), pal().text_normal);
            if hot {
                let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
                ui.painter().circle_filled(x.center(), 10.0, RED);
                icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), Color32::WHITE);
            }
        });
        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x > resp.rect.right() - 32.0) {
            revoke = Some(code);
        }
    }
    if list.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("No invites yet.").font(reg(14.0)).color(pal().header_secondary));
    }
    if let Some(c) = revoke {
        app.rest("DELETE", format!("/invites/{c}"), None, Tag::Quiet);
        if let Some(s) = &mut app.server {
            s.data.remove("invites");
        }
    }
}

fn bans(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Bans", "Bans by default are by account and IP. A user can circumvent an IP ban by using a proxy.");
    let q = search(ui, app, "Search Bans by User Id or Username");
    let list: Vec<Value> = app.server_data("bans", format!("/guilds/{gid}/bans?limit=1000")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut pick = None;
    for b in list.iter().filter(|b| q.is_empty() || b["user"]["username"].as_str().is_some_and(|n| n.to_lowercase().contains(&q)) || b["user"]["id"].as_str() == Some(q.as_str())) {
        let u = &b["user"];
        let id = sid(&u["id"]);
        let name = u["global_name"].as_str().or(u["username"].as_str()).unwrap_or_default().to_string();
        let avatar = u["avatar"].as_str().map(|h| format!("{}/avatars/{id}/{h}.png?size=64", crate::api::cdn()));
        let resp = row(ui, 48.0, |ui, r, _| {
            if let Some(a) = &avatar {
                super::avatar(ui, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 16.0), vec2(32.0, 32.0)), a);
            }
            ui.painter().text(pos2(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary);
        });
        if resp.clicked() {
            pick = Some((id, name, b["reason"].as_str().unwrap_or("No reason provided.").to_string()));
        }
    }
    if list.is_empty() {
        ui.label(RichText::new("No bans. Who's been good? You've been good.").font(reg(14.0)).color(pal().header_secondary));
    }
    if let Some((id, name, reason)) = pick {
        app.confirm = Some(super::Confirm::new(format!("'{name}'"), format!("Ban reason: {reason}"), "Revoke Ban", move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/bans/{id}"), None, Tag::Quiet);
            if let Some(s) = &mut app.server {
                s.data.remove("bans");
            }
        }));
    }
}

// what discord's audit log says each action number did
fn action(n: u64) -> &'static str {
    match n {
        1 => "updated the server",
        10 => "created a channel",
        11 => "updated a channel",
        12 => "deleted a channel",
        13..=15 => "changed channel permissions",
        20 => "kicked",
        21 => "pruned members",
        22 => "banned",
        23 => "unbanned",
        24 => "updated",
        25 => "updated roles for",
        26 => "moved a member",
        27 => "disconnected a member",
        28 => "added a bot",
        30 => "created a role",
        31 => "updated a role",
        32 => "deleted a role",
        40 => "created an invite",
        41 => "updated an invite",
        42 => "deleted an invite",
        50..=52 => "changed a webhook",
        60 => "created an emoji",
        61 => "updated an emoji",
        62 => "deleted an emoji",
        72 => "deleted a message",
        73 => "deleted messages",
        74 => "pinned a message",
        75 => "unpinned a message",
        80..=82 => "changed an integration",
        90..=92 => "changed a sticker",
        100..=102 => "changed an event",
        110..=112 => "changed a thread",
        140..=145 => "changed AutoMod",
        _ => "made a change",
    }
}

fn audit(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Audit Log", "");
    let log = app.server_data("audit", format!("/guilds/{gid}/audit-logs?limit=50")).cloned().unwrap_or(Value::Null);
    let users: HashMap<String, String> = log["users"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| (u["id"].as_str().unwrap_or_default().to_string(), u["global_name"].as_str().or(u["username"].as_str()).unwrap_or_default().to_string()))
        .collect();
    for e in log["audit_log_entries"].as_array().into_iter().flatten() {
        let who = users.get(e["user_id"].as_str().unwrap_or_default()).cloned().unwrap_or_else(|| "Someone".into());
        let target = e["target_id"].as_str().and_then(|t| users.get(t)).cloned();
        let what = action(e["action_type"].as_u64().unwrap_or(0));
        let when = chrono::DateTime::from_timestamp_millis(snowflake_ms(sid(&e["id"]))).map(|t| t.with_timezone(&chrono::Local).format("%-m/%-d/%y, %-I:%M %p").to_string()).unwrap_or_default();
        row(ui, 56.0, |ui, r, _| {
            let p = ui.painter();
            icons::draw(p, Icon::Pencil, Rect::from_min_size(pos2(r.left() + 12.0, r.center().y - 10.0), vec2(20.0, 20.0)), pal().header_secondary);
            let t = p.text(pos2(r.left() + 48.0, r.center().y - 9.0), Align2::LEFT_CENTER, &who, semi(16.0), pal().header_primary);
            let t = p.text(pos2(t.right() + 4.0, r.center().y - 9.0), Align2::LEFT_CENTER, what, reg(16.0), pal().text_normal);
            if let Some(tg) = &target {
                p.text(pos2(t.right() + 4.0, r.center().y - 9.0), Align2::LEFT_CENTER, tg, semi(16.0), pal().header_primary);
            }
            p.text(pos2(r.left() + 48.0, r.center().y + 11.0), Align2::LEFT_CENTER, &when, reg(12.0), pal().header_secondary);
        });
    }
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
