//! The small dialogs behind a person's menu: the note on them, their nickname (a server's or the
//! one you give a friend) and a group dm's name and icon.

use egui::{Align, Frame, Layout, RichText, Sense, vec2};
use serde_json::json;

use super::icons::Icon;
use super::{App, button, grey_button, icon_button};
use crate::net::Tag;
use crate::state::{Id, Store};
use crate::theme::*;

pub enum Dialog {
    Note { uid: Id, text: String },
    Nick { gid: Id, uid: Id, text: String, had: bool },
    Group { ch: Id, text: String, icon: Option<String> },
    Cleanup { picked: std::collections::HashSet<Id> },
    Accounts,
}

impl Dialog {
    pub fn note(app: &App, uid: Id) -> Self {
        Dialog::Note { uid, text: app.notes.get(&uid).cloned().unwrap_or_default() }
    }

    pub fn nick(store: &Store, gid: Id, uid: Id) -> Self {
        let had = if gid == 0 { store.nicks.contains_key(&uid) } else { store.members.get(&gid).and_then(|m| m.get(&uid)).is_some_and(|m| m.nick.is_some()) };
        let text = if had { store.name_in(gid, uid) } else { String::new() };
        Dialog::Nick { gid, uid, text, had }
    }

    pub fn cleanup() -> Self {
        Dialog::Cleanup { picked: Default::default() }
    }

    pub fn group(store: &Store, ch: Id) -> Self {
        Dialog::Group { ch, text: store.channels.get(&ch).map(|c| c.name.clone()).unwrap_or_default(), icon: None }
    }
}

enum Done {
    Save,
    Reset,
    Close,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    match app.user_dialog {
        Some(Dialog::Cleanup { .. }) => return cleanup(app, ctx),
        Some(Dialog::Accounts) => return super::accounts::manage_modal(app, ctx),
        _ => {}
    }
    let Some(d) = &mut app.user_dialog else { return };
    let mut done = None;
    let mut pick = false;
    let (title, about, label) = match d {
        Dialog::Note { .. } => ("Note", Some("Only visible to you"), "Note"),
        Dialog::Nick { gid: 0, had, .. } => (if *had { "Change Friend Nickname" } else { "Add Friend Nickname" }, Some("Find a friend faster with a personal nickname. It will only be visible to you in your direct messages."), "Friend Nickname"),
        Dialog::Nick { .. } => ("Change Nickname", None, "Nickname"),
        Dialog::Group { .. } => ("Edit Group", None, ""),
        Dialog::Cleanup { .. } | Dialog::Accounts => return,
    };
    let me = app.store.me;
    let icon_of = |ch: Id| app.store.channels.get(&ch).and_then(|c| c.icon.clone());
    let modal = crate::ui::motion::Modal::new("user_dialog".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                        done = Some(Done::Close);
                    }
                });
            });
            ui.add_space(16.0);
            if let Some(a) = about {
                ui.label(RichText::new(a).font(reg(14.0)).color(pal().text_normal));
                ui.add_space(16.0);
            }
            if let Dialog::Group { ch, icon, .. } = d {
                ui.vertical_centered(|ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::click());
                    match (&*icon, icon_of(*ch)) {
                        (Some(uri), _) => {
                            egui::Image::from_uri(uri.clone()).corner_radius(40).show_loading_spinner(false).paint_at(ui, r);
                        }
                        (None, Some(h)) => super::avatar(ui, r, &format!("{}/channel-icons/{ch}/{h}.png?size=128", crate::api::cdn())),
                        (None, None) => {
                            ui.painter().circle_filled(r.center(), 40.0, pal().bg_mod_selected);
                            super::icons::draw(ui.painter(), Icon::People, egui::Rect::from_center_size(r.center(), vec2(40.0, 40.0)), pal().header_secondary);
                        }
                    }
                    let badge = egui::Rect::from_center_size(r.right_top() + vec2(-6.0, 10.0), vec2(28.0, 28.0));
                    ui.painter().circle_filled(badge.center(), 14.0, pal().bg_floating);
                    super::icons::draw(ui.painter(), Icon::Pencil, badge.shrink(6.0), pal().header_primary);
                    let resp = resp.on_hover_text("Edit Group Icon");
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    pick = resp.clicked();
                });
                ui.add_space(24.0);
            } else {
                ui.label(RichText::new(label).font(semi(12.0)).color(pal().header_primary));
                ui.add_space(8.0);
            }
            let hint = match d {
                Dialog::Note { .. } => "Click to add a note",
                Dialog::Nick { .. } => "Nickname",
                Dialog::Group { .. } => "Group name",
                Dialog::Cleanup { .. } | Dialog::Accounts => "",
            };
            let text = match d {
                Dialog::Note { text, .. } | Dialog::Nick { text, .. } | Dialog::Group { text, .. } => text,
                Dialog::Cleanup { .. } | Dialog::Accounts => return,
            };
            let multi = label == "Note";
            input_frame().corner_radius(8).inner_margin(12).show(ui, |ui| {
                let edit = if multi { egui::TextEdit::multiline(text).desired_rows(4) } else { egui::TextEdit::singleline(text) };
                let r = ui.add(edit.frame(Frame::NONE).font(reg(16.0)).hint_text(hint).desired_width(ui.available_width()));
                if !ui.ctx().memory(|m| m.areas().visible_last_frame(&ui.layer_id())) {
                    r.request_focus();
                }
                if !multi && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(Done::Save);
                }
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if matches!(d, Dialog::Nick { had: true, .. }) {
                    let reset = if matches!(d, Dialog::Nick { gid: 0, .. }) { "Reset Friend Nickname" } else { "Reset Nickname" };
                    let g = ui.painter().layout_no_wrap(reset.into(), medium(14.0), pal().link);
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x, 38.0), Sense::click());
                    ui.painter().galley(egui::pos2(r.left(), r.center().y - g.size().y / 2.0), g, pal().link);
                    if resp.clicked() {
                        done = Some(Done::Reset);
                    }
                    ui.add_space(ui.available_width() - 212.0 - 8.0 - 100.0);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if button(ui, "Save", BLURPLE, 100.0).clicked() {
                        done = Some(Done::Save);
                    }
                    if grey_button(ui, "Cancel", 100.0).clicked() {
                        done = Some(Done::Close);
                    }
                });
            });
        });
    if pick && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
        if let (Some(Dialog::Group { icon, .. }), Ok(bytes)) = (&mut app.user_dialog, std::fs::read(&path)) {
            use base64::Engine;
            let mime = match bytes.get(..4) {
                Some([0x89, b'P', ..]) => "image/png",
                Some([b'G', b'I', ..]) => "image/gif",
                Some([b'R', b'I', ..]) => "image/webp",
                _ => "image/jpeg",
            };
            *icon = Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
        }
        return;
    }
    if modal.should_close() {
        done = Some(Done::Close);
    }
    let Some(done) = done else { return };
    let Some(d) = app.user_dialog.take() else { return };
    if matches!(done, Done::Close) {
        return;
    }
    let reset = matches!(done, Done::Reset);
    match d {
        Dialog::Note { uid, text } => {
            let was = app.notes.get(&uid).cloned();
            app.rest("PUT", format!("/users/@me/notes/{uid}"), Some(json!({ "note": text })), Tag::rollback(move |app: &mut App| {
                match &was {
                    Some(n) => app.notes.insert(uid, n.clone()),
                    None => app.notes.remove(&uid),
                };
            }));
            app.notes.insert(uid, text);
        }
        Dialog::Nick { gid: 0, uid, text, .. } => {
            let text = if reset { String::new() } else { text.trim().to_string() };
            let was = app.store.nicks.get(&uid).cloned();
            app.rest("PATCH", format!("/users/@me/relationships/{uid}"), Some(json!({ "nickname": (!text.is_empty()).then_some(&text) })), Tag::rollback(move |app: &mut App| {
                match &was {
                    Some(n) => app.store.nicks.insert(uid, n.clone()),
                    None => app.store.nicks.remove(&uid),
                };
            }));
            if text.is_empty() {
                app.store.nicks.remove(&uid);
            } else {
                app.store.nicks.insert(uid, text);
            }
        }
        Dialog::Nick { gid, uid, text, .. } => {
            let text = if reset { String::new() } else { text.trim().to_string() };
            let who = if uid == me { "@me".to_string() } else { uid.to_string() };
            app.rest("PATCH", format!("/guilds/{gid}/members/{who}"), Some(json!({ "nick": (!text.is_empty()).then_some(&text) })), Tag::reason());
        }
        Dialog::Group { ch, text, icon } => {
            let mut body = json!({ "name": text.trim() });
            if let Some(i) = icon {
                body["icon"] = json!(i);
            }
            app.rest("PATCH", format!("/channels/{ch}"), Some(body), Tag::reason());
        }
        Dialog::Cleanup { .. } | Dialog::Accounts => {}
    }
}

/// Clean up Group DMs: the groups by how long they've been quiet, oldest first, to tick and leave.
fn cleanup(app: &mut App, ctx: &egui::Context) {
    let mut groups: Vec<(Id, String, i64, usize)> = app
        .store
        .dms
        .iter()
        .filter_map(|c| app.store.channels.get(c))
        .filter(|c| c.kind == 3)
        .map(|c| (c.id, super::channel_title(&app.store, c.id), crate::state::snowflake_ms(c.last_msg.max(c.id)), c.recipients.len() + 1))
        .collect();
    groups.sort_by_key(|g| g.2);
    let Some(Dialog::Cleanup { picked }) = &mut app.user_dialog else { return };
    let mut done = None;
    let modal = crate::ui::motion::Modal::new("user_dialog".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Clean up Group DMs").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                        done = Some(Done::Close);
                    }
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Pick the group DMs you're done with, oldest first, and leave them all at once.").font(reg(14.0)).color(pal().text_normal));
            ui.add_space(16.0);
            if groups.is_empty() {
                ui.label(RichText::new("You aren't in any group DMs.").font(reg(14.0)).color(pal().text_muted));
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (id, title, last, n) in &groups {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r, 4, pal().bg_mod_hover, 0.05);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let on = picked.contains(id);
                    let boxr = egui::Rect::from_center_size(egui::pos2(r.right() - 20.0, r.center().y), vec2(20.0, 20.0));
                    if on {
                        ui.painter().rect_filled(boxr, 4, BLURPLE);
                        super::icons::draw(ui.painter(), Icon::Check, boxr.shrink(3.0), egui::Color32::WHITE);
                    } else {
                        ui.painter().rect_stroke(boxr, 4, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                    }
                    let when = chrono::DateTime::from_timestamp_millis(*last).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
                    super::text::paint(ui.painter(), egui::pos2(r.left() + 8.0, r.center().y - 9.0), egui::Align2::LEFT_CENTER, title, medium(16.0), pal().header_primary, r.width() - 60.0);
                    ui.painter().text(egui::pos2(r.left() + 8.0, r.center().y + 10.0), egui::Align2::LEFT_CENTER, format!("{n} Members • Last active {when}"), reg(12.0), pal().text_muted);
                    if resp.clicked() && !picked.remove(id) {
                        picked.insert(*id);
                    }
                }
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if grey_button(ui, "Cancel", 212.0).clicked() {
                    done = Some(Done::Close);
                }
                let label = if picked.is_empty() { "Leave".to_string() } else { format!("Leave {}", picked.len()) };
                if button(ui, &label, super::DANGER, 212.0).clicked() && !picked.is_empty() {
                    done = Some(Done::Save);
                }
            });
        });
    if modal.should_close() {
        done = Some(Done::Close);
    }
    let Some(done) = done else { return };
    if let (Done::Save, Some(Dialog::Cleanup { picked })) = (done, app.user_dialog.take()) {
        for c in picked {
            super::popups::close_dm(app, c);
        }
    } else {
        app.user_dialog = None;
    }
}
