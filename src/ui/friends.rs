use egui::{Align, Align2, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, button, icon_button, status_dot};
use super::icons::Icon;
use crate::net::Tag;
use crate::state::{BLOCKED, FRIEND, INCOMING, Id, OUTGOING, Status};
use crate::theme::*;

// discord dropped the blocked tab; blocked people live in settings now
const TABS: [&str; 3] = ["Online", "All", "Pending"];

pub fn show(app: &mut App, ui: &mut Ui) {
    let pending = app.store.relationships.values().filter(|t| **t == INCOMING).count() as u32;
    egui::Panel::top("friends_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), Stroke::new(1.0, pal().border_subtle));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                super::icons::draw(ui.painter(), Icon::Friends, r, pal().channel_default);
                ui.label(RichText::new("Friends").font(medium(16.0)).color(pal().header_primary));
                let (dot, _) = ui.allocate_exact_size(vec2(4.0, 4.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.0, egui::Color32::from_rgba_unmultiplied(151, 151, 159, 61));
                for (i, t) in TABS.iter().enumerate() {
                    let sel = app.friends_tab == i as u8;
                    let g = ui.painter().layout_no_wrap(t.to_string(), medium(16.0), pal().header_primary);
                    let extra = if i == 2 && pending > 0 { 24.0 } else { 0.0 };
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0 + extra, 32.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_hover);
                    }
                    let col = if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                    ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, *t, medium(16.0), col);
                    if extra > 0.0 {
                        super::badge(ui, pos2(r.right() - 6.0, r.center().y), pending, None);
                    }
                    if resp.clicked() {
                        app.friends_tab = i as u8;
                    }
                }
                let add = app.friends_tab == 4;
                let g = ui.painter().layout_no_wrap("Add Friend".into(), medium(16.0), egui::Color32::WHITE);
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click());
                let fill = if add {
                    BLURPLE.gamma_multiply(0.24)
                } else if resp.hovered() {
                    egui::Color32::from_rgb(0x44, 0x52, 0xbb)
                } else {
                    BLURPLE
                };
                ui.painter().rect_filled(r, CornerRadius::same(8), fill);
                ui.painter().galley(r.center() - g.size() / 2.0, g, if add { pal().link } else { egui::Color32::WHITE });
                if resp.clicked() {
                    app.friends_tab = 4;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::NewDm, 32.0, "New Group DM").clicked() {
                        app.new_dm = Some(Default::default());
                    }
                });
            });
        });

    if app.friends_tab != 4 && ui.available_width() > 1000.0 {
        active_now(app, ui);
    }
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: 24, right: 20, top: 16, bottom: 0 }).show(ui, |ui| {
            if app.friends_tab == 4 {
                add_friend(app, ui);
            } else {
                list(app, ui);
            }
        });
    });
}

fn add_friend(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("Add Friend").font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    ui.label(RichText::new("You can add friends with their Discord username.").color(pal().header_secondary));
    ui.add_space(16.0);
    let mut send = false;
    input_frame().corner_radius(8).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
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
    let qid = egui::Id::new("friends_q");
    let mut q: String = ui.data(|d| d.get_temp(qid).unwrap_or_default());
    input_frame().corner_radius(8).inner_margin(Margin::symmetric(12, 7)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            super::icons::draw(ui.painter(), Icon::Search, ir, pal().header_secondary);
            ui.add(egui::TextEdit::singleline(&mut q).frame(Frame::NONE).font(reg(16.0)).hint_text("Search").desired_width(ui.available_width()));
        });
    });
    ui.data_mut(|d| d.insert_temp(qid, q.clone()));
    let q = q.to_lowercase();
    if !q.is_empty() {
        people.retain(|(u, _)| app.store.name_in(0, *u).to_lowercase().contains(&q) || app.store.user(*u).is_some_and(|x| x.username.contains(&q)));
    }
    ui.add_space(20.0);
    let heading = format!("{} — {}", ["Online", "All Friends", "Pending"].get(tab as usize).unwrap_or(&"Friends"), people.len());
    ui.label(RichText::new(heading).font(medium(14.0)).color(pal().header_primary));
    ui.add_space(12.0);

    let mut act: Option<(Id, &'static str)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (uid, kind) in people {
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
            ui.painter().hline(row.x_range(), row.top(), Stroke::new(1.0, pal().bg_mod_hover));
            if resp.hovered() {
                ui.painter().rect_filled(row.shrink2(vec2(-10.0, 1.0)), CornerRadius::same(8), pal().bg_mod_hover);
            }
            let av = Rect::from_min_size(pos2(row.left(), row.center().y - 16.0), vec2(32.0, 32.0));
            let Some(u) = app.store.user(uid) else { continue };
            avatar(ui, av, &u.avatar_url());
            if let Some(d) = u.decoration_url() {
                super::text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(38.0, 38.0)));
            }
            let st = app.store.status(uid);
            if kind == FRIEND {
                status_dot(ui, av, st, if resp.hovered() { pal().bg_mod_hover } else { pal().bg_primary });
            }
            let nr = super::text::paint(ui.painter(), pos2(av.right() + 12.0, row.center().y - 9.0), Align2::LEFT_CENTER, &app.store.name_in(0, uid), semi(16.0), pal().header_primary, row.width() - 220.0);
            super::tag_chip(ui.painter(), u, pos2(nr.right() + 9.0, nr.center().y), 1.0);
            // a custom status shows in place of the status, as discord does
            let custom = app.store.activity_line(uid).filter(|(_, _, game)| game.is_empty());
            let sub = match kind {
                INCOMING => "Incoming Friend Request".to_string(),
                OUTGOING => "Outgoing Friend Request".to_string(),
                BLOCKED => "Blocked".to_string(),
                _ if custom.is_some() => custom.as_ref().map(|c| c.1.clone()).unwrap_or_default(),
                _ => format!("{st:?}").replace("Dnd", "Do Not Disturb"),
            };
            let mut x = av.right() + 12.0;
            if let Some(url) = custom.as_ref().and_then(|c| c.0.clone()).filter(|_| kind == FRIEND) {
                super::text::image(ui.painter(), &url, Rect::from_min_size(pos2(x, row.center().y + 2.0), vec2(16.0, 16.0)));
                x += 20.0;
            }
            super::text::paint(ui.painter(), pos2(x, row.center().y + 10.0), Align2::LEFT_CENTER, &sub, medium(14.0), pal().text_muted, row.width() - 220.0);

            let mut br = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(row.right() - 100.0, row.top()), row.max))
                    .layout(Layout::right_to_left(Align::Center)),
            );
            match kind {
                FRIEND => {
                    if icon_button(&mut br, Icon::MoreVertical, 36.0, "More").clicked() {
                        act = Some((uid, "dm"));
                    }
                    br.add_space(10.0);
                    if icon_button(&mut br, Icon::Bubble, 36.0, "Message").clicked() {
                        act = Some((uid, "dm"));
                    }
                }
                INCOMING => {
                    if icon_button(&mut br, Icon::Close, 36.0, "Ignore").clicked() {
                        act = Some((uid, "remove"));
                    }
                    if icon_button(&mut br, Icon::Check, 36.0, "Accept").clicked() {
                        act = Some((uid, "accept"));
                    }
                }
                _ => {
                    if icon_button(&mut br, Icon::Close, 36.0, if kind == BLOCKED { "Unblock" } else { "Cancel" }).clicked() {
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

/// The friends page's right column: what friends are up to right now.
fn active_now(app: &mut App, ui: &mut Ui) {
    egui::Panel::right("active_now")
        .exact_size(360.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 0 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().vline(r.left() - 16.0, r.y_range().expand(16.0), Stroke::new(1.0, pal().border_subtle));
            ui.label(RichText::new("Active Now").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(16.0);
            let mut busy: Vec<Id> = app
                .store
                .relationships
                .iter()
                .filter(|(u, t)| **t == FRIEND && app.store.activities.get(u).is_some_and(|a| a.iter().any(|x| x.0 != 4)))
                .map(|(u, _)| *u)
                .collect();
            busy.sort_by_key(|u| app.store.name_in(0, *u).to_lowercase());
            if busy.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("It's quiet for now...").font(semi(16.0)).color(pal().header_primary));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("When a friend starts an activity\u{2014}like playing a game or hanging out on voice\u{2014}we'll show it here!")
                            .font(reg(14.0))
                            .color(pal().header_secondary),
                    );
                });
            }
            let mut open = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for u in busy {
                    let Some(user) = app.store.user(u) else { continue };
                    let (card, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
                    let fill = if resp.hovered() { pal().bg_mod_hover } else { egui::Color32::TRANSPARENT };
                    ui.painter().rect(card, CornerRadius::same(8), fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let av = Rect::from_min_size(pos2(card.left() + 16.0, card.center().y - 20.0), vec2(40.0, 40.0));
                    avatar(ui, av, &user.avatar_url());
                    status_dot(ui, av, app.store.status(u), pal().bg_primary);
                    let name = app.store.name_in(0, u);
                    super::text::paint(ui.painter(), pos2(av.right() + 12.0, card.center().y - 10.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, card.width() - 90.0);
                    if let Some((_, plain, bold)) = app.store.activity_line(u) {
                        ui.painter().text(pos2(av.right() + 12.0, card.center().y + 10.0), Align2::LEFT_CENTER, format!("{plain}{bold}"), reg(14.0), pal().header_secondary);
                    }
                    if resp.clicked() {
                        open = Some(u);
                    }
                    ui.add_space(8.0);
                }
            });
            if let Some(u) = open {
                app.open_dm(u);
            }
        });
}
