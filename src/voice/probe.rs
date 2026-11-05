//! The input meter and Mic Test on Voice & Video. A thread opens the mic the way a call would,
//! runs it through the same chain and gate, and keeps going only while the page keeps asking.
//! Mic Test plays what the chain made back through the speakers, as Discord's does.

use rtrb::RingBuffer;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::audio::{self, FRAME, Mix, Shared, now_ms};
use super::dsp::{Chain, Gate};

pub struct Reading {
    pub level: i32,
    pub speaking: bool,
    pub has_mic: bool,
}

struct Mon {
    shared: Arc<Shared>,
    touched: AtomicU64,
    speaking: AtomicBool,
    has_mic: AtomicBool,
}

static MON: Mutex<Option<Arc<Mon>>> = Mutex::new(None);
static TESTING: AtomicBool = AtomicBool::new(false);

pub fn testing() -> bool {
    TESTING.load(Relaxed)
}

pub fn set_testing(on: bool) {
    TESTING.store(on, Relaxed);
}

/// Called every frame the page is on screen: keeps the mic open and says what it hears.
pub fn watch(s: &crate::cache::Settings) -> Reading {
    let Ok(mut slot) = MON.lock() else { return Reading { level: -100, speaking: false, has_mic: false } };
    let mon = match slot.as_ref() {
        Some(m) => m.clone(),
        None => {
            let mon = Arc::new(Mon {
                shared: Arc::new(Shared::default()),
                touched: AtomicU64::new(now_ms()),
                speaking: AtomicBool::new(false),
                has_mic: AtomicBool::new(false),
            });
            let m = mon.clone();
            if std::thread::Builder::new().name("mic-probe".into()).spawn(move || run(m)).is_err() {
                return Reading { level: -100, speaking: false, has_mic: false };
            }
            *slot = Some(mon.clone());
            mon
        }
    };
    mon.touched.store(now_ms(), Relaxed);
    mon.shared.apply(s);
    Reading { level: mon.shared.level.load(Relaxed), speaking: mon.speaking.load(Relaxed), has_mic: mon.has_mic.load(Relaxed) }
}

fn run(mon: Arc<Mon>) {
    let host = cpal::default_host();
    let shared = mon.shared.clone();
    let (far_p, far_c) = RingBuffer::<f32>::new(48_000);
    let (mut slot, slot_c) = RingBuffer::<f32>::new(48_000);
    let mix = Arc::new(Mutex::new(Mix::new(vec![slot_c], shared.clone(), Some(far_p))));
    let mut chain = Chain::new(shared.clone(), Some(far_c));
    let mut gate = Gate::new(false);
    let (mut seen, mut was) = (u32::MAX, false);
    let (mut mic, mut play) = (None, None);
    let (mut raw, mut buf) = (Vec::new(), Vec::new());
    while now_ms().saturating_sub(mon.touched.load(Relaxed)) < 700 {
        std::thread::park_timeout(Duration::from_millis(5));
        let (gen_now, test) = (shared.dev_gen.load(Relaxed), testing());
        if gen_now != seen {
            seen = gen_now;
            was = false;
            drop(play.take());
            drop(mic.take());
            buf.clear();
            let (want_in, _) = shared.devices.lock().map(|d| d.clone()).unwrap_or_default();
            mic = audio::pick_in(&host, want_in).and_then(|d| audio::capture(&d));
            mon.has_mic.store(mic.is_some(), Relaxed);
        }
        if test != was {
            was = test;
            play = None;
            if test {
                let (_, want_out) = shared.devices.lock().map(|d| d.clone()).unwrap_or_default();
                play = audio::pick_out(&host, want_out).and_then(|d| audio::playback(&d, &mix));
            }
        }
        let Some(m) = &mut mic else {
            shared.level.store(-100, Relaxed);
            continue;
        };
        audio::pull(m, &mut raw, &mut buf);
        while buf.len() >= FRAME {
            let mut f = buf[..FRAME].to_vec();
            buf.drain(..FRAME);
            let speech = chain.run(&mut f);
            let rms = (f.iter().map(|x| x * x).sum::<f32>() / FRAME as f32).sqrt();
            let db = 20.0 * (rms + 1e-9).log10();
            shared.level.store(db.max(-100.0) as i32, Relaxed);
            mon.speaking.store(gate.open(&shared, db, speech, false), Relaxed);
            if test {
                for x in f {
                    let _ = slot.push(x);
                    let _ = slot.push(x);
                }
            }
        }
    }
    set_testing(false);
    if let Ok(mut s) = MON.lock() {
        *s = None;
    }
}
