use egui::{Align2, Color32, ColorImage, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::sync::{Arc, Mutex};

use super::icons::{self, Icon};
use super::{App, avatar, channel_title};
use crate::state::{Channel, Id};
use crate::theme::*;
use crate::voice::{State, audio};

/// "Voice Connected" half of the account card: status and disconnect on top,
/// camera and screen share underneath, laid out in the area `ui` was given.
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
    let y = r.top() + 16.0;
    // discord's mark for the connection: a 32 tile washed in the state's colour, the signal in it
    let tile = Rect::from_min_size(r.min, vec2(32.0, 32.0));
    ui.painter().rect(tile, 8, col.gamma_multiply(0.12), Stroke::new(1.0, col.gamma_multiply(0.24)), egui::StrokeKind::Inside);
    signal(ui, tile.center(), col);
    // the status opens discord's connection popout: ping graph, stats, and the privacy code
    let t = ui.painter().text(pos2(tile.right() + 8.0, y - 8.0), Align2::LEFT_CENTER, text, medium(16.0), col);
    let status = ui.interact(t, egui::Id::new("voice_status"), Sense::click());
    if status.hovered() {
        ui.painter().hline(t.x_range(), t.bottom() - 1.0, Stroke::new(1.0, col));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    app.rects.insert("voice:status".into(), t);
    rtc_popout(app, &status);
    let sub = Rect::from_min_max(pos2(tile.right() + 8.0, y + 2.0), pos2(r.right() - 40.0, y + 18.0));
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

    let cam = app.voice.camera;
    let sharing = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let me = app.store.me;
    let w = (r.width() - 8.0) / 2.0;
    for i in 0..2 {
        let b = Rect::from_min_size(pos2(r.left() + i as f32 * (w + 8.0), r.bottom() - 32.0), vec2(w, 32.0));
        let (on, tip, key) = match i {
            0 => (cam, if cam { "Turn Off Camera" } else { "Turn On Camera" }, "voice:panel_camera"),
            _ => (sharing, if sharing { "Stop Streaming" } else { "Share Your Screen" }, "voice:panel_screen"),
        };
        let resp = ui.interact(b, egui::Id::new(("voice_panel_btn", i)), Sense::click());
        let fill = match (on, resp.hovered()) {
            (true, false) => GREEN.gamma_multiply(0.12),
            (true, true) => GREEN.gamma_multiply(0.24),
            (false, false) => pal().control,
            (false, true) => pal().control_hover,
        };
        ui.painter().rect_filled(b, CornerRadius::same(8), fill);
        let fg = if on { GREEN } else { pal().header_primary };
        // discord's buttons: the camera crossed out while it's off, the screen with its arrow
        let icon = match (i, on) {
            (0, false) => Icon::VideoSlash,
            (0, true) => Icon::Video,
            _ => Icon::ScreenArrow,
        };
        icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(20.0, 20.0)), fg);
        app.rects.insert(key.into(), b);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(tip).clicked() {
            match i {
                0 => app.voice.set_camera(&app.net, me, !cam),
                _ if sharing => app.voice.end_stream(&app.net),
                _ => app.share = Some(super::share::Picker::new(ui.ctx())),
            }
        }
    }
}

/// Discord's RTC popout over the voice panel: Connection, with the last few minutes of pings, the
/// server, the numbers and what they mean; and Privacy, with the call's end-to-end privacy code.
fn rtc_popout(app: &mut App, status: &egui::Response) {
    let tab_id = egui::Id::new("rtc_tab");
    let debug_id = egui::Id::new("rtc_debug");
    let frame = Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(16);
    let mut copy = None;
    let shown = egui::Popup::from_toggle_button_response(status)
        .align(egui::RectAlign::TOP_START)
        .gap(12.0)
        .frame(frame)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(256.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut tab: u8 = ui.data(|d| d.get_temp(tab_id)).unwrap_or(0);
            let (bar, _) = ui.allocate_exact_size(vec2(256.0, 24.0), Sense::hover());
            let mut x = bar.left();
            for (i, label) in ["Connection", "Privacy"].into_iter().enumerate() {
                let on = tab == i as u8;
                let g = ui.painter().layout_no_wrap(label.into(), medium(16.0), Color32::WHITE);
                let r = Rect::from_min_size(pos2(x, bar.top()), vec2(g.size().x, 24.0));
                let resp = ui.interact(r, egui::Id::new(("rtc_tab", i)), Sense::click());
                let col = if on { Color32::from_rgb(0x94, 0x9c, 0xf7) } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
                ui.painter().galley(pos2(x, bar.top()), g, col);
                if on {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(r.left(), bar.bottom() + 6.0), vec2(r.width(), 2.0)), 1, BLURPLE);
                }
                if resp.clicked() {
                    tab = i as u8;
                    ui.data_mut(|d| d.insert_temp(tab_id, tab));
                }
                app.rects.insert(format!("rtc:tab:{i}"), r);
                x = r.right() + 24.0;
            }
            ui.add_space(8.0);
            let (line, _) = ui.allocate_exact_size(vec2(256.0, 1.0), Sense::hover());
            ui.painter().hline(line.x_range().expand(16.0), line.center().y, Stroke::new(1.0, pal().border_subtle));
            ui.add_space(16.0);
            if tab == 1 {
                privacy(ui, app.voice.privacy.as_deref());
                return;
            }
            ping_graph(ui, &app.voice.pings);
            ui.add_space(20.0);
            let host = app.voice.host().unwrap_or("Connecting…").to_string();
            ui.label(egui::RichText::new(&host).font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let pings = &app.voice.pings;
            let avg = (!pings.is_empty()).then(|| pings.iter().map(|(_, p)| *p as f64).sum::<f64>() / pings.len() as f64);
            let fmt = |v: Option<String>| v.unwrap_or_else(|| "--".into());
            stat(ui, "Average ping: ", &format!("{} ms", fmt(avg.map(|a| format!("{a:.0}")))));
            stat(ui, "Last ping: ", &format!("{} ms", fmt(pings.back().map(|(_, p)| p.to_string()))));
            stat(ui, "Outbound packet loss rate: ", &format!("{}%", fmt(app.voice.loss.map(|l| format!("{l:.1}")))));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let muted = |s: &str| egui::RichText::new(s).font(reg(12.0)).color(pal().text_muted);
                ui.label(muted("You may notice delayed audio at 250 ms or higher. You may sound robotic if your packet loss rate is over 10%. If the problem persists, disconnect and try again. See our "));
                let link = ui.add(egui::Label::new(egui::RichText::new("troubleshooting guide").font(reg(12.0)).color(pal().link)).sense(Sense::click()));
                if link.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if link.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360045138471"));
                }
                ui.label(muted(" for more assistance."));
            });
            ui.add_space(16.0);
            let mut debug: bool = ui.data(|d| d.get_temp(debug_id)).unwrap_or(false);
            let info = format!(
                "server {host}\nendpoint {}\nend-to-end encrypted {}\naverage ping {} ms, last {} ms\noutbound loss {}%",
                app.voice.endpoint().unwrap_or("-"),
                if app.voice.privacy.is_some() { "yes" } else { "no" },
                fmt(avg.map(|a| format!("{a:.0}"))),
                fmt(pings.back().map(|(_, p)| p.to_string())),
                fmt(app.voice.loss.map(|l| format!("{l:.1}"))),
            );
            let (row, _) = ui.allocate_exact_size(vec2(256.0, 32.0), Sense::hover());
            let half = (row.width() - 8.0) / 2.0;
            for (i, (icon, label)) in [(Icon::Bug, "Debug"), (Icon::Copy, "Copy Logs")].into_iter().enumerate() {
                let b = Rect::from_min_size(pos2(row.left() + i as f32 * (half + 8.0), row.top()), vec2(half, 32.0));
                let resp = ui.interact(b, egui::Id::new(("rtc_btn", i)), Sense::click());
                ui.painter().rect(b, 8, if resp.hovered() { pal().control_hover } else { pal().control }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), pal().header_primary);
                let x = b.center().x - (g.size().x + 26.0) / 2.0;
                icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(x + 9.0, b.center().y), vec2(18.0, 18.0)), pal().header_primary);
                ui.painter().galley(pos2(x + 26.0, b.center().y - g.size().y / 2.0), g, pal().header_primary);
                if resp.clicked() {
                    if i == 0 {
                        debug = !debug;
                        ui.data_mut(|d| d.insert_temp(debug_id, debug));
                    } else {
                        copy = Some(info.clone());
                    }
                }
            }
            if debug {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&info).font(mono(11.0)).color(pal().header_secondary));
            }
            if app.voice.privacy.is_some() {
                ui.add_space(8.0);
                let (b, _) = ui.allocate_exact_size(vec2(256.0, 24.0), Sense::hover());
                ui.painter().rect_filled(b, 4, GREEN.gamma_multiply(0.12));
                icons::draw(ui.painter(), Icon::Lock, Rect::from_center_size(pos2(b.left() + 14.0, b.center().y), vec2(12.0, 12.0)), GREEN);
                ui.painter().text(pos2(b.left() + 26.0, b.center().y), Align2::LEFT_CENTER, "End-to-end encrypted", medium(12.0), GREEN);
            }
        });
    if let Some(r) = shown.as_ref().map(|s| s.response.rect) {
        app.rects.insert("rtc:popout".into(), r);
    }
    if let Some(text) = copy {
        status.ctx.copy_text(text);
        app.toast("Connection info copied.");
    }
}

/// "Average ping: 52 ms", the number in bold.
fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(egui::RichText::new(label).font(reg(12.0)).color(pal().text_normal));
        ui.label(egui::RichText::new(value).font(semi(12.0)).color(pal().header_primary));
    });
    ui.add_space(2.0);
}

/// The pings of the last four minutes as a line, 0 to 100ms (more if they run higher), with the
/// minutes along the bottom.
fn ping_graph(ui: &mut Ui, pings: &std::collections::VecDeque<(f64, u32)>) {
    let (r, _) = ui.allocate_exact_size(vec2(256.0, 96.0), Sense::hover());
    let plot = Rect::from_min_max(r.min, pos2(r.right() - 28.0, r.bottom() - 20.0));
    let p = ui.painter();
    p.rect_filled(plot, 2, Color32::BLACK);
    let top = pings.iter().map(|(_, ms)| *ms).max().unwrap_or(0).max(100).div_ceil(50) * 50;
    for v in [0, top / 2, top] {
        let y = plot.bottom() - plot.height() * v as f32 / top as f32;
        p.text(pos2(plot.right() + 6.0, y), Align2::LEFT_CENTER, v.to_string(), reg(12.0), pal().header_primary);
    }
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let span = 240.0;
    let x = |t: f64| plot.right() - plot.width() * ((now - t) / span).clamp(0.0, 1.0) as f32;
    // a label on each minute that fits
    let first = ((now - span) / 60.0).ceil() as i64;
    for m in first..=(now / 60.0) as i64 {
        let t = (m * 60) as f64;
        let at = x(t);
        if at < plot.left() + 24.0 || at > plot.right() - 20.0 {
            continue;
        }
        let local = chrono::DateTime::from_timestamp(m * 60, 0).map(|d| d.with_timezone(&chrono::Local).format("%-I:%M %p").to_string()).unwrap_or_default();
        p.text(pos2(at, plot.bottom() + 10.0), Align2::CENTER_CENTER, local, reg(12.0), pal().header_primary);
    }
    let pts: Vec<egui::Pos2> = pings.iter().filter(|(t, _)| now - t <= span).map(|(t, ms)| pos2(x(*t), plot.bottom() - plot.height() * (*ms).min(top) as f32 / top as f32)).collect();
    if pts.len() > 1 {
        p.add(egui::Shape::line(pts, Stroke::new(2.0, BLURPLE)));
    } else if let Some(pt) = pts.first() {
        p.circle_filled(*pt, 2.0, BLURPLE);
    }
}

/// The privacy tab: DAVE's code in groups of five, to read out and compare.
fn privacy(ui: &mut Ui, code: Option<&str>) {
    ui.label(egui::RichText::new("Voice Privacy Code").font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    match code {
        Some(code) => {
            ui.label(egui::RichText::new("Compare the numbers below with your friends. If they match, this call is end-to-end encrypted.").font(reg(14.0)).color(pal().text_normal));
            ui.add_space(12.0);
            let groups: Vec<String> = code.chars().collect::<Vec<_>>().chunks(5).map(|c| c.iter().collect()).collect();
            for row in groups.chunks(3) {
                ui.label(egui::RichText::new(row.join("  ")).font(mono(18.0)).color(pal().header_primary));
                ui.add_space(4.0);
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new("A new code is generated when people join or leave this call.").font(reg(12.0)).color(pal().text_muted));
        }
        None => {
            ui.label(egui::RichText::new("This call isn't end-to-end encrypted yet, so there's no code to compare.").font(reg(14.0)).color(pal().text_normal));
        }
    }
    ui.add_space(8.0);
    let link = ui.add(egui::Label::new(egui::RichText::new("Learn more about end-to-end encryption.").font(reg(12.0)).color(pal().link)).sense(Sense::click()));
    if link.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/25968222946071"));
    }
}

// a dot in the corner and two rings out from it, like a broadcast mark
fn signal(ui: &Ui, c: egui::Pos2, col: Color32) {
    let o = c + vec2(-6.0, 6.0);
    ui.painter().circle_filled(o + vec2(1.2, -1.2), 1.8, col);
    for rad in [6.5, 11.5] {
        let pts: Vec<egui::Pos2> = (0..=12)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
                o + vec2(a.cos(), a.sin()) * rad
            })
            .collect();
        ui.painter().add(egui::Shape::line(pts, Stroke::new(2.2, col)));
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

fn camera(ui: &Ui, c: egui::Pos2, k: f32, col: Color32) {
    icons::draw(ui.painter(), Icon::Video, Rect::from_center_size(c, vec2(20.0, 20.0) * k), col);
}

fn screen(ui: &Ui, c: egui::Pos2, k: f32, col: Color32) {
    icons::draw(ui.painter(), Icon::Screen, Rect::from_center_size(c, vec2(20.0, 20.0) * k), col);
}

fn hangup(ui: &Ui, c: egui::Pos2, col: Color32) {
    icons::draw(ui.painter(), Icon::HangUp, Rect::from_center_size(c, vec2(20.0, 20.0)), col);
}

/// Mute and deafen, drawn right to left inside the user panel.
pub fn user_buttons(app: &mut App, ui: &mut Ui) {
    let deaf = app.voice.deaf;
    let mute = app.voice.mute || deaf;
    // laid out right to left: each chevron sits after its button, groups 8 apart
    ui.add_space(8.0);
    let mut changed = device_menu(ui, "Output Device", true, &mut app.settings.speaker, deaf);
    ui.add_space(1.0);
    let d = split(ui, if deaf { Icon::HeadphonesOff } else { Icon::Headphones }, deaf, if deaf { "Undeafen" } else { "Deafen" });
    app.rects.insert("voice:deafen".into(), d.rect);
    if d.clicked() {
        app.voice.set_deaf(&app.net, !deaf);
    }
    ui.add_space(8.0);
    changed |= device_menu(ui, "Input Device", false, &mut app.settings.mic, mute);
    ui.add_space(1.0);
    let m = split(ui, if mute { Icon::MicOff } else { Icon::Mic }, mute, if mute { "Unmute" } else { "Mute" });
    app.rects.insert("voice:mute".into(), m.rect);
    if m.clicked() {
        app.voice.set_mute(&app.net, !mute);
    }
    if changed {
        app.save_settings(ui.ctx());
    }
}

const DANGER: Color32 = Color32::from_rgb(0xd2, 0x2d, 0x39);

/// The left half of Discord's mic/headphone split button; washed red with a red icon while off.
fn split(ui: &mut Ui, icon: Icon, off: bool, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let bg = match (off, resp.hovered()) {
        (true, h) => DANGER.gamma_multiply(if h { 0.2 } else { 0.12 }),
        (false, true) => pal().control_hover,
        (false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(r, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, bg);
    let col = if off { DANGER } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
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
        let ring = !ui.input(|i| i.modifiers.shift);
        app.join_voice(ch, ring);
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
    let g = ui.painter().layout_no_wrap("LIVE".into(), semi(11.0), Color32::WHITE);
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
    // discord's order is mute, deafen, camera, live; a server's own mute or deafen is red
    if let Some([mute, server_mute, deaf, server_deaf]) = voice.muted.get(&u).copied() {
        for (on, server, icon, tip) in [(deaf, server_deaf, Icon::HeadphonesOff, "Deafened"), (mute, server_mute, Icon::MicOff, "Muted")] {
            if !on {
                continue;
            }
            let r = Rect::from_center_size(pos2(x - 8.0, row.center().y), vec2(16.0, 16.0));
            icons::draw(ui.painter(), icon, r, if server { pal().critical } else { pal().channel_default });
            let tip = if server { format!("Server {tip}") } else { tip.to_string() };
            super::settings::mark(ui, &format!("voice_state:{u}:{tip}"), r);
            ui.interact(r, ui.id().with(("voice_state", u, &tip)), Sense::hover()).on_hover_text(tip);
            x -= 20.0;
        }
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
    if let Some(h) = app.dragged(&ctx, r.response.rect.height(), h) {
        app.ui.dm_call_h = h;
    }
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

/// 1 while the mouse is over the call and has moved in the last 3 seconds, easing to 0 after:
/// discord's call hides its bars, gradients and name tags once the mouse rests or leaves.
fn awake(ui: &Ui, area: Rect) -> f32 {
    let ctx = ui.ctx();
    let id = egui::Id::new("call_moved");
    let (now, over, moved) = ctx.input(|i| (i.time, i.pointer.hover_pos().is_some_and(|p| area.contains(p)), i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down()));
    if over && moved {
        ctx.data_mut(|d| d.insert_temp(id, now));
    }
    let left = 3.0 - (now - ctx.data(|d| d.get_temp::<f64>(id)).unwrap_or(f64::MIN));
    let on = over && left > 0.0;
    if on {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
    }
    ctx.animate_bool_with_time(egui::Id::new("call_awake"), on, 0.2)
}

/// Discord's 160 tall fade from black at the top and bottom of the call while it's awake.
fn gradient(ui: &Ui, r: Rect, top: bool, k: f32) {
    const STOPS: [(f32, f32); 13] = [
        (0.0, 1.0), (0.19, 0.738), (0.34, 0.541), (0.47, 0.382), (0.565, 0.278), (0.65, 0.194), (0.73, 0.126),
        (0.802, 0.075), (0.861, 0.042), (0.91, 0.021), (0.952, 0.008), (0.982, 0.002), (1.0, 0.0),
    ];
    let mut mesh = egui::Mesh::default();
    for (i, (t, a)) in STOPS.into_iter().enumerate() {
        let y = if top { r.top() + 160.0 * t } else { r.bottom() - 160.0 * t };
        let c = Color32::from_black_alpha((a * k * 255.0) as u8);
        mesh.colored_vertex(pos2(r.left(), y), c);
        mesh.colored_vertex(pos2(r.right(), y), c);
        if i > 0 {
            let j = (i * 2) as u32;
            mesh.add_triangle(j - 2, j - 1, j);
            mesh.add_triangle(j - 1, j, j + 1);
        }
    }
    ui.painter().add(mesh);
}

/// A tile's colour, the way discord picks it: the banner colour on their profile, else the main
/// colour of their avatar.
fn tile_color(app: &mut App, ctx: &egui::Context, u: Id, guild: Id) -> Color32 {
    match app.profiles.get(&u) {
        Some(p) => {
            if let Some(c) = p["user_profile"]["accent_color"].as_u64().or(p["user"]["accent_color"].as_u64()) {
                return role_color(c as u32);
            }
        }
        None => {
            let asked = egui::Id::new(("tile_profile", u));
            if !ctx.data(|d| d.get_temp::<bool>(asked).unwrap_or(false)) {
                ctx.data_mut(|d| d.insert_temp(asked, true));
                let q = if guild != 0 { format!("&guild_id={guild}") } else { String::new() };
                app.rest("GET", format!("/users/{u}/profile?with_mutual_guilds=false{q}"), None, crate::net::Tag::Profile(u));
            }
        }
    }
    app.store.user(u).and_then(|user| super::palette::avatar(ctx, &user.avatar_url())).unwrap_or(pal().bg_mod_selected)
}

/// Voice channel view: a tile per person, the stream on top when watching one, controls along the bottom.
pub fn room(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let shown = awake(ui, ui.max_rect());
    egui::Panel::top("room_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(Color32::BLACK).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            ui.multiply_opacity(shown);
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Speaker, r, pal().channel_default);
                ui.add(egui::Label::new(egui::RichText::new(&chan.name).font(semi(16.0)).color(pal().header_primary)).selectable(false));
                // discord's chat button, while the chat isn't already open beside the call
                if !app.voice_chat {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        icons::draw(ui.painter(), Icon::Bubble, r, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
                        app.rects.insert("voice:show_chat".into(), r);
                        if resp.on_hover_text("Show Chat").clicked() {
                            app.voice_chat = true;
                        }
                    });
                }
            });
        });
    egui::CentralPanel::default().frame(Frame::new().fill(Color32::BLACK)).show(ui, |ui| {
        let r = ui.max_rect();
        let mut members = app.voice.members(&app.store, chan.id);
        // in name order, like the list under the channel
        members.sort_by_cached_key(|u| app.store.name_in(chan.guild, *u).to_lowercase());
        let mut area = Rect::from_min_max(r.min + vec2(16.0, 16.0), pos2(r.right() - 16.0, r.bottom() - 80.0));
        if members.is_empty() {
            ui.painter().text(area.center() - vec2(0.0, 12.0), Align2::CENTER_CENTER, &chan.name, semi(24.0), pal().header_primary);
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
            let fill = tile_color(app, ui.ctx(), *u, chan.guild);
            ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
            app.rects.insert(format!("voice:tile:{u}"), tile);
            match app.voice.video.get(u).cloned() {
                Some(img) => frame(app, ui, format!("cam:{u}"), &img, tile),
                None => {
                    let av = if tile.width() >= 400.0 { 80.0 } else if tile.width() >= 200.0 { 64.0 } else { 40.0 };
                    let a = Rect::from_center_size(tile.center(), vec2(av, av));
                    if let Some(user) = app.store.user(*u) {
                        avatar(ui, a, &user.avatar_url().replace("size=80", "size=128"));
                    }
                }
            }
            // discord's overlay title: 12 in from the corner, a black wash, gone while idle
            if shown > 0.0 {
                let name = app.store.name_in(chan.guild, *u);
                let g = ui.painter().layout_no_wrap(name, medium(14.0), Color32::WHITE);
                let size = vec2((g.size().x + 24.0).min(tile.width() - 32.0), 30.0);
                let tag = Rect::from_min_size(pos2(tile.left() + 12.0, tile.bottom() - 12.0 - size.y), size);
                ui.painter().rect_filled(tag, CornerRadius::same(8), Color32::from_black_alpha((143.0 * shown) as u8));
                ui.painter().with_clip_rect(tag.shrink2(vec2(12.0, 0.0))).galley(pos2(tag.left() + 12.0, tag.center().y - g.size().y / 2.0), g, Color32::WHITE.gamma_multiply(shown));
            }
            if app.voice.live.contains(u) {
                live_badge(ui, tile.right_top() + vec2(-8.0, 8.0));
                watch_click(app, ui, *u, tile);
            }
            if app.voice.is_speaking(*u) {
                ui.painter().rect_stroke(tile, CornerRadius::same(8), Stroke::new(3.0, GREEN), egui::StrokeKind::Inside);
            }
        }
        if shown > 0.0 {
            gradient(ui, r, true, shown);
            gradient(ui, r, false, shown);
        }
        // the join button stays put; once in, the controls fade with the rest
        if app.voice.chan == Some(chan.id) {
            ui.multiply_opacity(shown);
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
            app.join_voice(ch, true);
        }
        return;
    }
    let mute = app.voice.mute || app.voice.deaf;
    let deaf = app.voice.deaf;
    let cam = app.voice.camera;
    let sharing = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let me = app.store.me;
    // discord's soundboard button, leftmost of the call controls
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let sb = Rect::from_center_size(c + vec2(-2.5 * 64.0, 0.0), vec2(52.0, 52.0));
    if guild != 0 {
        let open = app.soundboard.is_some();
        let r = ui.interact(sb, egui::Id::new(("ctl", "sb")), Sense::click());
        let fill = if open { Color32::WHITE } else { pal().bg_mod_selected };
        ui.painter().circle_filled(sb.center(), 26.0, if r.hovered() { fill.gamma_multiply(0.85) } else { fill });
        icons::draw(ui.painter(), Icon::Soundboard, Rect::from_center_size(sb.center(), vec2(22.0, 22.0)), if open { Color32::BLACK } else { Color32::WHITE });
        app.rects.insert("voice:soundboard".into(), sb);
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.on_hover_text("Soundboard").clicked() {
            app.soundboard = if open {
                None
            } else {
                app.rest("GET", format!("/guilds/{guild}/soundboard-sounds"), None, crate::net::Tag::Soundboard(guild));
                app.rest("GET", "/soundboard-default-sounds".into(), None, crate::net::Tag::Soundboard(0));
                Some((guild, None))
            };
        }
    }
    if app.soundboard.is_some() {
        super::soundboard::popout(app, ui, sb);
    }
    for i in 0..5 {
        let b = Rect::from_center_size(c + vec2((i as f32 - 1.5) * 64.0, 0.0), vec2(52.0, 52.0));
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
                3 => app.share = Some(super::share::Picker::new(ui.ctx())),
                _ => app.voice.leave(&app.net),
            }
        }
    }
}

type Devices = Arc<Mutex<Option<(Vec<String>, Vec<String>)>>>;

/// Input and output device names. Enumerating WASAPI endpoints takes a while, so it happens
/// once, on a thread; empty until it lands.
pub fn devices(ctx: &egui::Context) -> (Vec<String>, Vec<String>) {
    let key = egui::Id::new("voice_devices");
    let devs: Devices = match ctx.data(|d| d.get_temp::<Devices>(key)) {
        Some(d) => d,
        None => {
            let d: Devices = Arc::default();
            let (out, ctx2) = (d.clone(), ctx.clone());
            std::thread::spawn(move || {
                let list = audio::devices();
                if let Ok(mut o) = out.lock() {
                    *o = Some(list);
                }
                ctx2.request_repaint();
            });
            ctx.data_mut(|m| m.insert_temp(key, d.clone()));
            d
        }
    };
    devs.lock().ok().and_then(|d| d.clone()).unwrap_or_default()
}

/// The little chevron beside the panel's mic and headphones: a menu of devices to pick from.
fn device_menu(ui: &mut Ui, title: &str, output: bool, cur: &mut Option<String>, off: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, 32.0), Sense::click());
    let bg = match (off, resp.hovered()) {
        (true, h) => DANGER.gamma_multiply(if h { 0.2 } else { 0.12 }),
        (false, true) => pal().control_hover,
        (false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, bg);
    let col = if off { DANGER } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), col);
    let mut changed = false;
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(220.0);
        ui.label(RichText::new(title).font(medium(14.0)).color(pal().header_secondary));
        ui.add_space(4.0);
        let (ins, outs) = devices(ui.ctx());
        let names = if output { outs } else { ins };
        for name in std::iter::once(None).chain(names.iter().map(Some)) {
            let on = cur.as_ref() == name;
            let label = name.map(String::as_str).unwrap_or("Default");
            if ui.selectable_label(on, label).clicked() {
                *cur = name.cloned();
                changed = true;
            }
        }
    });
    changed
}

/// A call ringing for us: who it is, Incoming Call…, and the red and green buttons. Ctrl+Enter
/// answers and Escape declines, as discord's keybinds have it.
pub fn incoming(app: &mut App, ctx: &egui::Context) {
    let Some(ch) = app.voice.ringing else { return };
    let Some(chan) = app.store.channels.get(&ch) else { return };
    let caller = chan.recipients.first().and_then(|u| app.store.user(*u)).cloned();
    let title = channel_title(&app.store, ch);
    let (mut answer, mut decline) = (false, false);
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("incoming_call")).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(screen.center()).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(Margin::symmetric(24, 28)).show(ui, |ui| {
            ui.set_width(200.0);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
                // a ring that pulses out while it rings
                let k = (ui.input(|i| i.time) as f32 * 1.5).fract();
                ui.painter().circle_stroke(r.center(), 40.0 + 12.0 * k, Stroke::new(3.0, GREEN.gamma_multiply(1.0 - k)));
                match (&caller, chan.kind) {
                    (Some(u), 1) => avatar(ui, r, &u.avatar_url()),
                    _ => {
                        ui.painter().circle_filled(r.center(), 40.0, BLURPLE);
                        icons::draw(ui.painter(), Icon::People, r.shrink(20.0), Color32::WHITE);
                    }
                }
                ui.add_space(14.0);
                ui.label(RichText::new(&title).font(semi(20.0)).color(pal().header_primary));
                ui.add_space(4.0);
                ui.label(RichText::new("Incoming Call…").font(reg(14.0)).color(pal().header_secondary));
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    ui.spacing_mut().item_spacing.x = 32.0;
                    for (fill, tip, yes) in [(RED, "Decline", false), (GREEN, "Accept", true)] {
                        let (r, resp) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 28.0, if resp.hovered() { fill.gamma_multiply(0.85) } else { fill });
                        if yes {
                            phone(ui, r.center(), Color32::WHITE);
                        } else {
                            hangup(ui, r.center(), Color32::WHITE);
                        }
                        app.rects.insert(format!("call:{}", tip.to_lowercase()), r);
                        if resp.on_hover_text(tip).clicked() {
                            if yes {
                                answer = true;
                            } else {
                                decline = true;
                            }
                        }
                    }
                });
            });
        });
    });
    ctx.request_repaint();
    if answer {
        app.answer(ch);
    } else if decline {
        app.decline(ch);
    }
}
