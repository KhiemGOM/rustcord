//! The account side of the mock: second factors and the sudo tickets that guard them, the
//! settings blob, sessions, connections, authorized apps, blocked people and the profile.
//! The password is `hunter2`, every six-digit code is `123456`, the backup code `abcd1234`.

use base64::Engine;
use serde_json::{Map, Value, json};
use std::sync::Mutex;

#[allow(dead_code)]
#[path = "../../src/ui/settings/wire.rs"]
mod wire;

const PASSWORD: &str = "hunter2";
const CODE: &str = "123456";
const TOKEN: &str = "mock-token";

struct Acct {
    me: Map<String, Value>,
    mfa: bool,
    settings: Vec<u8>,
    sessions: Vec<Value>,
    connections: Vec<Value>,
    oauth: Vec<Value>,
    blocked: Vec<u64>,
    ignored: Vec<u64>,
    keys: Vec<Value>,
    harvested: bool,
    tickets: Vec<String>,
    tokens: Vec<String>,
    profile: Map<String, Value>,
    recent: Vec<Value>,
    widgets: Vec<Value>,
    guild_profile: Map<String, Value>,
    password: String,
    nonce: u64,
    initial: Vec<u8>,
}

static STATE: Mutex<Option<Acct>> = Mutex::new(None);

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn session(hash: &str, os: &str, platform: &str, loc: &str, when: &str) -> Value {
    json!({ "id_hash": hash, "approx_last_used_time": when, "client_info": { "os": os, "platform": platform, "location": loc } })
}

pub fn init(settings_b64: &str) {
    let settings = base64::engine::general_purpose::STANDARD.decode(settings_b64).unwrap_or_default();
    *STATE.lock().unwrap() = Some(fresh(settings));
}

fn fresh(settings: Vec<u8>) -> Acct {
    let mut me = Map::new();
    me.insert("mfa_enabled".into(), json!(false));
    Acct {
        me,
        mfa: false,
        initial: settings.clone(),
        settings,
        sessions: vec![
            session("aaa", "Windows", "Discord Client", "Bangkok, Thailand", "2026-10-07T10:00:00+00:00"),
            session("bbb", "Android", "Discord Android", "Bangkok, Thailand", "2026-10-01T10:00:00+00:00"),
            session("ccc", "iOS", "Discord iOS", "Hanoi, Vietnam", "2026-09-20T10:00:00+00:00"),
        ],
        connections: vec![
            json!({ "type": "github", "id": "1", "name": "you-on-github", "visibility": 1, "show_activity": true, "verified": true }),
            json!({ "type": "spotify", "id": "2", "name": "you-on-spotify", "visibility": 0, "show_activity": true, "verified": true }),
            json!({ "type": "twitch", "id": "3", "name": "you-on-twitch", "visibility": 0, "verified": true, "integrations": [{ "id": "7001", "guild": { "id": "999", "name": "ghostie's graveyard", "icon": null }, "account": { "id": "t1", "name": "ghostiefruit" } }] }),
        ],
        oauth: vec![
            json!({ "id": "1290000000000000000", "scopes": ["identify", "guilds"], "application": { "id": "104", "name": "Dyno", "icon": null, "description": "A multipurpose bot.", "terms_of_service_url": "https://dyno.example/terms", "privacy_policy_url": "https://dyno.example/privacy" } }),
            json!({ "id": "1280000000000000000", "scopes": ["identify", "email", "applications.commands"], "application": { "id": "105", "name": "Polar Anticheat", "icon": null, "description": "" } }),
        ],
        blocked: vec![105],
        ignored: vec![104],
        keys: vec![json!({ "id": "9001", "name": "YubiKey 5", "last_used": "2026-11-01T10:00:00+00:00" })],
        harvested: false,
        tickets: Vec::new(),
        tokens: Vec::new(),
        profile: Map::new(),
        recent: vec![json!({ "id": "1", "hash": "recent1", "color": "668CE2" }), json!({ "id": "2", "hash": "recent2", "color": "A1B2C3" })],
        widgets: Vec::new(),
        guild_profile: Map::new(),
        password: PASSWORD.into(),
        nonce: 1,
    }
}

fn err(code: u16, msg: &str) -> (u16, Value) {
    (code, json!({ "message": msg, "code": 50035 }))
}

fn guarded(a: &mut Acct, method: &str, p: &[&str], body: &Value, token: Option<&str>) -> Option<(u16, Value)> {
    let is = match (method, p) {
        ("PATCH", ["users", "@me"]) => body["new_password"].is_string(),
        ("POST", ["users", "@me", "disable" | "delete"]) => true,
        ("POST", ["users", "@me", "mfa", "totp", "enable" | "disable"]) => true,
        ("POST", ["users", "@me", "mfa", "sms", "enable"]) => true,
        ("POST", ["users", "@me", "mfa", "webauthn", "credentials"]) => true,
        ("POST", ["auth", "sessions", "logout"]) => true,
        ("POST", ["users", "@me", "mfa", "codes-verification"]) => true,
        ("PATCH" | "DELETE", ["users", "@me", "mfa", "webauthn", "credentials", _]) => true,
        _ => false,
    };
    if !is {
        return None;
    }
    let always = matches!(p, ["users", "@me", "mfa", "totp", "enable"]);
    if !(a.mfa || always) {
        return None;
    }
    if let Some(t) = token
        && let Some(i) = a.tokens.iter().position(|x| x == t)
    {
        a.tokens.remove(i);
        return None;
    }
    a.nonce += 1;
    let ticket = format!("ticket-{}", a.nonce);
    a.tickets.push(ticket.clone());
    let methods = if a.mfa {
        json!([{ "type": "totp" }, { "type": "sms" }, { "type": "backup" }, { "type": "password" }, { "type": "webauthn", "challenge": challenge() }])
    } else {
        json!([{ "type": "password" }])
    };
    Some((401, json!({ "message": "Two factor is required for this operation", "code": 60003, "mfa": { "ticket": ticket, "methods": methods } })))
}

/// What discord refuses in the status settings (11): a status that isn't one of the four, and a
/// custom status longer than 128 characters.
fn status_refusal(patch: &[u8]) -> Option<&'static str> {
    let text = |n: &wire::Node| match n {
        wire::Node::Bytes(b) => wire::decode(b).into_iter().find_map(|(f, v)| match (f, v) {
            (1, wire::Node::Bytes(s)) => Some(String::from_utf8_lossy(&s).into_owned()),
            _ => None,
        }),
        _ => None,
    };
    for (g, n) in wire::decode(patch) {
        let (11, wire::Node::Bytes(group)) = (g, n) else { continue };
        for (f, n) in wire::decode(&group) {
            match f {
                1 if !text(&n).is_some_and(|s| ["online", "idle", "dnd", "invisible"].contains(&s.as_str())) => return Some("Status must be online, idle, dnd or invisible."),
                2 if text(&n).is_some_and(|s| s.chars().count() > 128) => return Some("Custom status is too long."),
                _ => {}
            }
        }
    }
    None
}

fn challenge() -> String {
    json!({ "publicKey": { "challenge": "bW9jay1jaGFsbGVuZ2U", "rpId": "discord.com", "allowCredentials": [{ "type": "public-key", "id": "AQID" }], "userVerification": "preferred", "timeout": 60000 } }).to_string()
}

fn codes() -> Value {
    json!((0..8).map(|i| json!({ "code": format!("abcd123{i}"), "consumed": i == 7 })).collect::<Vec<_>>())
}

/// Some((status, body)) when the account module answers this request. `user` is the mock's
/// own user object for the account; `person` makes one for any id.
pub fn route(method: &str, path: &str, body: &[u8], mfa_token: Option<&str>, user: &Value, person: &dyn Fn(u64) -> Value) -> Option<(u16, Value)> {
    let path = path.split('?').next().unwrap_or(path);
    let p: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let p = match p.as_slice() {
        ["api", "v9", rest @ ..] => rest.to_vec(),
        _ => return None,
    };
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let mut g = STATE.lock().unwrap();
    let a = g.as_mut()?;
    if let Some(r) = guarded(a, method, &p, &b, mfa_token) {
        return Some(r);
    }
    let ok = |v: Value| Some((200, v));
    match (method, p.as_slice()) {
        ("POST", ["mock", "account-reset"]) => {
            *a = fresh(a.initial.clone());
            ok(json!({}))
        }
        ("GET", ["users", "@me", "settings-proto", "1"]) => ok(json!({ "settings": b64(&a.settings) })),
        ("PATCH", ["users", "@me", "settings-proto", "1"]) => {
            let patch = base64::engine::general_purpose::STANDARD.decode(b["settings"].as_str().unwrap_or("")).unwrap_or_default();
            if let Some(why) = status_refusal(&patch) {
                return Some(err(400, why));
            }
            a.settings = wire::merge(&a.settings, &patch);
            ok(json!({ "settings": b64(&a.settings) }))
        }
        ("GET", ["users", "@me", "harvest"]) => ok(if a.harvested { json!([{ "harvest_id": "1", "status": 0, "created_at": "2026-11-05T10:00:00Z", "completed_at": null }]) } else { json!([]) }),
        ("POST", ["users", "@me", "harvest"]) => {
            a.harvested = true;
            ok(json!({ "harvest_id": "1", "status": 0 }))
        }
        ("GET", ["safety-hub", "@me"]) => ok(json!({ "account_standing": { "state": 100 }, "classifications": [] })),
        ("POST", ["mfa", "finish"]) => {
            let t = b["ticket"].as_str().unwrap_or("");
            let data = b["data"].as_str().unwrap_or("");
            let good = match b["mfa_type"].as_str() {
                Some("password") => data == a.password,
                Some("totp") => data == CODE,
                Some("backup") => data == "abcd1234",
                Some("sms") => data == CODE,
                Some("webauthn") => serde_json::from_str::<Value>(data).is_ok_and(|c| c["response"]["signature"].is_string()),
                _ => false,
            };
            if !a.tickets.iter().any(|x| x == t) {
                return Some(err(400, "Invalid ticket"));
            }
            if !good {
                return Some(err(400, "Invalid two-factor code"));
            }
            a.tickets.retain(|x| x != t);
            let tok = format!("mfa-token-{}", a.tokens.len() + a.nonce as usize);
            a.tokens.push(tok.clone());
            ok(json!({ "token": tok }))
        }
        ("POST", ["users", "@me", "mfa", "totp", "enable"]) => {
            if b["code"].as_str() != Some(CODE) || b["secret"].as_str().is_none_or(|s| s.len() != 16) {
                return Some(err(400, "Invalid two-factor code"));
            }
            a.mfa = true;
            a.me.insert("mfa_enabled".into(), json!(true));
            ok(json!({ "token": TOKEN, "backup_codes": codes() }))
        }
        ("POST", ["users", "@me", "mfa", "totp", "disable"]) => {
            a.mfa = false;
            a.me.insert("mfa_enabled".into(), json!(false));
            ok(json!({ "token": TOKEN }))
        }
        ("POST", ["auth", "verify", "view-backup-codes-challenge"]) => {
            if b["password"].as_str() != Some(a.password.as_str()) {
                return Some(err(400, "Password does not match."));
            }
            ok(json!({ "nonce": "view-nonce", "regenerate_nonce": "regen-nonce" }))
        }
        ("POST", ["users", "@me", "mfa", "codes-verification"]) => {
            let want = if b["regenerate"].as_bool() == Some(true) { "regen-nonce" } else { "view-nonce" };
            if b["key"].as_str() != Some("EMAIL-KEY") || b["nonce"].as_str() != Some(want) {
                return Some(err(400, "Invalid verification key"));
            }
            ok(json!({ "backup_codes": codes() }))
        }
        ("POST", ["auth", "mfa", "sms", "send"]) => ok(json!({ "phone": "+*********0100" })),
        ("POST", ["users", "@me", "mfa", "sms", "enable"]) => {
            let f = a.me.get("flags").and_then(Value::as_u64).unwrap_or(0);
            a.me.insert("flags".into(), json!(f | 16));
            ok(json!({}))
        }
        ("POST", ["users", "@me", "mfa", "sms", "disable"]) => {
            if b["password"].as_str() != Some(a.password.as_str()) {
                return Some(err(400, "Password does not match."));
            }
            let f = a.me.get("flags").and_then(Value::as_u64).unwrap_or(0);
            a.me.insert("flags".into(), json!(f & !16));
            ok(json!({}))
        }
        ("POST", ["users", "@me", "mfa", "webauthn", "credentials"]) => {
            if b["credential"].is_null() {
                return ok(json!({ "ticket": "webauthn-ticket", "challenge": json!({ "publicKey": { "challenge": "bW9jaw", "rp": { "id": "discord.com", "name": "Discord" }, "user": { "id": "AQID", "name": "you", "displayName": "you" }, "pubKeyCredParams": [{ "type": "public-key", "alg": -7 }], "excludeCredentials": [] } }).to_string() }));
            }
            let cred = serde_json::from_str::<Value>(b["credential"].as_str().unwrap_or("")).unwrap_or_default();
            if b["ticket"] != "webauthn-ticket" || cred["response"]["attestationObject"].is_null() {
                return Some(err(400, "Invalid credential"));
            }
            a.nonce += 1;
            let key = json!({ "id": format!("{}", 9100 + a.nonce), "name": b["name"], "last_used": "2026-11-04T10:00:00+00:00" });
            a.keys.push(key.clone());
            let mut out = key;
            if !a.mfa {
                a.mfa = true;
                a.me.insert("mfa_enabled".into(), json!(true));
                out["backup_codes"] = codes();
            }
            ok(out)
        }
        ("GET", ["users", "@me", "mfa", "webauthn", "credentials"]) => ok(json!(a.keys)),
        ("PATCH", ["users", "@me", "mfa", "webauthn", "credentials", id]) => {
            if let Some(k) = a.keys.iter_mut().find(|k| k["id"] == *id) {
                k["name"] = b["name"].clone();
            }
            ok(json!({}))
        }
        ("DELETE", ["users", "@me", "mfa", "webauthn", "credentials", id]) => {
            a.keys.retain(|k| k["id"] != *id);
            Some((204, Value::Null))
        }
        ("PUT", ["users", "@me", "email"]) => Some((204, Value::Null)),
        ("POST", ["users", "@me", "email", "verify-code"]) => {
            if b["code"].as_str() != Some(CODE) {
                return Some(err(400, "Invalid verification code"));
            }
            ok(json!({ "token": "email-token" }))
        }
        ("POST", ["users", "@me", "phone"]) => {
            if b["phone_token"].is_string() {
                if b["phone_token"] != "phone-token" || b["password"].as_str() != Some(a.password.as_str()) {
                    return Some(err(400, "Password does not match."));
                }
                a.me.insert("phone".into(), json!(a.profile.get("pending_phone").cloned().unwrap_or(json!("+15555550100"))));
                return ok(json!({}));
            }
            a.profile.insert("pending_phone".into(), b["phone"].clone());
            Some((204, Value::Null))
        }
        ("DELETE", ["users", "@me", "phone"]) => {
            if b["password"].as_str() != Some(a.password.as_str()) {
                return Some(err(400, "Password does not match."));
            }
            a.me.insert("phone".into(), Value::Null);
            Some((204, Value::Null))
        }
        ("POST", ["phone-verifications", "verify"]) => {
            if b["code"].as_str() != Some(CODE) {
                return Some(err(400, "Invalid verification code"));
            }
            ok(json!({ "token": "phone-token" }))
        }
        ("PATCH", ["users", "@me"]) => {
            let pw = b["password"].as_str();
            if (b["username"].is_string() || b["new_password"].is_string() || b["email"].is_string()) && pw != Some(a.password.as_str()) {
                return Some(err(400, "Password does not match."));
            }
            if b["email"].is_string() && b["email_token"] != "email-token" {
                return Some(err(400, "Verify your old email first."));
            }
            for k in ["username", "email", "global_name"] {
                if !b[k].is_null() || b.get(k).is_some() && k == "global_name" {
                    a.me.insert(k.into(), b[k].clone());
                }
            }
            if let Some(n) = b["new_password"].as_str() {
                a.password = n.into();
            }
            let nitro = a.me.get("premium_type").and_then(Value::as_u64).unwrap_or(2) != 0;
            if let Some(e) = super::profiles::check_account(&b, nitro) {
                return Some(e);
            }
            if b["avatar"].is_string() {
                let n = a.recent.len() + 3;
                a.recent.insert(0, json!({ "id": n.to_string(), "hash": format!("e2eavatar{n}"), "color": "336699" }));
                a.recent.truncate(6);
                a.me.insert("avatar".into(), json!(format!("e2eavatar{n}")));
            } else if let Some(id) = b["avatar_id"].as_str() {
                let hash = a.recent.iter().find(|r| r["id"] == id).map(|r| r["hash"].clone());
                match hash {
                    Some(h) => {
                        a.me.insert("avatar".into(), h);
                    }
                    None => return Some(err(400, "Unknown avatar.")),
                }
            } else if b.get("avatar").is_some() {
                a.me.insert("avatar".into(), Value::Null);
            }
            if b["display_name_font_id"].is_u64() {
                a.me.insert("display_name_styles".into(), json!({ "font_id": b["display_name_font_id"], "effect_id": b["display_name_effect_id"], "colors": b["display_name_colors"] }));
            } else if b.get("display_name_font_id").is_some() {
                a.me.insert("display_name_styles".into(), Value::Null);
            }
            if let Some(g) = b.get("primary_guild_id") {
                let tag = if g.is_null() { json!({ "identity_enabled": false, "identity_guild_id": null, "tag": null, "badge": null }) } else { json!({ "identity_enabled": true, "identity_guild_id": g, "tag": "RUST", "badge": "" }) };
                a.me.insert("primary_guild".into(), tag);
            }
            if let Some(sku) = b.get("avatar_decoration_sku_id") {
                a.me.insert("avatar_decoration_data".into(), if sku.is_null() { Value::Null } else { json!({ "asset": format!("a_deco{}", sku.as_str().unwrap_or("")), "sku_id": sku }) });
            }
            if let Some(sku) = b.get("nameplate_sku_id") {
                let np = if sku.is_null() { Value::Null } else { json!({ "asset": format!("nameplates/mock/{}/", sku.as_str().unwrap_or("")), "sku_id": sku, "label": "Mock", "palette": "cobalt" }) };
                a.me.insert("collectibles".into(), if np.is_null() { Value::Null } else { json!({ "nameplate": np }) });
            }
            let mut u = user.clone();
            for (k, v) in &a.me {
                u[k] = v.clone();
            }
            if b["new_password"].is_string() {
                u["token"] = json!(TOKEN);
            }
            ok(u)
        }
        ("POST", ["users", "@me", "disable" | "delete"]) => {
            if b["password"].as_str() != Some(a.password.as_str()) {
                return Some(err(400, "Password does not match."));
            }
            Some((204, Value::Null))
        }
        ("GET", ["auth", "sessions"]) => ok(json!({ "user_sessions": a.sessions })),
        ("POST", ["auth", "sessions", "logout"]) => {
            let gone: Vec<&str> = b["session_id_hashes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            a.sessions.retain(|s| !gone.contains(&s["id_hash"].as_str().unwrap_or("")));
            Some((204, Value::Null))
        }
        ("GET", ["users", "@me", "connections"]) => ok(json!(a.connections)),
        ("PATCH", ["users", "@me", "connections", kind, id]) => {
            if let Some(c) = a.connections.iter_mut().find(|c| c["type"] == *kind && c["id"] == *id) {
                for (k, v) in b.as_object().into_iter().flatten() {
                    c[k] = v.clone();
                }
            }
            ok(json!({}))
        }
        ("DELETE", ["users", "@me", "connections", kind, id]) => {
            a.connections.retain(|c| !(c["type"] == *kind && c["id"] == *id));
            Some((204, Value::Null))
        }
        ("POST", ["integrations", _, "join"]) => Some((204, Value::Null)),
        ("GET", ["connections", kind, "authorize"]) => ok(json!({ "url": format!("mock://oauth/{kind}") })),
        ("GET", ["oauth2", "tokens"]) => ok(json!(a.oauth)),
        ("DELETE", ["oauth2", "tokens", id]) => {
            a.oauth.retain(|t| t["id"] != *id);
            Some((204, Value::Null))
        }
        ("GET", ["users", "@me", "relationships"]) => {
            let mut v: Vec<Value> = vec![json!({ "id": "101", "type": 1, "user": person(101) })];
            v.extend(a.blocked.iter().map(|id| json!({ "id": id.to_string(), "type": 2, "user": person(*id) })));
            v.extend(a.ignored.iter().map(|id| json!({ "id": id.to_string(), "type": 0, "user_ignored": true, "user": person(*id) })));
            ok(json!(v))
        }
        ("DELETE", ["users", "@me", "relationships", id]) => {
            let id: u64 = id.parse().unwrap_or(0);
            a.blocked.retain(|x| *x != id);
            Some((204, Value::Null))
        }
        ("DELETE", ["users", "@me", "relationships", id, "ignore"]) => {
            let id: u64 = id.parse().unwrap_or(0);
            a.ignored.retain(|x| *x != id);
            Some((204, Value::Null))
        }
        ("GET", ["users", "@me", "avatars"]) => ok(json!(a.recent)),
        ("DELETE", ["users", "@me", "avatars", id]) => {
            let before = a.recent.len();
            a.recent.retain(|r| r["id"] != *id);
            if a.recent.len() == before { Some(err(404, "Unknown avatar.")) } else { Some((204, Value::Null)) }
        }
        ("PUT", ["users", "@me", "widgets"]) => {
            if let Some(e) = super::profiles::check_widgets(&b) {
                return Some(e);
            }
            a.nonce += 1;
            let list: Vec<Value> = b["widgets"].as_array().into_iter().flatten().enumerate().map(|(i, w)| json!({ "id": w["id"].as_str().map_or_else(|| format!("w{}{i}", a.nonce), str::to_string), "updated_at": "2026-11-06T10:00:00+00:00", "data": w["data"] })).collect();
            a.widgets = list.clone();
            ok(json!({ "widgets": list }))
        }
        ("PATCH", ["users", "@me", "profile"]) => {
            let nitro = a.me.get("premium_type").and_then(Value::as_u64).unwrap_or(2) != 0;
            if let Some(e) = super::profiles::check_profile(&b, nitro) {
                return Some(e);
            }
            for (k, v) in b.as_object().into_iter().flatten() {
                a.profile.insert(k.clone(), v.clone());
            }
            ok(json!(a.profile))
        }
        ("PATCH", ["guilds", _, "profile", "@me"]) => {
            if let Some(e) = super::profiles::check_profile(&b, true) {
                return Some(e);
            }
            for (k, v) in b.as_object().into_iter().flatten() {
                a.guild_profile.insert(k.clone(), v.clone());
            }
            ok(json!(a.guild_profile))
        }
        ("GET", ["users", id, "profile"]) if *id == "100" || *id == "@me" => {
            let mut prof = json!({ "bio": "rustacean. writes too many side projects.", "accent_color": 0xE67E22, "pronouns": "" });
            for (k, v) in &a.profile {
                prof[k] = v.clone();
            }
            let mut u = user.clone();
            for (k, v) in &a.me {
                u[k] = v.clone();
            }
            if let Some(e) = a.profile.get("collectibles_sku_ids").and_then(|l| l.get(0)) {
                prof["profile_effect"] = json!({ "id": e });
            }
            if let Some(e) = a.profile.get("profile_frame_id").filter(|e| !e.is_null()) {
                prof["profile_frame"] = json!({ "id": e });
            }
            if a.profile.get("banner").is_some_and(Value::is_string) {
                prof["banner"] = json!("e2ebanner");
                u["banner"] = json!("e2ebanner");
            } else if a.profile.get("banner").is_some() {
                prof["banner"] = Value::Null;
            }
            let accounts: Vec<Value> = a.connections.iter().map(|c| json!({ "type": c["type"], "id": c["id"], "name": c["name"], "verified": true })).collect();
            let mut out = json!({
                "user": u, "user_profile": prof, "connected_accounts": accounts, "widgets": a.widgets,
                "badges": [
                    { "id": "premium", "description": "Subscriber since 12/2/25", "icon": "2ba85e8901a8d6d1b8e4d9f2b7a6c3d1", "link": "https://discord.com/settings/premium" },
                    { "id": "hypesquad_house_1", "description": "HypeSquad Bravery", "icon": "8a88d63823d8a71cd5e390baa45efa02", "link": "https://support.discord.com/hc/en-us/articles/360035962891" },
                    { "id": "quest_completed", "description": "Completed a Quest", "icon": "7d9ae358c8c5e118768335dbe68b4fb8", "link": "https://discord.com/quests" },
                    { "id": "legendary_gifter", "description": "Gifting LEGEND", "icon": "5e0ab9d21c3c4f0c8b0f7f2d1e1d9a77", "link": "https://discord.com/settings/gift-inventory" },
                ],
            });
            if !a.guild_profile.is_empty() {
                out["guild_member_profile"] = json!(a.guild_profile);
            }
            out["wishlist_settings"] = json!({ "7700": { "visibility": 1, "updated_at": "2026-11-01T10:00:00+00:00" } });
            ok(out)
        }
        _ => None,
    }
}
