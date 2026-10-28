//! What a Vencord theme can do to Fastcord: its colour variables. Themes are Discord CSS; the
//! custom properties they set (`--background-primary: #123`) map onto Fastcord's palette, and
//! everything else in them is about Discord's own markup.

use egui::Color32;
use std::collections::HashMap;

/// Every custom property set outside the other theme's blocks, later ones winning, with
/// `var()` references between them followed.
pub fn vars(css: &str, dark: bool) -> HashMap<String, String> {
    let css = strip_comments(css);
    let mut out = HashMap::new();
    let other = if dark { "theme-light" } else { "theme-dark" };
    let mut rest = css.as_str();
    while let Some(open) = rest.find('{') {
        let selector = &rest[..open];
        let Some(close) = rest[open..].find('}') else { break };
        let body = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        // an @media or similar wrapper opens a block inside the selector text; its rules follow
        let selector = selector.rsplit(['}', ';']).next().unwrap_or(selector);
        if selector.contains(other) {
            continue;
        }
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                let k = k.trim();
                if k.starts_with("--") {
                    out.insert(k.to_string(), v.trim().trim_end_matches("!important").trim().to_string());
                }
            }
        }
    }
    out
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i + 2..].find("*/").map_or("", |j| &rest[i + 2 + j + 2..]);
    }
    out.push_str(rest);
    out
}

/// A CSS colour, following `var()`s through `vars`.
pub fn color(v: &str, vars: &HashMap<String, String>, depth: u8) -> Option<Color32> {
    let v = v.trim();
    if depth > 8 {
        return None;
    }
    if let Some(inner) = v.strip_prefix("var(").and_then(|x| x.strip_suffix(')')) {
        let (name, fallback) = inner.split_once(',').map_or((inner, None), |(a, b)| (a, Some(b)));
        return vars.get(name.trim()).and_then(|x| color(x, vars, depth + 1)).or_else(|| fallback.and_then(|f| color(f, vars, depth + 1)));
    }
    if let Some(hex) = v.strip_prefix('#') {
        let hex: String = if hex.len() <= 4 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
        let n = u32::from_str_radix(hex.get(..6)?, 16).ok()?;
        let a = hex.get(6..8).and_then(|a| u8::from_str_radix(a, 16).ok()).unwrap_or(255);
        return Some(Color32::from_rgba_unmultiplied((n >> 16) as u8, (n >> 8) as u8, n as u8, a));
    }
    let (f, args) = v.split_once('(')?;
    let args: Vec<&str> = args.trim_end_matches(')').split([',', ' ', '/']).filter(|x| !x.trim().is_empty()).collect();
    let num = |s: &str, scale: f32| -> Option<f32> {
        let s = s.trim();
        match s.strip_suffix('%') {
            Some(p) => p.parse::<f32>().ok().map(|p| p / 100.0 * scale),
            None => s.trim_end_matches("deg").parse().ok(),
        }
    };
    let alpha = |i: usize| args.get(i).and_then(|a| num(a, 1.0)).map_or(255, |a| (a.clamp(0.0, 1.0) * 255.0) as u8);
    match f.trim() {
        "rgb" | "rgba" => Some(Color32::from_rgba_unmultiplied(num(args.first()?, 255.0)? as u8, num(args.get(1)?, 255.0)? as u8, num(args.get(2)?, 255.0)? as u8, alpha(3))),
        "hsl" | "hsla" => {
            let (h, s, l) = (num(args.first()?, 1.0)?, num(args.get(1)?, 1.0)?, num(args.get(2)?, 1.0)?);
            let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
            let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
            let (r, g, b) = match ((h.rem_euclid(360.0)) / 60.0) as u32 {
                0 => (c, x, 0.0),
                1 => (x, c, 0.0),
                2 => (0.0, c, x),
                3 => (0.0, x, c),
                4 => (x, 0.0, c),
                _ => (c, 0.0, x),
            };
            let m = l - c / 2.0;
            let u = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
            Some(Color32::from_rgba_unmultiplied(u(r), u(g), u(b), alpha(3)))
        }
        _ => None,
    }
}

/// Discord's variables for each of Fastcord's palette colours, newest names first.
pub const MAP: &[(&str, &[&str])] = &[
    ("bg_tertiary", &["--background-base-lowest", "--background-tertiary", "--background-secondary"]),
    ("bg_secondary", &["--background-surface-high", "--background-secondary"]),
    ("bg_secondary_alt", &["--background-base-low", "--background-secondary-alt"]),
    ("bg_primary", &["--background-base-lower", "--background-primary"]),
    ("bg_floating", &["--background-surface-higher", "--background-floating"]),
    ("bg_input", &["--chat-background-default", "--channeltextarea-background"]),
    ("bg_msg_hover", &["--message-background-hover", "--background-message-hover"]),
    ("bg_mod_hover", &["--interactive-background-hover", "--background-modifier-hover"]),
    ("bg_mod_selected", &["--interactive-background-selected", "--background-modifier-selected"]),
    ("border_muted", &["--border-muted"]),
    ("border_normal", &["--border-normal"]),
    ("border_subtle", &["--border-subtle"]),
    ("frame_bg", &["--app-frame-background"]),
    ("frame_border", &["--app-frame-border"]),
    ("input_bg", &["--input-background-default", "--input-background"]),
    ("control", &["--control-secondary-background-default"]),
    ("control_hover", &["--control-secondary-background-hover"]),
    ("bg_code", &["--background-code"]),
    ("text_normal", &["--text-default", "--text-normal"]),
    ("text_muted", &["--text-muted"]),
    ("chat_muted", &["--chat-text-muted"]),
    ("header_primary", &["--text-strong", "--header-primary"]),
    ("header_secondary", &["--interactive-text-default", "--header-secondary", "--interactive-normal"]),
    ("channel_default", &["--channels-default"]),
    ("channel_muted", &["--interactive-muted"]),
    ("interactive_hover", &["--interactive-text-hover", "--interactive-hover"]),
    ("link", &["--text-link"]),
    ("mention_bg", &["--mention-background"]),
    ("mention_fg", &["--mention-foreground"]),
    ("highlight_bg", &["--message-mentioned-background-default", "--background-mentioned"]),
    ("highlight_hover", &["--message-mentioned-background-hover", "--background-mentioned-hover"]),
];

/// The palette colours a theme's CSS sets, by field name.
pub fn palette(css: &str, dark: bool) -> Vec<(&'static str, Color32)> {
    let vars = vars(css, dark);
    MAP.iter().filter_map(|(field, names)| names.iter().find_map(|n| vars.get(*n).and_then(|v| color(v, &vars, 0))).map(|c| (*field, c))).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_theme_colours() {
        let css = "/* a theme */ @import url('x.css');\n:root { --accent: #ff0000; --background-primary: var(--accent); }\n.theme-dark { --text-normal: rgb(10, 20, 30); --background-tertiary: hsl(120 100% 25%); }\n.theme-light { --text-normal: #fff; }";
        let p = super::palette(css, true);
        let get = |f: &str| p.iter().find(|(k, _)| *k == f).map(|(_, c)| *c);
        assert_eq!(get("bg_primary"), Some(egui::Color32::from_rgb(255, 0, 0)));
        assert_eq!(get("text_normal"), Some(egui::Color32::from_rgb(10, 20, 30)));
        assert_eq!(get("bg_tertiary"), Some(egui::Color32::from_rgb(0, 128, 0)));
    }
}
