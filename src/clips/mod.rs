//! Clips: the last stretch of your own Go Live, kept in memory while a stream is up and clips are
//! on, and written out as an MP4 when the save keybind (or the clip button) fires. Nothing is
//! allocated, and nothing but a few atomics is read, while clips are off or no stream is live.

pub mod cut;
pub mod library;
pub mod mux;
pub mod ring;

use egui::ColorImage;
use ring::{Audio, Cursors, Frame, Video, RATE, TICKS};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub use library::Clip;

pub const CLIPS_ENABLED: u32 = 1;
pub const ALLOW_VOICE_RECORDING: u32 = 2;
pub const DEFAULT_KEYS: [u32; 2] = [0x12, 0x43];
pub const LENGTHS: [(u32, &str); 3] = [(30, "30 seconds"), (60, "1 minute"), (120, "2 minutes")];
pub const MAX_SAVES: usize = 15;
const VIDEO_CAP: usize = 256 << 20;
const SLACK: i64 = 6;

static ENABLED: AtomicBool = AtomicBool::new(false);
static STREAMING: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SECS: AtomicU32 = AtomicU32::new(30);
static SAVING: AtomicUsize = AtomicUsize::new(0);
static EPOCH: OnceLock<Instant> = OnceLock::new();
static REC: Mutex<Option<Rec>> = Mutex::new(None);
static FLAGS: Mutex<Vec<(u64, u32)>> = Mutex::new(Vec::new());
static DONE: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());
static READY: AtomicBool = AtomicBool::new(false);
static STAGE: AtomicBool = AtomicBool::new(false);

struct Rec {
    video: Video,
    audio: Audio,
    cur: Cursors,
    size: (u32, u32),
    fps: u32,
    app: String,
    look: Option<Arc<ColorImage>>,
}

pub enum Outcome {
    Saved(Clip),
    Failed,
}

pub struct Context {
    pub users: Vec<u64>,
    pub guild: u64,
    pub channel: u64,
}

pub fn ticks(at: Instant) -> i64 {
    let e = *EPOCH.get_or_init(|| Instant::now().checked_sub(std::time::Duration::from_secs(3600)).unwrap_or_else(Instant::now));
    (at.saturating_duration_since(e).as_nanos() / 100) as i64
}

fn now() -> i64 {
    ticks(Instant::now())
}

pub fn active() -> bool {
    ACTIVE.load(Relaxed)
}

pub fn streaming() -> bool {
    STREAMING.load(Relaxed)
}

pub fn buffered() -> usize {
    REC.lock().unwrap().as_ref().map_or(0, |r| r.video.bytes() + r.audio.len() * 2)
}

pub fn saving() -> usize {
    SAVING.load(Relaxed)
}

fn refresh() {
    let on = ENABLED.load(Relaxed) && STREAMING.load(Relaxed);
    if ACTIVE.swap(on, Relaxed) && !on {
        *REC.lock().unwrap() = None;
    }
}

pub fn configure(enabled: bool, secs: u32) {
    let secs = secs.max(5);
    let changed = SECS.swap(secs, Relaxed) != secs;
    ENABLED.store(enabled, Relaxed);
    refresh();
    if changed && let Some(r) = REC.lock().unwrap().as_mut() {
        let (v, a) = buffers(secs);
        (r.video, r.audio) = (v, a);
    }
}

fn buffers(secs: u32) -> (Video, Audio) {
    (Video::new(secs as i64 * TICKS, VIDEO_CAP), Audio::new(secs as i64 + SLACK + 4))
}

pub fn stream_start(size: (usize, usize), fps: u32, app: String) {
    STREAMING.store(true, Relaxed);
    refresh();
    if ACTIVE.load(Relaxed) {
        let (video, audio) = buffers(SECS.load(Relaxed));
        *REC.lock().unwrap() = Some(Rec { video, audio, cur: Cursors::default(), size: (size.0 as u32, size.1 as u32), fps, app, look: None });
    }
}

pub fn stream_end() {
    STREAMING.store(false, Relaxed);
    refresh();
}

pub fn video(data: &[u8], pts: i64, key: bool) {
    if !ACTIVE.load(Relaxed) {
        return;
    }
    if let Some(r) = REC.lock().unwrap().as_mut() {
        r.video.push(Frame { data: data.into(), pts, key });
    }
}

pub fn look(img: &Arc<ColorImage>) {
    if !ACTIVE.load(Relaxed) {
        return;
    }
    if let Some(r) = REC.lock().unwrap().as_mut() {
        r.look = Some(img.clone());
    }
}

pub fn mic(pcm: &[f32]) {
    if !ACTIVE.load(Relaxed) || STAGE.load(Relaxed) {
        return;
    }
    add(now() * RATE / TICKS, 0, pcm);
}

pub fn voice(user: u64, pcm: &[f32]) {
    if !ACTIVE.load(Relaxed) || STAGE.load(Relaxed) || !allowed(user) {
        return;
    }
    let mono: Vec<f32> = pcm.chunks_exact(2).map(|p| (p[0] + p[1]) * 0.5).collect();
    add(now() * RATE / TICKS, user, &mono);
}

fn add(clock: i64, src: u64, pcm: &[f32]) {
    if let Some(r) = REC.lock().unwrap().as_mut() {
        let at = r.cur.place(src, clock, pcm.len());
        r.audio.mix(at, pcm);
    }
}

pub fn set_stage(on: bool) {
    STAGE.store(on, Relaxed);
}

pub fn set_flags(user: u64, flags: u32) {
    let mut f = FLAGS.lock().unwrap();
    match f.iter_mut().find(|e| e.0 == user) {
        Some(e) => e.1 = flags,
        None => f.push((user, flags)),
    }
}

pub fn flags(user: u64) -> u32 {
    FLAGS.lock().unwrap().iter().find(|e| e.0 == user).map_or(0, |e| e.1)
}

pub fn forget_flags() {
    FLAGS.lock().unwrap().clear();
}

pub fn allowed(user: u64) -> bool {
    flags(user) & ALLOW_VOICE_RECORDING != 0
}

pub fn others_clipping(me: u64) -> bool {
    FLAGS.lock().unwrap().iter().any(|e| e.0 != me && e.1 & CLIPS_ENABLED != 0)
}

pub fn outcomes() -> Vec<Outcome> {
    if !READY.swap(false, Relaxed) {
        return Vec::new();
    }
    std::mem::take(&mut *DONE.lock().unwrap())
}

pub fn is_key(au: &[u8]) -> bool {
    au.windows(4).any(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == 5)
}

pub fn source_name(window: Option<isize>) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let Some(h) = window else { return "Screen".into() };
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h as _, buf.as_mut_ptr(), buf.len() as i32) };
    match String::from_utf16_lossy(&buf[..n.max(0) as usize]).trim() {
        "" => "Screen".into(),
        t => t.into(),
    }
}

pub fn default_dir() -> PathBuf {
    dirs::video_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("Rustcord Clips")
}

pub fn folder(setting: &str) -> PathBuf {
    if setting.is_empty() { default_dir() } else { PathBuf::from(setting) }
}

fn file_stem(app: &str, id: u64) -> String {
    let name: String = app.chars().take(20).map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    format!("{}_{id}", name.trim())
}

pub fn save(ctx: &egui::Context, dir: PathBuf, me: Context) -> Result<(), &'static str> {
    if !ACTIVE.load(Relaxed) {
        return Err("no_source");
    }
    if SAVING.load(Relaxed) >= MAX_SAVES {
        return Err("busy");
    }
    let secs = SECS.load(Relaxed);
    let (frames, pcm, size, fps, app, thumb, trim) = {
        let g = REC.lock().unwrap();
        let Some(r) = g.as_ref() else { return Err("warming_up") };
        let frames = r.video.cut(secs as i64 * TICKS);
        if frames.len() < 2 {
            return Err("warming_up");
        }
        let (first, last) = (frames[0].pts, frames[frames.len() - 1].pts);
        let pcm = r.audio.take(first * RATE / TICKS, last * RATE / TICKS);
        let trim = ((last - first) - secs as i64 * TICKS).max(0) / (TICKS / 1000);
        (frames, pcm, r.size, r.fps, r.app.clone(), r.look.clone(), trim)
    };
    SAVING.fetch_add(1, Relaxed);
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("clip-save".into())
        .spawn(move || {
            let out = write(&dir, frames, pcm, size, fps, app, thumb, trim, me);
            SAVING.fetch_sub(1, Relaxed);
            READY.store(true, Relaxed);
            DONE.lock().unwrap().push(match out {
                Ok(c) => Outcome::Saved(c),
                Err(_) => Outcome::Failed,
            });
            ctx.request_repaint();
        })
        .map_err(|_| "busy")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write(dir: &std::path::Path, frames: Vec<Frame>, pcm: Vec<i16>, size: (u32, u32), fps: u32, app: String, thumb: Option<Arc<ColorImage>>, trim: i64, me: Context) -> Result<Clip, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let created = chrono::Utc::now().timestamp_millis();
    let id = (created as u64) << 12 | getrandom::u32().unwrap_or(0) as u64 & 0xfff;
    let stem = file_stem(&app, id);
    let path = dir.join(format!("{stem}.mp4"));
    mux::start().map_err(|e| e.to_string())?;
    let len = mux::write(&path, &frames, size, fps, &pcm).map_err(|e| e.to_string())?;
    let clip = Clip {
        id: id.to_string(),
        title: String::new(),
        app,
        created,
        length_ms: len / (TICKS / 1000),
        trim_start_ms: trim,
        users: me.users,
        guild: me.guild,
        channel: me.channel,
        favorite: false,
        edit: None,
        size: std::fs::metadata(&path).map_or(0, |m| m.len()),
        path: path.clone(),
        has_thumb: false,
    };
    let mut clip = clip;
    clip.has_thumb = thumb.is_some_and(|img| library::thumbnail(&img, &clip.thumb()));
    library::write_meta(&clip);
    Ok(clip)
}

#[cfg(test)]
mod tests {
    use super::*;

    static ONE: Mutex<()> = Mutex::new(());

    fn run_stream(secs: i64) {
        let frames = mux::synthetic(320, 240, 30, 30 * secs as usize);
        let t0 = now() - secs * TICKS;
        for f in &frames {
            video(&f.data, t0 + f.pts, f.key);
        }
    }

    #[test]
    fn nothing_is_held_while_clips_are_off_or_no_stream_is_live() {
        let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
        stream_end();
        configure(false, 30);
        video(&[0, 0, 1, 0x65], 0, true);
        mic(&[0.5; 960]);
        assert!(REC.lock().unwrap().is_none());
        assert!(!active());
        configure(true, 30);
        assert!(!active() && REC.lock().unwrap().is_none());
        stream_start((320, 240), 30, "test".into());
        assert!(active() && REC.lock().unwrap().is_some());
        stream_end();
        assert!(REC.lock().unwrap().is_none());
        configure(false, 30);
    }

    #[test]
    fn a_save_writes_the_buffered_seconds_with_both_tracks() {
        let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
        configure(true, 5);
        stream_start((320, 240), 30, "Some Game: 2".into());
        run_stream(8);
        let t = now() * RATE / TICKS - 8 * RATE;
        for i in 0..400 {
            add(t + i * 960, 0, &[0.3; 960]);
        }
        let dir = std::env::temp_dir().join(format!("rustcord-clips-{}", std::process::id()));
        let ctx = egui::Context::default();
        save(&ctx, dir.clone(), Context { users: vec![1, 2], guild: 0, channel: 5 }).unwrap();
        let mut got = None;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Some(o) = outcomes().pop() {
                got = Some(o);
                break;
            }
        }
        stream_end();
        configure(false, 30);
        let Some(Outcome::Saved(c)) = got else { panic!("no clip saved") };
        assert!(c.path.exists() && c.path.to_string_lossy().contains("Some Game_ 2_"));
        assert!(c.length_ms >= 5000 && c.length_ms < 7200, "{}", c.length_ms);
        assert_eq!(c.users, vec![1, 2]);
        let listed = library::scan(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, c.id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_without_a_stream_says_why() {
        let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
        stream_end();
        configure(true, 30);
        let ctx = egui::Context::default();
        assert_eq!(save(&ctx, std::env::temp_dir(), Context { users: vec![], guild: 0, channel: 0 }), Err("no_source"));
        configure(false, 30);
    }

    #[test]
    fn only_people_who_allowed_it_are_recorded() {
        let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
        set_flags(7, ALLOW_VOICE_RECORDING);
        set_flags(8, CLIPS_ENABLED);
        assert!(allowed(7) && !allowed(8) && !allowed(9));
        assert!(others_clipping(1));
        forget_flags();
        assert!(!others_clipping(1));
    }
}
