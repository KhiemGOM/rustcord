//! The composer's live syntax view: markdown marks dimmed and the text between them styled as it
//! will read, mentions picked out, while the draft stays the text typed.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Galley, Stroke, Ui};

use crate::theme::*;

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    under: bool,
    strike: bool,
    spoiler: bool,
    code: bool,
}

const MARKS: [&str; 7] = ["```", "**", "__", "~~", "||", "`", "*"];

pub fn layout(ui: &Ui, text: &str, wrap: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut st = Style::default();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let mark = MARKS.iter().find(|m| rest.starts_with(**m)).copied().filter(|m| !st.code || m.starts_with('`'));
        if let Some(m) = mark {
            match m {
                "```" | "`" => st.code = !st.code,
                "**" => st.bold = !st.bold,
                "__" => st.under = !st.under,
                "~~" => st.strike = !st.strike,
                "||" => st.spoiler = !st.spoiler,
                _ => st.italic = !st.italic,
            }
            push(&mut job, m, st, true);
            i += m.len();
            continue;
        }
        if !st.code
            && rest.starts_with('<')
            && let Some(end) = rest.find('>').filter(|e| *e < 40 && is_mention(&rest[1..*e]))
        {
            let mut f = format(st, false);
            f.color = pal().mention_fg;
            f.background = pal().mention_bg;
            job.append(&rest[..=end], 0.0, f);
            i += end + 1;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        let mut n = c.len_utf8();
        while let Some(next) = text[i + n..].chars().next() {
            if MARKS.iter().any(|m| text[i + n..].starts_with(m)) || next == '<' {
                break;
            }
            n += next.len_utf8();
        }
        push(&mut job, &text[i..i + n], st, false);
        i += n;
    }
    ui.fonts_mut(|f| f.layout_job(job))
}

fn is_mention(inner: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    inner.strip_prefix("@&").or_else(|| inner.strip_prefix("@!")).or_else(|| inner.strip_prefix('@')).or_else(|| inner.strip_prefix('#')).is_some_and(digits)
        || inner.strip_prefix("a:").unwrap_or(inner).strip_prefix(':').and_then(|r| r.split_once(':')).is_some_and(|(_, id)| digits(id))
}

fn push(job: &mut LayoutJob, s: &str, st: Style, mark: bool) {
    job.append(s, 0.0, format(st, mark));
}

fn format(st: Style, mark: bool) -> TextFormat {
    let font = match (st.code, st.bold, st.italic) {
        (true, ..) => mono(16.0),
        (_, true, true) => bold_italic(16.0),
        (_, true, false) => bold(16.0),
        (_, false, true) => italic(16.0),
        _ => reg(16.0),
    };
    let ink = if mark { pal().channel_default } else if st.spoiler { pal().header_secondary } else { pal().text_normal };
    let mut f = TextFormat { font_id: font, color: ink, ..Default::default() };
    if st.under {
        f.underline = Stroke::new(1.0, ink);
    }
    if st.strike {
        f.strikethrough = Stroke::new(1.0, ink);
    }
    if st.code && !mark {
        f.background = pal().bg_code;
    }
    if st.spoiler && !mark {
        f.background = pal().spoiler;
    }
    f
}
