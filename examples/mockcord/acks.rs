//! Read acks the way discord takes them: each answer hands out a token, the next ack sends the
//! newest one back, and Mark Unread sends the manual form with the mentions it counted.

use serde_json::{Value, json};
use std::sync::Mutex;

static ISSUED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let ("POST", ["api", "v9", "channels", ch, "messages", msg, "ack"]) = (method, parts) else { return None };
    if ch.parse::<u64>().is_err() || msg.parse::<u64>().is_err() {
        return Some(bad("message_id", "Snowflake type should be a string of digits."));
    }
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    if b["manual"] == json!(true) {
        if !b["mention_count"].is_u64() {
            return Some(bad("mention_count", "A manual ack says how many mentions it leaves unread."));
        }
        if b.get("token").is_some_and(|t| !t.is_null()) {
            return Some(bad("token", "A manual ack carries no token."));
        }
        return Some((200, json!({ "token": null })));
    }
    let mut issued = ISSUED.lock().unwrap();
    if let Some(t) = b["token"].as_str()
        && !issued.iter().any(|i| i == t)
    {
        return Some(bad("token", "Invalid ack token."));
    }
    if b.get("last_viewed").is_some_and(|d| d.as_u64().is_none_or(|d| d == 0)) {
        return Some(bad("last_viewed", "Must be a day number since the Discord epoch."));
    }
    if b.get("flags").is_some_and(|f| f.as_u64().is_none_or(|f| f > 3)) {
        return Some(bad("flags", "Must be between 0 and 3."));
    }
    let token = format!("ack-{}", issued.len() + 1);
    issued.push(token.clone());
    Some((200, json!({ "token": token })))
}
