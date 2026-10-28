//! Plugins that act on the app as a whole rather than one screen: CustomIdle,
//! StreamerModeOnStream, RelationshipNotifier, NewGuildSettings, BetterSessions' background
//! check and XSOverlay.

use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{App, channel_title};
use crate::net::Tag;
use crate::plugins::{self, flag, num, on};
use crate::state::{Id, Status, sid};

#[derive(Default)]
pub struct State {
    // windows' last-input tick and when we saw it change, for CustomIdle
    input: Option<(u32, Instant)>,
    pub idle: bool,
    streaming: bool,
    sessions_at: Option<Instant>,
}

// what this client removed or left itself, which RelationshipNotifier keeps quiet about
static LEFT: Mutex<Option<HashSet<Id>>> = Mutex::new(None);

/// Every request goes past here, so our own removals can be told from other people's.
pub fn sent(method: &str, path: &str) {
    if method != "DELETE" {
        return;
    }
    let id = ["/users/@me/relationships/", "/users/@me/guilds/", "/channels/"]
        .iter()
        .find_map(|p| path.strip_prefix(p))
        .and_then(|rest| rest.parse::<Id>().ok());
    if let Some(id) = id {
        LEFT.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().insert(id);
    }
}

fn mine(id: Id) -> bool {
    LEFT.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|s| s.remove(&id))
}

fn idle_tick() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: li is a properly sized LASTINPUTINFO
    unsafe { GetLastInputInfo(&mut li) };
    li.dwTime
}

impl App {
    pub(super) fn plugin_tick(&mut self, ctx: &egui::Context) {
        if on("CustomIdle") && self.e2e.is_none() {
            let tick = idle_tick();
            let now = Instant::now();
            let since = match self.plugs.input {
                Some((t, at)) if t == tick => at,
                _ => {
                    self.plugs.input = Some((tick, now));
                    now
                }
            };
            let mins = num("CustomIdle", "idleTimeout");
            let away = mins > 0.0 && since.elapsed() >= Duration::from_secs_f64(mins * 60.0);
            if away != self.plugs.idle && self.store.presence.get(&self.store.me).is_none_or(|s| *s == Status::Online) {
                self.plugs.idle = away;
                self.send_presence();
            }
            ctx.request_repaint_after(Duration::from_secs(15));
        } else if self.plugs.idle {
            self.plugs.idle = false;
            self.send_presence();
        }

        let live = self.voice.stream.is_some();
        if live != self.plugs.streaming {
            self.plugs.streaming = live;
            if on("StreamerModeOnStream") && self.settings.streamer != live {
                self.settings.streamer = live;
                self.save_settings(ctx);
            }
        }

        if flag("BetterSessions", "backgroundCheck") && self.store.me != 0 {
            let every = Duration::from_secs_f64(num("BetterSessions", "checkInterval").max(1.0) * 60.0);
            if self.plugs.sessions_at.is_none_or(|t| t.elapsed() >= every) {
                self.plugs.sessions_at = Some(Instant::now());
                self.rest("GET", "/auth/sessions".into(), None, Tag::Remote("bs_check"));
            }
            ctx.request_repaint_after(every);
        }
    }

    /// Before the store takes in a dispatch, while it still knows what was there.
    pub(super) fn plugin_dispatch(&mut self, t: &str, d: &Value) {
        match t {
            "READY" if flag("RelationshipNotifier", "offlineRemovals") && self.store.me != 0 => {
                let friends: HashSet<Id> = d["relationships"].as_array().into_iter().flatten().filter(|r| r["type"].as_u64() == Some(1)).map(|r| sid(&r["id"])).collect();
                let guilds: HashSet<Id> = d["guilds"].as_array().into_iter().flatten().map(|g| sid(&g["id"])).collect();
                let groups: HashSet<Id> = d["private_channels"].as_array().into_iter().flatten().map(|c| sid(&c["id"])).collect();
                let mut lines = Vec::new();
                if flag("RelationshipNotifier", "friends") {
                    for (u, _) in self.store.relationships.iter().filter(|(u, k)| **k == 1 && !friends.contains(u)) {
                        lines.push(format!("{} removed you as a friend.", self.store.name_in(0, *u)));
                    }
                }
                if flag("RelationshipNotifier", "servers") && !guilds.is_empty() {
                    for g in self.store.guilds.values().filter(|g| !guilds.contains(&g.id)) {
                        lines.push(format!("You were removed from the server {}.", g.name));
                    }
                }
                if flag("RelationshipNotifier", "groups") {
                    for c in self.store.dms.iter().filter(|c| !groups.contains(c) && self.store.channels.get(c).is_some_and(|c| c.kind == 3)) {
                        lines.push(format!("You were removed from the group {}.", channel_title(&self.store, *c)));
                    }
                }
                for l in lines {
                    self.removed(l);
                }
            }
            "RELATIONSHIP_REMOVE" => {
                let u = sid(&d["id"]);
                let kind = self.store.relationships.get(&u).copied().unwrap_or(0);
                let name = self.store.name_in(0, u);
                if mine(u) || !on("RelationshipNotifier") {
                } else if kind == 1 && flag("RelationshipNotifier", "friends") {
                    self.removed(format!("{name} removed you as a friend."));
                } else if kind == 3 && flag("RelationshipNotifier", "friendRequestCancels") {
                    self.removed(format!("Friend request from {name} has been cancelled."));
                }
            }
            "GUILD_DELETE" if !d["unavailable"].as_bool().unwrap_or(false) => {
                let g = sid(&d["id"]);
                if !mine(g)
                    && flag("RelationshipNotifier", "servers")
                    && let Some(name) = self.store.guilds.get(&g).map(|g| g.name.clone())
                {
                    self.removed(format!("You were removed from the server {name}."));
                }
            }
            "CHANNEL_RECIPIENT_REMOVE" if sid(&d["user"]["id"]) == self.store.me => {
                let c = sid(&d["channel_id"]);
                if !mine(c) && flag("RelationshipNotifier", "groups") {
                    self.removed(format!("You were removed from the group {}.", channel_title(&self.store, c)));
                }
            }
            "GUILD_CREATE" if on("NewGuildSettings") => {
                let g = sid(&d["id"]);
                let joined = chrono::DateTime::parse_from_rfc3339(d["joined_at"].as_str().unwrap_or("")).map(|t| t.timestamp_millis()).unwrap_or(0);
                if !self.store.guilds.contains_key(&g) && chrono::Utc::now().timestamp_millis() - joined < 120_000 {
                    let p = "NewGuildSettings";
                    let mut body = json!({
                        "muted": flag(p, "guild"),
                        "suppress_everyone": flag(p, "everyone"),
                        "suppress_roles": flag(p, "role"),
                        "mute_scheduled_events": flag(p, "events"),
                        "notify_highlights": if flag(p, "highlights") { 1 } else { 0 },
                    });
                    let level = num(p, "messages") as u8;
                    if level != 3 {
                        body["message_notifications"] = level.into();
                    }
                    self.rest("PATCH", format!("/users/@me/guilds/{g}/settings"), Some(body), Tag::Quiet);
                }
            }
            "CALL_CREATE" | "CALL_UPDATE" if flag("XSOverlay", "callNotifications") => {
                let ringing = d["ringing"].as_array().is_some_and(|r| r.iter().any(|u| sid(u) == self.store.me));
                if ringing && self.voice.chan.is_none() {
                    let c = channel_title(&self.store, sid(&d["channel_id"]));
                    self.xsoverlay(format!("{c} is calling you..."), "Incoming call".into());
                }
            }
            _ => {}
        }
    }

    fn removed(&mut self, line: String) {
        self.toast(line.clone());
        if self.e2e.is_none() {
            crate::toast::send(crate::toast::Ping { title: "RelationshipNotifier".into(), body: line, avatar: String::new(), at: (0, 0) });
        }
    }

    /// XSOverlay's notification api over its websocket.
    pub(super) fn xsoverlay(&self, title: String, content: String) {
        if self.e2e.is_some() {
            return;
        }
        let p = "XSOverlay";
        let n = content.chars().count();
        let height = match n {
            0..=100 => 100,
            101..=200 => 150,
            201..=300 => 200,
            _ => 250,
        };
        let notif = json!({
            "type": 1,
            "timeout": num(p, "timeout"),
            "height": height,
            "opacity": num(p, "opacity"),
            "volume": num(p, "volume"),
            "audioPath": "default",
            "title": title,
            "content": content,
            "useBase64Icon": false,
            "icon": "default",
            "sourceApp": "Fastcord",
        });
        let msg = json!({ "sender": "Fastcord", "target": "xsoverlay", "command": "SendNotification", "jsonData": notif.to_string(), "rawData": null });
        let url = format!("ws://127.0.0.1:{}/?client=Fastcord", num(p, "webSocketPort") as u16);
        self.net.rt.spawn(async move {
            use futures_util::SinkExt;
            if let Ok((mut ws, _)) = tokio_tungstenite::connect_async(url).await {
                let _ = ws.send(tokio_tungstenite::tungstenite::Message::Text(msg.to_string().into())).await;
                let _ = ws.close(None).await;
            }
        });
    }
}

/// BetterSessions: a notification for each session we haven't seen before.
pub fn sessions_checked(app: &mut App, res: Result<Value, String>) {
    let Ok(v) = res else { return };
    let mut seen: Vec<Value> = plugins::get("BetterSessions", "seen").as_array().cloned().unwrap_or_default();
    let first = seen.is_empty();
    for s in v["user_sessions"].as_array().into_iter().flatten() {
        let hash = s["id_hash"].clone();
        if seen.contains(&hash) {
            continue;
        }
        seen.push(hash);
        let ci = &s["client_info"];
        if !first {
            let body = format!("New session:\n{} · {} · {}", ci["os"].as_str().unwrap_or(""), ci["platform"].as_str().unwrap_or(""), ci["location"].as_str().unwrap_or(""));
            app.toast(body.clone());
            if app.e2e.is_none() {
                crate::toast::send(crate::toast::Ping { title: "BetterSessions".into(), body, avatar: String::new(), at: (0, 0) });
            }
        }
    }
    plugins::set("BetterSessions", "seen", seen.into());
}
