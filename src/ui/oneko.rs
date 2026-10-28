//! oneko: adryd's cat chasing the pointer around the window, frame for frame as oneko.js.

use egui::{Id, LayerId, Order, Rect, TextureOptions, pos2, vec2};
use std::time::{Duration, Instant};

const SHEET: &str = "https://raw.githubusercontent.com/adryd325/oneko.js/14bab15a755d0e35cd4ae19c931d96d306f99f42/oneko.gif";
const SPEED: f32 = 10.0;

#[derive(Clone)]
struct Neko {
    x: f32,
    y: f32,
    frame: u32,
    idle: u32,
    anim: Option<&'static str>,
    anim_frame: u32,
    sprite: (i32, i32),
    at: Instant,
    seed: u64,
}

fn sprites(name: &str) -> &'static [(i32, i32)] {
    match name {
        "alert" => &[(-7, -3)],
        "scratchSelf" => &[(-5, 0), (-6, 0), (-7, 0)],
        "scratchWallN" => &[(0, 0), (0, -1)],
        "scratchWallS" => &[(-7, -1), (-6, -2)],
        "scratchWallE" => &[(-2, -2), (-2, -3)],
        "scratchWallW" => &[(-4, 0), (-4, -1)],
        "tired" => &[(-3, -2)],
        "sleeping" => &[(-2, 0), (-2, -1)],
        "N" => &[(-1, -2), (-1, -3)],
        "NE" => &[(0, -2), (0, -3)],
        "E" => &[(-3, 0), (-3, -1)],
        "SE" => &[(-5, -1), (-5, -2)],
        "S" => &[(-6, -3), (-7, -2)],
        "SW" => &[(-5, -3), (-6, -1)],
        "W" => &[(-4, -2), (-4, -3)],
        "NW" => &[(-1, 0), (-1, -1)],
        _ => &[(-3, -3)],
    }
}

impl Neko {
    fn set(&mut self, name: &str, f: u32) {
        let s = sprites(name);
        self.sprite = s[f as usize % s.len()];
    }

    fn rand(&mut self, n: u64) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed % n
    }

    fn idle(&mut self, screen: Rect) {
        self.idle += 1;
        if self.idle > 10 && self.rand(200) == 0 && self.anim.is_none() {
            let mut pool = vec!["sleeping", "scratchSelf"];
            if self.x < 32.0 {
                pool.push("scratchWallW");
            }
            if self.y < 32.0 {
                pool.push("scratchWallN");
            }
            if self.x > screen.width() - 32.0 {
                pool.push("scratchWallE");
            }
            if self.y > screen.height() - 32.0 {
                pool.push("scratchWallS");
            }
            self.anim = Some(pool[self.rand(pool.len() as u64) as usize]);
        }
        match self.anim {
            Some("sleeping") => {
                if self.anim_frame < 8 {
                    self.set("tired", 0);
                } else {
                    self.set("sleeping", self.anim_frame / 4);
                    if self.anim_frame > 192 {
                        self.anim = None;
                        self.anim_frame = 0;
                        return;
                    }
                }
            }
            Some(a) => {
                self.set(a, self.anim_frame);
                if self.anim_frame > 9 {
                    self.anim = None;
                    self.anim_frame = 0;
                    return;
                }
            }
            None => {
                self.set("idle", 0);
                return;
            }
        }
        self.anim_frame += 1;
    }

    fn step(&mut self, mouse: egui::Pos2, screen: Rect) {
        self.frame += 1;
        let (dx, dy) = (self.x - mouse.x, self.y - mouse.y);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < SPEED || dist < 48.0 {
            return self.idle(screen);
        }
        self.anim = None;
        self.anim_frame = 0;
        if self.idle > 1 {
            self.set("alert", 0);
            self.idle = self.idle.min(7) - 1;
            return;
        }
        let mut dir = String::new();
        if dy / dist > 0.5 {
            dir.push('N');
        }
        if dy / dist < -0.5 {
            dir.push('S');
        }
        if dx / dist > 0.5 {
            dir.push('W');
        }
        if dx / dist < -0.5 {
            dir.push('E');
        }
        self.set(&dir, self.frame);
        self.x = (self.x - dx / dist * SPEED).clamp(16.0, screen.width() - 16.0);
        self.y = (self.y - dy / dist * SPEED).clamp(16.0, screen.height() - 16.0);
    }
}

pub fn show(ctx: &egui::Context) {
    if !crate::plugins::on("oneko") {
        return;
    }
    let id = Id::new("oneko");
    let mut n = ctx.data_mut(|d| d.get_temp::<Neko>(id)).unwrap_or_else(|| Neko {
        x: 32.0,
        y: 32.0,
        frame: 0,
        idle: 0,
        anim: None,
        anim_frame: 0,
        sprite: (-3, -3),
        at: Instant::now(),
        seed: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64 | 1),
    });
    let screen = ctx.content_rect();
    if n.at.elapsed() >= Duration::from_millis(100) {
        n.at = Instant::now();
        let mouse = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(pos2(0.0, 0.0)) - screen.min.to_vec2();
        n.step(mouse, screen);
    }
    let still = n.anim.is_none() && n.sprite == (-3, -3);
    let at = Rect::from_center_size(screen.min + vec2(n.x, n.y), vec2(32.0, 32.0));
    let uv = Rect::from_min_size(pos2(-n.sprite.0 as f32 * 32.0 / 256.0, -n.sprite.1 as f32 * 32.0 / 128.0), vec2(32.0 / 256.0, 32.0 / 128.0));
    if let Ok(egui::load::TexturePoll::Ready { texture }) = ctx.try_load_texture(SHEET, TextureOptions::NEAREST, egui::load::SizeHint::default()) {
        ctx.layer_painter(LayerId::new(Order::Tooltip, id)).image(texture.id, at, uv, egui::Color32::WHITE);
    }
    ctx.data_mut(|d| d.insert_temp(id, n));
    // an idle cat only needs a look now and then for its naps
    ctx.request_repaint_after(Duration::from_millis(if still { 1000 } else { 100 }));
}
