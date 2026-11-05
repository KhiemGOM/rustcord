//! What stage channels add to voice: who is a speaker, who has a hand up, and the live stages.

use serde_json::Value;
use std::collections::HashMap;

use crate::state::{Id, sid};

#[derive(Clone, Copy, Default)]
pub struct Seat {
    pub audience: bool,
    pub hand: Option<i64>,
}

#[derive(Clone, Default)]
pub struct Live {
    pub topic: String,
}

#[derive(Default)]
pub struct Stage {
    pub seats: HashMap<Id, Seat>,
    pub live: HashMap<Id, Live>,
}

impl Stage {
    pub fn seat(&mut self, vs: &Value) {
        let uid = sid(&vs["user_id"]);
        if vs["channel_id"].is_null() {
            self.seats.remove(&uid);
            return;
        }
        let hand = vs["request_to_speak_timestamp"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis());
        self.seats.insert(uid, Seat { audience: vs["suppress"].as_bool() == Some(true), hand });
    }

    pub fn instance(&mut self, d: &Value) {
        let ch = sid(&d["channel_id"]);
        let live = Live { topic: d["topic"].as_str().unwrap_or("").to_string() };
        self.live.insert(ch, live);
    }

    pub fn dispatch(&mut self, t: &str, d: &Value) {
        match t {
            "READY" | "READY_SUPPLEMENTAL" => {
                for g in d["guilds"].as_array().into_iter().flatten() {
                    self.guild(g);
                }
            }
            "GUILD_CREATE" => self.guild(d),
            "STAGE_INSTANCE_CREATE" | "STAGE_INSTANCE_UPDATE" => self.instance(d),
            "STAGE_INSTANCE_DELETE" => {
                self.live.remove(&sid(&d["channel_id"]));
            }
            _ => {}
        }
    }

    fn guild(&mut self, g: &Value) {
        for s in g["stage_instances"].as_array().into_iter().flatten() {
            self.instance(s);
        }
        for vs in g["voice_states"].as_array().into_iter().flatten() {
            self.seat(vs);
        }
    }

    pub fn split(&self, members: &[Id]) -> (Vec<Id>, Vec<Id>) {
        let (mut speakers, mut audience): (Vec<Id>, Vec<Id>) = members.iter().copied().partition(|u| self.seats.get(u).is_none_or(|s| !s.audience));
        speakers.sort();
        audience.sort_by_key(|u| (self.seats.get(u).and_then(|s| s.hand).unwrap_or(i64::MAX), *u));
        (speakers, audience)
    }
}
