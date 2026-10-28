use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::sync::atomic::Ordering::Relaxed;

use super::{caption, divider, dropdown, link, note, radio, section, slider, toggle};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::Icon;

fn pick(ui: &mut Ui, label: &str, icon: Icon, names: &[String], cur: &mut Option<String>) -> bool {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    let shown = cur.clone().unwrap_or_else(|| "Default".into());
    let mut changed = false;
    let w = ui.available_width();
    dropdown(ui, w, Some(icon), &shown, |ui| {
        for name in std::iter::once(None).chain(names.iter().map(Some)) {
            if ui.selectable_label(cur.as_ref() == name, name.map(String::as_str).unwrap_or("Default")).clicked() {
                *cur = name.cloned();
                changed = true;
            }
        }
    });
    changed
}

fn volume(ui: &mut Ui, label: &str, v: &mut f32) -> bool {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(12.0);
    let mut pct = *v * 100.0;
    let w = ui.available_width();
    let moved = slider(ui, w, &mut pct, 0.0, 200.0);
    if moved {
        *v = pct.round() / 100.0;
    }
    moved
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    let (ins, outs) = crate::ui::call::devices(ui.ctx());
    let mut changed = false;
    section(ui, "Voice");
    super::lead(ui);
    let s = &mut app.settings;
    ui.spacing_mut().item_spacing.x = 16.0;
    ui.columns(2, |cols| {
        changed |= pick(&mut cols[0], "Microphone", Icon::Mic, &ins, &mut s.mic);
        changed |= pick(&mut cols[1], "Speaker", Icon::Headphones, &outs, &mut s.speaker);
        cols[0].add_space(24.0);
        cols[1].add_space(24.0);
        changed |= volume(&mut cols[0], "Microphone Volume", &mut s.input_volume);
        changed |= volume(&mut cols[1], "Speaker Volume", &mut s.output_volume);
    });
    super::end(ui, 32.0);
    note(ui, "Device changes apply the next time you join a call.");

    divider(ui);
    caption(ui, "Input Mode", "");
    super::end(ui, 15.0);
    let mut ptt = s.ptt;
    if radio(ui, &mut ptt, &[(false, "Voice Activity", ""), (true, "Push to Talk", "")]) {
        s.ptt = ptt;
        changed = true;
    }
    if s.ptt {
        let keys = s.keybinds.iter().find(|(a, k)| a == "ptt" && !k.is_empty()).map(|(_, k)| super::app::combo(k));
        caption(ui, "Shortcut", &keys.clone().unwrap_or_else(|| "No Keybind Set".into()));
        super::lead(ui);
        if link(ui, if keys.is_some() { "Edit Keybind" } else { "Add a Keybind" }).clicked() {
            app.settings_page = Some(super::Page::System);
        }
        super::end(ui, 24.0);
        let s = &mut app.settings;
        caption(ui, "Push to Talk Release Delay", "Push to talk works anywhere, even when Fastcord isn't focused.");
        super::lead(ui);
        let mut ms = s.ptt_delay_ms as f32;
        let w = ui.available_width();
        if slider(ui, w, &mut ms, 0.0, 2000.0) {
            s.ptt_delay_ms = (ms / 10.0).round() as u32 * 10;
            changed = true;
        }
        super::end(ui, 24.0);
    } else {
        caption(ui, "Input Sensitivity", "Controls how much sound Fastcord transmits from you.");
        changed |= toggle(ui, &mut s.auto_sensitivity, "Automatically determine input sensitivity", "");
        if !s.auto_sensitivity {
            super::lead(ui);
            let mut db = s.vad_db as f32;
            let w = ui.available_width();
            if slider(ui, w, &mut db, -100.0, 0.0) {
                s.vad_db = db.round() as i32;
                changed = true;
            }
            ui.add_space(8.0);
            let (bar, _) = ui.allocate_exact_size(vec2(w, 10.0), Sense::hover());
            ui.painter().rect_filled(bar, CornerRadius::same(5), super::TRACK);
            let x = |v: f32| bar.left() + bar.width() * ((v + 100.0) / 100.0).clamp(0.0, 1.0);
            if app.voice.chan.is_some() {
                let level = app.voice.shared.level.load(Relaxed) as f32;
                let fill = Rect::from_min_max(bar.min, pos2(x(level), bar.bottom()));
                ui.painter().rect_filled(fill, CornerRadius::same(5), if level > db { GREEN } else { YELLOW });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
            ui.painter().vline(x(db), bar.y_range().expand(3.0), Stroke::new(2.0, Color32::WHITE));
            ui.add_space(8.0);
            let txt = if app.voice.chan.is_some() { "You transmit while the bar is green." } else { "Join a voice channel to see your input level." };
            ui.painter().text(ui.cursor().left_top(), Align2::LEFT_TOP, txt, reg(14.0), pal().header_secondary);
            ui.add_space(18.0);
            super::end(ui, 24.0);
        }
    }

    section(ui, "Noise Suppression");
    changed |= toggle(ui, &mut app.settings.noise_suppression, "Krisp", "Removes background noise from your mic — keyboards, fans, barking dogs — before it goes out.");

    section(ui, "Camera");
    note(ui, "Fastcord uses your default camera. Turn it on from the call controls.");
    section(ui, "Streaming");
    note(ui, "Go Live from the call controls to share your screen.");
    section(ui, "Sounds");
    note(ui, "Call and message sounds are under Notifications.");
    section(ui, "Soundboard");
    note(ui, "Open the soundboard from the call controls to play a server's sounds.");
    section(ui, "Advanced");
    note(ui, "Fastcord has no advanced voice options yet.");
    changed
}
