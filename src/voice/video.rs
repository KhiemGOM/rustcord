//! H264 over RTP (RFC 6184) both ways, the decode worker, and the capture + encode thread.

use egui::ColorImage;
use openh264::OpenH264API;
use openh264::decoder::Decoder;
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType};
use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc as smpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use super::capture;
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
                    None => match Decoder::new() {
                        Ok(d) => decs.entry(ssrc).or_insert(d),
                        Err(_) => continue,
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
    Screen,
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

const MAX_W: usize = 1280;
const MAX_H: usize = 720;

pub type Preview = Box<dyn Fn(Arc<ColorImage>) + Send>;

pub fn send(src: Source, out: mpsc::UnboundedSender<Encoded>, shared: Arc<Shared>, preview: Preview) -> Result<Sender, String> {
    let fake = std::env::var_os("FASTCORD_FAKE_VIDEO").is_some() || (src == Source::Camera && std::env::var_os("FASTCORD_FAKE_CAM").is_some());
    let fps = if src == Source::Screen { 15 } else { 30 };
    let stop = Arc::new(AtomicBool::new(false));
    let key = Arc::new(AtomicBool::new(true));
    let (s2, k2) = (stop.clone(), key.clone());
    // COM objects stay on the thread that made them, so the source is opened over there
    let (opened, ready) = smpsc::sync_channel(1);
    let th = std::thread::Builder::new()
        .name("video-encode".into())
        .spawn(move || {
            let grab = if fake { Ok(capture::Grab::fake(src == Source::Screen)) } else { capture::Grab::open(src) };
            let mut grab = match grab {
                Ok(g) => g,
                Err(e) => {
                    let _ = opened.send(Err(e));
                    return;
                }
            };
            let (w, h) = fit(grab.size());
            let _ = opened.send(Ok((w, h)));
            let usage = if src == Source::Screen { UsageType::ScreenContentRealTime } else { UsageType::CameraVideoRealTime };
            let cfg = EncoderConfig::new()
                .bitrate(BitRate::from_bps(2_500_000))
                .max_frame_rate(FrameRate::from_hz(fps as f32))
                .usage_type(usage)
                // a keyframe every couple of seconds so late joiners don't wait on a PLI round trip
                .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2));
            let Ok(mut enc) = Encoder::with_api_config(OpenH264API::from_source(), cfg) else { return };
            let t0 = Instant::now();
            let frame_time = Duration::from_secs(1) / fps;
            let mut next = Instant::now();
            let mut bgra = vec![0u8; w * h * 4];
            let mut n = 0u64;
            while !s2.load(Relaxed) {
                next += frame_time;
                if let Some(d) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(d);
                }
                if !grab.frame(&mut bgra, w, h) {
                    continue;
                }
                n += 1;
                // our own tile, at a third of the frame rate; nobody needs a 30fps mirror
                if n % 3 == 1 {
                    let px = bgra.chunks_exact(4).map(|p| egui::Color32::from_rgb(p[2], p[1], p[0])).collect();
                    preview(Arc::new(ColorImage::new([w, h], px)));
                }
                if k2.swap(false, Relaxed) {
                    enc.force_intra_frame();
                }
                let yuv = YUVBuffer::from_bgra8_source(BgraSliceU8::new(&bgra, (w, h)));
                let Ok(bits) = enc.encode(&yuv) else { continue };
                let data = bits.to_vec();
                if data.is_empty() {
                    continue;
                }
                shared.video_out.fetch_add(1, Relaxed);
                let ts = (t0.elapsed().as_micros() as u64 * 9 / 100) as u32;
                if out.send(Encoded { data, ts }).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let size = ready.recv().map_err(|e| e.to_string())??;
    Ok(Sender { stop, key, size, fps, th: Some(th) })
}

// scale down to fit 720p, keeping even dimensions for 4:2:0
fn fit((w, h): (usize, usize)) -> (usize, usize) {
    let k = (MAX_W as f32 / w as f32).min(MAX_H as f32 / h as f32).min(1.0);
    (((w as f32 * k) as usize) & !1, ((h as f32 * k) as usize) & !1)
}
