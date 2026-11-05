//! The mock's login side: auth REST routes and a remote-auth gateway that
//! encrypts to whatever RSA key the client sends, like Discord's does.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use num_bigint::BigUint;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as Ws;

pub const TOKEN: &str = "mock-token";

static SCAN: std::sync::LazyLock<tokio::sync::broadcast::Sender<()>> = std::sync::LazyLock::new(|| tokio::sync::broadcast::channel(4).0);
static LOGINS: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);
static TICKETS: Mutex<Option<HashMap<String, Vec<u8>>>> = Mutex::new(None);

fn field(name: &str, code: &str, message: &str) -> Value {
    json!({ name: { "_errors": [{ "code": code, "message": message }] } })
}

fn form(fields: Vec<Value>) -> (u16, Option<Value>) {
    let mut errors = serde_json::Map::new();
    for f in fields {
        errors.extend(f.as_object().cloned().unwrap_or_default());
    }
    (400, Some(json!({ "code": 50035, "errors": errors, "message": "Invalid Form Body" })))
}

fn captcha() -> (u16, Option<Value>) {
    (400, Some(json!({
        "captcha_key": ["captcha-required"], "captcha_sitekey": "10000000-ffff-ffff-ffff-000000000001",
        "captcha_service": "hcaptcha", "captcha_rqdata": "mock-rqdata", "captcha_rqtoken": "mock-rqtoken",
    })))
}

fn solved(b: &Value) -> bool {
    b["captcha_key"] == "e2e-captcha"
}

fn login(b: &Value) -> (u16, Option<Value>) {
    let who = b["login"].as_str().unwrap_or("");
    if b["password"] != "hunter2" {
        let bad = |n| field(n, "INVALID_LOGIN", "Login or password is invalid.");
        return form(vec![bad("login"), bad("password")]);
    }
    match who {
        "captcha@mock.test" if !solved(b) => captcha(),
        "mfa@mock.test" => (200, Some(json!({ "mfa": true, "ticket": "mfa-ticket", "totp": true, "sms": true, "backup": true, "webauthn": null, "user_id": "100" }))),
        "newloc@mock.test" => {
            let mut g = LOGINS.lock().unwrap();
            let n = g.get_or_insert_default().entry(who.to_string()).or_default();
            *n += 1;
            if *n == 1 {
                return form(vec![field("login", "ACCOUNT_LOGIN_VERIFICATION_EMAIL", "New login location detected, please check your e-mail.")]);
            }
            (200, Some(json!({ "token": TOKEN })))
        }
        "disabled@mock.test" => (400, Some(json!({ "code": 20013, "message": "This account is disabled." }))),
        "deleted@mock.test" if b["undelete"] != true => (400, Some(json!({ "code": 20011, "message": "Account scheduled for deletion." }))),
        _ => (200, Some(json!({ "token": TOKEN }))),
    }
}

fn mfa(kind: &str, b: &Value) -> (u16, Option<Value>) {
    let want = match kind {
        "totp" => "123456",
        "sms" => "654321",
        "backup" => "abcd1234",
        "webauthn" => "mock-assertion",
        _ => return (404, None),
    };
    if b["ticket"] == "mfa-ticket" && b["code"] == want {
        (200, Some(json!({ "token": TOKEN })))
    } else {
        (400, Some(json!({ "code": 60008, "message": "Invalid two-factor code" })))
    }
}

fn register(b: &Value) -> (u16, Option<Value>) {
    let email = b["email"].as_str().unwrap_or("");
    if email.starts_with("captcha") && !solved(b) {
        return captcha();
    }
    let mut bad = Vec::new();
    if email == "taken@mock.test" {
        bad.push(field("email", "EMAIL_ALREADY_REGISTERED", "Email is already registered."));
    }
    if b["username"] == "taken" {
        bad.push(field("username", "USERNAME_ALREADY_TAKEN", "Username is unavailable. Try adding numbers, letters, underscores _ , or periods."));
    }
    if b["consent"] != true {
        bad.push(field("consent", "REQUIRED", "You need to agree to our terms of service to continue"));
    }
    if bad.is_empty() { (200, Some(json!({ "token": TOKEN }))) } else { form(bad) }
}

pub fn route(method: &str, path: &str, body: &[u8]) -> Option<(u16, Option<Value>)> {
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let p = path.strip_prefix("/api/v9")?;
    Some(match (method, p) {
        ("POST", "/auth/login") => login(&b),
        ("POST", "/auth/mfa/sms/send") => (200, Some(json!({ "phone": "+1•••••••12" }))),
        ("POST", p) if p.starts_with("/auth/mfa/") => mfa(&p["/auth/mfa/".len()..], &b),
        ("POST", "/mock/scan") => {
            let _ = SCAN.send(());
            (204, None)
        }
        ("POST", "/auth/register") => register(&b),
        ("POST", "/unique-username/username-attempt-unauthed") => (200, Some(json!({ "taken": b["username"] == "taken" }))),
        ("POST", "/auth/forgot") if b["login"] == "unknown@mock.test" => form(vec![field("login", "EMAIL_DOES_NOT_EXIST", "No account with that email.")]),
        ("POST", "/auth/forgot") => (204, None),
        ("POST", "/users/@me/remote-auth/login") => {
            let spki = TICKETS.lock().unwrap().get_or_insert_default().get(b["ticket"].as_str().unwrap_or("")).cloned();
            match spki.and_then(|k| seal(&k, TOKEN.as_bytes())) {
                Some(c) => (200, Some(json!({ "encrypted_token": STANDARD.encode(c) }))),
                None => (400, Some(json!({ "message": "Invalid ticket" }))),
            }
        }
        _ => return None,
    })
}

fn der_items(mut d: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    while d.len() > 2 {
        let tag = d[0];
        let (len, skip) = match d[1] {
            n if n < 0x80 => (n as usize, 2),
            0x81 => (d[2] as usize, 3),
            _ => (((d[2] as usize) << 8) | d[3] as usize, 4),
        };
        out.push((tag, &d[skip..skip + len]));
        d = &d[skip + len..];
    }
    out
}

fn public_key(spki: &[u8]) -> Option<(BigUint, BigUint)> {
    let outer = der_items(der_items(spki).first()?.1);
    let bits = outer.get(1)?.1;
    let key = der_items(der_items(&bits[1..]).first()?.1);
    Some((BigUint::from_bytes_be(key.first()?.1), BigUint::from_bytes_be(key.get(1)?.1)))
}

fn mgf1(seed: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0u32;
    while out.len() < len {
        out.extend_from_slice(&Sha256::digest([seed, &i.to_be_bytes()].concat()));
        i += 1;
    }
    out.truncate(len);
    out
}

fn noise() -> [u8; 32] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    Sha256::digest([&t.to_be_bytes()[..], &N.fetch_add(1, Ordering::Relaxed).to_be_bytes()].concat()).into()
}

fn seal(spki: &[u8], msg: &[u8]) -> Option<Vec<u8>> {
    let (n, e) = public_key(spki)?;
    let k = (n.bits() as usize).div_ceil(8);
    let mut db = Sha256::digest(b"").to_vec();
    db.resize(k - 33 - msg.len() - 1, 0);
    db.push(1);
    db.extend_from_slice(msg);
    let seed = noise();
    let masked_db: Vec<u8> = db.iter().zip(mgf1(&seed, k - 33)).map(|(a, b)| a ^ b).collect();
    let masked_seed: Vec<u8> = seed.iter().zip(mgf1(&masked_db, 32)).map(|(a, b)| a ^ b).collect();
    let em = [&[0u8][..], &masked_seed, &masked_db].concat();
    let c = BigUint::from_bytes_be(&em).modpow(&e, &n).to_bytes_be();
    Some([vec![0u8; k - c.len()], c].concat())
}

fn text(v: Value) -> Ws {
    Ws::Text(v.to_string().into())
}

async fn session(s: tokio::net::TcpStream, step: Duration) {
    let Ok(ws) = tokio_tungstenite::accept_async(s).await else { return };
    let (mut tx, mut rx) = ws.split();
    let _ = tx.send(text(json!({ "op": "hello", "heartbeat_interval": 41250, "timeout_ms": 120000 }))).await;
    let mut spki: Vec<u8> = Vec::new();
    let mut nonce = [0u8; 32];
    let mut stage: Option<u8> = None;
    let mut scan = SCAN.subscribe();
    loop {
        let wake = async {
            match stage {
                Some(0) => {
                    let _ = scan.recv().await;
                }
                Some(_) => tokio::time::sleep(step).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            msg = rx.next() => {
                let Some(Ok(Ws::Text(t))) = msg else { return };
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                match v["op"].as_str() {
                    Some("init") => {
                        spki = STANDARD.decode(v["encoded_public_key"].as_str().unwrap_or("")).unwrap_or_default();
                        nonce = noise();
                        let Some(c) = seal(&spki, &nonce) else { return };
                        let _ = tx.send(text(json!({ "op": "nonce_proof", "encrypted_nonce": STANDARD.encode(c) }))).await;
                    }
                    Some("nonce_proof") if v["nonce"] == URL_SAFE_NO_PAD.encode(nonce) => {
                        let fp = URL_SAFE_NO_PAD.encode(Sha256::digest(&spki));
                        let _ = tx.send(text(json!({ "op": "pending_remote_init", "fingerprint": fp }))).await;
                        stage = Some(0);
                    }
                    Some("heartbeat") => {
                        let _ = tx.send(text(json!({ "op": "heartbeat_ack" }))).await;
                    }
                    _ => {}
                }
            }
            _ = wake => match stage {
                Some(0) => {
                    let Some(c) = seal(&spki, b"4242:0:0:Mock Scanner") else { return };
                    let _ = tx.send(text(json!({ "op": "pending_ticket", "encrypted_user_payload": STANDARD.encode(c) }))).await;
                    stage = Some(1);
                }
                _ => {
                    let ticket = format!("ticket-{}", URL_SAFE_NO_PAD.encode(Sha256::digest(&spki)));
                    TICKETS.lock().unwrap().get_or_insert_default().insert(ticket.clone(), spki.clone());
                    let _ = tx.send(text(json!({ "op": "pending_login", "ticket": ticket }))).await;
                    stage = None;
                }
            },
        }
    }
}

pub async fn gateway(port: u16) {
    let l = TcpListener::bind(("127.0.0.1", port)).await.expect("bind remote auth");
    while let Ok((s, _)) = l.accept().await {
        tokio::spawn(session(s, Duration::from_millis(1500)));
    }
}
