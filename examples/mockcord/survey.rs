//! Discord's survey notice: none, as most accounts have none, except for the one fetch after a step
//! turns it on.

use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

pub fn route(method: &str, parts: &[&str]) -> Option<(u16, Value)> {
    Some(match (method, parts) {
        ("POST", ["api", "v9", "mock", "survey"]) => {
            ON.store(true, Ordering::Relaxed);
            (200, json!({}))
        }
        ("GET", ["api", "v9", "users", "@me", "survey"]) => (200, json!({ "survey": ON.swap(false, Ordering::Relaxed).then(|| json!({
            "id": "2024-11_blocking", "key": "2024-11_blocking", "prompt": "How is blocking users on Discord working for you?", "cta": "Give feedback",
            "url": "https://discord.com/survey", "embedded": false, "guild_requirements": [], "guild_size": [null, null], "guild_permissions": [],
        })) })),
        ("POST", ["api", "v9", "users", "@me", "survey", _, "seen"]) => (204, json!({})),
        _ => return None,
    })
}
