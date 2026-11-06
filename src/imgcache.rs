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
const MEM_CAP: usize = 16 << 20;

/// egui's texture loader, noting when each image was last drawn. egui keeps every texture it
/// ever made, and the decoded pixels and bytes behind it, so a long session's memory only grew.
/// An animation gets one texture whose pixels change with the frame, not one per frame.
#[derive(Default)]
pub struct Textures {
    inner: egui::load::DefaultTextureLoader,
    used: Mutex<HashMap<String, std::time::Instant>>,
    anims: Mutex<HashMap<String, (egui::TextureHandle, Arc<egui::ColorImage>)>>,
}

impl Textures {
    /// What hasn't been drawn for `age`, or `anim_age` for an animation's frames: those, and an
    /// animation's own uri once none of its frames has been, so its decoded frames go too.
    pub fn stale(&self, age: std::time::Duration, anim_age: std::time::Duration) -> Vec<String> {
        let used = self.used.lock();
        let old = |u: &str, t: &std::time::Instant| t.elapsed() >= if u.contains(crate::gif::SEP) { anim_age } else { age };
        let mut out: Vec<String> = used.iter().filter(|(u, t)| old(u, t)).map(|(u, _)| u.clone()).collect();
        let mut anims: Vec<&str> = out.iter().filter_map(|u| u.split_once(crate::gif::SEP).map(|(b, _)| b)).collect();
        anims.sort_unstable();
        anims.dedup();
        let anims: Vec<String> = anims
            .into_iter()
            .filter(|b| !used.iter().any(|(u, t)| u.split_once(crate::gif::SEP).is_some_and(|(x, _)| x == *b) && !old(u, t)))
            .map(str::to_string)
            .collect();
        out.extend(anims);
        out
    }

    fn frame(&self, ctx: &egui::Context, uri: &str, base: &str, options: egui::TextureOptions, size: egui::SizeHint) -> egui::load::TextureLoadResult {
        use egui::load::{ImagePoll, SizedTexture, TexturePoll};
        let image = match ctx.try_load_image(uri, size)? {
            ImagePoll::Ready { image } => image,
            // the next frame is still decoding: keep showing the last one
            ImagePoll::Pending { size } => {
                return Ok(match self.anims.lock().get(base) {
                    Some((h, shown)) => TexturePoll::Ready { texture: SizedTexture::new(h.id(), shown.source_size) },
                    None => TexturePoll::Pending { size },
                });
            }
        };
        let mut anims = self.anims.lock();
        let (h, shown) = anims.entry(base.to_string()).or_insert_with(|| (ctx.load_texture(base, image.clone(), options), image.clone()));
        if !Arc::ptr_eq(shown, &image) {
            h.set(image.clone(), options);
            *shown = image.clone();
        }
        Ok(TexturePoll::Ready { texture: SizedTexture::new(h.id(), image.source_size) })
    }
}

impl egui::load::TextureLoader for Textures {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Textures)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, options: egui::TextureOptions, size: egui::SizeHint) -> egui::load::TextureLoadResult {
        let now = std::time::Instant::now();
        let mut used = self.used.lock();
        match used.get_mut(uri) {
            Some(t) => *t = now,
            None => {
                used.insert(uri.to_string(), now);
            }
        }
        drop(used);
        match uri.split_once(crate::gif::SEP) {
            Some((base, _)) => self.frame(ctx, uri, base, options, size),
            None => self.inner.load(ctx, uri, options, size),
        }
    }

    fn forget(&self, uri: &str) {
        self.used.lock().remove(uri);
        // a frame's texture is its animation's, which goes when the animation does
        if !uri.contains(crate::gif::SEP) {
            self.anims.lock().remove(uri);
            self.inner.forget(uri);
        }
    }

    fn forget_all(&self) {
        self.used.lock().clear();
        self.anims.lock().clear();
        self.inner.forget_all();
    }

    fn end_pass(&self, pass_index: u64) {
        self.inner.end_pass(pass_index);
    }

    fn byte_size(&self) -> usize {
        self.inner.byte_size() + self.anims.lock().values().map(|(_, i)| i.pixels.len() * 4).sum::<usize>()
    }
}

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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    #[test]
    fn an_animation_goes_once_none_of_its_frames_is_drawn() {
        let t = super::Textures::default();
        let (old, now, sep) = (Instant::now() - Duration::from_secs(300), Instant::now(), crate::gif::SEP);
        t.used.lock().extend([
            ("a".to_string(), old),
            ("b".to_string(), now),
            (format!("g{sep}0"), old),
            (format!("g{sep}1"), old),
            (format!("h{sep}0"), old),
            (format!("h{sep}1"), now),
        ]);
        let mut stale = t.stale(Duration::from_secs(120), Duration::from_secs(120));
        stale.sort();
        assert_eq!(stale, ["a".to_string(), "g".into(), format!("g{sep}0"), format!("g{sep}1"), format!("h{sep}0")]);
    }
}
