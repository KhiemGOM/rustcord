//! Server pages' own data: the games a server's members play, and its discovery metadata,
//! saved by PATCH and given back by a later GET.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

static META: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

const CATEGORIES: [u64; 6] = [1, 2, 3, 4, 5, 6];

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

/// Whether a pattern is one discord's engine would take: brackets and groups close, and nothing
/// repeats nothing.
fn regex_fault(p: &str) -> Option<&'static str> {
    let (mut depth, mut class, mut prev_open) = (0i32, false, true);
    let mut chars = p.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
                prev_open = false;
                continue;
            }
            '[' if !class => class = true,
            ']' if class => class = false,
            '(' if !class => depth += 1,
            ')' if !class => {
                depth -= 1;
                if depth < 0 {
                    return Some("unopened group");
                }
            }
            '*' | '+' | '?' if !class && prev_open => return Some("repetition operator missing expression"),
            _ => {}
        }
        prev_open = !class && matches!(c, '(' | '|');
    }
    if class {
        Some("unclosed character class")
    } else if depth > 0 {
        Some("unclosed group")
    } else {
        None
    }
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    match (method, parts) {
        ("POST", ["api", "v9", "guilds", _, "auto-moderation", "rules", "validate"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let patterns: Vec<&str> = b["trigger_metadata"]["regex_patterns"].as_array().into_iter().flatten().filter_map(|p| p.as_str()).collect();
            if patterns.len() > 10 {
                return Some(bad("trigger_metadata.regex_patterns", "Must be 10 or fewer in length."));
            }
            let mut errors = serde_json::Map::new();
            for p in patterns {
                if let Some(why) = regex_fault(p) {
                    errors.insert(p.to_string(), json!({ "_errors": [{ "code": "INVALID_REGEX", "message": why }] }));
                }
            }
            if errors.is_empty() {
                return Some((200, b));
            }
            Some((400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { "trigger_metadata": { "regex_patterns": errors } } })))
        }
        ("GET", ["api", "v9", "guilds", _, "top-games"]) => Some((200, json!({ "top_games": [{ "game_application_id": "7100", "activity_level": 3, "activity_score": 120 }] }))),
        ("GET" | "PATCH", ["api", "v9", "guilds", g, "discovery-metadata"]) => {
            let mut all = META.lock().unwrap();
            let all = all.get_or_insert_with(HashMap::new);
            let cur = all.entry(g.to_string()).or_insert_with(|| json!({ "guild_id": g, "primary_category_id": null, "category_ids": [], "keywords": [], "emoji_discoverability_enabled": true, "partner_actioned_timestamp": null, "partner_application_timestamp": null, "is_published": false, "reasons_to_join": [], "social_links": [], "about": null }));
            if method == "PATCH" {
                let b: Value = serde_json::from_slice(body).unwrap_or_default();
                if let Some(c) = b.get("primary_category_id").filter(|c| !c.is_null()) {
                    if !c.as_u64().is_some_and(|c| CATEGORIES.contains(&c)) {
                        return Some(bad("primary_category_id", "Unknown category."));
                    }
                }
                if let Some(k) = b.get("keywords") {
                    let Some(list) = k.as_array() else { return Some(bad("keywords", "Must be a list.")) };
                    if list.len() > 10 {
                        return Some(bad("keywords", "Must be 10 or fewer in length."));
                    }
                    if list.iter().any(|w| !w.as_str().is_some_and(|w| !w.is_empty() && w.chars().count() <= 32)) {
                        return Some(bad("keywords", "Each tag is 1 to 32 characters."));
                    }
                }
                if b.get("emoji_discoverability_enabled").is_some_and(|v| !v.is_boolean()) {
                    return Some(bad("emoji_discoverability_enabled", "Must be a boolean."));
                }
                for (k, v) in b.as_object().into_iter().flatten() {
                    cur[k] = v.clone();
                }
            }
            Some((200, cur.clone()))
        }
        _ => None,
    }
}
