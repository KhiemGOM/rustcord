//! Voice channels and DM calls. The UI side (`VoiceUi`) lives on `App` and
//! talks to the main gateway; the session itself runs on the net runtime.

pub mod audio;
mod conn;
mod rtp;

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

pub struct Connect {
    pub tag: u64,
    pub endpoint: String,
    pub token: String,
    pub session: String,
    pub server: Id,
    pub channel: Id,
    pub me: Id,
    pub shared: Arc<Shared>,
}

pub enum Cmd {
    Connect(Connect),
    Leave,
}

pub struct Ev {
    pub tag: u64,
    pub kind: Kind,
}

pub enum Kind {
    State(State),
    Speaking(Id, bool),
    Closed(Option<String>),
}

/// Net-side handle on the running session; dropping the sender ends it.
#[derive(Default)]
pub struct Link {
    ctl: Option<mpsc::Sender<()>>,
}

impl Link {
    pub fn cmd(&mut self, c: Cmd, tx: &Tx) {
        match c {
            Cmd::Connect(c) => {
                let (s, r) = mpsc::channel(1);
                self.ctl = Some(s);
                tokio::spawn(conn::run(c, tx.clone(), r));
            }
            Cmd::Leave => self.ctl = None,
        }
    }
}

#[derive(Default)]
pub struct VoiceUi {
    pub state: State,
    pub guild: Id,
    pub chan: Option<Id>,
    pub mute: bool,
    pub deaf: bool,
    pub speaking: HashSet<Id>,
    /// who is in each DM call, from CALL_* and guildless voice states
    pub calls: HashMap<Id, Vec<Id>>,
    pub shared: Arc<Shared>,
    session: Option<String>,
    server: Option<(String, String, Id)>,
    live: Option<(String, String)>,
    tag: u64,
}

impl VoiceUi {
    pub fn join(&mut self, net: &Net, store: &Store, ch: Id) {
        if self.chan == Some(ch) {
            return;
        }
        let Some(c) = store.channels.get(&ch) else { return };
        if self.chan.is_some() {
            net.send(net::Cmd::Voice(Cmd::Leave));
        }
        let ring = c.guild == 0 && !self.calls.get(&ch).is_some_and(|u| !u.is_empty());
        self.chan = Some(ch);
        self.guild = c.guild;
        self.state = State::Joining;
        self.session = None;
        self.server = None;
        self.live = None;
        self.speaking.clear();
        self.tag += 1;
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
        self.chan = None;
        self.op4(net);
        self.reset(net);
    }

    fn reset(&mut self, net: &Net) {
        net.send(net::Cmd::Voice(Cmd::Leave));
        self.chan = None;
        self.state = State::Idle;
        self.session = None;
        self.server = None;
        self.live = None;
        self.speaking.clear();
        self.tag += 1;
    }

    pub fn set_mute(&mut self, net: &Net, on: bool) {
        self.mute = on;
        // undeafening by unmuting matches the official client
        if !on && self.deaf {
            self.deaf = false;
        }
        self.sync(net);
    }

    pub fn set_deaf(&mut self, net: &Net, on: bool) {
        self.deaf = on;
        self.sync(net);
    }

    fn sync(&mut self, net: &Net) {
        self.shared.mute.store(self.mute, Relaxed);
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
            "self_video": false,
        }})));
    }

    pub fn dispatch(&mut self, net: &Net, me: Id, t: &str, d: &Value) {
        match t {
            "VOICE_STATE_UPDATE" => {
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
            }
            "CALL_DELETE" => {
                self.calls.remove(&sid(&d["channel_id"]));
            }
            _ => {}
        }
    }

    fn connect(&mut self, net: &Net, me: Id) {
        let (Some(session), Some((endpoint, token, server)), Some(ch)) = (&self.session, &self.server, self.chan) else {
            return;
        };
        // our own voice state echoes back on every mute toggle; only a new session or token means reconnect
        let key = (session.clone(), token.clone());
        if self.live.as_ref() == Some(&key) {
            return;
        }
        self.live = Some(key);
        self.tag += 1;
        self.state = State::Connecting;
        self.speaking.clear();
        net.send(net::Cmd::Voice(Cmd::Connect(Connect {
            tag: self.tag,
            endpoint: endpoint.clone(),
            token: token.clone(),
            session: session.clone(),
            server: *server,
            channel: ch,
            me,
            shared: self.shared.clone(),
        })));
    }

    /// Returns an error worth showing the user, if the session died.
    pub fn event(&mut self, e: Ev) -> Option<String> {
        if e.tag != self.tag {
            return None;
        }
        match e.kind {
            Kind::State(s) => self.state = s,
            Kind::Speaking(u, true) => {
                self.speaking.insert(u);
            }
            Kind::Speaking(u, false) => {
                self.speaking.remove(&u);
            }
            Kind::Closed(err) => {
                self.state = State::Idle;
                self.speaking.clear();
                if err.is_some() {
                    self.chan = None;
                    self.session = None;
                    self.server = None;
                    self.live = None;
                }
                return err;
            }
        }
        None
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
