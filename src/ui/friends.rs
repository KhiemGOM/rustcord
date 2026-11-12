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
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.add_space(2.0);
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                super::icons::draw(ui.painter(), Icon::Friends, r, pal().channel_default);
                ui.label(RichText::new("Friends").font(medium(16.0)).color(pal().header_primary));
                let (dot, _) = ui.allocate_exact_size(vec2(4.0, 4.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.0, egui::Color32::from_rgba_unmultiplied(151, 151, 159, 61));
                // the tabs sit 16 apart
                ui.add_space(-4.0);
                for (i, t) in TABS.iter().enumerate() {
                    ui.add_space(4.0);
                    let sel = app.friends_tab == i as u8;
                    let g = ui.painter().layout_no_wrap(t.to_string(), medium(16.0), pal().header_primary);
                    let extra = if i == 2 && pending > 0 { 24.0 } else { 0.0 };
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0 + extra, 32.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_selected);
                    } else {
                        super::motion::wash(ui, &resp, r, 8, pal().bg_mod_hover, 0.3);
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
                ui.add_space(4.0);
                let add = app.friends_tab == 4;
                let g = ui.painter().layout_no_wrap("Add Friend".into(), medium(16.0), egui::Color32::WHITE);
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click());
                let fill = if add {
                    pal().blurple.gamma_multiply(0.24)
                } else {
                    super::motion::mix(pal().blurple, egui::Color32::from_rgb(0x44, 0x52, 0xbb), super::motion::hover(ui.ctx(), &resp, 0.3))
                };
                ui.painter().rect_filled(r, CornerRadius::same(8), fill);
                ui.painter().galley(r.center() - g.size() / 2.0, g, if add { pal().link } else { egui::Color32::WHITE });
                if resp.clicked() {
                    app.friends_tab = 4;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::NewDm, 32.0, "New Group DM").clicked() {
                        app.group_pick = Some(super::dmpick::Pick::new(None));
                    }
                });
            });
        });

    if app.friends_tab != 4 && ui.ctx().content_rect().width() > 1200.0 {
        super::now::show(app, ui);
    }
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: if app.friends_tab == 4 { 32 } else { 24 }, right: 20, top: 14, bottom: 0 }).show(ui, |ui| {
            if app.friends_tab == 4 {
                add_friend(app, ui);
            } else {
                list(app, ui);
            }
        });
    });
}

fn add_friend(app: &mut App, ui: &mut Ui) {
    ui.add_space(6.0);
    ui.label(RichText::new("Add Friend").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(5.0);
    ui.label(RichText::new("You can add friends with their Discord username.").font(medium(16.0)).color(pal().text_normal));
    ui.add_space(16.0);
    let mut send = false;
    let seen = egui::Id::new("add_friend_seen");
    let edge = match &app.add_friend_note {
        Some((true, _)) => pal().green,
        Some((false, _)) => pal().critical,
        None => pal().border_subtle,
    };
    input_frame().corner_radius(16).stroke(Stroke::new(1.0, edge)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let out = egui::TextEdit::singleline(&mut app.add_friend)
                .frame(Frame::NONE)
                .font(reg(16.0))
                .vertical_align(Align::Center)
                .min_size(vec2(0.0, 38.0))
                .hint_text("You can add friends with their Discord username.")
                .desired_width(ui.available_width() - 190.0)
                .show(ui);
            app.rects.insert("friends:add_text".into(), Rect::from_min_size(out.galley_pos, out.galley.size()));
            let r = out.response;
            if !ui.data(|d| d.get_temp::<bool>(seen)).unwrap_or(false) {
                r.request_focus();
                ui.data_mut(|d| d.insert_temp(seen, true));
            }
            if r.changed() {
                app.add_friend_note = None;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                send = true;
                r.request_focus();
            }
            let send_button = button(ui, "Send Friend Request", pal().blurple, 180.0);
            app.rects.insert("friends:add_send".into(), send_button.rect);
            if send_button.clicked() {
                send = true;
            }
        });
    });
    if let Some((ok, text)) = &app.add_friend_note {
        ui.add_space(8.0);
        let ink = if *ok { pal().green } else { pal().critical };
        ui.label(ui.painter().layout_job(super::bolded(text, 14.0, ink, ui.available_width())));
    }
    let name = app.add_friend.trim().to_string();
    if send && !name.is_empty() {
        app.rest_in("POST", "/users/@me/relationships".into(), Some(json!({ "username": name, "discriminator": null })), "Add Friend", Tag::AddFriend(name));
        app.add_friend.clear();
    }
    ui.add_space(32.0);
    let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rule.x_range(), rule.center().y, Stroke::new(1.0, pal().border_subtle));
    ui.add_space(24.0);
    ui.label(RichText::new("Other Places to Make Friends").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(5.0);
    ui.label(RichText::new("Don't have a username on hand? Check out our list of public servers that includes everything from gaming to cooking, music, anime and more.").font(medium(16.0)).color(pal().text_normal));
    ui.add_space(16.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(480.0), 64.0), Sense::click());
    app.rects.insert("friends:explore".into(), r);
    let fill = super::motion::mix(pal().bg_secondary, pal().bg_mod_hover, super::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    super::icons::draw(ui.painter(), Icon::Discover, Rect::from_center_size(pos2(r.left() + 32.0, r.center().y), vec2(24.0, 24.0)), pal().header_primary);
    ui.painter().text(pos2(r.left() + 62.0, r.center().y), Align2::LEFT_CENTER, "Explore Discoverable Servers", medium(16.0), pal().header_primary);
    super::icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.open_servers();
    }
}

fn list(app: &mut App, ui: &mut Ui) {
    ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("add_friend_seen")));
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
    people.sort_by_key(|(u, t)| (*t != INCOMING, app.store.name_in(0, *u).to_lowercase()));
    let qid = egui::Id::new("friends_q");
    let mut q: String = ui.data(|d| d.get_temp(qid).unwrap_or_default());
    input_frame().corner_radius(8).inner_margin(Margin::symmetric(12, 7)).show(ui, |ui| {
        ui.set_min_height(24.0);
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
    ui.add_space(25.0);
    let mut section = None;

    let mut act: Option<(Id, &'static str)> = None;
    let mut menu = None;
    crate::theme::auto_scroll(ui, |ui| egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (uid, kind) in people.iter().copied() {
            let key = if tab == 2 { kind } else { FRIEND };
            if section != Some(key) {
                if section.is_some() {
                    ui.add_space(20.0);
                }
                section = Some(key);
                let name = match (tab, kind) {
                    (0, _) => "Online",
                    (1, _) => "All friends",
                    (_, INCOMING) => "Received",
                    _ => "Sent",
                };
                let count = people.iter().filter(|(_, k)| tab != 2 || *k == kind).count();
                ui.label(RichText::new(format!("{name} — {count}")).font(medium(14.0)).color(pal().text_normal));
                ui.add_space(16.0);
            }
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
            app.rects.insert(format!("friend:{uid}"), row);
            ui.painter().hline(row.x_range(), row.top(), Stroke::new(1.0, pal().bg_mod_hover));
            super::motion::wash(ui, &resp, row.shrink2(vec2(-10.0, 1.0)), 8, pal().bg_mod_hover, 0.2);
            let av = Rect::from_min_size(pos2(row.left(), row.center().y - 16.0), vec2(32.0, 32.0));
            let Some(u) = app.store.user(uid) else { continue };
            avatar(ui, av, &u.avatar_url());
            if let Some(d) = u.decoration_url() {
                super::text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(38.0, 38.0)));
            }
            let st = app.store.status(uid);
            if kind == FRIEND {
                status_dot(ui, av, st, crate::ui::motion::mix(pal().bg_primary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            }
            // a nitro name keeps its own font here; a plain one is semibold
            let face = Some(name_font(u.font, 16.0)).filter(|f| *f != medium(16.0)).unwrap_or(semi(16.0));
            let nr = super::text::paint(ui.painter(), pos2(av.right() + 12.0, row.center().y - 9.0), Align2::LEFT_CENTER, &app.store.name_in(0, uid), face, pal().header_primary, row.width() - 220.0);
            super::tag_chip(ui.painter(), u, pos2(nr.right() + 9.0, nr.center().y), 1.0);
            let line = Some(kind).filter(|k| *k == FRIEND && st != Status::Offline).and_then(|_| app.store.activity_line(uid));
            let doing = line
                .as_ref()
                .filter(|l| l.3.is_some())
                .and_then(|_| app.store.activities.get(&uid)?.iter().find(|a| a.kind != 4))
                .map(|a| if a.spotify() { a.state.clone().unwrap_or_default().replace(';', ",") } else { a.name.clone() })
                .unwrap_or_default();
            let at = pos2(av.right() + 12.0, row.center().y + 10.0);
            match line {
                Some(line) => super::members::status_line(ui.painter(), line, at.x, at.y, row.right() - 220.0, pal().text_muted, 14.0, &doing),
                None => {
                    let sub = match kind {
                        INCOMING | OUTGOING => u.username.clone(),
                        BLOCKED => "Blocked".to_string(),
                        _ => format!("{st:?}").replace("Dnd", "Do Not Disturb"),
                    };
                    let ink = if kind == FRIEND { pal().text_muted } else { pal().header_secondary };
                    super::text::paint(ui.painter(), at, Align2::LEFT_CENTER, &sub, medium(14.0), ink, row.width() - 220.0);
                }
            }

            let mut br = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(row.right() - 100.0, row.top()), row.max))
                    .layout(Layout::right_to_left(Align::Center)),
            );
            match kind {
                FRIEND => {
                    let more = icon_button(&mut br, Icon::MoreVertical, 36.0, "More");
                    egui::Popup::menu(&more).align(egui::RectAlign::BOTTOM_START).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (label, what, danger) in [("Start Video Call", "video", false), ("Start Voice Call", "voice", false), ("Remove Friend", "remove_friend", true)] {
                            if super::menu_item(ui, None, label, danger) {
                                act = Some((uid, what));
                            }
                        }
                    });
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
            if resp.clicked() {
                if kind == FRIEND {
                    act = Some((uid, "dm"));
                } else if kind == INCOMING || kind == OUTGOING {
                    act = Some((uid, "profile"));
                }
            }
            if resp.secondary_clicked() && kind == FRIEND {
                menu = Some((uid, resp.interact_pointer_pos().unwrap_or(row.center())));
            }
        }
    }));
    match act {
        Some((u, "profile")) => app.open_full(u, 0, super::profile::Tab::Activity),
        Some((u, "dm")) => app.open_dm(u),
        Some((u, "video")) => app.call_user(u, true),
        Some((u, "voice")) => app.call_user(u, false),
        Some((u, "remove_friend")) => {
            let name = app.store.name_in(0, u);
            let body = format!("Are you sure you want to remove {name} from your friends?");
            app.confirm = Some(super::Confirm::new(format!("Remove '{name}'"), body, "Remove Friend", move |app: &mut App| {
                app.rest_in("DELETE", format!("/users/@me/relationships/{u}"), None, "Friends", Tag::reason())
            }));
        }
        Some((u, "accept")) => {
            app.store.relationships.insert(u, FRIEND);
            app.rest_in("PUT", format!("/users/@me/relationships/{u}"), Some(json!({})), "Friends", Tag::rollback(move |app: &mut App| { app.store.relationships.remove(&u); }));
        }
        Some((u, _)) => {
            let was = app.store.relationships.remove(&u);
            app.rest_in("DELETE", format!("/users/@me/relationships/{u}"), None, "Friends", Tag::rollback(move |app: &mut App| {
                if let Some(t) = was {
                    app.store.relationships.insert(u, t);
                }
            }));
        }
        None => {}
    }
    if let Some((u, at)) = menu {
        app.user_menu = Some((u, 0, None, at));
        app.menu_from = super::usermenu::Origin::Friends;
    }
}
