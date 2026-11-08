//! Several accounts on one machine: each signed-in account's token is kept beside the main one
//! in the data folder, and Switch Accounts swaps which of them is live.

use super::App;
use crate::cache::{self, SavedAccount};
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
        self.sign_in(token);
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

use super::{avatar, button, icon_button};
use super::icons::Icon;
use crate::theme::*;

fn shell(ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut close = false;
    let modal = crate::ui::motion::Modal::new("user_dialog".into())
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
                crate::ui::motion::wash(ui, &resp, b, 4, crate::theme::CRITICAL_BG, 0.05);
                ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, "Remove", medium(14.0), pal().critical);
                if resp.clicked() {
                    remove = Some(a.id);
                }
            }
        }
        ui.add_space(16.0);
        if button(ui, "Add an account", pal().blurple, 384.0).clicked() {
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
