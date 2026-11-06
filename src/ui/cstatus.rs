//! Set a custom status: the line under your name, with how long it stays. Enter saves, and
//! Shift+Enter does nothing.

use egui::{Color32, Frame, Key, RichText, Stroke};
use serde_json::json;

use super::{App, button, grey_button, icon_button, menu_item, settings};
use super::icons::Icon;
use crate::net::Tag;
use crate::theme::*;

const LENGTHS: [(&str, Option<i64>); 5] = [("Don't clear", None), ("30 minutes", Some(30 * 60)), ("1 hour", Some(3600)), ("4 hours", Some(4 * 3600)), ("Today", Some(-1))];

#[derive(Default)]
pub struct State {
    text: String,
    length: usize,
}

impl State {
    pub fn from(current: Option<&String>) -> Self {
        State { text: current.cloned().unwrap_or_default(), length: 0 }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(s) = &mut app.status_text else { return };
    let (mut save, mut close, mut clear) = (false, false, false);
    let had = app.store.custom_status.is_some();
    let modal = crate::ui::motion::Modal::new("custom_status".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Set a custom status").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            ui.label(RichText::new("What's cookin', good lookin'?").font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let r = settings::input(ui, &mut s.text, "Support has arrived!", false);
            save |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
            r.request_focus();
            s.text = s.text.chars().take(128).collect();
            ui.add_space(16.0);
            ui.label(RichText::new("Clear after").font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let w = ui.available_width();
            settings::dropdown(ui, w, None, LENGTHS[s.length].0, |ui| {
                for (i, (label, _)) in LENGTHS.iter().enumerate() {
                    if menu_item(ui, None, label, false) {
                        s.length = i;
                    }
                }
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if had {
                    clear |= grey_button(ui, "Remove", 120.0).clicked();
                }
                close |= grey_button(ui, "Cancel", if had { 120.0 } else { 192.0 }).clicked();
                let b = button(ui, "Save", BLURPLE, if had { 136.0 } else { 192.0 });
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "cstatus_save")), b.rect));
                save |= b.clicked();
            });
        });
    if clear || save {
        let text = if clear { String::new() } else { s.text.trim().to_string() };
        let length = s.length;
        app.status_text = None;
        app.set_custom_status(&text, length);
    } else if close || modal.should_close() {
        app.status_text = None;
    }
}

impl App {
    /// Empty text clears it; `length` is a row of LENGTHS.
    pub fn set_custom_status(&mut self, text: &str, length: usize) {
        let body = if text.is_empty() {
            json!({ "custom_status": null })
        } else {
            let expires = match LENGTHS[length].1 {
                None => json!(null),
                Some(secs) => {
                    let now = chrono::Local::now();
                    let end = if secs < 0 {
                        (now.date_naive() + chrono::Duration::days(1)).and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(chrono::Local).single()).unwrap_or(now)
                    } else {
                        now + chrono::Duration::seconds(secs)
                    };
                    json!(end.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                }
            };
            json!({ "custom_status": { "text": text, "expires_at": expires, "emoji_id": null, "emoji_name": null } })
        };
        self.store.custom_status = (!text.is_empty()).then(|| text.to_string());
        self.rest("PATCH", "/users/@me/settings".into(), Some(body), Tag::fail(super::SAVE_FAILED));
    }
}

pub const STATUS_FOR: [(&str, Option<i64>); 6] = [("For 15 Minutes", Some(900)), ("For 1 Hour", Some(3600)), ("For 8 Hours", Some(28_800)), ("For 24 Hours", Some(86_400)), ("For 3 Days", Some(259_200)), ("Forever", None)];

impl App {
    pub fn set_status_for(&mut self, status: &str, secs: Option<i64>) {
        self.set_status(status);
        self.status_until = secs.filter(|_| status != "online").map(|s| chrono::Utc::now().timestamp() + s);
        let until = self.status_until.and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        self.rest("PATCH", "/users/@me/settings".into(), Some(json!({ "status": status, "status_expires_at": until })), Tag::Quiet);
    }

    pub fn status_expiry(&mut self, ctx: &egui::Context) {
        let Some(at) = self.status_until else { return };
        let left = at - chrono::Utc::now().timestamp();
        if left <= 0 {
            self.set_status_for("online", None);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(left.min(30) as u64));
        }
    }
}
