use flate2::{Decompress, FlushDecompress};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep, sleep_until};
use tokio_tungstenite::tungstenite::Message as Ws;

use crate::api;
use crate::net::{Conn, Ev, Tx};

const SUFFIX: [u8; 4] = [0, 0, 0xff, 0xff];

// DEDUPE_USER_OBJECTS | PRIORITIZED_READY_PAYLOAD | VERSIONED_READ_STATES | USER_SETTINGS_PROTO | ...
const CAPABILITIES: u64 = 1 | 2 | 4 | 16 | 32 | 512;

struct Session {
    id: String,
    url: String,
    seq: Option<u64>,
}

enum End {
    Retry,
    Fresh,
    Fatal,
    Quit,
}

pub async fn run(token: String, tx: Tx, mut ops: mpsc::UnboundedReceiver<Value>) {
    let mut sess: Option<Session> = None;
    let mut backoff = 1u64;
    loop {
        tx.send(Ev::Conn(if sess.is_some() { Conn::Resuming } else { Conn::Connecting }));
        let started = std::time::Instant::now();
        match connect(&token, &tx, &mut ops, &mut sess).await {
            End::Fatal => {
                tx.send(Ev::AuthFailed);
                return;
            }
            End::Quit => return,
            End::Fresh => sess = None,
            End::Retry => {}
        }
        tx.send(Ev::Conn(Conn::Offline));
        if started.elapsed() > Duration::from_secs(30) {
            backoff = 1;
        }
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

fn inflate(z: &mut Decompress, data: &[u8], out: &mut Vec<u8>) -> Result<(), flate2::DecompressError> {
    out.clear();
    let mut input = data;
    loop {
        out.reserve((input.len() * 4).max(8192));
        let before = z.total_in();
        z.decompress_vec(input, out, FlushDecompress::Sync)?;
        input = &input[(z.total_in() - before) as usize..];
        if input.is_empty() && out.len() < out.capacity() {
            return Ok(());
        }
    }
}

async fn connect(
    token: &str,
    tx: &Tx,
    ops: &mut mpsc::UnboundedReceiver<Value>,
    sess: &mut Option<Session>,
) -> End {
    let host = sess.as_ref().map(|s| s.url.clone()).unwrap_or_else(|| api::gateway().to_string());
    let url = format!("{}/?encoding=json&v=9&compress=zlib-stream", host.trim_end_matches('/'));
    let Ok((ws, _)) = tokio_tungstenite::connect_async(url).await else {
        return End::Retry;
    };
    let (mut sink, mut stream) = ws.split();

    let mut z = Decompress::new(true);
    let mut buf: Vec<u8> = Vec::new();
    let mut out: Vec<u8> = Vec::new();
    let mut interval = Duration::from_secs(41);
    let mut next_beat = Instant::now() + Duration::from_secs(3600);
    let mut acked = true;

    loop {
        tokio::select! {
            msg = stream.next() => {
                let v: Value = match msg {
                    Some(Ok(Ws::Binary(b))) => {
                        buf.extend_from_slice(&b);
                        if !buf.ends_with(&SUFFIX) {
                            continue;
                        }
                        let r = inflate(&mut z, &buf, &mut out);
                        buf.clear();
                        if r.is_err() {
                            return End::Retry;
                        }
                        match serde_json::from_slice(&out) {
                            Ok(v) => v,
                            Err(_) => continue,
                        }
                    }
                    Some(Ok(Ws::Text(t))) => match serde_json::from_str(&t) {
                        Ok(v) => v,
                        Err(_) => continue,
                    },
                    Some(Ok(Ws::Close(f))) => {
                        let code: u16 = f.map(|f| f.code.into()).unwrap_or(1000);
                        return match code {
                            4004 => End::Fatal,
                            4007 | 4009 => End::Fresh,
                            4010..=4014 => End::Fatal,
                            _ => End::Retry,
                        };
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => return End::Retry,
                };
                match v["op"].as_u64().unwrap_or(99) {
                    0 => {
                        if let (Some(s), Some(sess)) = (v["s"].as_u64(), sess.as_mut()) {
                            sess.seq = Some(s);
                        }
                        let t = v["t"].as_str().unwrap_or("").to_string();
                        if t == "READY" {
                            *sess = Some(Session {
                                id: v["d"]["session_id"].as_str().unwrap_or("").to_string(),
                                url: v["d"]["resume_gateway_url"].as_str().map(str::to_string).unwrap_or(host.clone()),
                                seq: v["s"].as_u64(),
                            });
                            tx.send(Ev::Conn(Conn::Online));
                        } else if t == "RESUMED" {
                            tx.send(Ev::Conn(Conn::Online));
                        }
                        let d = match v {
                            Value::Object(mut m) => m.remove("d").unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        tx.send(Ev::Dispatch(t, d));
                    }
                    1 => next_beat = Instant::now(),
                    7 => return End::Retry,
                    9 => {
                        if v["d"].as_bool() != Some(true) {
                            *sess = None;
                        }
                        sleep(Duration::from_millis(1500)).await;
                        return End::Retry;
                    }
                    10 => {
                        interval = Duration::from_millis(v["d"]["heartbeat_interval"].as_u64().unwrap_or(41250));
                        next_beat = Instant::now() + interval / 3;
                        let hello = match sess {
                            Some(s) => json!({ "op": 6, "d": { "token": token, "session_id": s.id, "seq": s.seq } }),
                            None => json!({ "op": 2, "d": {
                                "token": token,
                                "capabilities": CAPABILITIES,
                                "properties": api::props(),
                                "presence": { "status": "unknown", "since": 0, "activities": [], "afk": false },
                                "compress": false,
                                "client_state": { "guild_versions": {} }
                            }}),
                        };
                        if sink.send(Ws::text(hello.to_string())).await.is_err() {
                            return End::Retry;
                        }
                    }
                    11 => acked = true,
                    _ => {}
                }
            }
            _ = sleep_until(next_beat) => {
                if !acked {
                    let _ = sink.close().await;
                    return End::Retry;
                }
                acked = false;
                let seq = sess.as_ref().and_then(|s| s.seq);
                if sink.send(Ws::text(json!({ "op": 1, "d": seq }).to_string())).await.is_err() {
                    return End::Retry;
                }
                next_beat = Instant::now() + interval;
            }
            op = ops.recv() => {
                let Some(op) = op else {
                    let _ = sink.close().await;
                    return End::Quit;
                };
                if sink.send(Ws::text(op.to_string())).await.is_err() {
                    return End::Retry;
                }
            }
        }
    }
}
