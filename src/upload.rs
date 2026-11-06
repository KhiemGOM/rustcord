//! Uploads in flight: how far each message's files have gone, and the way to stop them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};

pub struct Job {
    pub files: Vec<(String, u64)>,
    pub sent: AtomicU64,
    pub cancel: AtomicBool,
}

impl Job {
    pub fn total(&self) -> u64 {
        self.files.iter().map(|f| f.1).sum()
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }
}

fn jobs() -> &'static Mutex<HashMap<String, Arc<Job>>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Arc<Job>>>> = OnceLock::new();
    JOBS.get_or_init(Default::default)
}

pub fn start(nonce: &str, files: &[(PathBuf, String, String)]) -> Arc<Job> {
    let files = files.iter().map(|(p, name, _)| (name.clone(), std::fs::metadata(p).map_or(0, |m| m.len()))).collect();
    let job = Arc::new(Job { files, sent: AtomicU64::new(0), cancel: AtomicBool::new(false) });
    jobs().lock().unwrap().insert(nonce.to_string(), job.clone());
    job
}

pub fn get(nonce: &str) -> Option<Arc<Job>> {
    jobs().lock().unwrap().get(nonce).cloned()
}

pub fn cancel(nonce: &str) {
    if let Some(j) = get(nonce) {
        j.cancel.store(true, Relaxed);
    }
}

pub fn finish(nonce: &str) {
    jobs().lock().unwrap().remove(nonce);
}

/// The most one file may be: 10 MB, 50 MB with Nitro Basic or Classic or a tier 2 server, 100 MB
/// in a tier 3 server, 500 MB with Nitro.
pub fn max_bytes(premium: u64, tier: u8) -> u64 {
    let mb = |n: u64| n << 20;
    let own = match premium {
        2 => mb(500),
        1 | 3 => mb(50),
        _ => mb(10),
    };
    own.max(match tier {
        3 => mb(100),
        2 => mb(50),
        _ => 0,
    })
}

pub fn size_text(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        let mb = bytes as f64 / (1u64 << 20) as f64;
        if mb.fract() == 0.0 { format!("{mb:.0} MB") } else { format!("{mb:.1} MB") }
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

pub fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("txt" | "log") => "text/plain",
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{max_bytes, size_text};

    #[test]
    fn limits_follow_nitro_and_boosts() {
        assert_eq!(max_bytes(0, 0), 10 << 20);
        assert_eq!(max_bytes(0, 2), 50 << 20);
        assert_eq!(max_bytes(0, 3), 100 << 20);
        assert_eq!(max_bytes(2, 3), 500 << 20);
        assert_eq!(max_bytes(3, 3), 100 << 20);
    }

    #[test]
    fn sizes_read_like_discords() {
        assert_eq!(size_text(10 << 20), "10 MB");
        assert_eq!(size_text(4519), "5 KB");
        assert_eq!(size_text(1_500_000), "1.4 MB");
    }
}
