use super::*;

impl Store {
    pub(super) fn list_item(&mut self, gid: Id, v: &Value) -> ListItem {
        if v["group"].is_object() {
            return ListItem::Group {
                id: st(&v["group"]["id"]),
                count: v["group"]["count"].as_u64().unwrap_or(0) as u32,
            };
        }
        let m = &v["member"];
        let uid = self.upsert_user(&m["user"]);
        self.upsert_member(gid, uid, m);
        if let Some(s) = m["presence"]["status"].as_str() {
            self.presence.insert(uid, Status::parse(s));
            self.set_activities(uid, &m["presence"]["activities"]);
        }
        ListItem::Member(uid)
    }

    pub(super) fn member_list(&mut self, d: &Value) {
        let gid = sid(&d["guild_id"]);
        if !self.guilds.contains_key(&gid) {
            return;
        }
        let mut list = std::mem::take(&mut self.guilds.get_mut(&gid).unwrap().list);
        for op in d["ops"].as_array().into_iter().flatten() {
            match op["op"].as_str().unwrap_or("") {
                "SYNC" => {
                    let start = op["range"][0].as_u64().unwrap_or(0) as usize;
                    let items: Vec<ListItem> = op["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|i| self.list_item(gid, i))
                        .collect();
                    if list.len() < start {
                        list.resize(start, ListItem::Group { id: String::new(), count: 0 });
                    }
                    list.truncate(start);
                    list.extend(items);
                }
                "INSERT" => {
                    let i = op["index"].as_u64().unwrap_or(0) as usize;
                    let item = self.list_item(gid, &op["item"]);
                    list.insert(i.min(list.len()), item);
                }
                "UPDATE" => {
                    let i = op["index"].as_u64().unwrap_or(0) as usize;
                    let item = self.list_item(gid, &op["item"]);
                    if i < list.len() {
                        list[i] = item;
                    }
                }
                "DELETE" => {
                    let i = op["index"].as_u64().unwrap_or(0) as usize;
                    if i < list.len() {
                        list.remove(i);
                    }
                }
                _ => {}
            }
        }
        let counts: Map<String, u32> = d["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| (st(&g["id"]), g["count"].as_u64().unwrap_or(0) as u32))
            .collect();
        for it in list.iter_mut() {
            if let ListItem::Group { id, count } = it
                && let Some(c) = counts.get(id)
            {
                *count = *c;
            }
        }
        let g = self.guilds.get_mut(&gid).unwrap();
        if let Some(n) = d["member_count"].as_u64() {
            g.member_count = n as u32;
        }
        g.list = list;
    }

    /// Result of /channels/{forum}/threads/search: posts plus their opening messages.
    pub fn load_forum(&mut self, forum: Id, v: &Value) {
        let guild = self.channels.get(&forum).map(|c| c.guild).unwrap_or(0);
        let mine: Vec<Id> = v["members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| sid(&m["user_id"]) == self.me)
            .map(|m| sid(&m["id"]))
            .collect();
        let mut posts = Vec::new();
        for t in v["threads"].as_array().into_iter().flatten() {
            let mut c = Channel::parse(t, guild);
            c.joined |= mine.contains(&c.id);
            posts.push(c.id);
            self.put_channel(c);
        }
        for m in v["first_messages"].as_array().into_iter().flatten() {
            let m = self.parse_message(m);
            self.refs.insert(m.id, m);
        }
        let chans = &self.channels;
        posts.sort_by_key(|p| std::cmp::Reverse(chans.get(p).map(|c| c.last_msg.max(c.id)).unwrap_or(0)));
        self.forum_posts.insert(forum, posts);
    }

    pub fn set_reactors(&mut self, key: (Id, String, bool), v: &Value) {
        let users: Vec<Id> = v.as_array().into_iter().flatten().map(|u| self.upsert_user(u)).collect();
        if let Some(e) = self.reactors.get_mut(&key) {
            e.1 = Some(users);
        }
    }

    pub fn react_local(&mut self, channel: Id, id: Id, em: ReactEmoji, add: bool) {
        if let Some(m) = self.msg_mut(channel, id) {
            react(m, em, add, true);
        }
    }

    /// Sets the current user's poll votes to exactly `picks`, adjusting the shown counts.
    /// Returns false (and does nothing) when the message has no live poll.
    pub fn set_votes(&mut self, channel: Id, id: Id, picks: &[u8]) -> bool {
        let Some(p) = self.msg_mut(channel, id).and_then(|m| m.poll.as_mut()).filter(|p| !p.finalized) else {
            return false;
        };
        for a in p.answers.iter_mut() {
            let want = picks.contains(&a.id);
            if want && !a.me {
                a.count += 1;
            } else if !want && a.me {
                a.count = a.count.saturating_sub(1);
            }
            a.me = want;
        }
        true
    }

    pub fn prune_typing(&mut self) {
        self.typing.retain(|_, v| {
            v.retain(|(_, t)| t.elapsed().as_secs() < 10);
            !v.is_empty()
        });
    }
}
