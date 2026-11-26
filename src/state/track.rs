use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct GuildPrefs {
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    pub highlights_off: bool,
    pub mobile_push: bool,
}

impl Default for GuildPrefs {
    fn default() -> Self {
        Self { suppress_everyone: false, suppress_roles: false, highlights_off: false, mobile_push: true }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct ReadState {
    pub last_acked: Id,
    pub mentions: u32,
    // the newest pin you've seen, unix ms
    #[serde(default)]
    pub last_pin: i64,
    #[serde(default)]
    pub flags: Option<u8>,
}

/// One entry of a presence's activities: a game, a stream, Spotify, a custom status.
#[derive(Clone, Default)]
pub(crate) struct Activity {
    pub kind: u8,
    pub name: String,
    pub details: Option<String>,
    pub state: Option<String>,
    pub emoji: Option<String>,
    pub app: Id,
    // unix ms, 0 when absent
    pub start: i64,
    pub end: i64,
    pub created_at: i64,
    pub url: Option<String>,
    pub large: Option<String>,
    pub small: Option<String>,
    pub party: Option<(u32, u32)>,
    pub sync_id: Option<String>,
}

pub(crate) fn safe_stream_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

impl Activity {
    pub fn spotify(&self) -> bool {
        self.kind == 2 && self.sync_id.is_some()
    }

    /// When the activity began (unix ms): `timestamps.start` when present, else `created_at`.
    pub fn started_at(&self) -> i64 {
        if self.start > 0 { self.start } else { self.created_at }
    }

    /// An asset key ("spotify:…", "mp:external/…", "twitch:…", or an application asset id) as an image url.
    pub fn art(&self, key: &str) -> Option<String> {
        if let Some(id) = key.strip_prefix("spotify:") {
            (!id.is_empty()).then(|| format!("https://i.scdn.co/image/{id}"))
        } else if let Some(p) = key.strip_prefix("mp:external/") {
            (!p.is_empty()).then(|| format!("{}/external/{p}", api::media()))
        } else if let Some(p) = key.strip_prefix("mp:attachments/") {
            (!p.is_empty()).then(|| format!("{}/attachments/{p}", api::media()))
        } else if let Some(n) = key.strip_prefix("twitch:") {
            safe_stream_id(n).then(|| format!("https://static-cdn.jtvnw.net/previews-ttv/live_user_{n}-440x248.jpg"))
        } else if let Some(v) = key.strip_prefix("youtube:") {
            safe_stream_id(v).then(|| format!("https://i.ytimg.com/vi/{v}/hqdefault_live.jpg"))
        } else if key.starts_with("mp:") || key.contains(':') || self.app == 0 {
            None
        } else {
            Some(format!("{}/app-assets/{}/{key}.png?size=128", api::cdn(), self.app))
        }
    }
}

/// One card of discord's activity feed (the content inventory): a game played, an activity
/// launched, a week's top artist or game.
#[derive(Clone, Default)]
pub(crate) struct FeedEntry {
    // a snowflake for games and activities, so also when it happened
    pub id: Id,
    pub author: Id,
    // content_type: 1 played game, 3 top game, 6 top artist, 8 launched activity
    pub kind: u8,
    pub title: String,
    pub image: Option<String>,
    pub app: Id,
    pub live: bool,
    // unix ms, 0 for never
    pub expires: i64,
    pub count: u64,
    pub secs: u64,
    pub streak: u64,
    pub trending: bool,
    pub marathon: bool,
    pub returning: bool,
    pub first: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Msgs {
    pub list: Vec<Message>,
    pub at_start: bool,
    #[serde(skip)]
    pub loading: bool,
    #[serde(skip)]
    pub fresh: bool,
    #[serde(skip)]
    pub failed: Option<(Page, String)>,
    // after a jump into old history the list no longer reaches the newest message
    #[serde(skip)]
    pub detached: bool,
}

/// Which slice of history a fetch asks for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Page {
    Latest,
    Before(Id),
    After(Id),
    Around(Id),
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Folder {
    pub id: Id,
    pub name: Option<String>,
    pub color: Option<u32>,
    pub guilds: Vec<Id>,
}

// relationship types as discord numbers them
pub(crate) const FRIEND: u8 = 1;
pub(crate) const BLOCKED: u8 = 2;
pub(crate) const INCOMING: u8 = 3;
pub(crate) const OUTGOING: u8 = 4;

/// The store, plus what's worked out from all of it (server badges, the mention count) once and
/// kept until something takes the store mutably: a walk over every channel is too much per frame.
#[derive(Default)]
pub(crate) struct Tracked {
    pub(super) store: Store,
    pub(super) sums: std::cell::OnceCell<Sums>,
}

pub(super) struct Sums {
    pub(super) badges: Map<Id, (bool, u32)>,
    pub(super) mentions: u32,
    pub(super) threads: Map<Id, Vec<Id>>,
    pub(super) unread: bool,
}

impl std::ops::Deref for Tracked {
    type Target = Store;
    fn deref(&self) -> &Store {
        &self.store
    }
}

impl std::ops::DerefMut for Tracked {
    fn deref_mut(&mut self) -> &mut Store {
        self.sums.take();
        &mut self.store
    }
}

impl From<Store> for Tracked {
    fn from(store: Store) -> Self {
        Tracked { store, sums: Default::default() }
    }
}

impl Tracked {
    pub fn apply(&mut self, t: &str, d: &Value) -> Option<Id> {
        if !matches!(
            t,
            "PRESENCE_UPDATE"
                | "TYPING_START"
                | "MESSAGE_UPDATE"
                | "MESSAGE_DELETE"
                | "MESSAGE_DELETE_BULK"
                | "MESSAGE_REACTION_ADD"
                | "MESSAGE_REACTION_REMOVE"
                | "MESSAGE_REACTION_REMOVE_ALL"
                | "MESSAGE_REACTION_REMOVE_EMOJI"
                | "MESSAGE_POLL_VOTE_ADD"
                | "MESSAGE_POLL_VOTE_REMOVE"
                | "GUILD_MEMBER_LIST_UPDATE"
                | "THREAD_MEMBER_LIST_UPDATE"
                | "GUILD_MEMBERS_CHUNK"
                | "USER_UPDATE"
                | "CHANNEL_INFO"
                | "VOICE_CHANNEL_STATUS_UPDATE"
                | "VOICE_CHANNEL_START_TIME_UPDATE"
        ) {
            self.sums.take();
        }
        self.store.apply(t, d)
    }

    pub fn prune_typing(&mut self) {
        self.store.prune_typing();
    }

    pub fn set_messages(&mut self, ch: Id, page: Page, list: &[Value]) -> usize {
        self.store.set_messages(ch, page, list)
    }

    pub fn msgs_mut(&mut self) -> &mut Map<Id, Msgs> {
        &mut self.store.msgs
    }

    pub fn set_presence(&mut self, uid: Id, status: Status) {
        self.store.presence.insert(uid, status);
    }

    pub fn app_icons_mut(&mut self) -> &mut Map<Id, Option<String>> {
        &mut self.store.app_icons
    }

    pub fn game_names_mut(&mut self) -> &mut Map<Id, String> {
        &mut self.store.game_names
    }

    pub(super) fn sums(&self) -> &Sums {
        self.sums.get_or_init(|| {
            let s = &self.store;
            let mentions = s
                .reads
                .iter()
                .filter(|(ch, _)| s.channels.get(ch).is_some_and(|c| !s.muted.contains(&c.id) && !s.muted.contains(&c.guild)))
                .map(|(_, r)| r.mentions)
                .sum();
            let mut threads: Map<Id, Vec<Id>> = Default::default();
            for c in s.channels.values().filter(|c| c.is_thread()) {
                threads.entry(c.guild).or_default().push(c.id);
            }
            threads.values_mut().for_each(|t| t.sort_unstable());
            let unread = s.channels.values().any(|c| c.is_text() && !s.muted.contains(&c.guild) && !s.is_muted(c) && s.unread(c) && s.can_view(c));
            Sums { badges: s.guilds.keys().map(|g| (*g, s.guild_badge(*g))).collect(), mentions, threads, unread }
        })
    }

    pub fn guild_badge(&self, gid: Id) -> (bool, u32) {
        self.sums().badges.get(&gid).copied().unwrap_or_default()
    }

    /// Mentions over every channel that isn't muted, as the inbox and the title count them.
    pub fn mention_count(&self) -> u32 {
        self.sums().mentions
    }

    /// A server's threads, oldest first.
    pub fn threads_in(&self, gid: Id) -> impl Iterator<Item = &Channel> {
        self.sums().threads.get(&gid).into_iter().flatten().filter_map(|t| self.channels.get(t))
    }

    pub fn any_unread(&self) -> bool {
        self.sums().unread
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn non_badge_mutations_keep_sums_cached() {
        let mut t = Tracked::default();
        t.guild_badge(1);
        assert!(t.sums.get().is_some());

        t.prune_typing();
        assert!(t.sums.get().is_some());

        t.msgs_mut().entry(10).or_default();
        assert!(t.sums.get().is_some());

        t.set_messages(10, Page::Latest, &[]);
        assert!(t.sums.get().is_some());

        t.set_presence(2, Status::Online);
        assert!(t.sums.get().is_some());

        t.app_icons_mut().insert(3, None);
        assert!(t.sums.get().is_some());

        t.game_names_mut().insert(4, "Game".into());
        assert!(t.sums.get().is_some());

        t.apply("MESSAGE_UPDATE", &json!({ "channel_id": "10", "id": "1" }));
        assert!(t.sums.get().is_some());

        t.apply("MESSAGE_REACTION_ADD", &json!({ "channel_id": "10", "message_id": "1" }));
        assert!(t.sums.get().is_some());

        t.apply("MESSAGE_CREATE", &json!({ "channel_id": "10", "id": "2", "author": { "id": "9" } }));
        assert!(t.sums.get().is_none());
    }
}
