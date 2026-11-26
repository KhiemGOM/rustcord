//! Multi-codec video support (H.264, H.265/HEVC, AV1): Media Foundation hardware probing,
//! RFC 7798 HEVC RTP packetization/reassembly, and AV1 RTP v1.0 packetization/reassembly.

use davey::Codec as DaveCodec;
use serde_json::{Value, json};
use std::sync::OnceLock;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
use windows::core::GUID;

use super::rtp;
use super::video::{Assembler as H264Assembler, Got, H264_PT, MTU, RTX_PT};

pub const H265_PT: u8 = 103;
pub const H265_RTX_PT: u8 = 104;
pub const AV1_PT: u8 = 105;
pub const AV1_RTX_PT: u8 = 106;

const MF_SUBTYPE_HEVC: GUID = GUID::from_u128(0x43564548_0000_0010_8000_00aa00389b71);
const MF_SUBTYPE_AV1: GUID = GUID::from_u128(0x31305641_0000_0010_8000_00aa00389b71);

const MAX_FRAME_BYTES: usize = 4 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum VideoCodec {
    #[default]
    H264,
    H265,
    Av1,
}

impl VideoCodec {
    pub fn pt(self) -> u8 {
        match self {
            VideoCodec::H264 => H264_PT,
            VideoCodec::H265 => H265_PT,
            VideoCodec::Av1 => AV1_PT,
        }
    }

    pub fn rtx_pt(self) -> u8 {
        match self {
            VideoCodec::H264 => RTX_PT,
            VideoCodec::H265 => H265_RTX_PT,
            VideoCodec::Av1 => AV1_RTX_PT,
        }
    }

    pub fn from_pt(pt: u8) -> Option<Self> {
        match pt {
            H264_PT => Some(VideoCodec::H264),
            H265_PT => Some(VideoCodec::H265),
            AV1_PT => Some(VideoCodec::Av1),
            _ => None,
        }
    }

    pub fn from_sdp(name: &str) -> Self {
        match name.to_ascii_uppercase().as_str() {
            "H265" | "HEVC" => VideoCodec::H265,
            "AV1" => VideoCodec::Av1,
            _ => VideoCodec::H264,
        }
    }

    pub fn dave_codec(self) -> DaveCodec {
        match self {
            VideoCodec::H264 => DaveCodec::H264,
            VideoCodec::H265 => DaveCodec::H265,
            VideoCodec::Av1 => DaveCodec::AV1,
        }
    }

    pub fn mf_subtype(self) -> GUID {
        match self {
            VideoCodec::H264 => MFVideoFormat_H264,
            VideoCodec::H265 => MF_SUBTYPE_HEVC,
            VideoCodec::Av1 => MF_SUBTYPE_AV1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeResult {
    pub h265_enc: bool,
    pub h265_dec: bool,
    pub av1_enc: bool,
    pub av1_dec: bool,
}

fn has_hw_mft(category: GUID, input: GUID, output: GUID) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, MFSTARTUP_FULL).is_err() {
            return false;
        }
        let inp = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: input };
        let out = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: output };
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut n = 0u32;
        let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
        let ok = MFTEnum2(category, flags, Some(&inp), Some(&out), None, &mut list, &mut n).is_ok() && !list.is_null() && n > 0;
        if !list.is_null() {
            for i in 0..n as usize {
                drop(list.add(i).read());
            }
            CoTaskMemFree(Some(list as *const _));
        }
        ok
    }
}

pub fn probe_hw_codecs() -> ProbeResult {
    if cfg!(test) || crate::winsys::harness() || std::env::var_os("RUSTCORD_NO_CARD_DECODE").is_some() {
        return ProbeResult::default();
    }
    static PROBE: OnceLock<ProbeResult> = OnceLock::new();
    *PROBE.get_or_init(|| ProbeResult {
        h265_enc: has_hw_mft(MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_NV12, MF_SUBTYPE_HEVC),
        h265_dec: has_hw_mft(MFT_CATEGORY_VIDEO_DECODER, MF_SUBTYPE_HEVC, MFVideoFormat_NV12),
        av1_enc: has_hw_mft(MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_NV12, MF_SUBTYPE_AV1),
        av1_dec: has_hw_mft(MFT_CATEGORY_VIDEO_DECODER, MF_SUBTYPE_AV1, MFVideoFormat_NV12),
    })
}

pub fn align_dimensions(codec: VideoCodec, w: usize, h: usize) -> (usize, usize) {
    match codec {
        VideoCodec::Av1 => ((w & !63).max(64), (h & !15).max(16)),
        VideoCodec::H265 => ((w & !7).max(16), (h & !7).max(16)),
        VideoCodec::H264 => ((w & !1).max(2), (h & !1).max(2)),
    }
}

pub fn has_seq_header(codec: VideoCodec, bytes: &[u8]) -> bool {
    match codec {
        VideoCodec::H264 => bytes.windows(4).any(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == 7),
        VideoCodec::H265 => bytes.windows(4).any(|w| w[..3] == [0, 0, 1] && matches!((w[3] >> 1) & 0x3f, 32 | 33)),
        VideoCodec::Av1 => parse_av1_obus(bytes).iter().any(|o| o.obu_type == 1),
    }
}

pub fn negotiated_codecs_for(p: ProbeResult) -> Vec<Value> {
    let mut out = vec![json!({ "name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS_PT })];
    let mut prio = 1000u32;
    if p.av1_enc || p.av1_dec {
        out.push(json!({
            "name": "AV1", "type": "video", "priority": prio,
            "payload_type": AV1_PT, "rtx_payload_type": AV1_RTX_PT,
            "encode": p.av1_enc, "decode": p.av1_dec
        }));
        prio += 1000;
    }
    if p.h265_enc || p.h265_dec {
        out.push(json!({
            "name": "H265", "type": "video", "priority": prio,
            "payload_type": H265_PT, "rtx_payload_type": H265_RTX_PT,
            "encode": p.h265_enc, "decode": p.h265_dec
        }));
        prio += 1000;
    }
    out.push(json!({
        "name": "H264", "type": "video", "priority": prio,
        "payload_type": H264_PT, "rtx_payload_type": RTX_PT,
        "encode": true, "decode": true
    }));
    out
}

pub fn negotiated_codecs() -> Vec<Value> {
    negotiated_codecs_for(probe_hw_codecs())
}

fn annexb_nals(frame: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i + 3 <= frame.len() {
        if frame[i] == 0 && frame[i + 1] == 0 && frame[i + 2] == 1 {
            if let Some(s) = start {
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

/// RFC 7798 HEVC RTP packetizer (single NAL or FU type 49).
pub fn packetize_hevc(frame: &[u8], mut emit: impl FnMut(Vec<u8>, bool)) {
    let units = annexb_nals(frame);
    let n = units.len();
    for (k, nal) in units.into_iter().enumerate() {
        let last = k + 1 == n;
        if nal.len() <= MTU || nal.len() < 2 {
            emit(nal.to_vec(), last);
            continue;
        }
        let (h0, h1) = (nal[0], nal[1]);
        let nal_type = (h0 >> 1) & 0x3f;
        let payload_h0 = (h0 & 0x81) | (49 << 1);
        let body = &nal[2..];
        let chunks: Vec<&[u8]> = body.chunks(MTU - 3).collect();
        let c = chunks.len();
        for (j, chunk) in chunks.into_iter().enumerate() {
            let mut fu_hdr = nal_type;
            if j == 0 {
                fu_hdr |= 0x80;
            }
            if j + 1 == c {
                fu_hdr |= 0x40;
            }
            let mut pkt = Vec::with_capacity(3 + chunk.len());
            pkt.extend_from_slice(&[payload_h0, h1, fu_hdr]);
            pkt.extend_from_slice(chunk);
            emit(pkt, last && j + 1 == c);
        }
    }
}

#[derive(Default)]
pub struct HevcAssembler {
    buf: Vec<u8>,
    ts: Option<u32>,
    next: Option<u16>,
    broken: bool,
    in_fu: bool,
}

impl HevcAssembler {
    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, p: &[u8]) -> Got {
        let mut lost = false;
        if self.ts != Some(ts) {
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
        if p.len() < 2 {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        if self.buf.len().saturating_add(p.len() + 8) > MAX_FRAME_BYTES {
            self.buf.clear();
            self.broken = true;
            return Got::Lost;
        }
        let nal_type = (p[0] >> 1) & 0x3f;
        match nal_type {
            0..=47 => {
                self.buf.extend_from_slice(&[0, 0, 0, 1]);
                self.buf.extend_from_slice(p);
            }
            48 => {
                let mut rest = &p[2..];
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
            49 if p.len() > 3 => {
                let fu = p[2];
                let fu_type = fu & 0x3f;
                if fu & 0x80 != 0 {
                    let orig_h0 = (p[0] & 0x81) | (fu_type << 1);
                    self.buf.extend_from_slice(&[0, 0, 0, 1, orig_h0, p[1]]);
                    self.in_fu = true;
                } else if !self.in_fu {
                    self.broken = true;
                }
                self.buf.extend_from_slice(&p[3..]);
                if fu & 0x40 != 0 {
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

fn read_leb128(data: &[u8]) -> Option<(usize, usize)> {
    let mut val = 0usize;
    let mut shift = 0u32;
    for (i, &b) in data.iter().take(8).enumerate() {
        val |= ((b & 0x7f) as usize) << shift;
        if b & 0x80 == 0 {
            return Some((val, i + 1));
        }
        shift += 7;
    }
    None
}

fn write_leb128(mut val: usize, out: &mut Vec<u8>) {
    loop {
        let mut b = (val & 0x7f) as u8;
        val >>= 7;
        if val != 0 {
            b |= 0x80;
        }
        out.push(b);
        if val == 0 {
            break;
        }
    }
}

struct Av1Obu<'a> {
    obu_type: u8,
    hdr: u8,
    ext: Option<u8>,
    payload: &'a [u8],
}

fn parse_av1_obus(mut data: &[u8]) -> Vec<Av1Obu<'_>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let hdr = data[0];
        if hdr & 0x80 != 0 {
            break;
        }
        let obu_type = (hdr >> 3) & 0x0f;
        let has_ext = hdr & 0x04 != 0;
        let has_size = hdr & 0x02 != 0;
        let mut off = 1usize;
        let ext = if has_ext {
            if data.len() < 2 {
                break;
            }
            off = 2;
            Some(data[1])
        } else {
            None
        };
        let (payload, next) = if has_size {
            let Some((sz, used)) = read_leb128(&data[off..]) else { break };
            let start = off + used;
            if data.len() < start + sz {
                break;
            }
            (&data[start..start + sz], &data[start + sz..])
        } else {
            (&data[off..], &[][..])
        };
        out.push(Av1Obu { obu_type, hdr, ext, payload });
        data = next;
    }
    out
}

fn leb128_len(mut val: usize) -> usize {
    let mut n = 1;
    while val >= 0x80 {
        val >>= 7;
        n += 1;
    }
    n
}

fn emit_obu_with_size(elem: &[u8], out: &mut Vec<u8>) {
    if elem.is_empty() {
        return;
    }
    let hdr = elem[0];
    let has_ext = hdr & 0x04 != 0;
    let hdr_len = if has_ext { 2 } else { 1 };
    if elem.len() < hdr_len {
        return;
    }
    if hdr & 0x02 != 0 {
        out.extend_from_slice(elem);
        return;
    }
    let payload = &elem[hdr_len..];
    out.push(hdr | 0x02);
    if has_ext {
        out.push(elem[1]);
    }
    write_leb128(payload.len(), out);
    out.extend_from_slice(payload);
}

/// Splits an AV1 low-overhead OBU frame into AV1 RTP v1.0 payloads.
/// Strips Temporal Delimiter and Tile List OBUs, clears obu_has_size_field on wire,
/// sets the N bit on packets starting a new coded video sequence (Sequence Header OBU).
pub fn packetize_av1(frame: &[u8], mut emit: impl FnMut(Vec<u8>, bool)) {
    if frame.is_empty() {
        return;
    }
    let parsed = parse_av1_obus(frame);
    let mut elems: Vec<Vec<u8>> = Vec::new();
    let mut has_seq_hdr = false;
    for o in parsed {
        if matches!(o.obu_type, 2 | 8) {
            continue;
        }
        if o.obu_type == 1 {
            has_seq_hdr = true;
        }
        let mut e = Vec::with_capacity(2 + o.payload.len());
        e.push(o.hdr & !0x02);
        if let Some(ext) = o.ext {
            e.push(ext);
        }
        e.extend_from_slice(o.payload);
        elems.push(e);
    }
    if elems.is_empty() {
        return;
    }

    let mut pkts: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<Vec<u8>> = Vec::new();
    let mut cur_bytes = 1usize;

    let flush = |cur: &mut Vec<Vec<u8>>, pkts: &mut Vec<Vec<u8>>, is_first: bool, n_bit: bool| {
        if cur.is_empty() {
            return;
        }
        let count = cur.len();
        let w = if count <= 3 { (count as u8) << 4 } else { 0 };
        let aggr = w | if is_first && n_bit { 0x08 } else { 0 };
        let mut pkt = Vec::with_capacity(MTU);
        pkt.push(aggr);
        for (i, e) in cur.iter().enumerate() {
            if w == 0 || i + 1 < count {
                write_leb128(e.len(), &mut pkt);
            }
            pkt.extend_from_slice(e);
        }
        pkts.push(pkt);
        cur.clear();
    };

    for elem in elems {
        if elem.len() + 1 > MTU {
            let is_first = pkts.is_empty();
            flush(&mut cur, &mut pkts, is_first, has_seq_hdr);
            cur_bytes = 1;
            let cap = MTU.saturating_sub(1);
            let chunks: Vec<&[u8]> = elem.chunks(cap).collect();
            let total = chunks.len();
            for (ci, chunk) in chunks.into_iter().enumerate() {
                let z = if ci > 0 { 0x80 } else { 0 };
                let y = if ci + 1 < total { 0x40 } else { 0 };
                let n = if pkts.is_empty() && has_seq_hdr { 0x08 } else { 0 };
                let mut pkt = Vec::with_capacity(1 + chunk.len());
                pkt.push(z | y | (1 << 4) | n);
                pkt.extend_from_slice(chunk);
                pkts.push(pkt);
            }
            continue;
        }
        let prev_overhead = cur.last().map_or(0, |prev| leb128_len(prev.len()));
        if !cur.is_empty() && (cur.len() >= 3 || cur_bytes + prev_overhead + elem.len() > MTU) {
            let is_first = pkts.is_empty();
            flush(&mut cur, &mut pkts, is_first, has_seq_hdr);
            cur_bytes = 1;
        } else {
            cur_bytes += prev_overhead;
        }
        cur_bytes += elem.len();
        cur.push(elem);
    }
    let is_first = pkts.is_empty();
    flush(&mut cur, &mut pkts, is_first, has_seq_hdr);

    let total = pkts.len();
    for (i, pkt) in pkts.into_iter().enumerate() {
        emit(pkt, i + 1 == total);
    }
}

#[derive(Default)]
pub struct Av1Assembler {
    buf: Vec<u8>,
    cur_obu: Vec<u8>,
    ts: Option<u32>,
    next: Option<u16>,
    broken: bool,
    open_elem: bool,
}

impl Av1Assembler {
    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, p: &[u8]) -> Got {
        let mut lost = false;
        if self.ts != Some(ts) {
            lost = self.ts.is_some() && (!self.buf.is_empty() || !self.cur_obu.is_empty());
            self.buf.clear();
            self.cur_obu.clear();
            self.broken = false;
            self.open_elem = false;
            self.ts = Some(ts);
        }
        if self.next.is_some_and(|n| n != seq) {
            self.broken = true;
        }
        self.next = Some(seq.wrapping_add(1));
        if p.is_empty() {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        if self.buf.len().saturating_add(self.cur_obu.len()).saturating_add(p.len() + 16) > MAX_FRAME_BYTES {
            self.buf.clear();
            self.cur_obu.clear();
            self.broken = true;
            return Got::Lost;
        }
        let aggr = p[0];
        let z = aggr & 0x80 != 0;
        let y = aggr & 0x40 != 0;
        let w = ((aggr >> 4) & 0x03) as usize;
        if z != self.open_elem {
            self.broken = true;
        }
        let mut rest = &p[1..];
        let mut idx = 0usize;
        let mut elems: Vec<&[u8]> = Vec::new();
        while !rest.is_empty() {
            let last_elem = w != 0 && idx + 1 == w;
            let elem = if last_elem {
                let s = rest;
                rest = &[];
                s
            } else {
                let Some((len, used)) = read_leb128(rest) else {
                    self.broken = true;
                    break;
                };
                if rest.len() < used + len {
                    self.broken = true;
                    break;
                }
                let s = &rest[used..used + len];
                rest = &rest[used + len..];
                s
            };
            elems.push(elem);
            idx += 1;
            if w != 0 && idx >= w {
                break;
            }
        }
        if w != 0 && elems.len() != w {
            self.broken = true;
        }
        let n_elems = elems.len();
        for (i, elem) in elems.into_iter().enumerate() {
            let is_first = i == 0;
            let is_last = i + 1 == n_elems;
            if is_first && z {
                self.cur_obu.extend_from_slice(elem);
                if !(is_last && y) {
                    let finished = std::mem::take(&mut self.cur_obu);
                    emit_obu_with_size(&finished, &mut self.buf);
                }
            } else if is_last && y {
                self.cur_obu.clear();
                self.cur_obu.extend_from_slice(elem);
            } else {
                emit_obu_with_size(elem, &mut self.buf);
            }
        }
        self.open_elem = y;
        if !marker {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        if self.open_elem {
            self.broken = true;
        }
        let mut frame = Vec::new();
        if !self.buf.is_empty() {
            frame.extend_from_slice(&[0x12, 0x00]);
            frame.extend_from_slice(&self.buf);
        }
        self.buf.clear();
        self.cur_obu.clear();
        self.ts = None;
        self.open_elem = false;
        if self.broken || lost || frame.is_empty() { Got::Lost } else { Got::Frame(frame) }
    }
}

pub enum VideoAssembler {
    H264(H264Assembler),
    H265(HevcAssembler),
    Av1(Av1Assembler),
}

impl VideoAssembler {
    pub fn new(codec: VideoCodec) -> Self {
        match codec {
            VideoCodec::H264 => VideoAssembler::H264(H264Assembler::default()),
            VideoCodec::H265 => VideoAssembler::H265(HevcAssembler::default()),
            VideoCodec::Av1 => VideoAssembler::Av1(Av1Assembler::default()),
        }
    }

    pub fn codec(&self) -> VideoCodec {
        match self {
            VideoAssembler::H264(_) => VideoCodec::H264,
            VideoAssembler::H265(_) => VideoCodec::H265,
            VideoAssembler::Av1(_) => VideoCodec::Av1,
        }
    }

    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, p: &[u8]) -> Got {
        match self {
            VideoAssembler::H264(a) => a.push(seq, ts, marker, p),
            VideoAssembler::H265(a) => a.push(seq, ts, marker, p),
            VideoAssembler::Av1(a) => a.push(seq, ts, marker, p),
        }
    }
}

pub fn packetize_codec(codec: VideoCodec, frame: &[u8], emit: impl FnMut(Vec<u8>, bool)) {
    match codec {
        VideoCodec::H264 => super::video::packetize(frame, emit),
        VideoCodec::H265 => packetize_hevc(frame, emit),
        VideoCodec::Av1 => packetize_av1(frame, emit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hevc_roundtrips_single_and_fragmented_nals() {
        let mut frame = Vec::new();
        // Small VPS NAL (type 32 -> (32 << 1) = 0x40, tid=1)
        frame.extend_from_slice(&[0, 0, 0, 1, 0x40, 0x01, 0xaa, 0xbb]);
        // Large IDR NAL (type 19 -> (19 << 1) = 0x26, tid=1) larger than MTU
        frame.extend_from_slice(&[0, 0, 0, 1, 0x26, 0x01]);
        frame.extend((0..3000u32).map(|i| (i & 0xff) as u8));

        let mut pkts = Vec::new();
        packetize_hevc(&frame, |p, m| pkts.push((p, m)));
        assert!(pkts.len() >= 4);
        assert!(pkts.last().unwrap().1);

        let mut asm = HevcAssembler::default();
        let mut out = None;
        for (i, (p, m)) in pkts.iter().enumerate() {
            if let Got::Frame(f) = asm.push(i as u16, 9000, *m, p) {
                out = Some(f);
            }
        }
        assert_eq!(out.unwrap(), frame);
    }

    #[test]
    fn av1_roundtrips_obu_keyframe_strips_td_sets_n_bit_and_rejects_bad_zy() {
        let mut frame = Vec::new();
        // OBU_TEMPORAL_DELIMITER (type 2, has_size=1, size=0)
        frame.extend_from_slice(&[0x12, 0x00]);
        // OBU_SEQUENCE_HEADER (type 1, has_size=1, size=4)
        frame.extend_from_slice(&[0x0a, 0x04, 0xde, 0xad, 0xbe, 0xef]);
        // Large OBU_FRAME (type 6, has_size=1, payload > MTU)
        let body: Vec<u8> = (0..2600u32).map(|i| ((i * 31) & 0xff) as u8).collect();
        frame.push((6 << 3) | 0x02);
        write_leb128(body.len(), &mut frame);
        frame.extend_from_slice(&body);

        let mut pkts = Vec::new();
        packetize_av1(&frame, |p, m| pkts.push((p, m)));
        assert!(pkts.len() >= 3);
        // N bit (0x08) must be set on the first packet of a keyframe and cleared after
        assert_ne!(pkts[0].0[0] & 0x08, 0);
        assert_eq!(pkts[1].0[0] & 0x08, 0);
        assert!(pkts.last().unwrap().1);

        let mut asm = Av1Assembler::default();
        let mut out = None;
        for (i, (p, m)) in pkts.iter().enumerate() {
            if let Got::Frame(f) = asm.push(i as u16, 1234, *m, p) {
                out = Some(f);
            }
        }
        let rebuilt = out.unwrap();
        assert_eq!(rebuilt, frame);
        assert!(has_seq_header(VideoCodec::Av1, &rebuilt));

        // Z bit mismatch with previous packet's Y bit must fail
        let mut bad = Av1Assembler::default();
        assert!(matches!(bad.push(0, 99, true, &[0x90, 0x08, 1, 2]), Got::Lost));
        // Frame ending with open fragment (Y=1 on marker) must fail
        let mut frag = Av1Assembler::default();
        assert!(matches!(frag.push(0, 100, true, &[0x50, 0x08, 1, 2]), Got::Lost));
    }

    #[test]
    fn codec_alignment_and_priority_match_hardware_rules() {
        assert_eq!(align_dimensions(VideoCodec::Av1, 1920, 1080), (1920, 1072));
        assert_eq!(align_dimensions(VideoCodec::Av1, 1280, 720), (1280, 720));
        assert_eq!(align_dimensions(VideoCodec::H265, 1920, 1080), (1920, 1080));
        assert_eq!(align_dimensions(VideoCodec::H264, 1921, 1081), (1920, 1080));

        let all = negotiated_codecs_for(ProbeResult { h265_enc: true, h265_dec: true, av1_enc: true, av1_dec: true });
        let vids: Vec<(&str, u64)> = all.iter().filter(|c| c["type"] == "video").map(|c| (c["name"].as_str().unwrap(), c["priority"].as_u64().unwrap())).collect();
        assert_eq!(vids, vec![("AV1", 1000), ("H265", 2000), ("H264", 3000)]);

        let h264_only = negotiated_codecs_for(ProbeResult::default());
        let vids2: Vec<(&str, u64)> = h264_only.iter().filter(|c| c["type"] == "video").map(|c| (c["name"].as_str().unwrap(), c["priority"].as_u64().unwrap())).collect();
        assert_eq!(vids2, vec![("H264", 1000)]);

        assert!(has_seq_header(VideoCodec::H265, &[0, 0, 0, 1, 0x40, 0x01, 0xaa]));
        assert!(!has_seq_header(VideoCodec::H265, &[0, 0, 0, 1, 0x26, 0x01, 0xaa]));
    }

    #[test]
    fn codec_lookup_maps_payload_types_and_sdp_names() {
        assert_eq!(VideoCodec::from_pt(101), Some(VideoCodec::H264));
        assert_eq!(VideoCodec::from_pt(103), Some(VideoCodec::H265));
        assert_eq!(VideoCodec::from_pt(105), Some(VideoCodec::Av1));
        assert_eq!(VideoCodec::from_sdp("H265"), VideoCodec::H265);
        assert_eq!(VideoCodec::from_sdp("AV1"), VideoCodec::Av1);
        assert_eq!(VideoCodec::from_sdp("H264"), VideoCodec::H264);
    }
}
