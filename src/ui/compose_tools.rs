//! The composer's plugin buttons (SilentTyping, SilentMessageToggle, SendTimestamps,
//! PreviewMessage, Translate), the CharacterCounter, StickerPaste's chip and the popups they open.

use chrono::{Local, TimeZone};
use egui::{Align2, Color32, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::outgoing::{self, Stamp, Task};
use super::{App, icon_button};
use crate::plugins;
use crate::state::{Id, Message, make_nonce};
use crate::theme::*;

const FORMATS: [(&str, &str); 10] = [
    ("", "Default"),
    ("t", "Short Time"),
    ("T", "Long Time"),
    ("d", "Short Date"),
    ("D", "Long Date"),
    ("f", "Short Date/Time"),
    ("F", "Long Date/Time"),
    ("s", "Short Date, Short Time"),
    ("S", "Short Date, Long Time"),
    ("R", "Relative"),
];

pub fn count() -> usize {
    ["SilentMessageToggle", "SendTimestamps", "PreviewMessage", "Translate"].iter().filter(|p| plugins::on(p)).count() + plugins::flag("SilentTyping", "showIcon") as usize
}

fn slot(ui: &mut Ui, tip: &str, paint: impl FnOnce(&egui::Painter, Rect, Color32)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
    let col = if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    paint(ui.painter(), Rect::from_center_size(r.center(), vec2(20.0, 20.0)), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

fn keyboard(p: &egui::Painter, r: Rect, col: Color32, struck: bool) {
    let body = Rect::from_center_size(r.center(), vec2(20.0, 12.0));
    p.rect_stroke(body, 3, Stroke::new(1.5, col), egui::StrokeKind::Inside);
    for row in 0..2 {
        for k in 0..4 {
            p.circle_filled(pos2(body.left() + 4.0 + k as f32 * 4.0, body.top() + 3.5 + row as f32 * 3.0), 0.8, col);
        }
    }
    p.line_segment([pos2(body.left() + 5.0, body.bottom() - 2.5), pos2(body.right() - 5.0, body.bottom() - 2.5)], Stroke::new(1.5, col));
    if struck {
        p.line_segment([r.left_bottom() + vec2(1.0, -1.0), r.right_top() + vec2(-1.0, 1.0)], Stroke::new(2.0, col));
    }
}

/// The plugin buttons, laid out right to left after discord's own.
pub fn buttons(app: &mut App, ui: &mut Ui, ch: Id) {
    if plugins::flag("SilentTyping", "showIcon") {
        let on = plugins::get("SilentTyping", "isEnabled").as_bool().unwrap_or(false);
        if slot(ui, if on { "Disable Silent Typing" } else { "Enable Silent Typing" }, |p, r, c| keyboard(p, r, c, on)).clicked() {
            plugins::set("SilentTyping", "isEnabled", (!on).into());
        }
    }
    if plugins::on("SilentMessageToggle") {
        let on = app.plug.silent;
        let resp = slot(ui, if on { "Disable Silent Message" } else { "Enable Silent Message" }, |p, r, c| icons::draw(p, if on { Icon::BellOff } else { Icon::Bell }, r, if on { RED } else { c }));
        if resp.clicked() {
            app.plug.silent = !on;
        }
    }
    if plugins::on("SendTimestamps") {
        let resp = slot(ui, "Insert Timestamp", |p, r, c| icons::draw(p, Icon::CalendarPlus, r, c));
        if resp.clicked() {
            app.plug.stamp = match app.plug.stamp {
                Some(_) => None,
                None => {
                    let now = Local::now();
                    Some(Stamp { date: now.format("%Y-%m-%d").to_string(), time: now.format("%H:%M").to_string(), fmt: 0 })
                }
            };
        }
        app.rects.insert("stamp_button".into(), resp.rect);
    }
    if plugins::on("PreviewMessage") {
        let on = app.plug.preview;
        let resp = slot(ui, "Preview Message", |p, r, c| icons::draw(p, Icon::Eye, r, if on { BLURPLE } else { c }));
        if resp.clicked() {
            app.plug.preview = !on;
        }
        app.rects.insert("preview_button".into(), resp.rect);
    }
    if plugins::on("Translate") {
        let auto = plugins::flag("Translate", "autoTranslate");
        let resp = slot(ui, "Translate Message", |p, r, c| icons::draw(p, Icon::SideLanguage, r, if auto { BLURPLE } else { c }));
        let flip = resp.secondary_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.shift));
        if flip {
            plugins::set("Translate", "autoTranslate", (!auto).into());
            app.toast(if auto { "Auto translate off" } else { "Auto translate on" });
        } else if resp.clicked() {
            let text = app.drafts.get(&ch).cloned().unwrap_or_default();
            if !text.trim().is_empty() {
                outgoing::translate(app, text, "sent", Task::Draft(ch));
            }
        }
    }
}

/// CharacterCounter: how much of the limit is used, shifting from muted to red as it fills.
pub fn counter(app: &mut App, ui: &mut Ui, crect: Rect, used: usize, limit: usize) {
    if used == 0 {
        return;
    }
    let pct = used as f32 / limit as f32 * 100.0;
    let col = if !plugins::flag("CharacterCounter", "colorEffects") || pct < 50.0 {
        pal().text_muted
    } else if pct < 75.0 {
        Color32::from_rgb(240, 178, 50)
    } else if pct < 90.0 {
        Color32::from_rgb(245, 127, 49)
    } else {
        RED
    };
    let g = ui.painter().layout_no_wrap(format!("{used}/{limit}"), reg(12.0), col);
    let at = pos2(crect.right() - 16.0 - g.size().x, crect.bottom() - 4.0 - g.size().y);
    let r = Rect::from_min_size(at, g.size());
    ui.painter().galley(at, g, col);
    app.rects.insert("char_count".into(), r);
}

/// StickerPaste's sticker, waiting for the next send.
pub fn sticker_chip(app: &mut App, ui: &mut Ui) {
    let Some((id, name, fmt)) = app.plug.sticker.clone() else { return };
    let mut drop = false;
    Frame::new().inner_margin(egui::Margin { left: 16, right: 10, top: 10, bottom: 0 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            let url = format!("https://media.discordapp.net/stickers/{id}.{}?size=96", if fmt == 4 { "gif" } else { "png" });
            egui::Image::from_uri(crate::gif::uri(ui.ctx(), &url, true)).show_loading_spinner(false).paint_at(ui, r);
            ui.label(RichText::new(name).font(medium(14.0)).color(pal().text_normal));
            drop = icon_button(ui, Icon::Close, 20.0, "Remove Sticker").clicked();
        });
    });
    if drop {
        app.plug.sticker = None;
    }
}

fn stamp_unix(s: &Stamp) -> Option<i64> {
    let at = chrono::NaiveDateTime::parse_from_str(&format!("{} {}", s.date.trim(), s.time.trim()), "%Y-%m-%d %H:%M").ok()?;
    Some(Local.from_local_datetime(&at).single()?.timestamp())
}

fn sample(ts: i64, f: &str) -> String {
    let Some(t) = Local.timestamp_opt(ts, 0).single() else { return String::new() };
    let time = "%-I:%M %p";
    let long_time = "%-I:%M:%S %p";
    match f {
        "t" => t.format(time).to_string(),
        "T" => t.format(long_time).to_string(),
        "d" => t.format("%m/%d/%Y").to_string(),
        "D" => t.format("%B %-d, %Y").to_string(),
        "f" | "" => t.format(&format!("%B %-d, %Y {time}")).to_string(),
        "F" => t.format(&format!("%A, %B %-d, %Y {time}")).to_string(),
        "s" => t.format(&format!("%m/%d/%Y, {time}")).to_string(),
        "S" => t.format(&format!("%m/%d/%Y, {long_time}")).to_string(),
        _ => {
            let d = ts - Local::now().timestamp();
            let (n, unit) = match d.abs() {
                s if s < 60 => (s, "second"),
                s if s < 3600 => (s / 60, "minute"),
                s if s < 86400 => (s / 3600, "hour"),
                s => (s / 86400, "day"),
            };
            let plural = if n == 1 { "" } else { "s" };
            if d >= 0 { format!("in {n} {unit}{plural}") } else { format!("{n} {unit}{plural} ago") }
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    stamp_picker(app, ctx);
    preview(app, ctx);
}

fn stamp_picker(app: &mut App, ctx: &egui::Context) {
    let Some(mut st) = app.plug.stamp.take() else { return };
    let Some(ch) = app.cur() else { return };
    let at = app.rects.get("stamp_button").map_or(pos2(600.0, 500.0), |r| pos2(r.center().x, r.top() - 8.0));
    let mut insert = false;
    let ts = stamp_unix(&st);
    let area = egui::Area::new(egui::Id::new("stamp_picker")).order(egui::Order::Foreground).pivot(Align2::CENTER_BOTTOM).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(16).show(ui, |ui| {
            ui.set_width(300.0);
            ui.label(RichText::new("Timestamp Picker").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.date).desired_width(120.0).hint_text("YYYY-MM-DD"));
                ui.add(egui::TextEdit::singleline(&mut st.time).desired_width(70.0).hint_text("HH:MM"));
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Timestamp Format").font(medium(12.0)).color(pal().header_secondary));
            let shown = ts.unwrap_or_else(|| Local::now().timestamp());
            for (i, (code, label)) in FORMATS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                if resp.hovered() || st.fmt == i {
                    ui.painter().rect_filled(r, 4, pal().control_hover);
                }
                ui.painter().text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, format!("{} {}", if code.is_empty() { "·" } else { code }, label), reg(13.0), pal().text_muted);
                ui.painter().text(r.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, sample(shown, code), medium(13.0), pal().header_primary);
                if resp.clicked() {
                    st.fmt = i;
                }
            }
            ui.add_space(8.0);
            let ok = ts.is_some();
            let text = ts.map_or("Enter a date as YYYY-MM-DD and a time as HH:MM".to_string(), |t| format!("<t:{t}{}>", if FORMATS[st.fmt].0.is_empty() { String::new() } else { format!(":{}", FORMATS[st.fmt].0) }));
            ui.label(RichText::new(text).font(mono(12.0)).color(pal().text_muted));
            ui.add_space(8.0);
            if ui.add_enabled(ok, egui::Button::new("Insert")).clicked() {
                insert = true;
            }
        });
    });
    if insert && let Some(t) = ts {
        let code = FORMATS[st.fmt].0;
        let d = app.drafts.entry(ch).or_default();
        d.push_str(&format!("<t:{t}{}> ", if code.is_empty() { String::new() } else { format!(":{code}") }));
        app.focus_composer = true;
        app.caret_end = true;
    } else if !super::popups::dismissed(ctx, &area.response) || super::picker::on_picker_button(app, ctx) {
        app.plug.stamp = Some(st);
    }
}

fn preview(app: &mut App, ctx: &egui::Context) {
    if !app.plug.preview {
        return;
    }
    let Some(ch) = app.cur() else { return };
    let Some(comp) = app.rects.get("composer").copied() else { return };
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let draft = app.drafts.get(&ch).map(|d| d.trim().to_string()).unwrap_or_default();
    let content = app.convert_emoji(guild, &app.resolve_aliases(ch, &draft));
    let msg = Message { id: make_nonce().parse().unwrap_or(0), channel: ch, author: app.store.me, content, ..Default::default() };
    let area = egui::Area::new(egui::Id::new("message_preview")).order(egui::Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(pos2(comp.left(), comp.top() - 8.0)).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(12).show(ui, |ui| {
            ui.set_width(comp.width() - 24.0);
            ui.label(RichText::new("Message Preview").font(semi(12.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            if msg.content.is_empty() {
                ui.label(RichText::new("Nothing to preview yet.").color(pal().text_muted));
            } else {
                super::chat::preview(ui, &app.store, &msg);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (super::popups::pressed_outside(ctx, &area.response) && !super::picker::on_picker_button(app, ctx) && !app.rects.get("preview_button").zip(ctx.pointer_hover_pos()).is_some_and(|(r, p)| r.contains(p))) {
        app.plug.preview = false;
    }
}
