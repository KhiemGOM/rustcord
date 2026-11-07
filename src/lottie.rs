//! Lottie stickers (format 3): parsed with velato, drawn on the CPU by vello_cpu (already in the
//! tree for egui's text) one frame at a time, at the size they're shown.

use egui::ColorImage;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Duration;
use vello_cpu::kurbo::{Affine, Shape};
use vello_cpu::peniko::{BlendMode, Brush};
use vello_cpu::{PaintType, Pixmap, RenderContext, Resources};
use velato::model::{ImageAsset, fixed};
use velato::{Composition, RenderSink, Renderer};

const FPS: f64 = 30.0;
const MAX_FRAMES: usize = 300;

pub fn sniff(b: &[u8]) -> bool {
    b.first() == Some(&b'{') && b[..b.len().min(512)].windows(4).any(|w| w == b"\"fr\"")
}

struct Sink {
    cx: RenderContext,
}

impl RenderSink for Sink {
    fn push_layer(&mut self, blend: impl Into<BlendMode>, alpha: f32, transform: Affine, shape: &impl Shape) {
        self.cx.set_transform(transform);
        self.cx.push_layer(Some(&shape.to_path(0.1)), Some(blend.into()), Some(alpha), None, None);
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        self.cx.set_transform(transform);
        self.cx.push_clip_layer(&shape.to_path(0.1));
    }

    fn pop_layer(&mut self) {
        self.cx.pop_layer();
    }

    fn draw(&mut self, stroke: Option<&fixed::Stroke>, transform: Affine, brush: &fixed::Brush, shape: &impl Shape) {
        let paint = match brush {
            Brush::Solid(c) => PaintType::Solid(*c),
            Brush::Gradient(g) => PaintType::Gradient(g.clone()),
            Brush::Image(_) => return,
        };
        self.cx.set_transform(transform);
        self.cx.set_paint(paint);
        let path = shape.to_path(0.1);
        match stroke {
            Some(s) => {
                self.cx.set_stroke(s.clone());
                self.cx.stroke_path(&path);
            }
            None => self.cx.fill_path(&path),
        }
    }

    fn draw_image(&mut self, _: &ImageAsset, _: Affine, _: f64) {}
}

/// An animation drawn at one size: frame 0 is ready when it exists, the rest are drawn in the
/// background the first time something past frame 0 is asked for.
pub struct Player {
    comp: Arc<Composition>,
    side: u32,
    step: f64,
    frames: Mutex<Vec<Option<Arc<ColorImage>>>>,
    filling: AtomicBool,
}

fn draw(comp: &Composition, frame: f64, side: u32) -> ColorImage {
    let side = side.clamp(8, 1024) as u16;
    let (w, h) = (comp.width.max(1) as f64, comp.height.max(1) as f64);
    let k = f64::from(side) / w.max(h);
    let at = Affine::translate(((f64::from(side) - w * k) / 2.0, (f64::from(side) - h * k) / 2.0)) * Affine::scale(k);
    let mut sink = Sink { cx: RenderContext::new(side, side) };
    Renderer::new().append(comp, frame, at, 1.0, &mut sink);
    sink.cx.flush();
    let mut out = Pixmap::new(side, side);
    sink.cx.render(&mut out, &mut Resources::new());
    ColorImage::from_rgba_premultiplied([side as usize, side as usize], out.data_as_u8_slice())
}

impl Player {
    pub fn new(bytes: &[u8], side: u32) -> Result<(Self, Vec<Duration>), String> {
        let comp = Composition::from_slice(bytes).map_err(|e| e.to_string())?;
        let fps = if comp.frame_rate > 0.0 { comp.frame_rate } else { FPS };
        let step = (fps / FPS).max(1.0);
        let span = (comp.frames.end - comp.frames.start).max(1.0);
        let n = ((span / step).ceil() as usize).clamp(1, MAX_FRAMES);
        let mut frames = vec![None; n];
        frames[0] = Some(Arc::new(draw(&comp, comp.frames.start, side)));
        let each = Duration::from_secs_f64(step / fps);
        let p = Player { comp: Arc::new(comp), side, step, frames: Mutex::new(frames), filling: AtomicBool::new(false) };
        Ok((p, vec![each; n]))
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn bytes(&self) -> usize {
        self.frames.lock().unwrap().iter().flatten().map(|f| f.pixels.len() * 4).sum()
    }

    pub fn frame(self: &Arc<Self>, ctx: &egui::Context, i: usize) -> Arc<ColorImage> {
        let n = self.frames.lock().unwrap().len();
        let i = i % n;
        let (have, first) = {
            let f = self.frames.lock().unwrap();
            (f[i].clone(), f[0].clone())
        };
        if let Some(f) = have {
            return f;
        }
        if !self.filling.swap(true, Relaxed) {
            let (p, ctx) = (self.clone(), ctx.clone());
            std::thread::spawn(move || {
                for k in 1..n {
                    let img = Arc::new(draw(&p.comp, p.comp.frames.start + k as f64 * p.step, p.side));
                    p.frames.lock().unwrap()[k] = Some(img);
                    if k % 8 == 0 {
                        ctx.request_repaint();
                    }
                }
                ctx.request_repaint();
            });
        }
        let f = self.frames.lock().unwrap();
        f[..=i].iter().rev().flatten().next().cloned().or(first).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &[u8] = include_bytes!("../examples/fixtures/sticker.json");

    #[test]
    fn the_mock_sticker_is_recognised() {
        assert!(sniff(DOC));
        assert!(!sniff(b"{\"message\": \"404\"}"));
        assert!(!sniff(b"\x89PNG"));
    }

    #[test]
    fn frames_have_art_and_move() {
        let (p, durations) = Player::new(DOC, 160).unwrap();
        assert_eq!(durations.len(), 60);
        let first = p.frames.lock().unwrap()[0].clone().unwrap();
        let lit = first.pixels.iter().filter(|c| c.a() > 0).count();
        assert!(lit > 160 * 160 / 8, "frame 0 is nearly empty: {lit} pixels");
        let later = draw(&p.comp, 30.0, 160);
        assert!(first.pixels.iter().zip(&later.pixels).filter(|(a, b)| a != b).count() > 200);
    }
}
