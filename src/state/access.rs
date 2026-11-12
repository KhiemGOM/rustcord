use super::*;

impl Store {
    pub fn user(&self, id: Id) -> Option<&User> {
        self.users.get(&id)
    }

    pub(super) fn set_since(&mut self, uid: Id, v: &Value) {
        match v.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()) {
            Some(t) => {
                self.rel_since.insert(uid, t.timestamp_millis());
            }
            None => {
                self.rel_since.remove(&uid);
            }
        }
    }

    pub fn name_in(&self, guild: Id, user: Id) -> String {
        if guild == 0 && let Some(n) = self.nicks.get(&user) {
            return n.clone();
        }
        if let Some(n) = self.members.get(&guild).and_then(|m| m.get(&user)).and_then(|m| m.nick.clone()) {
            return n;
        }
        self.users
            .get(&user)
            .map(|u| u.display().to_string())
            .unwrap_or_else(|| "Unknown User".into())
    }

    pub fn role_color(&self, guild: Id, user: Id) -> Option<u32> {
        self.role_look(guild, user).map(|r| r.color)
    }

    /// The role that colours a member's name: highest of their roles that has a colour.
    pub fn role_look(&self, guild: Id, user: Id) -> Option<&Role> {
        let g = self.guilds.get(&guild)?;
        let m = self.members.get(&guild)?.get(&user)?;
        m.roles.iter().filter_map(|r| g.roles.get(r)).filter(|r| r.color != 0).max_by_key(|r| r.position)
    }

    /// The role whose icon or emoji shows next to a member's name.
    pub fn role_icon(&self, guild: Id, user: Id) -> Option<&Role> {
        let g = self.guilds.get(&guild)?;
        let m = self.members.get(&guild)?.get(&user)?;
        m.roles.iter().filter_map(|r| g.roles.get(r)).filter(|r| r.icon.is_some() || r.emoji.is_some()).max_by_key(|r| r.position)
    }

    pub fn status(&self, user: Id) -> Status {
        self.presence.get(&user).copied().unwrap_or_default()
    }

    pub fn nitro(&self) -> bool {
        self.me_raw["premium_type"].as_u64().unwrap_or(0) != 0
    }

    /// Every custom emoji we know, with the server it lives in and whether it can be sent from
    /// `here`. The current server comes first, the rest follow in rail order.
    pub fn emojis_for(&self, here: Id) -> Vec<(&Guild, &Emoji, bool)> {
        let nitro = self.nitro();
        let order = std::iter::once(here).chain(self.guild_order.iter().copied().filter(|g| *g != here));
        let mut out = Vec::new();
        for g in order.filter_map(|g| self.guilds.get(&g)) {
            for e in &g.emojis {
                let ok = !e.unavailable
                    && (e.roles.is_empty() || e.roles.iter().any(|r| g.my_roles.contains(r)))
                    && (nitro || (g.id == here && !e.animated));
                out.push((g, e, ok));
            }
        }
        out
    }

    /// Discord offers Invite to whoever may create one, and to everyone where the server has a
    /// link of its own.
    pub fn can_invite(&self, c: &Channel) -> bool {
        self.perms(c) & CREATE_INVITE != 0 || self.guilds.get(&c.guild).is_some_and(|g| g.vanity.is_some())
    }

    pub fn can_view(&self, c: &Channel) -> bool {
        self.perms(c) & VIEW_CHANNEL != 0
    }

    /// Whether the channel list shows it: in servers with onboarding's opt-in, only channels picked
    /// there (or whose category or parent was) make the list.
    pub fn listed(&self, c: &Channel) -> bool {
        if !self.opt_in.contains(&c.guild) {
            return true;
        }
        let mut id = c.id;
        for _ in 0..3 {
            if self.opted.contains(&id) {
                return true;
            }
            match self.channels.get(&id) {
                Some(c) if c.parent != 0 => id = c.parent,
                _ => break,
            }
        }
        false
    }

    /// Our effective permission bits in a channel. DMs and owners get everything.
    /// Server-wide permissions, before any channel overwrites.
    pub fn my_perms(&self, gid: Id) -> u64 {
        let Some(g) = self.guilds.get(&gid) else { return 0 };
        let (owner, roles) = self.roles_in(g);
        if owner {
            return u64::MAX;
        }
        let mut p = g.roles.get(&g.id).map(|r| r.perms).unwrap_or(0);
        for r in roles {
            if let Some(r) = g.roles.get(r) {
                p |= r.perms;
            }
        }
        if p & ADMINISTRATOR != 0 { u64::MAX } else { p }
    }

    pub fn perms(&self, c: &Channel) -> u64 {
        if c.is_thread()
            && let Some(p) = self.channels.get(&c.parent)
        {
            return self.perms(p);
        }
        let Some(g) = self.guilds.get(&c.guild) else {
            return u64::MAX;
        };
        let (owner, roles) = self.roles_in(g);
        if owner {
            return u64::MAX;
        }
        let mut p = g.roles.get(&g.id).map(|r| r.perms).unwrap_or(0);
        for r in roles {
            if let Some(r) = g.roles.get(r) {
                p |= r.perms;
            }
        }
        if p & ADMINISTRATOR != 0 {
            return u64::MAX;
        }
        let ow = |id: Id| c.overwrites.iter().find(|o| o.id == id);
        if let Some(o) = ow(g.id) {
            p = (p & !o.deny) | o.allow;
        }
        let (mut allow, mut deny) = (0, 0);
        for r in roles {
            if let Some(o) = c.overwrites.iter().find(|o| o.id == *r && o.kind == 0) {
                allow |= o.allow;
                deny |= o.deny;
            }
        }
        p = (p & !deny) | allow;
        if self.viewing.as_ref().is_none_or(|v| v.0 != g.id)
            && let Some(o) = c.overwrites.iter().find(|o| o.id == self.me && o.kind == 1)
        {
            p = (p & !o.deny) | o.allow;
        }
        p
    }

    /// Whether we own the server and the roles we have in it, or the roles we're viewing it as.
    pub(super) fn roles_in<'a>(&'a self, g: &'a Guild) -> (bool, &'a [Id]) {
        match &self.viewing {
            Some((gid, roles)) if *gid == g.id => (false, roles),
            _ => (g.owner == self.me, &g.my_roles),
        }
    }

    /// Seconds left of our own timeout in a server, 0 when we aren't in one.
    pub fn timeout_left(&self, gid: Id) -> i64 {
        self.guilds.get(&gid).map_or(0, |g| (g.my_timeout - chrono::Utc::now().timestamp()).max(0))
    }

    /// Whether we may write here: a timeout takes the permission away, a thread asks for its own
    /// and a locked one needs manage threads.

    /// Pinning and unpinning: anyone in a dm, Pin Messages (with history to read) in a server.
    pub fn can_pin(&self, c: &Channel) -> bool {
        c.guild == 0 || self.perms(c) & (PIN_MESSAGES | READ_MESSAGE_HISTORY) == PIN_MESSAGES | READ_MESSAGE_HISTORY
    }

    pub fn can_send(&self, c: &Channel) -> bool {
        if c.guild == 0 {
            return true;
        }
        let p = self.perms(c);
        if p & VIEW_CHANNEL == 0 {
            return true;
        }
        let bit = if c.is_thread() { SEND_IN_THREADS } else { SEND_MESSAGES };
        let unlocked = !c.locked || p & MANAGE_THREADS != 0;
        self.timeout_left(c.guild) == 0 && p & bit != 0 && unlocked

    }

    /// Milliseconds until slowmode lets us post again here.
    pub fn slow_left(&self, c: &Channel) -> i64 {
        if c.rate == 0 || self.perms(c) & BYPASS_SLOWMODE != 0 {
            return 0;
        }
        let now = chrono::Utc::now().timestamp_millis();
        let last = self.msgs.get(&c.id).and_then(|e| e.list.iter().rev().find(|m| m.author == self.me && !m.failed)).map_or(0, |m| if m.pending { now } else { m.ts() });
        (last + c.rate as i64 * 1000 - now).max(0)
    }

    pub fn limited(&self, c: &Channel) -> bool {
        let Some(g) = self.guilds.get(&c.guild) else { return false };
        let mut p = g.roles.get(&g.id).map_or(0, |r| r.perms);
        if let Some(o) = c.overwrites.iter().find(|o| o.id == g.id) {
            p = (p & !o.deny) | o.allow;
        }
        p & VIEW_CHANNEL == 0
    }

    pub fn full(&self, c: &Channel) -> bool {
        c.user_limit > 0 && self.guilds.get(&c.guild).is_some_and(|g| g.voice.values().filter(|ch| **ch == c.id).count() >= c.user_limit as usize)
    }

    /// Whether @everyone is kept out of the channel (or, for voice, kept from joining).
    pub fn locked(&self, c: &Channel) -> bool {
        let Some(g) = self.guilds.get(&c.guild) else { return false };
        let mut p = g.roles.get(&g.id).map_or(0, |r| r.perms);
        if let Some(o) = c.overwrites.iter().find(|o| o.id == g.id) {
            p = (p & !o.deny) | o.allow;
        }
        let bit = if c.is_voice() { CONNECT } else { VIEW_CHANNEL };
        p & bit == 0 || p & VIEW_CHANNEL == 0
    }

    /// The notification level a channel falls back to: its category's, then the server's own pick,
    /// then the server default.
    pub fn inherited_level(&self, c: &Channel) -> u8 {
        [c.parent, c.guild]
            .iter()
            .filter(|id| **id != 0)
            .find_map(|id| self.levels.get(id).copied())
            .unwrap_or_else(|| self.guilds.get(&c.guild).map(|g| g.default_notif).unwrap_or(0))
    }

    pub(super) fn voice_info(&mut self, ch: Id, status: &Value, start: &Value) {
        match status.as_str().filter(|s| !s.is_empty()) {
            Some(s) => self.voice_status.insert(ch, s.to_string()),
            None => self.voice_status.remove(&ch),
        };
        if let Some(t) = start.as_i64() {
            self.voice_start.insert(ch, t);
        }
    }

    pub(super) fn own_custom(c: Option<(String, Option<ReactEmoji>)>) -> (Option<String>, Option<ReactEmoji>) {
        match c {
            Some((text, emoji)) => (Some(text).filter(|t| !t.is_empty()), emoji),
            None => (None, None),
        }
    }

    // the status the account is set to, so a presence update of ours doesn't undo it
    pub(super) fn own_status(&mut self, raw: &[u8]) {
        let (status, show) = crate::proto::status(raw);
        if let Some(s) = status {
            self.presence.insert(self.me, Status::parse(&s));
        }
        if let Some(show) = show {
            self.hide_game = !show;
        }
    }

    pub(super) fn set_nick(&mut self, uid: Id, v: &Value) {
        match v.as_str().filter(|n| !n.is_empty()) {
            Some(n) => self.nicks.insert(uid, n.to_string()),
            None => self.nicks.remove(&uid),
        };
    }

    /// A dm or group dm that's muted, or every dm when the direct messages entry itself is.
    pub fn dm_muted(&self, ch: Id) -> bool {
        self.muted.contains(&ch) || self.muted.contains(&0)
    }

    /// Muted itself, or through its category or parent channel.
    pub fn is_muted(&self, c: &Channel) -> bool {
        let mut id = c.id;
        for _ in 0..3 {
            if self.muted.contains(&id) {
                return true;
            }
            match self.channels.get(&id) {
                Some(c) if c.parent != 0 => id = c.parent,
                _ => break,
            }
        }
        false
    }
}
