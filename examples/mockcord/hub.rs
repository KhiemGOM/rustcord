//! Server Hub: the widget catalog, the hub a server's admins save, and its settings.

use serde_json::{Value, json};
use std::sync::Mutex;

static SPACES: Mutex<Vec<(String, Value)>> = Mutex::new(Vec::new());

pub const EXPERIMENT: u32 = 4201393060;
pub const GUILD: &str = "1001";

const KINDS: [(&str, &str, &str); 4] = [
    ("whiteboard", "Whiteboard", "Draw and plan together."),
    ("leaderboard", "Gaming Leaderboard", "Who is on top of the leaderboard?"),
    ("popular_music", "Popular Music", "What the server is listening to."),
    ("image_text", "Text and Image", "Add text or an image"),
];

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    if let ("POST", ["api", "v9", "mock", "hub-reset"]) = (method, parts) {
        SPACES.lock().unwrap().clear();
        return Some((200, json!({})));
    }
    let ["api", "v9", "guilds", guild, "space", rest @ ..] = parts else { return None };
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut spaces = SPACES.lock().unwrap();
    Some(match (method, rest) {
        ("GET", ["widgets", "catalog"]) => (200, json!({ "widgets": KINDS.iter().map(|(t, n, d)| json!({
            "type": t, "name": n, "description": d, "assets": { "catalog_image_static_url": null, "catalog_image_animated_url": null },
            "max_instances": if *t == "image_text" { 5 } else { 1 }, "powerup_sku_id": null, "boost_price": null, "locked": false })).collect::<Vec<_>>() })),
        ("GET", []) => match spaces.iter().find(|s| s.0 == *guild) {
            Some(s) => (200, s.1.clone()),
            None => (404, json!({ "message": "Unknown Guild Space", "code": 10004 })),
        },
        ("PUT", []) => {
            let Some(list) = b["widgets"].as_array() else { return Some(bad("widgets", "This field is required")) };
            if !b["header"].is_object() {
                return Some(bad("header", "This field is required"));
            }
            let mut out = Vec::new();
            for (i, w) in list.iter().enumerate() {
                let kind = w["type"].as_str().unwrap_or("");
                if !KINDS.iter().any(|k| k.0 == kind) {
                    return Some(bad(&format!("widgets.{i}.type"), "Value must be one of the widget types."));
                }
                if !w["position"]["column"].is_u64() || !w["position"]["order"].is_u64() || w["config"]["type"] != kind {
                    return Some(bad(&format!("widgets.{i}.position"), "A widget needs its column, its order and a config of its own type."));
                }
                let id = w["id"].as_str().map_or_else(|| format!("77{i:04}"), str::to_string);
                out.push(json!({ "id": id, "type": kind, "position": w["position"], "config": w["config"] }));
            }
            let space = json!({ "guild_id": guild, "header": { "custom_banner_hash": null }, "widgets": out });
            spaces.retain(|s| s.0 != *guild);
            spaces.push((guild.to_string(), space.clone()));
            (200, space)
        }
        ("PATCH", ["settings"]) => {
            let known = b.as_object().is_some_and(|o| !o.is_empty() && o.keys().all(|k| k == "enabled" || k == "publish_status"));
            if !known || b.get("enabled").is_some_and(|e| !e.is_boolean()) || b.get("publish_status").is_some_and(|p| p.as_u64().is_none_or(|p| p > 1)) {
                return Some(bad("enabled", "Settings take enabled (a boolean) or publish_status (0 or 1)."));
            }
            (200, b)
        }
        _ => return None,
    })
}
