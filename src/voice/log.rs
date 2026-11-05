//! Debug logging and the diagnostic audio recording, both in the data folder.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

static ON: AtomicBool = AtomicBool::new(false);

pub fn set(on: bool) {
    ON.store(on, Relaxed);
}

pub fn line(text: &str) {
    if !ON.load(Relaxed) {
        return;
    }
    let _ = std::fs::create_dir_all(crate::cache::dir());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::cache::dir().join("voice.log")) {
        let _ = writeln!(f, "{} {text}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
    }
}

/// Mono 48 kHz, 16 bit.
pub fn wav(samples: &[i16]) {
    let bytes = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&96_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    let _ = std::fs::create_dir_all(crate::cache::dir());
    let _ = std::fs::write(crate::cache::dir().join(format!("voice-{}.wav", chrono::Local::now().format("%Y%m%d-%H%M%S"))), out);
}
