//! The slash commands Vencord's plugins add: FriendInvites, petpet and SpotifyShareCommands. They
//! run locally; what they have to say comes back as a toast.

use serde_json::json;

use super::App;
use super::outgoing::{self, Done};
use crate::plugins;
use crate::state::{Channel, Id};

pub fn commands() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut v = Vec::new();
    if plugins::on("FriendInvites") {
        v.push(("create friend invite", "", "Generates a friend invite link."));
        v.push(("view friend invites", "", "View a list of all generated friend invites."));
        v.push(("revoke friend invites", "", "Revokes all generated friend invites."));
    }
    if plugins::on("petpet") {
        v.push(("petpet", "image delay resolution", "Create a petpet gif. You can only specify one of the image options"));
    }
    if plugins::on("SpotifyShareCommands") {
        v.push(("track", "message", "Share your current Spotify track in chat"));
        v.push(("album", "message", "Share your current Spotify album in chat"));
        v.push(("artist", "message", "Share your current Spotify artist in chat"));
    }
    v
}

/// Runs `text` (a command without its slash) if it is one of ours.
pub fn run(app: &mut App, chan: &Channel, text: &str) -> bool {
    let Some((name, _, _)) = commands().into_iter().filter(|(n, ..)| text == *n || text.starts_with(&format!("{n} "))).max_by_key(|(n, ..)| n.len()) else { return false };
    let args = text[name.len()..].trim();
    let ch = chan.id;
    match name {
        "create friend invite" => friend_invite(app, Method::Create),
        "view friend invites" => friend_invite(app, Method::View),
        "revoke friend invites" => friend_invite(app, Method::Revoke),
        "petpet" => petpet(app, chan, args),
        _ => spotify(app, ch, name, args),
    }
    true
}

enum Method {
    Create,
    View,
    Revoke,
}

fn expiry(v: &serde_json::Value) -> String {
    chrono::DateTime::parse_from_rfc3339(v["expires_at"].as_str().unwrap_or("")).map_or(String::new(), |d| d.with_timezone(&chrono::Local).format("%b %e, %H:%M").to_string())
}

fn friend_invite(app: &mut App, what: Method) {
    let Some(api) = outgoing::api(app) else { return };
    outgoing::spawn(app, async move {
        let res = match what {
            Method::Create => api.call(reqwest::Method::POST, "/users/@me/invites", Some(json!({ "max_age": 604800, "max_uses": 5 }))).await.map(|v| {
                format!("discord.gg/{} · expires {} · max uses {}", v["code"].as_str().unwrap_or(""), expiry(&v), v["max_uses"])
            }),
            Method::View => api.call(reqwest::Method::GET, "/users/@me/invites", None).await.map(|v| {
                let lines: Vec<String> = v.as_array().into_iter().flatten().map(|i| format!("discord.gg/{} · expires {} · used {}/{}", i["code"].as_str().unwrap_or(""), expiry(i), i["uses"], i["max_uses"])).collect();
                if lines.is_empty() { "You have no active friend invites!".to_string() } else { lines.join("\n") }
            }),
            Method::Revoke => api.call(reqwest::Method::DELETE, "/users/@me/invites", None).await.map(|_| "All friend invites have been revoked.".to_string()),
        };
        match res {
            Ok(s) => Done::Toast(s),
            Err(e) => Done::Toast(e),
        }
    });
}

fn spotify(app: &mut App, ch: Id, kind: &str, msg: &str) {
    let Some(track) = app.store.spotify.clone() else { return app.toast("You're not listening to any music.") };
    let say = move |link: String| if msg.is_empty() { link } else { format!("{msg} {link}") };
    if kind == "track" {
        app.drafts.insert(ch, say(format!("https://open.spotify.com/track/{track}")));
        return app.post(ch, false);
    }
    let (path, msg) = (format!("open.spotify.com/{kind}/"), msg.to_string());
    outgoing::spawn(app, async move {
        let page = async { crate::api::http().get(format!("https://open.spotify.com/track/{track}")).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string()) };
        let res = page.await.and_then(|html| {
            let at = html.find(&path).ok_or("Failed to find the track on spotify.")?;
            let end = html[at..].find(['"', '?', '<', ' ']).map_or(html.len(), |e| at + e);
            let link = format!("https://{}", &html[at..end]);
            Ok(if msg.is_empty() { link } else { format!("{msg} {link}") })
        });
        Done::Say(ch, res)
    });
}

fn petpet(app: &mut App, chan: &Channel, args: &str) {
    let args = app.resolve_aliases(chan.id, args);
    let mut delay = 20u32;
    let mut res = 128u32;
    let mut target = None;
    for w in args.split_whitespace() {
        match w.split_once(':') {
            Some(("delay", v)) => delay = v.parse().unwrap_or(delay),
            Some(("resolution", v)) => res = v.parse().unwrap_or(res),
            _ => target = Some(w.to_string()),
        }
    }
    if delay < 20 {
        return app.toast("Delay must be at least 20.");
    }
    let user = target.as_deref().and_then(|t| t.trim_matches(|c: char| !c.is_ascii_digit()).parse::<Id>().ok()).filter(|_| target.as_deref().is_some_and(|t| !t.starts_with("http")));
    let source = match (&target, user) {
        (Some(t), None) => Source::Url(t.clone()),
        (_, Some(u)) => match app.store.user(u) {
            Some(user) => Source::Url(match &user.avatar {
                Some(h) => format!("{}/avatars/{u}/{h}.png?size=512", crate::api::cdn()),
                None => format!("{}/embed/avatars/{}.png", crate::api::cdn(), (u >> 22) % 6),
            }),
            None => return app.toast("Failed to fetch user."),
        },
        (None, None) => match app.files.iter().position(|f| f.is_image()) {
            Some(i) => Source::File(app.files.remove(i).path),
            None => return app.toast("No Image specified!"),
        },
    };
    outgoing::spawn(app, async move {
        let hands = async {
            let mut v = Vec::new();
            for i in 0..10 {
                let b = crate::api::http().get(format!("https://raw.githubusercontent.com/VenPlugs/petpet/main/frames/pet{i}.gif")).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
                v.push(b.to_vec());
            }
            Ok::<_, String>(v)
        };
        let made = async {
            let avatar = match source {
                Source::File(p) => tokio::fs::read(p).await.map_err(|e| e.to_string())?,
                Source::Url(u) => crate::api::http().get(u).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?.to_vec(),
            };
            let hands = hands.await?;
            let gif = tokio::task::spawn_blocking(move || render(&avatar, &hands, res.clamp(16, 512), delay)).await.map_err(|e| e.to_string())??;
            let dir = std::env::temp_dir().join("fastcord").join(chrono::Utc::now().timestamp_millis().to_string());
            tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
            let path = dir.join("petpet.gif");
            tokio::fs::write(&path, gif).await.map_err(|e| e.to_string())?;
            Ok(path)
        };
        Done::File(made.await)
    });
}

enum Source {
    Url(String),
    File(std::path::PathBuf),
}

fn render(avatar: &[u8], hands: &[Vec<u8>], res: u32, delay: u32) -> Result<Vec<u8>, String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::imageops::{self, FilterType};
    use image::{Delay, Frame, RgbaImage};
    let av = image::load_from_memory(avatar).map_err(|e| e.to_string())?.to_rgba8();
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
        for (i, hand) in hands.iter().enumerate() {
            let hand = image::load_from_memory(hand).map_err(|e| e.to_string())?.to_rgba8();
            let j = if i < 5 { i } else { 10 - i } as f32;
            let (w, h) = (0.8 + j * 0.02, 0.8 - j * 0.05);
            let (ox, oy) = ((1.0 - w) * 0.5 + 0.1, 1.0 - h - 0.08);
            let r = res as f32;
            let mut canvas = RgbaImage::new(res, res);
            imageops::overlay(&mut canvas, &imageops::resize(&av, (w * r) as u32, (h * r) as u32, FilterType::Triangle), (ox * r) as i64, (oy * r) as i64);
            imageops::overlay(&mut canvas, &imageops::resize(&hand, res, res, FilterType::Triangle), 0, 0);
            enc.encode_frame(Frame::from_parts(canvas, 0, 0, Delay::from_numer_denom_ms(delay, 1))).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}
