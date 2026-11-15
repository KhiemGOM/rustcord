//! Student Hubs and Share Nitro: a school email finds its hubs or the waitlist and a mailed
//! code joins one; a Nitro member's three trials, who may take one and who was sent one.

use serde_json::{Value, json};
use std::sync::Mutex;

static SENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub const CODE: &str = "246810";
const HUBS: [(&str, &str); 2] = [("1003", "Emoji Lounge University"), ("1001", "Mock Institute")];
const TRIALS: usize = 3;

fn bad(field: &str, why: &str) -> (u16, Value) {
    (400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } }))
}

pub fn route(method: &str, parts: &[&str], body: &[u8], user: impl Fn(u64) -> Value) -> Option<(u16, Value)> {
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut sent = SENT.lock().unwrap();
    Some(match (method, parts) {
        ("POST", ["api", "v9", "mock", "school-reset"]) => {
            sent.clear();
            (200, json!({}))
        }
        ("POST", ["api", "v9", "guilds", "automations", "email-domain-lookup"]) => {
            let email = b["email"].as_str().unwrap_or("");
            let Some((_, domain)) = email.split_once('@') else { return Some(bad("email", "Not a well formed email address.")) };
            let hubs: Vec<Value> = if domain == "mock.edu" { HUBS.iter().map(|(id, name)| json!({ "id": id, "name": name, "icon": null })).collect() } else { Vec::new() };
            match b["guild_id"].as_str() {
                Some(g) if hubs.iter().any(|h| h["id"] == g) => (200, json!({ "guilds_info": hubs, "has_matching_guild": true })),
                Some(_) => bad("guild_id", "Unknown Guild"),
                None => (200, json!({ "guilds_info": hubs, "has_matching_guild": false })),
            }
        }
        ("POST", ["api", "v9", "guilds", "automations", "email-domain-lookup", "verify-code"]) => {
            let hub = b["guild_id"].as_str().filter(|g| HUBS.iter().any(|h| h.0 == *g));
            match hub {
                Some(g) if b["code"] == CODE && b["email"].as_str().is_some_and(|e| e.ends_with("@mock.edu")) => (200, json!({ "guild": { "id": g }, "joined": true })),
                Some(_) => bad("code", "Invalid verification code"),
                None => bad("guild_id", "Unknown Guild"),
            }
        }
        ("POST", ["api", "v9", "hub-waitlist", "signup"]) => match (b["email"].as_str(), b["school"].as_str()) {
            (Some(e), Some(s)) if e.contains('@') && !s.is_empty() => (200, json!({ "email": e, "email_domain": e.rsplit('@').next(), "school": s })),
            _ => bad("school", "This field is required"),
        },
        ("GET", ["api", "v9", "users", "@me", "referrals", "eligibility"]) => (200, json!({
            "referrals_remaining": TRIALS - sent.len(), "sent_user_ids": *sent, "has_eligible_friends": true,
            "recipient_status": sent.iter().map(|u| (u.clone(), json!(2))).collect::<serde_json::Map<_, _>>(),
        })),
        ("POST", ["api", "v9", "users", "@me", "referrals", "eligible-users"]) => {
            let q = b["search_query"].as_str().unwrap_or("").to_lowercase();
            let users: Vec<Value> = [101, 102, 103].into_iter().map(user).filter(|u| !sent.iter().any(|s| u["id"] == s.as_str()) && u["username"].as_str().is_some_and(|n| n.to_lowercase().contains(&q))).collect();
            (200, json!({ "users": users }))
        }
        ("POST", ["api", "v9", "users", "@me", "referrals", id]) => {
            if sent.len() >= TRIALS || sent.iter().any(|s| s == id) || !["101", "102", "103"].contains(id) {
                (400, json!({ "message": "This user cannot be sent a trial", "code": 50035 }))
            } else {
                sent.push(id.to_string());
                (200, json!({ "id": format!("88{id}"), "referrer_id": "100", "user_id": id }))
            }
        }
        _ => return None,
    })
}
