use super::*;

const DEN: Id = 1008;
const HANGOUT: Id = 2801;
const DM: Id = 3000;
const ALICE: Id = 101;
const WALTER: Id = 102;

fn event(app: &mut App, t: &str, d: serde_json::Value) {
    app.rest("POST", "/mock/event".into(), Some(json!({ "t": t, "d": d })), crate::net::Tag::Quiet);
}

fn count(name: &str) -> usize {
    crate::sound::heard().iter().filter(|n| *n == name).count()
}

fn dm_voice(user: Id, joined: bool) -> serde_json::Value {
    json!({ "guild_id": null, "channel_id": joined.then(|| DM.to_string()), "user_id": user.to_string(), "session_id": "other" })
}

impl Run {
    pub(super) fn call_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let me = app.store.me;
        match self.step {
            2100 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&HANGOUT) {
                    return;
                }
                app.select_guild(Some(DEN));
                app.open(HANGOUT);
                app.join_voice(HANGOUT, false);
                if count("user_join") > 0 {
                    self.mark("call_cue_own_join");
                }
                self.next(0);
            }
            2101 => {
                let ready = app.voice.state == State::Connected && app.voice.chan == Some(HANGOUT) && app.voice.pings.len() >= 2;
                if !ready {
                    return;
                }
                if count("stream_started") > 0 {
                    self.mark("call_cue_stream_started");
                }
                if app.voice.pings.iter().any(|(_, ms)| (30..=400).contains(ms)) && app.rects.contains_key("voice:status") {
                    self.mark("call_ping_live");
                }
                if app.rects.contains_key(&format!("voice:tile_state:{ALICE}")) {
                    self.mark("call_tile_state");
                }
                self.shot(ctx, "call_connected_panel.png");
                self.cues = count("user_join");
                event(app, "VOICE_STATE_UPDATE", json!({ "guild_id": DEN.to_string(), "channel_id": HANGOUT.to_string(), "user_id": WALTER.to_string(), "session_id": "walter" }));
                self.next(0);
            }
            2102 => {
                if count("user_join") <= self.cues {
                    return;
                }
                self.mark("call_cue_others_join");
                self.cues = count("user_leave");
                event(app, "VOICE_STATE_UPDATE", json!({ "guild_id": DEN.to_string(), "channel_id": null, "user_id": WALTER.to_string(), "session_id": "walter" }));
                self.next(0);
            }
            2103 => {
                if count("user_leave") <= self.cues {
                    return;
                }
                self.mark("call_cue_others_leave");
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            2104 => {
                if app.voice.chan.is_some() || count("disconnect") == 0 {
                    return;
                }
                self.mark("call_cue_disconnect");
                self.cues = count("call_ringing");
                event(
                    app,
                    "CALL_CREATE",
                    json!({ "channel_id": DM.to_string(), "message_id": "1", "region": "local", "ringing": [me.to_string()], "voice_states": [dm_voice(ALICE, true)] }),
                );
                self.next(0);
            }
            2105 => {
                if app.voice.ringing != Some(DM) || count("call_ringing") <= self.cues || self.call_toasts.is_empty() {
                    return;
                }
                self.mark("call_incoming");
                if self.call_toasts.iter().any(|x| x.contains("incomingCall") && x.contains("Answer") && x.contains("Decline")) {
                    self.mark("call_toast");
                }
                if app.rects.contains_key(&format!("dmbubble:{DM}")) && app.rects.contains_key(&format!("dmcall:{DM}")) {
                    self.mark("call_ring_rail");
                }
                self.next(3700);
            }
            2106 => {
                if count("call_ringing") >= self.cues + 2 {
                    self.mark("call_ring_loops");
                }
                let Some(r) = app.rects.get("call:decline").copied() else { return };
                self.shot(ctx, "call_incoming.png");
                self.click(r.center());
                self.next(300);
            }
            2107 => {
                if app.voice.ringing.is_some() {
                    return;
                }
                self.cues = count("call_ringing");
                self.next(3700);
            }
            2108 => {
                if count("call_ringing") == self.cues {
                    self.mark("call_ring_stops");
                }
                event(app, "CALL_DELETE", json!({ "channel_id": DM.to_string() }));
                app.select_guild(None);
                app.open(DM);
                app.join_voice(DM, false);
                self.next(0);
            }
            2109 => {
                if app.voice.state != State::Connected || app.voice.chan != Some(DM) {
                    return;
                }
                self.cues = count("call_calling");
                event(app, "VOICE_STATE_UPDATE", dm_voice(ALICE, false));
                event(app, "CALL_UPDATE", json!({ "channel_id": DM.to_string(), "ringing": [ALICE.to_string()], "voice_states": [dm_voice(me, true)] }));
                self.next(0);
            }
            2110 => {
                if count("call_calling") <= self.cues {
                    return;
                }
                self.mark("call_calling_rings");
                self.next(3700);
            }
            2111 => {
                self.cues = count("user_join");
                event(app, "CALL_UPDATE", json!({ "channel_id": DM.to_string(), "ringing": [], "voice_states": [dm_voice(me, true)] }));
                event(app, "VOICE_STATE_UPDATE", dm_voice(ALICE, true));
                self.next(300);
            }
            2112 => {
                if count("user_join") <= self.cues {
                    return;
                }
                self.mark("call_cue_pickup");
                self.cues = count("call_calling");
                self.next(3700);
            }
            2113 => {
                if count("call_calling") == self.cues {
                    self.mark("call_calling_stops");
                }
                let now = chrono::Utc::now();
                let id = (((now.timestamp_millis() - 1_420_070_400_000) as u64) << 22).to_string();
                event(
                    app,
                    "MESSAGE_CREATE",
                    json!({
                        "id": id, "channel_id": DM.to_string(), "author": { "id": ALICE.to_string(), "username": "alice", "global_name": "Alice", "avatar": "av101" },
                        "content": "", "type": 3, "timestamp": now.to_rfc3339(), "call": { "participants": [ALICE.to_string()] },
                    }),
                );
                self.next(0);
            }
            2114 => {
                if !app.rects.contains_key(&format!("call_msg_join:{DM}")) {
                    return;
                }
                self.mark("call_msg_ongoing");
                self.shot(ctx, "call_message.png");
                let msg = app.store.msgs.get(&DM).and_then(|m| m.list.iter().rev().find(|m| m.kind == 3)).map(|m| m.id).unwrap_or(0);
                let ended = chrono::DateTime::from_timestamp_millis(crate::state::snowflake_ms(msg) + 125_000).unwrap_or_default().to_rfc3339();
                event(
                    app,
                    "MESSAGE_UPDATE",
                    json!({ "id": msg.to_string(), "channel_id": DM.to_string(), "call": { "participants": [ALICE.to_string(), me.to_string()], "ended_timestamp": ended } }),
                );
                self.next(0);
            }
            2115 => {
                let ended = app.store.msgs.get(&DM).and_then(|m| m.list.iter().rev().find(|m| m.kind == 3)).and_then(|m| m.call.as_ref()).is_some_and(|c| c.ended.is_some());
                if !ended || app.rects.contains_key(&format!("call_msg_join:{DM}")) {
                    return;
                }
                self.mark("call_msg_ended");
                let Some(r) = app.rects.get("voice:disconnect").copied() else { return };
                event(app, "CALL_DELETE", json!({ "channel_id": DM.to_string() }));
                self.click(r.center());
                self.next(0);
            }
            _ => {
                if app.voice.chan.is_none() {
                    self.step = 81;
                    self.next(300);
                }
            }
        }
    }
}
