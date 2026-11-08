//! SET_ACTIVITY the way Discord's rpc server takes it: the limits it enforces, and the activity
//! that goes to the gateway afterwards.

use serde_json::{Map, Value, json};

use crate::state::Id;

pub type Fail = (u32, String);

pub const INVALID_PAYLOAD: u32 = 4000;
pub const INVALID_SECRET: u32 = 5005;

type Obj = Map<String, Value>;

fn bad(path: &str, why: &str) -> Fail {
    (INVALID_PAYLOAD, format!("Invalid payload: \"{path}\" {why}"))
}

fn obj<'a>(o: &'a Obj, key: &str, path: &str) -> Result<Option<&'a Obj>, Fail> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        _ => Err(bad(&format!("{path}{key}"), "must be an object")),
    }
}

fn text(o: &Obj, key: &str, path: &str, min: usize, max: usize) -> Result<Option<String>, Fail> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let n = s.chars().count();
            if n < min {
                return Err(bad(&format!("{path}{key}"), &format!("length must be at least {min} characters long")));
            }
            if n > max {
                return Err(bad(&format!("{path}{key}"), &format!("length must be less than or equal to {max} characters long")));
            }
            Ok(Some(s.clone()))
        }
        _ => Err(bad(&format!("{path}{key}"), "must be a string")),
    }
}

fn link(o: &Obj, key: &str, path: &str, max: usize) -> Result<Option<String>, Fail> {
    let Some(s) = text(o, key, path, 1, max)? else { return Ok(None) };
    match reqwest::Url::parse(&s) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => Ok(Some(s)),
        _ => Err(bad(&format!("{path}{key}"), "must be a valid uri")),
    }
}

fn count(v: &Value, path: &str) -> Result<u64, Fail> {
    match v.as_f64() {
        Some(n) if n >= 0.0 => Ok(n as u64),
        _ => Err(bad(path, "must be larger than or equal to 0")),
    }
}

/// Seconds become milliseconds, as the desktop client reads anything with fewer digits than a
/// millisecond clock has, less two.
fn millis(v: &Value, path: &str) -> Result<i64, Fail> {
    let n = match v.as_f64() {
        Some(n) if n >= 1.0 => n,
        _ => return Err(bad(path, "must be larger than or equal to 1")),
    };
    let digits = (n as u64).to_string().len();
    Ok(if 13usize.saturating_sub(digits) > 2 { (n * 1000.0).floor() as i64 } else { n as i64 })
}

pub fn shape(input: &Value, app: Id, app_name: &str, session: &str) -> Result<Value, Fail> {
    let Value::Object(a) = input else { return Err(bad("activity", "must be an object")) };
    let mut r = Obj::new();
    r.insert("name".into(), json!(text(a, "name", "activity.", 1, 128)?.unwrap_or_else(|| app_name.to_string())));
    r.insert("application_id".into(), json!(app.to_string()));
    r.insert("platform".into(), json!("desktop"));
    let kind = match a.get("type") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().filter(|t| [0, 2, 3, 5].contains(t)).ok_or_else(|| bad("activity.type", "must be one of [0, 2, 3, 5]"))?,
    };
    r.insert("type".into(), json!(kind));
    for (key, min, max) in [("state", 2, 128), ("details", 2, 128)] {
        if let Some(s) = text(a, key, "activity.", min, max)? {
            r.insert(key.into(), json!(s));
        }
    }
    for key in ["state_url", "details_url"] {
        if let Some(s) = link(a, key, "activity.", 256)? {
            r.insert(key.into(), json!(s));
        }
    }
    if let Some(t) = obj(a, "timestamps", "activity.")? {
        let mut out = Obj::new();
        for key in ["start", "end"] {
            if let Some(v) = t.get(key).filter(|v| !v.is_null()) {
                out.insert(key.into(), json!(millis(v, &format!("activity.timestamps.{key}"))?));
            }
        }
        r.insert("timestamps".into(), Value::Object(out));
    }
    if let Some(s) = obj(a, "assets", "activity.")? {
        let mut out = Obj::new();
        for (key, min, max) in [("large_image", 1, 300), ("large_text", 2, 128), ("small_image", 1, 300), ("small_text", 2, 128), ("invite_cover_image", 1, 300)] {
            if let Some(t) = text(s, key, "activity.assets.", min, max)? {
                out.insert(key.into(), json!(t));
            }
        }
        for key in ["large_url", "small_url"] {
            if let Some(t) = link(s, key, "activity.assets.", 256)? {
                out.insert(key.into(), json!(t));
            }
        }
        r.insert("assets".into(), Value::Object(out));
    }
    let mut party_id = None;
    if let Some(p) = obj(a, "party", "activity.")? {
        let mut out = Obj::new();
        party_id = text(p, "id", "activity.party.", 2, 128)?;
        if let Some(id) = &party_id {
            out.insert("id".into(), json!(id));
        }
        match p.get("size") {
            None | Some(Value::Null) => {}
            Some(Value::Array(s)) if s.len() == 2 => {
                out.insert("size".into(), json!([count(&s[0], "activity.party.size")?, count(&s[1], "activity.party.size")?]));
            }
            _ => return Err(bad("activity.party.size", "must contain 2 items")),
        }
        if p.get("privacy").is_some_and(|v| !v.is_null() && !matches!(v.as_u64(), Some(0 | 1))) {
            return Err(bad("activity.party.privacy", "must be one of [0, 1]"));
        }
        r.insert("party".into(), Value::Object(out));
    }
    let mut secrets = Vec::new();
    if let Some(s) = obj(a, "secrets", "activity.")? {
        let mut out = Obj::new();
        for key in ["match", "join", "spectate"] {
            if let Some(t) = text(s, key, "activity.secrets.", 2, 128)? {
                secrets.push(t.clone());
                out.insert(key.into(), json!(t));
            }
        }
        r.insert("secrets".into(), Value::Object(out));
    }
    let mut metadata = Obj::new();
    match a.get("buttons") {
        None | Some(Value::Null) => {}
        Some(Value::Array(b)) if (1..=2).contains(&b.len()) => {
            let (mut labels, mut urls) = (Vec::new(), Vec::new());
            for (i, item) in b.iter().enumerate() {
                let Value::Object(item) = item else { return Err(bad(&format!("activity.buttons[{i}]"), "must be an object")) };
                let path = format!("activity.buttons[{i}].");
                labels.push(text(item, "label", &path, 1, 32)?.ok_or_else(|| bad(&format!("{path}label"), "is required"))?);
                urls.push(link(item, "url", &path, 512)?.ok_or_else(|| bad(&format!("{path}url"), "is required"))?);
            }
            if !secrets.is_empty() {
                return Err((INVALID_SECRET, "secrets cannot currently be sent with buttons".into()));
            }
            metadata.insert("button_urls".into(), json!(urls));
            r.insert("buttons".into(), json!(labels));
        }
        _ => return Err(bad("activity.buttons", "must contain between 1 and 2 items")),
    }
    if let Some(id) = &party_id
        && secrets.contains(id)
    {
        return Err((INVALID_SECRET, "secrets cannot match the party id".into()));
    }
    let mut unique = secrets.clone();
    unique.sort();
    unique.dedup();
    if unique.len() < secrets.len() {
        return Err((INVALID_SECRET, "secrets must be unique".into()));
    }
    match a.get("supported_platforms") {
        None | Some(Value::Null) => {}
        Some(Value::Array(p)) if (1..=10).contains(&p.len()) && p.iter().all(|s| s.as_str().is_some_and(|s| (1..=32).contains(&s.chars().count()))) => {
            r.insert("supported_platforms".into(), json!(p));
        }
        _ => return Err(bad("activity.supported_platforms", "must contain between 1 and 10 strings")),
    }
    match a.get("status_display_type") {
        None | Some(Value::Null) => {}
        Some(v) => {
            let n = v.as_u64().filter(|n| *n <= 2).ok_or_else(|| bad("activity.status_display_type", "must be one of [0, 1, 2]"))?;
            r.insert("status_display_type".into(), json!(n));
        }
    }
    let instance = a.get("instance").and_then(Value::as_bool).unwrap_or(false);
    let joinable = r.get("secrets").is_some_and(|s| s["join"].is_string());
    let flags = u64::from(instance) | if joinable { 2 } else { 0 };
    if flags > 0 {
        r.insert("flags".into(), json!(flags));
    }
    r.insert("metadata".into(), Value::Object(metadata));
    r.insert("session_id".into(), json!(session));
    Ok(Value::Object(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(v: Value) -> Result<Value, Fail> {
        shape(&v, 7, "Mock Game", "sess")
    }

    #[test]
    fn rich_presence_goes_out_in_gateway_shape() {
        let a = shaped(json!({
            "state": "In a group", "details": "Exploring", "timestamps": { "start": 1_700_000_000, "end": 1_700_003_600_000i64 },
            "assets": { "large_image": "logo", "large_text": "Logo" }, "party": { "id": "party-1", "size": [2, 4] },
            "buttons": [{ "label": "Watch", "url": "https://example.com/w" }], "instance": true, "bogus": 1
        }))
        .unwrap();
        assert_eq!(a["name"], "Mock Game");
        assert_eq!(a["application_id"], "7");
        assert_eq!(a["type"], 0);
        assert_eq!(a["timestamps"]["start"], 1_700_000_000_000i64);
        assert_eq!(a["timestamps"]["end"], 1_700_003_600_000i64);
        assert_eq!(a["party"]["size"], json!([2, 4]));
        assert_eq!(a["buttons"], json!(["Watch"]));
        assert_eq!(a["metadata"]["button_urls"], json!(["https://example.com/w"]));
        assert_eq!(a["flags"], 1);
        assert!(a.get("bogus").is_none());
    }

    #[test]
    fn what_discord_refuses_is_refused() {
        assert_eq!(shaped(json!({ "details": "x" })).unwrap_err().0, INVALID_PAYLOAD);
        assert_eq!(shaped(json!({ "type": 1 })).unwrap_err().0, INVALID_PAYLOAD);
        assert_eq!(shaped(json!({ "buttons": [{ "label": "a", "url": "javascript:alert(1)" }] })).unwrap_err().0, INVALID_PAYLOAD);
        assert_eq!(shaped(json!({ "party": { "id": "abc" }, "secrets": { "join": "abc" } })).unwrap_err().0, INVALID_SECRET);
        assert_eq!(shaped(json!({ "secrets": { "join": "same", "spectate": "same" } })).unwrap_err().0, INVALID_SECRET);
        assert_eq!(shaped(json!({ "secrets": { "join": "ab" }, "buttons": [{ "label": "a", "url": "https://x.y" }] })).unwrap_err().0, INVALID_SECRET);
    }
}
