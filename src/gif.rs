//! Animated gifs, animated webp, gifv mp4 clips and Lottie stickers, decoded off the UI thread.
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
use std::borrow::Cow;
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
    Lottie,
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
    } else if crate::lottie::sniff(b) {
        Some(Kind::Lottie)
    } else {
        None
    }
}

struct Anim {
    frames: Vec<Arc<ColorImage>>,
    // the longest side the frames were scaled down to, if they were
    side: Option<u32>,
    lottie: Option<Arc<crate::lottie::Player>>,
}

impl Anim {
    fn frame(&self, ctx: &egui::Context, i: usize) -> Arc<ColorImage> {
        match &self.lottie {
            Some(p) => p.frame(ctx, i),
            None => self.frames[i % self.frames.len()].clone(),
        }
    }
}

type Entry = Poll<Result<Arc<Anim>, String>>;

#[derive(Default)]
pub struct GifLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    // animations being decoded again, bigger, for a bigger place to draw them
    redo: Arc<Mutex<HashSet<String>>>,
}

/// The longest side worth decoding for where it's drawn: an emoji drawn at 33 pixels doesn't
/// need its 128-pixel frames. None for "as big as it comes".
fn side(hint: SizeHint) -> Option<u32> {
    match hint {
        SizeHint::Size { width, height, .. } => Some(width.max(height).max(1)),
        _ => None,
    }
}

/// `img` scaled down so its longest side is at most `side`, keeping what it's laid out as.
fn shrink(img: ColorImage, side: Option<u32>) -> ColorImage {
    let [w, h] = img.size;
    let Some(side) = side.filter(|s| (w.max(h) as u32) > *s) else { return img };
    let s = side as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
    let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, img.as_raw().to_vec()) else { return img };
    // premultiplied, so transparent edges don't darken
    let small = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Triangle);
    let mut out = ColorImage::from_rgba_premultiplied([nw as usize, nh as usize], small.as_raw());
    out.source_size = img.source_size;
    out
}

fn collect(frames: impl Iterator<Item = image::ImageResult<image::Frame>>, side: Option<u32>) -> Result<(Vec<Arc<ColorImage>>, Vec<Duration>), String> {
    let mut out = Vec::new();
    let mut durations = Vec::new();
    let mut bytes = 0;
    for f in frames {
        let f = f.map_err(|e| e.to_string())?;
        let buf = f.buffer();
        let img = shrink(ColorImage::from_rgba_unmultiplied([buf.width() as usize, buf.height() as usize], buf.as_raw()), side);
        bytes += img.pixels.len() * 4;
        if bytes > BUDGET && !out.is_empty() {
            break;
        }
        out.push(Arc::new(img));
        durations.push(f.delay().into());
    }
    Ok((out, durations))
}

fn decode(kind: Kind, data: &[u8], side: Option<u32>) -> Result<(Anim, Vec<Duration>), String> {
    if let Kind::Lottie = kind {
        let (player, durations) = crate::lottie::Player::new(data, side.unwrap_or(320))?;
        return Ok((Anim { frames: Vec::new(), side: Some(player.side()), lottie: Some(Arc::new(player)) }, durations));
    }
    let (frames, mut durations) = match kind {
        Kind::Gif => collect(image::codecs::gif::GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames(), side)?,
        Kind::Webp => collect(image::codecs::webp::WebPDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?.into_frames(), side)?,
        Kind::Mp4 => crate::mp4::frames(data, BUDGET)?.into_iter().map(|(img, d)| (Arc::new(shrink(img, side)), d)).unzip(),
        Kind::Apng => collect(image::codecs::png::PngDecoder::new(Cursor::new(data)).and_then(|d| d.apng()).map_err(|e| e.to_string())?.into_frames(), side)?,
        Kind::Lottie => unreachable!(),
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
    Ok((Anim { frames, side, lottie: None }, durations))
}

fn animation_url(url: &str) -> Cow<'_, str> {
    if !url.contains(".discordapp.net/") {
        return Cow::Borrowed(url);
    }
    let Ok(mut parsed) = reqwest::Url::parse(url) else { return Cow::Borrowed(url) };
    let host = parsed.host_str().unwrap_or_default();
    let proxy = host == "media.discordapp.net" || host.starts_with("images-ext-") && host.ends_with(".discordapp.net");
    if !proxy || ![".webp", ".gif", ".png"].iter().any(|ext| parsed.path().ends_with(ext))
        || parsed.query_pairs().any(|(k, v)| k == "animated" && v == "true")
    {
        return Cow::Borrowed(url);
    }
    let pairs: Vec<_> = parsed.query_pairs().filter(|(k, _)| k != "animated").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    parsed.set_query(None);
    parsed.query_pairs_mut().extend_pairs(pairs).append_pair("animated", "true");
    Cow::Owned(parsed.into())
}

fn is_animated(ctx: &egui::Context, url: &str) -> bool {
    ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url))).is_some()
        || matches!(ctx.try_load_bytes(url), Ok(BytesPoll::Ready { bytes, .. }) if kind(&bytes).is_some())
}

/// What to hand to `Image::from_uri` for `url`. Animated downloads come back as a
/// frame uri (frame 0 unless `play`, the window is focused and the clock says
/// otherwise); everything else, including urls still downloading, is unchanged.
/// Wakes egui up when the next frame is due, so only ask for things on screen.
pub fn uri(ctx: &egui::Context, url: &str, play: bool) -> String {
    let url = animation_url(url);
    if !is_animated(ctx, &url) {
        return url.to_string();
    }
    let i = if play && ctx.input(|i| i.focused) { frame_at(ctx, &url) } else { 0 };
    if TRACE.load(Relaxed) {
        let mut shown = SHOWN.lock();
        let e = shown.entry(url.to_string()).or_default();
        e.0.insert(i);
        e.1 = i;
        e.2 += 1;
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
    let url = animation_url(url);
    if !is_animated(ctx, &url) {
        return url.to_string();
    }
    let total = ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url.as_ref()))).map_or(0, |d| d.all().map(|d| d.as_millis()).sum::<u128>());
    let i = if once && total > 0 && ms >= total { last_frame(ctx, &url) } else { frame_for(ctx, &url, ms, false) };
    if TRACE.load(Relaxed) {
        let mut shown = SHOWN.lock();
        let e = shown.entry(url.to_string()).or_default();
        e.0.insert(i);
        e.1 = i;
        e.2 += 1;
    }
    format!("{url}{SEP}{i}")
}

/// How much longer the frame `uri_at` picks for `ms` stays up: None for a still (or a download
/// not in yet: its loader wakes egui when it lands) and for a single play resting on its last frame.
pub fn wait_at(ctx: &egui::Context, url: &str, ms: u128, once: bool) -> Option<u128> {
    let url = animation_url(url);
    if !is_animated(ctx, &url) {
        return None;
    }
    // animated but not decoded yet: keep looking so it doesn't sit on frame 0
    let Some(durs) = ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url.as_ref()))) else { return Some(16) };
    if durs.all().count() <= 1 {
        return None;
    }
    let total = durs.all().map(|d| d.as_millis()).sum::<u128>().max(1);
    if once && ms >= total {
        return None;
    }
    let t = ms % total;
    let mut end = 0;
    for d in durs.all() {
        end += d.as_millis();
        if t < end {
            return Some(end - t);
        }
    }
    None
}

fn last_frame(ctx: &egui::Context, url: &str) -> usize {
    ctx.data(|d| d.get_temp::<FrameDurations>(Id::new(url))).map_or(0, |d| d.all().count().saturating_sub(1))
}

// the e2e run asks which frames of an animation actually reached the screen
static TRACE: AtomicBool = AtomicBool::new(false);
static SHOWN: LazyLock<Mutex<BTreeMap<String, (HashSet<usize>, usize, u64)>>> = LazyLock::new(Default::default);

pub fn trace() {
    TRACE.store(true, Relaxed);
}

/// Most distinct frames shown so far of any url containing `part`.
pub fn shown(part: &str) -> usize {
    SHOWN.lock().iter().filter(|(u, _)| u.contains(part)).map(|(_, f)| f.0.len()).max().unwrap_or(0)
}

/// How many times a url containing `part` was asked for a frame to draw, repeats included.
pub fn draws(part: &str) -> u64 {
    SHOWN.lock().iter().filter(|(u, _)| u.contains(part)).map(|(_, f)| f.2).sum()
}

/// The frame most recently shown for a url containing `part`.
pub fn current(part: &str) -> usize {
    SHOWN.lock().iter().find(|(u, _)| u.contains(part)).map_or(0, |(_, f)| f.1)
}

impl GifLoader {
    fn spawn(&self, ctx: &egui::Context, uri: &str, kind: Kind, bytes: Vec<u8>, side: Option<u32>) {
        let (cache, redo, ctx, uri) = (self.cache.clone(), self.redo.clone(), ctx.clone(), uri.to_string());
        std::thread::spawn(move || {
            let entry = match decode(kind, &bytes, side) {
                Ok((anim, durations)) => {
                    ctx.data_mut(|d| *d.get_temp_mut_or_default(Id::new(uri.as_str())) = FrameDurations::new(durations));
                    Ok(Arc::new(anim))
                }
                Err(e) => Err(e),
            };
            // forgotten while it decoded: it stays forgotten; a failed redo keeps what's there
            if let Some(slot) = cache.lock().get_mut(&uri)
                && (entry.is_ok() || matches!(slot, Poll::Pending))
            {
                *slot = Poll::Ready(entry);
            }
            redo.lock().remove(&uri);
            ctx.request_repaint();
        });
    }
}

impl ImageLoader for GifLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(GifLoader)
    }

    fn load(&self, ctx: &egui::Context, frame_uri: &str, hint: SizeHint) -> ImageLoadResult {
        let (uri, index) = match frame_uri.rsplit_once(SEP) {
            Some((u, i)) => (u, i.parse().map_err(|_| LoadError::NotSupported)?),
            None => decode_animated_image_uri(frame_uri).map_err(|_| LoadError::NotSupported)?,
        };
        let want = side(hint);
        let have = match self.cache.lock().get(uri) {
            Some(Poll::Ready(Ok(a))) => Some((a.frame(ctx, index), a.side)),
            Some(Poll::Ready(Err(e))) => return Err(LoadError::Loading(e.clone())),
            Some(Poll::Pending) => return Ok(ImagePoll::Pending { size: None }),
            None => None,
        };
        if let Some((frame, side)) = have {
            // drawn bigger than it was decoded for (the image viewer): decode it again, bigger
            let bigger = side.is_some_and(|s| want.is_none_or(|w| w > s + s / 4));
            if bigger
                && let Ok(BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(uri)
                && let Some(kind) = kind(&bytes)
                && self.redo.lock().insert(uri.to_string())
            {
                self.spawn(ctx, uri, kind, bytes.to_vec(), want);
            }
            return Ok(ImagePoll::Ready { image: frame });
        }
        match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, .. }) => {
                let Some(kind) = kind(&bytes) else { return Err(LoadError::NotSupported) };
                self.cache.lock().insert(uri.to_string(), Poll::Pending);
                self.spawn(ctx, uri, kind, bytes.to_vec(), want);
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
                Poll::Ready(Ok(a)) => a.frames.iter().map(|f| f.pixels.len() * 4).sum::<usize>() + a.lottie.as_ref().map_or(0, |p| p.bytes()),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().values().any(|e| e.is_pending())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_webp_emoji_keeps_its_frames() {
        let bytes = include_bytes!("../examples/fixtures/agabounce.webp");
        assert!(matches!(kind(bytes), Some(Kind::Webp)));
        let (anim, durations) = decode(Kind::Webp, bytes, Some(48)).unwrap();
        assert!(anim.frames.len() > 1);
        assert_eq!(anim.frames.len(), durations.len());
        assert!(anim.frames.iter().any(|f| f.pixels != anim.frames[0].pixels));
        assert!(durations.iter().all(|d| !d.is_zero()));
    }

    #[test]
    fn webp_with_query_advances_through_the_loader() {
        let ctx = egui::Context::default();
        let url = "https://cdn.discordapp.com/emojis/1409837948899168256.webp?size=48&animated=true&name=agabounce&lossless=true";
        ctx.include_bytes(url, include_bytes!("../examples/fixtures/agabounce.webp").as_slice());
        ctx.add_image_loader(Arc::new(GifLoader::default()));
        let first = uri_at(&ctx, url, 0, false);
        let start = std::time::Instant::now();
        let first = loop {
            if let ImagePoll::Ready { image } = ctx.try_load_image(&first, SizeHint::default()).unwrap() {
                break image;
            }
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        };
        let frames: Vec<_> = (1..20).map(|n| {
            let uri = uri_at(&ctx, url, n * 50, false);
            let ImagePoll::Ready { image } = ctx.try_load_image(&uri, SizeHint::default()).unwrap() else { panic!("decoded frame missing") };
            image
        }).collect();
        assert!(frames.iter().any(|f| f.pixels != first.pixels));
    }

    #[test]
    fn decoded_animation_survives_byte_eviction() {
        use egui::load::{BytesLoader as _, TexturePoll};

        let ctx = egui::Context::default();
        let url = "https://cdn.discordapp.com/emojis/agabounce.webp?animated=true";
        ctx.include_bytes(url, include_bytes!("../examples/fixtures/agabounce.webp").as_slice());
        ctx.add_image_loader(Arc::new(GifLoader::default()));
        ctx.add_texture_loader(Arc::new(crate::imgcache::Textures::default()));
        let start = std::time::Instant::now();
        loop {
            let frame = uri_at(&ctx, url, 0, false);
            if let TexturePoll::Ready { .. } = ctx.try_load_texture(&frame, Default::default(), SizeHint::default()).unwrap() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        ctx.loaders().include.forget(url);
        for ms in (0..1000).step_by(20) {
            assert!(uri(&ctx, url, true).contains(SEP));
            let frame = uri_at(&ctx, url, ms, false);
            assert!(frame.contains(SEP), "decoded animation fell back to its unloaded source");
            assert!(matches!(ctx.try_load_texture(&frame, Default::default(), SizeHint::default()).unwrap(), TexturePoll::Ready { .. }));
        }
    }

    #[test]
    fn discord_proxy_requests_the_animated_webp() {
        let ctx = egui::Context::default();
        let proxy = "https://images-ext-1.discordapp.net/external/example/%3Fsize%3D48%26animated%3Dtrue/https/cdn.discordapp.com/emojis/1409837948899168256.webp";
        let animated = format!("{proxy}?animated=true");
        ctx.include_bytes(proxy, include_bytes!("../examples/fixtures/agabounce-still.webp").as_slice());
        ctx.include_bytes(animated.clone(), include_bytes!("../examples/fixtures/agabounce.webp").as_slice());
        assert_eq!(uri_at(&ctx, proxy, 0, false), format!("{animated}{SEP}0"));
    }
}
