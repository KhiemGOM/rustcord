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

const MAX_SAMPLES: usize = 10_000;
const MAX_FRAME_PIXELS: usize = 4096 * 4096;

pub fn is_mp4(b: &[u8]) -> bool {
    b.len() > 12 && &b[4..8] == b"ftyp"
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_be_bytes(b.get(at..end)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    let end = at.checked_add(8)?;
    Some(u64::from_be_bytes(b.get(at..end)?.try_into().ok()?))
}

/// The payloads of the boxes in `b` as (type, body).
fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(size) = be32(b, at) {
        let Some(kind) = at.checked_add(8).and_then(|e| b.get(at + 4..e)) else { break };
        let (head, size) = match size {
            1 => {
                let Some(n) = at.checked_add(8).and_then(|o| be64(b, o)).and_then(|v| usize::try_from(v).ok()) else { break };
                (16usize, n)
            }
            0 => (8usize, b.len().saturating_sub(at)),
            n => (8usize, n as usize),
        };
        let Some(end) = at.checked_add(size) else { break };
        let Some(start) = at.checked_add(head) else { break };
        if size < head || end > b.len() {
            break;
        }
        out.push((kind.try_into().unwrap(), &b[start..end]));
        at = end;
    }
    out
}

fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b).into_iter().find(|(k, _)| k == kind).map(|(_, body)| body)
}

/// Run-length tables (stts, ctts): version/flags, entry count, then pairs of u32.
fn runs(b: &[u8]) -> Vec<(u32, i32)> {
    let avail = b.len().saturating_sub(8) / 8;
    let n = (be32(b, 4).unwrap_or(0) as usize).min(avail);
    (0..n).filter_map(|i| Some((be32(b, 8 + i * 8)?, be32(b, 12 + i * 8)? as i32))).collect()
}

fn expand_runs(b: &[u8], limit: usize) -> Vec<i64> {
    let mut out = Vec::with_capacity(limit.min(1024));
    for (n, d) in runs(b) {
        let take = (n as usize).min(limit.saturating_sub(out.len()));
        out.extend(std::iter::repeat_n(d as i64, take));
        if out.len() >= limit {
            break;
        }
    }
    out
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
        let scale = child(mdia, b"mdhd").and_then(|m| be32(m, if m.first() == Some(&1) { 20 } else { 12 })).ok_or_else(bad)?;
        let stbl = child(child(mdia, b"minf").ok_or_else(bad)?, b"stbl").ok_or_else(bad)?;

        let stsd = child(stbl, b"stsd").ok_or_else(bad)?;
        let at = stsd.windows(4).position(|w| w == b"avcC").ok_or("not h264")? + 4;
        let avcc = stsd.get(at..).ok_or_else(bad)?;
        let nal_len = (*avcc.get(4).ok_or_else(bad)? & 3) as usize + 1;
        let mut headers = Vec::new();
        let mut p = 5usize;
        // sps list, then pps list; the sps count only uses the low 5 bits
        for mask in [0x1f, 0xff] {
            let n = *avcc.get(p).ok_or_else(bad)? & mask;
            p = p.checked_add(1).ok_or_else(bad)?;
            for _ in 0..n {
                let len_end = p.checked_add(2).ok_or_else(bad)?;
                let len = u16::from_be_bytes(avcc.get(p..len_end).ok_or_else(bad)?.try_into().unwrap()) as usize;
                let end = len_end.checked_add(len).ok_or_else(bad)?;
                headers.extend([0, 0, 0, 1]);
                headers.extend(avcc.get(len_end..end).ok_or_else(bad)?);
                p = end;
            }
        }

        let sizes = child(stbl, b"stsz").ok_or_else(bad)?;
        let count = be32(sizes, 8).ok_or_else(bad)? as usize;
        if count > MAX_SAMPLES {
            return Err(bad());
        }
        let fixed = be32(sizes, 4).ok_or_else(bad)? as usize;
        if fixed == 0 && count > sizes.len().saturating_sub(12) / 4 {
            return Err(bad());
        }
        let size_of = |i: usize| if fixed != 0 { Some(fixed) } else { be32(sizes, 12 + i * 4).map(|s| s as usize) };

        let chunks: Vec<usize> = if let Some(co) = child(stbl, b"stco") {
            let n = (be32(co, 4).ok_or_else(bad)? as usize).min(co.len().saturating_sub(8) / 4);
            (0..n).filter_map(|i| be32(co, 8 + i * 4).map(|o| o as usize)).collect()
        } else {
            let co = child(stbl, b"co64").ok_or_else(bad)?;
            let n = (be32(co, 4).ok_or_else(bad)? as usize).min(co.len().saturating_sub(8) / 8);
            (0..n).filter_map(|i| be64(co, 8 + i * 8).and_then(|o| usize::try_from(o).ok())).collect()
        };
        let stsc = child(stbl, b"stsc").ok_or_else(bad)?;
        let stsc_n = (be32(stsc, 4).unwrap_or(0) as usize).min(stsc.len().saturating_sub(8) / 12);
        let map: Vec<(usize, usize)> = (0..stsc_n)
            .filter_map(|i| Some((be32(stsc, 8 + i * 12)? as usize, be32(stsc, 12 + i * 12)? as usize)))
            .collect();

        let mut offs = Vec::with_capacity(count);
        for (ci, &base) in chunks.iter().enumerate() {
            if offs.len() == count {
                break;
            }
            let per = map.iter().rev().find(|(first, _)| *first <= ci + 1).map_or(1, |m| m.1).min(count - offs.len());
            let mut o = base;
            for _ in 0..per {
                let s = size_of(offs.len()).ok_or_else(bad)?;
                offs.push((o, s));
                o = o.checked_add(s).ok_or_else(bad)?;
            }
        }

        let deltas = expand_runs(child(stbl, b"stts").ok_or_else(bad)?, offs.len());
        let shifts = child(stbl, b"ctts").map(|c| expand_runs(c, offs.len())).unwrap_or_default();
        let mut dts = 0i64;
        let samples = offs
            .into_iter()
            .enumerate()
            .map(|(i, (off, size))| {
                let dur = deltas.get(i).copied().unwrap_or(0);
                let s = Sample { off, size, cts: dts.saturating_add(shifts.get(i).copied().unwrap_or(0)), dur };
                dts = dts.saturating_add(dur);
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
    let end = s.off.checked_add(s.size).ok_or("sample past end of file")?;
    let body = data.get(s.off..end).ok_or("sample past end of file")?;
    annexb.clear();
    if i == 0 {
        annexb.extend(&t.headers);
    }
    let mut at = 0usize;
    while let Some(hdr_end) = at.checked_add(t.nal_len).filter(|e| *e <= body.len()) {
        let len = body[at..hdr_end].iter().fold(0usize, |n, b| n << 8 | *b as usize);
        let nal_end = hdr_end.checked_add(len).ok_or("bad nal length")?;
        annexb.extend([0, 0, 0, 1]);
        annexb.extend(body.get(hdr_end..nal_end).ok_or("bad nal length")?);
        at = nal_end;
    }
    Ok(())
}

fn picture(yuv: &openh264::decoder::DecodedYUV) -> Result<ColorImage, String> {
    let (w, h) = yuv.dimensions();
    let px = w.checked_mul(h).filter(|&n| n > 0 && n <= MAX_FRAME_PIXELS).ok_or("frame too large")?;
    let mut rgba = vec![0; px * 4];
    yuv.write_rgba8(&mut rgba);
    Ok(ColorImage::from_rgba_unmultiplied([w, h], &rgba))
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
                    Ok(rest) => match rest.iter().map(picture).collect::<Result<_, _>>() {
                        Ok(q) => q,
                        Err(e) => return Some(Err(e)),
                    },
                    Err(e) => return Some(Err(e.to_string())),
                });
                continue;
            }
            if let Err(e) = sample(&self.t, &self.data, self.next, &mut self.annexb) {
                return Some(Err(e));
            }
            self.next += 1;
            match self.dec.decode(&self.annexb) {
                Ok(Some(yuv)) => return Some(if keep { picture(&yuv).map(Some) } else { Ok(None) }),
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
    let mut keep = |yuv: openh264::decoder::DecodedYUV, pics: &mut Vec<ColorImage>, stride: &mut usize, seen: usize| -> Result<(), String> {
        let (w, h) = yuv.dimensions();
        let px = w.checked_mul(h).filter(|&n| n > 0 && n <= MAX_FRAME_PIXELS).ok_or("frame too large")?;
        let k = w.div_ceil(max_w.max(1)).max(1);
        if *stride == 1 {
            let sw = (w / k).max(1);
            let sh = (h / k).max(1);
            let total = t.samples.len().saturating_mul(sw).saturating_mul(sh).saturating_mul(4);
            *stride = total.div_ceil(budget.max(1)).max(1);
        }
        if seen % *stride == 0 {
            rgba.resize(px * 4, 0);
            yuv.write_rgba8(&mut rgba);
            if k == 1 {
                pics.push(ColorImage::from_rgba_unmultiplied([w, h], &rgba));
            } else {
                let (sw, sh) = (w / k, h / k);
                let small: Vec<u8> = (0..sh).flat_map(|y| (0..sw).flat_map(move |x| (y * k * w + x * k) * 4..(y * k * w + x * k) * 4 + 4)).map(|i| rgba[i]).collect();
                pics.push(ColorImage::from_rgba_unmultiplied([sw, sh], &small));
            }
        }
        Ok(())
    };

    let mut seen = 0;
    let mut annexb = Vec::new();
    for i in 0..t.samples.len() {
        sample(&t, data, i, &mut annexb)?;
        if let Some(yuv) = dec.decode(&annexb).map_err(|e| e.to_string())? {
            keep(yuv, &mut pics, &mut stride, seen)?;
            seen += 1;
        }
    }
    for yuv in dec.flush_remaining().map_err(|e| e.to_string())? {
        keep(yuv, &mut pics, &mut stride, seen)?;
        seen += 1;
    }
    if pics.is_empty() {
        return Err("no frames decoded".into());
    }

    // each kept picture stays up until the next kept one starts
    let at = |n: usize| t.samples[order[n.min(order.len() - 1)]].cts;
    let secs = |ticks: i64| Duration::from_secs_f64(ticks.max(0) as f64 / t.scale as f64);
    let end = at(order.len() - 1).saturating_add(t.samples[order[order.len() - 1]].dur);
    Ok(pics
        .into_iter()
        .enumerate()
        .map(|(k, img)| {
            let from = at(k * stride);
            let to = if (k + 1) * stride < order.len() { at((k + 1) * stride) } else { end };
            (img, secs(to.saturating_sub(from)))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkbox(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn malformed_boxes_and_sample_tables_return_err() {
        // 64-bit box size = u64::MAX must not overflow
        let mut huge = Vec::new();
        huge.extend_from_slice(&1u32.to_be_bytes());
        huge.extend_from_slice(b"moov");
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(boxes(&huge).is_empty());

        // empty mdhd must not panic
        let hdlr = mkbox(b"hdlr", &[0, 0, 0, 0, 0, 0, 0, 0, b'v', b'i', b'd', b'e']);
        let mdhd = mkbox(b"mdhd", &[]);
        let mdia = mkbox(b"mdia", &[hdlr, mdhd].concat());
        let trak = mkbox(b"trak", &mdia);
        assert!(track(&trak).is_err());

        // u32::MAX run-length count must be capped cleanly
        let mut stts = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stts.extend_from_slice(&u32::MAX.to_be_bytes());
        stts.extend_from_slice(&1000u32.to_be_bytes());
        assert_eq!(expand_runs(&stts, 16).len(), 16);
    }
}
