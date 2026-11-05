//! H264 over RTP (RFC 6184) both ways, the decode worker, and the capture + encode thread.

use egui::ColorImage;
use openh264::OpenH264API;
use openh264::decoder::Decoder;
use openh264::encoder::{BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType};
use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc as smpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use super::capture;
use super::hwenc::{self, Convert, HwEnc};
use super::{Ev, Kind, Shared};
use crate::net::{self, Tx};
use crate::state::Id;

pub const H264_PT: u8 = 101;
pub const RTX_PT: u8 = 102;
pub const MTU: usize = 1200;

/// Rebuilds Annex B access units from RTP payloads of one SSRC.
#[derive(Default)]
pub struct Assembler {
    buf: Vec<u8>,
    ts: Option<u32>,
    next: Option<u16>,
    broken: bool,
    in_fu: bool,
}

pub enum Got {
    Nothing,
    Frame(Vec<u8>),
    /// a frame went missing; the decoder needs a keyframe
    Lost,
}

impl Assembler {
    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, p: &[u8]) -> Got {
        let mut lost = false;
        if self.ts != Some(ts) {
            // a new frame started before the old one saw its marker
            lost = self.ts.is_some() && !self.buf.is_empty();
            self.buf.clear();
            self.broken = false;
            self.in_fu = false;
            self.ts = Some(ts);
        }
        if self.next.is_some_and(|n| n != seq) {
            self.broken = true;
        }
        self.next = Some(seq.wrapping_add(1));
        if p.is_empty() {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        match p[0] & 0x1f {
            1..=23 => {
                self.buf.extend_from_slice(&[0, 0, 0, 1]);
                self.buf.extend_from_slice(p);
            }
            24 => {
                let mut rest = &p[1..];
                while rest.len() > 2 {
                    let n = u16::from_be_bytes([rest[0], rest[1]]) as usize;
                    if rest.len() < 2 + n {
                        self.broken = true;
                        break;
                    }
                    self.buf.extend_from_slice(&[0, 0, 0, 1]);
                    self.buf.extend_from_slice(&rest[2..2 + n]);
                    rest = &rest[2 + n..];
                }
            }
            28 if p.len() > 2 => {
                if p[1] & 0x80 != 0 {
                    self.buf.extend_from_slice(&[0, 0, 0, 1, (p[0] & 0xe0) | (p[1] & 0x1f)]);
                    self.in_fu = true;
                } else if !self.in_fu {
                    self.broken = true;
                }
                self.buf.extend_from_slice(&p[2..]);
                if p[1] & 0x40 != 0 {
                    self.in_fu = false;
                }
            }
            _ => {}
        }
        if !marker {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        let frame = std::mem::take(&mut self.buf);
        self.ts = None;
        if self.broken || lost { Got::Lost } else { Got::Frame(frame) }
    }
}

fn nals(frame: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i + 3 <= frame.len() {
        if frame[i] == 0 && frame[i + 1] == 0 && frame[i + 2] == 1 {
            if let Some(s) = start {
                // a 4-byte start code leaves its leading zero on the previous unit
                let end = if i > 0 && frame[i - 1] == 0 { i - 1 } else { i };
                out.push(&frame[s..end]);
            }
            i += 3;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        out.push(&frame[s..]);
    }
    out.retain(|n| !n.is_empty());
    out
}

/// Splits an Annex B frame into RTP payloads: single NAL units, or FU-A fragments when too big.
/// The bool is the marker bit, set on the last packet of the frame.
pub fn packetize(frame: &[u8], mut emit: impl FnMut(Vec<u8>, bool)) {
    let units = nals(frame);
    let n = units.len();
    for (k, nal) in units.into_iter().enumerate() {
        let last = k + 1 == n;
        if nal.len() <= MTU {
            emit(nal.to_vec(), last);
            continue;
        }
        let (head, body) = (nal[0], &nal[1..]);
        let chunks: Vec<&[u8]> = body.chunks(MTU - 2).collect();
        let c = chunks.len();
        for (j, chunk) in chunks.into_iter().enumerate() {
            let mut fu = vec![(head & 0xe0) | 28, head & 0x1f];
            if j == 0 {
                fu[1] |= 0x80;
            }
            if j + 1 == c {
                fu[1] |= 0x40;
            }
            fu.extend_from_slice(chunk);
            emit(fu, last && j + 1 == c);
        }
    }
}

// the hardware encoders and openh264, as Voice & Video's switches have them
static HW: AtomicBool = AtomicBool::new(true);
static SOFT: AtomicBool = AtomicBool::new(true);

pub fn set_codecs(hw: bool, openh264: bool) {
    HW.store(hw, Relaxed);
    SOFT.store(openh264, Relaxed);
}

/// Decodes on its own thread so a 1080p keyframe never stalls the media loop.
pub struct Decode {
    tx: smpsc::Sender<(u32, Id, Vec<u8>)>,
}

impl Decode {
    pub fn start(tag: u64, out: Tx, shared: Arc<Shared>, pli: mpsc::UnboundedSender<u32>) -> Decode {
        let (tx, rx) = smpsc::channel::<(u32, Id, Vec<u8>)>();
        let _ = std::thread::Builder::new().name("video-decode".into()).spawn(move || {
            let mut decs: HashMap<u32, Decoder> = HashMap::new();
            let mut rgba = Vec::new();
            while let Ok((ssrc, user, frame)) = rx.recv() {
                let dec = match decs.get_mut(&ssrc) {
                    Some(d) => d,
                    None => match SOFT.load(Relaxed).then(Decoder::new) {
                        Some(Ok(d)) => decs.entry(ssrc).or_insert(d),
                        _ => continue,
                    },
                };
                match dec.decode(&frame) {
                    Ok(Some(yuv)) => {
                        let (w, h) = yuv.dimensions();
                        rgba.resize(w * h * 4, 0);
                        yuv.write_rgba8(&mut rgba);
                        shared.video_in.fetch_add(1, Relaxed);
                        let img = Arc::new(ColorImage::from_rgba_unmultiplied([w, h], &rgba));
                        out.send(net::Ev::Voice(Ev { tag, kind: Kind::Frame(user, img) }));
                    }
                    Ok(None) => {}
                    Err(_) => {
                        let _ = pli.send(ssrc);
                    }
                }
            }
        });
        Decode { tx }
    }

    pub fn push(&self, ssrc: u32, user: Id, frame: Vec<u8>) {
        let _ = self.tx.send((ssrc, user, frame));
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Camera,
    // what Go Live shares, picked in the share dialog: a window, a monitor, or a capture device
    Window(isize),
    Monitor(isize),
    Device(usize),
}

/// How Go Live sends: the tallest the picture gets (0 keeps the source's size) and its frame rate.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Quality {
    pub height: usize,
    pub fps: u32,
}

impl Quality {
    pub const CAMERA: Quality = Quality { height: 720, fps: 30 };
}

pub struct Encoded {
    pub data: Vec<u8>,
    /// 90kHz RTP clock
    pub ts: u32,
}

/// Capture + encode thread; dropping it stops the capture.
pub struct Sender {
    stop: Arc<AtomicBool>,
    pub key: Arc<AtomicBool>,
    pub size: (usize, usize),
    pub fps: u32,
    pub bitrate: u32,
    th: Option<JoinHandle<()>>,
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.th.take() {
            let _ = t.join();
        }
    }
}

pub type Preview = Box<dyn Fn(Arc<ColorImage>) + Send>;

pub fn send(src: Source, q: Quality, out: mpsc::UnboundedSender<Encoded>, shared: Arc<Shared>, preview: Preview) -> Result<Sender, String> {
    let fake = std::env::var_os("RUSTCORD_FAKE_VIDEO").is_some() || (src == Source::Camera && std::env::var_os("RUSTCORD_FAKE_CAM").is_some());
    let fps = q.fps;
    let stop = Arc::new(AtomicBool::new(false));
    let key = Arc::new(AtomicBool::new(true));
    let (s2, k2) = (stop.clone(), key.clone());
    // COM objects stay on the thread that made them, so the source is opened over there
    let (opened, ready) = smpsc::sync_channel(1);
    let th = std::thread::Builder::new()
        .name("video-encode".into())
        .spawn(move || {
            let (mut pipe, mut size, bps) = match Pipe::open(src, fake, q.height, fps, HW.load(Relaxed)) {
                Ok(p) => p,
                Err(e) => {
                    let _ = opened.send(Err(e));
                    return;
                }
            };
            let _ = opened.send(Ok((size, bps)));
            let t0 = Instant::now();
            let frame_time = Duration::from_secs(1) / fps;
            let mut next = Instant::now();
            let mut n = 0u64;
            while !s2.load(Relaxed) {
                // never behind by more than a frame, so a hiccup doesn't come out as a burst
                next = (next + frame_time).max(Instant::now());
                if !pipe.paced()
                    && let Some(d) = next.checked_duration_since(Instant::now())
                {
                    std::thread::sleep(d);
                }
                n += 1;
                // our own tile at about 10 a second, whatever the stream runs at
                let look = ((n - 1) % (fps as u64 / 10).max(1) == 0).then_some(&preview);
                let time = (t0.elapsed().as_nanos() / 100) as i64;
                let key = k2.swap(false, Relaxed);
                let mut frames = Vec::new();
                match pipe.step(size, time, key, frame_time / 2, look, &mut frames) {
                    Ok(true) => {}
                    // nothing went in, so a keyframe asked for waits for the next one
                    Ok(false) => {
                        k2.fetch_or(key, Relaxed);
                    }
                    // the card gave out mid-stream: carry on in software
                    Err(_) => match Pipe::open(src, fake, q.height, fps, false) {
                        Ok((p, s, _)) => {
                            (pipe, size) = (p, s);
                            k2.store(true, Relaxed);
                        }
                        Err(_) => break,
                    },
                }
                for (data, t) in frames {
                    shared.video_out.fetch_add(1, Relaxed);
                    let ts = (t as u64 * 9 / 1000) as u32;
                    if out.send(Encoded { data, ts }).is_err() {
                        return;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let (size, bitrate) = ready.recv().map_err(|e| e.to_string())??;
    Ok(Sender { stop, key, size, fps, bitrate, th: Some(th) })
}

enum Pipe {
    // windows and monitors stay on the card from capture through to the encoder
    Card { wgc: capture::Wgc, conv: Convert, enc: HwEnc },
    Cpu { grab: capture::Grab, bgra: Vec<u8>, enc: Enc },
}

enum Enc {
    Card(HwEnc, Vec<u8>),
    Soft(Box<Encoder>),
}

impl Pipe {
    /// The cheapest way there is to send `src`, with the size it goes at and its bitrate.
    fn open(src: Source, fake: bool, height: usize, fps: u32, hw: bool) -> Result<(Pipe, (usize, usize), u32), String> {
        if hw
            && !fake
            && let Some(p) = card(src, height, fps)
        {
            return Ok(p);
        }
        let screen = matches!(src, Source::Window(_) | Source::Monitor(_));
        let grab = if fake { capture::Grab::fake(screen) } else { capture::Grab::open(src)? };
        let (w, h) = fit(grab.size(), height);
        let bps = bitrate(w, h, fps);
        let enc = match hw.then(|| HwEnc::any(w, h, fps, bps).ok()).flatten() {
            Some(e) => Enc::Card(e, Vec::new()),
            None => Enc::Soft(Box::new(soft(fps, bps).ok_or("no h264 encoder")?)),
        };
        Ok((Pipe::Cpu { grab, bgra: vec![0; w * h * 4], enc }, (w, h), bps))
    }

    fn paced(&self) -> bool {
        matches!(self, Pipe::Cpu { grab, .. } if grab.paced())
    }

    /// One tick: the newest picture to the encoder, if it'll take one. False if nothing went in.
    fn step(&mut self, (w, h): (usize, usize), time: i64, key: bool, wait: Duration, look: Option<&Preview>, frames: &mut Vec<(Vec<u8>, i64)>) -> windows::core::Result<bool> {
        match self {
            Pipe::Card { wgc, conv, enc } => {
                // a still window sends nothing new, so the last frame goes again
                if let Some((_f, tex, (cw, ch))) = unsafe { wgc.latest() } {
                    conv.put(&tex, cw, ch)?;
                }
                let sent = conv.has() && enc.ready(wait)?;
                if sent {
                    conv.draw(enc.slot())?;
                    enc.submit(time, key)?;
                }
                if let Some(p) = look
                    && let Some(img) = conv.preview()
                {
                    p(Arc::new(img));
                }
                frames.extend(enc.take());
                Ok(sent)
            }
            Pipe::Cpu { grab, bgra, enc } => {
                if !grab.frame(bgra, w, h) {
                    return Ok(false);
                }
                if let Some(p) = look {
                    let px = bgra.chunks_exact(4).map(|p| egui::Color32::from_rgb(p[2], p[1], p[0])).collect();
                    p(Arc::new(ColorImage::new([w, h], px)));
                }
                match enc {
                    Enc::Card(e, yuv) => {
                        let sent = e.ready(wait)?;
                        if sent {
                            hwenc::nv12(bgra, w, h, yuv);
                            e.upload(yuv);
                            e.submit(time, key)?;
                        }
                        frames.extend(e.take());
                        Ok(sent)
                    }
                    Enc::Soft(e) => {
                        if key {
                            e.force_intra_frame();
                        }
                        let yuv = YUVBuffer::from_bgra8_source(BgraSliceU8::new(bgra, (w, h)));
                        if let Ok(bits) = e.encode(&yuv) {
                            let data = bits.to_vec();
                            if !data.is_empty() {
                                frames.push((data, time));
                            }
                        }
                        Ok(true)
                    }
                }
            }
        }
    }
}

// capture, scaling, conversion and encoding all on one card, the first that can do the lot
// (the one driving the main display, usually), so the picture never comes back to the cpu
fn card(src: Source, height: usize, fps: u32) -> Option<(Pipe, (usize, usize), u32)> {
    let (target, monitor) = match src {
        Source::Window(h) => (h, false),
        Source::Monitor(m) => (m, true),
        _ => return None,
    };
    hwenc::adapters().into_iter().find_map(|a| {
        let (dev, ctx) = hwenc::device(Some(&a)).ok()?;
        let wgc = unsafe { capture::Wgc::open(&dev, &ctx, target, monitor) }.ok()?;
        let (w, h) = fit((wgc.w, wgc.h), height);
        let bps = bitrate(w, h, fps);
        let enc = HwEnc::on(&dev, &ctx, &a, w, h, fps, bps).ok()?;
        let conv = Convert::new(&dev, &ctx, &enc, fps, fit((w, h), 720)).ok()?;
        Some((Pipe::Card { wgc, conv, enc }, (w, h), bps))
    })
}

// 2.5Mbit for 720p at 30, scaled with the pixels sent each second
fn bitrate(w: usize, h: usize, fps: u32) -> u32 {
    (2_500_000.0 * (w * h) as f64 * fps as f64 / (1280.0 * 720.0 * 30.0)).clamp(1_500_000.0, 8_000_000.0) as u32
}

// for when no card's encoder works
fn soft(fps: u32, bps: u32) -> Option<Encoder> {
    if !SOFT.load(Relaxed) {
        return None;
    }
    // openh264's screen-content mode takes 40-70ms a frame at 720p, too slow for 30fps;
    // the real-time camera mode on low, split over a few threads, takes a few ms
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(4)) as u16;
    let cfg = EncoderConfig::new()
        .bitrate(BitRate::from_bps(bps))
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .usage_type(UsageType::CameraVideoRealTime)
        .complexity(Complexity::Low)
        .num_threads(threads)
        .max_slice_len(1200)
        // a keyframe every couple of seconds so late joiners don't wait on a PLI round trip
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2));
    Encoder::with_api_config(OpenH264API::from_source(), cfg).ok()
}

// scale down to fit the chosen height (and its 16:9 width), keeping even dimensions for 4:2:0
fn fit((w, h): (usize, usize), height: usize) -> (usize, usize) {
    let max_h = if height == 0 { 2160 } else { height };
    let k = (max_h as f32 * 16.0 / 9.0 / w as f32).min(max_h as f32 / h as f32).min(1.0);
    (((w as f32 * k) as usize) & !1, ((h as f32 * k) as usize) & !1)
}
