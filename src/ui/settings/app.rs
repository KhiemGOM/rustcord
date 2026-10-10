use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::{Look, action, button, caption, divider, dropdown, markers, note, primary, radio, section, sub, toggle};
use crate::cache::{RoleColors, Spoilers};
use crate::theme::*;
use crate::ui::App;

pub fn appearance(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    section(ui, "Theme");
    let mut sync = app.settings.theme == Theme::Sync;
    if toggle(ui, &mut sync, "Same as Device Theme", "Match your device's light or dark mode.") {
        app.settings.theme = if sync { Theme::Sync } else { Theme::Dark };
        set_theme(ui.ctx(), app.settings.theme);
        changed = true;
    }
    super::end(ui, 32.0);
    sub(ui, "Default Themes");
    let themes = [
        (Theme::Light, "Light", Color32::WHITE),
        (Theme::Ash, "Ash", Color32::from_rgb(50, 51, 57)),
        (Theme::Dark, "Dark", Color32::from_rgb(29, 29, 33)),
        (Theme::Onyx, "Onyx", Color32::BLACK),
        (Theme::Sync, "Sync with computer", Color32::WHITE),
    ];
    super::lead(ui);
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    for (i, (t, name, fill)) in themes.into_iter().enumerate() {
        let r = Rect::from_min_size(strip.min + vec2(56.0 * i as f32, 0.0), vec2(48.0, 48.0));
        let resp = ui.interact(r, egui::Id::new(("theme", name)), Sense::click());
        let on = app.settings.theme == t;
        ui.painter().rect_filled(r, CornerRadius::same(8), fill);
        if t == Theme::Sync {
            let half = Rect::from_min_max(pos2(r.center().x, r.top()), r.max);
            ui.painter().rect_filled(half, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, Color32::from_rgb(29, 29, 33));
        }
        let ring = if on { BLURPLE } else if resp.hovered() { pal().bg_accent } else { Color32::from_rgba_unmultiplied(151, 151, 159, 61) };
        ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(if on { 2.0 } else { 1.0 }, ring), egui::StrokeKind::Inside);
        if on {
            let tick = pos2(r.right() - 3.0, r.top() + 3.0);
            ui.painter().circle_filled(tick, 8.0, BLURPLE);
            crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Check, Rect::from_center_size(tick, vec2(10.0, 10.0)), Color32::WHITE);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        app.rects.insert(format!("theme:{name}"), r);
        if resp.clicked() && !on {
            app.settings.theme = t;
            set_theme(ui.ctx(), t);
            changed = true;
        }
    }

    section(ui, "App Icon");
    note(ui, "Fastcord ships with a single icon.");

    section(ui, "Messages");
    caption(ui, "Message Display", "");
    super::end(ui, 15.0);
    let mut compact = app.settings.compact;
    if radio(ui, &mut compact, &[(false, "Cozy", "Modern, beautiful, and easy on your eyes."), (true, "Compact", "Fit more messages on screen at one time. #IRC")]) {
        app.settings.compact = compact;
        app.heights.clear();
        changed = true;
    }
    if app.settings.compact {
        changed |= toggle(ui, &mut app.settings.compact_avatars, "Show avatars in Compact mode", "");
    }
    let s = &mut app.settings;
    sub(ui, "Display images, videos, and lolcats");
    let mut chat = toggle(ui, &mut s.inline_links, "When posted as links to chat", "");
    chat |= toggle(ui, &mut s.inline_uploads, "When uploaded directly to Discord", "Uploads larger than 10MB will not be previewed.");
    sub(ui, "Embeds and link previews");
    chat |= toggle(ui, &mut s.embeds, "Show embeds and preview website links pasted into chat", "");
    sub(ui, "Emoji");
    chat |= toggle(ui, &mut s.reactions, "Show emoji reactions on messages", "");
    changed |= quick_reactions(app, ui);
    let s = &mut app.settings;
    sub(ui, "Spoiler content");
    note(ui, "This controls when spoiler content is displayed.");
    chat |= radio(
        ui,
        &mut s.spoilers,
        &[(Spoilers::OnClick, "On click", ""), (Spoilers::Moderated, "On servers I moderate", ""), (Spoilers::Always, "Always", "")],
    );

    section(ui, "Chat Box");
    chat |= toggle(
        ui,
        &mut s.emoticons,
        "Automatically convert emoticons in your messages into emoji",
        "For example, when you type :-) Fastcord will convert it to 🙂",
    );
    chat |= toggle(ui, &mut s.send_button, "Show send message button", "");
    if chat {
        app.heights.clear();
    }

    section(ui, "Search");
    note(ui, "Search has no display options in Fastcord yet.");

    section(ui, "Streamer Mode");
    note(ui, "If you're a streamer and don't want to show personal information on stream, Streamer Mode hides it for you.");
    let s = &mut app.settings;
    let mut streamer = toggle(ui, &mut s.streamer, "Enable Streamer Mode", "You can also toggle it with a keybind.");
    streamer |= toggle(ui, &mut s.streamer_auto, "Automatically enable/disable", "Turns Streamer Mode on while OBS or XSplit is running, and off again when they close.");
    streamer |= toggle(ui, &mut s.hide_personal, "Hide Personal Information", "Hides your email, phone number, connected accounts and username.");
    streamer |= toggle(ui, &mut s.hide_invites, "Hide Invite Links", "Recommended if you don't want random people accessing your servers.");
    streamer |= toggle(ui, &mut s.streamer_no_sounds, "Disable Sounds", "All sound effects will be disabled.");
    streamer |= toggle(ui, &mut s.streamer_no_notifications, "Disable Notifications", "All desktop notifications will be disabled.");
    streamer |= toggle(ui, &mut s.hide_window, "Hide Fastcord Window from Screen Capture", "Your stream or recording shows a blank area where Fastcord's window is.");
    changed | chat | streamer
}

/// The four reactions on top of the message menu. They're changed from there, by right-clicking
/// one; here they can go back to following what was used last.
fn quick_reactions(app: &mut App, ui: &mut Ui) -> bool {
    super::lead(ui);
    ui.label(RichText::new("Quick reactions").font(medium(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    let custom = !app.settings.quick_reactions.is_empty();
    let how = if custom { "Picked by you. Right-click one in a message's menu to swap it." } else { "Your most recently used. Right-click one in a message's menu to pick your own." };
    ui.add(egui::Label::new(RichText::new(how).font(reg(14.0)).color(pal().header_secondary)).selectable(false));
    ui.add_space(8.0);
    let mut reset = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for e in app.quick_reactions() {
            let (r, _) = ui.allocate_exact_size(vec2(46.0, 40.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(8), pal().control);
            let url = crate::gif::uri(ui.ctx(), &crate::ui::chat::emoji_url(&e), true);
            egui::Image::from_uri(url).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(r.center(), vec2(24.0, 24.0)));
        }
        if custom {
            ui.add_space(10.0);
            reset = super::link(ui, "Use recently used").clicked();
        }
    });
    super::end(ui, 16.0);
    if reset {
        app.settings.quick_reactions.clear();
    }
    reset
}

fn preview(ui: &mut Ui, app: &App) {
    super::lead(ui);
    ui.label(RichText::new("Preview").font(medium(16.0)).color(pal().text_muted));
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 210.0), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(8), pal().bg_primary, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)), egui::StrokeKind::Inside);
    let me = app.store.user(app.store.me).cloned().unwrap_or_default();
    let size = app.settings.font_size;
    let accent = Color32::from_rgb(221, 128, 244);
    for (i, line) in ["what happened to all the beans", "here's a link https://discord.com/accessibility"].iter().enumerate() {
        let top = r.top() + 27.0 + i as f32 * 100.0;
        crate::ui::avatar(ui, Rect::from_min_size(pos2(r.left() + 26.0, top - 1.0), vec2(48.0, 48.0)), &me.avatar_url());
        let name = crate::ui::text::paint(ui.painter(), pos2(r.left() + 81.0, top), Align2::LEFT_TOP, me.display(), medium(16.0), accent, 200.0);
        ui.painter().text(pos2(name.right() + 8.0, top + 4.0), Align2::LEFT_TOP, "1:15 AM", medium(12.0), pal().text_muted);
        ui.painter().text(pos2(r.left() + 81.0, top + 22.0), Align2::LEFT_TOP, line, reg(size), pal().text_normal);
    }
    let b = Rect::from_min_size(pos2(r.right() - 41.0 - 120.0, r.top() + 153.0), vec2(120.0, 32.0));
    ui.painter().rect(b, CornerRadius::same(8), BLURPLE, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Example Button", medium(14.0), Color32::WHITE);
    ui.add_space(48.0);
}

pub fn accessibility(app: &mut App, ui: &mut Ui) -> bool {
    preview(ui, app);
    section(ui, "Text Readability");
    let s = &mut app.settings;
    caption(ui, "Text size in chat", "Adjust the size of the chat font.");
    let mut changed = markers(ui, &mut s.font_size, &[12.0, 14.0, 15.0, 16.0, 18.0, 20.0, 24.0], |v| format!("{v}px"), 16.0);
    changed |= toggle(ui, &mut s.underline_links, "Always underline links", "Make links stand out more.");

    section(ui, "Visual Density");
    caption(ui, "Space between message groups", "");
    changed |= markers(ui, &mut s.group_space, &[0.0, 4.0, 8.0, 16.0, 24.0], |v| format!("{v}px"), 16.0);
    caption(ui, "Zoom level", "You can change the zoom level with Ctrl +/- or Ctrl and the scroll wheel, and reset to the default with Ctrl+0.");
    let mut pct = (s.zoom * 100.0).round();
    if markers(ui, &mut pct, &crate::ui::ZOOMS, |v| format!("{v}"), 100.0) {
        s.zoom = pct / 100.0;
        changed = true;
    }
    if changed {
        app.heights.clear();
    }

    section(ui, "Color & Contrast");
    let s = &mut app.settings;
    sub(ui, "Role colors");
    let roles = radio(
        ui,
        &mut s.role_colors,
        &[
            (RoleColors::InNames, "Show role colors in names", ""),
            (RoleColors::NextToNames, "Show role colors next to names", ""),
            (RoleColors::Hidden, "Don't show role colors", ""),
        ],
    );

    section(ui, "Reduced Motion");
    let mut motion = toggle(
        ui,
        &mut s.reduced_motion,
        "Enable Reduced Motion",
        "Reduces the amount and intensity of animations, hover effects and other moving effects across Fastcord.",
    );
    motion |= toggle(ui, &mut s.gif_autoplay, "Automatically play GIFs when Fastcord is focused", "");
    motion |= toggle(ui, &mut s.animate_emoji, "Play animated emoji", "");

    section(ui, "Audio & Screen Reader");
    if let Some(v) = super::user::legacy(app, ui) {
        let mut sr = v["allow_accessibility_detection"].as_bool().unwrap_or(false);
        if toggle(ui, &mut sr, "Allow Discord to track screen reader usage", "Lets Discord know whether you use a screen reader, to improve accessibility features.") {
            super::user::set_legacy(app, serde_json::json!({ "allow_accessibility_detection": sr }));
        }
    }
    if roles | motion {
        app.heights.clear();
    }
    changed | roles | motion
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
    section(ui, "Keybinds");
    note(ui, "Global keybinds work even when Fastcord isn't focused.");
    let mut changed = false;
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
                        if ui.selectable_label(action == k, l).clicked() {
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

    section(ui, "Startup");
    let s = &mut app.settings;
    if toggle(ui, &mut s.open_on_startup, "Open Fastcord", "Save yourself a few clicks and let Fastcord greet you on computer startup.") {
        crate::winsys::set_startup(s.open_on_startup);
        changed = true;
    }
    if s.open_on_startup {
        changed |= toggle(ui, &mut s.start_minimized, "Start Minimized", "Fastcord starts in the background and remains out of your way.");
    }
    if changed {
        crate::hotkeys::set(&app.settings.keybinds);
    }
    changed
}

pub fn language(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Language & Time");
    note(ui, "Fastcord's own interface is English for now. This tells Discord which language to use for text it sends, like system messages and bot commands.");
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
