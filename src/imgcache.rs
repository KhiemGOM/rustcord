use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
use egui::mutex::Mutex;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Poll;
use tokio::runtime::Runtime;

type Entry = Poll<Result<(Arc<[u8]>, String), String>>;

// once decoded, egui caches the texture and never asks for the bytes again, so the raw bytes
// we hold go cold. keep at most this much of them in memory; a cold-evicted image is re-read
// from the disk cache if it's ever needed again.
const MEM_CAP: usize = 128 << 20;

struct Slot {
    entry: Entry,
    // a monotonic tick bumped on every access, for least-recently-used eviction
    used: u64,
}

pub struct DiskLoader {
    cache: Arc<Mutex<HashMap<String, Slot>>>,
    clock: AtomicU64,
    dir: PathBuf,
    rt: Arc<Runtime>,
    http: reqwest::Client,
    limit: Arc<tokio::sync::Semaphore>,
}

/// Drops the coldest decoded byte buffers until the in-memory total is back under the cap.
/// Pending and errored slots carry no bytes, so they're left alone.
fn evict(cache: &mut HashMap<String, Slot>) {
    let mut total: usize = cache.values().map(|s| match &s.entry {
        Poll::Ready(Ok((b, _))) => b.len(),
        _ => 0,
    }).sum();
    if total <= MEM_CAP {
        return;
    }
    let mut warm: Vec<(u64, usize, String)> = cache
        .iter()
        .filter_map(|(k, s)| match &s.entry {
            Poll::Ready(Ok((b, _))) => Some((s.used, b.len(), k.clone())),
            _ => None,
        })
        .collect();
    warm.sort_by_key(|w| w.0);
    for (_, len, k) in warm {
        if total <= MEM_CAP {
            break;
        }
        cache.remove(&k);
        total = total.saturating_sub(len);
    }
}

/// Drop the oldest cached image files once the folder grows past `cap` bytes, so a long-lived
/// install doesn't leave hundreds of megabytes of avatars and emoji on disk. Cheap: one stat
/// per file, and it only deletes when actually over.
fn prune(dir: &std::path::Path, cap: u64) {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= cap {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, len, path) in &files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(*len);
        }
    }
}

impl DiskLoader {
    pub fn new(dir: PathBuf, rt: Arc<Runtime>) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let d = dir.clone();
        rt.spawn(async move { prune(&d, 256 << 20) });
        DiskLoader {
            cache: Default::default(),
            clock: AtomicU64::new(0),
            dir,
            rt,
            http: crate::api::http(),
            limit: Arc::new(tokio::sync::Semaphore::new(12)),
        }
    }
}

fn sniff(b: &[u8]) -> &'static str {
    match b {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xff, 0xd8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

fn key(uri: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    uri.hash(&mut h);
    format!("{:016x}", h.finish())
}

impl BytesLoader for DiskLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(DiskLoader)
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return Err(LoadError::NotSupported);
        }
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        let mut cache = self.cache.lock();
        if let Some(s) = cache.get_mut(uri) {
            s.used = tick;
            return match &s.entry {
                Poll::Ready(Ok((b, mime))) => Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(b.clone()),
                    mime: Some(mime.clone()),
                }),
                Poll::Ready(Err(e)) => Err(LoadError::Loading(e.clone())),
                Poll::Pending => Ok(BytesPoll::Pending { size: None }),
            };
        }
        cache.insert(uri.to_string(), Slot { entry: Poll::Pending, used: tick });
        drop(cache);

        let uri = uri.to_string();
        let path = self.dir.join(key(&uri));
        let cache = self.cache.clone();
        let http = self.http.clone();
        let limit = self.limit.clone();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let res = match tokio::fs::read(&path).await {
                Ok(b) if !b.is_empty() => Ok(b),
                _ => {
                    let _permit = limit.acquire().await;
                    fetch(&http, &uri).await.inspect(|b| {
                        let tmp = path.with_extension("part");
                        if std::fs::write(&tmp, b).is_ok() {
                            let _ = std::fs::rename(&tmp, &path);
                        }
                    })
                }
            };
            let entry = res.map(|b| {
                let mime = sniff(&b).to_string();
                (Arc::<[u8]>::from(b), mime)
            });
            let keep = {
                let mut c = cache.lock();
                match c.get_mut(&uri) {
                    Some(s) => {
                        s.entry = Poll::Ready(entry);
                        evict(&mut c);
                        true
                    }
                    None => false,
                }
            };
            if keep {
                ctx.request_repaint();
            }
        });
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values()
            .map(|s| match &s.entry {
                Poll::Ready(Ok((b, _))) => b.len(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().values().any(|s| s.entry.is_pending())
    }
}

async fn fetch(http: &reqwest::Client, uri: &str) -> Result<Vec<u8>, String> {
    let r = http.get(uri).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("{} {}", r.status(), uri));
    }
    r.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
}
