//! Adaptive audio jitter buffer with out-of-order sequence reordering and PLC signaling.

use std::collections::BTreeMap;

use super::audio::FRAME;

pub enum PacketOut {
    Frame(Vec<u8>),
    Plc,
}

const MAX_BUF: usize = 16;

pub struct JitterBuf {
    next_seq: Option<i64>,
    last_ts: u32,
    buf: BTreeMap<i64, (u32, Vec<u8>)>,
    last_arrival_ms: Option<u64>,
    last_pkt_ts: u32,
    jitter_ms: f32,
}

impl Default for JitterBuf {
    fn default() -> Self {
        Self {
            next_seq: None,
            last_ts: 0,
            buf: BTreeMap::new(),
            last_arrival_ms: None,
            last_pkt_ts: 0,
            jitter_ms: 0.0,
        }
    }
}

impl JitterBuf {
    fn unwrap_seq(base: i64, seq: u16) -> i64 {
        let base_u16 = (base & 0xffff) as u16;
        let diff = seq.wrapping_sub(base_u16) as i16 as i64;
        base + diff
    }

    fn update_jitter(&mut self, ts: u32, now_ms: u64) {
        if let Some(prev_ms) = self.last_arrival_ms {
            let arrival_diff = now_ms.saturating_sub(prev_ms) as f32;
            let media_diff = ts.wrapping_sub(self.last_pkt_ts) as f32 / 48.0;
            let d = (arrival_diff - media_diff).abs().min(200.0);
            self.jitter_ms += (d - self.jitter_ms) / 16.0;
        }
        self.last_arrival_ms = Some(now_ms);
        self.last_pkt_ts = ts;
    }

    /// Adaptive reorder tolerance in 20 ms frames: 1 on clean networks, up to 4 under jitter.
    pub fn target_depth(&self) -> usize {
        if self.jitter_ms < 10.0 {
            1
        } else if self.jitter_ms < 25.0 {
            2
        } else if self.jitter_ms < 50.0 {
            3
        } else {
            4
        }
    }

    pub fn push(&mut self, seq: u16, ts: u32, now_ms: u64, payload: Vec<u8>) -> Vec<PacketOut> {
        self.update_jitter(ts, now_ms);
        let mut out = Vec::new();

        let Some(want) = self.next_seq else {
            self.next_seq = Some(seq as i64 + 1);
            self.last_ts = ts;
            out.push(PacketOut::Frame(payload));
            return out;
        };

        let ext = Self::unwrap_seq(want, seq);
        if ext < want {
            return out;
        }

        // Long silence / new talk spurt: flush stale buffer and resync immediately.
        if ts.wrapping_sub(self.last_ts) > FRAME as u32 * 8 || ext - want > MAX_BUF as i64 {
            self.buf.clear();
            self.next_seq = Some(ext + 1);
            self.last_ts = ts;
            out.push(PacketOut::Frame(payload));
            return out;
        }

        if ext == want {
            self.next_seq = Some(want + 1);
            self.last_ts = ts;
            out.push(PacketOut::Frame(payload));
            self.drain_ready(&mut out);
            return out;
        }

        // Future packet: hold for reordering while gap is within adaptive target depth.
        self.buf.insert(ext, (ts, payload));
        let depth = self.target_depth() as i64;
        while let Some(&first_seq) = self.buf.keys().next() {
            let cur = self.next_seq.unwrap_or(first_seq);
            let max_seq = *self.buf.keys().next_back().unwrap_or(&first_seq);
            if max_seq - cur <= depth && self.buf.len() <= MAX_BUF {
                break;
            }
            if cur < first_seq {
                let gap = (first_seq - cur).min(3);
                for _ in 0..gap {
                    out.push(PacketOut::Plc);
                }
                self.next_seq = Some(first_seq);
            }
            self.drain_ready(&mut out);
        }
        out
    }

    fn drain_ready(&mut self, out: &mut Vec<PacketOut>) {
        while let Some(want) = self.next_seq {
            let Some((ts, data)) = self.buf.remove(&want) else { break };
            self.next_seq = Some(want + 1);
            self.last_ts = ts;
            out.push(PacketOut::Frame(data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames_and_plcs(v: Vec<PacketOut>) -> Vec<Option<u8>> {
        v.into_iter()
            .map(|p| match p {
                PacketOut::Frame(b) => Some(b[0]),
                PacketOut::Plc => None,
            })
            .collect()
    }

    #[test]
    fn reorders_swapped_packets_without_plc() {
        let mut jb = JitterBuf::default();
        assert_eq!(frames_and_plcs(jb.push(10, 0, 0, vec![1])), vec![Some(1)]);
        // Packet 12 arrives before 11: buffered, no PLC yet
        assert_eq!(frames_and_plcs(jb.push(12, 1920, 20, vec![3])), vec![]);
        // Packet 11 arrives: emits 11 then 12 in order
        assert_eq!(frames_and_plcs(jb.push(11, 960, 25, vec![2])), vec![Some(2), Some(3)]);
    }

    #[test]
    fn emits_plc_when_missing_packet_exceeds_window() {
        let mut jb = JitterBuf::default();
        assert_eq!(frames_and_plcs(jb.push(1, 0, 0, vec![1])), vec![Some(1)]);
        // 2 is lost; 3 arrives, then 4 arrives -> triggers PLC for 2 and drains 3, 4
        assert_eq!(frames_and_plcs(jb.push(3, 1920, 40, vec![3])), vec![]);
        assert_eq!(frames_and_plcs(jb.push(4, 2880, 60, vec![4])), vec![None, Some(3), Some(4)]);
    }

    #[test]
    fn handles_u16_sequence_wraparound() {
        let mut jb = JitterBuf::default();
        assert_eq!(frames_and_plcs(jb.push(65534, 0, 0, vec![1])), vec![Some(1)]);
        assert_eq!(frames_and_plcs(jb.push(0, 1920, 40, vec![3])), vec![]);
        assert_eq!(frames_and_plcs(jb.push(65535, 960, 45, vec![2])), vec![Some(2), Some(3)]);
    }
}
