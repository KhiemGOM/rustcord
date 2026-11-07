//! Discord's Kick and Ban dialogs: a reason for the audit log, and for a ban a picked reason and how much of their history to delete.

use egui::{Align, Frame, Layout, RichText};
use serde_json::json;

use super::icons::Icon;
use super::{App, button, enter_submits, grey_button, icon_button, settings};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

const REASONS: [&str; 4] = ["Suspicious or spam account", "Compromised or hacked account", "Breaking server rules", "Other"];
const HISTORY: [(&str, i64); 7] = [("None", 0), ("Previous Hour", 3600), ("Previous 6 Hours", 21_600), ("Previous 12 Hours", 43_200), ("Previous 24 Hours", 86_400), ("Previous 3 Days", 259_200), ("Previous 7 Days", 604_800)];
const MAX_REASON: usize = 512;

pub struct Dialog {
    guild: Id,
    user: Id,
    ban: bool,
    pub(super) pick: Option<usize>,
    text: String,
    history: usize,
    need: bool,
}

impl Dialog {
    pub fn new(guild: Id, user: Id, ban: bool) -> Self {
        Dialog { guild, user, ban, pick: None, text: String::new(), history: 1, need: false }
    }

    fn reason(&self) -> String {
        match self.pick {
            Some(i) if self.ban && i < 3 => REASONS[i].to_string(),
            _ => self.text.clone(),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(d) = &mut app.kickban else { return };
    let name = app.store.name_in(d.guild, d.user);
    let (mut go, mut close) = (false, false);
    let modal = super::motion::Modal::new("kickban".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                let title = if d.ban { format!("Ban @{name}?") } else { format!("Kick @{name} from Server") };
                ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            if !d.ban {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Are you sure you want to kick @{name} from the server? They will be able to rejoin again with a new invite.")).font(reg(16.0)).color(pal().text_normal));
            }
            ui.add_space(20.0);
            ui.label(RichText::new(if d.ban { "Reason for Ban" } else { "Reason for Kick" }).font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            if d.ban {
                if d.need {
                    ui.label(RichText::new("Ban reason must be provided").font(medium(14.0)).color(RED));
                    ui.add_space(8.0);
                }
                let shown = d.pick.map_or("Select a reason", |i| REASONS[i]).to_string();
                settings::dropdown(ui, ui.available_width(), None, &shown, |ui| {
                    for (i, r) in REASONS.iter().enumerate() {
                        if super::menu_item(ui, (d.pick == Some(i)).then_some(Icon::Check), r, false) {
                            d.pick = Some(i);
                            d.need = false;
                        }
                    }
                });
                ui.add_space(8.0);
            }
            if !d.ban || d.pick == Some(3) {
                input_frame().corner_radius(8).inner_margin(12).show(ui, |ui| {
                    let r = ui.add(egui::TextEdit::multiline(&mut d.text).frame(Frame::NONE).desired_rows(if d.ban { 5 } else { 2 }).char_limit(MAX_REASON).desired_width(ui.available_width()));
                    settings::mark(ui, "kickban:reason", r.rect);
                });
            }
            if d.ban {
                ui.add_space(24.0);
                ui.label(RichText::new("Delete Message History").font(semi(14.0)).color(pal().header_primary));
                ui.add_space(8.0);
                settings::dropdown(ui, ui.available_width(), None, HISTORY[d.history].0, |ui| {
                    for (i, (label, _)) in HISTORY.iter().enumerate() {
                        if super::menu_item(ui, (d.history == i).then_some(Icon::Check), label, false) {
                            d.history = i;
                        }
                    }
                });
            }
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= grey_button(ui, "Cancel", 212.0).clicked();
                let b = button(ui, if d.ban { "Ban" } else { "Kick" }, super::DANGER, 212.0);
                settings::mark(ui, "kickban:go", b.rect);
                go = b.clicked();
            });
        });
    go |= enter_submits(ctx, false);
    if go {
        let d = app.kickban.as_mut().unwrap();
        if d.ban && d.pick.is_none_or(|i| i == 3 && d.text.trim().is_empty()) {
            d.need = true;
            return;
        }
    }
    if go && let Some(d) = app.kickban.take() {
        let (gid, uid) = (d.guild, d.user);
        let who = app.store.name_in(gid, uid);
        if d.ban {
            let body = json!({ "delete_message_seconds": HISTORY[d.history].1 });
            app.audit("PUT", format!("/guilds/{gid}/bans/{uid}"), Some(body), &d.reason(), Tag::fail(format!("Whoops... {who} could not be banned.  Please try again!")));
        } else {
            let reason = d.reason();
            let query = if reason.trim().is_empty() { String::new() } else { format!("?reason={}", crate::api::esc(reason.trim())) };
            app.rest("DELETE", format!("/guilds/{gid}/members/{uid}{query}"), None, Tag::fail(format!("Whoops... {who} could not be kicked.  Please try again!")));
        }
    } else if close || modal.should_close() {
        app.kickban = None;
    }
}
