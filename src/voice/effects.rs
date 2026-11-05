//! Camera choice and video backgrounds. A person mask comes from MediaPipe's selfie model, run
//! by the Windows ML runtime that ships with Windows, and a blur or a picture goes behind them.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use windows::AI::MachineLearning::*;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference};
use windows::core::{HSTRING, Interface};

const MODEL: &[u8] = include_bytes!("../../assets/selfie_segmentation.onnx");
const SIDE: usize = 256;

pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub bgra: Vec<u8>,
}

#[derive(Clone)]
pub enum Bg {
    None,
    Blur,
    Picture(Arc<Picture>),
}

struct Cfg {
    device: Option<String>,
    bg: Bg,
    rev: u64,
}

static CFG: Mutex<Cfg> = Mutex::new(Cfg { device: None, bg: Bg::None, rev: 0 });

pub static BROKEN: AtomicBool = AtomicBool::new(false);
pub static PLACED: AtomicU64 = AtomicU64::new(0);
pub static LOADS: AtomicU64 = AtomicU64::new(0);

static OPENED: Mutex<Option<String>> = Mutex::new(None);

pub const FAKE_CAMERAS: [&str; 2] = ["Fake Camera", "Fake Camera 2"];

pub fn note_open(name: String) {
    *OPENED.lock().unwrap_or_else(|e| e.into_inner()) = Some(name);
}

pub fn last_open() -> Option<String> {
    OPENED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn rev() -> u64 {
    cfg().rev
}

pub fn mode() -> &'static str {
    match cfg().bg {
        Bg::None => "none",
        Bg::Blur => "blur",
        Bg::Picture(_) => "picture",
    }
}

fn cfg() -> std::sync::MutexGuard<'static, Cfg> {
    CFG.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_device(name: Option<String>) {
    cfg().device = name;
}

pub fn device() -> Option<String> {
    cfg().device.clone()
}

pub fn set_background(bg: Bg) {
    let mut c = cfg();
    c.bg = bg;
    c.rev += 1;
}

/// A picture scaled to cover 1280x720, in the BGRA the frames use.
pub fn picture(bytes: &[u8]) -> Option<Picture> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.resize_to_fill(1280, 720, image::imageops::FilterType::Triangle).to_rgba8();
    let mut bgra = img.into_raw();
    bgra.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
    Some(Picture { w: 1280, h: 720, bgra })
}

struct Seg {
    sess: LearningModelSession,
    input: Vec<f32>,
}

impl Seg {
    fn new(kind: LearningModelDeviceKind) -> windows::core::Result<Seg> {
        let stream = InMemoryRandomAccessStream::new()?;
        let w = DataWriter::CreateDataWriter(&stream)?;
        w.WriteBytes(MODEL)?;
        w.StoreAsync()?.join()?;
        w.DetachStream()?;
        stream.Seek(0)?;
        let model = LearningModel::LoadFromStream(&RandomAccessStreamReference::CreateFromStream(&stream)?)?;
        let dev = LearningModelDevice::Create(kind)?;
        LOADS.fetch_add(1, Relaxed);
        Ok(Seg { sess: LearningModelSession::CreateFromModelOnDevice(&model, &dev)?, input: vec![0.0; 3 * SIDE * SIDE] })
    }

    fn mask(&mut self, bgra: &[u8], w: usize, h: usize, out: &mut [f32]) -> windows::core::Result<()> {
        let plane = SIDE * SIDE;
        for y in 0..SIDE {
            let row = &bgra[(y * h / SIDE) * w * 4..];
            for x in 0..SIDE {
                let p = &row[(x * w / SIDE) * 4..][..3];
                let i = y * SIDE + x;
                self.input[i] = p[2] as f32 / 255.0;
                self.input[plane + i] = p[1] as f32 / 255.0;
                self.input[2 * plane + i] = p[0] as f32 / 255.0;
            }
        }
        let bind = LearningModelBinding::CreateFromSession(&self.sess)?;
        bind.Bind(&HSTRING::from("pixel_values"), &TensorFloat::CreateFromShapeArrayAndDataArray(&[1, 3, SIDE as i64, SIDE as i64], &self.input)?)?;
        let res = self.sess.Evaluate(&bind, &HSTRING::new())?;
        let alphas: TensorFloat = res.Outputs()?.Lookup(&HSTRING::from("alphas"))?.cast()?;
        alphas.GetAsVectorView()?.GetMany(0, out)?;
        Ok(())
    }
}

struct Fx {
    seg: Option<Seg>,
    tried: bool,
    gpu: bool,
    mask: Vec<f32>,
    reset: bool,
    small: Vec<u8>,
    back: (usize, usize, u64, Vec<u8>),
}

thread_local! {
    static FX: RefCell<Fx> = const { RefCell::new(Fx { seg: None, tried: false, gpu: false, mask: Vec::new(), reset: true, small: Vec::new(), back: (0, 0, 0, Vec::new()) }) };
}

/// Puts the chosen background behind whoever is in `bgra` (w*h, packed). The model loads on
/// first use, on this thread, because WinRT objects stay where they were made.
pub fn apply(bgra: &mut [u8], w: usize, h: usize) {
    let (bg, rev) = {
        let c = cfg();
        (c.bg.clone(), c.rev)
    };
    if matches!(bg, Bg::None) || w < SIDE || h < SIDE {
        return;
    }
    FX.with_borrow_mut(|fx| fx.run(&bg, rev, bgra, w, h));
}

impl Fx {
    fn run(&mut self, bg: &Bg, rev: u64, bgra: &mut [u8], w: usize, h: usize) {
        if !self.tried {
            self.tried = true;
            match Seg::new(LearningModelDeviceKind::DirectXHighPerformance).map(|s| (s, true)).or_else(|_| Seg::new(LearningModelDeviceKind::Cpu).map(|s| (s, false))) {
                Ok((s, gpu)) => (self.seg, self.gpu) = (Some(s), gpu),
                Err(_) => BROKEN.store(true, Relaxed),
            }
            self.mask = vec![0.0; SIDE * SIDE];
        }
        let Some(seg) = &mut self.seg else { return };
        let mut now = vec![0.0; SIDE * SIDE];
        if seg.mask(bgra, w, h, &mut now).is_err() {
            if self.gpu {
                self.gpu = false;
                self.seg = Seg::new(LearningModelDeviceKind::Cpu).ok();
            } else {
                BROKEN.store(true, Relaxed);
            }
            return;
        }
        BROKEN.store(false, Relaxed);
        PLACED.fetch_add(1, Relaxed);
        // a little of the last mask carries over, so the edge doesn't shimmer
        for (m, f) in self.mask.iter_mut().zip(&now) {
            *m = if self.reset { *f } else { *m * 0.4 + *f * 0.6 };
        }
        self.reset = false;
        match bg {
            Bg::Picture(p) => {
                if (self.back.0, self.back.1, self.back.2) != (w, h, rev) {
                    self.back = (w, h, rev, scaled(p, w, h));
                }
                blend(bgra, &self.mask, w, h, Back::Pic(&self.back.3));
            }
            _ => {
                let (sw, sh) = (w / 8, h / 8);
                shrink(bgra, w, h, sw, sh, &mut self.small);
                for _ in 0..3 {
                    box_blur(&mut self.small, sw, sh, 3);
                }
                blend(bgra, &self.mask, w, h, Back::Small(&self.small, sw, sh));
            }
        }
    }
}

fn scaled(p: &Picture, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![255; w * h * 4];
    for y in 0..h {
        let row = &p.bgra[(y * p.h / h) * p.w * 4..];
        for x in 0..w {
            let o = (y * w + x) * 4;
            out[o..o + 3].copy_from_slice(&row[(x * p.w / w) * 4..][..3]);
        }
    }
    out
}

fn shrink(src: &[u8], w: usize, h: usize, sw: usize, sh: usize, out: &mut Vec<u8>) {
    out.resize(sw * sh * 4, 255);
    let (kx, ky) = (w / sw, h / sh);
    for y in 0..sh {
        for x in 0..sw {
            let mut sum = [0u32; 3];
            for dy in (0..ky).step_by(2) {
                for dx in (0..kx).step_by(2) {
                    let o = ((y * ky + dy) * w + x * kx + dx) * 4;
                    sum[0] += src[o] as u32;
                    sum[1] += src[o + 1] as u32;
                    sum[2] += src[o + 2] as u32;
                }
            }
            let n = (ky.div_ceil(2) * kx.div_ceil(2)) as u32;
            let o = (y * sw + x) * 4;
            out[o..o + 3].copy_from_slice(&[(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]);
        }
    }
}

fn box_blur(img: &mut [u8], w: usize, h: usize, r: usize) {
    let mut tmp = img.to_vec();
    let run = |src: &[u8], dst: &mut [u8], len: usize, step: usize, lines: usize, stride: usize| {
        for line in 0..lines {
            for c in 0..3 {
                let at = |i: usize| src[line * stride + i * step + c] as u32;
                let mut sum: u32 = (0..=r.min(len - 1)).map(at).sum::<u32>() + at(0) * r as u32;
                for i in 0..len {
                    dst[line * stride + i * step + c] = (sum / (2 * r as u32 + 1)) as u8;
                    sum += at((i + r + 1).min(len - 1));
                    sum -= at(i.saturating_sub(r));
                }
            }
        }
    };
    run(img, &mut tmp, w, 4, h, w * 4);
    run(&tmp, img, h, w * 4, w, 4);
}

enum Back<'a> {
    Pic(&'a [u8]),
    Small(&'a [u8], usize, usize),
}

// the mask is bilinear up to the frame; its soft edge is tightened so hair and shoulders keep
// their outline
fn blend(bgra: &mut [u8], mask: &[f32], w: usize, h: usize, back: Back) {
    let alpha: Vec<f32> = mask
        .iter()
        .map(|m| {
            let a = ((m - 0.3) / 0.4).clamp(0.0, 1.0);
            a * a * (3.0 - 2.0 * a)
        })
        .collect();
    let at = |n: usize, side: usize| {
        let f = ((n as f32 + 0.5) * SIDE as f32 / side as f32 - 0.5).clamp(0.0, SIDE as f32 - 1.001);
        (f as usize, f.fract())
    };
    let xs: Vec<(usize, f32)> = (0..w).map(|x| at(x, w)).collect();
    let sx: Vec<usize> = match back {
        Back::Small(_, sw, _) => (0..w).map(|x| x * sw / w).collect(),
        Back::Pic(_) => Vec::new(),
    };
    let rows = h.div_ceil(std::thread::available_parallelism().map_or(1, |n| n.get().min(4)));
    std::thread::scope(|sc| {
        for (k, band) in bgra.chunks_mut(rows * w * 4).enumerate() {
            let (alpha, xs, sx, back) = (&alpha, &xs, &sx, &back);
            sc.spawn(move || {
                for (ry, row) in band.chunks_exact_mut(w * 4).enumerate() {
                    let y = k * rows + ry;
                    let (y0, fy) = at(y, h);
                    let (r0, r1) = (&alpha[y0 * SIDE..(y0 + 1) * SIDE], &alpha[(y0 + 1) * SIDE..(y0 + 2) * SIDE]);
                    for (x, &(x0, fx)) in xs.iter().enumerate() {
                        let top = r0[x0] + (r0[x0 + 1] - r0[x0]) * fx;
                        let bot = r1[x0] + (r1[x0 + 1] - r1[x0]) * fx;
                        let a = top + (bot - top) * fy;
                        if a >= 0.996 {
                            continue;
                        }
                        let b = match *back {
                            Back::Pic(p) => &p[(y * w + x) * 4..],
                            Back::Small(img, sw, sh) => &img[((y * sh / h) * sw + sx[x]) * 4..],
                        };
                        let px = &mut row[x * 4..x * 4 + 3];
                        for c in 0..3 {
                            px[c] = (px[c] as f32 * a + b[c] as f32 * (1.0 - a)) as u8;
                        }
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn frame(w: usize, h: usize) -> Vec<u8> {
        let mut f = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                let inside = (x as f32 - w as f32 / 2.0).powi(2) / (w as f32 / 6.0).powi(2) + (y as f32 - h as f32 / 2.0).powi(2) / (h as f32 / 3.0).powi(2) < 1.0;
                let px = if inside { [90, 120, 200] } else { [(x * 255 / w) as u8, 140, (y * 255 / h) as u8] };
                f[o..o + 3].copy_from_slice(&px);
            }
        }
        f
    }

    #[test]
    fn blend_keeps_the_person_and_replaces_the_rest() {
        let (w, h) = (320, 288);
        let src = frame(w, h);
        let pic = vec![7u8; w * h * 4];
        let mut gone = src.clone();
        blend(&mut gone, &vec![0.0; SIDE * SIDE], w, h, Back::Pic(&pic));
        assert!(gone.chunks_exact(4).all(|p| p[..3] == [7, 7, 7]));
        let mut kept = src.clone();
        blend(&mut kept, &vec![1.0; SIDE * SIDE], w, h, Back::Pic(&pic));
        assert_eq!(kept, src);
    }

    #[test]
    fn blend_follows_the_mask() {
        let (w, h) = (256, 256);
        let src = vec![200u8; w * h * 4];
        let pic = vec![10u8; w * h * 4];
        let mask: Vec<f32> = (0..SIDE * SIDE).map(|i| if i % SIDE < SIDE / 2 { 1.0 } else { 0.0 }).collect();
        let mut out = src.clone();
        blend(&mut out, &mask, w, h, Back::Pic(&pic));
        assert_eq!(out[(10 * w + 10) * 4], 200);
        assert_eq!(out[(10 * w + 250) * 4], 10);
    }

    fn lap(f: impl FnOnce()) -> f64 {
        let t = Instant::now();
        f();
        t.elapsed().as_secs_f64() * 1000.0
    }

    fn stats(mut v: Vec<f64>) -> String {
        v.sort_by(f64::total_cmp);
        format!("median {:.2} ms, min {:.2} ms", v[v.len() / 2], v[0])
    }

    /// cargo test --release --bin rustcord seg_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn seg_bench() {
        for (name, kind) in [("cpu", LearningModelDeviceKind::Cpu), ("directx", LearningModelDeviceKind::DirectX), ("directx high perf", LearningModelDeviceKind::DirectXHighPerformance)] {
            let t = Instant::now();
            let Ok(mut seg) = Seg::new(kind) else {
                println!("{name}: no session");
                continue;
            };
            let load = t.elapsed();
            let (w, h) = (1280, 720);
            let img = frame(w, h);
            let mut out = vec![0.0; SIDE * SIDE];
            for _ in 0..5 {
                seg.mask(&img, w, h, &mut out).unwrap();
            }
            let times = (0..100).map(|_| lap(|| seg.mask(&img, w, h, &mut out).unwrap())).collect();
            println!("{name}: load {load:?}, mask {}", stats(times));
        }
        for (w, h) in [(640, 360), (1280, 720)] {
            let src = frame(w, h);
            let mut fx = Fx { seg: Seg::new(LearningModelDeviceKind::DirectXHighPerformance).ok(), tried: true, gpu: true, mask: vec![0.0; SIDE * SIDE], reset: true, small: Vec::new(), back: (0, 0, 0, Vec::new()) };
            let times = (0..60).map(|_| lap(|| fx.run(&Bg::Blur, 1, &mut src.clone(), w, h))).collect();
            println!("blur {w}x{h}: {}", stats(times));
        }
    }
}
