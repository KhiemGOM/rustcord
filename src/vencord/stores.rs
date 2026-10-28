//! What Vencord's stores read: Fastcord's own store, shaped like Discord's API objects. The JS
//! side wraps these in records with Discord's methods.

use serde_json::{Value, json};

use crate::state::{Id, Message, Status, Store, User};

/// The store and what's open, for the duration of one call into the plugins.
pub struct View<'a> {
    pub store: &'a Store,
    pub channel: Id,
    pub guild: Id,
    pub voice: Id,
}

fn s(id: Id) -> Value {
    if id == 0 { Value::Null } else { id.to_string().into() }
}

fn user(u: &User) -> Value {
    json!({ "id": u.id.to_string(), "username": u.username, "globalName": u.global_name, "avatar": u.avatar, "bot": u.bot, "discriminator": "0" })
}

pub fn message(store: &Store, m: &Message) -> Value {
    let guild = store.channels.get(&m.channel).map_or(0, |c| c.guild);
    let author = store.user(m.author).map(user).unwrap_or_else(|| json!({ "id": m.author.to_string(), "username": "unknown-user", "discriminator": "0" }));
    json!({
        "id": m.id.to_string(),
        "channel_id": m.channel.to_string(),
        "guild_id": s(guild),
        "author": author,
        "content": m.content,
        "type": m.kind,
        "flags": m.flags,
        "timestamp": chrono::DateTime::from_timestamp_millis(m.ts()).map(|t| t.to_rfc3339()),
        "edited": m.edited,
        "attachments": m.attachments.iter().map(|a| json!({ "url": a.url, "proxy_url": a.proxy, "filename": a.filename, "size": a.size, "width": a.w, "height": a.h, "content_type": a.ctype })).collect::<Vec<_>>(),
        "embeds": m.embeds.iter().map(|e| json!({ "type": e.kind, "title": e.title, "url": e.url, "description": e.desc, "color": e.color })).collect::<Vec<_>>(),
        "mentions": m.mentions.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
        "mention_roles": m.mention_roles.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
        "mention_everyone": m.mention_everyone,
        "message_reference": m.reply.map(|r| json!({ "message_id": r.to_string(), "channel_id": m.channel.to_string(), "guild_id": s(guild) })),
        "pinned": m.pinned,
    })
}

fn status(st: Status) -> &'static str {
    match st {
        Status::Online => "online",
        Status::Idle => "idle",
        Status::Dnd => "dnd",
        Status::Offline => "offline",
    }
}

pub fn query(v: &View, kind: &str, a: &str, b: &str) -> Value {
    let st = v.store;
    let id = |x: &str| x.parse::<Id>().unwrap_or(0);
    match kind {
        "me" => st.user(st.me).map(user).unwrap_or(Value::Null),
        "user" => st.user(id(a)).map(user).unwrap_or(Value::Null),
        "selected" => json!({ "channel": s(v.channel), "guild": s(v.guild), "voice": s(v.voice) }),
        "channel" => st.channels.get(&id(a)).map_or(Value::Null, |c| {
            json!({
                "id": c.id.to_string(), "guild_id": s(c.guild), "type": c.kind, "name": c.name, "topic": c.topic,
                "parent_id": s(c.parent), "nsfw": c.nsfw, "owner_id": s(c.owner), "position": c.position,
                "recipients": c.recipients.iter().map(|r| r.to_string()).collect::<Vec<_>>(), "last_message_id": s(c.last_msg),
            })
        }),
        "private" => json!(st.dms.iter().map(|c| c.to_string()).collect::<Vec<_>>()),
        "guild" => st.guilds.get(&id(a)).map_or(Value::Null, |g| {
            json!({ "id": g.id.to_string(), "name": g.name, "icon": g.icon, "ownerId": g.owner.to_string(), "features": g.features, "memberCount": g.member_count, "banner": g.banner, "premiumTier": g.tier })
        }),
        "guilds" => json!(st.guild_order.iter().map(|g| g.to_string()).collect::<Vec<_>>()),
        "channels" => json!(st.guilds.get(&id(a)).map(|g| g.channels.iter().map(|c| c.to_string()).collect::<Vec<_>>()).unwrap_or_default()),
        "roles" => st.guilds.get(&id(a)).map_or(json!({}), |g| {
            g.roles.values().map(|r| (r.id.to_string(), json!({ "id": r.id.to_string(), "name": r.name, "color": r.color, "position": r.position, "permissions": r.perms.to_string(), "hoist": r.hoist, "icon": r.icon }))).collect::<serde_json::Map<_, _>>().into()
        }),
        "member" => st.members.get(&id(a)).and_then(|m| m.get(&id(b))).map_or(Value::Null, |m| {
            json!({ "userId": b, "guildId": a, "nick": m.nick, "roles": m.roles.iter().map(|r| r.to_string()).collect::<Vec<_>>() })
        }),
        "messages" => json!(st.msgs.get(&id(a)).map(|e| e.list.iter().map(|m| message(st, m)).collect::<Vec<_>>()).unwrap_or_default()),
        "message" => st.find_msg(id(a), id(b)).map_or(Value::Null, |m| message(st, m)),
        "relationships" => st.relationships.iter().map(|(u, k)| (u.to_string(), json!(k))).collect::<serde_json::Map<_, _>>().into(),
        "presence" => json!({
            "status": status(st.presence.get(&id(a)).copied().unwrap_or_default()),
            "activities": st.activities.get(&id(a)).map(|v| v.iter().map(|(k, n, s, _)| json!({ "type": k, "name": n, "state": s })).collect::<Vec<_>>()).unwrap_or_default(),
        }),
        "voice" => {
            let ch = id(a);
            let gid = st.channels.get(&ch).map_or(0, |c| c.guild);
            json!(st.guilds.get(&gid).map(|g| g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, c)| json!({ "userId": u.to_string(), "channelId": c.to_string(), "guildId": gid.to_string() })).collect::<Vec<_>>()).unwrap_or_default())
        }
        "voiceOf" => {
            let u = id(a);
            st.guilds.values().find_map(|g| g.voice.get(&u).map(|c| json!({ "userId": a, "channelId": c.to_string(), "guildId": g.id.to_string() }))).unwrap_or(Value::Null)
        }
        "unread" => {
            let ch = id(a);
            let last = st.channels.get(&ch).map_or(0, |c| c.last_msg);
            let r = st.reads.get(&ch);
            json!({ "unread": r.is_some_and(|r| r.last_acked < last), "mentions": r.map_or(0, |r| r.mentions), "lastAcked": s(r.map_or(0, |r| r.last_acked)) })
        }
        _ => Value::Null,
    }
}
