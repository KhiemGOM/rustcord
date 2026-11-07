//! Gateway events that change something already on screen outside the message list: the open
//! settings pages, the inbox, the soundboard popout, quests and the events window.

use serde_json::{Value, json};

use super::App;
use crate::net::Tag;
use crate::state::{Id, Sound, Status, sid};

impl App {
    pub fn live(&mut self, t: &str, d: &Value) {
        if matches!(t, "PRESENCE_UPDATE" | "TYPING_START" | "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_ACK" | "VOICE_STATE_UPDATE" | "GUILD_MEMBER_LIST_UPDATE") {
            return;
        }
        let gid = sid(&d["guild_id"]);
        match t {
            "GUILD_BAN_ADD" | "GUILD_BAN_REMOVE" => {
                let uid = sid(&d["user"]["id"]);
                let add = t.ends_with("ADD");
                if let Some(Value::Array(a)) = self.server_page(gid, "bans") {
                    a.retain(|b| sid(&b["user"]["id"]) != uid);
                    if add {
                        a.insert(0, json!({ "user": d["user"], "reason": null }));
                    }
                }
            }
            "GUILD_MEMBER_ADD" => {
                for m in self.member_tables(gid) {
                    m.stale();
                }
            }
            "GUILD_MEMBER_REMOVE" => {
                let uid = sid(&d["user"]["id"]);
                for m in self.member_tables(gid) {
                    m.drop_user(uid);
                }
            }
            "GUILD_INTEGRATIONS_UPDATE" | "INTEGRATION_CREATE" | "INTEGRATION_UPDATE" | "INTEGRATION_DELETE" => {
                self.reload(gid, "integrations", format!("/guilds/{gid}/integrations?include_applications=true&include_role_connections_metadata=true"));
                self.reload(gid, "command_index", format!("/guilds/{gid}/application-command-index"));
            }
            "GUILD_AUDIT_LOG_ENTRY_CREATE" => super::server::audit_changed(self, gid),
            "GUILD_JOIN_REQUEST_CREATE" | "GUILD_JOIN_REQUEST_UPDATE" | "GUILD_JOIN_REQUEST_DELETE" => super::applications::event(self, t, d),
            "INVITE_CREATE" | "INVITE_DELETE" => self.reload(gid, "invites", format!("/guilds/{gid}/invites")),
            "USER_SETTINGS_UPDATE" => {
                let me = self.store.me;
                if let Some(s) = d["status"].as_str().filter(|s| matches!(*s, "online" | "idle" | "dnd" | "invisible")) {
                    self.store.presence.insert(me, Status::parse(s));
                }
                if let Some(c) = d.get("custom_status") {
                    self.store.custom_status = c["text"].as_str().filter(|t| !t.is_empty()).map(str::to_string);
                }
            }
            "WEBHOOKS_UPDATE" => {
                self.reload(gid, "webhooks", format!("/guilds/{gid}/webhooks"));
                let ch = sid(&d["channel_id"]);
                if self.chan_settings.as_ref().is_some_and(|s| s.chan == ch && s.data.contains_key("webhooks")) {
                    self.rest("GET", format!("/channels/{ch}/webhooks"), None, Tag::Chan("webhooks"));
                }
            }
            "GUILD_SOUNDBOARD_SOUND_CREATE" | "GUILD_SOUNDBOARD_SOUND_UPDATE" => {
                let id = sid(&d["sound_id"]);
                if let Some(items) = self.server_page(gid, "sounds").and_then(|v| v.get_mut("items")).and_then(Value::as_array_mut) {
                    items.retain(|s| sid(&s["sound_id"]) != id);
                    items.insert(0, d.clone());
                }
                if let Some((g, Some(list))) = &mut self.soundboard
                    && *g == gid
                {
                    match list.iter().position(|s| s.id == id) {
                        Some(i) => list[i] = Sound::parse(d),
                        None => list.insert(0, Sound::parse(d)),
                    }
                }
            }
            "GUILD_SOUNDBOARD_SOUND_DELETE" => {
                let id = sid(&d["sound_id"]);
                if let Some(items) = self.server_page(gid, "sounds").and_then(|v| v.get_mut("items")).and_then(Value::as_array_mut) {
                    items.retain(|s| sid(&s["sound_id"]) != id);
                }
                if let Some((g, Some(list))) = &mut self.soundboard
                    && *g == gid
                {
                    list.retain(|s| s.id != id);
                }
            }
            "GUILD_SOUNDBOARD_SOUNDS_UPDATE" | "SOUNDBOARD_SOUNDS" => {
                let sounds = d["soundboard_sounds"].clone();
                if let Some(v) = self.server_page(gid, "sounds") {
                    *v = json!({ "items": sounds });
                }
                if let Some((g, Some(list))) = &mut self.soundboard
                    && *g == gid
                {
                    let keep: Vec<Sound> = list.iter().filter(|s| s.guild != gid).cloned().collect();
                    *list = sounds.as_array().into_iter().flatten().map(Sound::parse).chain(keep).collect();
                }
            }
            "GUILD_APPLIED_BOOSTS_UPDATE" | "GUILD_POWERUP_ENTITLEMENTS_CREATE" | "GUILD_POWERUP_ENTITLEMENTS_DELETE" => {
                if self.boosts == Some(gid) {
                    self.rest("GET", format!("/guilds/{gid}/powerups?include_ends_at=true"), None, Tag::Boosts(gid, 1));
                    self.rest("GET", format!("/guilds/{gid}/premium/subscriptions?include_ended=true"), None, Tag::Boosts(gid, 2));
                }
            }
            "GUILD_SCHEDULED_EVENT_USER_ADD" | "GUILD_SCHEDULED_EVENT_USER_REMOVE" => {
                let add = t.ends_with("ADD");
                let (ev, uid, me) = (sid(&d["guild_scheduled_event_id"]), sid(&d["user_id"]), self.store.me);
                let Some(e) = self.events.as_mut().filter(|e| e.guild == gid) else { return };
                if uid == me && e.rsvp.contains(&ev) == add {
                    return;
                }
                if uid == me {
                    if add {
                        e.rsvp.insert(ev);
                    } else {
                        e.rsvp.remove(&ev);
                    }
                }
                if let Some(x) = e.list.as_mut().and_then(|l| l.iter_mut().find(|x| sid(&x["id"]) == ev)) {
                    let n = x["user_count"].as_u64().unwrap_or(0);
                    x["user_count"] = json!(if add { n + 1 } else { n.saturating_sub(1) });
                }
                e.users.remove(&ev);
                if e.detail == Some(ev) {
                    self.event_users(gid, ev);
                }
            }
            "USER_NOTE_UPDATE" => {
                self.notes.insert(sid(&d["id"]), d["note"].as_str().unwrap_or_default().to_string());
            }
            "USER_CONNECTIONS_UPDATE" => self.reload_remote("connections", "/users/@me/connections"),
            "SESSIONS_REPLACE" => {
                self.reload_remote("sessions", "/auth/sessions");
                let me = self.store.me;
                let best = d.as_array().and_then(|a| a.iter().find(|s| s["active"].as_bool() == Some(true)).or(a.first()));
                if let Some(s) = best.and_then(|s| s["status"].as_str()).filter(|s| matches!(*s, "online" | "idle" | "dnd" | "invisible"))
                    && self.store.status(me) != Status::parse(s)
                {
                    self.store.presence.insert(me, Status::parse(s));
                }
            }
            "SAVED_MESSAGE_CREATE" | "SAVED_MESSAGE_DELETE" => {
                let del = t.ends_with("DELETE");
                let sd = if del { d } else { &d["save_data"] };
                let id = sid(&sd["message_id"]);
                if del {
                    self.bookmarks.remove(&id);
                } else if sd["due_at"].is_null() {
                    self.bookmarks.insert(id);
                }
                match self.inbox.as_ref().map(|i| i.tab) {
                    Some(2) => self.rest("GET", "/users/@me/saved-messages/bookmarks?limit=25".into(), None, Tag::Saved(1)),
                    Some(3) => self.rest("GET", "/users/@me/saved-messages?bookmark_ids=true".into(), None, Tag::Saved(0)),
                    _ => {}
                }
            }
            "RECENT_MENTION_DELETE" => {
                let id = sid(&d["message_id"]);
                if let Some(l) = self.inbox.as_mut().and_then(|i| i.mentions.as_mut()) {
                    l.retain(|m| m.id != id);
                }
            }
            "QUESTS_USER_STATUS_UPDATE" => {
                let st = &d["user_status"];
                let id = st["quest_id"].as_str().unwrap_or_default();
                if let Some(q) = &mut self.quests {
                    for x in q.offered.iter_mut().chain(q.claimed.iter_mut()).flatten().filter(|x| x["id"].as_str() == Some(id)) {
                        x["user_status"] = st.clone();
                    }
                }
            }
            "VIRTUAL_CURRENCY_BALANCE_UPDATE" if self.quests.is_some() => {
                self.rest("GET", "/users/@me/virtual-currency/balance".into(), None, Tag::Quests(2));
            }
            "ENTITLEMENT_CREATE" | "ENTITLEMENT_UPDATE" | "ENTITLEMENT_DELETE" => {
                self.reload_remote("gifts", "/users/@me/entitlements/gifts");
                self.reload_remote("gift_codes", "/users/@me/entitlements/gift-codes");
                self.reload_remote("credits", "/users/@me/entitlements?with_sku=false&with_application=false&entitlement_type=11&exclude_ended=true");
            }
            "USER_PAYMENT_SOURCES_UPDATE" => self.reload_remote("sources", "/users/@me/billing/payment-sources"),
            "USER_SUBSCRIPTIONS_UPDATE" => self.reload_remote("subs", "/users/@me/billing/subscriptions?sync_level=1"),
            "PAYMENT_UPDATE" => self.reload_remote("payments", "/users/@me/billing/payments?limit=30"),
            "USER_PREMIUM_GUILD_SUBSCRIPTION_SLOT_CREATE" | "USER_PREMIUM_GUILD_SUBSCRIPTION_SLOT_UPDATE" => {
                self.reload_remote("slots", "/users/@me/guilds/premium/subscription-slots");
                self.reload_remote("cooldown", "/users/@me/guilds/premium/subscriptions/cooldown");
            }
            "GIFT_CODE_UPDATE" => {
                let code = d["code"].as_str().unwrap_or_default();
                if let Some(Some(Ok(Value::Array(list)))) = self.remote.get_mut("gift_codes") {
                    for g in list.iter_mut().filter(|g| g["code"].as_str() == Some(code)) {
                        g["uses"] = d["uses"].clone();
                    }
                }
            }
            "PRESENCES_REPLACE" => {
                for p in d.as_array().into_iter().flatten() {
                    self.store.apply("PRESENCE_UPDATE", p);
                }
            }
            _ => {}
        }
    }

    fn server_page(&mut self, gid: Id, key: &'static str) -> Option<&mut Value> {
        self.server.as_mut().filter(|s| s.guild == gid)?.data.get_mut(key).filter(|v| !v.is_null())
    }

    fn reload(&mut self, gid: Id, key: &'static str, path: String) {
        if self.server.as_ref().is_some_and(|s| s.guild == gid && s.data.contains_key(key)) {
            self.rest("GET", path, None, Tag::Server(key));
        }
    }

    fn reload_remote(&mut self, key: &'static str, path: &str) {
        if self.remote.contains_key(key) {
            self.rest("GET", path.to_string(), None, Tag::Remote(key));
        }
    }

    fn member_tables(&mut self, gid: Id) -> Vec<&mut super::community::Members> {
        let mut out = Vec::new();
        if let Some(s) = self.server.as_mut().filter(|s| s.guild == gid) {
            out.push(&mut s.pages.members);
        }
        if let Some(h) = self.home.as_mut().filter(|h| h.guild == gid) {
            out.push(&mut h.members);
        }
        out
    }
}
