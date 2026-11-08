//! Fails a run that stops drawing: the UI thread can't report its own hang, so a second thread watches the frame clock.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

const STALL: Duration = Duration::from_secs(60);

static BEAT: AtomicU64 = AtomicU64::new(0);
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn now() -> u64 {
    START.get().map_or(0, |t| t.elapsed().as_millis() as u64)
}

pub fn beat() {
    BEAT.store(now(), Relaxed);
}

pub fn start(dir: PathBuf) {
    START.get_or_init(Instant::now);
    beat();
    let _ = std::thread::Builder::new().name("e2e-watchdog".into()).spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let quiet = now().saturating_sub(BEAT.load(Relaxed));
            if quiet < STALL.as_millis() as u64 {
                continue;
            }
            if dir.join("results.json").exists() {
                return;
            }
            let at = std::fs::read_to_string(dir.join("progress")).unwrap_or_default();
            let why = format!("ui thread hung: no frame for {}s after step {}", quiet / 1000, at.trim());
            let out = serde_json::json!({ "ok": false, "error": why });
            let _ = std::fs::write(dir.join("results.json"), out.to_string());
            std::process::exit(3);
        }
    });
}
