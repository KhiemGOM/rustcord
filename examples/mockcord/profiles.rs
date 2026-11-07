//! The profile editor's side of the mock: the shop's catalog, what the account owns, recent
//! avatars, the widgets' game lists, username checks, and the rules the profile writes obey.

use serde_json::{Value, json};

pub const OWNED: [&str; 4] = ["7001", "7002", "7003", "7005"];
const TAKEN: [&str; 4] = ["alice", "bob_builds", "carol", "taken_name"];
const FONTS: [u64; 12] = [11, 3, 4, 6, 7, 8, 10, 12, 13, 14, 15, 16];

fn product(sku: &str, name: &str, nitro: bool, item: Value) -> Value {
    let mut it = item;
    it["sku_id"] = json!(sku);
    json!({ "sku_id": sku, "name": name, "summary": "", "type": it["type"], "premium_type": if nitro { 2 } else { 0 }, "items": [it], "category_sku_id": "8000" })
}

fn deco(sku: &str, name: &str, nitro: bool) -> Value {
    product(sku, name, nitro, json!({ "type": 0, "asset": format!("a_{}", name.to_lowercase().replace(' ', "_")), "label": name }))
}

fn plate(sku: &str, name: &str, nitro: bool) -> Value {
    product(sku, name, nitro, json!({ "type": 2, "asset": format!("nameplates/mock/{sku}/"), "label": name, "palette": "cobalt" }))
}

fn effect(cdn: &str, sku: &str, name: &str, nitro: bool) -> Value {
    product(sku, name, nitro, json!({
        "type": 1, "title": name, "description": "Show this effect when others view your profile.", "accessibilityLabel": name, "animationType": 2,
        "thumbnailPreviewSrc": format!("{cdn}/media/effect_{sku}.png"), "reducedMotionSrc": format!("{cdn}/media/effect_{sku}.png"),
        "effects": [
            { "src": format!("{cdn}/media/effect_{sku}_a.gif"), "loop": false, "duration": 1500, "start": 0, "loopDelay": 0, "zIndex": 110, "randomizedSources": [] },
            { "src": format!("{cdn}/media/effect_{sku}_b.gif"), "loop": true, "duration": 1500, "start": 1800, "loopDelay": 700, "zIndex": 100, "randomizedSources": [] },
        ],
    }))
}

fn frame(cdn: &str, sku: &str, name: &str, nitro: bool) -> Value {
    product(sku, name, nitro, json!({
        "type": 3, "title": name, "thumbnailPreviewSrc": format!("{cdn}/media/frame_{sku}.png"),
        "effects": [{ "src": format!("{cdn}/media/frame_{sku}_a.png"), "loop": true, "duration": 3000, "start": 0, "loopDelay": 0, "zIndex": 120, "randomizedSources": [] }],
    }))
}

pub fn catalog(cdn: &str) -> Vec<Value> {
    vec![
        deco("7001", "Mock Halo", false),
        deco("7101", "Nitro Ring", true),
        deco("7102", "Nitro Crown", true),
        deco("7201", "Shop Wings", false),
        deco("7202", "Shop Flames", false),
        plate("7002", "Mock Plate", false),
        plate("7104", "Nitro Plate", true),
        plate("7204", "Shop Plate", false),
        effect(cdn, "7003", "Mock Sparkle", false),
        effect(cdn, "7103", "Nitro Aurora", true),
        effect(cdn, "7203", "Shop Rain", false),
        frame(cdn, "7005", "Mock Frame", false),
        frame(cdn, "7205", "Shop Frame", false),
    ]
}

fn owned(cdn: &str) -> Vec<Value> {
    catalog(cdn).into_iter().filter(|p| OWNED.contains(&p["sku_id"].as_str().unwrap_or(""))).map(|mut p| {
        p["purchased_at"] = json!("2026-10-01T10:00:00+00:00");
        p
    }).collect()
}

fn games() -> Value {
    json!([
        { "id": "9001", "game_id": "9001", "name": "Mock Racers" },
        { "id": "9002", "game_id": "9002", "name": "Crab Quest" },
        { "id": "9003", "game_id": "9003", "name": "Outsider Game" },
        { "id": "9004", "game_id": "9004", "name": "Genshin Impact" },
        { "id": "9005", "game_id": "9005", "name": "Rust Simulator" },
    ])
}

pub fn route(method: &str, p: &[&str], b: &Value, cdn: &str) -> Option<(u16, Value)> {
    let ok = |v: Value| Some((200, v));
    match (method, p) {
        ("GET", ["api", "v9", "collectibles-categories", "v2"]) => ok(json!({ "categories": [{ "sku_id": "8000", "name": "Mock Collection", "products": catalog(cdn) }], "collections": [] })),
        ("GET", ["api", "v9", "users", "@me", "collectibles-purchases"]) => ok(json!(owned(cdn))),
        ("GET", ["api", "v9", "collectibles-products", sku]) => match catalog(cdn).into_iter().find(|p| p["sku_id"] == *sku) {
            Some(p) => ok(p),
            None => Some((404, json!({ "message": "Unknown Collectibles Product", "code": 10060 }))),
        },
        ("POST", ["api", "v9", "users", "@me", "pomelo-attempt"]) => {
            let name = b["username"].as_str().unwrap_or("");
            if name.len() < 2 || name.len() > 32 || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.') || name.contains("..") {
                return Some((400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { "username": { "_errors": [{ "code": "USERNAME_INVALID", "message": "Username is invalid." }] } } })));
            }
            ok(json!({ "taken": TAKEN.contains(&name) }))
        }
        ("GET", ["api", "v9", "widget-configs", "featured"]) => ok(json!([{ "application_id": "9001", "name": "Marvel Rivals" }, { "application_id": "9004", "name": "Wuthering Waves" }])),
        ("GET", ["api", "v9", "widget-configs", "developer"]) => ok(json!([{ "application_id": "9002", "name": "Crab Quest" }])),
        ("GET", ["api", "v9", "users", "@me", "widgets", "suggested-games"]) => ok(games()),
        ("GET", ["api", "v9", "wishlists", id]) => ok(json!({ "id": id, "visibility": 1, "wishlist_items": if *id == "101" { json!([{ "sku": { "id": "7201" }, "sku_id": "7201" }, { "sku": { "id": "7203" }, "sku_id": "7203" }]) } else { json!([]) } })),
        ("GET", ["api", "v9", "users", _, "application-identities"]) => ok(json!([{ "application_id": "9001", "name": "Mock Racers", "identity": { "username": "alice#racer" } }])),
        _ => None,
    }
}

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "INVALID", "message": why }] } } }))
}

fn colour(v: &Value) -> bool {
    v.is_null() || v.as_u64().is_some_and(|c| c <= 0xffffff)
}

/// What discord rejects in a profile write: too long, wrong shape, or a cosmetic the account doesn't own.
pub fn check_profile(b: &Value, nitro: bool) -> Option<(u16, Value)> {
    if b["bio"].as_str().is_some_and(|s| s.chars().count() > 190) {
        return Some(bad("bio", "Must be 190 or fewer in length."));
    }
    if b["pronouns"].as_str().is_some_and(|s| s.chars().count() > 40) {
        return Some(bad("pronouns", "Must be 40 or fewer in length."));
    }
    if !colour(&b["accent_color"]) {
        return Some(bad("accent_color", "Value must be a valid colour."));
    }
    if let Some(t) = b.get("theme_colors").filter(|t| !t.is_null()) {
        let list = t.as_array().cloned().unwrap_or_default();
        if list.len() != 2 || !list.iter().all(colour) {
            return Some(bad("theme_colors", "Must be a list of two colours."));
        }
        if !nitro {
            return Some(bad("theme_colors", "Theme colours need Nitro."));
        }
    }
    if let Some(s) = b["banner"].as_str()
        && !s.starts_with("data:image/")
    {
        return Some(bad("banner", "Not a valid image."));
    }
    for k in ["profile_effect_id", "profile_frame_id"] {
        if let Some(s) = b[k].as_str()
            && !OWNED.contains(&s)
            && !(nitro && s.starts_with("71"))
        {
            return Some(bad(k, "You do not own this item."));
        }
    }
    None
}

pub fn check_account(b: &Value, nitro: bool) -> Option<(u16, Value)> {
    if let Some(s) = b["avatar"].as_str()
        && !s.starts_with("data:image/")
    {
        return Some(bad("avatar", "Not a valid image."));
    }
    for k in ["avatar_decoration_sku_id", "nameplate_sku_id"] {
        if let Some(s) = b[k].as_str()
            && !OWNED.contains(&s)
            && !(nitro && s.starts_with("71"))
        {
            return Some(bad(k, "You do not own this item."));
        }
    }
    if let Some(f) = b["display_name_font_id"].as_u64() {
        if !FONTS.contains(&f) {
            return Some(bad("display_name_font_id", "Unknown font."));
        }
        if !nitro {
            return Some(bad("display_name_font_id", "Display name styles need Nitro."));
        }
        if !b["display_name_effect_id"].as_u64().is_some_and(|e| (1..=8).contains(&e)) {
            return Some(bad("display_name_effect_id", "Unknown effect."));
        }
        if !b["display_name_colors"].as_array().is_some_and(|c| (1..=3).contains(&c.len()) && c.iter().all(colour)) {
            return Some(bad("display_name_colors", "Must be one to three colours."));
        }
    }
    None
}

pub fn check_widgets(b: &Value) -> Option<(u16, Value)> {
    let list = b["widgets"].as_array()?;
    for w in list {
        let d = &w["data"];
        let max = match d["type"].as_str() {
            Some("favorite_games") => 1,
            Some("current_games") => 5,
            Some("want_to_play_games" | "played_games") => 20,
            _ => return Some(bad("widgets", "Unknown widget type.")),
        };
        let g = d["games"].as_array();
        if !g.is_some_and(|g| !g.is_empty() && g.len() <= max && g.iter().all(|g| g["game_id"].is_string())) {
            return Some(bad("widgets", "Too many or too few games."));
        }
    }
    None
}
