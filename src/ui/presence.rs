//! Discord's activity cards: what someone is playing, streaming or listening to, as the profile
//! popout, the full profile and Active Now draw them.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::text;
use crate::state::Activity;
use crate::theme::*;

pub const SPOTIFY: Color32 = Color32::from_rgb(0x1d, 0xb9, 0x54);
pub const TWITCH: Color32 = Color32::from_rgb(0x91, 0x46, 0xff);

pub enum Act {
    Play(String),
    Connect,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn clock(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// "2h", "16m": how long ago a game was started, for Active Now's subtitle.
pub fn short(ms: i64) -> String {
    let m = ((now_ms() - ms) / 60000).max(0);
    if m >= 60 { format!("{}h", m / 60) } else { format!("{m}m") }
}

pub fn headline(a: &Activity) -> String {
    match a.kind {
        1 => format!("Streaming {}", a.name),
        2 => format!("Listening to {}", a.name),
        3 => format!("Watching {}", a.name),
        5 => format!("Competing in {}", a.name),
        _ => format!("Playing {}", a.name),
    }
}

/// The activities worth a card: everything but the custom status.
pub fn shown(list: &[Activity]) -> impl Iterator<Item = &Activity> {
    list.iter().filter(|a| a.kind != 4)
}

fn art(a: &Activity, key: &Option<String>, icon: &Option<String>) -> Option<String> {
    key.as_deref().and_then(|k| a.art(k)).or_else(|| icon.clone())
}

fn lines(a: &Activity) -> Vec<String> {
    let mut v = Vec::new();
    if a.spotify() {
        v.extend(a.details.clone());
        v.extend(a.state.clone().map(|s| s.replace(';', ",")));
        return v;
    }
    v.extend(a.details.clone());
    v.extend(a.state.clone().map(|s| match a.party {
        Some((n, m)) => format!("{s} ({n} of {m})"),
        None => s,
    }));
    if a.start > 0 && a.end == 0 {
        v.push(format!("{} elapsed", clock((now_ms() - a.start) / 1000)));
    } else if a.end > 0 {
        v.push(format!("{} left", clock((a.end - now_ms()) / 1000)));
    }
    v
}

/// The artwork square with its small image over the corner.
pub fn paint_art(ui: &Ui, a: &Activity, icon: &Option<String>, r: Rect, radius: u8) {
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(radius), Color32::from_white_alpha(15));
    match art(a, &a.large, icon) {
        Some(url) => {
            crate::gif::paint_still(ui, &url, r, CornerRadius::same(radius), false);
        }
        None => icons::draw(p, Icon::Controller, Rect::from_center_size(r.center(), r.size() * 0.5), pal().header_secondary),
    }
    if a.large.is_some()
        && let Some(url) = a.small.as_deref().and_then(|k| a.art(k))
    {
        let s = Rect::from_center_size(r.right_bottom() - vec2(5.0, 5.0), vec2(r.width() * 0.4, r.width() * 0.4));
        p.circle_filled(s.center(), s.width() / 2.0 + 2.0, pal().bg_secondary);
        crate::gif::paint_still(ui, &url, s, CornerRadius::same((s.width() / 2.0) as u8), false);
    }
}

/// The card on a profile: "Playing X" over the artwork and its lines, with a progress bar and a
/// Play on Spotify button for a song. `w` is the card's width.
pub fn card(ui: &mut Ui, a: &Activity, w: f32, icon: &Option<String>, connected: bool) -> Option<Act> {
    let text_lines = lines(a);
    let song = a.spotify();
    let body = if song { 60.0 } else { (text_lines.len() as f32 * 16.0 + 18.0).max(60.0) };
    let h = 32.0 + body + 8.0 + if song { 44.0 } else { 0.0 };
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let mut act = None;
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, 41));
    let head = if song { "Listening to Spotify".to_string() } else { headline(a) };
    let g = text::paint(p, pos2(r.left() + 8.0, r.top() + 16.0), Align2::LEFT_CENTER, &head, medium(12.0), pal().header_primary, w - 60.0);
    if song {
        p.circle_filled(pos2(g.right() + 12.0, r.top() + 16.0), 6.0, SPOTIFY);
    }
    let image = Rect::from_min_size(pos2(r.left() + 8.0, r.top() + 32.0), vec2(60.0, 60.0));
    paint_art(ui, a, icon, image, if song { 4 } else { 8 });
    let p = ui.painter();
    let x = image.right() + 8.0;
    let tw = r.right() - x - 8.0;
    if song {
        let row = |y: f32, s: &str, f: egui::FontId, c: Color32| {
            text::paint(p, pos2(x, r.top() + y), Align2::LEFT_CENTER, s, f, c, tw);
        };
        row(43.0, text_lines.first().map_or("", String::as_str), semi(14.0), pal().header_primary);
        row(61.0, text_lines.get(1).map_or("", String::as_str), reg(12.0), pal().header_secondary);
        let (done, total) = ((now_ms() - a.start).clamp(0, (a.end - a.start).max(0)), (a.end - a.start).max(1));
        row(81.0, &clock(done / 1000), reg(12.0), pal().header_secondary);
        let t = p.layout_no_wrap(clock(total / 1000), reg(12.0), pal().header_secondary);
        p.galley(pos2(r.right() - 8.0 - t.size().x, r.top() + 81.0 - t.size().y / 2.0), t, pal().header_secondary);
        let bar = Rect::from_min_max(pos2(x + 38.0, r.top() + 79.0), pos2(r.right() - 8.0 - 38.0, r.top() + 83.0));
        p.rect_filled(bar, CornerRadius::same(2), Color32::from_white_alpha(46));
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * done as f32 / total as f32, 4.0)), CornerRadius::same(2), Color32::WHITE);
        let b = Rect::from_min_size(pos2(r.left() + 8.0, r.bottom() - 40.0), vec2(w - 16.0, 32.0));
        let resp = ui.interact(b, ui.id().with(("play", &a.name)), Sense::click());
        let p = ui.painter();
        p.rect_filled(b, CornerRadius::same(8), Color32::from_white_alpha(if resp.hovered() { 31 } else { 20 }));
        let label = if connected { "Play on Spotify" } else { "Connect Spotify" };
        let lw = text::paint(p, pos2(b.center().x + 12.0, b.center().y), Align2::CENTER_CENTER, label, medium(14.0), pal().header_primary, b.width() - 60.0);
        p.circle_filled(pos2(lw.left() - 16.0, b.center().y), 8.0, SPOTIFY);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            act = Some(match (&a.sync_id, connected) {
                (Some(id), true) => Act::Play(format!("https://open.spotify.com/track/{id}")),
                _ => Act::Connect,
            });
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    } else {
        let n = text_lines.len().max(1) as f32;
        let top = r.top() + 32.0 + (60.0 - n * 16.0) / 2.0;
        for (i, s) in text_lines.iter().enumerate() {
            let (f, c) = if i == 0 { (semi(14.0), pal().header_primary) } else { (reg(12.0), pal().header_secondary) };
            text::paint(p, pos2(x, top + 8.0 + i as f32 * 16.0), Align2::LEFT_CENTER, s, f, c, tw);
        }
        if a.start > 0 || a.end > 0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
    act
}

/// An entry of Recent activity on a full profile: art, title, artist and how long ago.
pub fn recent(ui: &mut Ui, w: f32, e: &super::profile::Recent) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 84.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(12), Color32::from_rgba_unmultiplied(151, 151, 159, 31));
    let image = Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 12.0), vec2(60.0, 60.0));
    p.rect_filled(image, CornerRadius::same(6), Color32::from_white_alpha(15));
    if let Some(url) = &e.image {
        crate::gif::paint_still(ui, url, image, CornerRadius::same(6), false);
    }
    let p = ui.painter();
    let x = image.right() + 12.0;
    text::paint(p, pos2(x, r.top() + 23.0), Align2::LEFT_CENTER, &e.title, semi(14.0), pal().header_primary, w - 120.0);
    text::paint(p, pos2(x, r.top() + 40.0), Align2::LEFT_CENTER, &e.subtitle, reg(12.0), pal().header_primary, w - 120.0);
    let mut tx = x;
    if e.song {
        icons::draw(p, Icon::Soundboard, Rect::from_center_size(pos2(x + 6.0, r.top() + 59.0), vec2(12.0, 12.0)), pal().header_secondary);
        tx += 16.0;
    }
    p.text(pos2(tx, r.top() + 59.0), Align2::LEFT_CENTER, &e.ago, reg(12.0), pal().header_secondary);
}
