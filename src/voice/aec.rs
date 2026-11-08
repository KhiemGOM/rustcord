//! Acoustic echo cancellation: a partitioned-block frequency-domain NLMS filter learns the path
//! from the speakers to the mic and subtracts what the speakers played, then a residual
//! suppressor takes down what the filter left behind while only the far end is talking.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const BLOCK: usize = 480;
const PARTS: usize = 30;
const BINS: usize = BLOCK + 1;
const MU: f32 = 0.6;

type Spec = Vec<Complex<f32>>;

pub struct Aec {
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    xf: Vec<Spec>,
    w: Vec<Spec>,
    at: usize,
    power: Vec<f32>,
    far_rms: [f32; PARTS],
    time: Vec<f32>,
    spec: Spec,
    acc: Spec,
    gain: f32,
    pub far_only: bool,
}

impl Aec {
    pub fn new() -> Aec {
        let mut plan = RealFftPlanner::<f32>::new();
        Aec {
            fwd: plan.plan_fft_forward(2 * BLOCK),
            inv: plan.plan_fft_inverse(2 * BLOCK),
            window: vec![0.0; 2 * BLOCK],
            xf: vec![vec![Complex::default(); BINS]; PARTS],
            w: vec![vec![Complex::default(); BINS]; PARTS],
            at: 0,
            power: vec![0.0; BINS],
            far_rms: [0.0; PARTS],
            time: vec![0.0; 2 * BLOCK],
            spec: vec![Complex::default(); BINS],
            acc: vec![Complex::default(); BINS],
            gain: 1.0,
            far_only: false,
        }
    }

    /// One block: `far` is what went to the speakers, `near` the mic, cleaned in place.
    pub fn block(&mut self, far: &[f32], near: &mut [f32]) {
        debug_assert!(far.len() == BLOCK && near.len() == BLOCK);
        self.window.copy_within(BLOCK.., 0);
        self.window[BLOCK..].copy_from_slice(far);
        self.time.copy_from_slice(&self.window);
        let _ = self.fwd.process(&mut self.time, &mut self.spec);

        let old = &self.xf[self.at];
        for ((p, o), n) in self.power.iter_mut().zip(old).zip(&self.spec) {
            *p = (*p + n.norm_sqr() - o.norm_sqr()).max(0.0);
        }
        self.xf[self.at].copy_from_slice(&self.spec);
        let newest = self.at;
        self.at = (self.at + 1) % PARTS;
        let part = |k: usize| (newest + PARTS - k) % PARTS;

        self.acc.fill(Complex::default());
        for k in 0..PARTS {
            for ((a, w), x) in self.acc.iter_mut().zip(&self.w[k]).zip(&self.xf[part(k)]) {
                *a += w * x;
            }
        }
        let _ = self.inv.process(&mut self.acc, &mut self.time);
        let scale = 1.0 / (2 * BLOCK) as f32;
        let mut echo = [0f32; BLOCK];
        for (y, t) in echo.iter_mut().zip(&self.time[BLOCK..]) {
            *y = t * scale;
        }
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        let (d_rms, y_rms, x_rms) = (rms(near), rms(&echo), rms(far));
        let related = near.iter().zip(&echo).map(|(d, y)| d * y).sum::<f32>() / (BLOCK as f32 * d_rms * y_rms).max(1e-10);
        let mu = if related > 0.5 { MU } else { 0.1 };
        self.far_rms[newest] = x_rms;
        let mut err = [0f32; BLOCK];
        for ((e, d), y) in err.iter_mut().zip(near.iter()).zip(&echo) {
            *e = d - y;
        }
        let e_rms = rms(&err);

        let far_peak = self.far_rms.iter().cloned().fold(0.0, f32::max);
        self.time[..BLOCK].fill(0.0);
        self.time[BLOCK..].copy_from_slice(near);
        let _ = self.fwd.process(&mut self.time, &mut self.spec);
        let near_power = self.spec.iter().map(|s| s.norm_sqr()).sum::<f32>();
        let far_power = self.power.iter().sum::<f32>();
        let shared = self.spec.iter().zip(&self.power).map(|(s, p)| (s.norm_sqr() * p).sqrt()).sum::<f32>();
        let overlap = shared / (near_power * far_power).sqrt().max(1e-10);
        let talking_over = overlap < 0.5;
        self.far_only = far_peak > 1e-3 && !talking_over;
        if x_rms > 1e-3 && !talking_over {
            self.time[..BLOCK].fill(0.0);
            self.time[BLOCK..].copy_from_slice(&err);
            let _ = self.fwd.process(&mut self.time, &mut self.spec);
            let eps = BLOCK as f32 * 1e-3;
            for k in 0..PARTS {
                let x = &self.xf[part(k)];
                for (((a, s), x), p) in self.acc.iter_mut().zip(&self.spec).zip(x).zip(&self.power) {
                    *a = x.conj() * s * (mu / (p + eps));
                }
                let _ = self.inv.process(&mut self.acc, &mut self.time);
                self.time[BLOCK..].fill(0.0);
                for t in &mut self.time[..BLOCK] {
                    *t *= scale;
                }
                let _ = self.fwd.process(&mut self.time, &mut self.acc);
                for (w, g) in self.w[k].iter_mut().zip(&self.acc) {
                    *w += g;
                }
            }
        }

        let (pe, py) = (e_rms * e_rms, y_rms * y_rms);
        let target = (pe / (pe + 0.3 * py + 1e-10)).sqrt().max(0.1);
        let step = (target - self.gain) / BLOCK as f32;
        for (n, e) in near.iter_mut().zip(&err) {
            self.gain += step;
            *n = e * self.gain;
        }
        self.gain = target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u32);

    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.0 >> 8) as f32 / (1 << 23) as f32 - 1.0
        }
    }

    fn speech(rng: &mut Rng, n: usize) -> Vec<f32> {
        let mut lp = 0.0;
        (0..n)
            .map(|i| {
                lp += 0.35 * (rng.next() - lp);
                lp * 0.5 * (0.6 + 0.4 * (i as f32 / 4000.0).sin())
            })
            .collect()
    }

    fn echo_path(far: &[f32]) -> Vec<f32> {
        let taps = [(2400, 0.5), (2500, -0.25), (2790, 0.2), (3300, -0.12), (4100, 0.08)];
        (0..far.len())
            .map(|i| taps.iter().filter(|(d, _)| i >= *d).map(|(d, g)| far[i - d] * g).sum())
            .collect()
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|v| v * v).sum()
    }

    fn run(far: &[f32], near: &mut [f32]) {
        let mut aec = Aec::new();
        for (f, n) in far.chunks(BLOCK).zip(near.chunks_mut(BLOCK)) {
            aec.block(f, n);
        }
    }

    #[test]
    fn cancels_a_delayed_echo() {
        let mut rng = Rng(7);
        let far = speech(&mut rng, 48_000 * 6);
        let echo = echo_path(&far);
        let mut out = echo.clone();
        run(&far, &mut out);
        let tail = 48_000 * 5..;
        let erle = 10.0 * (energy(&echo[tail.clone()]) / energy(&out[tail])).log10();
        assert!(erle > 20.0, "erle {erle} dB");
    }

    #[test]
    fn keeps_quiet_speech_over_loud_playback_without_echo() {
        let n = 48_000 * 4;
        let far: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * 0.0576).sin()).collect();
        let talker: Vec<f32> = (0..n).map(|i| 0.05 * (i as f32 * 0.0839).sin()).collect();
        let mut out = talker.clone();
        run(&far, &mut out);
        let tail = 48_000 * 2..;
        let error: Vec<f32> = out[tail.clone()].iter().zip(&talker[tail.clone()]).map(|(a,b)| a-b).collect();
        let snr = 10.0 * (energy(&talker[tail]) / energy(&error).max(1e-20)).log10();
        eprintln!("near-end speech: {snr:.1} dB SNR");
        assert!(snr > 10.0, "near-end speech distorted: {snr} dB SNR");
    }

    #[test]
    fn keeps_the_near_end_talker() {
        let mut rng = Rng(11);
        let far = speech(&mut rng, 48_000 * 6);
        let echo = echo_path(&far);
        let talker: Vec<f32> = (0..far.len()).map(|i| 0.3 * (i as f32 * 0.07).sin() * if (i / 24_000) % 2 == 0 { 1.0 } else { 0.0 }).collect();
        let mut out: Vec<f32> = echo.iter().zip(&talker).map(|(e, t)| e + t).collect();
        run(&far, &mut out);
        let tail = 48_000 * 4..;
        let kept = energy(&out[tail.clone()]) / energy(&talker[tail]);
        assert!(kept > 0.6 && kept < 1.6, "kept {kept}");
    }

    #[test]
    fn keeps_unrelated_quiet_speech_over_playback() {
        let n = 48_000 * 4;
        let far = speech(&mut Rng(41), n);
        let talker: Vec<f32> = speech(&mut Rng(73), n).into_iter().map(|v| v * 0.1).collect();
        let mut out = talker.clone();
        run(&far, &mut out);
        let tail = 48_000 * 2..;
        let error: Vec<f32> = out[tail.clone()].iter().zip(&talker[tail.clone()]).map(|(a, b)| a - b).collect();
        let snr = 10.0 * (energy(&talker[tail]) / energy(&error).max(1e-20)).log10();
        eprintln!("unrelated near-end speech: {snr:.1} dB SNR");
        assert!(snr > 10.0, "near-end speech distorted: {snr} dB SNR");
    }

    #[test]
    fn keeps_quiet_speech_after_learning_the_echo() {
        let n = 48_000 * 6;
        let far = speech(&mut Rng(53), n);
        let talker: Vec<f32> = (0..n).map(|i| if i < 96_000 { 0.0 } else { 0.01 * (i as f32 * 0.0839).sin() }).collect();
        let mut out: Vec<f32> = echo_path(&far).iter().zip(&talker).map(|(e, t)| e + t).collect();
        run(&far, &mut out);
        let tail = 48_000 * 4..;
        let error: Vec<f32> = out[tail.clone()].iter().zip(&talker[tail.clone()]).map(|(a, b)| a - b).collect();
        let snr = 10.0 * (energy(&talker[tail]) / energy(&error).max(1e-20)).log10();
        eprintln!("quiet speech with echo: {snr:.1} dB SNR");
        assert!(snr > 10.0, "quiet speech suppressed with echo: {snr} dB SNR");
    }
}
