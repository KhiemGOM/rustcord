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
const MEM_CAP: usize = 4 << 20;

/// egui's texture loader, noting when each image was last drawn. egui keeps every texture it
/// ever made, and the decoded pixels and bytes behind it, so a long session's memory only grew.
/// An animation gets one texture whose pixels change with the frame, not one per frame.
#[derive(Default)]
pub struct Textures {
    inner: egui::load::DefaultTextureLoader,
    // when each was last drawn, and whether the painter has been told it can be loaded again
    used: Mutex<HashMap<String, (std::time::Instant, bool)>>,
    anims: Mutex<HashMap<String, (egui::TextureHandle, Arc<egui::ColorImage>)>>,
    stills: Mutex<HashMap<(String, egui::TextureOptions), Still>>,
    jobs: Jobs,
    // the painter's count of lost devices when last looked at
    resets: std::sync::atomic::AtomicU32,
}

/// An image off the network as a texture. Its bytes are decoded here, not by egui's loader: a
/// thread each, but only so many pixels at once across them, and scaled down before they're a
/// texture when they come at under four fifths of what their place needs. A nameplate is 448x84
/// for a 224x42 row, and a link's preview once came 6920x2442 for a thumbnail, 66 MB of pixels.
struct Still {
    handle: egui::TextureHandle,
    source: egui::Vec2,
    // the pixels it came in, and how far down it was scaled (1 for not at all)
    full: [usize; 2],
    scale: f32,
    // the box the proxy was asked for it in, 0 for as it comes
    asked: u32,
}

/// A still on its way to being a texture.
struct Job {
    // tells it from an earlier one for the same picture, whose thread may still be at it
    id: u64,
    // what's fetched: the uri, or the proxy's version of it in a box
    from: String,
    asked: u32,
    state: State,
}

enum State {
    Fetching,
    Decoding,
    Decoded(Arc<egui::ColorImage>, [usize; 2], f32),
    Failed(String),
}

type Jobs = Arc<Mutex<HashMap<(String, egui::TextureOptions), Job>>>;

// the pixels being decoded at once, across every thread at it; past this one waits its turn
const DECODING: usize = 24 << 20;
static IN_FLIGHT: (std::sync::Mutex<usize>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());

struct Turn(usize);

impl Drop for Turn {
    fn drop(&mut self) {
        *IN_FLIGHT.0.lock().unwrap() -= self.0;
        IN_FLIGHT.1.notify_all();
    }
}

/// The picture in `bytes`, the size it came in, and how far it was scaled down (1 for not at
/// all) to cover `want` pixels whichever way they crop it.
fn decode(bytes: &[u8], want: Option<[u32; 2]>) -> Result<(egui::ColorImage, [usize; 2], f32), String> {
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?.into_dimensions().map_err(|e| e.to_string())?;
    // its pixels, and once more while they change form
    let turn = Turn(w as usize * h as usize * 8);
    let mut busy = IN_FLIGHT.0.lock().unwrap();
    while *busy > 0 && *busy + turn.0 > DECODING {
        busy = IN_FLIGHT.1.wait(busy).unwrap();
    }
    *busy += turn.0;
    drop(busy);
    let mut rgba = image::load_from_memory(bytes).map_err(|e| e.to_string())?.into_rgba8();
    let full = [w as usize, h as usize];
    let scale = want.map_or(1.0, |[ww, wh]| (ww as f32 / w as f32).max(wh as f32 / h as f32)).min(1.0);
    if scale >= 0.8 {
        return Ok((egui::ColorImage::from_rgba_unmultiplied(full, &rgba), full, 1.0));
    }
    // premultiplied first, so transparent edges don't darken as they're averaged
    for p in rgba.pixels_mut() {
        let a = p[3] as u32;
        for c in &mut p.0[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
    let (nw, nh) = (((w as f32 * scale).ceil() as u32).max(1), ((h as f32 * scale).ceil() as u32).max(1));
    let small = image::imageops::thumbnail(&rgba, nw, nh);
    let mut out = egui::ColorImage::from_rgba_premultiplied([nw as usize, nh as usize], small.as_raw());
    out.source_size = egui::vec2(w as f32, h as f32);
    Ok((out, full, scale))
}

/// `uri` as Discord's image proxy serves it inside a square `side` pixels wide (it keeps the
/// picture's shape and never enlarges), when it is the proxy's and nothing asked a size of it yet.
/// What a site links can be any size, and it is decoded whole before anything can scale it down.
fn boxed(uri: &str, side: u32) -> Option<String> {
    let path = uri.split('?').next().unwrap_or(uri);
    let proxy = uri.starts_with("https://media.discordapp.net/") || uri.starts_with("https://images-ext-") && path.contains(".discordapp.net/");
    (proxy && !uri.contains("width=") && ![".mp4", ".webm", ".mov"].iter().any(|v| path.ends_with(v)))
        .then(|| format!("{uri}{}width={side}&height={side}", if uri.contains('?') { '&' } else { '?' }))
}

/// The bytes behind `uri` go from memory: whoever needed them has them.
fn spent(ctx: &egui::Context, uri: &str) {
    for loader in ctx.loaders().bytes.lock().iter() {
        loader.forget(uri);
    }
}

impl Textures {
    /// What hasn't been drawn for `age`, or `anim_age` for an animation's frames: those, and an
    /// animation's own uri once none of its frames has been, so its decoded frames go too.
    pub fn stale(&self, age: std::time::Duration, anim_age: std::time::Duration) -> Vec<String> {
        let used = self.used.lock();
        let old = |u: &str, t: &(std::time::Instant, bool)| t.0.elapsed() >= if u.contains(crate::gif::SEP) { anim_age } else { age };
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
        // and what's being fetched for them under another name
        let boxed: Vec<String> = self.jobs.lock().iter().filter(|(k, j)| j.from != k.0 && out.contains(&k.0)).map(|(_, j)| j.from.clone()).collect();
        out.extend(boxed);
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

    fn still(&self, ctx: &egui::Context, uri: &str, options: egui::TextureOptions, size: egui::SizeHint) -> egui::load::TextureLoadResult {
        use egui::load::{BytesPoll, LoadError, SizedTexture, TexturePoll};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let want = match size {
            egui::SizeHint::Size { width, height, .. } => Some([width, height]),
            _ => None,
        };
        // enough of it to cover where it's drawn, whichever way that crops it
        let need = |[w, h]: [usize; 2]| want.map_or(1.0, |[ww, wh]| (ww as f32 / w as f32).max(wh as f32 / h as f32)).min(1.0);
        // twice what's drawn, as the chat asks for its own: sharp where a wide picture fills a square
        let side = want.map_or(0, |[w, h]| 2 * w.max(h));
        let key = (uri.to_string(), options);
        let made = self.stills.lock().get(&key).map(|s| (SizedTexture::new(s.handle.id(), s.source), s.scale >= need(s.full) && (s.asked == 0 || side != 0 && side <= s.asked)));
        if let Some((texture, true)) = made {
            return Ok(TexturePoll::Ready { texture });
        }
        // not made yet, or drawn bigger somewhere new: what there is shows meanwhile
        let shown = made.map(|(texture, _)| TexturePoll::Ready { texture });
        let mut jobs = self.jobs.lock();
        // one box per picture while it loads; drawn bigger than that, it starts again in a bigger one
        if jobs.get(&key).is_none_or(|j| j.asked != 0 && (side == 0 || side > j.asked)) {
            if let Some(old) = jobs.remove(&key) {
                spent(ctx, &old.from);
            }
            let from = Some(side).filter(|s| *s > 0).and_then(|s| boxed(uri, s));
            let asked = if from.is_some() { side } else { 0 };
            jobs.insert(key.clone(), Job { id: NEXT.fetch_add(1, Ordering::Relaxed), from: from.unwrap_or_else(|| uri.to_string()), asked, state: State::Fetching });
        }
        let Some(job) = jobs.get_mut(&key) else { return Ok(TexturePoll::Pending { size: None }) };
        match &job.state {
            State::Fetching => match ctx.try_load_bytes(&job.from) {
                Ok(BytesPoll::Ready { bytes, .. }) => {
                    job.state = State::Decoding;
                    spent(ctx, &job.from);
                    let (jobs, ctx, key, id) = (self.jobs.clone(), ctx.clone(), key, job.id);
                    std::thread::spawn(move || {
                        let state = match decode(bytes.as_ref(), want) {
                            Ok((image, full, scale)) => State::Decoded(Arc::new(image), full, scale),
                            Err(e) => State::Failed(e),
                        };
                        // forgotten or started over meanwhile, it isn't wanted
                        if let Some(job) = jobs.lock().get_mut(&key).filter(|j| j.id == id) {
                            job.state = state;
                        }
                        ctx.request_repaint();
                    });
                }
                Ok(BytesPoll::Pending { .. }) => {}
                Err(e) => {
                    let msg = e.to_string();
                    job.state = State::Failed(msg.clone());
                    return shown.map_or_else(|| Err(LoadError::Loading(msg)), Ok);
                }
            },
            State::Decoding => {}
            State::Failed(e) => return shown.map_or_else(|| Err(LoadError::Loading(e.clone())), Ok),
            State::Decoded(..) => {
                let Some(Job { asked, state: State::Decoded(image, full, scale), .. }) = jobs.remove(&key) else { return Ok(TexturePoll::Pending { size: None }) };
                drop(jobs);
                let source = image.source_size;
                let handle = ctx.load_texture(uri, image, options);
                eframe::reloadable(handle.id());
                let texture = SizedTexture::new(handle.id(), source);
                self.stills.lock().insert(key, Still { handle, source, full, scale, asked });
                return Ok(TexturePoll::Ready { texture });
            }
        }
        Ok(shown.unwrap_or(TexturePoll::Pending { size: None }))
    }

    /// After a lost device the painter has none of what was made here, having kept no copy:
    /// those textures go, and are loaded again as they're next drawn.
    fn after_reset(&self) {
        let resets = eframe::device_resets();
        if self.resets.swap(resets, Ordering::Relaxed) == resets {
            return;
        }
        for (uri, t) in self.used.lock().iter_mut().filter(|(_, t)| t.1) {
            t.1 = false;
            egui::load::TextureLoader::forget(&self.inner, uri);
        }
        self.anims.lock().clear();
        self.stills.lock().clear();
    }
}

impl egui::load::TextureLoader for Textures {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Textures)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, options: egui::TextureOptions, size: egui::SizeHint) -> egui::load::TextureLoadResult {
        self.after_reset();
        let now = std::time::Instant::now();
        let mut used = self.used.lock();
        let told = match used.get_mut(uri) {
            Some(t) => {
                t.0 = now;
                t.1
            }
            None => {
                used.insert(uri.to_string(), (now, false));
                false
            }
        };
        drop(used);
        let got = match uri.split_once(crate::gif::SEP) {
            Some((base, _)) => self.frame(ctx, uri, base, options, size),
            None if uri.starts_with("http") && !uri.contains(".svg") => self.still(ctx, uri, options, size),
            None => self.inner.load(ctx, uri, options, size),
        };
        if !told
            && !uri.starts_with("bytes://")
            && let Ok(egui::load::TexturePoll::Ready { texture }) = &got
        {
            eframe::reloadable(texture.id);
            if let Some(t) = self.used.lock().get_mut(uri) {
                t.1 = true;
            }
        }
        got
    }

    fn forget(&self, uri: &str) {
        self.used.lock().remove(uri);
        // a frame's texture is its animation's, which goes when the animation does
        if !uri.contains(crate::gif::SEP) {
            self.anims.lock().remove(uri);
            self.stills.lock().retain(|k, _| k.0 != uri);
            self.jobs.lock().retain(|k, _| k.0 != uri);
            self.inner.forget(uri);
        }
    }

    fn forget_all(&self) {
        self.used.lock().clear();
        self.anims.lock().clear();
        self.stills.lock().clear();
        self.jobs.lock().clear();
        self.inner.forget_all();
    }

    fn end_pass(&self, pass_index: u64) {
        self.inner.end_pass(pass_index);
    }

    fn byte_size(&self) -> usize {
        self.inner.byte_size() + self.anims.lock().values().map(|(_, i)| i.pixels.len() * 4).sum::<usize>() + self.stills.lock().values().map(|s| s.handle.byte_size()).sum::<usize>()
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
        t.used.lock().extend(
            [
                ("a".to_string(), old),
                ("b".to_string(), now),
                (format!("g{sep}0"), old),
                (format!("g{sep}1"), old),
                (format!("h{sep}0"), old),
                (format!("h{sep}1"), now),
            ]
            .map(|(u, t)| (u, (t, false))),
        );
        let mut stale = t.stale(Duration::from_secs(120), Duration::from_secs(120));
        stale.sort();
        assert_eq!(stale, ["a".to_string(), "g".into(), format!("g{sep}0"), format!("g{sep}1"), format!("h{sep}0")]);
    }

    #[test]
    fn only_a_proxy_picture_nobody_sized_is_asked_for_in_a_box() {
        let ext = "https://images-ext-1.discordapp.net/external/abc/https/site/a.png";
        assert_eq!(super::boxed(ext, 160), Some(format!("{ext}?width=160&height=160")));
        let media = "https://media.discordapp.net/attachments/1/2/a.png?ex=1";
        assert_eq!(super::boxed(media, 90), Some(format!("{media}&width=90&height=90")));
        for kept in [
            "https://media.discordapp.net/attachments/1/2/a.png?width=10&height=10",
            "https://media.discordapp.net/attachments/1/2/a.mp4",
            "https://cdn.discordapp.com/avatars/1/a.png?size=32",
            "http://127.0.0.1:1/cdn/attachments/1/2/a.png",
        ] {
            assert_eq!(super::boxed(kept, 90), None);
        }
    }

    #[test]
    fn failed_still_bytes_report_error_immediately() {
        use egui::load::{BytesLoadResult, BytesLoader, LoadError, TextureLoader as _};

        struct FailBytes;
        impl BytesLoader for FailBytes {
            fn id(&self) -> &str {
                "test::FailBytes"
            }
            fn load(&self, _ctx: &egui::Context, _uri: &str) -> BytesLoadResult {
                Err(LoadError::Loading("404 Not Found".into()))
            }
            fn forget(&self, _uri: &str) {}
            fn forget_all(&self) {}
            fn byte_size(&self) -> usize {
                0
            }
        }

        let ctx = egui::Context::default();
        ctx.add_bytes_loader(std::sync::Arc::new(FailBytes));
        let textures = super::Textures::default();
        let res = textures.load(&ctx, "https://cdn.discordapp.com/app-icons/1/bad.png?size=64", Default::default(), egui::SizeHint::default());
        assert!(res.is_err());
    }
}
