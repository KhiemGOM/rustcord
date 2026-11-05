//! Discord's Events window from the sidebar's Events row: the server's scheduled events as cards
//! with Copy Link and Interested, or its empty state, and Create Event for those allowed. A
//! card opens Event Details with who is interested; its menu starts, ends, edits or cancels.

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::{App, Confirm};
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;

#[derive(Default)]
pub struct Events {
    pub guild: Id,
    pub list: Option<Vec<Value>>,
    // events you've said you're interested in
    pub rsvp: HashSet<Id>,
    // the event whose details are open, and who is interested in each one opened
    pub detail: Option<Id>,
    pub users: HashMap<Id, Option<Vec<Value>>>,
}

impl App {
    pub fn open_events(&mut self, gid: Id) {
        self.events = Some(Events { guild: gid, ..Default::default() });
        self.rest("GET", format!("/guilds/{gid}/scheduled-events?with_user_count=true"), None, Tag::Events(gid, 0));
        self.rest("GET", format!("/users/@me/scheduled-events?guild_ids={gid}"), None, Tag::Events(gid, 1));
        self.read_events(gid);
    }

    /// Opening the window reads the server's event notifications.
    pub fn read_events(&mut self, gid: Id) {
        let Some(newest) = self.store.guilds.get(&gid).map(|g| g.newest_event).filter(|n| *n != 0) else { return };
        let r = self.store.event_reads.entry(gid).or_default();
        if r.last_acked >= newest && r.mentions == 0 {
            return;
        }
        r.last_acked = newest;
        r.mentions = 0;
        let state = json!({ "channel_id": gid.to_string(), "message_id": newest.to_string(), "read_state_type": 1 });
        self.rest("POST", "/read-states/ack-bulk".into(), Some(json!({ "read_states": [state] })), Tag::Quiet);
    }

    fn event_users(&mut self, gid: Id, id: Id) {
        if let Some(e) = &mut self.events {
            e.detail = Some(id);
            if e.users.contains_key(&id) {
                return;
            }
            e.users.insert(id, None);
        }
        self.rest("GET", format!("/guilds/{gid}/scheduled-events/{id}/users?limit=100&with_member=true"), None, Tag::EventUsers(gid, id));
    }
}

pub fn loaded(app: &mut App, gid: Id, part: u8, v: &Value) {
    let Some(e) = app.events.as_mut().filter(|e| e.guild == gid) else { return };
    if part == 2 {
        let path = format!("/guilds/{gid}/scheduled-events?with_user_count=true");
        return app.rest("GET", path, None, Tag::Events(gid, 0));
    }
    let list = v.as_array().cloned().unwrap_or_default();
    if part == 0 {
        e.list = Some(list);
    } else {
        e.rsvp = list.iter().map(|r| sid(&r["guild_scheduled_event_id"])).collect();
    }
}

pub fn users_loaded(app: &mut App, gid: Id, id: Id, v: &Value) {
    if let Some(e) = app.events.as_mut().filter(|e| e.guild == gid) {
        e.users.insert(id, Some(v.as_array().cloned().unwrap_or_default()));
    }
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

/// When it starts, the way discord words it on the card.
fn when(e: &Value) -> String {
    if e["status"].as_u64() == Some(2) {
        return "Happening now".into();
    }
    let Some(t) = e["scheduled_start_time"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Local)) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    let time = t.format(crate::markdown::hm());
    if t.date_naive() == today {
        format!("Today at {time}")
    } else if today.succ_opt() == Some(t.date_naive()) {
        format!("Tomorrow at {time}")
    } else {
        format!("{} at {time}", t.format("%a, %b %-d"))
    }
}

fn plate(ui: &Ui, r: Rect, icon: Icon, text: &str, on: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("event_btn", text, r.min.x as i32, r.min.y as i32)), Sense::click());
    super::settings::mark(ui, &format!("event:{text}"), r);
    let p = ui.painter();
    let fill = if on { Color32::from_rgb(0x00, 0x85, 0x45) } else if resp.hovered() { faint(51) } else { faint(31) };
    p.rect_filled(r, CornerRadius::same(8), fill);
    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, if on { Color32::from_white_alpha(20) } else { faint(61) }), egui::StrokeKind::Inside);
    let ink = if on { Color32::WHITE } else { pal().header_primary };
    if text.is_empty() {
        icons::draw(p, icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), ink);
    } else {
        icons::draw(p, icon, Rect::from_min_size(pos2(r.left() + 12.0, r.center().y - 8.0), vec2(16.0, 16.0)), ink);
        p.text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, text, medium(14.0), ink);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

enum Act {
    Toggle(Id, bool),
    Copy(String),
    Open(Id),
    Status(Id, u8),
    Edit(Value),
    Cancel(Id),
}

/// What the viewer may do with an event: moderators anything, creators their own.
fn can_edit(app: &App, gid: Id, e: &Value) -> bool {
    let perms = app.store.my_perms(gid);
    perms & crate::state::MANAGE_EVENTS != 0 || (perms & crate::state::CREATE_EVENTS != 0 && sid(&e["creator_id"]) == app.store.me)
}

fn item(ui: &mut Ui, icon: Option<Icon>, label: &str, danger: bool) -> bool {
    let r = super::menu_row(ui, icon, label, None, danger, super::Trail::None);
    super::settings::mark(ui, &format!("event_menu:{label}"), r.rect);
    r.clicked()
}

/// One event's card, 604 wide with 24 either side, as in the list and in Event Details.
fn card(app: &App, ui: &mut Ui, e: &Value, gid: Id, rsvp: &HashSet<Id>, acts: &mut Vec<Act>, whole: bool) {
    let id = sid(&e["id"]);
    let w = 554.0;
    let desc = e["description"].as_str().unwrap_or_default().trim().to_string();
    let dg = (!desc.is_empty()).then(|| ui.painter().layout(desc, reg(14.0), pal().text_normal, w - 36.0));
    let dh = dg.as_ref().map_or(0.0, |g| g.size().y + 16.0);
    let h = 50.0 + 32.0 + dh + 61.0;
    let (row, _) = ui.allocate_exact_size(vec2(604.0, h + 16.0), Sense::hover());
    let r = Rect::from_min_size(pos2(row.left() + 24.0, row.top()), vec2(w, h));
    let body = ui.interact(r, ui.id().with(("event_card", id)), if whole { Sense::hover() } else { Sense::click() });
    super::settings::mark(ui, &format!("event_card:{id}"), r);
    if body.hovered() && !whole {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if body.clicked() {
        acts.push(Act::Open(id));
    }
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(8), pal().bg_floating);
    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    let live = e["status"].as_u64() == Some(2);
    icons::draw(p, Icon::Events, Rect::from_min_size(pos2(r.left() + 18.0, r.top() + 18.0), vec2(20.0, 20.0)), if live { GREEN } else { BLURPLE });
    p.text(pos2(r.left() + 46.0, r.top() + 27.0), Align2::LEFT_CENTER, when(e), semi(14.0), if live { GREEN } else { pal().text_normal });
    let interested = rsvp.contains(&id);
    let count = e["user_count"].as_u64().unwrap_or(0);
    let cg = p.layout_no_wrap(count.to_string(), reg(14.0), pal().text_normal);
    let pill = Rect::from_min_size(pos2(r.right() - 16.0 - cg.size().x - 32.0, r.top() + 18.0), vec2(cg.size().x + 32.0, 20.0));
    p.rect_filled(pill, CornerRadius::same(10), pal().bg_primary);
    icons::draw(p, Icon::People, Rect::from_min_size(pos2(pill.left() + 8.0, pill.top() + 4.0), vec2(12.0, 12.0)), pal().text_normal);
    p.galley(pos2(pill.left() + 24.0, pill.center().y - cg.size().y / 2.0), cg, pal().text_normal);
    if let Some(c) = e["creator"].as_object() {
        let url = match c.get("avatar").and_then(|a| a.as_str()) {
            Some(h) => format!("{}/avatars/{}/{h}.png?size=40", crate::api::cdn(), c.get("id").and_then(|i| i.as_str()).unwrap_or_default()),
            None => format!("{}/embed/avatars/0.png", crate::api::cdn()),
        };
        super::avatar(ui, Rect::from_min_size(pos2(pill.left() - 32.0, r.top() + 18.0), vec2(20.0, 20.0)), &url);
    }
    let p = ui.painter();
    super::text::paint(p, pos2(r.left() + 18.0, r.top() + 62.0), Align2::LEFT_CENTER, e["name"].as_str().unwrap_or_default(), medium(20.0), pal().header_primary, w - 36.0);
    if let Some(g) = dg {
        p.galley(pos2(r.left() + 18.0, r.top() + 82.0), g, pal().text_normal);
    }
    let foot = r.bottom() - 61.0;
    p.hline(r.x_range().shrink(2.0), foot + 0.5, Stroke::new(1.0, faint(51)));
    let fy = foot + 30.0;
    let (icon, place) = match e["entity_type"].as_u64() {
        Some(3) => (Icon::Location, e["entity_metadata"]["location"].as_str().unwrap_or_default().to_string()),
        Some(1) => (Icon::Stage, app.store.channels.get(&sid(&e["channel_id"])).map(|c| c.name.clone()).unwrap_or_default()),
        _ => (Icon::Speaker, app.store.channels.get(&sid(&e["channel_id"])).map(|c| c.name.clone()).unwrap_or_default()),
    };
    icons::draw(p, icon, Rect::from_min_size(pos2(r.left() + 18.0, fy - 10.0), vec2(20.0, 20.0)), pal().text_normal);
    super::text::paint(p, pos2(r.left() + 42.0, fy), Align2::LEFT_CENTER, &place, reg(14.0), pal().text_normal, 220.0);
    let ib = Rect::from_min_size(pos2(r.right() - 16.0 - 106.0, fy - 16.0), vec2(106.0, 32.0));
    let cb = Rect::from_min_size(pos2(ib.left() - 8.0 - 103.0, fy - 16.0), vec2(103.0, 32.0));
    let mb = Rect::from_min_size(pos2(cb.left() - 8.0 - 32.0, fy - 16.0), vec2(32.0, 32.0));
    if plate(ui, ib, if interested { Icon::Check } else { Icon::Bell }, "Interested", interested).clicked() {
        acts.push(Act::Toggle(id, !interested));
    }
    if plate(ui, cb, Icon::Link, "Copy Link", false).clicked() {
        acts.push(Act::Copy(format!("https://discord.com/events/{gid}/{id}")));
    }
    let more = plate(ui, mb, Icon::More, "", false);
    super::settings::mark(ui, &format!("event_more:{id}"), mb);
    let status = e["status"].as_u64().unwrap_or(1);
    let editable = can_edit(app, gid, e);
    let dev = app.settings.dev;
    egui::Popup::menu(&more).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if editable {
            if status == 1 && item(ui, None, "Start Event", false) {
                acts.push(Act::Status(id, 2));
            }
            if status == 2 && item(ui, None, "End Event", false) {
                acts.push(Act::Status(id, 3));
            }
            if item(ui, None, "Edit Event", false) {
                acts.push(Act::Edit(e.clone()));
            }
            if status == 1 && item(ui, None, "Cancel Event", true) {
                acts.push(Act::Cancel(id));
            }
        }
        if dev && super::menu_item(ui, Some(Icon::Id), "Copy Event ID", false) {
            ui.ctx().copy_text(id.to_string());
        }
    });
}

fn user_name(u: &Value) -> String {
    let nick = u["member"]["nick"].as_str().filter(|n| !n.is_empty());
    let user = &u["user"];
    let global = user["global_name"].as_str().filter(|n| !n.is_empty());
    nick.or(global).or(user["username"].as_str()).unwrap_or_default().to_string()
}

/// Event Details: the event's card over the list of people interested in it.
fn details(app: &App, ctx: &egui::Context, gid: Id, e: &Value, rsvp: &HashSet<Id>, users: Option<&Vec<Value>>, acts: &mut Vec<Act>) -> bool {
    let mut close = false;
    let screen = ctx.content_rect();
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
    let m = crate::ui::motion::Modal::new("event_details".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(604.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (head, _) = ui.allocate_exact_size(vec2(604.0, 56.0), Sense::hover());
        ui.painter().text(pos2(head.left() + 24.0, head.top() + 30.0), Align2::LEFT_CENTER, "Event Details", semi(16.0), pal().header_primary);
        let x = Rect::from_min_size(pos2(head.right() - 42.0, head.top() + 15.0), vec2(24.0, 24.0));
        let xr = ui.interact(x.expand(8.0), ui.id().with("event_details_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if xr.hovered() { pal().interactive_hover } else { pal().text_muted });
        close = xr.clicked();
        egui::ScrollArea::vertical().max_height((screen.height() - 200.0).max(300.0)).auto_shrink([false, true]).show(ui, |ui| {
            card(app, ui, e, gid, rsvp, acts, true);
            let n = e["user_count"].as_u64().unwrap_or(0);
            let (hr, _) = ui.allocate_exact_size(vec2(604.0, 28.0), Sense::hover());
            ui.painter().text(pos2(hr.left() + 24.0, hr.center().y), Align2::LEFT_CENTER, format!("{n} Interested"), semi(14.0), pal().header_secondary);
            match users {
                None => {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                }
                Some(l) => {
                    for u in l {
                        let (r, _) = ui.allocate_exact_size(vec2(604.0, 44.0), Sense::hover());
                        let user = &u["user"];
                        let url = match user["avatar"].as_str() {
                            Some(h) => format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), user["id"].as_str().unwrap_or_default()),
                            None => format!("{}/embed/avatars/0.png", crate::api::cdn()),
                        };
                        super::avatar(ui, Rect::from_min_size(pos2(r.left() + 24.0, r.center().y - 16.0), vec2(32.0, 32.0)), &url);
                        super::text::paint(ui.painter(), pos2(r.left() + 68.0, r.center().y), Align2::LEFT_CENTER, &user_name(u), medium(16.0), pal().header_primary, 460.0);
                    }
                    ui.add_space(16.0);
                }
            }
        });
    });
    close || m.should_close()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(ev) = app.events.as_ref() else { return };
    let gid = ev.guild;
    let can_create = app.store.my_perms(gid) & (crate::state::MANAGE_EVENTS | crate::state::CREATE_EVENTS) != 0;
    let list = ev.list.clone();
    let rsvp = ev.rsvp.clone();
    let detail = ev.detail.and_then(|d| list.as_ref()?.iter().find(|e| sid(&e["id"]) == d).cloned());
    let users = ev.detail.and_then(|d| ev.users.get(&d)).cloned().flatten();
    let mut close = false;
    let mut create = false;
    let mut acts: Vec<Act> = Vec::new();
    let mut roles = false;
    let mut drop_detail = false;
    let screen = ctx.content_rect();
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
    let m = crate::ui::motion::Modal::new("events".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(604.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (head, _) = ui.allocate_exact_size(vec2(604.0, 56.0), Sense::hover());
        let p = ui.painter();
        icons::draw(p, Icon::Events, Rect::from_min_size(pos2(head.left() + 24.0, head.top() + 18.0), vec2(24.0, 24.0)), pal().text_muted);
        let n = list.as_ref().map_or(0, |l| l.len());
        if can_create {
            let t = p.text(pos2(head.left() + 56.0, head.top() + 30.0), Align2::LEFT_CENTER, "Events", semi(16.0), pal().header_primary);
            p.vline(t.right() + 18.0, head.top() + 22.0..=head.top() + 38.0, Stroke::new(1.0, faint(51)));
            let b = Rect::from_min_size(pos2(t.right() + 36.0, head.top() + 16.0), vec2(102.0, 32.0));
            let br = ui.interact(b, ui.id().with("events_create"), Sense::click());
            ui.painter().rect_filled(b, CornerRadius::same(8), if br.hovered() { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE });
            ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create Event", medium(14.0), Color32::WHITE);
            create = br.clicked();
        } else {
            p.text(pos2(head.left() + 56.0, head.top() + 30.0), Align2::LEFT_CENTER, format!("{n} Event{}", if n == 1 { "" } else { "s" }), semi(16.0), pal().header_primary);
        }
        let x = Rect::from_min_size(pos2(head.right() - 42.0, head.top() + 15.0), vec2(24.0, 24.0));
        let xr = ui.interact(x.expand(8.0), ui.id().with("events_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if xr.hovered() { pal().interactive_hover } else { pal().text_muted });
        close = xr.clicked();
        let max_h = (screen.height() - 200.0).max(300.0);
        egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_min_height(346.0);
            match &list {
                None => {
                    ui.add_space(140.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                }
                Some(l) if l.is_empty() => roles = empty(ui, can_create),
                Some(l) => {
                    for e in l {
                        card(app, ui, e, gid, &rsvp, &mut acts, detail.is_some());
                    }
                }
            }
        });
    });
    if let Some(e) = &detail {
        drop_detail = details(app, ctx, gid, e, &rsvp, users.as_ref(), &mut acts);
    }
    let mut open_detail = None;
    for a in acts {
        match a {
            Act::Copy(u) => {
                ctx.copy_text(u);
                app.toast("Link copied to clipboard.");
            }
            Act::Toggle(id, on) => {
                if let Some(ev) = &mut app.events {
                    if on {
                        ev.rsvp.insert(id);
                    } else {
                        ev.rsvp.remove(&id);
                    }
                    if let Some(e) = ev.list.as_mut().and_then(|l| l.iter_mut().find(|e| sid(&e["id"]) == id)) {
                        let n = e["user_count"].as_u64().unwrap_or(0);
                        e["user_count"] = serde_json::json!(if on { n + 1 } else { n.saturating_sub(1) });
                    }
                    ev.users.remove(&id);
                }
                app.rest(if on { "PUT" } else { "DELETE" }, format!("/guilds/{gid}/scheduled-events/{id}/users/@me"), None, Tag::Quiet);
                if app.events.as_ref().is_some_and(|e| e.detail == Some(id)) {
                    open_detail = Some(id);
                }
            }
            Act::Open(id) => open_detail = Some(id),
            Act::Status(id, to) => {
                let (title, body, button) = if to == 2 {
                    ("Start Event", "You are about to start this event", "Start Event")
                } else {
                    ("End Event", "Are you sure you want to end the event?", "Yes, end the event")
                };
                let c = Confirm::new(title, body, button, move |app| {
                    app.rest("PATCH", format!("/guilds/{gid}/scheduled-events/{id}"), Some(json!({ "status": to })), Tag::Events(gid, 2));
                });
                app.confirm = Some(if to == 2 { c.blurple() } else { c });
            }
            Act::Cancel(id) => {
                app.confirm = Some(Confirm::new("Cancel Event?", "Are you sure you want to cancel this event?", "Cancel Event", move |app| {
                    app.rest("DELETE", format!("/guilds/{gid}/scheduled-events/{id}"), None, Tag::Events(gid, 2));
                    if let Some(e) = &mut app.events {
                        e.detail = None;
                    }
                }));
            }
            Act::Edit(e) => {
                app.events = None;
                app.create = Some(super::create::Form::editing(gid, &e));
                return;
            }
        }
    }
    if let Some(id) = open_detail {
        app.event_users(gid, id);
    }
    if drop_detail {
        if let Some(e) = &mut app.events {
            e.detail = None;
        }
    } else if create {
        app.events = None;
        app.create = Some(super::create::Form::new(super::create::Kind::Event, gid, None));
    } else if roles {
        app.events = None;
        app.open_server_settings(gid);
        if let Some(s) = &mut app.server {
            s.page = super::server::Page::Roles;
        }
    } else if detail.is_none() && (close || m.should_close()) {
        app.events = None;
    }
}

/// The calendar in its black circle with two sparkles, and discord's two lines under it. Hands
/// back a click on the Server Settings > Roles link.
fn empty(ui: &mut Ui, can_create: bool) -> bool {
    let (r, _) = ui.allocate_exact_size(vec2(604.0, 320.0), Sense::hover());
    let p = ui.painter();
    let c = pos2(r.center().x, r.top() + 104.0);
    p.circle_filled(c, 32.0, pal().bg_primary);
    icons::draw(p, Icon::Events, Rect::from_center_size(c, vec2(40.0, 40.0)), pal().header_primary);
    p.text(c + vec2(-38.0, 26.0), Align2::CENTER_CENTER, "\u{2726}", reg(14.0), Color32::from_rgb(0xf0, 0xb2, 0x32));
    p.text(c + vec2(38.0, -26.0), Align2::CENTER_CENTER, "\u{2727}", reg(12.0), Color32::from_rgb(0x4f, 0xd1, 0xc5));
    p.text(pos2(r.center().x, r.top() + 183.0), Align2::CENTER_CENTER, "There are no upcoming events.", semi(24.0), pal().header_primary);
    p.text(pos2(r.center().x, r.top() + 215.0), Align2::CENTER_CENTER, "Schedule an event for any planned activity in your server.", reg(14.0), pal().text_normal);
    if !can_create {
        return false;
    }
    let lead = p.layout_no_wrap("You can give other people permission to create events in ".into(), reg(14.0), pal().text_normal);
    let link = p.layout_no_wrap("Server Settings > Roles".into(), reg(14.0), pal().link);
    let dot = p.layout_no_wrap(".".into(), reg(14.0), pal().text_normal);
    let x = r.center().x - (lead.size().x + link.size().x + dot.size().x) / 2.0;
    let y = r.top() + 237.0 - lead.size().y / 2.0;
    let lr = Rect::from_min_size(pos2(x + lead.size().x, y), link.size());
    p.galley(pos2(x + lead.size().x + link.size().x, y), dot, pal().text_normal);
    p.galley(pos2(x, y), lead, pal().text_normal);
    p.galley(lr.min, link, pal().link);
    let resp = ui.interact(lr, ui.id().with("events_roles"), Sense::click());
    if resp.hovered() {
        ui.painter().hline(lr.x_range(), lr.bottom() - 1.0, Stroke::new(1.0, pal().link));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
