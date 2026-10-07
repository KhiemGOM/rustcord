use egui::{Align, Align2, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, button, icon_button, status_dot};
use crate::net::Tag;
use crate::state::{BLOCKED, FRIEND, INCOMING, Id, OUTGOING, Status};
use crate::theme::*;

const TABS: [&str; 4] = ["Online", "All", "Pending", "Blocked"];

pub fn show(app: &mut App, ui: &mut Ui) {
    let pending = app.store.relationships.values().filter(|t| **t == INCOMING).count() as u32;
    egui::Panel::top("friends_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), Stroke::new(1.0, BG_TERTIARY));
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("👥").font(reg(18.0)).color(CHANNEL_DEFAULT));
                ui.label(RichText::new("Friends").font(semi(16.0)).color(HEADER_PRIMARY));
                let (sep, _) = ui.allocate_exact_size(vec2(17.0, 24.0), Sense::hover());
                ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, BG_ACCENT));
                for (i, t) in TABS.iter().enumerate() {
                    let sel = app.friends_tab == i as u8;
                    let g = ui.painter().layout_no_wrap(t.to_string(), semi(16.0), HEADER_PRIMARY);
                    let extra = if i == 2 && pending > 0 { 24.0 } else { 0.0 };
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0 + extra, 26.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_SELECTED);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_HOVER);
                    }
                    let col = if sel { HEADER_PRIMARY } else { HEADER_SECONDARY };
                    ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, *t, semi(16.0), col);
                    if extra > 0.0 {
                        super::badge(ui, pos2(r.right() - 6.0, r.center().y), pending, None);
                    }
                    if resp.clicked() {
                        app.friends_tab = i as u8;
                    }
                }
                let add = app.friends_tab == 4;
                let fill = if add { egui::Color32::TRANSPARENT } else { GREEN };
                let g = ui.painter().layout_no_wrap("Add Friend".into(), semi(14.0), if add { GREEN } else { egui::Color32::WHITE });
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 26.0), Sense::click());
                ui.painter().rect_filled(r, CornerRadius::same(4), fill);
                ui.painter().galley(r.center() - g.size() / 2.0, g, GREEN);
                if resp.clicked() {
                    app.friends_tab = 4;
                }
            });
        });

    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: 30, right: 20, top: 16, bottom: 0 }).show(ui, |ui| {
            if app.friends_tab == 4 {
                add_friend(app, ui);
            } else {
                list(app, ui);
            }
        });
    });
}

fn add_friend(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("ADD FRIEND").font(bold(16.0)).color(HEADER_PRIMARY));
    ui.add_space(8.0);
    ui.label(RichText::new("You can add friends with their Discord username.").color(HEADER_SECONDARY));
    ui.add_space(16.0);
    let mut send = false;
    Frame::new().fill(BG_TERTIARY).corner_radius(8).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut app.add_friend)
                    .frame(Frame::NONE)
                    .font(reg(16.0))
                    .hint_text("You can add friends with their Discord username.")
                    .desired_width(ui.available_width() - 190.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                send = true;
            }
            if button(ui, "Send Friend Request", BLURPLE, 180.0).clicked() {
                send = true;
            }
        });
    });
    let name = app.add_friend.trim().to_string();
    if send && !name.is_empty() {
        app.rest("POST", "/users/@me/relationships".into(), Some(json!({ "username": name, "discriminator": null })), Tag::Friend);
        app.add_friend.clear();
    }
}

fn list(app: &mut App, ui: &mut Ui) {
    let tab = app.friends_tab;
    let mut people: Vec<(Id, u8)> = app
        .store
        .relationships
        .iter()
        .map(|(u, t)| (*u, *t))
        .filter(|(u, t)| match tab {
            0 => *t == FRIEND && app.store.status(*u) != Status::Offline,
            1 => *t == FRIEND,
            2 => *t == INCOMING || *t == OUTGOING,
            _ => *t == BLOCKED,
        })
        .collect();
    people.sort_by_key(|(u, _)| app.store.name_in(0, *u).to_lowercase());
    let heading = format!("{} — {}", ["ONLINE", "ALL FRIENDS", "PENDING", "BLOCKED"][tab as usize], people.len());
    ui.label(RichText::new(heading).font(semi(12.0)).color(HEADER_SECONDARY));
    ui.add_space(12.0);

    let mut act: Option<(Id, &'static str)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (uid, kind) in people {
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
            ui.painter().hline(row.x_range(), row.top(), Stroke::new(1.0, BG_MOD_HOVER));
            if resp.hovered() {
                ui.painter().rect_filled(row.shrink2(vec2(-10.0, 1.0)), CornerRadius::same(8), BG_MOD_HOVER);
            }
            let av = Rect::from_min_size(pos2(row.left(), row.center().y - 16.0), vec2(32.0, 32.0));
            let Some(u) = app.store.user(uid) else { continue };
            avatar(ui, av, &u.avatar_url());
            let st = app.store.status(uid);
            if kind == FRIEND {
                status_dot(ui, av, st, if resp.hovered() { BG_MOD_HOVER } else { BG_PRIMARY });
            }
            ui.painter().text(pos2(av.right() + 12.0, row.center().y - 9.0), Align2::LEFT_CENTER, app.store.name_in(0, uid), semi(16.0), HEADER_PRIMARY);
            let sub = match kind {
                INCOMING => "Incoming Friend Request".to_string(),
                OUTGOING => "Outgoing Friend Request".to_string(),
                BLOCKED => "Blocked".to_string(),
                _ => format!("{st:?}").replace("Dnd", "Do Not Disturb"),
            };
            ui.painter().text(pos2(av.right() + 12.0, row.center().y + 10.0), Align2::LEFT_CENTER, sub, reg(14.0), HEADER_SECONDARY);

            let mut br = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(row.right() - 100.0, row.top()), row.max))
                    .layout(Layout::right_to_left(Align::Center)),
            );
            match kind {
                FRIEND => {
                    if icon_button(&mut br, "💬", 36.0, "Message").clicked() {
                        act = Some((uid, "dm"));
                    }
                }
                INCOMING => {
                    if icon_button(&mut br, "✖", 36.0, "Ignore").clicked() {
                        act = Some((uid, "remove"));
                    }
                    if icon_button(&mut br, "✔", 36.0, "Accept").clicked() {
                        act = Some((uid, "accept"));
                    }
                }
                _ => {
                    if icon_button(&mut br, "✖", 36.0, if kind == BLOCKED { "Unblock" } else { "Cancel" }).clicked() {
                        act = Some((uid, "remove"));
                    }
                }
            }
            if resp.clicked() && kind == FRIEND {
                act = Some((uid, "dm"));
            }
        }
    });
    match act {
        Some((u, "dm")) => app.open_dm(u),
        Some((u, "accept")) => {
            app.store.relationships.insert(u, FRIEND);
            app.rest("PUT", format!("/users/@me/relationships/{u}"), Some(json!({})), Tag::Quiet);
        }
        Some((u, _)) => {
            app.store.relationships.remove(&u);
            app.rest("DELETE", format!("/users/@me/relationships/{u}"), None, Tag::Quiet);
        }
        None => {}
    }
}
