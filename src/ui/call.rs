use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, avatar, channel_title, icon_button};
use crate::state::{Channel, Id};
use crate::theme::*;
use crate::voice::State;

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
    p.hline(r.x_range().expand(8.0), r.top(), Stroke::new(1.0, BG_MOD_HOVER));
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
        if sub_r.hovered() { HEADER_PRIMARY } else { HEADER_SECONDARY },
    );
    if sub_r.clicked() {
        app.open(ch);
    }
    let b = Rect::from_center_size(pos2(r.right() - 16.0, y), vec2(32.0, 32.0));
    let leave = ui.interact(b, egui::Id::new("voice_leave"), Sense::click());
    if leave.hovered() {
        ui.painter().rect_filled(b, CornerRadius::same(4), BG_MOD_HOVER);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    hangup(ui, b.center(), if leave.hovered() { HEADER_PRIMARY } else { HEADER_SECONDARY });
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
        let bg = if r.hovered() { BG_MOD_HOVER } else { BG_SECONDARY_ALT };
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
    phone(ui, rect.center(), if r.hovered() { INTERACTIVE_HOVER } else { HEADER_SECONDARY });
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let r = r.on_hover_text(if inside { "In Call" } else { "Start Voice Call" });
    app.rects.insert("dm:call".into(), r.rect);
    if r.clicked() && !inside {
        app.voice.join(&app.net, &app.store, ch);
    }
}

/// The call area at the top of a DM while a call is going on.
pub fn dm_strip(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let members = app.voice.members(&app.store, chan.id);
    if members.is_empty() {
        return;
    }
    egui::Panel::top("dm_call")
        .exact_size(220.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BG_FLOATING))
        .show(ui, |ui| {
            let r = ui.max_rect();
            let av = 80.0;
            let gap = 24.0;
            let w = members.len() as f32 * (av + gap) - gap;
            let mut x = r.center().x - w / 2.0;
            for u in &members {
                let a = Rect::from_min_size(pos2(x, r.top() + 40.0), vec2(av, av));
                if let Some(user) = app.store.user(*u) {
                    avatar(ui, a, &user.avatar_url());
                }
                if app.voice.is_speaking(*u) {
                    ui.painter().circle_stroke(a.center(), av / 2.0 + 3.0, Stroke::new(3.0, GREEN));
                }
                x += av + gap;
            }
            controls(app, ui, chan.id, Rect::from_min_max(pos2(r.left(), r.bottom() - 64.0), r.max), "Join Call");
        });
}

/// Voice channel view: a tile per person, controls along the bottom.
pub fn room(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("room_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BG_FLOATING).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Speaker, r, CHANNEL_DEFAULT);
                ui.add(egui::Label::new(egui::RichText::new(&chan.name).font(semi(16.0)).color(HEADER_PRIMARY)).selectable(false));
            });
        });
    egui::CentralPanel::default().frame(Frame::new().fill(BG_FLOATING)).show(ui, |ui| {
        let r = ui.max_rect();
        let members = app.voice.members(&app.store, chan.id);
        let area = Rect::from_min_max(r.min + vec2(16.0, 16.0), pos2(r.right() - 16.0, r.bottom() - 80.0));
        if members.is_empty() {
            ui.painter().text(area.center() - vec2(0.0, 12.0), Align2::CENTER_CENTER, &chan.name, bold(24.0), HEADER_PRIMARY);
            ui.painter().text(
                area.center() + vec2(0.0, 16.0),
                Align2::CENTER_CENTER,
                "No one is currently in voice",
                reg(14.0),
                HEADER_SECONDARY,
            );
        }
        for (u, tile) in members.iter().zip(grid(area, members.len())) {
            ui.painter().rect_filled(tile, CornerRadius::same(8), tile_color(*u));
            let av = (tile.height().min(tile.width()) * 0.35).clamp(40.0, 96.0);
            let a = Rect::from_center_size(tile.center(), vec2(av, av));
            if let Some(user) = app.store.user(*u) {
                avatar(ui, a, &user.avatar_url());
            }
            let name = app.store.name_in(chan.guild, *u);
            let g = ui.painter().layout_no_wrap(name, semi(13.0), Color32::WHITE);
            let tag = Rect::from_min_size(pos2(tile.left() + 8.0, tile.bottom() - 30.0), g.size() + vec2(16.0, 8.0));
            ui.painter().rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(140));
            ui.painter().galley(tag.min + vec2(8.0, 4.0), g, Color32::WHITE);
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
    C[((u >> 22) % 5) as usize]
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
    let spots = [c - vec2(64.0, 0.0), c, c + vec2(64.0, 0.0)];
    for (i, at) in spots.into_iter().enumerate() {
        let b = Rect::from_center_size(at, vec2(52.0, 52.0));
        let r = ui.interact(b, egui::Id::new(("ctl", i)), Sense::click());
        let (fill, glyph, on) = match i {
            0 => (if mute { Color32::WHITE } else { BG_MOD_SELECTED }, "🎤", mute),
            1 => (if deaf { Color32::WHITE } else { BG_MOD_SELECTED }, "🎧", deaf),
            _ => (RED, "", false),
        };
        ui.painter().circle_filled(b.center(), 26.0, if r.hovered() { fill.gamma_multiply(0.85) } else { fill });
        if i == 2 {
            hangup(ui, b.center(), Color32::WHITE);
        } else {
            let fg = if on { Color32::BLACK } else { Color32::WHITE };
            ui.painter().text(b.center(), Align2::CENTER_CENTER, glyph, reg(22.0), fg);
            if on {
                ui.painter().line_segment([b.center() + vec2(-10.0, -10.0), b.center() + vec2(10.0, 10.0)], Stroke::new(2.5, RED));
            }
        }
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            match i {
                0 => app.voice.set_mute(&app.net, !mute),
                1 => app.voice.set_deaf(&app.net, !deaf),
                _ => app.voice.leave(&app.net),
            }
        }
    }
}
