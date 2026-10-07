use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

pub const BG_TERTIARY: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x22);
pub const BG_SECONDARY: Color32 = Color32::from_rgb(0x2b, 0x2d, 0x31);
pub const BG_SECONDARY_ALT: Color32 = Color32::from_rgb(0x23, 0x24, 0x28);
pub const BG_PRIMARY: Color32 = Color32::from_rgb(0x31, 0x33, 0x38);
pub const BG_FLOATING: Color32 = Color32::from_rgb(0x11, 0x12, 0x14);
pub const BG_INPUT: Color32 = Color32::from_rgb(0x38, 0x3a, 0x40);
pub const BG_MSG_HOVER: Color32 = Color32::from_rgb(0x2e, 0x30, 0x35);
pub const BG_MOD_HOVER: Color32 = Color32::from_rgb(0x35, 0x37, 0x3c);
pub const BG_MOD_SELECTED: Color32 = Color32::from_rgb(0x40, 0x42, 0x49);
pub const BG_ACCENT: Color32 = Color32::from_rgb(0x4e, 0x50, 0x58);

pub const TEXT_NORMAL: Color32 = Color32::from_rgb(0xdb, 0xde, 0xe1);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x94, 0x9b, 0xa4);
pub const HEADER_PRIMARY: Color32 = Color32::from_rgb(0xf2, 0xf3, 0xf5);
pub const HEADER_SECONDARY: Color32 = Color32::from_rgb(0xb5, 0xba, 0xc1);
pub const CHANNEL_DEFAULT: Color32 = Color32::from_rgb(0x80, 0x84, 0x8e);
pub const CHANNEL_MUTED: Color32 = Color32::from_rgb(0x4e, 0x50, 0x58);
pub const INTERACTIVE_HOVER: Color32 = Color32::from_rgb(0xdb, 0xde, 0xe1);
pub const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);

pub const BLURPLE: Color32 = Color32::from_rgb(0x58, 0x65, 0xf2);
pub const RED: Color32 = Color32::from_rgb(0xf2, 0x3f, 0x43);
pub const GREEN: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);
pub const YELLOW: Color32 = Color32::from_rgb(0xf0, 0xb2, 0x32);
pub const GREY: Color32 = Color32::from_rgb(0x80, 0x84, 0x8e);

pub const MENTION_BG: Color32 = Color32::from_rgba_premultiplied(26, 30, 73, 76);
pub const MENTION_FG: Color32 = Color32::from_rgb(0xc9, 0xcd, 0xfb);
pub const HIGHLIGHT_BG: Color32 = Color32::from_rgba_premultiplied(24, 18, 5, 26);

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

pub fn install(ctx: &egui::Context) {
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
    ctx.set_fonts(f);

    let mut style = egui::Style::default();
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.panel_fill = BG_PRIMARY;
    v.window_fill = BG_FLOATING;
    v.extreme_bg_color = BG_TERTIARY;
    v.faint_bg_color = BG_SECONDARY;
    v.hyperlink_color = LINK;
    v.override_text_color = None;
    v.selection.bg_fill = Color32::from_rgb(0x3d, 0x5a, 0xbf);
    v.selection.stroke = Stroke::new(1.0, HEADER_PRIMARY);
    v.window_stroke = Stroke::NONE;
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 4.into();
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(90) };
    v.window_shadow = v.popup_shadow;
    v.text_cursor.stroke = Stroke::new(2.0, TEXT_NORMAL);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 4.into();
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, TEXT_NORMAL);
    }
    v.widgets.noninteractive.bg_fill = BG_SECONDARY;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = BG_INPUT;
    v.widgets.hovered.weak_bg_fill = BG_MOD_HOVER;
    v.widgets.hovered.bg_fill = BG_MOD_HOVER;
    v.widgets.active.weak_bg_fill = BG_MOD_SELECTED;
    v.widgets.active.bg_fill = BG_MOD_SELECTED;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_NORMAL);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, HEADER_SECONDARY);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, INTERACTIVE_HOVER);

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
    style.animation_time = 0.1;
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
