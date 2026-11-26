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

pub fn negotiated_codecs() -> Vec<Value> {
    let p = probe_hw_codecs();
    let mut out = vec![
        json!({ "name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS_PT }),
        json!({ "name": "H264", "type": "video", "priority": 1000, "payload_type": H264_PT, "rtx_payload_type": RTX_PT, "encode": true, "decode": true }),
    ];
    if p.h265_enc || p.h265_dec {
        out.push(json!({
            "name": "H265", "type": "video", "priority": 2000,
            "payload_type": H265_PT, "rtx_payload_type": H265_RTX_PT,
            "encode": p.h265_enc, "decode": p.h265_dec
        }));
    }
    if p.av1_enc || p.av1_dec {
        out.push(json!({
            "name": "AV1", "type": "video", "priority": 3000,
            "payload_type": AV1_PT, "rtx_payload_type": AV1_RTX_PT,
            "encode": p.av1_enc, "decode": p.av1_dec
        }));
    }
    out
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

/// Splits an AV1 bitstream (low-overhead OBU stream) into RTP payloads per AV1 RTP spec v1.0.
pub fn packetize_av1(frame: &[u8], mut emit: impl FnMut(Vec<u8>, bool)) {
    if frame.is_empty() {
        return;
    }
    let chunk_cap = MTU.saturating_sub(1);
    let chunks: Vec<&[u8]> = frame.chunks(chunk_cap).collect();
    let n = chunks.len();
    for (i, chunk) in chunks.into_iter().enumerate() {
        let z = if i > 0 { 0x80 } else { 0 };
        let y = if i + 1 < n { 0x40 } else { 0 };
        // W = 1 (one OBU element spanning this packet, no trailing length prefix needed)
        let aggr = z | y | (1 << 4);
        let mut pkt = Vec::with_capacity(1 + chunk.len());
        pkt.push(aggr);
        pkt.extend_from_slice(chunk);
        emit(pkt, i + 1 == n);
    }
}

#[derive(Default)]
pub struct Av1Assembler {
    buf: Vec<u8>,
    ts: Option<u32>,
    next: Option<u16>,
    broken: bool,
    open_elem: bool,
}

impl Av1Assembler {
    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, p: &[u8]) -> Got {
        let mut lost = false;
        if self.ts != Some(ts) {
            lost = self.ts.is_some() && !self.buf.is_empty();
            self.buf.clear();
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
        if self.buf.len().saturating_add(p.len() + 8) > MAX_FRAME_BYTES {
            self.buf.clear();
            self.broken = true;
            return Got::Lost;
        }
        let aggr = p[0];
        let z = aggr & 0x80 != 0;
        let y = aggr & 0x40 != 0;
        let w = ((aggr >> 4) & 0x03) as usize;
        if z && !self.open_elem {
            self.broken = true;
        }
        let mut rest = &p[1..];
        let mut idx = 0usize;
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
            self.buf.extend_from_slice(elem);
            idx += 1;
            if w != 0 && idx >= w {
                break;
            }
        }
        self.open_elem = y;
        if !marker {
            return if lost { Got::Lost } else { Got::Nothing };
        }
        let frame = std::mem::take(&mut self.buf);
        self.ts = None;
        self.open_elem = false;
        if self.broken || lost { Got::Lost } else { Got::Frame(frame) }
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
    fn av1_roundtrips_fragmented_and_multi_obu_packets() {
        let frame: Vec<u8> = (0..3100u32).map(|i| ((i * 31) & 0xff) as u8).collect();
        let mut pkts = Vec::new();
        packetize_av1(&frame, |p, m| pkts.push((p, m)));
        assert!(pkts.len() >= 3);
        let mut asm = Av1Assembler::default();
        let mut out = None;
        for (i, (p, m)) in pkts.iter().enumerate() {
            if let Got::Frame(f) = asm.push(i as u16, 1234, *m, p) {
                out = Some(f);
            }
        }
        assert_eq!(out.unwrap(), frame);

        // Also test W=0 multi-OBU packet with LEB128 lengths
        let mut multi = vec![0x00u8]; // Z=0, Y=0, W=0
        write_leb128(3, &mut multi);
        multi.extend_from_slice(&[1, 2, 3]);
        write_leb128(2, &mut multi);
        multi.extend_from_slice(&[4, 5]);
        let mut asm2 = Av1Assembler::default();
        match asm2.push(10, 5678, true, &multi) {
            Got::Frame(f) => assert_eq!(f, vec![1, 2, 3, 4, 5]),
            _ => panic!("expected assembled frame"),
        }
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
