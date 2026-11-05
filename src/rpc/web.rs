//! The socket on 127.0.0.1:6463 to 6472 that discord.com's "Open in app" button looks for. It
//! answers pages from discord.com, ptb.discord.com and canary.discord.com and nobody else, over
//! a websocket or a plain request, and does two things for them: show an invite, and show a
//! server template. Any other origin is turned away before a command is read.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::Msg;
use super::activity::Fail;

const ORIGINS: [&str; 3] = ["https://discord.com", "https://ptb.discord.com", "https://canary.discord.com"];
const MAX_HEAD: usize = 8192;
const MAX_BODY: usize = 16 * 1024;
const MAX_CONNS: usize = 16;

pub async fn serve() {
    let testing = super::testing();
    let ports: Vec<u16> = if testing { vec![0] } else { (6463..=6472).collect() };
    let mut bound = None;
    for p in ports {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", p)).await {
            bound = Some(l);
            break;
        }
    }
    let Some(listener) = bound else { return };
    let Ok(addr) = listener.local_addr() else { return };
    super::PORT.store(addr.port(), std::sync::atomic::Ordering::Relaxed);
    let slots = Arc::new(Semaphore::new(MAX_CONNS));
    loop {
        let Ok((s, peer)) = listener.accept().await else { continue };
        if !peer.ip().is_loopback() {
            continue;
        }
        let Ok(permit) = slots.clone().try_acquire_owned() else { continue };
        let port = addr.port();
        tokio::spawn(async move {
            conn(s, port).await;
            drop(permit);
        });
    }
}

fn allowed(origin: Option<&str>) -> bool {
    origin.is_some_and(|o| ORIGINS.contains(&o))
}

/// Only the same machine's own name for itself counts as the host, so a page on another
/// domain that has pointed its name at 127.0.0.1 gets nowhere.
fn local_host(host: Option<&str>, port: u16) -> bool {
    host.is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"))
}

struct Req {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
}

impl Req {
    fn header(&self, k: &str) -> Option<&str> {
        self.headers.iter().find(|(h, _)| h == k).map(|(_, v)| v.as_str())
    }

    fn param(&self, k: &str) -> Option<String> {
        let u = reqwest::Url::parse(&format!("http://local/?{}", self.query)).ok()?;
        u.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned())
    }
}

fn parse(head: &[u8]) -> Option<Req> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())).collect();
    Some(Req { method, path: path.to_string(), query: query.to_string(), headers })
}

async fn peek_head(s: &TcpStream) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; MAX_HEAD];
    loop {
        let n = s.peek(&mut buf).await.ok()?;
        if let Some(i) = buf[..n].windows(4).position(|w| w == b"

") {
            buf.truncate(i + 4);
            return Some(buf);
        }
        if n == buf.len() || n == 0 {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn conn(mut s: TcpStream, port: u16) {
    let Ok(Some(head)) = timeout(Duration::from_secs(5), peek_head(&s)).await else { return };
    let Some(req) = parse(&head) else { return };
    if !local_host(req.header("host"), port) {
        return refuse(&mut s, "421 Misdirected Request").await;
    }
    if req.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket")) {
        return socket_session(s, port).await;
    }
    let mut drain = vec![0u8; head.len()];
    if s.read_exact(&mut drain).await.is_err() {
        return;
    }
    let _ = timeout(Duration::from_secs(10), http(&mut s, req)).await;
}

async fn refuse(s: &mut TcpStream, status: &str) {
    let _ = s.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await;
}

async fn answer(s: &mut TcpStream, status: &str, headers: &[(&str, &str)], body: &[u8]) {
    let mut out = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        out += &format!("{k}: {v}\r\n");
    }
    out += "\r\n";
    let mut bytes = out.into_bytes();
    bytes.extend(body);
    let _ = s.write_all(&bytes).await;
}

fn referrer_origin(req: &Req) -> Option<String> {
    reqwest::Url::parse(req.header("referer")?).ok().map(|u| u.origin().ascii_serialization())
}

/// `POST /rpc` for browsers that can send it, `GET /rpc?payload=..&callback=..` for the rest,
/// which come back to the page that asked once it's done.
async fn http(s: &mut TcpStream, req: Req) {
    if req.path != "/rpc" {
        return refuse(s, "404 Not Found").await;
    }
    let origin = req.header("origin").map(str::to_string);
    match req.method.as_str() {
        "OPTIONS" => {
            let Some(o) = origin.filter(|o| allowed(Some(o))) else { return refuse(s, "403 Forbidden").await };
            let cors = [
                ("Access-Control-Allow-Origin", o.as_str()),
                ("Vary", "Origin"),
                ("Access-Control-Allow-Methods", "POST, OPTIONS"),
                ("Access-Control-Allow-Headers", "content-type"),
                ("Access-Control-Allow-Private-Network", "true"),
                ("Access-Control-Max-Age", "600"),
            ];
            answer(s, "204 No Content", &cors, b"").await;
        }
        "POST" => {
            let Some(o) = origin.filter(|o| allowed(Some(o))) else { return refuse(s, "403 Forbidden").await };
            let len: usize = req.header("content-length").and_then(|l| l.parse().ok()).unwrap_or(usize::MAX);
            if len > MAX_BODY {
                return refuse(s, "413 Payload Too Large").await;
            }
            let mut body = vec![0u8; len];
            if s.read_exact(&mut body).await.is_err() {
                return;
            }
            let reply = respond(&body);
            answer(s, "200 OK", &[("Access-Control-Allow-Origin", &o), ("Vary", "Origin"), ("Content-Type", "application/json")], reply.to_string().as_bytes()).await;
        }
        "GET" => {
            let callback = req.param("callback").and_then(|c| reqwest::Url::parse(&c).ok()).filter(|u| u.scheme() == "https" && allowed(Some(&u.origin().ascii_serialization())));
            let (Some(payload), Some(callback)) = (req.param("payload"), callback) else { return refuse(s, "400 Bad Request").await };
            if !allowed(referrer_origin(&req).as_deref()) {
                return refuse(s, "403 Forbidden").await;
            }
            let _ = respond(payload.as_bytes());
            answer(s, "302 Found", &[("Location", callback.as_str())], b"").await;
        }
        _ => refuse(s, "405 Method Not Allowed").await,
    }
}

fn respond(body: &[u8]) -> Value {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return json!({ "cmd": null, "data": { "code": 4000, "message": "Invalid payload" }, "evt": "ERROR", "nonce": null });
    };
    match command(&v) {
        Ok(data) => json!({ "cmd": v["cmd"], "data": data, "evt": null, "nonce": v["nonce"] }),
        Err((code, message)) => json!({ "cmd": v["cmd"], "data": { "code": code, "message": message }, "evt": "ERROR", "nonce": v["nonce"] }),
    }
}

/// A code is what follows discord.gg/ or /template/: letters, digits and dashes.
fn code(v: &Value) -> Option<String> {
    v.as_str().filter(|c| !c.is_empty() && c.len() <= 64 && c.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).map(str::to_string)
}

fn command(v: &Value) -> Result<Value, Fail> {
    if v["nonce"].as_str().is_none_or(str::is_empty) {
        return Err((4000, "Payload requires a nonce".into()));
    }
    match v["cmd"].as_str().unwrap_or("") {
        "INVITE_BROWSER" => {
            super::send(Msg::Invite(code(&v["args"]["code"]).ok_or((4011, "Invalid invite".to_string()))?));
            Ok(Value::Null)
        }
        "GUILD_TEMPLATE_BROWSER" => {
            super::send(Msg::Template(code(&v["args"]["code"]).ok_or((4017, "Invalid guild template id".to_string()))?));
            Ok(Value::Null)
        }
        c => Err((4002, format!("Invalid command: {c}"))),
    }
}

async fn socket_session(s: TcpStream, port: u16) {
    use futures_util::{SinkExt, StreamExt};
    let check = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let get = |k: &str| req.headers().get(k).and_then(|v| v.to_str().ok());
        if !allowed(get("origin")) || !local_host(get("host"), port) {
            let mut no = ErrorResponse::new(None);
            *no.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
            return Err(no);
        }
        Ok(resp)
    };
    let cfg = WebSocketConfig::default().max_message_size(Some(MAX_BODY)).max_frame_size(Some(MAX_BODY));
    let Ok(ws) = tokio_tungstenite::accept_hdr_async_with_config(s, check, Some(cfg)).await else { return };
    let (mut tx, mut rx) = ws.split();
    let ready = json!({
        "cmd": "DISPATCH", "evt": "READY", "nonce": null,
        "data": { "v": 1, "config": { "cdn_host": "cdn.discordapp.com", "api_endpoint": "//discord.com/api", "environment": "production" } },
    });
    if tx.send(Message::text(ready.to_string())).await.is_err() {
        return;
    }
    while let Ok(Some(Ok(m))) = timeout(Duration::from_secs(120), rx.next()).await {
        match m {
            Message::Text(t) => {
                if tx.send(Message::text(respond(t.as_bytes()).to_string())).await.is_err() {
                    return;
                }
            }
            Message::Ping(p) => {
                let _ = tx.send(Message::Pong(p)).await;
            }
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_discords_own_pages_get_in() {
        assert!(allowed(Some("https://discord.com")));
        assert!(allowed(Some("https://ptb.discord.com")));
        assert!(allowed(Some("https://canary.discord.com")));
        assert!(!allowed(None));
        assert!(!allowed(Some("http://discord.com")));
        assert!(!allowed(Some("https://discord.com.evil.example")));
        assert!(!allowed(Some("https://evil.example")));
        assert!(!allowed(Some("null")));
    }

    #[test]
    fn host_must_be_the_loopback_name() {
        assert!(local_host(Some("127.0.0.1:6463"), 6463));
        assert!(local_host(Some("localhost:6463"), 6463));
        assert!(!local_host(Some("evil.example:6463"), 6463));
        assert!(!local_host(Some("127.0.0.1:6464"), 6463));
        assert!(!local_host(None, 6463));
    }

    #[test]
    fn a_website_can_do_two_things() {
        assert_eq!(command(&json!({ "cmd": "GET_USER", "nonce": "n", "args": {} })).unwrap_err().0, 4002);
        assert_eq!(command(&json!({ "cmd": "SET_ACTIVITY", "nonce": "n", "args": {} })).unwrap_err().0, 4002);
        assert_eq!(command(&json!({ "cmd": "INVITE_BROWSER", "nonce": "n", "args": { "code": "../x" } })).unwrap_err().0, 4011);
        assert_eq!(command(&json!({ "cmd": "INVITE_BROWSER", "args": { "code": "abc" } })).unwrap_err().0, 4000);
    }
}
