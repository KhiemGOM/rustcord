//! The rich presence pipes, `\\.\pipe\discord-ipc-0` to `-9`: one connection per game, frames of
//! an opcode and a length (both u32 little endian) and a JSON body. A game is let in with a
//! client id Discord knows and can then set its activity, subscribe to join events and invite
//! people. Everything that needs an OAuth token is refused, as Discord does without one.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::{Instant, sleep_until, timeout};

mod auth;
pub use auth::SCOPES;

use super::activity::{self, Fail};
use super::{Bus, Msg, call};
use crate::state::Id;

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;

const MAX_FRAME: usize = 1 << 16;
const MAX_CLIENTS: usize = 32;
const PUBLISH_EVERY: Duration = Duration::from_secs(4);

const EVENTS: [&str; 3] = ["ACTIVITY_JOIN", "ACTIVITY_SPECTATE", "ACTIVITY_JOIN_REQUEST"];
const NEEDS_TOKEN_EVENTS: [&str; 17] = [
    "GUILD_STATUS", "GUILD_CREATE", "CHANNEL_CREATE", "VOICE_CHANNEL_SELECT", "VOICE_STATE_CREATE", "VOICE_STATE_UPDATE", "VOICE_STATE_DELETE",
    "VOICE_SETTINGS_UPDATE", "VOICE_CONNECTION_STATUS", "SPEAKING_START", "SPEAKING_STOP", "MESSAGE_CREATE", "MESSAGE_UPDATE", "MESSAGE_DELETE",
    "NOTIFICATION_CREATE", "ACTIVITY_INVITE", "CURRENT_USER_UPDATE",
];
const NEEDS_TOKEN_COMMANDS: [&str; 12] = [
    "GET_GUILD", "GET_GUILDS", "GET_CHANNEL", "GET_CHANNELS", "SELECT_VOICE_CHANNEL", "GET_SELECTED_VOICE_CHANNEL", "SELECT_TEXT_CHANNEL",
    "GET_VOICE_SETTINGS", "SET_VOICE_SETTINGS", "SET_USER_VOICE_SETTINGS", "GET_USER", "GET_RELATIONSHIPS",
];

static NEXT: AtomicU64 = AtomicU64::new(1);

fn path(n: u32) -> String {
    if super::testing() { format!(r"\\.\pipe\rustcord-test-ipc-{}", std::process::id()) } else { format!(r"\\.\pipe\discord-ipc-{n}") }
}

fn create(path: &str, first: bool) -> std::io::Result<NamedPipeServer> {
    ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).create(path)
}

/// Takes the first free pipe of the ten, as Discord does; a Discord that's already running
/// keeps the lower numbers and games find whichever answers first.
pub async fn serve() {
    let tries = if super::testing() { 1 } else { 10 };
    let Some((mut server, path)) = (0..tries).find_map(|n| create(&path(n), true).ok().map(|s| (s, path(n)))) else { return };
    let _ = super::PIPE.set(path.clone());
    let slots = Arc::new(Semaphore::new(MAX_CLIENTS));
    loop {
        if server.connect().await.is_err() {
            return;
        }
        let Ok(next) = create(&path, false) else { return };
        let io = std::mem::replace(&mut server, next);
        match slots.clone().try_acquire_owned() {
            Ok(permit) => {
                tokio::spawn(async move {
                    client(io).await;
                    drop(permit);
                });
            }
            Err(_) => drop(io),
        }
    }
}

async fn frames(mut rd: ReadHalf<NamedPipeServer>, tx: mpsc::Sender<(u32, Vec<u8>)>) {
    loop {
        let mut head = [0u8; 8];
        if rd.read_exact(&mut head).await.is_err() {
            return;
        }
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0; len.min(MAX_FRAME)];
        if len > MAX_FRAME || rd.read_exact(&mut body).await.is_err() || tx.send((op, body)).await.is_err() {
            return;
        }
    }
}

async fn put(wr: &mut WriteHalf<NamedPipeServer>, op: u32, body: &[u8]) -> bool {
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend(op.to_le_bytes());
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    timeout(Duration::from_secs(5), wr.write_all(&out)).await.is_ok_and(|r| r.is_ok())
}

async fn put_json(wr: &mut WriteHalf<NamedPipeServer>, op: u32, v: &Value) -> bool {
    put(wr, op, v.to_string().as_bytes()).await
}

fn dispatch(evt: &str, data: Value) -> Value {
    json!({ "cmd": "DISPATCH", "data": data, "evt": evt, "nonce": null })
}

fn id(v: &Value) -> Option<Id> {
    v.as_str()?.parse().ok().filter(|n| *n > 0)
}

fn no_activity() -> Fail {
    (5006, "No eligible activity for application. Ensure an activity includes a party and appropriate secret.".into())
}

#[derive(Default)]
struct Client {
    id: u64,
    app: Id,
    name: String,
    icon: Option<String>,
    subs: HashSet<String>,
    activity: Option<Value>,
    assets: Option<(Instant, HashMap<String, String>)>,
    external: HashMap<String, String>,
    live: bool,
    authed: bool,
    denied: bool,
    pending: Option<Option<Value>>,
    published: Option<Instant>,
    extra: Vec<Value>,
}

async fn client(io: NamedPipeServer) {
    let (rd, mut wr) = tokio::io::split(io);
    let (tx, mut rx) = mpsc::channel(8);
    let reader = tokio::spawn(frames(rd, tx));
    let mut c = Client { id: NEXT.fetch_add(1, Relaxed), ..Default::default() };
    run(&mut c, &mut rx, &mut wr).await;
    reader.abort();
    if c.live || c.pending.is_some() {
        c.send(None);
    }
}

async fn run(c: &mut Client, rx: &mut mpsc::Receiver<(u32, Vec<u8>)>, wr: &mut WriteHalf<NamedPipeServer>) {
    let Ok(Some((op, body))) = timeout(Duration::from_secs(10), rx.recv()).await else { return };
    let shaken = if op == HANDSHAKE { c.handshake(&body).await } else { Err((1003, "Handshake required".into())) };
    match shaken {
        Ok(ready) => {
            if !put_json(wr, FRAME, &ready).await {
                return;
            }
        }
        Err((code, message)) => {
            put_json(wr, CLOSE, &json!({ "code": code, "message": message })).await;
            return;
        }
    }
    let mut bus = super::bus();
    loop {
        let flush = c.published.map(|p| p + PUBLISH_EVERY).unwrap_or_else(Instant::now);
        tokio::select! {
            f = rx.recv() => {
                let Some((op, body)) = f else { return };
                match op {
                    FRAME => {
                        let reply = c.frame(&body).await;
                        if !put_json(wr, FRAME, &reply).await {
                            return;
                        }
                        for e in std::mem::take(&mut c.extra) {
                            if !put_json(wr, FRAME, &e).await {
                                return;
                            }
                        }
                    }
                    PING => {
                        if !put(wr, PONG, &body).await {
                            return;
                        }
                    }
                    CLOSE => {
                        put_json(wr, CLOSE, &json!({ "code": 1000, "message": "Closed" })).await;
                        return;
                    }
                    _ => {
                        put_json(wr, CLOSE, &json!({ "code": 1003, "message": "Invalid opcode" })).await;
                        return;
                    }
                }
            }
            b = bus.recv() => match b {
                Ok(Bus::SignedOut) | Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    put_json(wr, CLOSE, &json!({ "code": 1000, "message": "User logged out" })).await;
                    return;
                }
                Ok(Bus::JoinRequest { app, user }) if app == c.app && c.subs.contains("ACTIVITY_JOIN_REQUEST") => {
                    if !put_json(wr, FRAME, &dispatch("ACTIVITY_JOIN_REQUEST", json!({ "user": user_of(&user) }))).await {
                        return;
                    }
                }
                _ => {}
            },
            _ = sleep_until(flush), if c.pending.is_some() => {
                let p = c.pending.take().flatten();
                c.send(p);
            }
        }
    }
}

fn user_of(u: &Value) -> Value {
    json!({
        "id": u["id"], "username": u["username"], "discriminator": u["discriminator"], "global_name": u["global_name"],
        "avatar": u["avatar"], "avatar_decoration_data": u["avatar_decoration_data"], "bot": u["bot"].as_bool().unwrap_or(false),
        "flags": u["public_flags"].as_u64().or(u["flags"].as_u64()).unwrap_or(0), "premium_type": u["premium_type"].as_u64().unwrap_or(0),
    })
}

impl Client {
    async fn handshake(&mut self, body: &[u8]) -> Result<Value, Fail> {
        let v: Value = serde_json::from_slice(body).map_err(|_| (1003, "Payload not recognized".to_string()))?;
        if v["v"] != 1 {
            return Err((4004, format!("Invalid Version: {}", v["v"])));
        }
        if v["encoding"].as_str().is_some_and(|e| e != "json") {
            return Err((4005, "Invalid Encoding".into()));
        }
        let cid = id(&v["client_id"]).ok_or((4000, "Invalid Client ID".to_string()))?;
        let (user, _) = super::session().ok_or((1000, "User logged out".to_string()))?;
        let info = call("GET", &format!("/oauth2/applications/{cid}/rpc"), None).await.map_err(|e| {
            if crate::api::offline(&e) { (1001, "Service unavailable".to_string()) } else { (4000, "Invalid Client ID".to_string()) }
        })?;
        self.app = cid;
        self.name = info["name"].as_str().unwrap_or("").to_string();
        self.icon = info["icon"].as_str().map(str::to_string);
        Ok(dispatch(
            "READY",
            json!({
                "v": 1,
                "config": { "cdn_host": "cdn.discordapp.com", "api_endpoint": "//discord.com/api", "environment": "production" },
                "user": user_of(&user),
            }),
        ))
    }

    async fn frame(&mut self, body: &[u8]) -> Value {
        let Ok(v) = serde_json::from_slice::<Value>(body) else {
            return json!({ "cmd": null, "data": { "code": 4000, "message": "Invalid payload" }, "evt": "ERROR", "nonce": null });
        };
        match self.command(&v).await {
            Ok(data) => json!({ "cmd": v["cmd"], "data": data, "evt": null, "nonce": v["nonce"] }),
            Err((code, message)) => json!({ "cmd": v["cmd"], "data": { "code": code, "message": message }, "evt": "ERROR", "nonce": v["nonce"] }),
        }
    }

    async fn command(&mut self, v: &Value) -> Result<Value, Fail> {
        if v["nonce"].as_str().is_none_or(str::is_empty) {
            return Err((4000, "Payload requires a nonce".into()));
        }
        let args = &v["args"];
        match v["cmd"].as_str().unwrap_or("") {
            "SET_ACTIVITY" => self.set_activity(args).await,
            "SUBSCRIBE" => self.subscribe(v["evt"].as_str().unwrap_or(""), true),
            "UNSUBSCRIBE" => self.subscribe(v["evt"].as_str().unwrap_or(""), false),
            "SEND_ACTIVITY_JOIN_INVITE" => self.invite(&args["user_id"], "").await,
            "ACTIVITY_INVITE_USER" => {
                let content = args["content"].as_str().unwrap_or("");
                if args["type"] != 1 || content.chars().count() > 1024 || !args["pid"].is_u64() {
                    return Err((4000, "Invalid payload".into()));
                }
                self.invite(&args["user_id"], content).await
            }
            "CLOSE_ACTIVITY_JOIN_REQUEST" => {
                let user = id(&args["user_id"]).ok_or((4000, "Invalid payload".to_string()))?;
                super::send(Msg::CloseJoinRequest(user));
                Ok(Value::Null)
            }
            "ACCEPT_ACTIVITY_INVITE" => self.accept(args).await,
            "AUTHORIZE" => self.authorize(args).await,
            "AUTHENTICATE" => self.authenticate(args).await,
            c if NEEDS_TOKEN_COMMANDS.contains(&c) => Err((4006, "Not authenticated or invalid scope".into())),
            c => Err((4002, format!("Invalid command: {c}"))),
        }
    }

    fn subscribe(&mut self, evt: &str, on: bool) -> Result<Value, Fail> {
        if NEEDS_TOKEN_EVENTS.contains(&evt) {
            return Err((4006, "Not authenticated or invalid scope".into()));
        }
        let evt = EVENTS.iter().find(|e| **e == evt).ok_or_else(|| (4004, format!("Invalid event: {evt}")))?;
        if on {
            self.subs.insert(evt.to_string());
        } else {
            self.subs.remove(*evt);
        }
        Ok(json!({ "evt": evt }))
    }

    async fn set_activity(&mut self, args: &Value) -> Result<Value, Fail> {
        if !args["pid"].is_u64() {
            return Err((4002, "nonzero pid required".into()));
        }
        if args["activity"].is_null() {
            self.activity = None;
            self.publish(None);
            return Ok(Value::Null);
        }
        let (_, session) = super::session().ok_or((4010, "User logged out".to_string()))?;
        let mut act = activity::shape(&args["activity"], self.app, &self.name, &session)?;
        self.resolve_assets(&mut act).await;
        self.activity = Some(act.clone());
        self.publish(Some(act.clone()));
        Ok(act)
    }

    /// Image keys become what the gateway wants: a uploaded asset's id by its name, and an
    /// outside link the `mp:` path Discord's media proxy gave it.
    async fn resolve_assets(&mut self, act: &mut Value) {
        const FIELDS: [&str; 3] = ["large_image", "small_image", "invite_cover_image"];
        let keys: Vec<(&str, String)> = FIELDS.iter().filter_map(|f| Some((*f, act["assets"][*f].as_str()?.to_string()))).collect();
        if keys.is_empty() {
            return;
        }
        let is_url = |k: &str| k.starts_with("http:") || k.starts_with("https:");
        let urls: Vec<&String> = keys.iter().map(|(_, k)| k).filter(|k| is_url(k) && !self.external.contains_key(*k)).collect();
        if !urls.is_empty()
            && self.external.len() < 64
            && let Ok(Value::Array(list)) = call("POST", &format!("/applications/{}/external-assets", self.app), Some(json!({ "urls": urls }))).await
        {
            for e in list {
                if let (Some(u), Some(p)) = (e["url"].as_str(), e["external_asset_path"].as_str()) {
                    self.external.insert(u.to_string(), p.to_string());
                }
            }
        }
        let missing = |this: &Self| keys.iter().any(|(_, k)| !is_url(k) && !this.assets.as_ref().is_some_and(|(_, m)| m.contains_key(k)));
        let stale = self.assets.as_ref().is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(30));
        if missing(self)
            && stale
            && let Ok(Value::Array(list)) = call("GET", &format!("/oauth2/applications/{}/assets", self.app), None).await
        {
            let map = list.iter().filter_map(|a| Some((a["name"].as_str()?.to_string(), a["id"].as_str()?.to_string()))).collect();
            self.assets = Some((Instant::now(), map));
        }
        for (field, key) in keys {
            let found = if is_url(&key) {
                self.external.get(&key).map(|p| format!("mp:{p}"))
            } else {
                self.assets.as_ref().and_then(|(_, m)| m.get(&key).cloned())
            };
            match found {
                Some(v) => act["assets"][field] = json!(v),
                None => {
                    if let Some(a) = act["assets"].as_object_mut() {
                        a.remove(field);
                    }
                }
            }
        }
    }

    /// The gateway takes five presence updates in twenty seconds from everything; a game that
    /// sets its activity every second gets its latest one through every few.
    fn publish(&mut self, act: Option<Value>) {
        let due = self.published.is_none_or(|p| p.elapsed() >= PUBLISH_EVERY);
        if due {
            self.pending = None;
            self.send(act);
        } else {
            self.pending = Some(act);
        }
    }

    fn send(&mut self, activity: Option<Value>) {
        self.published = Some(Instant::now());
        self.live = activity.is_some();
        super::send(Msg::Activity { conn: self.id, app: self.app, name: self.name.clone(), icon: self.icon.clone(), activity });
    }

    async fn invite(&mut self, user: &Value, content: &str) -> Result<Value, Fail> {
        let party = self.activity.as_ref().and_then(|a| a["party"]["id"].as_str().filter(|_| a["secrets"]["join"].is_string())).ok_or_else(no_activity)?.to_string();
        let user = id(user).ok_or((4000, "Invalid payload".to_string()))?;
        let (_, session) = super::session().ok_or((4010, "User logged out".to_string()))?;
        let dm = call("POST", "/users/@me/channels", Some(json!({ "recipients": [user.to_string()] }))).await.map_err(|e| (1000, e))?;
        let body = json!({ "content": content, "tts": false, "activity": { "type": 1, "session_id": session, "party_id": party }, "application_id": self.app.to_string() });
        call("POST", &format!("/channels/{}/messages", dm["id"].as_str().unwrap_or("0")), Some(body)).await.map_err(|e| (1000, e))?;
        Ok(Value::Null)
    }

    async fn accept(&mut self, args: &Value) -> Result<Value, Fail> {
        let (Some(user), Some(session), Some(channel), Some(message)) = (id(&args["user_id"]), args["session_id"].as_str(), id(&args["channel_id"]), id(&args["message_id"])) else {
            return Err((4000, "Invalid payload".into()));
        };
        let invalid = || (4011, "Invite is expired or invalid.".to_string());
        if args["type"] != 1 || session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric()) || id(&args["application_id"]).is_some_and(|a| a != self.app) {
            return Err(invalid());
        }
        let path = format!("/users/{user}/sessions/{session}/activities/{}/1?channel_id={channel}&message_id={message}", self.app);
        let got = call("GET", &path, None).await.map_err(|_| invalid())?;
        let secret = got["secret"].as_str().ok_or_else(invalid)?;
        if self.subs.contains("ACTIVITY_JOIN") {
            self.extra.push(dispatch("ACTIVITY_JOIN", json!({ "secret": secret })));
        }
        Ok(Value::Null)
    }
}
