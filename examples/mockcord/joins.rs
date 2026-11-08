//! Member applications in a server that makes people apply: the requests by status with discord's
//! paging, approving and rejecting one (and the gateway telling everyone), and a person's other
//! requests. `POST /mock/join-request` makes a new pending one, as if someone had just applied.

use serde_json::{Value, json};
use std::sync::Mutex;

const ME: u64 = 100;
const STATUSES: [&str; 4] = ["STARTED", "SUBMITTED", "REJECTED", "APPROVED"];
const DAY: u64 = 86_400_000;

type Reply = (u16, Option<Value>, Option<(&'static str, Value)>);

static STATE: Mutex<Option<Vec<Value>>> = Mutex::new(None);

fn bad(field: &str, why: &str) -> Reply {
    (400, Some(json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } })), None)
}

fn form(why: &str) -> Value {
    json!([
        { "field_type": "TERMS", "label": "Server Rules", "values": ["Be kind", "No spam"], "required": true, "response": true },
        { "field_type": "TEXT_INPUT", "label": "Why do you want to join?", "required": true, "response": why },
        { "field_type": "MULTIPLE_CHOICE", "label": "How did you find us?", "values": ["A friend", "Discovery", "Search"], "required": true, "response": 1 },
    ])
}

fn snow(ms: u64) -> u64 {
    (ms - 1_420_070_400_000) << 22
}

fn make(id: u64, user: Value, status: &str, created: String, why: &str) -> Value {
    json!({ "join_request_id": id.to_string(), "guild_id": "1001", "user_id": user["id"], "user": user, "created_at": created, "application_status": status,
            "form_responses": form(why), "rejection_reason": null, "actioned_at": null, "actioned_by_user": null, "last_seen": null, "interview_channel_id": null })
}

fn seed(now: u64, user: fn(u64) -> Value, iso: fn(u64) -> String) -> Vec<Value> {
    let rows = [(3, 105, "SUBMITTED", "I want to learn Rust."), (2, 106, "SUBMITTED", "A friend told me about the egui channel."), (9, 107, "REJECTED", "Just looking."), (12, 103, "APPROVED", "Here for the gamedev talk."), (20, 106, "REJECTED", "Hi.")];
    rows.iter()
        .map(|(days, who, status, why)| {
            let at = now - days * DAY;
            let mut r = make(snow(at), user(*who), status, iso(at), why);
            if *status != "SUBMITTED" {
                r["actioned_at"] = json!(snow(at + DAY).to_string());
                r["actioned_by_user"] = user(ME);
            }
            if *status == "REJECTED" {
                r["rejection_reason"] = json!("Please read the rules again.");
            }
            r
        })
        .collect()
}

fn key(r: &Value) -> u64 {
    let f = if r["application_status"] == "SUBMITTED" { "join_request_id" } else { "actioned_at" };
    r[f].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn query(q: Option<&str>, k: &str) -> Option<String> {
    q?.split('&').find_map(|p| p.strip_prefix(&format!("{k}=")).map(str::to_string))
}

pub fn route(method: &str, parts: &[&str], q: Option<&str>, body: &[u8], user: fn(u64) -> Value, iso: fn(u64) -> String, now: u64) -> Option<Reply> {
    if let ("POST", ["api", "v9", "mock", "join-reset"]) = (method, parts) {
        *STATE.lock().unwrap() = None;
        return Some((200, Some(json!({})), None));
    }
    let guild = match (method, parts) {
        ("GET", ["api", "v9", "guilds", g, "requests"]) | ("PATCH", ["api", "v9", "guilds", g, "requests", _]) | ("GET", ["api", "v9", "guilds", g, "requests", "users", _]) => *g,
        ("POST", ["api", "v9", "mock", "join-request"]) => "1001",
        _ => return None,
    };
    if guild != "1001" {
        return Some((404, Some(json!({ "message": "Unknown Guild", "code": 10004 })), None));
    }
    let v: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut guard = STATE.lock().unwrap();
    let all = guard.get_or_insert_with(|| seed(now, user, iso));
    Some(match (method, parts) {
        ("GET", ["api", "v9", "guilds", _, "requests"]) => {
            let status = query(q, "status").unwrap_or_else(|| "SUBMITTED".into());
            if !STATUSES.contains(&status.as_str()) {
                return Some(bad("status", "Value must be one of ('STARTED', 'SUBMITTED', 'REJECTED', 'APPROVED')."));
            }
            let limit = query(q, "limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(25);
            if !(1..=100).contains(&limit) {
                return Some(bad("limit", "Must be between 1 and 100."));
            }
            let before = query(q, "before").and_then(|b| b.parse::<u64>().ok());
            let after = query(q, "after").and_then(|b| b.parse::<u64>().ok());
            let mut rows: Vec<&Value> = all.iter().filter(|r| r["application_status"] == status.as_str()).filter(|r| before.is_none_or(|b| key(r) < b) && after.is_none_or(|a| key(r) > a)).collect();
            rows.sort_by_key(|r| key(r));
            if after.is_none() {
                rows.reverse();
            }
            let total = all.iter().filter(|r| r["application_status"] == "SUBMITTED").count();
            let page: Vec<&Value> = rows.into_iter().take(limit).collect();
            (200, Some(json!({ "guild_join_requests": page, "total": total })), None)
        }
        ("GET", ["api", "v9", "guilds", _, "requests", "users", u]) => (200, Some(json!(all.iter().filter(|r| r["user_id"] == *u).collect::<Vec<_>>())), None),
        ("PATCH", ["api", "v9", "guilds", g, "requests", id]) => {
            let Some(r) = all.iter_mut().find(|r| r["join_request_id"] == *id) else { return Some((404, Some(json!({ "message": "Unknown Join Request", "code": 10000 })), None)) };
            if r["application_status"] != "SUBMITTED" {
                return Some(bad("action", "This application has already been reviewed."));
            }
            let action = v["action"].as_str().unwrap_or_default();
            if !["APPROVED", "REJECTED"].contains(&action) {
                return Some(bad("action", "Value must be one of ('APPROVED', 'REJECTED')."));
            }
            let why = v["rejection_reason"].as_str().unwrap_or_default();
            if action == "APPROVED" && !why.is_empty() {
                return Some(bad("rejection_reason", "Only a rejection has a reason."));
            }
            if why.chars().count() > 160 {
                return Some(bad("rejection_reason", "Must be 160 or fewer in length."));
            }
            r["application_status"] = json!(action);
            r["actioned_at"] = json!(snow(now).to_string());
            r["actioned_by_user"] = user(ME);
            r["rejection_reason"] = if why.is_empty() { Value::Null } else { json!(why) };
            let out = r.clone();
            (200, Some(out.clone()), Some(("GUILD_JOIN_REQUEST_UPDATE", json!({ "guild_id": g, "status": action, "request": out }))))
        }
        _ => {
            let n = all.len() as u64;
            let r = make(snow(now) + n, user(104 + n % 3), "SUBMITTED", iso(now), "Just arrived.");
            all.push(r.clone());
            (200, Some(r.clone()), Some(("GUILD_JOIN_REQUEST_CREATE", json!({ "guild_id": "1001", "status": "SUBMITTED", "request": r }))))
        }
    })
}
