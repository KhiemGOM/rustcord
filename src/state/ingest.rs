use super::*;

impl Store {
    /// One user_guild_settings entry: mutes and "unreads" overrides for a server and its channels.
    /// Notification levels don't hide unreads, only these do.
    pub(super) fn guild_settings(&mut self, s: &Value) {
        let gid = sid(&s["guild_id"]);
        let now = chrono::Utc::now();
        // a timed mute that already ran out still says muted: true until the client notices
        let on = |o: &Value| {
            o["muted"].as_bool() == Some(true)
                && o["mute_config"]["end_time"]
                    .as_str()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .is_none_or(|t| t > now)
        };
        // the flag bits differ between the guild entry and channel overrides
        let unreads = |o: &Value, all: u64, mentions: u64| {
            let f = o["flags"].as_u64().unwrap_or(0);
            if f & all != 0 {
                Some(true)
            } else if f & mentions != 0 {
                Some(false)
            } else {
                None
            }
        };
        if on(s) {
            self.muted.insert(gid);
        }
        if s["hide_muted_channels"].as_bool() == Some(true) {
            self.hide_muted.insert(gid);
        } else {
            self.hide_muted.remove(&gid);
        }
        if s["mute_scheduled_events"].as_bool() == Some(true) {
            self.events_muted.insert(gid);
        } else {
            self.events_muted.remove(&gid);
        }
        let prefs = self.gprefs.entry(gid).or_default();
        if let Some(on) = s["suppress_everyone"].as_bool() {
            prefs.suppress_everyone = on;
        }
        if let Some(on) = s["suppress_roles"].as_bool() {
            prefs.suppress_roles = on;
        }
        prefs.highlights_off = s["notify_highlights"].as_u64() == Some(2);
        prefs.mobile_push = s["mobile_push"].as_bool() != Some(false);
        if let Some(all) = unreads(s, 2048, 4096) {
            self.unreads.insert(gid, all);
        }
        if let Some(n) = s["message_notifications"].as_u64().filter(|n| *n < 3) {
            self.levels.insert(gid, n as u8);
        }
        if let Some(flags) = s["flags"].as_u64() {
            if flags & (1 << 14) != 0 {
                self.opt_in.insert(gid);
            } else {
                self.opt_in.remove(&gid);
            }
        }
        for o in s["channel_overrides"].as_array().into_iter().flatten() {
            let cid = sid(&o["channel_id"]);
            match o["collapsed"].as_bool() {
                Some(true) => {
                    self.collapsed.insert(cid);
                }
                Some(false) => {
                    self.collapsed.remove(&cid);
                }
                None => {}
            }
            if let Some(flags) = o["flags"].as_u64() {
                self.chan_flags.insert(cid, flags);
                if flags & (1 << 12) != 0 {
                    self.opted.insert(cid);
                } else {
                    self.opted.remove(&cid);
                }
                if gid == 0 {
                    if flags & 2048 != 0 {
                        self.pinned.insert(cid);
                    } else {
                        self.pinned.remove(&cid);
                    }
                }
            }
            if on(o) {
                self.muted.insert(cid);
            }
            if let Some(all) = unreads(o, 1024, 512) {
                self.unreads.insert(cid, all);
            }
            if let Some(n) = o["message_notifications"].as_u64().filter(|n| *n < 3) {
                self.levels.insert(cid, n as u8);
            }
        }
    }

    /// Whether every new message counts as unread, rather than only @mentions. Without the newer
    /// notification settings that's always so; with them the channel's (or its parent's) unread
    /// setting wins, then the server's, then the server's default notification level.
    pub(super) fn unreads_all(&self, c: &Channel) -> bool {
        if !self.new_notifs {
            return true;
        }
        let mut id = c.id;
        for _ in 0..3 {
            if let Some(all) = self.unreads.get(&id) {
                return *all;
            }
            match self.channels.get(&id) {
                Some(c) if c.parent != 0 => id = c.parent,
                _ => break,
            }
        }
        match self.unreads.get(&c.guild) {
            Some(all) => *all,
            None => self.guilds.get(&c.guild).is_none_or(|g| g.default_notif == 0),
        }
    }

    pub fn unread(&self, c: &Channel) -> bool {
        !self.is_muted(c) && self.unread_row(c)
    }

    /// Whether the channel's own row shows as unread. A muted category keeps its channels out of
    /// the server's unread mark, but discord still lights the rows inside it; only a channel's own
    /// mute quiets its row.
    pub fn unread_row(&self, c: &Channel) -> bool {
        if c.last_msg == 0 || self.muted.contains(&c.id) || (c.is_thread() && !c.joined) {
            return false;
        }
        if c.guild != 0 && !self.unreads_all(c) {
            return false;
        }
        match self.reads.get(&c.id) {
            Some(r) => c.last_msg > r.last_acked,
            None => c.kind == 1 || c.kind == 3,
        }
    }

    pub fn mentions(&self, ch: Id) -> u32 {
        self.reads.get(&ch).map(|r| r.mentions).unwrap_or(0)
    }

    pub fn guild_badge(&self, gid: Id) -> (bool, u32) {
        let Some(g) = self.guilds.get(&gid) else {
            return (false, 0);
        };
        let mut unread = false;
        let mut men = 0;
        for c in g.channels.iter().filter_map(|c| self.channels.get(c)) {
            if !c.is_text() {
                continue;
            }
            men += self.mentions(c.id);
            if !unread && !self.muted.contains(&gid) && self.unread(c) && self.can_view(c) && self.listed(c) {
                unread = true;
            }
        }
        (unread, men)
    }

    pub(super) fn set_activities(&mut self, uid: Id, v: &Value) {
        let list: Vec<_> = v
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                let e = &a["emoji"];
                let emoji = match (e["id"].as_str(), e["name"].as_str()) {
                    (Some(id), _) => Some(format!("{}/emojis/{id}.png?size=32", crate::api::cdn())),
                    (None, Some(n)) => Some(crate::markdown::twemoji(n)),
                    _ => None,
                };
                let t = &a["timestamps"];
                let party = a["party"]["size"].as_array().filter(|p| p.len() == 2).map(|p| (p[0].as_u64().unwrap_or(0) as u32, p[1].as_u64().unwrap_or(0) as u32));
                Activity {
                    kind: a["type"].as_u64().unwrap_or(0) as u8,
                    name: st(&a["name"]),
                    details: ost(&a["details"]),
                    state: ost(&a["state"]),
                    emoji,
                    app: sid(&a["application_id"]),
                    start: t["start"].as_i64().unwrap_or(0),
                    end: t["end"].as_i64().unwrap_or(0),
                    large: ost(&a["assets"]["large_image"]),
                    small: ost(&a["assets"]["small_image"]),
                    party,
                    sync_id: ost(&a["sync_id"]),
                }
            })
            .collect();
        if list.is_empty() {
            self.activities.remove(&uid);
        } else {
            self.activities.insert(uid, list);
        }
    }

    /// The member list's second line: the custom status if there is one, else what they're
    /// doing ("Playing", bold name). (emoji url, prefix, bold part, the kind of activity going on
    /// beside a custom status)
    pub fn activity_line(&self, uid: Id) -> Option<(Option<String>, String, String, Option<u8>)> {
        let list = self.activities.get(&uid)?;
        if let Some(a) = list.iter().find(|a| a.kind == 4)
            && (a.state.is_some() || a.emoji.is_some())
        {
            return Some((a.emoji.clone(), a.state.clone().unwrap_or_default(), String::new(), list.iter().find(|a| a.kind != 4).map(|a| a.kind)));
        }
        let a = list.iter().find(|a| a.kind != 4)?;
        let verb = match a.kind {
            1 => "Streaming ",
            2 => "Listening to ",
            3 => "Watching ",
            5 => "Competing in ",
            _ => "Playing ",
        };
        Some((None, verb.to_string(), a.name.clone(), None))
    }

    pub fn upsert_user(&mut self, v: &Value) -> Id {
        let mut u = User::parse(v);
        let id = u.id;
        if id == 0 {
            return id;
        }
        // some payloads (member list sync among them) carry a trimmed user; what they leave out
        // isn't gone
        if let Some(old) = self.users.get(&id) {
            if v.get("display_name_styles").is_none() {
                u.style = old.style.clone();
                u.font = old.font;
            }
            if v.get("primary_guild").is_none() {
                u.tag = old.tag.clone();
            }
            if v.get("avatar_decoration_data").is_none() {
                u.decoration = old.decoration.clone();
            }
            if v.get("collectibles").is_none() {
                u.nameplate = old.nameplate.clone();
            }
        }
        self.users.insert(id, u);
        id
    }

    pub(super) fn upsert_member(&mut self, guild: Id, user: Id, v: &Value) {
        if guild == 0 || user == 0 || !v.is_object() {
            return;
        }
        let boost = v["premium_since"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(0, |t| t.timestamp());
        let m = Member { nick: ost(&v["nick"]), roles: ids(&v["roles"]), boost };
        if user == self.me
            && let Some(g) = self.guilds.get_mut(&guild)
        {
            g.my_roles = m.roles.clone();
            g.my_flags = v["flags"].as_u64().unwrap_or(g.my_flags);
            if let Some(t) = v.get("communication_disabled_until") {
                g.my_timeout = t.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(0, |t| t.timestamp());
            }
            if let Some(t) = v["joined_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
                g.joined = t.timestamp();
            }
        }
        self.members.entry(guild).or_default().insert(user, m);
    }

    pub(super) fn put_channel(&mut self, c: Channel) {
        if c.guild != 0
            && let Some(g) = self.guilds.get_mut(&c.guild)
            && !g.channels.contains(&c.id)
        {
            g.channels.push(c.id);
        }
        self.channels.insert(c.id, c);
    }

    pub(super) fn put_guild(&mut self, v: &Value) -> Id {
        let p = if v["properties"].is_object() { &v["properties"] } else { v };
        let id = sid(&v["id"]);
        if id == 0 || v["unavailable"].as_bool() == Some(true) {
            return id;
        }
        let mut g = self.guilds.remove(&id).unwrap_or_default();
        g.id = id;
        g.name = st(&p["name"]);
        g.icon = ost(&p["icon"]);
        g.owner = sid(&p["owner_id"]);
        g.rules = sid(&p["rules_channel_id"]);
        g.afk = sid(&p["afk_channel_id"]);
        g.default_notif = p["default_message_notifications"].as_u64().unwrap_or(0) as u8;
        g.banner = ost(&p["banner"]);
        g.vanity = ost(&p["vanity_url_code"]);
        g.boosts = p["premium_subscription_count"].as_u64().or(v["premium_subscription_count"].as_u64()).unwrap_or(0) as u32;
        g.boost_bar = p["premium_progress_bar_enabled"].as_bool().unwrap_or(false);
        g.tier = p["premium_tier"].as_u64().or(v["premium_tier"].as_u64()).unwrap_or(0) as u8;
        if let Some(e) = v["guild_scheduled_events"].as_array() {
            g.events = e.iter().filter(|e| matches!(e["status"].as_u64(), Some(1 | 2))).count() as u32;
            g.newest_event = e.iter().map(|e| sid(&e["id"])).max().unwrap_or(0);
        }
        if let Some(t) = v["joined_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
            g.joined = t.timestamp();
        }
        if let Some(n) = v["member_count"].as_u64() {
            g.member_count = n as u32;
        }
        if let Some(roles) = v["roles"].as_array() {
            g.roles = roles.iter().map(parse_role).map(|r| (r.id, r)).collect();
        }
        if let Some(e) = v["emojis"].as_array() {
            g.emojis = e.iter().map(Emoji::parse).collect();
        }
        if let Some(f) = v["features"].as_array() {
            g.features = f.iter().filter_map(|x| x.as_str()).map(str::to_string).collect();
        }
        if let Some(t) = v.get("theme") {
            g.theme = guild_theme(t);
        }
        if let Some(i) = p.get("incidents_data") {
            let secs = |k: &str| i[k].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(0, |t| t.timestamp());
            g.invites_until = secs("invites_disabled_until");
            g.dms_until = secs("dms_disabled_until");
        }
        if let Some(list) = v["stickers"].as_array() {
            g.stickers = list.iter().map(|s| (sid(&s["id"]), st(&s["name"]), s["format_type"].as_u64().unwrap_or(1) as u8)).collect();
        }
        for vs in v["voice_states"].as_array().into_iter().flatten() {
            g.voice.insert(sid(&vs["user_id"]), sid(&vs["channel_id"]));
            if vs["self_stream"] == true {
                g.streaming.insert(sid(&vs["user_id"]));
            }
        }
        self.guilds.insert(id, g);
        if let Some(chs) = v["channels"].as_array() {
            let old: Vec<Id> = std::mem::take(&mut self.guilds.get_mut(&id).unwrap().channels);
            for c in old {
                self.channels.remove(&c);
            }
            for c in chs {
                self.put_channel(Channel::parse(c, id));
            }
        }
        for t in v["threads"].as_array().into_iter().flatten() {
            self.put_channel(Channel::parse(t, id));
        }
        for m in v["members"].as_array().into_iter().flatten() {
            let uid = sid(&m["user"]["id"]);
            if !m["user"].is_null() {
                self.upsert_user(&m["user"]);
            }
            self.upsert_member(id, uid, m);
        }
        if !self.guild_order.contains(&id) {
            self.guild_order.push(id);
        }
        id
    }

    pub fn load_ready(&mut self, d: &Value) {
        self.session = st(&d["session_id"]);
        let me = self.upsert_user(&d["user"]);
        self.me = me;
        for u in d["users"].as_array().into_iter().flatten() {
            self.upsert_user(u);
        }
        let mut seen = Vec::new();
        for g in d["guilds"].as_array().into_iter().flatten() {
            seen.push(self.put_guild(g));
        }
        self.guilds.retain(|id, _| seen.contains(id));
        let guilds = &self.guilds;
        self.channels.retain(|_, c| c.guild == 0 || guilds.contains_key(&c.guild));

        if let Some(mm) = d["merged_members"].as_array() {
            for (i, list) in mm.iter().enumerate() {
                let Some(&gid) = seen.get(i) else { break };
                for m in list.as_array().into_iter().flatten() {
                    let uid = sid(&m["user_id"]);
                    self.upsert_member(gid, if uid == 0 { me } else { uid }, m);
                }
            }
        }

        self.dms.clear();
        for c in d["private_channels"].as_array().into_iter().flatten() {
            let mut ch = Channel::parse(c, 0);
            ch.recipients = if c["recipient_ids"].is_array() {
                ids(&c["recipient_ids"])
            } else {
                c["recipients"]
                    .as_array()
                    .map(|a| a.iter().map(|u| self.upsert_user(u)).collect())
                    .unwrap_or_default()
            };
            self.dms.push(ch.id);
            self.channels.insert(ch.id, ch);
        }
        self.sort_dms();

        let rs = if d["read_state"].is_array() { &d["read_state"] } else { &d["read_state"]["entries"] };
        self.reads.clear();
        self.event_reads.clear();
        for r in rs.as_array().into_iter().flatten() {
            let state = ReadState {
                last_acked: sid(&r["last_message_id"]),
                mentions: r["mention_count"].as_u64().unwrap_or(0) as u32,
                last_pin: iso_ms(&r["last_pin_timestamp"]),
                flags: r["flags"].as_u64().map(|f| f as u8),
            };
            match r["read_state_type"].as_u64().unwrap_or(0) {
                0 => self.reads.insert(sid(&r["id"]), state),
                1 => self.event_reads.insert(sid(&r["id"]), state),
                _ => None,
            };
        }

        let ugs = if d["user_guild_settings"].is_array() {
            &d["user_guild_settings"]
        } else {
            &d["user_guild_settings"]["entries"]
        };
        self.muted.clear();
        self.events_muted.clear();
        self.gprefs.clear();
        self.unreads.clear();
        self.levels.clear();
        self.opt_in.clear();
        self.opted.clear();
        self.collapsed.clear();
        self.new_notifs = d["notification_settings"]["flags"].as_u64().unwrap_or(0) & 16 != 0;
        for s in ugs.as_array().into_iter().flatten() {
            self.guild_settings(s);
        }

        self.email = ost(&d["user"]["email"]);
        self.me_raw = d["user"].clone();
        self.experiments = d["apex_experiments"]["assignments"].as_object().map(|kinds| {
            let units = kinds.values().filter_map(|k| k.as_object()).flat_map(|k| k.values());
            let given = units.flat_map(|u| u["assignments"].as_array().into_iter().flatten());
            let hash = |a: &Value| a[0].as_u64().or_else(|| a[0].as_str()?.parse().ok());
            given.filter(|a| a[1].as_u64().is_some_and(|variant| variant != 0)).filter_map(|a| hash(a).map(|h| h as u32)).collect()
        });
        self.relationships.clear();
        for r in d["relationships"].as_array().into_iter().flatten() {
            let uid = sid(&r["id"]);
            if r["user"].is_object() {
                self.upsert_user(&r["user"]);
            }
            self.relationships.insert(uid, r["type"].as_u64().unwrap_or(0) as u8);
            self.set_since(uid, &r["since"]);
            self.set_nick(uid, &r["nickname"]);
        }

        let mut folders: Vec<Folder> = Vec::new();
        self.account = Default::default();
        if let Some(b64) = d["user_settings_proto"].as_str() {
            use base64::Engine;
            if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64) {
                folders = crate::proto::guild_folders(&raw);
                self.load_privacy(&raw, true);
                self.account = crate::proto::account(&raw, true);
                self.favorites = crate::proto::favorites::read(&raw).unwrap_or_default();
                self.dismissed = crate::proto::dismissed_versions(&raw);
                (self.custom_status, self.custom_emoji) = Self::own_custom(crate::proto::custom_status(&raw, chrono::Utc::now().timestamp_millis() as u64).flatten());
                self.own_status(&raw);
            }
        }
        if folders.is_empty() {
            folders = d["user_settings"]["guild_folders"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| Folder {
                    id: sid(&f["id"]),
                    name: ost(&f["name"]),
                    color: f["color"].as_u64().map(|c| c as u32),
                    guilds: ids(&f["guild_ids"]),
                })
                .collect();
        }
        self.arrange(folders, &seen);
    }

    /// Lays the rail out from the account's folders; servers they leave out sit on top, like the
    /// official client.
    pub fn arrange(&mut self, mut folders: Vec<Folder>, seen: &[Id]) {
        let mut order = Vec::new();
        for f in &folders {
            order.extend(f.guilds.iter().copied());
        }
        for g in seen {
            if !order.contains(g) {
                order.insert(0, *g);
            }
        }
        order.retain(|g| self.guilds.contains_key(g));
        let mut lone: Vec<Folder> = order
            .iter()
            .filter(|g| !folders.iter().any(|f| f.guilds.contains(g)))
            .map(|g| Folder { guilds: vec![*g], ..Default::default() })
            .collect();
        for f in &mut folders {
            f.guilds.retain(|g| self.guilds.contains_key(g));
        }
        folders.retain(|f| !f.guilds.is_empty());
        lone.append(&mut folders);
        self.folders = lone;
        self.guild_order = order;
    }

    /// The privacy lists in a settings blob; a whole snapshot also says which lists are empty.
    pub fn load_privacy(&mut self, raw: &[u8], full: bool) {
        if full {
            self.privacy.clear();
        }
        let mut seen: Vec<u64> = Vec::new();
        for (n, ids) in crate::proto::privacy(raw) {
            let list = self.privacy.entry(n).or_default();
            if !seen.contains(&n) {
                list.clear();
                seen.push(n);
            }
            list.extend(ids);
        }
    }

    pub fn load_supplemental(&mut self, d: &Value) {
        for p in d["merged_presences"]["friends"].as_array().into_iter().flatten() {
            self.set_activities(sid(&p["user_id"]), &p["activities"]);
            self.presence.insert(sid(&p["user_id"]), Status::parse(p["status"].as_str().unwrap_or("")));
        }
        for list in d["merged_presences"]["guilds"].as_array().into_iter().flatten() {
            for p in list.as_array().into_iter().flatten() {
                self.set_activities(sid(&p["user_id"]), &p["activities"]);
                self.presence.insert(sid(&p["user_id"]), Status::parse(p["status"].as_str().unwrap_or("")));
            }
        }
        for g in d["guilds"].as_array().into_iter().flatten() {
            let gid = sid(&g["id"]);
            if let Some(guild) = self.guilds.get_mut(&gid) {
                for vs in g["voice_states"].as_array().into_iter().flatten() {
                    guild.voice.insert(sid(&vs["user_id"]), sid(&vs["channel_id"]));
                    if vs["self_stream"] == true {
                        guild.streaming.insert(sid(&vs["user_id"]));
                    }
                }
            }
        }
    }
}
