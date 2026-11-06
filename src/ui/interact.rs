use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::App;
use super::botmodal::Modal;
use crate::net::Cmd;
use crate::state::{Busy, Channel, Id, Interaction, Map, Message, Stage, make_nonce, sid};

pub const LOADING: u64 = 1 << 7;

const MODAL_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Command,
    Component,
    Modal,
}

pub struct Waiting {
    pub channel: Id,
    pub msg: Id,
    pub kind: Kind,
    pub at: Instant,
}

#[derive(Clone)]
pub enum Press {
    Button,
    Select(u8, Vec<String>),
}

#[derive(Default)]
pub struct State {
    pub waiting: Map<String, Waiting>,
    pub modal: Option<Modal>,
    seen: Option<Id>,
}

impl App {
    fn interact(&mut self, payload: Value, w: Waiting) {
        let nonce = payload["nonce"].as_str().unwrap_or_default().to_string();
        self.interactions.waiting.insert(nonce, w);
        self.net.send(Cmd::Interact(payload));
    }

    fn app_bot(&mut self, scope: Id, app: Id) -> Id {
        let found = self.commands.get(&scope).and_then(|c| c.apps.iter().find(|a| a.0 == app)).cloned();
        if self.store.user(app).is_none() {
            let (name, icon) = found.map(|a| (a.1, a.2)).unwrap_or_default();
            self.store.upsert_user(&json!({ "id": app.to_string(), "username": name, "avatar": icon, "bot": true }));
        }
        app
    }

    /// Sends a command as discord does: the message that says it's on its way goes in first, under the nonce.
    pub fn send_command(&mut self, chan: &Channel, cmd: &Value, ty: u8, options: Vec<Value>, files: Vec<PathBuf>, target: Option<Id>, location: &str) {
        let scope = if chan.guild != 0 { chan.guild } else { chan.id };
        let nonce = make_nonce();
        let attachments: Vec<Value> = files
            .iter()
            .enumerate()
            .map(|(i, f)| json!({ "id": i.to_string(), "filename": f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "path": f }))
            .collect();
        let mut data = json!({
            "version": cmd["version"],
            "id": cmd["id"],
            "name": cmd["name"],
            "type": ty,
            "options": options,
            "application_command": cmd,
            "attachments": attachments,
        });
        if let Some(t) = target {
            data["target_id"] = json!(t.to_string());
        }
        if cmd["guild_id"].is_string() {
            data["guild_id"] = cmd["guild_id"].clone();
        }
        let mut payload = json!({
            "type": 2,
            "application_id": cmd["application_id"],
            "channel_id": chan.id.to_string(),
            "session_id": self.store.session,
            "data": data,
            "nonce": nonce,
            "analytics_location": location,
        });
        if chan.guild != 0 {
            payload["guild_id"] = json!(chan.guild.to_string());
        }
        let app = sid(&cmd["application_id"]);
        let author = self.app_bot(scope, app);
        let id: Id = nonce.parse().unwrap_or(0);
        let label = label(&options_path(&payload["data"]["options"]), cmd["name"].as_str().unwrap_or(""));
        let m = Message {
            id,
            channel: chan.id,
            author,
            kind: 20,
            nonce: Some(nonce.clone()),
            pending: true,
            app_id: app,
            interaction: Some(Interaction { user: self.store.me, owner: 0, in_guild: false, name: label }),
            busy: Some(Busy { nonce: nonce.clone(), component: String::new(), stage: Stage::Sending }),
            ..Default::default()
        };
        self.store.msgs.entry(chan.id).or_default().list.push(m);
        self.interact(payload, Waiting { channel: chan.id, msg: id, kind: Kind::Command, at: Instant::now() });
    }

    /// A button pressed or a menu answered on an app's message.
    pub fn press(&mut self, ch: Id, msg: Id, custom_id: &str, what: Press) {
        let Some(m) = self.store.find_msg(ch, msg) else { return };
        if m.busy.as_ref().is_some_and(|b| !matches!(b.stage, Stage::Failed(..))) {
            return;
        }
        let (flags, app) = (m.flags, if m.app_id != 0 { m.app_id } else { m.author });
        let guild = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
        let nonce = make_nonce();
        let data = match &what {
            Press::Button => json!({ "component_type": 2, "custom_id": custom_id }),
            Press::Select(kind, values) => json!({ "component_type": kind, "custom_id": custom_id, "type": kind, "values": values }),
        };
        let mut payload = json!({
            "type": 3,
            "nonce": nonce,
            "channel_id": ch.to_string(),
            "message_flags": flags,
            "message_id": msg.to_string(),
            "application_id": app.to_string(),
            "session_id": self.store.session,
            "data": data,
        });
        if guild != 0 {
            payload["guild_id"] = json!(guild.to_string());
        }
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg)) {
            m.busy = Some(Busy { nonce: nonce.clone(), component: custom_id.to_string(), stage: Stage::Sending });
        }
        self.interact(payload, Waiting { channel: ch, msg, kind: Kind::Component, at: Instant::now() });
    }

    pub fn submit_modal(&mut self, payload: Value, channel: Id) {
        self.interact(payload, Waiting { channel, msg: 0, kind: Kind::Modal, at: Instant::now() });
    }

    pub fn on_interaction(&mut self, t: &str, d: &Value) {
        let nonce = d["nonce"].as_str().unwrap_or_default().to_string();
        match t {
            "INTERACTION_CREATE" => {
                if let Some(w) = self.interactions.waiting.get(&nonce) {
                    let (ch, msg) = (w.channel, w.msg);
                    self.with_busy(ch, msg, |b| {
                        if matches!(b.stage, Stage::Sending) {
                            b.stage = Stage::Created;
                        }
                    });
                }
            }
            "INTERACTION_SUCCESS" => {
                let Some(w) = self.interactions.waiting.remove(&nonce) else { return };
                match w.kind {
                    Kind::Command => self.drop_local(w.channel, &nonce),
                    Kind::Component => self.settle(w.channel, w.msg, &nonce),
                    Kind::Modal => {
                        self.interactions.modal = None;
                    }
                }
            }
            "INTERACTION_FAILURE" => {
                let why = d["reason_code"].as_u64().map(|_| "The application did not respond".to_string());
                self.interaction_failed(&nonce, why);
            }
            "INTERACTION_MODAL_CREATE" => {
                if let Some(w) = self.interactions.waiting.remove(&nonce) {
                    self.drop_local(w.channel, &nonce);
                }
                self.interactions.modal = Modal::open(d);
            }
            "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE" => self.autocomplete_response(&nonce, d),
            _ => {}
        }
    }

    pub fn interaction_failed(&mut self, nonce: &str, why: Option<String>) {
        let Some(w) = self.interactions.waiting.remove(nonce) else {
            self.autocomplete_failed(nonce);
            return;
        };
        match w.kind {
            Kind::Command => {
                let sent = w.at;
                let text = why.unwrap_or_else(|| "This interaction failed".into());
                if let Some(m) = self.store.msgs.get_mut(&w.channel).and_then(|e| e.list.iter_mut().find(|m| m.nonce.as_deref() == Some(nonce) && m.pending)) {
                    m.pending = false;
                    m.busy = Some(Busy { nonce: nonce.to_string(), component: String::new(), stage: Stage::Failed(text, sent) });
                }
            }
            Kind::Component => {
                let text = why.unwrap_or_else(|| "This interaction failed".into());
                self.with_busy(w.channel, w.msg, |b| b.stage = Stage::Failed(text, w.at));
            }
            Kind::Modal => {
                if let Some(m) = &mut self.interactions.modal {
                    m.failed();
                }
            }
        }
    }

    pub fn interact_failed(&mut self, nonce: &str, status: u16, message: String) {
        let why = if status == 429 {
            "You are being rate limited.".to_string()
        } else if message.is_empty() {
            "This interaction failed".to_string()
        } else {
            message
        };
        self.interaction_failed(nonce, Some(why));
    }

    fn with_busy(&mut self, ch: Id, msg: Id, f: impl FnOnce(&mut Busy)) {
        if let Some(b) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg)).and_then(|m| m.busy.as_mut()) {
            f(b);
        }
    }

    fn settle(&mut self, ch: Id, msg: Id, nonce: &str) {
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg))
            && m.busy.as_ref().is_some_and(|b| b.nonce == nonce)
        {
            m.busy = None;
        }
    }

    fn drop_local(&mut self, ch: Id, nonce: &str) {
        if let Some(e) = self.store.msgs.get_mut(&ch) {
            e.list.retain(|m| !(m.pending && m.nonce.as_deref() == Some(nonce) && m.busy.is_some()));
        }
    }

    /// Failed attempts stay up until you leave the channel; a modal nobody answers gives up.
    pub fn interactions_tick(&mut self) {
        let cur = self.cur();
        if self.interactions.seen != cur {
            if let Some(old) = self.interactions.seen
                && let Some(e) = self.store.msgs.get_mut(&old)
            {
                e.list.retain(|m| !(failed(m) && m.nonce.as_deref() == Some(m.id.to_string().as_str())));
                for m in e.list.iter_mut().filter(|m| failed(m)) {
                    m.busy = None;
                }
            }
            self.interactions.seen = cur;
        }
        let stale: Vec<String> = self.interactions.waiting.iter().filter(|(_, w)| w.kind == Kind::Modal && w.at.elapsed() > MODAL_WAIT).map(|(n, _)| n.clone()).collect();
        for n in stale {
            self.interaction_failed(&n, None);
        }
    }
}

fn failed(m: &Message) -> bool {
    m.busy.as_ref().is_some_and(|b| matches!(b.stage, Stage::Failed(..)))
}

fn options_path(opts: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = opts;
    while let Some(o) = cur[0].as_object().filter(|o| matches!(o.get("type").and_then(Value::as_u64), Some(1 | 2))) {
        out.push(o["name"].as_str().unwrap_or("").to_string());
        cur = &cur[0]["options"];
    }
    out
}

fn label(path: &[String], root: &str) -> String {
    std::iter::once(root.to_string()).chain(path.iter().cloned()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_command_is_named_by_its_path() {
        let opts = json!([{ "type": 2, "name": "config", "options": [{ "type": 1, "name": "show", "options": [{ "type": 5, "name": "verbose", "value": true }] }] }]);
        assert_eq!(options_path(&opts), vec!["config".to_string(), "show".to_string()]);
        assert_eq!(label(&options_path(&opts), "team"), "team config show");
        assert_eq!(label(&options_path(&json!([{ "type": 3, "name": "what", "value": "x" }])), "remind"), "remind");
    }
}
