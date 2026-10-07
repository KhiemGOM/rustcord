//! Mic capture, VAD + opus encoding, and the playback mixer.
//!
//! The cpal callbacks only touch ring buffers. Capture pushes mono samples at
//! the device rate; a plain thread resamples to 48k, runs VAD and encodes.
//! Playback owns one ring per remote speaker (filled by the connection task
//! with decoded 48k stereo) and mixes them in the output callback.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub const SLOTS: usize = 32;
pub const FRAME: usize = 960;
const RING: usize = 48_000; // 0.5s of stereo
const PREFILL: usize = FRAME * 2 * 3; // 60ms before a speaker starts playing
const MAX_FILL: usize = FRAME * 2 * 10; // past 200ms we're drifting, drop the excess
const HANG: u32 = 15; // 300ms of hangover so word endings don't get clipped
pub const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];

pub struct Shared {
    pub mute: AtomicBool,
    pub deaf: AtomicBool,
    pub vad_db: AtomicI32,
    pub sent: AtomicU64,
    pub voiced: AtomicU64,
    pub recv: AtomicU64,
    pub decoded: AtomicU64,
    pub mic: AtomicI32,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            mute: AtomicBool::new(false),
            deaf: AtomicBool::new(false),
            vad_db: AtomicI32::new(-50),
            sent: AtomicU64::new(0),
            voiced: AtomicU64::new(0),
            recv: AtomicU64::new(0),
            decoded: AtomicU64::new(0),
            mic: AtomicI32::new(MIC_UNKNOWN),
        }
    }
}

pub const MIC_UNKNOWN: i32 = 0;
pub const MIC_NONE: i32 = 1;
pub const MIC_DEVICE: i32 = 2;
pub const MIC_FAKE: i32 = 3;

pub enum Out {
    Voice(Vec<u8>),
    Silence,
    /// talk spurt is over
    Idle,
}

pub struct Engine {
    stop: Arc<AtomicBool>,
    th: Option<JoinHandle<()>>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.th.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

pub fn start(shared: Arc<Shared>, out: mpsc::UnboundedSender<Out>) -> (Engine, Vec<Producer<f32>>) {
    let (prods, cons): (Vec<_>, Vec<_>) = (0..SLOTS).map(|_| RingBuffer::<f32>::new(RING)).unzip();
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let th = std::thread::Builder::new()
        .name("voice-audio".into())
        .spawn(move || audio_thread(shared, out, cons, s))
        .ok();
    (Engine { stop, th }, prods)
}

// cpal streams aren't Send everywhere, so they live and die on this thread
fn audio_thread(shared: Arc<Shared>, out: mpsc::UnboundedSender<Out>, cons: Vec<Consumer<f32>>, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();
    let _play = host.default_output_device().and_then(|d| playback(&d, cons, shared.clone()));
    let fake = std::env::var_os("FASTCORD_FAKE_MIC").is_some();
    let mut mic = None;
    if !fake {
        mic = host.default_input_device().and_then(|d| capture(&d));
    }
    shared.mic.store(
        if fake {
            MIC_FAKE
        } else if mic.is_some() {
            MIC_DEVICE
        } else {
            MIC_NONE
        },
        Relaxed,
    );
    if !fake && mic.is_none() {
        // nothing to send; the connection task already pushed its silence frames
        while !stop.load(Relaxed) {
            std::thread::park_timeout(Duration::from_millis(200));
        }
        return;
    }

    let mut tx = Tx::new(shared, out);
    let started = Instant::now();
    let mut made = 0u64;
    let mut phase = 0f32;
    let mut buf: Vec<f32> = Vec::with_capacity(FRAME * 4);
    let mut raw: Vec<f32> = Vec::new();
    while !stop.load(Relaxed) {
        std::thread::park_timeout(Duration::from_millis(5));
        match &mut mic {
            Some((_, cons, lerp)) => {
                raw.clear();
                if let Ok(chunk) = cons.read_chunk(cons.slots()) {
                    let (a, b) = chunk.as_slices();
                    raw.extend_from_slice(a);
                    raw.extend_from_slice(b);
                    chunk.commit_all();
                }
                for &x in &raw {
                    lerp.push([x], |[y]| buf.push(y));
                }
            }
            None => {
                // 440Hz bursts, a second on and a second off, so VAD has edges to find
                let due = started.elapsed().as_micros() as u64 * 48 / 1000;
                while made < due {
                    let on = (made / 48_000) % 2 == 0;
                    buf.push(if on { 0.25 * phase.sin() } else { 0.0 });
                    phase = (phase + TAU * 440.0 / 48_000.0) % TAU;
                    made += 1;
                }
            }
        }
        while buf.len() >= FRAME {
            tx.frame(&buf[..FRAME]);
            buf.drain(..FRAME);
        }
    }
}

struct Tx {
    shared: Arc<Shared>,
    out: mpsc::UnboundedSender<Out>,
    enc: Option<opus_rs::OpusEncoder>,
    hang: u32,
    talking: bool,
    tail: u8,
    stereo: Vec<f32>,
    pkt: [u8; 1276],
}

impl Tx {
    fn new(shared: Arc<Shared>, out: mpsc::UnboundedSender<Out>) -> Tx {
        let enc = opus_rs::OpusEncoder::new(48_000, 2, opus_rs::Application::Voip).ok().map(|mut e| {
            e.bitrate_bps = 64_000;
            e
        });
        Tx { shared, out, enc, hang: 0, talking: false, tail: 0, stereo: vec![0.0; FRAME * 2], pkt: [0; 1276] }
    }

    fn frame(&mut self, pcm: &[f32]) {
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
        let db = 20.0 * (rms + 1e-9).log10();
        let muted = self.shared.mute.load(Relaxed) || self.shared.deaf.load(Relaxed);
        if !muted && db > self.shared.vad_db.load(Relaxed) as f32 {
            self.hang = HANG;
        } else if muted {
            self.hang = 0;
        } else {
            self.hang = self.hang.saturating_sub(1);
        }
        if self.hang > 0 {
            let Some(enc) = &mut self.enc else { return };
            for (i, x) in pcm.iter().enumerate() {
                self.stereo[i * 2] = *x;
                self.stereo[i * 2 + 1] = *x;
            }
            if let Ok(n) = enc.encode(&self.stereo, FRAME, &mut self.pkt) {
                self.talking = true;
                self.tail = 5;
                let _ = self.out.send(Out::Voice(self.pkt[..n].to_vec()));
            }
        } else if self.talking {
            // five silence frames keep the far end's opus from interpolating into the gap
            let _ = self.out.send(Out::Silence);
            self.tail -= 1;
            if self.tail == 0 {
                self.talking = false;
                let _ = self.out.send(Out::Idle);
            }
        }
    }
}

type Mic = (cpal::Stream, Consumer<f32>, Lerp<1>);

fn capture(dev: &cpal::Device) -> Option<Mic> {
    let cfg = dev.default_input_config().ok()?;
    let mut conf = cfg.config();
    let ch = conf.channels as usize;
    let rate = conf.sample_rate;
    conf.buffer_size = cpal::BufferSize::Default;
    let (mut prod, cons) = RingBuffer::<f32>::new(rate as usize);
    let push = move |p: &mut Producer<f32>, frames: &mut dyn Iterator<Item = f32>| {
        for x in frames {
            if p.push(x).is_err() {
                break;
            }
        }
    };
    let err = |_| {};
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => dev
            .build_input_stream::<f32, _, _>(
                conf,
                move |data, _| push(&mut prod, &mut data.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32)),
                err,
                None,
            )
            .ok()?,
        cpal::SampleFormat::I16 => dev
            .build_input_stream::<i16, _, _>(
                conf,
                move |data, _| {
                    push(&mut prod, &mut data.chunks(ch).map(|f| f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / ch as f32))
                },
                err,
                None,
            )
            .ok()?,
        _ => return None,
    };
    stream.play().ok()?;
    Some((stream, cons, Lerp::new(rate as f64 / 48_000.0)))
}

fn playback(dev: &cpal::Device, slots: Vec<Consumer<f32>>, shared: Arc<Shared>) -> Option<cpal::Stream> {
    let cfg = dev.default_output_config().ok()?;
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return None;
    }
    let conf = cfg.config();
    let mut mix = Mix {
        primed: vec![false; slots.len()],
        slots,
        buf: vec![0.0; 48_000 * 2],
        lerp: Lerp::new(48_000.0 / conf.sample_rate as f64),
        ch: conf.channels as usize,
        shared,
    };
    let stream = dev.build_output_stream::<f32, _, _>(conf, move |data, _| mix.fill(data), |_| {}, None).ok()?;
    stream.play().ok()?;
    Some(stream)
}

struct Mix {
    slots: Vec<Consumer<f32>>,
    primed: Vec<bool>,
    buf: Vec<f32>,
    lerp: Lerp<2>,
    ch: usize,
    shared: Arc<Shared>,
}

impl Mix {
    fn fill(&mut self, out: &mut [f32]) {
        let frames = out.len() / self.ch;
        let need = (self.lerp.needed(frames) * 2).min(self.buf.len());
        let mix = &mut self.buf[..need];
        mix.fill(0.0);
        for (s, primed) in self.slots.iter_mut().zip(self.primed.iter_mut()) {
            let avail = s.slots() & !1;
            if !*primed {
                if avail < PREFILL {
                    continue;
                }
                *primed = true;
            }
            if avail > MAX_FILL
                && let Ok(c) = s.read_chunk(avail - PREFILL)
            {
                c.commit_all();
            }
            let take = s.slots().min(need) & !1;
            if take < need {
                *primed = false;
            }
            if let Ok(c) = s.read_chunk(take) {
                let (a, b) = c.as_slices();
                for (m, x) in mix.iter_mut().zip(a.iter().chain(b)) {
                    *m += x;
                }
                c.commit_all();
            }
        }
        if self.shared.deaf.load(Relaxed) {
            mix.fill(0.0);
        }
        let mut i = 0;
        let mut next = || {
            let f = if i + 1 < mix.len() { [mix[i], mix[i + 1]] } else { [0.0; 2] };
            i += 2;
            f
        };
        for f in out.chunks_mut(self.ch) {
            let [l, r] = self.lerp.pull(&mut next);
            let (l, r) = (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0));
            match f.len() {
                1 => f[0] = (l + r) * 0.5,
                _ => {
                    f[0] = l;
                    f[1] = r;
                    f[2..].fill(0.0);
                }
            }
        }
    }
}

/// Linear resampler. `step` is input samples per output sample.
struct Lerp<const N: usize> {
    step: f64,
    pos: f64,
    prev: [f32; N],
    cur: [f32; N],
}

impl<const N: usize> Lerp<N> {
    fn new(step: f64) -> Self {
        Lerp { step, pos: 0.0, prev: [0.0; N], cur: [0.0; N] }
    }

    fn at(&self) -> [f32; N] {
        let t = self.pos as f32;
        std::array::from_fn(|c| self.prev[c] + (self.cur[c] - self.prev[c]) * t)
    }

    /// push one input frame, get whatever output frames fall before it
    fn push(&mut self, x: [f32; N], mut emit: impl FnMut([f32; N])) {
        self.prev = self.cur;
        self.cur = x;
        while self.pos < 1.0 {
            emit(self.at());
            self.pos += self.step;
        }
        self.pos -= 1.0;
    }

    /// input frames `pull` will consume for the next `n` outputs
    fn needed(&self, n: usize) -> usize {
        let mut p = self.pos;
        let mut k = 0;
        for _ in 0..n {
            p += self.step;
            while p >= 1.0 {
                p -= 1.0;
                k += 1;
            }
        }
        k
    }

    fn pull(&mut self, src: &mut impl FnMut() -> [f32; N]) -> [f32; N] {
        self.pos += self.step;
        while self.pos >= 1.0 {
            self.pos -= 1.0;
            self.prev = self.cur;
            self.cur = src();
        }
        self.at()
    }
}
