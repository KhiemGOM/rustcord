use base64::Engine;
use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{Look, button, caption, divider, dropdown, h5, link, radio, section, slider, sub, toggle};
use crate::cache::{InputProfile, Noise};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::voice::probe;

const SUPPORT: &str = "https://support.discord.com/hc/en-us/articles/360045138471";
const SOUNDBOARD_HELP: &str = "https://support.discord.com/hc/en-us/articles/12612888127767";

fn flag(ui: &Ui, key: &str) -> bool {
    ui.data(|d| d.get_temp::<bool>(Id::new(("voice_open", key)))).unwrap_or(false)
}

fn set_flag(ui: &Ui, key: &str, on: bool) {
    ui.data_mut(|d| d.insert_temp(Id::new(("voice_open", key)), on));
}

fn device_row(ui: &mut Ui, icon: Icon, label: &str, on: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    crate::ui::list_row(label, resp.id);
    if resp.hovered() || resp.has_focus() || on {
        ui.painter().rect_filled(r, CornerRadius::same(4), super::faint(if on { 20 } else { 10 }));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    crate::ui::text::paint(ui.painter(), pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, medium(16.0), pal().text_normal, r.width() - 80.0);
    if on {
        let c = pos2(r.right() - 20.0, r.center().y);
        ui.painter().circle_filled(c, 10.0, BLURPLE);
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(c, vec2(14.0, 14.0)), Color32::WHITE);
    }
    resp.clicked()
}

fn pick(ui: &mut Ui, label: &str, icon: Icon, names: &[String], cur: &mut Option<String>) -> bool {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    let shown = cur.clone().unwrap_or_else(|| "Default".into());
    let mut changed = false;
    let w = ui.available_width();
    dropdown(ui, w, Some(icon), &shown, |ui| {
        for name in std::iter::once(None).chain(names.iter().map(Some)) {
            if device_row(ui, icon, name.map(String::as_str).unwrap_or("Default"), cur.as_ref() == name) {
                *cur = name.cloned();
                changed = true;
            }
        }
    });
    changed
}

fn volume(ui: &mut Ui, label: &str, v: &mut f32, max: f32) -> bool {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(12.0);
    let mut pct = *v * 100.0;
    let w = ui.available_width();
    let moved = slider(ui, w, &mut pct, 0.0, max);
    if moved {
        *v = pct.round() / 100.0;
    }
    moved
}

fn bars(ui: &mut Ui, w: f32, level: i32, live: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let n = (w / 7.5) as usize;
    let on = if live { (((level as f32 + 80.0) / 80.0).clamp(0.0, 1.0) * n as f32).round() as usize } else { 0 };
    for i in 0..n {
        let x = r.left() + i as f32 * w / n as f32;
        let col = if i < on { GREEN } else { super::TRACK };
        ui.painter().rect_filled(Rect::from_min_size(pos2(x, r.center().y - 12.0), vec2(3.0, 24.0)), CornerRadius::same(1), col);
    }
}

fn plain_link(ui: &mut Ui, text: &str, url: &str) {
    if link(ui, text).clicked() {
        let _ = open::that_detached(url);
    }
}

fn with_link(ui: &mut Ui, before: &str, text: &str, after: &str, url: &str) {
    super::lead(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let muted = |s: &str| RichText::new(s.to_string()).font(reg(14.0)).color(pal().header_secondary);
        ui.add(egui::Label::new(muted(before)).selectable(false));
        plain_link(ui, text, url);
        ui.add(egui::Label::new(muted(after)).selectable(false));
    });
    super::end(ui, 16.0);
}

fn accordion(ui: &mut Ui, key: &str, closed: &str, open: &str, subtitle: &str) -> bool {
    super::lead(ui);
    let on = flag(ui, key);
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, if on { open } else { closed }, medium(16.0), pal().header_primary);
    if !on {
        ui.painter().text(r.left_top() + vec2(0.0, 24.0), Align2::LEFT_TOP, subtitle, reg(14.0), pal().header_secondary);
    }
    let b = Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(40.0, 40.0));
    ui.painter().rect(b, CornerRadius::same(8), if resp.hovered() { super::faint(51) } else { super::faint(31) }, Stroke::new(1.0, super::faint(61)), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), if on { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(b.center(), vec2(24.0, 24.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    super::mark(ui, &format!("accordion:{key}"), b);
    super::end(ui, 24.0);
    if resp.clicked() {
        set_flag(ui, key, !on);
    }
    on != resp.clicked()
}

fn warning(ui: &mut Ui, before: &str, text: &str, after: &str, url: &str) {
    super::lead(ui);
    Frame::new()
        .fill(YELLOW.gamma_multiply(0.1))
        .stroke(Stroke::new(1.0, YELLOW))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let ink = |s: &str| RichText::new(s.to_string()).font(medium(14.0)).color(pal().text_normal);
                ui.add(egui::Label::new(ink(before)).selectable(false));
                plain_link(ui, text, url);
                ui.add(egui::Label::new(ink(after)).selectable(false));
            });
        });
    super::end(ui, 24.0);
}

fn sensitivity(ui: &mut Ui, w: f32, db: &mut f32, level: f32, live: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 16.0), Sense::click_and_drag());
    let x_of = |v: f32| r.left() + 8.0 + ((v + 100.0) / 100.0).clamp(0.0, 1.0) * (w - 16.0);
    let (knob, lit) = (x_of(*db), if live { x_of(level) } else { r.left() });
    let rail = Rect::from_center_size(r.center(), vec2(w, 4.0));
    for (from, to, col) in [(rail.left(), knob, YELLOW), (knob, rail.right(), GREEN)] {
        let dim = col.gamma_multiply(0.45);
        let cut = lit.clamp(from, to);
        ui.painter().rect_filled(Rect::from_min_max(pos2(from, rail.top()), pos2(cut, rail.bottom())), 0, col);
        ui.painter().rect_filled(Rect::from_min_max(pos2(cut, rail.top()), pos2(to, rail.bottom())), 0, dim);
    }
    ui.painter().circle(pos2(knob, r.center().y), 8.0, Color32::WHITE, Stroke::new(1.0, super::faint(61)));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let v = ((p.x - r.left() - 8.0) / (w - 16.0)).clamp(0.0, 1.0) * 100.0 - 100.0;
        if (v.round() - *db).abs() >= 1.0 {
            *db = v.round();
            return true;
        }
    }
    false
}

fn speaking_bar(ui: &mut Ui, w: f32, on: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 8.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(4), if on { GREEN } else { super::TRACK });
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    let (ins, outs) = crate::ui::call::devices(ui.ctx());
    let before = (app.settings.soundboard_volume, app.settings.stream_previews);
    let mut changed = false;
    let seen = probe::watch(&app.settings);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
    let in_call = app.voice.chan.is_some();

    section(ui, "Voice");
    super::lead(ui);
    let s = &mut app.settings;
    ui.spacing_mut().item_spacing.x = 16.0;
    ui.columns(2, |cols| {
        changed |= pick(&mut cols[0], "Microphone", Icon::Mic, &ins, &mut s.mic);
        changed |= pick(&mut cols[1], "Speaker", Icon::Headphones, &outs, &mut s.speaker);
        cols[0].add_space(24.0);
        cols[1].add_space(24.0);
        changed |= volume(&mut cols[0], "Microphone Volume", &mut s.input_volume, 100.0);
        changed |= volume(&mut cols[1], "Speaker Volume", &mut s.output_volume, 200.0);
    });
    super::end(ui, 32.0);

    super::lead(ui);
    let testing = probe::testing();
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        let look = if in_call { Look::Secondary } else { Look::Primary };
        let b = button(ui, if testing { "Stop Testing" } else { "Mic Test" }, look);
        let bw = b.rect.width();
        super::mark(ui, "mic_test", b.rect);
        if b.clicked() && !in_call {
            probe::set_testing(!testing);
        }
        if in_call {
            b.on_hover_text("Leave your call to test your mic.");
        }
        bars(ui, (w - bw - 24.0).max(40.0), seen.level, seen.has_mic);
    });
    super::end(ui, 24.0);
    with_link(ui, "Need help? Check out our ", "troubleshooting guide", ".", SUPPORT);

    divider(ui);
    caption(ui, "Input Profile", "");
    super::end(ui, 15.0);
    let mut profile = app.settings.input_profile;
    if radio(
        ui,
        &mut profile,
        &[
            (InputProfile::VoiceIsolation, "Voice Isolation", "Just your beautiful voice: let Rustcord cut through the noise"),
            (InputProfile::Studio, "Studio", "Pure audio: open mic with no processing"),
            (InputProfile::Custom, "Custom", "Advanced mode: give me all the buttons and dials!"),
        ],
    ) {
        app.settings.input_profile = profile;
        changed = true;
    }

    let custom = app.settings.input_profile == InputProfile::Custom;
    if custom {
        let ptt = app.settings.ptt;
        caption(ui, "Input Sensitivity", if app.settings.auto_sensitivity { "Controls how much sound Rustcord transmits from your mic. Speak to test your mic — if the meter is filled, Rustcord is transmitting your beautiful voice." } else { "Controls how much sound Rustcord transmits from your mic." });
        ui.add_enabled_ui(!ptt, |ui| {
            changed |= toggle(ui, &mut app.settings.auto_sensitivity, "Automatically determine input sensitivity", "");
            super::lead(ui);
            let w = ui.available_width();
            if app.settings.auto_sensitivity {
                speaking_bar(ui, w, seen.speaking && seen.has_mic);
            } else {
                let mut db = app.settings.vad_db as f32;
                if sensitivity(ui, w, &mut db, seen.level as f32, seen.has_mic) {
                    app.settings.vad_db = db as i32;
                    changed = true;
                }
            }
            super::end(ui, 24.0);
        });
        if !seen.has_mic {
            warning(ui, "You do not have any input devices enabled. You must grant Rustcord ", "access to your microphone", " in order to be able to observe input sensitivity.", "ms-settings:privacy-microphone");
        }

        super::lead(ui);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
        ui.painter().text(r.left_top(), Align2::LEFT_TOP, "Noise Suppression", medium(16.0), pal().header_primary);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_max_width(w - 260.0);
            let muted = |s: &str| RichText::new(s.to_string()).font(reg(14.0)).color(pal().header_secondary);
            ui.add(egui::Label::new(muted("Reduces background noise from your mic. Powered by ")).selectable(false));
            plain_link(ui, "Krisp", "https://krisp.ai");
            ui.add(egui::Label::new(muted(".")).selectable(false));
        });
        if app.settings.noise == Noise::Krisp {
            ui.add(egui::Label::new(RichText::new("krisp").font(medium(22.0)).color(pal().header_primary)).selectable(false));
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.right() - 240.0, r.top() - 2.0), vec2(240.0, 40.0))));
        let shown = match app.settings.noise {
            Noise::Krisp => "Krisp",
            Noise::Standard => "Standard",
            Noise::None => "None",
        };
        dropdown(&mut c, 240.0, None, shown, |ui| {
            for (n, label) in [(Noise::Krisp, "Krisp"), (Noise::Standard, "Standard"), (Noise::None, "None")] {
                if ui.selectable_label(app.settings.noise == n, label).clicked() {
                    app.settings.noise = n;
                    changed = true;
                }
            }
        });
        super::end(ui, 24.0);
        changed |= toggle(ui, &mut app.settings.echo_cancel, "Echo Cancellation", "");
    }

    if app.settings.input_profile != InputProfile::Studio {
        changed |= toggle(ui, &mut app.settings.ptt, "Push to Talk", "");
        if app.settings.ptt {
            let keys = app.settings.keybinds.iter().find(|(a, k)| a == "ptt" && !k.is_empty()).map(|(_, k)| super::system::combo(k));
            caption(ui, "Push to Talk Keybind", &keys.clone().unwrap_or_else(|| "No Keybind Set".into()));
            super::lead(ui);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add(egui::Label::new(RichText::new("You can add multiple Push-to-talk binds under ").font(reg(14.0)).color(pal().header_secondary)).selectable(false));
                if link(ui, "Keybind Settings").clicked() {
                    app.settings_page = Some(super::Page::System);
                }
            });
            super::end(ui, 24.0);
            let ms = app.settings.ptt_delay_ms;
            let shown = if ms >= 1000 { format!("{:.2}s", ms as f32 / 1000.0) } else { format!("{ms} ms") };
            caption(ui, "Push to Talk Release Delay", &shown);
            super::lead(ui);
            let mut v = ms as f32;
            let w = ui.available_width();
            if slider(ui, w, &mut v, 0.0, 2000.0) {
                app.settings.ptt_delay_ms = (v / 10.0).round() as u32 * 10;
                changed = true;
            }
            super::end(ui, 24.0);
        }
    }

    let more = accordion(ui, "advanced", "Show Advanced Voice Settings", "Hide Advanced Voice Settings", "Automatic Gain Control, Advanced Voice Activity, and Switch Voice Channel Warning");
    if more {
        let s = &mut app.settings;
        if custom {
            changed |= toggle(ui, &mut s.agc, "Automatic Gain Control", "Automatically adjust microphone volume to keep it clear and consistent.");
            ui.add_enabled_ui(s.auto_sensitivity && !s.ptt, |ui| {
                changed |= toggle(ui, &mut s.adv_vad, "Advanced Voice Activity", "Turning this off may help if your voice is not being detected by the automatic input sensitivity.");
            });
        }
        changed |= toggle(ui, &mut s.silence_warning, "No Audio Detected Warning", "Show a warning when Rustcord is not detecting audio from your mic");
        let mut ask = !s.voice_switch_ok;
        if toggle(ui, &mut ask, "Switch Voice Channel Warning", "Show a confirmation prompt before switching to a different voice channel.") {
            s.voice_switch_ok = !ask;
            changed = true;
        }
        caption(ui, "Global Attenuation", "Lower the volume of other applications by this percent when someone is speaking.\nSet to 0% to completely disable global attenuation.");
        super::lead(ui);
        ui.label(RichText::new("Attenuation Strength").font(medium(14.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        let mut pct = s.attenuation as f32;
        let w = ui.available_width();
        if slider(ui, w, &mut pct, 0.0, 100.0) {
            s.attenuation = pct.round() as u32;
            changed = true;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{}%", s.attenuation)).font(reg(14.0)).color(pal().header_secondary));
        super::end(ui, 24.0);
        changed |= toggle(ui, &mut s.atten_self, "When I speak", "");
        changed |= toggle(ui, &mut s.atten_others, "When others speak", "");
        changed |= toggle(ui, &mut s.qos, "Enable Quality of Service High Packet Priority", "Hints to your router that the packets Rustcord is transmitting are high priority.\nSome routers or internet service providers may misbehave when this is set.");
        changed |= toggle(ui, &mut s.openh264, "OpenH264 Video Codec provided by Cisco Systems, Inc.", "Turning this off will disable use of OpenH264 for video calls and media playback. This may result in some media being unplayable.");
    }

    changed |= super::camera::section(app, ui);
    section(ui, "Streaming");
    changed |= toggle(ui, &mut app.settings.stream_previews, "Show Stream Previews", "Allows others to see a preview of your stream before they join.");

    section(ui, "Sounds");
    if !app.settings.sounds {
        super::lead(ui);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add(egui::Label::new(RichText::new("Sounds are currently disabled. You can change this in ").font(reg(14.0)).color(pal().header_secondary)).selectable(false));
            if link(ui, "Notifications").clicked() {
                app.settings_page = Some(super::Page::Notifications);
            }
        });
        super::end(ui, 16.0);
    }
    changed |= sounds(app, ui);
    super::lead(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add(egui::Label::new(RichText::new("Looking for Event Sound Pack settings? You can find them in ").font(reg(16.0)).color(pal().header_secondary)).selectable(false));
        if link(ui, "Notifications").clicked() {
            app.settings_page = Some(super::Page::Notifications);
        }
        ui.add(egui::Label::new(RichText::new(".").font(reg(16.0)).color(pal().header_secondary)).selectable(false));
    });
    super::end(ui, 24.0);
    sub(ui, "Related Settings");
    related(app, ui);

    section(ui, "Soundboard");
    let mut vol = app.settings.soundboard_volume * 100.0;
    caption(ui, "Soundboard Volume", "");
    super::lead(ui);
    ui.add_space(0.0);
    let w = ui.available_width();
    if slider(ui, w, &mut vol, 0.0, 100.0) {
        app.settings.soundboard_volume = (vol.round() / 100.0).clamp(0.0, 1.0);
        changed = true;
    }
    ui.add_space(4.0);
    plain_link(ui, "Learn more about Soundboard volume.", SOUNDBOARD_HELP);
    super::end(ui, 32.0);
    changed |= entrance(app, ui);

    section(ui, "Advanced");
    if accordion(ui, "diagnostics", "Show Diagnostic Settings", "Hide Diagnostic Settings", "Settings that collect technical data for Rustcord's developers to analyze, such as debug logging and audio recording") {
        let s = &mut app.settings;
        changed |= toggle(ui, &mut s.aec_dump, "Diagnostic Audio Recording", "Diagnostic audio recording is used for analyzing audio problems. The last five minutes of voice is saved to the voice folder.");
        sub(ui, "Debug Logging Settings");
        changed |= toggle(ui, &mut s.debug_log, "Debug Logging", "Saves debug logs to the voice folder that you can look through when troubleshooting.");
        super::lead(ui);
        if super::secondary(ui, "Show Folder").clicked() {
            let _ = std::fs::create_dir_all(crate::cache::dir());
            let _ = open::that_detached(crate::cache::dir());
        }
        super::end(ui, 24.0);
    }
    super::lead(ui);
    if super::action(ui, "Reset all Voice & Video settings", "Return to the default set of Voice & Video settings.", "Reset", Look::Danger) {
        let body = "Resetting voice settings will clear all local mutes and local volumes.\nAre you sure you want to do this?";
        app.confirm = Some(crate::ui::Confirm::new("Reset Voice and Video Settings", body, "Okay", |app: &mut App| {
            app.settings.reset_voice();
            app.voice.configure(&app.settings);
            crate::cache::set_settings(&app.settings);
            push_voice(app);
        }));
    }

    sync(app, ui, before);
    changed
}

fn sounds(app: &mut App, ui: &mut Ui) -> bool {
    let more = flag(ui, "sounds");
    let shown = if more { crate::sound::VOICE_EVENTS.len() } else { 4 };
    let enabled = app.settings.sounds;
    let mut changed = false;
    let mut preview = None;
    for (key, label) in crate::sound::VOICE_EVENTS.iter().take(shown) {
        super::lead(ui);
        let on = enabled && !app.settings.muted_sounds.iter().any(|m| m == key);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        ui.painter().text(r.left_top(), Align2::LEFT_TOP, *label, medium(16.0), if enabled { pal().header_primary } else { pal().text_muted });
        let lr = Rect::from_min_size(r.left_top() + vec2(0.0, 24.0), vec2(110.0, 18.0));
        let lresp = ui.interact(lr, Id::new(("voice_preview", *key)), Sense::click());
        ui.painter().text(lr.left_top(), Align2::LEFT_TOP, "Preview Sound", reg(14.0), pal().link);
        if lresp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if lresp.clicked() {
            preview = Some(*key);
        }
        let sw = Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0));
        let resp = ui.interact(sw, Id::new(("voice_sound", *key)), Sense::click());
        ui.painter().rect_filled(sw, CornerRadius::same(12), if on { BLURPLE } else { Color32::from_black_alpha(31) });
        if !on {
            ui.painter().rect_stroke(sw, CornerRadius::same(12), Stroke::new(1.0, super::faint(51)), egui::StrokeKind::Inside);
        }
        let x = if on { sw.right() - 13.0 } else { sw.left() + 13.0 };
        ui.painter().circle_filled(pos2(x, sw.center().y), 9.0, Color32::WHITE);
        super::mark(ui, &format!("sound:{key}"), sw);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && enabled {
            app.settings.muted_sounds.retain(|m| m != key);
            if on {
                app.settings.muted_sounds.push(key.to_string());
            }
            changed = true;
        }
        super::end(ui, 12.0);
    }
    if let Some(k) = preview {
        crate::sound::play(k);
    }
    let n = crate::sound::VOICE_EVENTS.len() - 4;
    let title = |open: bool| if open { format!("Hide {n} sounds") } else { format!("Show {n} more sounds") };
    accordion(ui, "sounds", &title(false), &title(true), "Camera On, Camera Off, Voice Disconnected, and more");
    changed
}

fn related(app: &mut App, ui: &mut Ui) {
    super::lead(ui);
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(8), super::faint(10));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let sq = Rect::from_center_size(pos2(r.left() + 32.0, r.center().y), vec2(48.0, 48.0));
    ui.painter().rect_filled(sq, CornerRadius::same(8), super::faint(10));
    icons::draw(ui.painter(), Icon::Bell, Rect::from_center_size(sq.center(), vec2(24.0, 24.0)), pal().header_primary);
    ui.painter().text(pos2(r.left() + 68.0, r.center().y - 11.0), Align2::LEFT_CENTER, "Notifications", medium(16.0), pal().header_primary);
    ui.painter().text(pos2(r.left() + 68.0, r.center().y + 10.0), Align2::LEFT_CENTER, "Enable/disable sounds for new messages and incoming calls.", reg(14.0), pal().header_secondary);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(24.0, 24.0)), pal().header_secondary);
    super::mark(ui, "related:notifications", r);
    if resp.clicked() {
        app.settings_page = Some(super::Page::Notifications);
    }
    super::end(ui, 24.0);
}

fn entrance(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    super::lead(ui);
    ui.label(RichText::new("Entrance Sounds").font(medium(20.0)).color(pal().header_primary));
    ui.add_space(4.0);
    with_link(
        ui,
        "Choose a Soundboard sound to automatically play whenever you join a voice channel. Right-click on a voice channel to join without playing your entrance sound. ",
        "Learn More",
        ".",
        SOUNDBOARD_HELP,
    );
    let pick_id = Id::new("entrance_guild");
    let guild: u64 = ui.data(|d| d.get_temp(pick_id)).unwrap_or(0);
    h5(ui, "Choose a Server");
    let name = |app: &App, g: u64| if g == 0 { "All Servers".to_string() } else { app.store.guilds.get(&g).map(|x| x.name.clone()).unwrap_or_default() };
    let w = ui.available_width();
    let mut chosen = guild;
    let order: Vec<u64> = app.store.guild_order.clone();
    let names: Vec<(u64, String)> = order.iter().map(|g| (*g, name(app, *g))).collect();
    dropdown(ui, w, None, &name(app, guild), |ui| {
        if ui.selectable_label(guild == 0, "All Servers").clicked() {
            chosen = 0;
        }
        for (g, n) in &names {
            if ui.selectable_label(guild == *g, n).clicked() {
                chosen = *g;
            }
        }
    });
    if chosen != guild {
        ui.data_mut(|d| d.insert_temp(pick_id, chosen));
        super::refetch(app, "entrance_sounds");
    }
    ui.add_space(24.0);
    h5(ui, "Choose a sound");
    let path = if chosen == 0 { "/soundboard-default-sounds".to_string() } else { format!("/guilds/{chosen}/soundboard-sounds") };
    let list: Vec<crate::state::Sound> = match super::remote(app, "entrance_sounds", &path) {
        Some(Ok(v)) => v.as_array().cloned().or_else(|| v["items"].as_array().cloned()).unwrap_or_default().iter().map(crate::state::Sound::parse).collect(),
        _ => Vec::new(),
    };
    let current = app.settings.entrance.iter().find(|e| e.0 == chosen).copied();
    let label = current.map(|(_, s, _)| list.iter().find(|x| x.id == s).map(|x| x.name.clone()).unwrap_or_else(|| "Custom sound".into())).unwrap_or_else(|| "None".into());
    let mut set: Option<Option<(u64, u64)>> = None;
    let (r, resp) = ui.allocate_exact_size(vec2(w.min(400.0), 40.0), Sense::click());
    let resp = resp.on_hover_text("Change sound");
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, super::faint(61)), egui::StrokeKind::Inside);
    let shown = ui.painter().text(r.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, &label, reg(16.0), pal().text_normal);
    icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(pos2(r.right() - 22.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let sp = Rect::from_center_size(pos2(shown.right() + 18.0, r.center().y), vec2(24.0, 24.0));
    let preview = ui.interact(sp, Id::new("entrance_preview"), Sense::click());
    icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(sp.center(), vec2(18.0, 18.0)), if current.is_some() { pal().header_primary } else { pal().text_muted });
    if preview.clicked()
        && let Some((_, sound, source)) = current
    {
        app.play_sound(sound, Some(source).filter(|s| *s != 0));
    }
    super::mark(ui, "entrance:sound", r);
    egui::Popup::menu(&resp).width(r.width()).show(|ui| {
        if current.is_some() && ui.selectable_label(false, "Remove sound").clicked() {
            set = Some(None);
        }
        if list.is_empty() {
            ui.label(RichText::new("No sounds here yet.").color(pal().text_muted));
        }
        for x in &list {
            if ui.selectable_label(current.is_some_and(|c| c.1 == x.id), &x.name).clicked() {
                set = Some(Some((x.id, if chosen == 0 { 0 } else { chosen })));
            }
        }
    });
    super::end(ui, 0.0);
    if let Some(pick) = set {
        app.settings.entrance.retain(|e| e.0 != chosen);
        let (sound, source) = pick.unwrap_or((0, 0));
        if sound != 0 {
            app.settings.entrance.push((chosen, sound, source));
            app.play_sound(sound, Some(source).filter(|s| *s != 0));
        }
        push(app, crate::proto::entrance_patch(chosen, sound, source));
        changed = true;
    }
    changed
}

fn push(app: &mut App, bytes: Vec<u8>) {
    let settings = base64::engine::general_purpose::STANDARD.encode(bytes);
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(json!({ "settings": settings })), Tag::Quiet);
}

fn push_voice(app: &mut App) {
    let bytes = crate::proto::voice_patch(app.settings.soundboard_volume, app.settings.stream_previews);
    push(app, bytes);
}

fn sync(app: &mut App, ui: &Ui, before: (f32, bool)) {
    let key = Id::new("voice_proto_dirty");
    if (app.settings.soundboard_volume, app.settings.stream_previews) != before {
        ui.data_mut(|d| d.insert_temp(key, true));
    }
    if ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) && !ui.input(|i| i.pointer.any_down()) {
        ui.data_mut(|d| d.insert_temp(key, false));
        push_voice(app);
    }
}
