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
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::task::Poll;
use std::time::Duration;

// the decoded pixels an animation may keep whole (an emoji, a sticker); a bigger one keeps its
// bytes and is decoded as it plays, a few frames at a time. Kept whole, a clip in a chat was
// tens of MB for as long as it was on screen.
const WHOLE: usize = 4 << 20;
const MAX_ANIM_FRAMES: usize = 600;
// frames kept ready past the one on screen
const AHEAD: usize = 2;
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

fn is_sticker_uri(uri: &str) -> bool {
    let base = uri.split(['?', '#']).next().unwrap_or(uri);
    let trusted = [crate::api::asset(), crate::api::cdn(), crate::api::media()].iter().any(|p| base.starts_with(p));
    trusted && base.contains("/stickers/") && base.ends_with(".json")
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

fn kind_for(uri: &str, b: &[u8]) -> Option<Kind> {
    match kind(b) {
        Some(Kind::Lottie) if !is_sticker_uri(uri) => None,
        other => other,
    }
}

struct Anim {
    frames: Vec<Arc<ColorImage>>,
    // the longest side the frames were scaled down to, if they were
    side: Option<u32>,
    lottie: Option<Arc<crate::lottie::Player>>,
    rolling: Option<Arc<Rolling>>,
}

impl Anim {
    fn frame(&self, ctx: &egui::Context, i: usize) -> Arc<ColorImage> {
        match (&self.lottie, &self.rolling) {
            (Some(p), _) => p.frame(ctx, i),
            (_, Some(r)) => r.frame(ctx, i),
            _ => self.frames[i % self.frames.len()].clone(),
        }
    }
}

/// An animation's frames in order from its bytes, with how long each stays up.
trait Source {
    /// The next frame, or just past it when it isn't `keep`t (to get to a later one).
    fn next(&mut self, keep: bool) -> Option<Result<(Option<ColorImage>, Duration), String>>;
}

struct Pictures(image::Frames<'static>);

impl Source for Pictures {
    fn next(&mut self, keep: bool) -> Option<Result<(Option<ColorImage>, Duration), String>> {
        let f = self.0.next()?;
        Some(f.map_err(|e| e.to_string()).map(|f| {
            let buf = f.buffer();
            (keep.then(|| ColorImage::from_rgba_unmultiplied([buf.width() as usize, buf.height() as usize], buf.as_raw())), f.delay().into())
        }))
    }
}

struct Clip(crate::mp4::Clip, std::vec::IntoIter<Duration>);

impl Source for Clip {
    fn next(&mut self, keep: bool) -> Option<Result<(Option<ColorImage>, Duration), String>> {
        let pic = self.0.next(keep)?;
        Some(pic.map(|p| (p, self.1.next().unwrap_or_default())))
    }
}

fn anim_limits() -> image::Limits {
    let mut lim = image::Limits::default();
    lim.max_image_width = Some(8192);
    lim.max_image_height = Some(8192);
    lim.max_alloc = Some(128 << 20);
    lim
}

fn source(kind: Kind, bytes: &Arc<[u8]>) -> Result<Box<dyn Source>, String> {
    use image::ImageDecoder as _;
    let data = Cursor::new(bytes.clone());
    let pictures = |frames: image::ImageResult<image::Frames<'static>>| frames.map(|f| Box::new(Pictures(f)) as Box<dyn Source>).map_err(|e| e.to_string());
    match kind {
        Kind::Gif => pictures(image::codecs::gif::GifDecoder::new(data).and_then(|mut d| {
            d.set_limits(anim_limits())?;
            Ok(d.into_frames())
        })),
        Kind::Webp => pictures(image::codecs::webp::WebPDecoder::new(data).and_then(|mut d| {
            d.set_limits(anim_limits())?;
            Ok(d.into_frames())
        })),
        Kind::Apng => pictures(image::codecs::png::PngDecoder::new(data).and_then(|mut d| {
            d.set_limits(anim_limits())?;
            Ok(d.apng()?.into_frames())
        })),
        Kind::Mp4 => {
            let clip = crate::mp4::Clip::new(bytes.clone())?;
            let durations = clip.durations();
            Ok(Box::new(Clip(clip, durations.into_iter())))
        }
        Kind::Lottie => Err("a lottie isn't frames".into()),
    }
}

/// An animation's frames by number, from its bytes: on from the last one given, or from the
/// start again to go back.
struct Walk {
    kind: Kind,
    bytes: Arc<[u8]>,
    side: Option<u32>,
    source: Option<Box<dyn Source>>,
    next: usize,
}

impl Walk {
    fn at(&mut self, i: usize) -> Result<ColorImage, String> {
        if self.source.is_none() || self.next > i {
            self.source = Some(source(self.kind, &self.bytes)?);
            self.next = 0;
        }
        let Some(source) = &mut self.source else { return Err("no source".into()) };
        loop {
            let (frame, _) = source.next(self.next == i).ok_or("ran out of frames")??;
            self.next += 1;
            if let Some(frame) = frame {
                return Ok(shrink(frame, self.side));
            }
        }
    }
}

/// An animation too big to keep decoded: its bytes, its first frame (what's shown while it
/// isn't playing) and its last when that's known (where a single play rests), and the few
/// frames around the one on screen, which the `anim` thread keeps coming.
struct Rolling {
    kind: Kind,
    bytes: Arc<[u8]>,
    side: Option<u32>,
    count: usize,
    first: Arc<ColorImage>,
    last: Option<Arc<ColorImage>>,
    near: std::sync::Mutex<Near>,
}

struct Near {
    frames: VecDeque<(usize, Arc<ColorImage>)>,
    want: usize,
    // what was last handed out, shown again while the frame wanted isn't decoded yet
    shown: Arc<ColorImage>,
    asked: bool,
    broken: bool,
}

impl Rolling {
    fn has(&self, near: &Near, i: usize) -> Option<Arc<ColorImage>> {
        if i == 0 {
            Some(self.first.clone())
        } else if i + 1 == self.count && self.last.is_some() {
            self.last.clone()
        } else {
            near.frames.iter().find(|f| f.0 == i).map(|f| f.1.clone())
        }
    }

    fn frame(self: &Arc<Self>, ctx: &egui::Context, i: usize) -> Arc<ColorImage> {
        let i = i % self.count;
        let mut near = self.near.lock().unwrap();
        let have = self.has(&near, i);
        near.want = i;
        if let Some(f) = &have {
            near.shown = f.clone();
        }
        let coming = (1..=AHEAD).all(|k| self.has(&near, (i + k) % self.count).is_some());
        if !(have.is_some() && coming) && !near.broken && !std::mem::replace(&mut near.asked, true) {
            let _ = worker().send((self.clone(), ctx.clone()));
        }
        have.unwrap_or_else(|| near.shown.clone())
    }

    fn bytes(&self) -> usize {
        let pixels = |f: &Arc<ColorImage>| f.pixels.len() * 4;
        self.bytes.len() + pixels(&self.first) + self.last.as_ref().map_or(0, pixels) + self.near.lock().unwrap().frames.iter().map(|f| pixels(&f.1)).sum::<usize>()
    }
}

/// The thread that decodes the frames rolling animations are about to show. A decoder isn't
/// `Send`, so each animation's stays here, and goes when the animation does.
fn worker() -> &'static std::sync::mpsc::Sender<(Arc<Rolling>, egui::Context)> {
    static WORKER: std::sync::OnceLock<std::sync::mpsc::Sender<(Arc<Rolling>, egui::Context)>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(Arc<Rolling>, egui::Context)>();
        let _ = std::thread::Builder::new().name("anim".into()).spawn(move || {
            let mut walks: HashMap<usize, (std::sync::Weak<Rolling>, Walk)> = HashMap::new();
            for (r, ctx) in rx {
                walks.retain(|_, w| w.0.strong_count() > 0);
                let walk = &mut walks
                    .entry(Arc::as_ptr(&r) as usize)
                    .or_insert_with(|| (Arc::downgrade(&r), Walk { kind: r.kind, bytes: r.bytes.clone(), side: r.side, source: None, next: 0 }))
                    .1;
                loop {
                    // the frame on screen and the ones after it, whichever of them is missing first
                    let missing = {
                        let mut near = r.near.lock().unwrap();
                        let missing = (0..=AHEAD).map(|k| (near.want + k) % r.count).find(|i| r.has(&near, *i).is_none());
                        near.asked = missing.is_some();
                        missing
                    };
                    let Some(i) = missing else { break };
                    let frame = walk.at(i);
                    let mut near = r.near.lock().unwrap();
                    match frame {
                        Ok(frame) => {
                            near.frames.push_back((i, Arc::new(frame)));
                            let want = near.want;
                            near.frames.retain(|f| (f.0 + r.count - want) % r.count <= AHEAD);
                            if i == want {
                                ctx.request_repaint();
                            }
                        }
                        Err(_) => {
                            near.broken = true;
                            near.asked = false;
                            break;
                        }
                    }
                }
            }
        });
        tx
    })
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

fn decode(kind: Kind, bytes: Arc<[u8]>, side: Option<u32>) -> Result<(Anim, Vec<Duration>), String> {
    if let Kind::Lottie = kind {
        let (player, durations) = crate::lottie::Player::new(&bytes, side.unwrap_or(320))?;
        return Ok((Anim { frames: Vec::new(), side: Some(player.side()), lottie: Some(Arc::new(player)), rolling: None }, durations));
    }
    let mut source = source(kind, &bytes)?;
    let (mut frames, mut durations, mut kept, mut last) = (Vec::new(), Vec::new(), 0, None);
    // a clip's frames are counted and timed without decoding them, so one too big to keep is
    // ready with its first; anything else has to be gone through once to know
    let count = match kind {
        Kind::Mp4 => crate::mp4::Clip::new(bytes.clone())?.durations().len(),
        _ => 0,
    };
    while let Some(f) = source.next(true) {
        if durations.len() >= MAX_ANIM_FRAMES {
            break;
        }
        let (frame, stays) = f?;
        let frame = Arc::new(shrink(frame.ok_or("no frame")?, side));
        durations.push(stays);
        kept += frame.pixels.len() * 4;
        if kept <= WHOLE || frames.is_empty() {
            frames.push(frame);
        } else {
            frames.truncate(1);
            last = Some(frame);
            if count * frames[0].pixels.len() * 4 > WHOLE {
                last = None;
                break;
            }
        }
    }
    if frames.is_empty() {
        return Err("no frames".into());
    }
    if let Kind::Mp4 = kind {
        durations = crate::mp4::Clip::new(bytes.clone())?.durations();
    }
    // browsers stretch 0-10ms frame delays to 100ms and gifs are made to look right there
    for d in &mut durations {
        if *d < Duration::from_millis(11) {
            *d = Duration::from_millis(100);
        }
    }
    if kept <= WHOLE {
        return Ok((Anim { frames, side, lottie: None, rolling: None }, durations));
    }
    let first = frames.swap_remove(0);
    let near = Near { frames: VecDeque::new(), want: 0, shown: first.clone(), asked: false, broken: false };
    let rolling = Rolling { kind, bytes, side, count: durations.len(), first, last, near: std::sync::Mutex::new(near) };
    Ok((Anim { frames: Vec::new(), side, lottie: None, rolling: Some(Arc::new(rolling)) }, durations))
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

// what a url's bytes turned out not to be: animated. Every image drawn asks every frame, and
// asking the bytes again reads back from disk the ones their texture had let go.
#[derive(Clone, Copy)]
struct Still;

fn is_animated(ctx: &egui::Context, url: &str) -> bool {
    let id = Id::new(url);
    let (animated, still) = ctx.data(|d| (d.get_temp::<FrameDurations>(id).is_some(), d.get_temp::<Still>(id).is_some()));
    if animated || still {
        return animated;
    }
    let Ok(BytesPoll::Ready { bytes, .. }) = ctx.try_load_bytes(url) else { return false };
    let animated = kind_for(url, &bytes).is_some();
    if !animated {
        ctx.data_mut(|d| d.insert_temp(id, Still));
    }
    animated
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
    let now = t;
    let t = t % total;
    let mut end = 0;
    for (i, d) in durs.all().enumerate() {
        end += d.as_millis();
        if t < end {
            if wake {
                ctx.request_repaint_after(Duration::from_millis(wake_in(now, end - t) as u64));
            }
            return i;
        }
    }
    0
}

/// How long to sleep at `now` for a frame due in `wait`, all in ms: to the next 16 ms tick at or
/// after it. A screenful of animations each waking for its own frame drew on every refresh of a
/// 144 Hz monitor; on shared ticks they draw 62 times a second at most.
fn wake_in(now: u128, wait: u128) -> u128 {
    (now + wait).div_ceil(16) * 16 - now
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

fn decode_pool() -> &'static std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>> {
    static POOL: std::sync::OnceLock<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for i in 0..4 {
            let rx = rx.clone();
            let _ = std::thread::Builder::new().name(format!("gif-dec-{i}")).spawn(move || {
                loop {
                    let job = rx.lock().unwrap().recv();
                    match job {
                        Ok(f) => f(),
                        Err(_) => break,
                    }
                }
            });
        }
        tx
    })
}

impl GifLoader {
    fn spawn(&self, ctx: &egui::Context, uri: &str, kind: Kind, bytes: Arc<[u8]>, side: Option<u32>) {
        let (cache, redo, ctx, uri) = (self.cache.clone(), self.redo.clone(), ctx.clone(), uri.to_string());
        let _ = decode_pool().send(Box::new(move || {
            let entry = match decode(kind, bytes, side) {
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
        }));
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
                && let Some(kind) = kind_for(uri, &bytes)
                && self.redo.lock().insert(uri.to_string())
            {
                self.spawn(ctx, uri, kind, bytes.as_ref().into(), want);
            }
            return Ok(ImagePoll::Ready { image: frame });
        }
        match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, .. }) => {
                let Some(kind) = kind_for(uri, &bytes) else { return Err(LoadError::NotSupported) };
                self.cache.lock().insert(uri.to_string(), Poll::Pending);
                self.spawn(ctx, uri, kind, bytes.as_ref().into(), want);
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
                Poll::Ready(Ok(a)) => a.frames.iter().map(|f| f.pixels.len() * 4).sum::<usize>() + a.lottie.as_ref().map_or(0, |p| p.bytes()) + a.rolling.as_ref().map_or(0, |r| r.bytes()),
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
    #[test]
    fn animations_wake_on_shared_ticks() {
        for now in [0u128, 1, 15, 16, 17, 1000, 123_457] {
            for wait in [1u128, 7, 16, 20, 33, 100] {
                let sleep = super::wake_in(now, wait);
                assert!(sleep >= wait && sleep < wait + 16, "{now} {wait} {sleep}");
                assert_eq!((now + sleep) % 16, 0);
            }
        }
    }

    use super::*;

    #[test]
    fn animated_webp_emoji_keeps_its_frames() {
        let bytes = include_bytes!("../examples/fixtures/agabounce.webp");
        assert!(matches!(kind(bytes), Some(Kind::Webp)));
        let (anim, durations) = decode(Kind::Webp, bytes.as_slice().into(), Some(48)).unwrap();
        assert!(anim.frames.len() > 1);
        assert_eq!(anim.frames.len(), durations.len());
        assert!(anim.frames.iter().any(|f| f.pixels != anim.frames[0].pixels));
        assert!(durations.iter().all(|d| !d.is_zero()));
    }

    #[test]
    fn frames_by_number_are_the_frames_decoded_whole() {
        let webp: Arc<[u8]> = include_bytes!("../examples/fixtures/agabounce.webp").as_slice().into();
        let (whole, _) = decode(Kind::Webp, webp.clone(), Some(48)).unwrap();
        let mut walk = Walk { kind: Kind::Webp, bytes: webp, side: Some(48), source: None, next: 0 };
        let n = whole.frames.len();
        for i in [0, 1, 2, n - 1, 1, 0, n / 2, n / 2, n - 1] {
            assert_eq!(walk.at(i).unwrap().pixels, whole.frames[i].pixels, "frame {i}");
        }
        let clip: Arc<[u8]> = include_bytes!("../examples/fixtures/spin.mp4").as_slice().into();
        let all = crate::mp4::frames(&clip, usize::MAX).unwrap();
        let mut walk = Walk { kind: Kind::Mp4, bytes: clip.clone(), side: None, source: None, next: 0 };
        for i in [0, 3, 1, all.len() - 1, 2] {
            assert_eq!(walk.at(i).unwrap().pixels, all[i].0.pixels, "picture {i}");
        }
        assert_eq!(crate::mp4::Clip::new(clip).unwrap().durations(), all.iter().map(|f| f.1).collect::<Vec<_>>());
    }

    #[test]
    fn a_clip_too_big_to_keep_plays_from_a_few_frames() {
        let bytes = include_bytes!("../examples/fixtures/clip.mp4");
        let all = crate::mp4::frames(bytes, usize::MAX).unwrap();
        let whole: usize = all.iter().map(|f| f.0.pixels.len() * 4).sum();
        assert!(whole > WHOLE, "the fixture has to be more than is kept whole: {whole}");
        let ctx = egui::Context::default();
        let url = "https://media.tenor.com/clip.mp4";
        ctx.include_bytes(url, bytes.as_slice());
        let loader = Arc::new(GifLoader::default());
        ctx.add_image_loader(loader.clone());
        let frame = |i: usize| {
            let start = std::time::Instant::now();
            loop {
                if let Ok(ImagePoll::Ready { image }) = ctx.try_load_image(&format!("{url}{SEP}{i}"), SizeHint::default())
                    && image.pixels == all[i].0.pixels
                {
                    return;
                }
                assert!(start.elapsed() < Duration::from_secs(10), "frame {i} never came");
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        // twice round, then back to where a single play rests
        for i in (0..all.len()).chain(0..all.len()).chain([all.len() - 1, 1]) {
            frame(i);
            let held = loader.byte_size();
            assert!(held <= bytes.len() + (AHEAD + 3) * all[0].0.pixels.len() * 4, "{held} bytes held at frame {i}");
        }
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
