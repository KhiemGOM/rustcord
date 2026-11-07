//! Discord's Edit Image dialog: drag, zoom and turn a picture under a fixed frame before it goes
//! up as an avatar, banner or server icon. `open` takes the file's bytes, the frame's width over
//! its height and what to do with the result, as a data URI.

use egui::{Color32, Frame, Key, Modifiers, Rect, RichText, Sense, Stroke, Vec2, vec2};
use image::{DynamicImage, imageops::FilterType};
use std::io::Cursor;
use std::sync::mpsc::Receiver;

use super::icons::{self, Icon};
use super::{App, button, grey_button, icon_button, take_key};
use crate::theme::*;

const VIEW_W: f32 = 392.0;

pub struct Crop {
    bytes: Vec<u8>,
    uri: String,
    size: (u32, u32),
    aspect: f32,
    zoom: f32,
    // the picture's centre from the frame's, in points
    off: Vec2,
    turns: i32,
    on_done: Option<Box<dyn FnOnce(&mut App, String) + Send>>,
    job: Option<Receiver<Option<String>>>,
}

pub fn open(app: &mut App, bytes: Vec<u8>, aspect: f32, on_done: impl FnOnce(&mut App, String) + Send + 'static) {
    let Ok(size) = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format().map_err(|_| ()).and_then(|r| r.into_dimensions().map_err(|_| ())) else {
        app.toast("That file isn't a picture rustcord can read.");
        return;
    };
    let uri = format!("bytes://crop/{}", bytes.len() ^ (size.0 as usize) << 20);
    app.crop = Some(Crop { bytes, uri, size, aspect, zoom: 1.0, off: Vec2::ZERO, turns: 0, on_done: Some(Box::new(on_done)), job: None });
}

impl Crop {
    fn turned(&self) -> Vec2 {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        if self.turns % 2 == 0 { vec2(w, h) } else { vec2(h, w) }
    }

    fn view(&self) -> Vec2 {
        vec2(VIEW_W, VIEW_W / self.aspect)
    }

    // points per picture pixel
    fn scale(&self) -> f32 {
        let (v, p) = (self.view(), self.turned());
        (v.x / p.x).max(v.y / p.y) * self.zoom
    }

    fn clamp(&mut self) {
        let room = (self.turned() * self.scale() - self.view()) / 2.0;
        self.off = self.off.clamp(-room, room);
    }

    fn render(&self) -> Option<(Vec<u8>, &'static str)> {
        let rotated = |img: DynamicImage| match self.turns.rem_euclid(4) {
            1 => img.rotate90(),
            2 => img.rotate180(),
            3 => img.rotate270(),
            _ => img,
        };
        let s = self.scale();
        let (p, v) = (self.turned(), self.view());
        let c = p / 2.0 - self.off / s;
        let (w, h) = (v.x / s, v.y / s);
        let out_w = (w.round() as u32).clamp(1, (1024.0 * self.aspect.max(1.0)) as u32);
        let out_h = ((out_w as f32 / self.aspect).round() as u32).max(1);
        let cut = |img: DynamicImage| {
            let img = rotated(img);
            let x = ((c.x - w / 2.0).round().max(0.0) as u32).min(img.width().saturating_sub(1));
            let y = ((c.y - h / 2.0).round().max(0.0) as u32).min(img.height().saturating_sub(1));
            let (cw, ch) = ((w.round() as u32).max(1).min(img.width() - x), (h.round() as u32).max(1).min(img.height() - y));
            img.crop_imm(x, y, cw, ch).resize_exact(out_w, out_h, FilterType::Lanczos3)
        };
        let mut out = Vec::new();
        if image::guess_format(&self.bytes).ok()? == image::ImageFormat::Gif {
            use image::AnimationDecoder;
            let frames = image::codecs::gif::GifDecoder::new(Cursor::new(&self.bytes)).ok()?.into_frames().collect_frames().ok()?;
            let cut_frames = frames.into_iter().map(|f| {
                let delay = f.delay();
                image::Frame::from_parts(cut(DynamicImage::ImageRgba8(f.into_buffer())).to_rgba8(), 0, 0, delay)
            });
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.set_repeat(image::codecs::gif::Repeat::Infinite).ok()?;
            enc.encode_frames(cut_frames).ok()?;
            drop(enc);
            return Some((out, "image/gif"));
        }
        let img = cut(image::load_from_memory(&self.bytes).ok()?);
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
        Some((out, "image/png"))
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(c) = &mut app.crop else { return };
    if let Some(rx) = &c.job {
        match rx.try_recv() {
            Ok(done) => {
                let then = c.on_done.take();
                app.crop = None;
                match (done, then) {
                    (Some(uri), Some(f)) => f(app, uri),
                    _ => app.toast("Couldn't edit that picture."),
                }
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Err(_) => c.job = None,
        }
    }
    let busy = c.job.is_some();
    let (mut close, mut apply) = (false, false);
    let (arrows, turn) = ctx.input_mut(|i| {
        let step = |i: &mut egui::InputState, m: Modifiers| {
            let mut d = Vec2::ZERO;
            for (k, v) in [(Key::ArrowLeft, vec2(-1.0, 0.0)), (Key::ArrowRight, vec2(1.0, 0.0)), (Key::ArrowUp, vec2(0.0, -1.0)), (Key::ArrowDown, vec2(0.0, 1.0))] {
                if take_key(i, m, k) {
                    d += v;
                }
            }
            d
        };
        let turn = if take_key(i, Modifiers::ALT, Key::ArrowRight) { 1 } else if take_key(i, Modifiers::ALT, Key::ArrowLeft) { -1 } else { 0 };
        (step(i, Modifiers::NONE) * 0.01 + step(i, Modifiers::SHIFT) * 0.05, turn)
    });
    let view = c.view();
    c.off += arrows * view;
    if turn != 0 {
        c.turns += turn;
        c.off = Vec2::ZERO;
    }
    let modal = crate::ui::motion::Modal::new("crop".into())
        .backdrop_color(Color32::from_black_alpha(184))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(VIEW_W);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Edit Image").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            let (frame, resp) = ui.allocate_exact_size(view, Sense::drag());
            super::settings::mark(ui, "crop_view", frame);
            if resp.dragged() {
                c.off += resp.drag_delta();
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            c.clamp();
            let size = c.size_f() * c.scale();
            let at = Rect::from_center_size(frame.center() + c.off, size);
            let clip = ui.painter().with_clip_rect(frame);
            let img = egui::Image::from_bytes(c.uri.clone(), c.bytes.clone()).rotate(c.turns as f32 * std::f32::consts::FRAC_PI_2, Vec2::splat(0.5));
            let mut sub = ui.new_child(egui::UiBuilder::new().max_rect(frame));
            sub.set_clip_rect(frame);
            img.paint_at(&sub, at);
            if c.aspect == 1.0 {
                let r = frame.width() / 2.0;
                let dim = Color32::from_black_alpha(160);
                for (corner, a0) in [(frame.left_top(), 0.0), (frame.right_top(), 0.25), (frame.right_bottom(), 0.5), (frame.left_bottom(), 0.75)] {
                    let arc: Vec<_> = (0..=24).map(|i| (a0 + 0.25 + i as f32 / 96.0) * std::f32::consts::TAU).map(|a| frame.center() + vec2(a.cos(), a.sin()) * r).collect();
                    for w in arc.windows(2) {
                        clip.add(egui::Shape::convex_polygon(vec![corner, w[0], w[1]], dim, Stroke::NONE));
                    }
                }
                clip.circle_stroke(frame.center(), r, Stroke::new(1.0, Color32::from_white_alpha(60)));
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 24.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Image, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_secondary);
                let before = c.zoom;
                super::settings::slider(ui, VIEW_W - 16.0 - 24.0 - 24.0 - 36.0, &mut c.zoom, 1.0, 5.0);
                c.off *= c.zoom / before;
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Image, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), pal().header_secondary);
                if icon_button(ui, Icon::Refresh, 24.0, "Rotate").clicked() {
                    c.turns += 1;
                    c.off = Vec2::ZERO;
                }
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= grey_button(ui, "Cancel", (VIEW_W - 8.0) / 2.0).clicked();
                ui.add_enabled_ui(!busy, |ui| {
                    let b = button(ui, if busy { "Applying…" } else { "Apply" }, BLURPLE, (VIEW_W - 8.0) / 2.0);
                    super::settings::mark(ui, "crop_apply", b.rect);
                    apply |= b.clicked();
                });
            });
        });
    apply |= !busy && super::enter_submits(ctx, true);
    if apply {
        let Some(c) = &mut app.crop else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        let job = Crop { bytes: std::mem::take(&mut c.bytes), uri: String::new(), size: c.size, aspect: c.aspect, zoom: c.zoom, off: c.off, turns: c.turns, on_done: None, job: None };
        c.job = Some(rx);
        std::thread::spawn(move || {
            use base64::Engine;
            let uri = job.render().map(|(b, mime)| format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)));
            let _ = tx.send(uri);
        });
    } else if close || modal.should_close() {
        app.crop = None;
    }
}

impl Crop {
    fn size_f(&self) -> Vec2 {
        vec2(self.size.0 as f32, self.size.1 as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    fn picture(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::new_rgb8(w, h).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    fn crop(w: u32, h: u32, aspect: f32) -> Crop {
        Crop { bytes: picture(w, h), uri: String::new(), size: (w, h), aspect, zoom: 1.0, off: Vec2::ZERO, turns: 0, on_done: None, job: None }
    }

    #[test]
    fn square_frame_cuts_the_middle_of_a_wide_picture() {
        let c = crop(800, 400, 1.0);
        let (png, mime) = c.render().unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(image::load_from_memory(&png).unwrap().dimensions(), (400, 400));
    }

    #[test]
    fn turning_swaps_the_picture_sides() {
        let mut c = crop(800, 400, 1.0);
        c.turns = 1;
        assert_eq!(c.turned(), vec2(400.0, 800.0));
        let (png, _) = c.render().unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().dimensions(), (400, 400));
    }

    #[test]
    fn panning_stops_at_the_picture_edge() {
        let mut c = crop(800, 400, 1.0);
        c.off = vec2(9999.0, 9999.0);
        c.clamp();
        assert_eq!(c.off.y, 0.0);
        assert!(c.off.x > 0.0 && c.off.x < 9999.0);
    }
}
