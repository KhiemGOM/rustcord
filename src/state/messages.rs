use super::*;

impl Store {
    pub(super) fn sort_dms(&mut self) {
        let chans = &self.channels;
        self.dms
            .sort_by_key(|c| std::cmp::Reverse(chans.get(c).map(|c| c.last_msg.max(c.id)).unwrap_or(0)));
    }

    pub fn parse_message(&mut self, v: &Value) -> Message {
        let channel = sid(&v["channel_id"]);
        let guild = self.channels.get(&channel).map(|c| c.guild).unwrap_or(0);
        let author = self.upsert_user(&v["author"]);
        self.upsert_member(guild, author, &v["member"]);
        for u in v["mentions"].as_array().into_iter().flatten() {
            let uid = self.upsert_user(u);
            self.upsert_member(guild, uid, &u["member"]);
        }
        let mut m = Message {
            id: sid(&v["id"]),
            channel,
            author,
            kind: v["type"].as_u64().unwrap_or(0) as u8,
            nonce: v["nonce"].as_str().map(str::to_string).or_else(|| v["nonce"].as_u64().map(|n| n.to_string())),
            ..Default::default()
        };
        m.fill(v);
        let snap = &v["message_snapshots"][0]["message"];
        if m.flags & (1 << 14) != 0 && snap.is_object() {
            for u in snap["mentions"].as_array().into_iter().flatten() {
                self.upsert_user(u);
            }
            let flags = m.flags;
            m.fill(snap);
            m.flags = flags;
            m.forward = Some(snap["timestamp"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(m.ts(), |t| t.timestamp_millis()));
        }
        m.app_id = if v["application_id"].is_null() { author } else { sid(&v["application_id"]) };
        if v["thread"].is_object() {
            self.put_channel(Channel::parse(&v["thread"], guild));
        }
        if let Some(r) = v["message_reference"]["message_id"].as_str().and_then(|r| r.parse().ok()) {
            m.refer = Some((sid(&v["message_reference"]["channel_id"]), r));
        } else if v["message_reference"]["channel_id"].is_string() {
            m.refer = Some((sid(&v["message_reference"]["channel_id"]), 0));
        }
        let im = if v["interaction_metadata"].is_object() { &v["interaction_metadata"] } else { &v["interaction"] };
        if im["user"].is_object() {
            let user = self.upsert_user(&im["user"]);
            let owners = &im["authorizing_integration_owners"];
            let (owner, in_guild) = match (owners["0"].as_str(), owners["1"].as_str()) {
                (Some(g), _) => (sid(&owners["0"]), g != "0"),
                (_, Some(_)) => (sid(&owners["1"]), false),
                _ => (0, false),
            };
            m.interaction = Some(Interaction { user, owner, in_guild, name: st(&im["name"]) });
        }
        if matches!(m.kind, 25 | 44) {
            m.extra = if v["role_subscription_data"].is_object() { v["role_subscription_data"].clone() } else { v["purchase_notification"].clone() };
        } else if matches!(m.kind, 32..=34 | 49) {
            m.extra = serde_json::json!({ "application": v["application"]["name"] });
        }
        if v["referenced_message"].is_object() {
            let r = self.parse_message(&v["referenced_message"]);
            m.reply = Some(r.id);
            self.refs.insert(r.id, r);
        } else if let Some(r) = v["message_reference"]["message_id"].as_str() {
            if m.kind == 19 {
                m.reply = r.parse().ok();
            }
        }
        m
    }

    pub fn set_messages(&mut self, channel: Id, page: Page, raw: &[Value]) -> usize {
        let mut fresh: Vec<Message> = raw.iter().map(|v| self.parse_message(v)).collect();
        fresh.sort_by_key(|m| m.id);
        let n = fresh.len();
        let newest = self.channels.get(&channel).map(|c| c.last_msg).unwrap_or(0);
        let e = self.msgs.entry(channel).or_default();
        e.loading = false;
        match page {
            Page::Latest => {
                // pending sends survive a refetch
                let pend: Vec<Message> = e.list.drain(..).filter(|m| m.pending || m.failed).collect();
                e.list = fresh;
                e.list.extend(pend);
                e.fresh = true;
                e.detached = false;
                e.at_start = n < 50;
            }
            Page::Before(_) => {
                let mut merged = fresh;
                merged.retain(|m| !e.list.iter().any(|x| x.id == m.id));
                merged.append(&mut e.list);
                e.list = merged;
                e.at_start |= n < 50;
            }
            Page::After(_) => {
                fresh.retain(|m| !e.list.iter().any(|x| x.id == m.id));
                e.list.extend(fresh);
                e.detached = n >= 50;
            }
            Page::Around(_) => {
                e.detached = !fresh.iter().any(|m| m.id >= newest);
                e.list = fresh;
                e.fresh = true;
                e.at_start = false;
            }
        }
        n
    }

    pub fn add_pending(&mut self, channel: Id, content: String, nonce: String, reply: Option<Id>) {
        let m = Message {
            id: nonce.parse().unwrap_or(0),
            channel,
            author: self.me,
            content,
            reply,
            kind: if reply.is_some() { 19 } else { 0 },
            nonce: Some(nonce),
            pending: true,
            ..Default::default()
        };
        self.msgs.entry(channel).or_default().list.push(m);
    }

    pub fn drop_pending(&mut self, channel: Id, nonce: &str) {
        if let Some(e) = self.msgs.get_mut(&channel) {
            e.list.retain(|m| m.nonce.as_deref() != Some(nonce));
        }
    }

    pub fn fail_pending(&mut self, channel: Id, nonce: &str) {
        if let Some(e) = self.msgs.get_mut(&channel) {
            for m in e.list.iter_mut().filter(|m| m.nonce.as_deref() == Some(nonce)) {
                m.pending = false;
                m.failed = true;
            }
        }
    }

    pub fn find_msg(&self, channel: Id, id: Id) -> Option<&Message> {
        self.msgs
            .get(&channel)
            .and_then(|e| e.list.iter().find(|m| m.id == id))
            .or_else(|| self.refs.get(&id))
    }

    pub(super) fn msg_mut(&mut self, channel: Id, id: Id) -> Option<&mut Message> {
        self.msgs.get_mut(&channel)?.list.iter_mut().find(|m| m.id == id)
    }

    /// Something was pinned since you last opened the pins, which is when discord dots the pin button.
    pub fn pins_unread(&self, ch: Id) -> bool {
        let seen = self.reads.get(&ch).map_or(0, |r| r.last_pin);
        self.channels.get(&ch).is_some_and(|c| c.last_pin > seen)
    }

    /// The content inventory's inbox: what friends and people you share servers with played or
    /// listened to. Cards discord can't draw (listening sessions, expired live games) are left out.
    pub fn set_feed(&mut self, v: &Value) {
        let now = chrono::Utc::now().timestamp_millis();
        self.feed = v["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let c = &e["content"];
                let x = &c["extra"];
                let trait_of = |t: u64| c["traits"].as_array().into_iter().flatten().find(|r| r["type"].as_u64() == Some(t));
                let kind = c["content_type"].as_u64()? as u8;
                let title = match kind {
                    1 | 3 => st(&x["game_name"]),
                    6 => st(&x["artist"]["name"]),
                    8 => st(&x["activity_name"]),
                    _ => return None,
                };
                let entry = FeedEntry {
                    id: sid(&c["id"]),
                    author: sid(&c["author_id"]),
                    kind,
                    title,
                    image: ost(&x["media"]["image_url"]),
                    app: sid(&x["application_id"]),
                    live: trait_of(3).and_then(|t| t["is_live"].as_bool()).unwrap_or(false),
                    expires: iso_ms(&c["expires_at"]),
                    count: trait_of(12).and_then(|t| t["count"].as_u64()).unwrap_or(0),
                    secs: trait_of(2).and_then(|t| t["duration_seconds"].as_u64()).unwrap_or(0),
                    streak: trait_of(8).and_then(|t| t["streak_count_days"].as_u64()).unwrap_or(0),
                    trending: trait_of(9).and_then(|t| t["trending"].as_u64()).unwrap_or(0) != 0,
                    marathon: trait_of(6).and_then(|t| t["marathon"].as_bool()).unwrap_or(false),
                    returning: trait_of(5).is_some_and(|t| t["resurrected_last_played"].is_string()),
                    first: trait_of(1).and_then(|t| t["first_time"].as_bool()).unwrap_or(false),
                };
                (entry.expires == 0 || entry.expires > now).then_some(entry)
            })
            .collect();
    }

    pub fn mentions_me(&self, m: &Message) -> bool {
        if m.mentions.contains(&self.me) {
            return true;
        }
        let guild = self.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
        let prefs = self.gprefs.get(&guild);
        if m.mention_everyone && !prefs.is_some_and(|p| p.suppress_everyone) {
            return true;
        }
        !prefs.is_some_and(|p| p.suppress_roles) && self.guilds
            .get(&guild)
            .is_some_and(|g| m.mention_roles.iter().any(|r| g.my_roles.contains(r)))
    }
}
