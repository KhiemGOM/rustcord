//! Text that may contain emoji: channel names, nicknames, server names, titles.
//! Fonts can't do colour emoji, so each one becomes a gap in the galley with a
//! twemoji image painted over it, the same trick the message renderer uses.

use egui::text::{CCursor, LayoutJob, TextFormat, TextWrapping};
use egui::{Align, Align2, Color32, FontId, Galley, Painter, Pos2, Rect, Response, Sense, Ui, pos2, vec2};
use std::sync::Arc;

use crate::markdown::{take_emoji, twemoji};

#[derive(Clone)]
pub struct Rich {
    pub galley: Arc<Galley>,
    emojis: Vec<(usize, String, f32, f32)>,
}

// Unicode 16 emoji-data.txt, Emoji_Presentation
const PRESENTATION: [(u32, u32); 80] = [
    (0x231A, 0x231B), (0x23E9, 0x23EC), (0x23F0, 0x23F0), (0x23F3, 0x23F3), (0x25FD, 0x25FE), (0x2614, 0x2615),
    (0x2648, 0x2653), (0x267F, 0x267F), (0x2693, 0x2693), (0x26A1, 0x26A1), (0x26AA, 0x26AB), (0x26BD, 0x26BE),
    (0x26C4, 0x26C5), (0x26CE, 0x26CE), (0x26D4, 0x26D4), (0x26EA, 0x26EA), (0x26F2, 0x26F3), (0x26F5, 0x26F5),
    (0x26FA, 0x26FA), (0x26FD, 0x26FD), (0x2705, 0x2705), (0x270A, 0x270B), (0x2728, 0x2728), (0x274C, 0x274C),
    (0x274E, 0x274E), (0x2753, 0x2755), (0x2757, 0x2757), (0x2795, 0x2797), (0x27B0, 0x27B0), (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C), (0x2B50, 0x2B50), (0x2B55, 0x2B55), (0x1F004, 0x1F004), (0x1F0CF, 0x1F0CF), (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A), (0x1F1E6, 0x1F1FF), (0x1F201, 0x1F201), (0x1F21A, 0x1F21A), (0x1F22F, 0x1F22F), (0x1F232, 0x1F236),
    (0x1F238, 0x1F23A), (0x1F250, 0x1F251), (0x1F300, 0x1F320), (0x1F32D, 0x1F335), (0x1F337, 0x1F37C), (0x1F37E, 0x1F393),
    (0x1F3A0, 0x1F3CA), (0x1F3CF, 0x1F3D3), (0x1F3E0, 0x1F3F0), (0x1F3F4, 0x1F3F4), (0x1F3F8, 0x1F43E), (0x1F440, 0x1F440),
    (0x1F442, 0x1F4FC), (0x1F4FF, 0x1F53D), (0x1F54B, 0x1F54E), (0x1F550, 0x1F567), (0x1F57A, 0x1F57A), (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A4), (0x1F5FB, 0x1F64F), (0x1F680, 0x1F6C5), (0x1F6CC, 0x1F6CC), (0x1F6D0, 0x1F6D2), (0x1F6D5, 0x1F6D7),
    (0x1F6DC, 0x1F6DF), (0x1F6EB, 0x1F6EC), (0x1F6F4, 0x1F6FC), (0x1F7E0, 0x1F7EB), (0x1F7F0, 0x1F7F0), (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945), (0x1F947, 0x1F9FF), (0x1FA70, 0x1FA7C), (0x1FA80, 0x1FA89), (0x1FA8F, 0x1FAC6), (0x1FACE, 0x1FADC),
    (0x1FADF, 0x1FAE9), (0x1FAF0, 0x1FAF8),
];

fn text_style(cluster: &str) -> bool {
    let mut cs = cluster.chars();
    let (Some(c), None) = (cs.next(), cs.next()) else { return false };
    !PRESENTATION.iter().any(|(a, b)| (*a..=*b).contains(&(c as u32)))
}

fn has_emoji(s: &str) -> bool {
    s.chars().any(|c| c as u32 >= 0x2190 || c == '\u{a9}' || c == '\u{ae}')
}

/// Box-drawing characters routed to the `"boxes"` fallback (`malgun.ttf`). Horizontal divider runs
/// (`─`, `━`, `═`, dashed rules) stay in the standard font chain so decorative category headers
/// like `★━━Socialize━━★` render half-width like Discord instead of double-width CJK glyphs.
fn use_boxes_font(text: &str, byte_idx: usize, c: char) -> bool {
    let cp = c as u32;
    if !(0x2500..=0x257F).contains(&cp) {
        return false;
    }
    if matches!(cp, 0x2500 | 0x2501 | 0x2504..=0x2509 | 0x254C..=0x2550) {
        return false;
    }
    let after_symbol = text[..byte_idx].chars().next_back().is_some_and(|p| {
        let ps = p.to_string();
        (0x2600..=0x27BF).contains(&(p as u32)) || (take_emoji(&ps).is_some() && text_style(&ps))
    });
    !after_symbol
}

pub fn layout(ctx: &egui::Context, text: &str, font: FontId, color: Color32, max_w: f32) -> Rich {
    layout_em(ctx, text, font, color, max_w, 1.25)
}

/// `em` is the emoji size relative to the font size. The sidebars want 1.0, which is
/// what discord's own emoji font draws at in channel and member names.
pub fn layout_em(ctx: &egui::Context, text: &str, font: FontId, color: Color32, max_w: f32, em: f32) -> Rich {
    crate::theme::note_text(text);
    let mut job = LayoutJob::default();
    job.wrap = TextWrapping {
        max_width: max_w,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let spacing = crate::theme::name_spacing(&font);
    let fmt = TextFormat { font_id: font.clone(), color, valign: Align::Center, extra_letter_spacing: spacing, ..Default::default() };
    let mut emojis = Vec::new();
    if !has_emoji(text) {
        job.append(text, 0.0, fmt);
    } else {
        let (size, adv) = if em == 1.0 { (font.size * 4.0 / 3.0, font.size * 1.375) } else { ((font.size * em).round(), (font.size * em).round() + 1.0) };
        let w = ctx.fonts_mut(|f| f.glyph_width(&font, '\u{a0}'));
        let gap = TextFormat { color: Color32::TRANSPARENT, ..fmt.clone() };
        let (mut chars, mut plain, mut i) = (0usize, 0usize, 0usize);
        while i < text.len() {
            let rest = &text[i..];
            match take_emoji(rest) {
                Some(n) if em == 1.0 && text_style(&rest[..n]) => i += n,
                Some(n) => {
                    let run = &text[plain..i];
                    job.append(run, 0.0, fmt.clone());
                    chars += run.chars().count();
                    let lead = adv - w;
                    job.append("\u{a0}", lead, gap.clone());
                    // names (em 1.0) draw emoji with the system font like discord's; message text uses twemoji
                    let url = if em == 1.0 { crate::emoji_font::uri(&rest[..n]) } else { twemoji(&rest[..n]) };
                    emojis.push((chars, url, size, lead));
                    chars += 1;
                    i += n;
                    plain = i;
                }
                None => {
                    let c = rest.chars().next().unwrap_or(' ');
                    if use_boxes_font(text, i, c) {
                        let run = &text[plain..i];
                        job.append(run, 0.0, fmt.clone());
                        chars += run.chars().count() + 1;
                        job.append(&rest[..c.len_utf8()], 0.0, TextFormat { font_id: FontId::new(font.size, egui::FontFamily::Name("boxes".into())), ..fmt.clone() });
                        plain = i + c.len_utf8();
                    }
                    i += c.len_utf8();
                }
            }
        }
        job.append(&text[plain..], 0.0, fmt);
    }
    let galley = ctx.fonts_mut(|f| f.layout_job(job));
    Rich { galley, emojis }
}

impl Rich {
    pub fn size(&self) -> egui::Vec2 {
        self.galley.size()
    }

    /// y of the first line's baseline, from the galley's top.
    pub fn baseline(&self) -> f32 {
        baseline(&self.galley)
    }

    /// Recolour the glyphs by their x offset in the galley, for gradient names.
    pub fn tint(mut self, at: impl Fn(f32) -> Color32) -> Self {
        let mut g = (*self.galley).clone();
        for placed in &mut g.rows {
            let x0 = placed.pos.x;
            let row = Arc::make_mut(&mut placed.row);
            let range = row.visuals.glyph_vertex_range.clone();
            for v in &mut row.visuals.mesh.vertices[range] {
                v.color = at(x0 + v.pos.x);
            }
        }
        self.galley = Arc::new(g);
        self
    }

    pub fn paint(&self, p: &Painter, pos: Pos2) {
        p.galley(pos, self.galley.clone(), Color32::WHITE);
        for (url, r) in self.emoji_rects(pos) {
            image(p, url, r);
        }
    }

    /// Where each emoji image lands when the text is painted at `pos`.
    pub fn emoji_rects(&self, pos: Pos2) -> impl Iterator<Item = (&str, Rect)> {
        self.emojis.iter().filter_map(move |(idx, url, size, lead)| {
            let c = self.galley.pos_from_cursor(CCursor::new(*idx));
            if c.min.x - lead > self.galley.size().x {
                return None; // elided away
            }
            Some((url.as_str(), Rect::from_min_size(pos2(pos.x + (c.min.x - lead).max(0.0), pos.y + c.center().y - size / 2.0), vec2(*size, *size))))
        })
    }
}

pub fn baseline(g: &Galley) -> f32 {
    g.rows.first().and_then(|r| r.glyphs.first().map(|c| r.pos.y + c.pos.y)).unwrap_or(g.size().y * 0.8)
}

/// Paint an http image with any painter, so clip rects are honoured.
pub fn image(p: &Painter, url: &str, rect: Rect) {
    image_tint(p, url, rect, Color32::WHITE);
}

pub fn image_tint(p: &Painter, url: &str, rect: Rect, tint: Color32) {
    let poll = egui::Image::new(crate::gif::uri(p.ctx(), url, p.clip_rect().intersects(rect))).load_for_size(p.ctx(), rect.size());
    if let Ok(egui::load::TexturePoll::Ready { texture }) = poll {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        p.image(texture.id, rect, uv, tint);
    }
}

/// Paint an http image clipped to `radius`. Returns true when the texture was ready and drawn.
pub fn image_rounded(p: &Painter, url: &str, rect: Rect, radius: egui::CornerRadius) -> bool {
    let poll = egui::Image::new(crate::gif::uri(p.ctx(), url, p.clip_rect().intersects(rect))).load_for_size(p.ctx(), rect.size());
    if let Ok(egui::load::TexturePoll::Ready { texture }) = poll {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        p.add(egui::epaint::RectShape::filled(rect, radius, Color32::WHITE).with_texture(texture.id, uv));
        return true;
    }
    false
}

pub(crate) fn cover_uv(tex: egui::Vec2, dst: egui::Vec2) -> Rect {
    if tex.x <= 0.0 || tex.y <= 0.0 || dst.x <= 0.0 || dst.y <= 0.0 {
        return Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    }
    let k = (dst.x / tex.x).max(dst.y / tex.y);
    Rect::from_center_size(pos2(0.5, 0.5), vec2((dst.x / (tex.x * k)).min(1.0), (dst.y / (tex.y * k)).min(1.0)))
}

/// Paint an http image center-cropped to cover `rect`, clipped to `radius`.
pub fn image_cover(p: &Painter, url: &str, rect: Rect, radius: egui::CornerRadius) -> bool {
    let poll = egui::Image::new(crate::gif::uri(p.ctx(), url, p.clip_rect().intersects(rect))).load_for_size(p.ctx(), rect.size());
    if let Ok(egui::load::TexturePoll::Ready { texture }) = poll {
        let uv = cover_uv(texture.size, rect.size());
        p.add(egui::epaint::RectShape::filled(rect, radius, Color32::WHITE).with_texture(texture.id, uv));
        return true;
    }
    false
}

/// Like `Painter::text`, with emoji and an optional width limit (elides with …).
pub fn paint(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    paint_em(p, pos, align, text, font, color, max_w, 1.25)
}

pub fn paint_em(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, max_w: f32, em: f32) -> Rect {
    let r = layout_em(p.ctx(), text, font, color, max_w, em);
    let rect = align.anchor_size(pos, r.size());
    r.paint(p, rect.min);
    rect
}

/// A one-line label widget with emoji support.
pub fn label(ui: &mut Ui, text: &str, font: FontId, color: Color32, sense: Sense) -> Response {
    let r = layout(ui.ctx(), text, font, color, ui.available_width().max(0.0));
    let (rect, resp) = ui.allocate_exact_size(r.size(), sense);
    r.paint(ui.painter(), rect.min);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([0x58, 0x65, 0xf2, 0xff]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_lone_text_presentation_emoji_stays_text() {
        assert!(super::text_style("\u{1f39f}"));
        assert!(super::text_style("\u{2764}"));
        assert!(!super::text_style("\u{1f39f}\u{fe0f}"));
        assert!(!super::text_style("\u{1f600}"));
        assert!(!super::text_style("\u{1f1fb}\u{1f1f3}"));
    }

    #[test]
    fn cover_uv_crops_without_stretching() {
        let uv = cover_uv(vec2(400.0, 400.0), vec2(320.0, 180.0));
        assert!((uv.width() - 1.0).abs() < 1e-4);
        assert!((uv.height() - 180.0 / 320.0).abs() < 1e-4);
        assert!((uv.center().y - 0.5).abs() < 1e-4);

        let exact = cover_uv(vec2(440.0, 248.0), vec2(440.0, 248.0));
        assert!((exact.width() - 1.0).abs() < 1e-4);
        assert!((exact.height() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn image_rounded_reports_false_until_texture_is_ready() {
        let ctx = egui::Context::default();
        ctx.add_texture_loader(Arc::new(crate::imgcache::Textures::default()));
        let missing = "https://cdn.discordapp.com/app-icons/1/missing.png?size=64";
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(24.0, 24.0));
        let mut drew_missing = true;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            drew_missing = image_rounded(ui.painter(), missing, rect, egui::CornerRadius::same(4));
        });
        out.textures_delta.clear();
        assert!(!drew_missing);

        let ready = "https://cdn.discordapp.com/app-icons/1/ok.png?size=64";
        ctx.include_bytes(ready, tiny_png());
        let start = std::time::Instant::now();
        let mut drew_ready = false;
        while !drew_ready && start.elapsed() < std::time::Duration::from_secs(3) {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                drew_ready = image_rounded(ui.painter(), ready, rect, egui::CornerRadius::same(4));
            });
            out.textures_delta.clear();
            if !drew_ready {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert!(drew_ready, "included PNG never became ready for image_rounded");
    }

    #[test]
    fn horizontal_box_bars_skip_double_width_boxes_font() {
        let cat = "★━━━━━━Socialize━━━━━━★";
        for (idx, c) in cat.char_indices() {
            if c == '━' {
                assert!(!use_boxes_font(cat, idx, c));
            }
        }
        let chan = "💬┃general";
        let bar = chan.find('┃').unwrap();
        assert!(use_boxes_font(chan, bar, '┃'));
    }
}
