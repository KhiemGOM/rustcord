use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

use egui::{Id, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use latex_rust::{Color, Dim, MathFont, SvgOptions};
use pulldown_cmark::{Event, Options, Parser, Tag};

static FONT: LazyLock<Result<MathFont, String>> = LazyLock::new(|| MathFont::stix_two_math().map_err(|e| e.to_string()));

#[derive(Default)]
pub struct Cache {
    markdown: CommonMarkCache,
    formulas: Arc<Mutex<HashMap<(String, bool), Option<Arc<[u8]>>>>>,
    text: (String, String),
}

fn svg(formula: &str, inline: bool) -> Option<Arc<[u8]>> {
    if formula.len() > 8192 {
        return None;
    }
    let font = FONT.as_ref().ok()?;
    let options = SvgOptions {
        font_size_pt: Dim::from_i64(if inline { 12 } else { 20 }),
        color: Color::rgb(255, 255, 255),
        display: !inline,
    };
    latex_rust::latex_to_svg(formula, font, &options).ok().map(|s| s.into_bytes().into())
}

fn remote_text(text: &str) -> String {
    let mut edits: Vec<(Range<usize>, &str)> = Vec::new();
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        let (url, image) = match event {
            Event::Start(Tag::Image { dest_url, .. }) => (dest_url, true),
            Event::Start(Tag::Link { dest_url, .. }) => (dest_url, false),
            _ => continue,
        };
        let url = url.to_ascii_lowercase();
        let allowed = url.starts_with("https://") || url.starts_with("http://") || !image && (url.starts_with("mailto:") || url.starts_with('#'));
        if !allowed && edits.last().is_none_or(|(before, _)| range.start >= before.end) {
            edits.push((range, if image { "(image)" } else { "(link)" }));
        }
    }
    let mut shown = text.to_string();
    for (range, replacement) in edits.into_iter().rev() {
        shown.replace_range(range, replacement);
    }
    shown
}

impl Cache {
    pub fn show(&mut self, ui: &mut Ui, text: &str) {
        if self.text.0 != text {
            self.text = (text.to_string(), remote_text(text));
        }
        let formulas = self.formulas.clone();
        let render = move |ui: &mut Ui, formula: &str, inline: bool| {
            let bytes = formulas.lock().unwrap().entry((formula.to_string(), inline)).or_insert_with(|| svg(formula, inline)).clone();
            match bytes {
                Some(bytes) => {
                    let uri = format!("bytes://math/{:x}.svg", Id::new((formula, inline)).value());
                    let paint = |ui: &mut Ui| {
                        let r = ui.add(egui::Image::from_bytes(uri.clone(), bytes.clone()).fit_to_original_size(1.0).tint(ui.visuals().text_color()).max_width(ui.available_width()).show_loading_spinner(false));
                        ui.ctx().data_mut(|d| d.insert_temp(Id::new(("textfile_math", formula, inline)), r.rect));
                        crate::ui::settings::mark(ui, &format!("math:{inline}:{formula}"), r.rect);
                    };
                    if inline {
                        paint(ui);
                    } else {
                        ui.vertical_centered(paint);
                    }
                }
                None => {
                    let r = ui.monospace(formula);
                    crate::ui::settings::mark(ui, "math:literal", r.rect);
                }
            }
        };
        CommonMarkViewer::new().explicit_image_uri_scheme(true).render_math_fn(Some(&render)).show(ui, &mut self.markdown, &self.text.1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn attachments_do_not_open_local_images_or_active_links() {
        let text = "![local](file:///C:/secret.png) ![relative](image.png) [open](javascript:alert%281%29)";
        let shown = super::remote_text(text);
        assert!(!shown.contains("file:"));
        assert!(!shown.contains("image.png"));
        assert!(!shown.contains("javascript:"));
    }

    #[test]
    fn remote_images_and_literal_code_are_preserved() {
        let text = "![remote](https://example.com/image.png)\n\n`![code](file:///C:/image.png)`\n\n```markdown\n![code](file:///C:/image.png)\n```";
        assert_eq!(super::remote_text(text), text);
    }

    #[test]
    fn screenshot_integral_has_real_vector_glyphs() {
        let svg = super::svg(r"\int_0^\infty e^{-x^2}\,dx", false).unwrap();
        let svg = std::str::from_utf8(&svg).unwrap();
        assert!(svg.contains("<path"));
        assert!(!svg.contains("<text"));
        let tree = resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
        assert!(tree.size().width() > 40.0 && tree.size().height() > 20.0);
    }

    #[test]
    fn invalid_or_oversized_math_falls_back_to_source() {
        assert!(super::svg(r"\not_a_real_command{x}", false).is_none());
        assert!(super::svg(&"x".repeat(8193), false).is_none());
    }
}
