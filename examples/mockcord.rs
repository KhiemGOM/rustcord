//! Tiny fake Discord for the e2e harness: REST + CDN over plain HTTP, gateway over ws,
//! and a voice server (ws + udp on the same port number) with one fake speaker.
//! usage: mockcord <http_port> <ws_port> <request_log.jsonl> [voice_port] [remote_auth_port]

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

#[path = "mockcord/account.rs"]
mod account;
#[path = "mockcord_auth/mod.rs"]
mod auth;

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
    clip_flags: u64,
    voices: Vec<Voice>,
    // messages saved for later: bookmarks, and reminders with a due time
    saved: Vec<Value>,
    // channel settings saved over what READY says, and channels deleted since
    edits: serde_json::Map<String, Value>,
    gone: Vec<String>,
    speakers: std::collections::HashSet<u64>,
    hands: std::collections::HashMap<u64, String>,
    // running activities: channel, application, launch id, who has it open
    acts: Vec<(u64, u64, String, Vec<u64>)>,
    // applications the account has authorized
    authorized: std::collections::HashSet<u64>,
    fails: Vec<(String, String, u16, Value, u32)>,
}

const ACTIVITY_APPS: [(u64, &str, &str); 2] = [(9100, "Mock Quiz", "A quiz for the whole call"), (9101, "Mock Draw", "Draw something together")];

fn activity_guild(ch: u64) -> Option<u64> {
    match ch {
        2801 => Some(1008),
        2310 => Some(1003),
        _ => None,
    }
}

/// Tells the client how an activity stands: who's in it, none when it has ended.
fn activity_update(w: &mut World, ch: u64, app: u64) {
    let Some((_, _, launch, users)) = w.acts.iter().find(|a| a.0 == ch && a.1 == app) else { return };
    let participants: Vec<Value> = users.iter().map(|u| json!({ "user_id": u.to_string(), "session_id": "mock-session", "nonce": null })).collect();
    let d = json!({
        "application_id": app.to_string(),
        "launch_id": launch,
        "composite_instance_id": launch,
        "location": { "id": ch.to_string(), "kind": "gc", "channel_id": ch.to_string(), "guild_id": activity_guild(ch).map(|g| g.to_string()) },
        "guild_id": activity_guild(ch).map(|g| g.to_string()),
        "participants": participants,
    });
    let _ = w.events.send(json!({ "t": "EMBEDDED_ACTIVITY_UPDATE_V2", "d": d }));
}

fn activity_join(w: &mut World, ch: u64, app: u64, user: u64) {
    match w.acts.iter_mut().find(|a| a.0 == ch && a.1 == app) {
        Some(a) => {
            if !a.3.contains(&user) {
                a.3.push(user);
            }
        }
        None => w.acts.push((ch, app, format!("L{}", now_ms()), vec![user])),
    }
    activity_update(w, ch, app);
}

fn activity_page() -> Vec<u8> {
    include_bytes!("fixtures/activity.html").to_vec()
}

fn stage_guild(ch: u64) -> Option<u64> {
    match ch {
        2311 => Some(1003),
        2802 => Some(1008),
        _ => None,
    }
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

/// An id for a message made now, its increment bits keeping two in one millisecond apart.
fn fresh() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    snow(now_ms()) | (NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xfff)
}

fn iso(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).unwrap_or_default().to_rfc3339()
}

/// The activity feed: alice's top artist, bob playing right now, carol's wordle yesterday, the
/// holographic member's streak, and someone from outside the server who mustn't show.
fn feed() -> Value {
    let port = PORT.load(std::sync::atomic::Ordering::Relaxed);
    let now = now_ms();
    let entry = |author: u64, kind: u64, at: u64, traits: Value, extra: Value| {
        json!({ "content": { "id": snow(at).to_string(), "author_id": author.to_string(), "author_type": 1, "content_type": kind, "traits": traits, "extra": extra, "participants": [author.to_string()] } })
    };
    json!({ "entries": [
        { "content": { "id": "1478167121", "author_id": "101", "author_type": 1, "content_type": 6,
            "traits": [{ "type": 4, "range": 1 }, { "type": 12, "count": 8 }],
            "extra": { "type": "top_artist_extra", "artist": { "name": "Rocco" },
                       "media": { "image_url": format!("http://127.0.0.1:{port}/cdn/media/album.png") } } } },
        entry(102, 1, now - 754_000, json!([{ "type": 3, "is_live": true }]), json!({ "type": "played_game_extra", "game_name": "Factorio", "application_id": "9001" })),
        entry(103, 8, now - 86_400_000, json!([{ "type": 2, "duration_seconds": 120 }, { "type": 8, "streak_count_days": 4 }]),
              json!({ "type": "launched_activity_extra", "activity_name": "Wordle", "application_id": "9002" })),
        entry(999, 1, now - 3_600_000, json!([]), json!({ "type": "played_game_extra", "game_name": "Outsider Game", "application_id": "9003" })),
        entry(105, 1, now - 3_600_000, json!([{ "type": 2, "duration_seconds": 7415 }, { "type": 8, "streak_count_days": 14 }, { "type": 9, "trending": 1 }]),
              json!({ "type": "played_game_extra", "game_name": "Genshin Impact", "application_id": "9004" })),
        // a live game that has since ended doesn't get a card
        { "content": { "id": snow(now - 7_200_000).to_string(), "author_id": "101", "author_type": 1, "content_type": 1, "traits": [{ "type": 3, "is_live": true }],
            "extra": { "type": "played_game_extra", "game_name": "Expired", "application_id": "9001" }, "expires_at": iso(now - 60_000) } },
    ] })
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
    let mut u = json!({ "id": id.to_string(), "username": name, "global_name": global, "avatar": format!("av{id}"), "bot": bot, "discriminator": "0" });
    if id == 106 {
        u["display_name_styles"] = json!({ "font_id": 11, "effect_id": 6, "colors": [0xE1C2FB, 0xEDE9FE] });
    }
    // a name font with no colours: Medieval
    if id == 103 {
        u["display_name_styles"] = json!({ "font_id": 7, "effect_id": 1, "colors": [] });
    }
    u
}

fn member(id: u64) -> Value {
    let roles: Vec<String> = match id {
        101 => vec!["1100".into()],
        102 | 100 | 106 => vec!["1101".into()],
        103 => vec!["1104".into()],
        104 => vec!["1102".into()],
        105 => vec!["1103".into()],
        _ => vec![],
    };
    // 102 boosts the server, for the member list's gem
    let since = if id == 102 { json!("2024-03-03T10:00:00+00:00") } else { Value::Null };
    json!({ "roles": roles, "nick": null, "premium_since": since })
}

fn msg(id: u64, ch: u64, author: u64, content: &str) -> Value {
    json!({
        "id": id.to_string(), "channel_id": ch.to_string(), "author": user(author),
        "content": content, "type": 0, "attachments": [], "embeds": [], "mentions": [], "mention_roles": [],
        "mention_everyone": false, "edited_timestamp": null
    })
}

fn fail_body(status: u16) -> Value {
    match status {
        400 => json!({ "code": 50035, "message": "Invalid Form Body", "errors": { "name": { "_errors": [{ "code": "BASE_TYPE_MAX_LENGTH", "message": "Must be 32 or fewer in length." }] } } }),
        401 => json!({ "message": "401: Unauthorized", "code": 0 }),
        403 => json!({ "message": "Missing Permissions", "code": 50013 }),
        404 => json!({ "message": "Unknown Message", "code": 10008 }),
        429 => json!({ "message": "You are being rate limited.", "retry_after": 30.0, "global": false }),
        _ => json!({}),
    }
}

const SYSTEM: u64 = 2070;


/// One message of every kind discord draws specially, in a channel of their own, with a few
/// ordinary ones to hang reactions and menus on.
fn system_msgs() -> Vec<Value> {
    let t0 = now_ms() - 3 * 3_600_000;
    let at = |i: u64| snow(t0 + i * 60_000);
    let mut out = Vec::new();
    let mut add = |kind: u64, author: u64, content: &str, f: &dyn Fn(&mut Value)| {
        let mut m = msg(at(out.len() as u64), SYSTEM, author, content);
        m["type"] = json!(kind);
        f(&mut m);
        out.push(m);
    };
    add(0, 102, "this one has reactions", &|m| {
        m["reactions"] = json!([
            { "emoji": { "id": null, "name": "🔥" }, "count": 3, "me": false },
            { "emoji": { "id": null, "name": "👍" }, "count": 2, "me": false },
        ]);
    });
    add(7, 102, "", &|_| {});
    add(7, 104, "", &|_| {});
    add(6, 103, "", &|m| m["message_reference"] = json!({ "channel_id": SYSTEM.to_string(), "message_id": at(0).to_string() }));
    add(8, 102, "", &|_| {});
    add(9, 103, "2", &|_| {});
    add(18, 101, "release plan", &|m| m["message_reference"] = json!({ "channel_id": THREAD.to_string() }));
    add(21, 101, "", &|m| {
        m["referenced_message"] = msg(at(900), SYSTEM, 101, "the message a thread started from");
        m["message_reference"] = json!({ "channel_id": SYSTEM.to_string(), "message_id": at(900).to_string() });
    });
    add(24, 103, "badword in a message", &|m| {
        m["embeds"] = json!([{ "type": "auto_moderation_message", "fields": [
            { "name": "rule_name", "value": "No spoilers" }, { "name": "keyword", "value": "badword" }, { "name": "channel_id", "value": GENERAL.to_string() } ] }]);
    });
    add(25, 105, "", &|m| m["role_subscription_data"] = json!({ "tier_name": "Gold", "total_months_subscribed": 3, "is_renewal": true }));
    add(27, 101, "Rust talk", &|_| {});
    add(28, 101, "Rust talk", &|_| {});
    add(29, 102, "", &|_| {});
    add(30, 103, "", &|_| {});
    add(31, 101, "Q&A", &|_| {});
    add(36, 101, &iso(now_ms() + 3_600_000), &|_| {});
    add(37, 101, "", &|_| {});
    add(38, 101, "", &|_| {});
    add(39, 101, "", &|_| {});
    add(44, 102, "", &|m| m["purchase_notification"] = json!({ "type": 1, "guild_product_purchase": { "listing_id": "1", "product_name": "Rust Mug" } }));
    add(46, 103, "", &|m| {
        m["message_reference"] = json!({ "channel_id": SYSTEM.to_string(), "message_id": at(0).to_string() });
        m["embeds"] = json!([{ "type": "poll_result", "fields": [
            { "name": "poll_question_text", "value": "Worse leg" }, { "name": "victor_answer_id", "value": "1" },
            { "name": "victor_answer_text", "value": "fountain of fate" }, { "name": "victor_answer_votes", "value": "32" },
            { "name": "total_votes", "value": "50" } ] }]);
    });
    add(14, 101, "", &|_| {});
    add(15, 101, "", &|_| {});
    add(16, 101, "", &|_| {});
    add(17, 101, "", &|_| {});
    add(3, 101, "", &|m| m["call"] = json!({ "participants": ["101"], "ended_timestamp": iso(now_ms() - 3 * 3_600_000 + 20 * 60_000 + 7 * 60_000) }));
    add(4, 102, "new-name", &|_| {});
    add(20, 104, "/stats ran", &|m| {
        m["interaction_metadata"] = json!({ "id": "1", "type": 2, "user": user(101), "authorizing_integration_owners": { "0": "1000" } });
    });
    add(0, 101, "", &|m| {
        m["flags"] = json!(8192);
        m["attachments"] = json!([{ "id": "9", "filename": "voice-message.ogg", "size": 4096, "url": "http://127.0.0.1/voice.ogg", "proxy_url": "http://127.0.0.1/voice.ogg",
            "content_type": "audio/ogg", "duration_secs": 3.0, "waveform": "AAAA" }]);
    });
    out
}

const REPORT_MENU: &str = r#"{
 "version": "1.0", "variant": "1", "language": "en", "root_node_id": "root", "success_node_id": "done", "fail_node_id": "fail",
 "nodes": {
  "root": { "id": "root", "header": "Report Message", "subheader": "What's going on?", "key": "root",
            "elements": [{ "name": "preview", "type": "message_preview", "data": {} }],
            "children": [["It's spam", "spam"], ["Harassment", "harass"]], "button": null },
  "spam": { "id": "spam", "header": "It's spam", "key": "spam", "report_type": "spam",
            "elements": [{ "name": "reasons", "type": "checkbox", "should_submit_data": true,
                           "data": [["links", "Spam links", null], ["flood", "Message flooding", "Many messages in a row"]] }],
            "button": { "type": "submit" } },
  "harass": { "id": "harass", "header": "Harassment", "key": "harass",
              "elements": [{ "name": "details", "type": "text", "data": { "title": "Tell us more", "rows": 3, "placeholder": "Optional" } }],
              "button": { "type": "submit" } },
  "done": { "id": "done", "header": "Thanks for letting us know", "subheader": "We'll take a look.", "elements": [], "button": { "type": "done" } },
  "fail": { "id": "fail", "header": "Something went wrong", "elements": [], "button": { "type": "cancel" } }
 }
}"#;

fn unesc(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn failed_once() -> &'static Mutex<std::collections::HashSet<String>> {
    static S: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
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
        (102, "```rust\nfn main() {\n    println!(\"hello, rustcord\");\n}\n```"),
        (101, "||spoiler: it compiles first try||"),
        (100, "this is me testing ~~strikethrough~~ and __underline__"),
        (103, "check this out https://github.com/emilk/egui"),
        (105, "lol 😂😂"),
        (102, "brb, coffee ☕"),
        (106, "lurker here, first message in months 👋"),
    ];
    let n: u64 = std::env::var("MOCK_HISTORY").ok().and_then(|v| v.parse().ok()).unwrap_or(160);
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
                "footer": { "text": "rustcord releases" } }]);
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
        if i == n - 20 {
            m["mentions"] = json!([user(ME)]);
            m["content"] = json!("<@100> did you see the egui release notes?");
        }
        if i == n - 12 {
            m["content"] = json!("let's plan the next release in a thread");
            m["thread"] = thread();
        }
        if i == n - 6 || i == n - 40 {
            m["pinned"] = json!(true);
        }
        if i == n - 11 {
            m["content"] = json!("");
            m["poll"] = json!({
                "question": { "text": "which editor?" },
                "answers": [
                    { "answer_id": 1, "poll_media": { "text": "vim", "emoji": { "id": null, "name": "⌨️" } } },
                    { "answer_id": 2, "poll_media": { "text": "vscode" } },
                    { "answer_id": 3, "poll_media": { "text": "emacs" } },
                ],
                "allow_multiselect": false,
                "expiry": "2099-01-01T00:00:00+00:00",
                "results": { "is_finalized": false, "answer_counts": [
                    { "id": 1, "count": 7, "me_voted": false },
                    { "id": 2, "count": 11, "me_voted": false },
                    { "id": 3, "count": 2, "me_voted": false },
                ] }
            });
        }
        out.push(m);
    }
    out.extend(media_msgs(cdn));
    out.extend(system_msgs());
    for (i, (a, text)) in [(101, "anyone up for ranked?"), (103, "in 10")].iter().enumerate() {
        out.push(msg(snow(now_ms() - 8 * 60_000 + i as u64 * 60_000), THREAD_B, *a, text));
    }
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

    let early = |i: u64| snow(t0 - 6 * 60_000 + i * 60_000);
    let file = |name: &str, size: u64, ctype: &str, w: u32, h: u32| {
        let url = format!("{cdn}/media/{name}?ex=65f0a1b2&is=65ee2cb2&hm=abc");
        let mut a = json!({ "id": "9", "filename": name, "size": size, "url": url, "proxy_url": url, "content_type": ctype });
        if w > 0 {
            a["width"] = json!(w);
            a["height"] = json!(h);
        }
        a
    };
    let mut m = msg(early(0), MEDIA, 102, "a video with sound");
    m["attachments"] = json!([file("clip.mp4", 19_785, "video/mp4", 320, 180)]);
    out.push(m);
    let mut m = msg(early(1), MEDIA, 101, "an mp3");
    m["attachments"] = json!([file("tone.mp3", 12_347, "audio/mpeg", 0, 0)]);
    out.push(m);
    let mut m = msg(early(2), MEDIA, 102, "some code");
    m["attachments"] = json!([file("hello.rs", include_bytes!("fixtures/hello.rs").len() as u64, "text/plain; charset=utf-8", 0, 0)]);
    out.push(m);

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

fn side_thread(id: u64, name: &str, meta: Value) -> Value {
    json!({ "id": id.to_string(), "type": 11, "guild_id": "1000", "parent_id": GENERAL.to_string(), "name": name, "owner_id": "101",
            "message_count": 1, "member_count": 2, "thread_metadata": meta, "last_message_id": null, "member": { "user_id": ME.to_string() } })
}

fn thread() -> Value {
    json!({ "id": THREAD.to_string(), "type": 11, "guild_id": "1000", "parent_id": GENERAL.to_string(), "name": "release planning",
            "owner_id": "101", "message_count": 4, "member_count": 4, "thread_metadata": { "archived": false },
            "last_message_id": snow(now_ms() - 27 * 60_000).to_string(), "member": { "user_id": ME.to_string() },
            // pinned to, with no read state yet, so its pin button carries the dot
            "last_pin_timestamp": iso(now_ms() - 3_600_000) })
}

// a thread in a server of ours, started by someone else and not joined: Join Thread, and every
// moderation item in its menu
const THREAD_B: u64 = 2160;

fn thread_b() -> Value {
    json!({ "id": THREAD_B.to_string(), "type": 11, "guild_id": "1001", "parent_id": "2100", "name": "lobby banter",
            "owner_id": "101", "message_count": 2, "member_count": 2, "thread_metadata": { "archived": false, "locked": false },
            "last_message_id": snow(now_ms() - 5 * 60_000).to_string() })
}

fn thread_members(thread: &str, guild: &str) -> Value {
    let member = |u: u64| json!({ "user_id": u.to_string(), "thread_id": thread, "member": { "user": user(u), "roles": [] } });
    json!({ "guild_id": guild, "thread_id": thread, "members": [member(101), member(103), member(102)],
            "presences": [{ "user": { "id": "101" }, "status": "online", "activities": [] }, { "user": { "id": "103" }, "status": "idle", "activities": [] }] })
}

fn events_base(g: &str) -> Vec<Value> {
    match g {
        "1000" => vec![json!({
            "id": "6100", "guild_id": "1000", "name": "Rust release party", "description": "Watching the release notes together, then a quiz.",
            "channel_id": "2021", "creator_id": "101", "creator": user(101), "scheduled_start_time": iso(now_ms() + 26 * 3_600_000),
            "scheduled_end_time": null, "status": 1, "entity_type": 2, "entity_metadata": null, "user_count": 4,
        })],
        "1001" => vec![
            json!({ "id": "6200", "guild_id": "1001", "name": "Ranked night", "description": "Scrims in the hangout.", "channel_id": "2102",
                    "creator_id": "101", "creator": user(101), "scheduled_start_time": iso(now_ms() + 5 * 3_600_000), "scheduled_end_time": null,
                    "status": 1, "entity_type": 2, "entity_metadata": null, "user_count": 3 }),
            json!({ "id": "6201", "guild_id": "1001", "name": "Map review", "description": "", "channel_id": null, "creator_id": ME.to_string(),
                    "creator": user(ME), "scheduled_start_time": iso(now_ms() + 48 * 3_600_000), "scheduled_end_time": iso(now_ms() + 50 * 3_600_000),
                    "status": 1, "entity_type": 3, "entity_metadata": { "location": "the community hall" }, "user_count": 0 }),
        ],
        _ => Vec::new(),
    }
}

// what the server has for a guild's events: the ones READY lists, then edits, cancels and new ones
fn event_list(w: &W, g: &str) -> Vec<Value> {
    let w = w.lock().unwrap();
    let base = events_base(g);
    let mut out: Vec<Value> = base
        .iter()
        .filter(|e| !w.gone.contains(&format!("event:{}", e["id"].as_str().unwrap_or_default())))
        .map(|e| w.edits.get(&format!("event:{}", e["id"].as_str().unwrap_or_default())).cloned().unwrap_or_else(|| e.clone()))
        .collect();
    for (k, v) in &w.edits {
        let id = v["id"].as_str().unwrap_or_default();
        if k.starts_with("event:") && v["guild_id"] == g && !base.iter().any(|e| e["id"] == id) && !w.gone.contains(k) {
            out.push(v.clone());
        }
    }
    out.retain(|e| matches!(e["status"].as_u64(), Some(1 | 2)));
    out
}

fn quests_base() -> Vec<Value> {
    let quest = |id: &str, name: &str, task: Value, reward: Value, hue: &str| {
        json!({ "id": id, "preview": false, "config": {
            "id": id, "config_version": 2, "starts_at": iso(now_ms() - 86_400_000), "expires_at": iso(now_ms() + 6 * 86_400_000), "features": [],
            "assets": { "hero": "hero.png", "quest_bar_hero": "hero.png", "game_tile": "tile.png", "logotype": "logo.png" },
            "colors": { "primary": hue, "secondary": "#404a7b" },
            "messages": { "quest_name": name, "game_title": "Mock Racers", "game_publisher": "Mock Games" },
            "task_config_v2": { "tasks": task, "join_operator": "or" },
            "rewards_config": { "assignment_method": 1, "rewards": [reward], "rewards_expire_at": null, "platforms": 0 },
            "cta_config": { "link": "https://example.com", "button_label": "Play Free Now" } }, "user_status": null })
    };
    let orbs = |n: u64| json!({ "type": 4, "sku_id": "1", "name": format!("{n} Orbs"), "orb_quantity": n });
    let mut done = quest("9003", "Mock Racers Finals Quest", json!({ "PLAY_ON_DESKTOP": { "type": "PLAY_ON_DESKTOP", "target": 900, "applications": [] } }), orbs(840), "#eb459e");
    done["user_status"] = json!({ "user_id": ME.to_string(), "quest_id": "9003", "enrolled_at": iso(now_ms() - 3_600_000), "completed_at": iso(now_ms() - 60_000),
                                  "claimed_at": null, "progress": { "PLAY_ON_DESKTOP": { "event_name": "PLAY_ON_DESKTOP", "value": 900 } } });
    vec![
        quest("9001", "Mock Racers Trailer Quest", json!({ "WATCH_VIDEO": { "type": "WATCH_VIDEO", "target": 27 } }), orbs(240), "#5865f2"),
        quest("9002", "Mock Racers Play Quest", json!({ "PLAY_ON_DESKTOP": { "type": "PLAY_ON_DESKTOP", "target": 900, "applications": [] } }),
              json!({ "type": 2, "sku_id": "2", "messages": { "name": "Turbo Kart avatar decoration", "name_with_article": "a Turbo Kart avatar decoration" } }), "#3ba55d"),
        done,
    ]
}

fn quest_list(w: &W) -> Vec<Value> {
    let w = w.lock().unwrap();
    quests_base()
        .into_iter()
        .map(|mut q| {
            if let Some(s) = w.edits.get(&format!("quest:{}", q["id"].as_str().unwrap_or_default())) {
                q["user_status"] = s.clone();
            }
            q
        })
        .collect()
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

    let mut den = guild(1008, "Voice Den", vec![ch(2800, 0, "chat", 0, None), ch(2801, 2, "Hangout", 1, None), ch(2802, 13, "Panel", 2, None)]);
    den["owner_id"] = json!("100");
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

fn varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
    out
}

fn pb_len(field: u64, body: &[u8]) -> Vec<u8> {
    [varint(field << 3 | 2), varint(body.len() as u64), body.to_vec()].concat()
}

fn pb_int(field: u64, v: u64) -> Vec<u8> {
    [varint(field << 3), varint(v)].concat()
}

/// PreloadedUserSettings with appearance.theme (1 dark .. 4 midnight) and
/// text_and_images.display_compact_avatars, the latter as a BoolValue wrapper.
fn settings_proto(theme: u64, compact_avatars: bool) -> String {
    use base64::Engine;
    let appearance = pb_int(1, theme);
    let text = pb_len(41, &if compact_avatars { pb_int(1, 1) } else { Vec::new() });
    let blob = [pb_len(6, &text), pb_len(13, &appearance)].concat();
    base64::engine::general_purpose::STANDARD.encode(blob)
}

fn ready(last_general: u64) -> Value {
    let ch = |id: u64, kind: u8, name: &str, pos: i32, parent: Option<u64>| {
        json!({ "id": id.to_string(), "type": kind, "name": name, "position": pos,
                "parent_id": parent.map(|p| p.to_string()), "permission_overwrites": [], "last_message_id": null })
    };
    let mut general = ch(GENERAL, 0, "general", 0, Some(2010));
    general["last_message_id"] = json!(last_general.to_string());
    general["topic"] = json!("Talk about anything. Be nice. Rust only on Tuesdays.");
    // pinned since you last looked, so the pin button carries its dot
    general["last_pin_timestamp"] = json!(iso(now_ms() - 3_600_000));
    let mut help = ch(2012, 0, "help", 1, Some(2010));
    help["last_message_id"] = json!(snow(now_ms() - 60_000).to_string());
    let mut forum = ch(FORUM, 15, "ideas", 4, Some(2010));
    forum["available_tags"] = json!([{ "id": "1", "name": "feature", "emoji_name": "✨" }, { "id": "2", "name": "ux" }]);
    let mut slow = ch(2015, 0, "slow", 6, Some(2010));
    slow["rate_limit_per_user"] = json!(30);
    let mut readonly = ch(2016, 0, "read-only", 7, Some(2010));
    readonly["permission_overwrites"] = json!([{ "id": "1000", "type": 0, "allow": "0", "deny": "2048" }]);
    let mut news = ch(2002, 5, "📢・announcements", 1, Some(2000));
    news["permission_overwrites"] = json!([{ "id": "1000", "type": 0, "allow": "0", "deny": "2048" }]);
    let mut secret = ch(2014, 0, "mods-only", 3, Some(2010));
    secret["permission_overwrites"] = json!([{ "id": "1000", "type": 0, "allow": "0", "deny": "1024" }]);
    let roles = json!([
        { "id": "1000", "name": "@everyone", "color": 0, "position": 0, "permissions": "844736315296768", "hoist": false },
        { "id": "1100", "name": "Moderator", "color": 0xE67E22, "position": 3, "permissions": "8", "hoist": true },
        { "id": "1101", "name": "Contributor", "color": 0x3498DB, "position": 2, "permissions": "0", "hoist": false },
        { "id": "1102", "name": "Bots", "color": 0x9B59B6, "position": 1, "permissions": "0", "hoist": false },
        { "id": "1104", "name": "Duo", "color": 0xD96BFF, "position": 4, "permissions": "0", "hoist": false, "unicode_emoji": "🪙",
          "colors": { "primary_color": 0xD96BFF, "secondary_color": 0x6BD0FF, "tertiary_color": null } },
        { "id": "1103", "name": "Holographic", "color": 11127295, "position": 5, "permissions": "0", "hoist": false, "icon": "roleicon1103",
          "colors": { "primary_color": 11127295, "secondary_color": 16759788, "tertiary_color": 16761760 } },
    ]);
    let g1 = json!({
        "id": "1000", "name": "Rust Lounge", "icon": "icon1000", "owner_id": "101", "member_count": 6,
        "roles": roles, "emojis": [
            { "id": "5000", "name": "ferris", "animated": false }, { "id": "5100", "name": "party", "animated": true },
            { "id": "5101", "name": "omgParty", "animated": true },
            { "id": "5300", "name": "omgMods", "animated": false, "roles": ["1100"] },
        ],
        "channels": [
            ch(2010, 4, "Text Channels", 0, None), general, help, ch(2013, 0, "🎲┃off-topic", 2, Some(2010)), secret,
            ch(2000, 4, "Information", 1, None), ch(2001, 0, "rules", 0, Some(2000)), news, slow, readonly,
            ch(2020, 4, "Voice Channels", 2, None), ch(2021, 2, "Lounge", 0, Some(2020)), ch(2022, 2, "Gaming", 1, Some(2020)),
            forum, ch(MEDIA, 0, "media", 5, Some(2010)), ch(SYSTEM, 0, "system", 6, Some(2010)),
        ],
        "threads": [thread(), side_thread(2051, "locked talk", json!({ "archived": false, "locked": true })), side_thread(2052, "old talk", json!({ "archived": true, "locked": false }))],
        // alice server muted, carol muted and deafened herself
        "voice_states": [{ "user_id": "101", "channel_id": "2021", "mute": true }, { "user_id": "103", "channel_id": "2021", "self_mute": true, "self_deaf": true }],
    });
    let simple = |id: u64, name: &str, icon: Option<&str>, chans: &[(u64, &str)]| {
        let roles = json!([{ "id": id.to_string(), "name": "@everyone", "color": 0, "position": 0, "permissions": "3072" }]);
        let chans: Vec<Value> = chans.iter().enumerate().map(|(i, (cid, n))| ch(*cid, 0, n, i as i32, None)).collect();
        json!({ "id": id.to_string(), "name": name, "icon": icon, "owner_id": "102", "roles": roles, "emojis": [], "channels": chans })
    };
    let mut guilds = vec![g1, simple(1001, "Gamers United", None, &[(2100, "lobby"), (2101, "lfg")]),
                          simple(1002, "Design Club", Some("icon1002"), &[(2200, "showcase"), (2201, "feedback")])];
    guilds[2]["premium_subscription_count"] = json!(6);
    guilds[2]["premium_tier"] = json!(1);
    guilds[1]["emojis"] = json!([
        { "id": "5200", "name": "omgBruh", "animated": false },
        { "id": "5201", "name": "omgLapsed", "animated": false, "available": false },
    ]);
    // a community server of ours with onboarding, for the rows over its channels, and a voice
    // channel to open the chat of, edit and delete
    guilds[1]["owner_id"] = json!(ME.to_string());
    guilds[1]["features"] = json!(["COMMUNITY", "GUILD_ONBOARDING", "GUILD_TAGS"]);
    guilds[1]["channels"].as_array_mut().unwrap().push(ch(2102, 2, "Hangout", 2, None));
    guilds[1]["threads"] = json!([thread_b()]);
    guilds[1]["guild_scheduled_events"] = json!(events_base("1001"));
    guilds.extend(side_guilds());
    let mut folders = vec![json!({ "guild_ids": ["1000"] }), json!({ "id": 77, "name": "Hobbies", "color": 0xE91E63, "guild_ids": ["1001", "1002"] }),
                           json!({ "id": 78, "name": "Pals", "guild_ids": ["1003", "1004", "1005", "1006"] }), json!({ "guild_ids": ["1007"] }), json!({ "guild_ids": ["1008"] })];
    // e2e/bench.py: MOCK_GUILDS more servers of 115 channels each, about the size of a real account
    let extra: u64 = std::env::var("MOCK_GUILDS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut extra_reads = Vec::new();
    for g in 0..extra {
        let gid = 10_000 + g;
        let chans: Vec<Value> = (0..115u64)
            .map(|j| {
                let id = 100_000 + g * 200 + j;
                let mut c = if j % 15 == 0 { ch(id, 4, &format!("category {j}"), j as i32, None) } else { ch(id, if j % 15 == 14 { 2 } else { 0 }, &format!("channel-{j}"), j as i32, Some(100_000 + g * 200 + j / 15 * 15)) };
                c["last_message_id"] = json!(snow(now_ms() - (g * 115 + j) * 60_000).to_string());
                if j % 2 == 0 {
                    extra_reads.push(json!({ "id": id.to_string(), "last_message_id": snow(now_ms() - (g * 115 + j) * 60_000 - if j % 8 == 0 { 30_000 } else { 0 }).to_string(), "mention_count": (j % 40 == 0) as u32 }));
                }
                c
            })
            .collect();
        let roles: Vec<Value> = std::iter::once(json!({ "id": gid.to_string(), "name": "@everyone", "color": 0, "position": 0, "permissions": "1051648" }))
            .chain((1..20u64).map(|r| json!({ "id": (gid * 100 + r).to_string(), "name": format!("role {r}"), "color": r * 0x0a0b0c, "position": r, "permissions": "0", "hoist": r % 5 == 0 })))
            .collect();
        guilds.push(json!({ "id": gid.to_string(), "name": format!("Server {g}"), "icon": if g % 3 == 0 { Value::Null } else { json!(format!("icon{gid}")) },
                            "owner_id": "102", "member_count": 1000 + g * 37, "roles": roles, "emojis": [], "channels": chans }));
        if g % 8 == 0 {
            folders.push(json!({ "id": 1000 + g, "name": format!("Folder {g}"), "guild_ids": [] }));
        }
        folders.last_mut().unwrap()["guild_ids"].as_array_mut().unwrap().push(json!(gid.to_string()));
    }
    let mut users = vec![user(101), user(102), user(103), user(104), user(105), user(106)];
    users.extend((0..extra * 6).map(|i| user(20_000 + i)));
    let mut reads = vec![
        json!({ "id": GENERAL.to_string(), "last_message_id": snow(now_ms() - 20 * 95_000).to_string(), "mention_count": 1,
                "last_pin_timestamp": iso(now_ms() - 2 * 86_400_000) }),
        json!({ "id": "2012", "last_message_id": "0", "mention_count": 2 }),
        json!({ "id": "3000", "last_message_id": "0", "mention_count": 0 }),
        json!({ "id": "1001", "read_state_type": 1, "last_message_id": "0", "mention_count": 2 }),
    ];
    reads.extend(side_reads());
    reads.extend(extra_reads);
    let mut merged = vec![json!([{ "user_id": ME.to_string(), "roles": ["1101"] }]), json!([]), json!([]), json!([{ "user_id": ME.to_string(), "roles": ["1201"] }]), json!([]), json!([]), json!([]), json!([]), json!([])];
    merged.extend((0..extra).map(|_| json!([])));
    json!({
        "v": 9, "session_id": "mock-session", "resume_gateway_url": null,
        "user": { "id": ME.to_string(), "username": "you", "global_name": "You", "avatar": "av100", "email": "you@example.com", "premium_type": 0, "mfa_enabled": false, "phone": "+15555550100", "nsfw_allowed": true },
        "users": users,
        "relationships": [{ "id": "101", "type": 1 }, { "id": "102", "type": 1 }, { "id": "103", "type": 1 }, { "id": "106", "type": 3 }],
        "guilds": guilds,
        "merged_members": merged,
        "private_channels": [
            { "id": "3000", "type": 1, "recipient_ids": ["101"], "last_message_id": snow(now_ms() - 3_600_000).to_string() },
            { "id": "3001", "type": 3, "name": "", "recipient_ids": ["102", "103"], "last_message_id": snow(now_ms() - 7_200_000).to_string() },
            // muted below: its messages mustn't ping or pop up in the rail
            { "id": "3002", "type": 1, "recipient_ids": ["105"], "last_message_id": snow(now_ms() - 9_000_000).to_string() },
        ],
        "read_state": { "entries": reads, "partial": false, "version": 1 },
        "user_guild_settings": { "entries": [
            // onboarding's opt-in: only lobby was picked
            { "guild_id": "1001", "flags": 16384, "channel_overrides": [{ "channel_id": "2100", "flags": 4096 }, { "channel_id": "2102", "flags": 4096 }] },
            { "guild_id": "1004", "channel_overrides": [{ "channel_id": "2400", "muted": true }] },
            { "guild_id": "1005", "channel_overrides": [{ "channel_id": "2500", "muted": true }] },
            { "guild_id": "1006", "message_notifications": 1, "channel_overrides": [] },
            { "guild_id": null, "channel_overrides": [{ "channel_id": "3002", "muted": true }] },
        ] },
        "user_settings_proto": settings_proto(1, true),
        "user_settings": { "guild_folders": folders },
    })
}

/// A server channel from READY with the fields its settings page reads, as last saved.
fn channel(w: &W, id: &str) -> Option<Value> {
    let w = w.lock().unwrap();
    if w.gone.iter().any(|g| g == id) {
        return None;
    }
    if let Some(c) = w.edits.get(id) {
        return Some(c.clone());
    }
    if id == THREAD.to_string() {
        return Some(thread());
    }
    if id == THREAD_B.to_string() {
        return Some(thread_b());
    }
    let r = ready(0);
    let (g, c) = r["guilds"].as_array()?.iter().find_map(|g| g["channels"].as_array()?.iter().find(|c| c["id"] == id).map(|c| (g["id"].clone(), c.clone())))?;
    let mut c = c;
    c["guild_id"] = g;
    for (k, v) in [("topic", Value::Null), ("nsfw", json!(false)), ("flags", json!(0)), ("rate_limit_per_user", json!(0)), ("default_auto_archive_duration", json!(4320))] {
        if c.get(k).is_none() {
            c[k] = v;
        }
    }
    if c["type"] == json!(2) {
        c["bitrate"] = json!(64000);
        c["user_limit"] = json!(0);
        c["rtc_region"] = Value::Null;
        c["video_quality_mode"] = json!(1);
    }
    Some(c)
}

fn member_list() -> Value {
    let item = |id: u64, status: &str| json!({ "member": { "user": user(id), "roles": member(id)["roles"], "nick": null, "premium_since": member(id)["premium_since"], "presence": { "status": status } } });
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

fn query_str(path: &str, key: &str) -> String {
    let q = path.split_once('?').map_or("", |p| p.1);
    q.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}="))).unwrap_or("").replace('+', " ").replace("%20", " ")
}

/// Thirty discoverable servers, the first eight of them the featured ones; names carry "rust" in every third.
fn discoverable(i: u64) -> Value {
    let base = ["Crab Chess Club", "Pixel Painters", "Synthwave Radio", "Mock Quest Guild", "Speedrun Central", "Lofi Study Hall"];
    let name = if i % 3 == 0 { format!("Rust Learners {i}") } else { format!("{} {i}", base[(i % 6) as usize]) };
    let cat = [1, 2, 3, 5, 6][(i % 5) as usize];
    json!({ "id": (9000 + i).to_string(), "name": name, "description": format!("A mock community for the discovery page, number {i}."),
            "icon": format!("dg{i}"), "discovery_splash": format!("ds{i}"), "banner": null, "splash": null,
            "features": if i % 2 == 0 { json!(["VERIFIED", "DISCOVERABLE"]) } else { json!(["DISCOVERABLE"]) },
            "approximate_member_count": 120_000 + i * 1111, "approximate_presence_count": 9_000 + i * 37,
            "primary_category_id": cat })
}

fn query(path: &str, key: &str) -> Option<u64> {
    let q = path.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}=")).and_then(|v| v.parse().ok()))
}

/// Windows ships no gg sans, so the download path gets a system font in woff2 clothes.
const NAME_FONT_ASSETS: [&str; 11] = [
    "33d4f12a85e1f736", "e6f5f44abb520735", "c560709c3470bb66", "52b541f86401a5b6", "69c735ca5c604de7", "8f20cb550d739cea",
    "27b34a77ba5d693e", "45ef50dce38a931a", "4841dace333e9054", "c00df4f25667809b", "aa59ae54ce7e540a",
];

fn gg_stand_in(face: &str) -> Vec<u8> {
    static CACHE: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());
    if let Some((_, b)) = CACHE.lock().unwrap().iter().find(|(f, _)| f == face) {
        return b.clone();
    }
    let file = match face {
        "sans600normal" => "seguisb.ttf",
        "sans700normal" => "segoeuib.ttf",
        "sans600italic" => "seguisbi.ttf",
        "sans700italic" => "segoeuiz.ttf",
        f if f.starts_with("sans") && f.ends_with("italic") => "segoeuii.ttf",
        "mono700normal" => "consolab.ttf",
        f if f.starts_with("mono") => "consola.ttf",
        _ => "segoeui.ttf",
    };
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let ttf = std::fs::read(format!("{dir}\\Fonts\\{file}")).unwrap_or_default();
    let woff2 = ttf2woff2::encode(&ttf, 3.into()).unwrap_or_default();
    CACHE.lock().unwrap().push((face.to_string(), woff2.clone()));
    woff2
}

fn default_sounds() -> Value {
    json!([
        { "sound_id": "5001", "name": "airhorn", "emoji_name": "\u{1f4ef}", "emoji_id": null, "volume": 1.0, "user": user(101) },
        { "sound_id": "5002", "name": "quack", "emoji_name": "\u{1f986}", "emoji_id": null, "volume": 1.0, "user": user(102) },
    ])
}

/// A guild the way `GET /guilds/<id>` gives it: READY's, filled out, with what was saved over it.
fn guild_full(w: &W, gid: &str) -> Option<Value> {
    let mut g = ready(0)["guilds"].as_array()?.iter().find(|g| g["id"] == gid)?.clone();
    for (k, v) in [("system_channel_id", Value::Null), ("system_channel_flags", json!(0)), ("default_message_notifications", json!(0)), ("afk_channel_id", Value::Null),
                   ("afk_timeout", json!(300)), ("verification_level", json!(0)), ("explicit_content_filter", json!(0)), ("rules_channel_id", Value::Null),
                   ("public_updates_channel_id", Value::Null), ("owner_configured_content_level", json!(0)), ("premium_progress_bar_enabled", json!(false)),
                   ("premium_tier", json!(0)), ("features", json!([]))] {
        if g.get(k).is_none() {
            g[k] = v;
        }
    }
    if let Some(e) = w.lock().unwrap().edits.get(&format!("g:{gid}")).and_then(|e| e.as_object()) {
        for (k, v) in e {
            g[k] = v.clone();
        }
    }
    Some(g)
}

/// What a server's profile page gets: its name, icon and traits, with what was saved over it.
fn profile_full(w: &W, gid: &str) -> Option<Value> {
    let g = guild_full(w, gid)?;
    let mut p = json!({ "id": gid, "name": g["name"], "description": "", "icon_hash": g["icon"], "brand_color_primary": null, "visibility": 1, "traits": [],
                        "game_application_ids": ["7100"], "games": [{ "id": "7100", "name": "Mockcord Quest" }], "member_count": g["member_count"], "online_count": 3,
                        "tag": "", "badge": 6, "badge_color_primary": "#816bee", "badge_color_secondary": "#b79cf8" });
    if let Some(e) = w.lock().unwrap().edits.get(&format!("p:{gid}")).and_then(|e| e.as_object()) {
        for (k, v) in e {
            p[k] = v.clone();
        }
    }
    Some(p)
}

/// Merge a PATCH body into what's kept under `key`.
fn keep(w: &W, key: String, body: &[u8]) {
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut w = w.lock().unwrap();
    let e = w.edits.entry(key).or_insert_with(|| json!({}));
    for (k, v) in b.as_object().into_iter().flatten() {
        e[k] = v.clone();
    }
}

/// GUILD_UPDATE for a guild after a save: its fields, without the lists READY sends.
fn guild_update(w: &W, gid: &str) {
    let mut up = guild_full(w, gid).unwrap();
    for k in ["channels", "threads", "voice_states", "roles", "emojis"] {
        up.as_object_mut().map(|o| o.remove(k));
    }
    let _ = w.lock().unwrap().events.send(json!({ "t": "GUILD_UPDATE", "d": up }));
}

static BILL: Mutex<Value> = Mutex::new(Value::Null);

fn bill_state() -> Value {
    let now = now_ms();
    let day = 86_400_000;
    let addr = json!({ "name": "You", "line_1": "12 Nguyen Hue", "line_2": null, "city": "Ho Chi Minh City", "state": null, "postal_code": "700000", "country": "VN" });
    let source = |id: &str, brand: &str, last4: &str, month: u64, year: u64, invalid: bool, default: bool| {
        json!({ "id": id, "type": 1, "brand": brand, "last_4": last4, "expires_month": month, "expires_year": year, "invalid": invalid, "default": default, "country": "VN", "billing_address": addr })
    };
    let payments: Vec<Value> = (0..12u64)
        .map(|i| {
            let gift = i % 3 == 1;
            json!({ "id": (9900 + i).to_string(), "amount": if gift { 227045 } else { 113000 }, "currency": "vnd", "description": "Nitro", "status": if i == 2 { 2 } else { 1 },
                    "gift": gift, "created_at": iso(now - (i + 1) * 9 * day), "amount_refunded": 0, "tax": 0, "tax_inclusive": true,
                    "payment_source": { "id": "9501", "brand": "visa", "last_4": "4487" } })
        })
        .collect();
    let applied = |sub: &str, guild: &str| json!({ "id": sub, "guild_id": guild, "user_id": ME.to_string(), "ended": false });
    json!({
        "sources": [source("9501", "visa", "4487", 9, 2029, false, true), source("9502", "visa", "7790", 8, 2023, true, false), source("9503", "mastercard", "4683", 4, 2028, false, false)],
        "subs": [{ "id": "9601", "type": 1, "status": 1, "payment_source_id": "9501", "current_period_end": iso(now + 20 * day), "canceled_at": null, "items": [{ "plan_id": "511651880837840896", "quantity": 1 }] }],
        "slots": [
            { "id": "9701", "subscription_id": "9601", "canceled": false, "cooldown_ends_at": null, "premium_guild_subscription": applied("9801", "1002") },
            { "id": "9702", "subscription_id": "9601", "canceled": false, "cooldown_ends_at": null, "premium_guild_subscription": null },
            { "id": "9703", "subscription_id": "9601", "canceled": false, "cooldown_ends_at": iso(now + 5 * day), "premium_guild_subscription": applied("9803", "1001") },
        ],
        "payments": payments,
        "gifts": [{ "id": "9950", "sku_id": "521842831262875670", "sku": { "id": "521842831262875670", "name": "Nitro" }, "subscription_plan_id": "511651880837840896", "gifter_id": null }],
        "codes": [{ "code": "Zx81QpLmK3vT", "sku_id": "521842831262875670", "store_listing": { "sku": { "name": "Nitro" } }, "uses": 0, "max_uses": 1 }],
    })
}

fn billing(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let mut g = BILL.lock().unwrap();
    if g.is_null() {
        *g = bill_state();
    }
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let (now, day) = (now_ms(), 86_400_000);
    let find = |list: &str, key: &str, id: &str, g: &Value| g[list].as_array().and_then(|l| l.iter().position(|x| x[key] == id));
    let missing = || Some((404, json!({ "message": "Unknown", "code": 10000 })));
    match (method, parts) {
        ("GET", ["api", "v9", "users", "@me", "billing", "subscriptions"]) => Some((200, g["subs"].clone())),
        ("GET", ["api", "v9", "users", "@me", "billing", "payment-sources"]) => Some((200, g["sources"].clone())),
        ("GET", ["api", "v9", "users", "@me", "billing", "payments"]) => Some((200, g["payments"].clone())),
        ("GET", ["api", "v9", "users", "@me", "guilds", "premium", "subscription-slots"]) => Some((200, g["slots"].clone())),
        ("GET", ["api", "v9", "users", "@me", "guilds", "premium", "subscriptions", "cooldown"]) => Some((200, json!({ "ends_at": null }))),
        ("GET", ["api", "v9", "users", "@me", "entitlements", "gifts"]) => Some((200, g["gifts"].clone())),
        ("GET", ["api", "v9", "users", "@me", "entitlements", "gift-codes"]) => Some((200, g["codes"].clone())),
        ("GET", ["api", "v9", "users", "%40me" | "@me", "badges", "17"]) => Some((200, json!({ "badge_id": "17", "progress": { "current": 4 } }))),
        ("GET", ["api", "v9", "store", "published-listings", "skus", sku, "subscription-plans"]) => {
            let (month, year) = if *sku == "521842831262875670" { (113000, 1130000) } else { (42000, 420000) };
            Some((200, json!([
                { "id": "511651880837840896", "interval": 1, "interval_count": 1, "price": month, "currency": "vnd", "sku_id": sku },
                { "id": "511651885459963904", "interval": 2, "interval_count": 1, "price": year, "currency": "vnd", "sku_id": sku },
            ])))
        }
        ("DELETE", ["api", "v9", "users", "@me", "billing", "payment-sources", id]) => {
            let i = find("sources", "id", id, &g)?;
            g["sources"].as_array_mut()?.remove(i);
            Some((204, Value::Null))
        }
        ("PATCH", ["api", "v9", "users", "@me", "billing", "payment-sources", id]) => {
            let i = find("sources", "id", id, &g)?;
            if b["default"] == true {
                g["sources"].as_array_mut()?.iter_mut().for_each(|s| s["default"] = json!(false));
                g["sources"][i]["default"] = json!(true);
            }
            Some((200, g["sources"][i].clone()))
        }
        ("DELETE", ["api", "v9", "users", "@me", "billing", "subscriptions", id]) => {
            let i = find("subs", "id", id, &g)?;
            g["subs"][i]["canceled_at"] = json!(iso(now));
            Some((204, Value::Null))
        }
        ("DELETE", ["api", "v9", "guilds", _, "premium", "subscriptions", sub]) => {
            let Some(slot) = g["slots"].as_array_mut()?.iter_mut().find(|s| s["premium_guild_subscription"]["id"] == *sub) else { return missing() };
            if slot["cooldown_ends_at"].as_str().is_some() {
                return Some((400, json!({ "message": "Boost is on cooldown", "code": 50122 })));
            }
            slot["premium_guild_subscription"] = Value::Null;
            Some((204, Value::Null))
        }
        ("PUT", ["api", "v9", "guilds", guild, "premium", "subscriptions"]) => {
            let ids = b["user_premium_guild_subscription_slot_ids"].as_array()?.clone();
            let mut out = Vec::new();
            for id in ids {
                let Some(slot) = g["slots"].as_array_mut()?.iter_mut().find(|s| s["id"] == id) else { return missing() };
                if !slot["premium_guild_subscription"].is_null() {
                    return Some((400, json!({ "message": "Boost is already applied", "code": 50122 })));
                }
                let sub = json!({ "id": (now % 100_000).to_string(), "guild_id": guild, "user_id": ME.to_string(), "ended": false });
                slot["premium_guild_subscription"] = sub.clone();
                slot["cooldown_ends_at"] = json!(iso(now + 7 * day));
                out.push(sub);
            }
            Some((200, json!(out)))
        }
        ("POST", ["api", "v9", "users", "@me", "entitlements", "gift-codes"]) => {
            let sku = b["sku_id"].as_str().unwrap_or("");
            let i = g["gifts"].as_array()?.iter().position(|x| x["sku_id"] == sku)?;
            let gift = g["gifts"].as_array_mut()?.remove(i);
            let code = json!({ "code": format!("Gen{}", now % 1_000_000_000), "sku_id": sku, "store_listing": { "sku": { "name": gift["sku"]["name"] } }, "uses": 0, "max_uses": 1 });
            g["codes"].as_array_mut()?.push(code.clone());
            Some((200, code))
        }
        ("DELETE", ["api", "v9", "users", "@me", "entitlements", "gift-codes", code]) => {
            let i = find("codes", "code", code, &g)?;
            g["codes"].as_array_mut()?.remove(i);
            Some((204, Value::Null))
        }
        ("POST", ["api", "v9", "entitlements", "gift-codes", code, "redeem"]) => {
            if *code == "WUMP-AAAAA-BBBBB-CCCCC" {
                Some((200, json!({})))
            } else {
                Some((404, json!({ "message": "Unknown Gift Code", "code": 10038 })))
            }
        }
        _ => None,
    }
}

async fn route(w: &W, method: &str, path: &str, auth: &str, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
    let ok = |v: Value| (200, "application/json", v.to_string().into_bytes());
    let clean = path.split('?').next().unwrap_or("");
    if let Some((code, v)) = auth::route(method, clean, body) {
        return (code, "application/json", v.map(|v| v.to_string().into_bytes()).unwrap_or_default());
    }
    let parts: Vec<&str> = clean.trim_matches('/').split('/').collect();
    if !clean.contains("/mock/") {
        let mut w = w.lock().unwrap();
        if let Some(i) = w.fails.iter().position(|f| f.0 == method && clean.contains(f.1.as_str())) {
            let (status, reply) = (w.fails[i].2, w.fails[i].3.clone());
            w.fails[i].4 -= 1;
            if w.fails[i].4 == 0 {
                w.fails.remove(i);
            }
            return (status, "application/json", reply.to_string().into_bytes());
        }
    }
    if let Some((code, v)) = billing(method, &parts, body) {
        return (code, "application/json", if code == 204 { Vec::new() } else { v.to_string().into_bytes() });
    }
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
            let mut m = msg(fresh(), ch, 104, &text);
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
        ("GET", ["api", "v9", "gifs", "trending"]) => {
            let port = PORT.load(std::sync::atomic::Ordering::Relaxed);
            let src = |i: u32| format!("http://127.0.0.1:{port}/cdn/attachments/gif{i}.png");
            let cats: Vec<Value> = ["hello", "lol", "love", "happy birthday"].iter().enumerate().map(|(i, n)| json!({ "name": n, "src": src(i as u32) })).collect();
            ok(json!({ "categories": cats, "gifs": [{ "src": src(5), "width": 220, "height": 160 }] }))
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
        ("GET", ["api", "v9", "reporting", "menu", "message"]) => ok(serde_json::from_str(REPORT_MENU).unwrap()),
        ("POST", ["api", "v9", "reporting", "message"]) => ok(json!({ "report_id": "1" })),
        // the people behind a reaction: as many as it has, from a fixed cast
        ("GET", ["api", "v9", "channels", ch, "messages", id, "reactions", emoji]) => {
            let (ch, id) = (ch.to_string(), id.to_string());
            let emoji = unesc(emoji);
            let w = w.lock().unwrap();
            let n = w.msgs.iter().find(|m| m["channel_id"] == ch && m["id"] == id).and_then(|m| {
                m["reactions"].as_array()?.iter().find(|r| r["emoji"]["name"].as_str().is_some_and(|n| emoji.starts_with(n))).and_then(|r| r["count"].as_u64())
            });
            ok(json!([101, 102, 103, 105, 106].iter().take(n.unwrap_or(0) as usize).map(|u| user(*u)).collect::<Vec<_>>()))
        }
        ("POST", ["api", "v9", "channels", ch, "messages"]) => {
            let ch: u64 = ch.parse().unwrap_or(0);
            let b: Value = serde_json::from_slice(body).unwrap_or_else(|_| payload_json(body));
            // a send that fails the first time its nonce arrives, and goes through when sent again
            if b["content"] == "e2e-fail-once" && failed_once().lock().unwrap().insert(b["nonce"].to_string()) {
                return (403, "application/json", json!({ "message": "Missing Permissions", "code": 50013 }).to_string().into_bytes());
            }
            let mut m = msg(fresh(), ch, ME, b["content"].as_str().unwrap_or(""));
            m["nonce"] = b["nonce"].clone();
            m["member"] = member(ME);
            if let Some(p) = b["poll"].as_object() {
                let answers: Vec<Value> = p["answers"].as_array().into_iter().flatten().enumerate().map(|(i, a)| json!({ "answer_id": i + 1, "poll_media": a["poll_media"] })).collect();
                let hours = p["duration"].as_u64().unwrap_or(24);
                m["poll"] = json!({ "question": p["question"], "answers": answers, "allow_multiselect": p["allow_multiselect"], "expiry": iso(now_ms() + hours * 3_600_000),
                                    "results": { "is_finalized": false, "answer_counts": [] } });
            }
            let tx = {
                let mut w = w.lock().unwrap();
                w.msgs.push(m.clone());
                w.events.clone()
            };
            let _ = tx.send(json!({ "t": "MESSAGE_CREATE", "d": m.clone() }));
            let w = w.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let _ = tx.send(json!({ "t": "TYPING_START", "d": { "channel_id": ch.to_string(), "user_id": "101", "guild_id": "1000" } }));
                // a dm from someone else while you're busy elsewhere, so notifications have something to do;
                // the muted dm talks first and should stay quiet
                // kept like any other message, so jumping to them later finds them
                let dm = |ch: u64, from: u64, text: &str| {
                    let m = msg(fresh(), ch, from, text);
                    w.lock().unwrap().msgs.push(m.clone());
                    let _ = tx.send(json!({ "t": "MESSAGE_CREATE", "d": m }));
                };
                tokio::time::sleep(Duration::from_millis(300)).await;
                dm(3002, 105, "you muted me, right?");
                tokio::time::sleep(Duration::from_millis(300)).await;
                dm(3000, 101, "hey, saw your message — got a sec?");
            });
            ok(m)
        }
        ("POST", ["api", "v9", "mock", "settings-proto"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let proto = settings_proto(b["theme"].as_u64().unwrap_or(1), b["compact_avatars"].as_bool().unwrap_or(false));
            let tx = w.lock().unwrap().events.clone();
            let _ = tx.send(json!({ "t": "USER_SETTINGS_PROTO_UPDATE", "d": { "settings": { "type": 1, "proto": proto }, "partial": false } }));
            ok(json!({}))
        }
        ("PATCH", ["api", "v9", "users", "@me", "settings-proto", _]) => ok(json!({})),
        ("GET", ["api", "v9", "soundboard-default-sounds"]) => ok(json!([
            { "sound_id": "1", "name": "quack", "emoji_name": "🦆", "emoji_id": null, "volume": 1.0, "available": true },
            { "sound_id": "2", "name": "airhorn", "emoji_name": "📯", "emoji_id": null, "volume": 1.0, "available": true },
        ])),
        ("POST", ["api", "v9", "channels", _, "send-soundboard-sound"]) => (204, "application/json", Vec::new()),
        ("POST", ["api", "v9", "streams", _, "preview"]) => (204, "application/json", Vec::new()),
        // e2e/bench.py: a busy account's background traffic for `secs`, `rate` events a second:
        // presences in the open server, typing and messages in another one
        ("POST", ["api", "v9", "mock", "busy"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let (rate, secs) = (b["rate"].as_u64().unwrap_or(20).max(1), b["secs"].as_u64().unwrap_or(15));
            let w = w.clone();
            tokio::spawn(async move {
                let statuses = ["online", "idle", "dnd", "online"];
                for i in 0..rate * secs {
                    tokio::time::sleep(Duration::from_millis(1000 / rate)).await;
                    let e = match i % 10 {
                        0 => {
                            let m = msg(fresh(), 2101, 102 + i % 3, &format!("busy message {i}"));
                            w.lock().unwrap().msgs.push(m.clone());
                            json!({ "t": "MESSAGE_CREATE", "d": m })
                        }
                        1..=3 => json!({ "t": "TYPING_START", "d": { "channel_id": "2100", "user_id": (102 + i % 3).to_string(), "guild_id": "1001", "timestamp": now_ms() / 1000 } }),
                        _ => json!({ "t": "PRESENCE_UPDATE", "d": { "guild_id": "1000", "user": { "id": (101 + i % 5).to_string() },
                                     "status": statuses[(i / 10 % 4) as usize], "activities": [], "client_status": { "desktop": statuses[(i / 10 % 4) as usize] } } }),
                    };
                    let _ = w.lock().unwrap().events.send(e);
                }
            });
            ok(json!({}))
        }
        ("POST", ["api", "v9", "mock", "dm"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let (ch, from, text) = (b["channel"].as_u64().unwrap_or(3000), b["from"].as_u64().unwrap_or(101), b["text"].as_str().unwrap_or("").to_string());
            let after = Duration::from_millis(b["after_ms"].as_u64().unwrap_or(0));
            let w = w.clone();
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                let m = msg(fresh(), ch, from, &text);
                let mut w = w.lock().unwrap();
                w.msgs.push(m.clone());
                let _ = w.events.send(json!({ "t": "MESSAGE_CREATE", "d": m }));
            });
            ok(json!({}))
        }
        // any gateway event, after `after_ms`: calls ringing, voice states, call messages
        ("POST", ["api", "v9", "mock", "event"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let after = Duration::from_millis(b["after_ms"].as_u64().unwrap_or(0));
            let (t, d) = (b["t"].clone(), b["d"].clone());
            let w = w.clone();
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                let _ = w.lock().unwrap().events.send(json!({ "t": t, "d": d }));
            });
            ok(json!({}))
        }
        ("POST", ["api", "v9", "mock", "fail"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let status = b["status"].as_u64().unwrap_or(403) as u16;
            let reply = if b["body"].is_null() { fail_body(status) } else { b["body"].clone() };
            let f = (b["method"].as_str().unwrap_or("GET").to_string(), b["path"].as_str().unwrap_or("").to_string(), status, reply, b["times"].as_u64().unwrap_or(1) as u32);
            w.lock().unwrap().fails.push(f);
            ok(json!({}))
        }
        ("POST", ["api", "v9", "mock", "fail", "clear"]) => {
            w.lock().unwrap().fails.clear();
            ok(json!({}))
        }
        ("POST", ["api", "v9", "mock", "premium"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let tx = w.lock().unwrap().events.clone();
            let me = json!({ "id": ME.to_string(), "username": "you", "global_name": "You", "avatar": "av100", "premium_type": b["type"] });
            let _ = tx.send(json!({ "t": "USER_UPDATE", "d": me }));
            ok(json!({}))
        }
        ("POST", ["api", "v9", "channels", _, "typing"]) => (204, "application/json", Vec::new()),
        ("PUT", ["api", "v9", "channels", _, "recipients", _]) => (204, "application/json", Vec::new()),
        ("POST", ["api", "v9", "channels", ch, "followers"]) => ok(json!({ "channel_id": ch, "webhook_id": "9100" })),
        ("POST", ["api", "v9", "channels", _, "messages", _, "ack"]) => ok(json!({ "token": null })),
        ("PUT", ["api", "v9", "channels", _, "polls", _, "answers", "@me"]) => (204, "application/json", Vec::new()),
        ("GET", ["api", "v9", "channels", _, "threads", "search"]) => ok(forum_posts()),
        ("GET", ["api", "v9", "channels", ch, "pins"]) => {
            let w = w.lock().unwrap();
            let list: Vec<&Value> = w.msgs.iter().filter(|m| m["channel_id"] == *ch && m["pinned"] == json!(true)).rev().collect();
            ok(json!(list))
        }
        ("GET", ["api", "v9", "guilds", _, "messages", "search"]) | ("GET", ["api", "v9", "users", "@me", "messages", "search"]) => {
            let q = path.split("content=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_lowercase();
            let author = path.split("author_id=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_string();
            let w = w.lock().unwrap();
            let hits: Vec<Value> = w
                .msgs
                .iter()
                .rev()
                .filter(|m| (!q.is_empty() && m["content"].as_str().unwrap_or("").to_lowercase().contains(&q)) || (!author.is_empty() && m["author"]["id"] == author.as_str()))
                .take(25)
                .map(|m| json!([m]))
                .collect();
            ok(json!({ "total_results": hits.len(), "messages": hits }))
        }
        ("GET", ["api", "v9", "users", id, "profile"]) => {
            let id: u64 = id.parse().unwrap_or(0);
            ok(json!({
                "user": user(id), "user_profile": { "bio": "rustacean. writes too many side projects.", "accent_color": 0xE67E22, "pronouns": "" },
                "badges": [{ "id": "premium", "description": "Subscriber since 4/2/26", "icon": "2ba85e8901a8d6d1b8e4d9f2b7a6c3d1", "link": "https://discord.com/settings/premium" }],
                "connected_accounts": [],
                "mutual_friends": [user(102), user(103)], "mutual_friends_count": 2,
                "mutual_guilds": [{ "id": "1000", "nick": null }, { "id": "1001", "nick": null }],
                "guild_member": { "roles": member(id)["roles"], "joined_at": "2025-06-01T10:00:00+00:00", "premium_since": null },
            }))
        }
        ("GET", ["api", "v9", "content-inventory", "users", _, "outbox"]) => ok(json!({ "entries": [
            { "content": { "id": snow(now_ms() - 7_200_000).to_string(), "content_type": 1, "author_id": "101", "extra": { "game_name": "Rust Simulator", "media": { "image_url": null } } } },
            { "content": { "id": snow(now_ms() - 259_200_000).to_string(), "content_type": 6, "author_id": "101", "extra": { "artist": { "name": "marQ" } } } },
        ] })),
        ("GET", ["api", "v9", "guild-profile", id]) => ok(json!({
            "id": id, "name": "Rustaceans", "icon_hash": null, "description": "Crabs only.", "member_count": 4200, "online_count": 321,
            "tag": "RUST", "badge_hash": "", "premium_tier": 2, "premium_subscription_count": 14, "game_application_ids": [], "brand_color_primary": "#e67e22",
        })),
        ("PUT" | "DELETE", ["api", "v9", "guilds", _, "members", _, "roles", _]) => (204, "application/json", Vec::new()),
        ("GET", ["api", "v9", "users", "@me", "mentions"]) => {
            let w = w.lock().unwrap();
            let list: Vec<&Value> = w.msgs.iter().rev().filter(|m| m["mentions"].as_array().is_some_and(|u| !u.is_empty())).take(25).collect();
            ok(json!(list))
        }
        // settings pages
        ("GET", ["api", "v9", "users", "@me", "settings"]) => ok(json!({
            "explicit_content_filter": 1, "default_guilds_restricted": false, "show_current_game": true,
            "allow_accessibility_detection": false, "friend_source_flags": { "all": true },
        })),
        ("PATCH", ["api", "v9", "users", "@me", "settings"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "users", "@me", "profile"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "users", "@me", "guilds", _, "settings"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "users", "@me"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let mut u = user(ME);
            if let Some(n) = b["global_name"].as_str() {
                u["global_name"] = json!(n);
            }
            ok(u)
        }
        ("GET", ["api", "v9", "users", "@me", "email-settings"]) => ok(json!({ "initialized": true, "categories": { "communication": true, "social": true, "updates_and_announcements": false, "tips": false, "recommendations_and_events": false } })),
        ("PATCH", ["api", "v9", "users", "@me", "email-settings"]) => ok(json!({ "initialized": true, "categories": serde_json::from_slice::<Value>(body).unwrap_or_default()["settings"]["categories"] })),
        ("GET", ["api", "v9", "users", "@me", "consent"]) => ok(json!({ "usage_statistics": { "consented": true }, "personalization": { "consented": false } })),
        ("GET", ["api", "v9", "auth", "sessions"]) => ok(json!({ "user_sessions": [
            { "id_hash": "aaa", "approx_last_used_time": "2026-10-07T10:00:00+00:00", "client_info": { "os": "Windows", "platform": "Discord Client", "location": "Bangkok, Thailand" } },
            { "id_hash": "bbb", "approx_last_used_time": "2026-10-01T10:00:00+00:00", "client_info": { "os": "Android", "platform": "Discord Android", "location": "Bangkok, Thailand" } },
        ] })),
        ("GET", ["api", "v9", "content-inventory", "users", "@me"]) => ok(feed()),
        // Create Channel: the new channel, sent back and over the gateway as discord does
        ("POST", ["api", "v9", "channels", _, "invites"]) => ok(json!({ "code": "e2eInv1", "max_age": 2592000 })),
        ("PATCH", ["api", "v9", "guilds", g, "voice-states", who]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let u: u64 = if *who == "@me" { ME } else { who.parse().unwrap_or(0) };
            let mut wl = w.lock().unwrap();
            if let Some(s) = b["suppress"].as_bool() {
                if s { wl.speakers.remove(&u) } else { wl.speakers.insert(u) };
            }
            if b.get("request_to_speak_timestamp").is_some() {
                match b["request_to_speak_timestamp"].as_str() {
                    Some(t) => wl.hands.insert(u, t.to_string()),
                    None => wl.hands.remove(&u),
                };
            }
            let mut m = member(u);
            m["user"] = user(u);
            let vs = json!({ "guild_id": g, "channel_id": b["channel_id"], "user_id": u.to_string(), "session_id": "mock-voice-session", "mute": false, "deaf": false,
                "self_mute": false, "self_deaf": false, "suppress": !wl.speakers.contains(&u), "request_to_speak_timestamp": wl.hands.get(&u), "member": m });
            let _ = wl.events.send(json!({ "t": "VOICE_STATE_UPDATE", "d": vs }));
            (204, "application/json", Vec::new())
        }
        ("GET", ["api", "v9", "activities", "shelf"]) => {
            let apps: Vec<Value> = ACTIVITY_APPS.iter().map(|(id, name, desc)| json!({ "id": id.to_string(), "name": name, "description": desc, "icon": null, "flags": 0 })).collect();
            let acts: Vec<Value> = ACTIVITY_APPS
                .iter()
                .enumerate()
                .map(|(i, (id, ..))| json!({ "application_id": id.to_string(), "shelf_rank": i + 1, "client_platform_config": { "desktop": { "label_type": if i == 0 { 1 } else { 0 } } } }))
                .collect();
            ok(json!({ "activities": acts, "applications": apps, "assets": {} }))
        }
        ("POST", ["api", "v9", "activities", ch, app]) => {
            let (ch, app): (u64, u64) = (ch.parse().unwrap_or(0), app.parse().unwrap_or(0));
            activity_join(&mut w.lock().unwrap(), ch, app, ME);
            (204, "application/json", Vec::new())
        }
        ("POST", ["api", "v9", "applications", app, "activities", ch, "instances", _, "leave"]) => {
            let (ch, app): (u64, u64) = (ch.parse().unwrap_or(0), app.parse().unwrap_or(0));
            let mut wl = w.lock().unwrap();
            if let Some(a) = wl.acts.iter_mut().find(|a| a.0 == ch && a.1 == app) {
                a.3.retain(|u| *u != ME);
            }
            activity_update(&mut wl, ch, app);
            wl.acts.retain(|a| !a.3.is_empty());
            (204, "application/json", Vec::new())
        }
        ("POST", ["api", "v9", "applications", _, "proxy-tickets"]) => ok(json!({ "ticket": "mock-ticket" })),
        ("GET", ["api", "v9", "oauth2", "authorize"]) => {
            let app: u64 = query(path, "client_id").unwrap_or(0);
            match ACTIVITY_APPS.iter().find(|a| a.0 == app) {
                Some(a) => {
                    let done = w.lock().unwrap().authorized.contains(&app);
                    ok(json!({ "application": { "id": app.to_string(), "name": a.1, "icon": null }, "authorized": done, "user": user(ME) }))
                }
                None => ok(json!({ "application": { "id": "7200", "name": "Mock Arena" }, "authorized": false, "scopes": ["identify"] })),
            }
        }
        ("POST", ["api", "v9", "oauth2", "authorize"]) => {
            let app: u64 = query(path, "client_id").unwrap_or(0);
            if ACTIVITY_APPS.iter().any(|a| a.0 == app) {
                let state = path.split(['?', '&']).find_map(|p| p.strip_prefix("state=")).unwrap_or("");
                w.lock().unwrap().authorized.insert(app);
                return ok(json!({ "location": format!("https://127.0.0.1/activity-redirect?code=mock-code&state={state}") }));
            }
            let token = query_str(path, "response_type") == "token";
            let location = if token { "https://localhost/#access_token=mock-token&token_type=Bearer&expires_in=604800&scope=identify" } else { "https://localhost/?code=mock-code" };
            ok(json!({ "location": location }))
        }
        ("GET", ["api", "v9", "oauth2", "@me"]) if auth.contains("mock-access-token") => {
            ok(json!({ "application": { "id": "9100", "name": "Mock Quiz" }, "user": user(ME), "scopes": ["identify", "guilds"], "expires": iso(now_ms() + 3_600_000) }))
        }
        ("GET", ["api", "v9", "oauth2", "@me"]) => ok(json!({
            "application": { "id": "7200", "name": "Mock Arena", "description": "", "icon": null }, "user": user(ME), "scopes": ["identify"], "expires": "2099-01-01T00:00:00.000000+00:00",
        })),
        ("POST", ["api", "v9", "mock", "activity", "token"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            if b["code"] == "mock-code" { ok(json!({ "access_token": "mock-access-token" })) } else { (400, "application/json", b"{}".to_vec()) }
        }
        ("POST", ["api", "v9", "mock", "activity", "join"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let n = |k: &str| b[k].as_u64().unwrap_or(0);
            activity_join(&mut w.lock().unwrap(), n("channel"), n("app"), n("user"));
            ok(json!({}))
        }
        ("POST", ["api", "v9", "mock", "activity"]) => ok(json!({})),
        ("GET", ["activity", _]) => (200, "text/html", activity_page()),
        ("POST", ["api", "v9", "stage-instances"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let ch: u64 = b["channel_id"].as_str().and_then(|c| c.parse().ok()).unwrap_or(0);
            let inst = json!({ "id": snow(now_ms()).to_string(), "guild_id": stage_guild(ch).map(|g| g.to_string()), "channel_id": ch.to_string(), "topic": b["topic"], "privacy_level": 2 });
            let mut wl = w.lock().unwrap();
            wl.speakers.insert(ME);
            let _ = wl.events.send(json!({ "t": "STAGE_INSTANCE_CREATE", "d": inst.clone() }));
            ok(inst)
        }
        ("PATCH", ["api", "v9", "stage-instances", ch]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let inst = json!({ "id": "1", "guild_id": ch.parse::<u64>().ok().and_then(stage_guild).map(|g| g.to_string()), "channel_id": ch, "topic": b["topic"], "privacy_level": 2 });
            let _ = w.lock().unwrap().events.send(json!({ "t": "STAGE_INSTANCE_UPDATE", "d": inst.clone() }));
            ok(inst)
        }
        ("DELETE", ["api", "v9", "stage-instances", ch]) => {
            let inst = json!({ "id": "1", "guild_id": ch.parse::<u64>().ok().and_then(stage_guild).map(|g| g.to_string()), "channel_id": ch, "topic": "", "privacy_level": 2 });
            let _ = w.lock().unwrap().events.send(json!({ "t": "STAGE_INSTANCE_DELETE", "d": inst }));
            (204, "application/json", Vec::new())
        }
        ("POST", ["api", "v9", "users", "@me", "channels"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let to: Vec<u64> = b["recipients"].as_array().into_iter().flatten().filter_map(|s| s.as_str().and_then(|s| s.parse().ok())).collect();
            if to.len() > 1 {
                return ok(json!({ "id": "3011", "type": 3, "name": null, "owner_id": ME.to_string(), "last_message_id": null, "recipients": to.iter().map(|u| user(*u)).collect::<Vec<_>>() }));
            }
            ok(json!({ "id": "3010", "type": 1, "last_message_id": null, "recipients": [user(to.first().copied().unwrap_or(0))] }))
        }
        ("PUT", ["api", "v9", "guilds", "9000", "members", "@me"]) => {
            let g = json!({ "id": "9000", "name": "Preview Land", "owner_id": "102", "member_count": 40, "emojis": [], "roles": [{ "id": "9000", "name": "@everyone", "color": 0, "position": 0, "permissions": "1051648" }],
                            "channels": [{ "id": "9001", "type": 0, "guild_id": "9000", "name": "welcome", "position": 0, "permission_overwrites": [], "last_message_id": null }] });
            if path.contains("lurker=true") {
                let _ = w.lock().unwrap().events.send(json!({ "t": "GUILD_CREATE", "d": g }));
            }
            ok(json!({}))
        }
        ("POST", ["api", "v9", "guilds", "1000", "channels"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let c = json!({ "id": snow(now_ms()).to_string(), "type": b["type"], "guild_id": "1000", "name": b["name"], "position": 99,
                            "parent_id": b["parent_id"], "permission_overwrites": b["permission_overwrites"].as_array().cloned().unwrap_or_default(), "last_message_id": null });
            let _ = w.lock().unwrap().events.send(json!({ "t": "CHANNEL_CREATE", "d": c.clone() }));
            ok(c)
        }
        ("GET", ["api", "v9", "discovery", "categories"]) => ok(json!([
            { "id": 1, "name": "Gaming", "is_primary": true }, { "id": 2, "name": "Music", "is_primary": true }, { "id": 3, "name": "Entertainment", "is_primary": true },
            { "id": 5, "name": "Science & Tech", "is_primary": true }, { "id": 6, "name": "Education", "is_primary": true }, { "id": 4, "name": "Anime", "is_primary": false },
        ])),
        ("GET", ["api", "v9", "discoverable-guilds"]) => ok(json!({ "guilds": (0..8).map(discoverable).collect::<Vec<_>>(), "total": 8, "offset": 0, "limit": 30 })),
        ("GET", ["api", "v9", "discoverable-guilds", "search"]) => {
            let (q, cat) = (query_str(path, "query").to_lowercase(), query(path, "category_id"));
            let hits: Vec<Value> = (0..30).map(discoverable).filter(|g| g["name"].as_str().unwrap().to_lowercase().contains(&q) && cat.is_none_or(|c| g["primary_category_id"] == c)).collect();
            let (offset, limit) = (query(path, "offset").unwrap_or(0) as usize, query(path, "limit").unwrap_or(24) as usize);
            ok(json!({ "guilds": hits.iter().skip(offset).take(limit).collect::<Vec<_>>(), "total_count": hits.len(), "categories": [] }))
        }
        ("PUT", ["api", "v9", "guilds", _, "members", "@me"]) => (201, "application/json", b"{}".to_vec()),
        ("GET", ["api", "v9", "application-directory-static", "categories"]) => ok(json!([{ "id": 6, "name": "Games" }, { "id": 9, "name": "Social" }])),
        ("GET", ["api", "v9", "application-directory-static", "collections"]) => {
            let app = |id: &str, name: &str| json!({ "application": { "id": id, "name": name, "icon": "icon".to_string() + id, "cover_image": "cover".to_string() + id,
                                                     "description": format!("{name}, a mock app for the directory."), "categories": [6] } });
            ok(json!([{ "id": "1", "title": "Promoted Games", "position": 1, "application_directory_collection_items": [app("9101", "Rusty Racers"), app("9102", "Crab Chess")] }]))
        }
        ("GET", ["api", "v9", "application-directory-static", "search"]) => ok(json!({ "results": [], "num_pages": 0, "result_count": 0 })),
        ("GET", ["api", "v9", "guilds", g, "scheduled-events"]) => ok(json!(event_list(w, g))),
        ("POST", ["api", "v9", "guilds", g, "scheduled-events"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let id = snow(now_ms()).to_string();
            let mut e = json!({ "id": id, "guild_id": g, "creator_id": ME.to_string(), "creator": user(ME), "status": 1, "user_count": 0,
                                "entity_metadata": null, "scheduled_end_time": null });
            for (k, v) in b.as_object().into_iter().flatten() {
                e[k] = v.clone();
            }
            w.lock().unwrap().edits.insert(format!("event:{id}"), e.clone());
            ok(e)
        }
        ("PATCH", ["api", "v9", "guilds", g, "scheduled-events", id]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let Some(mut e) = event_list(w, g).into_iter().find(|e| e["id"] == *id) else { return (404, "text/plain", b"not found".to_vec()) };
            for (k, v) in b.as_object().into_iter().flatten() {
                e[k] = v.clone();
            }
            w.lock().unwrap().edits.insert(format!("event:{id}"), e.clone());
            ok(e)
        }
        ("DELETE", ["api", "v9", "guilds", _, "scheduled-events", id]) => {
            w.lock().unwrap().gone.push(format!("event:{id}"));
            (204, "application/json", Vec::new())
        }
        ("GET", ["api", "v9", "guilds", _, "scheduled-events", id, "users"]) => ok(json!([101, 103].map(|u| json!({ "guild_scheduled_event_id": id, "user": user(u), "member": { "nick": null } })))),
        ("GET", ["api", "v9", "quests", "@me"]) => ok(json!({ "quests": quest_list(w).into_iter().filter(|q| q["user_status"]["claimed_at"].is_null()).collect::<Vec<_>>(), "excluded_quests": [], "quest_enrollment_blocked_until": null })),
        ("GET", ["api", "v9", "quests", "@me", "claimed"]) => ok(json!({ "quests": quest_list(w).into_iter().filter(|q| q["user_status"]["claimed_at"].is_string()).collect::<Vec<_>>() })),
        ("POST", ["api", "v9", "quests", id, "enroll"]) => {
            let status = json!({ "user_id": ME.to_string(), "quest_id": id, "enrolled_at": iso(now_ms()), "completed_at": null, "claimed_at": null, "progress": {} });
            w.lock().unwrap().edits.insert(format!("quest:{id}"), status.clone());
            ok(status)
        }
        ("POST", ["api", "v9", "quests", id, "claim-reward"]) => {
            let Some(q) = quest_list(w).into_iter().find(|q| q["id"] == *id) else { return (404, "text/plain", b"not found".to_vec()) };
            let mut status = q["user_status"].clone();
            status["claimed_at"] = json!(iso(now_ms()));
            w.lock().unwrap().edits.insert(format!("quest:{id}"), status.clone());
            ok(json!({ "claimed_at": status["claimed_at"], "entitlements": [], "errors": [] }))
        }
        ("GET", ["api", "v9", "users", "@me", "virtual-currency", "balance"]) => {
            let claimed = quest_list(w).iter().filter(|q| q["user_status"]["claimed_at"].is_string() && q["id"] == "9003").count() as u64;
            ok(json!({ "balance": 1500 + 840 * claimed }))
        }
        ("GET", ["api", "v9", "users", "@me", "scheduled-events"]) => ok(json!([])),
        // the mock itself is the running game the client should spot
        ("GET", ["api", "v9", "applications", "detectable"]) => ok(json!([
            { "id": "7100", "name": "Mockcord Quest", "icon_hash": null, "executables": [{ "name": "mockcord.exe", "os": "win32", "is_launcher": false }] },
            { "id": "7101", "name": "Launcher Only", "icon_hash": null, "executables": [{ "name": "mockcord.exe", "os": "win32", "is_launcher": true }] },
        ])),
        // what a game's rich presence and a website's invite and template need to look up
        ("GET", ["api", "v9", "oauth2", "applications", id, "rpc"]) => match *id {
            "7100" | "7200" => ok(json!({ "id": id, "name": if *id == "7100" { "Mockcord Quest" } else { "Mock Arena" }, "icon": null, "description": "", "rpc_origins": [] })),
            _ => (404, "application/json", json!({ "message": "Unknown Application", "code": 10002 }).to_string().into_bytes()),
        },
        ("GET", ["api", "v9", "oauth2", "applications", _, "assets"]) => ok(json!([{ "id": "9001", "type": 1, "name": "logo" }, { "id": "9002", "type": 1, "name": "badge" }])),
        ("POST", ["api", "v9", "applications", _, "external-assets"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let list: Vec<Value> = b["urls"].as_array().into_iter().flatten().map(|u| json!({ "url": u, "external_asset_path": format!("external/mock/{}", u.as_str().unwrap_or("").replace("https://", "")) })).collect();
            ok(json!(list))
        }
        ("GET", ["api", "v9", "invites", "e2eRpc1"]) => ok(json!({
            "code": "e2eRpc1", "guild": { "id": "1000", "name": "Mock Server", "icon": null }, "channel": { "id": "2010", "name": "general" },
            "inviter": user(101), "approximate_presence_count": 2, "approximate_member_count": 5,
        })),
        ("POST", ["api", "v9", "invites", code]) if *code == "e2eRpc1" => ok(json!({ "code": code, "guild": { "id": "1000", "name": "Mock Server" } })),
        ("GET", ["api", "v9", "guilds", "templates", "e2eTpl1"]) => ok(json!({ "code": "e2eTpl1", "name": "Mock Template", "description": "A starting point.", "usage_count": 3 })),
        ("POST", ["api", "v9", "guilds", "templates", "e2eTpl1"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            ok(json!({ "id": "1999", "name": b["name"] }))
        }
        ("GET", ["api", "v9", "invites", _]) | ("GET", ["api", "v9", "guilds", "templates", _]) => {
            (404, "application/json", json!({ "message": "Unknown Invite", "code": 10006 }).to_string().into_bytes())
        }
        ("GET", ["api", "v9", "guilds", "1000", "roles", "member-counts"]) => ok(json!({ "1100": 1, "1101": 3, "1102": 1, "1103": 1, "1104": 1 })),
        ("GET", ["api", "v9", "guilds", "1000", "roles", _, "member-ids"]) => ok(json!(["101", "102"])),
        ("GET", ["api", "v9", "guilds", "1000", "roles", _, "connection-configurations"]) => ok(json!([])),
        ("PUT", ["api", "v9", "guilds", "1000", "roles", _, "connection-configurations"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("PATCH", ["api", "v9", "guilds", "1000", "roles", _, "members"]) => (204, "application/json", Vec::new()),
        ("POST", ["api", "v9", "guilds", "1000", "roles"]) => {
            let mut b: Value = serde_json::from_slice(body).unwrap_or_default();
            b["id"] = json!("1105");
            b["color"] = json!(0);
            b["position"] = json!(1);
            b["permissions"] = json!("1071698660929");
            ok(b)
        }
        ("PATCH", ["api", "v9", "guilds", "1000", "roles"]) => ok(json!([])),
        ("PATCH", ["api", "v9", "guilds", "1000", "roles", id]) => {
            let mut b: Value = serde_json::from_slice(body).unwrap_or_default();
            b["id"] = json!(id);
            b["name"] = b.get("name").cloned().unwrap_or(json!("Role"));
            b["color"] = b.get("color").cloned().unwrap_or(json!(0));
            b["position"] = json!(1);
            b["permissions"] = b.get("permissions").cloned().unwrap_or(json!("0"));
            ok(b)
        }
        ("DELETE", ["api", "v9", "guilds", "1000", "roles", _]) => (204, "application/json", Vec::new()),
        ("GET", ["api", "v9", "guilds", "1000", "emojis"]) => ok(json!([
            { "id": "5300", "name": "omgMods", "animated": false, "user": user(102) },
        ])),
        ("PATCH", ["api", "v9", "guilds", "1000", "emojis", _]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("GET", ["api", "v9", "guilds", "1000", "stickers"]) => ok(json!([
            { "id": "5400", "name": "wave", "tags": "wave", "description": "", "format_type": 1, "user": user(102) },
        ])),
        ("GET", ["api", "v9", "guilds", "1000", "invites"]) => ok(json!([
            { "code": "e2eInv1", "uses": 3, "max_uses": 0, "inviter": user(102), "channel": { "name": "general" }, "expires_at": iso(now_ms() + 86_400_000) },
        ])),
        ("GET", ["api", "v9", "guilds", "1000"]) => ok(json!({ "id": "1000", "verification_level": 1, "explicit_content_filter": 0, "mfa_level": 0, "owner_id": "102", "safety_alerts_channel_id": null })),
        ("PATCH", ["api", "v9", "guilds", "1000"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("POST", ["api", "v9", "guilds", "1000", "mfa"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("GET", ["api", "v9", "guilds", "1000", "bans"]) => ok(json!([{ "user": user(104), "reason": "spamming links" }])),
        ("DELETE", ["api", "v9", "guilds", "1000", "bans", _]) => (204, "application/json", Vec::new()),
        ("GET", ["api", "v9", "guilds", "1000", "audit-logs"]) => ok(json!({
            "users": [user(102), user(103)],
            "audit_log_entries": [
                { "id": "1500000000000000100", "user_id": "102", "target_id": "103", "action_type": 25, "changes": [{ "key": "$add", "new_value": [{ "id": "1101", "name": "Mods" }] }] },
                { "id": "1500000000000000000", "user_id": "102", "target_id": "1100", "action_type": 31, "reason": "tidy up", "changes": [{ "key": "name", "old_value": "Old", "new_value": "Admins" }] },
            ],
        })),
        ("GET", ["api", "v9", "guilds", "1000", "integrations"]) => ok(json!([
            { "id": "7700", "name": "Dyno", "type": "discord", "user": user(102), "scopes": ["bot", "applications.commands"], "application": { "id": "104", "name": "Dyno", "icon": null, "description": "A multipurpose bot.", "bot": { "id": "104", "public_flags": 65536 } } },
        ])),
        ("GET", ["api", "v9", "guilds", "1000", "webhooks"]) => ok(json!([
            { "id": "8800", "type": 1, "name": "Deploys", "avatar": null, "channel_id": "2001", "token": "tok" },
        ])),
        ("POST", ["api", "v9", "channels", _, "webhooks"]) => ok(json!({ "id": "8801", "type": 1, "name": "Spidey Bot", "avatar": null, "channel_id": "2001", "token": "tok2" })),
        ("PATCH", ["api", "v9", "webhooks", _]) | ("DELETE", ["api", "v9", "webhooks", _]) => ok(json!({})),
        ("GET", ["api", "v9", "guilds", "1000", "auto-moderation", "rules"]) => ok(json!([
            { "id": "9100", "name": "Bad words", "enabled": true, "event_type": 1, "trigger_type": 1, "trigger_metadata": { "keyword_filter": ["heck", "darn"] }, "actions": [{ "type": 1 }], "exempt_roles": [], "exempt_channels": [] },
        ])),
        ("POST", ["api", "v9", "guilds", "1000", "auto-moderation", "rules"]) => {
            let mut b: Value = serde_json::from_slice(body).unwrap_or_default();
            b["id"] = json!("9101");
            ok(b)
        }
        ("PATCH", ["api", "v9", "guilds", "1000", "auto-moderation", "rules", id]) => {
            let mut b: Value = serde_json::from_slice(body).unwrap_or_default();
            b["id"] = json!(id);
            ok(b)
        }
        ("GET", ["api", "v9", "guilds", g, "soundboard-sounds"]) => {
            let kept = w.lock().unwrap().edits.get(&format!("s:{g}")).cloned();
            ok(json!({ "items": kept.unwrap_or_else(default_sounds) }))
        }
        // the boosts page: discord's powerups catalogue, what Rust Lounge has on, and its boosters
        ("GET", ["api", "v9", "store", "published-listings", "skus"]) => {
            let port = PORT.load(std::sync::atomic::Ordering::Relaxed);
            let listing = |sku: u64, name: &str, desc: &str, cost: u64, perk: bool| {
                let meta = json!({ "boost_price": cost, "category_type": if perk { "perk" } else { "level" },
                                   "static_image_url": format!("http://127.0.0.1:{port}/cdn/media/perk{sku}.png") });
                json!({ "id": (sku + 1).to_string(), "description": desc, "sku": { "id": sku.to_string(), "name": name,
                        "tenant_metadata": { "guild_monetization": { "powerup": meta } } } })
            };
            ok(json!([
                listing(7001, "Level 1", "Level 1", 2, false), listing(7002, "Level 2", "Level 2", 5, false), listing(7003, "Level 3", "Level 3", 7, false),
                listing(7010, "Server Tag", "Let your members represent your server everywhere on Discord.", 3, true),
                listing(7011, "Enhanced Role Styles", "Make certain roles stand out with animated, vibrant gradients.", 3, true),
                listing(7012, "Pet Badge Pack", "Pet Badge Pack", 3, true), listing(7013, "Plant Badge Pack", "Plant Badge Pack", 3, true),
            ]))
        }
        ("GET", ["api", "v9", "guilds", "1000", "powerups"]) => ok(json!([
            { "id": "8001", "sku_id": "7001", "deleted": false }, { "id": "8002", "sku_id": "7010", "deleted": false },
        ])),
        ("GET", ["api", "v9", "guilds", "1000", "premium", "subscriptions"]) => {
            let now = now_ms();
            ok(json!([
                { "id": snow(now - 3_600_000).to_string(), "user_id": "101", "ended": false, "user": user(101) },
                { "id": snow(now - 20 * 86_400_000).to_string(), "user_id": "102", "ended": false, "ends_at": iso(now + 86_400_000), "user": user(102) },
                { "id": snow(now - 40 * 86_400_000).to_string(), "user_id": "103", "ended": true, "ends_at": iso(now - 2 * 86_400_000), "user": user(103) },
            ]))
        }
        ("GET", ["api", "v9", "applications", "public"]) => ok(json!([
            { "id": "9001", "name": "Factorio", "icon": "appicon9001" },
            { "id": "9002", "name": "Wordle", "icon": "appicon9002" },
            { "id": "9004", "name": "Genshin Impact", "icon": "appicon9004" },
            { "id": "9100", "name": "Mock Quiz", "icon": null, "description": "A quiz for the whole call" },
            { "id": "9101", "name": "Mock Draw", "icon": null, "description": "Draw something together" },
        ])),
        ("GET", ["api", "v9", "oauth2", "tokens"]) => ok(json!([
            { "id": "77", "scopes": ["identify", "guilds"], "application": { "id": "104", "name": "Dyno", "icon": null, "description": "A multipurpose bot." } },
        ])),
        ("GET", ["api", "v9", "users", "@me", "connections"]) => ok(json!([
            { "type": "github", "id": "1", "name": "you-on-github", "visibility": 1, "show_activity": true, "verified": true },
        ])),
        ("GET", ["api", "v9", "users", "@me", "billing", ..]) | ("GET", ["api", "v9", "users", "@me", "entitlements", ..]) => ok(json!([])),
        // a thread from a message or from nothing: the new channel back, and over the gateway
        ("POST", ["api", "v9", "channels", ch, "messages", _, "threads"]) | ("POST", ["api", "v9", "channels", ch, "threads"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let t = json!({ "id": snow(now_ms()).to_string(), "type": 11, "guild_id": "1000", "parent_id": ch, "name": b["name"],
                            "owner_id": ME.to_string(), "message_count": 0, "member_count": 1, "last_message_id": null,
                            "thread_metadata": { "archived": false, "auto_archive_duration": 4320 }, "member": { "user_id": ME.to_string() } });
            let _ = w.lock().unwrap().events.send(json!({ "t": "THREAD_CREATE", "d": t.clone() }));
            ok(t)
        }
        ("PUT", ["api", "v9", "users", "@me", "saved-messages", ch, id]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let mut w = w.lock().unwrap();
            let message = w.msgs.iter().find(|m| m["id"] == *id).cloned();
            w.saved.retain(|s| s["save_data"]["message_id"] != *id);
            w.saved.push(json!({ "message": message, "save_data": { "channel_id": ch, "message_id": id, "saved_at": iso(now_ms()), "due_at": b["due_at"] } }));
            ok(json!({}))
        }
        ("DELETE", ["api", "v9", "users", "@me", "saved-messages", _, id]) => {
            w.lock().unwrap().saved.retain(|s| s["save_data"]["message_id"] != *id);
            (204, "application/json", Vec::new())
        }
        ("GET", ["api", "v9", "users", "@me", "saved-messages", "bookmarks"]) => {
            let w = w.lock().unwrap();
            let list: Vec<&Value> = w.saved.iter().filter(|s| s["save_data"]["due_at"].is_null()).rev().collect();
            ok(json!({ "results": list, "has_more": false }))
        }
        ("GET", ["api", "v9", "users", "@me", "saved-messages"]) => {
            let w = w.lock().unwrap();
            let ids: Vec<Value> = w.saved.iter().filter(|s| s["save_data"]["due_at"].is_null()).map(|s| s["save_data"]["message_id"].clone()).collect();
            let reminders: Vec<&Value> = w.saved.iter().filter(|s| !s["save_data"]["due_at"].is_null()).collect();
            ok(json!({ "bookmark_ids": ids, "reminders": reminders }))
        }
        ("PUT", ["api", "v9", "users", "@me", "notes", _]) => (204, "application/json", Vec::new()),
        ("PATCH", ["api", "v9", "users", "@me", "relationships", _]) => (204, "application/json", Vec::new()),
        ("PUT" | "DELETE", ["api", "v9", "users", "@me", "relationships", _, "ignore"]) => (204, "application/json", Vec::new()),
        ("GET", ["api", "v9", "users", "@me", "application-command-index"]) => ok(json!({
            "applications": [{ "id": "104", "name": "Dyno", "icon": null }],
            "application_commands": [{ "id": "9101", "application_id": "104", "version": "1", "type": 2, "name": "Warn User", "options": [] }],
        })),
        // the members table: everyone, or the usernames asked for, newest first
        ("POST", ["api", "v9", "guilds", g, "members-search"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let words: Vec<String> = b["and_query"]["usernames"]["or_query"].as_array().into_iter().flatten().filter_map(|w| w.as_str().map(str::to_lowercase)).collect();
            let list: Vec<Value> = [101u64, 102, 103, 104, 105]
                .iter()
                .enumerate()
                .filter(|(_, id)| words.is_empty() || words.iter().any(|w| user(**id)["username"].as_str().unwrap_or("").contains(w.as_str())))
                .map(|(i, id)| {
                    let mut m = member(*id);
                    m["user"] = user(*id);
                    m["joined_at"] = json!(iso(now_ms() - (i as u64 + 1) * 5 * 86_400_000));
                    let code = (*id != 104).then_some("e2eInv1");
                    json!({ "member": m, "source_invite_code": code, "join_source_type": if code.is_some() { 5 } else { 1 }, "inviter_id": code.map(|_| "102") })
                })
                .collect();
            ok(json!({ "guild_id": g, "members": list, "page_result_count": list.len(), "total_result_count": list.len() }))
        }
        ("GET", ["api", "v9", "guilds", "1001", "onboarding"]) => ok(json!({
            "guild_id": "1001", "enabled": true, "mode": 0, "default_channel_ids": ["2100"], "responses": ["7101"],
            "prompts": [{ "id": "7001", "title": "What do you play?", "single_select": false, "required": false, "in_onboarding": true, "type": 0, "options": [
                { "id": "7101", "title": "Shooters", "description": "", "emoji": { "id": null, "name": "🎯" }, "role_ids": [], "channel_ids": ["2101"] },
                { "id": "7102", "title": "Puzzles", "description": "", "emoji": { "id": null, "name": "🧩" }, "role_ids": [], "channel_ids": [] },
            ] }],
        })),
        ("PUT", ["api", "v9", "guilds", _, "onboarding-responses"]) => ok(serde_json::from_slice(body).unwrap_or_default()),
        ("GET", ["api", "v9", "guilds", _, "prune"]) => ok(json!({ "pruned": 2 })),
        ("GET", ["api", "v9", "guilds", _, "regions"]) => ok(json!([
            { "id": "us-east", "name": "US East", "optimal": true, "deprecated": false },
            { "id": "rotterdam", "name": "Rotterdam", "optimal": false, "deprecated": false },
        ])),
        ("GET", ["api", "v9", "channels", _, "invites"]) => ok(json!([
            { "code": "e2eInv1", "uses": 3, "max_uses": 0, "expires_at": null, "inviter": user(102) },
        ])),
        ("GET", ["api", "v9", "channels", _, "webhooks"]) => ok(json!([])),
        // a channel as its settings page asks for it, with what was saved over it
        ("GET", ["api", "v9", "channels", ch]) => match channel(w, ch) {
            Some(c) => ok(c),
            None => (404, "text/plain", b"not found".to_vec()),
        },
        ("PATCH", ["api", "v9", "channels", ch]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let Some(mut c) = channel(w, ch) else { return (404, "text/plain", b"not found".to_vec()) };
            let thread = c["type"] == 11;
            for (k, v) in b.as_object().into_iter().flatten() {
                if thread && (k == "archived" || k == "locked") {
                    c["thread_metadata"][k] = v.clone();
                } else {
                    c[k] = v.clone();
                }
            }
            let mut w = w.lock().unwrap();
            w.edits.insert(ch.to_string(), c.clone());
            let _ = w.events.send(json!({ "t": if thread { "THREAD_UPDATE" } else { "CHANNEL_UPDATE" }, "d": c.clone() }));
            ok(c)
        }
        ("DELETE", ["api", "v9", "channels", ch]) => {
            let Some(c) = channel(w, ch) else { return (404, "text/plain", b"not found".to_vec()) };
            let mut w = w.lock().unwrap();
            w.gone.push(ch.to_string());
            let _ = w.events.send(json!({ "t": if c["type"] == 11 { "THREAD_DELETE" } else { "CHANNEL_DELETE" }, "d": c.clone() }));
            ok(c)
        }
        ("GET", ["api", "v9", "guilds", g]) if guild_full(w, g).is_some() => ok(guild_full(w, g).unwrap()),
        ("PATCH", ["api", "v9", "guilds", g]) if guild_full(w, g).is_some() => {
            keep(w, format!("g:{g}"), body);
            guild_update(w, g);
            ok(guild_full(w, g).unwrap())
        }
        ("DELETE", ["api", "v9", "guilds", g]) => {
            let _ = w.lock().unwrap().events.send(json!({ "t": "GUILD_DELETE", "d": { "id": g } }));
            (204, "application/json", Vec::new())
        }
        ("GET", ["api", "v9", "guilds", g, "profile"]) => ok(profile_full(w, g).unwrap_or_default()),
        ("PATCH", ["api", "v9", "guilds", g, "profile"]) => {
            keep(w, format!("p:{g}"), body);
            ok(profile_full(w, g).unwrap_or_default())
        }
        ("GET", ["api", "v9", "guilds", g, "widget"]) => ok(w.lock().unwrap().edits.get(&format!("w:{g}")).cloned().unwrap_or(json!({ "enabled": false, "channel_id": null }))),
        ("PATCH", ["api", "v9", "guilds", g, "widget"]) => {
            keep(w, format!("w:{g}"), body);
            ok(w.lock().unwrap().edits.get(&format!("w:{g}")).cloned().unwrap_or_default())
        }
        ("GET", ["api", "v9", "guilds", g, "vanity-url"]) => ok(w.lock().unwrap().edits.get(&format!("v:{g}")).cloned().unwrap_or(json!({ "code": null, "uses": 0 }))),
        ("PATCH", ["api", "v9", "guilds", g, "vanity-url"]) => {
            keep(w, format!("v:{g}"), body);
            ok(w.lock().unwrap().edits.get(&format!("v:{g}")).cloned().unwrap_or_default())
        }
        ("GET", ["api", "v9", "guilds", g, "member-verification"]) => ok(w.lock().unwrap().edits.get(&format!("m:{g}")).cloned().unwrap_or(json!({ "version": "1", "description": null, "form_fields": [] }))),
        ("PATCH", ["api", "v9", "guilds", g, "member-verification"]) => {
            keep(w, format!("m:{g}"), body);
            let form = w.lock().unwrap().edits.get(&format!("m:{g}")).cloned().unwrap_or_default();
            let apply = form["form_fields"].as_array().is_some_and(|f| f.iter().any(|x| x["field_type"] != "TERMS"));
            let mut feats: Vec<Value> = guild_full(w, g).map(|f| f["features"].as_array().cloned().unwrap_or_default()).unwrap_or_default();
            feats.retain(|f| f != "MEMBER_VERIFICATION_MANUAL_APPROVAL" && f != "MEMBER_VERIFICATION_GATE_ENABLED");
            if form["enabled"] == json!(true) {
                feats.push(json!(if apply { "MEMBER_VERIFICATION_MANUAL_APPROVAL" } else { "MEMBER_VERIFICATION_GATE_ENABLED" }));
            }
            keep(w, format!("g:{g}"), json!({ "features": feats }).to_string().as_bytes());
            guild_update(w, g);
            ok(form)
        }
        ("GET", ["api", "v9", "guilds", g, "templates"]) => ok(w.lock().unwrap().edits.get(&format!("t:{g}")).map(|t| json!([t])).unwrap_or(json!([]))),
        ("POST", ["api", "v9", "guilds", g, "templates"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let t = json!({ "code": format!("mock{g}"), "name": b["name"], "description": b["description"], "usage_count": 0, "is_dirty": false });
            w.lock().unwrap().edits.insert(format!("t:{g}"), t.clone());
            ok(t)
        }
        ("PATCH", ["api", "v9", "guilds", g, "templates", _]) => {
            keep(w, format!("t:{g}"), body);
            ok(w.lock().unwrap().edits.get(&format!("t:{g}")).cloned().unwrap_or_default())
        }
        ("PUT", ["api", "v9", "guilds", g, "templates", _]) => ok(w.lock().unwrap().edits.get(&format!("t:{g}")).cloned().unwrap_or_default()),
        ("DELETE", ["api", "v9", "guilds", g, "templates", _]) => {
            w.lock().unwrap().edits.remove(&format!("t:{g}"));
            ok(json!({}))
        }
        ("POST", ["api", "v9", "guilds", g, "soundboard-sounds"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let s = json!({ "sound_id": snow(now_ms()).to_string(), "name": b["name"], "volume": b["volume"], "emoji_name": b["emoji_name"], "emoji_id": b["emoji_id"], "user": user(ME) });
            let mut w = w.lock().unwrap();
            let list = w.edits.entry(format!("s:{g}")).or_insert_with(|| default_sounds());
            list.as_array_mut().unwrap().push(s.clone());
            ok(s)
        }
        ("PATCH", ["api", "v9", "guilds", g, "soundboard-sounds", id]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let mut w = w.lock().unwrap();
            let list = w.edits.entry(format!("s:{g}")).or_insert_with(|| default_sounds());
            for s in list.as_array_mut().unwrap().iter_mut().filter(|s| s["sound_id"] == *id) {
                for (k, v) in b.as_object().into_iter().flatten() {
                    s[k] = v.clone();
                }
            }
            ok(json!({}))
        }
        ("DELETE", ["api", "v9", "guilds", g, "soundboard-sounds", id]) => {
            let mut w = w.lock().unwrap();
            let list = w.edits.entry(format!("s:{g}")).or_insert_with(|| default_sounds());
            list.as_array_mut().unwrap().retain(|s| s["sound_id"] != *id);
            (204, "application/json", Vec::new())
        }
        ("PATCH", ["api", "v9", "channels", ch, "messages", id]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            if b["content"].as_str().is_some_and(|c| c.chars().count() > 2000) {
                return (400, "application/json", fail_body(400).to_string().into_bytes());
            }
            let mut w = w.lock().unwrap();
            let Some(m) = w.msgs.iter_mut().find(|m| m["channel_id"] == *ch && m["id"] == *id) else {
                return ok(json!({}));
            };
            if let Some(c) = b.get("content") {
                m["content"] = c.clone();
                m["edited_timestamp"] = json!(iso(now_ms()));
            }
            if let Some(f) = b["flags"].as_u64() {
                m["flags"] = json!(f);
                if f & 4 != 0 {
                    m["embeds"] = json!([]);
                }
            }
            let m = m.clone();
            let _ = w.events.send(json!({ "t": "MESSAGE_UPDATE", "d": m }));
            ok(m)
        }
        ("DELETE", ["api", "v9", "channels", ch, "messages", id]) => {
            let mut w = w.lock().unwrap();
            let Some(at) = w.msgs.iter().position(|m| m["channel_id"] == *ch && m["id"] == *id) else {
                return (204, "application/json", Vec::new());
            };
            w.msgs.remove(at);
            let _ = w.events.send(json!({ "t": "MESSAGE_DELETE", "d": { "id": id, "channel_id": ch } }));
            (204, "application/json", Vec::new())
        }
        ("PUT" | "DELETE", ["api", "v9", "channels", ch, "messages", "pins", id]) => {
            let mut w = w.lock().unwrap();
            let Some(m) = w.msgs.iter_mut().find(|m| m["channel_id"] == *ch && m["id"] == *id) else {
                return (204, "application/json", Vec::new());
            };
            m["pinned"] = json!(method == "PUT");
            let _ = w.events.send(json!({ "t": "CHANNEL_PINS_UPDATE", "d": { "channel_id": ch, "last_pin_timestamp": iso(now_ms()) } }));
            (204, "application/json", Vec::new())
        }
        ("DELETE", ["api", "v9", "guilds", _, "members", _]) | ("PUT", ["api", "v9", "guilds", _, "bans", _]) | ("PATCH", ["api", "v9", "guilds", _, "members", _]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            if b["nick"].as_str().is_some_and(|n| n.chars().count() > 32) {
                let reply = json!({ "code": 50035, "message": "Invalid Form Body", "errors": { "nick": { "_errors": [{ "code": "BASE_TYPE_MAX_LENGTH", "message": "Must be 32 or fewer in length." }] } } });
                return (400, "application/json", reply.to_string().into_bytes());
            }
            (204, "application/json", Vec::new())
        }
        (_, ["api", ..]) => ok(json!({})),
        ("GET", ["cdn", "emojis", f]) if f.ends_with(".gif") => (200, "image/gif", anim_gif(48, 48, 0)),
        ("GET", ["cdn", "media", "clip.mp4"]) if query(path, "width").is_some() => (200, "image/png", png(320, 180, 5, false)),
        ("GET", ["cdn", "media", "clip.mp4"]) => (200, "video/mp4", include_bytes!("fixtures/clip.mp4").to_vec()),
        ("GET", ["cdn", "media", "tone.mp3"]) => (200, "audio/mpeg", include_bytes!("fixtures/tone.mp3").to_vec()),
        ("GET", ["cdn", "media", "hello.rs"]) => (200, "text/plain", include_bytes!("fixtures/hello.rs").to_vec()),
        ("GET", ["cdn", "media", f]) if f.ends_with(".mp4") => (200, "video/mp4", include_bytes!("fixtures/spin.mp4").to_vec()),
        ("GET", ["cdn", "media", f]) if f.ends_with(".gif") => (200, "image/gif", anim_gif(if f.starts_with("dance") { 120 } else { 96 }, if f.starts_with("dance") { 90 } else { 72 }, f.len() as u32)),
        ("GET", ["cdn", "media", ..]) => (200, "image/png", png(160, 120, 3, false)),
        ("GET", ["cdn", "assets", "content", f]) => (200, "image/png", png(640, 360, f.bytes().map(u64::from).sum(), false)),
        ("GET", ["cdn", "soundboard-sounds", _]) => (200, "audio/ogg", b"OggS".to_vec()),
        ("GET", ["robots.txt"]) => (200, "text/plain", b"User-agent: *".to_vec()),
        ("GET", ["store" | "shop"]) => (
            200,
            "text/html",
            br#"<title>Discord</title><script>fetch("/api/v9/mock/webwin",{method:"POST",body:JSON.stringify({page:location.pathname,token:localStorage.getItem("token")})})</script>"#.to_vec(),
        ),
        ("GET", ["app"]) => (200, "text/html", br#"<link href="/assets/other.css" rel="stylesheet"><link href="/assets/fonts.css" rel="stylesheet">"#.to_vec()),
        ("GET", ["assets", "other.css"]) => (200, "text/css", b"body{margin:0}".to_vec()),
        ("GET", ["assets", "fonts.css"]) => {
            // same shape as discord's: upright and italic faces, plus a family we don't want
            let face = |fam: &str, style: &str, w: u16| {
                let file = fam.trim_start_matches("gg ");
                format!("@font-face{{font-display:swap;font-family:{fam};font-style:{style};font-weight:{w};src:url(/assets/mock-{file}{w}{style}.woff2) format(\"woff2\")}}")
            };
            let mut css = String::new();
            for w in [400, 500, 600, 700] {
                css += &face("gg sans", "normal", w);
                css += &face("gg sans", "italic", w);
            }
            css += &face("gg mono", "normal", 400);
            css += &face("gg mono", "normal", 700);
            css += &face("Source Code Pro", "normal", 400);
            (200, "text/css", css.into_bytes())
        }
        // the nitro display name fonts, by their real asset names
        ("GET", ["assets", f]) if NAME_FONT_ASSETS.iter().any(|h| f.strip_suffix(".woff2") == Some(*h)) => (200, "font/woff2", gg_stand_in("sans400normal")),
        // discord's sounds: whichever of the mp3s in MOCK_SOUNDS the asset name lands on
        ("GET", ["assets", f]) if f.ends_with(".mp3") => {
            let mut files: Vec<_> = std::env::var_os("MOCK_SOUNDS").and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten().map(|e| e.path()).collect();
            files.sort();
            let pick = f.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
            match files.get(pick % files.len().max(1)).and_then(|p| std::fs::read(p).ok()) {
                Some(b) => (200, "audio/mpeg", b),
                None => (404, "text/plain", Vec::new()),
            }
        }
        ("GET", ["assets", f]) if f.starts_with("mock-") && f.ends_with(".woff2") => (200, "font/woff2", gg_stand_in(&f[5..f.len() - 6])),
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
            let text: String = String::from_utf8_lossy(&body).chars().filter(|c| !matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}')).collect();
            let line = json!({ "t": now_ms(), "method": method, "path": path, "body": text, "locale": locale, "auth": header("authorization") });
            let _ = writeln!(wl.log, "{line}");
        }
        let mfa = header("x-discord-mfa-authorization");
        let (code, ctype, out) = match account::route(&method, &path, &body, mfa.as_deref(), &user(ME), &user) {
            Some((code, v)) => (code, "application/json", if v.is_null() { Vec::new() } else { v.to_string().into_bytes() }),
            None => route(&w, &method, &path, &header("authorization").unwrap_or_default(), &body).await,
        };
        let reason = match code {
            200 => "OK",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
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
                    if matches!(v["op"].as_u64(), Some(3 | 4 | 18..=22 | 37)) {
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
                        // a fresh session isn't in a call, whatever the last client left behind, and
                        // finds the channels READY lists as they were
                        {
                            let mut w = w.lock().unwrap();
                            w.voice_chan = None;
                            w.edits.clear();
                            w.gone.clear();
                        }
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
                    Some(37) => {
                        let mut out = vec![("GUILD_MEMBER_LIST_UPDATE".into(), member_list())];
                        for (g, s) in v["d"]["subscriptions"].as_object().into_iter().flatten() {
                            for t in s["thread_member_lists"].as_array().into_iter().flatten() {
                                out.push(("THREAD_MEMBER_LIST_UPDATE".into(), thread_members(t.as_str().unwrap_or_default(), g)));
                            }
                        }
                        out
                    }
                    // voice channel statuses and when their calls began, as discord answers op 43
                    Some(43) => vec![("CHANNEL_INFO".into(), json!({ "guild_id": v["d"]["guild_id"], "channels": [
                        { "id": "2021", "status": null, "voice_start_time": now_ms() / 1000 - 3725 },
                        { "id": "2022", "status": "ranked grind", "voice_start_time": null },
                    ] }))],
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
    wl.clip_flags = d["flags"].as_u64().unwrap_or(0);
    let stage = chan.and_then(stage_guild);
    if moved {
        wl.speakers.remove(&ME);
        wl.hands.remove(&ME);
    }
    let mut m = member(ME);
    m["user"] = user(ME);
    let me = json!({
        "guild_id": guild.map(|g| g.to_string()), "channel_id": chan.map(|c| c.to_string()), "user_id": ME.to_string(),
        "session_id": "mock-voice-session", "self_mute": d["self_mute"], "self_deaf": d["self_deaf"], "self_video": d["self_video"],
        "mute": false, "deaf": false, "suppress": stage.is_some() && !wl.speakers.contains(&ME),
        "request_to_speak_timestamp": wl.hands.get(&ME), "member": m,
    });
    let mut out = vec![("VOICE_STATE_UPDATE".to_string(), me)];
    let Some(ch) = chan else { return out };
    if !moved {
        return out;
    }
    let endpoint = format!("ws://127.0.0.1:{}", wl.voice_port);
    // Alice is always on camera and streaming her screen, so there's something to watch
    let mut alice = json!({ "guild_id": guild.map(|g| g.to_string()), "channel_id": ch.to_string(), "user_id": SPEAKER.to_string(),
        "session_id": "other", "self_mute": false, "self_deaf": false, "mute": guild.is_some(), "deaf": false, "self_video": true, "self_stream": true });
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
    if stage.is_some() {
        alice["self_video"] = json!(false);
        alice["self_stream"] = json!(false);
        alice["mute"] = json!(false);
        alice["suppress"] = json!(false);
        for (u, hand) in [(102u64, Some("2026-11-05T10:00:00.000Z")), (103, None)] {
            let mut m = member(u);
            m["user"] = user(u);
            out.push(("VOICE_STATE_UPDATE".into(), json!({ "guild_id": guild.map(|g| g.to_string()), "channel_id": ch.to_string(), "user_id": u.to_string(),
                "session_id": format!("other{u}"), "self_mute": false, "self_deaf": false, "mute": false, "deaf": false, "suppress": true, "request_to_speak_timestamp": hand, "member": m })));
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
        // as discord does once the stream is up and whenever viewers change: nothing to reconnect for
        22 => return vec![("STREAM_UPDATE".into(), json!({ "stream_key": d["stream_key"], "paused": d["paused"], "viewer_ids": [], "region": "local" }))],
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
    let mut told_clips = false;
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
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        let _ = tx.send(Ws::text(json!({ "op": 6, "d": { "t": d["t"] } }).to_string())).await;
                        // once the client's voice state says it's clipping, the other speaker says so too
                        if kind == Kind::Voice && !told_clips && w.lock().unwrap().clip_flags & 1 != 0 {
                            told_clips = true;
                            out.push(Reply::Json(json!({ "op": 18, "d": { "user_id": SPEAKER.to_string(), "flags": 3 } })));
                        }
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
    loop {
        // windows hands back a send to a port that has since closed (a stream the client stopped
        // watching) as an error on the next receive; the socket itself is fine
        let Ok((n, from)) = udp.recv_from(&mut buf).await else { continue };
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
    if let Some(port) = args.get(5).and_then(|p| p.parse().ok()) {
        tokio::spawn(auth::gateway(port));
    }
    let cdn = format!("http://127.0.0.1:{http_port}/cdn");
    PORT.store(http_port, std::sync::atomic::Ordering::Relaxed);
    account::init(&settings_proto(1, true));
    let (events, _) = broadcast::channel(256);
    let w: W = Arc::new(Mutex::new(World {
        msgs: seed(&cdn),
        log: std::fs::File::create(&log).expect("log file"),
        events,
        voice_port,
        voice_chan: None,
        clip_flags: 0,
        voices: Vec::new(),
        saved: Vec::new(),
        edits: Default::default(),
        gone: Vec::new(),
        speakers: Default::default(),
        hands: Default::default(),
        acts: Vec::new(),
        authorized: Default::default(),
        fails: Vec::new(),
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
