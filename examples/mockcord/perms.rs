//! A channel's permission overwrites, one request each: PUT sets or replaces one, DELETE takes
//! one away. Each answers 204 and tells the gateway the channel changed.

use super::{W, channel};
use serde_json::{Value, json};

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

pub fn route(w: &W, method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let ("PUT" | "DELETE", ["api", "v9", "channels", ch, "permissions", target]) = (method, parts) else { return None };
    let Some(mut c) = channel(w, ch) else { return Some((404, json!({ "message": "Unknown Channel", "code": 10003 }))) };
    if target.parse::<u64>().is_err() {
        return Some(bad("overwrite_id", "Snowflake type should be a string of digits."));
    }
    let mut list: Vec<Value> = c["permission_overwrites"].as_array().cloned().unwrap_or_default();
    list.retain(|o| o["id"] != *target);
    if method == "PUT" {
        let b: Value = serde_json::from_slice(body).unwrap_or_default();
        if b["id"].as_str().is_some_and(|id| id != *target) {
            return Some(bad("id", "The id must be the overwrite in the path."));
        }
        if !b["type"].as_u64().is_some_and(|t| t <= 1) {
            return Some(bad("type", "Value must be one of (0, 1)."));
        }
        for k in ["allow", "deny"] {
            if !b[k].as_str().is_some_and(|s| s.parse::<u64>().is_ok()) {
                return Some(bad(k, "Permissions are a string of digits."));
            }
        }
        list.push(json!({ "id": target, "type": b["type"], "allow": b["allow"], "deny": b["deny"] }));
    }
    c["permission_overwrites"] = json!(list);
    let mut wl = w.lock().unwrap();
    wl.edits.insert(ch.to_string(), c.clone());
    let _ = wl.events.send(json!({ "t": "CHANNEL_UPDATE", "d": c }));
    Some((204, Value::Null))
}
