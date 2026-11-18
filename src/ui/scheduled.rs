//! Scheduled messages: a time picked from the composer's plus menu or the poll dialog, the bar
//! over the composer while one is set, and the inbox tab listing what waits to go out.

use chrono::{DateTime, Datelike, Duration, Local, Timelike};
use egui::{Align, Align2, Frame, Layout, Rect, RichText, Sense, Ui, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, chat, menu_item, menu_row, menu_sep, modal_button, text};
use crate::net::{Cmd, Tag};
use crate::state::{Id, Map, Message, Set, Store, iso_ms, sid};
use crate::theme::*;

const SOONEST: i64 = 600;
const FURTHEST: i64 = 691_200;
const SUPPRESS_NOTIFICATIONS: u64 = 1 << 12;

pub struct Item {
    pub id: Id,
    pub ch: Id,
    pub at: i64,
    pub state: u64,
    pub files: usize,
    pub msg: Message,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    Draft(Id),
    Send(Id),
    Poll,
    Item(Id),
}

pub struct Picker {
    target: Target,
    date: String,
    time: String,
    text: String,
}

#[derive(Default)]
pub struct State {
    pub drafts: Map<Id, i64>,
    pub list: Option<Vec<Item>>,
    pub picker: Option<Picker>,
    pub edit: Option<(Id, String)>,
    busy: Set<Id>,
    pub asked: bool,
}

pub fn enabled(store: &Store) -> bool {
    store.experiments.as_ref().is_some_and(|e| e.contains(&super::favorites::hashed("2026-08-scheduled-messages")))
}

pub fn can(store: &Store, ch: Id) -> bool {
    enabled(store) && store.channels.get(&ch).is_some_and(|c| c.guild == 0 || store.perms(c) & crate::state::SEND_MESSAGES != 0)
}

fn at_hour(day: DateTime<Local>, hour: u32) -> DateTime<Local> {
    day.with_hour(hour).and_then(|t| t.with_minute(0)).and_then(|t| t.with_second(0)).and_then(|t| t.with_nanosecond(0)).unwrap_or(day)
}

pub fn presets(now: DateTime<Local>) -> Vec<(&'static str, DateTime<Local>)> {
    let min = now + Duration::seconds(SOONEST);
    let pick = |hour: u32, today: &'static str, tomorrow: &'static str| {
        let t = at_hour(now, hour);
        if t > min { (today, t) } else { (tomorrow, t + Duration::days(1)) }
    };
    let monday = at_hour(now - Duration::days(now.weekday().num_days_from_monday() as i64) + Duration::days(7), 9);
    vec![pick(9, "Today at 9:00am", "Tomorrow at 9:00am"), pick(13, "Today at 1:00pm", "Tomorrow at 1:00pm"), ("Monday at 9:00am", monday)]
}

fn first_guess(now: DateTime<Local>) -> DateTime<Local> {
    let t = at_hour(now, now.hour()) + Duration::hours(1);
    if t < now + Duration::seconds(SOONEST) { t + Duration::hours(1) } else { t }
}

pub fn problem(at: DateTime<Local>, now: DateTime<Local>) -> Option<&'static str> {
    if at < now + Duration::seconds(SOONEST) {
        Some("Scheduled time is too soon")
    } else if at > now + Duration::seconds(FURTHEST) {
        Some("Scheduled time is too far away")
    } else {
        None
    }
}

fn day_and_time(ms: i64, year: bool) -> (String, String) {
    let t = crate::markdown::local(ms);
    (t.format(if year { "%B %-d, %Y" } else { "%B %-d" }).to_string(), t.format(crate::markdown::hm()).to_string())
}

pub fn scheduled_for(ms: i64) -> String {
    let (d, t) = day_and_time(ms, true);
    format!("Message scheduled for {d} at {t}")
}

pub fn scheduling_for(ms: i64, message: bool) -> String {
    let (d, t) = day_and_time(ms, false);
    format!("Scheduling {}for {d} at {t}", if message { "message " } else { "" })
}

fn silent(content: &str, flags: u64) -> (String, u64) {
    match content.strip_prefix("@silent") {
        Some(rest) if rest.is_empty() || rest.starts_with(' ') => (rest.trim_start().to_string(), flags | SUPPRESS_NOTIFICATIONS),
        _ => (content.to_string(), flags & !SUPPRESS_NOTIFICATIONS),
    }
}

fn spoken(content: &str, flags: u64) -> String {
    if flags & SUPPRESS_NOTIFICATIONS != 0 { format!("@silent {content}") } else { content.to_string() }
}

fn iso(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)).unwrap_or_default()
}

impl Picker {
    pub fn push_days(&mut self, n: i64) {
        if let Some(t) = super::create::when(&self.date, &self.time) {
            self.date = (t + Duration::days(n)).format("%Y-%m-%d").to_string();
        }
    }

    pub fn new(target: Target, at: Option<i64>) -> Self {
        let t = at.and_then(DateTime::from_timestamp_millis).map_or_else(|| first_guess(Local::now()), |t| t.with_timezone(&Local));
        Picker { target, date: t.format("%Y-%m-%d").to_string(), time: t.format("%H:%M").to_string(), text: String::new() }
    }

    pub fn saying(mut self, text: String) -> Self {
        self.text = text;
        self
    }
}

impl App {
    pub fn fetch_scheduled(&mut self) {
        if !self.sched.asked {
            self.sched.asked = true;
            self.rest("GET", "/users/@me/scheduled-messages".into(), None, Tag::Keyed("sched:list".into()));
        }
    }

    /// The composer's message out as a scheduled one: the same body, held by discord until `at`.
    pub fn schedule(&mut self, ch: Id, at: i64, mut body: Value, files: Vec<(std::path::PathBuf, String, String)>) {
        let (content, flags) = silent(body["content"].as_str().unwrap_or(""), body["flags"].as_u64().unwrap_or(0));
        body["channel_id"] = json!(ch.to_string());
        body["content"] = json!(content);
        body["flags"] = json!(flags);
        body["scheduled_timestamp"] = json!(iso(at));
        self.sched.drafts.remove(&ch);
        self.keyed.insert(format!("sched:made:{at}"), None);
        self.net.send(Cmd::Schedule { channel: ch, body, files, tag: Tag::Keyed(format!("sched:made:{at}")) });
    }

    fn sched_act(&mut self, id: Id, key: &str, method: &'static str, path: String, body: Option<Value>) {
        self.sched.busy.insert(id);
        self.keyed.insert(format!("sched:{key}:{id}"), None);
        self.rest(method, path, body, Tag::Keyed(format!("sched:{key}:{id}")));
    }
}

fn parse(store: &mut Store, v: &Value) -> Item {
    let mut msg = store.parse_message(&v["message_preview"]);
    let args = &v["create_args"];
    if msg.content.is_empty() {
        msg.content = args["content"].as_str().unwrap_or("").to_string();
    }
    if msg.author == 0 {
        msg.author = store.me;
    }
    let at = iso_ms(&v["send_at_timestamp"]);
    if msg.id == 0 {
        msg.id = ((at - 1_420_070_400_000).max(0) as Id) << 22;
    }
    Item { id: sid(&v["scheduled_message_id"]), ch: sid(&args["channel_id"]), at, state: v["state"].as_u64().unwrap_or(0), files: v["attachment_uploads"].as_array().map_or(0, Vec::len), msg }
}

fn settle(app: &mut App) {
    let done: Vec<String> = app.keyed.iter().filter(|(k, v)| k.starts_with("sched:") && v.is_some()).map(|(k, _)| k.clone()).collect();
    for key in done {
        let Some(Some(res)) = app.keyed.remove(&key) else { continue };
        let mut parts = key.splitn(3, ':').skip(1);
        let (what, arg) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if what == "list" {
            let list = res.ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
            app.sched.list = Some(list.iter().map(|v| parse(&mut app.store, v)).collect());
            continue;
        }
        if let Ok(id) = arg.parse::<Id>() {
            app.sched.busy.remove(&id);
        }
        let said = match (what, res) {
            ("made", Ok(_)) => scheduled_for(arg.parse().unwrap_or(0)),
            ("made", Err(e)) => format!("Failed to schedule message. {e}"),
            ("update", Ok(_)) => "Scheduled message updated".into(),
            ("update", Err(e)) => format!("Failed to update message. {e}"),
            ("cancel", Ok(_)) => "Successfully canceled the message.".into(),
            ("cancel", Err(e)) => format!("Failed to cancel scheduled message. {e}"),
            ("send", Ok(_)) => "Message sent.".into(),
            (_, Err(e)) => format!("Failed to send scheduled message. {e}"),
            _ => continue,
        };
        app.toast(said);
        app.sched.asked = false;
        app.fetch_scheduled();
    }
}

pub fn menu(ui: &mut Ui) -> Option<Option<i64>> {
    let mut pick = None;
    let row = menu_row(ui, Some(Icon::CalendarPlus), "Schedule Message", None, false, super::Trail::Caret);
    super::submenu(ui, &row, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (label, at) in presets(Local::now()) {
            if menu_item(ui, None, label, false) {
                pick = Some(Some(at.timestamp_millis()));
            }
        }
        menu_sep(ui);
        if menu_item(ui, None, "Custom...", false) {
            pick = Some(None);
        }
    });
    pick
}

pub fn bar(app: &mut App, ui: &mut Ui, ch: Id) {
    let Some(at) = app.sched.drafts.get(&ch).copied() else { return };
    Frame::new().fill(pal().bg_secondary).corner_radius(egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let r = ui.add(egui::Label::new(RichText::new(scheduling_for(at, true)).font(reg(14.0)).color(pal().text_normal)).sense(Sense::click()));
            super::settings::mark(ui, "sched:bar", r.rect);
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.on_hover_text("Edit schedule time").clicked() {
                app.sched.picker = Some(Picker::new(Target::Draft(ch), Some(at)));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if super::icon_button(ui, Icon::Close, 20.0, "Close").clicked() {
                    app.sched.drafts.remove(&ch);
                }
            });
        });
    });
}

fn picker(app: &mut App, ctx: &egui::Context) {
    let Some(p) = &mut app.sched.picker else { return };
    let when = super::create::when(&p.date, &p.time);
    let wrong = match when {
        Some(t) => problem(t, Local::now()),
        None => Some("Scheduled time is too soon"),
    };
    let editing = matches!(p.target, Target::Item(_));
    let clearable = p.target == Target::Poll && app.poll.as_ref().is_some_and(|d| d.at.is_some());
    let (mut set, mut clear, mut cancel) = (false, false, false);
    let closed = super::create::modal(ctx, "schedule_time", Some(if editing { "Update Scheduled Message" } else { "Schedule Message" }), |ui| {
        super::create::pair(ui, "Date", &mut p.date, "Time", &mut p.time);
        if let Some(w) = wrong.filter(|_| when.is_some()) {
            ui.label(RichText::new(w).font(reg(12.0)).color(pal().critical));
        }
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let w = (ui.available_width() - 8.0) / 2.0;
            if modal_button(ui, if clearable { "Clear" } else { "Cancel" }, None, w).clicked() {
                clear = clearable;
                cancel = true;
            }
            let go = ui.add_enabled_ui(wrong.is_none(), |ui| modal_button(ui, if editing { "Update" } else { "Set Schedule" }, Some(pal().blurple), w)).inner;
            super::settings::mark(ui, "sched:set", go.rect);
            set = go.clicked();
        });
    });
    let target = p.target;
    let text = p.text.clone();
    if set && let Some(t) = when {
        let ms = t.timestamp_millis();
        match target {
            Target::Draft(ch) => {
                app.sched.drafts.insert(ch, ms);
            }
            Target::Send(ch) => app.schedule(ch, ms, json!({ "content": text, "flags": 0, "sticker_ids": [] }), Vec::new()),
            Target::Poll => {
                if let Some(d) = &mut app.poll {
                    d.at = Some(ms);
                }
            }
            Target::Item(id) => app.sched_act(id, "update", "PATCH", format!("/users/@me/scheduled-messages/{id}"), Some(json!({ "scheduled_timestamp": iso(ms) }))),
        }
        app.sched.picker = None;
    } else if cancel || closed {
        if clear && let Some(d) = &mut app.poll {
            d.at = None;
        }
        app.sched.picker = None;
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    settle(app);
    picker(app, ctx);
}

fn tool(ui: &mut Ui, icon: Icon, tip: &str, off: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let fill = if resp.hovered() && !off { pal().control_hover } else { pal().control };
    ui.painter().rect(r, 8, fill, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), if off { pal().text_muted } else { pal().header_primary });
    super::settings::mark(ui, &format!("sched:tool:{tip}"), r);
    resp.on_hover_text(tip).clicked() && !off
}

/// The inbox's Scheduled tab; hands back the channel of a row that was clicked.
pub fn list(ui: &mut Ui, app: &mut App) -> Option<Id> {
    app.fetch_scheduled();
    let Some(list) = &app.sched.list else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
        ui.add_space(24.0);
        return None;
    };
    if list.is_empty() {
        super::inboxtip::show(ui, super::webart::NOTHING_SCHEDULED, "Send it at the perfect moment", "Draft your message then click the plus button and ", "Schedule Message", " to choose when to send. We'll handle the rest.");
        return None;
    }
    enum Act {
        Send(Id),
        Edit(Id, String),
        Time(Id, i64),
        Cancel(Id),
        Save(Id, String, u64),
        Open(Id),
    }
    let mut act = None;
    let s = &app.store;
    let nitro = s.me_raw["premium_type"].as_u64() == Some(2);
    let mut edit = app.sched.edit.take();
    for it in list {
        let off = app.sched.busy.contains(&it.id);
        Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = match s.channels.get(&it.ch) {
                Some(c) if c.guild != 0 => format!("# {}", c.name),
                Some(c) => super::channel_title(s, c.id),
                None => "Unknown channel".into(),
            };
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let head = text::label(ui, &title, semi(16.0), pal().header_primary, Sense::click());
                    if head.clicked() {
                        act = Some(Act::Open(it.ch));
                    }
                    let (status, ink) = match it.state {
                        0 => (scheduled_for(it.at), pal().header_secondary),
                        3 => ("Message could not be sent, permission was denied".to_string(), pal().critical),
                        _ => ("Message could not be sent".to_string(), pal().critical),
                    };
                    ui.label(RichText::new(status).font(reg(12.0)).color(ink));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if tool(ui, Icon::Trash, "Cancel scheduled message", off) {
                        act = Some(Act::Cancel(it.id));
                    }
                    if tool(ui, Icon::Clock, "Edit schedule time", off) {
                        act = Some(Act::Time(it.id, it.at));
                    }
                    if tool(ui, Icon::Pencil, "Edit message", off) {
                        act = Some(Act::Edit(it.id, spoken(&it.msg.content, it.msg.flags)));
                    }
                    if tool(ui, Icon::Send, "Send now", off) {
                        act = Some(Act::Send(it.id));
                    }
                });
            });
            ui.add_space(8.0);
            match &mut edit {
                Some((id, draft)) if *id == it.id => {
                    let r = ui.add(egui::TextEdit::multiline(draft).desired_rows(1).desired_width(ui.available_width()).font(field_font(ui.ctx(), 16.0)));
                    super::settings::mark(ui, "sched:edit", r.rect);
                    r.request_focus();
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        *id = 0;
                    } else if enter && !draft.trim().is_empty() {
                        act = Some(Act::Save(it.id, draft.trim().to_string(), it.msg.flags));
                        *id = 0;
                    }
                    ui.label(RichText::new("escape to cancel • enter to save").font(reg(12.0)).color(pal().text_muted));
                }
                _ => {
                    let r = chat::preview(ui, s, &it.msg);
                    app.rects.insert(format!("inbox:sched:{}", it.id), r.rect);
                }
            }
            if it.files > 0 {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), Icon::Attach, r, pal().header_secondary);
                    let n = it.files;
                    ui.label(RichText::new(format!("{n} attachment{}", if n == 1 { "" } else { "s" })).font(reg(14.0)).color(pal().header_secondary));
                });
            }
        });
        ui.add_space(16.0);
    }
    if !nitro {
        ui.painter().text(ui.cursor().left_top() + vec2(0.0, 8.0), Align2::LEFT_CENTER, "Schedule even more messages with Nitro.", reg(12.0), pal().text_muted);
        ui.add_space(20.0);
    }
    app.sched.edit = edit.filter(|(id, _)| *id != 0);
    let mut open = None;
    match act {
        Some(Act::Send(id)) => app.sched_act(id, "send", "POST", format!("/users/@me/scheduled-messages/{id}/send"), None),
        Some(Act::Cancel(id)) => app.sched_act(id, "cancel", "DELETE", format!("/users/@me/scheduled-messages/{id}"), None),
        Some(Act::Time(id, at)) => app.sched.picker = Some(Picker::new(Target::Item(id), Some(at))),
        Some(Act::Edit(id, text)) => app.sched.edit = Some((id, text)),
        Some(Act::Save(id, text, flags)) => {
            let (content, flags) = silent(&text, flags);
            app.sched_act(id, "update", "PATCH", format!("/users/@me/scheduled-messages/{id}"), Some(json!({ "content": content, "flags": flags })));
        }
        Some(Act::Open(ch)) => open = Some(ch),
        None => {}
    }
    open
}

pub fn height(app: &App) -> f32 {
    match &app.sched.list {
        None => 90.0,
        Some(l) if l.is_empty() => 100.0,
        Some(l) => l.iter().map(|i| 150.0 + 20.0 * (i.msg.content.lines().count().max(1) - 1 + i.msg.content.len() / 60) as f32).sum::<f32>() + 28.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 11, h, m, 0).single().unwrap()
    }

    #[test]
    fn presets_roll_to_tomorrow_once_today_is_within_ten_minutes() {
        let p = presets(at(8, 0));
        assert_eq!((p[0].0, p[0].1), ("Today at 9:00am", at(9, 0)));
        assert_eq!(p[1].0, "Today at 1:00pm");
        assert_eq!(p[2].1, Local.with_ymd_and_hms(2026, 11, 16, 9, 0, 0).single().unwrap());
        let p = presets(at(8, 55));
        assert_eq!((p[0].0, p[0].1), ("Tomorrow at 9:00am", at(9, 0) + Duration::days(1)));
        assert_eq!(presets(at(13, 30))[1].0, "Tomorrow at 1:00pm");
    }

    #[test]
    fn a_time_must_be_ten_minutes_to_eight_days_out() {
        let now = at(12, 0);
        assert_eq!(problem(now + Duration::minutes(9), now), Some("Scheduled time is too soon"));
        assert_eq!(problem(now + Duration::minutes(11), now), None);
        assert_eq!(problem(now + Duration::days(9), now), Some("Scheduled time is too far away"));
        assert_eq!(first_guess(at(12, 20)), at(13, 0));
        assert_eq!(first_guess(at(12, 55)), at(14, 0));
    }

    #[test]
    fn silent_is_a_flag_not_text() {
        assert_eq!(silent("@silent hi", 0), ("hi".to_string(), SUPPRESS_NOTIFICATIONS));
        assert_eq!(silent("@silently", 0), ("@silently".to_string(), 0));
        assert_eq!(spoken("hi", SUPPRESS_NOTIFICATIONS), "@silent hi");
    }
}
