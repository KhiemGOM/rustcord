//! What the mock demands of a write before it does anything: the field names, types and
//! required fields discord's own client sends for the routes checked against its source.
//! A route with no entry here is not checked. Unknown fields are refused too, though discord
//! would ignore them, so a misspelt field fails a run instead of quietly saving nothing.
//! A spec is a type letter (s string, i integer, b bool, n snowflake string, a array, o object,
//! x anything) then `!` for required and `?` for may-be-null.

use serde_json::{Value, json};

type Spec = &'static [(&'static str, &'static str)];

const PALETTE: [(&str, &str); 13] = [
    ("#ff1c90", "#ff7fc0"),
    ("#ff8f1c", "#ffae77"),
    ("#eebe1a", "#fffc7f"),
    ("#32a070", "#57b59e"),
    ("#32839a", "#71c2d9"),
    ("#8a43ff", "#bd95ff"),
    ("#9b3fe5", "#cc8dff"),
    ("#942e8f", "#d46cb5"),
    ("#d14242", "#ff8989"),
    ("#814300", "#a88a6c"),
    ("#717224", "#c3c3c3"),
    ("#5d1d47", "#c58fbb"),
    ("#222222", "#cccccc"),
];

const ROUTES: &[(&str, &str, Spec)] = &[
    ("POST", "channels/*/messages/*/ack", &[("token", "s?"), ("last_viewed", "i"), ("flags", "i"), ("manual", "b"), ("mention_count", "i")]),
    ("PATCH", "guilds/*/requests/*", &[("action", "s!"), ("rejection_reason", "s?")]),
    ("PUT", "channels/*/permissions/*", &[("id", "n"), ("type", "i!"), ("allow", "s!"), ("deny", "s!")]),
    ("PATCH", "guilds/*/profile", &[("tag", "s?"), ("badge", "i?"), ("badge_color_primary", "s?"), ("badge_color_secondary", "s?"), ("description", "s"), ("brand_color_primary", "s?"), ("name", "s"), ("icon", "s?"), ("custom_banner", "s?"), ("visibility", "i"), ("traits", "a"), ("game_application_ids", "a")]),
    ("PUT", "guilds/*/incident-actions", &[("invites_disabled_until", "s?!"), ("dms_disabled_until", "s?!"), ("lockdown_duration_hours", "i?!")]),
    ("PATCH", "guilds/*/vanity-url", &[("code", "s?!")]),
    ("PATCH", "guilds/*/widget", &[("enabled", "b"), ("channel_id", "n?")]),
    ("POST", "guilds/*/mfa", &[("level", "i!")]),
    ("POST", "guilds/*/delete", &[]),
    ("POST", "guilds/*/bulk-ban/v2", &[("user_ids", "a!"), ("delete_message_seconds", "i")]),
    ("PATCH", "guilds/*", &[("name", "s"), ("region", "s?"), ("icon", "s?"), ("splash", "s?"), ("banner", "s?"), ("discovery_splash", "s?"), ("home_header", "s?"), ("description", "s?"), ("afk_channel_id", "n?"), ("afk_timeout", "i"), ("system_channel_id", "n?"), ("system_channel_flags", "i"), ("verification_level", "i"), ("explicit_content_filter", "i"), ("default_message_notifications", "i"), ("owner_configured_content_level", "i"), ("rules_channel_id", "n?"), ("public_updates_channel_id", "n?"), ("safety_alerts_channel_id", "n?"), ("premium_progress_bar_enabled", "b"), ("features", "a"), ("preferred_locale", "s")]),
    ("PUT", "users/@me/saved-messages/*/*", &[("source", "s"), ("due_at", "s?")]),
    ("PUT", "channels/*/polls/*/answers/@me", &[("answer_ids", "a!")]),
    ("POST", "users/@me/harvest", &[("backends", "a!")]),
    ("PATCH", "users/@me/connections/*/*", &[("visibility", "b"), ("show_activity", "b")]),
    ("PATCH", "users/@me/guilds/settings", &[("guilds", "o!")]),
    ("POST", "users/@me/channels", &[("recipients", "a!")]),
    ("POST", "channels/*/send-soundboard-sound", &[("sound_id", "n!"), ("source_guild_id", "n?"), ("emoji_id", "n?"), ("emoji_name", "s?")]),
    ("PUT", "guilds/*/scheduled-events/*/users/@me", &[("response", "i")]),
    ("POST", "invites/*", &[("session_id", "s?"), ("invite_instance_id", "s?"), ("received_installation_id", "s?")]),
    ("POST", "channels/*/invites", &[("max_age", "i"), ("max_uses", "i"), ("temporary", "b"), ("unique", "b"), ("flags", "i"), ("validate", "s?"), ("target_type", "i?"), ("role_ids", "a")]),
];

fn kind_ok(t: u8, v: &Value) -> bool {
    match t {
        b's' => v.is_string(),
        b'i' => v.is_i64() || v.is_u64(),
        b'b' => v.is_boolean(),
        b'n' => v.as_str().is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())),
        b'a' => v.is_array(),
        b'o' => v.is_object(),
        _ => true,
    }
}

fn spec_for(method: &str, parts: &[&str]) -> Option<Spec> {
    ROUTES.iter().find(|(m, p, _)| *m == method && p.split('/').count() == parts.len() && p.split('/').zip(parts).all(|(a, b)| a == "*" || a == *b)).map(|r| r.2)
}

fn refuse(field: &str, code: &str, message: &str) -> Value {
    json!({ "code": 50035, "message": "Invalid Form Body", "errors": { field: { "_errors": [{ "code": code, "message": message }] } } })
}

fn palette(v: &Value, pos: usize) -> bool {
    v.is_null() || v.as_str().is_some_and(|s| PALETTE.iter().any(|p| [p.0, p.1][pos] == s))
}

fn extra(method: &str, parts: &[&str], b: &Value) -> Option<Value> {
    if method == "PATCH" && parts.len() == 3 && parts[0] == "guilds" && parts[2] == "profile" {
        if let Some(t) = b["tag"].as_str() {
            let n = t.chars().count();
            if !(2..=4).contains(&n) || !t.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Some(refuse("tag", "BASE_TYPE_BAD_LENGTH", "Must be between 2 and 4 in length."));
            }
        }
        if let Some(n) = b["badge"].as_u64()
            && n > 40
        {
            return Some(refuse("badge", "NUMBER_TYPE_MAX", "Must be between 0 and 40."));
        }
        for (k, pos) in [("badge_color_primary", 0), ("badge_color_secondary", 1)] {
            if b.get(k).is_some_and(|v| !palette(v, pos)) {
                return Some(refuse(k, "INVALID_COLOR", "Invalid color."));
            }
        }
    }
    None
}

/// X-Context-Properties is base64 of a JSON object that says where the request came from.
pub fn context(raw: &str) -> Option<Value> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(raw).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    match decoded {
        Some(c) if c["location"].as_str().is_some_and(|l| !l.is_empty()) => None,
        _ => Some(refuse("X-Context-Properties", "BASE_TYPE_BAD", "Must be base64 of a JSON object with a location.")),
    }
}

/// The refusal for a write whose body breaks its route's rules, if the route has rules.
pub fn check(method: &str, parts: &[&str], body: &[u8]) -> Option<Value> {
    let spec = spec_for(method, parts.get(2..)?)?;
    let b: Value = if body.is_empty() { json!({}) } else { serde_json::from_slice(body).ok().filter(Value::is_object)? };
    for (k, v) in b.as_object()? {
        let Some((_, t)) = spec.iter().find(|(n, _)| n == k) else {
            return Some(refuse(k, "UNKNOWN_FIELD", "Discord does not send this field."));
        };
        let t = t.as_bytes();
        if v.is_null() && !t.contains(&b'?') {
            return Some(refuse(k, "BASE_TYPE_REQUIRED", "This field is required"));
        }
        if !v.is_null() && !kind_ok(t[0], v) {
            return Some(refuse(k, "BASE_TYPE_INVALID", "Value is the wrong type."));
        }
    }
    for (k, t) in spec.iter().filter(|(_, t)| t.ends_with('!')) {
        let _ = t;
        if b.get(k).is_none() {
            return Some(refuse(k, "BASE_TYPE_REQUIRED", "This field is required"));
        }
    }
    extra(method, parts.get(2..)?, &b)
}
