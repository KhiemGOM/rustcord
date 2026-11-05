//! Several accounts on one machine: each signed-in account's token is kept beside the main one
//! in the data folder, and Switch Accounts swaps which of them is live.

use super::App;
use crate::cache::{self, SavedAccount};
use crate::net::Cmd;
use crate::state::Id;

impl App {
    /// Once connected, the account is added to the ones Switch Accounts lists.
    pub fn remember_me(&mut self) {
        let (Some(token), Some(u)) = (&self.token, self.store.user(self.store.me)) else { return };
        cache::remember_account(&SavedAccount { id: u.id, name: u.display().to_string(), avatar: u.avatar.clone() }, token);
        self.accounts = cache::accounts();
    }

    pub fn switch_account(&mut self, id: Id) {
        let Some(token) = cache::account_token(id).filter(|_| id != self.store.me) else { return };
        self.reset_session();
        cache::set_token(Some(&token));
        self.net.send(Cmd::Start(token.clone()));
        self.token = Some(token);
    }

    /// To the login screen with the current account kept for switching back to.
    pub fn add_account(&mut self) {
        self.reset_session();
    }

    pub fn remove_account(&mut self, id: Id) {
        if id != self.store.me {
            cache::forget_account(id);
            self.accounts = cache::accounts();
        }
    }
}

use egui::{Align, Frame, Layout, RichText, Sense, pos2, vec2};
use serde_json::json;

use super::userdlg::Dialog;
use super::{avatar, button, grey_button, icon_button};
use super::icons::Icon;
use crate::net::Tag;
use crate::theme::*;

const CLEARS: [&str; 5] = ["Don't clear", "Today", "4 hours", "1 hour", "30 minutes"];

fn shell(ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut close = false;
    let modal = egui::Modal::new("user_dialog".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            add(ui);
        });
    close || modal.should_close()
}

/// The custom status editor: what it says and when it clears itself.
pub fn status_modal(app: &mut App, ctx: &egui::Context) {
    let Some(Dialog::Status { text, clear }) = &mut app.user_dialog else { return };
    let mut save = false;
    let close = shell(ctx, "Set a custom status", |ui| {
        ui.label(RichText::new("What's cookin', good lookin'?").font(semi(12.0)).color(pal().header_primary));
        ui.add_space(8.0);
        input_frame().corner_radius(8).inner_margin(12).show(ui, |ui| {
            let r = ui.add(egui::TextEdit::singleline(text).frame(Frame::NONE).font(reg(16.0)).char_limit(128).hint_text("Support has arrived!").desired_width(ui.available_width()));
            if !ui.ctx().memory(|m| m.areas().visible_last_frame(&ui.layer_id())) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                save = true;
            }
        });
        ui.add_space(16.0);
        ui.label(RichText::new("Clear after").font(semi(12.0)).color(pal().header_primary));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (i, label) in CLEARS.iter().enumerate() {
                let g = ui.painter().layout_no_wrap(label.to_string(), semi(14.0), pal().header_primary);
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 36.0), Sense::click());
                let on = *clear == i as u8;
                ui.painter().rect_filled(r, 0, if on { BLURPLE } else if resp.hovered() { pal().control_hover } else { pal().control });
                ui.painter().galley(pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, if on { egui::Color32::WHITE } else { pal().header_primary });
                if resp.clicked() {
                    *clear = i as u8;
                }
            }
        });
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if grey_button(ui, "Clear Status", 212.0).clicked() {
                text.clear();
                save = true;
            }
            if button(ui, "Save", BLURPLE, 212.0).clicked() {
                save = true;
            }
        });
    });
    if save && let Some(Dialog::Status { text, clear }) = app.user_dialog.take() {
        app.set_custom_status(text.trim(), clear);
    } else if close {
        app.user_dialog = None;
    }
}

impl App {
    pub fn set_custom_status(&mut self, text: &str, clear: u8) {
        let now = chrono::Utc::now();
        let midnight = chrono::Local::now().date_naive().succ_opt().and_then(|d| d.and_hms_opt(0, 0, 0)).and_then(|d| d.and_local_timezone(chrono::Local).single()).map(|d| d.with_timezone(&chrono::Utc));
        let end = match clear {
            1 => midnight,
            2 => Some(now + chrono::Duration::hours(4)),
            3 => Some(now + chrono::Duration::hours(1)),
            4 => Some(now + chrono::Duration::minutes(30)),
            _ => None,
        };
        let body = if text.is_empty() {
            json!({ "custom_status": null })
        } else {
            json!({ "custom_status": { "text": text, "expires_at": end.map(|e| e.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)) } })
        };
        self.store.custom_status = (!text.is_empty()).then(|| text.to_string());
        self.rest("PATCH", "/users/@me/settings".into(), Some(body), Tag::Quiet);
    }
}

/// Manage Accounts: who's signed in here, a way to take one off the list, and a way to add another.
pub fn manage_modal(app: &mut App, ctx: &egui::Context) {
    let me = app.store.me;
    let list = app.accounts.clone();
    let (mut remove, mut add) = (None, false);
    let close = shell(ctx, "Manage Accounts", |ui| {
        for a in &list {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            let av = egui::Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0));
            let url = match &a.avatar {
                Some(h) => format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), a.id),
                None => format!("{}/embed/avatars/{}.png", crate::api::cdn(), (a.id >> 22) % 6),
            };
            avatar(ui, av, &url);
            super::text::paint(ui.painter(), pos2(r.left() + 52.0, r.center().y), egui::Align2::LEFT_CENTER, &a.name, medium(16.0), pal().header_primary, r.width() - 160.0);
            if a.id == me {
                ui.painter().text(pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, "Current", reg(14.0), pal().text_muted);
            } else {
                let b = egui::Rect::from_center_size(pos2(r.right() - 40.0, r.center().y), vec2(80.0, 32.0));
                let resp = ui.interact(b, ui.id().with(("remove_account", a.id)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(b, 4, crate::theme::CRITICAL_BG);
                }
                ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, "Remove", medium(14.0), pal().critical);
                if resp.clicked() {
                    remove = Some(a.id);
                }
            }
        }
        ui.add_space(16.0);
        if button(ui, "Add an account", BLURPLE, 384.0).clicked() {
            add = true;
        }
    });
    if let Some(id) = remove {
        app.remove_account(id);
    }
    if add {
        app.user_dialog = None;
        app.add_account();
    } else if close {
        app.user_dialog = None;
    }
}
