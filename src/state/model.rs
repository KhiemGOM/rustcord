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

impl User {
    pub fn parse(v: &Value) -> Self {
        let s = &v["display_name_styles"];
        let colors: Vec<u32> = s["colors"].as_array().map(|a| a.iter().filter_map(|c| c.as_u64()).map(|c| c as u32).collect()).unwrap_or_default();
        User {
            id: sid(&v["id"]),
            username: st(&v["username"]),
            global_name: ost(&v["global_name"]),
            avatar: ost(&v["avatar"]),
            bot: v["bot"].as_bool().unwrap_or(false),
            style: (!colors.is_empty()).then(|| (s["effect_id"].as_u64().unwrap_or(1) as u8, colors)),
            font: s["font_id"].as_u64().unwrap_or(0) as u8,
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
        self.decoration.as_ref().map(|a| format!("{}/avatar-decoration-presets/{a}.png?size=96&passthrough=false", crate::api::cdn()))
    }

    /// The still of a nameplate, 448x84 with its own fade on the left.
    pub fn nameplate_url(&self) -> Option<String> {
        self.nameplate.as_ref().map(|a| format!("{}/assets/collectibles/{a}static.png", crate::api::cdn()))
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
pub(crate) struct Channel {
    pub id: Id,
    pub guild: Id,
    pub kind: u8,
    pub name: String,
    pub topic: Option<String>,
    pub position: i32,
    pub parent: Id,
    pub overwrites: Vec<Overwrite>,
    pub last_msg: Id,
    pub recipients: Vec<Id>,
    pub icon: Option<String>,
    #[serde(default)]
    pub owner: Id,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub joined: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub msg_count: u32,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub applied: Vec<Id>,
    #[serde(default)]
    pub default_react: Option<ReactEmoji>,
    #[serde(default)]
    pub require_tag: bool,
    #[serde(default)]
    pub archive_after: u32,
    #[serde(default)]
    pub nsfw: bool,
    // a forum's default view: 1 list, 2 gallery
    #[serde(default)]
    pub layout: u8,
    // when something was last pinned, unix ms
    #[serde(default)]
    pub last_pin: i64,
    #[serde(default)]
    pub rate: u32,
    #[serde(default)]
    pub user_limit: u32,
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
        Channel {
            id: sid(&v["id"]),
            guild: if g != 0 { g } else { guild },
            kind: v["type"].as_u64().unwrap_or(0) as u8,
            name: st(&v["name"]),
            topic: ost(&v["topic"]),
            position: v["position"].as_i64().unwrap_or(0) as i32,
            parent: sid(&v["parent_id"]),
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
            last_msg: sid(&v["last_message_id"]),
            recipients: Vec::new(),
            icon: ost(&v["icon"]),
            owner: sid(&v["owner_id"]),
            archived: v["thread_metadata"]["archived"].as_bool().unwrap_or(false),
            joined: v["member"].is_object(),
            locked: v["thread_metadata"]["locked"].as_bool().unwrap_or(false),
            msg_count: v["message_count"].as_u64().unwrap_or(0) as u32,
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
            require_tag: v["flags"].as_u64().unwrap_or(0) & (1 << 4) != 0,
            archive_after: v["default_auto_archive_duration"].as_u64().unwrap_or(0) as u32,
            last_pin: iso_ms(&v["last_pin_timestamp"]),
            nsfw: v["nsfw"].as_bool().unwrap_or(false),
            layout: v["default_forum_layout"].as_u64().unwrap_or(0) as u8,
            rate: v["rate_limit_per_user"].as_u64().unwrap_or(0) as u32,
            user_limit: v["user_limit"].as_u64().unwrap_or(0) as u32,
        }
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
pub(crate) struct Role {
    pub id: Id,
    pub name: String,
    pub color: u32,
    pub position: i32,
    pub perms: u64,
    pub hoist: bool,
    /// primary, secondary and optional tertiary colour when the role is a gradient
    #[serde(default)]
    pub grad: Vec<u32>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub managed: bool,
}

impl Role {
    /// The role's icon image, or its unicode emoji drawn by twemoji.
    pub fn icon_url(&self, size: u32) -> Option<String> {
        match (&self.icon, &self.emoji) {
            (Some(h), _) => Some(format!("{}/role-icons/{}/{h}.png?size={size}", api::cdn(), self.id)),
            (None, Some(e)) => Some(crate::markdown::twemoji(e)),
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
}

impl Attachment {
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
        let proxy = ost(&v["proxy_url"]);
        let proxied = proxy.is_some();
        let url = proxy.or_else(|| ost(&v["url"]))?;
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
    // unix ms
    pub timestamp: Option<i64>,
    pub fields: Vec<(String, String, bool)>,
    pub image: Option<Media>,
    pub thumb: Option<Media>,
    pub video: Option<Media>,
}

impl Embed {
    pub(super) fn parse(v: &Value) -> Self {
        Embed {
            kind: st(&v["type"]),
            color: v["color"].as_u64().map(|c| c as u32),
            title: ost(&v["title"]),
            url: ost(&v["url"]),
            desc: ost(&v["description"]),
            author: ost(&v["author"]["name"]),
            provider: ost(&v["provider"]["name"]),
            footer: ost(&v["footer"]["text"]),
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
