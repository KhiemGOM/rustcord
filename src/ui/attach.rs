use egui::{Align, Align2, Color32, Frame, Key, Layout, Margin, Modifiers, Rect, RichText, Sense, Stroke, pos2, vec2};

use super::composer::Upload;
use super::icons::{self, Icon};
use super::{App, button, icon_button, take_key};
use crate::theme::*;

pub struct Modify {
    idx: usize,
    name: String,
    pub alt: String,
    spoiler: bool,
    fresh: bool,
}

impl Modify {
    pub fn new(idx: usize, f: &Upload) -> Self {
        Modify { idx, name: f.name.clone(), alt: f.alt.clone(), spoiler: f.spoiler, fresh: true }
    }
}

fn label(ui: &mut egui::Ui, s: &str) {
    ui.add_space(16.0);
    ui.label(RichText::new(s).font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
}

pub fn modify(app: &mut App, ctx: &egui::Context) {
    let Some(m) = &mut app.modify else { return };
    let Some(f) = app.files.get(m.idx) else {
        app.modify = None;
        return;
    };
    let preview = f.thumb_bytes();
    let uri = format!("bytes://upload/{}", f.path.to_string_lossy());
    let (mut save, mut close) = (false, false);
    let modal = crate::ui::motion::Modal::new("modify_attachment".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Modify Attachment").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(16.0);
            let (pr, _) = ui.allocate_exact_size(vec2(432.0, 160.0), Sense::hover());
            let box_ = Rect::from_min_size(pr.min + vec2(16.0, 0.0), vec2(160.0, 160.0));
            match preview {
                Some(b) => egui::Image::from_bytes(uri, b).corner_radius(80).maintain_aspect_ratio(true).show_loading_spinner(false).paint_at(ui, box_),
                None => icons::draw(ui.painter(), Icon::File, Rect::from_center_size(box_.center(), vec2(72.0, 72.0)), BLURPLE),
            }
            label(ui, "Filename");
            let single = |ui: &mut egui::Ui, text: &mut String, focus: bool| {
                let (r, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect_filled(r, 8, pal().input_bg);
                let e = ui.put(r.shrink2(vec2(12.0, 10.0)), egui::TextEdit::singleline(text).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
                ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if e.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
                if focus {
                    e.request_focus();
                }
            };
            single(ui, &mut m.name, std::mem::take(&mut m.fresh));
            label(ui, "Description (Alt Text)");
            let (r, _) = ui.allocate_exact_size(vec2(432.0, 88.0), Sense::hover());
            ui.painter().rect_filled(r, 8, pal().input_bg);
            let e = ui.put(
                r.shrink2(vec2(12.0, 10.0)),
                egui::TextEdit::multiline(&mut m.alt).hint_text(RichText::new("Add a description").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO).desired_rows(3),
            );
            ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if e.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
            if m.alt.chars().count() > 1024 {
                m.alt = m.alt.chars().take(1024).collect();
            }
            ui.add_space(16.0);
            super::poll::check_row(ui, &mut m.spoiler, "Mark as spoiler");
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= super::grey_button(ui, "Cancel", 212.0).clicked();
                let b = button(ui, "Save", BLURPLE, 212.0);
                super::settings::mark(ui, "modify:save", b.rect);
                save = b.clicked();
            });
        });
    if save && let Some(m) = app.modify.take() {
        if let Some(f) = app.files.get_mut(m.idx) {
            if !m.name.trim().is_empty() {
                f.name = m.name.trim().to_string();
            }
            f.alt = m.alt;
            f.spoiler = m.spoiler;
        }
    } else if close || modal.should_close() {
        app.modify = None;
    }
}

/// The card keys, for the one that has focus: D or Backspace takes it off, Ctrl+Backspace all of
/// them, E opens its Modify Attachment.
pub fn card_keys(app: &mut App, ctx: &egui::Context, idx: usize) {
    let (d, bs, all, e) = ctx.input_mut(|i| {
        let all = take_key(i, Modifiers::COMMAND, Key::Backspace);
        (take_key(i, Modifiers::NONE, Key::D), take_key(i, Modifiers::NONE, Key::Backspace), all, take_key(i, Modifiers::NONE, Key::E))
    });
    if all {
        app.files.clear();
    } else if d || bs {
        app.files.remove(idx);
    } else if e && let Some(f) = app.files.get(idx) {
        app.modify = Some(Modify::new(idx, f));
    }
}

/// Discord's overlay while files are dragged over the window.
pub fn dragging(app: &App, ctx: &egui::Context) {
    if ctx.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let Some(chan) = app.cur().and_then(|c| app.store.channels.get(&c)) else { return };
    let to = match chan.kind {
        1 | 3 => format!("Upload to {}", super::channel_title(&app.store, chan.id)),
        _ if chan.is_thread() => "Upload to thread".to_string(),
        _ => format!("Upload to #{}", chan.name),
    };
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_overlay")));
    let r = ctx.content_rect();
    p.rect_filled(r, 0, Color32::from_rgba_unmultiplied(0, 0, 0, 200));
    let inner = r.shrink(16.0);
    p.rect_stroke(inner, 8, Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
    let c = inner.center();
    icons::draw(&p, Icon::Upload, Rect::from_center_size(pos2(c.x, c.y - 48.0), vec2(48.0, 48.0)), Color32::WHITE);
    p.text(pos2(c.x, c.y - 4.0), Align2::CENTER_CENTER, to, semi(24.0), Color32::WHITE);
    p.text(pos2(c.x, c.y + 24.0), Align2::CENTER_CENTER, "You can add comments before uploading.", reg(16.0), pal().header_secondary);
    p.text(pos2(c.x, c.y + 46.0), Align2::CENTER_CENTER, "Hold shift to upload directly.", reg(16.0), pal().header_secondary);
}
