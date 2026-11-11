use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use egui::{Id, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use latex_rust::{Color, Dim, MathFont, SvgOptions};

static FONT: LazyLock<Result<MathFont, String>> = LazyLock::new(|| MathFont::stix_two_math().map_err(|e| e.to_string()));

#[derive(Default)]
pub struct Cache {
    markdown: CommonMarkCache,
    formulas: Arc<Mutex<HashMap<(String, bool), Option<Arc<[u8]>>>>>,
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

impl Cache {
    pub fn show(&mut self, ui: &mut Ui, text: &str) {
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
        CommonMarkViewer::new().render_math_fn(Some(&render)).show(ui, &mut self.markdown, text);
    }
}

#[cfg(test)]
mod tests {
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
