//! Tiny fake Discord for the e2e harness: REST + CDN over plain HTTP, gateway over ws,
//! and a voice server (ws + udp on the same port number) with one fake speaker.
//! usage: mockcord <http_port> <ws_port> <request_log.jsonl> [voice_port]

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::XChaCha20Poly1305;
use davey::{DaveSession, MediaType, ProposalsOperationType};
use futures_util::{SinkExt, StreamExt};
use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use serde_json::{Value, json};
use std::io::Write;
use std::num::NonZeroU16;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message as Ws;

const ME: u64 = 100;
static PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
const GENERAL: u64 = 2011;
const MEDIA: u64 = 2060;
const SPEAKER: u64 = 101;
const SPEAKER_SSRC: u32 = 2001;
const CLIENT_SSRC: u32 = 1001;
const OWN_STREAM: u64 = 5000;
const ALICE_STREAM: u64 = 5001;

struct World {
    msgs: Vec<Value>,
    log: std::fs::File,
    events: broadcast::Sender<Value>,
    voice_port: u16,
    voice_chan: Option<(Option<u64>, u64)>,
    voices: Vec<Voice>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Voice,
    /// the client's own Go Live
    Own,
    /// the client watching Alice's stream
    Watch,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Voice => "voice",
            Kind::Own => "own_stream",
            Kind::Watch => "watch_stream",
        }
    }
}

// what one media session agreed on; sessions are told apart by the client's ssrc
struct Voice {
    kind: Kind,
    cssrc: u32,
    mode: String,
    key: [u8; 32],
    client: Option<SocketAddr>,
    dec: opus_rs::OpusDecoder,
    vdec: openh264::decoder::Decoder,
    asm: Asm,
    key_frame: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    // Alice's side of the E2EE group; media is only E2EE once the transition executed
    alice: DaveSession,
    e2ee: bool,
}

// the gateway's MLS external sender: it proposes adds, members commit them
struct Gw {
    signer: SignatureKeyPair,
    sender: Vec<u8>,
    provider: OpenMlsRustCrypto,
}

impl Gw {
    fn new() -> Gw {
        let signer = SignatureKeyPair::new(SignatureScheme::ECDSA_SECP256R1_SHA256).unwrap();
        let cred: Credential = BasicCredential::new(b"mock-voice-gateway".to_vec()).into();
        let sender = ExternalSender::new(signer.public().into(), cred).tls_serialize_detached().unwrap();
        Gw { signer, sender, provider: OpenMlsRustCrypto::default() }
    }

    /// Wraps a member's key package in an external Add proposal, framed the way op 27 carries it.
    /// Key packages arrive bare, not inside an MLSMessage (libdave and davey both send them that way).
    fn propose_add(&self, kp: &[u8], channel: u64) -> Option<Vec<u8>> {
        let kp = KeyPackageIn::tls_deserialize_exact(kp).ok()?;
        let kp = kp.validate(self.provider.crypto(), ProtocolVersion::Mls10).ok()?;
        let msg = ExternalProposal::new_add::<OpenMlsRustCrypto>(
            kp,
            GroupId::from_slice(&channel.to_be_bytes()),
            GroupEpoch::from(0),
            &self.signer,
            SenderExtensionIndex::new(0),
        )
        .ok()?;
        VLBytes::new(msg.tls_serialize_detached().ok()?).tls_serialize_detached().ok()
    }
}

type W = Arc<Mutex<World>>;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn snow(ms: u64) -> u64 {
    (ms - 1_420_070_400_000) << 22
}

fn user(id: u64) -> Value {
    let (name, global, bot) = match id {
        100 => ("you", "You", false),
        101 => ("alice", "Alice", false),
        102 => ("bob_builds", "Bob 🔨", false),
        103 => ("carol", "Carol ✿", false),
        104 => ("dyno", "Dyno", true),
        106 => ("frank", "Frank", false),
        _ => ("eve", "Eve", false),
    };
    json!({ "id": id.to_string(), "username": name, "global_name": global, "avatar": format!("av{id}"), "bot": bot, "discriminator": "0" })
}

fn member(id: u64) -> Value {
    let roles: Vec<String> = match id {
        101 => vec!["1100".into()],
        102 | 100 | 106 => vec!["1101".into()],
        104 => vec!["1102".into()],
        _ => vec![],
    };
    json!({ "roles": roles, "nick": null })
}

fn msg(id: u64, ch: u64, author: u64, content: &str) -> Value {
    json!({
        "id": id.to_string(), "channel_id": ch.to_string(), "author": user(author),
        "content": content, "type": 0, "attachments": [], "embeds": [], "mentions": [], "mention_roles": [],
        "mention_everyone": false, "edited_timestamp": null
    })
}

fn seed(cdn: &str) -> Vec<Value> {
    let lines = [
        (101, "morning everyone ☀️"),
        (102, "hey! did anyone try the new release yet?"),
        (102, "the changelog says startup is **way** faster"),
        (103, "yeah, cold start went from ~4s to *under a second* for me 🎉"),
        (101, "nice. `cargo build --release` and done"),
        (104, "📊 **Daily stats** — 1,204 messages, 87 active members"),
        (105, "can someone help me with lifetimes? <#2012>"),
        (101, "sure, post it in help and ping me <@101>"),
        (103, "> borrow checker is my friend\n> borrow checker is my friend\nrepeat until true 😅"),
        (102, "```rust\nfn main() {\n    println!(\"hello, fastcord\");\n}\n```"),
        (101, "||spoiler: it compiles first try||"),
        (100, "this is me testing ~~strikethrough~~ and __underline__"),
        (103, "check this out https://github.com/emilk/egui"),
        (105, "lol 😂😂"),
        (102, "brb, coffee ☕"),
        (106, "lurker here, first message in months 👋"),
    ];
    let n = 160u64;
    let start = now_ms() - n * 95_000;
    let mut out: Vec<Value> = Vec::new();
    for i in 0..n {
        let (a, text) = lines[(i as usize) % lines.len()];
        let id = snow(start + i * 95_000);
        let mut m = msg(id, GENERAL, a, text);
        if i == n - 9 {
            m["attachments"] = json!([{ "id": "1", "filename": "screenshot.png", "size": 183_220,
                "url": format!("{cdn}/attachments/shot.png"), "proxy_url": format!("{cdn}/attachments/shot.png"),
                "width": 800, "height": 450, "content_type": "image/png" }]);
            m["content"] = json!("here's the new layout");
        }
        if i == n - 7 {
            m["embeds"] = json!([{ "type": "rich", "title": "Release v1.2.0", "url": "https://example.com/release",
                "description": "Faster startup, smoother scrolling and **less jank**.", "color": 0x5865F2,
                "fields": [{ "name": "Cold start", "value": "0.4s", "inline": true }, { "name": "Idle CPU", "value": "0%", "inline": true },
                           { "name": "Memory", "value": "~90 MB", "inline": true }],
                "footer": { "text": "fastcord releases" } }]);
        }
        if i == n - 5 {
            m["reactions"] = json!([
                { "emoji": { "id": null, "name": "🔥" }, "count": 4, "me": true },
                { "emoji": { "id": null, "name": "👍" }, "count": 2, "me": false },
            ]);
        }
        if i == n - 3 {
            let prev = out[(n - 4) as usize].clone();
            m["type"] = json!(19);
            m["referenced_message"] = prev;
            m["content"] = json!("agreed, this one is great");
        }
        if i == n - 2 {
            m["mentions"] = json!([user(ME)]);
            m["content"] = json!("<@100> you around? need a review");
        }
        if i == n - 12 {
            m["content"] = json!("let's plan the next release in a thread");
            m["thread"] = thread();
        }
        if i == n - 6 || i == n - 40 {
            m["pinned"] = json!(true);
        }
        out.push(m);
    }
    out.extend(media_msgs(cdn));
    let t0 = now_ms() - 30 * 60_000;
    for (i, (a, text)) in [(101, "ok what's left for 1.3?"), (102, "voice + video, then polish"), (100, "I'll take the settings screen"), (103, "forums too 🙏")].iter().enumerate() {
        out.push(msg(snow(t0 + i as u64 * 60_000), THREAD, *a, text));
    }
    for (post, a, text) in [(POST_A, 103, "Could we get a compact message mode? Long threads get tall fast."), (POST_B, 105, "Emoji picker should remember recently used ones.")] {
        out.push(msg(post, FORUM, a, text));
    }
    out
}

// animated emoji, a gifv with a title, and gifs behind urls with query strings
fn media_msgs(cdn: &str) -> Vec<Value> {
    let t0 = now_ms() - 12 * 60_000;
    let at = |i: u64| snow(t0 + i * 60_000);
    let mut out = Vec::new();

    let mut m = msg(at(0), MEDIA, 101, "party time <a:party:5100> and a static <:ferris:5000> in one line <a:party:5100>");
    m["reactions"] = json!([
        { "emoji": { "id": "5100", "name": "party", "animated": true }, "count": 3, "me": false },
        { "emoji": { "id": null, "name": "🔥" }, "count": 1, "me": false },
    ]);
    out.push(m);

    out.push(msg(at(1), MEDIA, 101, "<a:party:5100>"));

    let link = "https://klipy.com/gifs/mock-spin";
    let mut m = msg(at(2), MEDIA, 103, link);
    m["embeds"] = json!([{ "type": "gifv", "url": link, "title": "Mock Spin Dance", "provider": { "name": "Klipy" },
        "thumbnail": { "url": format!("{cdn}/media/spin.png"), "proxy_url": format!("{cdn}/media/spin.png"), "width": 160, "height": 120 },
        "video": { "url": format!("{cdn}/media/spin.mp4"), "proxy_url": format!("{cdn}/media/spin.mp4"), "width": 160, "height": 120 } }]);
    out.push(m);

    let link = format!("{cdn}/media/wave.gif?size=96&name=wave");
    let mut m = msg(at(3), MEDIA, 101, &link);
    m["embeds"] = json!([{ "type": "image", "url": link,
        "thumbnail": { "url": link, "proxy_url": link, "width": 96, "height": 72 } }]);
    out.push(m);

    let mut m = msg(at(4), MEDIA, 102, "dance.gif as an upload");
    let dance = format!("{cdn}/media/dance.gif?ex=65f0a1b2&is=65ee2cb2&hm=abc");
    m["attachments"] = json!([{ "id": "2", "filename": "dance.gif", "size": 41_000, "url": dance, "proxy_url": dance,
        "width": 120, "height": 90, "content_type": "image/gif" }]);
    out.push(m);
    out
}

// six 100ms frames that all look different, so a screenshot tells which one was up
fn anim_gif(w: u32, h: u32, hue: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        let frames = (0..6u32).map(|f| {
            let a = f as f32 / 6.0 * std::f32::consts::TAU;
            let (cx, cy) = (0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin());
            let base = [(0xed, 0x42, 0x45), (0xfa, 0xa6, 0x1a), (0x3b, 0xa5, 0x5c), (0x58, 0x65, 0xf2), (0xeb, 0x45, 0x9e), (0x1a, 0xbc, 0x9c)][((hue + f) % 6) as usize];
            let img = image::RgbaImage::from_fn(w, h, |x, y| {
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                if ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() < 0.18 {
                    image::Rgba([255, 255, 255, 255])
                } else {
                    image::Rgba([base.0, base.1, base.2, 255])
                }
            });
            image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(100, 1))
        });
        enc.encode_frames(frames).unwrap();
    }
    out
}

const THREAD: u64 = 2050;
const FORUM: u64 = 2030;
const POST_A: u64 = 2031;
const POST_B: u64 = 2032;

fn thread() -> Value {
    json!({ "id": THREAD.to_string(), "type": 11, "guild_id": "1000", "parent_id": GENERAL.to_string(), "name": "release planning",
            "owner_id": "101", "message_count": 4, "member_count": 4, "thread_metadata": { "archived": false },
            "last_message_id": snow(now_ms() - 27 * 60_000).to_string(), "member": { "user_id": ME.to_string() } })
}

fn bot_buttons() -> Value {
    json!([{ "type": 1, "components": [
        { "type": 2, "style": 1, "label": "Snooze", "custom_id": "snooze", "emoji": { "name": "😴" } },
        { "type": 2, "style": 4, "label": "Cancel", "custom_id": "cancel" },
        { "type": 2, "style": 5, "label": "Docs", "url": "https://example.com/docs" },
    ]}])
}

// the client posts uploads and interactions as multipart; pull the json part out
fn payload_json(body: &[u8]) -> Value {
    let s = String::from_utf8_lossy(body);
    let Some(i) = s.find("name=\"payload_json\"") else { return Value::Null };
    let rest = &s[i..];
    let Some(j) = rest.find("\r\n\r\n") else { return Value::Null };
    let rest = &rest[j + 4..];
    let end = rest.find("\r\n--").unwrap_or(rest.len());
    serde_json::from_str(&rest[..end]).unwrap_or_default()
}

fn forum_posts() -> Value {
    let post = |id: u64, owner: u64, name: &str, tags: &[&str], count: u32| {
        json!({ "id": id.to_string(), "type": 11, "guild_id": "1000", "parent_id": FORUM.to_string(), "name": name,
                "owner_id": owner.to_string(), "message_count": count, "applied_tags": tags, "thread_metadata": { "archived": false },
                "last_message_id": snow(now_ms() - (id - 2030) * 3_600_000).to_string() })
    };
    json!({
        "threads": [post(POST_A, 103, "Compact mode", &["1"], 7), post(POST_B, 105, "Recent emoji in picker", &["1", "2"], 3)],
        "members": [], "has_more": false, "total_results": 2,
        "first_messages": [msg(POST_A, POST_A, 103, "Could we get a compact message mode? Long threads get tall fast."),
                           msg(POST_B, POST_B, 105, "Emoji picker should remember recently used ones.")],
    })
}

// servers for the rail and channel list checks: a folder of four, one that must show a pill,
// and three that must not (muted channel, muted category, "only @mentions")
fn side_guilds() -> Vec<Value> {
    let ch = |id: u64, kind: u8, name: &str, pos: i32, parent: Option<u64>| {
        json!({ "id": id.to_string(), "type": kind, "name": name, "position": pos, "parent_id": parent.map(|p| p.to_string()),
                "permission_overwrites": [], "last_message_id": snow(now_ms() - 90_000).to_string() })
    };
    let everyone = |id: u64| json!({ "id": id.to_string(), "name": "@everyone", "color": 0, "position": 0, "permissions": "1051648" });
    let guild = |id: u64, name: &str, chans: Vec<Value>| {
        json!({ "id": id.to_string(), "name": name, "icon": null, "owner_id": "102", "roles": [everyone(id)], "emojis": [], "channels": chans })
    };

    let mut trusted = ch(2302, 0, "『🔒』trusted", 1, Some(2300));
    trusted["permission_overwrites"] = json!([{ "id": "1003", "type": 0, "allow": "0", "deny": "1024" }, { "id": "1201", "type": 0, "allow": "1024", "deny": "0" }]);
    let mut news = ch(2304, 5, "『📢』news", 3, Some(2300));
    news["permission_overwrites"] = json!([{ "id": "1003", "type": 0, "allow": "0", "deny": "1024" }, { "id": "1201", "type": 0, "allow": "1024", "deny": "0" }]);
    let mut spicy = ch(2305, 0, "spicy-takes", 4, Some(2300));
    spicy["nsfw"] = json!(true);
    let mut staff = ch(2309, 2, "Staff Room", 1, Some(2308));
    staff["permission_overwrites"] = json!([{ "id": "1003", "type": 0, "allow": "0", "deny": "1048576" }]);
    let mut emoji = guild(1003, "Emoji Lounge", vec![
        ch(2300, 4, "Main", 0, None), ch(2301, 0, "rules", 0, Some(2300)), trusted, ch(2303, 0, "『💬』general", 2, Some(2300)),
        news, spicy, ch(2306, 15, "ideas", 5, Some(2300)), ch(2307, 16, "media-wall", 6, Some(2300)),
        ch(2308, 4, "Voice", 1, None), ch(2310, 2, "Lounge", 0, Some(2308)), staff, ch(2311, 13, "Town Hall", 2, Some(2308)),
    ]);
    emoji["rules_channel_id"] = json!("2301");
    emoji["roles"] = json!([everyone(1003), { "id": "1201", "name": "Trusted", "color": 0, "position": 1, "permissions": "0" }]);

    let mut den = guild(1008, "Voice Den", vec![ch(2800, 0, "chat", 0, None), ch(2801, 2, "Hangout", 1, None)]);
    den["voice_states"] = json!([{ "user_id": "101", "channel_id": "2801" }]);
    vec![
        emoji,
        guild(1004, "Quiet Corner", vec![ch(2400, 0, "noise", 0, None)]),
        guild(1005, "Muted Category", vec![ch(2500, 4, "Archive", 0, None), ch(2501, 0, "old-news", 0, Some(2500))]),
        guild(1006, "Mentions Only", vec![ch(2600, 0, "firehose", 0, None)]),
        guild(1007, "Loud Place", vec![ch(2700, 0, "alerts", 0, None)]),
        den,
    ]
}

fn side_reads() -> Vec<Value> {
    [2400, 2501, 2600, 2700].iter().map(|c| json!({ "id": c.to_string(), "last_message_id": "0", "mention_count": 0 })).collect()
}

fn ready(last_general: u64) -> Value {
    let ch = |id: u64, kind: u8, name: &str, pos: i32, parent: Option<u64>| {
        json!({ "id": id.to_string(), "type": kind, "name": name, "position": pos,
                "parent_id": parent.map(|p| p.to_string()), "permission_overwrites": [], "last_message_id": null })
    };
    let mut general = ch(GENERAL, 0, "general", 0, Some(2010));
    general["last_message_id"] = json!(last_general.to_string());
    general["topic"] = json!("Talk about anything. Be nice. Rust only on Tuesdays.");
    let mut help = ch(2012, 0, "help", 1, Some(2010));
    help["last_message_id"] = json!(snow(now_ms() - 60_000).to_string());
    let mut forum = ch(FORUM, 15, "ideas", 4, Some(2010));
    forum["available_tags"] = json!([{ "id": "1", "name": "feature", "emoji_name": "✨" }, { "id": "2", "name": "ux" }]);
    let mut secret = ch(2014, 0, "mods-only", 3, Some(2010));
    secret["permission_overwrites"] = json!([{ "id": "1000", "type": 0, "allow": "0", "deny": "1024" }]);
    let roles = json!([
        { "id": "1000", "name": "@everyone", "color": 0, "position": 0, "permissions": "3072", "hoist": false },
        { "id": "1100", "name": "Moderator", "color": 0xE67E22, "position": 3, "permissions": "8", "hoist": true },
        { "id": "1101", "name": "Contributor", "color": 0x3498DB, "position": 2, "permissions": "0", "hoist": false },
        { "id": "1102", "name": "Bots", "color": 0x9B59B6, "position": 1, "permissions": "0", "hoist": false },
    ]);
    let g1 = json!({
        "id": "1000", "name": "Rust Lounge", "icon": "icon1000", "owner_id": "101", "member_count": 6,
        "roles": roles, "emojis": [{ "id": "5000", "name": "ferris", "animated": false }, { "id": "5100", "name": "party", "animated": true }],
        "channels": [
            ch(2010, 4, "Text Channels", 0, None), general, help, ch(2013, 0, "🎲┃off-topic", 2, Some(2010)), secret,
            ch(2000, 4, "Information", 1, None), ch(2001, 0, "rules", 0, Some(2000)), ch(2002, 5, "📢・announcements", 1, Some(2000)),
            ch(2020, 4, "Voice Channels", 2, None), ch(2021, 2, "Lounge", 0, Some(2020)), ch(2022, 2, "Gaming", 1, Some(2020)),
            forum, ch(MEDIA, 0, "media", 5, Some(2010)),
        ],
        "threads": [thread()],
        "voice_states": [{ "user_id": "101", "channel_id": "2021" }, { "user_id": "103", "channel_id": "2021" }],
    });
    let simple = |id: u64, name: &str, icon: Option<&str>, chans: &[(u64, &str)]| {
        let roles = json!([{ "id": id.to_string(), "name": "@everyone", "color": 0, "position": 0, "permissions": "3072" }]);
        let chans: Vec<Value> = chans.iter().enumerate().map(|(i, (cid, n))| ch(*cid, 0, n, i as i32, None)).collect();
        json!({ "id": id.to_string(), "name": name, "icon": icon, "owner_id": "102", "roles": roles, "emojis": [], "channels": chans })
    };
    let mut guilds = vec![g1, simple(1001, "Gamers United", None, &[(2100, "lobby"), (2101, "lfg")]),
                          simple(1002, "Design Club", Some("icon1002"), &[(2200, "showcase"), (2201, "feedback")])];
    guilds.extend(side_guilds());
    let mut reads = vec![
        json!({ "id": GENERAL.to_string(), "last_message_id": snow(now_ms() - 20 * 95_000).to_string(), "mention_count": 1 }),
        json!({ "id": "2012", "last_message_id": "0", "mention_count": 2 }),
        json!({ "id": "3000", "last_message_id": "0", "mention_count": 0 }),
    ];
    reads.extend(side_reads());
    json!({
        "v": 9, "session_id": "mock-session", "resume_gateway_url": null,
        "user": { "id": ME.to_string(), "username": "you", "global_name": "You", "avatar": "av100", "email": "you@example.com" },
        "users": [user(101), user(102), user(103), user(104), user(105), user(106)],
        "relationships": [{ "id": "101", "type": 1 }, { "id": "102", "type": 1 }, { "id": "103", "type": 1 }, { "id": "106", "type": 3 }],
        "guilds": guilds,
        "merged_members": [[{ "user_id": ME.to_string(), "roles": ["1101"] }], [], [], [{ "user_id": ME.to_string(), "roles": ["1201"] }], [], [], [], [], []],
        "private_channels": [
            { "id": "3000", "type": 1, "recipient_ids": ["101"], "last_message_id": snow(now_ms() - 3_600_000).to_string() },
            { "id": "3001", "type": 3, "name": "", "recipient_ids": ["102", "103"], "last_message_id": snow(now_ms() - 7_200_000).to_string() },
        ],
        "read_state": { "entries": reads, "partial": false, "version": 1 },
        "user_guild_settings": { "entries": [
            { "guild_id": "1004", "channel_overrides": [{ "channel_id": "2400", "muted": true }] },
            { "guild_id": "1005", "channel_overrides": [{ "channel_id": "2500", "muted": true }] },
            { "guild_id": "1006", "message_notifications": 1, "channel_overrides": [] },
        ] },
        "user_settings": { "guild_folders": [{ "guild_ids": ["1000"] }, { "id": 77, "name": "Hobbies", "color": 0xE91E63, "guild_ids": ["1001", "1002"] },
                                          { "id": 78, "name": "Pals", "guild_ids": ["1003", "1004", "1005", "1006"] }, { "guild_ids": ["1007"] }, { "guild_ids": ["1008"] }] },
    })
}

fn member_list() -> Value {
    let item = |id: u64, status: &str| json!({ "member": { "user": user(id), "roles": member(id)["roles"], "nick": null, "presence": { "status": status } } });
    json!({
        "guild_id": "1000", "id": "everyone", "member_count": 6, "online_count": 4,
        "groups": [{ "id": "1100", "count": 1 }, { "id": "online", "count": 3 }, { "id": "offline", "count": 2 }],
        "ops": [{ "op": "SYNC", "range": [0, 99], "items": [
            { "group": { "id": "1100", "count": 1 } }, item(101, "online"),
            { "group": { "id": "online", "count": 3 } }, item(102, "idle"), item(103, "dnd"), item(100, "online"),
            { "group": { "id": "offline", "count": 2 } }, item(104, "offline"), item(105, "offline"),
        ]}]
    })
}

fn png(w: u32, h: u32, seed: u64, round: bool) -> Vec<u8> {
    let hues = [(0x58, 0x65, 0xf2), (0x3b, 0xa5, 0x5c), (0xfa, 0xa6, 0x1a), (0xed, 0x42, 0x45), (0xeb, 0x45, 0x9e), (0x1a, 0xbc, 0x9c)];
    let (r, g, b) = hues[(seed % hues.len() as u64) as usize];
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        if round {
            let d = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt();
            if d > 0.5 {
                return image::Rgba([0, 0, 0, 0]);
            }
            let eye = ((fx - 0.35).powi(2) + (fy - 0.4).powi(2)).sqrt() < 0.07 || ((fx - 0.65).powi(2) + (fy - 0.4).powi(2)).sqrt() < 0.07;
            return if eye { image::Rgba([60, 40, 10, 255]) } else { image::Rgba([0xff, 0xcc, 0x4d, 255]) };
        }
        let t = (fx + fy) / 2.0;
        let f = |c: u8| ((c as f32) * (0.65 + 0.35 * t)) as u8;
        let stripe = ((fx * 6.0).floor() as u32 + (fy * 6.0).floor() as u32) % 2 == 0;
        let k = if stripe { 1.0 } else { 0.9 };
        image::Rgba([(f(r) as f32 * k) as u8, (f(g) as f32 * k) as u8, (f(b) as f32 * k) as u8, 255])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn query(path: &str, key: &str) -> Option<u64> {
    let q = path.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}=")).and_then(|v| v.parse().ok()))
}

async fn route(w: &W, method: &str, path: &str, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
    let ok = |v: Value| (200, "application/json", v.to_string().into_bytes());
    let clean = path.split('?').next().unwrap_or("");
    let parts: Vec<&str> = clean.trim_matches('/').split('/').collect();
    match (method, parts.as_slice()) {
        ("GET", ["api", "v9", "channels", ch, "messages"]) => {
            let ch: u64 = ch.parse().unwrap_or(0);
            let id = |m: &Value| m["id"].as_str().unwrap().parse::<u64>().unwrap();
            let w = w.lock().unwrap();
            let all: Vec<&Value> = w.msgs.iter().filter(|m| m["channel_id"] == ch.to_string()).collect();
            let mut list: Vec<&Value> = if let Some(a) = query(path, "around") {
                let at = all.iter().position(|m| id(m) >= a).unwrap_or(all.len());
                all[at.saturating_sub(25)..(at + 25).min(all.len())].to_vec()
            } else if let Some(a) = query(path, "after") {
                all.into_iter().filter(|m| id(m) > a).take(50).collect()
            } else {
                let before = query(path, "before");
                let mut l: Vec<&Value> = all.into_iter().filter(|m| before.is_none_or(|b| id(m) < b)).collect();
                let n = l.len();
                l.split_off(n.saturating_sub(50))
            };
            list.reverse();
            ok(json!(list))
        }
        ("POST", ["api", "v9", "interactions"]) => {
            let b = payload_json(body);
            let ch: u64 = b["channel_id"].as_str().unwrap_or("0").parse().unwrap_or(0);
            let text = if b["type"] == json!(2) {
                format!("⏰ Reminder set: **{}**", b["data"]["options"][0]["value"].as_str().unwrap_or("?"))
            } else {
                "😴 Snoozed for 10 minutes.".to_string()
            };
            let mut m = msg(snow(now_ms()), ch, 104, &text);
            m["type"] = json!(20);
            m["application_id"] = json!("104");
            if b["type"] == json!(2) {
                m["components"] = bot_buttons();
            }
            let tx = {
                let mut w = w.lock().unwrap();
                w.msgs.push(m.clone());
                w.events.clone()
            };
            let _ = tx.send(json!({ "t": "MESSAGE_CREATE", "d": m }));
            (204, "application/json", Vec::new())
        }
        ("GET", ["api", "v9", "gifs", _]) => {
            let gifs: Vec<Value> = (0..8)
                .map(|i| json!({ "id": i.to_string(), "title": format!("mock {i}"), "url": format!("https://tenor.com/view/mock-{i}"),
                                 "src": format!("http://127.0.0.1:{}/cdn/attachments/gif{i}.png", PORT.load(std::sync::atomic::Ordering::Relaxed)),
                                 "width": 220 + (i % 3) * 40, "height": 160 + (i % 2) * 60 }))
                .collect();
            ok(json!(gifs))
        }
        ("GET", ["api", "v9", "guilds", _, "application-command-index"]) => ok(json!({
            "applications": [{ "id": "104", "name": "Dyno", "icon": null }],
            "application_commands": [
                { "id": "9001", "application_id": "104", "version": "1", "type": 1, "name": "remind", "description": "Set a reminder",
                  "options": [{ "type": 3, "name": "what", "description": "What to remind you about", "required": true },
                              { "type": 4, "name": "minutes", "description": "In how many minutes", "required": false }] },
                { "id": "9002", "application_id": "104", "version": "1", "type": 1, "name": "poll", "description": "Start a poll",
                  "options": [{ "type": 3, "name": "question", "description": "What to ask", "required": true }] },
            ],
        })),
        ("POST", ["api", "v9", "channels", ch, "messages"]) => {
            let ch: u64 = ch.parse().unwrap_or(0);
            let b: Value = serde_json::from_slice(body).unwrap_or_else(|_| payload_json(body));
            let mut m = msg(snow(now_ms()), ch, ME, b["content"].as_str().unwrap_or(""));
            m["nonce"] = b["nonce"].clone();
            m["member"] = member(ME);
            let tx = {
                let mut w = w.lock().unwrap();
                w.msgs.push(m.clone());
                w.events.clone()
            };
            let _ = tx.send(json!({ "t": "MESSAGE_CREATE", "d": m.clone() }));
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let _ = tx.send(json!({ "t": "TYPING_START", "d": { "channel_id": ch.to_string(), "user_id": "101", "guild_id": "1000" } }));
                // a dm from someone else while you're busy elsewhere, so notifications have something to do
                tokio::time::sleep(Duration::from_millis(600)).await;
                let _ = tx.send(json!({ "t": "MESSAGE_CREATE", "d": msg(snow(now_ms()), 3000, 101, "hey, saw your message — got a sec?") }));
            });
            ok(m)
        }
        ("POST", ["api", "v9", "channels", _, "typing"]) => (204, "application/json", Vec::new()),
        ("POST", ["api", "v9", "channels", _, "messages", _, "ack"]) => ok(json!({ "token": null })),
        ("GET", ["api", "v9", "channels", _, "threads", "search"]) => ok(forum_posts()),
        ("GET", ["api", "v9", "channels", ch, "pins"]) => {
            let w = w.lock().unwrap();
            let list: Vec<&Value> = w.msgs.iter().filter(|m| m["channel_id"] == *ch && m["pinned"] == json!(true)).rev().collect();
            ok(json!(list))
        }
        ("GET", ["api", "v9", "guilds", _, "messages", "search"]) => {
            let q = path.split("content=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_lowercase();
            let w = w.lock().unwrap();
            let hits: Vec<Value> = w
                .msgs
                .iter()
                .rev()
                .filter(|m| !q.is_empty() && m["content"].as_str().unwrap_or("").to_lowercase().contains(&q))
                .take(25)
                .map(|m| json!([m]))
                .collect();
            ok(json!({ "total_results": hits.len(), "messages": hits }))
        }
        ("GET", ["api", "v9", "users", id, "profile"]) => {
            let id: u64 = id.parse().unwrap_or(0);
            ok(json!({ "user": user(id), "user_profile": { "bio": "rustacean. writes too many side projects.", "accent_color": 0xE67E22, "pronouns": "" } }))
        }
        // settings pages
        ("GET", ["api", "v9", "users", "@me", "settings"]) => ok(json!({
            "explicit_content_filter": 1, "default_guilds_restricted": false, "show_current_game": true,
            "allow_accessibility_detection": false, "friend_source_flags": { "all": true },
        })),
        ("PATCH", ["api", "v9", "users", "@me", "settings"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "users", "@me", "profile"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "users", "@me"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let mut u = user(ME);
            if let Some(n) = b["global_name"].as_str() {
                u["global_name"] = json!(n);
            }
            ok(u)
        }
        ("GET", ["api", "v9", "users", "@me", "consent"]) => ok(json!({ "usage_statistics": { "consented": true }, "personalization": { "consented": false } })),
        ("GET", ["api", "v9", "auth", "sessions"]) => ok(json!({ "user_sessions": [
            { "id_hash": "aaa", "approx_last_used_time": "2026-10-07T10:00:00+00:00", "client_info": { "os": "Windows", "platform": "Discord Client", "location": "Bangkok, Thailand" } },
            { "id_hash": "bbb", "approx_last_used_time": "2026-10-01T10:00:00+00:00", "client_info": { "os": "Android", "platform": "Discord Android", "location": "Bangkok, Thailand" } },
        ] })),
        ("GET", ["api", "v9", "oauth2", "tokens"]) => ok(json!([
            { "id": "77", "scopes": ["identify", "guilds"], "application": { "id": "104", "name": "Dyno", "icon": null, "description": "A multipurpose bot." } },
        ])),
        ("GET", ["api", "v9", "users", "@me", "connections"]) => ok(json!([
            { "type": "github", "id": "1", "name": "you-on-github", "visibility": 1, "show_activity": true, "verified": true },
        ])),
        ("GET", ["api", "v9", "users", "@me", "billing", ..]) | ("GET", ["api", "v9", "users", "@me", "entitlements", ..]) => ok(json!([])),
        (_, ["api", ..]) => ok(json!({})),
        ("GET", ["cdn", "emojis", f]) if f.ends_with(".gif") => (200, "image/gif", anim_gif(48, 48, 0)),
        ("GET", ["cdn", "media", f]) if f.ends_with(".mp4") => (200, "video/mp4", include_bytes!("fixtures/spin.mp4").to_vec()),
        ("GET", ["cdn", "media", f]) if f.ends_with(".gif") => (200, "image/gif", anim_gif(if f.starts_with("dance") { 120 } else { 96 }, if f.starts_with("dance") { 90 } else { 72 }, f.len() as u32)),
        ("GET", ["cdn", "media", ..]) => (200, "image/png", png(160, 120, 3, false)),
        ("GET", ["cdn", "attachments", ..]) => (200, "image/png", png(800, 450, 2, false)),
        ("GET", ["cdn", "emoji", ..]) | ("GET", ["cdn", "emojis", ..]) => (200, "image/png", png(72, 72, 0, true)),
        ("GET", ["cdn", kind, id, ..]) => {
            let size = query(path, "size").unwrap_or(128).clamp(16, 256) as u32;
            let seed: u64 = id.bytes().map(|b| b as u64).sum::<u64>() + kind.len() as u64;
            (200, "image/png", png(size, size, seed, false))
        }
        _ => (404, "text/plain", b"not found".to_vec()),
    }
}

async fn http_conn(mut s: TcpStream, w: W) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        let head_end = loop {
            if let Some(p) = buf.windows(4).position(|x| x == b"\r\n\r\n") {
                break p + 4;
            }
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let mut lines = head.lines();
        let mut first = lines.next().unwrap_or("").split_whitespace();
        let method = first.next().unwrap_or("").to_string();
        let path = first.next().unwrap_or("").to_string();
        let headers: Vec<(String, String)> =
            lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
        let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let locale = header("x-discord-locale").unwrap_or_default();
        while buf.len() < head_end + len {
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        }
        let body = buf[head_end..head_end + len].to_vec();
        buf.drain(..head_end + len);

        {
            let mut wl = w.lock().unwrap();
            let line = json!({ "t": now_ms(), "method": method, "path": path, "body": String::from_utf8_lossy(&body), "locale": locale });
            let _ = writeln!(wl.log, "{line}");
        }
        let (code, ctype, out) = route(&w, &method, &path, &body).await;
        let reason = match code {
            200 => "OK",
            204 => "No Content",
            _ => "Not Found",
        };
        let head = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n", out.len());
        if s.write_all(head.as_bytes()).await.is_err() || s.write_all(&out).await.is_err() {
            return;
        }
    }
}

async fn ws_conn(s: TcpStream, w: W) {
    let Ok(ws) = tokio_tungstenite::accept_async(s).await else { return };
    let (mut tx, mut rx) = ws.split();
    let mut events = w.lock().unwrap().events.subscribe();
    let mut seq = 0u64;
    let _ = tx.send(Ws::text(json!({ "op": 10, "d": { "heartbeat_interval": 41250 } }).to_string())).await;
    loop {
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(Ws::Text(t))) = m else {
                    if matches!(m, Some(Ok(_))) { continue }
                    return;
                };
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                {
                    let mut wl = w.lock().unwrap();
                    let mut line = json!({ "t": now_ms(), "gateway_op": v["op"] });
                    if matches!(v["op"].as_u64(), Some(4 | 18..=22)) {
                        line["d"] = v["d"].clone();
                    }
                    let _ = writeln!(wl.log, "{line}");
                }
                let out: Vec<(String, Value)> = match v["op"].as_u64() {
                    Some(1) => {
                        let _ = tx.send(Ws::text(json!({ "op": 11 }).to_string())).await;
                        vec![]
                    }
                    Some(2) => {
                        let last = w.lock().unwrap().msgs.last().map(|m| m["id"].as_str().unwrap().parse::<u64>().unwrap()).unwrap_or(0);
                        vec![
                            ("READY".into(), ready(last)),
                            ("READY_SUPPLEMENTAL".into(), json!({ "merged_presences": { "friends": [
                                { "user_id": "101", "status": "online" }, { "user_id": "102", "status": "idle" }, { "user_id": "103", "status": "dnd" }
                            ], "guilds": [] }, "guilds": [] })),
                        ]
                    }
                    Some(6) => vec![("RESUMED".into(), json!({}))],
                    Some(4) => voice_state(&w, &v["d"]),
                    Some(op @ 18..=22) => stream_op(&w, op, &v["d"]),
                    Some(37) => vec![("GUILD_MEMBER_LIST_UPDATE".into(), member_list())],
                    Some(8) => {
                        let members: Vec<Value> = v["d"]["user_ids"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|id| id.as_str()?.parse::<u64>().ok())
                            .map(|id| { let mut m = member(id); m["user"] = user(id); m })
                            .collect();
                        vec![("GUILD_MEMBERS_CHUNK".into(), json!({ "guild_id": "1000", "members": members, "chunk_index": 0, "chunk_count": 1 }))]
                    }
                    _ => vec![],
                };
                for (t, d) in out {
                    seq += 1;
                    if tx.send(Ws::text(json!({ "op": 0, "t": t, "s": seq, "d": d }).to_string())).await.is_err() {
                        return;
                    }
                }
            }
            e = events.recv() => {
                let Ok(e) = e else { continue };
                seq += 1;
                if tx.send(Ws::text(json!({ "op": 0, "t": e["t"], "s": seq, "d": e["d"] }).to_string())).await.is_err() {
                    return;
                }
            }
        }
    }
}

// op 4 on the main gateway: echo our voice state, and hand out the voice server on a channel change
fn voice_state(w: &W, d: &Value) -> Vec<(String, Value)> {
    let guild = d["guild_id"].as_str().and_then(|g| g.parse::<u64>().ok());
    let chan = d["channel_id"].as_str().and_then(|c| c.parse::<u64>().ok());
    let mut wl = w.lock().unwrap();
    let moved = chan.map(|c| (guild, c)) != wl.voice_chan;
    wl.voice_chan = chan.map(|c| (guild, c));
    let mut m = member(ME);
    m["user"] = user(ME);
    let me = json!({
        "guild_id": guild.map(|g| g.to_string()), "channel_id": chan.map(|c| c.to_string()), "user_id": ME.to_string(),
        "session_id": "mock-voice-session", "self_mute": d["self_mute"], "self_deaf": d["self_deaf"], "self_video": d["self_video"],
        "mute": false, "deaf": false, "suppress": false, "member": m,
    });
    let mut out = vec![("VOICE_STATE_UPDATE".to_string(), me)];
    let Some(ch) = chan else { return out };
    if !moved {
        return out;
    }
    let endpoint = format!("ws://127.0.0.1:{}", wl.voice_port);
    // Alice is always on camera and streaming her screen, so there's something to watch
    let mut alice = json!({ "guild_id": guild.map(|g| g.to_string()), "channel_id": ch.to_string(), "user_id": SPEAKER.to_string(),
        "session_id": "other", "self_mute": false, "self_deaf": false, "self_video": true, "self_stream": true });
    match guild {
        Some(g) => {
            let mut am = member(SPEAKER);
            am["user"] = user(SPEAKER);
            alice["member"] = am;
            out.push(("VOICE_SERVER_UPDATE".into(), json!({ "token": "mock-voice-token", "guild_id": g.to_string(), "endpoint": endpoint })));
        }
        None => {
            // a DM call: Alice picks up right away
            out.push(("CALL_CREATE".into(), json!({ "channel_id": ch.to_string(), "message_id": "1", "region": "local",
                "ringing": [], "voice_states": [{ "user_id": ME.to_string(), "channel_id": ch.to_string(), "session_id": "mock-voice-session" }] })));
            out.push(("VOICE_SERVER_UPDATE".into(), json!({ "token": "mock-voice-token", "guild_id": null, "channel_id": ch.to_string(), "endpoint": endpoint })));
        }
    }
    out.push(("VOICE_STATE_UPDATE".into(), alice));
    out
}

// Go Live on the main gateway: ours gets rtc server 5000, Alice's 5001
fn stream_op(w: &W, op: u64, d: &Value) -> Vec<(String, Value)> {
    let (guild, ch) = w.lock().unwrap().voice_chan.unwrap_or((None, 0));
    let prefix = match guild {
        Some(g) => format!("guild:{g}:{ch}"),
        None => format!("call:{ch}"),
    };
    let (key, rtc) = match op {
        18 => (format!("{prefix}:{ME}"), OWN_STREAM),
        20 => (d["stream_key"].as_str().unwrap_or("").to_string(), ALICE_STREAM),
        19 => return vec![("STREAM_DELETE".into(), json!({ "stream_key": d["stream_key"], "reason": "user_requested", "unavailable": false }))],
        _ => return vec![],
    };
    let endpoint = format!("ws://127.0.0.1:{}", w.lock().unwrap().voice_port);
    vec![
        ("STREAM_CREATE".into(), json!({ "stream_key": key, "rtc_server_id": rtc.to_string(), "viewer_ids": [], "region": "local", "paused": false })),
        ("STREAM_SERVER_UPDATE".into(), json!({ "stream_key": key, "endpoint": endpoint, "token": "mock-stream-token", "guild_id": null })),
    ]
}

fn log(w: &W, line: Value) {
    let mut wl = w.lock().unwrap();
    let _ = writeln!(wl.log, "{line}");
}

fn seal(mode: &str, key: &[u8; 32], header: &[u8], body: &[u8], nonce: u32) -> Vec<u8> {
    let mut n = [0u8; 24];
    n[..4].copy_from_slice(&nonce.to_be_bytes());
    let p = Payload { msg: body, aad: header };
    let ct = if mode == "aead_aes256_gcm_rtpsize" {
        Aes256Gcm::new_from_slice(key).unwrap().encrypt(n[..12].into(), p).unwrap()
    } else {
        XChaCha20Poly1305::new_from_slice(key).unwrap().encrypt(&n.into(), p).unwrap()
    };
    [header, &ct, &nonce.to_be_bytes()].concat()
}

fn open_rtp(mode: &str, key: &[u8; 32], pkt: &[u8]) -> Option<Vec<u8>> {
    let cc = (pkt[0] & 0x0f) as usize;
    let ext = pkt[0] & 0x10 != 0;
    let hl = 12 + cc * 4 + if ext { 4 } else { 0 };
    if pkt.len() < hl + 20 {
        return None;
    }
    let mut n = [0u8; 24];
    n[..4].copy_from_slice(&pkt[pkt.len() - 4..]);
    let p = Payload { msg: &pkt[hl..pkt.len() - 4], aad: &pkt[..hl] };
    let mut plain = if mode == "aead_aes256_gcm_rtpsize" {
        Aes256Gcm::new_from_slice(key).ok()?.decrypt(n[..12].into(), p).ok()?
    } else {
        XChaCha20Poly1305::new_from_slice(key).ok()?.decrypt(&n.into(), p).ok()?
    };
    if ext {
        let words = u16::from_be_bytes([pkt[hl - 2], pkt[hl - 1]]) as usize;
        plain.drain(..(words * 4).min(plain.len()));
    }
    Some(plain)
}

// Independent of the client's code on purpose: H264 RTP packetizing and reassembly
fn split_nals(frame: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= frame.len() {
        if frame[i..i + 3] == [0, 0, 1] {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (k, &s) in starts.iter().enumerate() {
        let mut e = starts.get(k + 1).map(|n| n - 3).unwrap_or(frame.len());
        while e > s && frame[e - 1] == 0 && k + 1 < starts.len() {
            e -= 1;
        }
        out.push(&frame[s..e]);
    }
    out
}

fn h264_payloads(frame: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for nal in split_nals(frame) {
        if nal.len() <= 1000 {
            out.push(nal.to_vec());
            continue;
        }
        let parts: Vec<&[u8]> = nal[1..].chunks(1000).collect();
        for (j, p) in parts.iter().enumerate() {
            let mut fu = vec![(nal[0] & 0xe0) | 28, nal[0] & 0x1f];
            if j == 0 {
                fu[1] |= 0x80;
            }
            if j + 1 == parts.len() {
                fu[1] |= 0x40;
            }
            fu.extend_from_slice(p);
            out.push(fu);
        }
    }
    out
}

#[derive(Default)]
struct Asm {
    buf: Vec<u8>,
    ts: u32,
}

impl Asm {
    fn push(&mut self, ts: u32, marker: bool, p: &[u8]) -> Option<Vec<u8>> {
        if ts != self.ts {
            self.buf.clear();
            self.ts = ts;
        }
        match p.first()? & 0x1f {
            1..=23 => {
                self.buf.extend_from_slice(&[0, 0, 0, 1]);
                self.buf.extend_from_slice(p);
            }
            28 if p.len() > 2 => {
                if p[1] & 0x80 != 0 {
                    self.buf.extend_from_slice(&[0, 0, 0, 1, (p[0] & 0xe0) | (p[1] & 0x1f)]);
                }
                self.buf.extend_from_slice(&p[2..]);
            }
            _ => {}
        }
        marker.then(|| std::mem::take(&mut self.buf))
    }
}

enum Reply {
    Json(Value),
    Bin(u8, Vec<u8>),
}

async fn voice_conn(s: TcpStream, w: W, udp: Arc<UdpSocket>) {
    let Ok(ws) = tokio_tungstenite::accept_async(s).await else { return };
    let (mut tx, mut rx) = ws.split();
    let mut seq = 0u16;
    let live = Arc::new(AtomicBool::new(true));
    let gw = Gw::new();
    let (mut channel, mut dm, mut kind, mut cssrc) = (0u64, false, Kind::Voice, CLIENT_SSRC);
    let _ = tx.send(Ws::text(json!({ "op": 8, "d": { "heartbeat_interval": 1000.0 } }).to_string())).await;
    while let Some(Ok(m)) = rx.next().await {
        let mut out: Vec<Reply> = Vec::new();
        let line = match m {
            Ws::Binary(b) if !b.is_empty() => dave_bin(&w, &gw, cssrc, &b, channel, dm, &mut out),
            Ws::Text(t) => {
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                let d = &v["d"];
                let mut line = json!({ "t": now_ms(), "voice_op": v["op"] });
                match v["op"].as_u64() {
                    Some(0) => {
                        line["d"] = d.clone();
                        let server: u64 = d["server_id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                        (kind, cssrc) = match server {
                            OWN_STREAM => (Kind::Own, CLIENT_SSRC + 100),
                            ALICE_STREAM => (Kind::Watch, CLIENT_SSRC + 200),
                            _ => (Kind::Voice, CLIENT_SSRC),
                        };
                        let (g, ch) = w.lock().unwrap().voice_chan.unwrap_or((None, 0));
                        // stream groups are keyed one below their rtc server id
                        (channel, dm) = if kind == Kind::Voice { (ch, g.is_none()) } else { (server - 1, false) };
                        out.push(Reply::Json(json!({ "op": 2, "d": { "ssrc": cssrc, "ip": "127.0.0.1", "port": w.lock().unwrap().voice_port,
                            "modes": ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"], "heartbeat_interval": 1,
                            "streams": [{ "type": "video", "rid": "100", "ssrc": cssrc + 1, "rtx_ssrc": cssrc + 2, "active": false, "quality": 100 }] } })));
                    }
                    Some(1) => {
                        line["d"] = d.clone();
                        let mode = d["data"]["mode"].as_str().unwrap_or("").to_string();
                        let mut key = [0u8; 32];
                        let mut x = now_ms() | cssrc as u64;
                        for b in key.iter_mut() {
                            x ^= x << 13;
                            x ^= x >> 7;
                            x ^= x << 17;
                            *b = x as u8;
                        }
                        let mut alice = DaveSession::new(NonZeroU16::new(1).unwrap(), SPEAKER, channel, None).unwrap();
                        alice.set_external_sender(&gw.sender).unwrap();
                        {
                            let mut wl = w.lock().unwrap();
                            let client = format!("{}:{}", d["data"]["address"].as_str().unwrap_or(""), d["data"]["port"]).parse().ok();
                            for old in wl.voices.iter().filter(|v| v.cssrc == cssrc) {
                                old.live.store(false, Ordering::Relaxed);
                            }
                            wl.voices.retain(|v| v.cssrc != cssrc);
                            wl.voices.push(Voice {
                                kind,
                                cssrc,
                                mode: mode.clone(),
                                key,
                                client,
                                dec: opus_rs::OpusDecoder::new(48_000, 2).unwrap(),
                                vdec: openh264::decoder::Decoder::new().unwrap(),
                                asm: Asm::default(),
                                key_frame: Arc::new(AtomicBool::new(true)),
                                live: live.clone(),
                                alice,
                                e2ee: false,
                            });
                        }
                        out.push(Reply::Json(json!({ "op": 4, "d": { "mode": mode, "secret_key": key.to_vec(), "dave_protocol_version": 1,
                            "audio_codec": "opus", "video_codec": "H264", "media_session_id": "mock-media" } })));
                        out.push(Reply::Json(json!({ "op": 11, "d": { "user_ids": [SPEAKER.to_string()] } })));
                        let (a, v) = match kind {
                            Kind::Voice => (SPEAKER_SSRC, SPEAKER_SSRC + 1),
                            _ => (SPEAKER_SSRC + 100, SPEAKER_SSRC + 101),
                        };
                        out.push(Reply::Json(json!({ "op": 5, "d": { "user_id": SPEAKER.to_string(), "ssrc": a, "speaking": 1 } })));
                        if kind != Kind::Own {
                            out.push(Reply::Json(json!({ "op": 12, "d": { "user_id": SPEAKER.to_string(), "audio_ssrc": a, "video_ssrc": v, "rtx_ssrc": v + 1,
                                "streams": [{ "type": "video", "rid": "100", "ssrc": v, "rtx_ssrc": v + 1, "active": true, "quality": 100 }] } })));
                        }
                        out.push(Reply::Bin(25, gw.sender.clone()));
                        tokio::spawn(speaker(w.clone(), udp.clone(), live.clone(), cssrc));
                    }
                    Some(3) => {
                        line["seq_ack"] = d["seq_ack"].clone();
                        let _ = tx.send(Ws::text(json!({ "op": 6, "d": { "t": d["t"] } }).to_string())).await;
                    }
                    Some(5) => line["speaking"] = d["speaking"].clone(),
                    Some(7) => out.push(Reply::Json(json!({ "op": 9, "d": null }))),
                    Some(12) | Some(15) | Some(31) => line["d"] = d.clone(),
                    Some(23) => {
                        // everyone is ready, so media switches to E2EE now
                        line["d"] = d.clone();
                        out.push(Reply::Json(json!({ "op": 22, "d": { "transition_id": d["transition_id"] } })));
                        if let Some(v) = w.lock().unwrap().voices.iter_mut().find(|v| v.cssrc == cssrc) {
                            v.e2ee = v.alice.is_ready();
                            line["alice_ready"] = json!(v.e2ee);
                        }
                    }
                    _ => {}
                }
                line["server"] = json!(kind.name());
                line
            }
            Ws::Close(_) => break,
            _ => continue,
        };
        log(&w, line);
        for r in out {
            seq = seq.wrapping_add(1);
            let m = match r {
                Reply::Json(mut o) => {
                    o["seq"] = json!(seq);
                    Ws::text(o.to_string())
                }
                Reply::Bin(op, body) => Ws::binary([&seq.to_be_bytes()[..], &[op], &body].concat()),
            };
            if tx.send(m).await.is_err() {
                break;
            }
        }
    }
    live.store(false, Ordering::Relaxed);
    // forget the session, or a client socket that reuses its port would be matched to it
    w.lock().unwrap().voices.retain(|v| !Arc::ptr_eq(&v.live, &live));
    log(&w, json!({ "t": now_ms(), "voice_closed": kind.name() }));
}

// The two ways into an MLS group: in a guild channel the client commits Alice's add and
// welcomes her; in a DM call Alice commits and the client joins from her welcome.
fn dave_bin(w: &W, gw: &Gw, cssrc: u32, b: &[u8], channel: u64, dm: bool, out: &mut Vec<Reply>) -> Value {
    let mut line = json!({ "t": now_ms(), "voice_bin": b[0] });
    let mut wl = w.lock().unwrap();
    let Some(v) = wl.voices.iter_mut().find(|v| v.cssrc == cssrc) else { return line };
    line["server"] = json!(v.kind.name());
    match b[0] {
        26 if dm => {
            let cw = gw
                .propose_add(&b[1..], channel)
                .and_then(|p| v.alice.process_proposals(ProposalsOperationType::APPEND, &p, Some(&[ME])).ok().flatten());
            if let Some(cw) = cw {
                line["alice_committed"] = json!(v.alice.process_commit(&cw.commit).is_ok());
                if let Some(wel) = cw.welcome {
                    out.push(Reply::Bin(30, [&1u16.to_be_bytes()[..], &wel].concat()));
                }
            }
        }
        26 => {
            let kp = v.alice.create_key_package().unwrap();
            if let Some(p) = gw.propose_add(&kp, channel) {
                out.push(Reply::Bin(27, [&[0u8][..], &p].concat()));
            }
        }
        28 => {
            let mut rest = &b[1..];
            let parsed = MlsMessageIn::tls_deserialize(&mut rest).is_ok();
            let commit = b[1..b.len() - rest.len()].to_vec();
            line["alice_welcomed"] = json!(parsed && v.alice.process_welcome(rest).is_ok());
            out.push(Reply::Bin(29, [&1u16.to_be_bytes()[..], &commit].concat()));
        }
        _ => {}
    }
    line
}

async fn voice_udp(w: W, udp: Arc<UdpSocket>) {
    let mut buf = [0u8; 2048];
    let mut pcm = vec![0f32; 1920];
    while let Ok((n, from)) = udp.recv_from(&mut buf).await {
        let pkt = &buf[..n];
        if n == 74 && pkt[1] == 1 {
            let mut r = [0u8; 74];
            r[0..2].copy_from_slice(&2u16.to_be_bytes());
            r[2..4].copy_from_slice(&70u16.to_be_bytes());
            r[4..8].copy_from_slice(&pkt[4..8]);
            r[8..17].copy_from_slice(b"127.0.0.1");
            r[72..74].copy_from_slice(&from.port().to_be_bytes());
            let _ = udp.send_to(&r, from).await;
            let ssrc = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
            if let Some(v) = w.lock().unwrap().voices.iter_mut().find(|v| v.cssrc == ssrc) {
                v.client = Some(from);
            }
            log(&w, json!({ "t": now_ms(), "voice_discovery": from.to_string() }));
            continue;
        }
        if n < 12 {
            continue;
        }
        let line = {
            let mut wl = w.lock().unwrap();
            let Some(v) = wl.voices.iter_mut().find(|v| v.client == Some(from)) else { continue };
            if (200..=206).contains(&pkt[1]) {
                // RTCP; a PLI from the client means Alice owes it a keyframe
                if pkt[1] == 206 && pkt[0] & 0x1f == 1 {
                    v.key_frame.store(true, Ordering::Relaxed);
                }
                json!({ "t": now_ms(), "rtcp": pkt[1], "fmt": pkt[0] & 0x1f, "server": v.kind.name() })
            } else if pkt[1] & 0x7f == 101 {
                let plain = open_rtp(&v.mode, &v.key, pkt);
                let ts = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
                let Some(frame) = plain.and_then(|p| v.asm.push(ts, pkt[1] & 0x80 != 0, &p)) else { continue };
                let (frame, e2ee) = match v.alice.decrypt(ME, MediaType::VIDEO, &frame) {
                    Ok(f) if v.e2ee => (f, true),
                    _ => (frame, false),
                };
                let dims = match v.vdec.decode(&frame) {
                    Ok(Some(yuv)) => Some(openh264::formats::YUVSource::dimensions(&yuv)),
                    _ => None,
                };
                json!({ "t": now_ms(), "video_frame": dims, "e2ee": e2ee, "bytes": frame.len(), "server": v.kind.name() })
            } else {
                let plain = open_rtp(&v.mode, &v.key, pkt);
                let silence = plain.as_deref() == Some(&[0xf8, 0xff, 0xfe][..]);
                // after the transition every real frame must be E2EE; silence is the one passthrough
                let mut e2ee = false;
                let opus = match plain.as_deref() {
                    Some(p) if !silence && v.e2ee => match v.alice.decrypt(ME, MediaType::AUDIO, p) {
                        Ok(o) => {
                            e2ee = true;
                            Some(o)
                        }
                        Err(_) => None,
                    },
                    Some(p) if !silence => Some(p.to_vec()),
                    _ => None,
                };
                let mut level = None;
                if let Some(o) = opus
                    && v.dec.decode(&o, 960, &mut pcm).is_ok()
                {
                    level = Some((pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt());
                }
                json!({ "t": now_ms(), "voice_rtp": u16::from_be_bytes([pkt[2], pkt[3]]), "ssrc": u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]),
                        "pt": pkt[1] & 0x7f, "decrypted": plain.is_some(), "silence": silence, "e2ee": e2ee, "level": level })
            }
        };
        log(&w, line);
    }
}

// Alice, once the E2EE group is up: a quiet 330Hz tone (1.2s on, 0.8s off) in voice calls,
// plus video at ~16fps: her camera in voice, her screen on her stream connection
async fn speaker(w: W, udp: Arc<UdpSocket>, live: Arc<AtomicBool>, cssrc: u32) {
    let mut enc = opus_rs::OpusEncoder::new(48_000, 2, opus_rs::Application::Audio).unwrap();
    let mut venc = openh264::encoder::Encoder::new().unwrap();
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    let (mut seq, mut vseq, mut ts, mut nonce, mut phase) = (0u16, 0u16, 0u32, 0u32, 0f32);
    let mut frame = vec![0f32; 1920];
    let mut out = [0u8; 1276];
    let (mut sent, mut vsent) = (0u32, 0u32);
    for i in 0u64.. {
        tick.tick().await;
        if !live.load(Ordering::Relaxed) {
            break;
        }
        ts = ts.wrapping_add(960);
        let (mode, key, to, kind, want_key) = {
            let wl = w.lock().unwrap();
            let Some(v) = wl.voices.iter().find(|v| v.cssrc == cssrc) else { break };
            if !v.e2ee || v.kind == Kind::Own {
                continue;
            }
            (v.mode.clone(), v.key, v.client, v.kind, v.key_frame.clone())
        };
        let Some(to) = to else { continue };
        let (assrc, vssrc) = if kind == Kind::Voice { (SPEAKER_SSRC, SPEAKER_SSRC + 1) } else { (SPEAKER_SSRC + 100, SPEAKER_SSRC + 101) };
        if i % 3 == 0 {
            let (wd, ht) = (320usize, 180usize);
            let shift = (i / 3) as usize * 4;
            let rgb: Vec<u8> = (0..wd * ht)
                .flat_map(|p| {
                    let (x, y) = (p % wd, p / wd);
                    let check = ((x + shift) / 20 + y / 20) % 2 == 0;
                    match (kind, check) {
                        (Kind::Voice, true) => [0xed, 0x42, 0x45],
                        (Kind::Voice, false) => [0x30, 0x10, 0x10],
                        (_, true) => [0xf2, 0xf3, 0xf5],
                        (_, false) => [0x30, 0x33, 0x38],
                    }
                })
                .collect();
            if want_key.swap(false, Ordering::Relaxed) {
                venc.force_intra_frame();
            }
            let yuv = openh264::formats::YUVBuffer::from_rgb_source(openh264::formats::RgbSliceU8::new(&rgb, (wd, ht)));
            let bits = venc.encode(&yuv).map(|b| b.to_vec()).unwrap_or_default();
            let enc_frame = {
                let mut wl = w.lock().unwrap();
                let Some(v) = wl.voices.iter_mut().find(|v| v.cssrc == cssrc) else { break };
                v.alice.encrypt(MediaType::VIDEO, davey::Codec::H264, &bits).map(|c| c.into_owned()).unwrap_or_default()
            };
            let vts = (i as u32).wrapping_mul(1800);
            let parts = h264_payloads(&enc_frame);
            let n = parts.len();
            for (k, p) in parts.into_iter().enumerate() {
                let mut h = vec![0x90, 101 | if k + 1 == n { 0x80 } else { 0 }];
                h.extend_from_slice(&vseq.to_be_bytes());
                h.extend_from_slice(&vts.to_be_bytes());
                h.extend_from_slice(&vssrc.to_be_bytes());
                h.extend_from_slice(&[0xbe, 0xde, 0x00, 0x01]);
                let body = [&[0x51, 0x00, 0x00, 0x00][..], &p].concat();
                nonce = nonce.wrapping_add(1);
                let _ = udp.send_to(&seal(&mode, &key, &h, &body, nonce), to).await;
                vseq = vseq.wrapping_add(1);
            }
            vsent += 1;
        }
        if kind != Kind::Voice || i % 100 >= 60 {
            continue;
        }
        for k in 0..960 {
            let x = 0.06 * phase.sin();
            frame[k * 2] = x;
            frame[k * 2 + 1] = x;
            phase = (phase + std::f32::consts::TAU * 330.0 / 48_000.0) % std::f32::consts::TAU;
        }
        let Ok(n) = enc.encode(&frame, 960, &mut out) else { continue };
        let opus = {
            let mut wl = w.lock().unwrap();
            let Some(v) = wl.voices.iter_mut().find(|v| v.cssrc == cssrc) else { break };
            let Ok(opus) = v.alice.encrypt_opus(&out[..n]).map(|c| c.into_owned()) else { continue };
            opus
        };
        // one-element header extension (audio level), whose body is encrypted along with the opus data
        let mut h = vec![0x90, 0x78];
        h.extend_from_slice(&seq.to_be_bytes());
        h.extend_from_slice(&ts.to_be_bytes());
        h.extend_from_slice(&assrc.to_be_bytes());
        h.extend_from_slice(&[0xbe, 0xde, 0x00, 0x01]);
        let body = [&[0x10, 0x5a, 0x00, 0x00][..], &opus].concat();
        nonce = nonce.wrapping_add(1);
        let _ = udp.send_to(&seal(&mode, &key, &h, &body, nonce), to).await;
        seq = seq.wrapping_add(1);
        sent += 1;
    }
    log(&w, json!({ "t": now_ms(), "speaker_sent": sent, "video_sent": vsent }));
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let http_port: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(7701);
    let ws_port: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(7702);
    let log = args.get(3).cloned().unwrap_or_else(|| "mock_requests.jsonl".into());
    let voice_port: u16 = args.get(4).and_then(|p| p.parse().ok()).unwrap_or(ws_port + 1);
    let cdn = format!("http://127.0.0.1:{http_port}/cdn");
    PORT.store(http_port, std::sync::atomic::Ordering::Relaxed);
    let (events, _) = broadcast::channel(256);
    let w: W = Arc::new(Mutex::new(World {
        msgs: seed(&cdn),
        log: std::fs::File::create(&log).expect("log file"),
        events,
        voice_port,
        voice_chan: None,
        voices: Vec::new(),
    }));

    let http = TcpListener::bind(("127.0.0.1", http_port)).await.expect("bind http");
    let ws = TcpListener::bind(("127.0.0.1", ws_port)).await.expect("bind ws");
    let vws = TcpListener::bind(("127.0.0.1", voice_port)).await.expect("bind voice ws");
    let udp = Arc::new(UdpSocket::bind(("127.0.0.1", voice_port)).await.expect("bind voice udp"));
    println!("mockcord http={http_port} ws={ws_port} voice={voice_port}");
    tokio::spawn(voice_udp(w.clone(), udp.clone()));
    let w3 = w.clone();
    tokio::spawn(async move {
        while let Ok((s, _)) = vws.accept().await {
            tokio::spawn(voice_conn(s, w3.clone(), udp.clone()));
        }
    });
    let w2 = w.clone();
    tokio::spawn(async move {
        while let Ok((s, _)) = ws.accept().await {
            tokio::spawn(ws_conn(s, w2.clone()));
        }
    });
    while let Ok((s, _)) = http.accept().await {
        tokio::spawn(http_conn(s, w.clone()));
    }
}
