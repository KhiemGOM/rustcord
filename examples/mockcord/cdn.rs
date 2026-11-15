//! What discord's CDN accepts: a url with the wrong shape fails here the way it does there,
//! so a client building `avatars/{hash}.png` or an animated hash as `.gif` by mistake is caught.

type Reply = (u16, &'static str, Vec<u8>);

fn reject(code: u16, why: &str) -> Reply {
    (code, "application/json", format!("{{\"message\":\"{why}\",\"code\":0}}").into_bytes())
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn split(file: &str) -> Option<(&str, &str)> {
    let (stem, ext) = file.rsplit_once('.')?;
    (!stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')).then_some((stem, ext))
}

/// `hash.png`: any of the image types, and `.gif` only for an animated (`a_`) hash.
fn hashed(file: &str) -> bool {
    match split(file) {
        Some((stem, "gif")) => stem.starts_with("a_"),
        Some((_, "png" | "jpg" | "jpeg" | "webp")) => true,
        _ => false,
    }
}

fn numbered(file: &str, exts: &[&str]) -> bool {
    split(file).is_some_and(|(stem, ext)| numeric(stem) && exts.contains(&ext))
}

pub fn query(path: &str, key: &str) -> Option<String> {
    let (_, q) = path.split_once('?')?;
    q.split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')).map(str::to_string)
}

const SIZES: [u32; 31] = [16, 20, 22, 24, 28, 32, 40, 44, 48, 56, 60, 64, 80, 96, 100, 128, 160, 240, 256, 300, 320, 480, 512, 600, 640, 1024, 1280, 1536, 2048, 3072, 4096];

/// None when the path is fine; otherwise discord's refusal.
pub fn check(path: &str) -> Option<Reply> {
    let clean = path.split('?').next().unwrap_or("");
    let parts: Vec<&str> = clean.trim_start_matches('/').split('/').collect();
    let ok = match parts.as_slice() {
        ["cdn", "avatars" | "banners" | "icons" | "splashes" | "discovery-splashes" | "role-icons" | "channel-icons" | "app-icons" | "app-assets" | "guild-tag-badges" | "clan-badges", id, file] => {
            numeric(id) && hashed(file)
        }
        ["cdn", "guilds", g, "users", u, "avatars" | "banners", file] => numeric(g) && numeric(u) && hashed(file),
        ["cdn", "avatars", u, "archived", id, file] => numeric(u) && numeric(id) && hashed(file),
        ["cdn", "emojis", file] => numbered(file, &["png", "webp", "gif", "jpg"]),
        ["cdn", "stickers", file] => numbered(file, &["png", "webp", "gif", "json"]),
        ["cdn", "embed", "avatars", file] => split(file).is_some_and(|(s, e)| s.len() == 1 && s.as_bytes()[0] < b'6' && e == "png"),
        ["cdn", "avatar-decoration-presets" | "badge-icons", file] => split(file).is_some_and(|(_, e)| e == "png"),
        ["cdn", "attachments", ch, id, name] => numeric(ch) && numeric(id) && !name.is_empty(),
        ["cdn", "soundboard-sounds", id] => numeric(id),
        ["cdn", "quests", id, _] => numeric(id),
        ["cdn", "media" | "assets" | "emoji" | "external", ..] => true,
        _ => false,
    };
    if !ok {
        return Some(reject(404, "Not Found"));
    }
    if let Some(s) = query(path, "size") {
        match s.parse::<u32>() {
            Ok(n) if SIZES.contains(&n) => {}
            _ => return Some(reject(400, "Invalid size")),
        }
    }
    None
}

/// The size an attachment's file name promises (`photo-800x600-7.png`), else 800x450.
pub fn attachment_size(name: &str) -> (u32, u32) {
    let dims = name.split('-').find_map(|p| {
        let (w, h) = p.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    dims.unwrap_or((800, 450))
}

/// What a request for `width` and `height` gets of a picture `natural` big: the whole picture,
/// shrunk to fit inside them and never grown, the way discord's media proxy does.
pub fn fitted(natural: (u32, u32), path: &str) -> (u32, u32) {
    let ask = |k: &str| query(path, k).and_then(|v| v.parse::<u32>().ok());
    let (nw, nh) = (natural.0 as f32, natural.1 as f32);
    let scale = match (ask("width"), ask("height")) {
        (Some(w), Some(h)) => (w as f32 / nw).min(h as f32 / nh),
        (Some(w), None) => w as f32 / nw,
        (None, Some(h)) => h as f32 / nh,
        (None, None) => 1.0,
    }
    .min(1.0);
    (((nw * scale).round() as u32).max(1), ((nh * scale).round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatars_need_an_id_and_a_hash() {
        assert!(check("/cdn/avatars/100/av100.png?size=128").is_none());
        assert!(check("/cdn/avatars/av100.png").is_some());
        assert!(check("/cdn/avatars/100/av100").is_some());
        assert!(check("/cdn/avatars/abc/av100.png").is_some());
    }

    #[test]
    fn only_an_animated_hash_may_be_a_gif() {
        assert!(check("/cdn/avatars/100/a_5c1.gif").is_none());
        assert!(check("/cdn/avatars/100/5c1.gif").is_some());
        assert!(check("/cdn/avatars/100/a_5c1.png").is_none());
    }

    #[test]
    fn size_is_a_number_in_range() {
        assert!(check("/cdn/icons/1000/icon1000.png?size=600").is_none());
        assert!(check("/cdn/icons/1000/icon1000.png?size=big").is_some());
        assert!(check("/cdn/icons/1000/icon1000.png?size=8192").is_some());
    }

    #[test]
    fn attachments_carry_their_channel_and_id() {
        assert!(check("/cdn/attachments/2011/1/shot.png?ex=1").is_none());
        assert!(check("/cdn/attachments/shot.png").is_some());
    }

    #[test]
    fn a_proxied_picture_only_shrinks() {
        assert_eq!(fitted((800, 600), "/x?width=400&height=400"), (400, 300));
        assert_eq!(fitted((800, 600), "/x?width=1600&height=1600"), (800, 600));
    }
}
