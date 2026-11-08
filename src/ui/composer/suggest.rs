use super::*;

/// Application commands for a guild (or DM), as returned by the command index.
#[derive(Default)]
pub(crate) struct Commands {
    pub apps: Vec<(Id, String, Option<String>)>,
    pub list: Vec<Value>,
}

pub(super) enum Item {
    User(Id, String, String),
    Role(Id, String, u32),
    Everyone(&'static str),
    AtTime,
    AtGame,
    Game(Id, String, Option<String>),
    When(i64, char),
    Channel(Id, String),
    Emoji(Em),
    Command(usize, Vec<usize>),
    BuiltIn(usize),
    Sticker(Id, String, u8, String),
}

/// Discord's own commands, listed after the apps' as "Built-In": name, options, description.
pub(super) const BUILT_IN: [(&str, &str, &str); 15] = [
    ("shrug", "message", "Appends ¯\\_(ツ)_/¯ to your message."),
    ("tableflip", "message", "Appends (╯°□°)╯︵ ┻━┻ to your message."),
    ("unflip", "message", "Appends ┬─┬ ノ( ゜-゜ノ) to your message."),
    ("tts", "message", "Use text-to-speech to read the message to all members currently viewing the channel."),
    ("me", "message", "Displays text with emphasis."),
    ("spoiler", "message", "Marks your message as a spoiler."),
    ("gif", "query", "Search Animated GIFs on the Web"),
    ("sticker", "query", "Search your stickers"),
    ("nick", "new_nick", "Change nickname on this server."),
    ("thread", "name message", "Start new thread"),
    ("kick", "user reason", "Kick user"),
    ("ban", "user delete_messages reason", "Ban user"),
    ("timeout", "user duration reason", "Timeout user"),
    ("msg", "user message", "Message user"),
    ("leave", "silent", "Leave Group"),
];

/// A built-in as the application command it is in discord, so it gets the same pills.
pub(super) fn built_in_command(i: usize) -> Value {
    let (name, opts, desc) = BUILT_IN[i];
    let options: Vec<Value> = opts
        .split(' ')
        .map(|o| {
            let kind = match o {
                "user" => 6,
                "duration" | "delete_messages" => 4,
                "silent" => 5,
                _ => 3,
            };
            let required = matches!((name, o), ("me" | "spoiler" | "tts" | "thread" | "msg", _) | (_, "user" | "duration"));
            json!({ "name": o, "type": kind, "required": required, "description": "" })
        })
        .collect();
    json!({ "name": name, "type": 1, "description": desc, "options": options, "builtin": true })
}

/// Whether a built-in command makes sense here, as discord's predicates decide.
pub(super) fn built_in_here(app: &App, chan: &Channel, name: &str) -> bool {
    use crate::state::{BAN_MEMBERS, KICK_MEMBERS, MODERATE_MEMBERS};
    let perms = app.store.perms(chan);
    match name {
        "tts" => chan.guild != 0 && perms & (1 << 12) != 0,
        "nick" => chan.guild != 0,
        "thread" => chan.guild != 0 && matches!(chan.kind, 0 | 5),
        "kick" => chan.guild != 0 && perms & KICK_MEMBERS != 0,
        "ban" => chan.guild != 0 && perms & BAN_MEMBERS != 0,
        "timeout" => chan.guild != 0 && perms & MODERATE_MEMBERS != 0,
        "leave" => chan.kind == 3,
        _ => true,
    }
}

pub(super) struct Em {
    pub(super) name: String,
    // what goes into the message: `<:name:id>`, or the glyph
    pub(super) raw: String,
    pub(super) url: Option<String>,
    pub(super) server: Option<String>,
    pub(super) locked: bool,
}

/// The word the cursor sits in, when it's one autocomplete cares about.
pub(super) fn token(draft: &str, cursor: usize) -> Option<(usize, char, String)> {
    let bytes: Vec<(usize, char)> = draft.char_indices().collect();
    let cur_b = bytes.get(cursor).map(|(b, _)| *b).unwrap_or(draft.len());
    let before = &draft[..cur_b];
    if let Some(i) = before.rfind("@time ")
        && before[..i].chars().next_back().is_none_or(char::is_whitespace)
        && !before[i..].contains('\n')
    {
        return Some((i, 'T', before[i + 6..].to_string()));
    }
    if let Some(i) = before.rfind("@game ")
        && before[..i].chars().next_back().is_none_or(char::is_whitespace)
        && !before[i..].contains('\n')
    {
        return Some((i, 'G', before[i + 6..].to_string()));
    }
    let start = before.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    let word = &before[start..];
    let first = word.chars().next()?;
    match first {
        '@' | '#' => Some((start, first, word[1..].to_string())),
        ':' if word.chars().count() >= 3 && !word[1..].contains(':') => Some((start, ':', word[1..].to_string())),
        '/' if start == 0 => Some((0, '/', word[1..].to_string())),
        _ => None,
    }
}

pub(super) fn score(hay: &str, q: &str) -> Option<u8> {
    let h = hay.to_lowercase();
    if h.starts_with(q) {
        Some(0)
    } else if h.contains(q) {
        Some(1)
    } else {
        None
    }
}

pub(super) fn candidates(app: &App, chan: &Channel, kind: char, q: &str) -> Vec<Item> {
    let q = q.to_lowercase();
    let store = &app.store;
    let mut out: Vec<(u16, Item)> = Vec::new();
    match kind {
        '@' => {
            let mut ids: Vec<Id> = if chan.guild == 0 {
                chan.recipients.clone()
            } else {
                store.members.get(&chan.guild).map(|m| m.keys().copied().collect()).unwrap_or_default()
            };
            ids.sort();
            ids.dedup();
            for u in ids {
                let Some(user) = store.user(u) else { continue };
                let name = store.name_in(chan.guild, u);
                if let Some(s) = score(&name, &q).or_else(|| score(&user.username, &q)) {
                    out.push((s as u16, Item::User(u, name, user.username.clone())));
                }
            }
            if app.settings.game_suggestions && "game".starts_with(&q) {
                out.push((2, Item::AtGame));
            }
            if "time".starts_with(&q) {
                out.push((2, Item::AtTime));
            }
            if chan.guild != 0 {
                for e in ["everyone", "here"] {
                    if e.starts_with(&q) {
                        out.push((2, Item::Everyone(e)));
                    }
                }
                if let Some(g) = store.guilds.get(&chan.guild) {
                    for r in g.roles.values().filter(|r| r.id != g.id) {
                        if let Some(s) = score(&r.name, &q) {
                            out.push(((s + 2) as u16, Item::Role(r.id, r.name.clone(), r.color)));
                        }
                    }
                }
            }
        }
        'G' if app.settings.game_suggestions => {
            let q = game_query(&q);
            let q = q.as_str();
            let hits = (1..=q.len()).rev().filter(|n| q.is_char_boundary(*n)).find_map(|n| match app.keyed.get(&format!("games:{}", &q[..n])) {
                Some(Some(Ok(v))) => Some(v.clone()),
                _ => None,
            });
            for g in hits.iter().flat_map(|v| v.as_array()).flatten() {
                out.push((0, Item::Game(sid(&g["id"]), g["name"].as_str().unwrap_or_default().to_string(), g["icon"].as_str().map(str::to_string))));
            }
        }
        'T' => {
            if let Some(t) = crate::ui::when::parse(&q, chrono::Local::now()) {
                out.extend(crate::ui::when::STYLES.iter().map(|(st, _)| (0, Item::When(t.timestamp(), *st))));
            }
        }
        '#' => {
            if let Some(g) = store.guilds.get(&chan.guild) {
                for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
                    if c.kind == 4 || c.is_thread() || !store.can_view(c) {
                        continue;
                    }
                    if let Some(s) = score(&c.name, &q) {
                        out.push((s as u16, Item::Channel(c.id, c.name.clone())));
                    }
                }
            }
        }
        ':' => {
            // current server first, then prefix hits anywhere, then substring hits; locked ones trail their group
            let rank = |name: &str, home: bool, locked: bool, unicode: bool| {
                let m = if name.eq_ignore_ascii_case(&q) { 0 } else { score(name, &q)? + 1 };
                let tier = if home { 0 } else if m < 2 { 1 } else { 2 };
                Some(tier * 100 + locked as u16 * 30 + m as u16 * 10 + unicode as u16)
            };
            for (g, e, ok) in store.emojis_for(chan.guild) {
                if let Some(k) = rank(&e.name, g.id == chan.guild, !ok, false) {
                    let raw = format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id);
                    out.push((k, Item::Emoji(Em { name: e.name.clone(), raw, url: Some(e.url()), server: Some(g.name.clone()), locked: !ok })));
                }
            }
            if app.settings.sticker_suggestions && q.chars().count() >= 2 {
                let (here, nitro) = (chan.guild, store.nitro());
                for g in store.guild_order.iter().filter_map(|g| store.guilds.get(g)).filter(|g| nitro || g.id == here) {
                    for (id, name, fmt) in g.stickers.iter().filter(|s| s.2 != 3) {
                        if let Some(s) = score(name, &q) {
                            out.push((s as u16 * 10, Item::Sticker(*id, name.clone(), *fmt, g.name.clone())));
                        }
                    }
                }
            }
            if app.settings.sticker_suggestions && q.chars().count() >= 2 {
                for p in &app.packs {
                    for (id, name, fmt) in p.stickers.iter().filter(|s| s.2 != 3) {
                        if let Some(s) = score(name, &q) {
                            out.push((s as u16 * 10 + 5, Item::Sticker(*id, name.clone(), *fmt, p.name.clone())));
                        }
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            for (glyph, name) in crate::ui::picker::common() {
                if let Some(k) = rank(&name, false, false, true)
                    && seen.insert(glyph)
                {
                    out.push((k, Item::Emoji(Em { name, raw: glyph.to_string(), url: None, server: None, locked: false })));
                }
            }
        }
        '/' => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            if let Some(cmds) = app.commands.get(&scope).filter(|_| !app.settings.legacy_input) {
                for (i, c) in cmds.list.iter().enumerate() {
                    if c["type"].as_u64().unwrap_or(1) != 1 {
                        continue;
                    }
                    for path in slash::forms(c) {
                        if let Some(s) = score(&slash::form_name(c, &path), &q) {
                            out.push((s as u16, Item::Command(i, path)));
                        }
                    }
                }
            }
            // after the apps' own, so an app's /me still comes first as in discord
            for (i, (name, ..)) in BUILT_IN.iter().enumerate() {
                if built_in_here(app, chan, name)
                    && let Some(s) = score(name, &q)
                {
                    out.push((s as u16 + 1, Item::BuiltIn(i)));
                }
            }
        }
        _ => {}
    }
    out.sort_by_key(|(s, _)| *s);
    out.into_iter().map(|(_, i)| i).take(if kind == '/' { 25 } else { 10 }).collect()
}
