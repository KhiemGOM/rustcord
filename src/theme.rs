use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

/// The colours that change with the theme. Brand colours (blurple, status
/// colours) stay put across themes, like they do in the official client.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg_tertiary: Color32,
    pub bg_secondary: Color32,
    pub bg_secondary_alt: Color32,
    pub bg_primary: Color32,
    pub bg_floating: Color32,
    pub bg_input: Color32,
    pub bg_msg_hover: Color32,
    pub bg_mod_hover: Color32,
    pub bg_mod_selected: Color32,
    pub bg_accent: Color32,
    pub text_normal: Color32,
    pub text_muted: Color32,
    pub header_primary: Color32,
    pub header_secondary: Color32,
    pub channel_default: Color32,
    pub channel_muted: Color32,
    pub interactive_hover: Color32,
    pub link: Color32,
    pub mention_bg: Color32,
    pub mention_fg: Color32,
    pub highlight_bg: Color32,
    pub dark: bool,
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

// the grey everyone knows; discord calls it "ash" since the 2025 refresh
const ASH: Palette = Palette {
    bg_tertiary: rgb(0x1e1f22),
    bg_secondary: rgb(0x2b2d31),
    bg_secondary_alt: rgb(0x232428),
    bg_primary: rgb(0x313338),
    bg_floating: rgb(0x111214),
    bg_input: rgb(0x383a40),
    bg_msg_hover: rgb(0x2e3035),
    bg_mod_hover: rgb(0x35373c),
    bg_mod_selected: rgb(0x404249),
    bg_accent: rgb(0x4e5058),
    text_normal: rgb(0xdbdee1),
    text_muted: rgb(0x949ba4),
    header_primary: rgb(0xf2f3f5),
    header_secondary: rgb(0xb5bac1),
    channel_default: rgb(0x80848e),
    channel_muted: rgb(0x4e5058),
    interactive_hover: rgb(0xdbdee1),
    link: rgb(0x00a8fc),
    mention_bg: Color32::from_rgba_premultiplied(26, 30, 73, 76),
    mention_fg: rgb(0xc9cdfb),
    highlight_bg: Color32::from_rgba_premultiplied(24, 18, 5, 26),
    dark: true,
};

const DARK: Palette = Palette {
    bg_tertiary: rgb(0x0f0f11),
    bg_secondary: rgb(0x121214),
    bg_secondary_alt: rgb(0x17171a),
    bg_primary: rgb(0x1a1a1e),
    bg_floating: rgb(0x111113),
    bg_input: rgb(0x222327),
    bg_msg_hover: rgb(0x1e1e22),
    bg_mod_hover: rgb(0x202024),
    bg_mod_selected: rgb(0x2b2b30),
    bg_accent: rgb(0x3a3a41),
    channel_muted: rgb(0x3f4047),
    ..ASH
};

const ONYX: Palette = Palette {
    bg_tertiary: rgb(0x000000),
    bg_secondary: rgb(0x070709),
    bg_secondary_alt: rgb(0x0b0b0d),
    bg_primary: rgb(0x0c0c0e),
    bg_floating: rgb(0x0c0c0e),
    bg_input: rgb(0x18181b),
    bg_msg_hover: rgb(0x121214),
    bg_mod_hover: rgb(0x151518),
    bg_mod_selected: rgb(0x1f1f23),
    bg_accent: rgb(0x2c2c31),
    channel_muted: rgb(0x36373d),
    ..ASH
};

const LIGHT: Palette = Palette {
    bg_tertiary: rgb(0xe3e5e8),
    bg_secondary: rgb(0xf2f3f5),
    bg_secondary_alt: rgb(0xebedef),
    bg_primary: rgb(0xffffff),
    bg_floating: rgb(0xffffff),
    bg_input: rgb(0xebedef),
    bg_msg_hover: rgb(0xf6f6f7),
    bg_mod_hover: rgb(0xe0e1e5),
    bg_mod_selected: rgb(0xd4d7dc),
    bg_accent: rgb(0xc4c9ce),
    text_normal: rgb(0x313338),
    text_muted: rgb(0x5c5e66),
    header_primary: rgb(0x060607),
    header_secondary: rgb(0x4e5058),
    channel_default: rgb(0x5c5e66),
    channel_muted: rgb(0xa3a6aa),
    interactive_hover: rgb(0x2e3338),
    link: rgb(0x006ce7),
    mention_bg: Color32::from_rgba_premultiplied(13, 15, 36, 38),
    mention_fg: rgb(0x505cdc),
    highlight_bg: Color32::from_rgba_premultiplied(25, 17, 3, 26),
    dark: false,
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
    let prop = chain(None);
    let s = chain(semi.then_some("semi"));
    let b = chain(bold.then_some("bold"));
    f.families.insert(FontFamily::Proportional, prop);
    f.families.insert(FontFamily::Name("semi".into()), s);
    f.families.insert(FontFamily::Name("bold".into()), b);
    if code {
        f.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "code".into());
    }
    f
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
    v.window_stroke = Stroke::NONE;
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 4.into();
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if p.dark { 90 } else { 40 }) };
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

    style.spacing.item_spacing = egui::vec2(8.0, 0.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 8.0,
        floating: true,
        floating_allocated_width: 0.0,
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
