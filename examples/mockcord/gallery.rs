//! Messages shaped the way discord really sends them: galleries, stickers, forwards, components,
//! provider embeds, a thread's starter and a finished poll. Field names and nesting follow
//! the captures in ref/ and discord's own client, so the paths that read them get exercised.

use serde_json::{Value, json};

use super::{GENERAL, ME, iso, msg, now_ms, snow};

pub const GALLERY: u64 = 2080;
pub const NOTES: u64 = 2081;
const BOT: u64 = 104;

fn att(cdn: &str, id: u64, name: &str, ctype: &str, w: u32, h: u32, size: u64) -> Value {
    let url = format!("{cdn}/attachments/{GALLERY}/{id}/{name}?ex=65f0a1b2&is=65ee2cb2&hm=3f9a1c");
    json!({ "id": id.to_string(), "filename": name, "size": size, "url": url, "proxy_url": url, "width": w, "height": h,
            "content_type": ctype, "placeholder": "3PcNDYBYmId/eIBopIeIWpeaiAj4qNAB", "placeholder_version": 1, "flags": 0 })
}

fn picture(cdn: &str, id: u64, w: u32, h: u32) -> Value {
    att(cdn, id, &format!("photo-{w}x{h}-{id}.png"), "image/png", w, h, 120_000 + id * 811)
}

fn video(cdn: &str, id: u64) -> Value {
    att(cdn, id, "clip.mp4", "video/mp4", 320, 180, 19_785)
}

fn media(url: &str, w: u32, h: u32, ctype: &str) -> Value {
    json!({ "url": url, "proxy_url": url, "width": w, "height": h, "content_type": ctype, "placeholder": "3PcNDYBYmId/eIBopIeIWpeaiAj4qNAB", "placeholder_version": 1, "flags": 0 })
}

fn embed(kind: &str, url: &str, provider: (&str, &str), title: &str, description: &str, color: Option<u32>) -> Value {
    let mut e = json!({ "type": kind, "url": url, "title": title, "description": description,
                        "provider": { "name": provider.0, "url": provider.1 }, "content_scan_version": 0 });
    if let Some(c) = color {
        e["color"] = json!(c);
    }
    e
}

fn button(style: u8, label: &str, id: &str) -> Value {
    json!({ "type": 2, "style": style, "label": label, "custom_id": id })
}

pub fn messages(cdn: &str) -> Vec<Value> {
    let t0 = now_ms() - 5 * 3_600_000;
    let mut out = Vec::new();
    let mut add = |author: u64, content: &str, f: &dyn Fn(&mut Value)| {
        let mut m = msg(snow(t0 + out.len() as u64 * 90_000), GALLERY, author, content);
        f(&mut m);
        out.push(m);
    };

    let sizes: [(u32, u32); 11] = [(800, 600), (600, 800), (1024, 768), (500, 500), (1280, 720), (720, 1280), (900, 600), (640, 480), (800, 800), (1600, 900), (480, 640)];
    for (n, text) in [(2, "two photos"), (3, "three photos"), (4, "four photos"), (5, "five photos"), (7, "seven photos"), (11, "eleven photos, one more than a message holds")] {
        add(101, text, &|m| {
            m["attachments"] = json!((0..n).map(|i| picture(cdn, 7000 + n as u64 * 20 + i as u64, sizes[i].0, sizes[i].1)).collect::<Vec<_>>());
        });
    }
    add(102, "two photos and a clip", &|m| m["attachments"] = json!([picture(cdn, 7300, 800, 600), video(cdn, 7301), picture(cdn, 7302, 600, 800)]));
    add(103, "a clip and a photo", &|m| m["attachments"] = json!([video(cdn, 7310), picture(cdn, 7311, 1280, 720)]));
    add(103, "a photo, with a text file after it", &|m| {
        m["attachments"] = json!([picture(cdn, 7320, 800, 600), att(cdn, 7321, "notes.txt", "text/plain; charset=utf-8", 0, 0, 2048), picture(cdn, 7322, 800, 600)]);
    });

    let sticker = |id: &str, name: &str, format: u8| json!([{ "id": id, "name": name, "format_type": format }]);
    add(102, "", &|m| m["sticker_items"] = sticker("749054660769218631", "Wave", 3));
    add(101, "", &|m| m["sticker_items"] = sticker("754108890559176735", "Wumpus Hi", 2));
    add(103, "a gif sticker with words", &|m| m["sticker_items"] = sticker("749053689419006003", "Dancing", 4));
    add(101, "", &|m| m["sticker_items"] = sticker("819128354317312000", "Pixel Cat", 1));

    let original = snow(now_ms() - 26 * 3_600_000);
    let snapshot = |content: &str, attachments: Value, embeds: Value| {
        json!([{ "message": { "type": 0, "content": content, "embeds": embeds, "attachments": attachments, "timestamp": iso(snow_ms(original)),
                              "edited_timestamp": null, "flags": 0, "mentions": [], "mention_roles": [] } }])
    };
    let forward = |m: &mut Value, snaps: Value| {
        m["flags"] = json!(16384);
        m["message_reference"] = json!({ "type": 1, "channel_id": GENERAL.to_string(), "message_id": original.to_string(), "guild_id": "1000" });
        m["message_snapshots"] = snaps;
    };
    add(102, "", &|m| forward(m, snapshot("cold start went from ~4s to *under a second* for me 🎉", json!([]), json!([]))));
    add(103, "", &|m| forward(m, snapshot("the new layout", json!([picture(cdn, 7400, 800, 450)]), json!([]))));
    add(101, "this is the one I meant", &|m| {
        forward(m, snapshot("two of them", json!([picture(cdn, 7410, 800, 600), picture(cdn, 7411, 600, 800)]), json!([embed("link", "https://github.com/emilk/egui", ("GitHub", "https://github.com"), "emilk/egui", "egui: an easy-to-use immediate mode GUI in Rust", None)])));
    });

    add(BOT, "Pick a deploy target", &|m| {
        m["application_id"] = json!("9200");
        m["components"] = json!([
            { "type": 1, "id": 1, "components": [button(1, "Approve", "approve"), button(2, "Skip", "skip"), button(3, "Ship it", "ship"), button(4, "Roll back", "rollback"),
                json!({ "type": 2, "id": 5, "style": 5, "label": "Docs", "url": "https://docs.rs/egui" })] },
            { "type": 1, "id": 6, "components": [
                json!({ "type": 2, "id": 7, "style": 2, "label": "Already done", "custom_id": "done", "disabled": true, "emoji": { "id": null, "name": "✅" } }),
                json!({ "type": 2, "id": 8, "style": 2, "label": "Ferris", "custom_id": "ferris", "emoji": { "id": "5000", "name": "ferris", "animated": false } })] },
            { "type": 1, "id": 9, "components": [json!({ "type": 3, "id": 10, "custom_id": "region", "placeholder": "Choose a region", "min_values": 1, "max_values": 1, "options": [
                { "label": "Frankfurt", "value": "fra", "description": "eu-central", "default": false }, { "label": "Virginia", "value": "iad", "description": "us-east", "default": false },
                { "label": "Singapore", "value": "sin", "default": false }] })] },
            { "type": 1, "id": 11, "components": [json!({ "type": 5, "id": 12, "custom_id": "owner", "placeholder": "Pick a reviewer", "min_values": 1, "max_values": 1 })] },
        ]);
    });

    add(BOT, "", &|m| {
        m["application_id"] = json!("9200");
        m["flags"] = json!(32768);
        m["components"] = json!([
            { "type": 17, "id": 1, "accent_color": 5793266, "spoiler": false, "components": [
                { "type": 9, "id": 2, "components": [{ "type": 10, "id": 3, "content": "## Release 1.3.0\nCold start is **0.4s** now." }, { "type": 10, "id": 4, "content": "-# shipped by dyno" }],
                  "accessory": { "type": 11, "id": 5, "media": media(&format!("{cdn}/attachments/{GALLERY}/7500/photo-200x200-7500.png"), 200, 200, "image/png"), "description": "logo", "spoiler": false } },
                { "type": 14, "id": 6, "divider": true, "spacing": 1 },
                { "type": 10, "id": 7, "content": "Highlights:\n- voice and video\n- forums\n- polish" },
                { "type": 12, "id": 8, "items": [
                    { "media": media(&format!("{cdn}/attachments/{GALLERY}/7501/photo-800x600-7501.png"), 800, 600, "image/png"), "description": "one", "spoiler": false },
                    { "media": media(&format!("{cdn}/attachments/{GALLERY}/7502/photo-600x800-7502.png"), 600, 800, "image/png"), "description": null, "spoiler": false },
                    { "media": media(&format!("{cdn}/attachments/{GALLERY}/7503/photo-800x600-7503.png"), 800, 600, "image/png"), "description": null, "spoiler": false }] },
                { "type": 13, "id": 9, "name": "notes.txt", "size": 2048, "spoiler": false,
                  "file": media(&format!("{cdn}/attachments/{GALLERY}/7504/notes.txt?ex=65f0a1b2&is=65ee2cb2&hm=3f9a1c"), 0, 0, "text/plain") },
                { "type": 14, "id": 10, "divider": false, "spacing": 2 },
                { "type": 9, "id": 11, "components": [{ "type": 10, "id": 12, "content": "Want the changelog?" }],
                  "accessory": { "type": 2, "id": 13, "style": 1, "label": "Open", "custom_id": "changelog" } },
                { "type": 1, "id": 14, "components": [button(2, "Dismiss", "dismiss"), json!({ "type": 2, "id": 15, "style": 5, "label": "GitHub", "url": "https://github.com/emilk/egui" })] },
            ] },
            { "type": 10, "id": 16, "content": "a text display outside the container" },
        ]);
        m["attachments"] = json!([att(cdn, 7504, "notes.txt", "text/plain; charset=utf-8", 0, 0, 2048)]);
    });

    let yt = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    add(102, yt, &|m| {
        let mut e = embed("video", yt, ("YouTube", "https://www.youtube.com"), "Rick Astley - Never Gonna Give You Up (Official Video)", "The official video for “Never Gonna Give You Up” by Rick Astley", Some(16711680));
        e["author"] = json!({ "name": "Rick Astley", "url": "https://www.youtube.com/channel/UCuAXFkgsw1L7xaCfnd5JJOw" });
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7600/photo-1280x720-7600.png"), 1280, 720, "image/jpeg");
        e["video"] = json!({ "url": "https://www.youtube.com/embed/dQw4w9WgXcQ", "width": 1280, "height": 720, "flags": 0 });
        m["embeds"] = json!([e]);
    });
    let tw = "https://www.twitch.tv/rustlang";
    add(103, tw, &|m| {
        let mut e = embed("video", tw, ("Twitch", "https://www.twitch.tv"), "rustlang - Twitch", "Rust Foundation live: building egui apps", Some(10181046));
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7610/photo-1280x720-7610.png"), 1280, 720, "image/jpeg");
        e["video"] = json!({ "url": "https://player.twitch.tv/?channel=rustlang&enableExtensions=true&muted=false&parent=twitch.tv&player=discord_twitch_embed&quality=auto", "width": 620, "height": 378, "flags": 0 });
        m["embeds"] = json!([e]);
    });
    let tt = "https://www.tiktok.com/@scout2015/video/6718335390845095173";
    add(101, tt, &|m| {
        let mut e = embed("video", tt, ("TikTok", "https://www.tiktok.com"), "Scout the Bulldog on TikTok", "Scout and Suki get along", Some(16711782));
        e["author"] = json!({ "name": "Scout (@scout2015)", "url": "https://www.tiktok.com/@scout2015" });
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7620/photo-720x1280-7620.png"), 720, 1280, "image/jpeg");
        e["video"] = json!({ "url": "https://www.tiktok.com/player/v1/6718335390845095173?&music_info=1&description=1", "width": 340, "height": 700, "flags": 0 });
        m["embeds"] = json!([e]);
    });
    let medal = "https://medal.tv/games/rust/clips/abc123XYZ";
    add(102, medal, &|m| {
        let mut e = embed("video", medal, ("Medal.tv", "https://medal.tv"), "Insane clutch", "Watch “Insane clutch” by ferris on Medal, the largest game clip platform.", Some(16764928));
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7630/photo-1280x720-7630.png"), 1280, 720, "image/jpeg");
        e["video"] = json!({ "url": format!("{cdn}/media/spin.mp4"), "proxy_url": format!("{cdn}/media/spin.mp4"), "width": 1280, "height": 720, "flags": 0 });
        m["embeds"] = json!([e]);
    });
    let sp = "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT";
    add(103, sp, &|m| {
        let mut e = embed("link", sp, ("Spotify", "https://www.spotify.com"), "Never Gonna Give You Up", "Rick Astley · Whenever You Need Somebody · Song · 1987", Some(2021216));
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7640/photo-640x640-7640.png"), 640, 640, "image/jpeg");
        e["video"] = json!({ "url": "https://open.spotify.com/embed/track/4cOdK2wGLETKBW3PvgPWqT", "width": 400, "height": 80, "flags": 0 });
        m["embeds"] = json!([e]);
    });
    let news = "https://blog.rust-lang.org/2026/10/30/Rust-1.93.0.html";
    add(101, news, &|m| {
        let mut e = embed("article", news, ("Rust Blog", "https://blog.rust-lang.org"), "Announcing Rust 1.93.0", "The Rust team is happy to announce a new version of Rust, 1.93.0. Rust is a programming language empowering everyone to build reliable and efficient software.", None);
        e["author"] = json!({ "name": "The Rust Release Team" });
        e["thumbnail"] = media(&format!("{cdn}/attachments/{GALLERY}/7650/photo-1200x630-7650.png"), 1200, 630, "image/png");
        e["timestamp"] = json!(iso(now_ms() - 3 * 86_400_000));
        m["embeds"] = json!([e]);
    });
    let link = "https://github.com/emilk/egui";
    add(102, link, &|m| {
        let mut e = embed("link", link, ("GitHub", "https://github.com"), "GitHub - emilk/egui: egui: an easy-to-use immediate mode GUI in Rust that runs on both web and native", "egui: an easy-to-use immediate mode GUI in Rust that runs on both web and native - emilk/egui", Some(0x2b3137));
        e["image"] = media(&format!("{cdn}/attachments/{GALLERY}/7660/photo-1200x600-7660.png"), 1200, 600, "image/png");
        m["embeds"] = json!([e]);
    });

    add(101, "starting a thread on this one", &|m| {
        m["flags"] = json!(32);
        m["thread"] = thread();
    });

    let poll = |question: &str, answers: Vec<(&str, u64, bool)>, expiry: u64, finalized: bool| {
        json!({ "question": { "text": question }, "layout_type": 1, "allow_multiselect": false, "expiry": iso(expiry),
                "answers": answers.iter().enumerate().map(|(i, (t, _, _))| json!({ "answer_id": i + 1, "poll_media": { "text": t } })).collect::<Vec<_>>(),
                "results": { "is_finalized": finalized, "answer_counts": answers.iter().enumerate().map(|(i, (_, n, me))| json!({ "id": i + 1, "count": n, "me_voted": me })).collect::<Vec<_>>() } })
    };
    add(102, "", &|m| m["poll"] = poll("tabs or spaces?", vec![("tabs", 3, false), ("spaces", 9, true)], now_ms() + 2 * 86_400_000, false));
    add(103, "", &|m| m["poll"] = poll("which day for the release?", vec![("monday", 12, false), ("friday", 4, false)], now_ms() - 3_600_000, true));
    out
}

pub fn thread() -> Value {
    json!({ "id": NOTES.to_string(), "type": 11, "guild_id": "1000", "parent_id": GALLERY.to_string(), "name": "gallery notes", "owner_id": "101",
            "message_count": 2, "member_count": 2, "rate_limit_per_user": 0, "flags": 0, "last_message_id": null,
            "thread_metadata": { "archived": false, "archive_timestamp": iso(now_ms()), "auto_archive_duration": 1440, "locked": false },
            "member": { "user_id": ME.to_string(), "join_timestamp": iso(now_ms()), "flags": 1 } })
}

pub fn thread_messages(starter_of: &Value) -> Vec<Value> {
    let t0 = now_ms() - 4 * 3_600_000;
    let mut starter = msg(snow(t0 - 60_000), NOTES, starter_of["author"]["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(101), "");
    starter["type"] = json!(21);
    starter["message_reference"] = json!({ "channel_id": GALLERY.to_string(), "message_id": starter_of["id"], "guild_id": "1000" });
    starter["referenced_message"] = starter_of.clone();
    vec![starter, msg(snow(t0), NOTES, 102, "this thread has a starter at the top")]
}

fn snow_ms(id: u64) -> u64 {
    (id >> 22) + 1_420_070_400_000
}
