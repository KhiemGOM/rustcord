use egui::{Align, Align2, Frame, Key, Layout, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, avatar, icon_button, text};
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

fn modal_button(ui: &mut egui::Ui, text: &str, fill: Option<egui::Color32>, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let k = crate::ui::motion::hot(ui.ctx(), &resp);
    let (fill, ink, edge) = match fill {
        Some(c) => (crate::ui::motion::mix(c, c.gamma_multiply(0.85), k), egui::Color32::WHITE, Stroke::NONE),
        None => (crate::ui::motion::mix(pal().control, pal().control_hover, k), pal().header_primary, Stroke::new(1.0, pal().border_subtle)),
    };
    ui.painter().rect(rect, 8, fill, edge, egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, medium(16.0), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
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
    let title = if adding { "Invite to Group DM" } else if p.base.is_some() { "Add Friends to DM" } else { "New Message" };
    let mut link = false;
    let button_text = if adding { "Add" } else { "Create Message" };
    let modal = crate::ui::motion::Modal::new("group_pick".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 24, bottom: 4 }).show(ui, |ui| {
                ui.set_width(432.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
                });
                ui.add_space(5.3);
                let sub = if adding { format!("You can add {max} more {}.", if max == 1 { "person" } else { "people" }) } else { format!("Group DMs can have up to {LIMIT} members.") };
                ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(24.7);
                if p.q.is_empty() && ui.input_mut(|i| super::take_key(i, egui::Modifiers::NONE, Key::Backspace)) {
                    p.picked.pop();
                }
                let (row, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                let field = if adding { Rect::from_min_size(row.min, vec2(432.0 - 120.0, 40.0)) } else { row };
                if adding {
                    let at = Rect::from_min_size(pos2(row.right() - 112.0, row.top()), vec2(112.0, 40.0));
                    let ready = !p.picked.is_empty();
                    let resp = ui.interact(at, ui.id().with("pick_add"), if ready { Sense::click() } else { Sense::hover() });
                    ui.painter().rect_filled(at, 8, pal().blurple.gamma_multiply(if ready { 1.0 } else { 0.5 }));
                    ui.painter().text(at.center(), Align2::CENTER_CENTER, "Add", medium(16.0), egui::Color32::WHITE);
                    super::settings::mark(ui, "pick:go", at);
                    go |= resp.clicked();
                }
                ui.painter().rect_filled(field, 8, pal().input_bg);
                let e = ui.put(
                    Rect::from_min_max(pos2(field.left() + 12.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0)),
                    egui::TextEdit::singleline(&mut p.q).hint_text(RichText::new("Search").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO),
                );
                ui.painter().rect_stroke(field, 8, Stroke::new(1.0, pal().blurple), egui::StrokeKind::Inside);
                if std::mem::take(&mut p.fresh) || ui.memory(|m| m.focused().is_none()) {
                    e.request_focus();
                }
                ui.add_space(13.7);
                ui.label(RichText::new("Add friends or server members to group DMs.").font(reg(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
            });
            let h = ((ui.ctx().content_rect().height() - 100.0).min(800.0) - 264.5).clamp(144.0, 600.0);
            egui::ScrollArea::vertical().max_height(h).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(17, 0)).show(ui, |ui| {
                    for u in &people {
                        let Some(user) = store.user(*u) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(446.0, 48.0), Sense::click());
                        super::settings::mark(ui, &format!("pick:{u}"), r);
                        let on = p.picked.contains(u);
                        if resp.hovered() || on {
                            ui.painter().rect_filled(r, 8, pal().control_hover);
                        }
                        avatar(ui, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                        let n = text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y - 9.0), Align2::LEFT_CENTER, &store.name_in(0, *u), medium(16.0), pal().text_normal, 300.0);
                        ui.painter().text(pos2(n.left(), r.center().y + 10.0), Align2::LEFT_CENTER, &user.username, reg(12.0), pal().text_muted);
                        let cb = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(24.0, 24.0));
                        ui.painter().rect(cb, 4, if on { pal().blurple } else { egui::Color32::TRANSPARENT }, Stroke::new(2.0, if on { pal().blurple } else { pal().header_secondary }), egui::StrokeKind::Inside);
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
                if adding {
                    ui.label(RichText::new("Or, send an invite link").font(medium(16.0)).color(pal().header_primary));
                    ui.add_space(8.0);
                    let (r, _) = ui.allocate_exact_size(vec2(432.0, 48.0), Sense::hover());
                    ui.painter().rect(r, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "https://discord.gg/example", reg(16.0), pal().text_muted);
                    let b = Rect::from_center_size(pos2(r.right() - 36.0, r.center().y), vec2(60.0, 32.0));
                    let resp = ui.interact(b, ui.id().with("pick_link"), Sense::click());
                    ui.painter().rect_filled(b, 4, pal().blurple);
                    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create", medium(14.0), egui::Color32::WHITE);
                    super::settings::mark(ui, "pick:link", b);
                    link = resp.clicked();
                    return;
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    close |= modal_button(ui, "Cancel", None, 212.0).clicked();
                    let b = ui.add_enabled_ui(!p.picked.is_empty(), |ui| modal_button(ui, button_text, Some(pal().blurple), 212.0)).inner;
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
    if link && let Some(c) = app.group_pick.take().and_then(|p| p.base) {
        app.invite_people(c, "Group DM Invite Create");
        return;
    }
    if go && let Some(p) = app.group_pick.take() {
        match p.base.and_then(|c| app.store.channels.get(&c).map(|c| (c.id, c.kind, c.recipients.clone()))) {
            Some((c, 3, _)) => {
                for u in &p.picked {
                    app.rest_in("PUT", format!("/channels/{c}/recipients/{u}"), None, "Add Friends to DM", Tag::fail("Failed to add user to the group"));
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
