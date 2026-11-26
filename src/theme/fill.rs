//! What sits behind the window besides a plain theme: one of discord's 22 gradient presets, or a
//! theme the user (or a server) made from up to five colours.

use super::{PRESETS, Preset};
use std::sync::Mutex;

/// discord's id for each preset in `PRESETS`' order; the ids are what settings-proto stores.
pub const PRESET_IDS: [u32; 22] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 18, 20, 21, 19, 22];

pub const DEFAULT_MIX: i32 = 74;
pub const DEFAULT_COLOR: u32 = 0x5865f2;

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Custom {
    pub colors: Vec<u32>,
    pub angle: i32,
    pub mix: i32,
    pub light: bool,
}

impl Default for Custom {
    fn default() -> Self {
        Custom { colors: vec![DEFAULT_COLOR], angle: 0, mix: DEFAULT_MIX, light: false }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Fill {
    Preset(u8),
    Custom(Custom),
}

static MADE: Mutex<Vec<(Custom, &'static Preset)>> = Mutex::new(Vec::new());
const MAX_MADE: usize = 64;
const MAX_COLORS: usize = 5;

impl Fill {
    pub fn from_id(id: u32) -> Option<Fill> {
        PRESET_IDS.iter().position(|p| *p == id).map(|i| Fill::Preset(i as u8))
    }

    pub fn id(i: u8) -> u32 {
        PRESET_IDS.get(i as usize).copied().unwrap_or(0)
    }

    pub fn resolve(&self) -> &'static Preset {
        match self {
            Fill::Preset(i) => PRESETS.get(*i as usize).unwrap_or(&PRESETS[0]),
            Fill::Custom(c) => {
                let mut key = c.clone();
                key.colors.truncate(MAX_COLORS);
                let mut made = MADE.lock().unwrap();
                if let Some((_, p)) = made.iter().find(|(k, _)| k == &key) {
                    return p;
                }
                if made.len() >= MAX_MADE {
                    return made.last().map_or(&PRESETS[0], |(_, p)| *p);
                }
                let p: &'static Preset = Box::leak(Box::new(preset(&key)));
                made.push((key, p));
                p
            }
        }
    }
}

pub(super) fn stops_of(c: &Custom) -> Vec<(u32, f32)> {
    let colors: Vec<u32> = if c.colors.is_empty() { vec![DEFAULT_COLOR] } else { c.colors.iter().take(MAX_COLORS).copied().collect() };
    let mut stops: Vec<(u32, f32)> = colors.iter().enumerate().map(|(i, c)| (*c, i as f32 / (colors.len() - 1).max(1) as f32 * 100.0)).collect();
    if stops.len() == 1 {
        stops.push((stops[0].0, 100.0));
    }
    stops
}

pub fn rounded_custom_gradient(c: &Custom, r: egui::Rect, radius: f32) -> egui::Mesh {
    let stops = stops_of(c);
    rounded_stops(c.angle as f32, &stops, r, radius)
}

pub(super) fn rounded_stops(angle: f32, stops: &[(u32, f32)], r: egui::Rect, radius: f32) -> egui::Mesh {
    let rad = angle.to_radians();
    let dir = egui::vec2(rad.sin(), -rad.cos());
    let half = (r.width() * dir.x.abs() + r.height() * dir.y.abs()) / 2.0;
    let colour = |p: egui::Pos2| super::gradient_stops_at(stops, (p - r.center()).dot(dir) / half.max(1.0) / 2.0 + 0.5);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.center(), colour(r.center()));
    let corners = [(r.right_top(), -1.0, 1.0, 270.0), (r.right_bottom(), -1.0, -1.0, 0.0), (r.left_bottom(), 1.0, -1.0, 90.0), (r.left_top(), 1.0, 1.0, 180.0)];
    for (corner, sx, sy, start) in corners {
        let centre = corner + egui::vec2(sx * radius, sy * radius);
        for k in 0..=6 {
            let a = (start + 15.0 * k as f32).to_radians();
            let p = centre + egui::vec2(a.cos(), a.sin()) * radius;
            mesh.colored_vertex(p, colour(p));
        }
    }
    let n = mesh.vertices.len() as u32 - 1;
    for i in 1..=n {
        mesh.add_triangle(0, i, if i == n { 1 } else { i + 1 });
    }
    mesh
}

fn preset(c: &Custom) -> Preset {
    let stops = stops_of(c);
    let m = (30.0 + 0.7 * (100 - c.mix.clamp(0, 100)) as f32) / 100.0;
    Preset { name: "Custom Theme", light: c.light, angle: c.angle as f32, veil: Some(m), stops: Box::leak(stops.into_boxed_slice()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_round_trip() {
        for i in 0..22u8 {
            assert_eq!(Fill::from_id(Fill::id(i)), Some(Fill::Preset(i)));
        }
        assert_eq!(Fill::from_id(16), None);
        assert_eq!(PRESETS[19].name, "Aurora");
        assert_eq!(Fill::id(19), 21);
    }

    #[test]
    fn a_single_colour_is_a_flat_gradient() {
        let p = Fill::Custom(Custom { colors: vec![0x336699], ..Custom::default() }).resolve();
        assert_eq!(p.stops, &[(0x336699, 0.0), (0x336699, 100.0)]);
        let q = Fill::Custom(Custom { colors: vec![0x336699], ..Custom::default() }).resolve();
        assert!(std::ptr::eq(p, q));
    }
}
