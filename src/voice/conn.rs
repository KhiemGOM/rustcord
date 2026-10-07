//! One voice session: voice gateway v8, UDP media, DAVE (via davey) and opus decode.

use davey::{Codec, DaveSession, MediaType, ProposalsOperationType};
use futures_util::{SinkExt, StreamExt};
use rtrb::Producer;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU16;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::{Instant, interval, sleep_until, timeout};
use tokio_tungstenite::tungstenite::Message as Ws;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::audio::{self, FRAME, Out, SILENCE, Shared};
use super::rtp::{self, Cipher};
use super::video::{self, Assembler, Decode, Encoded, Got, H264_PT, Source};
use super::{Connect, Ctl, Ev, Kind, Media, State};
use crate::net::{self, Tx};
use crate::state::{Id, sid};

type Sock = WebSocketStream<MaybeTlsStream<TcpStream>>;

// same tolerance discord.js uses: a few frames always slip through a transition,
// a long streak means our group state is wrong and we need to be re-added
const FAIL_LIMIT: u32 = 36;

struct Inbound {
    user: Id,
    stereo: opus_rs::OpusDecoder,
    mono: Option<opus_rs::OpusDecoder>,
    next: Option<(u16, u32)>,
    out: Option<(usize, Producer<f32>)>,
    voice_at: Option<Instant>,
    speaking: bool,
}

struct Sess {
    c: Connect,
    tx: Tx,
    shared: Arc<Shared>,
    udp: Arc<UdpSocket>,
    ssrc: u32,
    cipher: Option<Cipher>,
    seq_ack: i64,
    seq: u16,
    ts: u32,
    clock: Instant,
    nonce: u32,
    talking: bool,
    acked: bool,
    dave: Option<DaveSession>,
    dave_ver: u16,
    pending: HashMap<u16, u16>,
    downgraded: bool,
    last_transition: Option<u16>,
    fails: u32,
    known: HashSet<Id>,
    users: HashMap<u32, Id>,
    inbound: HashMap<u32, Inbound>,
    slots: Vec<Option<Producer<f32>>>,
    pcm: Vec<f32>,
    vssrc: u32,
    rtx: u32,
    vseq: u16,
    asm: HashMap<u32, Assembler>,
    vusers: HashMap<u32, Id>,
    decode: Option<Decode>,
    pli_at: HashMap<u32, Instant>,
    cam: Option<video::Sender>,
    vout: mpsc::UnboundedSender<Encoded>,
}

enum End {
    Resume,
    Fresh,
    Fatal(String),
    Quit,
}

pub async fn run(c: Connect, tx: Tx, mut ctl: mpsc::Receiver<Ctl>) {
    let tag = c.tag;
    let err = session(c, &tx, &mut ctl).await.err();
    tx.send(net::Ev::Voice(Ev { tag, kind: Kind::Closed(err) }));
}

fn url(endpoint: &str) -> String {
    // the mock hands out ws:// endpoints; Discord never includes a scheme
    let base = if endpoint.contains("://") { endpoint.to_string() } else { format!("wss://{endpoint}") };
    format!("{}/?v=8", base.trim_end_matches('/'))
}

async fn session(c: Connect, tx: &Tx, ctl: &mut mpsc::Receiver<Ctl>) -> Result<(), String> {
    let udp = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
    let shared = c.shared.clone();
    let mut known = HashSet::new();
    known.insert(c.me);
    let (vout, mut vframes) = mpsc::unbounded_channel();
    let (pli_tx, mut plis) = mpsc::unbounded_channel();
    let decode = Some(Decode::start(c.tag, tx.clone(), shared.clone(), pli_tx));
    let mut s = Sess {
        c,
        tx: tx.clone(),
        shared,
        udp: Arc::new(udp),
        ssrc: 0,
        cipher: None,
        seq_ack: -1,
        seq: 0,
        ts: 0,
        clock: Instant::now(),
        nonce: 0,
        talking: false,
        acked: false,
        dave: None,
        dave_ver: 0,
        pending: HashMap::new(),
        downgraded: false,
        last_transition: None,
        fails: 0,
        known,
        users: HashMap::new(),
        inbound: HashMap::new(),
        slots: Vec::new(),
        pcm: vec![0.0; FRAME * 2],
        vssrc: 0,
        rtx: 0,
        vseq: 0,
        asm: HashMap::new(),
        vusers: HashMap::new(),
        decode,
        pli_at: HashMap::new(),
        cam: None,
        vout,
    };

    let (out_tx, mut frames) = mpsc::unbounded_channel();
    let mut engine = None;
    let mut resume = false;
    let mut tries = 0;
    loop {
        let r = if resume { s.resume().await } else { s.identify().await };
        let (mut ws, beat) = match r {
            Ok(v) => v,
            Err(_) if tries < 5 => {
                tries += 1;
                resume = false;
                s.state(State::Reconnecting);
                tokio::time::sleep(Duration::from_secs(tries)).await;
                continue;
            }
            Err(e) => return Err(e),
        };
        tries = 0;
        if s.c.kind == Media::GoLive && s.cam.is_none() {
            let _ = s.video(&mut ws, Some(Source::Screen)).await;
        } else if s.cam.is_some() && !resume {
            // a fresh session knows nothing about the video we were already sending
            let _ = s.announce(&mut ws).await;
        }
        if engine.is_none() && s.c.kind == Media::Voice {
            let (e, prods) = audio::start(s.shared.clone(), out_tx.clone());
            s.slots = prods.into_iter().map(Some).collect();
            engine = Some(e);
        }
        s.state(State::Connected);
        match s.pump(&mut ws, beat, &mut frames, &mut vframes, &mut plis, ctl).await {
            End::Quit => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            End::Fatal(e) => return Err(e),
            End::Resume => resume = true,
            End::Fresh => resume = false,
        }
        s.state(State::Reconnecting);
    }
}

impl Sess {
    fn emit(&self, k: Kind) {
        self.tx.send(net::Ev::Voice(Ev { tag: self.c.tag, kind: k }));
    }

    fn state(&self, st: State) {
        self.emit(Kind::State(st));
    }

    async fn identify(&mut self) -> Result<(Sock, Duration), String> {
        let mut ws = open(&self.c.endpoint).await?;
        let id = json!({ "op": 0, "d": {
            "server_id": self.c.server.to_string(),
            "user_id": self.c.me.to_string(),
            "session_id": self.c.session,
            "token": self.c.token,
            "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
            "video": true,
            "streams": [{ "type": if self.c.kind == Media::Voice { "video" } else { "screen" }, "rid": "100", "quality": 100 }],
        }});
        send(&mut ws, &id).await?;

        // anything that isn't part of the handshake gets replayed once we have a cipher and a DAVE session
        let mut early: Vec<Ws> = Vec::new();
        let mut beat = None;
        let mut ready = None;
        let deadline = Instant::now() + Duration::from_secs(10);
        while beat.is_none() || ready.is_none() {
            let m = next(&mut ws, deadline).await?;
            let Ws::Text(t) = &m else {
                early.push(m);
                continue;
            };
            let v: Value = serde_json::from_str(t).unwrap_or_default();
            match v["op"].as_u64() {
                Some(8) => beat = Some(hb(&v)),
                Some(2) => ready = Some(v["d"].clone()),
                _ => early.push(m),
            }
        }
        let ready = ready.unwrap_or_default();
        self.ssrc = ready["ssrc"].as_u64().unwrap_or(0) as u32;
        // the gateway hands out our video and retransmission ssrcs up front
        self.vssrc = ready["streams"][0]["ssrc"].as_u64().unwrap_or(self.ssrc as u64 + 1) as u32;
        self.rtx = ready["streams"][0]["rtx_ssrc"].as_u64().unwrap_or(self.ssrc as u64 + 2) as u32;
        let ip = ready["ip"].as_str().unwrap_or("").to_string();
        let port = ready["port"].as_u64().unwrap_or(0) as u16;
        let modes: Vec<&str> = ready["modes"].as_array().into_iter().flatten().filter_map(|m| m.as_str()).collect();
        let Some(mode) = rtp::MODES.into_iter().find(|m| modes.contains(m)) else {
            return Err("no supported encryption mode".into());
        };
        self.udp.connect((ip.as_str(), port)).await.map_err(|e| e.to_string())?;
        let (addr, port) = discover(&self.udp, self.ssrc).await?;
        send(&mut ws, &json!({ "op": 1, "d": {
            "protocol": "udp",
            "data": { "address": addr, "port": port, "mode": mode },
            "codecs": [
                { "name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS_PT },
                { "name": "H264", "type": "video", "priority": 1000, "payload_type": H264_PT, "rtx_payload_type": video::RTX_PT, "encode": true, "decode": true },
            ],
        }}))
        .await?;

        loop {
            let m = next(&mut ws, deadline).await?;
            let Ws::Text(t) = &m else {
                early.push(m);
                continue;
            };
            let v: Value = serde_json::from_str(t).unwrap_or_default();
            if v["op"].as_u64() != Some(4) {
                early.push(m);
                continue;
            }
            let d = &v["d"];
            let key: Vec<u8> = d["secret_key"].as_array().into_iter().flatten().filter_map(|b| b.as_u64()).map(|b| b as u8).collect();
            self.cipher = Cipher::new(d["mode"].as_str().unwrap_or(mode), &key);
            if self.cipher.is_none() {
                return Err("bad session description".into());
            }
            self.dave_ver = d["dave_protocol_version"].as_u64().unwrap_or(0) as u16;
            self.dave = None;
            self.pending.clear();
            if self.dave_ver > 0 {
                self.reinit(&mut ws).await?;
            }
            break;
        }

        // registers our ssrc; the silence lets Discord learn our address before we ever talk
        send(&mut ws, &json!({ "op": 5, "d": { "speaking": 0, "delay": 0, "ssrc": self.ssrc } })).await?;
        // ask the SFU for everyone's video at full quality, like the desktop client does
        send(&mut ws, &json!({ "op": 15, "d": { "any": 100 } })).await?;
        for _ in 0..5 {
            self.send_media(&SILENCE);
        }
        for m in early {
            self.handle(&mut ws, m).await?;
        }
        Ok((ws, beat.unwrap_or(Duration::from_secs(13))))
    }

    async fn resume(&mut self) -> Result<(Sock, Duration), String> {
        let mut ws = open(&self.c.endpoint).await?;
        send(&mut ws, &json!({ "op": 7, "d": {
            "server_id": self.c.server.to_string(),
            "session_id": self.c.session,
            "token": self.c.token,
            "seq_ack": self.seq_ack,
        }}))
        .await?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut beat = None;
        let mut resumed = false;
        while beat.is_none() || !resumed {
            let m = next(&mut ws, deadline).await?;
            let op = match &m {
                Ws::Text(t) => serde_json::from_str::<Value>(t).unwrap_or_default(),
                _ => Value::Null,
            };
            match op["op"].as_u64() {
                Some(8) => beat = Some(hb(&op)),
                Some(9) => resumed = true,
                _ => self.handle(&mut ws, m).await?,
            }
        }
        Ok((ws, beat.unwrap_or(Duration::from_secs(13))))
    }

    async fn pump(
        &mut self,
        ws: &mut Sock,
        beat: Duration,
        frames: &mut mpsc::UnboundedReceiver<Out>,
        vframes: &mut mpsc::UnboundedReceiver<Encoded>,
        plis: &mut mpsc::UnboundedReceiver<u32>,
        ctl: &mut mpsc::Receiver<Ctl>,
    ) -> End {
        let udp = self.udp.clone();
        let mut buf = vec![0u8; 2048];
        let mut next_beat = Instant::now() + beat;
        let mut unacked = 0;
        let mut tick = interval(Duration::from_millis(100));
        loop {
            let r = tokio::select! {
                m = ws.next() => match m {
                    Some(Ok(Ws::Close(f))) => {
                        let code: u16 = f.map(|f| f.code.into()).unwrap_or(1000);
                        return match code {
                            4004 | 4011 | 4014 | 4021 | 4022 => End::Fatal(close_reason(code)),
                            4006 | 4009 => End::Fresh,
                            _ => End::Resume,
                        };
                    }
                    Some(Ok(m)) => self.handle(ws, m).await,
                    Some(Err(_)) | None => return End::Resume,
                },
                r = udp.recv(&mut buf) => {
                    if let Ok(n) = r {
                        self.packet(&buf[..n]);
                    }
                    Ok(())
                }
                f = frames.recv() => match f {
                    Some(f) => self.frame(ws, f).await,
                    None => return End::Quit,
                },
                _ = sleep_until(next_beat) => {
                    if unacked >= 2 {
                        return End::Resume;
                    }
                    unacked += 1;
                    next_beat = Instant::now() + beat;
                    let t = chrono::Utc::now().timestamp_millis();
                    send(ws, &json!({ "op": 3, "d": { "t": t, "seq_ack": self.seq_ack } })).await
                }
                f = vframes.recv() => {
                    if let Some(f) = f {
                        self.send_video(f);
                    }
                    Ok(())
                }
                s = plis.recv() => {
                    if let Some(s) = s {
                        self.pli(s);
                    }
                    Ok(())
                }
                _ = tick.tick() => {
                    self.quiet();
                    Ok(())
                }
                c = ctl.recv() => match c {
                    Some(Ctl::Camera(on)) => self.video(ws, on.then_some(Source::Camera)).await,
                    None => return End::Quit,
                },
            };
            if r.is_err() {
                return End::Resume;
            }
            if self.acked {
                unacked = 0;
                self.acked = false;
            }
            if self.fails > FAIL_LIMIT
                && let Some(t) = self.last_transition
            {
                if send(ws, &json!({ "op": 31, "d": { "transition_id": t } })).await.is_err() {
                    return End::Resume;
                }
                let _ = self.reinit(ws).await;
            }
        }
    }

    async fn handle(&mut self, ws: &mut Sock, m: Ws) -> Result<(), String> {
        match m {
            Ws::Text(t) => {
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                if let Some(s) = v["seq"].as_i64() {
                    self.seq_ack = s;
                }
                self.text(ws, v).await
            }
            Ws::Binary(b) if b.len() >= 3 => {
                self.seq_ack = u16::from_be_bytes([b[0], b[1]]) as i64;
                self.binary(ws, b[2], &b[3..]).await
            }
            _ => Ok(()),
        }
    }

    async fn text(&mut self, ws: &mut Sock, v: Value) -> Result<(), String> {
        let d = &v["d"];
        match v["op"].as_u64().unwrap_or(99) {
            5 => self.map_ssrc(d["ssrc"].as_u64(), sid(&d["user_id"])),
            6 => self.acked = true,
            11 => {
                for u in d["user_ids"].as_array().into_iter().flatten() {
                    self.known.insert(sid(u));
                }
            }
            12 => {
                let u = sid(&d["user_id"]);
                self.known.insert(u);
                self.map_ssrc(d["audio_ssrc"].as_u64(), u);
                let v = d["video_ssrc"].as_u64().unwrap_or(0) as u32;
                let active = d["streams"].as_array().map(|a| a.iter().any(|s| s["active"].as_bool() != Some(false))).unwrap_or(v != 0);
                self.vusers.retain(|_, x| *x != u);
                if v != 0 && active {
                    self.vusers.insert(v, u);
                    self.pli(v);
                } else {
                    self.emit(Kind::VideoOff(u));
                }
            }
            13 => {
                let u = sid(&d["user_id"]);
                self.known.remove(&u);
                self.users.retain(|_, x| *x != u);
                self.vusers.retain(|_, x| *x != u);
                self.emit(Kind::VideoOff(u));
                let gone: Vec<u32> = self.inbound.iter().filter(|(_, i)| i.user == u).map(|(s, _)| *s).collect();
                for s in gone {
                    self.drop_inbound(s);
                }
            }
            21 => {
                let id = d["transition_id"].as_u64().unwrap_or(0) as u16;
                let ver = d["protocol_version"].as_u64().unwrap_or(0) as u16;
                self.pending.insert(id, ver);
                if id == 0 {
                    self.execute(0);
                } else {
                    if ver == 0
                        && let Some(dave) = &mut self.dave
                    {
                        dave.set_passthrough_mode(true, Some(24));
                    }
                    send(ws, &json!({ "op": 23, "d": { "transition_id": id } })).await?;
                }
            }
            22 => self.execute(d["transition_id"].as_u64().unwrap_or(0) as u16),
            24 => {
                if d["epoch"].as_u64() == Some(1) {
                    self.dave_ver = d["protocol_version"].as_u64().unwrap_or(0) as u16;
                    self.reinit(ws).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn binary(&mut self, ws: &mut Sock, op: u8, p: &[u8]) -> Result<(), String> {
        let Some(dave) = &mut self.dave else { return Ok(()) };
        match op {
            25 => {
                let _ = dave.set_external_sender(p);
            }
            27 if !p.is_empty() => {
                let kind = if p[0] == 0 { ProposalsOperationType::APPEND } else { ProposalsOperationType::REVOKE };
                let known: Vec<u64> = self.known.iter().copied().collect();
                if let Ok(Some(cw)) = dave.process_proposals(kind, &p[1..], Some(&known)) {
                    send_bin(ws, 28, &[&cw.commit, cw.welcome.as_deref().unwrap_or_default()]).await?;
                }
            }
            29 | 30 if p.len() >= 2 => {
                let id = u16::from_be_bytes([p[0], p[1]]);
                let ok = if op == 29 { dave.process_commit(&p[2..]).is_ok() } else { dave.process_welcome(&p[2..]).is_ok() };
                if !ok {
                    send(ws, &json!({ "op": 31, "d": { "transition_id": id } })).await?;
                    return self.reinit(ws).await;
                }
                if id == 0 {
                    self.last_transition = Some(0);
                } else {
                    self.pending.insert(id, self.dave_ver);
                    send(ws, &json!({ "op": 23, "d": { "transition_id": id } })).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn reinit(&mut self, ws: &mut Sock) -> Result<(), String> {
        self.fails = 0;
        let Some(ver) = NonZeroU16::new(self.dave_ver) else {
            if let Some(dave) = &mut self.dave {
                let _ = dave.reset();
                dave.set_passthrough_mode(true, Some(10));
            }
            return Ok(());
        };
        let (me, ch) = (self.c.me, self.c.channel);
        let mut dave = match self.dave.take() {
            Some(mut d) => d.reinit(ver, me, ch, None).map(|_| d).map_err(|e| e.to_string())?,
            None => DaveSession::new(ver, me, ch, None).map_err(|e| e.to_string())?,
        };
        let kp = dave.create_key_package().map_err(|e| e.to_string());
        self.dave = Some(dave);
        send_bin(ws, 26, &[&kp?]).await
    }

    fn execute(&mut self, id: u16) {
        let Some(ver) = self.pending.remove(&id) else { return };
        let old = self.dave_ver;
        self.dave_ver = ver;
        if ver == 0 && old != 0 {
            self.downgraded = true;
        } else if id > 0 && self.downgraded {
            self.downgraded = false;
            if let Some(d) = &mut self.dave {
                d.set_passthrough_mode(true, Some(10));
            }
        }
        self.last_transition = Some(id);
    }

    fn map_ssrc(&mut self, ssrc: Option<u64>, user: Id) {
        let Some(ssrc) = ssrc.filter(|s| *s != 0).map(|s| s as u32) else { return };
        if user == 0 {
            return;
        }
        self.users.insert(ssrc, user);
        if let Some(i) = self.inbound.get_mut(&ssrc) {
            i.user = user;
        }
    }

    async fn frame(&mut self, ws: &mut Sock, f: Out) -> Result<(), String> {
        match f {
            Out::Voice(opus) => {
                if !self.talking {
                    self.talking = true;
                    send(ws, &json!({ "op": 5, "d": { "speaking": 1, "delay": 0, "ssrc": self.ssrc } })).await?;
                    self.emit(Kind::Speaking(self.c.me, true));
                }
                self.shared.voiced.fetch_add(1, Relaxed);
                self.send_media(&opus);
            }
            Out::Silence => self.send_media(&SILENCE),
            Out::Idle => {
                if self.talking {
                    self.talking = false;
                    send(ws, &json!({ "op": 5, "d": { "speaking": 0, "delay": 0, "ssrc": self.ssrc } })).await?;
                    self.emit(Kind::Speaking(self.c.me, false));
                }
            }
        }
        Ok(())
    }

    fn send_media(&mut self, opus: &[u8]) {
        let Some(cipher) = &self.cipher else { return };
        let body = match &mut self.dave {
            Some(d) if self.dave_ver != 0 && d.is_ready() => match d.encrypt_opus(opus) {
                Ok(b) => {
                    if opus != SILENCE {
                        self.shared.e2ee_out.fetch_add(1, Relaxed);
                    }
                    b.into_owned()
                }
                Err(_) => return,
            },
            _ => opus.to_vec(),
        };
        // wall clock, so a pause in speech shows up as a timestamp jump like the receiver expects
        let now = (self.clock.elapsed().as_millis() as u32).wrapping_mul(48) / FRAME as u32 * FRAME as u32;
        self.ts = if now.wrapping_sub(self.ts) as i32 > FRAME as i32 { now } else { self.ts.wrapping_add(FRAME as u32) };
        let h = rtp::header(rtp::OPUS_PT, self.seq, self.ts, self.ssrc);
        self.seq = self.seq.wrapping_add(1);
        self.nonce = self.nonce.wrapping_add(1);
        let pkt = cipher.seal(&h, &body, self.nonce);
        if self.udp.try_send(&pkt).is_ok() {
            self.shared.sent.fetch_add(1, Relaxed);
        }
    }

    fn packet(&mut self, pkt: &[u8]) {
        if rtp::is_rtcp(pkt) {
            return self.rtcp(pkt);
        }
        if pkt.len() >= 12 && pkt[1] & 0x7f == H264_PT {
            return self.video_in(pkt);
        }
        if pkt.len() < 12 || pkt[1] & 0x7f != rtp::OPUS_PT {
            return;
        }
        let Some(cipher) = &self.cipher else { return };
        let Some(body) = cipher.open(pkt) else { return };
        self.shared.recv.fetch_add(1, Relaxed);
        let seq = u16::from_be_bytes([pkt[2], pkt[3]]);
        let ts = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
        let ssrc = u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
        let user = self.users.get(&ssrc).copied().unwrap_or(0);

        let opus = match &mut self.dave {
            Some(d) if body != SILENCE && d.is_ready() && (self.dave_ver != 0 || d.can_passthrough(user)) => {
                match d.decrypt(user, MediaType::AUDIO, &body) {
                    Ok(b) => {
                        self.fails = 0;
                        self.shared.e2ee_in.fetch_add(1, Relaxed);
                        b
                    }
                    Err(_) => {
                        if self.pending.is_empty() {
                            self.fails += 1;
                        }
                        return;
                    }
                }
            }
            _ => body,
        };
        self.decode(ssrc, user, seq, ts, &opus);
    }

    fn decode(&mut self, ssrc: u32, user: Id, seq: u16, ts: u32, opus: &[u8]) {
        if !self.inbound.contains_key(&ssrc) {
            let Ok(stereo) = opus_rs::OpusDecoder::new(48_000, 2) else { return };
            let out = self.slots.iter_mut().enumerate().find_map(|(n, s)| s.take().map(|p| (n, p)));
            let i = Inbound { user, stereo, mono: None, next: None, out, voice_at: None, speaking: false };
            self.inbound.insert(ssrc, i);
        }
        let me = self.c.me;
        let Some(i) = self.inbound.get_mut(&ssrc) else { return };
        if i.user == 0 {
            i.user = user;
        }

        // the ring in front of the mixer is the jitter buffer; here late packets are dropped,
        // short gaps get concealed and a long one is just the start of a new talk spurt
        let mut lost = 0;
        if let Some((want, last_ts)) = i.next {
            let d = seq.wrapping_sub(want) as i16;
            if d < 0 {
                return;
            }
            if d <= 3 && ts.wrapping_sub(last_ts) <= FRAME as u32 * 4 {
                lost = d as usize;
            }
        }
        i.next = Some((seq.wrapping_add(1), ts));
        if opus == SILENCE || opus.is_empty() {
            return;
        }

        let pcm = &mut self.pcm;
        for _ in 0..lost {
            if i.stereo.decode(&[], FRAME, pcm).is_ok() {
                push(&mut i.out, pcm);
            }
        }
        // mono packets need their own decoder, opus-rs won't upmix for us
        let ok = if opus[0] & 0x04 != 0 {
            i.stereo.decode(opus, FRAME, pcm).is_ok()
        } else {
            let mono = match &mut i.mono {
                Some(m) => m,
                None => match opus_rs::OpusDecoder::new(48_000, 1) {
                    Ok(m) => i.mono.insert(m),
                    Err(_) => return,
                },
            };
            let mut tmp = [0f32; FRAME];
            let ok = mono.decode(opus, FRAME, &mut tmp).is_ok();
            for (k, x) in tmp.iter().enumerate() {
                pcm[k * 2] = *x;
                pcm[k * 2 + 1] = *x;
            }
            ok
        };
        if !ok {
            return;
        }
        self.shared.decoded.fetch_add(1, Relaxed);
        push(&mut i.out, pcm);
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
        if rms > 0.001 {
            i.voice_at = Some(Instant::now());
            if !i.speaking && i.user != 0 && i.user != me {
                i.speaking = true;
                let u = i.user;
                self.emit(Kind::Speaking(u, true));
            }
        }
    }

    fn video_in(&mut self, pkt: &[u8]) {
        let Some(cipher) = &self.cipher else { return };
        let Some(body) = cipher.open(pkt) else { return };
        let seq = u16::from_be_bytes([pkt[2], pkt[3]]);
        let ts = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
        let ssrc = u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
        let first = !self.asm.contains_key(&ssrc);
        let got = self.asm.entry(ssrc).or_default().push(seq, ts, pkt[1] & 0x80 != 0, &body);
        if first {
            self.pli(ssrc);
        }
        let frame = match got {
            Got::Frame(f) => f,
            Got::Lost => return self.pli(ssrc),
            Got::Nothing => return,
        };
        let user = self.vusers.get(&ssrc).copied().unwrap_or(0);
        let frame = match &mut self.dave {
            Some(d) if d.is_ready() && (self.dave_ver != 0 || d.can_passthrough(user)) => match d.decrypt(user, MediaType::VIDEO, &frame) {
                Ok(f) => f,
                Err(_) => return self.pli(ssrc),
            },
            _ => frame,
        };
        if let Some(dec) = &self.decode {
            dec.push(ssrc, user, frame);
        }
    }

    /// Asks the sender for a keyframe (RTCP PLI), at most twice a second per stream.
    fn pli(&mut self, media: u32) {
        let Some(cipher) = &self.cipher else { return };
        if self.pli_at.get(&media).is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            return;
        }
        self.pli_at.insert(media, Instant::now());
        let mut h = [0x81, 206, 0, 2, 0, 0, 0, 0];
        h[4..8].copy_from_slice(&self.ssrc.to_be_bytes());
        self.nonce = self.nonce.wrapping_add(1);
        let _ = self.udp.try_send(&cipher.seal(&h, &media.to_be_bytes(), self.nonce));
    }

    fn rtcp(&mut self, pkt: &[u8]) {
        // PLI (fmt 1) or FIR (fmt 4) aimed at our video: next frame goes out as a keyframe
        if pkt.len() < 8 || pkt[1] != 206 || !matches!(pkt[0] & 0x1f, 1 | 4) {
            return;
        }
        let Some(plain) = self.cipher.as_ref().and_then(|c| c.open_rtcp(pkt)) else { return };
        if plain.len() >= 4
            && u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]) == self.vssrc
            && let Some(cam) = &self.cam
        {
            cam.key.store(true, Relaxed);
        }
    }

    /// Starts or stops our outgoing video and tells the gateway (op 12).
    async fn video(&mut self, ws: &mut Sock, src: Option<Source>) -> Result<(), String> {
        self.cam = None;
        if let Some(src) = src {
            let (tx, tag, me) = (self.tx.clone(), self.c.tag, self.c.me);
            let preview = move |img| tx.send(net::Ev::Voice(Ev { tag, kind: Kind::Frame(me, img) }));
            match video::send(src, self.vout.clone(), self.shared.clone(), Box::new(preview)) {
                Ok(s) => self.cam = Some(s),
                Err(e) => self.emit(Kind::Error(format!("couldn't start {}: {e}", if src == Source::Camera { "the camera" } else { "screen capture" }))),
            }
        }
        if self.cam.is_none() {
            self.emit(Kind::VideoOff(self.c.me));
        }
        self.announce(ws).await
    }

    async fn announce(&mut self, ws: &mut Sock) -> Result<(), String> {
        let d = match &self.cam {
            Some(cam) => json!({
                "audio_ssrc": self.ssrc, "video_ssrc": self.vssrc, "rtx_ssrc": self.rtx,
                "streams": [{
                    "type": "video", "rid": "100", "ssrc": self.vssrc, "active": true, "quality": 100, "rtx_ssrc": self.rtx,
                    "max_bitrate": 2_500_000, "max_framerate": cam.fps,
                    "max_resolution": { "type": "fixed", "width": cam.size.0, "height": cam.size.1 },
                }],
            }),
            None => json!({ "audio_ssrc": self.ssrc, "video_ssrc": 0, "rtx_ssrc": 0, "streams": [] }),
        };
        send(ws, &json!({ "op": 12, "d": d })).await
    }

    fn send_video(&mut self, f: Encoded) {
        let frame = match &mut self.dave {
            Some(d) if self.dave_ver != 0 && d.is_ready() => match d.encrypt(MediaType::VIDEO, Codec::H264, &f.data) {
                Ok(b) => b.into_owned(),
                Err(_) => return,
            },
            _ => f.data,
        };
        let Some(cipher) = &self.cipher else { return };
        let (mut seq, mut nonce) = (self.vseq, self.nonce);
        video::packetize(&frame, |p, marker| {
            let mut h = rtp::header(H264_PT | if marker { 0x80 } else { 0 }, seq, f.ts, self.vssrc).to_vec();
            // one header extension, playout-delay = 0, the same thing Discord's own clients send
            h[0] |= 0x10;
            h.extend_from_slice(&[0xbe, 0xde, 0, 1]);
            let body = [&[0x51, 0, 0, 0][..], &p].concat();
            seq = seq.wrapping_add(1);
            nonce = nonce.wrapping_add(1);
            let _ = self.udp.try_send(&cipher.seal(&h, &body, nonce));
        });
        self.vseq = seq;
        self.nonce = nonce;
    }

    fn quiet(&mut self) {
        let mut off = Vec::new();
        for i in self.inbound.values_mut() {
            if i.speaking && i.voice_at.is_none_or(|t| t.elapsed() > Duration::from_millis(300)) {
                i.speaking = false;
                off.push(i.user);
            }
        }
        for u in off {
            self.emit(Kind::Speaking(u, false));
        }
    }

    fn drop_inbound(&mut self, ssrc: u32) {
        let Some(i) = self.inbound.remove(&ssrc) else { return };
        if let Some((n, p)) = i.out {
            self.slots[n] = Some(p);
        }
        if i.speaking {
            self.emit(Kind::Speaking(i.user, false));
        }
    }
}

fn push(out: &mut Option<(usize, Producer<f32>)>, pcm: &[f32]) {
    let Some((_, p)) = out else { return };
    if let Ok(chunk) = p.write_chunk_uninit(pcm.len()) {
        chunk.fill_from_iter(pcm.iter().copied());
    }
}

async fn open(endpoint: &str) -> Result<Sock, String> {
    let (ws, _) = timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(url(endpoint)))
        .await
        .map_err(|_| "voice gateway timed out".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(ws)
}

async fn discover(udp: &UdpSocket, ssrc: u32) -> Result<(String, u16), String> {
    let mut req = [0u8; 74];
    req[0..2].copy_from_slice(&1u16.to_be_bytes());
    req[2..4].copy_from_slice(&70u16.to_be_bytes());
    req[4..8].copy_from_slice(&ssrc.to_be_bytes());
    let mut buf = [0u8; 2048];
    for _ in 0..5 {
        udp.send(&req).await.map_err(|e| e.to_string())?;
        // media can beat the reply here, so keep reading until this attempt's second is up
        let deadline = Instant::now() + Duration::from_secs(1);
        while let Ok(Ok(n)) = timeout(deadline.saturating_duration_since(Instant::now()), udp.recv(&mut buf)).await {
            if n < 74 || u16::from_be_bytes([buf[0], buf[1]]) != 2 {
                continue;
            }
            let ip = &buf[8..72];
            let end = ip.iter().position(|b| *b == 0).unwrap_or(ip.len());
            let ip = String::from_utf8_lossy(&ip[..end]).to_string();
            return Ok((ip, u16::from_be_bytes([buf[72], buf[73]])));
        }
    }
    Err("ip discovery failed".into())
}

fn hb(v: &Value) -> Duration {
    Duration::from_millis(v["d"]["heartbeat_interval"].as_f64().unwrap_or(13750.0) as u64)
}

fn close_reason(code: u16) -> String {
    match code {
        4004 => "voice authentication failed".into(),
        4011 => "voice server not found".into(),
        4014 => "disconnected from voice".into(),
        4021 => "voice rate limited".into(),
        4022 => "call ended".into(),
        _ => format!("voice closed ({code})"),
    }
}

async fn send(ws: &mut Sock, v: &Value) -> Result<(), String> {
    ws.send(Ws::text(v.to_string())).await.map_err(|e| e.to_string())
}

async fn send_bin(ws: &mut Sock, op: u8, parts: &[&[u8]]) -> Result<(), String> {
    let mut b = vec![op];
    for p in parts {
        b.extend_from_slice(p);
    }
    ws.send(Ws::binary(b)).await.map_err(|e| e.to_string())
}

async fn next(ws: &mut Sock, deadline: Instant) -> Result<Ws, String> {
    match timeout(deadline.saturating_duration_since(Instant::now()), ws.next()).await {
        Err(_) => Err("voice handshake timed out".into()),
        Ok(Some(Ok(Ws::Close(f)))) => Err(close_reason(f.map(|f| f.code.into()).unwrap_or(1000))),
        Ok(Some(Ok(m))) => Ok(m),
        Ok(_) => Err("voice gateway closed".into()),
    }
}
