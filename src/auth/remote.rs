use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{Instant, sleep, sleep_until};
use tokio_tungstenite::tungstenite::{Message as Ws, client::IntoClientRequest, http::HeaderValue};

use super::{Ev, Qr, rsa};
use crate::api::{self, Api};
use crate::net::{Ev as NetEv, Tx};

enum End {
    Retry,
    Token(String),
}

pub async fn run(api: Arc<Api>, tx: Tx) {
    loop {
        tx.send(NetEv::Auth(Ev::Qr(Qr::Init)));
        match session(&api, &tx).await {
            End::Token(t) => return tx.send(NetEv::Auth(Ev::Token(t))),
            End::Retry => sleep(Duration::from_millis(1500)).await,
        }
    }
}

fn text(v: Value) -> Ws {
    Ws::Text(v.to_string().into())
}

async fn session(api: &Api, tx: &Tx) -> End {
    let Some(key) = rsa::Key::generate() else { return End::Retry };
    let Ok(mut req) = format!("{}/?v=2", api::remote_auth()).into_client_request() else { return End::Retry };
    req.headers_mut().insert("Origin", HeaderValue::from_static("https://discord.com"));
    let Ok((ws, _)) = tokio_tungstenite::connect_async(req).await else { return End::Retry };
    let (mut sink, mut stream) = ws.split();
    let fingerprint = URL_SAFE_NO_PAD.encode(Sha256::digest(key.spki()));
    let mut beat = Duration::from_secs(41);
    let mut next_beat = Instant::now() + Duration::from_secs(3600);
    let _ = sink.send(text(json!({ "op": "init", "encoded_public_key": STANDARD.encode(key.spki()) }))).await;
    loop {
        tokio::select! {
            msg = stream.next() => {
                let Some(Ok(Ws::Text(t))) = msg else { return End::Retry };
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                let open = |field: &str| v[field].as_str().and_then(|s| STANDARD.decode(s).ok()).and_then(|c| key.decrypt(&c));
                match v["op"].as_str() {
                    Some("hello") => {
                        beat = Duration::from_millis(v["heartbeat_interval"].as_u64().unwrap_or(41_250));
                        next_beat = Instant::now() + beat;
                    }
                    Some("nonce_proof") => {
                        let Some(nonce) = open("encrypted_nonce") else { return End::Retry };
                        let _ = sink.send(text(json!({ "op": "nonce_proof", "nonce": URL_SAFE_NO_PAD.encode(nonce) }))).await;
                    }
                    Some("pending_remote_init") => {
                        if v["fingerprint"].as_str() != Some(fingerprint.as_str()) {
                            return End::Retry;
                        }
                        tx.send(NetEv::Auth(Ev::Qr(Qr::Ready(fingerprint.clone()))));
                    }
                    Some("pending_ticket") => {
                        let Some(user) = open("encrypted_user_payload") else { return End::Retry };
                        let user = String::from_utf8_lossy(&user).to_string();
                        let mut p = user.splitn(4, ':');
                        let (Some(id), Some(_), Some(avatar), Some(name)) = (p.next(), p.next(), p.next(), p.next()) else { return End::Retry };
                        let avatar = (avatar != "0").then(|| avatar.to_string());
                        tx.send(NetEv::Auth(Ev::Qr(Qr::Ticket { id: id.to_string(), avatar, name: name.to_string() })));
                    }
                    Some("pending_login") => {
                        let Some(ticket) = v["ticket"].as_str() else { return End::Retry };
                        tx.send(NetEv::Auth(Ev::Qr(Qr::Working)));
                        let sealed = api.remote_login(ticket).await.ok().and_then(|r| r.body["encrypted_token"].as_str().and_then(|t| STANDARD.decode(t).ok()));
                        return match sealed.and_then(|c| key.decrypt(&c)) {
                            Some(t) => End::Token(String::from_utf8_lossy(&t).to_string()),
                            None => End::Retry,
                        };
                    }
                    Some("cancel") => return End::Retry,
                    _ => {}
                }
            }
            _ = sleep_until(next_beat) => {
                let _ = sink.send(text(json!({ "op": "heartbeat" }))).await;
                next_beat = Instant::now() + beat;
            }
        }
    }
}
