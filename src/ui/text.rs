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

fn has_emoji(s: &str) -> bool {
    s.chars().any(|c| c as u32 >= 0x2190 || c == '\u{a9}' || c == '\u{ae}')
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
    let fmt = TextFormat { font_id: font.clone(), color, valign: Align::Center, ..Default::default() };
    let mut emojis = Vec::new();
    if !has_emoji(text) {
        job.append(text, 0.0, fmt);
    } else {
        let size = (font.size * em).round();
        let w = ctx.fonts_mut(|f| f.glyph_width(&font, '\u{a0}'));
        let gap = TextFormat { color: Color32::TRANSPARENT, ..fmt.clone() };
        let (mut chars, mut plain, mut i) = (0usize, 0usize, 0usize);
        while i < text.len() {
            let rest = &text[i..];
            match take_emoji(rest) {
                Some(n) => {
                    let run = &text[plain..i];
                    job.append(run, 0.0, fmt.clone());
                    chars += run.chars().count();
                    let lead = size - w + 1.0;
                    job.append("\u{a0}", lead, gap.clone());
                    emojis.push((chars, twemoji(&rest[..n]), size, lead));
                    chars += 1;
                    i += n;
                    plain = i;
                }
                None => i += rest.chars().next().map_or(1, char::len_utf8),
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
