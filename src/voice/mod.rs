//! Voice channels and DM calls. The UI side (`VoiceUi`) lives on `App` and
//! talks to the main gateway; the session itself runs on the net runtime.

pub mod audio;
mod capture;
mod conn;
mod hwenc;
mod rtp;
pub mod stage;
mod video;

pub use capture::cameras;
pub use video::{Quality, Source};

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use tokio::sync::mpsc;

use crate::net::{self, Net, Tx};
use crate::state::{Id, Store, sid};
pub use audio::Shared;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Joining,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Media {
    Voice,
    /// someone else's Go Live, receive only
    Watch,
    /// our own screen share
    GoLive,
}

pub enum Ctl {
    Camera(bool),
}

pub struct Connect {
    pub kind: Media,
    pub tag: u64,
    pub endpoint: String,
    pub token: String,
    pub session: String,
    pub server: Id,
    pub channel: Id,
    pub me: Id,
    pub shared: Arc<Shared>,
    // for our own Go Live: what to capture and how
    pub share: Option<(video::Source, video::Quality)>,
}

pub enum Cmd {
    Connect(Connect),
    Leave,
    EndStream,
    Ctl(Ctl),
}

pub struct Ev {
    pub tag: u64,
    pub kind: Kind,
}

pub enum Kind {
    State(State),
    Speaking(Id, bool),
    /// a decoded frame of someone's video, or our own preview
    Frame(Id, Arc<egui::ColorImage>),
    VideoOff(Id),
    Error(String),
    Closed(Option<String>),
    /// heartbeat round trip, ms
    Ping(u32),
    /// how much of our audio the server lost lately, percent
    Loss(f32),
    /// DAVE's privacy code for the call, while it's end-to-end encrypted
    Privacy(Option<String>),
}

/// Net-side handles on the running sessions (voice, and at most one stream); dropping a sender ends it.
#[derive(Default)]
pub struct Link {
    voice: Option<mpsc::Sender<Ctl>>,
    stream: Option<mpsc::Sender<Ctl>>,
}

impl Link {
    pub fn cmd(&mut self, c: Cmd, tx: &Tx) {
        match c {
            Cmd::Connect(c) => {
                let (s, r) = mpsc::channel(4);
                match c.kind {
                    Media::Voice => self.voice = Some(s),
                    _ => self.stream = Some(s),
                }
                tokio::spawn(conn::run(c, tx.clone(), r));
            }
            Cmd::Leave => {
                self.voice = None;
                self.stream = None;
            }
            Cmd::EndStream => self.stream = None,
            Cmd::Ctl(c) => {
                if let Some(v) = &self.voice {
                    let _ = v.try_send(c);
                }
            }
        }
    }
}

/// A Go Live session: someone else's we're watching, or our own.
pub struct Live {
    pub key: String,
    pub user: Id,
    pub mine: bool,
    pub state: State,
    pub frame: Option<Arc<egui::ColorImage>>,
    server: Option<Id>,
    endpoint: Option<(String, String)>,
    tag: u64,
    share: Option<(video::Source, video::Quality)>,
}

#[derive(Default)]
pub struct VoiceUi {
    pub state: State,
    pub guild: Id,
    pub chan: Option<Id>,
    pub mute: bool,
    pub deaf: bool,
    pub speaking: HashSet<Id>,
    // for the connection popout: (unix seconds, ms) over the last few minutes, the latest
    // outbound loss, and the call's privacy code
    pub pings: std::collections::VecDeque<(f64, u32)>,
    pub loss: Option<f32>,
    pub privacy: Option<String>,
    /// who is in each DM call, from CALL_* and guildless voice states
    pub calls: HashMap<Id, Vec<Id>>,
    /// a dm call ringing for us, while we're in no call
    pub ringing: Option<Id>,
    pub camera: bool,
    /// latest camera frame per user, ours included
    pub video: HashMap<Id, Arc<egui::ColorImage>>,
    /// who has their camera on / is streaming, from voice states
    pub cams: HashSet<Id>,
    pub live: HashSet<Id>,
    /// muted, server muted, deafened, server deafened, for whoever is any of them
    pub muted: HashMap<Id, [bool; 4]>,
    pub stream: Option<Live>,
    pub stage: stage::Stage,
    pub suppressed: bool,
    pub tex: HashMap<String, (egui::TextureHandle, usize)>,
    pub shared: Arc<Shared>,
    session: Option<String>,
    server: Option<(String, String, Id)>,
    joined: Option<(String, String)>,
    tag: u64,
    /// one counter for voice and stream sessions, so their events can never be mistaken for each other
    tags: u64,
}

impl VoiceUi {
    /// `ring` false calls a dm without ringing anyone, as shift+click on the call button does.
    pub fn join(&mut self, net: &Net, store: &Store, ch: Id, ring: bool) {
        if self.chan == Some(ch) {
            return;
        }
        let Some(c) = store.channels.get(&ch) else { return };
        if self.chan.is_some() {
            // moving channels drops the stream connection and the camera along with the old call
            if let Some(l) = self.stream.take() {
                net.send(net::Cmd::Gateway(json!({ "op": 19, "d": { "stream_key": l.key } })));
            }
            net.send(net::Cmd::Voice(Cmd::Leave));
            self.camera = false;
            self.video.clear();
            self.tex.clear();
        }
        let ring = ring && c.guild == 0 && !self.calls.get(&ch).is_some_and(|u| !u.is_empty());
        self.chan = Some(ch);
        self.guild = c.guild;
        self.state = State::Joining;
        self.session = None;
        self.server = None;
        self.joined = None;
        self.speaking.clear();
        self.pings.clear();
        self.loss = None;
        self.privacy = None;
        self.tag = self.next_tag();
        self.op4(net);
        if ring {
            net.send(net::Cmd::Rest {
                method: "POST",
                path: format!("/channels/{ch}/call/ring"),
                body: Some(json!({ "recipients": null })),
                tag: net::Tag::Quiet,
            });
        }
    }

    pub fn leave(&mut self, net: &Net) {
        if self.chan.is_none() {
            return;
        }
        crate::sound::cue("disconnect");
        self.chan = None;
        self.op4(net);
        self.reset(net);
    }

    /// The voice server's name, like discord shows it: the endpoint's first label.
    pub fn host(&self) -> Option<&str> {
        let e = self.endpoint()?;
        let e = e.split_once("://").map_or(e, |(_, rest)| rest);
        e.split(['.', ':']).next().filter(|h| !h.is_empty())
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.server.as_ref().map(|s| s.0.as_str())
    }

    fn reset(&mut self, net: &Net) {
        net.send(net::Cmd::Voice(Cmd::Leave));
        if let Some(l) = self.stream.take() {
            net.send(net::Cmd::Gateway(json!({ "op": 19, "d": { "stream_key": l.key } })));
        }
        self.camera = false;
        self.video.clear();
        self.tex.clear();
        self.chan = None;
        self.state = State::Idle;
        self.session = None;
        self.server = None;
        self.joined = None;
        self.speaking.clear();
        self.suppressed = false;
        self.tag = self.next_tag();
    }

    pub fn set_mute(&mut self, net: &Net, on: bool) {
        crate::sound::cue(if on { "mute" } else { "unmute" });
        self.mute = on;
        // undeafening by unmuting matches the official client
        if !on && self.deaf {
            self.deaf = false;
        }
        self.sync(net);
    }

    pub fn set_deaf(&mut self, net: &Net, on: bool) {
        crate::sound::cue(if on { "deafen" } else { "undeafen" });
        self.deaf = on;
        self.sync(net);
    }

    fn sync(&mut self, net: &Net) {
        self.shared.mute.store(self.mute || self.suppressed, Relaxed);
        self.shared.deaf.store(self.deaf, Relaxed);
        if self.chan.is_some() {
            self.op4(net);
        }
    }

    fn op4(&self, net: &Net) {
        let guild = (self.guild != 0).then(|| self.guild.to_string());
        net.send(net::Cmd::Gateway(json!({ "op": 4, "d": {
            "guild_id": guild,
            "channel_id": self.chan.map(|c| c.to_string()),
            "self_mute": self.mute || self.deaf,
            "self_deaf": self.deaf,
            "self_video": self.camera,
        }})));
    }

    pub fn set_camera(&mut self, net: &Net, me: Id, on: bool) {
        self.camera = on;
        if !on {
            self.video.remove(&me);
        }
        self.op4(net);
        net.send(net::Cmd::Voice(Cmd::Ctl(Ctl::Camera(on))));
    }

    fn next_tag(&mut self) -> u64 {
        self.tags += 1;
        self.tags
    }

    fn stream_key(&self, user: Id) -> Option<String> {
        let ch = self.chan?;
        Some(if self.guild != 0 { format!("guild:{}:{ch}:{user}", self.guild) } else { format!("call:{ch}:{user}") })
    }

    fn start_stream(&mut self, user: Id, share: Option<(video::Source, video::Quality)>) -> Option<String> {
        let key = self.stream_key(user)?;
        let tag = self.next_tag();
        let mine = share.is_some();
        self.stream = Some(Live { key: key.clone(), user, mine, state: State::Joining, frame: None, server: None, endpoint: None, tag, share });
        Some(key)
    }

    pub fn watch(&mut self, net: &Net, user: Id) {
        if self.stream.as_ref().is_some_and(|l| l.user == user) {
            return;
        }
        self.end_stream(net);
        if let Some(key) = self.start_stream(user, None) {
            net.send(net::Cmd::Gateway(json!({ "op": 20, "d": { "stream_key": key } })));
        }
    }

    pub fn go_live(&mut self, net: &Net, me: Id, src: video::Source, q: video::Quality) {
        self.end_stream(net);
        let Some(key) = self.start_stream(me, Some((src, q))) else { return };
        let (kind, guild) = if self.guild != 0 { ("guild", Some(self.guild.to_string())) } else { ("call", None) };
        net.send(net::Cmd::Gateway(json!({ "op": 18, "d": {
            "type": kind, "guild_id": guild, "channel_id": self.chan.map(|c| c.to_string()), "preferred_region": null,
        }})));
        net.send(net::Cmd::Gateway(json!({ "op": 22, "d": { "stream_key": key, "paused": false } })));
    }

    pub fn end_stream(&mut self, net: &Net) {
        self.tex.remove("stream");
        if let Some(l) = self.stream.take() {
            net.send(net::Cmd::Gateway(json!({ "op": 19, "d": { "stream_key": l.key } })));
            net.send(net::Cmd::Voice(Cmd::EndStream));
        }
    }

    fn connect_stream(&mut self, net: &Net, me: Id) {
        let Some(session) = self.session.clone() else { return };
        // a fresh tag, so the connection this one replaces can't end the stream as it closes
        let tag = self.next_tag();
        let Some(l) = &mut self.stream else { return };
        let (Some(server), Some((endpoint, token))) = (l.server, l.endpoint.clone()) else { return };
        l.state = State::Connecting;
        l.tag = tag;
        net.send(net::Cmd::Voice(Cmd::Connect(Connect {
            kind: if l.mine { Media::GoLive } else { Media::Watch },
            tag: l.tag,
            endpoint,
            token,
            session,
            server,
            // streams get their own E2EE group, keyed one below the rtc server id
            channel: server - 1,
            me,
            shared: self.shared.clone(),
            share: l.share,
        })));
    }

    // camera and Go Live flags ride along on every voice state
    fn flags(&mut self, vs: &Value) {
        let uid = sid(&vs["user_id"]);
        let gone = vs["channel_id"].is_null();
        for (set, key) in [(&mut self.cams, "self_video"), (&mut self.live, "self_stream")] {
            if !gone && vs[key].as_bool() == Some(true) {
                set.insert(uid);
            } else {
                set.remove(&uid);
            }
        }
        let on = |k: &str| vs[k].as_bool() == Some(true);
        let m = [on("self_mute") || on("mute"), on("mute"), on("self_deaf") || on("deaf"), on("deaf")];
        if gone || !m.contains(&true) {
            self.muted.remove(&uid);
        } else {
            self.muted.insert(uid, m);
        }
    }

    pub fn dispatch(&mut self, net: &Net, me: Id, t: &str, d: &Value) {
        self.stage.dispatch(t, d);
        match t {
            "READY" | "READY_SUPPLEMENTAL" => {
                for g in d["guilds"].as_array().into_iter().flatten() {
                    for vs in g["voice_states"].as_array().into_iter().flatten() {
                        self.flags(vs);
                    }
                }
            }
            "GUILD_CREATE" => {
                for vs in d["voice_states"].as_array().into_iter().flatten() {
                    self.flags(vs);
                }
            }
            "STREAM_CREATE" | "STREAM_UPDATE" | "STREAM_SERVER_UPDATE" => {
                let Some(l) = &mut self.stream else { return };
                if d["stream_key"].as_str() != Some(&l.key) {
                    return;
                }
                // discord sends STREAM_UPDATE whenever viewers come and go or it's paused; only a new
                // server or endpoint means connecting again
                let moved = if t == "STREAM_SERVER_UPDATE" {
                    let Some(ep) = d["endpoint"].as_str() else { return };
                    let at = Some((ep.to_string(), d["token"].as_str().unwrap_or("").to_string()));
                    std::mem::replace(&mut l.endpoint, at.clone()) != at
                } else if let Some(r) = d["rtc_server_id"].as_str().and_then(|r| r.parse().ok()) {
                    l.server.replace(r) != Some(r)
                } else {
                    false
                };
                if moved {
                    self.connect_stream(net, me);
                }
            }
            "STREAM_DELETE" => {
                if self.stream.as_ref().is_some_and(|l| d["stream_key"].as_str() == Some(&l.key)) {
                    self.stream = None;
                    net.send(net::Cmd::Voice(Cmd::EndStream));
                }
            }
            "VOICE_STATE_UPDATE" => {
                self.flags(d);
                self.stage.seat(d);
                let uid = sid(&d["user_id"]);
                let ch = sid(&d["channel_id"]);
                if sid(&d["guild_id"]) == 0 {
                    for users in self.calls.values_mut() {
                        users.retain(|u| *u != uid);
                    }
                    if ch != 0 {
                        self.calls.entry(ch).or_default().push(uid);
                    }
                }
                if uid != me || self.chan.is_none() {
                    return;
                }
                if ch == 0 {
                    // the server dropped us (kicked, or we joined from another device)
                    self.reset(net);
                    return;
                }
                if Some(ch) != self.chan {
                    self.chan = Some(ch);
                }
                let audience = d["suppress"].as_bool() == Some(true);
                if audience != self.suppressed {
                    self.suppressed = audience;
                    self.sync(net);
                }
                self.session = d["session_id"].as_str().map(str::to_string);
                self.connect(net, me);
            }
            "VOICE_SERVER_UPDATE" => {
                let Some(ch) = self.chan else { return };
                let Some(endpoint) = d["endpoint"].as_str() else { return };
                let server = match sid(&d["guild_id"]) {
                    0 => ch,
                    g => g,
                };
                if server != self.guild && server != ch {
                    return;
                }
                self.server = Some((endpoint.to_string(), d["token"].as_str().unwrap_or("").to_string(), server));
                self.connect(net, me);
            }
            "CALL_CREATE" | "CALL_UPDATE" => {
                let ch = sid(&d["channel_id"]);
                if let Some(vs) = d["voice_states"].as_array() {
                    self.calls.insert(ch, vs.iter().map(|v| sid(&v["user_id"])).collect());
                }
                let rung = d["ringing"].as_array().is_some_and(|r| r.iter().any(|u| sid(u) == me));
                if rung && self.chan != Some(ch) {
                    self.ringing = Some(ch);
                } else if self.ringing == Some(ch) {
                    self.ringing = None;
                }
            }
            "CALL_DELETE" => {
                let ch = sid(&d["channel_id"]);
                self.calls.remove(&ch);
                if self.ringing == Some(ch) {
                    self.ringing = None;
                }
            }
            _ => {}
        }
    }

    fn connect(&mut self, net: &Net, me: Id) {
        let (Some(session), Some((endpoint, token, server)), Some(ch)) = (self.session.clone(), self.server.clone(), self.chan) else {
            return;
        };
        // our own voice state echoes back on every mute toggle; only a new session or token means reconnect
        let key = (session.clone(), token.clone());
        if self.joined.as_ref() == Some(&key) {
            return;
        }
        self.joined = Some(key);
        self.tag = self.next_tag();
        self.state = State::Connecting;
        self.speaking.clear();
        net.send(net::Cmd::Voice(Cmd::Connect(Connect {
            kind: Media::Voice,
            tag: self.tag,
            endpoint,
            token,
            session,
            server,
            channel: ch,
            me,
            shared: self.shared.clone(),
            share: None,
        })));
    }

    /// Returns an error worth showing the user, if the session died.
    pub fn event(&mut self, e: Ev) -> Option<String> {
        if let Some(l) = &mut self.stream
            && e.tag == l.tag
        {
            match e.kind {
                Kind::State(s) => l.state = s,
                Kind::Frame(_, img) => l.frame = Some(img),
                Kind::Error(err) => return Some(err),
                Kind::Closed(err) => {
                    self.stream = None;
                    return err.map(|e| format!("stream ended: {e}"));
                }
                _ => {}
            }
            return None;
        }
        if e.tag != self.tag {
            return None;
        }
        match e.kind {
            Kind::Frame(u, img) => {
                self.video.insert(u, img);
            }
            Kind::VideoOff(u) => {
                self.video.remove(&u);
                self.tex.remove(&format!("cam:{u}"));
            }
            Kind::Error(err) => return Some(err),
            Kind::State(s) => self.state = s,
            Kind::Speaking(u, true) => {
                self.speaking.insert(u);
            }
            Kind::Speaking(u, false) => {
                self.speaking.remove(&u);
            }
            Kind::Ping(ms) => {
                let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
                self.pings.push_back((now, ms));
                while self.pings.front().is_some_and(|(t, _)| now - t > 300.0) {
                    self.pings.pop_front();
                }
            }
            Kind::Loss(p) => self.loss = Some(p),
            Kind::Privacy(c) => self.privacy = c,
            Kind::Closed(err) => {
                self.state = State::Idle;
                self.speaking.clear();
                if err.is_some() {
                    self.chan = None;
                    self.session = None;
                    self.server = None;
                    self.joined = None;
                }
                return err;
            }
        }
        None
    }

    /// Device choice is read when a call starts; the VAD threshold applies live.
    pub fn configure(&self, s: &crate::cache::Settings) {
        self.shared.vad_db.store(s.vad_db, Relaxed);
        self.shared.auto_vad.store(s.auto_sensitivity, Relaxed);
        self.shared.denoise.store(s.noise_suppression, Relaxed);
        self.shared.ptt.store(s.ptt, Relaxed);
        self.shared.ptt_delay_ms.store(s.ptt_delay_ms, Relaxed);
        self.shared.in_gain.store(s.input_volume.to_bits(), Relaxed);
        self.shared.out_gain.store(s.output_volume.to_bits(), Relaxed);
        if let Ok(mut p) = self.shared.peers.lock() {
            *p = s.peers.iter().filter(|(_, v)| v.muted || v.volume != 100.0).map(|(u, v)| (*u, if v.muted { 0.0 } else { v.volume / 100.0 })).collect();
        }
        if let Ok(mut d) = self.shared.devices.lock() {
            *d = (s.mic.clone(), s.speaker.clone());
        }
    }

    pub fn is_speaking(&self, u: Id) -> bool {
        self.speaking.contains(&u)
    }

    /// Everyone in a voice channel or DM call, in a stable order.
    pub fn members(&self, store: &Store, ch: Id) -> Vec<Id> {
        let mut v: Vec<Id> = match store.channels.get(&ch).map(|c| c.guild) {
            Some(g) if g != 0 => store
                .guilds
                .get(&g)
                .map(|g| g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, _)| *u).collect())
                .unwrap_or_default(),
            _ => self.calls.get(&ch).cloned().unwrap_or_default(),
        };
        if self.chan == Some(ch) && !v.contains(&store.me) {
            v.push(store.me);
        }
        v.sort();
        v.dedup();
        v
    }
}
