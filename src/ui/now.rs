//! The friends page's right column: what friends are up to right now, as Discord's cards. A
//! header with the friend and what they're in, then a row for each thing they're doing: the song
//! with its art, the game with its details and timer, a stream, or the voice channel they're in.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::presence::{self, SPOTIFY};
use super::icons::{self, Icon};
use super::{App, avatar, status_dot, text};
use crate::net::Tag;
use crate::state::{Activity, FRIEND, Id};
use crate::theme::*;

enum Body<'a> {
    Playing(&'a Activity),
    Voice(Id, Id),
    Stream(Id, Option<&'a Activity>),
}

fn header_line(a: &Activity) -> String {
    match a.kind {
        2 => format!("Listening to {}", if a.spotify() { "Spotify" } else { &a.name }),
        1 => format!("Streaming {}", a.name),
        3 => format!("Watching {}", a.name),
        5 => format!("Competing in {}", a.name),
        _ if a.start > 0 => format!("{} \u{2013} {}", a.name, presence::short(a.start)),
        _ => a.name.clone(),
    }
}

fn body_lines(a: &Activity) -> Vec<String> {
    let mut v = Vec::new();
    v.extend(a.details.clone());
    v.extend(a.state.clone().map(|s| match a.party {
        Some((n, m)) => format!("{s} ({n} of {m})"),
        None => s.replace(';', ","),
    }));
    if !a.spotify() && a.kind == 0 && a.start > 0 {
        v.push(format!("{} elapsed", presence::clock((chrono::Utc::now().timestamp_millis() - a.start) / 1000)));
    }
    v
}

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::Panel::right("active_now")
        .exact_size((ui.available_width() * 0.3).clamp(360.0, 420.0))
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 0 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().vline(r.left() - 16.0, r.y_range().expand(16.0), hairline(ui.pixels_per_point(), pal().border_subtle));
            ui.label(RichText::new("Active Now").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(16.0);
            let store = &app.store;
            let mut in_voice: crate::state::Map<Id, (Id, Id)> = Default::default();
            let mut live: crate::state::Set<Id> = Default::default();
            for g in store.guilds.values() {
                for (u, c) in &g.voice {
                    if *c != 0 && store.relationships.get(u) == Some(&FRIEND) {
                        in_voice.insert(*u, (g.id, *c));
                        if g.streaming.contains(u) {
                            live.insert(*u);
                        }
                    }
                }
            }
            let mut busy: Vec<Id> = store
                .relationships
                .iter()
                .filter(|(u, t)| **t == FRIEND && (in_voice.contains_key(u) || store.activities.get(u).is_some_and(|a| presence::shown(a).next().is_some())))
                .map(|(u, _)| *u)
                .collect();
            busy.sort_by_cached_key(|u| store.name_in(0, *u).to_lowercase());
            let want: Vec<Id> = {
                let mut w: Vec<Id> = busy.iter().filter_map(|u| store.activities.get(u)).flatten().map(|a| a.app).filter(|a| *a != 0 && !store.app_icons.contains_key(a)).collect();
                w.sort_unstable();
                w.dedup();
                w.truncate(50);
                w
            };
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
            let mut channel = None;
            let mut watch = None;
            let mut ticking = false;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for u in &busy {
                    let Some(user) = store.user(*u) else { continue };
                    let acts: Vec<&Activity> = store.activities.get(u).map(|a| presence::shown(a).collect()).unwrap_or_default();
                    let mut bodies: Vec<Body> = acts.iter().filter(|a| a.kind != 4 && (a.spotify() || !body_lines(a).is_empty())).take(2).map(|a| Body::Playing(a)).collect();
                    if let Some((g, c)) = in_voice.get(u) {
                        bodies.push(Body::Voice(*g, *c));
                        if live.contains(u) {
                            bodies.push(Body::Stream(*c, acts.iter().copied().find(|a| a.kind == 0)));
                        }
                    }
                    let preview = (ui.available_width() - 32.0) * 9.0 / 16.0;
                    let heights: Vec<f32> = bodies
                        .iter()
                        .map(|b| match b {
                            Body::Playing(a) if body_lines(a).len() > 2 => 84.0,
                            Body::Stream(..) => 62.0 + preview + 16.0,
                            _ => 68.0,
                        })
                        .collect();
                    let total = 68.0 + heights.iter().sum::<f32>();
                    let (card, resp) = ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::click());
                    crate::ui::settings::mark(ui, &format!("now_card:{u}"), card);
                    crate::ui::settings::said(ui.ctx(), &format!("now_rows:{u}"), &bodies.len().to_string());
                    let hot = resp.hovered();
                    let p = ui.painter();
                    p.rect_filled(card, CornerRadius::same(12), if hot { Color32::from_rgb(0x12, 0x12, 0x14) } else { Color32::BLACK });
                    p.rect_filled(Rect::from_min_size(card.min, vec2(card.width(), 68.0)), CornerRadius { nw: 12, ne: 12, sw: if bodies.is_empty() { 12 } else { 0 }, se: if bodies.is_empty() { 12 } else { 0 } }, Color32::from_rgba_unmultiplied(151, 151, 159, 20));
                    p.rect_stroke(card, CornerRadius::same(12), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let av = Rect::from_min_size(pos2(card.left() + 16.0, card.top() + 18.0), vec2(32.0, 32.0));
                    avatar(ui, av, &user.avatar_url());
                    status_dot(ui, av, store.status(*u), Color32::from_rgb(0x1b, 0x1b, 0x1e));
                    let name = store.name_in(0, *u);
                    let first = acts.first().copied();
                    let sub = match (first, in_voice.get(u)) {
                        (Some(a), _) => header_line(a),
                        (None, Some(_)) => "In Voice".to_string(),
                        _ => String::new(),
                    };
                    let p = ui.painter();
                    text::paint(p, pos2(av.right() + 12.0, card.top() + 25.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, card.width() - 130.0);
                    text::paint(p, pos2(av.right() + 12.0, card.top() + 45.0), Align2::LEFT_CENTER, &sub, reg(14.0), pal().header_secondary, card.width() - 130.0);
                    let badge = Rect::from_center_size(pos2(card.right() - 32.0, card.top() + 34.0), vec2(32.0, 32.0));
                    match first {
                        Some(a) if a.spotify() => {
                            p.circle_filled(badge.center(), 16.0, SPOTIFY);
                            icons::draw(p, Icon::Soundboard, Rect::from_center_size(badge.center(), vec2(18.0, 18.0)), Color32::BLACK);
                        }
                        Some(a) if a.kind == 1 => {
                            p.circle_filled(badge.center(), 16.0, presence::TWITCH);
                            icons::draw(p, Icon::Screen, Rect::from_center_size(badge.center(), vec2(18.0, 18.0)), Color32::WHITE);
                        }
                        Some(a) => {
                            p.rect_filled(badge, CornerRadius::same(8), Color32::from_white_alpha(15));
                            match store.app_icons.get(&a.app).cloned().flatten() {
                                Some(h) => text::image(p, &format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), a.app), badge),
                                None => icons::draw(p, Icon::Controller, Rect::from_center_size(badge.center(), vec2(20.0, 20.0)), pal().header_secondary),
                            }
                        }
                        None => icons::draw(p, Icon::Speaker, Rect::from_center_size(badge.center(), vec2(20.0, 20.0)), pal().header_secondary),
                    }
                    let mut y = card.top() + 68.0;
                    for (b, h) in bodies.iter().zip(&heights) {
                        let row = Rect::from_min_size(pos2(card.left(), y), vec2(card.width(), *h));
                        let art = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 12.0), vec2(44.0, 44.0));
                        let x = art.right() + 12.0;
                        match b {
                            Body::Playing(a) => {
                                let icon = store.app_icons.get(&a.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=96", crate::api::cdn(), a.app));
                                presence::paint_art(ui, a, &icon, art, 4);
                                let lines = body_lines(a);
                                let mut ly = row.top() + 12.0 + (44.0 - lines.len().min(3) as f32 * 16.0) / 2.0;
                                for (i, l) in lines.iter().take(3).enumerate() {
                                    let (f, c) = if i == 0 { (semi(14.0), pal().header_primary) } else { (reg(12.0), pal().header_secondary) };
                                    text::paint(ui.painter(), pos2(x, ly + 8.0), Align2::LEFT_CENTER, l, f, c, row.right() - x - 16.0);
                                    ly += 16.0;
                                }
                                ticking |= a.start > 0 && !a.spotify();
                            }
                            Body::Stream(c, game) => {
                                let who = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 10.0), vec2(32.0, 32.0));
                                avatar(ui, who, &user.avatar_url());
                                text::paint(ui.painter(), pos2(who.right() + 12.0, row.top() + 18.0), Align2::LEFT_CENTER, &name, semi(14.0), pal().header_primary, row.width() - 130.0);
                                let doing = game.map_or("Sharing their screen", |a| a.name.as_str());
                                text::paint(ui.painter(), pos2(who.right() + 12.0, row.top() + 36.0), Align2::LEFT_CENTER, doing, reg(12.0), pal().header_secondary, row.width() - 130.0);
                                super::call::live_badge(ui, pos2(row.right() - 16.0, row.top() + 18.0));
                                let shot = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 62.0), vec2(row.width() - 32.0, preview));
                                ui.painter().rect_filled(shot, CornerRadius::same(8), Color32::from_rgb(0x1e, 0x1f, 0x22));
                                super::settings::mark(ui, &format!("now_stream:{u}"), shot);
                                super::settings::said(ui.ctx(), &format!("now_stream:{u}"), doing);
                                if ui.rect_contains_pointer(shot) {
                                    let g = ui.painter().layout_no_wrap("Watch Stream".into(), semi(14.0), Color32::WHITE);
                                    let b = Rect::from_center_size(shot.center(), vec2(g.size().x + 32.0, 32.0));
                                    let br = ui.interact(b, ui.id().with(("now_watch", u)), Sense::click());
                                    ui.painter().rect_filled(b, CornerRadius::same(8), if br.hovered() { pal().control_hover } else { pal().control });
                                    ui.painter().galley(pos2(b.left() + 16.0, b.center().y - g.size().y / 2.0), g, Color32::WHITE);
                                    super::settings::mark(ui, &format!("now_watch:{u}"), b);
                                    if br.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if br.clicked() {
                                        watch = Some((*c, *u));
                                    }
                                }
                            }
                            Body::Voice(g, c) => {
                                let rr = ui.interact(row, ui.id().with(("now_voice", u)), Sense::click());
                                if rr.hovered() {
                                    ui.painter().rect_filled(row, 0, Color32::from_white_alpha(8));
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                ui.painter().rect_filled(art, CornerRadius::same(4), Color32::from_white_alpha(15));
                                icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(art.center(), vec2(22.0, 22.0)), pal().header_secondary);
                                let ch = store.channels.get(c).map_or("Voice".to_string(), |c| c.name.clone());
                                let gn = store.guilds.get(g).map_or(String::new(), |g| g.name.clone());
                                text::paint(ui.painter(), pos2(x, row.top() + 25.0), Align2::LEFT_CENTER, &ch, semi(14.0), pal().header_primary, row.right() - x - 16.0);
                                text::paint(ui.painter(), pos2(x, row.top() + 43.0), Align2::LEFT_CENTER, &gn, reg(12.0), pal().header_secondary, row.right() - x - 16.0);
                                if rr.clicked() {
                                    channel = Some((*g, *c));
                                }
                            }
                        }
                        y += h;
                    }
                    if resp.clicked() && channel.is_none() && watch.is_none() {
                        open = Some(*u);
                    }
                    ui.add_space(8.0);
                }
            });
            if ticking {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            if !want.is_empty() {
                for a in &want {
                    app.store.app_icons.insert(*a, None);
                }
                let q: Vec<String> = want.iter().map(|a| format!("application_ids={a}")).collect();
                app.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::AppIcons(want));
            }
            if let Some((c, u)) = watch {
                app.watch_stream(c, u);
            } else if let Some((g, c)) = channel {
                app.ui.guild = Some(g);
                app.open(c);
            } else if let Some(u) = open {
                app.open_dm(u);
            }
        });
}
