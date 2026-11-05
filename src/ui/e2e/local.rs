//! Steps 1600 to 1699: a game on the rich presence pipe, a website on the 6463 socket, and a
//! second launch handing over a discord:// link, each against the real services in this process.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::Key;
use serde_json::{Value, json};

use super::Run;
use crate::state::Id;
use crate::ui::App;

#[derive(Default)]
struct Shared {
    release: AtomicBool,
    out: Mutex<Value>,
}

#[derive(Default)]
pub struct State {
    game: Arc<Shared>,
    web: Arc<Shared>,
    exit: Arc<Mutex<Option<i32>>>,
    home: (Option<Id>, Id),
    seen: Option<egui::Rect>,
    target: Id,
}

fn set(s: &Shared, k: &str, v: Value) {
    if let Ok(mut o) = s.out.lock() {
        o[k] = v;
    }
}

fn read(s: &Shared) -> Value {
    s.out.lock().map(|o| o.clone()).unwrap_or_default()
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn put(f: &mut File, op: u32, v: &Value) {
    let b = v.to_string();
    let mut o = op.to_le_bytes().to_vec();
    o.extend((b.len() as u32).to_le_bytes());
    o.extend(b.as_bytes());
    let _ = f.write_all(&o);
}

fn get(f: &mut File) -> Option<(u32, Value)> {
    let mut h = [0u8; 8];
    f.read_exact(&mut h).ok()?;
    let op = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let mut b = vec![0; u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize];
    f.read_exact(&mut b).ok()?;
    Some((op, serde_json::from_slice(&b).unwrap_or_default()))
}

fn cmd(f: &mut File, cmd: &str, evt: Option<&str>, args: Value) -> Value {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let nonce = format!("n{}", N.fetch_add(1, Relaxed));
    put(f, 1, &json!({ "cmd": cmd, "evt": evt, "args": args, "nonce": nonce }));
    while let Some((op, v)) = get(f) {
        if op == 1 && v["nonce"] == nonce {
            return v;
        }
    }
    Value::Null
}

fn dial(name: &str) -> Option<File> {
    for _ in 0..50 {
        if let Ok(f) = OpenOptions::new().read(true).write(true).open(name) {
            return Some(f);
        }
        pause(100);
    }
    None
}

fn code(v: &Value) -> Value {
    v["data"]["code"].clone()
}

fn game(s: Arc<Shared>, name: String, me: String) {
    let Some(mut bad) = dial(&name) else { return };
    put(&mut bad, 0, &json!({ "v": 1, "client_id": "6" }));
    set(&s, "unknown_app", get(&mut bad).map_or(Value::Null, |(op, v)| json!([op, v["code"]])));
    let Some(mut old) = dial(&name) else { return };
    put(&mut old, 0, &json!({ "v": 2, "client_id": "7200" }));
    set(&s, "old_version", get(&mut old).map_or(Value::Null, |(op, v)| json!([op, v["code"]])));

    let Some(mut f) = dial(&name) else { return };
    put(&mut f, 0, &json!({ "v": 1, "client_id": "7200" }));
    let ready = get(&mut f).map(|(_, v)| v).unwrap_or_default();
    set(&s, "ready", json!(ready["evt"] == "READY" && ready["data"]["user"]["id"] == me.as_str() && ready["data"]["v"] == 1));
    put(&mut f, 3, &json!({ "nonce": "ping" }));
    set(&s, "pong", get(&mut f).map_or(Value::Null, |(op, _)| json!(op)));
    set(&s, "subscribe", cmd(&mut f, "SUBSCRIBE", Some("ACTIVITY_JOIN"), json!({}))["data"]["evt"].clone());
    set(&s, "gated_event", code(&cmd(&mut f, "SUBSCRIBE", Some("VOICE_STATE_CREATE"), json!({ "channel_id": "1" }))));
    set(&s, "bad_event", code(&cmd(&mut f, "SUBSCRIBE", Some("NOT_AN_EVENT"), json!({}))));
    set(&s, "gated_command", code(&cmd(&mut f, "GET_USER", None, json!({ "id": "1" }))));
    set(&s, "authorize", code(&cmd(&mut f, "AUTHORIZE", None, json!({ "client_id": "7200", "scopes": ["rpc"] }))));
    set(&s, "bad_command", code(&cmd(&mut f, "NOPE", None, json!({}))));
    set(&s, "invite_without_party", code(&cmd(&mut f, "SEND_ACTIVITY_JOIN_INVITE", None, json!({ "user_id": "101", "pid": 1 }))));
    set(&s, "bad_activity", code(&cmd(&mut f, "SET_ACTIVITY", None, json!({ "pid": 1, "activity": { "details": "x" } }))));
    let act = |details: &str| {
        json!({ "pid": std::process::id(), "activity": {
            "details": details, "state": "In a group", "timestamps": { "start": 1_700_000_000 },
            "assets": { "large_image": "logo", "large_text": "Arena", "small_image": "https://example.com/badge.png" },
            "party": { "id": "party-e2e", "size": [1, 4] }, "secrets": { "join": "join-e2e-secret" }, "instance": true,
        } })
    };
    let first = cmd(&mut f, "SET_ACTIVITY", None, act("Exploring the arena"));
    set(&s, "set", json!([first["evt"].clone(), first["data"]["application_id"].clone(), first["data"]["name"].clone()]));
    cmd(&mut f, "SET_ACTIVITY", None, act("Round 2"));
    set(&s, "done", json!(true));
    for _ in 0..600 {
        if s.release.load(Relaxed) {
            break;
        }
        pause(100);
    }
}

fn http(port: u16, req: &str) -> (u16, String, String) {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else { return (0, String::new(), String::new()) };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = s.write_all(req.as_bytes());
    let mut all = Vec::new();
    let _ = s.read_to_end(&mut all);
    let text = String::from_utf8_lossy(&all).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, head.to_string(), body.to_string())
}

fn post(port: u16, origin: Option<&str>, host: &str, body: &Value) -> (u16, String, Value) {
    let o = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
    let b = body.to_string();
    let req = format!("POST /rpc?v=1 HTTP/1.1\r\nHost: {host}\r\n{o}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len());
    let (status, head, body) = http(port, &req);
    (status, head, serde_json::from_str(&body).unwrap_or_default())
}

fn upgrade(port: u16, origin: &str) -> (u16, Option<TcpStream>) {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else { return (0, None) };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let req = format!(
        "GET /?v=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: {origin}\r\n\r\n"
    );
    let _ = s.write_all(req.as_bytes());
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && s.read_exact(&mut b).is_ok() {
        head.push(b[0]);
    }
    let status = String::from_utf8_lossy(&head).split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, Some(s))
}

fn frame_in(s: &mut TcpStream) -> Option<Value> {
    let mut h = [0u8; 2];
    s.read_exact(&mut h).ok()?;
    let len = if h[1] & 0x7f == 126 {
        let mut l = [0u8; 2];
        s.read_exact(&mut l).ok()?;
        u16::from_be_bytes(l) as usize
    } else {
        (h[1] & 0x7f) as usize
    };
    let mut b = vec![0; len];
    s.read_exact(&mut b).ok()?;
    serde_json::from_slice(&b).ok()
}

fn frame_out(s: &mut TcpStream, v: &Value) {
    let b = v.to_string();
    let mut o = vec![0x81, 0x80 | b.len() as u8, 0, 0, 0, 0];
    o.extend(b.as_bytes());
    let _ = s.write_all(&o);
}

fn web_negatives(s: Arc<Shared>, port: u16) {
    let host = format!("127.0.0.1:{port}");
    let ok = "https://discord.com";
    let invite = json!({ "cmd": "INVITE_BROWSER", "args": { "code": "e2eRpc1" }, "nonce": "w1" });
    set(&s, "evil_post", json!(post(port, Some("https://evil.example"), &host, &invite).0));
    set(&s, "lookalike_post", json!(post(port, Some("https://discord.com.evil.example"), &host, &invite).0));
    set(&s, "no_origin_post", json!(post(port, None, &host, &invite).0));
    set(&s, "rebinding_post", json!(post(port, Some(ok), &format!("evil.example:{port}"), &invite).0));
    set(&s, "set_activity", json!(post(port, Some(ok), &host, &json!({ "cmd": "SET_ACTIVITY", "args": { "pid": 1, "activity": { "details": "hi there" } }, "nonce": "w2" })).2["data"]["code"]));
    set(&s, "get_user", json!(post(port, Some(ok), &host, &json!({ "cmd": "GET_USER", "args": { "id": "1" }, "nonce": "w3" })).2["data"]["code"]));
    let pre = |origin: &str| {
        http(port, &format!("OPTIONS /rpc?v=1 HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Private-Network: true\r\nConnection: close\r\n\r\n"))
    };
    let (status, head, _) = pre(ok);
    set(&s, "preflight", json!(status == 204 && head.contains("Access-Control-Allow-Origin: https://discord.com") && head.contains("Access-Control-Allow-Private-Network: true")));
    set(&s, "preflight_evil", json!(pre("https://evil.example").0));
    let get = http(port, &format!("GET /rpc?v=1&payload=%7B%7D&callback=https%3A%2F%2Fdiscord.com%2Fx HTTP/1.1\r\nHost: {host}\r\nReferer: https://evil.example/\r\nConnection: close\r\n\r\n"));
    set(&s, "get_evil_referer", json!(get.0));
    set(&s, "ws_evil", json!(upgrade(port, "https://evil.example").0));
    set(&s, "ws_lookalike", json!(upgrade(port, "https://discord.com.evil.example").0));
    let (status, head, body) = post(port, Some(ok), &host, &invite);
    set(&s, "invite_post", json!(status == 200 && head.contains("Access-Control-Allow-Origin: https://discord.com") && body["evt"].is_null() && body["nonce"] == "w1"));
    set(&s, "done", json!(true));
}

fn web_socket(s: Arc<Shared>, port: u16) {
    let (status, stream) = upgrade(port, "https://canary.discord.com");
    let Some(mut st) = stream else { return };
    let ready = frame_in(&mut st).unwrap_or_default();
    set(&s, "ws_ready", json!(status == 101 && ready["evt"] == "READY" && ready["data"]["user"].is_null()));
    frame_out(&mut st, &json!({ "cmd": "GET_USER", "args": { "id": "1" }, "nonce": "s1" }));
    set(&s, "ws_other", frame_in(&mut st).map_or(Value::Null, |v| v["data"]["code"].clone()));
    frame_out(&mut st, &json!({ "cmd": "GUILD_TEMPLATE_BROWSER", "args": { "code": "e2eTpl1" }, "nonce": "s2" }));
    let reply = frame_in(&mut st).unwrap_or_default();
    set(&s, "ws_template", json!(reply["cmd"] == "GUILD_TEMPLATE_BROWSER" && reply["evt"].is_null() && reply["nonce"] == "s2"));
    set(&s, "done", json!(true));
}

fn second_launch(exit: Arc<Mutex<Option<i32>>>, url: String) {
    *exit.lock().unwrap() = None;
    std::thread::spawn(move || {
        let Ok(exe) = std::env::current_exe() else { return };
        let Ok(mut c) = std::process::Command::new(exe).arg(url).spawn() else { return };
        for _ in 0..150 {
            if let Ok(Some(st)) = c.try_wait() {
                *exit.lock().unwrap() = st.code();
                return;
            }
            pause(100);
        }
        let _ = c.kill();
        *exit.lock().unwrap() = Some(-1);
    });
}

impl Run {
    pub(super) fn local_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let game_is = |app: &App, id: Id| app.game.as_ref().is_some_and(|g| g.id == id);
        match self.step {
            1600 => {
                let (Some(name), port) = (crate::rpc::pipe_name(), crate::rpc::web_port()) else { return };
                if port == 0 || app.conn != crate::net::Conn::Online || app.store.me == 0 {
                    return;
                }
                self.local.home = (app.ui.guild, app.cur().unwrap_or(0));
                let (s, name, me) = (self.local.game.clone(), name.to_string(), app.store.me.to_string());
                std::thread::spawn(move || game(s, name, me));
                self.next(0);
            }
            1601 => {
                let o = read(&self.local.game);
                if o["done"] != true || !game_is(app, 7200) {
                    return;
                }
                let both = |a: &str, b: Value| o[a] == b;
                if both("ready", json!(true)) && both("pong", json!(4)) && both("subscribe", json!("ACTIVITY_JOIN")) && both("unknown_app", json!([2, 4000])) && both("old_version", json!([2, 4004])) {
                    self.mark("rpc_handshake");
                }
                if both("gated_event", json!(4006)) && both("bad_event", json!(4004)) && both("gated_command", json!(4006)) && both("bad_command", json!(4002)) && both("authorize", json!(5000)) {
                    self.mark("rpc_refusals");
                }
                if both("bad_activity", json!(4000)) && both("invite_without_party", json!(5006)) && both("set", json!([null, "7200", "Mock Arena"])) {
                    self.mark("rpc_set_activity");
                }
                self.mark("rpc_game_panel");
                self.next(0);
            }
            1602 => {
                // the second update waits out the gateway's pacing before it replaces the first
                let acts = app.activities();
                let Some(a) = acts.iter().find(|a| a["application_id"] == "7200" && a["details"] == "Round 2") else { return };
                if a["assets"]["large_image"] == "9001"
                    && a["assets"]["small_image"].as_str().is_some_and(|s| s.starts_with("mp:external/mock/example.com/badge.png"))
                    && a["timestamps"]["start"] == 1_700_000_000_000i64
                    && a["flags"] == 3
                    && acts.iter().filter(|a| a["application_id"] == "7100").count() <= 1
                {
                    self.mark("rpc_activity_shape");
                }
                self.shot(ctx, "rpc_panel.png");
                self.local.game.release.store(true, Relaxed);
                self.next(0);
            }
            1603 => {
                if game_is(app, 7200) || app.activities().iter().any(|a| a["application_id"] == "7200") {
                    return;
                }
                self.mark("rpc_activity_cleared");
                let (s, port) = (self.local.web.clone(), crate::rpc::web_port());
                std::thread::spawn(move || web_negatives(s, port));
                self.next(0);
            }
            1604 => {
                let o = read(&self.local.web);
                if o["done"] != true {
                    return;
                }
                if [("evil_post", 403), ("lookalike_post", 403), ("no_origin_post", 403), ("rebinding_post", 421), ("preflight_evil", 403), ("get_evil_referer", 403), ("ws_evil", 403), ("ws_lookalike", 403)]
                    .iter()
                    .all(|(k, v)| o[*k] == *v)
                    && o["set_activity"] == 4002
                    && o["get_user"] == 4002
                    && o["preflight"] == true
                {
                    self.mark("rpc_web_origin");
                }
                if o["invite_post"] == true {
                    self.mark("rpc_web_invite_reply");
                }
                self.next(0);
            }
            1605 => {
                let Some(r) = app.rects.get("link_invite:accept").copied() else { return };
                if app.links.invite.as_ref().is_none_or(|i| i.info.is_none()) {
                    return;
                }
                if self.local.seen.replace(r) != Some(r) {
                    return;
                }
                self.shot(ctx, "rpc_invite.png");
                self.mark("rpc_invite_modal");
                self.click(r.center());
                self.local.seen = None;
                self.next(600);
            }
            1606 => {
                if app.links.invite.is_some() {
                    return;
                }
                self.mark("rpc_invite_accept");
                self.local.web = Default::default();
                let (s, port) = (self.local.web.clone(), crate::rpc::web_port());
                std::thread::spawn(move || web_socket(s, port));
                self.next(0);
            }
            1607 => {
                let Some(r) = app.rects.get("link_template:create").copied() else { return };
                if app.links.template.as_ref().is_none_or(|t| t.info.is_none()) {
                    return;
                }
                if self.local.seen.replace(r) != Some(r) {
                    return;
                }
                let o = read(&self.local.web);
                if o["ws_ready"] == true && o["ws_other"] == 4002 && o["ws_template"] == true {
                    self.mark("rpc_web_socket");
                }
                self.shot(ctx, "rpc_template.png");
                self.mark("rpc_template_modal");
                self.click(r.center());
                self.local.seen = None;
                self.next(600);
            }
            1608 => {
                if app.links.template.is_some() {
                    return;
                }
                self.mark("rpc_template_create");
                let cur = app.cur().unwrap_or(0);
                let Some(to) = app.store.channels.values().find(|c| c.guild == 1000 && c.is_text() && !c.is_thread() && c.id != cur).map(|c| c.id) else {
                    return self.fail("no second text channel to open a link to".into());
                };
                self.local.target = to;
                second_launch(self.local.exit.clone(), format!("discord://-/channels/1000/{to}"));
                self.next(0);
            }
            1609 => {
                if app.cur() != Some(self.local.target) || self.local.exit.lock().ok().and_then(|e| *e).is_none() {
                    return;
                }
                if *self.local.exit.lock().unwrap() == Some(0) {
                    self.mark("rpc_second_instance");
                }
                self.mark("rpc_link_channel");
                second_launch(self.local.exit.clone(), "discord://-/invite/e2eRpc1".into());
                self.next(0);
            }
            1610 => {
                if app.links.invite.as_ref().is_none_or(|i| i.info.is_none()) {
                    return;
                }
                self.mark("rpc_link_invite");
                self.key(Key::Escape);
                self.next(400);
            }
            1611 => {
                if app.links.invite.is_some() || self.local.exit.lock().ok().and_then(|e| *e).is_none() {
                    return;
                }
                self.mark("rpc_link_closed");
                let (guild, ch) = self.local.home;
                app.ui.guild = guild;
                if ch != 0 {
                    app.open(ch);
                }
                self.step = 100;
            }
            _ => self.step = 100,
        }
    }
}
