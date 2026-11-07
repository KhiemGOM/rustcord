//! A linked role's requirements: PUT saves the groups of connection elements, refusing what
//! discord's client never sends (a field a platform doesn't report, an operator out of range), and
//! a later GET gives them back.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

static SAVED: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

const FIELDS: &[(&str, &[&str])] = &[
    ("bluesky", &["created_at", "followers_count", "statuses_count"]),
    ("steam", &["created_at", "game_count", "item_count_tf2", "item_count_dota2"]),
    ("twitter", &["created_at", "verified", "followers_count", "statuses_count"]),
    ("reddit", &["created_at", "total_karma", "gold", "mod"]),
    ("paypal", &["created_at", "verified"]),
    ("ebay", &["created_at", "positive_feedback_percentage", "top_rated_seller", "unique_negative_feedback_count", "unique_positive_feedback_count"]),
    ("tiktok", &["verified", "follower_count", "following_count", "likes_count"]),
];

fn bad(why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } }))
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let ("GET" | "PUT", ["api", "v9", "guilds", _, "roles", role, "connections", "configuration"]) = (method, parts) else { return None };
    let mut all = SAVED.lock().unwrap();
    let all = all.get_or_insert_with(HashMap::new);
    if method == "GET" {
        return Some((200, all.get(*role).cloned().unwrap_or_else(|| json!([]))));
    }
    let v: Value = serde_json::from_slice(body).unwrap_or_default();
    let Some(groups) = v.as_array() else { return Some(bad("The configuration is a list of groups.")) };
    for g in groups {
        let Some(g) = g.as_array().filter(|g| !g.is_empty()) else { return Some(bad("A group is a list of requirements.")) };
        for e in g {
            let Some(kind) = e["connection_type"].as_str() else { return Some(bad("connection_type is required.")) };
            if let Some(f) = e.get("connection_metadata_field").filter(|f| !f.is_null()) {
                let known = FIELDS.iter().find(|p| p.0 == kind).is_some_and(|p| p.1.contains(&f.as_str().unwrap_or_default()));
                if !known {
                    return Some(bad("That connection does not report this field."));
                }
                if !e["operator"].as_u64().is_some_and(|o| (1..=4).contains(&o)) {
                    return Some(bad("operator must be between 1 and 4."));
                }
                if !e["value"].as_str().is_some_and(|s| s.parse::<u64>().is_ok()) {
                    return Some(bad("value is a number as a string."));
                }
            }
        }
    }
    all.insert(role.to_string(), v.clone());
    Some((200, v))
}
