use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{card, danger, divider, h5, markers, note, primary, radio, secondary, title, toggle};
use crate::cache::{RoleColors, Spoilers};
use crate::theme::*;
use crate::ui::App;

pub fn appearance(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Appearance");
    h5(ui, "Theme");
    let themes = [
        (Theme::Light, "Light", palette(Theme::Light).bg_primary, palette(Theme::Light).bg_tertiary),
        (Theme::Ash, "Ash", palette(Theme::Ash).bg_primary, palette(Theme::Ash).bg_tertiary),
        (Theme::Dark, "Dark", palette(Theme::Dark).bg_primary, palette(Theme::Dark).bg_tertiary),
        (Theme::Onyx, "Onyx", palette(Theme::Onyx).bg_primary, palette(Theme::Onyx).bg_secondary),
        (Theme::Sync, "Sync with computer", Color32::from_rgb(0xff, 0xff, 0xff), Color32::from_rgb(0x1e, 0x1f, 0x22)),
    ];
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 8.0);
        for (t, label, a, b) in themes {
            let (r, resp) = ui.allocate_exact_size(vec2(72.0, 96.0), Sense::click());
            let c = pos2(r.center().x, r.top() + 32.0);
            let sel = app.settings.theme == t;
            if t == Theme::Sync {
                ui.painter().circle_filled(c, 30.0, b);
                // left half light, right half dark, like the official swatch
                let half: Vec<egui::Pos2> = (0..=16).map(|i| {
                    let ang = std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 16.0;
                    c + vec2(30.0 * ang.cos(), 30.0 * ang.sin())
                }).collect();
                ui.painter().add(egui::Shape::convex_polygon(half, a, Stroke::NONE));
            } else {
                ui.painter().circle_filled(c, 30.0, a);
                ui.painter().circle_filled(c + vec2(10.0, 10.0), 12.0, b);
            }
            let ring = if sel { BLURPLE } else if resp.hovered() { pal().bg_accent } else { pal().bg_mod_hover };
            ui.painter().circle_stroke(c, 31.0, Stroke::new(if sel { 3.0 } else { 1.0 }, ring));
            if sel {
                let tick = c + vec2(22.0, -22.0);
                ui.painter().circle_filled(tick, 9.0, BLURPLE);
                crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Check, Rect::from_center_size(tick, vec2(12.0, 12.0)), Color32::WHITE);
            }
            let galley = ui.painter().layout(label.to_string(), reg(12.0), pal().header_secondary, 72.0);
            ui.painter().galley(pos2(r.center().x - galley.size().x / 2.0, r.top() + 68.0), galley, pal().header_secondary);
            app.rects.insert(format!("theme:{label}"), r);
            if resp.clicked() && !sel {
                app.settings.theme = t;
                set_theme(ui.ctx(), t);
                changed = true;
            }
        }
    });
    divider(ui);
    h5(ui, "Message display");
    let mut compact = app.settings.compact;
    if radio(ui, &mut compact, &[(false, "Cozy", "Modern, beautiful, and easy on your eyes."), (true, "Compact", "Fit more messages on screen at one time. #IRC")]) {
        app.settings.compact = compact;
        app.heights.clear();
        changed = true;
    }
    if app.settings.compact {
        changed |= toggle(ui, &mut app.settings.compact_avatars, "Show avatars in Compact mode", "");
    }
    divider(ui);
    h5(ui, "Chat font scaling");
    if markers(ui, &mut app.settings.font_size, &[12.0, 14.0, 15.0, 16.0, 18.0, 20.0, 24.0], |v| format!("{v}px"), 16.0) {
        app.heights.clear();
        changed = true;
    }
    h5(ui, "Space between message groups");
    if markers(ui, &mut app.settings.group_space, &[0.0, 4.0, 8.0, 16.0, 24.0], |v| format!("{v}px"), 16.0) {
        app.heights.clear();
        changed = true;
    }
    h5(ui, "Zoom level");
    let mut pct = (app.settings.zoom * 100.0).round();
    if markers(ui, &mut pct, &[50.0, 67.0, 75.0, 80.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0], |v| format!("{v}"), 100.0) {
        app.settings.zoom = pct / 100.0;
        changed = true;
    }
    note(ui, "You can change the zoom level with Ctrl +/- and reset to the default with Ctrl+0.");
    divider(ui);
    changed |= toggle(ui, &mut app.settings.clock24, "Use 24-hour clock", "Show timestamps like 16:20 instead of 4:20 PM.");
    changed
}

pub fn accessibility(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Accessibility");
    let s = &mut app.settings;
    h5(ui, "Role colors");
    let mut changed = radio(
        ui,
        &mut s.role_colors,
        &[
            (RoleColors::InNames, "Show role colors in names", ""),
            (RoleColors::NextToNames, "Show role colors next to names", ""),
            (RoleColors::Hidden, "Don't show role colors", ""),
        ],
    );
    divider(ui);
    h5(ui, "Link decoration");
    changed |= toggle(ui, &mut s.underline_links, "Always underline links", "Make links stand out more by underlining them.");
    h5(ui, "Reduced motion");
    changed |= toggle(
        ui,
        &mut s.reduced_motion,
        "Enable Reduced Motion",
        "Reduces the amount and intensity of animations, hover effects and other moving effects across Fastcord.",
    );
    changed |= toggle(ui, &mut s.gif_autoplay, "Automatically play GIFs when Fastcord is focused", "");
    changed |= toggle(ui, &mut s.animate_emoji, "Play animated emoji", "");
    h5(ui, "Chat input");
    changed |= toggle(ui, &mut s.send_button, "Show send message button", "");
    if changed {
        app.heights.clear();
    }
    changed
}

pub fn chat(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Chat");
    let s = &mut app.settings;
    h5(ui, "Display images, videos, and lolcats");
    let mut changed = toggle(ui, &mut s.inline_links, "When posted as links to chat", "");
    changed |= toggle(ui, &mut s.inline_uploads, "When uploaded directly to Discord", "Uploads larger than 10MB will not be previewed.");
    h5(ui, "Embeds and link previews");
    changed |= toggle(ui, &mut s.embeds, "Show embeds and preview website links pasted into chat", "");
    h5(ui, "Emoji");
    changed |= toggle(ui, &mut s.reactions, "Show emoji reactions on messages", "");
    changed |= toggle(
        ui,
        &mut s.emoticons,
        "Automatically convert emoticons in your messages into emoji",
        "For example, when you type :-) Fastcord will convert it to 🙂",
    );
    h5(ui, "Spoiler content");
    note(ui, "This controls when spoiler content is displayed.");
    ui.add_space(4.0);
    changed |= radio(
        ui,
        &mut s.spoilers,
        &[(Spoilers::OnClick, "On click", ""), (Spoilers::Moderated, "On servers I moderate", ""), (Spoilers::Always, "Always", "")],
    );
    if changed {
        app.heights.clear();
    }
    changed
}

pub const SOUNDS: [(&str, &str); 9] = [
    ("message", "Message"),
    ("deafen", "Deafen"),
    ("undeafen", "Undeafen"),
    ("mute", "Mute"),
    ("unmute", "Unmute"),
    ("disconnect", "Voice Disconnected"),
    ("join", "User Join"),
    ("leave", "User Leave"),
    ("ring", "Incoming Ring"),
];

pub fn notifications(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Notifications");
    let s = &mut app.settings;
    let mut changed = toggle(ui, &mut s.notify, "Enable Desktop Notifications", "If you're looking for per-channel or per-server notifications, right-click the desired server icon and select Mute.");
    changed |= toggle(ui, &mut s.notify_all_dms, "Notify me about every direct message", "Otherwise only direct messages that mention you notify.");
    changed |= toggle(ui, &mut s.badge, "Enable Unread Message Badge", "Shows the number of unread mentions in the window title.");
    changed |= toggle(ui, &mut s.flash, "Enable Taskbar Flashing", "Flashes the taskbar button when you get a new notification.");
    h5(ui, "Sounds");
    let mut all_off = !s.sounds;
    if toggle(ui, &mut all_off, "Disable All Notification Sounds", "") {
        s.sounds = !all_off;
        changed = true;
    }
    let mut preview = None;
    for (key, label) in SOUNDS {
        let mut on = !s.muted_sounds.iter().any(|m| m == key);
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).font(semi(16.0)).color(if s.sounds { pal().header_primary } else { pal().text_muted }));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if super::switch(ui, on).clicked() && s.sounds {
                    on = !on;
                    s.muted_sounds.retain(|m| m != key);
                    if !on {
                        s.muted_sounds.push(key.to_string());
                    }
                    changed = true;
                }
                ui.add_space(8.0);
                let r = crate::ui::icon_button(ui, crate::ui::icons::Icon::Play, 24.0, "Preview");
                if r.clicked() {
                    preview = Some(key);
                }
            });
        });
        divider(ui);
    }
    if let Some(k) = preview {
        crate::sound::play(k);
    }
    h5(ui, "Email notifications");
    note(ui, "Email preferences live with your account on discord.com.");
    ui.add_space(4.0);
    if secondary(ui, "Manage Email Notifications").clicked() {
        super::open_web("/settings/notifications");
    }
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

pub fn keybinds(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Keybinds");
    note(ui, "Global keybinds work even when Fastcord isn't focused.");
    ui.add_space(8.0);
    let mut changed = false;
    if primary(ui, "Add a Keybind").clicked() {
        app.settings.keybinds.push(("ptt".into(), Vec::new()));
        app.sform.recording = Some(app.settings.keybinds.len() - 1);
        app.sform.rec_keys.clear();
        changed = true;
    }
    ui.add_space(12.0);
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
        card(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("ACTION").font(bold(12.0)).color(pal().header_secondary));
                let (action, keys) = &mut app.settings.keybinds[i];
                let shown = ACTIONS.iter().find(|a| a.0 == action).map(|a| a.1).unwrap_or("Push to Talk");
                egui::ComboBox::from_id_salt(("kb_action", i)).width(cols[0].available_width() - 8.0).selected_text(shown).show_ui(&mut cols[0], |ui| {
                    for (k, l) in ACTIONS {
                        if ui.selectable_label(action == k, l).clicked() {
                            *action = k.to_string();
                            changed = true;
                        }
                    }
                });
                cols[1].label(RichText::new("KEYBIND").font(bold(12.0)).color(pal().header_secondary));
                let shown = if recording { combo(&app.sform.rec_keys) } else { combo(keys) };
                let (r, _) = cols[1].allocate_exact_size(vec2(cols[1].available_width(), 40.0), Sense::hover());
                let stroke = if recording { RED } else { pal().border_normal };
                cols[1].painter().rect(r, CornerRadius::same(3), pal().input_bg, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                cols[1].painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, shown, semi(14.0), pal().header_primary);
                let mut b = cols[1].new_child(egui::UiBuilder::new().max_rect(r.shrink(4.0)).layout(Layout::right_to_left(Align::Center)));
                let label = if recording { "Stop Recording" } else { "Record Keybind" };
                let w = b.painter().layout_no_wrap(label.into(), semi(13.0), Color32::WHITE).size().x + 20.0;
                if crate::ui::button(&mut b, label, if recording { RED } else { Color32::from_rgb(0x4e, 0x50, 0x58) }, w).clicked() {
                    if recording {
                        app.sform.recording = None;
                    } else {
                        app.sform.recording = Some(i);
                        app.sform.rec_keys.clear();
                    }
                }
            });
            ui.add_space(4.0);
            if super::link(ui, "Remove Keybind").clicked() {
                remove = Some(i);
            }
        });
        ui.add_space(8.0);
    }
    if let Some(i) = remove {
        app.settings.keybinds.remove(i);
        app.sform.recording = None;
        changed = true;
    }
    divider(ui);
    h5(ui, "Default keybinds");
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
        ui.horizontal(|ui| {
            ui.label(RichText::new(what).font(reg(15.0)).color(pal().text_normal));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                input_frame().corner_radius(4).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                    ui.label(RichText::new(keys).font(bold(12.0)).color(pal().header_secondary));
                });
            });
        });
        ui.add_space(6.0);
    }
    if changed {
        crate::hotkeys::set(&app.settings.keybinds);
    }
    changed
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

pub fn language(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Language");
    note(ui, "Fastcord's own interface is English for now. This tells Discord which language to use for text it sends, like system messages and bot commands.");
    ui.add_space(8.0);
    h5(ui, "Select a language");
    let mut changed = false;
    for (code, native, english) in LOCALES {
        let sel = app.settings.locale == code;
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        super::mark(ui, &format!("lang:{code}"), r);
        let fill = if sel { pal().bg_mod_selected } else if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary };
        ui.painter().rect_filled(r, CornerRadius::same(4), fill);
        let c = pos2(r.left() + 22.0, r.center().y);
        if sel {
            ui.painter().circle_filled(c, 10.0, Color32::WHITE);
            ui.painter().circle_filled(c, 5.5, BLURPLE);
        } else {
            ui.painter().circle_stroke(c, 9.0, Stroke::new(2.0, pal().interactive_hover));
        }
        crate::ui::text::paint(ui.painter(), pos2(r.left() + 46.0, r.center().y), Align2::LEFT_CENTER, native, semi(16.0), pal().header_primary, 300.0);
        ui.painter().text(pos2(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, english, reg(14.0), pal().header_secondary);
        if resp.clicked() && !sel {
            app.settings.locale = code.to_string();
            changed = true;
        }
        ui.add_space(4.0);
    }
    changed
}

pub fn windows(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Windows Settings");
    h5(ui, "System startup behavior");
    let s = &mut app.settings;
    let mut changed = false;
    if toggle(ui, &mut s.open_on_startup, "Open Fastcord", "Save yourself a few clicks and let Fastcord greet you on computer startup.") {
        crate::winsys::set_startup(s.open_on_startup);
        changed = true;
    }
    if s.open_on_startup {
        changed |= toggle(ui, &mut s.start_minimized, "Start Minimized", "Fastcord starts in the background and remains out of your way.");
    }
    changed
}

pub fn streamer(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Streamer Mode");
    note(ui, "If you're a streamer and don't want to show personal information on stream, Streamer Mode hides it for you.");
    ui.add_space(8.0);
    let s = &mut app.settings;
    let mut changed = toggle(ui, &mut s.streamer, "Enable Streamer Mode", "You can also toggle it with a keybind.");
    changed |= toggle(ui, &mut s.streamer_auto, "Automatically enable/disable", "Turns Streamer Mode on while OBS or XSplit is running, and off again when they close.");
    h5(ui, "Options");
    changed |= toggle(ui, &mut s.hide_personal, "Hide Personal Information", "Hides your email, phone number, connected accounts and username.");
    changed |= toggle(ui, &mut s.hide_invites, "Hide Invite Links", "Recommended if you don't want random people accessing your servers.");
    changed |= toggle(ui, &mut s.streamer_no_sounds, "Disable Sounds", "All sound effects will be disabled.");
    changed |= toggle(ui, &mut s.streamer_no_notifications, "Disable Notifications", "All desktop notifications will be disabled.");
    changed |= toggle(ui, &mut s.hide_window, "Hide Fastcord Window from Screen Capture", "Your stream or recording shows a blank area where Fastcord's window is.");
    changed
}

pub fn advanced(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Advanced");
    let mut changed = toggle(
        ui,
        &mut app.settings.dev,
        "Developer Mode",
        "Developer Mode exposes context menu items helpful for people writing bots using the Discord API.",
    );
    h5(ui, "Cache");
    note(ui, "Avatars, emoji and images are cached on disk so they appear instantly. Clearing it frees space; everything downloads again as needed.");
    ui.add_space(4.0);
    if danger(ui, "Clear Image Cache", false).clicked() {
        let _ = std::fs::remove_dir_all(crate::cache::dir().join("img"));
        app.toast("Image cache cleared.");
        changed = true;
    }
    changed
}
