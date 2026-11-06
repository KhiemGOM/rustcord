//! Animated gifs, animated webp and gifv mp4 clips, decoded off the UI thread.
//! egui_extras' loader decodes every frame inline the first time an image is
//! drawn, which froze the window for ~0.4s when the GIF picker filled up.
//! Registered after egui_extras' loaders, so it's asked first.
//!
//! egui only animates a uri that ends in .gif or .webp, which no cdn url with a
//! query string does, so everything is drawn through `uri()` instead: it looks at
//! the downloaded bytes and picks the frame itself.

use egui::load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::mutex::Mutex;
use egui::{ColorImage, FrameDurations, Id, decode_animated_image_uri, has_gif_magic_header};
use image::AnimationDecoder as _;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::task::Poll;
use std::time::Duration;

// pixels kept per animation; longer gifs get cut short, mp4s get thinned out
const BUDGET: usize = 96 << 20;
// egui's own frame uris use '#', and it tacks another one onto anything ending in .gif
pub const SEP: char = '\u{1f}';

#[derive(Clone, Copy)]
enum Kind {
    Gif,
    Webp,
    Mp4,
    Apng,
}

fn kind(b: &[u8]) -> Option<Kind> {
    if has_gif_magic_header(b) {
        Some(Kind::Gif)
    } else if b.len() > 21 && &b[..4] == b"RIFF" && &b[8..16] == b"WEBPVP8X" && b[20] & 2 != 0 {
        Some(Kind::Webp)
    } else if crate::mp4::is_mp4(b) {
        Some(Kind::Mp4)
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) && b[..b.len().min(4096)].windows(4).any(|w| w == b"acTL") {
        // an animated png says so in a chunk ahead of its image data
        Some(Kind::Apng)
    } else {
        None
    }
}

struct Anim {
    frames: Vec<Arc<ColorImage>>,
}

type Entry = Poll<Result<Arc<Anim>, String>>;

#[derive(Default)]
pub struct GifLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
}

fn collect(frames: impl Iterator<Item = image::ImageResult<image::Frame>>) -> Result<(Vec<Arc<ColorImage>>, Vec<Duration>), String> {
    let mut out = Vec::new();
    let mut durations = Vec::new();
    let mut bytes = 0;
    for f in frames {
        let f = f.map_err(|e| e.to_string())?;
        let buf = f.buffer();
        bytes += buf.as_raw().len();
        if bytes > BUDGET && !out.is_empty() {
            break;
        }
        out.push(Arc::new(ColorImage::from_rgba_unmultiplied([buf.width() as usize, buf.height() as usize], buf.as_raw())));
        durations.push(f.delay().into());
    }
    Ok((out, durations))
}

fn decode(kind: Kind, data: &[u8]) -> Result<(Anim, Vec<Duration>), String> {
    let (frames, mut durations) = match kind {
        Kind::Gif => collect(image::codecs::gif::GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames())?,
        Kind::Webp => collect(image::codecs::webp::WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames())?,
        Kind::Mp4 => crate::mp4::frames(data, BUDGET)?.into_iter().map(|(img, d)| (Arc::new(img), d)).unzip(),
        Kind::Apng => collect(image::codecs::png::PngDecoder::new(Cursor::new(data)).and_then(|d| d.apng()).map_err(|e| e.to_string())?.into_frames())?,
    };
    if frames.is_empty() {
        return Err("no frames".into());
    }
    // browsers stretch 0-10ms frame delays to 100ms and gifs are made to look right there
    for d in &mut durations {
        if *d < Duration::from_millis(11) {
            *d = Duration::from_millis(100);
        }
    }
    Ok((Anim { frames }, durations))
}

/// What to hand to `Image::from_uri` for `url`. Animated downloads come back as a
/// frame uri (frame 0 unless `play`, the window is focused and the clock says
/// otherwise); everything else, including urls still downloading, is unchanged.
/// Wakes egui up when the next frame is due, so only ask for things on screen.
pub fn uri(ctx: &egui::Context, url: &str, play: bool) -> String {
    let Ok(BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(url) else { return url.to_string() };
    if kind(&bytes).is_none() {
        return url.to_string();
    }
    let i = if play && ctx.input(|i| i.focused) { frame_at(ctx, url) } else { 0 };
    if TRACE.load(Relaxed) {
        let mut shown = SHOWN.lock();
        let e = shown.entry(url.to_string()).or_default();
        e.0.insert(i);
        e.1 = i;
    }
    format!("{url}{SEP}{i}")
}

/// Avatars, server icons and banners with an `a_` hash are animated; the png url
/// is a still. Draws it, and the gif over it while `hot` (hovered, like discord).
pub fn paint_still(ui: &egui::Ui, url: &str, rect: egui::Rect, radius: egui::CornerRadius, hot: bool) {
    let draw = |u: String| egui::Image::from_uri(u).corner_radius(radius).show_loading_spinner(false).paint_at(ui, rect);
    draw(url.to_string());
    if hot && url.contains("/a_") {
        draw(uri(ui.ctx(), &url.replacen(".png?", ".gif?", 1), true));
    }
}

fn frame_at(ctx: &egui::Context, url: &str) -> usize {
    let t = (ctx.input(|i| i.time) * 1000.0) as u128;
    frame_for(ctx, url, t, true)
}

fn frame_for(ctx: &egui::Context, url: &str, t: u128, wake: bool) -> usize {
    let Some(durs) = ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url))) else { return 0 };
    let total = durs.all().map(|d| d.as_millis()).sum::<u128>().max(1);
    let t = t % total;
    let mut end = 0;
    for (i, d) in durs.all().enumerate() {
        end += d.as_millis();
        if t < end {
            if wake {
                ctx.request_repaint_after(Duration::from_millis((end - t) as u64));
            }
            return i;
        }
    }
    0
}

/// `uri` for an animation that has been running `ms` since it was started, rather than since
/// the app was: the frame for that moment, and the last one once a single play is over.
pub fn uri_at(ctx: &egui::Context, url: &str, ms: u128, once: bool) -> String {
    let Ok(BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(url) else { return url.to_string() };
    if kind(&bytes).is_none() {
        return url.to_string();
    }
    let total = ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url))).map_or(0, |d| d.all().map(|d| d.as_millis()).sum::<u128>());
    let i = if once && total > 0 && ms >= total { last_frame(ctx, url) } else { frame_for(ctx, url, ms, false) };
    if TRACE.load(Relaxed) {
        let mut shown = SHOWN.lock();
        let e = shown.entry(url.to_string()).or_default();
        e.0.insert(i);
        e.1 = i;
    }
    format!("{url}{SEP}{i}")
}

fn last_frame(ctx: &egui::Context, url: &str) -> usize {
    ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url))).map_or(0, |d| d.all().count().saturating_sub(1))
}

// the e2e run asks which frames of an animation actually reached the screen
static TRACE: AtomicBool = AtomicBool::new(false);
static SHOWN: LazyLock<Mutex<BTreeMap<String, (HashSet<usize>, usize)>>> = LazyLock::new(Default::default);

pub fn trace() {
    TRACE.store(true, Relaxed);
}

/// Most distinct frames shown so far of any url containing `part`.
pub fn shown(part: &str) -> usize {
    SHOWN.lock().iter().filter(|(u, _)| u.contains(part)).map(|(_, f)| f.0.len()).max().unwrap_or(0)
}

/// The frame most recently shown for a url containing `part`.
pub fn current(part: &str) -> usize {
    SHOWN.lock().iter().find(|(u, _)| u.contains(part)).map_or(0, |(_, f)| f.1)
}

impl ImageLoader for GifLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(GifLoader)
    }

    fn load(&self, ctx: &egui::Context, frame_uri: &str, _: SizeHint) -> ImageLoadResult {
        let (uri, index) = match frame_uri.rsplit_once(SEP) {
            Some((u, i)) => (u, i.parse().map_err(|_| LoadError::NotSupported)?),
            None => decode_animated_image_uri(frame_uri).map_err(|_| LoadError::NotSupported)?,
        };
        if let Some(e) = self.cache.lock().get(uri) {
            return match e {
                Poll::Ready(Ok(a)) => Ok(ImagePoll::Ready { image: a.frames[index % a.frames.len()].clone() }),
                Poll::Ready(Err(e)) => Err(LoadError::Loading(e.clone())),
                Poll::Pending => Ok(ImagePoll::Pending { size: None }),
            };
        }
        match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, .. }) => {
                let Some(kind) = kind(&bytes) else { return Err(LoadError::NotSupported) };
                self.cache.lock().insert(uri.to_string(), Poll::Pending);
                let (cache, ctx, uri) = (self.cache.clone(), ctx.clone(), uri.to_string());
                let bytes = bytes.to_vec();
                std::thread::spawn(move || {
                    let entry = match decode(kind, &bytes) {
                        Ok((anim, durations)) => {
                            ctx.data_mut(|d| *d.get_temp_mut_or_default(Id::new(uri.as_str())) = FrameDurations::new(durations));
                            Ok(Arc::new(anim))
                        }
                        Err(e) => Err(e),
                    };
                    if let Some(slot) = cache.lock().get_mut(&uri) {
                        *slot = Poll::Ready(entry);
                    }
                    ctx.request_repaint();
                });
                Ok(ImagePoll::Pending { size: None })
            }
            Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
            Err(e) => Err(e),
        }
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
            .map(|e| match e {
                Poll::Ready(Ok(a)) => a.frames.iter().map(|f| f.pixels.len() * 4).sum(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().values().any(|e| e.is_pending())
    }
}
