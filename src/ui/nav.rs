use super::*;

impl App {
    pub fn cur(&self) -> Option<Id> {
        let ch = *self.ui.chan.get(&self.ui.guild.unwrap_or(0))?;
        self.store.channels.contains_key(&ch).then_some(ch)
    }

    pub fn select_guild(&mut self, g: Option<Id>) {
        if g.is_none() && self.ui.guild.is_some() {
            self.prev_guild = self.ui.guild;
        }
        self.ui.guild = g;
        self.boosts = None;
        self.server_shop = None;
        self.home = None;
        self.directory = None;
        let ch = match self.cur() {
            Some(c) => Some(c),
            None => match g {
                Some(g) => side::first_channel(&self.store, g),
                None => self.store.dms.first().copied(),
            },
        };
        if let Some(c) = ch {
            self.open(c);
        }
    }

    pub fn open(&mut self, ch: Id) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        self.boosts = None;
        self.server_shop = None;
        self.home = None;
        let guild = c.guild;
        let forum = c.is_forum();
        let is_thread = c.is_thread();
        // member list subscriptions are per parent channel, threads borrow it
        let sub = if c.is_thread() { c.parent } else { ch };
        // search results belong to one server (or one dm), don't carry them elsewhere
        if self.search.as_ref().is_some_and(|s| s.scope != if guild != 0 { guild } else { ch }) {
            self.search = None;
        }
        if self.cur() != Some(ch) {
            self.reply = None;
            self.editing = None;
            self.files.clear();
            self.pins = None;
            self.post = None;
            self.hold_ack = None;
        }
        self.friends = false;
        self.quests = None;
        self.shop = None;
        if is_thread {
            self.side_thread = None;
        }
        self.nav.visit(titlebar::Loc::Chan(ch, None));
        self.ui.chan.insert(guild, ch);
        crate::net::OPEN.store(ch, std::sync::atomic::Ordering::Relaxed);
        self.ui.recent.retain(|x| *x != ch);
        self.ui.recent.insert(0, ch);
        self.ui.recent.truncate(15);
        self.focus_composer = true;
        let acked = self.store.reads.get(&ch).map(|r| r.last_acked).unwrap_or(0);
        self.marker = (c.last_msg > acked && acked != 0).then_some((ch, acked));
        self.marker_bar = self.marker.is_some();
        if forum {
            let path = format!("/channels/{ch}/threads/search?sort_by=last_message_time&sort_order=desc&limit=25&offset=0");
            self.rest("GET", path, None, Tag::Forum(ch));
        } else {
            let jumping = self.scroll_to.is_some_and(|(c, _)| c == ch);
            let e = self.store.msgs.entry(ch).or_default();
            if (!e.fresh || e.failed.is_some()) && !e.loading && !jumping {
                e.failed = None;
                e.loading = true;
                self.net.send(Cmd::Messages { channel: ch, page: Page::Latest });
            }
        }
        if guild != 0 {
            let mut chans = serde_json::Map::new();
            chans.insert(sub.to_string(), json!([[0, 99]]));
            self.net.send(Cmd::Gateway(json!({
                "op": 37,
                "d": { "subscriptions": { guild.to_string(): {
                    "typing": true, "activities": true, "threads": true, "channels": chans
                }}}
            })));
            if is_thread {
                self.subscribe_thread(guild, ch);
            }
            if self.info_asked.insert(guild) {
                self.net.send(Cmd::Gateway(json!({ "op": 43, "d": { "guild_id": guild.to_string(), "fields": ["status", "voice_start_time"] } })));
            }
        }
    }

    // history from REST has no member objects; ask the gateway for the authors it hasn't sent this
    // connection (the cache's roles and nicknames may be old), and whoever they mention, bots'
    // embeds included, so names show nicknames and role colours like discord's
    pub(super) fn request_members(&mut self, ch: Id) {
        let Some(guild) = self.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0) else { return };
        let asked = &mut self.members_asked;
        let mut ids: Vec<String> = Vec::new();
        let mut want = |id: Id| {
            if id != 0 && ids.len() < 100 && asked.insert((guild, id)) {
                ids.push(id.to_string());
            }
        };
        for m in self.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default() {
            want(m.author);
            m.mentions.iter().for_each(|u| want(*u));
            let texts = m.embeds.iter().flat_map(|e| e.desc.iter().chain(e.fields.iter().map(|f| &f.1)));
            for t in std::iter::once(&m.content).chain(texts) {
                for part in t.split("<@").skip(1) {
                    if let Some(id) = part.split('>').next().map(|x| x.trim_start_matches('!')).and_then(|x| x.parse().ok()) {
                        want(id);
                    }
                }
            }
        }
        if ids.is_empty() {
            return;
        }
        self.net.send(Cmd::Gateway(json!({
            "op": 8,
            "d": { "guild_id": [guild.to_string()], "user_ids": ids, "presences": false }
        })));
    }

    /// The size a panel's edge was dragged to, if it was.
    pub fn dragged(&mut self, ctx: &egui::Context, panel: &str, now: f32, was: f32) -> Option<f32> {
        if (now - was).abs() < 0.5 || !edge_held(ctx, panel) {
            return None;
        }
        self.save_at = Some(Instant::now() + Duration::from_secs(1));
        Some(now)
    }

    /// Open a channel scrolled to one message, fetching the history around it if needed.
    pub fn jump_to(&mut self, ch: Id, id: Id) {
        self.scroll_to = Some((ch, id));
        self.scroll_tries = 0;
        let moved = self.cur() != Some(ch);
        if moved {
            let g = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
            self.ui.guild = (g != 0).then_some(g);
            self.open(ch);
        }
        self.nav.jumped(ch, id, moved);
        let e = self.store.msgs.entry(ch).or_default();
        if !e.list.iter().any(|m| m.id == id) {
            e.loading = true;
            self.net.send(Cmd::Messages { channel: ch, page: Page::Around(id) });
        }
    }

    pub fn retry_messages(&mut self, ch: Id) {
        let Some(e) = self.store.msgs.get_mut(&ch) else { return };
        let page = e.failed.take().map_or(Page::Latest, |f| f.0);
        e.loading = true;
        self.net.send(Cmd::Messages { channel: ch, page });
    }

    pub fn jump_to_present(&mut self, ch: Id) {
        let e = self.store.msgs.entry(ch).or_default();
        if e.detached {
            e.loading = true;
            self.net.send(Cmd::Messages { channel: ch, page: Page::Latest });
        }
        self.scroll_fix = Some((ch, f32::MAX));
    }

    pub fn send_message(&mut self, ch: Id) {
        self.post(ch, false);
    }

    /// The draft and uploads out as a message; `tts` reads it aloud to everyone looking.
    pub fn post(&mut self, ch: Id, tts: bool) {
        let text = self.drafts.remove(&ch).unwrap_or_default();
        let mut text = self.resolve_aliases(ch, text.trim());
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        text = self.convert_emoji(guild, &text);
        if self.settings.emoticons {
            text = emoticons(&text);
        }
        self.aliases.remove(&ch);
        let files: Vec<(PathBuf, String, String)> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|u| {
                let name = if u.spoiler { format!("SPOILER_{}", u.name) } else { u.name };
                (u.path, name, u.alt)
            })
            .collect();
        if text.is_empty() && files.is_empty() {
            return;
        }
        let nonce = crate::state::make_nonce();
        let reply = self.reply.take();
        self.store.add_pending(ch, text.clone(), nonce.clone(), reply);
        self.jump_to_present(ch);
        let reply = reply.map(|r| (r, self.reply_ping));
        self.remember_send(&nonce, actions::Unsent { content: text.clone(), reply, files: files.clone(), tts });
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply, files, stickers: Vec::new(), tts });
        self.hold_ack = None;
        self.last_typing = None;
    }

    /// Text that didn't come from the composer, so its draft, uploads and reply stay put.
    pub fn send_text(&mut self, ch: Id, text: String) {
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        let mut text = self.convert_emoji(guild, text.trim());
        if self.settings.emoticons {
            text = emoticons(&text);
        }
        let nonce = crate::state::make_nonce();
        self.store.add_pending(ch, text.clone(), nonce.clone(), None);
        self.jump_to_present(ch);
        self.remember_send(&nonce, actions::Unsent { content: text.clone(), reply: None, files: Vec::new(), tts: false });
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply: None, files: Vec::new(), stickers: Vec::new(), tts: false });
    }

    /// The four one-click reactions on top of the message menu, the toolbar taking the first three:
    /// the ones picked by hand, else the latest used, topped up with discord's defaults.
    pub fn quick_reactions(&self) -> Vec<ReactEmoji> {
        // discord's four are the account's most used; what was used here leads until those arrive
        let mut list = match (self.settings.quick_reactions.is_empty(), self.ui.frequent.is_empty()) {
            (false, _) => self.settings.quick_reactions.clone(),
            (true, false) => self.ui.frequent.iter().take(4).cloned().collect(),
            (true, true) => self.ui.quick.clone(),
        };
        for n in ["✅", "❤️", "🔥", "😂"] {
            if list.len() < 4 && !list.iter().any(|q| q.id == 0 && q.name == n) {
                list.push(ReactEmoji { id: 0, name: n.to_string(), animated: false });
            }
        }
        list.truncate(4);
        list
    }

    /// Puts an emoji in one of the quick reaction slots; one already there trades places.
    pub fn set_quick(&mut self, slot: usize, em: ReactEmoji, ctx: &egui::Context) {
        let mut list = self.quick_reactions();
        if slot >= list.len() {
            return;
        }
        match list.iter().position(|q| *q == em) {
            Some(i) => list.swap(slot, i),
            None => list[slot] = em,
        }
        self.settings.quick_reactions = list;
        self.save_settings(ctx);
    }

    /// The next zoom level up or down from where it is now.
    pub(super) fn step_zoom(&mut self, ctx: &egui::Context, up: bool) {
        let now = ctx.zoom_factor() * 100.0;
        let next = if up { ZOOMS.iter().find(|z| **z > now + 0.5) } else { ZOOMS.iter().rev().find(|z| **z < now - 0.5) };
        if let Some(z) = next {
            ctx.set_zoom_factor(z / 100.0);
            self.toast(format!("Zoom {z}%"));
        }
    }

    pub fn react(&mut self, ch: Id, msg: Id, em: ReactEmoji, add: bool) {
        if add {
            self.ui.quick.retain(|q| *q != em);
            self.ui.quick.insert(0, em.clone());
            self.ui.quick.truncate(3);
            self.used(em.clone());
        }
        let e = crate::api::esc(&em.api_name());
        let path = if add {
            format!("/channels/{ch}/messages/{msg}/reactions/{e}/@me?location=Message&type=0")
        } else {
            format!("/channels/{ch}/messages/{msg}/reactions/{e}/0/@me?location=Message&burst=false")
        };
        let back = em.clone();
        self.rest(if add { "PUT" } else { "DELETE" }, path, None, Tag::undo("Reaction failed to send", move |app: &mut App| {
            app.store.react_local(ch, msg, back.clone(), !add);
        }));
        self.store.react_local(ch, msg, em, add);
    }

    /// An emoji went out, as a reaction or in a message: it moves up the picker's frequently used.
    pub fn used(&mut self, em: ReactEmoji) {
        favs::used_emoji(self, &em);
    }

    pub fn rest(&self, method: &'static str, path: String, body: Option<serde_json::Value>, tag: Tag) {
        self.net.send(Cmd::Rest { method, path, body, tag });
    }

    pub fn rest_in(&self, method: &'static str, path: String, body: Option<serde_json::Value>, location: &str, tag: Tag) {
        self.net.send(Cmd::Located { method, path, body, context: json!({ "location": location }), tag });
    }

    /// Notification settings the way discord saves them: servers go in the bulk route keyed by
    /// id, direct messages on their own.
    /// Fold categories shut or open, here and on the account.
    pub fn set_collapsed(&mut self, cats: &[Id], shut: bool) {
        let mut by_guild: std::collections::BTreeMap<Id, serde_json::Map<String, serde_json::Value>> = Default::default();
        for c in cats {
            let changed = if shut { !self.store.collapsed.contains(c) } else { self.store.collapsed.contains(c) };
            if !changed {
                continue;
            }
            if shut {
                self.store.collapsed.insert(*c);
            } else {
                self.store.collapsed.remove(c);
            }
            let gid = self.store.channels.get(c).map_or(0, |c| c.guild);
            by_guild.entry(gid).or_default().insert(c.to_string(), json!({ "collapsed": shut }));
        }
        for (gid, overrides) in by_guild {
            self.guild_settings(gid, json!({ "channel_overrides": overrides }), Tag::Quiet);
        }
    }

    pub fn guild_settings(&self, gid: Id, body: serde_json::Value, tag: Tag) {
        if gid == 0 {
            self.rest("PATCH", "/users/@me/guilds/@me/settings".into(), Some(body), tag);
        } else {
            self.rest("PATCH", "/users/@me/guilds/settings".into(), Some(json!({ "guilds": { gid.to_string(): body } })), tag);
        }
    }

    pub fn audit(&self, method: &'static str, path: String, body: Option<serde_json::Value>, reason: &str, tag: Tag) {
        self.net.send(Cmd::Audit { method, path, body, reason: reason.trim().to_string(), tag });
    }

    pub fn proto_patch(&self, bytes: Vec<u8>, tag: Tag) {
        use base64::Engine;
        self.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(json!({ "settings": base64::engine::general_purpose::STANDARD.encode(bytes) })), tag);
    }

    pub fn mark_read(&mut self, chans: &[Id]) {
        let mut states = Vec::new();
        for ch in chans {
            let Some(last) = self.store.channels.get(ch).map(|c| c.last_msg).filter(|l| *l != 0) else { continue };
            let r = self.store.reads.entry(*ch).or_default();
            if r.last_acked < last || r.mentions > 0 {
                r.last_acked = last;
                r.mentions = 0;
                states.push(json!({ "channel_id": ch.to_string(), "message_id": last.to_string(), "read_state_type": 0 }));
            }
        }
        if !states.is_empty() {
            self.rest("POST", "/read-states/ack-bulk".into(), Some(json!({ "read_states": states })), Tag::Quiet);
        }
    }

    pub fn set_muted(&mut self, id: Id, guild: bool, muted: bool) {
        self.mute_for(id, guild, muted, None);
    }

    /// Muting with discord's mute_config: a window in seconds, or None for until turned back on.
    pub fn mute_for(&mut self, id: Id, guild: bool, muted: bool, secs: Option<i64>) {
        let cfg = match secs {
            Some(s) if muted => json!({ "selected_time_window": s, "end_time": (chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }),
            _ => json!({ "selected_time_window": -1, "end_time": null }),
        };
        let was = self.store.muted.contains(&id);
        let back = move |app: &mut App| {
            if was {
                app.store.muted.insert(id);
            } else {
                app.store.muted.remove(&id);
            }
        };
        if muted {
            self.store.muted.insert(id);
        } else {
            self.store.muted.remove(&id);
        }
        if guild {
            self.guild_settings(id, if muted { json!({ "muted": true, "mute_config": cfg }) } else { json!({ "muted": false }) }, Tag::undo(SAVE_FAILED, back));
        } else {
            let g = self.store.channels.get(&id).map(|c| c.guild).unwrap_or(0);
            let over = if muted { json!({ "muted": true, "mute_config": cfg }) } else { json!({ "muted": false }) };
            self.guild_settings(g, json!({ "channel_overrides": { id.to_string(): over } }), Tag::undo(SAVE_FAILED, back));
        }
    }

    /// One bit of a channel override's flags, the rest kept: 4096 on the list of a server that
    /// only lists picked channels, 2048 pinned to the top, 1024 and 512 the unread setting.
    pub fn chan_flag(&mut self, ch: Id, bit: u64, on: bool) {
        let Some(g) = self.store.channels.get(&ch).map(|c| c.guild) else { return };
        let (was_flags, was_opted) = (self.store.chan_flags.get(&ch).copied(), self.store.opted.contains(&ch));
        let flags = self.store.chan_flags.entry(ch).or_default();
        if on {
            *flags |= bit;
        } else {
            *flags &= !bit;
        }
        let flags = *flags;
        if bit == OPTED {
            if on {
                self.store.opted.insert(ch);
            } else {
                self.store.opted.remove(&ch);
            }
        }
        let mut body = json!({ "channel_overrides": { ch.to_string(): { "flags": flags } } });
        let joined_picks = bit == OPTED && on && self.store.opt_in.insert(g);
        if joined_picks {
            body["flags"] = json!(self.guild_flags(g));
        }
        self.guild_settings(g, body, Tag::undo(SAVE_FAILED, move |app: &mut App| {
            match was_flags {
                Some(f) => app.store.chan_flags.insert(ch, f),
                None => app.store.chan_flags.remove(&ch),
            };
            if was_opted {
                app.store.opted.insert(ch);
            } else {
                app.store.opted.remove(&ch);
            }
            if joined_picks {
                app.store.opt_in.remove(&g);
            }
        }));
    }

    /// A server's own flags: the unread setting's bits kept, and 1 << 14 while it lists only
    /// the channels picked.
    pub fn guild_flags(&self, gid: Id) -> u64 {
        let unreads = match self.store.unreads.get(&gid) {
            Some(true) => 2048,
            Some(false) => 4096,
            None => 0,
        };
        unreads | if self.store.opt_in.contains(&gid) { 1u64 << 14 } else { 0 }
    }

    /// Switches a server to its picked channels, starting from these.
    pub fn opt_in_many(&mut self, gid: Id, ids: &[Id]) {
        let mut overrides = serde_json::Map::new();
        for id in ids {
            let flags = self.store.chan_flags.entry(*id).or_default();
            *flags |= OPTED;
            overrides.insert(id.to_string(), json!({ "flags": *flags }));
            self.store.opted.insert(*id);
        }
        self.store.opt_in.insert(gid);
        let body = json!({ "flags": self.guild_flags(gid), "channel_overrides": overrides });
        self.guild_settings(gid, body, Tag::Quiet);
    }

    /// Edit Per-server Profile: Settings > Profiles on that server.
    pub fn edit_server_profile(&mut self, gid: Id) {
        self.sform.profile_guild = Some(gid);
        self.settings_page = Some(settings::Page::Profiles);
    }

    pub fn opt_in(&mut self, ch: Id) {
        self.chan_flag(ch, OPTED, true);
    }

    /// A channel's notification level, as Discord's bell menu sets it; None goes back to the
    /// category or server default.
    pub fn set_level(&mut self, ch: Id, level: Option<u8>) {
        let was = self.store.levels.get(&ch).copied();
        let back = move |app: &mut App| {
            match was {
                Some(n) => app.store.levels.insert(ch, n),
                None => app.store.levels.remove(&ch),
            };
        };
        if self.store.guilds.contains_key(&ch) {
            if let Some(n) = level {
                self.store.levels.insert(ch, n);
            }
            self.guild_settings(ch, json!({ "message_notifications": level.unwrap_or(3) }), Tag::undo(SAVE_FAILED, back));
            return;
        }
        match level {
            Some(n) => self.store.levels.insert(ch, n),
            None => self.store.levels.remove(&ch),
        };
        let g = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
        let body = json!({ "channel_overrides": { ch.to_string(): { "message_notifications": level.unwrap_or(3) } } });
        self.guild_settings(g, body, Tag::undo(SAVE_FAILED, back));
    }

    pub fn mark_unread(&mut self, ch: Id, id: Id) {
        let (prev, mentions) = {
            let list = self.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default();
            let prev = list.iter().rev().find(|m| m.id < id).map(|m| m.id).unwrap_or(((id >> 22) - 1) << 22);
            let mentions = list.iter().filter(|m| m.id > prev && m.author != self.store.me && self.store.mentions_me(m)).count();
            (prev, mentions)
        };
        self.store.reads.entry(ch).or_default().last_acked = prev;
        self.rest("POST", format!("/channels/{ch}/messages/{prev}/ack"), Some(json!({ "manual": true, "mention_count": mentions })), Tag::Quiet);
        self.marker = Some((ch, prev));
        self.marker_bar = true;
        self.hold_ack = Some(ch);
    }

    pub fn set_status(&mut self, s: &str) {
        let me = self.store.me;
        self.store.presence.insert(me, Status::parse(s));
        self.send_presence();
    }

    /// Our status, with the game we detected unless activity sharing is off.
    pub(super) fn send_presence(&mut self) {
        let status = match self.store.presence.get(&self.store.me) {
            Some(Status::Idle) => "idle",
            Some(Status::Dnd) => "dnd",
            Some(Status::Offline) => "invisible",
            _ => "online",
        };
        self.shared = self.presence_key();
        let activities = if self.store.hide_game { json!([]) } else { json!(self.activities()) };
        self.net.send(Cmd::Gateway(json!({ "op": 3, "d": { "status": status, "since": 0, "activities": activities, "afk": false } })));
    }
}

fn edge_held(ctx: &egui::Context, panel: &str) -> bool {
    ctx.read_response(egui::Id::new(panel).with("__resize")).is_some_and(|r| r.dragged() || r.drag_stopped())
}

/// Gives a panel its saved size back: egui remembers the one a small window last squeezed it to.
pub fn keep_size(ctx: &egui::Context, panel: &str, axis: usize, size: f32) {
    use egui::containers::panel::PanelState;
    let id = egui::Id::new(panel);
    if edge_held(ctx, panel) {
        return;
    }
    let Some(mut st) = PanelState::load(ctx, id) else { return };
    let mut now = st.outer_rect.size();
    if (now[axis] - size).abs() >= 0.5 {
        now[axis] = size;
        st.outer_rect = egui::Rect::from_min_size(st.outer_rect.min, now);
        ctx.data_mut(|d| d.insert_persisted(id, st));
    }
}

#[cfg(test)]
mod tests {
    fn members_w(ctx: &egui::Context, screen: f32, keep: bool) -> f32 {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(screen, 600.0))), ..Default::default() };
        let mut w = 0.0;
        let mut out = ctx.run_ui(raw, |ui| {
            if keep {
                super::keep_size(ui.ctx(), "members", 0, 300.0);
            }
            let room = ui.available_width();
            let r = egui::Panel::right("members").default_size(300.0).size_range(200.0..=(room - 420.0).clamp(200.0, 480.0)).resizable(true).show(ui, |ui| ui.allocate_space(ui.available_size()));
            w = r.response.rect.width();
        });
        out.textures_delta.clear();
        w
    }

    #[test]
    fn a_panel_squeezed_by_a_narrow_window_gets_its_width_back() {
        for keep in [false, true] {
            let ctx = egui::Context::default();
            assert_eq!(members_w(&ctx, 1400.0, keep).round(), 300.0);
            assert_eq!(members_w(&ctx, 640.0, keep).round(), 220.0);
            assert_eq!(members_w(&ctx, 1400.0, keep).round(), if keep { 300.0 } else { 220.0 });
        }
    }
}
