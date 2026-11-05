//! The chevrons beside the user panel's mic and headphones: Input and Output Options.

use egui::{Color32, CornerRadius, Rect, Sense, Ui, vec2};

use super::icons::{self, Icon};
use crate::theme::*;

/// The little chevron beside the panel's mic and headphones: discord's Input and Output Options.
pub fn device_menu(ui: &mut Ui, output: bool, off: bool, s: &mut crate::cache::Settings, voice_settings: &mut bool, live: &mut bool) -> bool {
    use super::{Trail, menu_row, menu_sep};
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, 32.0), Sense::click());
    let bg = match (off, resp.hovered()) {
        (true, h) => super::call::DANGER.gamma_multiply(if h { 0.2 } else { 0.12 }),
        (false, true) => pal().control_hover,
        (false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, bg);
    let col = if off { super::call::DANGER } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), col);
    let resp = resp.on_hover_text(if output { "Output Options" } else { "Input Options" });
    let mut changed = false;
    egui::Popup::menu(&resp).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (ins, outs) = super::call::devices(ui.ctx());
        let (names, cur, what) = if output { (outs, &mut s.speaker, "Output Device") } else { (ins, &mut s.mic, "Input Device") };
        let shown = cur.clone().unwrap_or_else(|| "Default".into());
        let m = menu_row(ui, None, what, Some(&shown), false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for name in std::iter::once(None).chain(names.iter().map(Some)) {
                let on = cur.as_ref() == name;
                if menu_row(ui, None, name.map_or("Default", String::as_str), None, false, Trail::Radio(on)).clicked() {
                    *cur = name.cloned();
                    changed = true;
                }
            }
        });
        if !output {
            let profiles = ["Voice Isolation", "Studio", "Custom"];
            let m = menu_row(ui, None, "Input Profile", Some(profiles[s.input_profile.min(2) as usize]), false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, label) in profiles.iter().enumerate() {
                    if menu_row(ui, None, label, None, false, Trail::Radio(s.input_profile == i as u8)).clicked() {
                        s.input_profile = i as u8;
                        match i {
                            0 => (s.noise_suppression, s.auto_sensitivity) = (true, true),
                            1 => (s.noise_suppression, s.auto_sensitivity, s.vad_db) = (false, false, -100),
                            _ => {}
                        }
                        changed = true;
                    }
                }
            });
        }
        menu_sep(ui);
        let vol = if output { &mut s.output_volume } else { &mut s.input_volume };
        let text = format!("{}%", (*vol * 100.0).round());
        if let Some((t, end)) = super::usermenu::menu_slider(ui, if output { "Output Volume" } else { "Input Volume" }, *vol / 2.0, Some(&text)) {
            *vol = (t * 200.0).round() / 100.0;
            changed |= end;
            *live = true;
        }
        menu_sep(ui);
        if menu_row(ui, None, "Voice Settings", None, false, Trail::None).clicked() {
            *voice_settings = true;
            ui.close();
        }
    });
    changed
}
