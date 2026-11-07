//! Server Settings, the full-window layer discord opens from the server menu: the profile, tag,
//! engagement, perks, emoji, stickers, soundboard, members, roles, invites, access, bans, audit
//! log, community and template.

mod access;
mod community;
mod discovery;
mod engage;
mod form;
mod perks;
mod profile;
mod sound;
mod tag;
mod template;

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;
use std::collections::HashMap;

mod apps;
mod automod;
mod bar;
mod expression;
mod invites;
mod members;
mod moderation;
pub(super) mod perms;
mod role_edit;
mod roles;
mod safety;

use super::App;
use super::sidebar_view;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{ADMINISTRATOR, BAN_MEMBERS, CREATE_GUILD_EXPRESSIONS, Id, KICK_MEMBERS, MANAGE_GUILD, MANAGE_GUILD_EXPRESSIONS, MANAGE_NICKNAMES, MANAGE_ROLES, MANAGE_WEBHOOKS, MODERATE_MEMBERS, VIEW_AUDIT_LOG, VIEW_GUILD_INSIGHTS};
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
    Discovery,
    Template,
}

const SIDE: &[(&str, &[(Page, &str)])] = &[
    ("", &[(Page::Profile, "Server Profile"), (Page::Tag, "Server Tag"), (Page::Engagement, "Engagement"), (Page::Perks, "Boost Perks")]),
    ("EXPRESSION", &[(Page::Emoji, "Emoji"), (Page::Stickers, "Stickers"), (Page::Soundboard, "Soundboard")]),
    ("PEOPLE", &[(Page::Members, "Members"), (Page::Roles, "Roles"), (Page::Invites, "Invites"), (Page::Access, "Access")]),
    ("APPS", &[(Page::Integrations, "Integrations"), (Page::Directory, "App Directory")]),
    ("MODERATION", &[(Page::Safety, "Safety Setup"), (Page::Audit, "Audit Log"), (Page::Bans, "Bans"), (Page::AutoMod, "AutoMod")]),
];

fn shown(page: Page, perms: u64, community: bool) -> bool {
    let any = |m: u64| perms & m != 0;
    match page {
        Page::Profile | Page::Engagement | Page::Perks | Page::Directory => true,
        Page::Tag | Page::Invites | Page::Access | Page::Template | Page::Safety => any(MANAGE_GUILD),
        Page::Discovery => community && any(MANAGE_GUILD),
        Page::Emoji | Page::Stickers | Page::Soundboard => any(MANAGE_GUILD_EXPRESSIONS | CREATE_GUILD_EXPRESSIONS),
        Page::Members => any(ADMINISTRATOR | MANAGE_GUILD | BAN_MEMBERS | KICK_MEMBERS | MODERATE_MEMBERS | MANAGE_ROLES | MANAGE_NICKNAMES),
        Page::Roles => any(MANAGE_ROLES),
        Page::Integrations => any(MANAGE_GUILD | MANAGE_WEBHOOKS),
        Page::Audit => any(VIEW_AUDIT_LOG),
        Page::Bans => any(BAN_MEMBERS | VIEW_AUDIT_LOG),
        Page::AutoMod => !community && any(MANAGE_GUILD),
        Page::Community => any(MANAGE_GUILD) || community && any(VIEW_GUILD_INSIGHTS),
    }
}

pub struct State {
    pub guild: Id,
    pub page: Page,
    pub search: String,
    pub data: HashMap<&'static str, Value>,
    pub draft: HashMap<&'static str, Value>,
    pub dirty: bool,
    pub saving: bool,
    pub errors: Vec<(String, String)>,
    pub shake: f64,
    pub sub: u8,
    pub sound: Option<sound::Form>,
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

impl State {
    fn sub_open(&self) -> bool {
        self.sub != 0 || self.sound.is_some()
    }
}

impl App {
    pub fn open_server_settings(&mut self, gid: Id) {
        self.server = Some(State { guild: gid, page: Page::Profile, search: String::new(), data: HashMap::new(), draft: HashMap::new(), dirty: false, saving: false, errors: Vec::new(), shake: 0.0, sub: 0, sound: None, pages: Pages::default() });
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
    let Some(s) = &mut app.server else { return };
    match key {
        "sounds_done" => return drop(s.data.remove("sounds")),
        "template_made" => return drop(s.data.remove("template")),
        _ => {}
    }
    let failed = v.as_array().is_some_and(|a| a.is_empty());
    s.saving = false;
    if key == "guild" && s.sub == 5 {
        s.sub = if !failed && v["features"].as_array().is_some_and(|f| f.iter().any(|x| x == "COMMUNITY")) { 4 } else { 3 };
    }
    let mine = matches!(key, "profile" | "guild" | "widget" | "vanity" | "template" | "verification");
    if failed && mine && s.data.get(key).is_some_and(|d| !d.is_null()) {
        return;
    }
    if mine {
        s.draft.clear();
    }
    s.data.insert(key, v);
}

pub fn open_role(app: &mut App, gid: Id, role: Id, tab: u8) {
    roles::open(app, gid, role, tab);
}

pub fn audit_changed(app: &mut App, gid: Id) {
    moderation::changed(app, gid);
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
    let p = &st.pages;
    let depth = p.roles.deep() || p.apps.deep() || p.automod.deep() || p.safety.deep();
    let modal = st.pages.roles.edit.as_ref().is_some_and(|e| e.adding()) || st.pages.emoji.editing();
    if !modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) && app.confirm.is_none() && !st.sub_open() {
        if form::guard(app, ctx) {
            app.server = None;
        }
        return;
    }
    let screen = ctx.content_rect();
    let area = Rect::from_min_max(pos2(screen.left(), screen.top() + 32.0), screen.max);
    let mut goto = None;
    let mut close = false;
    let mut delete = false;
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("server_area"), area));
    crate::ui::motion::layer(ctx, egui::Area::new("server_settings".into()).order(egui::Order::Foreground).fixed_pos(area.min), area).show(ctx, |ui| {
        ui.set_min_size(area.size());
        ui.painter().rect_filled(area, 0, pal().bg_primary);
        let kind = if matches!(page, Page::Members | Page::Profile) { sidebar_view::Kind::Fill } else { sidebar_view::Kind::Default };
        let col = sidebar_view::column(area.left(), area.width(), sidebar_view::SIDEBAR, kind, true);
        let side_right = col.region - 12.0;
        ui.data_mut(|d| {
            d.insert_temp(egui::Id::new("server_col"), (col.left, col.right));
            d.insert_temp(egui::Id::new("notice"), col.notice);
        });
        let side = Rect::from_min_max(pos2(area.left(), area.top()), pos2(side_right + 6.0, area.bottom()));
        ui.painter().rect_filled(side, 0, pal().bg_secondary_alt);
        let mut sc = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(side_right - 239.0, area.top()), pos2(side_right, area.bottom()))));
        egui::ScrollArea::vertical().id_salt("server_side").auto_shrink([false, false]).show(&mut sc, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(46.0);
            let gname = app.store.guilds.get(&gid).map(|g| g.name.to_uppercase()).unwrap_or_default();
            let perms = app.store.my_perms(gid);
            let community = app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "COMMUNITY"));
            let owner = app.store.guilds.get(&gid).is_some_and(|g| g.owner == app.store.me);
            let mut first = true;
            for (head, items) in SIDE.iter() {
                let items: Vec<_> = items.iter().filter(|(p, _)| shown(*p, perms, community)).collect();
                if items.is_empty() {
                    continue;
                }
                if !first {
                    ui.add_space(8.0);
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
                    ui.add_space(8.0);
                }
                first = false;
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                let h = if head.is_empty() { gname.as_str() } else { *head };
                super::text::paint(ui.painter(), pos2(r.left() + 10.0, r.center().y + 4.0), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary, r.width() - 20.0);
                for (p, label) in items {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    super::settings::mark(ui, &format!("side:{label}"), r);
                    let on = *p == page;
                    if on {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                    } else {
                        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
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
            let pick = |l: &[(Page, &'static str)]| l.iter().copied().filter(|(p, _)| shown(*p, perms, community)).collect::<Vec<_>>();
            let tail = [pick(&[(Page::Community, if community { "Community Overview" } else { "Enable Community" }), (Page::Discovery, "Discovery")]), pick(&[(Page::Template, "Server Template")])];
            for (gi, items) in tail.iter().enumerate() {
                let del = gi == 1 && owner;
                if items.is_empty() && !del {
                    continue;
                }
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
                ui.add_space(8.0);
                for (p, label) in items {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    super::settings::mark(ui, &format!("side:{label}"), r);
                    if page == *p {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                    } else {
                        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
                    }
                    ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, *label, medium(16.0), if page == *p || resp.hovered() { pal().header_primary } else { pal().header_secondary });
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        goto = Some(*p);
                    }
                }
                if del {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
                    super::settings::mark(ui, "side:Delete Server", r);
                    ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "Delete Server", medium(16.0), RED);
                    icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(16.0, 16.0)), RED);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        delete = true;
                    }
                }
            }
            ui.add_space(60.0);
        });
        // the round ESC button off to the right
        let esc = Rect::from_center_size(pos2(col.tools + 18.0, area.top() + 78.0 - 32.0 + 18.0), vec2(36.0, 36.0));
        let er = ui.interact(esc, ui.id().with("esc"), Sense::click());
        ui.painter().circle_stroke(esc.center(), 17.0, Stroke::new(2.0, crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &er))));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(esc.center(), vec2(18.0, 18.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &er)));
        ui.painter().text(pos2(esc.center().x, esc.bottom() + 14.0), Align2::CENTER_CENTER, "ESC", semi(13.0), pal().header_secondary);
        close = er.clicked();
        let gutter = (col.tools - col.right).min(16.0);
        let body = Rect::from_min_max(pos2(col.left, area.top()), pos2(col.right + gutter, area.bottom()));
        let mut bc = ui.new_child(egui::UiBuilder::new().max_rect(body));
        let out = egui::ScrollArea::vertical().id_salt(("server_page", page as u8, depth)).auto_shrink([false, false]).show(&mut bc, |ui| {
            Frame::new().inner_margin(Margin { left: 0, right: gutter as i8, top: 46, bottom: 80 }).show(ui, |ui| match page {
                Page::Profile => profile::show(app, ui, gid),
                Page::Tag => tag::show(app, ui, gid),
                Page::Engagement => engage::show(app, ui, gid),
                Page::Perks => perks::show(app, ui, gid),
                Page::Soundboard => sound::show(app, ui, gid),
                Page::Access => access::show(app, ui, gid),
                Page::Community => community::show(app, ui, gid),
                Page::Discovery => discovery::show(app, ui, gid),
                Page::Template => template::show(app, ui, gid),
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
                Page::Directory => {
                    app.server = None;
                    app.open_directory();
                }
            });
        });
        crate::ui::e2e::scrolled(ui.ctx(), &out);
    });
    sound::modal_form(app, ctx);
    community::modal(app, ctx);
    if delete {
        let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
        app.confirm = Some(super::Confirm::new(format!("Delete '{name}'"), format!("Are you sure you want to delete **{name}**? This action cannot be undone."), "Delete Server", move |app: &mut App| {
            app.server = None;
            super::settings::guarded(app, "POST", &format!("/guilds/{gid}/delete"), None);
        }));
    }
    if close && form::guard(app, ctx) {
        app.server = None;
    }
    match goto {
        Some(Page::Directory) => {
            if form::guard(app, ctx) {
                app.server = None;
                app.open_directory();
            }
        }
        Some(p) if form::guard(app, ctx) => {
            if let Some(s) = &mut app.server {
                s.page = p;
                s.search.clear();
                s.sub = 0;
            }
        }
        _ => {}
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
    crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(31)));
    draw(ui, r, resp.hovered());
    resp
}

fn search(ui: &mut Ui, app: &mut App, hint: &str) -> String {
    let s = app.server.as_mut().unwrap();
    super::settings::input(ui, &mut s.search, hint, false);
    ui.add_space(16.0);
    s.search.trim().to_lowercase()
}

