//! Pictures Discord serves from its own site under a content hash: the glyph for each kind of
//! file, the marks of connected services, illustrations. They are fetched when first drawn, as the
//! fonts and app icons are; none is in the repository. The names come out of Discord's bundle.

use egui::{Color32, Painter, Rect, pos2};

pub const ADD_FRIEND: &str = "25c27c1328c986f6.svg";
pub const NO_PINS: &str = "1772d54cb566d95b.svg";
pub const NO_THREADS: &str = "content/a55e53565529b99537f24e9111b39ad84565ad76ac91760cfb5450a069dd640c.svg";
pub const NO_BOOKMARKS: &str = "content/bd0327359da757ec3765798e53afa43e172da33e2255bf985c3667f9ff57c8cb.svg";
pub const NO_REMINDERS: &str = "content/995c79565fdbe049160aefe22dc300af4abbae1146c475d5f8b3b8c85173aabc.svg";
pub const NOTHING_SCHEDULED: &str = "content/c5582c3bbb97001b8a0f4686a09a93c734eef416f3d0e3c83eba4b6e8bbee2c2.svg";

/// Draws it once it has arrived. False until then, so a caller can keep its stand-in.
pub fn draw(p: &Painter, file: &str, rect: Rect) -> bool {
    let url = format!("{}/assets/{file}", crate::api::asset());
    let poll = egui::Image::new(crate::gif::uri(p.ctx(), &url, p.clip_rect().intersects(rect))).load_for_size(p.ctx(), rect.size());
    let Ok(egui::load::TexturePoll::Ready { texture }) = poll else { return false };
    p.image(texture.id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    true
}

const CODE: &[&str] = &[
    "c++", "cpp", "cc", "c", "h", "hpp", "mm", "m", "json", "js", "ts", "rb", "rake", "py", "asm", "fs", "pyc", "dtd", "cgi", "bat", "rss", "java", "graphml", "idb", "lua", "o", "gml", "prl", "sls",
    "conf", "cmake", "make", "sln", "vbe", "cxx", "wbf", "vbs", "r", "wml", "php", "bash", "applescript", "fcgi", "yaml", "ex", "exs", "sh", "ml", "actionscript",
];

/// The glyph Discord's file card draws for a file of that name, by the rules in its bundle.
pub fn file_icon(name: &str) -> &'static str {
    let name = name.to_lowercase();
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    let has = |list: &[&str]| list.contains(&ext);
    if ext == "psd" {
        "1084490cf680dd7c.svg"
    } else if has(&["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"]) {
        "5dea307a4037ec4e.svg"
    } else if has(&["mp4", "mov", "webm", "mkv", "avi"]) {
        "98178bd2173d947a.svg"
    } else if ext == "pdf" {
        "a42338c030af6d9c.svg"
    } else if name.contains(".ae") {
        "88b289225ff6142a.svg"
    } else if ext == "sketch" {
        "c9f0992f0ab98d49.svg"
    } else if ext == "ai" {
        "93636da24d7ab467.svg"
    } else if has(&["rar", "zip", "7z", "tar"]) || name.ends_with(".tar.gz") {
        "f89b66e97ea52ba7.svg"
    } else if has(CODE) {
        "f4bad77442422ded.svg"
    } else if has(&["txt", "rtf", "doc", "docx", "md", "pages", "ppt", "pptx", "pptm", "key", "log"]) {
        "9ebe3ef63aafee84.svg"
    } else if has(&["xls", "xlsx", "numbers", "csv"]) {
        "b8ca4488a6d443bc.svg"
    } else if has(&["html", "xhtml", "htm", "xml", "xsd", "css", "styl", "svg"]) {
        "6e688b0c124e0ca8.svg"
    } else if has(&["mp3", "ogg", "opus", "wav", "aiff", "flac"]) {
        "a6db1d05d35f13d0.svg"
    } else {
        "94660b205108a49f.svg"
    }
}

/// A connected service's mark by the name Discord shows for it: (on a dark theme, on a light one).
const SERVICES: &[(&str, &str, &str)] = &[
    ("Twitch", "4fda00c96319c8ae.svg", "4fda00c96319c8ae.svg"),
    ("YouTube", "0fa530ba9c04ac32.svg", "0fa530ba9c04ac32.svg"),
    ("Bluesky", "2709f058378a099d.svg", "2709f058378a099d.svg"),
    ("Bungie.net", "099cd81cf3b4cc98.svg", "5b0c8422db0e5507.svg"),
    ("Skype", "883a13b030d276ec.svg", "883a13b030d276ec.svg"),
    ("League of Legends", "302a27a2e5cc3fb6.svg", "302a27a2e5cc3fb6.svg"),
    ("Steam", "1f7ec18f3695d4cf.svg", "dbfa31449965dbd5.svg"),
    ("Reddit", "adfd927dcc2049a5.svg", "adfd927dcc2049a5.svg"),
    ("Facebook", "17be29f77bee4405.svg", "17be29f77bee4405.svg"),
    ("Twitter", "0815e2c86f2e0e37.svg", "0815e2c86f2e0e37.svg"),
    ("X", "a61999ae9bfb9658.svg", "5cf814b9693aac47.svg"),
    ("Spotify", "d5719388ffc613da.svg", "d5719388ffc613da.svg"),
    ("Samsung Galaxy", "4ed35737da17ede2.svg", "4ed35737da17ede2.svg"),
    ("GitHub", "a35ff3e86ffa1eb2.svg", "4db64e0649c64e7e.svg"),
    ("PlayStation Network", "eed203a7ec517e23.svg", "792da49081c21457.svg"),
    ("Epic Games", "199eceff4fca1a0c.svg", "4fb7893066c0ef83.svg"),
    ("Roblox", "a4d8e9b0404a2d00.svg", "e04a073ed006173a.svg"),
    ("PayPal", "dcb64a4ff8f61b2c.svg", "dcb64a4ff8f61b2c.svg"),
    ("eBay", "b28a7a265581b6e3.svg", "b28a7a265581b6e3.svg"),
    ("TikTok", "a303feb844769624.svg", "53641d0496c85aec.svg"),
    ("Instagram", "c05dded52023ed43.svg", "c05dded52023ed43.svg"),
    ("Mastodon", "0e6385723fe5cd49.svg", "0e6385723fe5cd49.svg"),
    ("Crunchyroll", "94ef3e8b7fa85a2e.svg", "94ef3e8b7fa85a2e.svg"),
    ("Domain", "b4376756bcbbf1ef.svg", "10d4972595802406.svg"),
    ("Amazon Music", "e3c4aacc1a54395d.svg", "e3c4aacc1a54395d.svg"),
    ("Meta Quest", "ac961aa8d4bb48a5.svg", "d64a395149091d96.svg"),
];

pub fn service(name: &str) -> Option<&'static str> {
    let under = crate::theme::pal().bg_primary;
    let light = under.r() as u32 + under.g() as u32 + under.b() as u32 > 384;
    SERVICES.iter().find(|s| s.0 == name).map(|s| if light { s.2 } else { s.1 })
}

#[cfg(test)]
mod tests {
    use super::file_icon;

    #[test]
    fn a_file_gets_the_glyph_discords_rules_give_it() {
        assert_eq!(file_icon("Report.PDF"), "a42338c030af6d9c.svg");
        assert_eq!(file_icon("backup.tar.gz"), "f89b66e97ea52ba7.svg");
        assert_eq!(file_icon("main.rs"), "94660b205108a49f.svg");
        assert_eq!(file_icon("main.py"), "f4bad77442422ded.svg");
        assert_eq!(file_icon("notes.md"), "9ebe3ef63aafee84.svg");
        assert_eq!(file_icon("sheet.csv"), "b8ca4488a6d443bc.svg");
        assert_eq!(file_icon("song.flac"), "a6db1d05d35f13d0.svg");
        assert_eq!(file_icon("no extension"), "94660b205108a49f.svg");
    }
}
