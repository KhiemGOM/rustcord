//! The pages under a server's top rows that aren't channels: Members, discord's member-safety
//! table for moderators, and Channels & Roles, onboarding's questions and the channel browser.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashSet;

use super::App;
use super::icons::{self, Icon};
use super::motion;
use crate::net::Tag;
use crate::state::{Channel, Id, sid, snowflake_ms};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Members,
    // Customize, then Browse Channels
    Channels(u8),
}

#[derive(Default)]
pub struct Members {
    pub query: String,
    sent: Option<(String, u8)>,
    edited: f64,
    pub sort: u8,
    pub signal: u8,
    pub list: Option<Vec<Value>>,
    pub total: u64,
    pub picked: HashSet<Id>,
}

impl Members {
    pub fn stale(&mut self) {
        self.sent = None;
    }

    pub fn drop_user(&mut self, uid: Id) {
        if let Some(l) = &mut self.list {
            let n = l.len();
            l.retain(|m| sid(&m["member"]["user"]["id"]) != uid);
            self.total = self.total.saturating_sub((n - l.len()) as u64);
        }
        self.picked.remove(&uid);
    }
}

pub struct Home {
    pub guild: Id,
    pub page: Page,
    pub members: Members,
    pub onboarding: Option<Value>,
    pub browse: String,
}

// .headerCell's padding-inline, and each cell's
const CELL: f32 = 8.0;
// a member row
const ROW: f32 = 60.0;

const SORTS: [(u8, &str); 4] = [(1, "Member Since (Newest first)"), (2, "Member Since (Oldest first)"), (3, "Joined Discord (Newest first)"), (4, "Joined Discord (Oldest first)")];

impl App {
    pub fn open_home(&mut self, gid: Id, page: Page) {
        self.boosts = None;
        if let Some(h) = self.home.as_mut().filter(|h| h.guild == gid) {
            h.page = page;
            return;
        }
        self.home = Some(Home { guild: gid, page, members: Members { sort: 1, ..Default::default() }, onboarding: None, browse: String::new() });
        if matches!(page, Page::Channels(_)) || self.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_ONBOARDING")) {
            self.rest("GET", format!("/guilds/{gid}/onboarding"), None, Tag::Home("onboarding"));
        }
    }

    /// Takes a channel off the list of a server that only lists picked ones.
    pub fn opt_out(&mut self, ch: Id) {
        self.chan_flag(ch, super::OPTED, false);
    }
}

pub fn loaded(app: &mut App, key: &'static str, v: Value) {
    if key == "prune" {
        let Some(gid) = app.server.as_ref().map(|s| s.guild).or(app.home.as_ref().map(|h| h.guild)) else { return };
        let n = v["pruned"].as_u64().unwrap_or(0);
        let body = format!("**{n}** member{} who haven't been seen in **7 days** and have no roles will be kicked. They can rejoin with a new invite.", if n == 1 { "" } else { "s" });
        app.confirm = Some(super::Confirm::new("Prune Members", body, "Prune", move |app: &mut App| {
            app.rest("POST", format!("/guilds/{gid}/prune"), Some(json!({ "days": 7, "compute_prune_count": false })), Tag::reason());
        }));
        return;
    }
    let Some(h) = &mut app.home else { return };
    match key {
        "members" => {
            let gid = h.guild;
            let rows = fill(&mut h.members, &v);
            remember(app, gid, rows);
        }
        "onboarding" => h.onboarding = Some(v),
        _ => {}
    }
}

/// Stores a members-search answer in the table, handing back the members in it.
pub fn fill(m: &mut Members, v: &Value) -> Vec<Value> {
    m.total = v["total_result_count"].as_u64().unwrap_or(0);
    m.list = Some(v["members"].as_array().cloned().unwrap_or_default());
    v["members"].as_array().into_iter().flatten().map(|m| m["member"].clone()).collect()
}

/// What the table learned about each member goes where the row's menu looks for their roles.
pub fn remember(app: &mut App, gid: Id, rows: Vec<Value>) {
    for mut m in rows {
        app.store.apply("USER_UPDATE", &m["user"].clone());
        m["guild_id"] = json!(gid.to_string());
        app.store.apply("GUILD_MEMBER_UPDATE", &m);
    }
}

/// "5 days ago", "1 month ago": the member table's way of putting it.
fn ago(ms: i64) -> String {
    let d = (chrono::Utc::now().timestamp_millis() - ms).max(0) / 86_400_000;
    let (n, unit) = if d < 1 {
        let h = (chrono::Utc::now().timestamp_millis() - ms).max(0) / 3_600_000;
        if h < 1 { (0, "") } else { (h, "hour") }
    } else if d < 30 {
        (d, "day")
    } else if d < 365 {
        (d / 30, "month")
    } else {
        (d / 365, "year")
    };
    if unit.is_empty() { "Just now".into() } else { format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" }) }
}

/// "a month ago", "9 days ago": the channel browser's, which reads like moment's.
fn moment(ms: i64) -> String {
    let s = (chrono::Utc::now().timestamp_millis() - ms).max(0) / 1000;
    let (m, h, d) = (s / 60, s / 3600, s / 86400);
    match () {
        _ if s < 45 => "a few seconds ago".into(),
        _ if s < 90 => "a minute ago".into(),
        _ if m < 45 => format!("{} minutes ago", m.max(2)),
        _ if m < 90 => "an hour ago".into(),
        _ if h < 22 => format!("{} hours ago", h.max(2)),
        _ if h < 36 => "a day ago".into(),
        _ if d < 26 => format!("{} days ago", d.max(2)),
        _ if d < 45 => "a month ago".into(),
        _ if d < 320 => format!("{} months ago", (d / 30).max(2)),
        _ if d < 548 => "a year ago".into(),
        _ => format!("{} years ago", (d / 365).max(2)),
    }
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(page) = app.home.as_ref().map(|h| h.page) else { return };
    let (icon, title) = match page {
        Page::Members => (Icon::People, "Members"),
        Page::Channels(_) => (Icon::ChannelsRoles, "Channels & Roles"),
    };
    egui::Panel::top("home_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 20, right: 8, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.left() - 20.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(20.0, 20.0)), pal().channel_default);
            ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, title, medium(16.0), pal().header_primary);
        });
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| match page {
        Page::Members => members(app, ui, gid),
        Page::Channels(tab) => channels(app, ui, gid, tab),
    });
}

fn button(ui: &mut Ui, icon: Option<Icon>, text: &str, ink: Color32) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), ink);
    let w = g.size().x + 24.0 + if icon.is_some() { 20.0 } else { 0.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    ui.painter().rect(r, 8, faint(if resp.hovered() { 51 } else { 31 }), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
    let mut x = r.left() + 12.0;
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, Rect::from_min_size(pos2(x, r.center().y - 8.0), vec2(16.0, 16.0)), ink);
        x += 20.0;
    }
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn checkbox(ui: &Ui, id: egui::Id, r: Rect, on: bool, hot: bool) {
    let ctx = ui.ctx();
    let k = motion::tween(ctx, id.with("fill"), on, 0.15, motion::EASE);
    let edge = Stroke::new(1.0, faint(if hot { 200 } else { 163 }).gamma_multiply(1.0 - k));
    ui.painter().rect(r, 4, motion::mix(Color32::from_black_alpha(20), BLURPLE, k), edge, egui::StrokeKind::Inside);
    let u = motion::tween(ctx, id.with("tick"), on, 0.3, motion::Curve::Bezier(0.65, 0.0, 0.83, 0.83));
    if u > 0.25 {
        let s = motion::keys(&[(0.0, 0.7), (0.25, 0.7), (0.75, 1.1), (1.0, 1.0)], u);
        let c = r.center();
        let p = |x: f32, y: f32| c + vec2(x, y) * s;
        ui.painter().add(egui::Shape::line(vec![p(-5.0, 0.0), p(-1.5, 3.5), p(5.0, -3.5)], Stroke::new(2.0, Color32::WHITE)));
    }
}

fn members(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(mut m) = app.home.as_mut().map(|h| std::mem::take(&mut h.members)) else { return };
    table(app, ui, gid, &mut m, Tag::Home("members"), 16.0);
    if let Some(h) = &mut app.home {
        h.members = m;
    }
}

/// The member table, drawn from `m`; the search answer comes back under `tag`.
pub fn table(app: &mut App, ui: &mut Ui, gid: Id, tb: &mut Members, tag: Tag, inset: f32) {
    // the search goes out once typing pauses, and again whenever the sort changes
    let now = ui.input(|i| i.time);
    let mut ask = None;
    {
        let want = (tb.query.trim().to_string(), tb.sort);
        if tb.sent.as_ref() != Some(&want) && now - tb.edited > 0.35 {
            tb.sent = Some(want.clone());
            let (words, ids): (Vec<&str>, Vec<&str>) = want.0.split_whitespace().partition(|w| w.parse::<u64>().is_err());
            let mut and = json!({});
            if want.0.len() > 1 {
                if !words.is_empty() {
                    and["usernames"] = json!({ "or_query": words });
                }
                if !ids.is_empty() {
                    and["user_id"] = json!({ "or_query": ids });
                }
            }
            ask = Some(json!({ "or_query": {}, "and_query": and, "limit": 250, "sort": want.1 }));
        }
        if tb.sent.as_ref().is_some_and(|s| s.0 != tb.query.trim()) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
    if let Some(body) = ask {
        app.rest("POST", format!("/guilds/{gid}/members-search"), Some(body), tag);
    }
    let list = tb.list.clone().unwrap_or_default();
    let total = tb.total;
    let roles: Vec<(Id, String, u32, i32)> = app.store.guilds.get(&gid).map(|g| g.roles.values().map(|r| (r.id, r.name.clone(), r.color, r.position)).collect()).unwrap_or_default();
    let me = app.store.me;
    let mut acts: Vec<(&str, Id, egui::Pos2)> = Vec::new();
    let mut role_add: Option<(Id, Id)> = None;
    let mut role_drop: Option<(Id, Id)> = None;
    let (mut prune, mut bulk_kick, mut bulk_ban) = (false, false, false);
    let outer = ui.max_rect().shrink(inset);
    let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    let card_top = outer.top();
    // the card's frame goes in under everything once its height is known
    let under = cu.painter().add(egui::Shape::Noop);
    let picked = tb.picked.len();
    // the title row: Recent Members, or how many are picked and what to do with them
    let (head, _) = cu.allocate_exact_size(vec2(outer.width(), 48.0), Sense::hover());
    let p = cu.painter();
    if picked > 0 {
        p.text(pos2(head.left() + 13.0, head.center().y), Align2::LEFT_CENTER, format!("{picked} Selected"), medium(16.0), pal().header_primary);
    } else {
        p.text(pos2(head.left() + 13.0, head.center().y), Align2::LEFT_CENTER, "Recent Members", medium(16.0), pal().header_primary);
    }
    let mut row = cu.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(head.left() + 200.0, head.top() + 8.0), pos2(head.right() - 12.0, head.bottom() - 8.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    row.spacing_mut().item_spacing.x = 8.0;
    if picked > 0 {
        let b = button(&mut row, None, "Ban", pal().critical);
        app.rects.insert("members:bulk:Ban".into(), b.rect);
        bulk_ban = b.clicked();
        let k = button(&mut row, None, "Kick", pal().critical);
        app.rects.insert("members:bulk:Kick".into(), k.rect);
        bulk_kick = k.clicked();
    } else {
        let pr = button(&mut row, None, "Prune", pal().critical);
        app.rects.insert("members:prune".into(), pr.rect);
        prune = pr.on_hover_text("Prune Members").clicked();
        let sr = button(&mut row, Some(Icon::Sort), "Sort", pal().text_normal);
        app.rects.insert("members:sort".into(), sr.rect);
        egui::Popup::menu(&sr).show(|ui| {
            super::menu_body(ui, |ui| {
                for (v, l) in SORTS {
                    let on = tb.sort == v;
                    if super::menu_item(ui, on.then_some(Icon::Check), l, false) {
                        tb.sort = v;
                    }
                }
            });
        });
        // discord's small search box
        let (b, _) = row.allocate_exact_size(vec2(250.0, 32.0), Sense::hover());
        row.painter().rect(b, 8, Color32::from_black_alpha(31), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        icons::draw(row.painter(), Icon::Search, Rect::from_center_size(pos2(b.left() + 16.0, b.center().y), vec2(12.0, 12.0)), pal().text_normal);
        let field = Rect::from_min_max(pos2(b.left() + 30.0, b.top()), pos2(b.right() - 8.0, b.bottom()));
        let before = tb.query.clone();
        let r = row.put(
            field,
            egui::TextEdit::singleline(&mut tb.query)
                .frame(Frame::NONE)
                .font(reg(14.0))
                .vertical_align(egui::Align::Center)
                .hint_text(RichText::new("Search by username or id").font(reg(14.0)).color(pal().text_muted))
                .desired_width(field.width()),
        );
        app.rects.insert("members:search".into(), r.rect);
        if tb.query != before {
            tb.edited = now;
        }
    }
    let flags = |m: &Value| {
        let mem = &m["member"];
        let timed = mem["communication_disabled_until"].as_str().is_some_and(|t| chrono::DateTime::parse_from_rfc3339(t).is_ok_and(|t| t.timestamp() > chrono::Utc::now().timestamp()));
        (timed, mem["unusual_dm_activity_until"].is_string())
    };
    // the table is auto layout, 100% wide: the date, join method and signals columns ask for 11% of it, roles 22%,
    // and the name column takes what's left of it, none going under what its content needs (.smallCol, .mediumCol)
    let word = |t: &str, f: egui::FontId| cu.painter().layout_no_wrap(t.into(), f, pal().text_normal).size().x;
    let inner = outer.width() - 2.0 - 2.0 * CELL;
    let code = list.iter().filter_map(|m| m["source_invite_code"].as_str()).max_by_key(|c| c.chars().count()).unwrap_or("");
    let flagged = list.iter().any(|m| flags(m) != (false, false));
    let hw = |t: &str| word(t, bold(12.0));
    let mins = [
        (32.0f32 + 8.0 + 24.0).max(hw("NAME")) + 2.0 * CELL,
        word("months", medium(14.0)).max(hw("MEMBER")) + 2.0 * CELL,
        word("months", medium(14.0)).max(hw("DISCORD")) + 2.0 * CELL,
        (word(code, medium(14.0)) + 24.0).max(hw("METHOD")) + 2.0 * CELL,
        (40.0 + 28.0f32).max(hw("ROLES")) + 2.0 * CELL,
        if flagged { word("Unusual DMs", medium(12.0)) + 16.0 } else { 0.0 }.max(hw("SIGNALS") + 24.0) + 2.0 * CELL,
    ];
    let share = [0.0, 0.11, 0.11, 0.11, 0.22, 0.11];
    let mut w: [f32; 6] = std::array::from_fn(|i| (share[i] * inner).max(mins[i]));
    let (check_w, act_w) = (20.0 + 2.0 * CELL, 28.0 * 2.0 + 8.0 + 2.0 * CELL);
    w[0] = (inner - check_w - act_w - w[1..].iter().sum::<f32>()).max(mins[0]);
    let want = (check_w + act_w + w.iter().sum::<f32>() + 2.0 + 2.0 * CELL).max(outer.width());
    let rows_h = (outer.bottom() - head.bottom() - 57.0 - 64.0).max(3.0 * ROW);
    let mut picks: Vec<(Id, bool)> = Vec::new();
    egui::ScrollArea::horizontal().id_salt(("members_wide", gid)).auto_shrink([false, true]).show(&mut cu, |cu| {
        cu.set_min_width(want);
        let table = Rect::from_min_size(pos2(outer.left() + 1.0 + CELL, head.bottom()), vec2(want - 2.0 - 2.0 * CELL, 0.0));
        let mut starts = [0.0; 7];
        starts[0] = table.left() + check_w;
        for i in 0..6 {
            starts[i + 1] = starts[i] + w[i];
        }
        let col = |i: usize| starts[i] + CELL;
        let end = |i: usize| starts[i] + w[i] - CELL;
        let (hr, _) = cu.allocate_exact_size(vec2(want, 57.0), Sense::hover());
        cu.painter().hline(hr.x_range(), hr.top() + 0.5, Stroke::new(1.0, pal().border_subtle));
        let all = !list.is_empty() && list.iter().all(|m| tb.picked.contains(&sid(&m["member"]["user"]["id"])));
        let cb = Rect::from_min_size(pos2(table.left() + CELL, hr.center().y - 10.0), vec2(20.0, 20.0));
        let cr = cu.interact(cb, cu.id().with("pick_all"), Sense::click());
        app.rects.insert("members:pick_all".into(), cb);
        checkbox(cu, cr.id, cb, all, cr.hovered());
        if cr.clicked() {
            if all {
                tb.picked.clear();
            } else {
                tb.picked = list.iter().map(|m| sid(&m["member"]["user"]["id"])).filter(|u| *u != me).collect();
            }
        }
        for (i, t) in ["NAME", "MEMBER SINCE", "JOINED DISCORD", "JOIN METHOD", "ROLES", "SIGNALS"].iter().enumerate() {
            let g = cu.painter().layout(t.to_string(), bold(12.0), pal().text_normal, (end(i) - col(i)).max(20.0));
            let at = pos2(col(i), hr.center().y - g.size().y / 2.0);
            cu.painter().galley(at, g, pal().text_normal);
        }
        let sig_text = cu.painter().layout_no_wrap("SIGNALS".into(), bold(12.0), pal().text_normal).size().x;
        let sig = Rect::from_center_size(pos2(col(5) + sig_text + 16.0, hr.center().y), vec2(24.0, 24.0));
        let sr = cu.interact(sig, cu.id().with("signal_filter"), Sense::click());
        app.rects.insert("members:signal".into(), sig);
        icons::draw(cu.painter(), Icon::Sort, Rect::from_center_size(sig.center(), vec2(14.0, 14.0)), if tb.signal != 0 { BLURPLE } else { pal().text_normal });
        egui::Popup::menu(&sr.on_hover_text("Filter based on safety info, such as timed out or engaging in unusual DM activity.")).show(|ui| {
            super::menu_body(ui, |ui| {
                for (v, l) in [(0, "All members"), (1, "Timed out"), (2, "Unusual DM activity")] {
                    if super::menu_item(ui, (tb.signal == v).then_some(Icon::Check), l, false) {
                        tb.signal = v;
                    }
                }
            });
        });
        egui::ScrollArea::vertical().id_salt(("members_table", gid)).max_height(rows_h).auto_shrink([false, true]).show(cu, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if tb.list.is_none() {
                ui.add_space(24.0);
                crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
            }
            for m in &list {
                let mem = &m["member"];
                let u = &mem["user"];
                let (timed, unusual) = flags(m);
                if (tb.signal == 1 && !timed) || (tb.signal == 2 && !unusual) {
                    continue;
                }
                let uid = sid(&u["id"]);
                let (r, resp) = ui.allocate_exact_size(vec2(want, ROW), Sense::click());
                crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 0.0)), 8, faint(20), 0.05);
                ui.painter().hline(r.x_range().shrink(8.0), r.bottom() - 0.5, Stroke::new(1.0, faint(31)));
                let y = r.center().y;
                let cell = |i: usize| ui.painter().with_clip_rect(Rect::from_min_max(pos2(col(i), r.top()), pos2(end(i), r.bottom())));
                let room = |i: usize| (end(i) - col(i)).max(0.0);
                let cb = Rect::from_min_size(pos2(table.left() + CELL, y - 10.0), vec2(20.0, 20.0));
                let on = tb.picked.contains(&uid);
                let c = ui.interact(cb, ui.id().with(("pick", uid)), Sense::click());
                app.rects.insert(format!("members:pick:{uid}"), cb);
                if uid != me {
                    checkbox(ui, c.id, cb, on, c.hovered());
                    if c.clicked() {
                        picks.push((uid, !on));
                    }
                }
                let user = app.store.user(uid).cloned();
                let av = Rect::from_min_size(pos2(col(0), y - 16.0), vec2(32.0, 32.0));
                if let Some(user) = &user {
                    super::avatar(ui, av, &user.avatar_url());
                }
                let mine: Vec<&(Id, String, u32, i32)> = {
                    let ids: Vec<Id> = mem["roles"].as_array().into_iter().flatten().map(sid).collect();
                    let mut v: Vec<_> = roles.iter().filter(|r| ids.contains(&r.0)).collect();
                    v.sort_by_key(|r| std::cmp::Reverse(r.3));
                    v
                };
                let ink = mine.iter().find(|r| r.2 != 0).map_or(pal().header_primary, |r| role_color(r.2));
                let name = mem["nick"].as_str().or(u["global_name"].as_str()).or(u["username"].as_str()).unwrap_or_default().to_string();
                let text_w = (room(0) - 40.0).max(0.0);
                let p = cell(0);
                let chip = user.as_ref().and_then(|u| u.tag.as_ref()).map_or(0.0, |(t, _, _)| 4.0 + 8.0 + 16.0 + p.layout_no_wrap(t.clone(), semi(12.0), pal().text_normal).size().x);
                let n = super::text::paint_em(&p, pos2(av.right() + 8.0, y - 9.0), Align2::LEFT_CENTER, &name, medium(14.0), ink, (text_w - chip).max(0.0), 1.0);
                if let Some(user) = &user {
                    super::tag_chip(&p, user, pos2(n.right() + 4.0, y - 9.0), 1.0);
                }
                super::text::paint(&p, pos2(av.right() + 8.0, y + 10.0), Align2::LEFT_CENTER, u["username"].as_str().unwrap_or_default(), reg(12.0), pal().text_normal, text_w);
                let joined = mem["joined_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis());
                for (i, ms) in [(1, joined), (2, Some(snowflake_ms(uid)))] {
                    if let Some(ms) = ms {
                        let g = ui.painter().layout(ago(ms), medium(14.0), pal().text_normal, room(i));
                        ui.painter().galley(pos2(col(i), y - g.size().y / 2.0), g, pal().text_normal);
                    }
                }
                let p = cell(3);
                match m["source_invite_code"].as_str() {
                    Some(code) => {
                        let inviter = sid(&m["inviter_id"]);
                        let top = if inviter != 0 { y - 17.0 } else { y - 9.0 };
                        let w = (room(3) - 4.0).max(24.0);
                        let g = p.layout_no_wrap(code.to_string(), medium(14.0), pal().text_normal);
                        let chip = Rect::from_min_size(pos2(col(3), top), vec2((g.size().x + 24.0).min(w), 18.0));
                        p.rect_filled(chip, 4, faint(31));
                        icons::draw(&p, Icon::Link, Rect::from_min_size(pos2(chip.left() + 4.0, chip.center().y - 6.0), vec2(12.0, 12.0)), pal().text_normal);
                        super::text::paint(&p, pos2(chip.left() + 20.0, chip.center().y), Align2::LEFT_CENTER, code, medium(14.0), pal().text_normal, (chip.width() - 24.0).max(0.0));
                        if inviter != 0 {
                            let t = p.text(pos2(col(3), y + 10.0), Align2::LEFT_CENTER, "Invited by", medium(12.0), pal().text_normal);
                            let mut x = t.right() + 4.0;
                            if let Some(iu) = app.store.user(inviter) {
                                super::avatar(ui, Rect::from_min_size(pos2(x, y + 2.0), vec2(16.0, 16.0)), &iu.avatar_url());
                                x += 20.0;
                            }
                            let ik = app.store.role_color(gid, inviter).map(role_color).unwrap_or(pal().header_primary);
                            super::text::paint(&p, pos2(x, y + 10.0), Align2::LEFT_CENTER, &app.store.name_in(gid, inviter), medium(12.0), ik, (end(3) - x).max(0.0));
                        }
                    }
                    None => {
                        p.text(pos2(col(3), y), Align2::LEFT_CENTER, "Unknown", medium(14.0), pal().text_muted);
                    }
                }
                let p = cell(4);
                let mut x = col(4);
                let more_w = if mine.len() > 1 { 28.0 } else { 0.0 };
                if let Some(top) = mine.first() {
                    let g = p.layout_no_wrap(top.1.clone(), medium(12.0), pal().text_normal);
                    let w = (g.size().x + 28.0).min((room(4) - more_w - 24.0).max(40.0));
                    let chip = Rect::from_min_size(pos2(x, y - 12.0), vec2(w, 24.0));
                    let rc = if top.2 == 0 { Color32::from_rgb(196, 201, 206) } else { role_color(top.2) };
                    p.rect_filled(chip, 4, Color32::from_rgba_unmultiplied(rc.r(), rc.g(), rc.b(), 39));
                    let dot = Rect::from_center_size(pos2(chip.left() + 14.0, y), vec2(12.0, 12.0));
                    let dr = ui.interact(dot.expand(2.0), ui.id().with(("role_dot", uid)), Sense::click());
                    app.rects.insert(format!("members:role_dot:{uid}"), dot);
                    p.circle_filled(dot.center(), 6.0, rc);
                    if dr.hovered() {
                        icons::draw(&p, Icon::Close, dot.shrink(2.0), Color32::BLACK);
                    }
                    if dr.on_hover_text(format!("Remove role {}", top.1)).clicked() {
                        role_drop = Some((uid, top.0));
                    }
                    super::text::paint(&p, pos2(chip.left() + 24.0, y), Align2::LEFT_CENTER, &top.1, medium(12.0), pal().text_normal, (w - 28.0).max(0.0));
                    x = chip.right() + 4.0;
                }
                if mine.len() > 1 {
                    let more = Rect::from_min_size(pos2(x, y - 12.0), vec2(24.0, 24.0));
                    p.rect_filled(more, 4, Color32::BLACK);
                    p.text(more.center(), Align2::CENTER_CENTER, format!("+{}", mine.len() - 1), medium(12.0), pal().header_primary);
                    let mr = ui.interact(more, ui.id().with(("more_roles", uid)), Sense::click());
                    let names: Vec<String> = mine.iter().map(|r| r.1.clone()).collect();
                    mr.on_hover_text(names.join("\n"));
                    x = more.right() + 8.0;
                }
                let adding = egui::Id::new(("add_role_open", uid));
                let open = egui::Popup::is_any_open(ui.ctx()) && ui.data(|d| d.get_temp::<bool>(adding)).unwrap_or(false);
                if !open {
                    ui.data_mut(|d| d.remove::<bool>(adding));
                }
                if x + 16.0 <= end(4) && (open || resp.hovered() || ui.rect_contains_pointer(Rect::from_min_size(pos2(x, y - 10.0), vec2(20.0, 20.0)))) {
                    let plus = Rect::from_min_size(pos2(x, y - 8.0), vec2(16.0, 16.0));
                    let pr = ui.interact(plus, ui.id().with(("add_role", uid)), Sense::click());
                    app.rects.insert(format!("members:add_role:{uid}"), plus);
                    if pr.clicked() {
                        ui.data_mut(|d| d.insert_temp(adding, true));
                    }
                    icons::draw(&p, Icon::CirclePlus, plus, crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &pr)));
                    egui::Popup::menu(&pr).show(|ui| {
                        super::menu_body(ui, |ui| {
                            let mut left: Vec<&(Id, String, u32, i32)> = roles.iter().filter(|r| r.0 != gid && !mine.iter().any(|m| m.0 == r.0)).collect();
                            left.sort_by_key(|r| std::cmp::Reverse(r.3));
                            for r in left {
                                if super::menu_item(ui, None, &r.1, false) {
                                    role_add = Some((uid, r.0));
                                }
                            }
                        });
                    });
                }
                let p = cell(5);
                let mut sx = col(5);
                for (on, label) in [(timed, "Timed out"), (unusual, "Unusual DMs")] {
                    if on {
                        let g = p.layout_no_wrap(label.into(), medium(12.0), pal().text_normal);
                        let chip = Rect::from_min_size(pos2(sx, y - 11.0), vec2(g.size().x + 16.0, 22.0));
                        p.rect_filled(chip, 4, faint(31));
                        p.galley(pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0), g, pal().text_normal);
                        sx = chip.right() + 4.0;
                    }
                }
                for (k, (icon, tip)) in [(Icon::People, "Open Mod View"), (Icon::More, "More Options")].into_iter().enumerate() {
                    let b = Rect::from_center_size(pos2(starts[6] + CELL + 14.0 + k as f32 * 36.0, y), vec2(28.0, 28.0));
                    let br = ui.interact(b, ui.id().with(("member_btn", uid, k)), Sense::click());
                    app.rects.insert(format!("members:btn:{uid}:{k}"), b);
                    if br.hovered() {
                        ui.painter().circle_filled(b.center(), 14.0, faint(41));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(18.0, 18.0)), pal().header_secondary);
                    if br.on_hover_text(tip).clicked() {
                        acts.push((if k == 0 { "mod" } else { "menu" }, uid, b.left_bottom()));
                    }
                }
                app.rects.insert(format!("members:row:{uid}"), r);
                if resp.clicked() {
                    acts.push(("profile", uid, pos2(col(0), r.top())));
                }
            }
        });
    });
    let shown = list.len() as u64;
    cu.add_space(16.0);
    let (f, _) = cu.allocate_exact_size(vec2(outer.width(), 24.0), Sense::hover());
    let foot = if total > shown { format!("Showing {shown} of {total} members") } else { format!("Showing {shown} members") };
    cu.painter().text(pos2(f.left() + 16.0, f.center().y), Align2::LEFT_CENTER, foot, reg(16.0), pal().text_muted);
    cu.add_space(16.0);
    let card = Rect::from_min_max(pos2(outer.left(), card_top), pos2(outer.right(), cu.min_rect().bottom()));
    cu.painter().set(under, egui::Shape::rect_stroke(card, CornerRadius::same(12), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside));
    for (u, on) in picks {
        if on {
            tb.picked.insert(u);
        } else {
            tb.picked.remove(&u);
        }
    }
    let chosen: Vec<Id> = tb.picked.iter().copied().collect();
    for (what, uid, at) in acts {
        match what {
            "profile" => app.open_profile(uid, gid, at),
            "mod" => app.open_mod_view(gid, uid),
            _ => app.user_menu = Some((uid, gid, None, at)),
        }
    }
    if let Some((u, r)) = role_add {
        app.rest("PUT", format!("/guilds/{gid}/members/{u}/roles/{r}"), Some(json!({})), Tag::rollback(|app: &mut App| refresh(app)));
        refresh(app);
    }
    if let Some((u, r)) = role_drop {
        app.rest("DELETE", format!("/guilds/{gid}/members/{u}/roles/{r}"), None, Tag::rollback(|app: &mut App| refresh(app)));
        refresh(app);
    }
    if prune {
        app.rest("GET", format!("/guilds/{gid}/prune?days=7"), None, Tag::Home("prune"));
    }
    if bulk_kick || bulk_ban {
        let n = chosen.len();
        let (title, verb) = if bulk_ban { ("Ban Members", "ban") } else { ("Kick Members", "kick") };
        let body = format!("Are you sure you want to {verb} **{n}** member{} from the server?", if n == 1 { "" } else { "s" });
        app.confirm = Some(super::Confirm::new(title, body, if bulk_ban { "Ban" } else { "Kick" }, move |app: &mut App| {
            if bulk_ban {
                let ids: Vec<String> = chosen.iter().take(200).map(|u| u.to_string()).collect();
                app.rest("POST", format!("/guilds/{gid}/bulk-ban/v2"), Some(json!({ "user_ids": ids, "delete_message_seconds": 0 })), Tag::reason());
            } else {
                for u in &chosen {
                    app.rest("DELETE", format!("/guilds/{gid}/members/{u}"), None, Tag::fail(format!("Whoops... {} could not be kicked.  Please try again!", app.store.name_in(gid, *u))));
                }
            }
            for t in tables(app) {
                t.picked.clear();
            }
            refresh(app);
        }));
    }
}

/// Asks the table again, after something in it changed.
pub fn refresh(app: &mut App) {
    for m in tables(app) {
        m.sent = None;
    }
}

fn tables(app: &mut App) -> impl Iterator<Item = &mut Members> {
    app.home.as_mut().map(|h| &mut h.members).into_iter().chain(app.server.as_mut().map(|s| &mut s.pages.members))
}

fn channels(app: &mut App, ui: &mut Ui, gid: Id, tab: u8) {
    let me = app.store.me;
    let listed: Vec<Channel> = {
        let s = &app.store;
        let mut v: Vec<Channel> = s.channels.values().filter(|c| c.guild == gid && !c.is_thread() && c.kind != 4 && s.can_view(c)).cloned().collect();
        v.sort_by_key(|c| (s.channels.get(&c.parent).map_or(-1, |p| p.position), c.is_voice(), c.position, c.id));
        v
    };
    let mut goto = None;
    egui::ScrollArea::vertical().id_salt(("channels_roles", gid, tab)).auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: 32, right: 32, top: 24, bottom: 32 }).show(ui, |ui| {
            // the tabs, discord's blurple line under the open one
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, pal().border_subtle));
            let mut x = bar.left();
            for (i, label) in ["Customize", "Browse Channels"].into_iter().enumerate() {
                let on = tab == i as u8;
                let ink = if on { Color32::from_rgb(0x7c, 0x88, 0xf7) } else { pal().header_primary };
                let g = ui.painter().layout_no_wrap(label.into(), semi(16.0), ink);
                let mut w = g.size().x;
                let count = (i == 1).then(|| ui.painter().layout_no_wrap(listed.len().to_string(), bold(12.0), pal().header_primary));
                if let Some(c) = &count {
                    w += c.size().x + 18.0;
                }
                let r = Rect::from_min_size(pos2(x, bar.top()), vec2(w, 40.0));
                let resp = ui.interact(r, ui.id().with(("home_tab", i)), Sense::click());
                ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, ink);
                if let Some(c) = count {
                    let pill = Rect::from_min_size(pos2(r.right() - c.size().x - 8.0, r.center().y - 9.0), vec2(c.size().x + 8.0, 18.0));
                    ui.painter().rect_filled(pill, 4, faint(41));
                    ui.painter().galley(pos2(pill.left() + 4.0, pill.center().y - c.size().y / 2.0), c, pal().header_primary);
                }
                if on {
                    ui.painter().rect_filled(Rect::from_min_max(pos2(r.left(), bar.bottom() - 2.0), pos2(r.right(), bar.bottom())), 0, BLURPLE);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                app.rects.insert(format!("home:tab:{i}"), r);
                if resp.clicked() {
                    goto = Some(i as u8);
                }
                x = r.right() + 56.0;
            }
            ui.add_space(32.0);
            if tab == 0 {
                customize(app, ui, gid, me);
            } else {
                browse(app, ui, gid, &listed);
            }
        });
    });
    if let Some(t) = goto {
        app.open_home(gid, Page::Channels(t));
    }
}

fn customize(app: &mut App, ui: &mut Ui, gid: Id, me: Id) {
    let Some(h) = app.home.as_mut() else { return };
    let Some(ob) = h.onboarding.clone() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let mut chosen: Vec<Id> = ob["responses"].as_array().into_iter().flatten().map(sid).collect();
    let mut changed = false;
    let mut add: Vec<Id> = Vec::new();
    let mut remove: Vec<Id> = Vec::new();
    let fresh = fresh(app, gid);
    let w = ui.available_width();
    let side = 300.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 40.0;
        ui.allocate_ui_with_layout(vec2(w - side - 40.0, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(w - side - 40.0);
            ui.label(RichText::new("Customization Question").font(semi(16.0)).color(pal().header_primary));
            ui.label(RichText::new("Answer questions to get access to more channels and roles.").font(reg(12.0)).color(pal().header_secondary));
            ui.add_space(20.0);
            for prompt in ob["prompts"].as_array().into_iter().flatten() {
                let single = prompt["single_select"].as_bool() == Some(true);
                Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(Margin::symmetric(20, 20)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(prompt["title"].as_str().unwrap_or_default()).font(semi(16.0)).color(pal().header_primary));
                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                        for o in prompt["options"].as_array().into_iter().flatten() {
                            let id = sid(&o["id"]);
                            let on = chosen.contains(&id);
                            let title = o["title"].as_str().unwrap_or_default().to_string();
                            let emoji = o["emoji"]["name"].as_str().map(str::to_string);
                            let label = format!("{}{title}", emoji.map(|e| format!("{e} ")).unwrap_or_default());
                            let g = super::text::layout_em(ui.ctx(), &label, medium(16.0), pal().text_normal, f32::INFINITY, 1.0);
                            let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + 40.0).max(270.0), 48.0), Sense::click());
                            ui.painter().rect(r, 8, crate::ui::motion::mix(Color32::from_black_alpha(31), faint(31), crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(if on { 2.0 } else { 1.0 }, if on { BLURPLE } else { faint(61) }), egui::StrokeKind::Inside);
                            g.paint(ui.painter(), pos2(r.left() + 20.0, r.center().y - g.size().y / 2.0));
                            if on {
                                let c = pos2(r.right(), r.top());
                                ui.painter().circle_filled(c, 9.0, BLURPLE);
                                ui.painter().add(egui::Shape::line(vec![c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0), c + vec2(4.0, -3.0)], Stroke::new(1.6, Color32::WHITE)));
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            app.rects.insert(format!("home:option:{title}"), r);
                            if resp.clicked() {
                                let channels = |o: &Value| -> Vec<Id> { o["channel_ids"].as_array().into_iter().flatten().map(sid).collect() };
                                let picked: Vec<&Value> = prompt["options"].as_array().into_iter().flatten().filter(|p| chosen.contains(&sid(&p["id"]))).collect();
                                let mine = channels(o);
                                if on {
                                    let kept: Vec<Id> = picked.iter().filter(|p| sid(&p["id"]) != id).flat_map(|p| channels(p)).collect();
                                    remove.extend(picked.iter().flat_map(|p| channels(p)).filter(|c| !kept.contains(c)));
                                    chosen.retain(|c| *c != id);
                                } else {
                                    if single {
                                        let before = picked.first().map(|p| channels(p)).unwrap_or_default();
                                        add.extend(mine.iter().filter(|c| !before.contains(c)));
                                        remove.extend(before.iter().filter(|c| !mine.contains(c)));
                                        let ids: Vec<Id> = prompt["options"].as_array().into_iter().flatten().map(|o| sid(&o["id"])).collect();
                                        chosen.retain(|c| !ids.contains(c));
                                    } else {
                                        add.extend(mine);
                                    }
                                    chosen.push(id);
                                }
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(16.0);
                    ui.label(RichText::new(if single { "Choose one." } else { "Choose all that apply." }).font(reg(12.0)).color(pal().header_secondary));
                });
                ui.add_space(20.0);
            }
        });
        ui.allocate_ui_with_layout(vec2(side, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(side);
            ui.label(RichText::new("My Profile").font(semi(16.0)).color(pal().header_primary));
            ui.label(RichText::new("This is how you'll look to others!").font(reg(12.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            Frame::new().inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
                let (av, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
                if let Some(u) = app.store.user(me) {
                    super::avatar(ui, av, &u.avatar_url().replace("size=80", "size=128"));
                }
                ui.add_space(12.0);
                ui.label(RichText::new(app.store.name_in(gid, me)).font(medium(20.0)).color(pal().header_primary));
                ui.add_space(12.0);
                let (l, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(l, 0, faint(51));
                ui.add_space(16.0);
                ui.label(RichText::new("ROLES").font(bold(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                let g = app.store.guilds.get(&gid);
                let mut mine: Vec<(String, u32, i32)> = app.store.members.get(&gid).and_then(|m| m.get(&me)).map(|m| m.roles.iter().filter_map(|r| g.and_then(|g| g.roles.get(r))).map(|r| (r.name.clone(), r.color, r.position)).collect()).unwrap_or_default();
                mine.sort_by_key(|r| std::cmp::Reverse(r.2));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for (name, c, _) in mine {
                        let t = ui.painter().layout_no_wrap(name, medium(12.0), pal().text_normal);
                        let (r, _) = ui.allocate_exact_size(vec2(t.size().x + 30.0, 24.0), Sense::hover());
                        ui.painter().rect_filled(r, 12, faint(31));
                        ui.painter().circle_filled(pos2(r.left() + 12.0, r.center().y), 5.0, if c == 0 { Color32::from_rgb(196, 201, 206) } else { role_color(c) });
                        ui.painter().galley(pos2(r.left() + 22.0, r.center().y - t.size().y / 2.0), t, pal().text_normal);
                    }
                });
            });
        });
    });
    if changed {
        // discord saves each answer as it's picked, and counts it seen
        let now = chrono::Utc::now().timestamp_millis();
        let seen: serde_json::Map<String, Value> = ob["prompts"].as_array().into_iter().flatten().map(|p| (sid(&p["id"]).to_string(), json!(now))).collect();
        let picked: serde_json::Map<String, Value> = chosen.iter().map(|c| (c.to_string(), json!(now))).collect();
        let body = json!({ "onboarding_responses": chosen.iter().map(|c| c.to_string()).collect::<Vec<_>>(), "onboarding_prompts_seen": seen, "onboarding_responses_seen": picked });
        app.rest("PUT", format!("/guilds/{gid}/onboarding-responses"), Some(body), Tag::reason());
        if fresh && let Some(ob) = app.home.as_ref().and_then(|h| h.onboarding.as_ref()).filter(|o| o["enabled"].as_bool() == Some(true)) {
            add.extend(ob["default_channel_ids"].as_array().into_iter().flatten().map(sid));
        }
        if !add.is_empty() || !remove.is_empty() {
            let mut overrides = serde_json::Map::new();
            for (ids, on) in [(&remove, false), (&add, true)] {
                for id in ids {
                    let flags = app.store.chan_flags.entry(*id).or_default();
                    if on {
                        *flags |= super::OPTED;
                    } else {
                        *flags &= !super::OPTED;
                    }
                    overrides.insert(id.to_string(), json!({ "flags": *flags }));
                    if on {
                        app.store.opted.insert(*id);
                    } else {
                        app.store.opted.remove(id);
                    }
                }
            }
            app.guild_settings(gid, json!({ "channel_overrides": overrides }), Tag::Quiet);
        }
        if let Some(h) = &mut app.home
            && let Some(ob) = &mut h.onboarding
        {
            ob["responses"] = json!(chosen.iter().map(|c| c.to_string()).collect::<Vec<_>>());
        }
    }
}

const COMPLETED_ONBOARDING: u64 = 2;

fn follow(app: &mut App, gid: Id, id: Id, cat: bool, fresh: bool, listed: &[Channel]) {
    let on = app.store.opted.contains(&id);
    if !on && let Some(c) = app.store.channels.get(&id).map(|c| if cat { id } else { c.parent }) {
        app.ui.collapsed.remove(&c);
    }
    if !fresh {
        if on {
            app.opt_out(id);
        } else {
            app.opt_in(id);
        }
        return;
    }
    let mut ids: Vec<Id> = listed.iter().filter(|c| cat || c.id != id).map(|c| c.id).collect();
    if cat {
        ids.push(id);
    }
    if let Some(ob) = app.home.as_ref().and_then(|h| h.onboarding.as_ref()).filter(|o| o["enabled"].as_bool() == Some(true)) {
        ids.extend(ob["default_channel_ids"].as_array().into_iter().flatten().map(sid));
    }
    ids.sort_unstable();
    ids.dedup();
    app.opt_in_many(gid, &ids);
}

fn fresh(app: &App, gid: Id) -> bool {
    !app.store.opt_in.contains(&gid)
        && app.store.guilds.get(&gid).is_some_and(|g| g.my_flags & COMPLETED_ONBOARDING == 0)
        && !app.store.opted.iter().any(|id| app.store.channels.get(id).is_some_and(|c| c.guild == gid))
}

fn browse(app: &mut App, ui: &mut Ui, gid: Id, listed: &[Channel]) {
    let fresh = fresh(app, gid);
    let mut create = false;
    let Some(h) = app.home.as_mut() else { return };
    let w = ui.available_width();
    ui.horizontal(|ui| {
        let (b, _) = ui.allocate_exact_size(vec2(w - 140.0, 44.0), Sense::hover());
        ui.painter().rect(b, 8, Color32::from_black_alpha(31), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(b.left() + 22.0, b.center().y), vec2(18.0, 18.0)), pal().text_normal);
        let field = Rect::from_min_max(pos2(b.left() + 44.0, b.top()), pos2(b.right() - 12.0, b.bottom()));
        ui.put(
            field,
            egui::TextEdit::singleline(&mut h.browse)
                .frame(Frame::NONE)
                .font(reg(16.0))
                .vertical_align(egui::Align::Center)
                .hint_text(RichText::new("Search Channels").font(reg(16.0)).color(pal().text_muted))
                .desired_width(field.width()),
        );
        ui.add_space(20.0);
        create = super::button(ui, "+ Create", BLURPLE, 120.0).clicked();
    });
    let q = h.browse.trim().to_lowercase();
    ui.add_space(16.0);
    let mut toggle: Option<(Id, bool)> = None;
    let mut open: Option<Id> = None;
    let mut groups: Vec<(Id, Vec<&Channel>)> = Vec::new();
    for c in listed.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q)) {
        match groups.iter_mut().find(|(p, _)| *p == c.parent) {
            Some((_, v)) => v.push(c),
            None => groups.push((c.parent, vec![c])),
        }
    }
    for (parent, chans) in groups {
        let (hr, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        let cat = app.store.channels.get(&parent);
        let title = cat.map_or("Uncategorized".to_string(), |c| c.name.clone());
        super::text::paint_em(ui.painter(), pos2(hr.left(), hr.center().y), Align2::LEFT_CENTER, &title, semi(20.0), pal().header_primary, w - 240.0, 1.0);
        if let Some(cat) = cat {
            let followed = app.store.opted.contains(&cat.id);
            let sw = Rect::from_min_size(pos2(hr.right() - 48.0, hr.center().y - 12.0), vec2(48.0, 24.0));
            let sr = ui.interact(sw, ui.id().with(("follow", cat.id)), Sense::click());
            super::settings::paint_switch(ui, sw, &sr, followed);
            ui.painter().text(pos2(sw.left() - 16.0, hr.center().y), Align2::RIGHT_CENTER, "Follow Category", medium(16.0), pal().header_primary);
            app.rects.insert(format!("browse:follow:{}", cat.id), sw);
            if sr.clicked() {
                toggle = Some((cat.id, true));
            }
        }
        let locked = app.store.opted.contains(&parent);
        ui.add_space(4.0);
        Frame::new().stroke(Stroke::new(1.0, faint(41))).corner_radius(8).show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let n = chans.len();
            for (i, c) in chans.into_iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(w, 67.0), Sense::click());
                let hot = resp.contains_pointer();
                if !locked {
                    crate::ui::motion::wash(ui, &resp, r, 8, faint(20), 0.05);
                }
                if i + 1 < n {
                    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, faint(41)));
                }
                let name = if locked { pal().text_muted } else { pal().header_primary };
                super::side::chan_icon(&app.store, ui.painter(), c, Rect::from_center_size(pos2(r.left() + 26.0, r.top() + 22.0), vec2(20.0, 20.0)), pal().channel_default);
                super::text::paint_em(ui.painter(), pos2(r.left() + 46.0, r.top() + 22.0), Align2::LEFT_CENTER, &c.name, medium(16.0), name, w - 240.0, 1.0);
                if c.last_msg != 0 {
                    ui.painter().text(pos2(r.left() + 16.0, r.top() + 47.0), Align2::LEFT_CENTER, format!("Active {}", moment(snowflake_ms(c.last_msg))), medium(12.0), pal().text_normal);
                }
                let on = fresh || app.store.opted.contains(&c.id) || locked;
                let cb = Rect::from_center_size(pos2(r.right() - 28.0, r.center().y), vec2(20.0, 20.0));
                checkbox(ui, resp.id, cb, on, hot && !locked);
                app.rects.insert(format!("browse:chan:{}", c.id), cb);
                let label = if c.is_forum() { "Open" } else if c.is_voice() { "Join" } else { "View" };
                let g = ui.painter().layout_no_wrap(label.into(), semi(14.0), Color32::BLACK);
                let btn = Rect::from_center_size(pos2(cb.left() - 16.0 - (g.size().x + 24.0).max(60.0) / 2.0, r.center().y), vec2((g.size().x + 24.0).max(60.0), 30.0));
                let br = ui.interact(btn, resp.id.with("open"), Sense::click());
                if hot {
                    let fill = if br.hovered() { Color32::from_rgb(0xe0, 0xe0, 0xe3) } else { Color32::WHITE };
                    ui.painter().rect_filled(btn, 8, fill);
                    ui.painter().galley(pos2(btn.center().x - g.size().x / 2.0, btn.center().y - g.size().y / 2.0), g, Color32::from_rgb(6, 6, 7));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    app.rects.insert(format!("browse:open:{}", c.id), btn);
                }
                if br.clicked() {
                    if c.is_voice() {
                        app.join_voice(c.id, true);
                    } else {
                        open = Some(c.id);
                    }
                } else if resp.clicked() && !locked {
                    toggle = Some((c.id, false));
                }
                if locked && hot {
                    resp.on_hover_text("You must unfollow this category to add or remove specific channels.");
                }
            }
        });
        ui.add_space(20.0);
    }
    if let Some(id) = open {
        app.open(id);
    }
    if let Some((id, cat)) = toggle {
        follow(app, gid, id, cat, fresh, listed);
    }
    if create {
        app.create = Some(super::create::Form::new(super::create::Kind::Channel, gid, None));
    }
}
