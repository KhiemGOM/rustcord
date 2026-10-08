use egui::{Align2, Color32, ColorImage, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};

use super::icons::{self, Icon};
use super::{App, avatar, channel_title, icon_button};
use crate::state::{Channel, Id};
use crate::theme::*;
use crate::voice::{State, audio};

/// "Voice Connected" box that sits above the user panel.
pub fn panel(app: &mut App, ui: &mut Ui) {
    let Some(ch) = app.voice.chan else { return };
    let (text, col) = match app.voice.state {
        State::Connected => ("Voice Connected", GREEN),
        State::Reconnecting => ("Reconnecting", YELLOW),
        _ => ("RTC Connecting", YELLOW),
    };
    let place = match app.store.channels.get(&ch) {
        Some(c) if c.guild != 0 => {
            let g = app.store.guilds.get(&c.guild).map(|g| g.name.as_str()).unwrap_or("");
            format!("{} / {}", c.name, g)
        }
        Some(_) => channel_title(&app.store, ch),
        None => String::new(),
    };
    let r = ui.max_rect();
    let p = ui.painter();
    p.hline(r.x_range().expand(8.0), r.top(), Stroke::new(1.0, pal().bg_mod_hover));
    let y = r.center().y;
    bars(ui, pos2(r.left() + 2.0, y - 7.0), col);
    p.text(pos2(r.left() + 22.0, y - 9.0), Align2::LEFT_CENTER, text, semi(14.0), col);
    let sub = Rect::from_min_max(pos2(r.left(), y + 1.0), pos2(r.right() - 40.0, y + 19.0));
    let sub_r = ui.interact(sub, egui::Id::new("voice_place"), Sense::click());
    ui.painter().with_clip_rect(sub).text(
        pos2(sub.left(), sub.center().y),
        Align2::LEFT_CENTER,
        place,
        reg(12.0),
        if sub_r.hovered() { pal().header_primary } else { pal().header_secondary },
    );
    if sub_r.clicked() {
        app.open(ch);
    }
    let b = Rect::from_center_size(pos2(r.right() - 16.0, y), vec2(32.0, 32.0));
    let leave = ui.interact(b, egui::Id::new("voice_leave"), Sense::click());
    if leave.hovered() {
        ui.painter().rect_filled(b, CornerRadius::same(4), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    hangup(ui, b.center(), if leave.hovered() { pal().header_primary } else { pal().header_secondary });
    app.rects.insert("voice:disconnect".into(), b);
    if leave.on_hover_text("Disconnect").clicked() {
        app.voice.leave(&app.net);
    }
}

fn bars(ui: &Ui, at: egui::Pos2, col: Color32) {
    for (i, h) in [5.0, 9.0, 13.0].into_iter().enumerate() {
        let x = at.x + i as f32 * 5.0;
        ui.painter().rect_filled(
            Rect::from_min_max(pos2(x, at.y + 14.0 - h), pos2(x + 3.0, at.y + 14.0)),
            CornerRadius::same(1),
            col,
        );
    }
}

// an upright handset for "call"
fn phone(ui: &Ui, c: egui::Pos2, col: Color32) {
    let p = ui.painter();
    p.add(egui::Shape::line(
        vec![c + vec2(-6.0, -7.0), c + vec2(-7.0, -2.0), c + vec2(-3.0, 4.0), c + vec2(3.0, 7.5), c + vec2(7.0, 6.0)],
        Stroke::new(3.0, col),
    ));
    p.circle_filled(c + vec2(-5.5, -7.0), 2.5, col);
    p.circle_filled(c + vec2(6.5, 6.0), 2.5, col);
}

// a camcorder: body plus the lens flare on the right
fn camera(ui: &Ui, c: egui::Pos2, k: f32, col: Color32) {
    let p = ui.painter();
    p.rect_filled(Rect::from_center_size(c + vec2(-2.5, 0.0) * k, vec2(13.0, 10.0) * k), CornerRadius::same((2.0 * k) as u8), col);
    p.add(egui::Shape::convex_polygon(
        vec![c + vec2(4.0, -1.5) * k, c + vec2(9.0, -4.5) * k, c + vec2(9.0, 4.5) * k, c + vec2(4.0, 1.5) * k],
        col,
        Stroke::NONE,
    ));
}

// a monitor on its stand
fn screen(ui: &Ui, c: egui::Pos2, k: f32, col: Color32) {
    let p = ui.painter();
    p.rect_stroke(Rect::from_center_size(c + vec2(0.0, -1.5) * k, vec2(16.0, 11.0) * k), CornerRadius::same((2.0 * k) as u8), Stroke::new(2.0 * k, col), egui::StrokeKind::Middle);
    p.line_segment([c + vec2(0.0, 4.0) * k, c + vec2(0.0, 7.0) * k], Stroke::new(2.0 * k, col));
    p.line_segment([c + vec2(-4.0, 7.5) * k, c + vec2(4.0, 7.5) * k], Stroke::new(2.0 * k, col));
}

// a handset tipped on its side, the shape Discord uses for "leave"
fn hangup(ui: &Ui, c: egui::Pos2, col: Color32) {
    let p = ui.painter();
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, -1.0), vec2(16.0, 5.0)), CornerRadius::same(3), col);
    p.rect_filled(Rect::from_center_size(c + vec2(-6.0, 2.5), vec2(5.0, 5.0)), CornerRadius::same(2), col);
    p.rect_filled(Rect::from_center_size(c + vec2(6.0, 2.5), vec2(5.0, 5.0)), CornerRadius::same(2), col);
}

fn toggle(ui: &mut Ui, icon: Icon, on: bool, size: f32, tip: &str) -> egui::Response {
    let r = icon_button(ui, icon, size, tip);
    if on {
        let c = r.rect.center();
        let d = size * 0.26;
        let bg = if r.hovered() { pal().bg_mod_hover } else { pal().bg_secondary_alt };
        ui.painter().line_segment([c + vec2(-d, -d), c + vec2(d, d)], Stroke::new(4.0, bg));
        ui.painter().line_segment([c + vec2(-d, -d), c + vec2(d, d)], Stroke::new(2.0, RED));
    }
    r
}

/// Mute and deafen, drawn right to left inside the user panel.
pub fn user_buttons(app: &mut App, ui: &mut Ui) {
    let deaf = app.voice.deaf;
    let mute = app.voice.mute || deaf;
    let d = toggle(ui, Icon::Headphones, deaf, 32.0, if deaf { "Undeafen" } else { "Deafen" });
    app.rects.insert("voice:deafen".into(), d.rect);
    if d.clicked() {
        app.voice.set_deaf(&app.net, !deaf);
    }
    let m = toggle(ui, Icon::Mic, mute, 32.0, if mute { "Unmute" } else { "Mute" });
    app.rects.insert("voice:mute".into(), m.rect);
    if m.clicked() {
        app.voice.set_mute(&app.net, !mute);
    }
}

/// Green ring around a speaking user's avatar.
pub fn ring(ui: &Ui, av: Rect) {
    ui.painter().circle_stroke(av.center(), av.width() / 2.0 + 1.5, Stroke::new(2.0, GREEN));
}

pub fn call_button(app: &mut App, ui: &mut Ui, ch: Id) {
    let inside = app.voice.chan == Some(ch);
    let (rect, r) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    phone(ui, rect.center(), if r.hovered() { pal().interactive_hover } else { pal().header_secondary });
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let r = r.on_hover_text(if inside { "In Call" } else { "Start Voice Call" });
    app.rects.insert("dm:call".into(), r.rect);
    if r.clicked() && !inside {
        app.voice.join(&app.net, &app.store, ch);
    }
}

/// Uploads a frame to the GPU when it's a new image, and returns something paintable.
fn texture(app: &mut App, ctx: &egui::Context, key: String, img: &Arc<ColorImage>) -> egui::TextureId {
    let ptr = Arc::as_ptr(img) as usize;
    if let Some((h, p)) = app.voice.tex.get_mut(&key) {
        if *p != ptr {
            h.set(img.clone(), egui::TextureOptions::LINEAR);
            *p = ptr;
        }
        return h.id();
    }
    let h = ctx.load_texture(key.clone(), img.clone(), egui::TextureOptions::LINEAR);
    let id = h.id();
    app.voice.tex.insert(key, (h, ptr));
    id
}

// letterboxed into `r`
fn frame(app: &mut App, ui: &Ui, key: String, img: &Arc<ColorImage>, r: Rect) {
    let id = texture(app, ui.ctx(), key, img);
    let k = (r.width() / img.size[0] as f32).min(r.height() / img.size[1] as f32);
    let fr = Rect::from_center_size(r.center(), vec2(img.size[0] as f32 * k, img.size[1] as f32 * k));
    egui::Image::new(egui::load::SizedTexture::new(id, fr.size())).corner_radius(6).paint_at(ui, fr);
}

fn live_badge(ui: &Ui, right_top: egui::Pos2) {
    let g = ui.painter().layout_no_wrap("LIVE".into(), bold(11.0), Color32::WHITE);
    let r = Rect::from_min_size(pos2(right_top.x - g.size().x - 12.0, right_top.y), g.size() + vec2(12.0, 4.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), RED);
    ui.painter().galley(r.min + vec2(6.0, 2.0), g, Color32::WHITE);
}

/// LIVE pill and camera mark at the end of a sidebar voice row; returns the width they took.
pub fn badges(ui: &Ui, voice: &crate::voice::VoiceUi, u: Id, row: Rect) -> f32 {
    let mut x = row.right() - 12.0;
    if voice.live.contains(&u) {
        live_badge(ui, pos2(x, row.center().y - 8.0));
        x -= 42.0;
    }
    if voice.cams.contains(&u) {
        camera(ui, pos2(x - 8.0, row.center().y), 0.8, pal().channel_default);
        x -= 20.0;
    }
    row.right() - 12.0 - x
}

/// The call area at the top of a DM while a call is going on.
pub fn dm_strip(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let members = app.voice.members(&app.store, chan.id);
    if members.is_empty() {
        return;
    }
    let inside = app.voice.chan == Some(chan.id);
    let watching = app.voice.stream.as_ref().filter(|_| inside).map(|l| l.frame.clone());
    let video = watching.is_some() || members.iter().any(|u| app.voice.video.contains_key(u));
    let ctx = ui.ctx().clone();
    let h = if app.ui.dm_call_h > 0.0 { app.ui.dm_call_h } else if video { 340.0 } else { 220.0 };
    // own id per mode so toggling video re-seeds the default instead of keeping the other mode's height
    let r = egui::Panel::top(if video { "dm_call_video" } else { "dm_call" })
        .default_size(h)
        .size_range(160.0..=(ui.available_height() - 300.0).clamp(160.0, 640.0))
        .resizable(true)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_floating))
        .show(ui, |ui| {
            let r = ui.max_rect();
            let top = Rect::from_min_max(r.min + vec2(16.0, 16.0), pos2(r.right() - 16.0, r.bottom() - 72.0));
            if let Some(f) = watching {
                stage(app, ui, top, f);
            } else {
                let sizes: Vec<egui::Vec2> = members
                    .iter()
                    .map(|u| if app.voice.video.contains_key(u) { vec2(top.height() * 16.0 / 9.0, top.height()) } else { vec2(80.0, 80.0) })
                    .collect();
                let w: f32 = sizes.iter().map(|s| s.x + 24.0).sum::<f32>() - 24.0;
                let mut x = r.center().x - w / 2.0;
                for (u, &s) in members.iter().zip(&sizes) {
                    let a = Rect::from_min_size(pos2(x, top.center().y - s.y / 2.0), s);
                    match app.voice.video.get(u).cloned() {
                        Some(img) => {
                            frame(app, ui, format!("cam:{u}"), &img, a);
                            if app.voice.is_speaking(*u) {
                                ui.painter().rect_stroke(a, CornerRadius::same(6), Stroke::new(3.0, GREEN), egui::StrokeKind::Outside);
                            }
                        }
                        None => {
                            if let Some(user) = app.store.user(*u) {
                                avatar(ui, a, &user.avatar_url());
                            }
                            if app.voice.is_speaking(*u) {
                                ui.painter().circle_stroke(a.center(), s.x / 2.0 + 3.0, Stroke::new(3.0, GREEN));
                            }
                        }
                    }
                    if app.voice.live.contains(u) {
                        live_badge(ui, a.right_top() + vec2(0.0, -6.0));
                        watch_click(app, ui, *u, a);
                    }
                    x += s.x + 24.0;
                }
            }
            controls(app, ui, chan.id, Rect::from_min_max(pos2(r.left(), r.bottom() - 64.0), r.max), "Join Call");
        });
    app.ui.dm_call_h = app.dragged(&ctx, r.response.rect.height(), h);
}

// a live user's tile or avatar starts watching their stream when clicked
fn watch_click(app: &mut App, ui: &Ui, u: Id, r: Rect) {
    if u == app.store.me || app.voice.chan.is_none() {
        return;
    }
    let resp = ui.interact(r, egui::Id::new(("watch", u)), Sense::click());
    app.rects.insert(format!("voice:watch:{u}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let b = Rect::from_center_size(r.center(), vec2(120.0, 32.0));
        ui.painter().rect_filled(b, CornerRadius::same(16), Color32::from_black_alpha(180));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Watch Stream", semi(13.0), Color32::WHITE);
    }
    if resp.clicked() {
        app.voice.watch(&app.net, u);
    }
}

// the big area showing the stream we're watching, or our own while live
fn stage(app: &mut App, ui: &mut Ui, r: Rect, img: Option<Arc<ColorImage>>) {
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::BLACK);
    let Some((who, mine)) = app.voice.stream.as_ref().map(|l| (l.user, l.mine)) else { return };
    match img {
        Some(img) => frame(app, ui, "stream".into(), &img, r),
        None => {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Connecting to stream…", reg(14.0), pal().header_secondary);
        }
    }
    let label = if mine { "You're live".to_string() } else { format!("{}'s screen", app.store.name_in(app.voice.guild, who)) };
    let g = ui.painter().layout_no_wrap(label, semi(13.0), Color32::WHITE);
    let tag = Rect::from_min_size(r.min + vec2(10.0, 10.0), g.size() + vec2(16.0, 8.0));
    ui.painter().rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(160));
    ui.painter().galley(tag.min + vec2(8.0, 4.0), g, Color32::WHITE);
    live_badge(ui, pos2(tag.right() + 52.0, tag.top() + 3.0));
    let stop = Rect::from_min_size(pos2(r.right() - 130.0, r.top() + 10.0), vec2(120.0, 30.0));
    let resp = ui.interact(stop, egui::Id::new("stop_stream"), Sense::click());
    ui.painter().rect_filled(stop, CornerRadius::same(4), if resp.hovered() { RED.gamma_multiply(0.85) } else { RED });
    ui.painter().text(stop.center(), Align2::CENTER_CENTER, if mine { "Stop Streaming" } else { "Stop Watching" }, semi(13.0), Color32::WHITE);
    app.rects.insert("voice:stopstream".into(), stop);
    if resp.clicked() {
        app.voice.end_stream(&app.net);
    }
}

/// Voice channel view: a tile per person, the stream on top when watching one, controls along the bottom.
pub fn room(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("room_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_floating).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Speaker, r, pal().channel_default);
                ui.add(egui::Label::new(egui::RichText::new(&chan.name).font(semi(16.0)).color(pal().header_primary)).selectable(false));
            });
        });
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_floating)).show(ui, |ui| {
        let r = ui.max_rect();
        let members = app.voice.members(&app.store, chan.id);
        let mut area = Rect::from_min_max(r.min + vec2(16.0, 16.0), pos2(r.right() - 16.0, r.bottom() - 80.0));
        if members.is_empty() {
            ui.painter().text(area.center() - vec2(0.0, 12.0), Align2::CENTER_CENTER, &chan.name, bold(24.0), pal().header_primary);
            ui.painter().text(
                area.center() + vec2(0.0, 16.0),
                Align2::CENTER_CENTER,
                "No one is currently in voice",
                reg(14.0),
                pal().header_secondary,
            );
        }
        if let Some(f) = app.voice.stream.as_ref().filter(|_| app.voice.chan == Some(chan.id)).map(|l| l.frame.clone()) {
            let top = Rect::from_min_max(area.min, pos2(area.right(), area.top() + area.height() * 0.68));
            stage(app, ui, top, f);
            area.min.y = top.bottom() + 8.0;
        }
        for (u, tile) in members.iter().zip(grid(area, members.len())) {
            ui.painter().rect_filled(tile, CornerRadius::same(8), tile_color(*u));
            match app.voice.video.get(u).cloned() {
                Some(img) => frame(app, ui, format!("cam:{u}"), &img, tile),
                None => {
                    let av = (tile.height().min(tile.width()) * 0.35).clamp(40.0, 96.0);
                    let a = Rect::from_center_size(tile.center(), vec2(av, av));
                    if let Some(user) = app.store.user(*u) {
                        avatar(ui, a, &user.avatar_url());
                    }
                }
            }
            let name = app.store.name_in(chan.guild, *u);
            let g = ui.painter().layout_no_wrap(name, semi(13.0), Color32::WHITE);
            let tag = Rect::from_min_size(pos2(tile.left() + 8.0, tile.bottom() - 30.0), g.size() + vec2(16.0, 8.0));
            ui.painter().rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(140));
            ui.painter().galley(tag.min + vec2(8.0, 4.0), g, Color32::WHITE);
            if app.voice.live.contains(u) {
                live_badge(ui, tile.right_top() + vec2(-8.0, 8.0));
                watch_click(app, ui, *u, tile);
            }
            if app.voice.is_speaking(*u) {
                ui.painter().rect_stroke(tile, CornerRadius::same(8), Stroke::new(3.0, GREEN), egui::StrokeKind::Inside);
            }
        }
        controls(app, ui, chan.id, Rect::from_min_max(pos2(r.left(), r.bottom() - 72.0), r.max), "Join Voice");
    });
}

fn grid(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let gap = 8.0;
    // try every column count and keep whichever gives the biggest 16:9 tiles
    let mut best = (0.0, 1, 1);
    for cols in 1..=n {
        let rows = n.div_ceil(cols);
        let w = (area.width() - gap * (cols - 1) as f32) / cols as f32;
        let h = (area.height() - gap * (rows - 1) as f32) / rows as f32;
        let tw = w.min(h * 16.0 / 9.0);
        if tw > best.0 {
            best = (tw, cols, rows);
        }
    }
    let (tw, cols, rows) = best;
    let th = tw * 9.0 / 16.0;
    let total_h = rows as f32 * th + (rows - 1) as f32 * gap;
    let mut out = Vec::with_capacity(n);
    for row in 0..rows {
        let in_row = (n - row * cols).min(cols);
        let row_w = in_row as f32 * tw + (in_row - 1) as f32 * gap;
        let x0 = area.center().x - row_w / 2.0;
        let y = area.center().y - total_h / 2.0 + row as f32 * (th + gap);
        for k in 0..in_row {
            out.push(Rect::from_min_size(pos2(x0 + k as f32 * (tw + gap), y), vec2(tw, th)));
        }
    }
    out
}

fn tile_color(u: Id) -> Color32 {
    const C: [Color32; 5] = [
        Color32::from_rgb(0x3e, 0x45, 0x8f),
        Color32::from_rgb(0x2d, 0x5e, 0x45),
        Color32::from_rgb(0x7a, 0x5a, 0x22),
        Color32::from_rgb(0x7a, 0x30, 0x34),
        Color32::from_rgb(0x6b, 0x34, 0x5c),
    ];
    C[((u ^ (u >> 22)) % 5) as usize]
}

fn controls(app: &mut App, ui: &mut Ui, ch: Id, bar: Rect, join_text: &str) {
    let inside = app.voice.chan == Some(ch);
    let c = bar.center();
    if !inside {
        let b = Rect::from_center_size(c, vec2(140.0, 40.0));
        let r = ui.interact(b, egui::Id::new(("join", ch)), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(20), if r.hovered() { GREEN.gamma_multiply(0.85) } else { GREEN });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, join_text, semi(14.0), Color32::WHITE);
        app.rects.insert("voice:join".into(), b);
        if r.clicked() {
            app.voice.join(&app.net, &app.store, ch);
        }
        return;
    }
    let mute = app.voice.mute || app.voice.deaf;
    let deaf = app.voice.deaf;
    let cam = app.voice.camera;
    let sharing = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let me = app.store.me;
    for i in 0..5 {
        let b = Rect::from_center_size(c + vec2((i as f32 - 2.0) * 64.0, 0.0), vec2(52.0, 52.0));
        let r = ui.interact(b, egui::Id::new(("ctl", i)), Sense::click());
        // muted and deafened get the slash; camera and screen just light up when on
        let (on, slash, tip, key) = match i {
            0 => (mute, true, if mute { "Unmute" } else { "Mute" }, "voice:mute_big"),
            1 => (deaf, true, if deaf { "Undeafen" } else { "Deafen" }, "voice:deafen_big"),
            2 => (cam, false, if cam { "Turn Off Camera" } else { "Turn On Camera" }, "voice:camera"),
            3 => (sharing, false, if sharing { "Stop Streaming" } else { "Share Your Screen" }, "voice:screen"),
            _ => (false, false, "Disconnect", "voice:leave_big"),
        };
        let fill = if i == 4 { RED } else if on { Color32::WHITE } else { pal().bg_mod_selected };
        ui.painter().circle_filled(b.center(), 26.0, if r.hovered() { fill.gamma_multiply(0.85) } else { fill });
        if i == 4 {
            hangup(ui, b.center(), Color32::WHITE);
        } else {
            let fg = if on { Color32::BLACK } else { Color32::WHITE };
            let icon = Rect::from_center_size(b.center(), vec2(22.0, 22.0));
            match i {
                0 => icons::draw(ui.painter(), Icon::Mic, icon, fg),
                1 => icons::draw(ui.painter(), Icon::Headphones, icon, fg),
                2 => camera(ui, b.center(), 1.2, fg),
                _ => screen(ui, b.center(), 1.2, fg),
            }
            if on && slash {
                ui.painter().line_segment([b.center() + vec2(-10.0, -10.0), b.center() + vec2(10.0, 10.0)], Stroke::new(2.5, RED));
            }
        }
        app.rects.insert(key.into(), b);
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.on_hover_text(tip).clicked() {
            match i {
                0 => app.voice.set_mute(&app.net, !mute),
                1 => app.voice.set_deaf(&app.net, !deaf),
                2 => app.voice.set_camera(&app.net, me, !cam),
                3 if sharing => app.voice.end_stream(&app.net),
                3 => app.voice.go_live(&app.net, me),
                _ => app.voice.leave(&app.net),
            }
        }
    }
}

type Devices = Arc<Mutex<Option<(Vec<String>, Vec<String>)>>>;

/// Voice & Video settings page. Returns true when a setting changed.
pub fn settings(app: &mut App, ui: &mut Ui) -> bool {
    ui.label(RichText::new("Voice & Video").font(bold(20.0)).color(pal().header_primary));
    ui.add_space(12.0);
    // enumerating WASAPI endpoints takes a while, so it happens once, on a thread
    let key = egui::Id::new("voice_devices");
    let devs: Devices = match ui.data(|d| d.get_temp::<Devices>(key)) {
        Some(d) => d,
        None => {
            let d: Devices = Arc::default();
            let (out, ctx) = (d.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let list = audio::devices();
                if let Ok(mut o) = out.lock() {
                    *o = Some(list);
                }
                ctx.request_repaint();
            });
            ui.data_mut(|m| m.insert_temp(key, d.clone()));
            d
        }
    };
    let (ins, outs) = devs.lock().ok().and_then(|d| d.clone()).unwrap_or_default();
    let s = &mut app.settings;
    let mut changed = false;
    let mut goto_keybinds = false;
    ui.columns(2, |cols| {
        changed |= pick(&mut cols[0], "INPUT DEVICE", "mic_pick", &ins, &mut s.mic);
        changed |= pick(&mut cols[1], "OUTPUT DEVICE", "speaker_pick", &outs, &mut s.speaker);
    });
    ui.label(RichText::new("Device changes apply the next time you join a call.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(12.0);
    ui.columns(2, |cols| {
        for (i, (label, v)) in [("INPUT VOLUME", &mut s.input_volume), ("OUTPUT VOLUME", &mut s.output_volume)].into_iter().enumerate() {
            let ui = &mut cols[i];
            ui.label(RichText::new(label).font(bold(12.0)).color(pal().header_secondary));
            let mut pct = *v * 100.0;
            ui.spacing_mut().slider_width = ui.available_width() - 60.0;
            if ui.add(egui::Slider::new(&mut pct, 0.0..=200.0).step_by(1.0).suffix("%")).changed() {
                *v = pct / 100.0;
                changed = true;
            }
        }
    });
    ui.add_space(16.0);
    ui.label(RichText::new("INPUT MODE").font(bold(12.0)).color(pal().header_secondary));
    let mut ptt = s.ptt;
    if crate::ui::settings::radio(ui, &mut ptt, &[(false, "Voice Activity", ""), (true, "Push to Talk", "")]) {
        s.ptt = ptt;
        changed = true;
    }
    ui.add_space(8.0);
    if s.ptt {
        let keys = s.keybinds.iter().find(|(a, k)| a == "ptt" && !k.is_empty()).map(|(_, k)| crate::ui::settings::combo(k));
        ui.columns(2, |cols| {
            cols[0].label(RichText::new("SHORTCUT").font(bold(12.0)).color(pal().header_secondary));
            cols[0].label(RichText::new(keys.clone().unwrap_or_else(|| "No Keybind Set".into())).font(semi(16.0)).color(pal().header_primary));
            if crate::ui::settings::link(&mut cols[0], if keys.is_some() { "Edit Keybind" } else { "Add a Keybind" }).clicked() {
                goto_keybinds = true;
            }
            cols[1].label(RichText::new("PUSH TO TALK RELEASE DELAY").font(bold(12.0)).color(pal().header_secondary));
            let mut ms = s.ptt_delay_ms as f32;
            cols[1].spacing_mut().slider_width = cols[1].available_width() - 70.0;
            if cols[1].add(egui::Slider::new(&mut ms, 0.0..=2000.0).step_by(10.0).suffix("ms")).changed() {
                s.ptt_delay_ms = ms as u32;
                changed = true;
            }
        });
        ui.label(RichText::new("Push to talk works anywhere, even when Fastcord isn't focused.").font(reg(14.0)).color(pal().header_secondary));
        if goto_keybinds {
            app.settings_page = Some(crate::ui::settings::Page::Keybinds);
        }
        return changed;
    }
    ui.add_space(4.0);
    ui.label(RichText::new("INPUT SENSITIVITY").font(bold(12.0)).color(pal().header_secondary));
    changed |= crate::ui::settings::toggle(ui, &mut s.auto_sensitivity, "Automatically determine input sensitivity", "");
    if s.auto_sensitivity {
        return changed;
    }
    let mut db = s.vad_db as f32;
    ui.spacing_mut().slider_width = ui.available_width().min(500.0);
    let r = ui.add(egui::Slider::new(&mut db, -100.0..=0.0).step_by(1.0).show_value(false));
    ui.label(RichText::new(format!("{db:.0} dB")).font(reg(13.0)).color(pal().header_secondary));
    if r.changed() {
        s.vad_db = db as i32;
        changed = true;
    }
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width().min(500.0), 10.0), Sense::hover());
    ui.painter().rect_filled(bar, CornerRadius::same(5), pal().bg_tertiary);
    let x = |v: f32| bar.left() + bar.width() * ((v + 100.0) / 100.0).clamp(0.0, 1.0);
    if app.voice.chan.is_some() {
        let level = app.voice.shared.level.load(Relaxed) as f32;
        let fill = Rect::from_min_max(bar.min, pos2(x(level), bar.bottom()));
        ui.painter().rect_filled(fill, CornerRadius::same(5), if level > db { GREEN } else { YELLOW });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    ui.painter().vline(x(db), bar.y_range().expand(3.0), Stroke::new(2.0, Color32::WHITE));
    let note = if app.voice.chan.is_some() {
        "You transmit while the bar is green."
    } else {
        "Join a voice channel to see your input level."
    };
    ui.label(RichText::new(note).font(reg(14.0)).color(pal().header_secondary));
    changed
}

fn pick(ui: &mut Ui, label: &str, id: &str, names: &[String], cur: &mut Option<String>) -> bool {
    ui.label(RichText::new(label).font(bold(12.0)).color(pal().header_secondary));
    let shown = cur.clone().unwrap_or_else(|| "Default".into());
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(ui.available_width() - 8.0).selected_text(shown).show_ui(ui, |ui| {
        changed |= ui.selectable_value(cur, None, "Default").changed();
        for n in names {
            changed |= ui.selectable_value(cur, Some(n.clone()), n).changed();
        }
    });
    changed
}
