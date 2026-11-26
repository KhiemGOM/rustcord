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

#[derive(serde::Deserialize)]
struct Envelope<'a> {
    op: u64,
    #[serde(default)]
    s: Option<u64>,
    #[serde(default)]
    t: Option<String>,
    #[serde(borrow)]
    d: &'a serde_json::value::RawValue,
}

/// READY with each of its servers left as the text it arrived in, for `state::ready_guilds` to
/// parse one at a time. Parsed whole, a big account's READY is a tree of some 50 MB that lives
/// for the half second it takes to read, on top of everything else a start holds.
fn ready(text: &str) -> Option<Value> {
    use serde_json::value::RawValue;
    let e: Envelope = serde_json::from_str(text).ok()?;
    if e.t.as_deref() != Some("READY") {
        return None;
    }
    let mut d = serde_json::Map::new();
    for (key, part) in serde_json::from_str::<std::collections::BTreeMap<String, &RawValue>>(e.d.get()).ok()? {
        let part = if key == "guilds" {
            Value::Array(serde_json::from_str::<Vec<&RawValue>>(part.get()).ok()?.into_iter().map(|g| Value::String(g.get().to_string())).collect())
        } else {
            serde_json::from_str(part.get()).ok()?
        };
        d.insert(key, part);
    }
    let mut whole = serde_json::Map::new();
    whole.insert("op".into(), e.op.into());
    whole.insert("s".into(), e.s.into());
    whole.insert("t".into(), "READY".into());
    whole.insert("d".into(), Value::Object(d));
    Some(Value::Object(whole))
}

fn parse(text: &[u8]) -> serde_json::Result<Value> {
    // discord names the event first, a serialised map last
    let named = |b: &[u8]| b.windows(11).any(|w| w == br#""t":"READY""#);
    if (named(&text[..text.len().min(64)]) || named(&text[text.len().saturating_sub(64)..]))
        && let Some(v) = std::str::from_utf8(text).ok().and_then(ready)
    {
        return Ok(v);
    }
    serde_json::from_slice(text)
}

const MAX_INFLATE: usize = 128 << 20;

fn inflate(z: &mut Decompress, data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    out.clear();
    let mut input = data;
    loop {
        let room = MAX_INFLATE.saturating_sub(out.len());
        if room == 0 {
            return Err("gateway frame exceeded decompression limit".into());
        }
        out.reserve((input.len() * 4).max(8192).min(room));
        let before = z.total_in();
        z.decompress_vec(input, out, FlushDecompress::Sync).map_err(|e| e.to_string())?;
        if out.len() > MAX_INFLATE {
            return Err("gateway frame exceeded decompression limit".into());
        }
        input = &input[(z.total_in() - before) as usize..];
        if input.is_empty() && out.len() < out.capacity() {
            return Ok(());
        }
    }
}

fn valid_gateway_url(raw: &str) -> Option<String> {
    let u = reqwest::Url::parse(raw).ok()?;
    let loopback = u.host_str().is_some_and(|h| matches!(h, "127.0.0.1" | "localhost" | "[::1]"));
    let allow_ws = cfg!(any(test, feature = "e2e")) && u.scheme() == "ws" && loopback;
    (u.scheme() == "wss" || allow_ws).then(|| raw.to_string())
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
    let mut identified = false;

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
                        let v = parse(&out);
                        // READY grows both to tens of MB; don't keep that for the whole session
                        if out.capacity() > 1 << 20 {
                            out = Vec::new();
                        }
                        if buf.capacity() > 1 << 20 {
                            buf = Vec::new();
                        }
                        match v {
                            Ok(v) => v,
                            Err(_) => continue,
                        }
                    }
                    Some(Ok(Ws::Text(t))) => match parse(t.as_bytes()) {
                        Ok(v) => v,
                        Err(_) => continue,
                    },
                    Some(Ok(Ws::Close(f))) => {
                        let code: u16 = f.map(|f| f.code.into()).unwrap_or(1000);
                        return match code {
                            4004 => End::Fatal,
                            4007 | 4009 => End::Fresh,
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
                                url: v["d"]["resume_gateway_url"].as_str().and_then(valid_gateway_url).unwrap_or(host.clone()),
                                seq: v["s"].as_u64(),
                            });
                            identified = true;
                            tx.send(Ev::Conn(Conn::Online));
                        } else if t == "RESUMED" {
                            identified = true;
                            tx.send(Ev::Conn(Conn::Online));
                        }
                        let d = match v {
                            Value::Object(mut m) => m.remove("d").unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        let open = crate::net::OPEN.load(std::sync::atomic::Ordering::Relaxed).to_string();
                        let elsewhere = d["channel_id"].as_str().is_some_and(|c| c != open);
                        let soon = match t.as_str() {
                            "PRESENCE_UPDATE" => true,
                            "TYPING_START" => elsewhere,
                            "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_DELETE" | "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                                elsewhere && d["guild_id"].is_string()
                            }
                            _ => false,
                        };
                        if soon { tx.send_soon(Ev::Dispatch(t, d)) } else { tx.send(Ev::Dispatch(t, d)) }
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
            op = ops.recv(), if identified => {
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    fn zlib(n: usize) -> Vec<u8> {
        use std::io::Write;
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let zeros = vec![0u8; 1 << 20];
        let mut left = n;
        while left > 0 {
            let k = left.min(zeros.len());
            z.write_all(&zeros[..k]).unwrap();
            left -= k;
        }
        z.flush().unwrap();
        z.get_ref().clone()
    }

    #[test]
    fn frames_near_the_inflate_limit_never_panic() {
        for n in [super::MAX_INFLATE - 4096, super::MAX_INFLATE - 1, super::MAX_INFLATE + 1] {
            let mut out = Vec::new();
            let r = super::inflate(&mut flate2::Decompress::new(true), &zlib(n), &mut out);
            assert_eq!(r.is_ok(), n < super::MAX_INFLATE, "{n}: {r:?}");
        }
    }

    fn sample() -> Value {
        json!({ "t": "READY", "s": 1, "op": 0, "d": {
            "session_id": "abc",
            "user": { "id": "1", "username": "me" },
            "guilds": [
                { "id": "10", "properties": { "name": "one" }, "channels": [{ "id": "11", "type": 0, "name": "general" }] },
                { "id": "20", "properties": { "name": "two \"quoted\" [x]" }, "channels": [] },
            ],
            "users": [{ "id": "2", "username": "other" }],
        } })
    }

    #[test]
    fn ready_keeps_everything_and_leaves_each_server_as_its_text() {
        let whole = sample();
        // a map serialised (the event's name last), and the order discord sends (first)
        for text in [whole.to_string(), format!(r#"{{"t":"READY","s":1,"op":0,"d":{}}}"#, whole["d"])] {
            let split = super::parse(text.as_bytes()).unwrap();
            assert_eq!((&split["t"], &split["s"], &split["op"]), (&json!("READY"), &json!(1), &json!(0)));
            let mut d = split["d"].clone();
            assert!(d["guilds"].as_array().unwrap().iter().all(Value::is_string));
            d["guilds"] = crate::state::ready_guilds(&split["d"]).map(|g| g.into_owned()).collect();
            assert_eq!(d, whole["d"]);
        }
    }

    #[test]
    fn a_store_reads_the_same_from_ready_whole_or_split() {
        let whole = sample();
        let split = super::parse(whole.to_string().as_bytes()).unwrap();
        let read = |d: &Value| {
            let mut s = crate::state::Store::default();
            s.load_ready(d);
            let mut names: Vec<String> = s.guilds.values().map(|g| g.name.clone()).collect();
            names.sort();
            (names, s.channels.len(), s.users.len())
        };
        assert_eq!(read(&split["d"]), read(&whole["d"]));
        assert_eq!(read(&split["d"]).0, ["one", "two \"quoted\" [x]"]);
    }

    #[test]
    fn other_events_parse_as_they_are() {
        let v = json!({ "t": "MESSAGE_CREATE", "s": 2, "op": 0, "d": { "content": "READY" } });
        assert_eq!(super::parse(v.to_string().as_bytes()).unwrap(), v);
    }
}
