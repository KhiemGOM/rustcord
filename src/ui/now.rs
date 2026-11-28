//! The friends page's right column: what friends are up to right now, as Discord's cards. A
//! header with the friend and what they're in, then a row for each thing they're doing: the song
//! with its art, the game with its details and timer, a stream, or the voice channel they're in.

use std::cmp::Reverse;

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::presence::{self, SPOTIFY};
use super::{App, avatar, status_dot, text};
use crate::net::Tag;
use crate::state::{Activity, FRIEND, Id, Map, Set, Status, Store};
use crate::theme::*;

const PASSIVE_APP_IDS: [Id; 1] = [438122941302046720]; // Wallpaper Engine
const PASSIVE_NAMES: [&str; 21] = [
    "wallpaper engine",
    "lively wallpaper",
    "rainmeter",
    "translucenttb",
    "sharex",
    "nvidia broadcast",
    "razer synapse",
    "logitech g hub",
    "corsair icue",
    "icue",
    "steelseries gg",
    "steelseries sonar",
    "armoury crate",
    "msi afterburner",
    "rivatuner statistics server",
    "obs studio",
    "streamlabs desktop",
    "streamlabs obs",
    "autohotkey",
    "powertoys",
    "displayfusion",
];

enum Body<'a> {
    Playing(&'a Activity),
    ExternalStream(&'a Activity),
    Voice(Id, Id),
    Stream(Id, Option<&'a Activity>),
}

#[derive(Debug, PartialEq, Eq)]
enum Card {
    Friend(Id),
    Group(Id, Id, Vec<Id>),
}

fn card_name_font(font: u8, size: f32) -> egui::FontId {
    let styled = name_font(font, size);
    if styled == medium(size) { semi(size) } else { styled }
}

fn is_passive_activity(a: &Activity) -> bool {
    if a.kind != 0 {
        return false;
    }
    if PASSIVE_APP_IDS.contains(&a.app) {
        return true;
    }
    let name = a.name.trim().to_ascii_lowercase();
    PASSIVE_NAMES.contains(&name.as_str())
}

fn is_active_now_activity(a: &Activity) -> bool {
    !matches!(a.kind, 4 | 6) && !a.name.trim().is_empty() && !is_passive_activity(a)
}

fn same_activity(a: &Activity, b: &Activity) -> bool {
    if a.spotify() && b.spotify() {
        return true;
    }
    if a.kind != b.kind {
        return false;
    }
    (a.app != 0 && a.app == b.app) || a.name.trim().eq_ignore_ascii_case(b.name.trim())
}

fn activity_rank(a: &Activity) -> (bool, bool, bool, i64) {
    (!plain(a), a.large.is_some(), a.details.is_some() || a.state.is_some(), a.started_at())
}

/// Active Now activities for one user: passive utilities and statuses filtered out, duplicates
/// collapsed to their richest/newest entry, and live streams lifted to the front.
fn active_activities(list: &[Activity]) -> Vec<&Activity> {
    let mut out: Vec<&Activity> = Vec::new();
    for a in list.iter().filter(|a| is_active_now_activity(a)) {
        if let Some(pos) = out.iter().position(|e| same_activity(e, a)) {
            if activity_rank(a) > activity_rank(out[pos]) {
                out[pos] = a;
            }
        } else {
            out.push(a);
        }
    }
    out.sort_by_key(|a| u8::from(a.kind != 1));
    out
}

fn header_line_at(a: &Activity, now_ms: i64) -> String {
    match a.kind {
        2 => format!("Listening to {}", if a.spotify() { "Spotify" } else { &a.name }),
        1 => match presence::stream_platform(a) {
            Some(platform) => format!("Streaming on {platform}"),
            None => format!("Streaming {}", a.name),
        },
        3 => format!("Watching {}", a.name),
        5 => format!("Competing in {}", a.name),
        _ => {
            let since = a.started_at();
            if since > 0 { format!("{} \u{2013} {}", a.name, presence::short_from(now_ms, since)) } else { a.name.clone() }
        }
    }
}

fn header_line(a: &Activity) -> String {
    header_line_at(a, chrono::Utc::now().timestamp_millis())
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

fn dm_affinity(store: &Store) -> Map<Id, (Id, usize)> {
    let mut map: Map<Id, (Id, usize)> = Default::default();
    let total = store.dms.len();
    for (idx, cid) in store.dms.iter().enumerate() {
        let Some(c) = store.channels.get(cid).filter(|c| c.kind == 1) else { continue };
        let Some(&other) = c.recipients.iter().find(|u| **u != store.me) else { continue };
        let score = (c.last_msg, total - idx);
        map.entry(other).and_modify(|e| *e = (*e).max(score)).or_insert(score);
    }
    map
}

fn feed_affinity(store: &Store) -> Map<Id, usize> {
    let mut map: Map<Id, usize> = Default::default();
    let total = store.feed.len();
    for (idx, e) in store.feed.iter().enumerate() {
        map.entry(e.author).or_insert(total - idx);
    }
    map
}

fn friend_activities(store: &Store, u: Id) -> Vec<&Activity> {
    if store.presence.get(&u) == Some(&Status::Offline) {
        return Vec::new();
    }
    store.activities.get(&u).map(|a| active_activities(a)).unwrap_or_default()
}

fn order_cards(store: &Store, in_voice: &Map<Id, (Id, Id)>, live: &Set<Id>) -> Vec<Card> {
    let dm_aff = dm_affinity(store);
    let feed_aff = feed_affinity(store);
    let inside = |(g, c): (Id, Id)| -> Vec<Id> { store.guilds.get(&g).map(|g| g.voice.iter().filter(|(_, x)| **x == c).map(|(u, _)| *u).collect()).unwrap_or_default() };
    let sort_people = |v: &mut Vec<Id>| {
        v.sort_by_cached_key(|u| (Reverse(dm_aff.get(u).copied().unwrap_or((0, 0))), store.name_in(0, *u).to_lowercase(), *u));
    };

    let doing = |u: Id| live.contains(&u) || !friend_activities(store, u).is_empty();
    let mut by_channel: Map<(Id, Id), Vec<Id>> = Default::default();
    for (&u, &gc) in in_voice {
        if !doing(u) {
            by_channel.entry(gc).or_default().push(u);
        }
    }
    let grouped: Set<Id> = by_channel.values().flatten().copied().collect();
    let mut cards: Vec<Card> = by_channel
        .into_iter()
        .map(|((g, c), mut friends)| {
            sort_people(&mut friends);
            let mut rest: Vec<Id> = inside((g, c)).into_iter().filter(|u| !friends.contains(u) && *u != store.me).collect();
            sort_people(&mut rest);
            friends.extend(rest);
            Card::Group(g, c, friends)
        })
        .collect();

    for (&u, &t) in &store.relationships {
        if t == FRIEND && store.users.contains_key(&u) && ((in_voice.contains_key(&u) && !grouped.contains(&u)) || doing(u)) {
            cards.push(Card::Friend(u));
        }
    }

    cards.sort_by_cached_key(|card| match card {
        Card::Friend(u) => {
            let acts = friend_activities(store, *u);
            let priority: u8 = if live.contains(u) {
                4
            } else if acts.iter().any(|a| a.kind == 1) {
                3
            } else if in_voice.contains_key(u) {
                2
            } else {
                1
            };
            let voice_ms = in_voice.get(u).and_then(|(_, c)| store.voice_start.get(c)).copied().unwrap_or(0).saturating_mul(1000);
            let started_ms = acts.iter().map(|a| a.started_at()).max().unwrap_or(0).max(voice_ms);
            let (dm_msg, dm_pos) = dm_aff.get(u).copied().unwrap_or((0, 0));
            let feed_pos = feed_aff.get(u).copied().unwrap_or(0);
            (Reverse(priority), Reverse(started_ms), Reverse(dm_msg), Reverse(dm_pos), Reverse(feed_pos), Reverse(1usize), store.name_in(0, *u).to_lowercase(), *u)
        }
        Card::Group(_, c, who) => {
            let voice_ms = store.voice_start.get(c).copied().unwrap_or(0).saturating_mul(1000);
            let (dm_msg, dm_pos) = who.iter().filter_map(|u| dm_aff.get(u).copied()).max().unwrap_or((0, 0));
            let feed_pos = who.iter().filter_map(|u| feed_aff.get(u).copied()).max().unwrap_or(0);
            let first_name = who.first().map(|u| store.name_in(0, *u).to_lowercase()).unwrap_or_default();
            (Reverse(2u8), Reverse(voice_ms), Reverse(dm_msg), Reverse(dm_pos), Reverse(feed_pos), Reverse(who.len()), first_name, *c)
        }
    });
    cards
}

fn want_app_icons(store: &Store, cards: &[Card]) -> Vec<Id> {
    let mut w: Vec<Id> = cards
        .iter()
        .filter_map(|c| match c {
            Card::Friend(u) => Some(friend_activities(store, *u)),
            Card::Group(..) => None,
        })
        .flatten()
        .map(|a| a.app)
        .filter(|a| *a != 0 && !store.app_icons.contains_key(a))
        .collect();
    w.sort_unstable();
    w.dedup();
    w
}

/// Crisp vector Twitch glitch glyph scaled into `rect`.
fn twitch_mark(p: &egui::Painter, rect: Rect) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - vec2(12.0, 12.0) * s;
    let pt = |x: f32, y: f32| o + vec2(x, y) * s;
    let bx = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pt(x0, y0), pt(x1, y1));
    p.rect_filled(bx(3.0, 2.0, 21.0, 18.0), CornerRadius::same((2.0 * s).round().max(1.0) as u8), presence::TWITCH);
    p.add(Shape::convex_polygon(vec![pt(6.0, 17.5), pt(11.5, 17.5), pt(6.0, 22.0)], presence::TWITCH, Stroke::NONE));
    p.rect_filled(bx(6.0, 4.5, 19.0, 14.5), CornerRadius::same((1.0 * s).round().max(1.0) as u8), Color32::WHITE);
    p.rect_filled(bx(10.5, 6.8, 12.2, 11.5), CornerRadius::ZERO, presence::TWITCH);
    p.rect_filled(bx(14.8, 6.8, 16.5, 11.5), CornerRadius::ZERO, presence::TWITCH);
}

fn paint_app_badge(p: &egui::Painter, icons: &Map<Id, Option<String>>, a: &Activity, badge: Rect) {
    let drawn = presence::app_icon_url(icons, a.app, 64).is_some_and(|url| text::image_rounded(p, &url, badge, CornerRadius::same(4)));
    if !drawn {
        p.rect_filled(badge, CornerRadius::same(4), Color32::from_white_alpha(15));
        icons::draw(p, Icon::Controller, Rect::from_center_size(badge.center(), vec2(16.0, 16.0)), pal().header_secondary);
    }
}

fn lines(ui: &Ui, x: f32, row: Rect, texts: &[&str], right: f32) {
    if texts.is_empty() {
        return;
    }
    let top = row.top() + (row.height() - 18.0 - 16.0 * (texts.len() as f32 - 1.0)) / 2.0;
    for (i, t) in texts.iter().enumerate() {
        let (y, f, c) = if i == 0 { (top + 9.0, semi(14.0), pal().text_normal) } else { (top + 18.0 + 16.0 * (i as f32 - 1.0) + 8.0, reg(12.0), pal().text_muted) };
        text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, t, f, c, right - x);
    }
}

fn server_icon(ui: &Ui, store: &Store, g: Id, art: Rect) {
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
fn faces(ui: &Ui, store: &Store, row: Rect, who: &[Id]) {
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
fn voice_icon(ui: &Ui, store: &Store, g: Id, art: Rect) {
    server_icon(ui, store, g, art);
    let c = art.min + vec2(26.0, 26.0);
    ui.painter().circle_filled(c, 8.0, pal().bg_floating);
    icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(c, vec2(10.0, 10.0)), pal().header_secondary);
}

fn card_colors(p: &Palette, hot: bool) -> (Color32, Color32, Color32) {
    let base = if hot { p.bg_secondary.blend(p.control) } else { p.bg_secondary };
    let head_wash = Color32::from_rgba_unmultiplied(151, 151, 159, 20);
    let ring = base.blend(head_wash);
    (base, head_wash, ring)
}

fn card_frame(ui: &Ui, card: Rect, hot: bool, body: bool) {
    let p = ui.painter();
    let (base, head_wash, _) = card_colors(pal(), hot);
    p.rect_filled(card, CornerRadius::same(16), base);
    let tail = if body { 0 } else { 16 };
    p.rect_filled(Rect::from_min_size(card.min, vec2(card.width(), 68.0)), CornerRadius { nw: 16, ne: 16, sw: tail, se: tail }, head_wash);
    p.rect_stroke(card, CornerRadius::same(16), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
}

fn names(store: &Store, who: &[Id]) -> String {
    let n = |u: &Id| store.name_in(0, *u);
    match who {
        [a, b] => format!("{} and {}", n(a), n(b)),
        [a, b, c] => format!("{}, {}, and {}", n(a), n(b), n(c)),
        [a, b, rest @ ..] => format!("{}, {}, and {} others", n(a), n(b), rest.len()),
        _ => who.first().map(n).unwrap_or_default(),
    }
}

fn group_card(ui: &mut Ui, store: &Store, g: Id, c: Id, who: &[Id]) -> bool {
    let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), 136.0), Sense::hover());
    card_frame(ui, card, false, true);
    let first_user = who.first().and_then(|u| store.user(*u));
    if let Some(first) = first_user {
        let av = Rect::from_min_size(pos2(card.left() + 16.0, card.top() + 19.0), vec2(32.0, 32.0));
        avatar(ui, av, &first.avatar_url());
        let (_, _, ring) = card_colors(pal(), false);
        status_dot(ui, av, store.status(first.id), ring);
    }
    let x = card.left() + 61.0;
    let right = card.right() - 16.0;
    let name = names(store, who);
    let head_font = if who.len() == 1 { card_name_font(first_user.map_or(0, |u| u.font), 16.0) } else { semi(16.0) };
    text::paint(ui.painter(), pos2(x, card.top() + 26.5), Align2::LEFT_CENTER, &name, head_font, pal().text_normal, right - x);
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
            let mut in_voice: Map<Id, (Id, Id)> = Default::default();
            let mut live: Set<Id> = Default::default();
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
            let inside = |(g, c): (Id, Id)| -> Vec<Id> { store.guilds.get(&g).map(|g| g.voice.iter().filter(|(_, x)| **x == c).map(|(u, _)| *u).collect()).unwrap_or_default() };
            let cards = order_cards(store, &in_voice, &live);
            let mut players: Map<String, Vec<Id>> = Default::default();
            for card in &cards {
                if let Card::Friend(u) = card {
                    for a in friend_activities(store, *u).into_iter().filter(|a| a.kind == 0) {
                        players.entry(if a.app != 0 { a.app.to_string() } else { a.name.clone() }).or_default().push(*u);
                    }
                }
            }
            let playing = |a: &Activity| players.get(&if a.app != 0 { a.app.to_string() } else { a.name.clone() }).cloned().unwrap_or_default();
            let want = want_app_icons(store, &cards);
            if cards.is_empty() {
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
            let mut open_url: Option<String> = None;
            let mut ticking = false;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for card_item in &cards {
                    match card_item {
                        Card::Group(g, c, who) => {
                            if group_card(ui, store, *g, *c, who) {
                                channel = Some((*g, *c));
                            }
                            ui.add_space(12.0);
                        }
                        Card::Friend(u) => {
                            let Some(user) = store.user(*u) else { continue };
                            let acts = friend_activities(store, *u);
                            let mut bodies: Vec<Body> = acts
                                .iter()
                                .copied()
                                .filter(|a| a.kind == 1 || a.spotify() || !body_lines(a, 1).is_empty())
                                .take(2)
                                .map(|a| if a.kind == 1 { Body::ExternalStream(a) } else { Body::Playing(a) })
                                .collect();
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
                                    Body::Playing(a) if body_lines(a, 1).len() > 2 => 84.0,
                                    Body::ExternalStream(a) if presence::stream_preview_url(a).is_some() || presence::stream_url(a).is_some() => 62.0 + preview + 16.0,
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
                            let (_, _, ring) = card_colors(pal(), resp.hovered());
                            status_dot(ui, av, store.status(*u), ring);
                            let name = store.name_in(0, *u);
                            let first = acts.first().copied();
                            let sub = match (first, in_voice.get(u)) {
                                (Some(a), _) => header_line(a),
                                (None, Some(_)) => "In a Voice Channel".to_string(),
                                _ => String::new(),
                            };
                            let p = ui.painter();
                            text::paint(p, pos2(av.right() + 12.0, card.top() + 26.5), Align2::LEFT_CENTER, &name, card_name_font(user.font, 16.0), pal().text_normal, card.width() - 130.0);
                            text::paint(p, pos2(av.right() + 12.0, card.top() + 46.0), Align2::LEFT_CENTER, &sub, reg(14.0), pal().text_muted, card.width() - 130.0);
                            let badge = Rect::from_center_size(pos2(card.right() - 29.0, card.top() + 35.0), vec2(24.0, 24.0));
                            match first {
                                Some(a) if a.spotify() => {
                                    p.circle_filled(badge.center(), 12.0, SPOTIFY);
                                    icons::draw(p, Icon::Soundboard, Rect::from_center_size(badge.center(), vec2(14.0, 14.0)), Color32::BLACK);
                                }
                                Some(a) if a.kind == 1 => {
                                    if presence::stream_platform(a) == Some("Twitch") {
                                        twitch_mark(p, badge);
                                    } else {
                                        p.circle_filled(badge.center(), 12.0, presence::TWITCH);
                                        icons::draw(p, Icon::Screen, Rect::from_center_size(badge.center(), vec2(14.0, 14.0)), Color32::WHITE);
                                    }
                                }
                                Some(a) => paint_app_badge(p, &store.app_icons, a, badge),
                                None => {}
                            }
                            let mut clicked_body = false;
                            let mut y = card.top() + 68.0;
                            for (b, h) in bodies.iter().zip(&heights) {
                                let row = Rect::from_min_size(pos2(card.left(), y), vec2(card.width(), *h));
                                let art = Rect::from_min_size(pos2(row.left() + 17.0, row.top() + 17.0), vec2(32.0, 32.0));
                                let x = art.right() + 12.0;
                                match b {
                                    Body::Playing(a) => {
                                        let icon = presence::app_icon_url(&store.app_icons, a.app, 96);
                                        presence::paint_art(ui, a, &icon, art, 4);
                                        let who = playing(a);
                                        let texts = body_lines(a, who.len().max(1));
                                        lines(ui, x, row, &texts.iter().take(3).map(String::as_str).collect::<Vec<_>>(), row.right() - if plain(a) { 120.0 } else { 16.0 });
                                        if plain(a) {
                                            faces(ui, store, row, &who);
                                        }
                                        ticking |= a.start > 0 && !a.spotify() && !plain(a);
                                    }
                                    Body::ExternalStream(a) => {
                                        let top_row = Rect::from_min_size(row.min, vec2(row.width(), 62.0));
                                        let icon_rect = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 15.0), vec2(32.0, 32.0));
                                        if presence::stream_platform(a) == Some("Twitch") {
                                            twitch_mark(ui.painter(), icon_rect);
                                        } else {
                                            ui.painter().rect_filled(icon_rect, CornerRadius::same(6), presence::TWITCH);
                                            icons::draw(ui.painter(), Icon::Screen, Rect::from_center_size(icon_rect.center(), vec2(18.0, 18.0)), Color32::WHITE);
                                        }
                                        let texts = presence::stream_lines(a);
                                        lines(ui, icon_rect.right() + 12.0, top_row, &texts.iter().take(2).map(String::as_str).collect::<Vec<_>>(), row.right() - 16.0);
                                        let stream_link = presence::stream_url(a);
                                        let preview_url = presence::stream_preview_url(a);
                                        if preview_url.is_some() || stream_link.is_some() {
                                            let shot = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 62.0), vec2(row.width() - 32.0, preview));
                                            ui.painter().rect_filled(shot, CornerRadius::same(8), pal().bg_tertiary);
                                            if let Some(img) = &preview_url {
                                                text::image_cover(ui.painter(), img, shot, CornerRadius::same(8));
                                            }
                                            let hit = ui.interact(shot, ui.id().with(("now_ext_stream", u, &a.name)), Sense::click());
                                            super::settings::mark(ui, &format!("now_ext_stream:{u}"), shot);
                                            if hit.hovered() {
                                                ui.painter().rect_filled(shot, CornerRadius::same(8), Color32::from_black_alpha(56));
                                                if stream_link.is_some() {
                                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                                }
                                            }
                                            let play_r = if hit.hovered() { 21.0 } else { 19.0 };
                                            ui.painter().circle_filled(shot.center(), play_r, Color32::from_black_alpha(if hit.hovered() { 195 } else { 150 }));
                                            icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(shot.center() + vec2(1.5, 0.0), vec2(16.0, 16.0)), Color32::WHITE);
                                            ui.painter().rect_stroke(shot, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                            if hit.clicked() {
                                                clicked_body = true;
                                                if let Some(link) = stream_link {
                                                    open_url = Some(link);
                                                }
                                            }
                                        }
                                    }
                                    Body::Stream(c, game) => {
                                        let who = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 10.0), vec2(32.0, 32.0));
                                        avatar(ui, who, &user.avatar_url());
                                        text::paint(ui.painter(), pos2(who.right() + 12.0, row.top() + 18.0), Align2::LEFT_CENTER, &name, card_name_font(user.font, 14.0), pal().header_primary, row.width() - 130.0);
                                        let doing = game.map_or("Sharing their screen", |a| a.name.as_str());
                                        text::paint(ui.painter(), pos2(who.right() + 12.0, row.top() + 36.0), Align2::LEFT_CENTER, doing, reg(12.0), pal().header_secondary, row.width() - 130.0);
                                        super::call::live_badge(ui, pos2(row.right() - 16.0, row.top() + 18.0));
                                        let shot = Rect::from_min_size(pos2(row.left() + 16.0, row.top() + 62.0), vec2(row.width() - 32.0, preview));
                                        ui.painter().rect_filled(shot, CornerRadius::same(8), pal().bg_tertiary);
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
                                                clicked_body = true;
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
                                            clicked_body = true;
                                            channel = Some((*g, *c));
                                        }
                                    }
                                }
                                y += h;
                            }
                            if resp.clicked() && !clicked_body && channel.is_none() && watch.is_none() && open_url.is_none() {
                                open = Some(*u);
                            }
                            ui.add_space(12.0);
                        }
                    }
                }
            });
            if ticking {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            for chunk in want.chunks(20) {
                for a in chunk {
                    app.store.app_icons.entry(*a).or_insert(None);
                }
                let q: Vec<String> = chunk.iter().map(|a| format!("application_ids={a}")).collect();
                app.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::AppIcons(chunk.to_vec()));
            }
            if let Some(url) = open_url {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            } else if let Some((c, u)) = watch {
                app.watch_stream(c, u);
            } else if let Some((g, c)) = channel {
                app.ui.guild = Some(g);
                app.open(c);
            } else if let Some(u) = open {
                app.open_dm(u);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Channel, Guild, User};

    fn add_friend(store: &mut Store, id: Id, name: &str) {
        store.users.insert(id, User { id, username: name.into(), ..Default::default() });
        store.relationships.insert(id, FRIEND);
        store.presence.insert(id, Status::Online);
    }

    #[test]
    fn header_line_falls_back_to_created_at_and_formats_twitch() {
        let now = 1_700_000_000_000i64;
        let modrinth = Activity {
            kind: 0,
            name: "Modrinth".into(),
            state: Some("Idling...".into()),
            created_at: now - 10 * 60_000,
            ..Default::default()
        };
        assert_eq!(header_line_at(&modrinth, now), "Modrinth \u{2013} 10m");

        let both = Activity {
            kind: 0,
            name: "KovaaK's".into(),
            start: now - 7 * 60_000,
            created_at: now - 20 * 60_000,
            ..Default::default()
        };
        assert_eq!(header_line_at(&both, now), "KovaaK's \u{2013} 7m");

        let no_time = Activity { kind: 0, name: "Modrinth".into(), ..Default::default() };
        assert_eq!(header_line_at(&no_time, now), "Modrinth");

        let twitch = Activity {
            kind: 1,
            name: "Grand Theft Auto VII".into(),
            large: Some("twitch:ex1fly".into()),
            ..Default::default()
        };
        assert_eq!(header_line_at(&twitch, now), "Streaming on Twitch");
    }

    #[test]
    fn deduplicates_duplicate_activity_rows_while_keeping_distinct_ones() {
        let list = vec![
            Activity { kind: 3, name: "Crunchyroll".into(), app: 777, details: Some("Haikyu!!".into()), state: Some("Ep 1".into()), created_at: 100, ..Default::default() },
            Activity { kind: 3, name: "Crunchyroll".into(), app: 777, details: Some("Haikyu!!".into()), state: Some("Ep 2".into()), created_at: 200, ..Default::default() },
            Activity { kind: 2, name: "Spotify".into(), sync_id: Some("trk1".into()), details: Some("Song".into()), ..Default::default() },
            Activity { kind: 0, name: "Rust Simulator".into(), app: 900, details: Some("Lab".into()), ..Default::default() },
            Activity { kind: 0, name: "Rust Simulator".into(), app: 900, ..Default::default() },
        ];
        let deduped = active_activities(&list);
        assert_eq!(deduped.len(), 3);
        let cr = deduped.iter().find(|a| a.app == 777).unwrap();
        assert_eq!(cr.state.as_deref(), Some("Ep 2"));
        let rs = deduped.iter().find(|a| a.app == 900).unwrap();
        assert_eq!(rs.details.as_deref(), Some("Lab"));
        assert!(deduped.iter().any(|a| a.spotify()));
    }

    #[test]
    fn passive_background_utilities_are_filtered_for_empty_state() {
        let mut store = Store::default();
        add_friend(&mut store, 1, "outrivals");
        store.activities.insert(
            1,
            vec![
                Activity { kind: 0, name: "Wallpaper Engine".into(), app: 438122941302046720, ..Default::default() },
                Activity { kind: 4, name: "Custom Status".into(), state: Some("hi".into()), ..Default::default() },
                Activity { kind: 6, name: "Hang Status".into(), ..Default::default() },
            ],
        );
        let cards = order_cards(&store, &Default::default(), &Default::default());
        assert!(cards.is_empty(), "only passive/status entries should show empty state");

        // Passive app in voice still forms a voice group card rather than a game card.
        let mut g = Guild { id: 10, name: "Guild".into(), ..Default::default() };
        g.voice.insert(1, 20);
        store.guilds.insert(10, g);
        let mut in_voice: Map<Id, (Id, Id)> = Default::default();
        in_voice.insert(1, (10, 20));
        let cards = order_cards(&store, &in_voice, &Default::default());
        assert_eq!(cards, vec![Card::Group(10, 20, vec![1])]);
    }

    #[test]
    fn orders_by_stream_voice_recency_and_dm_affinity() {
        let mut store = Store::default();
        store.me = 999;
        add_friend(&mut store, 1, "bloody");
        add_friend(&mut store, 2, "Cryptic");
        add_friend(&mut store, 3, "Hitz");
        add_friend(&mut store, 4, "Ex1fly");
        add_friend(&mut store, 5, "Zara");
        add_friend(&mut store, 6, "Adam");

        let now = 1_700_000_000_000i64;
        // Games with different recencies: Hitz (7m) > bloody (2h) > Cryptic (1d)
        store.activities.insert(1, vec![Activity { kind: 0, name: "Minecraft".into(), start: now - 2 * 3_600_000, ..Default::default() }]);
        store.activities.insert(2, vec![Activity { kind: 0, name: "Roblox".into(), start: now - 24 * 3_600_000, ..Default::default() }]);
        store.activities.insert(3, vec![Activity { kind: 0, name: "KovaaK's".into(), start: now - 7 * 60_000, ..Default::default() }]);
        // Twitch stream should outrank plain games regardless of name
        store.activities.insert(4, vec![Activity { kind: 1, name: "Twitch".into(), large: Some("twitch:ex1fly".into()), ..Default::default() }]);
        // Same start time: Zara has a recent DM so she beats Adam even though Adam < Zara alphabetically
        store.activities.insert(5, vec![Activity { kind: 0, name: "Valorant".into(), start: now - 30 * 60_000, ..Default::default() }]);
        store.activities.insert(6, vec![Activity { kind: 0, name: "Valorant".into(), start: now - 30 * 60_000, ..Default::default() }]);

        store.channels.insert(500, Channel { id: 500, kind: 1, last_msg: 9000, recipients: Box::new([5]), ..Default::default() });
        store.channels.insert(501, Channel { id: 501, kind: 1, last_msg: 1000, recipients: Box::new([6]), ..Default::default() });
        store.dms = vec![500, 501];

        let mut in_voice: Map<Id, (Id, Id)> = Default::default();
        let mut live: Set<Id> = Default::default();
        // Put Cryptic (id 2) in voice + Go Live -> highest priority!
        in_voice.insert(2, (10, 20));
        live.insert(2);

        let cards = order_cards(&store, &in_voice, &live);
        assert_eq!(
            cards,
            vec![
                Card::Friend(2), // Go Live in voice (priority 4)
                Card::Friend(4), // Twitch stream (priority 3)
                Card::Friend(3), // Hitz - 7m (priority 1, newest)
                Card::Friend(5), // Zara - 30m (recent DM beats Adam)
                Card::Friend(6), // Adam - 30m
                Card::Friend(1), // bloody - 2h
            ]
        );
    }

    #[test]
    fn card_name_font_uses_nitro_font_and_falls_back_to_semibold() {
        assert_eq!(card_name_font(0, 16.0), semi(16.0));
        assert_eq!(card_name_font(255, 16.0), semi(16.0));
        assert_eq!(card_name_font(7, 16.0), name_font(7, 16.0));
        assert_ne!(card_name_font(7, 16.0), semi(16.0));
        assert_eq!(card_name_font(7, 14.0), name_font(7, 14.0));
    }

    #[test]
    fn want_app_icons_deduplicates_and_skips_known_or_zero_ids() {
        let mut store = Store::default();
        add_friend(&mut store, 1, "a");
        add_friend(&mut store, 2, "b");
        store.app_icons.insert(100, Some("known".into()));
        store.activities.insert(
            1,
            vec![
                Activity { kind: 0, name: "G1".into(), app: 100, ..Default::default() },
                Activity { kind: 0, name: "G2".into(), app: 200, ..Default::default() },
                Activity { kind: 0, name: "G0".into(), app: 0, ..Default::default() },
            ],
        );
        store.activities.insert(2, vec![Activity { kind: 0, name: "G2".into(), app: 200, ..Default::default() }, Activity { kind: 0, name: "G3".into(), app: 300, ..Default::default() }]);
        let cards = order_cards(&store, &Default::default(), &Default::default());
        assert_eq!(want_app_icons(&store, &cards), vec![200, 300]);
    }

    #[test]
    fn body_lines_omits_clock_for_created_at_only_and_pads_hours() {
        let now = chrono::Utc::now().timestamp_millis();
        let modrinth = Activity {
            kind: 0,
            name: "Modrinth".into(),
            state: Some("Idling...".into()),
            created_at: now - 10 * 60_000,
            ..Default::default()
        };
        assert_eq!(body_lines(&modrinth, 1), vec!["Modrinth", "Idling..."]);

        let medal = Activity {
            kind: 0,
            name: "Minecraft with Medal".into(),
            details: Some("Clipping Minecraft".into()),
            state: Some("with Medal".into()),
            start: now - (2 * 3600 + 34 * 60 + 21) * 1000,
            ..Default::default()
        };
        let lines = body_lines(&medal, 1);
        assert_eq!(lines.len(), 3);
        assert!(lines[2].starts_with("02:34:") && lines[2].ends_with(" elapsed"), "got {}", lines[2]);
    }

    #[test]
    fn app_badge_draws_controller_fallback_until_icon_texture_is_ready() {
        use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
        use std::sync::{Arc, Mutex};

        struct StagedBytes(Mutex<Option<Result<Arc<[u8]>, String>>>);
        impl BytesLoader for StagedBytes {
            fn id(&self) -> &str {
                "test::StagedBytes"
            }
            fn load(&self, _ctx: &egui::Context, _uri: &str) -> BytesLoadResult {
                match self.0.lock().unwrap().as_ref() {
                    None => Ok(BytesPoll::Pending { size: None }),
                    Some(Ok(b)) => Ok(BytesPoll::Ready { size: None, bytes: Bytes::Shared(b.clone()), mime: Some("image/png".into()) }),
                    Some(Err(e)) => Err(LoadError::Loading(e.clone())),
                }
            }
            fn forget(&self, _uri: &str) {}
            fn forget_all(&self) {}
            fn byte_size(&self) -> usize {
                0
            }
        }

        let ctx = egui::Context::default();
        let bytes = Arc::new(StagedBytes(Mutex::new(None)));
        ctx.add_bytes_loader(bytes.clone());
        ctx.add_texture_loader(Arc::new(crate::imgcache::Textures::default()));
        let mut icons: Map<Id, Option<String>> = Default::default();
        icons.insert(55, Some("badgehash".into()));
        icons.insert(56, Some("brokenhash".into()));
        let act = Activity { kind: 0, name: "Medal".into(), app: 55, ..Default::default() };
        let badge = Rect::from_min_size(pos2(0.0, 0.0), vec2(24.0, 24.0));

        // While Pending, fallback draws the translucent badge rect PLUS the controller icon shapes.
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_app_badge(ui.painter(), &icons, &act, badge);
        });
        let pending_prims = out.shapes.len();
        out.textures_delta.clear();
        assert!(pending_prims > 1, "expected fallback badge + controller icon while pending, got {pending_prims} shapes");

        // Once bytes arrive, the loaded app icon renders as a single textured rounded rect.
        let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([0x22, 0xc5, 0x5e, 0xff]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        *bytes.0.lock().unwrap() = Some(Ok(Arc::from(png.into_inner())));

        let start = std::time::Instant::now();
        let mut textured = false;
        let mut ready_prims = 0;
        while !textured && start.elapsed() < std::time::Duration::from_secs(3) {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                paint_app_badge(ui.painter(), &icons, &act, badge);
            });
            textured = out.shapes.iter().any(|cs| matches!(&cs.shape, Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default()));
            ready_prims = out.shapes.len();
            out.textures_delta.clear();
            if !textured {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert!(textured, "loaded app icon should paint its rounded texture");
        assert_eq!(ready_prims, 1, "loaded app icon should not paint a grey fallback box behind itself");

        // On failed CDN fetch, fallback draws the badge + controller icon instead of a blank square.
        *bytes.0.lock().unwrap() = Some(Err("404 Not Found".into()));
        let broken = Activity { kind: 0, name: "Broken".into(), app: 56, ..Default::default() };
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_app_badge(ui.painter(), &icons, &broken, badge);
        });
        let failed_prims = out.shapes.len();
        out.textures_delta.clear();
        assert!(failed_prims > 1, "expected fallback badge + controller icon on fetch error, got {failed_prims} shapes");
    }

    #[test]
    fn card_colors_follow_theme_palette() {
        for (i, name) in [(0, "Ash"), (1, "Dark"), (2, "Onyx"), (3, "Light")] {
            let p = crate::theme::palette(i);
            let (idle, _, ring) = card_colors(&p, false);
            assert_eq!(idle, p.bg_secondary, "{name} idle card should use bg_secondary");
            assert_ne!(ring, Color32::from_rgb(0x1b, 0x1b, 0x1e), "{name} ring should not be hardcoded");
            if name != "Onyx" {
                assert_ne!(idle, Color32::BLACK, "{name} card must not be hardcoded black");
            }
        }
    }
}
