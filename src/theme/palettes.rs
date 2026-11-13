use super::*;

pub(super) const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

// the grey every theme builds its translucent overlays from
pub(super) const OVERLAY: u32 = 0x97979f;
// menus hover with the same wash in every theme
pub(crate) const MENU_HOVER: Color32 = tint(OVERLAY, 122);
pub(crate) const CRITICAL_BG: Color32 = tint(0xd22d39, 78);
pub(crate) const PROFILE_CARD: Color32 = tint(OVERLAY, 161);

/// `fg` at `permille`/1000 opacity laid over `bg`, as one opaque colour.
pub(super) const fn mix(fg: u32, permille: u32, bg: u32) -> Color32 {
    const fn ch(fg: u32, bg: u32, permille: u32, shift: u32) -> u8 {
        let (f, b) = ((fg >> shift) & 0xff, (bg >> shift) & 0xff);
        ((f * permille + b * (1000 - permille) + 500) / 1000) as u8
    }
    Color32::from_rgb(ch(fg, bg, permille, 16), ch(fg, bg, permille, 8), ch(fg, bg, permille, 0))
}

/// `fg` at `permille`/1000 opacity, kept translucent (premultiplied, the way egui wants it).
pub(super) const fn tint(fg: u32, permille: u32) -> Color32 {
    let a = (permille * 255 + 500) / 1000;
    const fn ch(fg: u32, a: u32, shift: u32) -> u8 {
        ((((fg >> shift) & 0xff) * a + 127) / 255) as u8
    }
    Color32::from_rgba_premultiplied(ch(fg, a, 16), ch(fg, a, 8), ch(fg, a, 0), a as u8)
}

// the grey everyone knows; "ash" in the picker, theme-dark in discord's css
pub(super) const ASH: Palette = Palette {
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
    reacted_fg: rgb(0xdee3ff),
    highlight_bg: tint(0xf8a300, 78),
    highlight_hover: tint(0xf8a300, 39),
    mention_bar: rgb(0xfcb529),
    blurple: rgb(0x5865f2),
    red: rgb(0xf23f43),
    green: rgb(0x23a55a),
    yellow: rgb(0xf0b232),
    grey: rgb(0x80848e),
    focus: rgb(0x00a8fc),
    danger: rgb(0xd22d39),
    notice: [rgb(0x622a00), rgb(0xffe7cf)],
    dark: true,
};

// theme-darker
pub(super) const DARK: Palette = Palette {
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
    reacted_fg: rgb(0xbac7ff),
    mention_bar: rgb(0xea9800),
    blurple: rgb(0x5865f2),
    red: rgb(0xf23f43),
    green: rgb(0x23a55a),
    yellow: rgb(0xf0b232),
    grey: rgb(0x80848e),
    focus: rgb(0x00a8fc),
    danger: rgb(0xd22d39),
    notice: [rgb(0x431000), rgb(0xffc356)],
    ..ASH
};

// theme-midnight
pub(super) const ONYX: Palette = Palette {
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
    reacted_fg: rgb(0x99adff),
    mention_bar: rgb(0xce8100),
    blurple: rgb(0x5865f2),
    red: rgb(0xf23f43),
    green: rgb(0x23a55a),
    yellow: rgb(0xf0b232),
    grey: rgb(0x80848e),
    focus: rgb(0x00a8fc),
    danger: rgb(0xd22d39),
    notice: [rgb(0x240000), rgb(0xfaaa00)],
    ..ASH
};

pub(super) const LIGHT: Palette = Palette {
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
    reacted_fg: rgb(0x263378),
    dark: false,
    mention_bar: rgb(0x945300),
    blurple: rgb(0x5865f2),
    red: rgb(0xf23f43),
    green: rgb(0x23a55a),
    yellow: rgb(0xf0b232),
    grey: rgb(0x80848e),
    focus: rgb(0x00a8fc),
    danger: rgb(0xd22d39),
    notice: [rgb(0xffe7cf), rgb(0x5e2700)],
    ..ASH
};
