//! Per-server notification settings: what a PATCH to the guild's settings may carry, and what a
//! later read gets back.

use serde_json::{Map, Value, json};
use std::sync::Mutex;

static SAVED: Mutex<Vec<(String, Map<String, Value>)>> = Mutex::new(Vec::new());

const BOOLS: [&str; 6] = ["muted", "suppress_everyone", "suppress_roles", "mute_scheduled_events", "mobile_push", "hide_muted_channels"];

fn bad(msg: &str) -> (u16, Value) {
    (400, json!({ "code": 50035, "message": "Invalid Form Body", "errors": { "_errors": [{ "message": msg }] } }))
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let ["api", "v9", "users", "@me", "guilds", gid, "settings"] = parts else { return None };
    if method != "PATCH" {
        return None;
    }
    let Ok(Value::Object(b)) = serde_json::from_slice::<Value>(body) else { return Some(bad("body must be an object")) };
    for k in BOOLS {
        if b.get(k).is_some_and(|v| !v.is_boolean()) {
            return Some(bad(&format!("{k} must be a boolean")));
        }
    }
    if b.get("message_notifications").is_some_and(|v| !v.as_u64().is_some_and(|n| n <= 3)) {
        return Some(bad("message_notifications must be 0, 1, 2 or 3"));
    }
    if b.get("notify_highlights").is_some_and(|v| !v.as_u64().is_some_and(|n| n <= 2)) {
        return Some(bad("notify_highlights must be 0, 1 or 2"));
    }
    if let Some(Value::Object(o)) = b.get("channel_overrides") {
        for (id, v) in o {
            if id.parse::<u64>().is_err() || !v.is_object() {
                return Some(bad("channel_overrides must map channel ids to objects"));
            }
        }
    } else if b.contains_key("channel_overrides") {
        return Some(bad("channel_overrides must be an object"));
    }
    let mut all = SAVED.lock().unwrap();
    let at = match all.iter().position(|(g, _)| g == gid) {
        Some(i) => i,
        None => {
            all.push((gid.to_string(), Map::new()));
            all.len() - 1
        }
    };
    let saved = &mut all[at].1;
    for (k, v) in b {
        if k == "channel_overrides" {
            let Value::Object(new) = v else { continue };
            let Value::Object(cur) = saved.entry(k).or_insert_with(|| json!({})) else { continue };
            for (c, o) in new {
                if let (Some(Value::Object(old)), Value::Object(o)) = (cur.get_mut(&c), &o) {
                    old.extend(o.clone());
                } else {
                    cur.insert(c, o);
                }
            }
        } else {
            saved.insert(k, v);
        }
    }
    let mut out = saved.clone();
    out.insert("guild_id".into(), json!(gid));
    Some((200, Value::Object(out)))
}

const PEOPLE: [&str; 6] = ["alice", "bob_builds", "carol", "dyno", "eve", "frank"];

pub fn friend_request(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let ("POST", ["api", "v9", "users", "@me", "relationships"]) = (method, parts) else { return None };
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let Some(name) = b["username"].as_str().filter(|n| !n.trim().is_empty()) else { return Some(bad("username is required")) };
    if !b["discriminator"].is_null() && !b["discriminator"].is_string() {
        return Some(bad("discriminator must be a string or null"));
    }
    if PEOPLE.contains(&name.to_lowercase().as_str()) {
        Some((204, Value::Null))
    } else {
        Some((400, json!({ "code": 80004, "message": "No users with DiscordTag exist" })))
    }
}

pub fn install(method: &str, parts: &[&str], path: &str, body: &[u8]) -> Option<(u16, Value)> {
    let ["api", "v9", "oauth2", "authorize"] = parts else { return None };
    let q: Vec<(String, String)> = path.split_once('?').map(|(_, q)| q).unwrap_or_default().split('&').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_string(), v.replace("%20", " ").replace('+', " "))).collect();
    let get = |k: &str| q.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let (name, cid) = match get("client_id")? {
        "9101" => ("Rusty Racers", "9101"),
        "9102" => ("Crab Chess", "9102"),
        _ => return None,
    };
    let bot = get("scope").is_some_and(|s| s.split(' ').any(|x| x == "bot"));
    if method == "GET" {
        let mut v = json!({ "application": { "id": cid, "name": name, "icon": null, "description": "a mock app" }, "authorized": false });
        if bot {
            v["bot"] = json!({ "id": cid, "username": name, "bot": true });
            v["guilds"] = json!([{ "id": "1000", "name": "Rust Lounge", "permissions": "32" }, { "id": "1001", "name": "Gamers United", "permissions": "0" }]);
        }
        return Some((200, v));
    }
    if method != "POST" {
        return None;
    }
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    if b["authorize"] != json!(true) || !b["permissions"].is_string() {
        return Some(bad("authorize must be true and permissions a string"));
    }
    if bot && b["guild_id"] != json!("1000") {
        return Some(bad("guild_id must be a server you manage"));
    }
    if !bot && b["integration_type"] != json!(1) {
        return Some(bad("integration_type must be 1 to install to an account"));
    }
    Some((200, json!({ "location": "/oauth2/authorized" })))
}

pub fn guide(method: &str, parts: &[&str]) -> Option<(u16, Value)> {
    match (method, parts) {
        ("GET", ["api", "v9", "guilds", "1001", "new-member-welcome"]) => Some((
            200,
            json!({
                "guild_id": "1001", "enabled": true,
                "welcome_message": { "author_ids": [], "message": "Welcome to Gamers United! Here is how to get going." },
                "new_member_actions": [
                    { "channel_id": "2100", "action_type": 0, "title": "Say hi in the lobby", "description": "Tell us what you play.", "emoji_name": "👋" },
                    { "channel_id": "2102", "action_type": 1, "title": "Join the hangout", "description": "Hop in a call.", "emoji_name": "🎧" },
                ],
                "resource_channels": [{ "channel_id": "2100", "title": "Read the rules", "description": "Be kind to each other.", "emoji_name": "📜" }],
            }),
        )),
        ("GET", ["api", "v9", "guilds", "1001", "new-member-actions"]) => Some((200, json!({ "channel_actions": { "2100": { "completed": true } } }))),
        _ => None,
    }
}

pub fn discovery(method: &str, parts: &[&str]) -> Option<(u16, Value)> {
    let ("GET", ["api", "v9", "guilds", gid, "discovery-requirements"]) = (method, parts) else { return None };
    Some((200, json!({ "guild_id": gid, "safe_environment": true, "healthy": true, "health_score_pending": false, "size": 12, "minimum_size": 1000, "age": true, "sufficient": false, "sufficient_without_grace_period": false, "valid_rules_channel": true })))
}
