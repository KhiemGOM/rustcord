//! Inline playback of ogg/opus clips: voice messages and soundboard sounds. Decodes off the
//! ui thread, then plays a prebuilt pcm buffer through one cpal stream, the way sound.rs does.
//! Only one clip plays at a time; starting another replaces it.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Current {
    key: String,
    pos: Arc<AtomicUsize>,
    len: usize,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

static CUR: Mutex<Option<Current>> = Mutex::new(None);

/// If `key` is the clip playing, pause/resume it and return false. Otherwise stop whatever's
/// playing and return true — the caller should fetch the bytes and hand them to `play_bytes`.
pub fn toggle(key: &str) -> bool {
    let mut cur = CUR.lock().unwrap();
    if let Some(c) = cur.as_ref().filter(|c| c.key == key && !c.stop.load(Ordering::Relaxed)) {
        let p = !c.paused.load(Ordering::Relaxed);
        c.paused.store(p, Ordering::Relaxed);
        return false;
    }
    if let Some(c) = cur.take() {
        c.stop.store(true, Ordering::Relaxed);
    }
    true
}

/// Decode and play the fetched bytes for `key`.
pub fn play_bytes(key: String, bytes: Vec<u8>) {
    spawn(key, bytes);
}

/// 0.0..=1.0 progress for `key`, or None if it isn't the clip playing.
pub fn progress(key: &str) -> Option<f32> {
    let cur = CUR.lock().unwrap();
    let c = cur.as_ref().filter(|c| c.key == key && !c.stop.load(Ordering::Relaxed))?;
    Some((c.pos.load(Ordering::Relaxed) as f32 / c.len.max(1) as f32).clamp(0.0, 1.0))
}

pub fn is_playing(key: &str) -> bool {
    let cur = CUR.lock().unwrap();
    cur.as_ref().is_some_and(|c| c.key == key && !c.stop.load(Ordering::Relaxed) && !c.paused.load(Ordering::Relaxed))
}

fn spawn(key: String, bytes: Vec<u8>) {
    // stay quiet under the e2e harness, like the notification sounds do
    if std::env::var_os("FASTCORD_SILENT").is_some() {
        return;
    }
    std::thread::spawn(move || {
        let Some((pcm48, channels)) = decode(&bytes) else { return };
        let Some(dev) = cpal::default_host().default_output_device() else { return };
        let Ok(cfg) = dev.default_output_config() else { return };
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            return;
        }
        let conf = cfg.config();
        let out_ch = conf.channels as usize;
        let out_rate = conf.sample_rate as usize;
        let pcm = Arc::new(resample(&pcm48, channels, 48_000, out_rate, out_ch));
        let len = pcm.len();
        let pos = Arc::new(AtomicUsize::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut cur = CUR.lock().unwrap();
            *cur = Some(Current { key: key.clone(), pos: pos.clone(), len, paused: paused.clone(), stop: stop.clone() });
        }
        let (p, at, pause) = (pcm.clone(), pos.clone(), paused.clone());
        let stream = dev.build_output_stream::<f32, _, _>(
            conf,
            move |data, _| {
                if pause.load(Ordering::Relaxed) {
                    data.fill(0.0);
                    return;
                }
                for s in data.iter_mut() {
                    let i = at.fetch_add(1, Ordering::Relaxed);
                    *s = p.get(i).copied().unwrap_or(0.0);
                }
            },
            |_| {},
            None,
        );
        let Ok(stream) = stream else { return };
        if stream.play().is_err() {
            return;
        }
        // hold the stream until the clip ends or something replaces it
        while !stop.load(Ordering::Relaxed) && pos.load(Ordering::Relaxed) < len {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let mut cur = CUR.lock().unwrap();
        if cur.as_ref().is_some_and(|c| c.key == key) {
            *cur = None;
        }
    });
}

/// ogg/opus -> interleaved 48 kHz f32, with its channel count.
fn decode(bytes: &[u8]) -> Option<(Vec<f32>, usize)> {
    let mut rd = ogg::PacketReader::new(Cursor::new(bytes));
    let mut dec: Option<opus_rs::OpusDecoder> = None;
    let mut channels = 1usize;
    let mut pre_skip = 0usize;
    let mut out: Vec<f32> = Vec::new();
    let mut buf = vec![0f32; 5760 * 2];
    while let Ok(Some(pkt)) = rd.read_packet() {
        let d = &pkt.data;
        if d.starts_with(b"OpusHead") {
            channels = (*d.get(9).unwrap_or(&1)).max(1) as usize;
            pre_skip = u16::from_le_bytes([*d.get(10)?, *d.get(11)?]) as usize;
            dec = opus_rs::OpusDecoder::new(48_000, channels).ok();
            continue;
        }
        if d.starts_with(b"OpusTags") {
            continue;
        }
        let Some(dc) = dec.as_mut() else { continue };
        if let Ok(n) = dc.decode(d, 5760, &mut buf) {
            out.extend_from_slice(&buf[..n * channels]);
        }
    }
    if out.is_empty() {
        return None;
    }
    let skip = (pre_skip * channels).min(out.len());
    Some((out[skip..].to_vec(), channels))
}

/// Linear resample + channel map from the decoded clip to the device's format.
fn resample(src: &[f32], in_ch: usize, in_rate: usize, out_rate: usize, out_ch: usize) -> Vec<f32> {
    let frames = src.len() / in_ch;
    if frames == 0 {
        return Vec::new();
    }
    let out_frames = frames * out_rate / in_rate;
    let mut out = Vec::with_capacity(out_frames * out_ch);
    for f in 0..out_frames {
        let sp = f as f64 * in_rate as f64 / out_rate as f64;
        let i = sp as usize;
        let t = (sp - i as f64) as f32;
        for oc in 0..out_ch {
            let ic = oc.min(in_ch - 1);
            let a = src.get(i * in_ch + ic).copied().unwrap_or(0.0);
            let b = src.get((i + 1) * in_ch + ic).copied().unwrap_or(a);
            out.push(a + (b - a) * t);
        }
    }
    out
}
