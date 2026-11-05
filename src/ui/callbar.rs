use egui::{Align2, Color32, CornerRadius, Frame, Rect, Response, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, DANGER, Trail, menu_body, menu_item, menu_row, menu_sep};
use crate::state::Id;
use crate::theme::*;

const BAR: Color32 = Color32::from_rgb(11, 11, 13);

#[derive(Default)]
pub struct CallUi {
    pub focus: Option<Id>,
    pub full: bool,
    pub pop: bool,
    pub mic_test: bool,
    pub start: Option<super::stage::Form>,
    pub asked: bool,
    pub started: Option<Id>,
    pub audience_shut: bool,
}

fn hit(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, r: Rect) -> Response {
    let resp = ui.interact(r, egui::Id::new(id), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(19), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn unit(ui: &mut Ui, id: &'static str, left: f32, cy: f32, caret: bool) -> (Response, Option<Response>) {
    let main = hit(ui, (id, 0), Rect::from_min_size(pos2(left + 5.0, cy - 19.0), vec2(40.0, 38.0)));
    let more = caret.then(|| {
        let r = Rect::from_min_size(pos2(left + 46.0, cy - 19.0), vec2(20.0, 38.0));
        let resp = hit(ui, (id, 1), r);
        icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), Color32::WHITE);
        resp
    });
    (main, more)
}

fn glyph(ui: &Ui, r: &Response, icon: Icon, on: bool, ink: Color32) {
    if on {
        ui.painter().rect_filled(r.rect, CornerRadius::same(19), Color32::WHITE);
    }
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.rect.center(), vec2(20.0, 20.0)), if on { Color32::BLACK } else { ink });
}

fn corner(ui: &mut Ui, id: &'static str, right: f32, cy: f32, icon: Icon, tip: &str) -> Response {
    let r = Rect::from_center_size(pos2(right - 12.0, cy), vec2(24.0, 24.0));
    let resp = ui.interact(r, egui::Id::new(id), Sense::click());
    icons::draw(ui.painter(), icon, r, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

pub fn controls(app: &mut App, ui: &mut Ui, ch: Id, bar: Rect, join_text: &str) {
    let inside = app.voice.chan == Some(ch);
    let room = join_text != "Join Call";
    if !inside {
        let b = Rect::from_center_size(bar.center(), vec2(140.0, 40.0));
        let r = ui.interact(b, egui::Id::new(("join", ch)), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(20), if r.hovered() { GREEN.gamma_multiply(0.85) } else { GREEN });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, join_text, semi(14.0), Color32::WHITE);
        app.rects.insert("voice:join".into(), b);
        if r.clicked() {
            app.join_voice(ch, true);
        }
        return;
    }
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let mute = app.voice.mute || app.voice.deaf || app.voice.suppressed;
    let cam = app.voice.camera;
    let sharing = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let cy = if room { bar.bottom() - 41.0 } else { bar.center().y };
    let cells = if guild != 0 { 3 } else { 2 };
    let w2 = cells as f32 * 48.0 + 2.0;
    let x0 = bar.center().x - (140.0 + 12.0 + w2 + 12.0 + 66.0) / 2.0;
    let sec = |x: f32, w: f32| Rect::from_min_size(pos2(x, cy - 25.0), vec2(w, 50.0));
    let (s1, s2) = (sec(x0, 140.0), sec(x0 + 152.0, w2));
    let p = ui.painter().clone();
    p.rect_filled(s1, CornerRadius::same(25), BAR);
    p.rect_filled(s2, CornerRadius::same(25), BAR);

    let (m, mc) = unit(ui, "mic", s1.left(), cy, true);
    glyph(ui, &m, if mute { Icon::MicOff } else { Icon::Mic }, false, if mute { DANGER } else { Color32::WHITE });
    app.rects.insert("voice:mute_big".into(), m.rect);
    app.rects.insert("voice:mic_more".into(), mc.as_ref().map_or(Rect::NOTHING, |c| c.rect));
    if m.on_hover_text(if mute { "Unmute" } else { "Mute" }).clicked() {
        app.voice.set_mute(&app.net, !mute);
    }
    if let Some(c) = mc {
        let c = c.on_hover_text("More Microphone Options");
        audio_popup(app, &c, false);
    }

    let (v, vc) = unit(ui, "cam", s1.left() + 70.0, cy, true);
    glyph(ui, &v, if cam { Icon::Video } else { Icon::VideoSlash }, false, Color32::WHITE);
    app.rects.insert("voice:camera".into(), v.rect);
    app.rects.insert("voice:camera_more".into(), vc.as_ref().map_or(Rect::NOTHING, |c| c.rect));
    if v.on_hover_text(if cam { "Turn Off Camera" } else { "Turn On Camera" }).clicked() {
        super::settings::camera::toggle_camera(app, ui.ctx(), !cam);
    }
    if let Some(c) = vc {
        let c = c.on_hover_text("More Camera Options");
        camera_popup(app, &c);
    }

    let (sh, _) = unit(ui, "share", s2.left(), cy, false);
    glyph(ui, &sh, Icon::ScreenArrow, sharing, Color32::WHITE);
    app.rects.insert("voice:screen".into(), sh.rect);
    if sh.on_hover_text(if sharing { "Stop Streaming" } else { "Share Your Screen" }).clicked() {
        if sharing {
            app.voice.end_stream(&app.net);
        } else {
            app.share = Some(super::share::Picker::new(ui.ctx()));
        }
    }
    let mut at = s2.left() + 48.0;
    let sb = Rect::from_center_size(pos2(at + 25.0, cy), vec2(40.0, 38.0));
    if guild != 0 {
        let open = app.soundboard.is_some();
        let (b, _) = unit(ui, "sb", at, cy, false);
        glyph(ui, &b, Icon::Soundboard, open, Color32::WHITE);
        app.rects.insert("voice:soundboard".into(), b.rect);
        if b.on_hover_text("Open Soundboard").clicked() {
            app.soundboard = if open {
                None
            } else {
                app.rest("GET", format!("/guilds/{guild}/soundboard-sounds"), None, crate::net::Tag::Soundboard(guild));
                Some((guild, None))
            };
        }
        at += 48.0;
    }
    if app.soundboard.is_some() && guild != 0 {
        super::soundboard::popout(app, ui, sb);
    }
    let (mo, _) = unit(ui, "more", at, cy, false);
    glyph(ui, &mo, Icon::More, false, Color32::WHITE);
    let mo = mo.on_hover_text("More Options");
    more_popup(app, &mo);

    let end = Rect::from_min_size(pos2(s2.right() + 12.0, cy - 25.0), vec2(66.0, 50.0));
    let r = ui.interact(end, egui::Id::new(("ctl", "end")), Sense::click());
    p.rect_filled(end, CornerRadius::same(25), if r.hovered() { DANGER.gamma_multiply(0.85) } else { DANGER });
    icons::draw(&p, Icon::HangUp, Rect::from_center_size(end.center(), vec2(20.0, 20.0)), Color32::WHITE);
    app.rects.insert("voice:leave_big".into(), end);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.on_hover_text("Disconnect").clicked() {
        app.voice.leave(&app.net);
    }

    if !room {
        return;
    }
    if guild != 0 && corner(ui, "call_invite", bar.left() + 40.0, cy, Icon::PersonAdd, "Invite").clicked() {
        app.invite_people(ch);
    }
    let full = app.call.full;
    if corner(ui, "call_full", bar.right() - 16.0, cy, Icon::Fullscreen, if full { "Exit Full Screen" } else { "Full Screen" }).clicked() {
        app.call.full = !full;
    }
    app.rects.insert("voice:invite".into(), Rect::from_center_size(pos2(bar.left() + 28.0, cy), vec2(24.0, 24.0)));
    app.rects.insert("voice:fullscreen".into(), Rect::from_center_size(pos2(bar.right() - 28.0, cy), vec2(24.0, 24.0)));
    if corner(ui, "call_pop", bar.right() - 60.0, cy, Icon::PopOut, "Pop Out").clicked() {
        app.call.pop = true;
        app.call.full = false;
    }
    if full && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.call.full = false;
    }
}

pub fn popout(app: &mut App, ctx: &egui::Context) {
    if !app.call.pop {
        return;
    }
    let Some(ch) = app.voice.chan else {
        app.call.pop = false;
        return;
    };
    let Some(chan) = app.store.channels.get(&ch).cloned() else { return };
    let builder = egui::ViewportBuilder::default().with_title(format!("{} — Rustcord", super::channel_title(&app.store, ch))).with_inner_size([960.0, 600.0]).with_min_inner_size([480.0, 320.0]);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("call_popout"), builder, |ui, _| {
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            app.call.pop = false;
            return;
        }
        super::call::room(app, ui, &chan);
    });
}

pub fn popped(app: &mut App, ui: &mut Ui, chan: &crate::state::Channel) {
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0, Color32::BLACK);
    ui.painter().text(r.center() - vec2(0.0, 24.0), Align2::CENTER_CENTER, format!("{} is in its own window", chan.name), semi(20.0), pal().header_primary);
    let b = Rect::from_center_size(r.center() + vec2(0.0, 24.0), vec2(104.0, 40.0));
    let resp = ui.interact(b, egui::Id::new("pop_in"), Sense::click());
    ui.painter().rect_filled(b, CornerRadius::same(8), if resp.hovered() { Color32::from_rgb(0xe0, 0xe0, 0xe3) } else { Color32::WHITE });
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Pop In", medium(14.0), Color32::BLACK);
    app.rects.insert("voice:pop_in".into(), b);
    if resp.clicked() {
        app.call.pop = false;
    }
}

fn frame() -> Frame {
    Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(8)
}

fn audio_popup(app: &mut App, at: &Response, output: bool) {
    let mut changed = false;
    let mut voice_settings = false;
    egui::Popup::menu(at).align(egui::RectAlign::TOP).gap(8.0).frame(frame()).show(|ui| {
        (changed, voice_settings) = audio_menu(ui, &mut app.settings, output);
    });
    if changed {
        app.save_settings(&at.ctx);
    }
    if voice_settings {
        app.settings_page = Some(super::settings::Page::Voice);
    }
}

pub fn audio_menu(ui: &mut Ui, s: &mut crate::cache::Settings, output: bool) -> (bool, bool) {
    let (mut changed, mut voice) = (false, false);
    ui.set_width(222.0);
    menu_body(ui, |ui| {
        let (ins, outs) = super::call::devices(ui.ctx());
        let names = if output { outs } else { ins };
        let cur = if output { s.speaker.clone() } else { s.mic.clone() };
        let hint = cur.clone().unwrap_or_else(|| "Default".into());
        let row = menu_row(ui, None, if output { "Output Device" } else { "Input Device" }, Some(&hint), false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for name in std::iter::once(None).chain(names.iter().map(Some)) {
                let label = name.map(String::as_str).unwrap_or("Default");
                if menu_row(ui, None, label, None, false, Trail::Radio(cur.as_ref() == name)).clicked() {
                    *(if output { &mut s.speaker } else { &mut s.mic }) = name.cloned();
                    changed = true;
                }
            }
        });
        if !output {
            let at = if s.noise_suppression && s.auto_sensitivity { 0 } else if !s.noise_suppression && !s.auto_sensitivity { 1 } else { 2 };
            let names = ["Voice Isolation", "Studio", "Custom"];
            let row = menu_row(ui, None, "Input Profile", Some(names[at]), false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, n) in names.iter().enumerate() {
                    if menu_row(ui, None, n, None, false, Trail::Radio(at == i)).clicked() && at != i {
                        (s.noise_suppression, s.auto_sensitivity) = [(true, true), (false, false), (true, false)][i];
                        changed = true;
                    }
                }
            });
        }
        menu_sep(ui);
        let vol = if output { &mut s.output_volume } else { &mut s.input_volume };
        let mut pct = *vol * 100.0;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(if output { "Output Volume" } else { "Input Volume" }).font(medium(14.0)).color(pal().header_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{}%", pct.round())).font(reg(12.0)).color(pal().text_muted));
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let w = ui.available_width() - 8.0;
            if super::settings::slider(ui, w, &mut pct, 0.0, 200.0) {
                *vol = pct.round() / 100.0;
                changed = true;
            }
        });
        ui.add_space(4.0);
        menu_sep(ui);
        voice = menu_item(ui, None, "Voice Settings", false);
    });
    (changed, voice)
}

fn camera_popup(app: &mut App, at: &Response) {
    let (mut open, mut background, mut picked) = (false, false, None);
    let cams = super::settings::camera::cameras(app, &at.ctx);
    let cur = super::settings::camera::shown(app, &cams);
    egui::Popup::menu(at).align(egui::RectAlign::TOP).gap(8.0).frame(frame()).show(|ui| {
        ui.set_min_width(222.0);
        menu_body(ui, |ui| {
            let row = menu_row(ui, None, "Camera", Some(&cur), false, Trail::Caret);
            super::settings::mark(ui, "callcam:row", row.rect);
            egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for name in &cams {
                    let item = menu_row(ui, None, name, None, false, Trail::Radio(*name == cur));
                    super::settings::mark(ui, &format!("callcam:dev:{name}"), item.rect);
                    if item.clicked() {
                        picked = Some(name.clone());
                    }
                }
            });
            background = menu_item(ui, None, "Change Video Background", false);
            menu_sep(ui);
            open = menu_item(ui, None, "Video Settings", false);
        });
    });
    if let Some(name) = picked {
        super::settings::camera::choose(app, &at.ctx, &name);
        app.save_settings(&at.ctx);
    }
    if background {
        at.ctx.data_mut(|d| d.insert_temp(egui::Id::new("settings_goto"), "Video Background".to_string()));
    }
    if open || background {
        app.settings_page = Some(super::settings::Page::Voice);
    }
}

fn more_popup(app: &mut App, at: &Response) {
    let mut changed = false;
    egui::Popup::menu(at).align(egui::RectAlign::TOP).gap(8.0).frame(frame()).show(|ui| {
        ui.set_min_width(222.0);
        menu_body(ui, |ui| {
            let s = &mut app.settings;
            if menu_item(ui, None, if s.hide_call_names { "Show Names" } else { "Hide Names" }, false) {
                s.hide_call_names = !s.hide_call_names;
                changed = true;
            }
            if menu_item(ui, None, if s.hide_idle_tiles { "Show Non-Video Participants" } else { "Hide Non-Video Participants" }, false) {
                s.hide_idle_tiles = !s.hide_idle_tiles;
                changed = true;
            }
        });
    });
    if changed {
        app.save_settings(&at.ctx);
    }
}

fn switch(ui: &mut Ui, on: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(48.0, 24.0), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(12), if on { BLURPLE } else { Color32::from_rgb(0x80, 0x84, 0x8e) });
    let x = if on { r.right() - 12.0 } else { r.left() + 12.0 };
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn krisp(app: &mut App, ui: &mut Ui, center: egui::Pos2) {
    let r = Rect::from_center_size(center, vec2(32.0, 32.0));
    let resp = ui.interact(r, egui::Id::new("krisp_btn"), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    if resp.hovered() || open {
        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let on = app.settings.noise_suppression;
    icons::draw(ui.painter(), Icon::Waveform, Rect::from_center_size(center, vec2(20.0, 20.0)), if on || resp.hovered() { pal().header_primary } else { pal().header_secondary });
    app.rects.insert("voice:krisp".into(), r);
    let resp = resp.on_hover_text("Noise Suppression powered by Krisp");
    let mut changed = false;
    let frame = Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(16);
    let shown = egui::Popup::from_toggle_button_response(&resp)
        .align(egui::RectAlign::TOP)
        .gap(8.0)
        .frame(frame)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(244.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Noise Suppression").font(medium(16.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if switch(ui, app.settings.noise_suppression).clicked() {
                        app.settings.noise_suppression = !app.settings.noise_suppression;
                        changed = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Enable Krisp noise suppression! Make some noise—like clapping your hands—while speaking. Your friends will hear nothing but your beautiful voice.").font(reg(14.0)).color(pal().text_normal));
            ui.add_space(8.0);
            ui.label(RichText::new("Mic Test").font(medium(16.0)).color(pal().header_primary));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (b, t) = ui.allocate_exact_size(vec2(60.0, 32.0), Sense::click());
                ui.painter().rect_filled(b, CornerRadius::same(8), if t.hovered() { pal().control_hover } else { pal().control });
                ui.painter().text(b.center(), Align2::CENTER_CENTER, if app.call.mic_test { "Stop" } else { "Test" }, medium(14.0), pal().header_primary);
                app.rects.insert("krisp:test".into(), b);
                if t.clicked() {
                    app.call.mic_test = !app.call.mic_test;
                }
                let (m, _) = ui.allocate_exact_size(vec2(176.0, 20.0), Sense::hover());
                let level = if app.call.mic_test && app.voice.chan.is_some() { app.voice.shared.level.load(std::sync::atomic::Ordering::Relaxed) as f32 } else { -100.0 };
                let lit = ((level + 60.0) / 60.0).clamp(0.0, 1.0) * 24.0;
                for i in 0..24 {
                    let seg = Rect::from_min_size(pos2(m.left() + i as f32 * 7.4 + 1.0, m.top()), vec2(3.0, 20.0));
                    ui.painter().rect_filled(seg, CornerRadius::same(1), if (i as f32) < lit { GREEN } else { pal().bg_mod_selected });
                }
                if app.call.mic_test {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                }
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Powered by").font(medium(16.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("krisp").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let link = ui.add(egui::Label::new(RichText::new("Learn More").font(reg(14.0)).color(pal().link)).sense(Sense::click()));
                    if link.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if link.clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360040843952"));
                    }
                });
            });
        });
    if shown.is_none() {
        app.call.mic_test = false;
    }
    if changed {
        app.save_settings(&resp.ctx);
    }
}
