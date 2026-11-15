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

/// A game with no rich presence, which Discord shows by how many friends are in it.
fn plain(a: &Activity) -> bool {
    a.kind == 0 && !a.spotify() && a.details.is_none() && a.state.is_none()
}

fn body_lines(a: &Activity, players: usize) -> Vec<String> {
    if plain(a) {
        return vec![a.name.clone(), format!("{players} {}", if players == 1 { "Person" } else { "People" })];
    }
    let mut v = Vec::new();
    v.extend(a.details.clone().or_else(|| Some(a.name.clone()).filter(|_| !a.spotify() && a.kind == 0)));
    v.extend(a.state.clone().map(|s| match a.party {
        Some((n, m)) => format!("{s} ({n} of {m})"),
        None => s.replace(';', ","),
    }));
    if !a.spotify() && a.kind == 0 && a.start > 0 {
        v.push(format!("{} elapsed", presence::clock((chrono::Utc::now().timestamp_millis() - a.start) / 1000)));
    }
    v
}

fn lines(ui: &Ui, x: f32, row: Rect, texts: &[&str], right: f32) {
    let top = row.top() + (row.height() - 18.0 - 16.0 * (texts.len() as f32 - 1.0)) / 2.0;
    for (i, t) in texts.iter().enumerate() {
        let (y, f, c) = if i == 0 { (top + 9.0, semi(14.0), pal().text_normal) } else { (top + 18.0 + 16.0 * (i as f32 - 1.0) + 8.0, reg(12.0), pal().text_muted) };
        text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, t, f, c, right - x);
    }
}

fn server_icon(ui: &Ui, store: &crate::state::Store, g: Id, art: Rect) {
    match store.guilds.get(&g) {
        Some(g) => match g.icon_url() {
            Some(u) => crate::gif::paint_still(ui, &u, art, CornerRadius::same(8), false),
            None => {
                ui.painter().rect_filled(art, CornerRadius::same(8), pal().bg_accent);
                ui.painter().text(art.center(), Align2::CENTER_CENTER, g.acronym(), medium(12.0), pal().header_primary);
            }
        },
        None => icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(art.center(), vec2(22.0, 22.0)), pal().header_secondary),
    }
}

/// The people on a row's right: two faces, then how many more.
fn faces(ui: &Ui, store: &crate::state::Store, row: Rect, who: &[Id]) {
    for (i, u) in who.iter().take(2).enumerate() {
        if let Some(u) = store.user(*u) {
            let x = if who.len() == 1 { row.right() - 41.0 } else { row.right() - 93.0 + 22.0 * i as f32 };
            avatar(ui, Rect::from_min_size(pos2(x, row.top() + 22.0), vec2(24.0, 24.0)), &u.avatar_url());
        }
    }
    if who.len() > 2 {
        text::paint(ui.painter(), pos2(row.right() - 40.0, row.top() + 34.0), Align2::LEFT_CENTER, &format!("+{}", who.len() - 2), semi(14.0), pal().text_normal, 30.0);
    }
}

/// A voice row's server icon carries a speaker in its corner.
fn voice_icon(ui: &Ui, store: &crate::state::Store, g: Id, art: Rect) {
    server_icon(ui, store, g, art);
    let c = art.min + vec2(26.0, 26.0);
    ui.painter().circle_filled(c, 8.0, pal().bg_floating);
    icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(c, vec2(10.0, 10.0)), pal().header_secondary);
}

fn card_frame(ui: &Ui, card: Rect, hot: bool, body: bool) {
    let p = ui.painter();
    p.rect_filled(card, CornerRadius::same(16), if hot { Color32::from_rgb(0x12, 0x12, 0x14) } else { Color32::BLACK });
    let tail = if body { 0 } else { 16 };
    p.rect_filled(Rect::from_min_size(card.min, vec2(card.width(), 68.0)), CornerRadius { nw: 16, ne: 16, sw: tail, se: tail }, Color32::from_rgba_unmultiplied(151, 151, 159, 20));
    p.rect_stroke(card, CornerRadius::same(16), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
}

fn names(store: &crate::state::Store, who: &[Id]) -> String {
    let n = |u: &Id| store.name_in(0, *u);
    match who {
        [a, b] => format!("{} and {}", n(a), n(b)),
        [a, b, c] => format!("{}, {}, and {}", n(a), n(b), n(c)),
        [a, b, rest @ ..] => format!("{}, {}, and {} others", n(a), n(b), rest.len()),
        _ => who.first().map(n).unwrap_or_default(),
    }
}

fn group_card(ui: &mut Ui, store: &crate::state::Store, g: Id, c: Id, who: &[Id]) -> bool {
    let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 136.0), Sense::hover());
    card_frame(ui, card, false, true);
    if let Some(first) = who.first().and_then(|u| store.user(*u)) {
        let av = Rect::from_min_size(pos2(card.left() + 16.0, card.top() + 19.0), vec2(32.0, 32.0));
        avatar(ui, av, &first.avatar_url());
        status_dot(ui, av, store.status(first.id), Color32::from_rgb(0x1b, 0x1b, 0x1e));
    }
    let x = card.left() + 61.0;
    let right = card.right() - 16.0;
    let name = names(store, who);
    text::paint(ui.painter(), pos2(x, card.top() + 26.5), Align2::LEFT_CENTER, &name, semi(16.0), pal().text_normal, right - x);
    text::paint(ui.painter(), pos2(x, card.top() + 46.0), Align2::LEFT_CENTER, "In a Voice Channel", reg(14.0), pal().text_muted, right - x);
    let row = Rect::from_min_size(pos2(card.left(), card.top() + 68.0), vec2(card.width(), 68.0));
    let hit = ui.interact(row, ui.id().with(("now_group", c)), Sense::click());
    if hit.hovered() {
        ui.painter().rect_filled(row, CornerRadius { nw: 0, ne: 0, sw: 16, se: 16 }, Color32::from_white_alpha(8));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    voice_icon(ui, store, g, Rect::from_min_size(pos2(card.left() + 17.0, row.top() + 17.0), vec2(32.0, 32.0)));
    let ch = store.channels.get(&c).map_or("Voice".to_string(), |c| c.name.clone());
    let gn = store.guilds.get(&g).map_or(String::new(), |g| g.name.clone());
    lines(ui, card.left() + 61.0, row, &[&gn, &ch], right - 120.0);
    faces(ui, store, row, who);
    hit.clicked()
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
            ui.add_space(10.5);
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
            let doing = |u: &Id| live.contains(u) || store.activities.get(u).is_some_and(|a| presence::shown(a).next().is_some());
            let inside = |(g, c): (Id, Id)| -> Vec<Id> { store.guilds.get(&g).map(|g| g.voice.iter().filter(|(_, x)| **x == c).map(|(u, _)| *u).collect()).unwrap_or_default() };
            let by_name = |v: &mut Vec<Id>| v.sort_by_cached_key(|u| store.name_in(0, *u).to_lowercase());
            let mut by_channel: crate::state::Map<(Id, Id), Vec<Id>> = Default::default();
            for (u, gc) in &in_voice {
                if !doing(u) {
                    by_channel.entry(*gc).or_default().push(*u);
                }
            }
            let grouped: crate::state::Set<Id> = by_channel.values().flatten().copied().collect();
            let mut groups: Vec<((Id, Id), Vec<Id>)> = by_channel
                .into_iter()
                .map(|(gc, mut friends)| {
                    by_name(&mut friends);
                    let mut rest: Vec<Id> = inside(gc).into_iter().filter(|u| !friends.contains(u) && *u != store.me).collect();
                    by_name(&mut rest);
                    friends.extend(rest);
                    (gc, friends)
                })
                .collect();
            groups.sort_by_cached_key(|(_, v)| store.name_in(0, v[0]).to_lowercase());
            let mut busy: Vec<Id> = store
                .relationships
                .iter()
                .filter(|(u, t)| **t == FRIEND && ((in_voice.contains_key(u) && !grouped.contains(u)) || live.contains(u) || store.activities.get(u).is_some_and(|a| presence::shown(a).next().is_some())))
                .map(|(u, _)| *u)
                .collect();
            busy.sort_by_cached_key(|u| store.name_in(0, *u).to_lowercase());
            let mut players: crate::state::Map<String, Vec<Id>> = Default::default();
            for u in &busy {
                for a in store.activities.get(u).map(|a| presence::shown(a).filter(|a| a.kind == 0).collect::<Vec<_>>()).unwrap_or_default() {
                    players.entry(if a.app != 0 { a.app.to_string() } else { a.name.clone() }).or_default().push(*u);
                }
            }
            let playing = |a: &Activity| players.get(&if a.app != 0 { a.app.to_string() } else { a.name.clone() }).cloned().unwrap_or_default();
            let want: Vec<Id> = {
                let mut w: Vec<Id> = busy.iter().filter_map(|u| store.activities.get(u)).flatten().map(|a| a.app).filter(|a| *a != 0 && !store.app_icons.contains_key(a)).collect();
                w.sort_unstable();
                w.dedup();
                w.truncate(50);
                w
            };
            if busy.is_empty() && groups.is_empty() {
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
                for ((g, c), who) in &groups {
                    if group_card(ui, store, *g, *c, who) {
                        channel = Some((*g, *c));
                    }
                    ui.add_space(12.0);
                }
                for u in &busy {
                    let Some(user) = store.user(*u) else { continue };
                    let acts: Vec<&Activity> = store.activities.get(u).map(|a| presence::shown(a).collect()).unwrap_or_default();
                    let mut bodies: Vec<Body> = acts.iter().filter(|a| a.kind != 4 && (a.spotify() || !body_lines(a, 1).is_empty())).take(2).map(|a| Body::Playing(a)).collect();
                    if let Some((g, c)) = in_voice.get(u) {
                        if !grouped.contains(u) {
                            bodies.push(Body::Voice(*g, *c));
                        }
                        if live.contains(u) {
                            bodies.push(Body::Stream(*c, acts.iter().copied().find(|a| a.kind == 0)));
                        }
                    }
                    let preview = (ui.available_width() - 32.0) * 9.0 / 16.0;
                    let heights: Vec<f32> = bodies
                        .iter()
                        .map(|b| match b {
                            Body::Playing(a) if body_lines(a, 1).len() > 2 => 84.0,
                            Body::Stream(..) => 62.0 + preview + 16.0,
                            _ => 68.0,
                        })
                        .collect();
                    let total = 68.0 + heights.iter().sum::<f32>();
                    let (card, resp) = ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::click());
                    crate::ui::settings::mark(ui, &format!("now_card:{u}"), card);
                    crate::ui::settings::said(ui.ctx(), &format!("now_rows:{u}"), &bodies.len().to_string());
                    card_frame(ui, card, resp.hovered(), !bodies.is_empty());
                    let av = Rect::from_min_size(pos2(card.left() + 16.0, card.top() + 19.0), vec2(32.0, 32.0));
                    avatar(ui, av, &user.avatar_url());
                    status_dot(ui, av, store.status(*u), Color32::from_rgb(0x1b, 0x1b, 0x1e));
                    let name = store.name_in(0, *u);
                    let first = acts.first().copied();
                    let sub = match (first, in_voice.get(u)) {
                        (Some(a), _) => header_line(a),
                        (None, Some(_)) => "In a Voice Channel".to_string(),
                        _ => String::new(),
                    };
                    let p = ui.painter();
                    text::paint(p, pos2(av.right() + 12.0, card.top() + 26.5), Align2::LEFT_CENTER, &name, semi(16.0), pal().text_normal, card.width() - 130.0);
                    text::paint(p, pos2(av.right() + 12.0, card.top() + 46.0), Align2::LEFT_CENTER, &sub, reg(14.0), pal().text_muted, card.width() - 130.0);
                    let badge = Rect::from_center_size(pos2(card.right() - 29.0, card.top() + 35.0), vec2(24.0, 24.0));
                    match first {
                        Some(a) if a.spotify() => {
                            p.circle_filled(badge.center(), 12.0, SPOTIFY);
                            icons::draw(p, Icon::Soundboard, Rect::from_center_size(badge.center(), vec2(14.0, 14.0)), Color32::BLACK);
                        }
                        Some(a) if a.kind == 1 => {
                            p.circle_filled(badge.center(), 12.0, presence::TWITCH);
                            icons::draw(p, Icon::Screen, Rect::from_center_size(badge.center(), vec2(14.0, 14.0)), Color32::WHITE);
                        }
                        Some(a) => {
                            p.rect_filled(badge, CornerRadius::same(4), Color32::from_white_alpha(15));
                            match store.app_icons.get(&a.app).cloned().flatten() {
                                Some(h) => text::image(p, &format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), a.app), badge),
                                None => icons::draw(p, Icon::Controller, Rect::from_center_size(badge.center(), vec2(16.0, 16.0)), pal().header_secondary),
                            }
                        }
                        None => {}
                    }
                    let mut y = card.top() + 68.0;
                    for (b, h) in bodies.iter().zip(&heights) {
                        let row = Rect::from_min_size(pos2(card.left(), y), vec2(card.width(), *h));
                        let art = Rect::from_min_size(pos2(row.left() + 17.0, row.top() + 17.0), vec2(32.0, 32.0));
                        let x = art.right() + 12.0;
                        match b {
                            Body::Playing(a) => {
                                let icon = store.app_icons.get(&a.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=96", crate::api::cdn(), a.app));
                                presence::paint_art(ui, a, &icon, art, 4);
                                let who = playing(a);
                                let texts = body_lines(a, who.len().max(1));
                                lines(ui, x, row, &texts.iter().take(3).map(String::as_str).collect::<Vec<_>>(), row.right() - if plain(a) { 120.0 } else { 16.0 });
                                if plain(a) {
                                    faces(ui, store, row, &who);
                                }
                                ticking |= a.start > 0 && !a.spotify() && !plain(a);
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
                                voice_icon(ui, store, *g, art);
                                let ch = store.channels.get(c).map_or("Voice".to_string(), |c| c.name.clone());
                                let gn = store.guilds.get(g).map_or(String::new(), |g| g.name.clone());
                                lines(ui, x, row, &[&gn, &ch], row.right() - 120.0);
                                faces(ui, store, row, &inside((*g, *c)));
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
                    ui.add_space(12.0);
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
