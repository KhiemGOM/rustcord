//! Video retransmission ring (RTCP NACK / RFC 4588 RTX) and adaptive bandwidth estimation (BWE).

const RTX_RING_CAP: usize = 128;
const MAX_RTX_BURST: usize = 24;
const RTX_WINDOW_MS: u64 = 100;

pub struct RtxEntry {
    pub seq: u16,
    pub ts: u32,
    pub payload: Vec<u8>,
    pub last_sent_ms: u64,
}

#[derive(Default)]
pub struct RtxRing {
    slots: Vec<Option<RtxEntry>>,
    win_start_ms: u64,
    win_sent: usize,
}

impl RtxRing {
    pub fn push(&mut self, seq: u16, ts: u32, payload: Vec<u8>) {
        if self.slots.len() < RTX_RING_CAP {
            self.slots.resize_with(RTX_RING_CAP, || None);
        }
        let idx = (seq as usize) % RTX_RING_CAP;
        self.slots[idx] = Some(RtxEntry { seq, ts, payload, last_sent_ms: 0 });
    }

    pub fn take_for_nack(&mut self, seqs: &[u16], now_ms: u64, rtt_ms: u32) -> Vec<(u16, u32, Vec<u8>)> {
        if now_ms.saturating_sub(self.win_start_ms) >= RTX_WINDOW_MS {
            self.win_start_ms = now_ms;
            self.win_sent = 0;
        }
        let min_gap = (rtt_ms.max(25)) as u64;
        let mut out = Vec::new();
        for &seq in seqs {
            if self.win_sent >= MAX_RTX_BURST {
                break;
            }
            let idx = (seq as usize) % RTX_RING_CAP;
            let Some(Some(entry)) = self.slots.get_mut(idx) else { continue };
            if entry.seq != seq {
                continue;
            }
            if entry.last_sent_ms != 0 && now_ms.saturating_sub(entry.last_sent_ms) < min_gap {
                continue;
            }
            entry.last_sent_ms = now_ms.max(1);
            self.win_sent += 1;
            out.push((entry.seq, entry.ts, entry.payload.clone()));
        }
        out
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.win_start_ms = 0;
        self.win_sent = 0;
    }
}

/// Splits a decrypted compound RTCP datagram into `(fmt, pt, body_after_8b_hdr)` items.
pub fn iter_rtcp_compound<'a>(first_hdr: &[u8], plain: &'a [u8]) -> Vec<(u8, u8, &'a [u8])> {
    let mut out = Vec::new();
    if first_hdr.len() < 8 || first_hdr[0] >> 6 != 2 {
        return out;
    }
    let first_words = u16::from_be_bytes([first_hdr[2], first_hdr[3]]) as usize;
    let first_body_len = first_words.saturating_sub(1) * 4;
    if first_body_len <= plain.len() {
        out.push((first_hdr[0] & 0x1f, first_hdr[1], &plain[..first_body_len]));
    } else {
        out.push((first_hdr[0] & 0x1f, first_hdr[1], plain));
        return out;
    }
    let mut rest = &plain[first_body_len..];
    while rest.len() >= 8 && rest[0] >> 6 == 2 {
        let fmt = rest[0] & 0x1f;
        let pt = rest[1];
        let words = u16::from_be_bytes([rest[2], rest[3]]) as usize;
        let pkt_bytes = (words + 1) * 4;
        if pkt_bytes < 8 || pkt_bytes > rest.len() {
            break;
        }
        out.push((fmt, pt, &rest[8..pkt_bytes]));
        rest = &rest[pkt_bytes..];
    }
    out
}

/// Parses an RTCP REMB payload (PT=206, FMT=15, body after 8-byte common+sender header).
/// Returns `(bitrate_bps, media_ssrcs)`.
pub fn parse_remb(plain: &[u8]) -> Option<(u32, Vec<u32>)> {
    if plain.len() < 12 || &plain[4..8] != b"REMB" {
        return None;
    }
    let count = plain[8] as usize;
    if plain.len() < 12 + count * 4 {
        return None;
    }
    let exp = (plain[9] >> 2) as u32;
    let mantissa = (((plain[9] & 0x03) as u64) << 16) | ((plain[10] as u64) << 8) | (plain[11] as u64);
    let bps = mantissa.checked_shl(exp).unwrap_or(u64::MAX).min(u32::MAX as u64) as u32;
    let mut ssrcs = Vec::with_capacity(count);
    for c in plain[12..12 + count * 4].chunks_exact(4) {
        ssrcs.push(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    Some((bps, ssrcs))
}

/// Parses an RFC 4585 Generic NACK RTCP payload (decrypted body after the 8-byte common+sender header).
/// Returns `(media_ssrc, requested_sequence_numbers)`.
pub fn parse_nack(plain: &[u8]) -> Option<(u32, Vec<u16>)> {
    if plain.len() < 8 {
        return None;
    }
    let media_ssrc = u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]);
    let mut seqs = Vec::new();
    for fci in plain[4..].chunks_exact(4) {
        let pid = u16::from_be_bytes([fci[0], fci[1]]);
        let blp = u16::from_be_bytes([fci[2], fci[3]]);
        seqs.push(pid);
        for bit in 0..16u16 {
            if blp & (1 << bit) != 0 {
                seqs.push(pid.wrapping_add(bit + 1));
            }
        }
    }
    Some((media_ssrc, seqs))
}

/// Builds an RFC 4588 RTX payload: 2-byte Original Sequence Number (OSN) + original RTP payload.
pub fn rtx_payload(orig_seq: u16, orig_payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + orig_payload.len());
    out.extend_from_slice(&orig_seq.to_be_bytes());
    out.extend_from_slice(orig_payload);
    out
}

/// AIMD loss- and delay-based video bandwidth estimator feeding `HwEnc::set_bitrate`.
pub struct Bwe {
    min_bps: u32,
    max_bps: u32,
    cur_bps: u32,
    base_rtt: u32,
}

impl Default for Bwe {
    fn default() -> Self {
        Self::new(400_000, 4_000_000)
    }
}

impl Bwe {
    pub fn new(min_bps: u32, max_bps: u32) -> Self {
        let max_bps = max_bps.max(min_bps);
        Self { min_bps, max_bps, cur_bps: max_bps, base_rtt: 0 }
    }

    pub fn reset(&mut self, max_bps: u32) {
        self.max_bps = max_bps.max(self.min_bps);
        self.cur_bps = self.max_bps;
        self.base_rtt = 0;
    }

    /// Updates estimate from RTCP Receiver Report `fraction_lost` (0..=255) and heartbeat RTT (ms).
    pub fn on_report(&mut self, fraction_lost: u8, rtt_ms: u32) -> u32 {
        if rtt_ms > 0 {
            self.base_rtt = if self.base_rtt == 0 { rtt_ms } else { self.base_rtt.min(rtt_ms) };
        }
        let delay_spike = self.base_rtt > 0 && rtt_ms > self.base_rtt.saturating_add(80);
        if fraction_lost > 25 || (fraction_lost > 10 && delay_spike) {
            // >10% packet loss or congestion delay spike: back off multiplicatively
            let cut = 1.0 - (fraction_lost as f32 / 512.0).clamp(0.12, 0.35);
            self.cur_bps = ((self.cur_bps as f32 * cut) as u32).max(self.min_bps);
        } else if fraction_lost < 5 && !delay_spike {
            // <2% loss and stable RTT: ramp back up smoothly
            let next = self.cur_bps.saturating_add((self.cur_bps / 20).max(50_000));
            self.cur_bps = next.min(self.max_bps);
        }
        self.cur_bps
    }

    pub fn on_remb(&mut self, remb_bps: u32) -> u32 {
        if remb_bps > 0 {
            self.cur_bps = remb_bps.clamp(self.min_bps, self.max_bps);
        }
        self.cur_bps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generic_nack_pid_and_blp_bitmask() {
        let mut plain = Vec::new();
        plain.extend_from_slice(&0x1122_3344u32.to_be_bytes());
        // PID = 100, BLP = bit 0 (101) and bit 3 (104)
        plain.extend_from_slice(&100u16.to_be_bytes());
        plain.extend_from_slice(&0b0000_1001u16.to_be_bytes());
        let (ssrc, seqs) = parse_nack(&plain).unwrap();
        assert_eq!(ssrc, 0x1122_3344);
        assert_eq!(seqs, vec![100, 101, 104]);
    }

    #[test]
    fn rtx_ring_enforces_rtt_spacing_and_burst_cap() {
        let mut ring = RtxRing::default();
        for s in 0..40u16 {
            ring.push(s, 9000 + s as u32, vec![s as u8]);
        }
        let seqs: Vec<u16> = (0..40u16).collect();
        let first = ring.take_for_nack(&seqs, 100, 50);
        assert_eq!(first.len(), MAX_RTX_BURST);
        // Same window hits burst cap
        assert!(ring.take_for_nack(&[30], 110, 50).is_empty());
        // Next window allows unsent seqs, still skips recently sent seq within RTT
        assert!(ring.take_for_nack(&[0], 130, 50).is_empty());
        assert_eq!(ring.take_for_nack(&[0], 210, 50).len(), 1);
        assert_eq!(rtx_payload(0x1234, &[0xaa, 0xbb]), vec![0x12, 0x34, 0xaa, 0xbb]);
    }

    #[test]
    fn parses_compound_rtcp_and_remb_bitrate() {
        // First packet: empty RR (words=1 -> 0 body bytes)
        let hdr = [0x80, 201, 0, 1, 0, 0, 0, 1];
        // Second packet: REMB (PT=206, FMT=15, words=5 -> 24 bytes total: 8 hdr + 16 body)
        let mut plain = vec![0x8f, 206, 0, 5, 0, 0, 0, 1];
        plain.extend_from_slice(&0u32.to_be_bytes()); // unused media ssrc
        plain.extend_from_slice(b"REMB");
        plain.push(1); // 1 SSRC
        // exp = 10 (<<10 = *1024), mantissa = 2000 -> 2_048_000 bps
        let exp = 10u8;
        let m = 2000u32;
        plain.push((exp << 2) | ((m >> 16) as u8 & 0x03));
        plain.push(((m >> 8) & 0xff) as u8);
        plain.push((m & 0xff) as u8);
        plain.extend_from_slice(&0x5566_7788u32.to_be_bytes());

        let pkts = iter_rtcp_compound(&hdr, &plain);
        assert_eq!(pkts.len(), 2);
        assert_eq!(pkts[0].1, 201);
        assert_eq!((pkts[1].0, pkts[1].1), (15, 206));
        let (bps, ssrcs) = parse_remb(pkts[1].2).unwrap();
        assert_eq!(bps, 2_048_000);
        assert_eq!(ssrcs, vec![0x5566_7788]);

        let mut bwe = Bwe::new(500_000, 4_000_000);
        assert_eq!(bwe.on_remb(bps), 2_048_000);
    }

    #[test]
    fn bwe_backs_off_on_loss_and_recovers_when_clean() {
        let mut bwe = Bwe::new(500_000, 4_000_000);
        let dropped = bwe.on_report(40, 30);
        assert!(dropped < 4_000_000);
        let recovered = bwe.on_report(0, 32);
        assert!(recovered > dropped);
    }
}
