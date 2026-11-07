use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

mod fill;

pub use fill::{Custom, DEFAULT_COLOR, Fill};

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

impl Palette {
    fn map(self, f: impl Fn(Color32) -> Color32) -> Palette {
        Palette {
            bg_tertiary: f(self.bg_tertiary),
            bg_secondary: f(self.bg_secondary),
            bg_secondary_alt: f(self.bg_secondary_alt),
            bg_primary: f(self.bg_primary),
            bg_floating: f(self.bg_floating),
            bg_input: f(self.bg_input),
            bg_msg_hover: f(self.bg_msg_hover),
            bg_mod_hover: f(self.bg_mod_hover),
            bg_mod_selected: f(self.bg_mod_selected),
            bg_accent: f(self.bg_accent),
            border_muted: f(self.border_muted),
            border_normal: f(self.border_normal),
            border_subtle: f(self.border_subtle),
            frame_bg: f(self.frame_bg),
            frame_border: f(self.frame_border),
            input_bg: f(self.input_bg),
            control: f(self.control),
            control_hover: f(self.control_hover),
            spine: f(self.spine),
            critical: f(self.critical),
            positive: f(self.positive),
            bg_code: f(self.bg_code),
            spoiler: f(self.spoiler),
            spoiler_revealed: f(self.spoiler_revealed),
            text_normal: f(self.text_normal),
            text_muted: f(self.text_muted),
            chat_muted: f(self.chat_muted),
            header_primary: f(self.header_primary),
            header_secondary: f(self.header_secondary),
            channel_default: f(self.channel_default),
            channel_muted: f(self.channel_muted),
            interactive_hover: f(self.interactive_hover),
            link: f(self.link),
            mention_bg: f(self.mention_bg),
            mention_fg: f(self.mention_fg),
            highlight_bg: f(self.highlight_bg),
            highlight_hover: f(self.highlight_hover),
            mention_bar: f(self.mention_bar),
            dark: self.dark,
        }
    }
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
static PALETTES: [Palette; 4] = [LIGHT, ASH, DARK, ONYX];
static CURRENT: std::sync::atomic::AtomicPtr<Palette> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
static GRADIENT: std::sync::atomic::AtomicPtr<Preset> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
static LOOK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static BUILT: std::sync::Mutex<Vec<(u64, &'static Palette)>> = std::sync::Mutex::new(Vec::new());
static SATURATION: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(100);
static SATURATE_CUSTOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One Nitro colour theme: the stops of its background gradient (percent along the line, at the css angle).
pub struct Preset {
    pub name: &'static str,
    pub light: bool,
    pub angle: f32,
    pub veil: Option<f32>,
    pub stops: &'static [(u32, f32)],
}

pub static PRESETS: [Preset; 22] = [
    Preset { name: "Mint Apple", light: true, angle: 180.0, veil: None, stops: &[(0x56b69f, 6.15), (0x63bc61, 48.7), (0x9eca67, 93.07)] },
    Preset { name: "Citrus Sherbert", light: true, angle: 180.0, veil: None, stops: &[(0xf3b336, 31.1), (0xee8558, 67.09)] },
    Preset { name: "Retro Raincloud", light: true, angle: 148.71, veil: None, stops: &[(0x3a7ca1, 5.64), (0x7f7eb9, 26.38), (0x7f7eb9, 49.92), (0x3a7ca1, 73.12)] },
    Preset { name: "Hanami", light: true, angle: 38.08, veil: None, stops: &[(0xefaab3, 3.56), (0xefd696, 35.49), (0xa6daa2, 68.78)] },
    Preset { name: "Sunrise", light: true, angle: 154.19, veil: None, stops: &[(0x9f4175, 8.62), (0xc49064, 48.07), (0xa6953d, 76.04)] },
    Preset { name: "Cotton Candy", light: true, angle: 180.14, veil: None, stops: &[(0xf4abb8, 8.5), (0xb1c2fc, 94.28)] },
    Preset { name: "LoFi Vibes", light: true, angle: 179.52, veil: None, stops: &[(0xa4c0f7, 7.08), (0xa9e4e8, 34.94), (0xb0e2b8, 65.12), (0xcfdfa2, 96.23)] },
    Preset { name: "Desert Khaki", light: true, angle: 38.99, veil: None, stops: &[(0xe7dbd0, 12.92), (0xdfd0b2, 32.92), (0xe0d6a3, 52.11)] },
    Preset { name: "Sunset", light: false, angle: 141.68, veil: None, stops: &[(0x48288c, 27.57), (0xdb7f4b, 71.25)] },
    Preset { name: "Chroma Glow", light: false, angle: 128.92, veil: None, stops: &[(0x0eb5bf, 3.94), (0x4c0ce0, 26.1), (0xa308a7, 39.82), (0x9a53ff, 56.89), (0x218be0, 76.45)] },
    Preset { name: "Forest", light: false, angle: 162.27, veil: None, stops: &[(0x142215, 11.2), (0x2d4d39, 29.93), (0x454c32, 48.64), (0x5a7c58, 67.85), (0xa98e4b, 83.54)] },
    Preset { name: "Crimson Moon", light: false, angle: 64.92, veil: None, stops: &[(0x950909, 16.17), (0x000000, 72.0)] },
    Preset { name: "Midnight Blurple", light: false, angle: 48.17, veil: None, stops: &[(0x5348ca, 11.21), (0x140730, 61.92)] },
    Preset { name: "Mars", light: false, angle: 170.82, veil: None, stops: &[(0x895240, 14.61), (0x8f4343, 74.62)] },
    Preset { name: "Dusk", light: false, angle: 180.0, veil: None, stops: &[(0x665069, 12.84), (0x91a3d1, 85.99)] },
    Preset { name: "Under the Sea", light: false, angle: 179.14, veil: None, stops: &[(0x647962, 1.91), (0x588575, 48.99), (0x6a8482, 96.35)] },
    Preset { name: "Retro Storm", light: false, angle: 148.71, veil: None, stops: &[(0x3a7ca1, 5.64), (0x58579a, 26.38), (0x58579a, 49.92), (0x3a7ca1, 73.12)] },
    Preset { name: "Neon Nights", light: false, angle: 180.0, veil: None, stops: &[(0x01a89e, 0.0), (0x7d60ba, 50.0), (0xb43898, 100.0)] },
    Preset { name: "Strawberry Lemonade", light: false, angle: 161.03, veil: None, stops: &[(0xaf1a6c, 18.79), (0xc26b20, 49.76), (0xe7a525, 80.72)] },
    Preset { name: "Aurora", light: false, angle: 239.16, veil: None, stops: &[(0x062053, 10.39), (0x191fbb, 26.87), (0x13929a, 48.31), (0x218573, 64.98), (0x051a81, 92.5)] },
    Preset { name: "Sepia", light: false, angle: 69.98, veil: None, stops: &[(0x857664, 14.14), (0x5b4421, 60.35)] },
    Preset { name: "Blurple Twilight", light: false, angle: 47.61, veil: None, stops: &[(0x2c3fe7, 11.18), (0x261d83, 64.54)] },
];

/// Display options the renderers read without an `App` in reach (markdown,
/// emoji urls). Set from settings in `App::apply_settings`.
#[derive(Clone, Copy)]
pub struct View {
    pub underline_links: bool,
    pub animate_emoji: bool,
    pub hide_invites: bool,
    pub hide_personal: bool,
    pub name_styles: bool,
    pub own_profiles: bool,
    pub switch_icons: bool,
    pub density: crate::cache::Density,
}

const VIEW_DEFAULT: View = View {
    underline_links: false,
    animate_emoji: true,
    hide_invites: false,
    hide_personal: false,
    name_styles: true,
    own_profiles: false,
    switch_icons: false,
    density: crate::cache::Density::Default,
};

/// A list row's height after the density setting: 4px tighter when compact, 4 roomier when spacious.
pub fn row(h: f32) -> f32 {
    match view().density {
        crate::cache::Density::Compact => h - 4.0,
        crate::cache::Density::Default => h,
        crate::cache::Density::Spacious => h + 4.0,
    }
}
static VIEW: std::sync::RwLock<View> = std::sync::RwLock::new(VIEW_DEFAULT);

pub fn view() -> View {
    VIEW.read().map(|v| *v).unwrap_or(VIEW_DEFAULT)
}

pub fn set_view(v: View) {
    if let Ok(mut cur) = VIEW.write() {
        *cur = v;
    }
}

static ME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_me(id: u64) {
    ME.store(id, std::sync::atomic::Ordering::Relaxed);
}

pub fn me() -> u64 {
    ME.load(std::sync::atomic::Ordering::Relaxed)
}

static REDUCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_reduced_motion(on: bool) {
    REDUCED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Animation length, or zero when reduced motion is on.
pub fn anim(secs: f32) -> f32 {
    if REDUCED.load(std::sync::atomic::Ordering::Relaxed) { 0.0 } else { secs }
}

/// A palette by theme, whatever the current one is: 0 light, 1 ash, 2 dark, 3 onyx.
pub fn palette(i: usize) -> &'static Palette {
    &PALETTES[i]
}

pub fn pal() -> &'static Palette {
    let p = CURRENT.load(std::sync::atomic::Ordering::Relaxed);
    if p.is_null() { &PALETTES[1] } else { unsafe { &*p } }
}

/// A 1px css border as Chromium draws it: whole device pixels, rounded down, never a blur.
pub fn hairline(ppp: f32, color: Color32) -> Stroke {
    Stroke::new(ppp.floor().max(1.0) / ppp, color)
}

/// Onyx draws a few hairlines the other themes leave out.
pub fn onyx() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 3 && GRADIENT.load(std::sync::atomic::Ordering::Relaxed).is_null()
}

/// The wash and outline discord gives every text field.
pub fn input_frame() -> egui::Frame {
    egui::Frame::new().fill(pal().input_bg).stroke(Stroke::new(1.0, pal().border_normal))
}

/// What the palette is built from: the base theme, a gradient preset over it, how far colours are
/// pulled toward grey, and whether contrast is raised.
pub struct Look {
    pub theme: Theme,
    pub fill: Option<Fill>,
    pub saturation: f32,
    pub saturate_custom: bool,
    pub contrast: bool,
}

/// Switch the live palette to `look` (`Sync` follows the system's light/dark setting).
pub fn set_look(ctx: &egui::Context, look: &Look) {
    use std::sync::atomic::Ordering::Relaxed;
    let gradient = look.fill.as_ref().map(Fill::resolve);
    let idx = match (gradient, look.theme) {
        (Some(g), _) => {
            if g.light {
                0
            } else {
                2
            }
        }
        (None, Theme::Light) => 0,
        (None, Theme::Ash) => 1,
        (None, Theme::Dark) => 2,
        (None, Theme::Onyx) => 3,
        (None, Theme::Sync) => match ctx.system_theme() {
            Some(egui::Theme::Light) => 0,
            _ => 1,
        },
    };
    let sat = (look.saturation.clamp(0.0, 1.0) * 100.0).round() as u8;
    let at = gradient.map_or(0, |g| g as *const Preset as u64);
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (idx, at, sat, look.contrast, look.saturate_custom).hash(&mut h);
        h.finish()
    };
    if LOOK.swap(key, Relaxed) == key {
        return;
    }
    THEME.store(idx, Relaxed);
    GRADIENT.store(gradient.map_or(std::ptr::null_mut(), |g| g as *const Preset as *mut Preset), Relaxed);
    SATURATION.store(sat, Relaxed);
    SATURATE_CUSTOM.store(look.saturate_custom, Relaxed);
    let palette = {
        let mut built = BUILT.lock().unwrap();
        match built.iter().find(|(k, _)| *k == key) {
            Some((_, p)) => *p,
            None => {
                let p: &'static Palette = Box::leak(Box::new(build(idx as usize, gradient, sat, look.contrast)));
                built.push((key, p));
                p
            }
        }
    };
    CURRENT.store(palette as *const Palette as *mut Palette, Relaxed);
    style(ctx);
}

fn build(base: usize, gradient: Option<&Preset>, sat: u8, contrast: bool) -> Palette {
    let mut p = PALETTES[base];
    if let Some(g) = gradient {
        let (floor, rail, chat) = match g.veil {
            Some(m) if p.dark => (0.54 + 0.36 * m, 0.51 + 0.34 * m, 0.48 + 0.32 * m),
            Some(m) => (0.51 + 0.34 * m, 0.48 + 0.32 * m, 0.45 + 0.30 * m),
            None if p.dark => (0.84, 0.72, 0.62),
            None => (0.78, 0.66, 0.56),
        };
        p.bg_tertiary = veil(p.bg_tertiary, floor);
        p.frame_bg = veil(p.frame_bg, floor);
        p.bg_secondary_alt = veil(p.bg_secondary_alt, rail);
        p.bg_primary = veil(p.bg_primary, chat);
    }
    if contrast {
        let (ink, soft) = if p.dark { (0xffffff, 0xe6e6ea) } else { (0x000000, 0x1a1a1e) };
        p.text_normal = rgb(ink);
        p.header_primary = rgb(ink);
        p.header_secondary = rgb(soft);
        p.interactive_hover = rgb(ink);
        p.text_muted = rgb(soft);
        p.chat_muted = rgb(soft);
        p.channel_default = rgb(soft);
        p.channel_muted = if p.dark { rgb(0x96979e) } else { rgb(0x595a63) };
        p.link = if p.dark { rgb(0x8cc2ff) } else { rgb(0x0047a8) };
        p.border_muted = tint(OVERLAY, 500);
        p.border_normal = tint(OVERLAY, 800);
        p.border_subtle = tint(OVERLAY, 650);
        p.frame_border = tint(OVERLAY, 650);
        p.bg_accent = mix(OVERLAY, 500, if p.dark { 0x1a1a1e } else { 0xfbfbfb });
        p.control = tint(OVERLAY, 200);
        p.control_hover = tint(OVERLAY, 300);
    }
    if sat < 100 {
        let k = sat as f32 / 100.0;
        p = p.map(|c| desaturate(c, k));
    }
    p
}

/// `c` made see-through so the gradient behind the window shows, `a` of it staying.
fn veil(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8)
}

fn desaturate(c: Color32, k: f32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut h = egui::ecolor::Hsva::from_srgba_unmultiplied([r, g, b, 255]);
    h.s *= k;
    let [r, g, b, _] = h.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The colour `t` (0 to 1) along a preset's gradient.
pub fn gradient_at(preset: &Preset, t: f32) -> Color32 {
    let k = SATURATION.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0;
    let stops = preset.stops;
    let t = t.clamp(0.0, 1.0) * 100.0;
    let col = |c: u32| desaturate(rgb(c), k);
    let i = stops.iter().position(|(_, s)| *s >= t).unwrap_or(stops.len() - 1);
    if i == 0 {
        return col(stops[0].0);
    }
    let (a, b) = (stops[i - 1], stops[i]);
    col(a.0).lerp_to_gamma(col(b.0), ((t - a.1) / (b.1 - a.1).max(0.0001)).clamp(0.0, 1.0))
}

/// The gradient behind the window, drawn before anything else; nothing on a plain theme.
pub fn backdrop(ctx: &egui::Context) {
    let ptr = GRADIENT.load(std::sync::atomic::Ordering::Relaxed);
    let Some(preset) = (unsafe { ptr.as_ref() }) else { return };
    let at = (ptr as usize, SATURATION.load(std::sync::atomic::Ordering::Relaxed));
    let screen = ctx.content_rect();
    let id = egui::Id::new("backdrop");
    let kept = ctx.data(|d| d.get_temp::<((usize, u8), egui::Rect, std::sync::Arc<egui::Mesh>)>(id)).filter(|k| k.0 == at && k.1 == screen);
    let mesh = kept.map_or_else(
        || {
            let mesh = std::sync::Arc::new(gradient_mesh(preset, screen));
            ctx.data_mut(|d| d.insert_temp(id, (at, screen, mesh.clone())));
            mesh
        },
        |k| k.2,
    );
    ctx.layer_painter(egui::LayerId::background()).add(egui::Shape::mesh(mesh));
}

pub fn gradient_mesh(preset: &Preset, r: egui::Rect) -> egui::Mesh {
    let rad = preset.angle.to_radians();
    let dir = egui::vec2(rad.sin(), -rad.cos());
    let half = (r.width() * dir.x.abs() + r.height() * dir.y.abs()) / 2.0;
    let (cols, rows) = (24u32, 14u32);
    let mut mesh = egui::Mesh::default();
    for j in 0..=rows {
        for i in 0..=cols {
            let pos = egui::pos2(r.left() + r.width() * i as f32 / cols as f32, r.top() + r.height() * j as f32 / rows as f32);
            let t = (pos - r.center()).dot(dir) / half.max(1.0) / 2.0 + 0.5;
            mesh.colored_vertex(pos, gradient_at(preset, t));
        }
    }
    for j in 0..rows {
        for i in 0..cols {
            let a = j * (cols + 1) + i;
            mesh.add_triangle(a, a + 1, a + cols + 1);
            mesh.add_triangle(a + 1, a + cols + 2, a + cols + 1);
        }
    }
    mesh
}

/// A theme swatch: the gradient inside a rounded rectangle, as a fan from the middle.
pub fn rounded_gradient(preset: &Preset, r: egui::Rect, radius: f32) -> egui::Mesh {
    let rad = preset.angle.to_radians();
    let dir = egui::vec2(rad.sin(), -rad.cos());
    let half = (r.width() * dir.x.abs() + r.height() * dir.y.abs()) / 2.0;
    let colour = |p: egui::Pos2| gradient_at(preset, (p - r.center()).dot(dir) / half.max(1.0) / 2.0 + 0.5);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.center(), colour(r.center()));
    let corners = [(r.right_top(), -1.0, 1.0, 270.0), (r.right_bottom(), -1.0, -1.0, 0.0), (r.left_bottom(), 1.0, -1.0, 90.0), (r.left_top(), 1.0, 1.0, 180.0)];
    for (corner, sx, sy, start) in corners {
        let centre = corner + egui::vec2(sx * radius, sy * radius);
        for k in 0..=6 {
            let a = (start + 15.0 * k as f32).to_radians();
            let p = centre + egui::vec2(a.cos(), a.sin()) * radius;
            mesh.colored_vertex(p, colour(p));
        }
    }
    let n = mesh.vertices.len() as u32 - 1;
    for i in 1..=n {
        mesh.add_triangle(0, i, if i == n { 1 } else { i + 1 });
    }
    mesh
}

/// `c` with the see-through taken out, for surfaces that must hide what's behind them.
pub fn solid(c: Color32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgb(r, g, b)
}

/// The colour the window is cleared to before anything draws.
pub fn clear() -> Color32 {
    if GRADIENT.load(std::sync::atomic::Ordering::Relaxed).is_null() { pal().bg_tertiary } else { Color32::BLACK }
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

pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-bold".into()))
}

pub fn italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("italic".into()))
}

pub fn bold_italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold-italic".into()))
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
    use std::sync::atomic::Ordering::Relaxed;
    let c = Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    match SATURATION.load(Relaxed) {
        k if k < 100 && SATURATE_CUSTOM.load(Relaxed) => desaturate(c, k as f32 / 100.0),
        _ => c,
    }
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
    // discord's own faces once they've been downloaded, see fonts.rs
    let have: Vec<&str> = crate::fonts::FACES
        .iter()
        .filter_map(|(stem, ..)| {
            f.font_data.insert(stem.to_string(), Arc::new(FontData::from_static(crate::fonts::cached(stem)?)));
            Some(*stem)
        })
        .collect();
    let gg = |stem: &'static str, or: Option<&'static str>| if have.contains(&stem) { Some(stem) } else { or };
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
    let mut add = |key: &str, file: &str| -> bool {
        match crate::fonts::mapped(std::path::Path::new(&format!("{dir}{file}"))) {
            Some(b) => {
                f.font_data.insert(key.into(), Arc::new(FontData::from_static(b)));
                true
            }
            None => false,
        }
    };
    let ui = add("ui", "segoeui.ttf");
    let semi = add("semi", "seguisb.ttf");
    let bold = add("bold", "segoeuib.ttf");
    let ui_i = gg("ggsans-400i", None).is_none() && add("ui-i", "segoeuii.ttf");
    let semi_i = gg("ggsans-600i", None).is_none() && add("semi-i", "seguisbi.ttf");
    let bold_i = gg("ggsans-700i", None).is_none() && add("bold-i", "segoeuiz.ttf");
    let sym = add("sym", "seguisym.ttf");
    let code = add("code", "consola.ttf");
    // last resort for pictographs the ui fonts lack; egui draws its monochrome outlines
    let emoji = add("emoji", "seguiemj.ttf");
    // channel names lean on things like "・" and "┃", and plenty of people write CJK
    let yahei = cjk && add("cjk", "msyh.ttc");
    let hangul = cjk && add("hangul", "malgun.ttf");

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
    for (fam, first) in [
        ("medium", gg("ggsans-500", None)),
        ("semi", gg("ggsans-600", semi.then_some("semi"))),
        ("bold", gg("ggsans-700", bold.then_some("bold"))),
        ("italic", gg("ggsans-400i", ui_i.then_some("ui-i"))),
        ("semi-italic", gg("ggsans-600i", semi_i.then_some("semi-i"))),
        ("bold-italic", gg("ggsans-700i", bold_i.then_some("bold-i"))),
    ] {
        f.families.insert(FontFamily::Name(fam.into()), chain(first));
    }
    f.families.insert(FontFamily::Proportional, chain(gg("ggsans-400", None)));
    // every name font family exists from the start; until its file arrives it's just gg sans
    for (id, ..) in crate::fonts::NAME_FONTS {
        let key = format!("name{id}");
        let have = crate::fonts::cached_name(id).map(|b| f.font_data.insert(key.clone(), Arc::new(FontData::from_static(b)))).is_some();
        let fam = chain(if have { Some(key.as_str()) } else { gg("ggsans-500", None) });
        f.families.insert(FontFamily::Name(key.into()), fam);
    }
    let m = f.families.get_mut(&FontFamily::Monospace).unwrap();
    if code {
        m.insert(0, "code".into());
    }
    if let Some(gm) = gg("ggmono-400", None) {
        m.insert(0, gm.into());
    }
    let mut mb = m.clone();
    if let Some(gm) = gg("ggmono-700", None) {
        mb.insert(0, gm.into());
    }
    f.families.insert(FontFamily::Name("mono-bold".into()), mb);
    f
}

/// A nitro display name in its font, or gg sans medium when it has none.
pub fn name_font(font: u8, size: f32) -> FontId {
    if view().name_styles && crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == font) {
        FontId::new(size, FontFamily::Name(format!("name{font}").into()))
    } else {
        medium(size)
    }
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts(false));
    style(ctx);
}

/// --scrollbar-thin-thumb and --scrollbar-auto-thumb: light, ash, dark, onyx
pub fn thumb(auto: bool) -> Color32 {
    const THUMBS: [[u32; 2]; 4] = [[0x8c8c94, 0x8f8f96], [0x757880, 0x7d7d87], [0x5e616b, 0x666670], [0x595963, 0x595963]];
    rgb(THUMBS[THEME.load(std::sync::atomic::Ordering::Relaxed) as usize][auto as usize])
}

/// Shows `add` with the look of discord's `auto` scrollers (the chat, the friends list): an 8px thumb
/// 4px in from the edge, where the thin ones have a 4px thumb 2px in.
pub fn auto_scroll<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let prev = ui.style().clone();
    let st = ui.style_mut();
    st.spacing.scroll.bar_width = 8.0;
    st.spacing.scroll.floating_width = 8.0;
    st.spacing.scroll.bar_outer_margin = 4.0;
    for w in [&mut st.visuals.widgets.inactive, &mut st.visuals.widgets.hovered, &mut st.visuals.widgets.active] {
        w.bg_fill = thumb(true);
    }
    let out = add(ui);
    ui.set_style(prev);
    out
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
    style.spacing.default_area_size = egui::vec2(600.0, 4096.0);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 4.0,
        bar_outer_margin: 2.0,
        floating: true,
        floating_allocated_width: 0.0,
        // thin() paints the track at full opacity even at rest; discord has no track and hides the thumb until hover or scroll
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 0.0,
        active_handle_opacity: 1.0,
        interact_handle_opacity: 1.0,
        floating_width: 4.0,
        handle_min_length: 40.0,
        ..egui::style::ScrollStyle::thin()
    };
    // egui paints a scroll handle in the widget's bg_fill; discord's is one flat colour whatever the pointer does
    for w in [&mut style.visuals.widgets.inactive, &mut style.visuals.widgets.hovered, &mut style.visuals.widgets.active] {
        w.bg_fill = thumb(false);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_saturation_is_grey() {
        let p = build(1, None, 0, false);
        for c in [p.link, p.positive, p.mention_fg, p.critical] {
            assert!(c.r().abs_diff(c.g()) <= 1 && c.g().abs_diff(c.b()) <= 1, "{c:?}");
        }
    }

    #[test]
    fn gradient_lets_the_backdrop_through() {
        let p = build(2, Some(&PRESETS[8]), 100, false);
        assert!(p.bg_primary.a() < 255 && p.bg_tertiary.a() < 255);
        assert_eq!(p.bg_floating.a(), 255);
    }

    #[test]
    fn contrast_goes_to_the_extremes() {
        assert_eq!(build(1, None, 100, true).text_normal, Color32::WHITE);
        assert_eq!(build(0, None, 100, true).text_normal, Color32::BLACK);
    }

    #[test]
    fn gradients_run_stop_to_stop() {
        let p = &PRESETS[11];
        assert_eq!(gradient_at(p, 0.0), rgb(0x950909));
        assert_eq!(gradient_at(p, 1.0), rgb(0x000000));
    }
}
