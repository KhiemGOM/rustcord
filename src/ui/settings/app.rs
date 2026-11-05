use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{caption, markers, note, radio, section, sub, toggle};
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
    note(ui, "Rustcord ships with a single icon.");

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
        "For example, when you type :-) Rustcord will convert it to 🙂",
    );
    chat |= toggle(ui, &mut s.send_button, "Show send message button", "");
    if chat {
        app.heights.clear();
    }

    section(ui, "Search");
    note(ui, "Search has no display options in Rustcord yet.");

    section(ui, "Streamer Mode");
    note(ui, "If you're a streamer and don't want to show personal information on stream, Streamer Mode hides it for you.");
    let s = &mut app.settings;
    let mut streamer = toggle(ui, &mut s.streamer, "Enable Streamer Mode", "You can also toggle it with a keybind.");
    streamer |= toggle(ui, &mut s.streamer_auto, "Automatically enable/disable", "Turns Streamer Mode on while OBS or XSplit is running, and off again when they close.");
    streamer |= toggle(ui, &mut s.hide_personal, "Hide Personal Information", "Hides your email, phone number, connected accounts and username.");
    streamer |= toggle(ui, &mut s.hide_invites, "Hide Invite Links", "Recommended if you don't want random people accessing your servers.");
    streamer |= toggle(ui, &mut s.streamer_no_sounds, "Disable Sounds", "All sound effects will be disabled.");
    streamer |= toggle(ui, &mut s.streamer_no_notifications, "Disable Notifications", "All desktop notifications will be disabled.");
    streamer |= toggle(ui, &mut s.hide_window, "Hide Rustcord Window from Screen Capture", "Your stream or recording shows a blank area where Rustcord's window is.");
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
        "Reduces the amount and intensity of animations, hover effects and other moving effects across Rustcord.",
    );
    motion |= toggle(ui, &mut s.gif_autoplay, "Automatically play GIFs when Rustcord is focused", "");
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
