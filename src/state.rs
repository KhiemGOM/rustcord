use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::api;

pub type Id = u64;

pub fn sid(v: &Value) -> Id {
    match v {
        Value::String(s) => s.parse().unwrap_or(0),
        Value::Number(n) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

fn st(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn ost(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn perm(v: &Value) -> u64 {
    match v {
        Value::String(s) => s.parse().unwrap_or(0),
        Value::Number(n) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

fn ids(v: &Value) -> Vec<Id> {
    v.as_array().map(|a| a.iter().map(sid).collect()).unwrap_or_default()
}

pub fn snowflake_ms(id: Id) -> i64 {
    (id >> 22) as i64 + 1_420_070_400_000
}

pub fn make_nonce() -> String {
    let ms = chrono::Utc::now().timestamp_millis() - 1_420_070_400_000;
    ((ms as u64) << 22).to_string()
}

pub const CREATE_INVITE: u64 = 1;
pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const KICK_MEMBERS: u64 = 1 << 1;
pub const ADMINISTRATOR: u64 = 1 << 3;
pub const MANAGE_MESSAGES: u64 = 1 << 13;
pub const SEND_MESSAGES: u64 = 1 << 11;
pub const CONNECT: u64 = 1 << 20;
pub const MANAGE_CHANNELS: u64 = 1 << 4;
pub const MANAGE_GUILD: u64 = 1 << 5;
pub const VIEW_GUILD_INSIGHTS: u64 = 1 << 19;
pub const MANAGE_EVENTS: u64 = 1 << 33;
pub const CREATE_EVENTS: u64 = 1 << 44;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Status {
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
pub struct User {
    pub id: Id,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub bot: bool,
    /// nitro display name style: effect id and colours (the font is ignored)
    #[serde(default)]
    pub style: Option<(u8, Vec<u32>)>,
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
pub struct Overwrite {
    pub id: Id,
    pub kind: u8,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Channel {
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
    pub msg_count: u32,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub applied: Vec<Id>,
    #[serde(default)]
    pub nsfw: bool,
    // a forum's default view: 1 list, 2 gallery
    #[serde(default)]
    pub layout: u8,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Tag {
    pub id: Id,
    pub name: String,
    pub emoji: Option<String>,
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
            msg_count: v["message_count"].as_u64().unwrap_or(0) as u32,
            tags: v["available_tags"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| Tag { id: sid(&t["id"]), name: st(&t["name"]), emoji: ost(&t["emoji_name"]) })
                .collect(),
            applied: ids(&v["applied_tags"]),
            nsfw: v["nsfw"].as_bool().unwrap_or(false),
            layout: v["default_forum_layout"].as_u64().unwrap_or(0) as u8,
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
pub struct Role {
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Emoji {
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
    fn parse(v: &Value) -> Emoji {
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
pub enum ListItem {
    Group { id: String, count: u32 },
    Member(Id),
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Guild {
    pub id: Id,
    pub name: String,
    pub icon: Option<String>,
    pub owner: Id,
    pub roles: HashMap<Id, Role>,
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub stickers: Vec<(Id, String, u8)>,
    #[serde(default)]
    pub features: Vec<String>,
    pub channels: Vec<Id>,
    pub my_roles: Vec<Id>,
    pub member_count: u32,
    // 0 all messages, 1 only mentions; what the server picked for people who haven't chosen
    #[serde(default)]
    pub default_notif: u8,
    #[serde(default)]
    pub rules: Id,
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
    // our member flags here and when we joined (unix seconds), for the server guide's checklist
    #[serde(default)]
    pub my_flags: u64,
    #[serde(default)]
    pub joined: i64,
    #[serde(skip)]
    pub voice: HashMap<Id, Id>,
    #[serde(skip)]
    pub list: Vec<ListItem>,
}

impl Guild {
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
pub struct Member {
    pub nick: Option<String>,
    pub roles: Vec<Id>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Attachment {
    pub url: String,
    pub proxy: String,
    pub filename: String,
    pub size: u64,
    pub w: u32,
    pub h: u32,
    pub ctype: String,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.w > 0 && (self.ctype.starts_with("image/") || has_img_ext(&self.filename))
    }
}

fn has_img_ext(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| n.ends_with(e))
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Media {
    pub url: String,
    pub w: u32,
    pub h: u32,
}

impl Media {
    fn parse(v: &Value) -> Option<Media> {
        let url = ost(&v["proxy_url"]).or_else(|| ost(&v["url"]))?;
        Some(Media {
            url,
            w: v["width"].as_u64().unwrap_or(0) as u32,
            h: v["height"].as_u64().unwrap_or(0) as u32,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Embed {
    pub kind: String,
    pub color: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub desc: Option<String>,
    pub author: Option<String>,
    pub provider: Option<String>,
    pub footer: Option<String>,
    pub fields: Vec<(String, String, bool)>,
    pub image: Option<Media>,
    pub thumb: Option<Media>,
    pub video: Option<Media>,
}

impl Embed {
    fn parse(v: &Value) -> Self {
        Embed {
            kind: st(&v["type"]),
            color: v["color"].as_u64().map(|c| c as u32),
            title: ost(&v["title"]),
            url: ost(&v["url"]),
            desc: ost(&v["description"]),
            author: ost(&v["author"]["name"]),
            provider: ost(&v["provider"]["name"]),
            footer: ost(&v["footer"]["text"]),
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
pub struct ReactEmoji {
    pub id: Id,
    pub name: String,
    pub animated: bool,
}

impl ReactEmoji {
    fn parse(v: &Value) -> Self {
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
pub struct Reaction {
    pub emoji: ReactEmoji,
    pub count: u32,
    pub me: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub id: Id,
    pub channel: Id,
    pub author: Id,
    pub content: String,
    pub kind: u8,
    pub edited: bool,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    pub reactions: Vec<Reaction>,
    pub stickers: Vec<(Id, String, u8)>,
    #[serde(default)]
    pub components: Vec<Vec<Comp>>,
    #[serde(default)]
    pub app_id: Id,
    #[serde(default)]
    pub flags: u64,
    pub reply: Option<Id>,
    pub mentions: Vec<Id>,
    pub mention_roles: Vec<Id>,
    pub mention_everyone: bool,
    pub nonce: Option<String>,
    #[serde(default)]
    pub thread: Option<Id>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip)]
    pub pending: bool,
    #[serde(skip)]
    pub failed: bool,
}

impl Message {
    pub fn ts(&self) -> i64 {
        snowflake_ms(self.id)
    }

    fn fill(&mut self, v: &Value) {
        if let Some(c) = v["content"].as_str() {
            self.content = c.to_string();
        }
        if !v["edited_timestamp"].is_null() && v.get("edited_timestamp").is_some() {
            self.edited = true;
        }
        if let Some(a) = v["attachments"].as_array() {
            self.attachments = a
                .iter()
                .map(|a| Attachment {
                    url: st(&a["url"]),
                    proxy: ost(&a["proxy_url"]).unwrap_or_else(|| st(&a["url"])),
                    filename: st(&a["filename"]),
                    size: a["size"].as_u64().unwrap_or(0),
                    w: a["width"].as_u64().unwrap_or(0) as u32,
                    h: a["height"].as_u64().unwrap_or(0) as u32,
                    ctype: st(&a["content_type"]),
                })
                .collect();
        }
        if let Some(e) = v["embeds"].as_array() {
            self.embeds = e.iter().map(Embed::parse).collect();
        }
        if let Some(r) = v["reactions"].as_array() {
            self.reactions = r
                .iter()
                .map(|r| Reaction {
                    emoji: ReactEmoji::parse(&r["emoji"]),
                    count: r["count"].as_u64().unwrap_or(1) as u32,
                    me: r["me"].as_bool().unwrap_or(false),
                })
                .collect();
        }
        if let Some(s) = v["sticker_items"].as_array() {
            self.stickers = s
                .iter()
                .map(|s| (sid(&s["id"]), st(&s["name"]), s["format_type"].as_u64().unwrap_or(1) as u8))
                .collect();
        }
        if let Some(m) = v["mentions"].as_array() {
            self.mentions = m.iter().map(|u| sid(&u["id"])).collect();
        }
        if v.get("mention_roles").is_some() {
            self.mention_roles = ids(&v["mention_roles"]);
        }
        if let Some(b) = v["mention_everyone"].as_bool() {
            self.mention_everyone = b;
        }
        if let Some(b) = v["pinned"].as_bool() {
            self.pinned = b;
        }
        if v["thread"].is_object() {
            self.thread = Some(sid(&v["thread"]["id"]));
        }
        if let Some(f) = v["flags"].as_u64() {
            self.flags = f;
        }
        if let Some(rows) = v["components"].as_array() {
            self.components = rows
                .iter()
                .filter(|r| r["type"].as_u64() == Some(1))
                .map(|r| r["components"].as_array().into_iter().flatten().filter_map(Comp::parse).collect())
                .collect();
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Comp {
    Button { style: u8, label: String, emoji: Option<ReactEmoji>, custom_id: String, url: Option<String>, disabled: bool },
    Select { kind: u8, custom_id: String, placeholder: String, options: Vec<(String, String, Option<String>)>, disabled: bool },
}

impl Comp {
    fn parse(v: &Value) -> Option<Comp> {
        let disabled = v["disabled"].as_bool().unwrap_or(false);
        match v["type"].as_u64()? {
            2 => Some(Comp::Button {
                style: v["style"].as_u64().unwrap_or(2) as u8,
                label: st(&v["label"]),
                emoji: v["emoji"].is_object().then(|| ReactEmoji::parse(&v["emoji"])),
                custom_id: st(&v["custom_id"]),
                url: ost(&v["url"]),
                disabled,
            }),
            k @ (3 | 5..=8) => Some(Comp::Select {
                kind: k as u8,
                custom_id: st(&v["custom_id"]),
                placeholder: ost(&v["placeholder"]).unwrap_or_else(|| "Make a selection".into()),
                options: v["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| (st(&o["label"]), st(&o["value"]), ost(&o["description"])))
                    .collect(),
                disabled,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ReadState {
    pub last_acked: Id,
    pub mentions: u32,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Msgs {
    pub list: Vec<Message>,
    pub at_start: bool,
    #[serde(skip)]
    pub loading: bool,
    #[serde(skip)]
    pub fresh: bool,
    // after a jump into old history the list no longer reaches the newest message
    #[serde(skip)]
    pub detached: bool,
}

/// Which slice of history a fetch asks for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Latest,
    Before(Id),
    After(Id),
    Around(Id),
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Folder {
    pub id: Id,
    pub name: Option<String>,
    pub color: Option<u32>,
    pub guilds: Vec<Id>,
}

// relationship types as discord numbers them
pub const FRIEND: u8 = 1;
pub const BLOCKED: u8 = 2;
pub const INCOMING: u8 = 3;
pub const OUTGOING: u8 = 4;

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    pub me: Id,
    pub users: HashMap<Id, User>,
    pub guilds: HashMap<Id, Guild>,
    pub guild_order: Vec<Id>,
    pub channels: HashMap<Id, Channel>,
    pub dms: Vec<Id>,
    pub reads: HashMap<Id, ReadState>,
    pub members: HashMap<Id, HashMap<Id, Member>>,
    pub muted: HashSet<Id>,
    // "unreads" overrides by guild or channel id: true for all messages, false for only @mentions
    #[serde(default)]
    pub unreads: HashMap<Id, bool>,
    // the account opted into discord's newer notification settings, which is what makes the above count
    #[serde(default)]
    pub new_notifs: bool,
    // message_notifications the user picked per server or channel (3 = inherit is left out)
    #[serde(default)]
    pub levels: HashMap<Id, u8>,
    // servers where muted channels are left out of the list
    #[serde(default)]
    pub hide_muted: HashSet<Id>,
    // servers that only list the channels picked in onboarding or browse channels, and the picks
    #[serde(default)]
    pub opt_in: HashSet<Id>,
    #[serde(default)]
    pub opted: HashSet<Id>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(skip)]
    pub account: crate::proto::Account,
    #[serde(default)]
    pub custom_status: Option<String>,
    #[serde(default)]
    pub relationships: HashMap<Id, u8>,
    #[serde(default)]
    pub email: Option<String>,
    // READY's user object as-is: phone, mfa, verified, premium type, flags
    #[serde(default)]
    pub me_raw: Value,
    #[serde(skip)]
    pub forum_posts: HashMap<Id, Vec<Id>>,
    #[serde(skip)]
    pub session: String,
    #[serde(skip)]
    pub msgs: HashMap<Id, Msgs>,
    #[serde(skip)]
    pub refs: HashMap<Id, Message>,
    #[serde(skip)]
    pub presence: HashMap<Id, Status>,
    // what each user is doing, live from presences: (kind, name, state, emoji url)
    #[serde(skip)]
    pub activities: HashMap<Id, Vec<(u8, String, Option<String>, Option<String>)>>,
    #[serde(skip)]
    pub typing: HashMap<Id, Vec<(Id, Instant)>>,
}

impl Store {
    pub fn user(&self, id: Id) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn name_in(&self, guild: Id, user: Id) -> String {
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
        if g.owner == self.me {
            return u64::MAX;
        }
        let mut p = g.roles.get(&g.id).map(|r| r.perms).unwrap_or(0);
        for r in &g.my_roles {
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
        if g.owner == self.me {
            return u64::MAX;
        }
        let mut p = g.roles.get(&g.id).map(|r| r.perms).unwrap_or(0);
        for r in &g.my_roles {
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
        for r in &g.my_roles {
            if let Some(o) = c.overwrites.iter().find(|o| o.id == *r && o.kind == 0) {
                allow |= o.allow;
                deny |= o.deny;
            }
        }
        p = (p & !deny) | allow;
        if let Some(o) = c.overwrites.iter().find(|o| o.id == self.me && o.kind == 1) {
            p = (p & !o.deny) | o.allow;
        }
        p
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

    /// One user_guild_settings entry: mutes and "unreads" overrides for a server and its channels.
    /// Notification levels don't hide unreads, only these do.
    fn guild_settings(&mut self, s: &Value) {
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
        if let Some(all) = unreads(s, 2048, 4096) {
            self.unreads.insert(gid, all);
        }
        if let Some(n) = s["message_notifications"].as_u64().filter(|n| *n < 3) {
            self.levels.insert(gid, n as u8);
        }
        if s["flags"].as_u64().unwrap_or(0) & (1 << 14) != 0 {
            self.opt_in.insert(gid);
        } else {
            self.opt_in.remove(&gid);
        }
        for o in s["channel_overrides"].as_array().into_iter().flatten() {
            let cid = sid(&o["channel_id"]);
            if o["flags"].as_u64().unwrap_or(0) & (1 << 12) != 0 {
                self.opted.insert(cid);
            } else {
                self.opted.remove(&cid);
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
    fn unreads_all(&self, c: &Channel) -> bool {
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
        if c.last_msg == 0 || self.is_muted(c) || (c.is_thread() && !c.joined) {
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

    fn set_activities(&mut self, uid: Id, v: &Value) {
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
                (a["type"].as_u64().unwrap_or(0) as u8, st(&a["name"]), ost(&a["state"]), emoji)
            })
            .collect();
        if list.is_empty() {
            self.activities.remove(&uid);
        } else {
            self.activities.insert(uid, list);
        }
    }

    /// The member list's second line: the custom status if there is one, else what they're
    /// doing ("Playing", bold name). (emoji url, prefix, bold part)
    pub fn activity_line(&self, uid: Id) -> Option<(Option<String>, String, String)> {
        let list = self.activities.get(&uid)?;
        if let Some((_, _, state, emoji)) = list.iter().find(|a| a.0 == 4)
            && (state.is_some() || emoji.is_some())
        {
            return Some((emoji.clone(), state.clone().unwrap_or_default(), String::new()));
        }
        let (kind, name, _, _) = list.iter().find(|a| a.0 != 4)?;
        let verb = match kind {
            1 => "Streaming ",
            2 => "Listening to ",
            3 => "Watching ",
            5 => "Competing in ",
            _ => "Playing ",
        };
        Some((None, verb.to_string(), name.clone()))
    }

    fn upsert_user(&mut self, v: &Value) -> Id {
        let u = User::parse(v);
        let id = u.id;
        if id != 0 {
            self.users.insert(id, u);
        }
        id
    }

    fn upsert_member(&mut self, guild: Id, user: Id, v: &Value) {
        if guild == 0 || user == 0 || !v.is_object() {
            return;
        }
        let m = Member { nick: ost(&v["nick"]), roles: ids(&v["roles"]) };
        if user == self.me
            && let Some(g) = self.guilds.get_mut(&guild)
        {
            g.my_roles = m.roles.clone();
            g.my_flags = v["flags"].as_u64().unwrap_or(g.my_flags);
            if let Some(t) = v["joined_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
                g.joined = t.timestamp();
            }
        }
        self.members.entry(guild).or_default().insert(user, m);
    }

    fn put_channel(&mut self, c: Channel) {
        if c.guild != 0
            && let Some(g) = self.guilds.get_mut(&c.guild)
            && !g.channels.contains(&c.id)
        {
            g.channels.push(c.id);
        }
        self.channels.insert(c.id, c);
    }

    fn put_guild(&mut self, v: &Value) -> Id {
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
        g.default_notif = p["default_message_notifications"].as_u64().unwrap_or(0) as u8;
        g.banner = ost(&p["banner"]);
        g.boosts = p["premium_subscription_count"].as_u64().or(v["premium_subscription_count"].as_u64()).unwrap_or(0) as u32;
        g.boost_bar = p["premium_progress_bar_enabled"].as_bool().unwrap_or(false);
        g.tier = p["premium_tier"].as_u64().or(v["premium_tier"].as_u64()).unwrap_or(0) as u8;
        if let Some(e) = v["guild_scheduled_events"].as_array() {
            g.events = e.len() as u32;
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
        if let Some(list) = v["stickers"].as_array() {
            g.stickers = list.iter().map(|s| (sid(&s["id"]), st(&s["name"]), s["format_type"].as_u64().unwrap_or(1) as u8)).collect();
        }
        for vs in v["voice_states"].as_array().into_iter().flatten() {
            g.voice.insert(sid(&vs["user_id"]), sid(&vs["channel_id"]));
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
        for r in rs.as_array().into_iter().flatten() {
            if r["read_state_type"].as_u64().unwrap_or(0) != 0 {
                continue;
            }
            self.reads.insert(
                sid(&r["id"]),
                ReadState {
                    last_acked: sid(&r["last_message_id"]),
                    mentions: r["mention_count"].as_u64().unwrap_or(0) as u32,
                },
            );
        }

        let ugs = if d["user_guild_settings"].is_array() {
            &d["user_guild_settings"]
        } else {
            &d["user_guild_settings"]["entries"]
        };
        self.muted.clear();
        self.unreads.clear();
        self.levels.clear();
        self.opt_in.clear();
        self.opted.clear();
        self.new_notifs = d["notification_settings"]["flags"].as_u64().unwrap_or(0) & 16 != 0;
        for s in ugs.as_array().into_iter().flatten() {
            self.guild_settings(s);
        }

        self.email = ost(&d["user"]["email"]);
        self.me_raw = d["user"].clone();
        self.relationships.clear();
        for r in d["relationships"].as_array().into_iter().flatten() {
            let uid = sid(&r["id"]);
            if r["user"].is_object() {
                self.upsert_user(&r["user"]);
            }
            self.relationships.insert(uid, r["type"].as_u64().unwrap_or(0) as u8);
        }

        let mut folders: Vec<Folder> = Vec::new();
        self.account = Default::default();
        if let Some(b64) = d["user_settings_proto"].as_str() {
            use base64::Engine;
            if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64) {
                folders = crate::proto::guild_folders(&raw);
                self.account = crate::proto::account(&raw, true);
                self.custom_status = crate::proto::custom_status(&raw, chrono::Utc::now().timestamp_millis() as u64).flatten();
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
        let mut order = Vec::new();
        for f in &folders {
            order.extend(f.guilds.iter().copied());
        }
        for g in &seen {
            if !order.contains(g) {
                order.insert(0, *g);
            }
        }
        order.retain(|g| self.guilds.contains_key(g));
        // guilds missing from the folder list sit on top, like the official client
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
                }
            }
        }
    }

    fn sort_dms(&mut self) {
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
        m.app_id = if v["application_id"].is_null() { author } else { sid(&v["application_id"]) };
        if v["thread"].is_object() {
            self.put_channel(Channel::parse(&v["thread"], guild));
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

    fn msg_mut(&mut self, channel: Id, id: Id) -> Option<&mut Message> {
        self.msgs.get_mut(&channel)?.list.iter_mut().find(|m| m.id == id)
    }

    pub fn mentions_me(&self, m: &Message) -> bool {
        if m.mention_everyone || m.mentions.contains(&self.me) {
            return true;
        }
        let guild = self.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
        self.guilds
            .get(&guild)
            .is_some_and(|g| m.mention_roles.iter().any(|r| g.my_roles.contains(r)))
    }

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
                    if let Some(pos) = e.list.iter().position(|x| x.pending && x.nonce.is_some() && x.nonce == m.nonce) {
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
            "MESSAGE_ACK" => {
                let r = self.reads.entry(sid(&d["channel_id"])).or_default();
                r.last_acked = r.last_acked.max(sid(&d["message_id"]));
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
            "CHANNEL_DELETE" => {
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
                if ids(&d["removed_member_ids"]).contains(&self.me)
                    && let Some(c) = self.channels.get_mut(&sid(&d["id"]))
                {
                    c.joined = false;
                }
            }
            "RELATIONSHIP_ADD" | "RELATIONSHIP_UPDATE" => {
                let uid = sid(&d["id"]);
                if d["user"].is_object() {
                    self.upsert_user(&d["user"]);
                }
                self.relationships.insert(uid, d["type"].as_u64().unwrap_or(0) as u8);
            }
            "RELATIONSHIP_REMOVE" => {
                self.relationships.remove(&sid(&d["id"]));
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
                    } else {
                        g.voice.insert(uid, ch);
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
            "USER_SETTINGS_PROTO_UPDATE" if d["settings"]["type"].as_u64() == Some(1) => {
                use base64::Engine;
                let raw = d["settings"]["proto"].as_str().and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
                if let Some(raw) = raw {
                    let full = d["partial"].as_bool() == Some(false);
                    self.account.merge(crate::proto::account(&raw, full));
                    if let Some(c) = crate::proto::custom_status(&raw, chrono::Utc::now().timestamp_millis() as u64) {
                        self.custom_status = c;
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn list_item(&mut self, gid: Id, v: &Value) -> ListItem {
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

    fn member_list(&mut self, d: &Value) {
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
        let counts: HashMap<String, u32> = d["groups"]
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

    pub fn react_local(&mut self, channel: Id, id: Id, em: ReactEmoji, add: bool) {
        if let Some(m) = self.msg_mut(channel, id) {
            react(m, em, add, true);
        }
    }

    pub fn prune_typing(&mut self) -> bool {
        let mut any = false;
        for v in self.typing.values_mut() {
            v.retain(|(_, t)| t.elapsed().as_secs() < 10);
            any |= !v.is_empty();
        }
        any
    }
}

fn parse_role(r: &Value) -> Role {
    Role {
        id: sid(&r["id"]),
        name: st(&r["name"]),
        color: r["color"].as_u64().unwrap_or(0) as u32,
        position: r["position"].as_i64().unwrap_or(0) as i32,
        perms: perm(&r["permissions"]),
        hoist: r["hoist"].as_bool().unwrap_or(false),
        grad: role_grad(&r["colors"]),
        icon: ost(&r["icon"]),
        emoji: ost(&r["unicode_emoji"]),
    }
}

fn role_grad(c: &Value) -> Vec<u32> {
    let at = |k: &str| c[k].as_u64().map(|v| v as u32);
    match (at("primary_color"), at("secondary_color")) {
        (Some(a), Some(b)) => [Some(a), Some(b), at("tertiary_color")].into_iter().flatten().collect(),
        _ => Vec::new(),
    }
}

// gateway echoes our own reactions too, so `me` reactions are idempotent
fn react(m: &mut Message, em: ReactEmoji, add: bool, me: bool) {
    match m.reactions.iter().position(|r| r.emoji == em) {
        Some(i) => {
            let r = &mut m.reactions[i];
            if me && r.me == add {
                return;
            }
            if add {
                r.count += 1;
            } else {
                r.count = r.count.saturating_sub(1);
            }
            if me {
                r.me = add;
            }
            if r.count == 0 {
                m.reactions.remove(i);
            }
        }
        None if add => m.reactions.push(Reaction { emoji: em, count: 1, me }),
        None => {}
    }
}
