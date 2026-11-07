//! Set your status: the line under your name and the emoji before it, with how long it stays. Enter
//! saves, and Shift+Enter does nothing.

use egui::{Color32, CornerRadius, Frame, Key, Margin, Rect, RichText, Sense, Stroke, TextEdit, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, Pick, button, icon_button, menu_item, settings};
use crate::net::Tag;
use crate::state::ReactEmoji;
use crate::theme::*;

const LENGTHS: [(&str, Option<i64>); 5] = [("Don't clear", None), ("30 minutes", Some(30 * 60)), ("1 hour", Some(3600)), ("4 hours", Some(4 * 3600)), ("24 hours", Some(-1))];

const TODAY: usize = 4;

#[derive(Default)]
pub struct State {
    text: String,
    pub emoji: Option<ReactEmoji>,
    length: usize,
}

impl State {
    pub fn from(store: &crate::state::Store) -> Self {
        State { text: store.custom_status.clone().unwrap_or_default(), emoji: store.custom_emoji.clone(), length: TODAY }
    }
}

pub fn chose(app: &mut App, em: ReactEmoji) {
    if let Some(s) = &mut app.status_text {
        s.emoji = Some(em);
    }
}

fn ends(length: usize, clock24: bool) -> String {
    let Some(secs) = LENGTHS[length].1 else { return LENGTHS[length].0.to_string() };
    let end = if secs < 0 { chrono::Local::now() + chrono::Duration::hours(24) } else { chrono::Local::now() + chrono::Duration::seconds(secs) };
    format!("{} ({})", LENGTHS[length].0, end.format(if clock24 { "%H:%M" } else { "%-I:%M %p" }))
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let clock24 = app.settings.clock24();
    let me = app.store.me;
    let picking = matches!(app.picker, Some((Pick::Status, _)));
    let avatar = app.store.user(me).map(|u| u.avatar_url()).unwrap_or_default();
    let Some(s) = &mut app.status_text else { return };
    let (mut save, mut close, mut pick) = (false, false, None);
    let modal = crate::ui::motion::Modal::new("custom_status".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Set your status").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 88.0), Sense::hover());
            ui.painter().rect_filled(card, CornerRadius::same(8), pal().bg_tertiary);
            let av = Rect::from_center_size(pos2(card.left() + 16.0 + 28.0, card.center().y), vec2(56.0, 56.0));
            crate::ui::avatar(ui, av, &avatar);
            let blank = s.text.trim().is_empty() && s.emoji.is_none();
            let words = if blank { "Support has arrived!" } else { s.text.trim() };
            let shown = ui.painter().layout(words.to_string(), if blank { italic(14.0) } else { reg(14.0) }, pal().header_primary.gamma_multiply(if blank { 0.76 } else { 1.0 }), card.width() - 140.0);
            let lead = if s.emoji.is_some() { 24.0 } else { 0.0 };
            let height = (shown.size().y + 22.0).max(40.0);
            let bubble = Rect::from_min_size(pos2(av.right() + 12.0, card.center().y - height / 2.0), vec2(shown.size().x + 24.0 + lead, height));
            ui.painter().rect(bubble, 16, pal().bg_floating, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            if let Some(e) = &s.emoji {
                egui::Image::from_uri(crate::ui::chat::emoji_url(e)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(bubble.left() + 20.0, bubble.center().y), vec2(18.0, 18.0)));
            }
            ui.painter().galley(pos2(bubble.left() + 12.0 + lead, bubble.center().y - shown.size().y / 2.0), shown, pal().header_primary);
            ui.add_space(16.0);
            ui.label(RichText::new("Status").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(8.0);
            Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61))).corner_radius(8).inner_margin(Margin::symmetric(8, 7)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (b, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    match &s.emoji {
                        Some(e) => {
                            egui::Image::from_uri(crate::ui::chat::emoji_url(e)).show_loading_spinner(false).paint_at(ui, b);
                        }
                        None => icons::draw(ui.painter(), Icon::Smile, b, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary }),
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "cstatus_emoji")), b));
                    if resp.clicked() {
                        pick = Some(b.left_bottom());
                    }
                    let clear_w = if blank { 0.0 } else { 32.0 };
                    let r = ui.add(TextEdit::singleline(&mut s.text).frame(Frame::NONE).margin(Margin::ZERO).font(reg(16.0)).text_color(pal().text_normal).hint_text(RichText::new("Support has arrived!").color(pal().text_muted)).desired_width(ui.available_width() - clear_w));
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "cstatus_input")), r.rect));
                    save |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    if !picking {
                        r.request_focus();
                    }
                    s.text = s.text.chars().take(128).collect();
                    if clear_w > 0.0 {
                        let (x, xr) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), if xr.hovered() { pal().interactive_hover } else { pal().header_secondary });
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "cstatus_clear")), x));
                        if xr.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if xr.clicked() {
                            s.text.clear();
                            s.emoji = None;
                        }
                    }
                });
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                let w = ui.available_width() - 136.0 - 16.0;
                settings::dropdown(ui, w, None, &ends(s.length, clock24), |ui| {
                    for i in 0..LENGTHS.len() {
                        if menu_item(ui, None, &ends(i, clock24), false) {
                            s.length = i;
                        }
                    }
                });
                ui.add_space(16.0 - ui.spacing().item_spacing.x);
                let b = button(ui, "Save", BLURPLE, 136.0);
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "cstatus_save")), b.rect));
                save |= b.clicked();
            });
        });
    if save {
        let (text, emoji, length) = (s.text.trim().to_string(), s.emoji.clone(), s.length);
        app.status_text = None;
        app.set_custom_status(&text, emoji, length);
    } else if close || modal.should_close() {
        app.status_text = None;
    } else if let Some(at) = pick {
        app.picker = Some((Pick::Status, at));
    }
}

impl App {
    /// No text and no emoji clears it; `length` is a row of LENGTHS.
    pub fn set_custom_status(&mut self, text: &str, emoji: Option<ReactEmoji>, length: usize) {
        let now = chrono::Local::now();
        let none = text.is_empty() && emoji.is_none();
        let expires = match LENGTHS[length].1 {
            _ if none => 0,
            None => 0,
            Some(secs) => (now + chrono::Duration::seconds(if secs < 0 { 24 * 3600 } else { secs })).timestamp_millis() as u64,
        };
        self.store.custom_status = (!text.is_empty()).then(|| text.to_string());
        self.store.custom_emoji = emoji.clone();
        self.proto_patch(crate::proto::custom_status_patch(text, emoji.as_ref(), expires, now.timestamp_millis() as u64), Tag::fail(super::SAVE_FAILED));
    }
}

pub const STATUS_FOR: [(&str, Option<i64>); 6] = [("For 15 Minutes", Some(900)), ("For 1 Hour", Some(3600)), ("For 8 Hours", Some(28_800)), ("For 24 Hours", Some(86_400)), ("For 3 Days", Some(259_200)), ("Forever", None)];

impl App {
    pub fn set_status_for(&mut self, status: &str, secs: Option<i64>) {
        self.set_status(status);
        self.status_until = secs.filter(|_| status != "online").map(|s| chrono::Utc::now().timestamp() + s);
        let expires = self.status_until.map_or(0, |t| t as u64 * 1000);
        self.proto_patch(crate::proto::status_patch(status, expires, chrono::Utc::now().timestamp_millis() as u64), Tag::Quiet);
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
