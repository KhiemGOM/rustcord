//! The host side of the Embedded App SDK's postMessage protocol, as Discord's client speaks it:
//! `[0, handshake]` from the frame, a READY dispatch back, then `[1, {cmd, args, nonce}]` requests
//! and responses and `DISPATCH` events. Nothing here touches the network or the screen: a message
//! in gives the frames to post back and the few things only the app can do (the authorize flow,
//! the token check, an external link).

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::state::Id;

const HELLO: u64 = 3;
const FRAME: u64 = 1;
const CLOSE: u64 = 2;
const HANDSHAKE: u64 = 0;

pub const INVALID_PAYLOAD: u32 = 4000;
pub const INVALID_COMMAND: u32 = 4002;
pub const INVALID_EVENT: u32 = 4004;
pub const INVALID_CHANNEL: u32 = 4005;
pub const INVALID_PERMISSIONS: u32 = 4006;
pub const INVALID_CLIENTID: u32 = 4007;
pub const INVALID_TOKEN: u32 = 4009;
pub const OAUTH2_ERROR: u32 = 5000;

const CLOSE_UNSUPPORTED: u32 = 1003;
const CLOSE_INVALID_CLIENTID: u32 = 4000;
const CLOSE_INVALID_ORIGIN: u32 = 4001;
const CLOSE_INVALID_VERSION: u32 = 4004;
const CLOSE_INVALID_ENCODING: u32 = 4005;

/// What an activity may ask the user to hand over: the SDK's documented scopes without the ones
/// that would let a web page act as the account (reading messages, joining servers or group
/// chats, taking over the client).
pub const SCOPES: [&str; 14] = [
    "identify", "email", "guilds", "guilds.members.read", "guilds.channels.read", "dm_channels.read", "applications.commands",
    "rpc.activities.write", "rpc.voice.read", "rpc.voice.write", "rpc.video.read", "rpc.video.write", "rpc.screenshare.read",
    "rpc.screenshare.write",
];

pub fn scope_text(scope: &str) -> &'static str {
    match scope {
        "identify" => "Access your username, avatar, and banner",
        "email" => "Access your email address",
        "guilds" => "Know what servers you're in",
        "guilds.members.read" => "Read your member info (nickname, avatar, roles, etc...) for servers you belong to",
        "guilds.channels.read" => "Read the channel list for servers you belong to and some channel info (name, type, sort order, etc...)",
        "dm_channels.read" => "See information about the DMs and GDMs you are in",
        "applications.commands" => "Use commands in a server",
        "rpc.activities.write" => "Set your current activity",
        "rpc.voice.read" => "Read your voice settings and listen for voice events",
        "rpc.voice.write" => "Alter your voice settings and use soundboard",
        "rpc.video.read" => "Read your video state",
        "rpc.video.write" => "Alter your video settings and start video",
        "rpc.screenshare.read" => "Read your screenshare state",
        "rpc.screenshare.write" => "Alter your screenshare settings and start screenshare",
        _ => "",
    }
}

pub struct Env {
    pub app_name: String,
    pub channel: Value,
    pub channel_id: Id,
    pub participants: Vec<Value>,
    pub cdn: String,
    pub api: String,
    pub locale: String,
}

pub enum Act {
    Send(Value),
    Close(u32, String),
    Authorize { nonce: String, scopes: Vec<String>, state: Option<String>, quiet: bool },
    Authenticate { nonce: String, token: String },
    Link { nonce: String, url: String },
}

#[derive(Default)]
pub struct Socket {
    app: Id,
    frame_id: String,
    origin: String,
    open: bool,
    authing: bool,
    authed: bool,
    scopes: HashSet<String>,
    subs: Vec<(String, Value)>,
    sent: std::collections::HashMap<String, Value>,
    speaking: Option<HashSet<String>>,
    pub rich: Option<Value>,
}

pub struct View {
    pub me: Value,
    pub participants: Vec<Value>,
    pub voice: Vec<Value>,
    pub speaking: Vec<Id>,
    pub channel_id: Id,
}

pub fn frame(cmd: &str, data: Value, nonce: &str) -> Value {
    json!([FRAME, { "cmd": cmd, "data": data, "evt": null, "nonce": nonce }])
}

fn failure(cmd: &str, code: u32, message: &str, nonce: &str) -> Value {
    json!([FRAME, { "cmd": cmd, "data": { "code": code, "message": message }, "evt": "ERROR", "nonce": nonce }])
}

fn dispatch(evt: &str, data: Value) -> Value {
    json!([FRAME, { "cmd": "DISPATCH", "evt": evt, "data": data, "nonce": null }])
}

fn close(code: u32, message: &str) -> Act {
    Act::Close(code, message.into())
}

impl Socket {
    pub fn new(app: Id, frame_id: String, origin: String) -> Self {
        Socket { app, frame_id, origin, ..Self::default() }
    }

    fn has(&self, any: &[&str]) -> bool {
        any.iter().any(|s| self.scopes.contains(*s))
    }

    pub fn fresh(&self) -> Socket {
        Socket::new(self.app, self.frame_id.clone(), self.origin.clone())
    }

    pub fn on_message(&mut self, origin: &str, data: &Value, env: &Env) -> Vec<Act> {
        if origin != self.origin {
            return vec![close(CLOSE_INVALID_ORIGIN, "Origin has changed")];
        }
        let (Some(op), Some(body)) = (data.get(0).and_then(Value::as_u64), data.get(1)) else { return Vec::new() };
        match op {
            HANDSHAKE if self.open => vec![close(CLOSE_UNSUPPORTED, "Already connected")],
            HANDSHAKE => self.handshake(body, env),
            FRAME if !self.open => vec![close(CLOSE_UNSUPPORTED, "Not connected")],
            FRAME => {
                let req = match body {
                    Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                    v => v.clone(),
                };
                self.request(&req, env)
            }
            CLOSE => {
                *self = self.fresh();
                Vec::new()
            }
            HELLO => Vec::new(),
            _ => vec![close(CLOSE_UNSUPPORTED, "Invalid opcode")],
        }
    }

    fn handshake(&mut self, h: &Value, env: &Env) -> Vec<Act> {
        let text = |k: &str| h.get(k).and_then(Value::as_str);
        if h.get("v").and_then(Value::as_u64) != Some(1) {
            return vec![close(CLOSE_INVALID_VERSION, "Invalid Version")];
        }
        if text("encoding").is_some_and(|e| e != "json") {
            return vec![close(CLOSE_INVALID_ENCODING, "Invalid Encoding")];
        }
        if text("client_id").and_then(|c| c.parse::<Id>().ok()) != Some(self.app) {
            return vec![close(CLOSE_INVALID_CLIENTID, "Application does not match the connection's")];
        }
        if text("frame_id") != Some(self.frame_id.as_str()) {
            return vec![close(CLOSE_UNSUPPORTED, "Unrecognized iframe ID")];
        }
        self.open = true;
        let config = json!({ "cdn_host": env.cdn, "api_endpoint": env.api, "environment": "production" });
        vec![Act::Send(dispatch("READY", json!({ "v": 1, "config": config })))]
    }

    fn request(&mut self, req: &Value, env: &Env) -> Vec<Act> {
        let nonce = req.get("nonce").and_then(Value::as_str).unwrap_or("");
        let cmd = req.get("cmd").and_then(Value::as_str).unwrap_or("");
        let args = req.get("args").cloned().unwrap_or(Value::Null);
        if nonce.is_empty() {
            return vec![Act::Send(failure(cmd, INVALID_PAYLOAD, "Payload requires a nonce", nonce))];
        }
        let reply = |v: Value| vec![Act::Send(frame(cmd, v, nonce))];
        let deny = |code: u32, msg: &str| vec![Act::Send(failure(cmd, code, msg, nonce))];
        let need = |ok: bool| ok.then_some(()).ok_or(());
        match cmd {
            "AUTHORIZE" => self.authorize(&args, env, nonce),
            "AUTHENTICATE" => {
                let Some(token) = args.get("access_token").and_then(Value::as_str).filter(|t| !t.is_empty()) else {
                    return deny(INVALID_TOKEN, "No access token provided");
                };
                if self.authed || self.authing {
                    return deny(INVALID_COMMAND, "Already authenticated");
                }
                self.authing = true;
                vec![Act::Authenticate { nonce: nonce.into(), token: token.into() }]
            }
            "GET_CHANNEL" => {
                if need(self.has(&["rpc", "guilds", "guilds.channels.read", "bot"])).is_err() {
                    return deny(INVALID_PERMISSIONS, "Not authenticated or invalid scope");
                }
                let id = args.get("channel_id").and_then(Value::as_str).unwrap_or("");
                if id != env.channel_id.to_string() {
                    return deny(INVALID_CHANNEL, &format!("Invalid channel id: {id}"));
                }
                reply(env.channel.clone())
            }
            "GET_ACTIVITY_INSTANCE_CONNECTED_PARTICIPANTS" | "GET_INSTANCE_CONNECTED_PARTICIPANTS" => {
                if !self.authed {
                    return deny(INVALID_PERMISSIONS, "Not authenticated or invalid scope");
                }
                reply(json!({ "participants": env.participants }))
            }
            "SET_ACTIVITY" => {
                if need(self.has(&["rpc", "rpc.activities.write", "activities.write"])).is_err() {
                    return deny(INVALID_PERMISSIONS, "Not authenticated or invalid scope");
                }
                let Some(a) = args.get("activity").filter(|a| a.is_object()) else { return deny(INVALID_PAYLOAD, "activity is required") };
                let mut shown = a.clone();
                shown["application_id"] = json!(self.app.to_string());
                shown["name"] = json!(env.app_name);
                shown["type"] = json!(0);
                self.rich = Some(shown.clone());
                reply(shown)
            }
            "SUBSCRIBE" | "UNSUBSCRIBE" => {
                let evt = req.get("evt").and_then(Value::as_str).unwrap_or("");
                let sub = cmd == "SUBSCRIBE";
                match self.check(evt, &args) {
                    Ok(()) if sub => {
                        self.subs.retain(|(e, a)| !(e == evt && *a == args));
                        self.subs.push((evt.into(), args));
                        self.sent.retain(|k, _| !k.starts_with(evt));
                        self.speaking = None;
                    }
                    Ok(()) => self.subs.retain(|(e, a)| !(e == evt && *a == args)),
                    Err((code, msg)) => return deny(code, msg),
                }
                reply(json!({ "evt": evt }))
            }
            "OPEN_EXTERNAL_LINK" => match args.get("url").and_then(Value::as_str) {
                Some(u) => vec![Act::Link { nonce: nonce.into(), url: u.into() }],
                None => deny(INVALID_PAYLOAD, "url is required"),
            },
            "SET_ORIENTATION_LOCK_STATE" | "SET_CONFIG" | "CAPTURE_LOG" | "SET_PREFERS_PICTURE_IN_PICTURE_ON_NAVIGATE_AWAY" => reply(Value::Null),
            "GET_PLATFORM_BEHAVIORS" => reply(json!({ "iosKeyboardResizesView": false })),
            "USER_SETTINGS_GET_LOCALE" => reply(json!({ "locale": env.locale })),
            "GET_SKUS_EMBEDDED" => reply(json!({ "skus": [] })),
            "GET_ENTITLEMENTS_EMBEDDED" => reply(json!({ "entitlements": [] })),
            _ => deny(INVALID_COMMAND, &format!("Invalid command: {cmd}")),
        }
    }

    fn check(&self, evt: &str, args: &Value) -> Result<(), (u32, &'static str)> {
        let auth = (INVALID_PERMISSIONS, "Not authenticated or invalid scope");
        match evt {
            "ACTIVITY_LAYOUT_MODE_UPDATE" | "ACTIVITY_PIP_MODE_UPDATE" | "ORIENTATION_UPDATE" | "THERMAL_STATE_UPDATE" => Ok(()),
            "ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE" => self.authed.then_some(()).ok_or(auth),
            "CURRENT_USER_UPDATE" => self.has(&["identify"]).then_some(()).ok_or(auth),
            "VOICE_STATE_UPDATE" | "SPEAKING_START" | "SPEAKING_STOP" => {
                if !self.has(&["rpc", "rpc.voice.read"]) {
                    return Err(auth);
                }
                args.get("channel_id").and_then(Value::as_str).map(|_| ()).ok_or((INVALID_PAYLOAD, "Invalid subscription parameters provided"))
            }
            _ => Err((INVALID_EVENT, "Invalid event")),
        }
    }

    fn authorize(&mut self, args: &Value, env: &Env, nonce: &str) -> Vec<Act> {
        let deny = |code: u32, msg: &str| vec![Act::Send(failure("AUTHORIZE", code, msg, nonce))];
        if args.get("client_id").and_then(Value::as_str).and_then(|c| c.parse::<Id>().ok()) != Some(self.app) {
            return deny(INVALID_CLIENTID, "Application does not match the connection's");
        }
        if self.authed || self.authing {
            return deny(INVALID_COMMAND, "Already authenticated");
        }
        if args.get("response_type").and_then(Value::as_str).is_some_and(|r| r != "code") {
            return deny(INVALID_COMMAND, "Authorization response_type is not supported");
        }
        if args.get("redirect_uri").is_some() {
            return deny(OAUTH2_ERROR, "Redirect URI cannot be used in the RPC OAuth2 Authorization flow");
        }
        let scopes: Vec<String> = match args.get("scope").or_else(|| args.get("scopes")) {
            Some(Value::String(s)) => s.split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            Some(Value::Array(a)) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        };
        if scopes.is_empty() {
            return deny(OAUTH2_ERROR, "No scopes provided");
        }
        if let Some(bad) = scopes.iter().find(|s| !SCOPES.contains(&s.as_str())) {
            return deny(OAUTH2_ERROR, &format!("Scope \"{bad}\" is not available to activities"));
        }
        let _ = env;
        self.authing = true;
        let state = args.get("state").and_then(Value::as_str).map(str::to_string);
        let quiet = args.get("prompt").and_then(Value::as_str) == Some("none");
        vec![Act::Authorize { nonce: nonce.into(), scopes, state, quiet }]
    }

    pub fn authorized(&mut self, nonce: &str, code: &str) -> Value {
        self.authing = false;
        frame("AUTHORIZE", json!({ "code": code }), nonce)
    }

    pub fn authenticated(&mut self, nonce: &str, data: Value) -> Value {
        self.authing = false;
        self.authed = true;
        self.scopes = data["scopes"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect();
        frame("AUTHENTICATE", data, nonce)
    }

    pub fn refused(&mut self, cmd: &str, nonce: &str, code: u32, message: &str) -> Value {
        self.authing = false;
        failure(cmd, code, message, nonce)
    }

    pub fn answer(&self, cmd: &str, nonce: &str, data: Value) -> Value {
        frame(cmd, data, nonce)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_authed(&self) -> bool {
        self.authed
    }

    pub fn wants_view(&self) -> bool {
        !self.subs.is_empty()
    }

    /// The events the frame is subscribed to that something changed for, as frames to post.
    pub fn sync(&mut self, v: &View) -> Vec<Value> {
        let mut out = Vec::new();
        let subs = self.subs.clone();
        for (evt, args) in &subs {
            let mut changed = |key: String, now: Value, out: &mut Vec<Value>, data: Value| {
                if self.sent.get(&key) != Some(&now) {
                    self.sent.insert(key, now);
                    out.push(dispatch(evt, data));
                }
            };
            match evt.as_str() {
                "ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE" => {
                    let d = json!({ "participants": v.participants });
                    changed(evt.clone(), d.clone(), &mut out, d);
                }
                "CURRENT_USER_UPDATE" => changed(evt.clone(), v.me.clone(), &mut out, v.me.clone()),
                "ACTIVITY_LAYOUT_MODE_UPDATE" => changed(evt.clone(), json!(0), &mut out, json!({ "layout_mode": 0 })),
                "ACTIVITY_PIP_MODE_UPDATE" => changed(evt.clone(), json!(false), &mut out, json!({ "is_pip_mode": false })),
                "VOICE_STATE_UPDATE" if args["channel_id"] == v.channel_id.to_string() => {
                    for s in &v.voice {
                        changed(format!("{evt}{}", s["user"]["id"]), s.clone(), &mut out, s.clone());
                    }
                }
                _ => {}
            }
        }
        let wants = |e: &str| subs.iter().any(|(s, a)| s == e && a["channel_id"] == v.channel_id.to_string());
        if wants("SPEAKING_START") || wants("SPEAKING_STOP") {
            let now: HashSet<String> = v.speaking.iter().map(|u| u.to_string()).collect();
            if let Some(was) = &self.speaking {
                let ch = v.channel_id.to_string();
                for u in now.difference(was).filter(|_| wants("SPEAKING_START")) {
                    out.push(dispatch("SPEAKING_START", json!({ "user_id": u, "channel_id": ch })));
                }
                for u in was.difference(&now).filter(|_| wants("SPEAKING_STOP")) {
                    out.push(dispatch("SPEAKING_STOP", json!({ "user_id": u, "channel_id": ch })));
                }
            }
            self.speaking = Some(now);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            app_name: "Mock".into(),
            channel: json!({ "id": "9", "name": "Hangout" }),
            channel_id: 9,
            participants: vec![json!({ "id": "7" })],
            cdn: "cdn.test".into(),
            api: "//api.test".into(),
            locale: "en-US".into(),
        }
    }

    fn open() -> Socket {
        let mut s = Socket::new(55, "f1".into(), "https://55.discordsays.com".into());
        let hs = json!([0, { "v": 1, "encoding": "json", "client_id": "55", "frame_id": "f1" }]);
        s.on_message("https://55.discordsays.com", &hs, &env());
        s
    }

    fn req(s: &mut Socket, cmd: &str, args: Value) -> Vec<Act> {
        s.on_message("https://55.discordsays.com", &json!([1, { "cmd": cmd, "args": args, "nonce": "n" }]), &env())
    }

    fn sent(a: &[Act]) -> Value {
        match a.first() {
            Some(Act::Send(v)) => v[1].clone(),
            _ => Value::Null,
        }
    }

    #[test]
    fn handshake_answers_ready() {
        let mut s = Socket::new(55, "f1".into(), "https://55.discordsays.com".into());
        let hs = json!([0, { "v": 1, "encoding": "json", "client_id": "55", "frame_id": "f1" }]);
        let out = s.on_message("https://55.discordsays.com", &hs, &env());
        let v = sent(&out);
        assert_eq!(v["evt"], "READY");
        assert_eq!(v["data"]["config"]["cdn_host"], "cdn.test");
    }

    #[test]
    fn handshake_must_match_frame_app_and_origin() {
        for (origin, client, frame_id) in [("https://evil.com", "55", "f1"), ("https://55.discordsays.com", "56", "f1"), ("https://55.discordsays.com", "55", "f2")] {
            let mut s = Socket::new(55, "f1".into(), "https://55.discordsays.com".into());
            let hs = json!([0, { "v": 1, "client_id": client, "frame_id": frame_id }]);
            assert!(matches!(s.on_message(origin, &hs, &env()).first(), Some(Act::Close(..))), "{origin} {client} {frame_id}");
        }
    }

    #[test]
    fn nothing_before_the_handshake_or_from_another_origin() {
        let mut s = Socket::new(55, "f1".into(), "https://55.discordsays.com".into());
        let r = json!([1, { "cmd": "GET_CHANNEL", "args": {}, "nonce": "n" }]);
        assert!(matches!(s.on_message("https://55.discordsays.com", &r, &env()).first(), Some(Act::Close(..))));
        let mut s = open();
        assert!(matches!(s.on_message("https://evil.com", &r, &env()).first(), Some(Act::Close(..))));
    }

    #[test]
    fn channel_needs_a_scope_and_only_its_own() {
        let mut s = open();
        assert_eq!(sent(&req(&mut s, "GET_CHANNEL", json!({ "channel_id": "9" })))["data"]["code"], INVALID_PERMISSIONS);
        s.authenticated("a", json!({ "scopes": ["guilds"] }));
        assert_eq!(sent(&req(&mut s, "GET_CHANNEL", json!({ "channel_id": "9" })))["data"]["name"], "Hangout");
        assert_eq!(sent(&req(&mut s, "GET_CHANNEL", json!({ "channel_id": "10" })))["data"]["code"], INVALID_CHANNEL);
    }

    #[test]
    fn authorize_refuses_tokens_redirects_other_apps_and_big_scopes() {
        for args in [
            json!({ "client_id": "55", "response_type": "token", "scope": "identify" }),
            json!({ "client_id": "56", "response_type": "code", "scope": "identify" }),
            json!({ "client_id": "55", "response_type": "code", "scope": "identify", "redirect_uri": "https://x" }),
            json!({ "client_id": "55", "response_type": "code", "scope": ["identify", "messages.read"] }),
            json!({ "client_id": "55", "response_type": "code", "scope": ["rpc"] }),
            json!({ "client_id": "55", "response_type": "code", "scope": [] }),
        ] {
            let mut s = open();
            assert_eq!(sent(&req(&mut s, "AUTHORIZE", args.clone()))["evt"], "ERROR", "{args}");
        }
        let mut s = open();
        let out = req(&mut s, "AUTHORIZE", json!({ "client_id": "55", "response_type": "code", "scope": ["identify", "guilds"], "state": "s" }));
        assert!(matches!(out.first(), Some(Act::Authorize { scopes, quiet: false, .. }) if scopes == &["identify", "guilds"]));
        assert_eq!(sent(&req(&mut s, "AUTHORIZE", json!({ "client_id": "55", "scope": "identify" })))["evt"], "ERROR");
    }

    #[test]
    fn subscribing_sends_state_once_then_only_changes() {
        let mut s = open();
        s.authenticated("a", json!({ "scopes": ["identify"] }));
        let r = s.on_message("https://55.discordsays.com", &json!([1, { "cmd": "SUBSCRIBE", "evt": "ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE", "args": {}, "nonce": "n" }]), &env());
        assert_eq!(sent(&r)["data"]["evt"], "ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE");
        let mut v = View { me: json!({ "id": "7" }), participants: vec![json!({ "id": "7" })], voice: vec![], speaking: vec![], channel_id: 9 };
        assert_eq!(s.sync(&v).len(), 1);
        assert!(s.sync(&v).is_empty());
        v.participants.push(json!({ "id": "8" }));
        let out = s.sync(&v);
        assert_eq!(out[0][1]["data"]["participants"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_need_their_scope() {
        let mut s = open();
        s.authenticated("a", json!({ "scopes": ["identify"] }));
        let sub = |s: &mut Socket, evt: &str| {
            let r = s.on_message("https://55.discordsays.com", &json!([1, { "cmd": "SUBSCRIBE", "evt": evt, "args": { "channel_id": "9" }, "nonce": "n" }]), &env());
            sent(&r)
        };
        assert_eq!(sub(&mut s, "VOICE_STATE_UPDATE")["evt"], "ERROR");
        assert_eq!(sub(&mut s, "SPEAKING_START")["evt"], "ERROR");
        assert_eq!(sub(&mut s, "NOT_AN_EVENT")["data"]["code"], INVALID_EVENT);
        assert_eq!(sub(&mut s, "CURRENT_USER_UPDATE")["evt"], Value::Null);
    }

    #[test]
    fn speaking_changes_are_dispatched() {
        let mut s = open();
        s.authenticated("a", json!({ "scopes": ["rpc.voice.read"] }));
        for evt in ["SPEAKING_START", "SPEAKING_STOP"] {
            s.on_message("https://55.discordsays.com", &json!([1, { "cmd": "SUBSCRIBE", "evt": evt, "args": { "channel_id": "9" }, "nonce": "n" }]), &env());
        }
        let mut v = View { me: json!({}), participants: vec![], voice: vec![], speaking: vec![], channel_id: 9 };
        assert!(s.sync(&v).is_empty());
        v.speaking = vec![7];
        assert_eq!(s.sync(&v)[0][1]["evt"], "SPEAKING_START");
        v.speaking = vec![];
        assert_eq!(s.sync(&v)[0][1]["evt"], "SPEAKING_STOP");
    }

    #[test]
    fn the_no_op_commands_answer_null() {
        let mut s = open();
        let v = sent(&req(&mut s, "SET_ORIENTATION_LOCK_STATE", json!({ "lock_state": 1 })));
        assert_eq!((v["cmd"].as_str(), v["evt"].is_null(), v["data"].is_null()), (Some("SET_ORIENTATION_LOCK_STATE"), true, true));
    }
}
