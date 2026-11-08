use egui::{Align2, Color32, CornerRadius, Sense, Stroke, Ui, pos2, vec2};

use super::{Look, action, caption, dropdown, note, section, toggle};
use crate::clips::{self, DEFAULT_KEYS, LENGTHS};
use crate::theme::*;
use crate::ui::App;

fn bind_index(app: &mut App) -> usize {
    match app.settings.keybinds.iter().position(|(a, _)| crate::ui::clips::is_save_clip(a)) {
        Some(i) => i,
        None => {
            app.settings.keybinds.push(("save_clip".into(), DEFAULT_KEYS.to_vec()));
            app.settings.keybinds.len() - 1
        }
    }
}

fn keybind(app: &mut App, ui: &mut Ui) -> bool {
    let i = bind_index(app);
    let mut changed = false;
    if app.sform.recording == Some(i)
        && let Some(done) = crate::hotkeys::record(&mut app.sform.rec_keys)
    {
        app.settings.keybinds[i].1 = done;
        app.sform.recording = None;
        changed = true;
    }
    let recording = app.sform.recording == Some(i);
    if recording {
        ui.ctx().request_repaint();
    }
    caption(ui, "Clipping Keybind", &format!("Set up your keybind to capture clips. The default is {}.", super::combo(&DEFAULT_KEYS)));
    let shown = if recording { super::combo(&app.sform.rec_keys) } else { super::combo(&app.settings.keybinds[i].1) };
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
    let stroke = if recording { pal().red } else { Color32::from_rgba_unmultiplied(151, 151, 159, 61) };
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, shown, medium(14.0), pal().header_primary);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 4.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    c.spacing_mut().item_spacing.x = 8.0;
    if crate::ui::icon_button(&mut c, crate::ui::icons::Icon::Refresh, 32.0, "Reset to default").clicked() {
        app.settings.keybinds[i].1 = DEFAULT_KEYS.to_vec();
        app.sform.recording = None;
        changed = true;
    }
    if crate::ui::icon_button(&mut c, crate::ui::icons::Icon::Pencil, 32.0, if recording { "Stop Recording" } else { "Edit keybind" }).clicked() {
        app.sform.rec_keys.clear();
        app.sform.recording = if recording { None } else { Some(i) };
    }
    super::end(ui, 32.0);
    changed
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    section(ui, "Clipping");
    note(ui, "Clip any fun or amazing gaming moments by saving them directly to your computer.");
    let mut on = app.settings.clips;
    if toggle(ui, &mut on, "Enable Clipping", "Enables you to capture clips while you’re streaming by using the clipping keybind.") {
        app.settings.clips = on;
        changed = true;
    }
    ui.add_enabled_ui(on, |ui| {
        changed |= keybind(app, ui);
    });

    section(ui, "Capture Settings");
    let mut secs = app.settings.clips_secs;
    ui.add_enabled_ui(on, |ui| {
        caption(ui, "Clip Length", "By default, clips are 30 seconds long.");
        let shown = LENGTHS.iter().find(|l| l.0 == secs).map_or("30 seconds", |l| l.1);
        let w = ui.available_width();
        dropdown(ui, w, None, shown, |ui| {
            for (s, label) in LENGTHS {
                if ui.selectable_label(secs == s, label).clicked() {
                    secs = s;
                }
            }
        });
        super::end(ui, 32.0);
        let mut rem = app.settings.clips_reminders;
        if toggle(ui, &mut rem, "Enable Reminders", "After a clip is captured, show a reminder for new clips") {
            app.settings.clips_reminders = rem;
            changed = true;
        }
    });
    if secs != app.settings.clips_secs {
        app.settings.clips_secs = secs;
        changed = true;
    }
    note(ui, "Please note that clips are currently captured at the same quality as your stream when streaming.");

    section(ui, "Storage");
    let dir = clips::folder(&app.settings.clips_dir);
    ui.add_enabled_ui(on, |ui| {
        caption(ui, "Storage Location", "Choose where to store your clips.");
        ui.horizontal(|ui| {
            let w = ui.available_width() - 140.0;
            let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
            ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)), egui::StrokeKind::Inside);
            crate::ui::text::paint(ui.painter(), pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &dir.display().to_string(), medium(14.0), pal().text_normal, w - 24.0);
            if super::secondary(ui, "Edit Location").clicked()
                && let Some(p) = rfd::FileDialog::new().set_directory(&dir).pick_folder()
            {
                app.settings.clips_dir = p.display().to_string();
                app.clips.list = None;
                changed = true;
            }
        });
        super::end(ui, 32.0);
        if action(ui, "Clips", "Everything you've captured, to edit, share or delete.", "View Clips", Look::Secondary) {
            app.open_clips();
        }
    });
    changed
}
