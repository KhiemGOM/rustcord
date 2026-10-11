use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

//// The colours that change with the theme. Brand colours (blurple, status
/// colours) stay put across themes, like they do in the official client.
/// Values are the custom properties discord's web css resolves per theme; the
/// comments name the one each field was read off.
#[derive(Clone, Copy)]
pub struct Palette {
    /// --background-base-lowest: server rail, channel list, title bar
    pub bg_tertiary: Color32,
    /// --background-surface-high: cards, embeds, chat header, popouts, modals
    pub bg_secondary: Color32,
    /// --background-base-low: user panel
    pub bg_secondary_alt: Color32,
    /// --background-base-lower: chat and member list
    pub bg_primary: Color32,
    /// --background-surface-higher: context menus
    pub bg_floating: Color32,
    /// --chat-background-default: the composer's input box
    pub bg_input: Color32,
    /// --message-background-hover over base-lower
    pub bg_msg_hover: Color32,
    /// --interactive-background-hover over base-lowest
    pub bg_mod_hover: Color32,
    /// --interactive-background-selected over base-lowest
    pub bg_mod_selected: Color32,
    /// --border-subtle over base-lower, for rules and outlines that must be opaque
    pub bg_accent: Color32,
    /// --border-muted as an overlay: header rule, the account card's outline
    pub border_muted: Color32,
    /// --border-normal as an overlay: code block outline, embed bars, input outlines
    pub border_normal: Color32,
    /// --border-subtle as an overlay: embed and file card outlines, rules in the channel list
    pub border_subtle: Color32,
    /// --app-frame-background and --app-frame-border: the title bar and the rules between app regions
    pub frame_bg: Color32,
    pub frame_border: Color32,
    /// --input-background-default: text fields, a black wash over whatever they sit on
    pub input_bg: Color32,
    /// --control-secondary-background-default and -hover: buttons on the account card
    pub control: Color32,
    pub control_hover: Color32,
    /// --spine-default: reply spine
    pub spine: Color32,
    pub critical: Color32,
    // --text-feedback-positive: live timers and the like
    pub positive: Color32,
    /// --background-code
    pub bg_code: Color32,
    /// --spoiler-hidden-background and --spoiler-revealed-background
    pub spoiler: Color32,
    pub spoiler_revealed: Color32,
    /// --text-default
    pub text_normal: Color32,
    /// --text-muted
    pub text_muted: Color32,
    /// --chat-text-muted: timestamps
    pub chat_muted: Color32,
    /// --text-strong
    pub header_primary: Color32,
    /// --interactive-text-default
    pub header_secondary: Color32,
    /// --channels-default
    pub channel_default: Color32,
    /// --interactive-muted
    pub channel_muted: Color32,
    /// --interactive-text-hover
    pub interactive_hover: Color32,
    /// --text-link
    pub link: Color32,
    /// --mention-background and --mention-foreground
    pub mention_bg: Color32,
    pub mention_fg: Color32,
    /// --message-mentioned-background-default and -hover, and the bar beside (--icon-feedback-warning)
    pub highlight_bg: Color32,
    pub highlight_hover: Color32,
    pub mention_bar: Color32,
    pub dark: bool,
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

// the grey every theme builds its translucent overlays from
const OVERLAY: u32 = 0x97979f;
// menus hover with the same wash in every theme
pub const MENU_HOVER: Color32 = tint(OVERLAY, 122);
pub const CRITICAL_BG: Color32 = tint(0xd22d39, 78);
pub const PROFILE_CARD: Color32 = tint(OVERLAY, 161);

/// `fg` at `permille`/1000 opacity laid over `bg`, as one opaque colour.
const fn mix(fg: u32, permille: u32, bg: u32) -> Color32 {
    const fn ch(fg: u32, bg: u32, permille: u32, shift: u32) -> u8 {
        let (f, b) = ((fg >> shift) & 0xff, (bg >> shift) & 0xff);
        ((f * permille + b * (1000 - permille) + 500) / 1000) as u8
    }
    Color32::from_rgb(ch(fg, bg, permille, 16), ch(fg, bg, permille, 8), ch(fg, bg, permille, 0))
}

/// `fg` at `permille`/1000 opacity, kept translucent (premultiplied, the way egui wants it).
const fn tint(fg: u32, permille: u32) -> Color32 {
    let a = (permille * 255 + 500) / 1000;
    const fn ch(fg: u32, a: u32, shift: u32) -> u8 {
        ((((fg >> shift) & 0xff) * a + 127) / 255) as u8
    }
    Color32::from_rgba_premultiplied(ch(fg, a, 16), ch(fg, a, 8), ch(fg, a, 0), a as u8)
}

// the grey everyone knows; "ash" in the picker, theme-dark in discord's css
const ASH: Palette = Palette {
    bg_tertiary: rgb(0x2c2d32),
    bg_secondary: rgb(0x393a41),
    bg_secondary_alt: rgb(0x36373e),
    bg_primary: rgb(0x323339),
    bg_floating: rgb(0x3c3d45),
    bg_input: rgb(0x393a41),
    bg_msg_hover: mix(OVERLAY, 122, 0x323339),
    bg_mod_hover: mix(OVERLAY, 122, 0x2c2d32),
    bg_mod_selected: mix(OVERLAY, 200, 0x2c2d32),
    bg_accent: mix(OVERLAY, 122, 0x323339),
    border_muted: tint(OVERLAY, 39),
    border_normal: tint(OVERLAY, 200),
    border_subtle: tint(OVERLAY, 122),
    frame_bg: rgb(0x25262a),
    frame_border: tint(OVERLAY, 122),
    input_bg: tint(0x000000, 78),
    control: tint(OVERLAY, 122),
    control_hover: tint(OVERLAY, 161),
    spine: rgb(0x595a63),
    critical: rgb(0xffa09b),
    positive: rgb(0x7ecb94),
    bg_code: tint(0x5865f2, 78),
    spoiler: rgb(0x7d7e87),
    spoiler_revealed: rgb(0x232428),
    text_normal: rgb(0xf3f3f4),
    text_muted: rgb(0xabacb2),
    chat_muted: rgb(0x9d9ea5),
    header_primary: rgb(0xffffff),
    header_secondary: rgb(0xc5c6ca),
    channel_default: rgb(0x999aa1),
    channel_muted: rgb(0x4e5058),
    interactive_hover: rgb(0xffffff),
    link: rgb(0x76aff6),
    mention_bg: tint(0x5865f2, 239),
    mention_fg: rgb(0xcdd7ff),
    highlight_bg: tint(0xf8a300, 78),
    highlight_hover: tint(0xf8a300, 39),
    mention_bar: rgb(0xfcb529),
    dark: true,
};

// theme-darker
const DARK: Palette = Palette {
    bg_tertiary: rgb(0x121214),
    bg_secondary: rgb(0x242429),
    bg_secondary_alt: rgb(0x202024),
    bg_primary: rgb(0x1a1a1e),
    bg_floating: rgb(0x28282d),
    bg_input: rgb(0x222327),
    bg_msg_hover: mix(OVERLAY, 122, 0x1a1a1e),
    bg_mod_hover: mix(OVERLAY, 122, 0x121214),
    bg_mod_selected: mix(OVERLAY, 200, 0x121214),
    bg_accent: mix(OVERLAY, 122, 0x1a1a1e),
    input_bg: tint(0x000000, 122),
    frame_bg: rgb(0x070709),
    spine: rgb(0x474851),
    critical: rgb(0xf87e7a),
    positive: rgb(0x5eb479),
    spoiler: rgb(0x666770),
    spoiler_revealed: tint(0x6c6f7c, 160),
    text_normal: rgb(0xefeff1),
    text_muted: rgb(0x96979e),
    chat_muted: rgb(0x81828a),
    header_primary: rgb(0xfbfbfb),
    header_secondary: rgb(0xabacb2),
    channel_default: rgb(0x81828a),
    channel_muted: rgb(0x4d505b),
    interactive_hover: rgb(0xfbfbfb),
    link: rgb(0x4d96ee),
    mention_fg: rgb(0xa9bbff),
    mention_bar: rgb(0xea9800),
    ..ASH
};

// theme-midnight
const ONYX: Palette = Palette {
    bg_tertiary: rgb(0x000000),
    bg_secondary: rgb(0x0a0a0c),
    bg_secondary_alt: rgb(0x000000),
    bg_primary: rgb(0x000000),
    bg_floating: rgb(0x121214),
    bg_input: rgb(0x101013),
    bg_msg_hover: mix(OVERLAY, 122, 0x000000),
    bg_mod_hover: mix(OVERLAY, 122, 0x000000),
    bg_mod_selected: mix(OVERLAY, 239, 0x000000),
    bg_accent: mix(OVERLAY, 200, 0x000000),
    border_muted: tint(OVERLAY, 161),
    border_normal: tint(OVERLAY, 239),
    border_subtle: tint(OVERLAY, 200),
    frame_bg: rgb(0x000000),
    frame_border: tint(OVERLAY, 200),
    input_bg: tint(0x000000, 122),
    control_hover: tint(OVERLAY, 200),
    spine: rgb(0x383940),
    critical: rgb(0xeb5f5e),
    positive: rgb(0x41a063),
    spoiler: rgb(0x595a63),
    spoiler_revealed: tint(0x6c6f7c, 160),
    text_normal: rgb(0xd4d5d8),
    text_muted: rgb(0x81828a),
    chat_muted: rgb(0x73747d),
    header_primary: rgb(0xdcdcdf),
    header_secondary: rgb(0x96979e),
    channel_default: rgb(0x7a7b83),
    channel_muted: rgb(0x4d505b),
    interactive_hover: rgb(0xdcdcdf),
    link: rgb(0x2781e7),
    mention_fg: rgb(0x8ca0fd),
    mention_bar: rgb(0xce8100),
    ..ASH
};

const LIGHT: Palette = Palette {
    bg_tertiary: rgb(0xf3f3f4),
    bg_secondary: rgb(0xffffff),
    bg_secondary_alt: rgb(0xfbfbfb),
    bg_primary: rgb(0xfbfbfb),
    bg_floating: rgb(0xffffff),
    bg_input: rgb(0xffffff),
    bg_msg_hover: mix(OVERLAY, 122, 0xfbfbfb),
    bg_mod_hover: mix(OVERLAY, 122, 0xf3f3f4),
    bg_mod_selected: mix(OVERLAY, 239, 0xf3f3f4),
    bg_accent: mix(OVERLAY, 278, 0xfbfbfb),
    border_muted: tint(OVERLAY, 200),
    border_normal: tint(OVERLAY, 400),
    border_subtle: tint(OVERLAY, 278),
    frame_bg: rgb(0xe8e8ea),
    frame_border: tint(OVERLAY, 278),
    input_bg: tint(0x000000, 20),
    spine: rgb(0xc5c6ca),
    critical: rgb(0xb92733),
    positive: rgb(0x01713a),
    bg_code: tint(0x5865f2, 39),
    spoiler: rgb(0x8f9097),
    spoiler_revealed: rgb(0xebedef),
    text_normal: rgb(0x2e2e34),
    text_muted: rgb(0x6c6d76),
    chat_muted: rgb(0x70717a),
    header_primary: rgb(0x28282d),
    header_secondary: rgb(0x595a63),
    channel_default: rgb(0x666770),
    channel_muted: rgb(0xc4c9ce),
    interactive_hover: rgb(0x28282d),
    link: rgb(0x006dd4),
    mention_fg: rgb(0x2e3c88),
    dark: false,
    mention_bar: rgb(0x945300),
    ..ASH
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    Light,
    #[default]
    Ash,
    Dark,
    Onyx,
    Sync,
}

static THEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);
const PALETTES: [Palette; 4] = [LIGHT, ASH, DARK, ONYX];

/// Display options the renderers read without an `App` in reach (markdown,
/// emoji urls). Set from settings in `App::apply_settings`.
#[derive(Clone, Copy)]
pub struct View {
    pub underline_links: bool,
    pub animate_emoji: bool,
    pub hide_invites: bool,
}

const VIEW_DEFAULT: View = View { underline_links: false, animate_emoji: true, hide_invites: false };
static VIEW: std::sync::RwLock<View> = std::sync::RwLock::new(VIEW_DEFAULT);

pub fn view() -> View {
    VIEW.read().map(|v| *v).unwrap_or(VIEW_DEFAULT)
}

pub fn set_view(v: View) {
    if let Ok(mut cur) = VIEW.write() {
        *cur = v;
    }
}

static REDUCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_reduced_motion(on: bool) {
    REDUCED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Animation length, or zero when reduced motion is on.
pub fn anim(secs: f32) -> f32 {
    if REDUCED.load(std::sync::atomic::Ordering::Relaxed) { 0.0 } else { secs }
}

pub fn pal() -> &'static Palette {
    &PALETTES[THEME.load(std::sync::atomic::Ordering::Relaxed) as usize]
}

/// Onyx draws a few hairlines the other themes leave out.
pub fn onyx() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 3
}

/// The wash and outline discord gives every text field.
pub fn input_frame() -> egui::Frame {
    egui::Frame::new().fill(pal().input_bg).stroke(Stroke::new(1.0, pal().border_normal))
}

/// Switch the live palette. `Sync` follows the system's light/dark setting.
pub fn set_theme(ctx: &egui::Context, t: Theme) {
    let idx = match t {
        Theme::Light => 0,
        Theme::Ash => 1,
        Theme::Dark => 2,
        Theme::Onyx => 3,
        Theme::Sync => match ctx.system_theme() {
            Some(egui::Theme::Light) => 0,
            _ => 1,
        },
    };
    if THEME.swap(idx, std::sync::atomic::Ordering::Relaxed) != idx {
        style(ctx);
    }
}



pub const BLURPLE: Color32 = Color32::from_rgb(0x58, 0x65, 0xf2);
pub const RED: Color32 = Color32::from_rgb(0xf2, 0x3f, 0x43);
pub const GREEN: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);
pub const YELLOW: Color32 = Color32::from_rgb(0xf0, 0xb2, 0x32);
pub const GREY: Color32 = Color32::from_rgb(0x80, 0x84, 0x8e);
// discord's keyboard focus ring
pub const FOCUS: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);


/// Set once any text with CJK/Hangul (or the "・" channel names love) shows up;
/// only then are the ~30MB fallback fonts worth loading.
pub static CJK_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn note_text(s: &str) {
    use std::sync::atomic::Ordering::Relaxed;
    if !CJK_WANTED.load(Relaxed)
        && s.chars().any(|c| matches!(c as u32, 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x1100..=0x11FF))
    {
        CJK_WANTED.store(true, Relaxed);
    }
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

pub fn semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semi".into()))
}

pub fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn status_color(s: crate::state::Status) -> Color32 {
    use crate::state::Status::*;
    match s {
        Online => GREEN,
        Idle => YELLOW,
        Dnd => RED,
        Offline => GREY,
    }
}

pub fn role_color(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Pull `c` toward black or white until it reaches `ratio` against `bg`; discord does this
/// to display name style colours so a pale one isn't lost on the light theme.
pub fn contrast(c: Color32, bg: Color32, ratio: f32) -> Color32 {
    let lum = |c: Color32| {
        let lin = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    };
    let reads = |a: Color32| (lum(a).max(lum(bg)) + 0.05) / (lum(a).min(lum(bg)) + 0.05) >= ratio;
    let to = if lum(bg) > 0.4 { Color32::BLACK } else { Color32::WHITE };
    (0..=20).map(|i| c.lerp_to_gamma(to, i as f32 / 20.0)).find(|a| reads(*a)).unwrap_or(to)
}

/// Colour `t` of the way (0 to 1) along evenly spaced stops, like css `linear-gradient`.
pub fn ramp(stops: &[Color32], t: f32) -> Color32 {
    let last = stops.len() - 1;
    if last == 0 {
        return stops[0];
    }
    let x = t.clamp(0.0, 1.0) * last as f32;
    let i = (x as usize).min(last - 1);
    stops[i].lerp_to_gamma(stops[i + 1], x - i as f32)
}

/// Font setup. `cjk` adds the big CJK/Hangul fallbacks (~30MB), which we load
/// off the UI thread after the first frame so startup doesn't pay for them.
pub fn fonts(cjk: bool) -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
    let mut add = |key: &str, file: &str| -> bool {
        match std::fs::read(format!("{dir}{file}")) {
            Ok(b) => {
                f.font_data.insert(key.into(), Arc::new(FontData::from_owned(b)));
                true
            }
            Err(_) => false,
        }
    };
    let ui = add("ui", "segoeui.ttf");
    let semi = add("semi", "seguisb.ttf");
    let bold = add("bold", "segoeuib.ttf");
    let sym = add("sym", "seguisym.ttf");
    let code = add("code", "consola.ttf");
    // last resort for pictographs the ui fonts lack; egui draws its monochrome outlines
    let emoji = add("emoji", "seguiemj.ttf");
    // channel names lean on things like "・" and "┃", and plenty of people write CJK
    let yahei = cjk && add("cjk", "msyh.ttc");
    let hangul = cjk && add("hangul", "malgun.ttf");
    // discord's own face once it has been downloaded, see fonts.rs
    let gg: Option<Vec<Vec<u8>>> = crate::fonts::WEIGHTS.iter().map(|w| crate::fonts::cached(*w)).collect();
    if let Some(files) = &gg {
        for (w, b) in crate::fonts::WEIGHTS.iter().zip(files) {
            f.font_data.insert(format!("gg{w}"), Arc::new(FontData::from_owned(b.clone())));
        }
    }

    let defaults = f.families[&FontFamily::Proportional].clone();
    let chain = |first: Option<&str>| {
        let mut v: Vec<String> = first.into_iter().map(str::to_string).collect();
        if ui {
            v.push("ui".into());
        }
        if sym {
            v.push("sym".into());
        }
        v.extend(defaults.iter().cloned());
        if yahei {
            v.push("cjk".into());
        }
        if hangul {
            v.push("hangul".into());
        }
        if emoji {
            v.push("emoji".into());
        }
        v.dedup();
        v
    };
    let gg = gg.is_some();
    let prop = chain(gg.then_some("gg400"));
    let m = chain(gg.then_some("gg500"));
    let s = chain(if gg { Some("gg600") } else { semi.then_some("semi") });
    let b = chain(if gg { Some("gg700") } else { bold.then_some("bold") });
    f.families.insert(FontFamily::Proportional, prop);
    f.families.insert(FontFamily::Name("medium".into()), m);
    f.families.insert(FontFamily::Name("semi".into()), s);
    f.families.insert(FontFamily::Name("bold".into()), b);
    // every name font family exists from the start; until its file arrives it's just gg sans
    for (id, ..) in crate::fonts::NAME_FONTS {
        let key = format!("name{id}");
        let have = crate::fonts::cached_name(id).map(|b| f.font_data.insert(key.clone(), Arc::new(FontData::from_owned(b)))).is_some();
        let fam = chain(if have { Some(key.as_str()) } else if gg { Some("gg500") } else { None });
        f.families.insert(FontFamily::Name(key.into()), fam);
    }
    if code {
        f.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "code".into());
    }
    f
}

/// A nitro display name in its font, or gg sans medium when it has none.
pub fn name_font(font: u8, size: f32) -> FontId {
    if crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == font) {
        FontId::new(size, FontFamily::Name(format!("name{font}").into()))
    } else {
        medium(size)
    }
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts(false));
    style(ctx);
}

/// egui's own style, rebuilt from the current palette whenever the theme changes.
pub fn style(ctx: &egui::Context) {
    let p = pal();
    let mut style = egui::Style::default();
    style.visuals = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let v = &mut style.visuals;
    v.panel_fill = pal().bg_primary;
    v.window_fill = pal().bg_floating;
    v.extreme_bg_color = pal().bg_tertiary;
    v.faint_bg_color = pal().bg_secondary;
    v.hyperlink_color = pal().link;
    v.override_text_color = None;
    v.selection.bg_fill = if p.dark { Color32::from_rgb(0x3d, 0x5a, 0xbf) } else { Color32::from_rgb(0xb3, 0xc5, 0xf5) };
    v.selection.stroke = Stroke::new(1.0, pal().header_primary);
    // discord's menus and popouts: a hairline, 8 rounded, the deep drop shadow
    v.window_stroke = Stroke::new(1.0, p.border_subtle);
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 8.into();
    v.popup_shadow = egui::Shadow { offset: [0, 12], blur: 24, spread: 0, color: Color32::from_black_alpha(if p.dark { 61 } else { 30 }) };
    v.window_shadow = v.popup_shadow;
    v.text_cursor.stroke = Stroke::new(2.0, pal().text_normal);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 4.into();
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, pal().text_normal);
    }
    v.widgets.noninteractive.bg_fill = pal().bg_secondary;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = pal().bg_input;
    v.widgets.hovered.weak_bg_fill = pal().bg_mod_hover;
    v.widgets.hovered.bg_fill = pal().bg_mod_hover;
    v.widgets.active.weak_bg_fill = pal().bg_mod_selected;
    v.widgets.active.bg_fill = pal().bg_mod_selected;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal().text_normal);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, pal().header_secondary);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, pal().interactive_hover);
    v.widgets.active.fg_stroke = Stroke::new(1.0, pal().interactive_hover);

    style.spacing.item_spacing = egui::vec2(8.0, 0.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 8.0,
        floating: true,
        floating_allocated_width: 0.0,
        // thin() paints the track at full opacity even at rest; discord has no track and hides the thumb until hover or scroll
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 0.0,
        active_handle_opacity: 0.7,
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::thin()
    };
    style.text_styles.insert(egui::TextStyle::Body, reg(16.0));
    style.text_styles.insert(egui::TextStyle::Button, reg(14.0));
    style.text_styles.insert(egui::TextStyle::Small, reg(12.0));
    style.text_styles.insert(egui::TextStyle::Monospace, mono(14.0));
    style.interaction.selectable_labels = true;
    style.animation_time = anim(0.1);
    // egui keeps one style per egui theme; we pin egui to one and swap its style
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
