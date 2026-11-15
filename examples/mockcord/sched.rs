//! Scheduled messages as discord keeps them: a time ten minutes to eight days out, the message
//! held until then, and the list that the inbox reads.

use serde_json::{Value, json};
use std::sync::Mutex;

static HELD: Mutex<Vec<Value>> = Mutex::new(Vec::new());

pub const EXPERIMENT: u32 = 1950700659;

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

fn when(v: &Value, now_ms: u64) -> Result<String, (u16, Value)> {
    let Some(s) = v.as_str() else { return Err(bad("scheduled_timestamp", "This field is required")) };
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) else { return Err(bad("scheduled_timestamp", "Not a timestamp.")) };
    let ahead = t.timestamp_millis() - now_ms as i64;
    if ahead < 600_000 - 5_000 {
        return Err((400, json!({ "message": "Scheduled time is too soon", "code": 50035 })));
    }
    if ahead > 691_200_000 {
        return Err((400, json!({ "message": "Scheduled time is too far away", "code": 50035 })));
    }
    Ok(s.to_string())
}

pub fn route(method: &str, parts: &[&str], body: &[u8], me: u64, now_ms: u64) -> Option<(u16, Value)> {
    let ["api", "v9", "users", "@me", "scheduled-messages", rest @ ..] = parts else { return None };
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut held = HELD.lock().unwrap();
    Some(match (method, rest) {
        ("GET", []) => (200, Value::Array(held.clone())),
        ("POST", []) => {
            if b["channel_id"].as_str().is_none_or(|c| c.parse::<u64>().is_err()) {
                return Some(bad("channel_id", "Snowflake type should be a string of digits."));
            }
            if !b["attachments"].is_array() {
                return Some(bad("attachments", "This field is required"));
            }
            if b["content"].as_str().is_none_or(str::is_empty) && b["poll"].is_null() && b["attachments"].as_array().is_none_or(Vec::is_empty) {
                return Some((400, json!({ "message": "Cannot send an empty message", "code": 50006 })));
            }
            let at = match when(&b["scheduled_timestamp"], now_ms) {
                Ok(at) => at,
                Err(e) => return Some(e),
            };
            if held.len() >= 5 {
                return Some((400, json!({ "message": "Maximum number of scheduled messages reached.", "code": 30083 })));
            }
            let id = (now_ms << 22) | held.len() as u64;
            let item = json!({
                "user_id": me.to_string(), "scheduled_message_id": id.to_string(), "send_at_timestamp": at, "state": 0,
                "create_args": { "channel_id": b["channel_id"], "content": b["content"], "type": 0, "flags": b["flags"], "message_reference": b["message_reference"] },
                "attachment_uploads": b["attachments"],
                "message_preview": { "id": id.to_string(), "channel_id": b["channel_id"], "content": b["content"], "type": 0, "flags": b["flags"], "author": { "id": me.to_string(), "username": "you" }, "poll": b["poll"], "attachments": [], "embeds": [], "mentions": [] },
            });
            held.push(item.clone());
            (200, item)
        }
        ("PATCH", [id]) => {
            let Some(it) = held.iter_mut().find(|i| i["scheduled_message_id"] == *id) else { return Some((404, json!({ "message": "Unknown scheduled message", "code": 10008 }))) };
            if !b["scheduled_timestamp"].is_null() {
                match when(&b["scheduled_timestamp"], now_ms) {
                    Ok(at) => it["send_at_timestamp"] = json!(at),
                    Err(e) => return Some(e),
                }
            }
            if let Some(c) = b["content"].as_str() {
                it["create_args"]["content"] = json!(c);
                it["message_preview"]["content"] = json!(c);
                it["create_args"]["flags"] = b["flags"].clone();
            }
            (200, it.clone())
        }
        ("DELETE", [id]) | ("POST", [id, "send"]) => {
            let before = held.len();
            held.retain(|i| i["scheduled_message_id"] != *id);
            if held.len() == before { (404, json!({ "message": "Unknown scheduled message", "code": 10008 })) } else { (204, Value::Null) }
        }
        _ => return None,
    })
}
