use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::privacy::write;
use super::wire::Patch;
use super::{Page, caption, dropdown, markers, radio, section, sub, toggle};
use crate::cache::{Animate, Density, RoleColors, Settings, Spoilers, VideoBg};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

pub fn appearance(app: &mut App, ui: &mut Ui) -> bool {
    let mut changed = false;
    section(ui, "Messages");
    sub(ui, "Show images, videos, and lolcats...");
    let mut chat = synced(app, ui, "When posted as links to chat", "", |s| &mut s.inline_links, 10);
    chat |= synced(app, ui, "When uploaded directly to Discord", "Uploads larger than 10MB will not be previewed.", |s| &mut s.inline_uploads, 9);
    chat |= synced(app, ui, "Show embeds and link previews", "", |s| &mut s.embeds, 12);
    chat |= synced(app, ui, "Show emoji reactions", "", |s| &mut s.reactions, 13);
    changed |= quick_reactions(app, ui);
    let s = &mut app.settings;
    let shown = match s.spoilers {
        Spoilers::OnClick => "On click",
        Spoilers::Moderated => "On servers I moderate",
        Spoilers::Always => "Always",
    };
    let mut spoilers = s.spoilers;
    if choice(ui, "Show spoiler content", shown, |ui| {
        for (v, name) in [(Spoilers::OnClick, "On click"), (Spoilers::Moderated, "On servers I moderate"), (Spoilers::Always, "Always")] {
            if ui.selectable_label(spoilers == v, name).clicked() {
                spoilers = v;
            }
        }
    }) {
        chat = true;
    }
    if spoilers != s.spoilers {
        s.spoilers = spoilers;
        let name = match spoilers {
            Spoilers::OnClick => "ON_CLICK",
            Spoilers::Moderated => "IF_MODERATOR",
            Spoilers::Always => "ALWAYS",
        };
        write(app, Patch::new().text(6, 4, name));
    }
    let s = &mut app.settings;
    let compact = s.compact;
    let mut avatars = !compact || s.compact_avatars;
    let desc = "To hide avatars, go to Accessibility and select Compact under Chat Message Display.";
    ui.add_enabled_ui(compact, |ui| {
        if toggle(ui, &mut avatars, "Show user avatars", desc) {
            chat = true;
        }
    });
    if compact && avatars != s.compact_avatars {
        s.compact_avatars = avatars;
        write(app, Patch::new().flag(6, 41, avatars));
    }
    related(app, ui, Page::Accessibility, "Accessibility", "Adjust the visual density of chat messages and UI");

    sub(ui, "Threads");
    changed |= synced(app, ui, "Open threads in split view", "", |s| &mut s.thread_sidebar, 3);


    section(ui, "Chat Box");
    chat |= synced(app, ui, "Preview emoji, mentions, and markdown syntax as you type", "", |s| &mut s.md_preview, 2);
    chat |= synced(app, ui, "Automatically convert emoticons in your messages to emoji", "For example, when you type :) Rustcord will convert it to 🙂", |s| &mut s.emoticons, 21);
    chat |= synced(app, ui, "Show stickers in autocomplete results", "", |s| &mut s.sticker_suggestions, 28);
    chat |= synced(app, ui, "Show games in autocomplete results", "", |s| &mut s.game_suggestions, 39);
    chat |= toggle(ui, &mut app.settings.send_button, "Show send message button", "");
    if chat {
        app.heights.clear();
    }

    section(ui, "Search");
    sub(ui, "By default, searching in DMs...");
    let s = &mut app.settings;
    let mut all = s.search_all_dms;
    if radio(ui, &mut all, &[(false, "Searches only in the selected DM", ""), (true, "Searches across all my DMs", "")]) {
        s.search_all_dms = all;
        write(app, Patch::new().flag(6, 36, all));
        changed = true;
    }

    section(ui, "Streamer Mode");
    let s = &mut app.settings;
    let mut streamer = toggle(ui, &mut s.streamer, "Enable Streamer Mode", "Streamer Mode hides your personal info so others watching your stream can’t see it.");
    streamer |= toggle(ui, &mut s.streamer_auto, "Automatically enable/disable", "Turns Streamer Mode on while OBS or XSplit is running, and off again when they close.");
    sub(ui, "If Streamer Mode is enabled...");
    streamer |= toggle(ui, &mut s.hide_personal, "Hide my personal information, such as email, connected accounts, and notes", "");
    streamer |= toggle(ui, &mut s.hide_invites, "Hide invite links to my Discord servers", "");
    streamer |= toggle(ui, &mut s.streamer_no_sounds, "Disable all sound effects", "");
    streamer |= toggle(ui, &mut s.streamer_no_notifications, "Disable notifications", "");
    streamer |= toggle(ui, &mut s.hide_window, "Hide Rustcord Window from Screen Capture", "Your stream or recording shows a blank area where Rustcord's window is.");
    changed | chat | streamer
}

/// A theme swatch's ring, tick and click; true when it was clicked.
pub(super) fn tile(app: &mut App, ui: &mut Ui, name: &str, r: Rect, on: bool) -> bool {
    let resp = ui.interact(r, egui::Id::new(("theme", name)), Sense::click());
    let ring = if on { BLURPLE } else { crate::ui::motion::mix(Color32::from_rgba_unmultiplied(151, 151, 159, 61), pal().bg_accent, crate::ui::motion::hot(ui.ctx(), &resp)) };
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(if on { 2.0 } else { 1.0 }, ring), egui::StrokeKind::Inside);
    if on {
        let tick = pos2(r.right() - 3.0, r.top() + 3.0);
        ui.painter().circle_filled(tick, 8.0, BLURPLE);
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(tick, vec2(10.0, 10.0)), Color32::WHITE);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    app.rects.insert(format!("theme:{name}"), r);
    resp.clicked()
}

/// The link row to a neighbouring page: its icon, a name, one line, a chevron.
pub(super) fn related(app: &mut App, ui: &mut Ui, page: Page, name: &str, desc: &str) {
    super::lead(ui);
    ui.add_space(24.0);
    ui.add(egui::Label::new(RichText::new("Related Settings").font(semi(16.0)).color(pal().text_normal)).selectable(false));
    ui.add_space(16.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let icon = Rect::from_min_size(r.min, vec2(56.0, 56.0));
    ui.painter().rect_filled(icon, CornerRadius::same(8), pal().control);
    let glyph = match page {
        Page::Accessibility => Icon::SideAccess,
        _ => Icon::SideAppearance,
    };
    icons::draw(ui.painter(), glyph, Rect::from_center_size(icon.center(), vec2(24.0, 24.0)), pal().header_primary);
    ui.painter().text(pos2(r.left() + 70.0, r.top() + 10.0), Align2::LEFT_TOP, name, medium(16.0), pal().header_primary);
    ui.painter().text(pos2(r.left() + 70.0, r.top() + 31.0), Align2::LEFT_TOP, desc, reg(14.0), pal().header_secondary);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(24.0, 24.0)), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.settings_page = Some(page);
    }
    super::end(ui, 16.0);
}

/// A label on the left and a dropdown on the right; true when the menu's pick changed something.
pub(super) fn choice(ui: &mut Ui, label: &str, shown: &str, menu: impl FnOnce(&mut Ui)) -> bool {
    super::lead(ui);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, label, medium(16.0), pal().header_primary);
    let right = Rect::from_min_size(pos2(r.right() - 296.0, r.top()), vec2(296.0, 40.0));
    let mut resp = None;
    ui.scope_builder(UiBuilder::new().max_rect(right), |ui| {
        resp = Some(dropdown(ui, 296.0, None, shown, menu));
    });
    super::end(ui, 24.0);
    resp.is_some_and(|r| r.clicked())
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
    changed |= toggle(ui, &mut s.name_styles, "Display Name Styles", "Enable display name styles — including font, effect, and color — across Discord.");

    section(ui, "Visual Density");
    caption(ui, "UI Density", "Adjust the space between server, channel, and member lists.");
    let mut density = s.density;
    let density_changed = radio(ui, &mut density, &[(Density::Compact, "Compact", ""), (Density::Default, "Default", ""), (Density::Spacious, "Spacious", "")]);
    if density_changed {
        s.density = density;
    }
    caption(ui, "Chat Message Display", "Change the appearance of chat messages.");
    let mut compact = s.compact;
    let mut flipped = false;
    if radio(ui, &mut compact, &[(false, "Cozy", "Modern, beautiful, and easy on your eyes."), (true, "Compact", "Fit more messages on screen at one time. #IRC")]) {
        s.compact = compact;
        changed = true;
        flipped = true;
    }
    caption(ui, "Space Between Message Groups", "Adjust the spacing between message groups.");
    changed |= markers(ui, &mut s.group_space, &[0.0, 4.0, 8.0, 16.0, 24.0], |v| format!("{v}px"), 16.0);
    caption(ui, "Zoom level", "You can change the zoom level with Ctrl +/- or Ctrl and the scroll wheel, and reset to the default with Ctrl+0.");
    let mut pct = (s.zoom * 100.0).round();
    if markers(ui, &mut pct, &crate::ui::ZOOMS, |v| format!("{v}"), 100.0) {
        s.zoom = pct / 100.0;
        changed = true;
    }
    if flipped {
        write(app, Patch::new().flag(6, 17, app.settings.compact));
    }
    if density_changed {
        let v = match app.settings.density {
            Density::Compact => 1,
            Density::Spacious => 2,
            Density::Default => 4,
        };
        write(app, Patch::new().num(13, 12, v));
    }
    changed |= density_changed;
    if changed {
        app.heights.clear();
    }

    section(ui, "Color & Contrast");
    let s = &mut app.settings;
    caption(ui, "Saturation", "Reduce the saturation of colors within the app, for those with color sensitivities. This does not affect the saturation of images, videos, role colors or other user content.");
    let mut pct = (s.saturation * 100.0).round();
    let stops: Vec<f32> = (0..=10).map(|i| i as f32 * 10.0).collect();
    let moved = markers(ui, &mut pct, &stops, |v| format!("{v}%"), 100.0);
    if moved {
        s.saturation = pct / 100.0;
    }
    let mut colors = moved;
    colors |= toggle(ui, &mut s.saturate_custom, "Apply saturation setting to custom colors", "Apply the above setting to custom color choices, like role colors.");
    colors |= toggle(ui, &mut s.high_contrast, "Enable High Contrast Mode", "Enhance visibility with bold colors and sharp contrast.");
    colors |= toggle(ui, &mut s.shop_cursor, "Enable custom cursor in shop", "Use a custom cursor design when browsing the shop. Can be turned off if the cursor feels distracting or interferes with navigation.");
    colors |= toggle(ui, &mut s.sync_contrast, "Sync contrast settings", "This allows Discord to use your computer's contrast theme.");
    let mut role = s.role_colors;
    let shown = match role {
        RoleColors::InNames => "In names",
        RoleColors::NextToNames => "Next to names",
        RoleColors::Hidden => "Hidden",
    };
    let picked = choice(ui, "Role Colors", shown, |ui| {
        for (v, name) in [(RoleColors::InNames, "In names"), (RoleColors::NextToNames, "Next to names"), (RoleColors::Hidden, "Hidden")] {
            if ui.selectable_label(role == v, name).clicked() {
                role = v;
            }
        }
    });
    let roles = picked || role != s.role_colors;
    s.role_colors = role;
    colors |= toggle(ui, &mut s.switch_icons, "Show on/off indicators", "On forms, menus and settings, on/off toggles will have icons.");
    related(app, ui, Page::Appearance, "Appearance", "Change your app theme");

    section(ui, "Reduced Motion");
    let s = &mut app.settings;
    let mut motion = toggle(
        ui,
        &mut s.reduced_motion,
        "Enable Reduced Motion",
        "Reduce the amount and intensity of animations, hover effects, and other moving effects.",
    );
    motion |= toggle(ui, &mut s.sync_motion, "Sync with computer setting", "");
    caption(ui, "Minimum notification duration", "Keep pop-up notifications on screen for at least this long. Notifications that already stay longer are unaffected.");
    motion |= markers(ui, &mut s.min_toast_secs, &[3.0, 5.0, 10.0, 15.0, 20.0], |v| format!("{v}s"), 5.0);
    motion |= synced(app, ui, "Play GIFs when Rustcord is focused", "", |s| &mut s.gif_autoplay, 11);
    motion |= synced(app, ui, "Play animated emoji", "", |s| &mut s.animate_emoji, 14);
    sub(ui, "Play sticker animations");
    let mut stickers = app.settings.stickers;
    if radio(
        ui,
        &mut stickers,
        &[
            (Animate::Always, "Always animate", ""),
            (Animate::OnInteraction, "Animate on interaction", "On the desktop client, stickers will animate on hover or focus. On mobile clients, stickers will animate on long-press."),
            (Animate::Never, "Never animate", ""),
        ],
    ) {
        app.settings.stickers = stickers;
        write(app, Patch::new().sub(6, 15, &[(1, stickers as u64)]));
        motion = true;
    }

    section(ui, "Audio & Screen Reader");
    let s = &mut app.settings;
    caption(ui, "Text-to-Speech rate", "Control the speed at which text is read aloud when using Speak Message or other text-to-speech features.");
    let mut audio = speech_rate(ui, &mut s.tts_rate);
    audio |= synced(app, ui, "Show image descriptions", "Image descriptions are used to describe images for screen readers.", |s| &mut s.image_descriptions, 7);
    audio |= synced(app, ui, "Use the legacy chat input", "Disables most chat features but may work better with screen readers.", |s| &mut s.legacy_input, 24);
    if audio {
        app.heights.clear();
    }
    if roles | motion | colors {
        app.heights.clear();
    }
    changed | roles | motion | colors | audio
}

/// What the account has never been told: settings that used to stay on this machine and differ from
/// Discord's default are sent up at login rather than lost to it.
pub fn backfill(app: &mut App) {
    let (a, s) = (&app.store.account, &app.settings);
    let mut patch = Patch::new();
    if a.image_descriptions.is_none() && s.image_descriptions {
        patch = patch.flag(6, 7, true);
    }
    if a.stickers.is_none() && s.stickers != Animate::Always {
        patch = patch.sub(6, 15, &[(1, s.stickers as u64)]);
    }
    if a.search_all_dms.is_none() && s.search_all_dms {
        patch = patch.flag(6, 36, true);
    }
    if a.density.is_none() && s.density != Density::Default {
        patch = patch.num(13, 12, if s.density == Density::Compact { 1 } else { 2 });
    }
    if a.preview_video.is_none() && s.preview_video {
        patch = patch.flag(5, 5, true);
    }
    if a.video_bg.is_none() {
        match s.video_bg {
            VideoBg::Blur => patch = patch.sub(5, 1, &[(1, 1)]),
            VideoBg::Preset(n) => patch = patch.num(5, 2, n as u64),
            VideoBg::None | VideoBg::Custom => {}
        }
    }
    let peers = (a.peers.is_none() && !s.peers.is_empty()).then(|| crate::proto::peers_patch(&s.peers, 0));
    if !patch.is_empty() {
        write(app, patch);
    }
    if let Some(bytes) = peers {
        super::privacy::send(app, bytes);
    }
}

fn synced(app: &mut App, ui: &mut Ui, label: &str, desc: &str, field: fn(&mut Settings) -> &mut bool, f: u64) -> bool {
    let mut on = *field(&mut app.settings);
    let hit = toggle(ui, &mut on, label, desc);
    if hit {
        *field(&mut app.settings) = on;
        write(app, Patch::new().flag(6, f, on));
    }
    hit
}

/// The slider from "Slower" to "Faster" with the speed beside it, and a button that reads a line aloud.
fn speech_rate(ui: &mut Ui, rate: &mut f32) -> bool {
    super::lead(ui);
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 20.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "Slower", medium(12.0), pal().header_secondary);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, format!("x{rate:.1}"), medium(12.0), pal().header_secondary);
    ui.painter().text(r.right_center(), Align2::RIGHT_CENTER, "Faster", medium(12.0), pal().header_secondary);
    ui.add_space(8.0);
    let mut moved = super::slider(ui, w, rate, 0.5, 2.0);
    *rate = (*rate * 10.0).round() / 10.0;
    ui.add_space(16.0);
    if super::secondary(ui, "Preview").clicked() {
        crate::speech::say("This is how fast Rustcord reads text aloud.".into());
        moved = false;
    }
    super::end(ui, 24.0);
    moved
}
