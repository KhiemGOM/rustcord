//! Inline video and audio files in chat. Media Foundation's source reader does the decoding
//! (whatever codecs the machine has: h264, hevc, vp9, aac, mp3, flac, wav), one thread per
//! stream, each blocked while the clip is paused. Audio goes out through cpal and the count of
//! samples it has played is the clock video frames are shown against. One clip plays at a time.

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{GUID, HSTRING, Interface};

use crate::voice::hwdec::{Mips, halvings};

type Card = (ID3D11Device, ID3D11DeviceContext);

const QUEUE: usize = 4;
const MAX_W: usize = 3840;
const MAX_H: usize = 2160;
// what a picture can be decoded down to, so a resize rarely has to change anything
const SCALES: [f32; 5] = [0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 1.0];
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
const SOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
const EOS: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
const TYPE_CHANGED: u32 = MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32;

static VOLUME: AtomicU32 = AtomicU32::new(0x3f80_0000);
static UPLOADS: AtomicU64 = AtomicU64::new(0);
static MUTED: AtomicBool = AtomicBool::new(false);
static GPU: AtomicBool = AtomicBool::new(true);
static CUR: Mutex<Option<Arc<Session>>> = Mutex::new(None);
static RESUME: LazyLock<Mutex<HashMap<String, f64>>> = LazyLock::new(Default::default);

pub fn set_gpu(on: bool) {
    GPU.store(on, Relaxed);
}

pub fn set_volume(v: f32, muted: bool) {
    VOLUME.store(v.clamp(0.0, 1.0).to_bits(), Relaxed);
    MUTED.store(muted, Relaxed);
}

pub fn uploads() -> u64 {
    UPLOADS.load(Relaxed)
}

pub fn volume() -> (f32, bool) {
    (f32::from_bits(VOLUME.load(Relaxed)), MUTED.load(Relaxed))
}

#[derive(Clone)]
pub enum Phase {
    Loading(f32),
    Ready,
    Failed(String),
}

#[derive(Clone, Copy, Default)]
pub struct Info {
    pub dur: f64,
    pub w: usize,
    pub h: usize,
    pub fps: f32,
}

pub struct View {
    pub phase: Phase,
    pub info: Info,
    pub pos: f64,
    pub playing: bool,
}

struct Frame {
    pts: f64,
    img: ColorImage,
}

#[derive(Clone, Copy)]
enum Cmd {
    Play,
    Pause,
    Seek(f64),
    Resize,
}

enum Opened {
    Video(Result<Info, String>),
    Audio(Result<Option<(f64, bool)>, String>),
}

struct St {
    phase: Phase,
    info: Info,
    has_video: bool,
    out: bool,
    playing: bool,
    ended: bool,
    wall: f64,
    since: Option<Instant>,
    seen: u64,
    off_since: Option<Instant>,
    audio_done: bool,
    video_done: bool,
}

pub struct Session {
    st: Mutex<St>,
    pcm: Mutex<VecDeque<f32>>,
    frames: Mutex<VecDeque<Frame>>,
    played: AtomicU64,
    base: AtomicU64,
    rate: AtomicU32,
    txs: Mutex<Vec<Sender<Cmd>>>,
    stop: AtomicBool,
    tex: Mutex<Option<TextureHandle>>,
    // the pixels it's drawn at, and when that last wasn't more than this
    want: Mutex<((usize, usize), Instant)>,
    file: PathBuf,
    key: String,
}

pub fn current(key: &str) -> Option<Arc<Session>> {
    CUR.lock().unwrap().clone().filter(|s| s.key == key)
}

pub fn stop() {
    if let Some(s) = CUR.lock().unwrap().take() {
        let pos = s.pos();
        RESUME.lock().unwrap().insert(s.key.clone(), pos);
        s.stop.store(true, Relaxed);
        s.txs.lock().unwrap().clear();
    }
}

pub fn tick(frame: u64) {
    let Some(s) = CUR.lock().unwrap().clone() else { return };
    let (gone, forgotten) = {
        let mut st = s.st.lock().unwrap();
        let off = frame > st.seen + 1;
        st.off_since = if off { st.off_since.or(Some(Instant::now())) } else { None };
        (st.playing && off, st.off_since.is_some_and(|t| t.elapsed() > Duration::from_secs(30)))
    };
    if gone {
        s.pause();
    }
    if forgotten {
        stop();
    }
}

pub fn open(rt: &tokio::runtime::Runtime, key: &str, url: &str, video: bool, frame: u64) {
    stop();
    crate::audioplay::stop();
    static SWEPT: std::sync::Once = std::sync::Once::new();
    SWEPT.call_once(sweep);
    let resume = RESUME.lock().unwrap().get(key).copied().unwrap_or(0.0);
    let s = Arc::new(Session::new(key, url, frame));
    *CUR.lock().unwrap() = Some(s.clone());
    let (http, url) = (crate::api::http(), url.to_string());
    rt.spawn(async move {
        match download(&http, &url, &s).await {
            Ok(()) => {
                std::thread::spawn(move || start(s, video, resume));
            }
            Err(e) => s.st.lock().unwrap().phase = Phase::Failed(e),
        }
    });
}

fn sweep() {
    let old = Duration::from_secs(24 * 3600);
    for f in std::fs::read_dir(std::env::temp_dir().join("rustcord-media")).into_iter().flatten().flatten() {
        if f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e > old)) {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

async fn download(http: &reqwest::Client, url: &str, s: &Session) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    if s.file.exists() {
        return Ok(());
    }
    let e = |e: &dyn std::fmt::Display| e.to_string();
    let mut r = http.get(url).send().await.and_then(|r| r.error_for_status()).map_err(|x| e(&x))?;
    let total = r.content_length().unwrap_or(0) as f32;
    let part = s.file.with_extension(format!("{}.part", std::process::id()));
    std::fs::create_dir_all(part.parent().unwrap()).map_err(|x| e(&x))?;
    let mut f = tokio::fs::File::create(&part).await.map_err(|x| e(&x))?;
    let mut got = 0f32;
    while let Some(c) = r.chunk().await.map_err(|x| e(&x))? {
        if s.stop.load(Relaxed) {
            return Err("stopped".into());
        }
        f.write_all(&c).await.map_err(|x| e(&x))?;
        got += c.len() as f32;
        s.st.lock().unwrap().phase = Phase::Loading(if total > 0.0 { got / total } else { 0.0 });
    }
    f.flush().await.map_err(|x| e(&x))?;
    drop(f);
    std::fs::rename(&part, &s.file).map_err(|x| e(&x))
}

fn start(s: Arc<Session>, video: bool, resume: f64) {
    let (done, opened) = channel();
    let mut txs = Vec::new();
    if video {
        let (tx, rx) = channel();
        txs.push(tx);
        let (s, done) = (s.clone(), done.clone());
        std::thread::spawn(move || {
            video_thread(s, rx, done);
            crate::winsys::trim();
        });
    }
    let (tx, rx) = channel();
    txs.push(tx);
    let (s2, done2) = (s.clone(), done.clone());
    std::thread::spawn(move || audio_thread(s2, rx, done2));
    drop(done);
    *s.txs.lock().unwrap() = txs;
    let (mut vid, mut aud, mut err) = (None, None, None);
    for _ in 0..(1 + video as usize) {
        match opened.recv() {
            Ok(Opened::Video(Ok(i))) => vid = Some(i),
            Ok(Opened::Video(Err(e))) => err = Some(e),
            Ok(Opened::Audio(Ok(a))) => aud = a,
            Ok(Opened::Audio(Err(e))) if !video => err = Some(e),
            Ok(Opened::Audio(Err(_))) => {}
            Err(_) => break,
        }
    }
    let mut st = s.st.lock().unwrap();
    st.has_video = vid.is_some();
    st.out = aud.is_some_and(|a| a.1);
    st.info = match (vid, aud) {
        (Some(v), a) => Info { dur: a.map_or(v.dur, |a| a.0.max(v.dur)), ..v },
        (None, Some(a)) => Info { dur: a.0, ..Default::default() },
        (None, None) => {
            st.phase = Phase::Failed(err.unwrap_or_else(|| "can't play this file".into()));
            return;
        }
    };
    st.phase = Phase::Ready;
    drop(st);
    if resume > 1.0 && resume < s.st.lock().unwrap().info.dur - 1.0 {
        s.seek(resume);
    }
    s.play();
}

impl Session {
    fn new(key: &str, url: &str, frame: u64) -> Session {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut h);
        let ext = url.split(['?', '#']).next().and_then(|u| u.rsplit_once('.')).map(|(_, e)| e).filter(|e| e.len() <= 5).unwrap_or("bin");
        let file = match url.strip_prefix("file:///") {
            Some(local) => PathBuf::from(local),
            None => std::env::temp_dir().join("rustcord-media").join(format!("{:x}.{ext}", h.finish())),
        };
        Session {
            st: Mutex::new(St {
                phase: Phase::Loading(0.0),
                info: Info::default(),
                has_video: false,
                out: false,
                playing: false,
                ended: false,
                wall: 0.0,
                since: None,
                seen: frame,
                off_since: None,
                audio_done: false,
                video_done: false,
            }),
            pcm: Mutex::default(),
            frames: Mutex::default(),
            played: AtomicU64::new(0),
            base: AtomicU64::new(0),
            rate: AtomicU32::new(48_000),
            txs: Mutex::default(),
            stop: AtomicBool::new(false),
            tex: Mutex::default(),
            want: Mutex::new(((0, 0), Instant::now())),
            file,
            key: key.to_string(),
        }
    }

    fn send(&self, c: Cmd) {
        for tx in self.txs.lock().unwrap().iter() {
            let _ = tx.send(c);
        }
    }

    fn clock(&self, st: &St) -> f64 {
        let t = if st.out {
            f64::from_bits(self.base.load(Relaxed)) + self.played.load(Relaxed) as f64 / self.rate.load(Relaxed).max(1) as f64
        } else {
            st.wall + st.since.map_or(0.0, |t| t.elapsed().as_secs_f64())
        };
        if st.info.dur > 0.0 { t.min(st.info.dur) } else { t }
    }

    pub fn pos(&self) -> f64 {
        let st = self.st.lock().unwrap();
        self.clock(&st)
    }

    pub fn seen(&self, frame: u64) {
        self.st.lock().unwrap().seen = frame;
    }

    pub fn play(&self) {
        let mut st = self.st.lock().unwrap();
        if !matches!(st.phase, Phase::Ready) || st.playing {
            return;
        }
        let rewind = st.ended;
        st.playing = true;
        st.since = Some(Instant::now());
        drop(st);
        if rewind {
            self.seek(0.0);
        }
        self.send(Cmd::Play);
    }

    pub fn pause(&self) {
        let mut st = self.st.lock().unwrap();
        if !st.playing {
            return;
        }
        st.wall = self.clock(&st);
        st.since = None;
        st.playing = false;
        drop(st);
        self.send(Cmd::Pause);
    }

    pub fn toggle(&self) {
        if self.st.lock().unwrap().playing { self.pause() } else { self.play() }
    }

    pub fn seek(&self, t: f64) {
        let mut st = self.st.lock().unwrap();
        let t = t.clamp(0.0, if st.info.dur > 0.0 { st.info.dur } else { f64::MAX });
        st.wall = t;
        st.since = st.playing.then(Instant::now);
        st.ended = false;
        st.audio_done = false;
        st.video_done = false;
        self.base.store(t.to_bits(), Relaxed);
        self.played.store(0, Relaxed);
        drop(st);
        self.frames.lock().unwrap().clear();
        self.send(Cmd::Seek(t));
    }

    pub fn view(&self) -> View {
        let mut st = self.st.lock().unwrap();
        let pos = self.clock(&st);
        if st.playing {
            let a = !st.out || (st.audio_done && self.pcm.lock().unwrap().is_empty());
            let v = !st.has_video || (st.video_done && self.frames.lock().unwrap().is_empty());
            let t = st.out || st.has_video || pos >= st.info.dur - 0.05;
            if a && v && t {
                st.wall = st.info.dur;
                st.since = None;
                st.playing = false;
                st.ended = true;
                self.send(Cmd::Pause);
            }
        }
        View { phase: st.phase.clone(), info: st.info, pos: if st.ended { st.info.dur } else { pos }, playing: st.playing }
    }

    /// The newest picture, for drawing at `size` points. Fullscreen draws the inline player too,
    /// so the biggest asked for lasts a second.
    pub fn frame(&self, ctx: &egui::Context, size: egui::Vec2) -> Option<egui::TextureId> {
        let px = size * ctx.pixels_per_point();
        let px = (px.x.ceil() as usize, px.y.ceil() as usize);
        let mut want = self.want.lock().unwrap();
        if px != want.0 && ((px.0 >= want.0.0 && px.1 >= want.0.1) || want.1.elapsed() > Duration::from_secs(1)) {
            *want = (px, Instant::now());
            self.send(Cmd::Resize);
        } else if px == want.0 {
            want.1 = Instant::now();
        }
        drop(want);
        let (t, playing) = {
            let st = self.st.lock().unwrap();
            (self.clock(&st), st.playing)
        };
        let mut newest = None;
        {
            let mut f = self.frames.lock().unwrap();
            while f.front().is_some_and(|x| !playing || x.pts <= t + 0.002) {
                newest = f.pop_front();
            }
        }
        let mut tex = self.tex.lock().unwrap();
        if let Some(fr) = newest {
            match tex.as_mut() {
                Some(h) => {
                    UPLOADS.fetch_add(1, Relaxed);
                    h.set(fr.img, TextureOptions::LINEAR)
                }
                None => *tex = Some(ctx.load_texture(format!("media:{}", self.key), fr.img, TextureOptions::LINEAR)),
            }
        }
        tex.as_ref().map(|h| h.id())
    }
}

enum Got {
    Cmd(Cmd),
    Nothing,
    Quit,
}

enum Wait {
    No,
    A(Duration),
    Forever,
}

fn poll(rx: &Receiver<Cmd>, wait: Wait) -> Got {
    match wait {
        Wait::No => match rx.try_recv() {
            Ok(c) => Got::Cmd(c),
            Err(TryRecvError::Empty) => Got::Nothing,
            Err(_) => Got::Quit,
        },
        Wait::A(d) => match rx.recv_timeout(d) {
            Ok(c) => Got::Cmd(c),
            Err(RecvTimeoutError::Timeout) => Got::Nothing,
            Err(_) => Got::Quit,
        },
        Wait::Forever => rx.recv().map_or(Got::Quit, Got::Cmd),
    }
}

unsafe fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    }
}

/// Lets the reader decode on the graphics card, where its surfaces don't count against us.
unsafe fn gpu_manager() -> windows::core::Result<(IMFDXGIDeviceManager, Card)> {
    unsafe {
        // RUSTCORD_NO_CARD_DECODE: as on a machine whose card can't, to check the software path
        if std::env::var_os("RUSTCORD_NO_CARD_DECODE").is_some() {
            return Err(windows::core::Error::empty());
        }
        let (mut dev, mut ctx) = (None, None);
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut dev), None, Some(&mut ctx))?;
        let (dev, ctx) = (dev.ok_or_else(windows::core::Error::empty)?, ctx.ok_or_else(windows::core::Error::empty)?);
        let _ = dev.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
        let (mut token, mut mgr) = (0u32, None);
        MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
        let mgr = mgr.ok_or_else(windows::core::Error::empty)?;
        mgr.ResetDevice(&dev, token)?;
        Ok((mgr, (dev, ctx)))
    }
}

unsafe fn reader(path: &std::path::Path, advanced: bool, gpu: Option<&IMFDXGIDeviceManager>) -> windows::core::Result<IMFSourceReader> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 3)?;
        let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
        if advanced {
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
        }
        if let Some(mgr) = gpu {
            attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, mgr)?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
        }
        let url = HSTRING::from(path.to_string_lossy().as_ref());
        let r = MFCreateSourceReaderFromURL(&url, &attrs)?;
        r.SetStreamSelection(ALL, false)?;
        Ok(r)
    }
}

unsafe fn seek_to(r: &IMFSourceReader, t: f64) {
    unsafe {
        let _ = r.SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from((t * 1e7) as i64));
    }
}

unsafe fn duration(r: &IMFSourceReader) -> f64 {
    unsafe { r.GetPresentationAttribute(SOURCE, &MF_PD_DURATION).ok().and_then(|v| u64::try_from(&v).ok()).map_or(0.0, |d| d as f64 / 1e7) }
}

struct Pic {
    w: usize,
    h: usize,
    stride: i32,
}

unsafe fn video_type(r: &IMFSourceReader) -> windows::core::Result<Pic> {
    unsafe {
        let cur = r.GetCurrentMediaType(VIDEO)?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (w, h) = ((size >> 32) as usize, (size & 0xffff_ffff) as usize);
        let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(w as i32 * 4);
        Ok(Pic { w, h, stride })
    }
}

/// The smallest size of the picture that still covers `want` pixels.
fn fit((w, h): (usize, usize), want: (usize, usize)) -> (usize, usize) {
    let k = SCALES.into_iter().find(|k| w as f32 * k >= want.0 as f32 && h as f32 * k >= want.1 as f32).unwrap_or(1.0);
    (((w as f32 * k) as usize & !1).max(2), ((h as f32 * k) as usize & !1).max(2))
}

unsafe fn output(r: &IMFSourceReader, (w, h): (usize, usize), native: u64) -> windows::core::Result<()> {
    unsafe {
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        let size = (w as u64) << 32 | h as u64;
        if size != native {
            mt.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        }
        if r.SetCurrentMediaType(VIDEO, None, &mt).is_err() && size != native {
            mt.SetUINT64(&MF_MT_FRAME_SIZE, native)?;
            r.SetCurrentMediaType(VIDEO, None, &mt)?;
        }
        Ok(())
    }
}

struct VideoReader {
    r: IMFSourceReader,
    info: Info,
    pic: Pic,
    native: u64,
    card: Option<Card>,
}

/// A frame that's on the card, averaged down to what covers `want`, without its full size ever
/// coming back. None if the frame isn't on the card after all.
unsafe fn card_image(sample: &IMFSample, mips: &mut Option<Mips>, (dev, ctx): &Card, pic: &Pic, want: (usize, usize)) -> Option<ColorImage> {
    unsafe {
        let dxgi = sample.GetBufferByIndex(0).ok()?.cast::<IMFDXGIBuffer>().ok()?;
        let mut tex: Option<ID3D11Texture2D> = None;
        dxgi.GetResource(&ID3D11Texture2D::IID, &raw mut tex as *mut *mut _).ok()?;
        let tex = tex?;
        let slice = dxgi.GetSubresourceIndex().ok()?;
        let mut d = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut d);
        let full = (pic.w as u32, pic.h as u32);
        let steps = halvings((pic.w, pic.h), want);
        if !mips.as_ref().is_some_and(|m| m.fits(full, steps, d.Format)) {
            *mips = Some(Mips::new(dev, ctx, full, steps, d.Format).ok()?);
        }
        let m = mips.as_ref()?;
        let whole = D3D11_BOX { left: 0, top: 0, front: 0, right: full.0, bottom: full.1, back: 1 };
        ctx.CopySubresourceRegion(&m.tex, 0, 0, 0, 0, &tex, slice, Some(&whole));
        m.read().ok()?
    }
}

unsafe fn open_video(path: &std::path::Path, gpu: bool, want: (usize, usize)) -> windows::core::Result<VideoReader> {
    unsafe {
        let (mgr, card) = if gpu { gpu_manager().map(|(m, c)| (Some(m), Some(c)))? } else { (None, None) };
        let r = reader(path, true, mgr.as_ref())?;
        r.SetStreamSelection(VIDEO, true)?;
        let native = r.GetNativeMediaType(VIDEO, 0)?;
        let size = native.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (nw, nh) = ((size >> 32) as usize, (size & 0xffff_ffff) as usize);
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(30 << 32 | 1);
        let fps = (rate >> 32) as f32 / (rate & 0xffff_ffff).max(1) as f32;
        let k = (MAX_W as f32 / nw as f32).min(MAX_H as f32 / nh as f32).min(1.0);
        let top = (((nw as f32 * k) as usize) & !1, ((nh as f32 * k) as usize) & !1);
        output(&r, if card.is_some() { top } else { fit(top, want) }, size)?;
        let pic = video_type(&r)?;
        let info = Info { dur: duration(&r), w: top.0, h: top.1, fps: if fps.is_finite() && fps > 0.0 { fps } else { 30.0 } };
        Ok(VideoReader { r, info, pic, native: size, card })
    }
}

unsafe fn to_image(sample: &IMFSample, pic: &Pic) -> Option<ColorImage> {
    unsafe {
        let buf = sample.ConvertToContiguousBuffer().ok()?;
        let (mut p, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
        buf.Lock(&mut p, None, Some(&mut len)).ok()?;
        let pitch = pic.stride.unsigned_abs() as usize;
        let img = (len as usize >= pitch * pic.h).then(|| {
            let src = std::slice::from_raw_parts(p, len as usize);
            let mut px = Vec::with_capacity(pic.w * pic.h);
            for y in 0..pic.h {
                let row = if pic.stride < 0 { pic.h - 1 - y } else { y } * pitch;
                px.extend(src[row..row + pic.w * 4].chunks_exact(4).map(|c| Color32::from_rgb(c[2], c[1], c[0])));
            }
            ColorImage::new([pic.w, pic.h], px)
        });
        let _ = buf.Unlock();
        img
    }
}

fn video_thread(s: Arc<Session>, rx: Receiver<Cmd>, done: Sender<Opened>) {
    unsafe {
        init();
        let asked = || s.want.lock().unwrap().0;
        let gpu = GPU.load(Relaxed);
        let opened = open_video(&s.file, gpu, asked()).or_else(|e| if gpu { open_video(&s.file, false, asked()) } else { Err(e) });
        let VideoReader { mut r, info, mut pic, mut native, mut card } = match opened {
            Ok(v) => v,
            Err(e) => {
                let _ = done.send(Opened::Video(Err(e.message())));
                return;
            }
        };
        let top = (info.w, info.h);
        let mut shown = false;
        let mut mips = None;
        let mut steps = halvings(top, asked());
        let _ = done.send(Opened::Video(Ok(info)));
        drop(done);
        let (mut playing, mut need, mut eof, mut skip) = (false, true, false, 0.0f64);
        let mut settle = None;
        loop {
            if s.stop.load(Relaxed) {
                return;
            }
            // setting the decoder up leaves far more behind than playing touches
            if shown && settle.is_none() {
                settle = Some((Instant::now() + Duration::from_secs(3), false));
            }
            if let Some((t, false)) = settle
                && Instant::now() >= t
            {
                crate::winsys::trim();
                settle = Some((t, true));
            }
            let len = s.frames.lock().unwrap().len();
            let want = !eof && ((playing && len < QUEUE) || (need && len == 0));
            let wait = if want {
                Wait::No
            } else if playing && !eof {
                Wait::A(Duration::from_millis(4))
            } else {
                Wait::Forever
            };
            match poll(&rx, wait) {
                Got::Quit => return,
                Got::Cmd(Cmd::Play) => playing = true,
                Got::Cmd(Cmd::Pause) => playing = false,
                Got::Cmd(Cmd::Seek(t)) => {
                    seek_to(&r, t);
                    s.frames.lock().unwrap().clear();
                    (skip, need, eof) = (t, true, false);
                }
                Got::Cmd(Cmd::Resize) => {
                    let size = fit(top, asked());
                    let changed = if card.is_some() {
                        std::mem::replace(&mut steps, halvings(top, asked())) != steps
                    } else if size != (pic.w, pic.h)
                        && output(&r, size, native).is_ok()
                        && let Ok(p) = video_type(&r)
                    {
                        pic = p;
                        true
                    } else {
                        false
                    };
                    // a paused picture won't change by itself
                    if changed && !playing {
                        let t = s.pos();
                        seek_to(&r, t);
                        s.frames.lock().unwrap().clear();
                        (skip, need, eof) = (t, true, false);
                    }
                }
                Got::Nothing if want => {
                    let (mut flags, mut ts, mut sample) = (0u32, 0i64, None);
                    let read = r.ReadSample(VIDEO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample));
                    if flags & TYPE_CHANGED != 0
                        && let Ok(p) = video_type(&r)
                    {
                        pic = p;
                    }
                    let pts = ts as f64 / 1e7;
                    let due = sample.as_ref().filter(|_| pts >= skip - 0.0005);
                    let img = due.and_then(|sm| card.as_ref().and_then(|c| card_image(sm, &mut mips, c, &pic, asked())).or_else(|| to_image(sm, &pic)));
                    // a card that opened and then can't deliver: start over on the processor
                    if card.is_some() && !shown && (read.is_err() || (due.is_some() && img.is_none())) {
                        card = None;
                        mips = None;
                        if let Ok(o) = open_video(&s.file, false, asked()) {
                            (r, pic, native) = (o.r, o.pic, o.native);
                            seek_to(&r, skip);
                            continue;
                        }
                        flags |= EOS;
                    } else if read.is_err() {
                        flags |= EOS;
                    }
                    if let Some(img) = img {
                        s.frames.lock().unwrap().push_back(Frame { pts, img });
                        need = false;
                        shown = true;
                    }
                    if flags & EOS != 0 {
                        eof = true;
                        s.st.lock().unwrap().video_done = true;
                    }
                }
                Got::Nothing => {}
            }
        }
    }
}

unsafe fn open_audio(path: &std::path::Path, ch: u32, rate: u32) -> windows::core::Result<Option<(IMFSourceReader, f64, u32, u32)>> {
    unsafe {
        let r = reader(path, false, None)?;
        if r.SetStreamSelection(AUDIO, true).is_err() || r.GetNativeMediaType(AUDIO, 0).is_err() {
            return Ok(None);
        }
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
        mt.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)?;
        mt.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
        mt.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        mt.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, ch * 4)?;
        mt.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, ch * 4 * rate)?;
        r.SetCurrentMediaType(AUDIO, None, &mt)?;
        let cur = r.GetCurrentMediaType(AUDIO)?;
        let got = (cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?, cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?);
        let dur = duration(&r);
        Ok(Some((r, dur, got.0, got.1)))
    }
}

fn audio_thread(s: Arc<Session>, rx: Receiver<Cmd>, done: Sender<Opened>) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    unsafe {
        init();
        let dev = std::env::var_os("RUSTCORD_SILENT").is_none().then(|| cpal::default_host().default_output_device()).flatten();
        let cfg = dev.as_ref().and_then(|d| d.default_output_config().ok()).filter(|c| c.sample_format() == cpal::SampleFormat::F32);
        let (ch, rate) = cfg.as_ref().map_or((2, 48_000), |c| (c.channels() as u32, c.sample_rate()));
        let (r, dur, got_ch, got_rate) = match open_audio(&s.file, ch, rate) {
            Ok(Some(a)) => a,
            Ok(None) => {
                let _ = done.send(Opened::Audio(Ok(None)));
                return;
            }
            Err(e) => {
                let _ = done.send(Opened::Audio(Err(e.message())));
                return;
            }
        };
        let Some(cfg) = cfg.filter(|_| (got_ch, got_rate) == (ch, rate)) else {
            let _ = done.send(Opened::Audio(Ok(Some((dur, false)))));
            return;
        };
        s.rate.store(rate, Relaxed);
        let (pcm, played) = (s.clone(), s.clone());
        let stream = dev.unwrap().build_output_stream::<f32, _, _>(
            cfg.config(),
            move |data, _| {
                let (v, muted) = volume();
                let gain = if muted { 0.0 } else { v };
                let Ok(mut q) = pcm.pcm.try_lock() else {
                    data.fill(0.0);
                    return;
                };
                let mut n = 0;
                for o in data.iter_mut() {
                    *o = q.pop_front().map_or(0.0, |x| {
                        n += 1;
                        x * gain
                    });
                }
                played.played.fetch_add(n / ch as u64, Relaxed);
            },
            |_| {},
            None,
        );
        let Ok(stream) = stream else {
            let _ = done.send(Opened::Audio(Ok(Some((dur, false)))));
            return;
        };
        let _ = stream.pause();
        let _ = done.send(Opened::Audio(Ok(Some((dur, true)))));
        drop(done);
        let cap = (rate * ch) as usize;
        let (mut playing, mut eof, mut skip) = (false, false, 0.0f64);
        loop {
            if s.stop.load(Relaxed) {
                return;
            }
            let want = playing && !eof && s.pcm.lock().unwrap().len() < cap;
            let wait = if want {
                Wait::No
            } else if playing && !eof {
                Wait::A(Duration::from_millis(20))
            } else {
                Wait::Forever
            };
            match poll(&rx, wait) {
                Got::Quit => return,
                Got::Cmd(Cmd::Play) => {
                    playing = true;
                    let _ = stream.play();
                }
                Got::Cmd(Cmd::Pause) => {
                    playing = false;
                    let _ = stream.pause();
                }
                Got::Cmd(Cmd::Resize) => {}
                Got::Cmd(Cmd::Seek(t)) => {
                    seek_to(&r, t);
                    let mut q = s.pcm.lock().unwrap();
                    q.clear();
                    s.played.store(0, Relaxed);
                    s.base.store(t.to_bits(), Relaxed);
                    drop(q);
                    (skip, eof) = (t, false);
                }
                Got::Nothing if want => {
                    let (mut flags, mut ts, mut sample) = (0u32, 0i64, None);
                    if r.ReadSample(AUDIO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)).is_err() {
                        flags |= EOS;
                    }
                    if let Some(buf) = sample.and_then(|sm| sm.ConvertToContiguousBuffer().ok()) {
                        let (mut p, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
                        if buf.Lock(&mut p, None, Some(&mut len)).is_ok() {
                            let all = std::slice::from_raw_parts(p as *const f32, len as usize / 4);
                            let drop_n = (((skip - ts as f64 / 1e7).max(0.0) * rate as f64) as usize * ch as usize).min(all.len());
                            s.pcm.lock().unwrap().extend(&all[drop_n..]);
                            let _ = buf.Unlock();
                        }
                    }
                    if flags & EOS != 0 {
                        eof = true;
                        s.st.lock().unwrap().audio_done = true;
                    }
                }
                Got::Nothing => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures").join(name)
    }

    #[test]
    fn first_video_frame_is_the_right_way_up() {
        unsafe {
            init();
            let VideoReader { r, info, pic, .. } = open_video(&fixture("clip.mp4"), false, (320, 180)).unwrap();
            assert_eq!((info.w, info.h), (320, 180));
            assert!(info.dur > 3.5 && info.dur < 4.5, "duration {}", info.dur);
            let (mut flags, mut sample) = (0u32, None);
            r.ReadSample(VIDEO, 0, None, Some(&mut flags), None, Some(&mut sample)).unwrap();
            let img = to_image(&sample.unwrap(), &pic).unwrap();
            let [tr, _, tb, _] = img.pixels[10 * 320 + 10].to_array();
            let [br, _, bb, _] = img.pixels[170 * 320 + 10].to_array();
            assert!(tr > 180 && tb < 90, "top row is not red: {tr} {tb}");
            assert!(bb > 180 && br < 90, "bottom row is not blue: {br} {bb}");
        }
    }

    fn cpu_secs() -> f64 {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
        [k, u].iter().map(|t| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7).sum()
    }

    #[test]
    #[ignore]
    fn plays_a_clip_for_how_much_cpu() {
        let Some(clip) = std::env::var_os("RUSTCORD_BENCH_CLIP") else { return };
        unsafe { std::env::set_var("RUSTCORD_SILENT", "1") };
        let s = Arc::new(Session::new("bench", "http://bench/clip.mp4", 0));
        std::fs::create_dir_all(s.file.parent().unwrap()).unwrap();
        std::fs::copy(clip, &s.file).unwrap();
        let ctx = egui::Context::default();
        let started = s.clone();
        std::thread::spawn(move || start(started, true, 0.0));
        let tick = |secs: f64| {
            let (t, up, cpu) = (Instant::now(), uploads(), cpu_secs());
            while t.elapsed().as_secs_f64() < secs {
                s.frame(&ctx, egui::vec2(320.0, 180.0));
                std::thread::sleep(Duration::from_millis(16));
            }
            let w = t.elapsed().as_secs_f64();
            ((uploads() - up) as f64 / w, (cpu_secs() - cpu) / w * 100.0)
        };
        while !matches!(s.view().phase, Phase::Ready) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let info = s.view().info;
        let (fps, cpu) = tick(6.0);
        println!("{}x{} {:.0}fps source, playing: {fps:.1} frames/s shown, {cpu:.1}% of one core", info.w, info.h, info.fps);
        s.pause();
        std::thread::sleep(Duration::from_millis(300));
        let (fps, cpu) = tick(3.0);
        println!("paused: {fps:.1} frames/s shown, {cpu:.2}% of one core");
        assert!(fps < 0.5);
    }

    #[test]
    fn audio_decodes_to_float_at_the_asked_rate() {
        unsafe {
            init();
            for file in ["tone.mp3", "clip.mp4"] {
                let (r, dur, ch, rate) = open_audio(&fixture(file), 2, 48_000).unwrap().unwrap();
                assert_eq!((ch, rate), (2, 48_000), "{file}");
                assert!(dur > 2.5, "{file} duration {dur}");
                let (mut samples, mut peak) = (0usize, 0f32);
                loop {
                    let (mut flags, mut sample) = (0u32, None);
                    r.ReadSample(AUDIO, 0, None, Some(&mut flags), None, Some(&mut sample)).unwrap();
                    if let Some(buf) = sample.and_then(|s| s.ConvertToContiguousBuffer().ok()) {
                        let (mut p, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
                        buf.Lock(&mut p, None, Some(&mut len)).unwrap();
                        let all = std::slice::from_raw_parts(p as *const f32, len as usize / 4);
                        samples += all.len();
                        peak = all.iter().fold(peak, |m, x| m.max(x.abs()));
                        buf.Unlock().unwrap();
                    }
                    if flags & EOS != 0 {
                        break;
                    }
                }
                let secs = samples as f64 / 2.0 / 48_000.0;
                assert!((secs - dur).abs() < 0.5, "{file}: {secs}s decoded of {dur}s");
                assert!(peak > 0.1, "{file} is silent, peak {peak}");
            }
        }
    }
}
