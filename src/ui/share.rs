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

pub const GAMING: u8 = 1;
pub const SCREENSHARE: u8 = 2;
pub const CUSTOM: u8 = 3;

/// What a picture size and frame rate need, from Discord's table: Nitro (2), Nitro or Nitro
/// Classic (1), or a server boosted to a level; Source at 5 fps is Screenshare's alone.
struct Need {
    height: usize,
    fps: u32,
    nitro: u8,
    tier: u8,
}

const fn need(height: usize, fps: u32, nitro: u8, tier: u8) -> Need {
    Need { height, fps, nitro, tier }
}

const TABLE: [Need; 18] = [
    need(0, 60, 2, 0),
    need(0, 30, 2, 0),
    need(0, 15, 2, 0),
    need(0, 5, 0, 0),
    need(1440, 60, 1, 2),
    need(1440, 30, 1, 2),
    need(1440, 15, 1, 2),
    need(1080, 60, 1, 2),
    need(1080, 30, 1, 2),
    need(1080, 15, 1, 2),
    need(720, 60, 1, 1),
    need(720, 30, 0, 0),
    need(720, 15, 0, 0),
    need(720, 5, 0, 0),
    need(480, 60, 1, 1),
    need(480, 30, 0, 0),
    need(480, 15, 0, 0),
    need(480, 5, 0, 0),
];

/// Whether the account (its premium_type) may stream this in a server boosted to `tier`.
fn allowed(preset: u8, height: usize, fps: u32, premium: u64, tier: u8) -> bool {
    let nitro = match premium {
        2 => 2,
        1 => 1,
        _ => 0,
    };
    TABLE.iter().any(|n| {
        n.height == height
            && n.fps == fps
            && (n.height != 0 || n.fps != 5 || preset == SCREENSHARE)
            && ((n.nitro == 0 && n.tier == 0) || (n.nitro != 0 && nitro >= n.nitro) || (n.tier != 0 && tier >= n.tier))
    })
}

/// The picture size and frame rate a stream mode gives this account: Gaming the best of 1440p60,
/// 1080p60, 720p60 and 720p30 it may use, Screenshare Source at 15 or else 5 fps, Custom what was
/// picked if it may, else 720p30.
pub fn quality(preset: u8, custom: (usize, u32), premium: u64, tier: u8) -> (usize, u32) {
    let first = |list: &[(usize, u32)]| list.iter().copied().find(|(h, f)| allowed(preset, *h, *f, premium, tier));
    match preset {
        SCREENSHARE => first(&[(0, 15), (0, 5)]),
        CUSTOM => first(&[custom]),
        _ => first(&[(1440, 60), (1080, 60), (720, 60), (720, 30)]),
    }
    .unwrap_or((720, 30))
}

/// Discord's "Stream in HD resolution with Nitro" strip under the picker; true when clicked.
fn banner(ui: &egui::Ui, r: Rect, rects: &mut Vec<(String, Rect)>) -> bool {
    let stops = [(0.0, Color32::from_rgb(130, 76, 185)), (0.5, Color32::from_rgb(173, 74, 183)), (1.0, Color32::from_rgb(162, 94, 134))];
    let p = ui.painter();
    p.with_clip_rect(Rect::from_min_max(r.min, pos2(r.left() + 8.0, r.bottom()))).rect_filled(r, 8, stops[0].1);
    p.with_clip_rect(Rect::from_min_max(pos2(r.right() - 8.0, r.top()), r.max)).rect_filled(r, 8, stops[2].1);
    let mut m = egui::Mesh::default();
    let (x0, x1) = (r.left() + 8.0, r.right() - 8.0);
    for (i, (t, c)) in stops.iter().enumerate() {
        let x = x0 + (x1 - x0) * t;
        m.colored_vertex(pos2(x, r.top()), *c);
        m.colored_vertex(pos2(x, r.bottom()), *c);
        if i > 0 {
            let k = i as u32 * 2;
            m.add_triangle(k - 2, k - 1, k);
            m.add_triangle(k - 1, k, k + 1);
        }
    }
    p.add(m);
    icons::draw(p, Icon::Lock, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(12.0, 12.0)), Color32::WHITE);
    let lead = p.layout_no_wrap("Stream in HD resolution with ".into(), medium(12.0), Color32::WHITE);
    let x = r.left() + 32.0;
    let lead_w = lead.size().x;
    p.galley(pos2(x, r.center().y - lead.size().y / 2.0), lead, Color32::WHITE);
    let link = p.layout_no_wrap("Nitro".into(), semi(12.0), Color32::WHITE);
    let lr = Rect::from_min_size(pos2(x + lead_w, r.center().y - link.size().y / 2.0), link.size());
    p.line_segment([pos2(lr.left(), lr.bottom()), pos2(lr.right(), lr.bottom())], Stroke::new(1.0, Color32::WHITE));
    p.galley(lr.min, link, Color32::WHITE);
    let label = p.layout_no_wrap("Get Nitro".into(), semi(12.0), Color32::from_rgb(104, 94, 112));
    let b = Rect::from_min_size(pos2(r.right() - 16.0 - label.size().x - 38.0, r.center().y - 12.0), vec2(label.size().x + 38.0, 24.0));
    p.rect_filled(b, 12, Color32::from_rgb(234, 232, 233));
    icons::draw(p, Icon::Nitro, Rect::from_center_size(pos2(b.left() + 19.0, b.center().y), vec2(14.0, 14.0)), Color32::from_rgb(130, 76, 185));
    p.galley(pos2(b.left() + 30.0, b.center().y - label.size().y / 2.0), label, Color32::from_rgb(104, 94, 112));
    rects.push(("share:nitro".into(), b));
    let link = ui.interact(lr, ui.id().with("share_nitro_link"), Sense::click());
    let button = ui.interact(b, ui.id().with("share_nitro"), Sense::click());
    if link.hovered() || button.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    link.clicked() || button.clicked()
}

fn res(h: usize) -> String {
    if h == 0 { "Source".into() } else { format!("{h}p") }
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
    let premium = app.store.me_raw["premium_type"].as_u64().unwrap_or(0);
    let tier = app.store.guilds.get(&app.voice.guild).map_or(0, |g| g.tier);
    let preset = app.settings.stream_preset;
    let custom = (app.settings.stream_height, app.settings.stream_fps);
    let (height, fps) = quality(preset, custom, premium, tier);
    let mut pick = None;
    let mut chosen: Option<(u8, usize, u32)> = None;
    let mut upsell = false;
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
            let name = match preset {
                SCREENSHARE => "Screenshare",
                CUSTOM => "Custom",
                _ => "Gaming",
            };
            let goal = match preset {
                SCREENSHARE => Some("Clearer text"),
                CUSTOM => None,
                _ => Some("Smoother video"),
            };
            let line: Vec<String> = goal.map(str::to_string).into_iter().chain([res(height), format!("{fps}fps")]).collect();
            ui.painter().text(pos2(foot.left(), foot.top() + 10.0), Align2::LEFT_CENTER, name, semi(16.0), pal().header_primary);
            let mut x = foot.left();
            for (i, seg) in line.iter().enumerate() {
                if i > 0 {
                    ui.painter().circle_filled(pos2(x + 9.0, foot.bottom() - 9.0), 1.5, pal().text_muted.gamma_multiply(0.5));
                    x += 18.0;
                }
                let g = ui.painter().layout_no_wrap(seg.clone(), reg(12.0), pal().text_muted);
                let w = g.size().x;
                ui.painter().galley(pos2(x, foot.bottom() - 9.0 - g.size().y / 2.0), g, pal().text_muted);
                x += w;
            }
            let gear = Rect::from_min_size(pos2(foot.right() - 40.0, foot.top()), vec2(40.0, 40.0));
            if premium != 2 {
                let pill = Rect::from_min_size(pos2(gear.left() - 9.0 - 111.0, foot.top()), vec2(111.0, 40.0));
                ui.painter().rect_filled(pill, 10, Color32::from_black_alpha(45));
                let sd = Rect::from_min_size(pill.min + vec2(4.0, 4.0), vec2(51.0, 32.0));
                ui.painter().rect_filled(sd, 8, pal().bg_secondary);
                ui.painter().text(sd.center(), Align2::CENTER_CENTER, "SD", medium(14.0), pal().header_primary);
                let hd = Rect::from_min_max(pos2(sd.right(), sd.top()), pos2(pill.right() - 4.0, sd.bottom()));
                let hit = ui.interact(hd, ui.id().with("share_hd"), Sense::click());
                ui.painter().text(hd.center(), Align2::CENTER_CENTER, "HD", medium(14.0), Color32::from_rgb(0xbe, 0x78, 0xcd));
                rects.push(("share:hd".into(), hd));
                if hit.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                upsell |= hit.clicked();
                ui.add_space(15.0);
                let (row, _) = ui.allocate_exact_size(vec2(910.0, 40.0), Sense::hover());
                upsell |= banner(ui, row.expand2(vec2(8.0, 0.0)), &mut rects);
            }
            let resp = ui.interact(gear, ui.id().with("share_gear"), Sense::click());
            ui.painter().rect_filled(gear, 8, crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            icons::draw(ui.painter(), Icon::Gear, Rect::from_center_size(gear.center(), vec2(20.0, 20.0)), pal().header_primary);
            rects.push(("share:gear".into(), gear));
            egui::Popup::menu(&resp.on_hover_text("Options")).align(egui::RectAlign::TOP_END).show(|ui| {
                super::menu_body(ui, |ui| {
                    use super::{Trail, menu_row, menu_row_at, menu_sep, submenu};
                    ui.set_width(220.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (gaming, screen) = (quality(GAMING, custom, premium, tier), quality(SCREENSHARE, custom, premium, tier));
                    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                    ui.painter().text(pos2(head.left() + 8.0, head.center().y), Align2::LEFT_CENTER, "Stream Mode", semi(12.0), pal().text_muted);
                    let modes = [
                        (GAMING, "Gaming", Some(format!("Smoother video ({}, {}fps)", res(gaming.0), gaming.1))),
                        (SCREENSHARE, "Screenshare", Some(format!("Clearer text ({}, {}fps)", res(screen.0), screen.1))),
                        (CUSTOM, "Custom", None),
                    ];
                    for (p, label, sub) in modes {
                        if menu_row(ui, None, label, sub.as_deref(), false, Trail::Radio(preset == p)).clicked() {
                            chosen = Some((p, custom.0, custom.1));
                        }
                    }
                    if preset == CUSTOM {
                        menu_sep(ui);
                        let locked = |ui: &mut egui::Ui, r: &egui::Response, label: &str| {
                            let w = ui.painter().layout_no_wrap(label.into(), medium(14.0), Color32::WHITE).size().x;
                            let at = Rect::from_center_size(pos2(r.rect.left() + 8.0 + w + 12.0, r.rect.center().y), vec2(14.0, 14.0));
                            icons::draw(ui.painter(), Icon::Nitro, at, Color32::from_rgb(0xff, 0x73, 0xfa));
                        };
                        let row = menu_row(ui, None, "Screen Resolution", Some(&res(height)), false, Trail::Caret);
                        submenu(ui, &row, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for h in [720, 1080, 1440, 0] {
                                let label = res(h);
                                let (r, _) = menu_row_at(ui, false, &label, None, false, Trail::Radio(height == h));
                                if h != 720 {
                                    locked(ui, &r, &label);
                                }
                                if r.clicked() {
                                    if allowed(CUSTOM, h, fps, premium, tier) { chosen = Some((CUSTOM, h, fps)) } else { upsell = true }
                                }
                            }
                        });
                        let row = menu_row(ui, None, "Frame Rate", Some(&format!("{fps}fps")), false, Trail::Caret);
                        submenu(ui, &row, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for f in [15, 30, 60] {
                                let label = format!("{f}fps");
                                let (r, _) = menu_row_at(ui, false, &label, None, false, Trail::Radio(fps == f));
                                if f == 60 {
                                    locked(ui, &r, &label);
                                }
                                if r.clicked() {
                                    if allowed(CUSTOM, height, f, premium, tier) { chosen = Some((CUSTOM, height, f)) } else { upsell = true }
                                }
                            }
                        });
                    }
                });
            });
        });
    drop(items);
    for (k, r) in rects {
        app.rects.insert(k, r);
    }
    if let Some((p, h, f)) = chosen {
        app.settings.stream_preset = p;
        app.settings.stream_height = h;
        app.settings.stream_fps = f;
        app.save_settings(ctx);
    }
    if upsell {
        app.share = None;
        app.settings_page = Some(super::settings::Page::Nitro);
        return;
    }
    if let Some(kind) = pick {
        let src = match kind {
            Kind::Window(h) => crate::voice::Source::Window(h),
            Kind::Screen(m) => crate::voice::Source::Monitor(m),
            Kind::Device(i) => crate::voice::Source::Device(i),
        };
        let q = crate::voice::Quality { height, fps };
        app.voice.go_live(&app.net, app.store.me, src, q);
        app.share = None;
        return;
    }
    if modal.should_close() {
        app.share = None;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_modes_give_what_discord_allows_each_account() {
        assert_eq!(quality(GAMING, (0, 60), 0, 0), (720, 30));
        assert_eq!(quality(GAMING, (0, 60), 0, 1), (720, 60));
        assert_eq!(quality(GAMING, (0, 60), 0, 2), (1440, 60));
        assert_eq!(quality(GAMING, (0, 60), 2, 0), (1440, 60));
        assert_eq!(quality(GAMING, (0, 60), 3, 0), (720, 30));
        assert_eq!(quality(SCREENSHARE, (0, 60), 0, 0), (0, 5));
        assert_eq!(quality(SCREENSHARE, (0, 60), 2, 0), (0, 15));
        assert_eq!(quality(CUSTOM, (0, 60), 0, 0), (720, 30));
        assert_eq!(quality(CUSTOM, (480, 15), 0, 0), (480, 15));
        assert_eq!(quality(CUSTOM, (0, 60), 2, 0), (0, 60));
        assert!(!allowed(CUSTOM, 0, 5, 2, 3));
    }
}
