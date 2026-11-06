//! Discord's inline players: video and audio attachments with the controls bar, the
//! full-window video view and the glue to media.rs, which decodes and plays.

use super::icons::{self, Icon};
use super::{App, take_key, take_text};
use crate::media::{self, Phase, Session, View};
use crate::state::Attachment;
use crate::theme::*;
use egui::{Align2, Color32, Context, CornerRadius, Id, Key, Modifiers, Rect, Sense, Ui, pos2, vec2};

pub enum Do {
    Open { key: String, url: String, video: bool },
    Save(String),
    Fullscreen(String),
    Volume,
    Fetch(String),
}

const PROGRESS: Color32 = BLURPLE;

fn clock(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn now(ctx: &Context) -> f64 {
    ctx.input(|i| i.time)
}

fn period(v: &View) -> std::time::Duration {
    std::time::Duration::from_secs_f32((1.0 / v.info.fps.max(1.0)).clamp(1.0 / 60.0, 0.1))
}

fn icon_btn(ui: &mut Ui, id: Id, rect: Rect, icon: Icon, size: f32, live: bool) -> egui::Response {
    let r = ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let c = if r.hovered() && live { Color32::WHITE } else { pal().header_secondary };
    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(size, size)), c);
    if r.hovered() && live {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

struct Bar<'a> {
    key: &'a str,
    s: &'a Session,
    v: &'a View,
    video: bool,
    fs: bool,
    live: bool,
}

fn controls(ui: &mut Ui, id: Id, rect: Rect, b: &Bar, out: &mut Vec<Do>) {
    let ctx = ui.ctx().clone();
    let (s, v) = (b.s, b.v);
    let p = ui.painter().clone();
    let mut x = rect.left();
    let cell = |x: f32| Rect::from_min_size(pos2(x, rect.top()), vec2(32.0, 32.0));
    let play = icon_btn(ui, id.with("play"), cell(x), if v.playing { Icon::Pause } else { Icon::Play }, 16.0, b.live);
    if play.clicked() {
        s.toggle();
    }
    x += 44.0;
    let text = format!("{} / {}", clock(v.pos), clock(v.info.dur));
    let t = p.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, mono(12.0), Color32::WHITE);
    let mut right = rect.right();
    if b.video {
        right -= 8.0;
        let r = icon_btn(ui, id.with("fs"), Rect::from_min_size(pos2(right - 32.0, rect.top()), vec2(32.0, 32.0)), if b.fs { Icon::ExpandExit } else { Icon::Expand }, 18.0, b.live);
        if r.clicked() {
            out.push(Do::Fullscreen(b.key.to_string()));
        }
        right -= 32.0;
    }
    let vol = Rect::from_min_size(pos2(right - 32.0, rect.top()), vec2(32.0, 32.0));
    let (level, muted) = media::volume();
    let vr = icon_btn(ui, id.with("vol"), vol, if muted || level == 0.0 { Icon::VolumeOff } else { Icon::Volume }, 24.0, b.live);
    if vr.clicked() {
        media::set_volume(if muted && level == 0.0 { 1.0 } else { level }, !muted);
        out.push(Do::Volume);
    }
    volume_popup(ui, id, vol, b.live, out);

    let bar = Rect::from_min_max(pos2(t.right() + 11.0, rect.center().y - 3.0), pos2(vol.left() - 7.0, rect.center().y + 3.0));
    if bar.width() < 20.0 {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::new(("player_bar", b.fs)), (bar, vol)));
    let hit = ui.interact(bar.expand2(vec2(0.0, 9.0)), id.with("seek"), if b.live { Sense::click_and_drag() } else { Sense::hover() });
    let frac = |hit: &egui::Response| hit.interact_pointer_pos().or(hit.hover_pos()).map_or(0.0, |p| ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0) as f64);
    let rest_id = id.with("seek_resume");
    if hit.drag_started() {
        ctx.data_mut(|d| d.insert_temp(rest_id, v.playing));
        s.pause();
    }
    if (hit.dragged() || hit.clicked() || hit.drag_started()) && v.info.dur > 0.0 {
        s.seek(frac(&hit) * v.info.dur);
    }
    if hit.drag_stopped() && ctx.data(|d| d.get_temp::<bool>(rest_id)).unwrap_or(false) {
        s.play();
    }
    let shown = if hit.dragged() { frac(&hit) } else if v.info.dur > 0.0 { v.pos / v.info.dur } else { 0.0 };
    p.rect_filled(bar, CornerRadius::same(3), Color32::from_rgba_unmultiplied(201, 202, 206, 77));
    p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * shown as f32, bar.height())), CornerRadius::same(3), PROGRESS);
    if hit.hovered() || hit.dragged() {
        p.circle_filled(pos2(bar.left() + bar.width() * shown as f32, bar.center().y), 6.0, Color32::WHITE);
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if (hit.hovered() || hit.dragged()) && v.info.dur > 0.0 {
        let at = frac(&hit) * v.info.dur;
        let px = bar.left() + bar.width() * frac(&hit) as f32;
        let label = p.layout_no_wrap(clock(at), medium(12.0), Color32::WHITE);
        let box_ = Rect::from_center_size(pos2(px, bar.top() - 18.0), label.size() + vec2(12.0, 8.0));
        let tip = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id.with("tip")));
        tip.rect_filled(box_, CornerRadius::same(4), Color32::from_black_alpha(220));
        tip.galley(box_.center() - label.size() / 2.0, label, Color32::WHITE);
    }
}

fn volume_popup(ui: &mut Ui, id: Id, btn: Rect, live: bool, out: &mut Vec<Do>) {
    let ctx = ui.ctx().clone();
    let pid = id.with("volpop");
    let track = Rect::from_min_size(pos2(btn.center().x - 3.0, btn.top() - 92.0), vec2(6.0, 76.0));
    let zone = btn.union(track.expand2(vec2(14.0, 12.0)));
    let open = live && ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| zone.contains(p));
    let seen = ctx.data(|d| d.get_temp::<bool>(pid)).unwrap_or(false);
    if open != seen {
        ctx.data_mut(|d| d.insert_temp(pid, open));
    }
    if !open && !seen {
        return;
    }
    let area = egui::Area::new(pid).order(egui::Order::Tooltip).fixed_pos(zone.min).show(&ctx, |ui| {
        let (_, resp) = ui.allocate_exact_size(zone.size(), Sense::click_and_drag());
        let panel = Rect::from_min_max(pos2(btn.center().x - 14.0, track.top() - 10.0), pos2(btn.center().x + 14.0, track.bottom() + 10.0));
        ui.painter().rect_filled(panel, CornerRadius::same(6), Color32::from_black_alpha(190));
        let (level, muted) = media::volume();
        let shown = if muted { 0.0 } else { level };
        ui.painter().rect_filled(track, CornerRadius::same(3), Color32::from_rgba_unmultiplied(201, 202, 206, 77));
        let fill = Rect::from_min_max(pos2(track.left(), track.bottom() - track.height() * shown), track.right_bottom());
        ui.painter().rect_filled(fill, CornerRadius::same(3), PROGRESS);
        ui.painter().circle_filled(pos2(track.center().x, fill.top()), 6.0, Color32::WHITE);
        resp
    });
    let resp = area.inner;
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let v = ((track.bottom() - p.y) / track.height()).clamp(0.0, 1.0);
        media::set_volume(v, false);
        if resp.drag_stopped() || resp.clicked() {
            out.push(Do::Volume);
        }
    }
    if resp.drag_stopped() {
        out.push(Do::Volume);
    }
}

fn hit(i: &mut egui::InputState, k: Key, text: &str) -> bool {
    take_key(i, Modifiers::NONE, k) | take_text(i, text)
}

fn key_actions(ui: &mut Ui, s: &Session, v: &View, key: &str, out: &mut Vec<Do>) {
    let step = |t: f64| s.seek(v.pos + t);
    ui.input_mut(|i| {
        if hit(i, Key::K, "k") {
            s.toggle();
        }
        if hit(i, Key::J, "j") {
            step(-10.0);
        }
        if hit(i, Key::L, "l") {
            step(10.0);
        }
        if take_key(i, Modifiers::NONE, Key::ArrowLeft) {
            step(-5.0);
        }
        if take_key(i, Modifiers::NONE, Key::ArrowRight) {
            step(5.0);
        }
        if hit(i, Key::M, "m") {
            let (l, m) = media::volume();
            media::set_volume(l, !m);
            out.push(Do::Volume);
        }
        if hit(i, Key::F, "f") {
            out.push(Do::Fullscreen(key.to_string()));
        }
        for n in 0..10u8 {
            let k = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9][n as usize];
            if hit(i, k, &n.to_string()) && v.info.dur > 0.0 {
                s.seek(v.info.dur * n as f64 / 10.0);
                s.play();
            }
        }
    });
}

fn focused(ctx: &Context, id: Id) -> Option<Id> {
    let f = ctx.memory(|m| m.focused())?;
    [id, id.with("play"), id.with("seek"), id.with("vol"), id.with("fs")].contains(&f).then_some(f)
}

fn take_focus(ui: &Ui, id: Id, hovered: bool) {
    if hovered && ui.input(|i| i.pointer.any_pressed() || i.pointer.any_click()) {
        ui.memory_mut(|m| m.request_focus(id));
    }
}

fn pop(ctx: &Context, id: Id, rect: Rect, p: &egui::Painter) {
    let Some((t0, to_play)) = ctx.data(|d| d.get_temp::<(f64, bool)>(id)) else { return };
    let k = ((now(ctx) - t0) / 0.45) as f32;
    if k >= 1.0 {
        return;
    }
    let a = ((1.0 - k) * 150.0) as u8;
    p.circle_filled(rect.center(), 24.0 * (1.0 + 0.5 * k), Color32::from_black_alpha(a));
    icons::draw(p, if to_play { Icon::Play } else { Icon::Pause }, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)), Color32::from_white_alpha((255.0 * (1.0 - k)) as u8));
    ctx.request_repaint();
}

fn center_button(ui: &mut Ui, rect: Rect, hot: bool, spinner: bool) {
    let c = rect.center();
    ui.painter().circle_filled(c, 24.0, Color32::from_black_alpha(if hot { 204 } else { 153 }));
    if spinner {
        egui::Spinner::new().size(24.0).color(Color32::WHITE).paint_at(ui, Rect::from_center_size(c, vec2(24.0, 24.0)));
    } else {
        icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(c + vec2(1.0, 0.0), vec2(24.0, 24.0)), Color32::WHITE);
    }
}

/// How a video fills its box: which corners are round and the part of the picture that shows.
#[derive(Clone, Copy)]
pub struct Look {
    pub radius: CornerRadius,
    pub uv: Rect,
}

impl Look {
    pub const FULL: Look = Look { radius: CornerRadius::same(8), uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) };
}

pub fn video(ui: &mut Ui, a: &Attachment, size: egui::Vec2, poster: &str, out: &mut Vec<Do>) -> Rect {
    video_in(ui, a, size, poster, out, Look::FULL)
}

pub fn video_in(ui: &mut Ui, a: &Attachment, size: egui::Vec2, poster: &str, out: &mut Vec<Do>, look: Look) -> Rect {
    let ctx = ui.ctx().clone();
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let id = resp.id;
    let radius = look.radius;
    let session = media::current(&a.url);
    let view = session.as_ref().map(|s| s.view());
    ui.painter().rect_filled(rect, radius, Color32::BLACK);
    let tex = session.as_ref().and_then(|s| s.frame(&ctx));
    match tex {
        Some(t) => egui::Image::from_texture(egui::load::SizedTexture::new(t, size)).uv(look.uv).corner_radius(radius).paint_at(ui, rect),
        None => egui::Image::from_uri(poster.to_string()).uv(look.uv).corner_radius(radius).show_loading_spinner(false).paint_at(ui, rect),
    }
    resp.context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if super::menu_item(ui, None, "Save Video", false) {
            out.push(Do::Save(a.url.clone()));
        }
        super::menu_sep(ui);
        if super::menu_item(ui, None, "Copy Link", false) {
            ui.ctx().copy_text(a.url.clone());
        }
        if super::menu_item(ui, None, "Open Link", false) {
            let _ = open::that_detached(&a.url);
        }
    });
    let hovered = ui.rect_contains_pointer(rect);
    let clip = ui.clip_rect();
    if let Some(s) = &session
        && clip.intersects(rect)
    {
        s.seen(ctx.cumulative_frame_nr());
    }
    match (&session, &view) {
        (Some(s), Some(v)) => match &v.phase {
            Phase::Loading(f) => {
                center_button(ui, rect, false, true);
                ui.painter().text(rect.center() + vec2(0.0, 40.0), Align2::CENTER_CENTER, format!("{}%", (f * 100.0) as u32), medium(12.0), Color32::WHITE);
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
            Phase::Failed(why) => {
                ui.painter().rect_filled(rect, radius, Color32::from_black_alpha(160));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "This video can't be played", medium(14.0), Color32::WHITE);
                ui.painter().text(rect.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, why.chars().take(60).collect::<String>(), reg(12.0), pal().text_muted);
            }
            Phase::Ready => {
                if resp.clicked() {
                    s.toggle();
                    let t = now(&ctx);
                    ctx.data_mut(|d| d.insert_temp(id.with("pop"), (t, !v.playing)));
                }
                if hovered && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                    let t = now(&ctx);
                    ctx.data_mut(|d| d.insert_temp(id.with("move"), t));
                }
                let moved = ctx.data(|d| d.get_temp::<f64>(id.with("move"))).unwrap_or(0.0);
                let show = !v.playing || (hovered && now(&ctx) - moved < 3.0);
                let vis = ctx.animate_bool_with_time(id.with("vis"), show, 0.2);
                if vis > 0.0 {
                    let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 32.0), rect.right_bottom());
                    let b = Bar { key: &a.url, s, v, video: true, fs: false, live: vis > 0.5 };
                    ui.painter().rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: radius.sw, se: radius.se }, Color32::from_black_alpha((153.0 * vis) as u8));
                    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bar));
                    inner.set_opacity(vis);
                    controls(&mut inner, id, bar, &b, out);
                }
                pop(&ctx, id.with("pop"), rect, ui.painter());
                if let Some(f) = focused(&ctx, id) {
                    ctx.memory_mut(|m| m.set_focus_lock_filter(f, egui::EventFilter { horizontal_arrows: true, ..Default::default() }));
                    key_actions(ui, s, v, &a.url, out);
                }
                if v.playing {
                    ctx.request_repaint_after(period(v));
                }
            }
        },
        _ => {
            center_button(ui, rect, hovered, false);
            if resp.clicked() {
                out.push(Do::Open { key: a.url.clone(), url: a.url.clone(), video: true });
            }
        }
    }
    if hovered && !a.url.starts_with("file:") {
        let btn = Rect::from_min_size(pos2(rect.right() - 36.0, rect.top() + 4.0), vec2(32.0, 32.0));
        let r = ui.interact(btn, id.with("dl"), Sense::CLICK);
        if r.hovered() {
            ui.painter().rect_filled(btn, CornerRadius::same(4), Color32::from_rgba_unmultiplied(151, 151, 159, 31));
        }
        icons::draw(ui.painter(), Icon::Download, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), if r.hovered() { pal().interactive_hover } else { pal().header_secondary });
        if r.on_hover_text("Download").clicked() {
            out.push(Do::Save(a.url.clone()));
        }
    }
    if hovered && session.is_none() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    take_focus(ui, id, hovered);
    rect
}

pub fn audio(ui: &mut Ui, a: &Attachment, out: &mut Vec<Do>) -> Rect {
    let ctx = ui.ctx().clone();
    let w = ui.available_width().min(432.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 110.0), Sense::click());
    let id = resp.id;
    let p = ui.painter().clone();
    p.rect(rect, CornerRadius::same(8), pal().bg_primary, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let session = media::current(&a.url);
    let view = session.as_ref().map(|s| s.view());
    icons::draw(&p, Icon::File, Rect::from_min_size(rect.min + vec2(17.0, 25.0), vec2(24.0, 24.0)), BLURPLE);
    let name = super::text::paint(&p, pos2(rect.left() + 49.0, rect.top() + 28.0), Align2::LEFT_CENTER, &a.filename, reg(16.0), pal().link, rect.width() - 49.0 - 52.0);
    let hovered = ui.rect_contains_pointer(rect);
    let mut sub = (super::chat::human_size(a.size), pal().text_muted.gamma_multiply(0.7));
    if let Some(Phase::Failed(_)) = view.as_ref().map(|v| &v.phase) {
        sub = ("This audio can't be played".into(), RED);
    }
    p.text(pos2(rect.left() + 49.0, rect.top() + 47.0), Align2::LEFT_CENTER, sub.0, medium(12.0), sub.1);
    let link = ui.interact(name, id.with("name"), Sense::CLICK);
    if link.hovered() {
        p.hline(name.x_range(), name.bottom() - 1.0, egui::Stroke::new(1.0, pal().link));
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if link.clicked() {
        out.push(Do::Save(a.url.clone()));
    }
    if hovered {
        let btn = Rect::from_min_size(pos2(rect.right() - 40.0, rect.top() + 8.0), vec2(32.0, 32.0));
        let r = ui.interact(btn, id.with("dl"), Sense::CLICK);
        icons::draw(&p, Icon::Download, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), if r.hovered() { pal().interactive_hover } else { pal().header_secondary });
        if r.on_hover_text("Download").clicked() {
            out.push(Do::Save(a.url.clone()));
        }
    }
    let bar = Rect::from_min_max(pos2(rect.left() + 17.0, rect.bottom() - 49.0), pos2(rect.right() - 17.0, rect.bottom() - 17.0));
    p.rect_filled(bar, CornerRadius::same(8), Color32::from_black_alpha(102));
    let placeholder;
    let (s, v) = match (&session, &view) {
        (Some(s), Some(v)) => (s, v),
        _ => {
            placeholder = icon_btn(ui, id.with("play"), Rect::from_min_size(bar.min, vec2(32.0, 32.0)), Icon::Play, 16.0, true);
            p.text(pos2(bar.left() + 44.0, bar.center().y), Align2::LEFT_CENTER, "-:-- / -:--", mono(12.0), Color32::WHITE);
            let track = Rect::from_min_max(pos2(bar.left() + 147.0, bar.center().y - 3.0), pos2(bar.right() - 39.0, bar.center().y + 3.0));
            p.rect_filled(track, CornerRadius::same(3), Color32::from_rgba_unmultiplied(201, 202, 206, 77));
            icons::draw(&p, Icon::Volume, Rect::from_center_size(pos2(bar.right() - 16.0, bar.center().y), vec2(24.0, 24.0)), pal().header_secondary);
            if placeholder.clicked() || (focused(&ctx, id).is_some() && ui.input(|i| i.key_pressed(Key::K))) {
                out.push(Do::Open { key: a.url.clone(), url: a.url.clone(), video: false });
            }
            take_focus(ui, id, hovered);
            return rect;
        }
    };
    if let Some(s2) = Some(s)
        && ui.clip_rect().intersects(rect)
    {
        s2.seen(ctx.cumulative_frame_nr());
    }
    match &v.phase {
        Phase::Loading(f) => {
            p.text(pos2(bar.left() + 44.0, bar.center().y), Align2::LEFT_CENTER, format!("Loading {}%", (f * 100.0) as u32), mono(12.0), Color32::WHITE);
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        Phase::Failed(_) => {}
        Phase::Ready => {
            let b = Bar { key: &a.url, s, v, video: false, fs: false, live: true };
            controls(ui, id, bar, &b, out);
            if let Some(f) = focused(&ctx, id) {
                ctx.memory_mut(|m| m.set_focus_lock_filter(f, egui::EventFilter { horizontal_arrows: true, ..Default::default() }));
                key_actions(ui, s, v, &a.url, out);
            }
            if v.playing {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }
    take_focus(ui, id, hovered);
    rect
}

pub fn overlay(app: &mut App, ctx: &Context) {
    let Some(key) = app.media_fs.clone() else { return };
    let Some(s) = media::current(&key) else { return leave(app, ctx) };
    let v = s.view();
    let screen = ctx.content_rect();
    let id = Id::new("media_fs");
    let mut out = Vec::new();
    let mut exit = ctx.input_mut(|i| take_key(i, Modifiers::NONE, Key::Escape));
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (bg, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.memory_mut(|m| m.request_focus(resp.id));
        ui.painter().rect_filled(bg, CornerRadius::ZERO, Color32::BLACK);
        let (iw, ih) = (v.info.w.max(1) as f32, v.info.h.max(1) as f32);
        let k = (bg.width() / iw).min(bg.height() / ih);
        let pic = Rect::from_center_size(bg.center(), vec2(iw * k, ih * k));
        if let Some(t) = s.frame(ctx) {
            egui::Image::from_texture(egui::load::SizedTexture::new(t, pic.size())).paint_at(ui, pic);
        }
        s.seen(ctx.cumulative_frame_nr());
        if resp.clicked() {
            s.toggle();
            let t = now(ctx);
            ctx.data_mut(|d| d.insert_temp(id.with("pop"), (t, !v.playing)));
        }
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            let t = now(ctx);
            ctx.data_mut(|d| d.insert_temp(id.with("move"), t));
        }
        let moved = ctx.data(|d| d.get_temp::<f64>(id.with("move"))).unwrap_or(0.0);
        let vis = ctx.animate_bool_with_time(id.with("vis"), !v.playing || now(ctx) - moved < 1.0, 0.2);
        if vis > 0.0 {
            let bar = Rect::from_min_max(pos2(bg.left(), bg.bottom() - 32.0), bg.right_bottom());
            ui.painter().rect_filled(bar, CornerRadius::ZERO, Color32::from_black_alpha((153.0 * vis) as u8));
            let b = Bar { key: &key, s: &s, v: &v, video: true, fs: true, live: vis > 0.5 };
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bar));
            inner.set_opacity(vis);
            controls(&mut inner, id, bar, &b, &mut out);
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        pop(ctx, id.with("pop"), bg, ui.painter());
        key_actions(ui, &s, &v, &key, &mut out);
        if v.playing {
            ctx.request_repaint_after(period(&v));
        }
    });
    exit |= out.iter().any(|d| matches!(d, Do::Fullscreen(_)));
    out.retain(|d| !matches!(d, Do::Fullscreen(_)));
    for d in out {
        apply(app, ctx, d);
    }
    if exit {
        leave(app, ctx);
    }
}

fn leave(app: &mut App, ctx: &Context) {
    app.media_fs = None;
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
}

pub fn apply(app: &mut App, ctx: &Context, d: Do) {
    match d {
        Do::Open { key, url, video } => media::open(&app.net.rt, &key, &url, video, ctx.cumulative_frame_nr()),
        Do::Fullscreen(key) => {
            app.media_fs = Some(key);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        Do::Volume => {
            let (v, m) = media::volume();
            app.settings.media_volume = v;
            app.settings.media_muted = m;
            crate::cache::set_settings(&app.settings);
        }
        Do::Fetch(url) => super::textfile::fetch(app, ctx, url),
        Do::Save(_) => {}
    }
}
