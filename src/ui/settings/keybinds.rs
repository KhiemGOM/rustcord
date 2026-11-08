use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::{Look, button, dropdown, section};
use crate::theme::*;
use crate::ui::App;
use crate::ui::clips::is_save_clip;
use crate::ui::icons::Icon;
use crate::ui::keys::{SHEET, keycap};

pub const ACTIONS: [(&str, &str, &str); 18] = [
    ("ptt", "Push to Talk (Normal)", "Hold to temporarily enable your microphone while in Push to Talk mode."),
    ("ptt_priority", "Push to Talk (Priority)", "Hold to temporarily enable your microphone while in Push to Talk mode. Other speakers who are not also transmitting with priority will temporarily have their volume lowered while you are speaking."),
    ("push_to_mute", "Push to Mute", "Hold to temporarily disable your microphone while in Voice Activity mode."),
    ("vad_priority", "Voice Activity Priority", "Hold to transmit your voice with priority in Voice Activity mode. Other speakers who are not also transmitting with priority will temporarily have their volume lowered while you are speaking."),
    ("mute", "Toggle Mute", "Toggle your microphone’s transmission on and off."),
    ("deafen", "Toggle Deafen", "Toggle your speaker’s playback on and off. Also disables your microphone while deafened."),
    ("voice_mode", "Toggle VAD", "Toggle to switch between Push To Talk and Voice Activity mode."),
    ("streamer", "Toggle Streamer Mode", "Toggle to enable/disable Streamer Mode."),
    ("voice_chat", "Toggle Voice Channel Chat", "Toggle the text chat panel for the voice channel you are connected to."),
    ("camera", "Toggle Camera", "Toggle your camera on while in a voice channel."),
    ("screenshare", "Toggle Screen Share", "Toggle to start/stop streaming in your current voice channel."),
    ("back", "Navigate Back", "Navigate backward in page history"),
    ("forward", "Navigate Forward", "Navigate forward in page history"),
    ("goto_voice", "Switch To Voice Channel", "Open the voice channel you are connected to."),
    ("disconnect", "Disconnect From Voice Channel", "Leave the voice channel you are connected to."),
    ("soundboard", "Open Soundboard", "Open and close the soundboard in your current server call."),
    ("soundboard_hold", "Open Soundboard (Hold)", "Keep the soundboard open while the keys are held."),
    ("save_clip", "Save Clip", "Clips the last 30 seconds to 2 minutes of your stream, depending on your settings."),
];

pub fn key_name(vk: u32) -> String {
    match vk {
        0x08 => "BACKSPACE".into(),
        0x09 => "TAB".into(),
        0x0D => "ENTER".into(),
        0x10 | 0xA0 | 0xA1 => "SHIFT".into(),
        0x11 | 0xA2 | 0xA3 => "CTRL".into(),
        0x12 | 0xA4 | 0xA5 => "ALT".into(),
        0x13 => "PAUSE".into(),
        0x14 => "CAPS LOCK".into(),
        0x1B => "ESC".into(),
        0x20 => "SPACE".into(),
        0x21 => "PAGE UP".into(),
        0x22 => "PAGE DOWN".into(),
        0x23 => "END".into(),
        0x24 => "HOME".into(),
        0x25 => "LEFT".into(),
        0x26 => "UP".into(),
        0x27 => "RIGHT".into(),
        0x28 => "DOWN".into(),
        0x2D => "INSERT".into(),
        0x2E => "DELETE".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x5B | 0x5C => "WIN".into(),
        0x60..=0x69 => format!("NUMPAD {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x05 => "MOUSE 4".into(),
        0x06 => "MOUSE 5".into(),
        0x04 => "MIDDLE MOUSE".into(),
        0xC0 => "`".into(),
        _ => format!("KEY {vk}"),
    }
}

pub fn combo(keys: &[u32]) -> String {
    if keys.is_empty() {
        return "No Keybind Set".into();
    }
    keys.iter().map(|k| key_name(*k)).collect::<Vec<_>>().join(" + ")
}

#[cfg(test)]
#[test]
fn save_clip_is_listed_once() {
    assert_eq!(ACTIONS.iter().filter(|a| a.0 == "save_clip").count(), 1);
}

fn banner(ui: &mut Ui, text: &str) {
    super::lead(ui);
    let g = ui.painter().layout(text.into(), medium(14.0), pal().text_normal, ui.available_width() - 64.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 24.0), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, pal().link), egui::StrokeKind::Inside);
    let c = pos2(r.left() + 24.0, r.center().y);
    ui.painter().circle_filled(c, 8.0, pal().link);
    ui.painter().text(c, Align2::CENTER_CENTER, "i", bold(12.0), Color32::WHITE);
    ui.painter().galley(pos2(r.left() + 48.0, r.top() + 12.0), g, pal().text_normal);
    super::end(ui, 24.0);
}

pub fn custom(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Custom Keybinds");
    super::note(ui, "Keybinds are custom keyboard shortcuts that let you perform specific actions in Rustcord without needing to click through menus.");
    banner(ui, "Keybinds are disabled while this page is visible.");
    let mut changed = false;
    super::lead(ui);
    let add = button(ui, "Add a Keybind", Look::Secondary);
    super::mark(ui, "keybind:add", add.rect);
    if add.clicked() {
        app.settings.keybinds.push((String::new(), Vec::new()));
        app.sform.recording = Some(app.settings.keybinds.len() - 1);
        app.sform.rec_keys.clear();
        changed = true;
    }
    super::end(ui, 24.0);
    if let Some(i) = app.sform.recording
        && let Some(done) = crate::hotkeys::record(&mut app.sform.rec_keys)
    {
        if let Some(k) = app.settings.keybinds.get_mut(i) {
            k.1 = done;
        }
        app.sform.recording = None;
        changed = true;
    }
    if app.sform.recording.is_some() {
        ui.ctx().request_repaint();
    }
    let clips_on = app.settings.clips;
    let mut rows: Vec<usize> = (0..app.settings.keybinds.len()).filter(|i| clips_on || !is_save_clip(&app.settings.keybinds[*i].0)).collect();
    rows.sort_by_key(|i| !is_save_clip(&app.settings.keybinds[*i].0));
    if rows.is_empty() {
        super::note(ui, "You haven’t added any custom keybinds yet.");
    }
    let mut remove = None;
    for i in rows {
        let recording = app.sform.recording == Some(i);
        let managed = is_save_clip(&app.settings.keybinds[i].0);
        super::card(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("Action").font(medium(14.0)).color(pal().header_secondary));
                cols[0].add_space(8.0);
                let (action, keys) = &mut app.settings.keybinds[i];
                let off = action.starts_with('!');
                let id = action.trim_start_matches('!').to_string();
                let shown = ACTIONS.iter().find(|a| a.0 == id).map_or("Unassigned", |a| a.1);
                let w = cols[0].available_width();
                cols[0].add_enabled_ui(!managed, |ui| {
                    dropdown(ui, w, None, shown, |ui| {
                        for (k, l) in std::iter::once(("", "Unassigned")).chain(ACTIONS.iter().filter(|a| a.0 != "save_clip").map(|a| (a.0, a.1))) {
                            if crate::ui::option(ui, id == k, l).clicked() {
                                *action = format!("{}{k}", if off { "!" } else { "" });
                                changed = true;
                            }
                        }
                    });
                });
                let about = ACTIONS.iter().find(|a| a.0 == id).map_or("This action doesn’t do anything! Please choose one from the drop down.", |a| a.2);
                cols[0].add_space(8.0);
                cols[0].add(egui::Label::new(RichText::new(about).font(reg(12.0)).color(pal().text_muted)));
                cols[1].label(RichText::new("Keybind").font(medium(14.0)).color(pal().header_secondary));
                cols[1].add_space(8.0);
                let shown = if recording { combo(&app.sform.rec_keys) } else { combo(keys) };
                let label = if recording {
                    "Stop Recording"
                } else if keys.is_empty() {
                    "Record Keybind"
                } else {
                    "Edit Keybind"
                };
                let bw = cols[1].painter().layout_no_wrap(label.into(), medium(16.0), Color32::WHITE).size().x + 32.0;
                let (r, _) = cols[1].allocate_exact_size(vec2(cols[1].available_width(), 40.0), Sense::hover());
                let field = Rect::from_min_max(r.min, pos2(r.right() - bw - 8.0, r.bottom()));
                let stroke = if recording { pal().red } else { Color32::from_rgba_unmultiplied(151, 151, 159, 61) };
                cols[1].painter().rect(field, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                cols[1].painter().text(pos2(field.left() + 12.0, field.center().y), Align2::LEFT_CENTER, shown, medium(14.0), pal().header_primary);
                let mut b = cols[1].new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
                if button(&mut b, label, if recording { Look::Filled } else { Look::Secondary }).clicked() {
                    if recording {
                        app.sform.recording = None;
                    } else {
                        app.sform.recording = Some(i);
                        app.sform.rec_keys.clear();
                    }
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let (action, _) = &mut app.settings.keybinds[i];
                let mut on = !action.starts_with('!');
                if super::switch(ui, on).on_hover_text(if on { "Disable keybind" } else { "Enable keybind" }).clicked() {
                    on = !on;
                    *action = format!("{}{}", if on { "" } else { "!" }, action.trim_start_matches('!'));
                    changed = true;
                }
                ui.add_space(8.0);
                if !managed && crate::ui::icon_button(ui, Icon::Trash, 24.0, "Remove Keybind").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        app.settings.keybinds.remove(i);
        app.sform.recording = None;
        changed = true;
    }
    if changed {
        app.sync_hotkeys();
    }
    changed
}

fn group(ui: &mut Ui, rows: &[(&str, &[&[&str]])]) {
    super::lead(ui);
    const ROW: f32 = 48.0;
    let (g, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW * rows.len() as f32), Sense::hover());
    ui.painter().rect(g, CornerRadius::same(8), pal().bg_tertiary, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 40)), egui::StrokeKind::Inside);
    for (i, (label, combos)) in rows.iter().enumerate() {
        let r = Rect::from_min_size(g.min + vec2(0.0, i as f32 * ROW), vec2(g.width(), ROW));
        if i > 0 {
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 40)));
        }
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, label, reg(16.0), pal().text_normal);
        let mut c = ui.new_child(UiBuilder::new().max_rect(r.shrink2(vec2(16.0, 0.0))).layout(Layout::right_to_left(Align::Center)));
        c.spacing_mut().item_spacing.x = 4.0;
        for (n, keys) in combos.iter().rev().enumerate() {
            if n > 0 {
                c.add_space(12.0);
            }
            for k in keys.iter().rev() {
                keycap(&mut c, k);
            }
        }
    }
    super::end(ui, 32.0);
}

const DRAG: [(&str, &[&[&str]]); 4] = [("Start Drag and Drop", &[&["ctrl", "d"]]), ("Move", &[&["↑"], &["↓"]]), ("Drop item", &[&["spacebar"], &["enter"]]), ("Cancel", &[&["esc"]])];

pub fn defaults(ui: &mut Ui) {
    section(ui, "Default Keybinds");
    group(ui, &[("Show Keyboard Shortcuts List", &[&["ctrl", "/"]])]);
    let order = ["Messages", "Navigation", "Drag and Drop", "Chat", "Voice and Video", "Miscellaneous"];
    for name in order {
        if name == "Drag and Drop" {
            super::sub(ui, name);
            super::note(ui, "These shortcuts work when focused on a draggable item");
            group(ui, &DRAG);
            continue;
        }
        let Some((title, sub, rows)) = SHEET.iter().find(|s| s.0 == name) else { continue };
        super::sub(ui, title);
        if let Some(s) = sub {
            super::note(ui, s);
        }
        let rows: Vec<_> = rows.iter().filter(|r| r.0 != "Raging Demon").copied().collect();
        group(ui, &rows);
    }
}
