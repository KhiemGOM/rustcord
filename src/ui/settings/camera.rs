use egui::load::BytesPoll;
use egui::{Align2, Color32, Context, CornerRadius, Frame, Id, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use super::privacy::write;
use super::wire::Patch;
use super::{dropdown, link, mark, primary, toggle};
use crate::cache::VideoBg;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::voice::effects::{self, Bg};
use crate::voice::preview::{self, Preview};

const PRESETS: [(&str, &str); 4] = [
    ("Cybercity", "e321ddf99270fad41d4c5b21de89a82984d8469c892a3f9a924f5996764d33b4"),
    ("Discord the Movie", "435961b527cedf836cfdfb34cf7c4c7353b248f93be15c30a9a3b778b7d20c76"),
    ("Wumpus Vacation", "cc4247f477caba753bfda3b66dd480a417c59ccd741b3809fedbfc3c8bdd49fd"),
    ("Vaporwave", "857242c637737acc689e8e06fdde1ce120fc595fd65833f4f86944eaea46527e"),
];
const BLUR_TILE: &str = "9b1bb92a5685e3f1d67378826197687bb24f8cf1319a4ab2375fbc8a3fb7d0fe";

static TICKET: AtomicU64 = AtomicU64::new(0);
static UPLOADS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static APPLIED: RefCell<Option<(Option<String>, VideoBg, Option<String>, bool)>> = const { RefCell::new(None) };
}

fn asset(hash: &str) -> String {
    format!("{}/assets/content/{hash}.png", crate::api::cdn())
}

pub fn cameras(app: &App, ctx: &Context) -> Vec<String> {
    if std::env::var_os("RUSTCORD_FAKE_VIDEO").is_some() || std::env::var_os("RUSTCORD_FAKE_CAM").is_some() {
        return effects::FAKE_CAMERAS.iter().map(|n| n.to_string()).collect();
    }
    let id = Id::new(("camera_list", app.sform.session));
    let list: Arc<std::sync::Mutex<Option<Vec<String>>>> = match ctx.data(|d| d.get_temp(id)) {
        Some(l) => l,
        None => {
            let l: Arc<std::sync::Mutex<Option<Vec<String>>>> = Arc::default();
            let (out, ctx2) = (l.clone(), ctx.clone());
            std::thread::spawn(move || {
                let names = crate::voice::cameras();
                if let Ok(mut o) = out.lock() {
                    *o = Some(names);
                }
                ctx2.request_repaint();
            });
            ctx.data_mut(|d| d.insert_temp(id, l.clone()));
            l
        }
    };
    list.lock().ok().and_then(|l| l.clone()).unwrap_or_default()
}

/// Keeps the capture's camera and background in step with the settings, and draws the
/// preview modal that comes before the camera goes on.
pub fn frame(app: &mut App, ctx: &Context) {
    sync(app, ctx);
    modal(app, ctx);
}

/// The call's camera button: asks first when "Always preview video" is on.
pub fn toggle_camera(app: &mut App, ctx: &Context, on: bool) {
    if on && app.settings.preview_video {
        ctx.data_mut(|d| d.insert_temp(Id::new("camera_modal"), true));
    } else {
        let me = app.store.me;
        app.voice.set_camera(&app.net, me, on);
    }
}

/// A blur or no background and the device cost nothing to set. A picture is fetched and
/// decoded only once a camera, a preview or the Voice & Video page could show it, and the
/// frames in between get the blur.
fn sync(app: &App, ctx: &Context) {
    let s = &app.settings;
    let wanted = app.voice.camera || preview::RUNNING.load(Relaxed) > 0 || app.settings_page == Some(super::Page::Voice) || ctx.data(|d| d.get_temp::<bool>(Id::new("camera_modal")).unwrap_or(false));
    let picture = matches!(s.video_bg, VideoBg::Preset(_) | VideoBg::Custom);
    if APPLIED.with_borrow(|a| a.as_ref().is_some_and(|a| a.0 == s.camera && a.1 == s.video_bg && a.2 == s.custom_bg && (a.3 || !picture || !wanted))) {
        return;
    }
    effects::set_device(s.camera.clone());
    let mut loaded = false;
    let bytes = match s.video_bg {
        VideoBg::None => {
            effects::set_background(Bg::None);
            None
        }
        VideoBg::Blur => {
            effects::set_background(Bg::Blur);
            None
        }
        _ if !wanted => {
            effects::set_background(Bg::Blur);
            None
        }
        VideoBg::Preset(n) => match ctx.try_load_bytes(&asset(PRESETS[n as usize % PRESETS.len()].1)) {
            Ok(BytesPoll::Ready { bytes, .. }) => {
                loaded = true;
                Some(AsRef::<[u8]>::as_ref(&bytes).to_vec())
            }
            Ok(BytesPoll::Pending { .. }) => {
                effects::set_background(Bg::Blur);
                return;
            }
            Err(_) => {
                loaded = true;
                effects::set_background(Bg::Blur);
                None
            }
        },
        VideoBg::Custom => match s.custom_bg.as_ref().and_then(|p| std::fs::read(p).ok()) {
            Some(b) => {
                loaded = true;
                Some(b)
            }
            None => {
                loaded = true;
                effects::set_background(Bg::None);
                None
            }
        },
    };
    if let Some(bytes) = bytes {
        let ticket = TICKET.fetch_add(1, Relaxed) + 1;
        std::thread::spawn(move || {
            if let Some(p) = effects::picture(&bytes)
                && TICKET.load(Relaxed) == ticket
            {
                effects::set_background(Bg::Picture(Arc::new(p)));
            }
        });
    } else {
        TICKET.fetch_add(1, Relaxed);
    }
    APPLIED.with_borrow_mut(|a| *a = Some((s.camera.clone(), s.video_bg, s.custom_bg.clone(), loaded)));
}

pub fn shown(app: &App, cams: &[String]) -> String {
    app.settings.camera.clone().or_else(|| cams.first().cloned()).unwrap_or_else(|| "No Video Devices".into())
}

fn picker(app: &mut App, ui: &mut Ui, cams: &[String]) -> bool {
    let ctx = ui.ctx().clone();
    let mut changed = false;
    let w = ui.available_width();
    let cur = shown(app, cams);
    if cams.is_empty() {
        let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        let faint = Color32::from_rgba_unmultiplied(151, 151, 159, 61);
        ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, faint), egui::StrokeKind::Inside);
        icons::draw(ui.painter(), Icon::Camera, Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
        ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &cur, medium(16.0), pal().text_muted);
        mark(ui, "camera:device", r);
        return false;
    }
    let resp = dropdown(ui, w, Some(Icon::Camera), &cur, |ui| {
        for name in cams {
            let item = ui.selectable_label(*name == cur, name);
            mark(ui, &format!("camera:dev:{name}"), item.rect);
            if item.clicked() {
                choose(app, &ctx, name);
                changed = true;
            }
        }
    });
    mark(ui, "camera:device", resp.rect);
    changed
}

/// Picks the camera: previews restart on it, and a camera already on in a call is reopened.
pub fn choose(app: &mut App, ctx: &Context, name: &str) {
    app.settings.camera = Some(name.to_string());
    effects::set_device(Some(name.to_string()));
    stop(ctx, "settings");
    stop(ctx, "modal");
    if app.voice.camera {
        let me = app.store.me;
        app.voice.set_camera(&app.net, me, true);
    }
}

fn picture_tile(ui: &mut Ui, r: Rect, url: &str, selected: bool, id: Id) -> bool {
    let resp = ui.interact(r, id, Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_tertiary);
    egui::Image::new(url).fit_to_exact_size(r.size()).corner_radius(4).paint_at(ui, r);
    ring(ui, r, selected, &resp);
    resp.clicked()
}

fn ring(ui: &Ui, r: Rect, selected: bool, resp: &egui::Response) {
    if selected {
        ui.painter().rect_stroke(r.expand(4.0), CornerRadius::same(4), Stroke::new(2.0, pal().blurple), egui::StrokeKind::Inside);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn text_tile(ui: &mut Ui, r: Rect, icon: Icon, text: &str, selected: bool, id: Id, back: Option<&str>) -> bool {
    let resp = ui.interact(r, id, Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_tertiary);
    if let Some(url) = back {
        egui::Image::new(url).fit_to_exact_size(r.size()).corner_radius(4).paint_at(ui, r);
    }
    let c = r.center();
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(c.x, c.y - 10.0), vec2(18.0, 18.0)), Color32::WHITE);
    ui.painter().text(pos2(c.x, c.y + 8.0), Align2::CENTER_CENTER, text, reg(14.0), Color32::WHITE);
    ring(ui, r, selected, &resp);
    resp.clicked()
}

fn push_bg(app: &mut App, bg: VideoBg) {
    let patch = match bg {
        VideoBg::None => Patch::new().sub(5, 1, &[(1, 0)]),
        VideoBg::Blur => Patch::new().sub(5, 1, &[(1, 1)]),
        VideoBg::Preset(n) => Patch::new().num(5, 2, n as u64),
        VideoBg::Custom => return,
    };
    write(app, patch);
}

fn pick_custom(app: &mut App) -> bool {
    let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() else { return false };
    use_custom(app, &path)
}

pub fn use_custom(app: &mut App, path: &std::path::Path) -> bool {
    let dir = crate::cache::dir().join("backgrounds");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let dest = dir.join(format!("custom.{ext}"));
    if std::fs::create_dir_all(&dir).is_err() || std::fs::copy(path, &dest).is_err() {
        return false;
    }
    app.settings.custom_bg = Some(dest.to_string_lossy().into_owned());
    app.settings.video_bg = VideoBg::Custom;
    UPLOADS.fetch_add(1, Relaxed);
    true
}

fn custom_uri(ctx: &Context, path: &str) -> Option<String> {
    let uri = format!("bytes://custom_bg/{}", UPLOADS.load(Relaxed));
    let id = Id::new("custom_bg_uri");
    if ctx.data(|d| d.get_temp::<String>(id)).as_deref() != Some(&uri) {
        ctx.include_bytes(uri.clone(), std::fs::read(path).ok()?);
        ctx.data_mut(|d| d.insert_temp(id, uri.clone()));
    }
    Some(uri)
}

fn backgrounds(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    let head = ui.add(egui::Label::new(RichText::new("Video Background").font(medium(16.0)).color(pal().header_primary)).selectable(false));
    if ui.data_mut(|d| d.get_temp::<String>(Id::new("settings_goto"))).as_deref() == Some("Video Background") {
        ui.data_mut(|d| d.remove_temp::<String>(Id::new("settings_goto")));
        ui.scroll_to_rect(head.rect.translate(vec2(0.0, -64.0)), Some(egui::Align::TOP));
    }
    ui.add_space(16.0);
    let w = ui.available_width();
    let cols = (((w + 16.0) / 148.0) as usize).max(1);
    let tw = (w - 16.0 * (cols - 1) as f32) / cols as f32;
    let tile = vec2(tw, (tw * 9.0 / 16.0).round());
    let custom = app.settings.custom_bg.clone();
    let rows = (3 + usize::from(custom.is_some()) + PRESETS.len()).div_ceil(cols);
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    ui.allocate_rect(Rect::from_min_size(pos2(left, top), vec2(w, rows as f32 * tile.y + (rows - 1) as f32 * 16.0)), Sense::hover());
    let cur = app.settings.video_bg;
    let mut pick = None;
    let mut k = 0;
    let mut at = || {
        k += 1;
        Rect::from_min_size(pos2(left + ((k - 1) % cols) as f32 * (tile.x + 16.0), top + ((k - 1) / cols) as f32 * (tile.y + 16.0)), tile)
    };
    let r = at();
    if text_tile(ui, r, Icon::BackgroundNone, "None", cur == VideoBg::None, Id::new("bg_none"), None) {
        pick = Some(VideoBg::None);
    }
    mark(ui, "camera:bg:none", r);
    let r = at();
    if text_tile(ui, r, Icon::BackgroundBlur, "Blur", cur == VideoBg::Blur, Id::new("bg_blur"), Some(&asset(BLUR_TILE))) {
        pick = Some(VideoBg::Blur);
    }
    mark(ui, "camera:bg:blur", r);
    let r = at();
    if text_tile(ui, r, Icon::BackgroundCustom, "Custom", false, Id::new("bg_custom"), None) && pick_custom(app) {
        changed = true;
    }
    mark(ui, "camera:bg:custom", r);
    if let Some(p) = &custom {
        let r = at();
        if let Some(uri) = custom_uri(ui.ctx(), p)
            && picture_tile(ui, r, &uri, cur == VideoBg::Custom, Id::new("bg_custom_pic"))
        {
            pick = Some(VideoBg::Custom);
        }
    }
    for (i, (name, hash)) in PRESETS.iter().enumerate() {
        let r = at();
        if picture_tile(ui, r, &asset(hash), cur == VideoBg::Preset(i as u8), Id::new(("bg", i))) {
            pick = Some(VideoBg::Preset(i as u8));
        }
        mark(ui, &format!("camera:bg:{name}"), r);
    }
    if let Some(p) = pick {
        app.settings.video_bg = p;
        push_bg(app, p);
        changed = true;
    }
    if effects::BROKEN.load(Relaxed) {
        ui.add_space(12.0);
        ui.add(egui::Label::new(RichText::new("Uh oh! Something went wrong when setting your background. Please try again or use a different background.").font(reg(14.0)).color(pal().red)));
    }
    changed
}

fn paint_frame(app: &mut App, ui: &Ui, key: &str, img: &Arc<egui::ColorImage>, r: Rect) {
    let ptr = Arc::as_ptr(img) as usize;
    let id = match app.voice.tex.get_mut(key) {
        Some((h, p)) => {
            if *p != ptr {
                h.set(img.clone(), egui::TextureOptions::LINEAR);
                *p = ptr;
            }
            h.id()
        }
        None => {
            let h = ui.ctx().load_texture(key, img.clone(), egui::TextureOptions::LINEAR);
            let id = h.id();
            app.voice.tex.insert(key.into(), (h, ptr));
            id
        }
    };
    let k = (r.width() / img.size[0] as f32).min(r.height() / img.size[1] as f32);
    let fr = Rect::from_center_size(r.center(), vec2(img.size[0] as f32 * k, img.size[1] as f32 * k));
    egui::Image::new(egui::load::SizedTexture::new(id, fr.size())).paint_at(ui, fr);
}

/// The black box: the live picture, or a Test Video button while nothing is running. The
/// camera in a call is its own picture; otherwise a preview opens the camera and lives
/// until the box stops being drawn.
fn viewport(app: &mut App, ui: &mut Ui, r: Rect, key: &'static str, start_on_open: bool) {
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::BLACK);
    let ctx = ui.ctx().clone();
    let me = app.store.me;
    if app.voice.camera {
        if let Some(img) = app.voice.video.get(&me).cloned() {
            paint_frame(app, ui, key, &img, r);
        }
        return;
    }
    let id = Id::new(("camera_preview", key));
    let visit = (app.sform.session, app.sform.visit);
    let mut prev: Option<(Arc<Preview>, (u32, u32))> = ctx.data(|d| d.get_temp(id));
    if prev.as_ref().is_some_and(|(p, v)| *v != visit || (p.ended() && p.error().is_none())) {
        stop(&ctx, key);
        prev = None;
    }
    if prev.is_none() && start_on_open {
        prev = Some(begin(&ctx, key, visit));
    }
    let Some((p, _)) = prev else {
        let mut c = ui.new_child(UiBuilder::new().max_rect(Rect::from_center_size(r.center(), vec2(104.0, 40.0))));
        let resp = primary(&mut c, "Test Video");
        mark(ui, "camera:test", resp.rect);
        if resp.clicked() {
            begin(&ctx, key, visit);
        }
        return;
    };
    match (p.frame(), p.error()) {
        (Some(img), _) => paint_frame(app, ui, key, &img, r),
        (None, Some(e)) => {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, e, reg(16.0), pal().header_secondary);
        }
        _ => {}
    }
}

fn begin(ctx: &Context, key: &'static str, visit: (u32, u32)) -> (Arc<Preview>, (u32, u32)) {
    let p = (Arc::new(Preview::start(ctx)), visit);
    ctx.data_mut(|d| d.insert_temp(Id::new(("camera_preview", key)), p.clone()));
    p
}

fn stop(ctx: &Context, key: &str) {
    let id = Id::new(("camera_preview", key));
    let old = ctx.data_mut(|d| {
        let held = d.get_temp::<(Arc<Preview>, (u32, u32))>(id);
        d.remove::<(Arc<Preview>, (u32, u32))>(id);
        held
    });
    drop(old);
}

pub fn section(app: &mut App, ui: &mut Ui) -> bool {
    super::section(ui, "Camera");
    let ctx = ui.ctx().clone();
    let cams = cameras(app, &ctx);
    let mut changed = false;
    super::lead(ui);
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 220.0), Sense::hover());
    viewport(app, ui, r, "settings", false);
    super::end(ui, 24.0);
    if toggle(ui, &mut app.settings.preview_video, "Always preview video", "Pops up preview modal every time you turn on video") {
        write(app, Patch::new().flag(5, 5, app.settings.preview_video));
        changed = true;
    }
    super::lead(ui);
    ui.label(RichText::new("Camera").font(medium(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    changed |= picker(app, ui, &cams);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Looking for more camera options? Check out your ").font(reg(14.0)).color(pal().header_secondary));
        if link(ui, "system camera settings").clicked() {
            let _ = open::that("ms-settings:camera");
        }
        ui.label(RichText::new(".").font(reg(14.0)).color(pal().header_secondary));
    });
    ui.add_space(24.0);
    changed |= backgrounds(app, ui);
    changed
}

fn modal(app: &mut App, ctx: &Context) {
    let open_id = Id::new("camera_modal");
    if !ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
        return;
    }
    let cams = cameras(app, ctx);
    let (mut close, mut go, mut changed) = (false, false, false);
    let m = crate::ui::motion::Modal::new("camera_preview_modal".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Ready to video chat?").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= crate::ui::icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(432.0, 243.0), Sense::hover());
            viewport(app, ui, r, "modal", true);
            ui.add_space(20.0);
            changed |= picker(app, ui, &cams);
            ui.add_space(20.0);
            changed |= backgrounds(app, ui);
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(200.0, 38.0), Sense::click());
                let on = app.settings.preview_video;
                let b = Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(24.0, 24.0));
                if on {
                    ui.painter().rect_filled(b, 6, pal().blurple);
                    icons::draw(ui.painter(), Icon::Check, b.shrink(4.0), Color32::WHITE);
                } else {
                    ui.painter().rect_stroke(b, 6, Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                }
                ui.painter().text(pos2(b.right() + 10.0, r.center().y), Align2::LEFT_CENTER, "Always preview video", reg(14.0), pal().text_normal);
                mark(ui, "camera:modal:always", r);
                if resp.clicked() {
                    app.settings.preview_video = !on;
                    write(app, Patch::new().flag(5, 5, !on));
                    changed = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = crate::ui::button(ui, "Turn On Camera", pal().blurple, 140.0);
                    mark(ui, "camera:modal:on", resp.rect);
                    go = resp.clicked();
                });
            });
        });
    if go || close || m.should_close() {
        ctx.data_mut(|d| {
            d.remove::<bool>(open_id);
        });
        stop(ctx, "modal");
    }
    if go {
        let me = app.store.me;
        app.voice.set_camera(&app.net, me, true);
    }
    if changed {
        app.save_settings(ctx);
    }
}
