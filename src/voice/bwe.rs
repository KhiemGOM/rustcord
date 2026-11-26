//! Video retransmission ring (RTCP NACK / RFC 4588 RTX) and adaptive bandwidth estimation (BWE).

const RTX_RING_CAP: usize = 128;

pub struct RtxEntry {
    pub seq: u16,
    pub ts: u32,
    pub payload: Vec<u8>,
}

#[derive(Default)]
pub struct RtxRing {
    slots: Vec<Option<RtxEntry>>,
}

impl RtxRing {
    pub fn push(&mut self, seq: u16, ts: u32, payload: Vec<u8>) {
        if self.slots.len() < RTX_RING_CAP {
            self.slots.resize_with(RTX_RING_CAP, || None);
        }
        let idx = (seq as usize) % RTX_RING_CAP;
        self.slots[idx] = Some(RtxEntry { seq, ts, payload });
    }

    pub fn get(&self, seq: u16) -> Option<&RtxEntry> {
        let idx = (seq as usize) % RTX_RING_CAP;
        self.slots.get(idx)?.as_ref().filter(|e| e.seq == seq)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
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
    fn rtx_ring_stores_looks_up_and_wraps_payloads() {
        let mut ring = RtxRing::default();
        ring.push(5, 9000, vec![1, 2, 3]);
        assert_eq!(ring.get(5).unwrap().payload, vec![1, 2, 3]);
        assert!(ring.get(6).is_none());
        let wrapped = 5u16.wrapping_add(RTX_RING_CAP as u16);
        ring.push(wrapped, 9999, vec![9]);
        assert!(ring.get(5).is_none());
        assert_eq!(ring.get(wrapped).unwrap().payload, vec![9]);
        assert_eq!(rtx_payload(0x1234, &[0xaa, 0xbb]), vec![0x12, 0x34, 0xaa, 0xbb]);
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
