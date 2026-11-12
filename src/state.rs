use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::api;

mod live;
mod model;
pub(crate) use model::*;
mod msg;
pub(crate) use msg::*;
mod track;
pub(crate) use track::*;
mod access;
mod ingest;
mod messages;
mod apply;
mod lists;

/// std's map and set with foldhash: SipHash over snowflakes was a fifth of every frame.
pub type Map<K, V> = HashMap<K, V, foldhash::fast::RandomState>;
pub type Set<K> = HashSet<K, foldhash::fast::RandomState>;

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
    // a snowflake's low bits count up, so two sends in the same millisecond don't share one
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ms = chrono::Utc::now().timestamp_millis() - 1_420_070_400_000;
    (((ms as u64) << 22) | (NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xfff)).to_string()
}

pub const CREATE_INVITE: u64 = 1;
pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const KICK_MEMBERS: u64 = 1 << 1;
pub const BAN_MEMBERS: u64 = 1 << 2;
pub const MODERATE_MEMBERS: u64 = 1 << 40;
pub const ADMINISTRATOR: u64 = 1 << 3;
pub const MANAGE_MESSAGES: u64 = 1 << 13;
pub const ADD_REACTIONS: u64 = 1 << 6;
pub const CREATE_PUBLIC_THREADS: u64 = 1 << 35;
pub const SEND_MESSAGES: u64 = 1 << 11;
pub const CONNECT: u64 = 1 << 20;
pub const MANAGE_CHANNELS: u64 = 1 << 4;
pub const MANAGE_ROLES: u64 = 1 << 28;
pub const SET_VOICE_CHANNEL_STATUS: u64 = 1 << 48;
pub const MANAGE_GUILD: u64 = 1 << 5;
pub const MANAGE_THREADS: u64 = 1 << 34;
pub const MANAGE_WEBHOOKS: u64 = 1 << 29;
pub const SEND_IN_THREADS: u64 = 1 << 38;
pub const BYPASS_SLOWMODE: u64 = 1 << 52;
pub const MUTE_MEMBERS: u64 = 1 << 22;
pub const DEAFEN_MEMBERS: u64 = 1 << 23;
pub const MOVE_MEMBERS: u64 = 1 << 24;
pub const MANAGE_NICKNAMES: u64 = 1 << 27;
pub const VIEW_GUILD_INSIGHTS: u64 = 1 << 19;
pub const MANAGE_EVENTS: u64 = 1 << 33;
pub const CREATE_EVENTS: u64 = 1 << 44;
pub const PIN_MESSAGES: u64 = 1 << 51;
pub const READ_MESSAGE_HISTORY: u64 = 1 << 16;
pub const VIEW_AUDIT_LOG: u64 = 1 << 7;
pub const MANAGE_GUILD_EXPRESSIONS: u64 = 1 << 30;
pub const CREATE_GUILD_EXPRESSIONS: u64 = 1 << 43;
pub const SETTINGS_ACCESS: u64 = MANAGE_GUILD | MANAGE_ROLES | ADMINISTRATOR | BAN_MEMBERS | MANAGE_NICKNAMES | CREATE_GUILD_EXPRESSIONS | MANAGE_GUILD_EXPRESSIONS | MANAGE_WEBHOOKS | VIEW_AUDIT_LOG | VIEW_GUILD_INSIGHTS;

#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    pub me: Id,
    /// View Server as Role: the server and the roles its channels and permissions are worked out from instead of ours
    #[serde(skip)]
    pub viewing: Option<(Id, Vec<Id>)>,
    pub users: Map<Id, User>,
    pub guilds: Map<Id, Guild>,
    pub guild_order: Vec<Id>,
    pub channels: Map<Id, Channel>,
    pub dms: Vec<Id>,
    pub reads: Map<Id, ReadState>,
    pub members: Map<Id, Map<Id, Member>>,
    pub muted: Set<Id>,
    // "unreads" overrides by guild or channel id: true for all messages, false for only @mentions
    #[serde(default)]
    pub unreads: Map<Id, bool>,
    // the account opted into discord's newer notification settings, which is what makes the above count
    #[serde(default)]
    pub new_notifs: bool,
    // message_notifications the user picked per server or channel (3 = inherit is left out)
    #[serde(default)]
    pub levels: Map<Id, u8>,
    // servers where muted channels are left out of the list
    #[serde(default)]
    pub hide_muted: Set<Id>,
    // servers whose new events are muted, and how far you've read their event notifications
    #[serde(default)]
    pub events_muted: Set<Id>,
    #[serde(default)]
    pub gprefs: Map<Id, GuildPrefs>,
    #[serde(default)]
    pub event_reads: Map<Id, ReadState>,
    // servers that only list the channels picked in onboarding or browse channels, and the picks
    #[serde(default)]
    pub opt_in: Set<Id>,
    #[serde(default)]
    pub opted: Set<Id>,
    /// Categories folded shut, which discord keeps on the account, not on the machine.
    #[serde(default)]
    pub collapsed: Set<Id>,
    // the flags of each channel override: 2048 pinned to the top, 4096 opted in
    #[serde(default)]
    pub chan_flags: Map<Id, u64>,
    // the servers each privacy list names, by settings field: 3 dms off, 13 activity status off,
    // 15 activity joining off, 16 message requests off
    #[serde(skip)]
    pub privacy: Map<u64, Set<Id>>,
    #[serde(skip)]
    pub lurking: Set<Id>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(skip)]
    pub account: crate::proto::Account,
    #[serde(default)]
    pub favorites: crate::proto::favorites::Favorites,
    // who is in each thread, from the gateway's thread member lists
    #[serde(skip)]
    pub thread_members: Map<Id, Vec<Id>>,
    #[serde(default)]
    pub custom_status: Option<String>,
    #[serde(default)]
    pub custom_emoji: Option<ReactEmoji>,
    // activity sharing turned off: discord's show_current_game
    #[serde(skip)]
    pub hide_game: bool,
    // voice channels' statuses and when their calls started (unix seconds), from op 43
    #[serde(skip)]
    pub voice_status: Map<Id, String>,
    #[serde(skip)]
    pub voice_start: Map<Id, i64>,
    #[serde(default)]
    pub relationships: Map<Id, u8>,
    // when each relationship began, from the relationship's `since`
    #[serde(default)]
    pub rel_since: Map<Id, i64>,
    // what you call a friend, and the people you ignore
    #[serde(default)]
    pub nicks: Map<Id, String>,
    #[serde(default)]
    pub ignored: Set<Id>,
    // dms pinned to the top of the list
    #[serde(default)]
    pub pinned: Set<Id>,
    #[serde(default)]
    pub email: Option<String>,
    // READY's user object as-is: phone, mfa, verified, premium type, flags
    #[serde(default)]
    pub me_raw: Value,
    #[serde(skip)]
    pub forum_posts: Map<Id, Vec<Id>>,
    #[serde(skip)]
    pub session: String,
    #[serde(skip)]
    pub embedded: Map<Id, Vec<Instance>>,
    #[serde(skip)]
    pub msgs: Map<Id, Msgs>,
    #[serde(skip)]
    pub refs: Map<Id, Message>,
    // who reacted, by message, emoji and whether super: the count it was asked at and the users once they arrive
    #[serde(skip)]
    pub reactors: Map<(Id, String, bool), (u32, Option<Vec<Id>>)>,
    #[serde(skip)]
    pub presence: Map<Id, Status>,
    // what each user is doing, live from presences
    #[serde(skip)]
    pub activities: Map<Id, Vec<Activity>>,
    #[serde(skip)]
    pub typing: Map<Id, Vec<(Id, Instant)>>,
    // the activity feed, best first, and application icons by id (None once looked up and missing)
    #[serde(skip)]
    pub feed: Vec<FeedEntry>,
    #[serde(skip)]
    pub app_icons: Map<Id, Option<String>>,
    // names of the games mentioned with @game, as the autocomplete gave them
    #[serde(skip)]
    pub game_names: Map<Id, String>,
}

/// A server's own theme: a preset, or one colour (a theme with more is ignored, as the client does).
fn guild_theme(t: &Value) -> Option<crate::theme::Fill> {
    use crate::theme::{Custom, Fill};
    if t["enabled"].as_bool() != Some(true) {
        return None;
    }
    let c = &t["custom_user_theme_settings"];
    if let Some([col]) = c["colors"].as_array().map(Vec::as_slice)
        && c["gradient_color_stops"].as_array().is_none_or(|s| s.is_empty())
    {
        let rgb = u32::from_str_radix(col.as_str()?.trim_start_matches('#'), 16).ok()?;
        return Some(Fill::Custom(Custom { colors: vec![rgb], angle: c["gradient_angle"].as_i64().unwrap_or(0) as i32, mix: c["base_mix"].as_i64().unwrap_or(0) as i32, light: false }));
    }
    Fill::from_id(t["preset_id"].as_u64()? as u32)
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
        mentionable: r["mentionable"].as_bool().unwrap_or(false),
        managed: r["managed"].as_bool().unwrap_or(false),
    }
}

/// An ISO timestamp as unix ms, 0 when there isn't one.
pub fn iso_ms(v: &Value) -> i64 {
    v.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(0, |t| t.timestamp_millis())
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
        None if add => m.reactions.push(Reaction { emoji: em, count: 1, me, ..Default::default() }),
        None => {}
    }
}
