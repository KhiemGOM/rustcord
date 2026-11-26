use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub(crate) enum Status {
    Online,
    Idle,
    Dnd,
    #[default]
    Offline,
}

impl Status {
    pub fn parse(s: &str) -> Self {
        match s {
            "online" => Status::Online,
            "idle" => Status::Idle,
            "dnd" => Status::Dnd,
            _ => Status::Offline,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct User {
    pub id: Id,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub bot: bool,
    /// nitro display name style: effect id and colours (the font is ignored)
    #[serde(default)]
    pub style: Option<(u8, Vec<u32>)>,
    /// nitro display name font, discord's font_id; 0 for none
    #[serde(default)]
    pub font: u8,
    /// server tag shown after the name: (tag, its server, badge hash)
    #[serde(default)]
    pub tag: Option<(String, Id, String)>,
    /// avatar decoration asset
    #[serde(default)]
    pub decoration: Option<String>,
    /// nameplate collectible: the asset folder its images live in
    #[serde(default)]
    pub nameplate: Option<String>,
    #[serde(default)]
    pub flags: u64,
}

pub(super) fn parse_name_style(s: &Value) -> (Option<(u8, Vec<u32>)>, u8) {
    let colors: Vec<u32> = s["colors"].as_array().map(|a| a.iter().filter_map(|c| c.as_u64()).map(|c| c as u32).collect()).unwrap_or_default();
    let style = (!colors.is_empty()).then(|| (s["effect_id"].as_u64().unwrap_or(1) as u8, colors));
    let font = s["font_id"].as_u64().unwrap_or(0) as u8;
    (style, font)
}

pub(super) fn decoration_asset_url(asset: &str) -> String {
    format!("{}/avatar-decoration-presets/{asset}.png?size=96&passthrough=true", crate::api::cdn())
}

impl User {
    pub fn parse(v: &Value) -> Self {
        let (style, font) = parse_name_style(&v["display_name_styles"]);
        User {
            id: sid(&v["id"]),
            username: st(&v["username"]),
            global_name: ost(&v["global_name"]),
            avatar: ost(&v["avatar"]),
            bot: v["bot"].as_bool().unwrap_or(false),
            style,
            font,
            tag: {
                let g = &v["primary_guild"];
                (g["identity_enabled"].as_bool() == Some(true))
                    .then(|| (st(&g["tag"]), sid(&g["identity_guild_id"]), st(&g["badge"])))
                    .filter(|(t, _, _)| !t.is_empty())
            },
            decoration: ost(&v["avatar_decoration_data"]["asset"]),
            nameplate: ost(&v["collectibles"]["nameplate"]["asset"]),
            flags: v["public_flags"].as_u64().unwrap_or(0),
        }
    }

    pub fn tag_badge_url(&self) -> Option<String> {
        self.tag.as_ref().filter(|(_, _, b)| !b.is_empty()).map(|(_, g, b)| format!("{}/guild-tag-badges/{g}/{b}.png?size=32", crate::api::cdn()))
    }

    pub fn decoration_url(&self) -> Option<String> {
        self.decoration.as_deref().map(decoration_asset_url)
    }

    pub fn display(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    pub fn avatar_url(&self) -> String {
        match &self.avatar {
            Some(h) => format!("{}/avatars/{}/{}.png?size=80", api::cdn(), self.id, h),
            None => format!("{}/embed/avatars/{}.png", api::cdn(), (self.id >> 22) % 6),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Overwrite {
    pub id: Id,
    pub kind: u8,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct ChannelExtra {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overwrites: Vec<Overwrite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub owner: Id,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_react: Option<ReactEmoji>,
    #[serde(default)]
    pub archive_after: u32,
    #[serde(default)]
    pub last_pin: i64,
    #[serde(default)]
    pub rate: u32,
    #[serde(default)]
    pub user_limit: u32,
}

impl ChannelExtra {
    fn is_empty(&self) -> bool {
        self.topic.is_none()
            && self.overwrites.is_empty()
            && self.icon.is_none()
            && self.owner == 0
            && self.tags.is_empty()
            && self.applied.is_empty()
            && self.default_react.is_none()
            && self.archive_after == 0
            && self.last_pin == 0
            && self.rate == 0
            && self.user_limit == 0
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Channel {
    pub id: Id,
    pub guild: Id,
    pub parent: Id,
    pub last_msg: Id,
    pub name: String,
    #[serde(default, skip_serializing_if = "slice_empty")]
    pub recipients: Box<[Id]>,
    pub position: i32,
    #[serde(default)]
    pub msg_count: u32,
    pub kind: u8,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub joined: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub require_tag: bool,
    #[serde(default)]
    pub resource: bool,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub layout: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Box<ChannelExtra>>,
}

fn slice_empty<T>(s: &[T]) -> bool {
    s.is_empty()
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Tag {
    pub id: Id,
    pub name: String,
    pub emoji: Option<String>,
    #[serde(default)]
    pub emoji_id: Id,
}

impl Channel {
    pub fn parse(v: &Value, guild: Id) -> Self {
        let g = sid(&v["guild_id"]);
        let extra = ChannelExtra {
            topic: ost(&v["topic"]),
            overwrites: v["permission_overwrites"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|o| Overwrite {
                            id: sid(&o["id"]),
                            kind: o["type"].as_u64().unwrap_or(0) as u8,
                            allow: perm(&o["allow"]),
                            deny: perm(&o["deny"]),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            icon: ost(&v["icon"]),
            owner: sid(&v["owner_id"]),
            tags: v["available_tags"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| Tag { id: sid(&t["id"]), name: st(&t["name"]), emoji: ost(&t["emoji_name"]), emoji_id: sid(&t["emoji_id"]) })
                .collect(),
            applied: ids(&v["applied_tags"]),
            default_react: Some(&v["default_reaction_emoji"])
                .filter(|e| e.is_object())
                .map(|e| ReactEmoji { id: sid(&e["emoji_id"]), name: st(&e["emoji_name"]), animated: false })
                .filter(|e| e.id != 0 || !e.name.is_empty()),
            archive_after: v["default_auto_archive_duration"].as_u64().unwrap_or(0) as u32,
            last_pin: iso_ms(&v["last_pin_timestamp"]),
            rate: v["rate_limit_per_user"].as_u64().unwrap_or(0) as u32,
            user_limit: v["user_limit"].as_u64().unwrap_or(0) as u32,
        };
        Channel {
            id: sid(&v["id"]),
            guild: if g != 0 { g } else { guild },
            parent: sid(&v["parent_id"]),
            last_msg: sid(&v["last_message_id"]),
            name: st(&v["name"]),
            recipients: Box::new([]),
            position: v["position"].as_i64().unwrap_or(0) as i32,
            msg_count: v["message_count"].as_u64().unwrap_or(0) as u32,
            kind: v["type"].as_u64().unwrap_or(0) as u8,
            archived: v["thread_metadata"]["archived"].as_bool().unwrap_or(false),
            joined: v["member"].is_object(),
            locked: v["thread_metadata"]["locked"].as_bool().unwrap_or(false),
            require_tag: v["flags"].as_u64().unwrap_or(0) & (1 << 4) != 0,
            resource: v["flags"].as_u64().unwrap_or(0) & (1 << 7) != 0,
            nsfw: v["nsfw"].as_bool().unwrap_or(false),
            layout: v["default_forum_layout"].as_u64().unwrap_or(0) as u8,
            extra: (!extra.is_empty()).then(|| Box::new(extra)),
        }
    }

    pub fn topic(&self) -> Option<&str> {
        self.extra.as_ref().and_then(|e| e.topic.as_deref())
    }

    pub fn overwrites(&self) -> &[Overwrite] {
        self.extra.as_ref().map_or(&[], |e| &e.overwrites)
    }

    pub fn icon(&self) -> Option<&str> {
        self.extra.as_ref().and_then(|e| e.icon.as_deref())
    }

    pub fn owner(&self) -> Id {
        self.extra.as_ref().map_or(0, |e| e.owner)
    }

    pub fn tags(&self) -> &[Tag] {
        self.extra.as_ref().map_or(&[], |e| &e.tags)
    }

    pub fn applied(&self) -> &[Id] {
        self.extra.as_ref().map_or(&[], |e| &e.applied)
    }

    pub fn default_react(&self) -> Option<&ReactEmoji> {
        self.extra.as_ref().and_then(|e| e.default_react.as_ref())
    }

    pub fn archive_after(&self) -> u32 {
        self.extra.as_ref().map_or(0, |e| e.archive_after)
    }

    pub fn last_pin(&self) -> i64 {
        self.extra.as_ref().map_or(0, |e| e.last_pin)
    }

    pub fn set_last_pin(&mut self, ts: i64) {
        if ts != 0 || self.extra.is_some() {
            self.extra.get_or_insert_with(Default::default).last_pin = ts;
        }
    }

    pub fn rate(&self) -> u32 {
        self.extra.as_ref().map_or(0, |e| e.rate)
    }

    pub fn user_limit(&self) -> u32 {
        self.extra.as_ref().map_or(0, |e| e.user_limit)
    }

    pub fn is_text(&self) -> bool {
        matches!(self.kind, 0 | 5 | 1 | 3 | 10 | 11 | 12)
    }

    pub fn is_thread(&self) -> bool {
        matches!(self.kind, 10..=12)
    }

    pub fn is_forum(&self) -> bool {
        matches!(self.kind, 15 | 16)
    }

    pub fn is_voice(&self) -> bool {
        matches!(self.kind, 2 | 13)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct RoleExtra {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grad: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
}

impl RoleExtra {
    pub fn into_opt(self) -> Option<Box<Self>> {
        (!self.grad.is_empty() || self.icon.is_some() || self.emoji.is_some()).then(|| Box::new(self))
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Role {
    pub id: Id,
    pub name: String,
    pub perms: u64,
    pub color: u32,
    pub position: i32,
    pub hoist: bool,
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub managed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Box<RoleExtra>>,
}

impl Role {
    pub fn grad(&self) -> &[u32] {
        self.extra.as_ref().map_or(&[], |e| &e.grad)
    }

    pub fn has_icon(&self) -> bool {
        self.extra.as_ref().is_some_and(|e| e.icon.is_some() || e.emoji.is_some())
    }

    pub fn icon_url(&self, size: u32) -> Option<String> {
        let e = self.extra.as_ref()?;
        match (&e.icon, &e.emoji) {
            (Some(h), _) => Some(format!("{}/role-icons/{}/{h}.png?size={size}", api::cdn(), self.id)),
            (None, Some(em)) => Some(crate::markdown::twemoji(em)),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Emoji {
    pub id: Id,
    pub name: String,
    pub animated: bool,
    #[serde(default)]
    pub roles: Vec<Id>,
    // `available: false` from the api: the server lost the boost slot for it
    #[serde(default)]
    pub unavailable: bool,
}

impl Emoji {
    pub(super) fn parse(v: &Value) -> Emoji {
        Emoji {
            id: sid(&v["id"]),
            name: st(&v["name"]),
            animated: v["animated"].as_bool().unwrap_or(false),
            roles: ids(&v["roles"]),
            unavailable: v["available"].as_bool() == Some(false),
        }
    }

    pub fn url(&self) -> String {
        format!(
            "{}",
            crate::markdown::emoji_url(self.id, self.animated)
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum ListItem {
    Group { id: String, count: u32 },
    Member(Id),
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Guild {
    pub id: Id,
    pub name: String,
    pub icon: Option<String>,
    pub owner: Id,
    pub roles: Map<Id, Role>,
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub stickers: Vec<(Id, String, u8)>,
    #[serde(default)]
    pub features: Vec<String>,
    /// The server's own invite link, which anyone in it may hand out.
    #[serde(default)]
    pub vanity: Option<String>,
    #[serde(default)]
    pub hub_enabled: Option<bool>,
    #[serde(default)]
    pub hub_published: bool,
    #[serde(skip)]
    pub theme: Option<crate::theme::Fill>,
    pub channels: Vec<Id>,
    pub my_roles: Vec<Id>,
    pub member_count: u32,
    // 0 all messages, 1 only mentions; what the server picked for people who haven't chosen
    #[serde(default)]
    pub default_notif: u8,
    #[serde(default)]
    pub rules: Id,
    #[serde(default)]
    pub afk: Id,
    // the boosted banner over the channel list, and the boost goal bar under it
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub boosts: u32,
    #[serde(default)]
    pub boost_bar: bool,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub events: u32,
    #[serde(default)]
    pub newest_event: Id,
    // our member flags here and when we joined (unix seconds), for the server guide's checklist
    #[serde(default)]
    pub my_flags: u64,
    /// we joined but haven't passed the server's rules screening yet
    #[serde(default)]
    pub my_pending: bool,
    #[serde(default)]
    pub joined: i64,
    #[serde(default)]
    pub my_timeout: i64,
    #[serde(default)]
    pub invites_until: i64,
    #[serde(default)]
    pub dms_until: i64,
    #[serde(skip)]
    pub voice: Map<Id, Id>,
    #[serde(skip)]
    pub streaming: Set<Id>,
    #[serde(skip)]
    pub list: Vec<ListItem>,
}

impl Guild {
    /// Invites are paused by the old feature flag or by a lockdown that hasn't run out.
    pub fn dms_paused(&self) -> bool {
        self.dms_until > chrono::Utc::now().timestamp()
    }

    pub fn invites_paused(&self) -> bool {
        self.features.iter().any(|f| f == "INVITES_DISABLED") || self.invites_until > chrono::Utc::now().timestamp()
    }

    /// Discord keeps the "Get Started" checklist for members who joined in the last month and
    /// haven't finished it: started home actions, not completed.
    pub fn new_member(&self) -> bool {
        self.my_flags & 32 != 0 && self.my_flags & 64 == 0 && chrono::Utc::now().timestamp() - self.joined < 30 * 86400
    }

    pub fn banner_url(&self) -> Option<String> {
        self.banner.as_ref().map(|h| format!("{}/banners/{}/{h}.png?size=480", api::cdn(), self.id))
    }

    pub fn icon_url(&self) -> Option<String> {
        self.icon
            .as_ref()
            .map(|h| format!("{}/icons/{}/{}.png?size=96", api::cdn(), self.id, h))
    }

    // same rule as the official client: first letter of each word, punctuation and emoji kept
    pub fn acronym(&self) -> String {
        let name = self.name.replace("'s ", " ");
        let mut out = String::new();
        let mut in_word = false;
        for c in name.chars() {
            if c.is_alphanumeric() || c == '_' {
                if !in_word {
                    out.push(c);
                }
                in_word = true;
            } else {
                in_word = false;
                if !c.is_whitespace() {
                    out.push(c);
                }
            }
        }
        out
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Member {
    pub nick: Option<String>,
    pub roles: Vec<Id>,
    // when they started boosting the server, unix secs, 0 if they aren't
    #[serde(default)]
    pub boost: i64,
    #[serde(default)]
    pub joined: i64,
    #[serde(default)]
    pub flags: u64,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub decoration: Option<String>,
    #[serde(default)]
    pub nameplate: Option<String>,
    #[serde(default)]
    pub style: Option<(u8, Vec<u32>)>,
    #[serde(default)]
    pub font: u8,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Attachment {
    pub url: String,
    pub proxy: String,
    pub filename: String,
    pub size: u64,
    pub w: u32,
    pub h: u32,
    pub ctype: String,
    // voice messages: length in seconds and the base64 amplitude waveform
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub waveform: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub id: Id,
    #[serde(default)]
    pub flags: u32,
}

impl Attachment {
    pub fn spoiler(&self) -> bool {
        self.flags & 8 != 0 || self.filename.starts_with("SPOILER_")
    }

    pub fn is_image(&self) -> bool {
        self.w > 0 && (self.ctype.starts_with("image/") || has_img_ext(&self.filename))
    }

    pub fn is_audio(&self) -> bool {
        self.ctype.starts_with("audio/") || [".mp3", ".ogg", ".wav", ".flac", ".m4a", ".opus"].iter().any(|e| self.filename.to_ascii_lowercase().ends_with(e))
    }

    pub fn is_video(&self) -> bool {
        let n = self.filename.to_ascii_lowercase();
        self.w > 0 && (self.ctype.starts_with("video/") || [".mp4", ".m4v", ".mov", ".webm", ".wmv"].iter().any(|e| n.ends_with(e)))
    }

    pub fn is_text(&self) -> bool {
        const EXTS: [&str; 34] = [
            "txt", "md", "log", "rs", "py", "js", "mjs", "ts", "tsx", "jsx", "json", "html", "htm", "css", "c", "h", "cpp", "hpp", "cs", "java", "go", "kt", "rb", "sh", "yml", "yaml", "toml", "xml", "csv", "ini", "cfg", "lua", "sql", "php",
        ];
        let ext = self.filename.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        self.size > 0 && (self.ctype.starts_with("text/") || ext.is_some_and(|e| EXTS.contains(&e.as_str())))
    }

    /// Opus-in-ogg (discord voice messages, soundboard) is what we can decode and play inline.
    pub fn playable(&self) -> bool {
        self.ctype.contains("ogg") || self.ctype.contains("opus") || self.filename.to_ascii_lowercase().ends_with(".ogg")
    }
}

pub(super) fn has_img_ext(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| n.ends_with(e))
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Media {
    pub url: String,
    pub w: u32,
    pub h: u32,
    // Discord kept a copy of it, so `url` is that copy; otherwise `url` is the provider's own page
    #[serde(default)]
    pub proxied: bool,
}

impl Media {
    pub(super) fn parse(v: &Value) -> Option<Media> {
        let proxy = ost(&v["proxy_url"]).and_then(|u| crate::api::web_url(&u));
        let proxied = proxy.is_some();
        let url = proxy.or_else(|| ost(&v["url"]).and_then(|u| crate::api::web_url(&u)))?;
        Some(Media {
            url,
            proxied,
            w: v["width"].as_u64().unwrap_or(0) as u32,
            h: v["height"].as_u64().unwrap_or(0) as u32,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Embed {
    pub kind: String,
    pub color: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub desc: Option<String>,
    pub author: Option<String>,
    pub provider: Option<String>,
    pub footer: Option<String>,
    #[serde(default)]
    pub footer_icon: Option<String>,
    // unix ms
    pub timestamp: Option<i64>,
    pub fields: Vec<(String, String, bool)>,
    pub image: Option<Media>,
    pub thumb: Option<Media>,
    pub video: Option<Media>,
    #[serde(default)]
    pub flags: u32,
}

impl Embed {
    pub(super) fn parse(v: &Value) -> Self {
        Embed {
            kind: st(&v["type"]),
            color: v["color"].as_u64().map(|c| c as u32),
            title: ost(&v["title"]),
            url: ost(&v["url"]).and_then(|u| crate::api::web_url(&u)),
            desc: ost(&v["description"]),
            author: ost(&v["author"]["name"]),
            provider: ost(&v["provider"]["name"]),
            footer: ost(&v["footer"]["text"]),
            footer_icon: ost(&v["footer"]["proxy_icon_url"]).or_else(|| ost(&v["footer"]["icon_url"])).and_then(|u| crate::api::web_url(&u)),
            timestamp: v["timestamp"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis()),
            fields: v["fields"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|f| {
                            (st(&f["name"]), st(&f["value"]), f["inline"].as_bool().unwrap_or(false))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            image: Media::parse(&v["image"]),
            thumb: Media::parse(&v["thumbnail"]),
            video: Media::parse(&v["video"]),
            flags: v["flags"].as_u64().unwrap_or(0) as u32,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq)]
pub(crate) struct ReactEmoji {
    pub id: Id,
    pub name: String,
    pub animated: bool,
}

impl ReactEmoji {
    pub(super) fn parse(v: &Value) -> Self {
        ReactEmoji {
            id: sid(&v["id"]),
            name: st(&v["name"]),
            animated: v["animated"].as_bool().unwrap_or(false),
        }
    }

    // the form the reactions endpoint wants in its path
    pub fn api_name(&self) -> String {
        if self.id != 0 {
            format!("{}:{}", self.name, self.id)
        } else {
            self.name.clone()
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Reaction {
    pub emoji: ReactEmoji,
    pub count: u32,
    pub me: bool,
    #[serde(default)]
    pub burst: u32,
    #[serde(default)]
    pub me_burst: bool,
    /// The first of a super reaction's own colours, which tints its count.
    #[serde(default)]
    pub burst_color: Option<u32>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Call {
    pub with: Vec<Id>,
    pub ended: Option<i64>,
}

/// Who ran the command behind an app's reply, and who installed the app they ran it in.
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Interaction {
    pub user: Id,
    pub owner: Id,
    pub in_guild: bool,
    #[serde(default)]
    pub name: String,
}

/// Where an interaction this client sent has got to.
#[derive(Clone)]
pub(crate) enum Stage {
    Sending,
    Created,
    Failed(String, std::time::Instant),
}

/// An interaction waiting on a message: the command that made it, or the component pressed on it.
#[derive(Clone)]
pub(crate) struct Busy {
    pub nonce: String,
    pub component: String,
    pub stage: Stage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_channel_and_role_sizes() {
        assert!(std::mem::size_of::<Channel>() <= 96, "Channel={}", std::mem::size_of::<Channel>());
        assert!(std::mem::size_of::<Role>() <= 64, "Role={}", std::mem::size_of::<Role>());

        let plain = Channel::parse(&json!({ "id": "10", "type": 0, "name": "general", "position": 1 }), 1);
        assert!(plain.extra.is_none());
        let rt: Channel = serde_json::from_slice(&serde_json::to_vec(&plain).unwrap()).unwrap();
        assert!(rt.extra.is_none());
        assert_eq!(rt.name, "general");

        let forum = Channel::parse(&json!({ "id": "20", "type": 15, "name": "ideas", "topic": "hello", "rate_limit_per_user": 5 }), 1);
        assert!(forum.extra.is_some());
        assert_eq!(forum.topic(), Some("hello"));
        assert_eq!(forum.rate(), 5);
    }
}
