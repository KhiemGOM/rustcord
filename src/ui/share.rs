//! Discord's Go Live picker: Applications, Entire Screen and Devices as tabs over a grid of live
//! thumbnails, the stream's quality along the bottom with a gear to change it.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align2, Color32, ColorImage, Frame, Rect, Sense, Stroke, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::shareable::{self, Kind};
use crate::theme::*;

pub struct Picker {
    list: shareable::Shareable,
    pub tab: usize,
    // textures by item, with the image they were made from
    tex: HashMap<String, (usize, egui::TextureHandle)>,
}

impl Picker {
    pub fn new(ctx: &egui::Context) -> Self {
        Picker { list: shareable::start(ctx.clone()), tab: 0, tex: HashMap::new() }
    }
}

const TABS: [(&str, Icon); 3] = [("Applications", Icon::Media), ("Entire Screen", Icon::Screen), ("Devices", Icon::Video)];

fn texture(tex: &mut HashMap<String, (usize, egui::TextureHandle)>, ctx: &egui::Context, key: String, img: &Arc<ColorImage>) -> egui::TextureId {
    let ptr = Arc::as_ptr(img) as usize;
    match tex.get(&key) {
        Some((p, t)) if *p == ptr => t.id(),
        _ => {
            let t = ctx.load_texture(key.clone(), (**img).clone(), egui::TextureOptions::LINEAR);
            let id = t.id();
            tex.insert(key, (ptr, t));
            id
        }
    }
}

/// `inner` fitted inside `outer`, keeping its shape, centred.
fn contain(outer: Rect, size: egui::Vec2) -> Rect {
    let k = (outer.width() / size.x).min(outer.height() / size.y);
    Rect::from_center_size(outer.center(), size * k)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(p) = &mut app.share else { return };
    let items = p.list.items.lock().unwrap_or_else(|e| e.into_inner());
    let shown: Vec<(usize, &shareable::Item)> = items
        .iter()
        .filter(|i| match (p.tab, i.kind) {
            (0, Kind::Window(_)) | (1, Kind::Screen(_)) | (2, Kind::Device(_)) => true,
            _ => false,
        })
        .enumerate()
        .collect();
    let (height, fps) = (app.settings.stream_height, app.settings.stream_fps);
    let mut pick = None;
    let mut quality = None;
    let mut rects = Vec::new();
    // measured from discord's: 958 wide, 24 in, 438 by 246 tiles two to a row
    let modal = crate::ui::motion::Modal::new("share_picker".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(910.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (bar, _) = ui.allocate_exact_size(vec2(910.0, 40.0), Sense::hover());
            ui.painter().rect_filled(bar, 8, pal().bg_tertiary);
            let w = (bar.width() - 8.0) / 3.0;
            for (i, (label, icon)) in TABS.into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(bar.left() + 4.0 + i as f32 * w, bar.top() + 4.0), vec2(w, 32.0));
                let resp = ui.interact(r, ui.id().with(("share_tab", i)), Sense::click());
                let on = p.tab == i;
                if on {
                    ui.painter().rect_filled(r, 6, pal().bg_secondary);
                }
                let col = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), col);
                let x = r.center().x - (g.size().x + 24.0) / 2.0;
                icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(16.0, 16.0)), col);
                ui.painter().galley(pos2(x + 24.0, r.center().y - g.size().y / 2.0), g, col);
                rects.push((format!("share:tab:{i}"), r));
                if resp.clicked() {
                    p.tab = i;
                }
            }
            ui.add_space(24.0);
            let room = (ui.ctx().content_rect().height() - 330.0).clamp(280.0, 594.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(910.0);
                if shown.is_empty() {
                    let (r, _) = ui.allocate_exact_size(vec2(910.0, 120.0), Sense::hover());
                    let what = ["No windows to share", "No screens found", "No capture devices found"][p.tab];
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, what, medium(16.0), pal().header_secondary);
                }
                for row in shown.chunks(2) {
                    let (line, _) = ui.allocate_exact_size(vec2(910.0, 297.0), Sense::hover());
                    for (k, (n, item)) in row.iter().enumerate() {
                        let tile = Rect::from_min_size(pos2(line.left() + k as f32 * 454.0, line.top()), vec2(438.0, 246.0));
                        let resp = ui.interact(tile, ui.id().with(("share_item", p.tab, *n)), Sense::click());
                        ui.painter().rect_filled(tile, 8, Color32::BLACK);
                        let key = format!("{:?}", item.kind);
                        match (&item.thumb, &item.icon) {
                            (Some(img), _) => {
                                let id = texture(&mut p.tex, ui.ctx(), format!("{key}:thumb"), img);
                                let r = contain(tile, vec2(img.size[0] as f32, img.size[1] as f32));
                                egui::Image::new(egui::load::SizedTexture::new(id, r.size())).corner_radius(8).paint_at(ui, r);
                            }
                            // minimised: the app's icon, as tall as the tile
                            (None, Some(img)) => {
                                let id = texture(&mut p.tex, ui.ctx(), format!("{key}:icon"), img);
                                let r = Rect::from_center_size(tile.center(), vec2(tile.height(), tile.height()));
                                egui::Image::new(egui::load::SizedTexture::new(id, r.size())).paint_at(ui, r);
                            }
                            (None, None) => {
                                let glyph = if p.tab == 2 { Icon::Video } else { Icon::Screen };
                                icons::draw(ui.painter(), glyph, Rect::from_center_size(tile.center(), vec2(48.0, 48.0)), pal().header_secondary);
                            }
                        }
                        if resp.hovered() {
                            ui.painter().rect_stroke(tile, 8, Stroke::new(2.0, pal().blurple), egui::StrokeKind::Inside);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let y = tile.bottom() + 18.0;
                        let ir = Rect::from_center_size(pos2(tile.left() + 9.0, y), vec2(16.0, 16.0));
                        match &item.icon {
                            Some(img) => {
                                let id = texture(&mut p.tex, ui.ctx(), format!("{key}:icon"), img);
                                egui::Image::new(egui::load::SizedTexture::new(id, ir.size())).paint_at(ui, ir);
                            }
                            None => icons::draw(ui.painter(), if p.tab == 2 { Icon::Video } else { Icon::Screen }, ir, pal().header_secondary),
                        }
                        super::text::paint(ui.painter(), pos2(ir.right() + 8.0, y), Align2::LEFT_CENTER, &item.title, semi(14.0), pal().text_normal, tile.width() - 26.0);
                        rects.push((format!("share:item:{n}"), tile));
                        if resp.clicked() {
                            pick = Some(item.kind);
                        }
                    }
                }
            });
            ui.add_space(16.0);
            let (foot, _) = ui.allocate_exact_size(vec2(910.0, 40.0), Sense::hover());
            ui.painter().text(foot.left_top(), Align2::LEFT_TOP, "Custom", semi(16.0), pal().header_primary);
            let res = if height == 0 { "Source".to_string() } else { format!("{height}p") };
            ui.painter().text(foot.left_bottom(), Align2::LEFT_BOTTOM, format!("{res} · {fps}fps"), reg(12.0), pal().text_muted);
            let gear = Rect::from_min_size(pos2(foot.right() - 40.0, foot.top()), vec2(40.0, 40.0));
            let resp = ui.interact(gear, ui.id().with("share_gear"), Sense::click());
            ui.painter().rect_filled(gear, 8, crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            icons::draw(ui.painter(), Icon::Gear, Rect::from_center_size(gear.center(), vec2(20.0, 20.0)), pal().header_primary);
            rects.push(("share:gear".into(), gear));
            egui::Popup::menu(&resp.on_hover_text("Stream Quality")).show(|ui| {
                super::menu_body(ui, |ui| {
                    use super::{Trail, menu_row, menu_sep};
                    for (label, h) in [("720p", 720), ("1080p", 1080), ("Source", 0)] {
                        if menu_row(ui, None, label, None, false, Trail::Check(height == h)).clicked() {
                            quality = Some((h, fps));
                        }
                    }
                    menu_sep(ui);
                    for f in [15, 30, 60] {
                        if menu_row(ui, None, &format!("{f} FPS"), None, false, Trail::Check(fps == f)).clicked() {
                            quality = Some((height, f));
                        }
                    }
                });
            });
        });
    drop(items);
    for (k, r) in rects {
        app.rects.insert(k, r);
    }
    if let Some((h, f)) = quality {
        app.settings.stream_height = h;
        app.settings.stream_fps = f;
        app.save_settings(ctx);
    }
    if let Some(kind) = pick {
        let src = match kind {
            Kind::Window(h) => crate::voice::Source::Window(h),
            Kind::Screen(m) => crate::voice::Source::Monitor(m),
            Kind::Device(i) => crate::voice::Source::Device(i),
        };
        let q = crate::voice::Quality { height: app.settings.stream_height, fps: app.settings.stream_fps };
        app.voice.go_live(&app.net, app.store.me, src, q);
        app.share = None;
        return;
    }
    if modal.should_close() {
        app.share = None;
    }
}

