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
    if s >= 3600 { format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// "1d", "2h", "16m": how long ago a game was started, for Active Now's subtitle.
pub fn short_from(now: i64, ms: i64) -> String {
    let m = ((now - ms) / 60000).max(0);
    if m >= 1440 {
        format!("{}d", m / 1440)
    } else if m >= 60 {
        format!("{}h", m / 60)
    } else {
        format!("{m}m")
    }
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

/// The activities worth a card: everything but custom and hang statuses.
pub fn shown(list: &[Activity]) -> impl Iterator<Item = &Activity> {
    list.iter().filter(|a| !matches!(a.kind, 4 | 6))
}

/// Discord CDN URL for an application's icon, rejecting missing ids and empty hashes.
pub fn app_icon_url(icons: &crate::state::Map<crate::state::Id, Option<String>>, app: crate::state::Id, size: u32) -> Option<String> {
    (app != 0)
        .then(|| icons.get(&app)?.as_deref().filter(|h| !h.is_empty()).map(|h| format!("{}/app-icons/{app}/{h}.png?size={size}", crate::api::cdn())))
        .flatten()
}

fn url_host(url: &str) -> Option<String> {
    let clean = crate::api::web_url(url)?;
    reqwest::Url::parse(&clean).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase))
}

fn is_twitch_host(host: &str) -> bool {
    host == "twitch.tv" || host.ends_with(".twitch.tv")
}

fn is_youtube_host(host: &str) -> bool {
    host == "youtube.com" || host.ends_with(".youtube.com") || host == "youtu.be"
}

fn twitch_channel_from_url(url: &str) -> Option<String> {
    let clean = crate::api::web_url(url)?;
    let parsed = reqwest::Url::parse(&clean).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !is_twitch_host(&host) {
        return None;
    }
    let seg = parsed.path_segments()?.find(|s| !s.is_empty())?;
    let low = seg.to_ascii_lowercase();
    if !crate::state::safe_stream_id(&low) || matches!(low.as_str(), "videos" | "directory" | "clip" | "settings" | "downloads" | "jobs" | "p") {
        return None;
    }
    Some(low)
}

pub fn stream_platform(a: &Activity) -> Option<&'static str> {
    if let Some(url) = a.url.as_deref() {
        let host = url_host(url)?;
        return if is_twitch_host(&host) {
            Some("Twitch")
        } else if is_youtube_host(&host) {
            Some("YouTube")
        } else {
            None
        };
    }
    if a.large.as_deref().and_then(|k| k.strip_prefix("twitch:")).is_some_and(crate::state::safe_stream_id)
        || a.name.eq_ignore_ascii_case("twitch")
    {
        Some("Twitch")
    } else if a.large.as_deref().and_then(|k| k.strip_prefix("youtube:")).is_some_and(crate::state::safe_stream_id)
        || a.name.eq_ignore_ascii_case("youtube")
    {
        Some("YouTube")
    } else {
        None
    }
}

pub fn stream_url(a: &Activity) -> Option<String> {
    if let Some(url) = a.url.as_deref() {
        let clean = crate::api::web_url(url)?;
        let host = reqwest::Url::parse(&clean).ok()?.host_str()?.to_ascii_lowercase();
        return (is_twitch_host(&host) || is_youtube_host(&host)).then_some(clean);
    }
    if let Some(ch) = a.large.as_deref().and_then(|k| k.strip_prefix("twitch:")).filter(|s| crate::state::safe_stream_id(s)) {
        return Some(format!("https://www.twitch.tv/{ch}"));
    }
    if let Some(v) = a.large.as_deref().and_then(|k| k.strip_prefix("youtube:")).filter(|s| crate::state::safe_stream_id(s)) {
        return Some(format!("https://www.youtube.com/watch?v={v}"));
    }
    None
}

pub fn stream_preview_url(a: &Activity) -> Option<String> {
    if let Some(u) = a.large.as_deref().and_then(|k| a.art(k)) {
        return Some(u);
    }
    let ch = a.url.as_deref().and_then(twitch_channel_from_url)?;
    Some(format!("https://static-cdn.jtvnw.net/previews-ttv/live_user_{ch}-440x248.jpg"))
}

pub fn stream_lines(a: &Activity) -> Vec<String> {
    let platform = stream_platform(a);
    let non_platform_name = (!a.name.is_empty() && platform.is_none_or(|p| !a.name.eq_ignore_ascii_case(p))).then(|| a.name.clone());
    let mut v = Vec::new();
    if let Some(d) = &a.details {
        v.push(d.clone());
        if let Some(s) = &a.state {
            v.push(s.clone());
        } else if let Some(n) = non_platform_name {
            v.push(n);
        }
    } else if let Some(s) = &a.state {
        v.push(s.clone());
        if let Some(n) = non_platform_name {
            v.push(n);
        }
    } else if !a.name.is_empty() {
        v.push(a.name.clone());
    }
    v
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
    let rad = CornerRadius::same(radius);
    let drawn = art(a, &a.large, icon).is_some_and(|url| text::image_rounded(p, &url, r, rad));
    if !drawn {
        p.rect_filled(r, rad, Color32::from_white_alpha(15));
        icons::draw(p, Icon::Controller, Rect::from_center_size(r.center(), r.size() * 0.5), pal().header_secondary);
    }
    if a.large.is_some()
        && let Some(url) = a.small.as_deref().and_then(|k| a.art(k))
    {
        let s = Rect::from_center_size(r.right_bottom() - vec2(5.0, 5.0), vec2(r.width() * 0.4, r.width() * 0.4));
        p.circle_filled(s.center(), s.width() / 2.0 + 2.0, pal().bg_secondary);
        text::image_rounded(p, &url, s, CornerRadius::same((s.width() / 2.0) as u8));
    }
}

/// The card on a profile: "Playing X" over the artwork and its lines, with a progress bar and a
/// Play on Spotify and Listen Along buttons for a song: side by side with words on a full
/// profile (`wide`), an icon beside Play on Spotify in the popout. `w` is the card's width.
pub fn card(ui: &mut Ui, a: &Activity, w: f32, icon: &Option<String>, connected: bool, name: &str, wide: bool) -> Option<Act> {
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
        let (play, along) = if wide {
            let half = (b.width() - 8.0) / 2.0;
            (Rect::from_min_size(pos2(b.left() + half + 8.0, b.top()), vec2(half, 32.0)), Rect::from_min_size(b.min, vec2(half, 32.0)))
        } else {
            (Rect::from_min_size(b.min, vec2(b.width() - 40.0, 32.0)), Rect::from_min_size(pos2(b.right() - 32.0, b.top()), vec2(32.0, 32.0)))
        };
        let url = a.sync_id.as_ref().filter(|_| connected).map(|id| format!("https://open.spotify.com/track/{}", crate::api::esc(id)));
        let resp = ui.interact(play, ui.id().with(("play", &a.name)), Sense::click());
        let listen = ui.interact(along, ui.id().with(("listen", &a.name)), Sense::click());
        super::settings::mark(ui, "spotify:play", play);
        super::settings::mark(ui, "spotify:listen", along);
        let p = ui.painter();
        p.rect_filled(play, CornerRadius::same(8), Color32::from_white_alpha(if resp.hovered() { 31 } else { 20 }));
        let label = if connected { "Play on Spotify" } else { "Connect Spotify" };
        let lw = text::paint(p, pos2(play.center().x + 12.0, play.center().y), Align2::CENTER_CENTER, label, medium(14.0), pal().header_primary, play.width() - 60.0);
        p.circle_filled(pos2(lw.left() - 16.0, play.center().y), 8.0, SPOTIFY);
        p.rect_filled(along, CornerRadius::same(8), Color32::from_white_alpha(if listen.hovered() { 31 } else { 20 }));
        if wide {
            text::paint(p, along.center(), Align2::CENTER_CENTER, "Listen Along", medium(14.0), pal().header_primary, along.width() - 16.0);
        } else {
            icons::draw(p, Icon::ListenAlong, Rect::from_center_size(along.center(), vec2(16.0, 16.0)), pal().header_primary);
        }
        if resp.hovered() || listen.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            act = Some(url.clone().map_or(Act::Connect, Act::Play));
        }
        if listen.hovered() {
            super::settings::said(ui.ctx(), "listen_tip", &format!("Listen along with {name}"));
        }
        if listen.on_hover_text(format!("Listen along with {name}")).clicked() {
            act = Some(url.map_or(Act::Connect, Act::Play));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_zero_pads_hours_minutes_and_seconds() {
        assert_eq!(clock(-5), "00:00");
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(59), "00:59");
        assert_eq!(clock(60), "01:00");
        assert_eq!(clock(3599), "59:59");
        assert_eq!(clock(3600), "01:00:00");
        assert_eq!(clock(2 * 3600 + 34 * 60 + 21), "02:34:21");
        assert_eq!(clock(10 * 3600 + 5 * 60 + 9), "10:05:09");
    }

    #[test]
    fn short_formats_minutes_hours_and_days() {
        let now = 1_700_000_000_000i64;
        assert_eq!(short_from(now, now + 5000), "0m");
        assert_eq!(short_from(now, now - 10 * 60_000), "10m");
        assert_eq!(short_from(now, now - 2 * 3_600_000), "2h");
        assert_eq!(short_from(now, now - 26 * 3_600_000), "1d");
    }

    #[test]
    fn app_icon_url_rejects_missing_and_empty_hashes() {
        let mut icons: crate::state::Map<crate::state::Id, Option<String>> = Default::default();
        icons.insert(10, None);
        icons.insert(11, Some(String::new()));
        icons.insert(12, Some("abc123".into()));
        assert_eq!(app_icon_url(&icons, 0, 64), None);
        assert_eq!(app_icon_url(&icons, 9, 64), None);
        assert_eq!(app_icon_url(&icons, 10, 64), None);
        assert_eq!(app_icon_url(&icons, 11, 64), None);
        assert!(app_icon_url(&icons, 12, 64).unwrap().ends_with("/app-icons/12/abc123.png?size=64"));
    }

    #[test]
    fn stream_helpers_resolve_twitch_and_external_assets() {
        let twitch = Activity {
            kind: 1,
            name: "Twitch".into(),
            details: Some("Grand Theft Auto VII".into()),
            state: Some("Playing In Single Player".into()),
            large: Some("twitch:ex1fly".into()),
            url: Some("https://www.twitch.tv/ex1fly".into()),
            ..Default::default()
        };
        assert_eq!(stream_platform(&twitch), Some("Twitch"));
        assert_eq!(stream_url(&twitch).as_deref(), Some("https://www.twitch.tv/ex1fly"));
        assert_eq!(stream_preview_url(&twitch).as_deref(), Some("https://static-cdn.jtvnw.net/previews-ttv/live_user_ex1fly-440x248.jpg"));
        assert_eq!(stream_lines(&twitch), vec!["Grand Theft Auto VII", "Playing In Single Player"]);

        let url_only = Activity {
            kind: 1,
            name: "Grand Theft Auto VII".into(),
            details: Some("Live now".into()),
            url: Some("https://twitch.tv/Ex1fly?ref=discord".into()),
            ..Default::default()
        };
        assert_eq!(stream_platform(&url_only), Some("Twitch"));
        assert_eq!(stream_preview_url(&url_only).as_deref(), Some("https://static-cdn.jtvnw.net/previews-ttv/live_user_ex1fly-440x248.jpg"));
        assert_eq!(stream_lines(&url_only), vec!["Live now", "Grand Theft Auto VII"]);

        let external = Activity {
            kind: 1,
            name: "Twitch".into(),
            large: Some("mp:external/abc/https/cdn.example/thumb.jpg".into()),
            url: Some("https://www.twitch.tv/ex1fly".into()),
            ..Default::default()
        };
        assert!(stream_preview_url(&external).unwrap().ends_with("/external/abc/https/cdn.example/thumb.jpg"));

        let large_only = Activity {
            kind: 1,
            name: "Twitch".into(),
            large: Some("twitch:shroud".into()),
            ..Default::default()
        };
        assert_eq!(stream_url(&large_only).as_deref(), Some("https://www.twitch.tv/shroud"));
    }

    #[test]
    fn stream_helpers_reject_untrusted_hosts_and_unsafe_ids() {
        let spoof = Activity {
            kind: 1,
            name: "Twitch".into(),
            large: Some("twitch:shroud".into()),
            url: Some("https://evil.example/login".into()),
            ..Default::default()
        };
        assert_eq!(stream_platform(&spoof), None);
        assert_eq!(stream_url(&spoof), None);

        let backslash = Activity {
            kind: 1,
            name: "Twitch".into(),
            url: Some("https://evil.example\\@twitch.tv/shroud".into()),
            ..Default::default()
        };
        assert_eq!(stream_platform(&backslash), None);
        assert_eq!(stream_url(&backslash), None);

        let bad_twitch = Activity {
            kind: 1,
            name: "Twitch".into(),
            large: Some("twitch:../evil?x=1".into()),
            ..Default::default()
        };
        assert_eq!(stream_url(&bad_twitch), None);
        assert_eq!(bad_twitch.art("twitch:../evil?x=1"), None);

        let bad_yt = Activity {
            kind: 1,
            name: "YouTube".into(),
            large: Some("youtube:abc&list=evil".into()),
            ..Default::default()
        };
        assert_eq!(stream_url(&bad_yt), None);
        assert_eq!(bad_yt.art("youtube:abc&list=evil"), None);
    }
}
