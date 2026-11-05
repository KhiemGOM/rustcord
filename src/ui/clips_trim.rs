//! Edit Clip: a frame at the handle you moved last, two trim handles on a timeline, and a title.
//! Arrow keys move the active handle 0.1 s, Shift 1 s. Saving keeps the trim beside the clip; it's
//! cut out of the file when the clip is shared.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, TextureHandle, pos2, vec2};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::clips_gallery::Gallery;
use super::icons::Icon;
use super::{App, grey_button, icon_button};
use crate::clips::{Clip, library};
use crate::theme::*;

const STEP: i64 = 100;
const MIN: i64 = 100;
const W: f32 = 640.0;

type Frames = Arc<Mutex<Option<Vec<(egui::ColorImage, Duration)>>>>;

pub struct Trim {
    id: String,
    pub title: String,
    pub len: i64,
    pub start: i64,
    pub end: i64,
    pub end_active: bool,
    frames: Frames,
    shot: Option<(usize, TextureHandle)>,
}

impl Trim {
    pub fn new(c: &Clip) -> Trim {
        let frames: Frames = Default::default();
        let (out, path) = (frames.clone(), c.path.clone());
        std::thread::spawn(move || {
            let pics = std::fs::read(&path).ok().and_then(|b| crate::mp4::frames_within(&b, 64 << 20, 320).ok()).unwrap_or_default();
            *out.lock().unwrap() = Some(pics);
        });
        let (start, end) = c.edit.unwrap_or((c.trim_start_ms, c.length_ms));
        Trim { id: c.id.clone(), title: c.title.clone(), len: c.length_ms, start, end, end_active: false, frames, shot: None }
    }

    pub fn active(&self) -> i64 {
        if self.end_active { self.end } else { self.start }
    }

    pub fn nudge(&mut self, by: i64) {
        if self.end_active {
            self.end = (self.end + by).clamp(self.start + MIN, self.len);
        } else {
            self.start = (self.start + by).clamp(0, self.end - MIN);
        }
    }

    pub fn set(&mut self, ms: i64) {
        if self.end_active {
            self.end = ms.clamp(self.start + MIN, self.len);
        } else {
            self.start = ms.clamp(0, self.end - MIN);
        }
    }

    fn near(&mut self, ms: i64) {
        self.end_active = (ms - self.end).abs() < (ms - self.start).abs();
        self.set(ms);
    }
}

fn stamp(ms: i64) -> String {
    format!("{}:{:02}.{}", ms / 60000, ms / 1000 % 60, ms / 100 % 10)
}

pub fn show(app: &mut App, ctx: &egui::Context, g: &mut Gallery) {
    let Some(t) = &mut g.edit else { return };
    let (mut save, mut close) = (false, false);
    ctx.input_mut(|i| {
        for (key, dir) in [(egui::Key::ArrowLeft, -1), (egui::Key::ArrowRight, 1)] {
            while i.consume_key(egui::Modifiers::SHIFT, key) {
                t.nudge(dir * 1000);
            }
            while i.consume_key(egui::Modifiers::NONE, key) {
                t.nudge(dir * STEP);
            }
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
            t.end_active = !t.end_active;
        }
    });
    let pos = t.active();
    let at = {
        let f = t.frames.lock().unwrap();
        f.as_ref().filter(|f| !f.is_empty()).map(|f| {
            let mut acc = Duration::ZERO;
            let ms = pos.max(0) as u128;
            let i = f.iter().position(|(_, d)| {
                acc += *d;
                acc.as_millis() > ms
            });
            let i = i.unwrap_or(f.len() - 1);
            (i, f[i].0.clone())
        })
    };
    if let Some((i, img)) = at
        && t.shot.as_ref().is_none_or(|(n, _)| *n != i)
    {
        t.shot = Some((i, ctx.load_texture("clip_frame", img, egui::TextureOptions::LINEAR)));
    }
    let loading = t.frames.lock().unwrap().is_none();
    let modal = egui::Modal::new("clips_edit".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(W);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Edit Clip").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(16.0);
            let (view, _) = ui.allocate_exact_size(vec2(W, W * 9.0 / 16.0), Sense::hover());
            ui.painter().rect_filled(view, CornerRadius::same(8), Color32::BLACK);
            match &t.shot {
                Some((_, tex)) => {
                    let s = tex.size_vec2();
                    let k = (view.width() / s.x).min(view.height() / s.y);
                    egui::Image::new(tex).paint_at(ui, Rect::from_center_size(view.center(), s * k));
                }
                None => {
                    ui.painter().text(view.center(), Align2::CENTER_CENTER, if loading { "Loading preview…" } else { "No preview" }, medium(14.0), pal().header_secondary);
                }
            }
            ui.add_space(16.0);
            let (bar, resp) = ui.allocate_exact_size(vec2(W, 40.0), Sense::click_and_drag());
            let track = Rect::from_min_max(pos2(bar.left(), bar.center().y - 4.0), pos2(bar.right(), bar.center().y + 4.0));
            let whole = t.len.max(1) as f32;
            let x = |ms: i64| bar.left() + bar.width() * ms as f32 / whole;
            ui.painter().rect_filled(track, 4, pal().bg_mod_selected);
            let kept = Rect::from_min_max(pos2(x(t.start), track.top()), pos2(x(t.end), track.bottom()));
            ui.painter().rect_filled(kept, 4, BLURPLE);
            if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.drag_started() || resp.dragged()) {
                let ms = ((p.x - bar.left()) / bar.width() * t.len as f32) as i64;
                if resp.drag_started() || resp.clicked() {
                    t.near(ms);
                } else {
                    t.set(ms);
                }
            }
            for (ms, end) in [(t.start, false), (t.end, true)] {
                let h = Rect::from_center_size(pos2(x(ms), bar.center().y), vec2(10.0, 32.0));
                let on = t.end_active == end;
                ui.painter().rect(h, 4, Color32::WHITE, Stroke::new(if on { 3.0 } else { 0.0 }, BLURPLE), egui::StrokeKind::Outside);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}{}", if t.end_active { "" } else { "▸ " }, stamp(t.start))).font(medium(14.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{}{}", stamp(t.end), if t.end_active { " ◂" } else { "" })).font(medium(14.0)).color(pal().header_primary));
                    ui.add_space(ui.available_width() / 2.0 - 40.0);
                    ui.label(RichText::new(format!("{} long", stamp(t.end - t.start))).font(reg(14.0)).color(pal().header_secondary));
                });
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Clip Title").font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let (field, _) = ui.allocate_exact_size(vec2(W, 40.0), Sense::hover());
            ui.painter().rect(field, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let inner = Rect::from_min_max(pos2(field.left() + 12.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
            ui.put(inner, egui::TextEdit::singleline(&mut t.title).hint_text(RichText::new("Untitled").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(W - 96.0 - 8.0 - 96.0 - 8.0 - 96.0);
                if grey_button(ui, "Reset", 96.0).clicked() {
                    (t.start, t.end) = (0, t.len);
                }
                if grey_button(ui, "Cancel", 96.0).clicked() {
                    close = true;
                }
                save = super::button(ui, "Save", BLURPLE, 96.0).clicked();
            });
        });
    close |= modal.should_close();
    if save {
        let whole = t.start == 0 && t.end == t.len;
        let edit = (!whole).then_some((t.start, t.end));
        let (id, title) = (t.id.clone(), t.title.trim().to_string());
        if let Some(c) = app.clips.list.as_mut().and_then(|l| l.iter_mut().find(|c| c.id == id)) {
            (c.edit, c.title) = (edit, title);
            library::write_meta(c);
        }
        g.edit = None;
    } else if close {
        g.edit = None;
    }
}

/// The file to send for `c`: the clip itself, or its trim cut out into the temp folder.
pub fn export(c: &Clip) -> Option<PathBuf> {
    let Some((from, to)) = c.edit else { return Some(c.path.clone()) };
    let dir = std::env::temp_dir().join("rustcord").join("clips");
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(c.path.file_name()?);
    crate::clips::cut::cut(&c.path, &out, from, to).ok()?;
    let mut meta = c.clone();
    meta.path = out.clone();
    meta.edit = None;
    meta.length_ms = to - from;
    library::write_meta(&meta);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trim(len: i64) -> Trim {
        Trim::new(&Clip { length_ms: len, ..Clip::default() })
    }

    #[test]
    fn handles_move_a_tenth_of_a_second_and_stay_apart_and_inside() {
        let mut t = trim(5000);
        t.nudge(STEP);
        t.nudge(1000);
        assert_eq!(t.start, 1100);
        t.end_active = true;
        t.nudge(-STEP);
        assert_eq!(t.end, 4900);
        t.nudge(100_000);
        assert_eq!(t.end, 5000);
        t.nudge(-100_000);
        assert_eq!(t.end, 1100 + MIN);
        t.end_active = false;
        t.nudge(100_000);
        assert_eq!(t.start, 1100);
        t.nudge(-100_000);
        assert_eq!(t.start, 0);
    }

    #[test]
    fn a_click_picks_the_nearer_handle() {
        let mut t = trim(10_000);
        t.near(8000);
        assert!(t.end_active);
        assert_eq!(t.end, 8000);
        t.near(1000);
        assert!(!t.end_active);
        assert_eq!(t.start, 1000);
    }
}
