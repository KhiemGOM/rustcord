//! Discord's Create Channel, Create Category and Create Event modals, from the server menu and a
//! category's plus.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use chrono::Datelike;
use serde_json::json;

use super::App;
use super::icons::{self, Icon};
use super::settings::switch;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Channel,
    Category,
    Event,
}

pub struct Form {
    pub kind: Kind,
    pub guild: Id,
    pub parent: Option<Id>,
    // channel type: 0 text, 2 voice, 15 forum
    pub chan: u8,
    pub name: String,
    pub private: bool,
    // the event's three steps, then where, what and when
    pub step: u8,
    pub voice: bool,
    pub place: Option<Id>,
    pub location: String,
    pub date: String,
    pub time: String,
    pub end_date: String,
    pub end_time: String,
    pub about: String,
    pub busy: bool,
    // an event being edited, and whether it has started (its start can't change then)
    pub edit: Option<Id>,
    pub started: bool,
    // how it repeats: index into repeats()
    pub repeat: u8,
    // the cover picked: its file name and the data uri that goes to discord
    pub cover: Option<(String, String)>,
    pub stage: bool,
}

impl Form {
    pub fn new(kind: Kind, guild: Id, parent: Option<Id>) -> Self {
        // discord starts an event on the next hour
        let start = chrono::Local::now() + chrono::Duration::hours(1);
        let date = start.format("%Y-%m-%d").to_string();
        let time = start.format("%H:00").to_string();
        Form {
            kind,
            guild,
            parent,
            chan: 0,
            name: String::new(),
            private: false,
            step: 0,
            voice: true,
            place: None,
            location: String::new(),
            end_date: date.clone(),
            end_time: (start + chrono::Duration::hours(1)).format("%H:00").to_string(),
            date,
            time,
            about: String::new(),
            busy: false,
            edit: None,
            started: false,
            repeat: 0,
            cover: None,
            stage: false,
        }
    }

    /// The event wizard filled in from an event as discord sent it.
    pub fn editing(guild: Id, e: &serde_json::Value) -> Self {
        let mut f = Form::new(Kind::Event, guild, None);
        let local = |k: &str| e[k].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Local));
        f.edit = Some(crate::state::sid(&e["id"]));
        f.started = e["status"].as_u64() == Some(2);
        f.name = e["name"].as_str().unwrap_or_default().to_string();
        f.about = e["description"].as_str().unwrap_or_default().to_string();
        f.voice = e["entity_type"].as_u64() != Some(3);
        f.place = Some(crate::state::sid(&e["channel_id"])).filter(|c| *c != 0);
        f.location = e["entity_metadata"]["location"].as_str().unwrap_or_default().to_string();
        if let Some(s) = local("scheduled_start_time") {
            f.date = s.format("%Y-%m-%d").to_string();
            f.time = s.format("%H:%M").to_string();
        }
        if let Some(t) = local("scheduled_end_time") {
            f.end_date = t.format("%Y-%m-%d").to_string();
            f.end_time = t.format("%H:%M").to_string();
        }
        let rule = &e["recurrence_rule"];
        f.repeat = match (rule["frequency"].as_u64(), rule["interval"].as_u64()) {
            (Some(2), Some(2)) => 2,
            (Some(2), _) => 1,
            (Some(1), _) => 3,
            (Some(0), _) => 4,
            (Some(3), _) => 5,
            _ => 0,
        };
        f
    }
}

pub(super) fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

/// Discord's modal: 480 wide, 24 in, the title and a close cross on top.
pub(super) fn modal(ctx: &egui::Context, id: &'static str, title: Option<&str>, add: impl FnOnce(&mut Ui)) -> bool {
    let mut close = false;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51))).inner_margin(Margin::same(24));
    let r = egui::Modal::new(id.into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(432.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if let Some(t) = title {
            let (r, _) = ui.allocate_exact_size(vec2(432.0, 24.0), Sense::hover());
            ui.painter().text(pos2(r.left(), r.top() + 2.0), Align2::LEFT_TOP, t, semi(20.0), pal().header_primary);
            let x = Rect::from_center_size(pos2(r.right() + 6.0, r.center().y - 4.0), vec2(40.0, 40.0));
            let resp = ui.interact(x, ui.id().with("close"), Sense::click());
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
            close = resp.clicked();
            ui.add_space(24.0);
        }
        add(ui);
    });
    close || r.should_close()
}

pub(super) fn label(ui: &mut Ui, s: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, s, medium(16.0), pal().header_primary);
    ui.add_space(8.0);
}

/// The radio rows: ring, icon, name, and a muted line under them.
pub(super) fn choice(ui: &mut Ui, on: bool, icon: Icon, name: &str, desc: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    let p = ui.painter();
    let c = pos2(r.left() + 10.0, r.top() + 10.0);
    if on {
        p.circle_filled(c, 10.0, BLURPLE);
        p.circle_filled(c, 4.0, Color32::WHITE);
    } else {
        p.circle_stroke(c, 9.0, Stroke::new(2.0, pal().header_secondary));
    }
    icons::draw(p, icon, Rect::from_min_size(pos2(r.left() + 32.0, r.top()), vec2(20.0, 20.0)), pal().header_primary);
    p.text(pos2(r.left() + 56.0, r.top() + 10.0), Align2::LEFT_CENTER, name, reg(16.0), pal().text_normal);
    p.text(pos2(r.left() + 32.0, r.top() + 33.0), Align2::LEFT_CENTER, desc, reg(14.0), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(12.0);
    resp.clicked()
}

/// A 40 tall box with an optional glyph in front, the way discord's channel name field looks.
pub(super) fn field(ui: &mut Ui, value: &mut String, hint: &str, glyph: Option<Icon>, rows: usize) -> egui::Response {
    let r = Frame::new()
        .fill(Color32::from_black_alpha(31))
        .stroke(Stroke::new(1.0, faint(61)))
        .corner_radius(8)
        .inner_margin(Margin { left: 12, right: 12, top: 9, bottom: 9 })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(g) = glyph {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::hover());
                    icons::draw(ui.painter(), g, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().text_normal);
                    ui.add_space(8.0);
                }
                let edit = if rows > 1 { egui::TextEdit::multiline(value).desired_rows(rows) } else { egui::TextEdit::singleline(value) };
                ui.add(
                    edit.frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(reg(16.0))
                        .text_color(pal().text_normal)
                        .hint_text(RichText::new(hint).color(pal().text_muted))
                        .desired_width(ui.available_width()),
                )
            })
            .inner
        })
        .inner;
    super::settings::mark(ui, &format!("field:{hint}"), r.rect);
    r
}

/// Lock, label and blurb on the left, the switch on the right.
pub(super) fn private(ui: &mut Ui, on: &mut bool, what: &str, desc: &str) {
    let w = ui.available_width();
    let g = ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 72.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 24.0 + g.size().y), Sense::hover());
    let p = ui.painter();
    icons::draw(p, Icon::Lock, Rect::from_min_size(r.left_top(), vec2(16.0, 16.0)), pal().header_primary);
    p.text(pos2(r.left() + 22.0, r.top() + 9.0), Align2::LEFT_CENTER, what, medium(16.0), pal().header_primary);
    p.galley(pos2(r.left(), r.top() + 24.0), g, pal().header_secondary);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0))));
    if switch(&mut c, *on).clicked() {
        *on = !*on;
    }
}

/// Cancel on the left, the action on the right, each half the width. The action stays dim
/// until there's something to send.
pub(super) fn footer(ui: &mut Ui, go: &str, ready: bool) -> (bool, bool) {
    ui.add_space(24.0);
    let w = (ui.available_width() - 9.0) / 2.0;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let cancel = plate(ui, Rect::from_min_size(r.min, vec2(w, 40.0)), "Cancel", false, true);
    let ok = plate(ui, Rect::from_min_size(pos2(r.right() - w, r.top()), vec2(w, 40.0)), go, true, ready);
    (cancel, ok)
}

/// A button at a set place and size: blurple, or the faint grey of discord's secondary.
pub(super) fn plate(ui: &Ui, r: Rect, text: &str, primary: bool, ready: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(("plate", text)), if ready { Sense::click() } else { Sense::hover() });
    super::settings::mark(ui, &format!("plate:{text}"), r);
    let fade = if ready { 1.0 } else { 0.5 };
    let (fill, ink) = if primary {
        (if resp.hovered() && ready { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE }, Color32::WHITE)
    } else {
        (if resp.hovered() { faint(51) } else { faint(31) }, pal().header_primary)
    };
    ui.painter().rect_filled(r, CornerRadius::same(8), fill.gamma_multiply(fade));
    let edge = if primary { Color32::from_white_alpha(20) } else { faint(61) };
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, edge.gamma_multiply(fade)), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(16.0), ink.gamma_multiply(fade));
    if resp.hovered() && ready {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(f) = app.create.as_mut() else { return };
    let close;
    let mut cancel = false;
    let mut send = false;
    match f.kind {
        Kind::Channel => {
            close = modal(ctx, "create_channel", Some("Create Channel"), |ui| {
                label(ui, "Channel Type");
                for (t, icon, name, desc) in [
                    (0u8, Icon::Hash, "Text", "Send messages, images, GIFs, emoji, opinions, and puns"),
                    (2, Icon::Speaker, "Voice", "Hang out together with voice, video, and screen share"),
                    (15, Icon::Forum, "Forum", "Create a space for organized discussions"),
                ] {
                    if choice(ui, f.chan == t, icon, name, desc) {
                        f.chan = t;
                    }
                }
                ui.add_space(12.0);
                label(ui, "Channel Name");
                let glyph = match f.chan {
                    2 => Icon::Speaker,
                    15 => Icon::Forum,
                    _ => Icon::Hash,
                };
                field(ui, &mut f.name, "new-channel", Some(glyph), 1);
                // text and forum names are lowercase with dashes, as discord rewrites them while you type
                if f.chan != 2 {
                    let fixed = f.name.to_lowercase().replace(' ', "-");
                    if fixed != f.name {
                        f.name = fixed;
                    }
                }
                ui.add_space(20.0);
                private(ui, &mut f.private, "Private Channel", "Only selected members and roles will be able to view this channel.");
                (cancel, send) = footer(ui, "Create Channel", !f.name.trim().is_empty() && !f.busy);
            });
        }
        Kind::Category => {
            close = modal(ctx, "create_category", Some("Create Category"), |ui| {
                label(ui, "Category Name");
                field(ui, &mut f.name, "New Category", None, 1);
                ui.add_space(20.0);
                private(
                    ui,
                    &mut f.private,
                    "Private Category",
                    "By making a category private, only select members and roles will be able to view this category. Linked channels in this category will automatically match to this setting.",
                );
                (cancel, send) = footer(ui, "Create Category", !f.name.trim().is_empty() && !f.busy);
            });
        }
        Kind::Event => {
            let voices: Vec<(Id, String)> = {
                let s = &app.store;
                let mut v: Vec<&crate::state::Channel> = s.channels.values().filter(|c| c.guild == f.guild && c.is_voice() && s.can_view(c)).collect();
                v.sort_by_key(|c| (c.position, c.id));
                v.into_iter().map(|c| (c.id, c.name.clone())).collect()
            };
            if f.place.is_none() {
                f.place = voices.first().map(|v| v.0);
            }
            f.stage = f.place.and_then(|p| app.store.channels.get(&p)).is_some_and(|c| c.kind == 13);
            let f = app.create.as_mut().unwrap();
            close = modal(ctx, "create_event", None, |ui| cancel = event(ui, f, &voices, &mut send));
        }
    }
    let Some(f) = app.create.as_ref() else { return };
    let ready = !f.name.trim().is_empty() && !f.busy;
    send |= matches!(f.kind, Kind::Channel | Kind::Category) && ready && super::enter_submits(ctx, true);
    if send {
        let gid = f.guild;
        let (path, body) = match f.kind {
            Kind::Event => (format!("/guilds/{gid}/scheduled-events{}", f.edit.map(|e| format!("/{e}")).unwrap_or_default()), event_body(f)),
            _ => {
                let kind = if f.kind == Kind::Category { 4 } else { f.chan };
                let mut b = json!({ "name": f.name.trim(), "type": kind });
                if let Some(p) = f.parent.filter(|_| f.kind == Kind::Channel) {
                    b["parent_id"] = json!(p.to_string());
                }
                // private: @everyone (the server's id) can't see it
                if f.private {
                    b["permission_overwrites"] = json!([{ "id": gid.to_string(), "type": 0, "allow": "0", "deny": "1024" }]);
                }
                (format!("/guilds/{gid}/channels"), b)
            }
        };
        if body.is_object() {
            app.create.as_mut().unwrap().busy = true;
            let method = if app.create.as_ref().is_some_and(|f| f.edit.is_some()) { "PATCH" } else { "POST" };
            app.rest(method, path, Some(body), Tag::Created);
        }
    } else if close || cancel {
        app.create = None;
    }
}

fn event(ui: &mut Ui, f: &mut Form, voices: &[(Id, String)], send: &mut bool) -> bool {
    // the three step bars along the top: done or current in blurple
    let w = (ui.available_width() - 16.0) / 3.0;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    for (i, name) in ["Location", "Event Info", "Review"].iter().enumerate() {
        let x = r.left() + i as f32 * (w + 8.0);
        let col = if i as u8 <= f.step { BLURPLE } else { pal().text_muted };
        ui.painter().rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(w, 4.0)), CornerRadius::same(2), col);
        ui.painter().text(pos2(x, r.top() + 12.0), Align2::LEFT_TOP, *name, reg(12.0), if i as u8 == f.step { BLURPLE } else { pal().text_muted });
    }
    ui.add_space(24.0);
    let heading = |ui: &mut Ui, h: &str, s: &str| {
        ui.label(RichText::new(h).font(semi(24.0)).line_height(Some(30.0)).color(pal().header_primary));
        ui.add_space(4.0);
        ui.label(RichText::new(s).font(reg(14.0)).line_height(Some(18.0)).color(pal().header_secondary));
        ui.add_space(24.0);
    };
    let close;
    match f.step {
        0 => {
            heading(ui, "Where is your event?", "So no one gets lost on where to go.");
            if choice(ui, f.voice, Icon::Speaker, "Voice Channel", "Hang out with voice, video, screenshare, and Go Live.") {
                f.voice = true;
            }
            if choice(ui, !f.voice, Icon::Location, "Somewhere Else", "Text channel, external link, or in-person location.") {
                f.voice = false;
            }
            ui.add_space(8.0);
            if f.voice {
                label(ui, "Select a Channel");
                let shown = voices.iter().find(|v| Some(v.0) == f.place).map(|v| v.1.clone()).unwrap_or_else(|| "Select a Channel".into());
                super::settings::dropdown(ui, ui.available_width(), Some(Icon::Speaker), &shown, |ui| {
                    for (id, name) in voices {
                        if super::menu_item(ui, Some(Icon::Speaker), name, false) {
                            f.place = Some(*id);
                        }
                    }
                });
            } else {
                label(ui, "Enter a location");
                field(ui, &mut f.location, "Add a location, link, or something.", None, 1);
            }
            let ready = if f.voice { f.place.is_some() } else { !f.location.trim().is_empty() };
            let (c, next, _) = steps(ui, false, "Next", ready);
            close = c;
            if next {
                f.step = 1;
            }
        }
        1 => {
            heading(ui, "What's your event about?", "Fill out the details of your event.");
            label(ui, "Event Topic");
            field(ui, &mut f.name, "What's your event?", None, 1);
            ui.add_space(20.0);
            pair(ui, "Start Date", &mut f.date, "Start Time", &mut f.time);
            if !f.voice {
                ui.add_space(20.0);
                pair(ui, "End Date", &mut f.end_date, "End Time", &mut f.end_time);
            }
            ui.add_space(20.0);
            label(ui, "Frequency");
            let opts = repeats(f);
            super::settings::dropdown(ui, ui.available_width(), None, &opts[(f.repeat as usize).min(opts.len() - 1)], |ui| {
                for (i, name) in opts.iter().enumerate() {
                    if super::menu_item(ui, None, name, false) {
                        f.repeat = i as u8;
                    }
                }
            });
            ui.add_space(20.0);
            label(ui, "Description");
            field(ui, &mut f.about, "Tell people a little more about your event. Markdown, new lines, and links are supported.", None, 3);
            ui.add_space(20.0);
            label(ui, "Cover Image");
            cover(ui, f);
            let ready = !f.name.trim().is_empty() && start(f).is_some() && (f.voice || end(f).is_some());
            let (c, next, back) = steps(ui, true, "Next", ready);
            close = c;
            if next {
                f.step = 2;
            } else if back {
                f.step = 0;
            }
        }
        _ => {
            heading(ui, "Here's a preview of your event.", "This event will auto start when it's time.");
            let when = start(f).map(|t| t.format("%a %b %-d · %-I:%M %p").to_string()).unwrap_or_default();
            let place = if f.voice { voices.iter().find(|v| Some(v.0) == f.place).map(|v| v.1.clone()).unwrap_or_default() } else { f.location.clone() };
            Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, faint(51))).inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), Icon::Events, r, pal().header_secondary);
                    ui.label(RichText::new(when).font(semi(14.0)).color(pal().header_secondary));
                });
                ui.add_space(8.0);
                ui.label(RichText::new(f.name.trim()).font(semi(20.0)).color(pal().header_primary));
                if !f.about.trim().is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(f.about.trim()).font(reg(14.0)).color(pal().text_normal));
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), if f.voice { Icon::Speaker } else { Icon::Location }, r, pal().header_secondary);
                    ui.label(RichText::new(place).font(medium(14.0)).color(pal().header_secondary));
                });
            });
            let (c, go, back) = steps(ui, true, if f.edit.is_some() { "Save Event" } else { "Create Event" }, !f.busy);
            close = c;
            *send = go;
            if back {
                f.step = 1;
            }
        }
    }
    close
}

/// The event footer: Back on the left from the second step, Cancel and the next step on the right.
fn steps(ui: &mut Ui, back: bool, go: &str, ready: bool) -> (bool, bool, bool) {
    ui.add_space(24.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let mut went_back = false;
    if back {
        let b = ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "Back", medium(14.0), pal().header_primary);
        let resp = ui.interact(b.expand(4.0), ui.id().with("event_back"), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        went_back = resp.clicked();
    }
    let gw = ui.painter().layout_no_wrap(go.into(), medium(16.0), Color32::WHITE).size().x.max(68.0) + 32.0;
    let go_r = Rect::from_min_size(pos2(r.right() - gw, r.top()), vec2(gw, 40.0));
    let next = plate(ui, go_r, go, true, ready);
    let close = plate(ui, Rect::from_min_size(pos2(go_r.left() - 108.0, r.top()), vec2(100.0, 40.0)), "Cancel", false, true);
    (close, next, went_back)
}

fn pair(ui: &mut Ui, a: &str, av: &mut String, b: &str, bv: &mut String) {
    let w = (ui.available_width() - 16.0) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (name, v, hint) in [(a, av, "YYYY-MM-DD"), (b, bv, "HH:MM")] {
            ui.allocate_ui(vec2(w, 72.0), |ui| {
                ui.set_width(w);
                ui.vertical(|ui| {
                    label(ui, name);
                    field(ui, v, hint, Some(if hint.starts_with('Y') { Icon::Events } else { Icon::Clock }), 1);
                });
            });
        }
    });
}

fn when(date: &str, time: &str) -> Option<chrono::DateTime<chrono::Local>> {
    let d = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let t = chrono::NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    d.and_time(t).and_local_timezone(chrono::Local).single()
}

fn start(f: &Form) -> Option<chrono::DateTime<chrono::Local>> {
    when(&f.date, &f.time).filter(|t| f.started || *t > chrono::Local::now())
}

/// Discord's repeat choices, worded for the start date.
fn repeats(f: &Form) -> Vec<String> {
    let Some(s) = when(&f.date, &f.time) else { return vec!["Does not repeat".into()] };
    let nth = ["first", "second", "third", "fourth", "fifth"][((s.day() - 1) / 7) as usize];
    vec![
        "Does not repeat".into(),
        format!("Weekly on {}", s.format("%A")),
        format!("Every other week on {}", s.format("%A")),
        format!("Monthly on the {nth} {}", s.format("%A")),
        format!("Annually on {}", s.format("%B %-d")),
        "Every weekday (Monday to Friday)".into(),
    ]
}

/// The cover image row: a button to pick one, or the one picked with a way to drop it.
fn cover(ui: &mut Ui, f: &mut Form) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let name = f.cover.as_ref().map(|c| c.0.clone());
    let w = 168.0;
    if plate(ui, Rect::from_min_size(r.min, vec2(w, 40.0)), if name.is_some() { "Change Image" } else { "Upload Cover Image" }, false, true)
        && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
        && let Ok(bytes) = std::fs::read(&path)
    {
        use base64::Engine;
        let kind = match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
            Some("jpg" | "jpeg") => "jpeg",
            Some("gif") => "gif",
            Some("webp") => "webp",
            _ => "png",
        };
        let uri = format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
        f.cover = Some((path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), uri));
    }
    if let Some(n) = name {
        let g = ui.painter().layout_no_wrap(n, reg(14.0), pal().header_secondary);
        let at = pos2(r.left() + w + 12.0, r.center().y - g.size().y / 2.0);
        ui.painter().galley(at, g, pal().header_secondary);
        let x = Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(20.0, 20.0));
        let resp = ui.interact(x.expand(4.0), ui.id().with("cover_drop"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
        if resp.clicked() {
            f.cover = None;
        }
    }
}

fn end(f: &Form) -> Option<chrono::DateTime<chrono::Local>> {
    when(&f.end_date, &f.end_time).filter(|t| start(f).is_some_and(|s| *t > s))
}

fn event_body(f: &Form) -> serde_json::Value {
    let iso = |t: chrono::DateTime<chrono::Local>| t.with_timezone(&chrono::Utc).to_rfc3339();
    let Some(s) = start(f) else { return serde_json::Value::Null };
    let mut b = json!({ "name": f.name.trim(), "privacy_level": 2, "description": f.about.trim() });
    if !f.started {
        b["scheduled_start_time"] = json!(iso(s));
    }
    if let Some((_, uri)) = &f.cover {
        b["image"] = json!(uri);
    }
    let wd = s.weekday().num_days_from_monday();
    let rule = match f.repeat {
        1 => Some(json!({ "frequency": 2, "interval": 1, "by_weekday": [wd] })),
        2 => Some(json!({ "frequency": 2, "interval": 2, "by_weekday": [wd] })),
        3 => Some(json!({ "frequency": 1, "interval": 1, "by_n_weekday": [{ "n": (s.day() - 1) / 7 + 1, "day": wd }] })),
        4 => Some(json!({ "frequency": 0, "interval": 1, "by_month": [s.month()], "by_month_day": [s.day()] })),
        5 => Some(json!({ "frequency": 3, "interval": 1, "by_weekday": [0, 1, 2, 3, 4] })),
        _ => None,
    };
    b["recurrence_rule"] = match rule {
        Some(mut r) => {
            r["start"] = json!(iso(s));
            r["end"] = serde_json::Value::Null;
            r
        }
        None => serde_json::Value::Null,
    };
    if f.voice {
        b["entity_type"] = json!(if f.stage { 1 } else { 2 });
        b["channel_id"] = json!(f.place.map(|c| c.to_string()));
    } else {
        b["entity_type"] = json!(3);
        b["entity_metadata"] = json!({ "location": f.location.trim() });
        b["scheduled_end_time"] = json!(end(f).map(iso));
    }
    b
}

/// What came back from a create: a channel opens like a fresh one in discord, an event just closes.
pub fn created(app: &mut App, v: &serde_json::Value) {
    app.create = None;
    if v["entity_type"].is_number() {
        let gid = crate::state::sid(&v["guild_id"]);
        app.open_events(gid);
    }
    if v["type"].is_number() && v["guild_id"].is_string() && v["entity_type"].is_null() {
        app.store.apply("CHANNEL_CREATE", v);
        let id = crate::state::sid(&v["id"]);
        if v["type"].as_u64() != Some(4) {
            app.open(id);
        }
    }
}
