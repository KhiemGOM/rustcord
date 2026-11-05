//! The rolling buffers behind a clip: the last stretch of encoded video, and of mixed audio.

use std::collections::VecDeque;
use std::sync::Arc;

pub const TICKS: i64 = 10_000_000;
pub const RATE: i64 = 48_000;

#[derive(Clone)]
pub struct Frame {
    pub data: Arc<[u8]>,
    pub pts: i64,
    pub key: bool,
}

/// Annex B access units, kept from the keyframe before the oldest second wanted, so any cut starts
/// on something that decodes.
pub struct Video {
    frames: VecDeque<Frame>,
    keys: VecDeque<u64>,
    base: u64,
    bytes: usize,
    span: i64,
    cap: usize,
}

impl Video {
    pub fn new(span: i64, cap: usize) -> Video {
        Video { frames: VecDeque::new(), keys: VecDeque::new(), base: 0, bytes: 0, span, cap }
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn push(&mut self, f: Frame) {
        if f.key {
            self.keys.push_back(self.base + self.frames.len() as u64);
        } else if self.keys.is_empty() {
            return;
        }
        let now = f.pts;
        self.bytes += f.data.len();
        self.frames.push_back(f);
        while self.keys.len() > 1 && (self.bytes > self.cap || self.pts_at(self.keys[1]).is_some_and(|p| now - p >= self.span)) {
            self.drop_gop();
        }
    }

    fn pts_at(&self, seq: u64) -> Option<i64> {
        self.frames.get((seq - self.base) as usize).map(|f| f.pts)
    }

    fn drop_gop(&mut self) {
        self.keys.pop_front();
        let Some(next) = self.keys.front().copied() else { return };
        while self.base < next {
            if let Some(f) = self.frames.pop_front() {
                self.bytes -= f.data.len();
            }
            self.base += 1;
        }
    }

    /// The last `len` ticks, starting on the latest keyframe at or before that point.
    pub fn cut(&self, len: i64) -> Vec<Frame> {
        let Some(last) = self.frames.back() else { return Vec::new() };
        let from = last.pts - len;
        let start = self.keys.iter().rev().find(|k| self.pts_at(**k).is_some_and(|p| p <= from)).or(self.keys.front());
        let Some(start) = start else { return Vec::new() };
        self.frames.iter().skip((*start - self.base) as usize).cloned().collect()
    }
}

/// Mono 48 kHz, mixed in place by wall-clock sample index so voices from different sources line
/// up without being queued.
pub struct Audio {
    ring: Vec<i16>,
    head: i64,
}

impl Audio {
    pub fn new(secs: i64) -> Audio {
        Audio { ring: vec![0; (secs * RATE) as usize], head: 0 }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn mix(&mut self, at: i64, pcm: &[f32]) {
        let cap = self.ring.len() as i64;
        let end = at + pcm.len() as i64;
        if self.head == 0 {
            self.head = at;
        }
        if at < self.head - cap || end > self.head + cap {
            return;
        }
        while self.head < end {
            let i = (self.head % cap) as usize;
            self.ring[i] = 0;
            self.head += 1;
        }
        for (k, x) in pcm.iter().enumerate() {
            let i = ((at + k as i64) % cap) as usize;
            self.ring[i] = (self.ring[i] as i32 + (x.clamp(-1.0, 1.0) * 32767.0) as i32).clamp(-32768, 32767) as i16;
        }
    }

    pub fn head(&self) -> i64 {
        self.head
    }

    pub fn take(&self, from: i64, to: i64) -> Vec<i16> {
        let cap = self.ring.len() as i64;
        let from = from.max(self.head - cap).max(0);
        let to = to.min(self.head);
        (from..to).map(|i| self.ring[(i % cap) as usize]).collect()
    }
}

/// Where each source's next samples go: back to back while it keeps talking, re-anchored to the
/// clock after a gap or when it drifts.
#[derive(Default)]
pub struct Cursors(Vec<(u64, i64)>);

impl Cursors {
    pub fn place(&mut self, src: u64, now: i64, n: usize) -> i64 {
        let slot = match self.0.iter().position(|c| c.0 == src) {
            Some(i) => i,
            None => {
                self.0.push((src, now));
                self.0.len() - 1
            }
        };
        let c = &mut self.0[slot].1;
        if *c + (RATE / 25) < now || *c > now + RATE / 10 {
            *c = now;
        }
        let at = *c;
        *c += n as i64;
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sec: i64, key: bool, n: usize) -> Frame {
        Frame { data: vec![0u8; n].into(), pts: sec * TICKS / 10, key }
    }

    #[test]
    fn video_keeps_the_keyframe_before_the_span() {
        let mut v = Video::new(30 * TICKS, usize::MAX);
        for i in 0..1200 {
            v.push(frame(i, i % 20 == 0, 100));
        }
        let cut = v.cut(30 * TICKS);
        assert!(cut[0].key);
        let span = cut.last().unwrap().pts - cut[0].pts;
        assert!(span >= 30 * TICKS && span < 32 * TICKS, "{span}");
        assert!(v.frames.len() < 340);
    }

    #[test]
    fn video_ignores_frames_before_the_first_keyframe() {
        let mut v = Video::new(TICKS, usize::MAX);
        v.push(frame(0, false, 10));
        assert!(v.cut(TICKS).is_empty());
        v.push(frame(1, true, 10));
        v.push(frame(2, false, 10));
        assert_eq!(v.cut(TICKS).len(), 2);
    }

    #[test]
    fn video_stays_under_its_byte_cap() {
        let mut v = Video::new(120 * TICKS, 10_000);
        for i in 0..2000 {
            v.push(frame(i, i % 20 == 0, 100));
        }
        assert!(v.bytes() <= 10_000 + 20 * 100, "{}", v.bytes());
        assert!(v.cut(120 * TICKS)[0].key);
    }

    #[test]
    fn audio_sums_sources_and_wraps() {
        let mut a = Audio::new(1);
        let ones = vec![0.5f32; 960];
        a.mix(0, &ones);
        a.mix(0, &ones);
        assert_eq!(a.take(0, 960)[10], 32766);
        for i in 0..200 {
            a.mix(i * 960, &ones);
        }
        let t = a.take(a.head() - 960, a.head());
        assert_eq!(t.len(), 960);
        assert_eq!(t[0], 16383);
        assert_eq!(a.take(0, 100).len(), 0);
    }

    #[test]
    fn cursors_follow_a_talker_and_reset_after_a_gap() {
        let mut c = Cursors::default();
        assert_eq!(c.place(1, 1000, 960), 1000);
        assert_eq!(c.place(1, 1960, 960), 1960);
        assert_eq!(c.place(1, 100_000, 960), 100_000);
        assert_eq!(c.place(2, 5, 960), 5);
    }
}
