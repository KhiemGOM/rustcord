use std::sync::OnceLock;
use std::sync::atomic::Ordering::Relaxed;

use super::*;

pub struct ProfileTheme {
    pub top: Color32,
    pub bottom: Color32,
    pub light: bool,
    pub button: Color32,
}

pub fn profile_theme(primary: u32, secondary: u32) -> ProfileTheme {
    let ch = |c: u32, s: u32| ((c >> s) & 255) as f32;
    let darkness = 1.0 - (0.299 * ch(primary, 16) + 0.587 * ch(primary, 8) + 0.114 * ch(primary, 0)) / 255.0;
    let light = darkness <= 0.5;
    let over = if light { 255.0 } else { 0.0 };
    let veil = |c: u32| {
        let v = |s: u32| (ch(c, s) * 0.4 + over * 0.6).round() as u8;
        Color32::from_rgb(v(16), v(8), v(0))
    };
    ProfileTheme { top: veil(primary), bottom: veil(secondary), light, button: readable_on_white(primary) }
}

fn linear(c: f32) -> f32 {
    let c = c / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn luminance([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn to_lab(c: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(linear);
    let f = |t: f32| if t > (6.0f32 / 29.0).powi(3) { t.cbrt() } else { t / (3.0 * (6.0f32 / 29.0).powi(2)) + 4.0 / 29.0 };
    let x = f((0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047);
    let y = f(0.2126729 * r + 0.7151522 * g + 0.0721750 * b);
    let z = f((0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883);
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

fn from_lab([l, a, b]: [f32; 3]) -> [f32; 3] {
    let inv = |t: f32| if t > 6.0 / 29.0 { t.powi(3) } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let y = inv((l + 16.0) / 116.0);
    let x = 0.95047 * inv((l + 16.0) / 116.0 + a / 500.0);
    let z = 1.08883 * inv((l + 16.0) / 116.0 - b / 200.0);
    let gamma = |c: f32| (255.0 * if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }).clamp(0.0, 255.0);
    [gamma(3.2404542 * x - 1.5371385 * y - 0.4985314 * z), gamma(-0.9692660 * x + 1.8760108 * y + 0.0415560 * z), gamma(0.0556434 * x - 0.2040259 * y + 1.0572252 * z)]
}

fn readable_on_white(c: u32) -> Color32 {
    let mut cur = [(c >> 16) & 255, (c >> 8) & 255, c & 255].map(|v| v as f32);
    let contrast = |c: [f32; 3]| 1.05 / (luminance(c) + 0.05);
    for _ in 0..100 {
        let k = contrast(cur);
        if (7.0..=10.0).contains(&k) {
            break;
        }
        let mut lab = to_lab(cur);
        lab[0] += if k < 7.0 { -18.0 } else { 18.0 };
        cur = from_lab(lab);
    }
    let [r, g, b] = cur.map(|v| v.round() as u8);
    Color32::from_rgb(r, g, b)
}

pub fn profile_palette(light: bool) -> &'static Palette {
    static CACHE: OnceLock<[Palette; 2]> = OnceLock::new();
    let both = CACHE.get_or_init(|| {
        let make = |base: Palette, ink: fn(u8) -> Color32| Palette { text_normal: ink(245), header_secondary: ink(224), text_muted: ink(194), ..base };
        [make(DARK, Color32::from_white_alpha), make(LIGHT, Color32::from_black_alpha)]
    });
    &both[light as usize]
}

pub struct Scope(*mut Palette);

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.store(self.0, Relaxed);
    }
}

pub fn scope(p: Option<&'static Palette>) -> Option<Scope> {
    p.map(|p| Scope(CURRENT.swap(p as *const Palette as *mut Palette, Relaxed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_light_primary_makes_a_light_card_under_a_white_veil() {
        let t = profile_theme(0xa79680, 0x000000);
        assert!(t.light);
        assert_eq!(t.top, Color32::from_rgb(220, 213, 204));
        let d = profile_theme(0x1e1f22, 0xffffff);
        assert!(!d.light);
        assert_eq!(d.top, Color32::from_rgb(12, 12, 14));
        assert_eq!(d.bottom, Color32::from_rgb(102, 102, 102));
    }

    #[test]
    fn the_message_button_darkens_the_primary_until_white_reads_on_it() {
        assert_eq!(profile_theme(0xa79680, 0).button, Color32::from_rgb(0x4b, 0x3e, 0x2b));
    }
}
