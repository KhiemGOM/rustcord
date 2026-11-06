//! The member side of a guild in the mock: the members table's search, kicks, bans, timeouts,
//! roles and prune, kept in state so a table asked again after an action shows its result, and
//! refusing what Discord refuses (the owner and yourself, admins for timeouts, bad bodies).

use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const OWNER: u64 = 101;
const ME: u64 = 100;
const IDS: [u64; 7] = [101, 102, 103, 104, 105, 106, 107];
const ROLES: [u64; 5] = [1100, 1101, 1102, 1103, 1104];
const ADMIN: &str = "1100";
const DAY: u64 = 86_400_000;
const BAN_DELETE_MAX: u64 = 604_800;
const BULK_MAX: usize = 200;

pub struct Base {
    pub user: fn(u64) -> Value,
    pub member: fn(u64) -> Value,
    pub iso: fn(u64) -> String,
    pub now: u64,
}

#[derive(Default)]
struct State {
    gone: HashSet<u64>,
    banned: Vec<(u64, String)>,
    roles: HashMap<u64, Vec<String>>,
    timeout: HashMap<u64, u64>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn fresh(now: u64) -> State {
    let mut s = State::default();
    s.banned.push((104, "spamming links".into()));
    s.roles.insert(106, vec![]);
    s.roles.insert(107, vec![]);
    s.timeout.insert(103, now + DAY);
    s
}

fn fail(code: u16, n: u64, message: &str) -> (u16, Option<Value>, Option<(&'static str, Value)>) {
    (code, Some(json!({ "message": message, "code": n })), None)
}

fn missing() -> (u16, Option<Value>, Option<(&'static str, Value)>) {
    fail(403, 50013, "Missing Permissions")
}

fn bad(field: &str, why: &str) -> (u16, Option<Value>, Option<(&'static str, Value)>) {
    (400, Some(json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_BAD", "message": why }] } } })), None)
}

fn id(s: &str) -> Option<u64> {
    s.parse().ok()
}

fn roles_of(st: &State, b: &Base, u: u64) -> Vec<String> {
    st.roles.get(&u).cloned().unwrap_or_else(|| (b.member)(u)["roles"].as_array().into_iter().flatten().filter_map(|r| r.as_str().map(String::from)).collect())
}

fn member(st: &State, b: &Base, u: u64) -> Value {
    let mut m = (b.member)(u);
    m["roles"] = json!(roles_of(st, b, u));
    m["user"] = (b.user)(u);
    m["communication_disabled_until"] = st.timeout.get(&u).filter(|t| **t > b.now).map_or(Value::Null, |t| json!((b.iso)(*t)));
    m
}

fn is_admin(st: &State, b: &Base, u: u64) -> bool {
    u == OWNER || roles_of(st, b, u).iter().any(|r| r == ADMIN)
}

fn update(st: &State, b: &Base, g: &str, u: u64) -> (&'static str, Value) {
    let mut m = member(st, b, u);
    m["guild_id"] = json!(g);
    ("GUILD_MEMBER_UPDATE", m)
}

fn present(st: &State, u: u64) -> bool {
    IDS.contains(&u) && !st.gone.contains(&u)
}

fn ban(st: &mut State, u: u64, reason: &str) {
    st.gone.insert(u);
    if !st.banned.iter().any(|b| b.0 == u) {
        st.banned.push((u, reason.into()));
    }
}

fn search(st: &State, b: &Base, body: &Value) -> (u16, Option<Value>, Option<(&'static str, Value)>) {
    let sort = body["sort"].as_u64().unwrap_or(1);
    if !(1..=4).contains(&sort) {
        return bad("sort", "Value must be one of (1, 2, 3, 4).");
    }
    let limit = body["limit"].as_u64().unwrap_or(25);
    if !(1..=250).contains(&limit) {
        return bad("limit", "Must be between 1 and 250.");
    }
    let strs = |v: &Value| -> Vec<String> { v["or_query"].as_array().into_iter().flatten().filter_map(|w| w.as_str().map(str::to_lowercase)).collect() };
    let words = strs(&body["and_query"]["usernames"]);
    let ids = strs(&body["and_query"]["user_id"]);
    let mut rows: Vec<(u64, u64)> = IDS
        .iter()
        .filter(|u| present(st, **u))
        .map(|u| (*u, b.now - (IDS.iter().position(|x| x == u).unwrap_or(0) as u64 + 1) * 5 * DAY))
        .filter(|(u, _)| {
            let name = (b.user)(*u)["username"].as_str().unwrap_or_default().to_lowercase();
            (words.is_empty() || words.iter().any(|w| name.contains(w.as_str()))) && (ids.is_empty() || ids.contains(&u.to_string()))
        })
        .collect();
    match sort {
        1 => rows.sort_by_key(|r| std::cmp::Reverse(r.1)),
        2 => rows.sort_by_key(|r| r.1),
        3 => rows.sort_by_key(|r| std::cmp::Reverse(r.0)),
        _ => rows.sort_by_key(|r| r.0),
    }
    let total = rows.len();
    let list: Vec<Value> = rows
        .iter()
        .take(limit as usize)
        .map(|(u, joined)| {
            let mut m = member(st, b, *u);
            m["joined_at"] = json!((b.iso)(*joined));
            let code = (*u != 104).then_some("e2eInv1");
            let mut row = json!({ "member": m, "source_invite_code": code, "join_source_type": if code.is_some() { 5 } else { 1 }, "inviter_id": code.map(|_| "102") });
            if *u == 105 {
                row["member"]["unusual_dm_activity_until"] = json!((b.iso)(b.now + 2 * DAY));
            }
            row
        })
        .collect();
    (200, Some(json!({ "guild_id": "1000", "members": list, "page_result_count": list.len(), "total_result_count": total })), None)
}

fn prunable(st: &State, b: &Base) -> Vec<u64> {
    IDS.iter().copied().filter(|u| present(st, *u) && roles_of(st, b, *u).is_empty()).collect()
}

fn days(q: Option<&str>, body: &Value) -> Option<u64> {
    let d = q.and_then(|q| q.split('&').find_map(|p| p.strip_prefix("days=")).and_then(|d| d.parse().ok())).or(body["days"].as_u64())?;
    (1..=30).contains(&d).then_some(d)
}

/// What the member endpoints answer: status, body and a gateway event, or None when the path isn't theirs.
pub fn route(method: &str, parts: &[&str], query: Option<&str>, body: &[u8], b: &Base) -> Option<(u16, Option<Value>, Option<(&'static str, Value)>)> {
    let v: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut guard = STATE.lock().unwrap();
    let st = guard.get_or_insert_with(|| fresh(b.now));
    let gone = |g: &str, u: u64| ("GUILD_MEMBER_REMOVE", json!({ "guild_id": g, "user": (b.user)(u) }));
    Some(match (method, parts) {
        ("POST", ["api", "v9", "guilds", _, "members-search"]) => search(st, b, &v),
        ("DELETE", ["api", "v9", "guilds", g, "members", u]) if *u != "@me" => {
            let u = id(u)?;
            if !present(st, u) {
                return Some(fail(404, 10007, "Unknown Member"));
            }
            if u == OWNER || u == ME {
                return Some(missing());
            }
            st.gone.insert(u);
            (204, None, Some(gone(g, u)))
        }
        ("PUT", ["api", "v9", "guilds", g, "bans", u]) => {
            let u = id(u)?;
            if let Some(s) = v.get("delete_message_seconds").filter(|s| !s.is_null()) {
                if !s.as_u64().is_some_and(|s| s <= BAN_DELETE_MAX) {
                    return Some(bad("delete_message_seconds", "Must be between 0 and 604800."));
                }
            }
            if u == OWNER || u == ME {
                return Some(missing());
            }
            ban(st, u, v["reason"].as_str().unwrap_or_default());
            (204, None, Some(gone(g, u)))
        }
        ("DELETE", ["api", "v9", "guilds", _, "bans", u]) => {
            let u = id(u)?;
            if !st.banned.iter().any(|b| b.0 == u) {
                return Some(fail(404, 10026, "Unknown Ban"));
            }
            st.banned.retain(|b| b.0 != u);
            st.gone.remove(&u);
            (204, None, None)
        }
        ("GET", ["api", "v9", "guilds", _, "bans"]) => (200, Some(json!(st.banned.iter().map(|(u, r)| json!({ "user": (b.user)(*u), "reason": r })).collect::<Vec<_>>())), None),
        ("POST", ["api", "v9", "guilds", g, "bulk-ban"]) => {
            let Some(list) = v["user_ids"].as_array().filter(|l| !l.is_empty()) else { return Some(bad("user_ids", "This field is required")) };
            if list.len() > BULK_MAX {
                return Some(bad("user_ids", "Must be 200 or fewer in length."));
            }
            let Some(ids): Option<Vec<u64>> = list.iter().map(|x| x.as_str().and_then(id)).collect() else { return Some(bad("user_ids", "Snowflake type should be a string of digits.")) };
            let (mut done, mut failed) = (vec![], vec![]);
            for u in ids {
                if u == OWNER || u == ME {
                    failed.push(u.to_string());
                } else {
                    ban(st, u, "");
                    done.push(u.to_string());
                }
            }
            if done.is_empty() {
                return Some(fail(500, 500000, "Failed to ban users"));
            }
            let first = done[0].parse().unwrap_or(0);
            (200, Some(json!({ "banned_users": done, "failed_users": failed })), Some(gone(g, first)))
        }
        ("PATCH", ["api", "v9", "guilds", g, "members", u]) if *u != "@me" => {
            let u = id(u)?;
            if !present(st, u) {
                return Some(fail(404, 10007, "Unknown Member"));
            }
            if let Some(r) = v.get("roles") {
                let Some(list): Option<Vec<String>> = r.as_array().and_then(|a| a.iter().map(|x| x.as_str().map(String::from)).collect()) else { return Some(bad("roles", "Must be a list of snowflakes.")) };
                if list.iter().any(|r| !id(r).is_some_and(|r| ROLES.contains(&r))) {
                    return Some(bad("roles", "Invalid role."));
                }
                st.roles.insert(u, list);
            }
            if let Some(t) = v.get("communication_disabled_until") {
                if t.is_null() {
                    st.timeout.remove(&u);
                } else {
                    let Some(until) = t.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis().max(0) as u64) else {
                        return Some(bad("communication_disabled_until", "Could not parse as datetime."));
                    };
                    if until <= b.now || until > b.now + 28 * DAY {
                        return Some(bad("communication_disabled_until", "Must be within 28 days from now."));
                    }
                    if is_admin(st, b, u) {
                        return Some(missing());
                    }
                    st.timeout.insert(u, until);
                }
            }
            (200, Some(member(st, b, u)), Some(update(st, b, g, u)))
        }
        ("PUT" | "DELETE", ["api", "v9", "guilds", g, "members", u, "roles", r]) => {
            let (u, r) = (id(u)?, id(r)?);
            if !present(st, u) {
                return Some(fail(404, 10007, "Unknown Member"));
            }
            if !ROLES.contains(&r) {
                return Some(fail(404, 10011, "Unknown Role"));
            }
            let mut list = roles_of(st, b, u);
            list.retain(|x| *x != r.to_string());
            if method == "PUT" {
                list.push(r.to_string());
            }
            st.roles.insert(u, list);
            (204, None, Some(update(st, b, g, u)))
        }
        ("GET", ["api", "v9", "guilds", _, "prune"]) => match days(query, &Value::Null) {
            Some(_) => (200, Some(json!({ "pruned": prunable(st, b).len() })), None),
            None => bad("days", "Must be between 1 and 30."),
        },
        ("POST", ["api", "v9", "guilds", g, "prune"]) => {
            if days(None, &v).is_none() {
                return Some(bad("days", "Must be between 1 and 30."));
            }
            let list = prunable(st, b);
            for u in &list {
                st.gone.insert(*u);
            }
            let pruned = if v["compute_prune_count"].as_bool() == Some(false) { Value::Null } else { json!(list.len()) };
            (200, Some(json!({ "pruned": pruned })), list.first().map(|u| gone(g, *u)))
        }
        _ => return None,
    })
}
