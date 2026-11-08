//! Mic capture, VAD + opus encoding, and the playback mixer.
//!
//! The cpal callbacks only touch ring buffers. Capture pushes mono samples at
//! the device rate; a plain thread resamples to 48k, runs VAD and encodes.
//! Playback owns one ring per remote speaker (filled by the connection task
//! with decoded 48k stereo) and mixes them in the output callback.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use super::dsp::{Chain, Gate};

pub const SLOTS: usize = 32;
pub const FRAME: usize = 960;
const RING: usize = 48_000; // 0.5s of stereo
const PREFILL: usize = FRAME * 2 * 3; // 60ms before a speaker starts playing
const MAX_FILL: usize = FRAME * 2 * 10; // past 200ms we're drifting, drop the excess
pub const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];

pub struct Shared {
    pub mute: AtomicBool,
    pub deaf: AtomicBool,
    pub vad_db: AtomicI32,
    /// input level of the last mic frame, dBFS
    pub level: AtomicI32,
    /// (mic, speaker) by name, None for the system default
    pub devices: Mutex<(Option<String>, Option<String>)>,
    pub sent: AtomicU64,
    pub voiced: AtomicU64,
    pub recv: AtomicU64,
    pub decoded: AtomicU64,
    /// frames that went through DAVE, each way
    pub e2ee_out: AtomicU64,
    pub e2ee_in: AtomicU64,
    /// video frames decoded from others, and encoded by us
    pub video_in: AtomicU64,
    pub video_out: AtomicU64,
    /// what discord's server last reported having of our video: the highest packet sequence it
    /// saw, and the share of packets it lost (out of 256)
    pub video_acked: AtomicU64,
    pub video_lost: AtomicU32,
    pub mic: AtomicI32,
    /// voice settings: push-to-talk, its release delay, input/output gain (f32 bits),
    /// and whether the vad threshold follows the noise floor
    pub ptt: AtomicBool,
    pub ptt_delay_ms: AtomicU32,
    pub in_gain: AtomicU32,
    pub out_gain: AtomicU32,
    pub auto_vad: AtomicBool,
    pub peers: Mutex<std::collections::HashMap<u64, f32>>,
    /// what the mic goes through: dsp::NOISE_*, echo cancellation, gain control, rnnoise's
    /// voice detection, and the studio profile's always-open mic
    pub noise: AtomicU8,
    pub echo: AtomicBool,
    pub agc: AtomicBool,
    pub adv_vad: AtomicBool,
    pub open_mic: AtomicBool,
    /// how far other apps duck (percent), and for whom; whether we're sending speech, and the
    /// millisecond (by `now_ms`) anyone else last made a sound
    pub atten: AtomicU32,
    pub atten_self: AtomicBool,
    pub atten_others: AtomicBool,
    pub talking: AtomicBool,
    pub others_ms: AtomicU64,
    /// bumped whenever the device choice changes, so the engine reopens them
    pub dev_gen: AtomicU32,
    pub qos: AtomicBool,
    pub silence_warn: AtomicBool,
    pub aec_dump: AtomicBool,
    /// set when the mic has been dead silent for a while in a call
    pub no_audio: AtomicBool,
}

impl Shared {
    pub fn apply(&self, s: &crate::cache::Settings) {
        use crate::cache::{InputProfile, Noise};
        let studio = s.input_profile == InputProfile::Studio;
        let custom = s.input_profile == InputProfile::Custom;
        let isolation = s.input_profile == InputProfile::VoiceIsolation;
        let noise = match (s.input_profile, s.noise) {
            (InputProfile::Studio, _) => super::dsp::NOISE_NONE,
            (InputProfile::VoiceIsolation, _) => super::dsp::NOISE_KRISP,
            (_, Noise::Krisp) => super::dsp::NOISE_KRISP,
            (_, Noise::Standard) => super::dsp::NOISE_STANDARD,
            (_, Noise::None) => super::dsp::NOISE_NONE,
        };
        let auto = isolation || (custom && s.auto_sensitivity);
        self.vad_db.store(s.vad_db, Relaxed);
        self.auto_vad.store(auto, Relaxed);
        self.noise.store(noise, Relaxed);
        self.echo.store(isolation || (custom && s.echo_cancel), Relaxed);
        self.agc.store(isolation || (custom && s.agc), Relaxed);
        self.adv_vad.store(isolation || (custom && s.adv_vad), Relaxed);
        self.open_mic.store(studio, Relaxed);
        self.ptt.store(s.ptt && !studio, Relaxed);
        self.ptt_delay_ms.store(s.ptt_delay_ms, Relaxed);
        self.in_gain.store(s.input_volume.to_bits(), Relaxed);
        self.out_gain.store(s.output_volume.to_bits(), Relaxed);
        self.atten.store(s.attenuation, Relaxed);
        self.atten_self.store(s.atten_self, Relaxed);
        self.atten_others.store(s.atten_others, Relaxed);
        self.qos.store(s.qos, Relaxed);
        self.silence_warn.store(s.silence_warning, Relaxed);
        self.aec_dump.store(s.aec_dump, Relaxed);
        let want = (s.mic.clone(), s.speaker.clone());
        if let Ok(mut d) = self.devices.lock()
            && *d != want
        {
            *d = want;
            self.dev_gen.fetch_add(1, Relaxed);
        }
    }
}

fn want_in_name(s: &Shared) -> Option<String> {
    s.devices.lock().ok().and_then(|d| d.0.clone())
}

fn want_out_name(s: &Shared) -> Option<String> {
    s.devices.lock().ok().and_then(|d| d.1.clone())
}

/// Milliseconds on a clock that starts with the first call.
pub fn now_ms() -> u64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            mute: AtomicBool::new(false),
            deaf: AtomicBool::new(false),
            vad_db: AtomicI32::new(-50),
            level: AtomicI32::new(-100),
            devices: Mutex::new((None, None)),
            sent: AtomicU64::new(0),
            voiced: AtomicU64::new(0),
            recv: AtomicU64::new(0),
            decoded: AtomicU64::new(0),
            e2ee_out: AtomicU64::new(0),
            e2ee_in: AtomicU64::new(0),
            video_in: AtomicU64::new(0),
            video_out: AtomicU64::new(0),
            video_acked: AtomicU64::new(0),
            video_lost: AtomicU32::new(0),
            mic: AtomicI32::new(MIC_UNKNOWN),
            ptt: AtomicBool::new(false),
            ptt_delay_ms: AtomicU32::new(20),
            in_gain: AtomicU32::new(1.0f32.to_bits()),
            out_gain: AtomicU32::new(1.0f32.to_bits()),
            auto_vad: AtomicBool::new(true),
            peers: Mutex::default(),
            noise: AtomicU8::new(super::dsp::NOISE_KRISP),
            echo: AtomicBool::new(true),
            agc: AtomicBool::new(true),
            adv_vad: AtomicBool::new(true),
            open_mic: AtomicBool::new(false),
            atten: AtomicU32::new(0),
            atten_self: AtomicBool::new(false),
            atten_others: AtomicBool::new(true),
            talking: AtomicBool::new(false),
            others_ms: AtomicU64::new(0),
            dev_gen: AtomicU32::new(0),
            qos: AtomicBool::new(false),
            silence_warn: AtomicBool::new(true),
            aec_dump: AtomicBool::new(false),
            no_audio: AtomicBool::new(false),
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

pub fn pick_in(host: &cpal::Host, want: Option<String>) -> Option<cpal::Device> {
    want.and_then(|n| host.input_devices().ok()?.find(|d| name(d) == n))
        .or_else(|| for_calls(host, false))
        .or_else(|| host.default_input_device())
}

pub fn pick_out(host: &cpal::Host, want: Option<String>) -> Option<cpal::Device> {
    want.and_then(|n| host.output_devices().ok()?.find(|d| name(d) == n))
        .or_else(|| for_calls(host, true))
        .or_else(|| host.default_output_device())
}

// cpal streams aren't Send everywhere, so they live and die on this thread
fn audio_thread(shared: Arc<Shared>, out: mpsc::UnboundedSender<Out>, cons: Vec<Consumer<f32>>, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();
    // test runs shouldn't make noise; decoding still happens, only the speakers stay closed
    let silent = std::env::var_os("RUSTCORD_SILENT").is_some();
    let fake = std::env::var_os("RUSTCORD_FAKE_MIC").is_some();
    let (far_p, far_c) = RingBuffer::<f32>::new(RING);
    let mix = Arc::new(Mutex::new(Mix::new(cons, shared.clone(), Some(far_p))));
    let open = |host: &cpal::Host| {
        let (want_in, want_out) = shared.devices.lock().map(|d| d.clone()).unwrap_or_default();
        let play = pick_out(host, want_out).filter(|_| !silent).and_then(|d| playback(&d, &mix));
        let mic = if fake { None } else { pick_in(host, want_in).and_then(|d| capture(&d)) };
        shared.mic.store(if fake { MIC_FAKE } else if mic.is_some() { MIC_DEVICE } else { MIC_NONE }, Relaxed);
        super::log::line(&format!("audio in {:?} out {:?} mic {}", want_in_name(&shared), want_out_name(&shared), shared.mic.load(Relaxed)));
        (play, mic)
    };
    let mut seen = shared.dev_gen.load(Relaxed);
    let (mut play, mut mic) = open(&host);
    let duck = super::duck::start(shared.clone());

    let mut tx = Tx::new(shared.clone(), out, far_c);
    let started = Instant::now();
    let mut made = 0u64;
    let mut phase = 0f32;
    let mut buf: Vec<f32> = Vec::with_capacity(FRAME * 4);
    let mut raw: Vec<f32> = Vec::new();
    while !stop.load(Relaxed) {
        std::thread::park_timeout(Duration::from_millis(5));
        let now = shared.dev_gen.load(Relaxed);
        if now != seen {
            seen = now;
            drop(play.take());
            drop(mic.take());
            buf.clear();
            (play, mic) = open(&host);
        }
        match &mut mic {
            Some(m) => pull(m, &mut raw, &mut buf),
            None if fake => {
                // 440Hz bursts, a second on and a second off, so VAD has edges to find
                let due = started.elapsed().as_micros() as u64 * 48 / 1000;
                while made < due {
                    let on = (made / 48_000) % 2 == 0;
                    buf.push(if on { 0.25 * phase.sin() } else { 0.0 });
                    phase = (phase + TAU * 440.0 / 48_000.0) % TAU;
                    made += 1;
                }
            }
            // nothing to send; the connection task already pushed its silence frames
            None => {}
        }
        while buf.len() >= FRAME {
            tx.frame(&buf[..FRAME]);
            buf.drain(..FRAME);
        }
    }
    drop(duck);
    drop(play);
}

struct Tx {
    shared: Arc<Shared>,
    out: mpsc::UnboundedSender<Out>,
    enc: Option<opus_rs::OpusEncoder>,
    chain: Chain,
    gate: Gate,
    talking: bool,
    tail: u8,
    stereo: Vec<f32>,
    pkt: [u8; 1276],
    dead: u32,
    dump: std::collections::VecDeque<i16>,
}

impl Drop for Tx {
    fn drop(&mut self) {
        self.save_dump();
    }
}

impl Tx {
    fn save_dump(&mut self) {
        if !self.dump.is_empty() {
            let (a, b) = self.dump.as_slices();
            super::log::wav(&[a, b].concat());
            self.dump.clear();
        }
    }

    fn new(shared: Arc<Shared>, out: mpsc::UnboundedSender<Out>, far: Consumer<f32>) -> Tx {
        let enc = opus_rs::OpusEncoder::new(48_000, 2, opus_rs::Application::Voip).ok().map(|mut e| {
            e.bitrate_bps = 64_000;
            e
        });
        Tx {
            chain: Chain::new(shared.clone(), Some(far)),
            gate: Gate::new(true),
            shared,
            out,
            enc,
            talking: false,
            tail: 0,
            stereo: vec![0.0; FRAME * 2],
            pkt: [0; 1276],
            dead: 0,
            dump: Default::default(),
        }
    }

    fn frame(&mut self, pcm: &[f32]) {
        let mut pcm = pcm.to_vec();
        let speech = self.chain.run(&mut pcm);
        let pcm = pcm.as_slice();
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
        let db = 20.0 * (rms + 1e-9).log10();
        self.shared.level.store(db.max(-100.0) as i32, Relaxed);
        let muted = self.shared.mute.load(Relaxed) || self.shared.deaf.load(Relaxed) || crate::hotkeys::MUTE_HELD.load(Relaxed);
        let send = self.gate.open(&self.shared, db, speech, muted);
        self.shared.talking.store(send, Relaxed);
        if self.shared.aec_dump.load(Relaxed) {
            self.dump.extend(pcm.iter().map(|x| (x * 32767.0) as i16));
            while self.dump.len() > 48_000 * 300 {
                self.dump.drain(..FRAME);
            }
        } else {
            self.save_dump();
        }
        if muted || db > -99.0 {
            self.dead = 0;
        } else {
            self.dead += 1;
            if self.dead == 250 && self.shared.silence_warn.load(Relaxed) {
                self.shared.no_audio.store(true, Relaxed);
            }
        }
        if send {
            let Some(enc) = &mut self.enc else { return };
            crate::clips::mic(pcm);
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

/// Windows' default device for calls, its "communications" role, which is where voice goes when
/// nothing was picked. cpal only knows the general default, often a monitor or the speakers.
fn for_calls(host: &cpal::Host, output: bool) -> Option<cpal::Device> {
    use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eRender};
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    // listing first also has cpal set up COM on this thread
    let mut list = if output { host.output_devices().ok()? } else { host.input_devices().ok()? };
    let id = unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let d = en.GetDefaultAudioEndpoint(if output { eRender } else { eCapture }, eCommunications).ok()?;
        d.GetId().ok()?.to_string().ok()?
    };
    list.find(|d| d.id().is_ok_and(|x| x.id() == id))
}

fn name(d: &cpal::Device) -> String {
    d.description().map(|d| d.name().to_string()).unwrap_or_default()
}

/// Input and output device names, for the settings page. Slow-ish on WASAPI, call it off the UI thread.
pub fn devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    let ins = host.input_devices().map(|it| it.map(|d| name(&d)).collect()).unwrap_or_default();
    let outs = host.output_devices().map(|it| it.map(|d| name(&d)).collect()).unwrap_or_default();
    (ins, outs)
}

pub type Mic = (cpal::Stream, Consumer<f32>, Lerp<1>);

/// Whatever the mic has gathered since last time, resampled to 48k, onto the end of `buf`.
pub fn pull(mic: &mut Mic, raw: &mut Vec<f32>, buf: &mut Vec<f32>) {
    let (_, cons, lerp) = mic;
    raw.clear();
    if let Ok(chunk) = cons.read_chunk(cons.slots()) {
        let (a, b) = chunk.as_slices();
        raw.extend_from_slice(a);
        raw.extend_from_slice(b);
        chunk.commit_all();
    }
    for &x in raw.iter() {
        lerp.push([x], |[y]| buf.push(y));
    }
}

pub fn capture(dev: &cpal::Device) -> Option<Mic> {
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

pub fn playback(dev: &cpal::Device, mix: &Arc<Mutex<Mix>>) -> Option<cpal::Stream> {
    let cfg = dev.default_output_config().ok()?;
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return None;
    }
    let conf = cfg.config();
    mix.lock().ok()?.reconfigure(conf.sample_rate, conf.channels as usize);
    let m = mix.clone();
    let stream = dev
        .build_output_stream::<f32, _, _>(
            conf,
            move |data, _| match m.try_lock() {
                Ok(mut m) => m.fill(data),
                Err(_) => data.fill(0.0),
            },
            |_| {},
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some(stream)
}

pub struct Mix {
    slots: Vec<Consumer<f32>>,
    primed: Vec<bool>,
    buf: Vec<f32>,
    lerp: Lerp<2>,
    ch: usize,
    shared: Arc<Shared>,
    // what was played, mono at 48k, for the echo canceller
    reference: Option<Producer<f32>>,
}

impl Mix {
    pub fn new(slots: Vec<Consumer<f32>>, shared: Arc<Shared>, reference: Option<Producer<f32>>) -> Mix {
        Mix {
            primed: vec![false; slots.len()],
            slots,
            buf: vec![0.0; 48_000 * 2],
            lerp: Lerp::new(1.0),
            ch: 2,
            shared,
            reference,
        }
    }

    fn reconfigure(&mut self, rate: u32, ch: usize) {
        self.lerp = Lerp::new(48_000.0 / rate as f64);
        self.ch = ch;
        self.primed.fill(false);
    }

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
        let gain = f32::from_bits(self.shared.out_gain.load(Relaxed));
        if (gain - 1.0).abs() > 0.001 {
            mix.iter_mut().for_each(|x| *x *= gain);
        }
        if mix.iter().any(|x| x.abs() > 0.01) {
            self.shared.others_ms.store(now_ms(), Relaxed);
        }
        if let Some(r) = &mut self.reference {
            for f in mix.chunks_exact(2) {
                if r.push((f[0] + f[1]) * 0.5).is_err() {
                    break;
                }
            }
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
pub struct Lerp<const N: usize> {
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
