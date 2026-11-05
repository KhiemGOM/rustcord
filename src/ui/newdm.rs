//! Select Friends: the dialog behind New Group DM and the + beside Direct Messages. One friend
//! opens their dm, more make a group of up to ten.

use egui::{Align2, Color32, Frame, Key, Margin, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, avatar, button, icon_button, text};
use crate::net::Tag;
use crate::state::{FRIEND, Id};
use crate::theme::*;

const MOST: usize = 9;

#[derive(Default)]
pub struct State {
    query: String,
    pub(super) picked: Vec<Id>,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(s) = &mut app.new_dm else { return };
    let store = &app.store;
    let ql = s.query.trim().to_lowercase();
    let mut friends: Vec<(Id, String, String)> = store
        .relationships
        .iter()
        .filter(|(_, t)| **t == FRIEND)
        .filter_map(|(u, _)| store.user(*u).map(|user| (*u, user.display().to_string(), user.username.clone())))
        .filter(|(_, n, un)| ql.is_empty() || n.to_lowercase().contains(&ql) || un.to_lowercase().contains(&ql))
        .collect();
    friends.sort_by_key(|(_, n, _)| n.to_lowercase());
    let (mut close, mut go) = (false, false);
    let mut toggle = None;
    let modal = egui::Modal::new("new_dm".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Select Friends").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(4.0);
            let left = MOST.saturating_sub(s.picked.len());
            let sub = if left == 0 { "This group is full.".to_string() } else { format!("You can add {left} more friend{}.", if left == 1 { "" } else { "s" }) };
            ui.label(RichText::new(sub).font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            Frame::new().fill(pal().input_bg).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for u in &s.picked {
                        let name = store.user(*u).map(|u| u.display().to_string()).unwrap_or_default();
                        Frame::new().fill(pal().bg_accent).corner_radius(4).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                            ui.label(RichText::new(name).font(medium(14.0)).color(pal().header_primary));
                        });
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut s.query)
                            .frame(Frame::NONE)
                            .margin(Margin::ZERO)
                            .font(reg(16.0))
                            .hint_text(RichText::new("Type the username of a friend").color(pal().text_muted))
                            .desired_width(ui.available_width().max(120.0)),
                    );
                    if ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    if s.query.is_empty() && ui.input_mut(|i| super::take_key(i, egui::Modifiers::NONE, Key::Backspace)) {
                        s.picked.pop();
                    }
                });
            });
            ui.add_space(12.0);
            egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                for (u, name, username) in &friends {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 8, pal().control_hover);
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", format!("newdm:{u}"))), r));
                    if let Some(user) = store.user(*u) {
                        avatar(ui, egui::Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                    }
                    let nr = text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y), Align2::LEFT_CENTER, name, semi(16.0), pal().header_primary, 200.0);
                    ui.painter().text(pos2(nr.right() + 8.0, r.center().y), Align2::LEFT_CENTER, username, reg(14.0), pal().header_secondary);
                    let on = s.picked.contains(u);
                    let b = egui::Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(24.0, 24.0));
                    if on {
                        ui.painter().rect_filled(b, 6, BLURPLE);
                        icons::draw(ui.painter(), Icon::Check, b.shrink(4.0), Color32::WHITE);
                    } else {
                        ui.painter().rect_stroke(b, 6, Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                    }
                    if resp.clicked() && (on || s.picked.len() < MOST) {
                        toggle = Some(*u);
                    }
                }
                if friends.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("No friends found").color(pal().header_secondary)));
                    ui.add_space(16.0);
                }
            });
            ui.add_space(16.0);
            let label = if s.picked.len() > 1 { "Create Group DM" } else { "Create DM" };
            ui.add_enabled_ui(!s.picked.is_empty(), |ui| {
                let b = button(ui, label, BLURPLE, 392.0);
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "newdm_go")), b.rect));
                go |= b.clicked();
            });
        });
    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        if ql.is_empty() {
            go = true;
        } else {
            toggle = friends.first().map(|f| f.0);
            s.query.clear();
        }
    }
    if let Some(u) = toggle {
        match s.picked.iter().position(|p| *p == u) {
            Some(i) => {
                s.picked.remove(i);
            }
            None if s.picked.len() < MOST => s.picked.push(u),
            None => {}
        }
    }
    if go && !s.picked.is_empty() {
        let picked = std::mem::take(&mut s.picked);
        app.new_dm = None;
        match picked[..] {
            [one] => app.dm_text(one, String::new()),
            _ => {
                let ids: Vec<String> = picked.iter().map(|u| u.to_string()).collect();
                app.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": ids })), Tag::OpenDm(String::new()));
            }
        }
    } else if close || modal.should_close() {
        app.new_dm = None;
    }
}
