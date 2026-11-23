//! gifv embeds (tenor, giphy, klipy, imgur) ship their loop as a short h264 mp4. This
//! reads the sample tables of a plain (non-fragmented) mp4 and feeds the samples to
//! openh264, the same decoder the voice code uses for camera video.

use egui::ColorImage;
use openh264::OpenH264API;
use openh264::decoder::{Decoder, DecoderConfig, Flush};
use openh264::formats::YUVSource;
use std::time::Duration;

struct Sample {
    off: usize,
    size: usize,
    cts: i64,
    dur: i64,
}

struct Track {
    scale: u32,
    nal_len: usize,
    headers: Vec<u8>,
    samples: Vec<Sample>,
}

pub fn is_mp4(b: &[u8]) -> bool {
    b.len() > 12 && &b[4..8] == b"ftyp"
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The payloads of the boxes in `b` as (type, body).
fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(size) = be32(b, at) {
        let Some(kind) = b.get(at + 4..at + 8) else { break };
        let (head, size) = match size {
            1 => (16, be64(b, at + 8).unwrap_or(0) as usize),
            0 => (8, b.len() - at),
            n => (8, n as usize),
        };
        if size < head || at + size > b.len() {
            break;
        }
        out.push((kind.try_into().unwrap(), &b[at + head..at + size]));
        at += size;
    }
    out
}

fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b).into_iter().find(|(k, _)| k == kind).map(|(_, body)| body)
}

/// Run-length tables (stts, ctts): version/flags, entry count, then pairs of u32.
fn runs(b: &[u8]) -> Vec<(u32, i32)> {
    let n = be32(b, 4).unwrap_or(0) as usize;
    (0..n).filter_map(|i| Some((be32(b, 8 + i * 8)?, be32(b, 12 + i * 8)? as i32))).collect()
}

fn track(moov: &[u8]) -> Result<Track, String> {
    let bad = || "unsupported mp4".to_string();
    for (k, trak) in boxes(moov) {
        if &k != b"trak" {
            continue;
        }
        let mdia = child(trak, b"mdia").ok_or_else(bad)?;
        if child(mdia, b"hdlr").and_then(|h| h.get(8..12)) != Some(b"vide") {
            continue;
        }
        let scale = child(mdia, b"mdhd").and_then(|m| be32(m, if m[0] == 1 { 20 } else { 12 })).ok_or_else(bad)?;
        let stbl = child(child(mdia, b"minf").ok_or_else(bad)?, b"stbl").ok_or_else(bad)?;

        let stsd = child(stbl, b"stsd").ok_or_else(bad)?;
        let at = stsd.windows(4).position(|w| w == b"avcC").ok_or("not h264")? + 4;
        let avcc = stsd.get(at..).ok_or_else(bad)?;
        let nal_len = (*avcc.get(4).ok_or_else(bad)? & 3) as usize + 1;
        let mut headers = Vec::new();
        let mut p = 5;
        // sps list, then pps list; the sps count only uses the low 5 bits
        for mask in [0x1f, 0xff] {
            let n = *avcc.get(p).ok_or_else(bad)? & mask;
            p += 1;
            for _ in 0..n {
                let len = u16::from_be_bytes(avcc.get(p..p + 2).ok_or_else(bad)?.try_into().unwrap()) as usize;
                headers.extend([0, 0, 0, 1]);
                headers.extend(avcc.get(p + 2..p + 2 + len).ok_or_else(bad)?);
                p += 2 + len;
            }
        }

        let sizes = child(stbl, b"stsz").ok_or_else(bad)?;
        let count = be32(sizes, 8).ok_or_else(bad)? as usize;
        let fixed = be32(sizes, 4).ok_or_else(bad)? as usize;
        let size_of = |i: usize| if fixed != 0 { Some(fixed) } else { be32(sizes, 12 + i * 4).map(|s| s as usize) };

        let chunks: Vec<usize> = if let Some(co) = child(stbl, b"stco") {
            (0..be32(co, 4).ok_or_else(bad)? as usize).filter_map(|i| be32(co, 8 + i * 4).map(|o| o as usize)).collect()
        } else {
            let co = child(stbl, b"co64").ok_or_else(bad)?;
            (0..be32(co, 4).ok_or_else(bad)? as usize).filter_map(|i| be64(co, 8 + i * 8).map(|o| o as usize)).collect()
        };
        let stsc = child(stbl, b"stsc").ok_or_else(bad)?;
        let map: Vec<(usize, usize)> = (0..be32(stsc, 4).unwrap_or(0) as usize)
            .filter_map(|i| Some((be32(stsc, 8 + i * 12)? as usize, be32(stsc, 12 + i * 12)? as usize)))
            .collect();

        let mut offs = Vec::with_capacity(count);
        for (ci, &base) in chunks.iter().enumerate() {
            let per = map.iter().rev().find(|(first, _)| *first <= ci + 1).map_or(1, |m| m.1);
            let mut o = base;
            for _ in 0..per {
                if offs.len() == count {
                    break;
                }
                let s = size_of(offs.len()).ok_or_else(bad)?;
                offs.push((o, s));
                o += s;
            }
        }

        let deltas: Vec<i64> = runs(child(stbl, b"stts").ok_or_else(bad)?).into_iter().flat_map(|(n, d)| std::iter::repeat_n(d as i64, n as usize)).collect();
        let shifts: Vec<i64> = child(stbl, b"ctts").map(|c| runs(c).into_iter().flat_map(|(n, d)| std::iter::repeat_n(d as i64, n as usize)).collect()).unwrap_or_default();
        let mut dts = 0;
        let samples = offs
            .into_iter()
            .enumerate()
            .map(|(i, (off, size))| {
                let dur = deltas.get(i).copied().unwrap_or(0);
                let s = Sample { off, size, cts: dts + shifts.get(i).copied().unwrap_or(0), dur };
                dts += dur;
                s
            })
            .collect();
        return Ok(Track { scale: scale.max(1), nal_len, headers, samples });
    }
    Err("no video track".into())
}

/// Sample `i` as the decoder takes it: start codes for length prefixes, the headers ahead of the first.
fn sample(t: &Track, data: &[u8], i: usize, annexb: &mut Vec<u8>) -> Result<(), String> {
    let s = &t.samples[i];
    let body = data.get(s.off..s.off + s.size).ok_or("sample past end of file")?;
    annexb.clear();
    if i == 0 {
        annexb.extend(&t.headers);
    }
    let mut at = 0;
    while at + t.nal_len <= body.len() {
        let len = body[at..at + t.nal_len].iter().fold(0usize, |n, b| n << 8 | *b as usize);
        annexb.extend([0, 0, 0, 1]);
        annexb.extend(body.get(at + t.nal_len..at + t.nal_len + len).ok_or("bad nal length")?);
        at += t.nal_len + len;
    }
    Ok(())
}

fn picture(yuv: &openh264::decoder::DecodedYUV) -> ColorImage {
    let (w, h) = yuv.dimensions();
    let mut rgba = vec![0; w * h * 4];
    yuv.write_rgba8(&mut rgba);
    ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

/// A clip played a picture at a time, in display order: one too big to keep decoded is decoded
/// as it plays, and again from the start each time round.
pub struct Clip {
    data: std::sync::Arc<[u8]>,
    t: Track,
    dec: Decoder,
    next: usize,
    // what the decoder still held when the samples ran out
    tail: Option<std::collections::VecDeque<ColorImage>>,
    annexb: Vec<u8>,
}

impl Clip {
    pub fn new(data: std::sync::Arc<[u8]>) -> Result<Self, String> {
        let t = track(child(&data, b"moov").ok_or("no moov box (fragmented mp4?)")?)?;
        if t.samples.is_empty() {
            return Err("empty video track".into());
        }
        let dec = Decoder::with_api_config(OpenH264API::from_source(), DecoderConfig::new().flush_after_decode(Flush::NoFlush)).map_err(|e| e.to_string())?;
        Ok(Self { data, t, dec, next: 0, tail: None, annexb: Vec::new() })
    }

    /// How long each picture stays up, in display order.
    pub fn durations(&self) -> Vec<Duration> {
        let t = &self.t;
        let mut starts: Vec<i64> = t.samples.iter().map(|s| s.cts).collect();
        starts.sort_unstable();
        let last = t.samples.iter().max_by_key(|s| s.cts).map_or(0, |s| s.cts + s.dur);
        starts.push(last);
        starts.windows(2).map(|w| Duration::from_secs_f64((w[1] - w[0]).max(0) as f64 / t.scale as f64)).collect()
    }

    /// The next picture, or just past it when it isn't `keep`t (to get to a later one).
    pub fn next(&mut self, keep: bool) -> Option<Result<Option<ColorImage>, String>> {
        loop {
            if let Some(tail) = &mut self.tail {
                return tail.pop_front().map(|p| Ok(keep.then_some(p)));
            }
            if self.next == self.t.samples.len() {
                self.tail = Some(match self.dec.flush_remaining() {
                    Ok(rest) => rest.iter().map(picture).collect(),
                    Err(e) => return Some(Err(e.to_string())),
                });
                continue;
            }
            if let Err(e) = sample(&self.t, &self.data, self.next, &mut self.annexb) {
                return Some(Err(e));
            }
            self.next += 1;
            match self.dec.decode(&self.annexb) {
                Ok(Some(yuv)) => return Some(Ok(keep.then(|| picture(&yuv)))),
                Ok(None) => {}
                Err(e) => return Some(Err(e.to_string())),
            }
        }
    }
}

/// Decodes the whole clip. Frames come back in display order with how long each stays up.
/// Clips too big for `budget` bytes of pixels are thinned out rather than cut short.
pub fn frames(data: &[u8], budget: usize) -> Result<Vec<(ColorImage, Duration)>, String> {
    frames_within(data, budget, usize::MAX)
}

/// `frames`, with pictures wider than `max_w` shrunk by a whole factor as they come out of the decoder.
pub fn frames_within(data: &[u8], budget: usize, max_w: usize) -> Result<Vec<(ColorImage, Duration)>, String> {
    let moov = child(data, b"moov").ok_or("no moov box (fragmented mp4?)")?;
    let t = track(moov)?;
    if t.samples.is_empty() {
        return Err("empty video track".into());
    }
    let api = OpenH264API::from_source();
    let mut dec = Decoder::with_api_config(api, DecoderConfig::new().flush_after_decode(Flush::NoFlush)).map_err(|e| e.to_string())?;

    // decoded pictures come out in display order, so pair them with the sorted presentation times
    let mut order: Vec<usize> = (0..t.samples.len()).collect();
    order.sort_by_key(|&i| t.samples[i].cts);

    let mut pics: Vec<ColorImage> = Vec::new();
    let mut stride = 1;
    let mut rgba = Vec::new();
    let mut keep = |yuv: openh264::decoder::DecodedYUV, pics: &mut Vec<ColorImage>, stride: &mut usize, seen: usize| {
        let (w, h) = yuv.dimensions();
        let k = w.div_ceil(max_w.max(1)).max(1);
        if *stride == 1 {
            *stride = (t.samples.len() * (w / k) * (h / k) * 4).div_ceil(budget.max(1)).max(1);
        }
        if seen % *stride == 0 {
            rgba.resize(w * h * 4, 0);
            yuv.write_rgba8(&mut rgba);
            if k == 1 {
                pics.push(ColorImage::from_rgba_unmultiplied([w, h], &rgba));
            } else {
                let (sw, sh) = (w / k, h / k);
                let small: Vec<u8> = (0..sh).flat_map(|y| (0..sw).flat_map(move |x| (y * k * w + x * k) * 4..(y * k * w + x * k) * 4 + 4)).map(|i| rgba[i]).collect();
                pics.push(ColorImage::from_rgba_unmultiplied([sw, sh], &small));
            }
        }
    };

    let mut seen = 0;
    let mut annexb = Vec::new();
    for i in 0..t.samples.len() {
        sample(&t, data, i, &mut annexb)?;
        if let Some(yuv) = dec.decode(&annexb).map_err(|e| e.to_string())? {
            keep(yuv, &mut pics, &mut stride, seen);
            seen += 1;
        }
    }
    for yuv in dec.flush_remaining().map_err(|e| e.to_string())? {
        keep(yuv, &mut pics, &mut stride, seen);
        seen += 1;
    }
    if pics.is_empty() {
        return Err("no frames decoded".into());
    }

    // each kept picture stays up until the next kept one starts
    let at = |n: usize| t.samples[order[n.min(order.len() - 1)]].cts;
    let secs = |ticks: i64| Duration::from_secs_f64(ticks.max(0) as f64 / t.scale as f64);
    let end = at(order.len() - 1) + t.samples[order[order.len() - 1]].dur;
    Ok(pics
        .into_iter()
        .enumerate()
        .map(|(k, img)| {
            let from = at(k * stride);
            let to = if (k + 1) * stride < order.len() { at((k + 1) * stride) } else { end };
            (img, secs(to - from))
        })
        .collect())
}
