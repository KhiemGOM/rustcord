//! A look at the camera outside a call, for settings' Test Video and the preview before the
//! camera goes on. It opens the same capture the call does, backgrounds included, and stops
//! on its own once nobody has asked for a frame for a moment.

use egui::ColorImage;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::capture::Grab;
use super::video::Source;

const HEIGHT: usize = 360;
const IDLE: Duration = Duration::from_millis(1500);

pub static RUNNING: AtomicU32 = AtomicU32::new(0);
static GONE: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

#[derive(Default)]
struct Out {
    frame: Mutex<Option<Arc<ColorImage>>>,
    error: Mutex<Option<String>>,
    asked: AtomicU64,
    done: AtomicBool,
}

pub struct Preview {
    out: Arc<Out>,
    th: Option<JoinHandle<()>>,
}

/// Waits for dropped previews to let go of the camera, so the next opener doesn't find it busy.
pub fn settle() {
    let before = std::mem::take(&mut *GONE.lock().unwrap());
    before.into_iter().for_each(|t| _ = t.join());
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Preview {
    pub fn start(ctx: &egui::Context) -> Preview {
        let out = Arc::new(Out::default());
        out.asked.store(now(), Relaxed);
        let (o, ctx) = (out.clone(), ctx.clone());
        RUNNING.fetch_add(1, Relaxed);
        let th = std::thread::Builder::new().name("camera-preview".into()).spawn(move || {
            run(&o, &ctx);
            RUNNING.fetch_sub(1, Relaxed);
            o.done.store(true, Relaxed);
            ctx.request_repaint();
        });
        Preview { out, th: th.ok() }
    }

    /// The newest frame; asking keeps the camera running.
    pub fn frame(&self) -> Option<Arc<ColorImage>> {
        self.out.asked.store(now(), Relaxed);
        self.out.frame.lock().ok()?.clone()
    }

    pub fn error(&self) -> Option<String> {
        self.out.error.lock().ok()?.clone()
    }

    pub fn ended(&self) -> bool {
        self.out.done.load(Relaxed)
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.out.asked.store(0, Relaxed);
        if let Some(t) = self.th.take() {
            let mut gone = GONE.lock().unwrap();
            gone.retain(|t| !t.is_finished());
            gone.push(t);
        }
    }
}

fn run(out: &Out, ctx: &egui::Context) {
    let fake = std::env::var_os("RUSTCORD_FAKE_VIDEO").is_some() || std::env::var_os("RUSTCORD_FAKE_CAM").is_some();
    let opened = if fake { Ok(Grab::fake(false)) } else { Grab::open(Source::Camera) };
    let mut grab = match opened {
        Ok(g) => g,
        Err(e) => {
            if let Ok(mut m) = out.error.lock() {
                *m = Some(e);
            }
            return;
        }
    };
    let (sw, sh) = grab.size();
    let (w, h) = (((sw * HEIGHT / sh.max(1)).max(2)) & !1, HEIGHT);
    let mut bgra = vec![0; w * h * 4];
    let mut last = Instant::now() - Duration::from_secs(1);
    loop {
        if now().saturating_sub(out.asked.load(Relaxed)) > IDLE.as_millis() as u64 {
            return;
        }
        if !grab.paced() {
            std::thread::sleep(Duration::from_millis(40));
        }
        if !grab.frame(&mut bgra, w, h) || last.elapsed() < Duration::from_millis(45) {
            continue;
        }
        last = Instant::now();
        let px = bgra.chunks_exact(4).map(|p| egui::Color32::from_rgb(p[2], p[1], p[0])).collect();
        if let Ok(mut f) = out.frame.lock() {
            *f = Some(Arc::new(ColorImage::new([w, h], px)));
        }
        ctx.request_repaint();
    }
}
