use super::*;

impl Store {
    /// Applies a gateway dispatch. Returns the channel id when a new message arrived.
    pub fn apply(&mut self, t: &str, d: &Value) -> Option<Id> {
        match t {
            "READY" => self.load_ready(d),
            "READY_SUPPLEMENTAL" => self.load_supplemental(d),
            "MESSAGE_CREATE" => {
                let m = self.parse_message(d);
                let ch = m.channel;
                let mentioned = m.author != self.me && self.mentions_me(&m);
                if let Some(c) = self.channels.get_mut(&ch) {
                    c.last_msg = c.last_msg.max(m.id);
                }
                if m.author == self.me {
                    self.reads.entry(ch).or_default().last_acked = m.id;
                } else if mentioned || self.channels.get(&ch).is_some_and(|c| c.guild == 0) {
                    self.reads.entry(ch).or_default().mentions += 1;
                }
                if self.channels.get(&ch).is_some_and(|c| c.guild == 0) {
                    self.dms.retain(|c| *c != ch);
                    self.dms.insert(0, ch);
                }
                if let Some(ty) = self.typing.get_mut(&ch) {
                    ty.retain(|(u, _)| *u != m.author);
                }
                if let Some(e) = self.msgs.get_mut(&ch).filter(|e| !e.detached) {
                    if let Some(pos) = e.list.iter().position(|x| (x.pending || x.failed) && x.nonce.is_some() && x.nonce == m.nonce) {
                        e.list.remove(pos);
                    }
                    if !e.list.iter().any(|x| x.id == m.id) {
                        // unsent messages always sit at the tail
                        let sent = e.list.iter().take_while(|x| !x.pending && !x.failed).count();
                        let at = e.list[..sent].partition_point(|x| x.id < m.id);
                        e.list.insert(at, m);
                    }
                }
                return Some(ch);
            }
            "MESSAGE_UPDATE" => {
                let ch = sid(&d["channel_id"]);
                let id = sid(&d["id"]);
                if let Some(m) = self.msg_mut(ch, id) {
                    m.fill(d);
                }
            }
            "MESSAGE_DELETE" => {
                let ch = sid(&d["channel_id"]);
                let id = sid(&d["id"]);
                if let Some(e) = self.msgs.get_mut(&ch) {
                    e.list.retain(|m| m.id != id);
                }
            }
            "MESSAGE_DELETE_BULK" => {
                let ch = sid(&d["channel_id"]);
                let gone = ids(&d["ids"]);
                if let Some(e) = self.msgs.get_mut(&ch) {
                    e.list.retain(|m| !gone.contains(&m.id));
                }
            }
            "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                let add = t == "MESSAGE_REACTION_ADD";
                let me = sid(&d["user_id"]) == self.me;
                let em = ReactEmoji::parse(&d["emoji"]);
                if let Some(m) = self.msg_mut(sid(&d["channel_id"]), sid(&d["message_id"])) {
                    react(m, em, add, me);
                }
            }
            "MESSAGE_POLL_VOTE_ADD" | "MESSAGE_POLL_VOTE_REMOVE" => {
                let add = t == "MESSAGE_POLL_VOTE_ADD";
                let me = sid(&d["user_id"]) == self.me;
                let aid = d["answer_id"].as_u64().unwrap_or(0) as u8;
                if let Some(p) = self.msg_mut(sid(&d["channel_id"]), sid(&d["message_id"])).and_then(|m| m.poll.as_mut()) {
                    if let Some(a) = p.answers.iter_mut().find(|a| a.id == aid) {
                        // our own vote is already counted optimistically, so skip the echo for it;
                        // other people's always count
                        match (add, me) {
                            (true, true) if !a.me => {
                                a.count += 1;
                                a.me = true;
                            }
                            (true, false) => a.count += 1,
                            (false, true) if a.me => {
                                a.count = a.count.saturating_sub(1);
                                a.me = false;
                            }
                            (false, false) => a.count = a.count.saturating_sub(1),
                            _ => {}
                        }
                    }
                }
            }
            "MESSAGE_REACTION_REMOVE_ALL" => {
                if let Some(m) = self.msg_mut(sid(&d["channel_id"]), sid(&d["message_id"])) {
                    m.reactions.clear();
                }
            }
            "MESSAGE_REACTION_REMOVE_EMOJI" => {
                let em = ReactEmoji::parse(&d["emoji"]);
                if let Some(m) = self.msg_mut(sid(&d["channel_id"]), sid(&d["message_id"])) {
                    m.reactions.retain(|r| r.emoji != em);
                }
            }
            "CHANNEL_PINS_UPDATE" => {
                if let Some(c) = self.channels.get_mut(&sid(&d["channel_id"])) {
                    c.last_pin = iso_ms(&d["last_pin_timestamp"]);
                }
            }
            "CHANNEL_PINS_ACK" => self.reads.entry(sid(&d["channel_id"])).or_default().last_pin = iso_ms(&d["timestamp"]),
            "MESSAGE_ACK" => {
                let r = self.reads.entry(sid(&d["channel_id"])).or_default();
                let acked = sid(&d["message_id"]);
                r.last_acked = if d["manual"].as_bool() == Some(true) { acked } else { r.last_acked.max(acked) };
                r.mentions = d["mention_count"].as_u64().unwrap_or(0) as u32;
            }
            "TYPING_START" => {
                let uid = sid(&d["user_id"]);
                if uid != self.me {
                    let gid = sid(&d["guild_id"]);
                    if d["member"]["user"].is_object() {
                        self.upsert_user(&d["member"]["user"]);
                    }
                    self.upsert_member(gid, uid, &d["member"]);
                    let ty = self.typing.entry(sid(&d["channel_id"])).or_default();
                    ty.retain(|(u, _)| *u != uid);
                    ty.push((uid, Instant::now()));
                }
            }
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" => {
                let mut c = Channel::parse(d, 0);
                if c.guild == 0 {
                    c.recipients = d["recipients"]
                        .as_array()
                        .map(|a| a.iter().map(|u| self.upsert_user(u)).collect())
                        .unwrap_or_else(|| ids(&d["recipient_ids"]));
                    if !self.dms.contains(&c.id) {
                        self.dms.insert(0, c.id);
                    }
                }
                if let Some(old) = self.channels.get(&c.id) {
                    c.last_msg = c.last_msg.max(old.last_msg);
                }
                self.put_channel(c);
            }
            "EMBEDDED_ACTIVITY_UPDATE_V2" => {
                let ch = sid(&d["location"]["channel_id"]);
                let launch = d["launch_id"].as_str().unwrap_or_default();
                let id = d["composite_instance_id"].as_str().unwrap_or(launch).to_string();
                let users: Vec<Id> = d["participants"].as_array().into_iter().flatten().map(|p| sid(&p["user_id"])).collect();
                let list = self.embedded.entry(ch).or_default();
                list.retain(|i| i.id != id);
                if !users.is_empty() {
                    list.push(Instance { app: sid(&d["application_id"]), id, users });
                }
                if list.is_empty() {
                    self.embedded.remove(&ch);
                }
            }
            "CHANNEL_DELETE" => {
                self.embedded.remove(&sid(&d["id"]));
                let id = sid(&d["id"]);
                if let Some(c) = self.channels.remove(&id)
                    && let Some(g) = self.guilds.get_mut(&c.guild)
                {
                    g.channels.retain(|x| *x != id);
                }
                self.dms.retain(|x| *x != id);
            }
            "GUILD_CREATE" | "GUILD_UPDATE" => {
                self.put_guild(d);
            }
            "GUILD_DELETE" => {
                if d["unavailable"].as_bool() != Some(true) {
                    let id = sid(&d["id"]);
                    self.guilds.remove(&id);
                    self.lurking.remove(&id);
                    self.guild_order.retain(|g| *g != id);
                }
            }
            "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => {
                if let Some(g) = self.guilds.get_mut(&sid(&d["guild_id"])) {
                    let r = parse_role(&d["role"]);
                    g.roles.insert(r.id, r);
                }
            }
            "GUILD_ROLE_DELETE" => {
                if let Some(g) = self.guilds.get_mut(&sid(&d["guild_id"])) {
                    g.roles.remove(&sid(&d["role_id"]));
                }
            }
            "GUILD_MEMBER_UPDATE" => {
                let uid = self.upsert_user(&d["user"]);
                self.upsert_member(sid(&d["guild_id"]), uid, d);
            }
            "GUILD_STICKERS_UPDATE" => {
                if let Some(g) = self.guilds.get_mut(&sid(&d["guild_id"])) {
                    g.stickers = d["stickers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|s| (sid(&s["id"]), st(&s["name"]), s["format_type"].as_u64().unwrap_or(1) as u8))
                        .collect();
                }
            }
            "GUILD_EMOJIS_UPDATE" => {
                if let Some(g) = self.guilds.get_mut(&sid(&d["guild_id"])) {
                    g.emojis = d["emojis"].as_array().into_iter().flatten().map(Emoji::parse).collect();
                }
            }
            "GUILD_MEMBER_LIST_UPDATE" => self.member_list(d),
            "CHANNEL_RECIPIENT_ADD" | "CHANNEL_RECIPIENT_REMOVE" | "GUILD_MEMBER_ADD" | "GUILD_MEMBER_REMOVE" => self.live(t, d),
            "GUILD_SCHEDULED_EVENT_CREATE" | "GUILD_SCHEDULED_EVENT_UPDATE" | "GUILD_SCHEDULED_EVENT_DELETE" => {
                let gid = sid(&d["guild_id"]);
                let live = matches!(d["status"].as_u64(), Some(1 | 2));
                if let Some(g) = self.guilds.get_mut(&gid) {
                    match t {
                        "GUILD_SCHEDULED_EVENT_CREATE" if live => {
                            g.events += 1;
                            g.newest_event = g.newest_event.max(sid(&d["id"]));
                        }
                        "GUILD_SCHEDULED_EVENT_UPDATE" if !live => g.events = g.events.saturating_sub(1),
                        "GUILD_SCHEDULED_EVENT_DELETE" if live => g.events = g.events.saturating_sub(1),
                        _ => {}
                    }
                }
            }
            "THREAD_CREATE" | "THREAD_UPDATE" => {
                let mut c = Channel::parse(d, 0);
                if let Some(old) = self.channels.get(&c.id) {
                    // updates only carry `member` when membership changed
                    if d.get("member").is_none() {
                        c.joined = old.joined;
                    }
                    c.last_msg = c.last_msg.max(old.last_msg);
                }
                if d["newly_created"].as_bool() == Some(true) && c.owner == self.me {
                    c.joined = true;
                }
                if let Some(posts) = self.forum_posts.get_mut(&c.parent)
                    && !posts.contains(&c.id)
                {
                    posts.insert(0, c.id);
                }
                self.put_channel(c);
            }
            "THREAD_DELETE" => {
                let id = sid(&d["id"]);
                if let Some(c) = self.channels.remove(&id)
                    && let Some(g) = self.guilds.get_mut(&c.guild)
                {
                    g.channels.retain(|x| *x != id);
                }
            }
            "THREAD_LIST_SYNC" => {
                let gid = sid(&d["guild_id"]);
                let mine: Vec<Id> = d["members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| sid(&m["user_id"]) == self.me)
                    .map(|m| sid(&m["id"]))
                    .collect();
                for t in d["threads"].as_array().into_iter().flatten() {
                    let mut c = Channel::parse(t, gid);
                    c.joined = mine.contains(&c.id) || self.channels.get(&c.id).is_some_and(|o| o.joined);
                    self.put_channel(c);
                }
            }
            "THREAD_MEMBER_UPDATE" => {
                if sid(&d["user_id"]) == self.me
                    && let Some(c) = self.channels.get_mut(&sid(&d["id"]))
                {
                    c.joined = true;
                }
            }
            "THREAD_MEMBERS_UPDATE" => {
                let id = sid(&d["id"]);
                let removed = ids(&d["removed_member_ids"]);
                if removed.contains(&self.me)
                    && let Some(c) = self.channels.get_mut(&id)
                {
                    c.joined = false;
                }
                let guild = sid(&d["guild_id"]);
                let added: Vec<Id> = d["added_members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| {
                        let uid = sid(&m["user_id"]);
                        if m["member"]["user"].is_object() {
                            self.upsert_user(&m["member"]["user"]);
                            self.upsert_member(guild, uid, &m["member"]);
                        }
                        uid
                    })
                    .collect();
                if let Some(list) = self.thread_members.get_mut(&id) {
                    list.retain(|u| !removed.contains(u));
                    for u in added {
                        if !list.contains(&u) {
                            list.push(u);
                        }
                    }
                }
            }
            "THREAD_MEMBER_LIST_UPDATE" => {
                let guild = sid(&d["guild_id"]);
                for p in d["presences"].as_array().into_iter().flatten() {
                    let uid = sid(&p["user"]["id"]);
                    self.presence.insert(uid, Status::parse(p["status"].as_str().unwrap_or("")));
                    self.set_activities(uid, &p["activities"]);
                }
                let list = d["members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| {
                        let uid = sid(&m["user_id"]);
                        if m["member"]["user"].is_object() {
                            self.upsert_user(&m["member"]["user"]);
                            self.upsert_member(guild, uid, &m["member"]);
                        }
                        uid
                    })
                    .collect();
                self.thread_members.insert(sid(&d["thread_id"]), list);
            }
            "RELATIONSHIP_ADD" | "RELATIONSHIP_UPDATE" => {
                let uid = sid(&d["id"]);
                if d["user"].is_object() {
                    self.upsert_user(&d["user"]);
                }
                self.relationships.insert(uid, d["type"].as_u64().unwrap_or(0) as u8);
                self.set_since(uid, &d["since"]);
                self.set_nick(uid, &d["nickname"]);
            }
            "RELATIONSHIP_REMOVE" => {
                let uid = sid(&d["id"]);
                self.relationships.remove(&uid);
                self.rel_since.remove(&uid);
                self.nicks.remove(&uid);
            }
            "GUILD_MEMBERS_CHUNK" => {
                let gid = sid(&d["guild_id"]);
                for m in d["members"].as_array().into_iter().flatten() {
                    let uid = self.upsert_user(&m["user"]);
                    self.upsert_member(gid, uid, m);
                }
            }
            "PRESENCE_UPDATE" => {
                let uid = sid(&d["user"]["id"]);
                if d["user"]["username"].is_string() {
                    self.upsert_user(&d["user"]);
                }
                self.presence.insert(uid, Status::parse(d["status"].as_str().unwrap_or("")));
                self.set_activities(uid, &d["activities"]);
            }
            "VOICE_STATE_UPDATE" => {
                let gid = sid(&d["guild_id"]);
                let uid = sid(&d["user_id"]);
                if d["member"]["user"].is_object() {
                    self.upsert_user(&d["member"]["user"]);
                    self.upsert_member(gid, uid, &d["member"]);
                }
                if let Some(g) = self.guilds.get_mut(&gid) {
                    let ch = sid(&d["channel_id"]);
                    if ch == 0 {
                        g.voice.remove(&uid);
                        g.streaming.remove(&uid);
                    } else {
                        g.voice.insert(uid, ch);
                        if d["self_stream"] == true {
                            g.streaming.insert(uid);
                        } else {
                            g.streaming.remove(&uid);
                        }
                    }
                }
            }
            "USER_GUILD_SETTINGS_UPDATE" => {
                // the event carries the whole entry, so drop what the server and its channels had
                let gid = sid(&d["guild_id"]);
                let chans = self.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default();
                for id in chans.iter().chain([&gid]) {
                    self.muted.remove(id);
                    self.unreads.remove(id);
                    self.levels.remove(id);
                }
                self.guild_settings(d);
            }
            "NOTIFICATION_SETTINGS_UPDATE" => self.new_notifs = d["flags"].as_u64().unwrap_or(0) & 16 != 0,
            "USER_UPDATE" => {
                if sid(&d["id"]) == self.me && d.get("premium_type").is_some() {
                    self.me_raw["premium_type"] = d["premium_type"].clone();
                }
                self.upsert_user(d);
            }
            "CHANNEL_INFO" => {
                for c in d["channels"].as_array().into_iter().flatten() {
                    self.voice_info(sid(&c["id"]), &c["status"], &c["voice_start_time"]);
                }
            }
            "VOICE_CHANNEL_STATUS_UPDATE" => self.voice_info(sid(&d["id"]), &d["status"], &Value::Null),
            "VOICE_CHANNEL_START_TIME_UPDATE" => {
                let id = sid(&d["id"]);
                match d["voice_start_time"].as_i64() {
                    Some(t) => self.voice_start.insert(id, t),
                    None => self.voice_start.remove(&id),
                };
            }
            "USER_SETTINGS_PROTO_UPDATE" if d["settings"]["type"].as_u64() == Some(1) => {
                use base64::Engine;
                let raw = d["settings"]["proto"].as_str().and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
                if let Some(raw) = raw {
                    let full = d["partial"].as_bool() == Some(false);
                    self.load_privacy(&raw, full);
                    let folders = crate::proto::guild_folders(&raw);
                    if !folders.is_empty() {
                        let seen = self.guild_order.clone();
                        self.arrange(folders, &seen);
                    }
                    self.account.merge(crate::proto::account(&raw, full));
                    if let Some(c) = crate::proto::custom_status(&raw, chrono::Utc::now().timestamp_millis() as u64) {
                        (self.custom_status, self.custom_emoji) = Self::own_custom(c);
                    }
                    self.own_status(&raw);
                }
            }
            _ => {}
        }
        None
    }
}
