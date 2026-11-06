//! Discord's full profile modal, and the pieces the profile popout shares with it: roles with
//! add and remove, the note box, the status bubble and the badges.

use egui::{Align2, Color32, CornerRadius, Frame, Key, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::presence::{self, Act};
use super::icons::{self, Icon};
use super::{App, avatar, popups, status_dot, text};
use crate::net::Tag;
use crate::state::{Id, MANAGE_ROLES, Store, sid, snowflake_ms};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Tab {
    Activity,
    Friends,
    Servers,
    Wishlist,
    Board,
}

pub struct Full {
    pub uid: Id,
    pub gid: Id,
    pub tab: Tab,
}

pub struct Recent {
    pub title: String,
    pub subtitle: String,
    pub image: Option<String>,
    pub ago: String,
    pub song: bool,
}

pub fn recents(v: &Value) -> Vec<Recent> {
    let now = chrono::Utc::now().timestamp_millis();
    let pick = |x: &Value, keys: &[&str]| keys.iter().find_map(|k| x[*k].as_str().filter(|s| !s.is_empty()).map(str::to_string));
    v["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let c = &e["content"];
            let x = &c["extra"];
            let title = pick(x, &["track_title", "song_name", "game_name", "activity_name"]).or_else(|| x["artist"]["name"].as_str().map(str::to_string))?;
            let song = x["track_title"].is_string() || x["song_name"].is_string();
            let secs = ((now - snowflake_ms(sid(&c["id"]))) / 1000).max(0);
            let ago = match secs {
                s if s < 3600 => format!("{}m ago", (s / 60).max(1)),
                s if s < 86400 => format!("{}h ago", s / 3600),
                s => format!("{}d ago", s / 86400),
            };
            Some(Recent {
                title,
                subtitle: pick(x, &["artist_name"]).or_else(|| x["artist"]["name"].as_str().map(str::to_string)).unwrap_or_default(),
                image: x["media"]["image_url"].as_str().map(str::to_string),
                ago,
                song,
            })
        })
        .collect()
}

impl App {
    pub fn open_full(&mut self, uid: Id, gid: Id, tab: Tab) {
        self.profile = None;
        self.full_profile = Some(Full { uid, gid, tab });
        if !self.profiles.contains_key(&uid) || self.profiles[&uid].is_null() {
            let q = if gid != 0 { format!("&guild_id={gid}") } else { String::new() };
            self.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=true&with_mutual_friends=true{q}"), None, Tag::Profile(uid));
        }
        if !self.notes.contains_key(&uid) {
            self.notes.insert(uid, String::new());
            self.rest("GET", format!("/users/@me/notes/{uid}"), None, Tag::Note(uid));
        }
        let key = format!("idents:{uid}");
        if !self.keyed.contains_key(&key) {
            self.keyed.insert(key.clone(), None);
            self.rest("GET", format!("/users/{uid}/application-identities?with_profiles=true"), None, Tag::Keyed(key));
        }
        if !self.outbox.contains_key(&uid) {
            self.outbox.insert(uid, Vec::new());
            self.rest("GET", format!("/content-inventory/users/{uid}/outbox"), None, Tag::Outbox(uid));
        }
    }

    pub fn save_note(&mut self, uid: Id, note: String) {
        let was = self.notes.insert(uid, note.clone());
        self.rest("PUT", format!("/users/@me/notes/{uid}"), Some(json!({ "note": note })), Tag::rollback(move |app: &mut App| {
            match &was {
                Some(n) => app.notes.insert(uid, n.clone()),
                None => app.notes.remove(&uid),
            };
        }));
    }

    pub fn set_role(&mut self, gid: Id, uid: Id, role: Id, on: bool) {
        let m = self.store.members.entry(gid).or_default().entry(uid).or_default();
        let had = m.roles.contains(&role);
        if on {
            if !m.roles.contains(&role) {
                m.roles.push(role);
            }
        } else {
            m.roles.retain(|r| *r != role);
        }
        self.rest(if on { "PUT" } else { "DELETE" }, format!("/guilds/{gid}/members/{uid}/roles/{role}"), None, Tag::rollback(move |app: &mut App| {
            let m = app.store.members.entry(gid).or_default().entry(uid).or_default();
            m.roles.retain(|r| *r != role);
            if had {
                m.roles.push(role);
            }
        }));
    }
}

/// The roles you may hand out or take back: below your highest one, or all of them as owner.
pub fn can_manage(store: &Store, gid: Id, role: Id) -> bool {
    let Some(g) = store.guilds.get(&gid) else { return false };
    let Some(r) = g.roles.get(&role) else { return false };
    if role == gid {
        return false;
    }
    if g.owner == store.me {
        return true;
    }
    if store.my_perms(gid) & MANAGE_ROLES == 0 {
        return false;
    }
    let top = g.my_roles.iter().filter_map(|r| g.roles.get(r)).map(|r| r.position).max().unwrap_or(0);
    r.position < top
}

pub fn can_manage_any(store: &Store, gid: Id) -> bool {
    store.guilds.get(&gid).is_some_and(|g| g.roles.keys().any(|r| can_manage(store, gid, *r)))
}

/// A member's roles, highest first: what the member list knows, else what their profile said.
pub fn member_roles(store: &Store, p: Option<&Value>, gid: Id, uid: Id) -> Vec<Id> {
    let Some(g) = store.guilds.get(&gid) else { return Vec::new() };
    let mut ids: Vec<Id> = match store.members.get(&gid).and_then(|m| m.get(&uid)) {
        Some(m) => m.roles.clone(),
        None => p.and_then(|p| p["guild_member"]["roles"].as_array()).into_iter().flatten().map(sid).collect(),
    };
    ids.retain(|r| *r != gid && g.roles.contains_key(r));
    ids.sort_by_key(|r| -g.roles[r].position);
    ids
}

pub enum RoleAct {
    Remove(Id),
    Add(Pos2),
}

/// Role pills in rows, with their remove buttons and the add one when you can manage roles.
/// `rows` caps what's shown before a "+N" pill, which opens the rest.
pub fn roles_row(ui: &mut Ui, store: &Store, gid: Id, uid: Id, roles: &[Id], w: f32, rows: usize) -> Option<RoleAct> {
    let g = store.guilds.get(&gid)?;
    let manage_any = can_manage_any(store, gid) && uid != 0;
    let open_id = ui.id().with(("roles_open", uid));
    let open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let pills: Vec<(Id, egui::Galley, bool)> = roles
        .iter()
        .map(|r| {
            let galley = ui.painter().layout_no_wrap(g.roles[r].name.clone(), reg(12.0), pal().header_primary);
            (*r, std::sync::Arc::try_unwrap(galley).unwrap_or_else(|a| (*a).clone()), can_manage(store, gid, *r))
        })
        .collect();
    let width = |p: &(Id, egui::Galley, bool)| 32.0 + p.1.size().x + if p.2 { 18.0 } else { 0.0 };
    let (mut x, mut row, mut shown) = (0.0, 0usize, 0usize);
    let mut spots = Vec::new();
    for (i, p) in pills.iter().enumerate() {
        let pw = width(p);
        if x > 0.0 && x + pw > w {
            row += 1;
            x = 0.0;
        }
        let last = rows != 0 && !open && row + 1 == rows;
        let reserve = if last && i + 1 < pills.len() { 44.0 } else { 0.0 };
        if rows != 0 && !open && (row >= rows || x + pw + reserve > w) {
            break;
        }
        spots.push((x, row));
        x += pw + 4.0;
        shown += 1;
    }
    let hidden = pills.len() - shown;
    let plus_w = if hidden > 0 { 16.0 + 12.0 + 12.0 * (hidden.to_string().len() as f32 - 1.0) } else { 0.0 };
    let mut tail = x;
    let mut tail_row = spots.last().map_or(0, |s| s.1);
    if x == 0.0 && !spots.is_empty() {
        tail_row = row;
    }
    let mut extra = Vec::new();
    if hidden > 0 {
        if tail + plus_w > w {
            tail = 0.0;
            tail_row += 1;
        }
        extra.push((tail, tail_row, plus_w));
        tail += plus_w + 4.0;
    }
    if manage_any {
        if tail + 24.0 > w {
            tail = 0.0;
            tail_row += 1;
        }
        extra.push((tail, tail_row, 24.0));
    }
    let total_rows = tail_row.max(spots.last().map_or(0, |s| s.1)) + 1;
    if pills.is_empty() && !manage_any {
        return None;
    }
    let (r, _) = ui.allocate_exact_size(vec2(w, total_rows as f32 * 28.0 - 4.0), Sense::hover());
    let mut act = None;
    for ((id, galley, removable), (x, row)) in pills.into_iter().zip(&spots) {
        let pill = Rect::from_min_size(pos2(r.left() + x, r.top() + *row as f32 * 28.0), vec2(32.0 + galley.size().x + if removable { 18.0 } else { 0.0 }, 24.0));
        let role = &g.roles[&id];
        let p = ui.painter();
        p.rect_stroke(pill, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        let col = if role.color == 0 { Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(role.color) };
        p.circle_filled(pos2(pill.left() + 12.0, pill.center().y), 6.0, col);
        p.galley(pos2(pill.left() + 22.0, pill.center().y - galley.size().y / 2.0), galley.into(), pal().header_primary);
        if removable {
            let b = Rect::from_center_size(pos2(pill.right() - 12.0, pill.center().y), vec2(16.0, 16.0));
            let resp = ui.interact(b, ui.id().with(("role_x", id)), Sense::click());
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "role_x", id)), b));
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(b.center(), vec2(10.0, 10.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.on_hover_text(format!("Remove role {}", role.name)).clicked() {
                act = Some(RoleAct::Remove(id));
            }
        }
    }
    let mut it = extra.into_iter();
    if hidden > 0 {
        let (x, row, pw) = it.next().unwrap();
        let pill = Rect::from_min_size(pos2(r.left() + x, r.top() + row as f32 * 28.0), vec2(pw, 24.0));
        let resp = ui.interact(pill, ui.id().with("roles_more"), Sense::click());
        let p = ui.painter();
        p.rect_stroke(pill, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        p.text(pill.center(), Align2::CENTER_CENTER, format!("+{hidden}"), reg(12.0), pal().header_secondary);
        if resp.on_hover_text(format!("View {hidden} more role{}", if hidden == 1 { "" } else { "s" })).clicked() {
            ui.data_mut(|d| d.insert_temp(open_id, true));
        }
    }
    if let Some((x, row, _)) = it.next() {
        let pill = Rect::from_min_size(pos2(r.left() + x, r.top() + row as f32 * 28.0), vec2(24.0, 24.0));
        let resp = ui.interact(pill, ui.id().with("role_add"), Sense::click());
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "role_add")), pill));
        let p = ui.painter();
        p.rect_stroke(pill, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        icons::draw(p, Icon::Plus, Rect::from_center_size(pill.center(), vec2(12.0, 12.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text("Add role").clicked() {
            act = Some(RoleAct::Add(pill.left_bottom() + vec2(0.0, 4.0)));
        }
    }
    act
}

/// What a role pill's buttons asked for, carried out.
pub fn do_role(app: &mut App, gid: Id, uid: Id, act: RoleAct) {
    match act {
        RoleAct::Remove(r) => app.set_role(gid, uid, r, false),
        RoleAct::Add(at) => app.role_add = Some((gid, uid, at, String::new())),
    }
}

/// The list that opens from a profile's + button: roles they don't have, searchable.
pub fn role_picker(app: &mut App, ctx: &egui::Context) {
    let Some((gid, uid, at, _)) = &app.role_add else { return };
    let (gid, uid, at) = (*gid, *uid, *at);
    let store = &app.store;
    let have = member_roles(store, app.profiles.get(&uid), gid, uid);
    let mut list: Vec<(Id, String, u32)> = store
        .guilds
        .get(&gid)
        .map(|g| g.roles.values().filter(|r| !have.contains(&r.id) && can_manage(store, gid, r.id)).map(|r| (r.id, r.name.clone(), r.color)).collect())
        .unwrap_or_default();
    list.sort_by_key(|(id, ..)| std::cmp::Reverse(store.guilds[&gid].roles[id].position));
    let mut pick = None;
    let mut query = std::mem::take(&mut app.role_add.as_mut().unwrap().3);
    super::motion::slide(ctx, egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("role_picker")), vec2(0.0, 1.0));
    let area = egui::Area::new(egui::Id::new("role_picker"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(8).show(ui, |ui| {
                ui.set_width(220.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .font(reg(14.0))
                        .hint_text("Role")
                        .desired_width(204.0),
                );
                edit.request_focus();
                let q = query.to_lowercase();
                egui::ScrollArea::vertical().max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (id, name, color) in list.iter().filter(|(_, n, _)| n.to_lowercase().contains(&q)) {
                        let (r, resp) = ui.allocate_exact_size(vec2(204.0, 32.0), Sense::click());
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "role_pick")), r));
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let col = if *color == 0 { Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(*color) };
                        ui.painter().circle_filled(pos2(r.left() + 14.0, r.center().y), 6.0, col);
                        text::paint(ui.painter(), pos2(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, name, medium(14.0), pal().header_primary, 170.0);
                        if resp.clicked() {
                            pick = Some(*id);
                        }
                    }
                    if list.is_empty() {
                        ui.label(RichText::new("No roles to add").font(reg(14.0)).color(pal().header_secondary));
                    }
                });
            });
        });
    if let Some(r) = app.role_add.as_mut() {
        r.3 = query;
    }
    if let Some(r) = pick {
        app.set_role(gid, uid, r, true);
        app.role_add = None;
    } else if popups::dismissed(ctx, &area.response) {
        app.role_add = None;
    }
}

/// A profile's badges, `size` apiece, from x along cy. Each says what it is on hover and opens
/// its page when clicked. Hands back where the next thing starts.
pub fn badges(ui: &mut Ui, p: Option<&Value>, mut x: f32, cy: f32, size: f32) -> f32 {
    for b in p.and_then(|p| p["badges"].as_array()).into_iter().flatten() {
        let Some(icon) = b["icon"].as_str() else { continue };
        let br = Rect::from_min_size(pos2(x, cy - size / 2.0), vec2(size, size));
        egui::Image::from_uri(format!("{}/badge-icons/{icon}.png", crate::api::cdn())).show_loading_spinner(false).paint_at(ui, br);
        let resp = ui.interact(br, ui.id().with(("badge", icon)), Sense::click()).on_hover_text(b["description"].as_str().unwrap_or(""));
        if let Some(link) = b["link"].as_str()
            && resp.hovered()
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(link));
            }
        }
        x += size + 2.0;
    }
    x
}

pub enum StatusAct {
    React,
    Reply,
}

/// Somebody's custom status as a bubble beside their avatar; hovering it offers to react or
/// reply. Not drawn when they have none.
pub fn status_bubble(ui: &mut Ui, top: Rect, store: &Store, uid: Id, reacts: bool) -> Option<StatusAct> {
    let a = store.activities.get(&uid)?.iter().find(|a| a.kind == 4 && (a.state.is_some() || a.emoji.is_some()))?;
    let state = a.state.clone().unwrap_or_default();
    let g = ui.painter().layout(state, reg(14.0), Color32::WHITE, 120.0);
    let em = if a.emoji.is_some() { 24.0 } else { 0.0 };
    let w = (g.size().x + em + 24.0).max(40.0);
    let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(w, (g.size().y + 20.0).max(36.0)));
    let p = ui.painter();
    p.rect(b, 16, pal().bg_floating, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    if let Some(e) = &a.emoji {
        text::image(p, e, Rect::from_center_size(pos2(b.left() + 20.0, b.center().y), vec2(18.0, 18.0)));
    }
    p.galley(pos2(b.left() + 12.0 + em, b.center().y - g.size().y / 2.0), g, pal().header_primary);
    let mut act = None;
    if reacts && ui.rect_contains_pointer(b.expand2(vec2(60.0, 6.0))) {
        for (i, (icon, tip, what)) in [(Icon::AddReaction, "React to status", StatusAct::React), (Icon::Reply, "Reply to status", StatusAct::Reply)].into_iter().enumerate() {
            let c = Rect::from_center_size(pos2(b.right() + 18.0 + i as f32 * 28.0, b.center().y), vec2(24.0, 24.0));
            let resp = ui.interact(c, ui.id().with(("status_btn", i)), Sense::click());
            let p = ui.painter();
            p.circle(c.center(), 12.0, crate::ui::motion::mix(pal().bg_floating, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, pal().border_subtle));
            icons::draw(p, icon, Rect::from_center_size(c.center(), vec2(14.0, 14.0)), pal().header_primary);
            if resp.on_hover_text(tip).clicked() {
                act = Some(what);
            }
        }
    }
    act
}

/// The note only you see: Enter saves it, Shift+Enter starts a new line, six at most. Hands
/// back the text to store when it was committed and changed.
pub fn note_box(ui: &mut Ui, rects: &mut crate::state::Map<String, Rect>, key: &str, uid: Id, saved: &str, w: f32, size: f32, focus: bool) -> Option<String> {
    let draft = egui::Id::new(("note_draft", uid));
    let mut note: String = ui.data(|d| d.get_temp(draft)).unwrap_or_else(|| saved.to_string());
    let nid = egui::Id::new((key, uid));
    let focused = ui.memory(|m| m.has_focus(nid));
    let enter = focused && ui.input_mut(|i| super::take_key(i, egui::Modifiers::NONE, Key::Enter));
    let before = note.clone();
    let r = ui.add(
        egui::TextEdit::multiline(&mut note)
            .id(nid)
            .frame(Frame::NONE)
            .desired_rows(1)
            .font(reg(size))
            .hint_text(RichText::new("Click to add a note").font(reg(size)).color(pal().text_muted))
            .desired_width(w),
    );
    if focus && !ui.memory(|m| m.areas().visible_last_frame(&ui.layer_id())) {
        r.request_focus();
    }
    if note.split('\n').count() > 6 {
        note = before;
    }
    rects.insert(key.into(), r.rect);
    if enter {
        r.surrender_focus();
    }
    let commit = enter || r.lost_focus();
    if !commit && !r.has_focus() {
        note = saved.to_string();
    }
    let out = (commit && note != saved).then(|| note.clone());
    ui.data_mut(|d| d.insert_temp(draft, note));
    out
}

fn since(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default()
}

fn iso(v: &Value) -> i64 {
    v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map_or(0, |d| d.timestamp_millis())
}

fn round_button(ui: &mut Ui, r: Rect, icon: Icon, tip: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("fp_btn", tip)), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 }));
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip).clicked()
}

enum Do {
    Close,
    Tab(Tab),
    Add,
    Message,
    More(Pos2),
    Edit,
    Open(Id),
    Guild(Id),
    Role(RoleAct),
    Note(String),
    Play(String),
    Tag,
    Avatar,
}

/// The modal: the person down the left, their activity and mutuals on the right.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(Full { uid, gid, tab }) = app.full_profile else { return };
    let Some(user) = app.store.user(uid).cloned() else {
        app.full_profile = None;
        return;
    };
    let me = app.store.me;
    let worn = app.profiles.get(&uid).filter(|p| !p.is_null()).cloned();
    let layers = super::cosmo::layers_of(app, worn.as_ref());
    let fx_since = super::cosmo::since(ctx, ("full", uid));
    let widgets = worn.as_ref().map(super::profed::widgets_of).unwrap_or_default();
    super::profed::board::want_game_icons(app, &widgets);
    let linked: Vec<String> = match app.keyed.get(&format!("idents:{uid}")) {
        Some(Some(Ok(v))) => v.as_array().into_iter().flatten().filter_map(|i| i["name"].as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    let wish = (tab == Tab::Wishlist).then(|| super::profed::board::wish(app, uid));
    let p = app.profiles.get(&uid).filter(|p| !p.is_null());
    let name = app.store.name_in(gid, uid);
    let status = app.store.status(uid);
    let rel = app.store.relationships.get(&uid).copied();
    let stranger = !user.bot && uid != me && rel.is_none();
    let roles = if gid != 0 { member_roles(&app.store, p, gid, uid) } else { Vec::new() };
    let guild = app.store.guilds.get(&gid);
    let joined = p.map_or(0, |p| iso(&p["guild_member"]["joined_at"]));
    let saved = app.notes.get(&uid).cloned().unwrap_or_default();
    let friends: Vec<Id> = p.and_then(|p| p["mutual_friends"].as_array()).into_iter().flatten().map(|f| sid(&f["id"])).collect();
    let servers: Vec<Id> = p.and_then(|p| p["mutual_guilds"].as_array()).into_iter().flatten().map(|g| sid(&g["id"])).collect();
    let n_friends = p.and_then(|p| p["mutual_friends_count"].as_u64()).unwrap_or(friends.len() as u64);
    let recent = app.outbox.get(&uid);
    let live: Vec<crate::state::Activity> = app.store.activities.get(&uid).map(|l| presence::shown(l).cloned().collect()).unwrap_or_default();
    let spotify = app.profiles.get(&me).is_some_and(|m| m["connected_accounts"].as_array().is_some_and(|a| a.iter().any(|c| c["type"] == "spotify")));
    let icons_by_app = app.store.app_icons.clone();
    let screen = ctx.content_rect();
    let h = (screen.height() - 100.0).clamp(480.0, 800.0);
    let mut act = None;
    let mut note_rect = Rect::NOTHING;
    let store = &app.store;
    let notes = &mut app.rects;
    let frame = Frame::new().fill(pal().bg_primary).corner_radius(16).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = crate::ui::motion::Modal::new("full_profile".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(960.0);
        ui.set_height(h);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect();
        let panel = Rect::from_min_max(pos2(area.left() + 48.0, area.top() + 48.0), pos2(area.left() + 448.0, area.bottom()));
        let wash = ui.painter().add(egui::Shape::Noop);
        let cr = CornerRadius { nw: 16, ne: 16, sw: 0, se: 0 };
        ui.painter().rect_filled(panel, cr, pal().bg_secondary);
        let banner = Rect::from_min_size(panel.min, vec2(400.0, 140.0));
        let accent = p
            .and_then(|p| p["user_profile"]["accent_color"].as_u64().or(p["user"]["accent_color"].as_u64()))
            .map(|c| role_color(c as u32))
            .or_else(|| super::palette::avatar(ui.ctx(), &user.avatar_url()))
            .unwrap_or(BLURPLE);
        ui.painter().rect_filled(banner, cr, accent);
        if let Some(b) = p.and_then(|p| p["user"]["banner"].as_str()) {
            crate::gif::paint_still(ui, &format!("{}/banners/{uid}/{b}.png?size=600", crate::api::cdn()), banner, cr, ui.rect_contains_pointer(banner));
        }
        let ring = popups::theme_top(p).unwrap_or(pal().bg_secondary);
        let av = Rect::from_min_size(pos2(panel.left() + 16.0, banner.bottom() - 62.0), vec2(120.0, 120.0));
        ui.painter().circle_filled(av.center(), 68.0, ring);
        avatar(ui, av, &user.avatar_url().replace("size=80", "size=256"));
        if let Some(d) = user.decoration_url() {
            text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(144.0, 144.0)));
        }
        status_dot(ui, Rect::from_min_size(av.right_bottom() - vec2(95.0, 95.0), vec2(95.0, 95.0)), status, ring);
        if user.avatar.is_some() {
            let hit = ui.interact(av, ui.id().with("fp_avatar"), Sense::click());
            if hit.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.on_hover_text("View Profile Picture").clicked() {
                act = Some(Do::Avatar);
            }
        }
        let mut y = banner.bottom() + 80.0;
        let x = panel.left() + 32.0;
        let face = if crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == user.font) { name_font(user.font, 24.0) } else { semi(24.0) };
        let nr = text::paint(ui.painter(), pos2(x, y + 15.0), Align2::LEFT_CENTER, &name, face, pal().header_primary, 336.0);
        if user.bot {
            super::app_tag(ui.painter(), pos2(nr.right() + 8.0, nr.center().y), user.flags & (1 << 16) != 0, 1.0);
        }
        y += 32.0;
        let mut r = text::paint(ui.painter(), pos2(x, y + 9.0), Align2::LEFT_CENTER, &user.username, reg(14.0), pal().header_primary, 200.0);
        if let Some(pr) = p.and_then(|p| p["user_profile"]["pronouns"].as_str()).filter(|s| !s.is_empty()) {
            ui.painter().circle_filled(pos2(r.right() + 9.0, r.center().y), 2.0, pal().header_primary);
            r = text::paint(ui.painter(), pos2(r.right() + 18.0, y + 9.0), Align2::LEFT_CENTER, pr, reg(14.0), pal().header_primary, 120.0);
        }
        let chip = super::tag_chip(ui.painter(), &user, pos2(r.right() + 8.0, r.center().y), 1.0);
        let next = chip.map_or(r.right() + 8.0, |t| t.right() + 6.0);
        if let Some(t) = chip
            && super::tagpop::hit(ui, t, "fp_tag")
        {
            act = Some(Do::Tag);
        }
        badges(ui, p, next, r.center().y, 20.0);
        y += 44.0;
        if uid == me {
            if text_button(ui, Rect::from_min_size(pos2(x, y), vec2(110.0, 32.0)), "Edit Profile", Icon::Pencil, true) {
                act = Some(Do::Edit);
            }
        } else {
            let mut bx = x;
            if stranger {
                if text_button(ui, Rect::from_min_size(pos2(bx, y), vec2(110.0, 32.0)), "Add Friend", Icon::PersonAdd, true) {
                    act = Some(Do::Add);
                }
                bx += 122.0;
            } else if rel == Some(crate::state::FRIEND) || user.bot {
                if text_button(ui, Rect::from_min_size(pos2(bx, y), vec2(130.0, 32.0)), "Send Message", Icon::Bubble, true) {
                    act = Some(Do::Message);
                }
                bx += 142.0;
            }
            if stranger && round_button(ui, Rect::from_min_size(pos2(bx, y), vec2(32.0, 32.0)), Icon::Bubble, "Message") {
                act = Some(Do::Message);
            }
            if stranger {
                bx += 40.0;
            }
            let mr = Rect::from_min_size(pos2(bx, y), vec2(32.0, 32.0));
            if round_button(ui, mr, Icon::More, "More") {
                act = Some(Do::More(mr.left_bottom() + vec2(0.0, 4.0)));
            }
        }
        y += 56.0;
        let label = |ui: &Ui, y: f32, s: &str| {
            ui.painter().text(pos2(x, y + 8.0), Align2::LEFT_CENTER, s, medium(12.0), pal().header_secondary);
        };
        label(ui, y, "Member Since");
        y += 24.0;
        let mut cx = x;
        icons::draw(ui.painter(), Icon::Discord, Rect::from_min_size(pos2(cx, y), vec2(18.0, 18.0)), pal().header_secondary);
        cx = text::paint(ui.painter(), pos2(cx + 22.0, y + 9.0), Align2::LEFT_CENTER, &since(snowflake_ms(uid)), reg(14.0), pal().header_primary, 150.0).right();
        if let (Some(g), true) = (guild, joined > 0) {
            ui.painter().circle_filled(pos2(cx + 10.0, y + 9.0), 2.0, pal().header_secondary);
            if let Some(url) = g.icon_url() {
                text::image(ui.painter(), &url, Rect::from_min_size(pos2(cx + 20.0, y), vec2(18.0, 18.0)));
            }
            text::paint(ui.painter(), pos2(cx + 44.0, y + 9.0), Align2::LEFT_CENTER, &since(joined), reg(14.0), pal().header_primary, 150.0);
        }
        y += 42.0;
        if rel == Some(crate::state::FRIEND)
            && let Some(ms) = store.rel_since.get(&uid)
        {
            label(ui, y, "Friends Since");
            y += 24.0;
            text::paint(ui.painter(), pos2(x, y + 9.0), Align2::LEFT_CENTER, &since(*ms), reg(14.0), pal().header_primary, 200.0);
            y += 42.0;
        }
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(x, y), pos2(x + 336.0, panel.bottom() - 8.0))));
        body.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if guild.is_some() && (!roles.is_empty() || can_manage_any(store, gid)) {
            body.label(RichText::new("Roles").font(medium(12.0)).color(pal().header_secondary));
            body.add_space(8.0);
            if let Some(a) = roles_row(&mut body, store, gid, uid, &roles, 336.0, 0) {
                act = Some(Do::Role(a));
            }
            body.add_space(24.0);
        }
        if uid != me && !crate::theme::view().hide_personal {
            body.label(RichText::new("Note (only visible to you)").font(medium(12.0)).color(pal().header_secondary));
            body.add_space(4.0);
            if let Some(n) = note_box(&mut body, notes, "full_note", uid, &saved, 336.0, 14.0, false) {
                act = Some(Do::Note(n));
            }
            note_rect = notes.get("full_note").copied().unwrap_or(Rect::NOTHING);
        }
        popups::theme_wash(ui, wash, p, panel);
        super::cosmo::paint_layers(ui, panel, &layers, fx_since);

        let rx = area.left() + 481.0;
        let mut tabs: Vec<(Tab, String)> = Vec::new();
        if !widgets.is_empty() {
            tabs.push((Tab::Board, "Board".into()));
        }
        tabs.push((Tab::Activity, "Activity".into()));
        if !user.bot {
            tabs.push((Tab::Wishlist, "Wishlist".into()));
        }
        if uid != me && !user.bot {
            tabs.extend([
                (Tab::Friends, format!("{n_friends} Mutual Friend{}", if n_friends == 1 { "" } else { "s" })),
                (Tab::Servers, format!("{} Mutual Server{}", servers.len(), if servers.len() == 1 { "" } else { "s" })),
            ]);
        }
        let mut tx = rx;
        let ty = area.top() + 76.0;
        for (t, label) in &tabs {
            let on = *t == tab;
            let g = ui.painter().layout_no_wrap(label.clone(), medium(14.0), if on { pal().header_primary } else { pal().header_secondary }, );
            let tr = Rect::from_min_size(pos2(tx, ty - 10.0), vec2(g.size().x, 32.0));
            let resp = ui.interact(tr, ui.id().with(("fp_tab", label)), Sense::click());
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "fp_tab", *t as u8)), tr));
            let ink = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
            let g = ui.painter().layout_no_wrap(label.clone(), medium(14.0), ink);
            ui.painter().galley(pos2(tx, ty - g.size().y / 2.0 + 6.0), g, ink);
            if on {
                ui.painter().rect_filled(Rect::from_min_size(pos2(tx, ty + 20.0), vec2(tr.width(), 2.0)), 1, pal().header_primary);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                act = Some(Do::Tab(*t));
            }
            tx += tr.width() + 24.0;
        }
        ui.painter().hline(rx..=rx + 432.0, ty + 21.0, Stroke::new(1.0, pal().border_subtle));
        let cb = Rect::from_center_size(pos2(area.right() - 24.0 - 18.0, area.top() + 38.0), vec2(36.0, 36.0));
        let resp = ui.interact(cb, ui.id().with("fp_close"), Sense::click());
        ui.painter().circle_filled(cb.center(), 18.0, Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 }));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(cb.center(), vec2(16.0, 16.0)), pal().header_primary);
        if resp.clicked() {
            act = Some(Do::Close);
        }
        let list = Rect::from_min_max(pos2(rx, ty + 36.0), pos2(rx + 432.0, area.bottom() - 16.0));
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut content, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            ui.set_width(432.0);
            let head = |ui: &mut Ui, s: &str| {
                ui.label(RichText::new(s).font(medium(12.0)).color(pal().header_secondary));
            };
            match tab {
                Tab::Board => {
                    let r = Rect::from_min_size(ui.cursor().min, vec2(432.0, 0.0));
                    let mut h = super::profed::board::paint_widgets(ui, store, r, &widgets, pal().header_primary);
                    if !linked.is_empty() {
                        ui.painter().text(pos2(r.left(), r.top() + h + 8.0), Align2::LEFT_CENTER, "Linked accounts", medium(12.0), pal().header_secondary);
                        for (i, n) in linked.iter().enumerate() {
                            ui.painter().text(pos2(r.left(), r.top() + h + 32.0 + i as f32 * 22.0), Align2::LEFT_CENTER, n, reg(14.0), pal().header_primary);
                        }
                        h += 40.0 + linked.len() as f32 * 22.0;
                    }
                    ui.allocate_exact_size(vec2(432.0, h), Sense::hover());
                }
                Tab::Wishlist => {
                    if let Some(w) = &wish {
                        let r = Rect::from_min_size(ui.cursor().min, vec2(432.0, 0.0));
                        let h = super::profed::board::paint_wishlist(ui, r, w, pal().header_primary);
                        ui.allocate_exact_size(vec2(432.0, h), Sense::hover());
                    }
                }
                Tab::Activity => {
                    if !live.is_empty() {
                        head(ui, "Current activity");
                    }
                    for a in &live {
                        let icon = icons_by_app.get(&a.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=96", crate::api::cdn(), a.app));
                        match presence::card(ui, a, 432.0, &icon, spotify) {
                            Some(Act::Play(u)) => act = Some(Do::Play(u)),
                            Some(Act::Connect) => act = Some(Do::Play("https://discord.com/settings/connections".into())),
                            _ => {}
                        }
                    }
                    head(ui, "Recent activity");
                    match recent {
                        Some(r) if !r.is_empty() => {
                            for e in r {
                                presence::recent(ui, 432.0, e);
                            }
                        }
                        _ if live.is_empty() => {
                            ui.label(RichText::new("No recent activity").font(reg(14.0)).color(pal().header_secondary));
                        }
                        _ => {}
                    }
                }
                Tab::Friends => {
                    if friends.is_empty() {
                        ui.label(RichText::new("No Mutual Friends").font(semi(16.0)).color(pal().header_primary));
                    }
                    for f in &friends {
                        let Some(u) = store.user(*f) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(432.0, 56.0), Sense::click());
                        row_frame(ui, r, &resp);
                        let a = Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 12.0), vec2(32.0, 32.0));
                        avatar(ui, a, &u.avatar_url());
                        status_dot(ui, a, store.status(*f), pal().bg_primary);
                        text::paint(ui.painter(), pos2(a.right() + 12.0, r.top() + 20.0), Align2::LEFT_CENTER, u.display(), semi(16.0), pal().header_primary, 300.0);
                        text::paint(ui.painter(), pos2(a.right() + 12.0, r.top() + 38.0), Align2::LEFT_CENTER, &u.username, reg(12.0), pal().header_secondary, 300.0);
                        if resp.clicked() {
                            act = Some(Do::Open(*f));
                        }
                    }
                }
                Tab::Servers => {
                    if servers.is_empty() {
                        ui.label(RichText::new("No Mutual Servers").font(semi(16.0)).color(pal().header_primary));
                    }
                    for s in &servers {
                        let Some(g) = store.guilds.get(s) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(432.0, 56.0), Sense::click());
                        row_frame(ui, r, &resp);
                        let a = Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 12.0), vec2(32.0, 32.0));
                        ui.painter().circle_filled(a.center(), 16.0, pal().bg_accent);
                        match g.icon_url() {
                            Some(url) => text::image(ui.painter(), &url, a),
                            None => {
                                ui.painter().text(a.center(), Align2::CENTER_CENTER, g.acronym(), semi(12.0), pal().header_primary);
                            }
                        }
                        text::paint(ui.painter(), pos2(a.right() + 12.0, r.center().y), Align2::LEFT_CENTER, &g.name, semi(16.0), pal().header_primary, 340.0);
                        if resp.clicked() {
                            act = Some(Do::Guild(*s));
                        }
                    }
                }
            }
        });
    });
    if note_rect != Rect::NOTHING {
        app.rects.insert("full_note".into(), note_rect);
    }
    if m.should_close() {
        app.full_profile = None;
    }
    if matches!(act, Some(Do::Close)) || app.full_profile.is_none() {
        super::cosmo::forget(ctx, ("full", uid));
    }
    match act {
        Some(Do::Close) => app.full_profile = None,
        Some(Do::Tab(t)) => app.full_profile = Some(Full { uid, gid, tab: t }),
        Some(Do::Add) => app.rest("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), Tag::Friend),
        Some(Do::Message) => {
            app.full_profile = None;
            app.open_dm(uid);
        }
        Some(Do::More(at)) => app.user_menu = Some((uid, gid, None, at)),
        Some(Do::Edit) => {
            app.full_profile = None;
            app.settings_page = Some(super::settings::Page::Profiles);
        }
        Some(Do::Open(f)) => app.open_full(f, gid, Tab::Activity),
        Some(Do::Guild(g)) => {
            app.full_profile = None;
            app.ui.guild = Some(g);
        }
        Some(Do::Role(a)) => do_role(app, gid, uid, a),
        Some(Do::Note(n)) => app.save_note(uid, n),
        Some(Do::Play(u)) => ctx.open_url(egui::OpenUrl::new_tab(u)),
        Some(Do::Avatar) => {
            let big = user_avatar_big(&app.store, uid);
            app.viewer = Some((big.clone(), big, 1024, 1024));
        }
        Some(Do::Tag) => {
            if let Some((g, ..)) = user_tag(&app.store, uid) {
                app.tag_pop = Some((g, Pos2::new(ctx.content_rect().center().x, 140.0)));
            }
        }
        None => {}
    }
}

pub fn user_tag(store: &Store, uid: Id) -> Option<(Id, String, String)> {
    store.user(uid)?.tag.clone().map(|(t, g, b)| (g, t, b))
}

fn row_frame(ui: &Ui, r: Rect, resp: &egui::Response) {
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn text_button(ui: &mut Ui, r: Rect, label: &str, icon: Icon, primary: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(("fp_text_btn", label)), Sense::click());
    let fill = if primary { crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x44, 0x52, 0xbb), crate::ui::motion::hot(ui.ctx(), &resp)) } else { Color32::from_rgba_unmultiplied(151, 151, 159, 31) };
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), Color32::WHITE);
    let total = 20.0 + g.size().x;
    let x = r.center().x - total / 2.0;
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(16.0, 16.0)), Color32::WHITE);
    ui.painter().galley(pos2(x + 20.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn user_avatar_big(store: &Store, uid: Id) -> String {
    store.user(uid).map(|u| u.avatar_url().replace("size=80", "size=1024")).unwrap_or_default()
}
