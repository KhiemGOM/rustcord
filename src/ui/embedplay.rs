//! Video embeds that play in the chat. A file Discord proxies goes to the inline player; a
//! provider's page (YouTube, Twitch, ...) opens in one web view at a time, only for hosts
//! Discord's own client frames, and is placed over the embed and clipped to the chat's viewport.

use egui::{Context, Rect};

use super::{App, activity::view};
use crate::state::Id;
use crate::webwin::frame::Frame;

const HOSTS: [&str; 9] = [
    "youtube.com",
    "youtube-nocookie.com",
    "twitch.tv",
    "vimeo.com",
    "streamable.com",
    "medal.tv",
    "tiktok.com",
    "soundcloud.com",
    "dailymotion.com",
];

const TWITCH_PARENTS: [&str; 6] = ["discord.co", "discord.com", "discordapp.com", "ptb.discord.com", "canary.discord.com", "localhost"];

pub type Key = (Id, usize);

pub struct Play {
    pub key: Key,
    pub frame: Frame,
}

fn query(u: &mut reqwest::Url, drop: &[&str], add: &[(&str, &str)]) {
    let kept: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| !drop.contains(&&**k)).map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    u.query_pairs_mut().clear().extend_pairs(kept).extend_pairs(add);
}

/// The address to frame for an embed's video url, with the parameters Discord adds so it plays
/// on its own once clicked; None for a host Discord doesn't frame.
pub fn src(url: &str) -> Option<String> {
    let mut u = reqwest::Url::parse(url).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    if u.scheme() == "http" && host == "127.0.0.1" && reqwest::Url::parse(crate::api::cdn()).is_ok_and(|c| c.origin() == u.origin()) {
        return Some(u.into());
    }
    let on = |d: &str| host == d || host.strip_suffix(d).is_some_and(|p| p.ends_with('.'));
    if u.scheme() != "https" || !HOSTS.iter().any(|d| on(d)) {
        return None;
    }
    if on("youtube.com") || on("youtube-nocookie.com") {
        query(&mut u, &["autoplay", "auto_play"], &[("autoplay", "1"), ("auto_play", "1")]);
    } else if on("twitch.tv") {
        let parents: Vec<(&str, &str)> = TWITCH_PARENTS.iter().map(|p| ("parent", *p)).collect();
        query(&mut u, &["parent", "autoplay"], &[&parents[..], &[("autoplay", "true")]].concat());
    } else if on("tiktok.com") {
        query(&mut u, &["utm_source"], &[("utm_source", "discord.gg")]);
    }
    Some(u.into())
}

pub fn start(app: &mut App, ctx: &Context, key: Key, url: &str) {
    let Some(src) = src(url) else { return app.open_link(ctx, url, false) };
    crate::media::stop();
    crate::audioplay::stop();
    let main = crate::winsys::WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    app.embed = Some(Play { key, frame: Frame::open(ctx, main, src) });
}

pub fn px(r: Rect, ppp: f32) -> [i32; 4] {
    [(r.left() * ppp).round() as i32, (r.top() * ppp).round() as i32, (r.width() * ppp).round() as i32, (r.height() * ppp).round() as i32]
}

/// After the frame is drawn: the web view follows its embed, or goes when the chat no longer
/// draws it (another channel, scrolled well away, a different page).
pub fn place(app: &mut App, ctx: &Context) {
    let at = app.embed_at.take();
    let Some(p) = &app.embed else { return };
    let Some((_, full, clip)) = at.filter(|(k, ..)| *k == p.key) else {
        app.embed = None;
        return;
    };
    let covered = view::overlaid(app, ctx);
    let seen = full.intersect(clip);
    let pressed = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !seen.contains(p)));
    let ppp = ctx.pixels_per_point();
    let Some(p) = &app.embed else { return };
    p.frame.place(px(full, ppp), px(clip, ppp), !covered);
    if pressed {
        p.frame.blur();
    }
}

#[cfg(test)]
mod tests {
    use super::src;

    #[test]
    fn only_discords_providers_are_framed() {
        for u in ["http://www.youtube.com/embed/x", "https://evil.com/embed", "https://youtube.com.evil.com/embed/x", "https://notyoutube.com/x", "file:///c:/x", "javascript:1", "https://discord.com/channels/1/2"] {
            assert_eq!(src(u), None, "{u}");
        }
        assert!(src("https://www.youtube.com/embed/abc?rel=0").is_some_and(|u| u.contains("autoplay=1") && u.contains("rel=0")));
        assert!(src("https://player.vimeo.com/video/1").is_some());
    }

    #[test]
    fn twitch_gets_discords_parents_not_its_own() {
        let u = src("https://clips.twitch.tv/embed?clip=A&parent=evil.com&autoplay=false").unwrap();
        assert!(u.contains("clip=A") && u.contains("parent=discord.com") && u.contains("autoplay=true"));
        assert!(!u.contains("evil.com") && !u.contains("autoplay=false"));
    }
}
