//! Member Applications: when a server makes people apply to join, its Members page gains Pending,
//! Rejected and Approved tabs. Each lists the requests; one opens beside the table to approve,
//! reject or read what the person answered.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::App;
use super::icons::{self, Icon};
use super::motion;
use crate::net::Tag;
use crate::state::{ADMINISTRATOR, BAN_MEMBERS, Id, KICK_MEMBERS, sid, snowflake_ms};
use crate::theme::*;

const PAGE: usize = 100;
const STATUS: [&str; 3] = ["SUBMITTED", "REJECTED", "APPROVED"];
const TABS: [&str; 4] = ["All Members", "Pending", "Rejected", "Approved"];
const DETAIL: f32 = 340.0;

#[derive(Default)]
pub struct Joins {
    guilds: HashMap<Id, Book>,
}

#[derive(Default)]
struct Book {
    tab: usize,
    oldest: bool,
    lists: [Vec<Value>; 3],
    fetched: [bool; 3],
    loading: [bool; 3],
    done: [bool; 3],
    pending: u64,
    selected: Option<String>,
    others: HashMap<Id, Vec<Value>>,
    reject: Option<(String, String)>,
    busy: bool,
}

fn key(r: &Value, i: usize) -> Id {
    sid(if i == 0 { &r["join_request_id"] } else { &r["actioned_at"] })
}

fn rid(r: &Value) -> String {
    r["join_request_id"].as_str().unwrap_or_default().to_string()
}

impl App {
    fn book(&mut self, gid: Id) -> &mut Book {
        self.joins.guilds.entry(gid).or_default()
    }

    fn join_users(&mut self, list: &[Value]) {
        for r in list {
            self.store.apply("USER_UPDATE", &r["user"].clone());
            if r["actioned_by_user"].is_object() {
                self.store.apply("USER_UPDATE", &r["actioned_by_user"].clone());
            }
        }
    }
}

pub fn applies(app: &App, gid: Id) -> bool {
    app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "MEMBER_VERIFICATION_MANUAL_APPROVAL")) && app.store.my_perms(gid) & (KICK_MEMBERS | ADMINISTRATOR) != 0
}

pub fn tab(app: &App, gid: Id) -> usize {
    app.joins.guilds.get(&gid).map_or(0, |b| b.tab)
}

pub fn counts(app: &App, gid: Id) -> Option<([usize; 3], u64, bool)> {
    app.joins.guilds.get(&gid).map(|b| ([b.lists[0].len(), b.lists[1].len(), b.lists[2].len()], b.pending, b.oldest))
}

fn fetch(app: &mut App, gid: Id, i: usize) {
    let oldest = app.book(gid).oldest;
    let b = app.book(gid);
    if b.loading[i] || b.done[i] {
        return;
    }
    b.loading[i] = true;
    b.fetched[i] = true;
    let last = b.lists[i].last().map(|r| key(r, i));
    let cursor = match (oldest, last) {
        (false, None) => format!("before={}", (chrono::Utc::now().timestamp_millis() as u64 - 1_420_070_400_000) << 22),
        (false, Some(l)) => format!("before={l}"),
        (true, None) => format!("after={gid}"),
        (true, Some(l)) => format!("after={l}"),
    };
    app.rest("GET", format!("/guilds/{gid}/requests?status={}&limit={PAGE}&{cursor}", STATUS[i]), None, Tag::Joins(gid, i as u8));
}

pub fn loaded(app: &mut App, gid: Id, i: u8, v: &Value) {
    let i = i as usize;
    let list: Vec<Value> = v["guild_join_requests"].as_array().cloned().unwrap_or_default();
    app.join_users(&list);
    let b = app.book(gid);
    b.loading[i] = false;
    b.done[i] = list.len() < PAGE;
    if i == 0 && let Some(t) = v["total"].as_u64() {
        b.pending = t;
    }
    for r in list {
        if !b.lists[i].iter().any(|x| rid(x) == rid(&r)) {
            b.lists[i].push(r);
        }
    }
}

pub fn failed(app: &mut App, gid: Id, i: u8) {
    let b = app.book(gid);
    b.loading[i as usize] = false;
    b.done[i as usize] = true;
}

pub fn event(app: &mut App, t: &str, d: &Value) {
    let gid = sid(&d["guild_id"]);
    let me = app.store.me;
    if !app.joins.guilds.contains_key(&gid) {
        return;
    }
    let req = &d["request"];
    if t != "GUILD_JOIN_REQUEST_DELETE" && sid(&req["user_id"]) == me {
        return;
    }
    let id = if t == "GUILD_JOIN_REQUEST_DELETE" { d["id"].as_str().unwrap_or_default().to_string() } else { rid(req) };
    if t != "GUILD_JOIN_REQUEST_DELETE" {
        app.join_users(std::slice::from_ref(req));
    }
    let b = app.book(gid);
    let was = (0..3).find(|i| b.lists[*i].iter().any(|r| rid(r) == id));
    for l in &mut b.lists {
        l.retain(|r| rid(r) != id);
    }
    let was_pending = was == Some(0);
    let now = (t != "GUILD_JOIN_REQUEST_DELETE").then(|| STATUS.iter().position(|s| req["application_status"] == *s)).flatten();
    if was_pending && now != Some(0) {
        b.pending = b.pending.saturating_sub(1);
    } else if !was_pending && now == Some(0) {
        b.pending += 1;
    }
    if let Some(n) = now.filter(|n| b.fetched[*n]) {
        let at = b.lists[n].iter().position(|r| key(r, n) < key(req, n)).unwrap_or(b.lists[n].len());
        let at = if b.oldest { b.lists[n].len() } else { at };
        b.lists[n].insert(at, req.clone());
    }
    if b.selected.as_deref() == Some(id.as_str()) && now.is_none() {
        b.selected = None;
    }
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

/// The row of tabs under the Members title, when there are applications to review.
pub fn header(app: &mut App, ui: &mut Ui, gid: Id, members: bool) -> bool {
    let tabbed = members && applies(app, gid);
    if tabbed {
        egui::Panel::top("home_tabs")
            .exact_size(49.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 20, right: 8, top: 0, bottom: 1 }))
            .show(ui, |ui| tabs(app, ui, gid));
    }
    tabbed
}

fn tabs(app: &mut App, ui: &mut Ui, gid: Id) {
    if !app.book(gid).fetched[0] {
        fetch(app, gid, 0);
    }
    let pending = app.book(gid).pending;
    let r = ui.max_rect();
    ui.painter().hline(r.left() - 20.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (i, t) in TABS.iter().enumerate() {
            let label = if i == 1 && pending > 0 { format!("Pending ({pending})") } else { t.to_string() };
            let sel = tab(app, gid) == i;
            let g = ui.painter().layout_no_wrap(label, medium(14.0), pal().header_primary);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click());
            app.rects.insert(format!("joins:tab:{i}"), r);
            if sel {
                ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_selected);
            } else {
                motion::wash(ui, &resp, r, 8, pal().bg_mod_hover, 0.3);
            }
            let col = if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary };
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, g.text(), medium(14.0), col);
            if resp.clicked() && !sel {
                let b = app.book(gid);
                b.tab = i;
                b.selected = None;
                if i > 0 && !b.fetched[i - 1] {
                    fetch(app, gid, i - 1);
                }
            }
        }
    });
}

fn when(id: Id, long: bool) -> String {
    let t = chrono::DateTime::from_timestamp_millis(snowflake_ms(id)).unwrap_or_default().with_timezone(&chrono::Local);
    t.format(if long { "%B %-d, %Y" } else { "%b %-d, %Y, %-I:%M %p" }).to_string()
}

fn iso(s: &Value, long: bool) -> String {
    let t = s.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&chrono::Local));
    t.map_or_else(String::new, |t| t.format(if long { "%B %-d, %Y" } else { "%b %-d, %Y, %-I:%M %p" }).to_string())
}

fn name(app: &App, r: &Value) -> (String, String) {
    let uid = sid(&r["user_id"]);
    let user = app.store.users.get(&uid);
    (user.map_or_else(|| r["user"]["username"].as_str().unwrap_or_default().to_string(), |u| u.display().to_string()), user.map_or_else(|| r["user"]["username"].as_str().unwrap_or_default().to_string(), |u| u.username.clone()))
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    let i = tab(app, gid) - 1;
    let sel = app.book(gid).selected.clone();
    let selected = sel.and_then(|s| app.book(gid).lists.iter().flatten().find(|r| rid(r) == s).cloned());
    if let Some(r) = &selected {
        let uid = sid(&r["user_id"]);
        if !app.book(gid).others.contains_key(&uid) {
            app.book(gid).others.insert(uid, Vec::new());
            app.rest("GET", format!("/guilds/{gid}/requests/users/{uid}"), None, Tag::JoinsFor(gid, uid));
        }
    }
    if selected.is_some() {
        egui::Panel::right("join_detail")
            .exact_size(DETAIL)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary).stroke(Stroke::new(1.0, pal().border_subtle)))
            .show(ui, |ui| detail(app, ui, gid, selected.as_ref().unwrap()));
    }
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| table(app, ui, gid, i));
}

fn table(app: &mut App, ui: &mut Ui, gid: Id, i: usize) {
    let oldest = app.book(gid).oldest;
    let mut rows = app.book(gid).lists[i].clone();
    rows.sort_by_key(|r| key(r, i));
    if !oldest {
        rows.reverse();
    }
    let selected = app.book(gid).selected.clone();
    let mut pick = None;
    let mut flip = false;
    let mut menu_act: Option<(&str, Id)> = None;
    let (can_mod, perms) = (super::modview::allowed(&app.store, gid), app.store.my_perms(gid));
    Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
        let (h, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        let date_x = h.right() - 360.0;
        ui.painter().text(pos2(h.left() + 8.0, h.center().y), Align2::LEFT_CENTER, "NAME", semi(12.0), pal().header_secondary);
        let col = if i == 0 { "APPLIED" } else { "REVIEWED" };
        let sort = Rect::from_min_size(pos2(date_x, h.top()), vec2(140.0, 40.0));
        let resp = ui.interact(sort, ui.id().with("join_sort"), Sense::click());
        app.rects.insert("joins:sort".into(), sort);
        ui.painter().text(pos2(date_x + 8.0, h.center().y), Align2::LEFT_CENTER, col, semi(12.0), pal().header_secondary);
        let arrow = pos2(date_x + 78.0, h.center().y);
        let dir = if oldest { -1.0 } else { 1.0 };
        ui.painter().add(egui::Shape::line(vec![arrow + vec2(-4.0, -2.0 * dir), arrow + vec2(0.0, 2.0 * dir), arrow + vec2(4.0, -2.0 * dir)], Stroke::new(1.5, pal().header_secondary)));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        flip = resp.clicked();
        ui.painter().hline(h.x_range(), h.bottom(), Stroke::new(1.0, pal().border_subtle));
        if rows.is_empty() {
            empty(ui, i, app.book(gid).loading[i]);
            return;
        }
        let out = ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for r in &rows {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
                let on = selected.as_deref() == Some(rid(r).as_str());
                if on {
                    ui.painter().rect_filled(row, CornerRadius::same(4), pal().bg_mod_selected);
                } else {
                    motion::wash(ui, &resp, row, 4, pal().bg_mod_hover, 0.05);
                }
                app.rects.insert(format!("joins:row:{}", rid(r)), row);
                let av = Rect::from_min_size(pos2(row.left() + 8.0, row.center().y - 20.0), vec2(40.0, 40.0));
                let uid = sid(&r["user_id"]);
                let url = app.store.users.get(&uid).map(|u| u.avatar_url()).unwrap_or_default();
                super::avatar(ui, av, &url);
                let (display, user) = name(app, r);
                ui.painter().text(pos2(av.right() + 12.0, row.center().y - 9.0), Align2::LEFT_CENTER, display, medium(14.0), pal().header_primary);
                ui.painter().text(pos2(av.right() + 12.0, row.center().y + 9.0), Align2::LEFT_CENTER, user, reg(12.0), pal().text_normal);
                let at = if i == 0 { when(sid(&r["join_request_id"]), false) } else { when(sid(&r["actioned_at"]), false) };
                ui.painter().text(pos2(date_x + 8.0, row.center().y), Align2::LEFT_CENTER, at, reg(14.0), pal().text_normal);
                let more = (i == 0).then(|| {
                    let b = Rect::from_center_size(pos2(row.right() - 28.0, row.center().y), vec2(28.0, 28.0));
                    let br = ui.interact(b, ui.id().with(("join_more", rid(r))), Sense::click());
                    app.rects.insert(format!("joins:more:{}", rid(r)), b);
                    if br.hovered() {
                        ui.painter().circle_filled(b.center(), 14.0, faint(41));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    icons::draw(ui.painter(), Icon::More, Rect::from_center_size(b.center(), vec2(18.0, 18.0)), pal().header_secondary);
                    br.on_hover_text("More Options")
                });
                if let Some(br) = &more {
                    let outranks = uid != app.store.me && super::usermenu::rank(&app.store, gid, app.store.me) > super::usermenu::rank(&app.store, gid, uid);
                    egui::Popup::menu(br).width(220.0).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if can_mod && super::menu_item(ui, None, "Open in Mod View", false) {
                            menu_act = Some(("mod", uid));
                        }
                        if outranks && perms & BAN_MEMBERS != 0 && super::menu_item(ui, None, "Ban User", true) {
                            menu_act = Some(("ban", uid));
                        }
                        if super::menu_item(ui, None, "Copy User ID", false) {
                            menu_act = Some(("copy", uid));
                        }
                    });
                }
                if resp.clicked() {
                    pick = Some(rid(r));
                }
                ui.painter().hline(row.x_range(), row.bottom(), Stroke::new(1.0, pal().border_subtle));
            }
        });
        if out.state.offset.y + out.inner_rect.height() > out.content_size.y - 200.0 && !app.book(gid).done[i] {
            fetch(app, gid, i);
        }
    });
    match menu_act {
        Some(("mod", u)) => app.open_mod_view(gid, u),
        Some(("ban", u)) => app.ban(gid, u),
        Some((_, u)) => ui.ctx().copy_text(u.to_string()),
        None => {}
    }
    if flip {
        let b = app.book(gid);
        b.oldest = !b.oldest;
        b.lists = Default::default();
        b.fetched = [false; 3];
        b.done = [false; 3];
        b.loading = [false; 3];
        fetch(app, gid, i);
    }
    if let Some(p) = pick {
        let b = app.book(gid);
        b.selected = if b.selected.as_deref() == Some(p.as_str()) { None } else { Some(p) };
    }
}

fn empty(ui: &mut Ui, i: usize, loading: bool) {
    if loading {
        motion::spinner(ui, 24.0, pal().text_muted);
        return;
    }
    let (title, body) = [("There are no pending applications.", "You\u{2019}ll be able to manage new member applications here."), ("There are no rejected applications.", "You\u{2019}ll be able to view rejected applications here."), ("There are no approved applications.", "You\u{2019}ll be able to view approved member applications here.")][i];
    let r = ui.available_rect_before_wrap();
    let c = pos2(r.center().x, r.top() + 120.0);
    ui.painter().circle_filled(c, 24.0, faint(51));
    icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(c, vec2(24.0, 24.0)), pal().header_secondary);
    ui.painter().text(pos2(c.x, c.y + 48.0), Align2::CENTER_CENTER, title, medium(16.0), pal().header_primary);
    ui.painter().text(pos2(c.x, c.y + 72.0), Align2::CENTER_CENTER, body, medium(14.0), pal().header_secondary);
}

fn action(ui: &mut Ui, label: &str, danger: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), pal().header_primary);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 36.0), Sense::click());
    let fill = if danger { pal().critical } else { faint(if resp.hovered() { 51 } else { 31 }) };
    ui.painter().rect_filled(r, CornerRadius::same(8), if danger && resp.hovered() { fill.gamma_multiply(0.85) } else { fill });
    ui.painter().galley(r.center() - g.size() / 2.0, g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(16.0);
    ui.label(RichText::new(text).font(medium(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
}

fn line(ui: &mut Ui, l: &str, r: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(l).font(reg(14.0)).color(pal().text_normal));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(r).font(reg(14.0)).color(pal().text_normal));
        });
    });
}

fn detail(app: &mut App, ui: &mut Ui, gid: Id, r: &Value) {
    let uid = sid(&r["user_id"]);
    let id = rid(r);
    let status = r["application_status"].as_str().unwrap_or("SUBMITTED").to_string();
    let (display, user) = name(app, r);
    let mut close = false;
    let (mut approve, mut reject) = (false, false);
    let busy = app.book(gid).busy;
    Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
        let av = Rect::from_min_size(head.min, vec2(48.0, 48.0));
        let url = app.store.users.get(&uid).map(|u| u.avatar_url()).unwrap_or_default();
        super::avatar(ui, av, &url);
        ui.painter().text(pos2(av.right() + 12.0, head.center().y - 10.0), Align2::LEFT_CENTER, display, medium(18.0), pal().header_primary);
        ui.painter().text(pos2(av.right() + 12.0, head.center().y + 10.0), Align2::LEFT_CENTER, user, medium(14.0), pal().text_normal);
        let x = Rect::from_center_size(pos2(head.right() - 12.0, head.center().y), vec2(24.0, 24.0));
        let resp = ui.interact(x, ui.id().with("join_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
        close = resp.clicked();
        app.rects.insert("joins:close".into(), x);
        ui.add_space(16.0);
        match status.as_str() {
            "SUBMITTED" => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let a = ui.add_enabled_ui(!busy, |ui| action(ui, "Approve", false)).inner;
                    app.rects.insert("joins:approve".into(), a.rect);
                    approve = a.clicked();
                    let b = ui.add_enabled_ui(!busy, |ui| action(ui, "Reject", false)).inner;
                    app.rects.insert("joins:reject".into(), b.rect);
                    reject = b.clicked();
                });
            }
            s => {
                let by = r["actioned_by_user"].as_object().and_then(|u| u.get("global_name").filter(|g| g.is_string()).or(u.get("username"))).and_then(|v| v.as_str()).unwrap_or_default();
                ui.label(RichText::new(if s == "APPROVED" { "Approved" } else { "Rejected" }).font(medium(16.0)).color(pal().header_primary));
                if !by.is_empty() {
                    ui.label(RichText::new(format!("By {by} \u{2022} {}", when(sid(&r["actioned_at"]), true))).font(reg(14.0)).color(pal().header_secondary));
                }
                if let Some(why) = r["rejection_reason"].as_str().filter(|w| !w.is_empty()) {
                    ui.label(RichText::new(format!("Reason: {why}")).font(reg(14.0)).color(pal().header_secondary));
                }
            }
        }
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for f in r["form_responses"].as_array().into_iter().flatten() {
                let label = f["label"].as_str().unwrap_or_default();
                let answer = match f["field_type"].as_str() {
                    Some("MULTIPLE_CHOICE") => f["response"].as_u64().and_then(|i| f["values"].get(i as usize)).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    Some("TERMS") => if f["response"].as_bool() == Some(true) { "I have read and agree to the rules.".into() } else { String::new() },
                    _ => f["response"].as_str().unwrap_or_default().to_string(),
                };
                caption(ui, label);
                Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(answer).font(reg(14.0)).color(pal().text_normal));
                });
            }
            caption(ui, "Account");
            line(ui, "Discord Join Date", &when(uid, true));
            ui.add_space(4.0);
            line(ui, "Application Date", &iso(&r["created_at"], true));
            let others: Vec<Value> = app.book(gid).others.get(&uid).map(|l| l.iter().filter(|o| rid(o) != id).cloned().collect()).unwrap_or_default();
            if !others.is_empty() {
                caption(ui, "Other Applications");
                for o in &others {
                    let s = o["application_status"].as_str().unwrap_or_default();
                    line(ui, &iso(&o["created_at"], true), match s { "APPROVED" => "Approved", "REJECTED" => "Rejected", _ => "" });
                    ui.add_space(4.0);
                }
            }
        });
    });
    if close {
        app.book(gid).selected = None;
    }
    if approve {
        app.book(gid).busy = true;
        app.rest("PATCH", format!("/guilds/{gid}/requests/{id}"), Some(json!({ "action": "APPROVED" })), Tag::JoinsAct(gid));
    }
    if reject {
        app.book(gid).reject = Some((id, String::new()));
    }
}

pub fn done(app: &mut App, gid: Id, ok: bool, v: &Value) {
    app.book(gid).busy = false;
    if ok {
        let mut d = json!({ "guild_id": gid.to_string(), "status": v["application_status"], "request": v });
        d["request"]["user_id"] = v["user_id"].clone();
        event(app, "GUILD_JOIN_REQUEST_UPDATE", &d);
    }
}

pub fn others(app: &mut App, gid: Id, uid: Id, v: &Value) {
    let list: Vec<Value> = v.as_array().cloned().unwrap_or_default();
    app.book(gid).others.insert(uid, list);
}

pub fn modals(app: &mut App, ctx: &egui::Context) {
    let Some(gid) = app.joins.guilds.iter().find(|(_, b)| b.reject.is_some()).map(|(g, _)| *g) else { return };
    let Some((id, mut reason)) = app.book(gid).reject.clone() else { return };
    let (mut ok, mut cancel) = (false, false);
    let modal = motion::Modal::new("join_reject".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.label(RichText::new("Give the user a reason for the denial (OPTIONAL)").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(16.0);
            let r = Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut reason).frame(Frame::NONE).desired_rows(3).desired_width(f32::INFINITY).font(reg(14.0)));
            });
            app.rects.insert("joins:reason".into(), r.response.rect);
            reason = reason.chars().take(160).collect();
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = super::button(ui, "Reject", pal().critical, 96.0);
                    app.rects.insert("joins:reject_ok".into(), b.rect);
                    ok = b.clicked();
                    cancel = super::grey_button(ui, "Cancel", 96.0).clicked();
                });
            });
        });
    if ok {
        let body = json!({ "action": "REJECTED", "rejection_reason": Some(reason.clone()).filter(|r| !r.is_empty()) });
        let b = app.book(gid);
        b.reject = None;
        b.busy = true;
        app.rest("PATCH", format!("/guilds/{gid}/requests/{id}"), Some(body), Tag::JoinsAct(gid));
    } else if cancel || modal.should_close() {
        app.book(gid).reject = None;
    } else if let Some(b) = app.joins.guilds.get_mut(&gid) {
        b.reject = Some((id, reason));
    }
}
