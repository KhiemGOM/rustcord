use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::app::SOUNDS;
use super::{Look, action, button, divider, dropdown, note, primary, section, sub, toggle};
use crate::theme::*;
use crate::ui::App;

pub fn notifications(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Overview");
    let s = &mut app.settings;
    let mut changed = toggle(ui, &mut s.notify, "Enable Desktop Notifications", "If you're looking for per-channel or per-server notifications, right-click the desired server icon and select Mute.");

    section(ui, "Sounds");
    let mut on = s.sounds;
    if toggle(ui, &mut on, "Enable", "") {
        s.sounds = on;
        changed = true;
    }
    let mut preview = None;
    for (key, label) in SOUNDS {
        super::lead(ui);
        let mut on = !s.muted_sounds.iter().any(|m| m == key);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, medium(16.0), if s.sounds { pal().header_primary } else { pal().text_muted });
        let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        if super::switch(&mut c, on).clicked() && s.sounds {
            on = !on;
            s.muted_sounds.retain(|m| m != key);
            if !on {
                s.muted_sounds.push(key.to_string());
            }
            changed = true;
        }
        c.add_space(16.0);
        if crate::ui::icon_button(&mut c, crate::ui::icons::Icon::Play, 24.0, "Preview").clicked() {
            preview = Some(key);
        }
        super::end(ui, 32.0);
    }
    if let Some(k) = preview {
        crate::sound::play(k);
    }

    section(ui, "Badges");
    changed |= toggle(ui, &mut s.badge, "Enable Unread Message Badge", "Shows a red badge on the app icon when you have unread messages.");
    changed |= toggle(ui, &mut s.flash, "Enable Taskbar Flashing", "Flashes the taskbar button when you get a new notification.");

    section(ui, "Email");
    if action(ui, "Email Notifications", "Email preferences live with your account on discord.com.", "Manage", Look::Secondary) {
        super::open_web("/settings/notifications");
    }

    section(ui, "Advanced");
    changed |= toggle(ui, &mut s.notify_all_dms, "Notify me about every direct message", "Otherwise only direct messages that mention you notify.");
    changed
}

pub const ACTIONS: [(&str, &str); 4] = [("ptt", "Push to Talk"), ("mute", "Toggle Mute"), ("deafen", "Toggle Deafen"), ("streamer", "Toggle Streamer Mode")];

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

const LOCALES: [(&str, &str, &str); 32] = [
    ("id", "Bahasa Indonesia", "Indonesian"),
    ("da", "Dansk", "Danish"),
    ("de", "Deutsch", "German"),
    ("en-GB", "English, UK", "English, UK"),
    ("en-US", "English, US", "English, US"),
    ("es-ES", "Español", "Spanish"),
    ("es-419", "Español, LATAM", "Spanish, LATAM"),
    ("fr", "Français", "French"),
    ("hr", "Hrvatski", "Croatian"),
    ("it", "Italiano", "Italian"),
    ("lt", "Lietuviškai", "Lithuanian"),
    ("hu", "Magyar", "Hungarian"),
    ("nl", "Nederlands", "Dutch"),
    ("no", "Norsk", "Norwegian"),
    ("pl", "Polski", "Polish"),
    ("pt-BR", "Português do Brasil", "Portuguese, Brazilian"),
    ("ro", "Română", "Romanian"),
    ("fi", "Suomi", "Finnish"),
    ("sv-SE", "Svenska", "Swedish"),
    ("vi", "Tiếng Việt", "Vietnamese"),
    ("tr", "Türkçe", "Turkish"),
    ("cs", "Čeština", "Czech"),
    ("el", "Ελληνικά", "Greek"),
    ("bg", "български", "Bulgarian"),
    ("ru", "Pусский", "Russian"),
    ("uk", "Українська", "Ukrainian"),
    ("hi", "हिन्दी", "Hindi"),
    ("th", "ไทย", "Thai"),
    ("zh-CN", "中文", "Chinese, China"),
    ("ja", "日本語", "Japanese"),
    ("zh-TW", "繁體中文", "Chinese, Taiwan"),
    ("ko", "한국어", "Korean"),
];

pub fn system(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "General");
    let mut changed = false;
    let s = &mut app.settings;
    if toggle(ui, &mut s.open_on_startup, "Automatically open Rustcord when your computer starts up", "") {
        crate::winsys::set_startup(s.open_on_startup);
        changed = true;
    }
    if s.open_on_startup {
        changed |= toggle(ui, &mut s.start_minimized, "Start Rustcord minimized", "Start Rustcord in the background, out of your way.");
    }
    changed |= toggle(ui, &mut s.minimize_to_tray, "Minimize Rustcord to System Tray", "Clicking X minimizes Rustcord instead of closing it completely.");

    section(ui, "Keybinds");
    note(ui, "Global keybinds work even when Rustcord isn't focused.");
    super::lead(ui);
    if primary(ui, "Add a Keybind").clicked() {
        app.settings.keybinds.push(("ptt".into(), Vec::new()));
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
    let mut remove = None;
    for i in 0..app.settings.keybinds.len() {
        let recording = app.sform.recording == Some(i);
        super::card(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("Action").font(medium(14.0)).color(pal().header_secondary));
                cols[0].add_space(8.0);
                let (action, keys) = &mut app.settings.keybinds[i];
                let shown = ACTIONS.iter().find(|a| a.0 == action).map(|a| a.1).unwrap_or("Push to Talk");
                let w = cols[0].available_width();
                dropdown(&mut cols[0], w, None, shown, |ui| {
                    for (k, l) in ACTIONS {
                        if crate::ui::option(ui, action == k, l).clicked() {
                            *action = k.to_string();
                            changed = true;
                        }
                    }
                });
                cols[1].label(RichText::new("Keybind").font(medium(14.0)).color(pal().header_secondary));
                cols[1].add_space(8.0);
                let shown = if recording { combo(&app.sform.rec_keys) } else { combo(keys) };
                let label = if recording { "Stop Recording" } else { "Record Keybind" };
                let bw = cols[1].painter().layout_no_wrap(label.into(), medium(16.0), Color32::WHITE).size().x + 32.0;
                let (r, _) = cols[1].allocate_exact_size(vec2(cols[1].available_width(), 40.0), Sense::hover());
                let field = Rect::from_min_max(r.min, pos2(r.right() - bw - 8.0, r.bottom()));
                let stroke = if recording { RED } else { Color32::from_rgba_unmultiplied(151, 151, 159, 61) };
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
            ui.add_space(8.0);
            if super::link(ui, "Remove Keybind").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        app.settings.keybinds.remove(i);
        app.sform.recording = None;
        changed = true;
    }
    divider(ui);
    sub(ui, "Default keybinds");
    for (what, keys) in [
        ("Quick Switcher", "CTRL + K"),
        ("Navigate between channels", "ALT + UP / DOWN"),
        ("Navigate between unread channels", "ALT + SHIFT + UP / DOWN"),
        ("Mark channel as read", "ESC"),
        ("Edit your last message", "UP"),
        ("Insert a new line", "SHIFT + ENTER"),
        ("Open emoji picker", "CTRL + E"),
        ("Upload a file", "CTRL + SHIFT + U"),
        ("Mark message unread", "ALT + CLICK"),
        ("Delete without confirmation", "SHIFT + CLICK"),
        ("Zoom in / out / reset", "CTRL + = / - / 0"),
    ] {
        super::lead(ui);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, what, reg(16.0), pal().text_normal);
        let g = ui.painter().layout_no_wrap(keys.into(), medium(12.0), pal().header_secondary);
        let chip = Rect::from_min_size(pos2(r.right() - g.size().x - 16.0, r.top()), vec2(g.size().x + 16.0, 28.0));
        ui.painter().rect(chip, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)), egui::StrokeKind::Inside);
        ui.painter().galley(chip.min + vec2(8.0, 6.0), g, pal().header_secondary);
        super::end(ui, 12.0);
    }
    if changed {
        crate::hotkeys::set(&app.settings.keybinds);
    }
    changed
}

pub fn language(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Language & Time");
    note(ui, "Rustcord's own interface is English for now. This tells Discord which language to use for text it sends, like system messages and bot commands.");
    let mut changed = false;
    for (code, native, english) in LOCALES {
        super::lead(ui);
        let on = app.settings.locale == code;
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
        super::mark(ui, &format!("lang:{code}"), r);
        let c = pos2(r.left() + 11.0, r.center().y);
        if on {
            ui.painter().circle_filled(c, 11.0, BLURPLE);
            ui.painter().circle_filled(c, 4.5, Color32::WHITE);
        } else {
            ui.painter().circle_stroke(c, 10.0, Stroke::new(2.0, pal().header_secondary));
        }
        crate::ui::text::paint(ui.painter(), pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, native, reg(16.0), pal().text_normal, 300.0);
        ui.painter().text(pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, english, reg(14.0), pal().header_secondary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        super::end(ui, 12.0);
        if resp.clicked() && !on {
            app.settings.locale = code.to_string();
            changed = true;
        }
    }
    super::end(ui, 24.0);
    sub(ui, "Time Format");
    changed |= toggle(ui, &mut app.settings.clock24, "Use 24-hour clock", "Show timestamps like 16:20 instead of 4:20 PM.");
    changed
}

pub fn developer(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Developer Mode");
    let mut changed = toggle(
        ui,
        &mut app.settings.dev,
        "Developer Mode",
        "Developer Mode exposes context menu items helpful for people writing bots using the Discord API.",
    );
    section(ui, "Cache");
    if action(
        ui,
        "Image Cache",
        "Avatars, emoji and images are cached on disk so they appear instantly. Clearing it frees space; everything downloads again as needed.",
        "Clear",
        Look::Danger,
    ) {
        let _ = std::fs::remove_dir_all(crate::cache::dir().join("img"));
        app.toast("Image cache cleared.");
        changed = true;
    }
    changed
}
