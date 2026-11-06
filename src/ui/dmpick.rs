use egui::{Align, Align2, Frame, Key, Layout, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, avatar, button, icon_button, text};
use crate::net::Tag;
use crate::state::{FRIEND, Id};
use crate::theme::*;

const LIMIT: usize = 10;

/// Discord's friend picker for a group DM: a new one, or more people for the one open.
pub struct Pick {
    base: Option<Id>,
    pub picked: Vec<Id>,
    q: String,
    fresh: bool,
}

impl Pick {
    pub fn new(base: Option<Id>) -> Self {
        Pick { base, picked: Vec::new(), q: String::new(), fresh: true }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(p) = &mut app.group_pick else { return };
    let store = &app.store;
    let there: Vec<Id> = p.base.and_then(|c| store.channels.get(&c)).map(|c| c.recipients.clone()).unwrap_or_default();
    let adding = p.base.is_some_and(|c| store.channels.get(&c).is_some_and(|c| c.kind == 3));
    let max = LIMIT.saturating_sub(1 + there.len());
    let ql = p.q.trim().to_lowercase();
    let mut people: Vec<Id> = store.relationships.iter().filter(|(_, t)| **t == FRIEND).map(|(u, _)| *u).collect();
    if !ql.is_empty()
        && let Some(g) = app.ui.guild
    {
        people.extend(store.members.get(&g).into_iter().flat_map(|m| m.keys().copied()));
    }
    people.sort_by_key(|u| store.name_in(0, *u).to_lowercase());
    people.dedup();
    people.retain(|u| *u != store.me && !there.contains(u));
    people.retain(|u| ql.is_empty() || store.user(*u).is_some_and(|x| x.username.to_lowercase().contains(&ql) || store.name_in(0, *u).to_lowercase().contains(&ql)));
    people.truncate(60);
    let (mut go, mut close) = (false, false);
    let title = if p.base.is_some() { "Add Friends to DM" } else { "New Message" };
    let button_text = if adding { "Add" } else { "Create Message" };
    let modal = crate::ui::motion::Modal::new("group_pick".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 24, bottom: 8 }).show(ui, |ui| {
                ui.set_width(432.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
                });
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Group DMs can have up to {LIMIT} members.")).font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(16.0);
                if p.q.is_empty() && ui.input_mut(|i| super::take_key(i, egui::Modifiers::NONE, Key::Backspace)) {
                    p.picked.pop();
                }
                let (field, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect_filled(field, 8, pal().input_bg);
                let e = ui.put(
                    Rect::from_min_max(pos2(field.left() + 12.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0)),
                    egui::TextEdit::singleline(&mut p.q).hint_text(RichText::new("Search").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO),
                );
                ui.painter().rect_stroke(field, 8, Stroke::new(1.0, BLURPLE), egui::StrokeKind::Inside);
                if std::mem::take(&mut p.fresh) || ui.memory(|m| m.focused().is_none()) {
                    e.request_focus();
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Add friends or server members to group DMs.").font(reg(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
            });
            let h = (ui.ctx().content_rect().height() - 320.0).clamp(144.0, 600.0);
            egui::ScrollArea::vertical().max_height(h).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(8, 0)).show(ui, |ui| {
                    for u in &people {
                        let Some(user) = store.user(*u) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(464.0, 48.0), Sense::click());
                        super::settings::mark(ui, &format!("pick:{u}"), r);
                        let on = p.picked.contains(u);
                        if resp.hovered() || on {
                            ui.painter().rect_filled(r, 8, pal().control_hover);
                        }
                        avatar(ui, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                        let n = text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y - 8.0), Align2::LEFT_CENTER, &store.name_in(0, *u), semi(16.0), pal().header_primary, 300.0);
                        ui.painter().text(pos2(n.left(), r.center().y + 10.0), Align2::LEFT_CENTER, &user.username, reg(12.0), pal().header_secondary);
                        let cb = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(24.0, 24.0));
                        ui.painter().rect(cb, 4, if on { BLURPLE } else { egui::Color32::TRANSPARENT }, Stroke::new(2.0, if on { BLURPLE } else { pal().header_secondary }), egui::StrokeKind::Inside);
                        if on {
                            icons::draw(ui.painter(), Icon::Check, cb.shrink(3.0), egui::Color32::WHITE);
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            if on {
                                p.picked.retain(|x| x != u);
                            } else if p.picked.len() < max {
                                p.picked.push(*u);
                            }
                        }
                    }
                });
            });
            let (sep, _) = ui.allocate_exact_size(vec2(480.0, 1.0), Sense::hover());
            ui.painter().hline(sep.x_range(), sep.center().y, Stroke::new(1.0, pal().border_subtle));
            Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    close |= super::grey_button(ui, "Cancel", 212.0).clicked();
                    let b = ui.add_enabled_ui(!p.picked.is_empty(), |ui| button(ui, button_text, BLURPLE, 212.0)).inner;
                    super::settings::mark(ui, "pick:go", b.rect);
                    go = b.clicked();
                });
            });
        });
    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        if ql.is_empty() {
            go |= !p.picked.is_empty();
        } else if let Some(u) = people.first() {
            if p.picked.contains(u) {
                p.picked.retain(|x| x != u);
            } else if p.picked.len() < max {
                p.picked.push(*u);
            }
            p.q.clear();
        }
    }
    if go && let Some(p) = app.group_pick.take() {
        match p.base.and_then(|c| app.store.channels.get(&c).map(|c| (c.id, c.kind, c.recipients.clone()))) {
            Some((c, 3, _)) => {
                for u in &p.picked {
                    app.rest("PUT", format!("/channels/{c}/recipients/{u}"), None, Tag::Quiet);
                }
            }
            other => {
                let mut ids: Vec<String> = other.map(|(_, _, r)| r).unwrap_or_default().iter().map(|u| u.to_string()).collect();
                ids.extend(p.picked.iter().map(|u| u.to_string()));
                if let [one] = p.picked.as_slice()
                    && p.base.is_none()
                {
                    app.open_dm(*one);
                } else {
                    app.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": ids })), Tag::OpenDm(String::new()));
                }
            }
        }
    } else if close || modal.should_close() {
        app.group_pick = None;
    }
}
