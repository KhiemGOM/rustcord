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
