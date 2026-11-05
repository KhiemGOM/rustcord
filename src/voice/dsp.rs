//! What the mic goes through before it's sent or tested: echo cancellation, noise suppression
//! (rnnoise as "Krisp", or a spectral gate as "Standard"), input volume, automatic gain control,
//! and then the gate that decides whether you're talking.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rtrb::Consumer;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use super::aec::{Aec, BLOCK};
use super::audio::{FRAME, MIC_FAKE, Shared};

pub const NOISE_NONE: u8 = 0;
pub const NOISE_KRISP: u8 = 1;
pub const NOISE_STANDARD: u8 = 2;

pub struct Chain {
    shared: Arc<Shared>,
    far: Option<Consumer<f32>>,
    aec: Aec,
    rnn: Box<nnnoiseless::DenoiseState<'static>>,
    rnn_out: Vec<f32>,
    std: StdNoise,
    agc: Agc,
}

impl Chain {
    pub fn new(shared: Arc<Shared>, far: Option<Consumer<f32>>) -> Chain {
        Chain {
            shared,
            far,
            aec: Aec::new(),
            rnn: nnnoiseless::DenoiseState::new(),
            rnn_out: vec![0.0; nnnoiseless::DenoiseState::FRAME_SIZE],
            std: StdNoise::new(),
            agc: Agc::default(),
        }
    }

    /// One 20 ms frame of mic, cleaned in place. Returns rnnoise's best guess that it was
    /// speech, when it ran.
    pub fn run(&mut self, pcm: &mut [f32]) -> Option<f32> {
        debug_assert_eq!(pcm.len(), FRAME);
        let real = self.shared.mic.load(Relaxed) != MIC_FAKE;
        let echo = real && self.shared.echo.load(Relaxed);
        self.far_for(pcm, echo);
        let noise = if real { self.shared.noise.load(Relaxed) } else { NOISE_NONE };
        let mut speech = None;
        let want_vad = self.shared.adv_vad.load(Relaxed) && self.shared.auto_vad.load(Relaxed) && real;
        if noise == NOISE_KRISP || want_vad {
            let fs = nnnoiseless::DenoiseState::FRAME_SIZE;
            let mut best = 0f32;
            for chunk in pcm.chunks_mut(fs) {
                let scaled: Vec<f32> = chunk.iter().map(|x| x * 32768.0).collect();
                best = best.max(self.rnn.process_frame(&mut self.rnn_out, &scaled));
                if noise == NOISE_KRISP {
                    for (o, d) in chunk.iter_mut().zip(&self.rnn_out) {
                        *o = d / 32768.0;
                    }
                }
            }
            speech = Some(best);
        }
        if noise == NOISE_STANDARD {
            for chunk in pcm.chunks_mut(BLOCK) {
                self.std.block(chunk);
            }
        }
        let gain = f32::from_bits(self.shared.in_gain.load(Relaxed));
        for x in pcm.iter_mut() {
            *x *= gain;
        }
        if real && self.shared.agc.load(Relaxed) {
            self.agc.frame(pcm);
        }
        for x in pcm.iter_mut() {
            *x = soft_clip(*x);
        }
        speech
    }

    fn far_for(&mut self, pcm: &mut [f32], echo: bool) {
        let Some(far) = &mut self.far else { return };
        if !echo {
            let n = far.slots();
            if let Ok(c) = far.read_chunk(n) {
                c.commit_all();
            }
            return;
        }
        let behind = far.slots().saturating_sub(BLOCK * 8);
        if behind > 0
            && let Ok(c) = far.read_chunk(behind)
        {
            c.commit_all();
        }
        for chunk in pcm.chunks_mut(BLOCK) {
            let mut block = [0f32; BLOCK];
            let n = far.slots().min(BLOCK);
            if let Ok(c) = far.read_chunk(n) {
                let (a, b) = c.as_slices();
                for (o, x) in block.iter_mut().zip(a.iter().chain(b)) {
                    *o = *x;
                }
                c.commit_all();
            }
            self.aec.block(&block, chunk);
        }
    }
}

fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.9 { x } else { x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh()) }
}

/// Keeps speech near -20 dBFS: turns down fast when it's loud, up slowly when it's quiet, and
/// leaves the gain alone through anything that sounds like background.
#[derive(Default)]
struct Agc {
    db: f32,
}

impl Agc {
    fn frame(&mut self, pcm: &mut [f32]) {
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
        let level = 20.0 * (rms + 1e-9).log10();
        let from = self.db;
        if level > -50.0 {
            let want = (-20.0 - level).clamp(-12.0, 30.0);
            let rate = if want < self.db { 0.35 } else { 0.03 };
            self.db += (want - self.db) * rate;
        }
        let (a, b) = (10f32.powf(from / 20.0), 10f32.powf(self.db / 20.0));
        let n = pcm.len() as f32;
        for (i, x) in pcm.iter_mut().enumerate() {
            *x *= a + (b - a) * i as f32 / n;
        }
    }
}

/// Spectral noise gate: tracks the quietest level each frequency settles at, and turns down
/// what doesn't stand above it. Half-overlapped sqrt-Hann frames, so it adds 5 ms.
struct StdNoise {
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    win: Vec<f32>,
    held: Vec<f32>,
    out: Vec<f32>,
    noise: Vec<f32>,
    smooth: Vec<f32>,
    slow: Vec<f32>,
    gain: Vec<f32>,
    time: Vec<f32>,
    spec: Vec<Complex<f32>>,
    fresh: bool,
}

const HOP: usize = BLOCK / 2;

impl StdNoise {
    fn new() -> StdNoise {
        let mut plan = RealFftPlanner::<f32>::new();
        let bins = BLOCK / 2 + 1;
        StdNoise {
            fwd: plan.plan_fft_forward(BLOCK),
            inv: plan.plan_fft_inverse(BLOCK),
            win: (0..BLOCK).map(|i| (std::f32::consts::PI * (i as f32 + 0.5) / BLOCK as f32).sin()).collect(),
            held: vec![0.0; BLOCK],
            out: vec![0.0; BLOCK],
            noise: vec![1e9; bins],
            smooth: vec![0.0; bins],
            slow: vec![0.0; bins],
            gain: vec![1.0; bins],
            time: vec![0.0; BLOCK],
            spec: vec![Complex::default(); bins],
            fresh: true,
        }
    }

    fn block(&mut self, pcm: &mut [f32]) {
        for hop in pcm.chunks_mut(HOP) {
            self.held.copy_within(HOP.., 0);
            self.held[HOP..].copy_from_slice(hop);
            for ((t, h), w) in self.time.iter_mut().zip(&self.held).zip(&self.win) {
                *t = h * w;
            }
            let _ = self.fwd.process(&mut self.time, &mut self.spec);
            for (k, s) in self.spec.iter_mut().enumerate() {
                let p = s.norm_sqr();
                if self.fresh {
                    (self.smooth[k], self.slow[k], self.noise[k]) = (p, p, p);
                }
                self.smooth[k] = 0.6 * self.smooth[k] + 0.4 * p;
                self.slow[k] = 0.92 * self.slow[k] + 0.08 * p;
                let (sm, slow) = (self.smooth[k], self.slow[k]);
                self.noise[k] = if slow < self.noise[k] { slow.max(1e-9) } else { self.noise[k] * 1.005 + 1e-10 };
                let g = (1.0 - 2.0 * self.noise[k] / (sm + 1e-12)).clamp(0.15, 1.0);
                self.gain[k] = 0.5 * self.gain[k] + 0.5 * g;
                *s *= self.gain[k];
            }
            self.fresh = false;
            self.spec[0].im = 0.0;
            if let Some(last) = self.spec.last_mut() {
                last.im = 0.0;
            }
            let _ = self.inv.process(&mut self.spec, &mut self.time);
            let scale = 1.0 / BLOCK as f32;
            for ((o, t), w) in self.out.iter_mut().zip(&self.time).zip(&self.win) {
                *o += t * w * scale;
            }
            hop.copy_from_slice(&self.out[..HOP]);
            self.out.copy_within(HOP.., 0);
            self.out[HOP..].fill(0.0);
        }
    }
}

const HANG: u32 = 15;

/// Decides, a frame at a time, whether what the mic heard goes out.
pub struct Gate {
    floor: f32,
    hang: u32,
    released: Option<Instant>,
    keyed: bool,
    cues: bool,
}

impl Gate {
    pub fn new(cues: bool) -> Gate {
        Gate { floor: -60.0, hang: 0, released: None, keyed: false, cues }
    }

    /// `db` is the frame's level after the chain, `speech` rnnoise's opinion if it ran.
    pub fn open(&mut self, shared: &Shared, db: f32, speech: Option<f32>, muted: bool) -> bool {
        // automatic sensitivity: follow the noise floor down fast and up slowly, open 15 dB above it
        self.floor = if db < self.floor { db } else { self.floor + (db - self.floor) * 0.002 };
        let open = if shared.open_mic.load(Relaxed) {
            true
        } else if shared.ptt.load(Relaxed) {
            let held = crate::hotkeys::PTT_HELD.load(Relaxed);
            let open = if held {
                self.released = None;
                true
            } else {
                let at = *self.released.get_or_insert_with(Instant::now);
                at.elapsed().as_millis() < shared.ptt_delay_ms.load(Relaxed) as u128
            };
            if open != self.keyed && self.cues {
                crate::sound::cue(if open { "ptt_start" } else { "ptt_stop" });
            }
            self.keyed = open;
            open
        } else if shared.auto_vad.load(Relaxed) {
            match speech {
                Some(p) => p > 0.5 && db > -80.0,
                None => db > (self.floor + 15.0).clamp(-70.0, -25.0),
            }
        } else {
            db > shared.vad_db.load(Relaxed) as f32
        };
        if !muted && open {
            self.hang = HANG;
        } else if muted {
            self.hang = 0;
        } else {
            self.hang = self.hang.saturating_sub(1);
        }
        self.hang > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn tone(amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (i as f32 * 0.07).sin()).collect()
    }

    fn noise(amp: f32, n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                amp * ((s >> 8) as f32 / (1 << 23) as f32 - 1.0)
            })
            .collect()
    }

    #[test]
    fn agc_brings_quiet_speech_up_and_leaves_background() {
        let mut agc = Agc::default();
        let mut quiet = tone(0.01 * 1.414, FRAME * 300);
        for f in quiet.chunks_mut(FRAME) {
            agc.frame(f);
        }
        let out = 20.0 * rms(&quiet[FRAME * 280..]).log10();
        assert!((-24.0..-16.0).contains(&out), "{out} dBFS");
        let mut agc = Agc::default();
        let mut hiss = noise(0.002, FRAME * 100, 3);
        for f in hiss.chunks_mut(FRAME) {
            agc.frame(f);
        }
        assert!(agc.db == 0.0);
    }

    #[test]
    fn standard_suppression_takes_down_steady_noise_and_keeps_a_tone() {
        let n = BLOCK * 400;
        let mut hiss = noise(0.02, n, 5);
        let before = rms(&hiss[n / 2..]);
        let mut ns = StdNoise::new();
        for c in hiss.chunks_mut(BLOCK) {
            ns.block(c);
        }
        let after = rms(&hiss[n / 2..]);
        assert!(after < before * 0.5, "{after} from {before}");
        let late = |t: Vec<f32>| -> Vec<f32> { t.into_iter().enumerate().map(|(i, x)| if i > n / 4 { x } else { 0.0 }).collect() };
        let mut mixed: Vec<f32> = late(tone(0.3, n)).iter().zip(noise(0.02, n, 9)).map(|(t, h)| t + h).collect();
        let want = rms(&tone(0.3, n / 2));
        let mut ns = StdNoise::new();
        for c in mixed.chunks_mut(BLOCK) {
            ns.block(c);
        }
        assert!(rms(&mixed[n / 2..]) > want * 0.8);
    }
}
