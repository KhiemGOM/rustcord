use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, ColorImage, Mesh, Rect, TextureHandle, Ui, pos2, vec2};

use super::draft::{Draft, Pic};

#[derive(Default)]
pub struct Banner {
    key: String,
    rx: Option<Receiver<ColorImage>>,
    texture: Option<TextureHandle>,
}

fn blurred(image: &ColorImage) -> ColorImage {
    let pixels: Vec<u8> = image.pixels.iter().flat_map(Color32::to_srgba_unmultiplied).collect();
    let image = image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels).unwrap();
    let image = image::imageops::thumbnail(&image, 256, 256);
    let image = image::imageops::blur(&image, 25.0 * image.width() as f32 / 960.0);
    ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw())
}

impl Banner {
    pub fn paint(&mut self, ui: &Ui, draft: &Draft, region: Rect) {
        let url = match &draft.banner {
            Pic::Keep => draft.banner_url.clone(),
            Pic::New(uri) => super::card::data_image(ui.ctx(), uri),
            _ => None,
        }.unwrap_or_default();
        if url != self.key {
            *self = Self { key: url.clone(), ..Default::default() };
        }
        if url.is_empty() {
            return;
        }
        if self.texture.is_none() && self.rx.is_none() {
            let Ok(egui::load::ImagePoll::Ready { image }) = ui.ctx().try_load_image(&url, Default::default()) else { return };
            let (tx, rx) = channel();
            self.rx = Some(rx);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = tx.send(blurred(&image));
                ctx.request_repaint();
            });
        }
        if let Some(image) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.texture = Some(ui.ctx().load_texture("profile backdrop", image, Default::default()));
            self.rx = None;
        }
        let Some(texture) = &self.texture else { return };
        let r = Rect::from_min_size(region.min, vec2(region.width(), 530.0));
        let aspect = texture.size_vec2().x / texture.size_vec2().y;
        let shown = (r.aspect_ratio() / aspect).min(1.0);
        let x = (1.0 - shown) / 2.0;
        let mut mesh = Mesh::with_texture(texture.id());
        let stops = [(0.0, 128), (0.4, 128), (0.6, 102), (0.8, 51), (1.0, 0)];
        for (y, alpha) in stops {
            let color = Color32::from_white_alpha(alpha);
            mesh.vertices.push(egui::epaint::Vertex { pos: pos2(r.left(), r.top() + y * r.height()), uv: pos2(x, y), color });
            mesh.vertices.push(egui::epaint::Vertex { pos: pos2(r.right(), r.top() + y * r.height()), uv: pos2(1.0 - x, y), color });
        }
        for i in 0..4 {
            let a = i * 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
        ui.painter().with_clip_rect(region).add(mesh);
        crate::ui::settings::mark(ui, "profed_backdrop", r.intersect(region));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn backdrop_is_small_and_softens_source_edges() {
        let mut image = egui::ColorImage::filled([960, 400], egui::Color32::BLACK);
        for y in 0..400 {
            for x in 480..960 {
                image[(x, y)] = egui::Color32::WHITE;
            }
        }
        let small = super::blurred(&image);
        assert_eq!(small.width(), 256);
        let edge = small[(128, small.height() / 2)].r();
        assert!((80..180).contains(&edge));
        assert_eq!(small[(0, 0)], egui::Color32::BLACK);
        assert_eq!(small[(255, 0)], egui::Color32::WHITE);
    }
}
