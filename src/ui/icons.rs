//! Vector icons drawn straight onto the painter. Emoji glyphs made decent icons
//! until a font lacked one or sat on the wrong baseline; these always line up.
//! Coordinates are on a 24x24 grid and scale with the target rect.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};
use std::f32::consts::{FRAC_PI_4, PI, TAU};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Hash,
    Speaker,
    Megaphone,
    Forum,
    Stage,
    Thread,
    People,
    Pin,
    Gear,
    Mic,
    Headphones,
    Plus,
    Smile,
    AddReaction,
    Search,
    Reply,
    Pencil,
    Trash,
    Close,
    Check,
    ChevronDown,
    ChevronRight,
    Folder,
    File,
    Play,
    Image,
    Gem,
    ArrowRight,
    ArrowLeft,
    Bubble,
    Send,
}

struct G<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    c: Color32,
}

impl G<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.o + vec2(x, y) * self.s
    }
    fn stroke(&self, w: f32) -> Stroke {
        Stroke::new(w * self.s, self.c)
    }
    fn line(&self, pts: &[(f32, f32)], w: f32) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::line(pts, self.stroke(w)));
    }
    fn fill(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::convex_polygon(pts, self.c, Stroke::NONE));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.pt(x, y), r * self.s, self.c);
    }
    fn ring(&self, x: f32, y: f32, r: f32, w: f32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s, self.stroke(w));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32) {
        let r = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect_filled(r, round * self.s, self.c);
    }
    fn rect_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32, w: f32) {
        let r = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect_stroke(r, round * self.s, self.stroke(w), egui::StrokeKind::Middle);
    }
    // angles in radians, y pointing down like the screen
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32, w: f32) {
        let n = 16;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.pt(x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke(w)));
    }
    fn half_disc(&self, x: f32, y: f32, r: f32) {
        // the top half of a disc, for shoulders
        let n = 16;
        let mut pts = vec![self.pt(x + r, y)];
        for i in 0..=n {
            let a = -(i as f32) * PI / n as f32;
            pts.push(self.pt(x + r * a.cos(), y + r * a.sin()));
        }
        self.p.add(Shape::convex_polygon(pts, self.c, Stroke::NONE));
    }
}

fn rotate(pts: &[(f32, f32)], a: f32) -> Vec<(f32, f32)> {
    let (s, c) = a.sin_cos();
    pts.iter()
        .map(|(x, y)| {
            let (dx, dy) = (x - 12.0, y - 12.0);
            (12.0 + dx * c - dy * s, 12.0 + dx * s + dy * c)
        })
        .collect()
}

pub fn draw(p: &Painter, icon: Icon, rect: Rect, c: Color32) {
    let side = rect.width().min(rect.height());
    let g = G { p, o: rect.center() - vec2(side, side) / 2.0, s: side / 24.0, c };
    match icon {
        Icon::Hash => {
            g.line(&[(10.0, 3.5), (8.0, 20.5)], 2.0);
            g.line(&[(16.5, 3.5), (14.5, 20.5)], 2.0);
            g.line(&[(4.5, 8.5), (20.0, 8.5)], 2.0);
            g.line(&[(3.8, 15.5), (19.3, 15.5)], 2.0);
        }
        Icon::Speaker => {
            g.fill(&[(3.0, 9.0), (7.0, 9.0), (12.0, 4.5), (12.0, 19.5), (7.0, 15.0), (3.0, 15.0)]);
            g.arc(12.5, 12.0, 4.0, -FRAC_PI_4, FRAC_PI_4, 2.0);
            g.arc(12.5, 12.0, 7.5, -FRAC_PI_4 * 1.2, FRAC_PI_4 * 1.2, 2.0);
        }
        Icon::Megaphone => {
            g.fill(&[(6.0, 9.0), (18.0, 4.0), (18.0, 20.0), (6.0, 15.0)]);
            g.rect(3.0, 9.0, 7.0, 15.0, 1.0);
            g.line(&[(8.0, 15.0), (9.5, 20.5)], 2.5);
            g.line(&[(20.5, 10.0), (20.5, 14.0)], 2.0);
        }
        Icon::Forum => {
            g.rect_line(3.0, 4.0, 16.0, 13.5, 3.0, 2.0);
            g.fill(&[(5.0, 13.0), (8.5, 13.0), (5.0, 16.5)]);
            g.line(&[(18.0, 8.5), (21.0, 8.5), (21.0, 18.0), (19.0, 18.0), (19.0, 20.5), (16.0, 18.0), (9.0, 18.0), (9.0, 15.5)], 2.0);
        }
        Icon::Stage => {
            g.circle(12.0, 12.0, 2.5);
            g.arc(12.0, 12.0, 5.5, -PI * 0.75, -PI * 0.25, 2.0);
            g.arc(12.0, 12.0, 5.5, PI * 0.25, PI * 0.75, 2.0);
            g.arc(12.0, 12.0, 9.0, -PI * 0.75, -PI * 0.25, 2.0);
            g.arc(12.0, 12.0, 9.0, PI * 0.25, PI * 0.75, 2.0);
        }
        Icon::Thread => {
            g.line(&[(8.5, 3.0), (7.0, 15.0)], 2.0);
            g.line(&[(13.5, 3.0), (12.0, 10.0)], 2.0);
            g.line(&[(3.5, 7.0), (16.0, 7.0)], 2.0);
            g.line(&[(3.0, 12.0), (10.0, 12.0)], 2.0);
            g.rect(12.0, 12.0, 21.0, 19.0, 3.0);
            g.fill(&[(14.0, 18.5), (17.0, 18.5), (14.0, 21.5)]);
        }
        Icon::People => {
            g.circle(9.0, 8.0, 3.5);
            g.half_disc(9.0, 19.5, 6.0);
            g.circle(16.5, 8.5, 3.0);
            g.fill(&[(15.5, 13.5), (17.5, 13.5), (21.5, 16.5), (21.5, 19.5), (16.0, 19.5)]);
        }
        Icon::Pin => {
            let shape = [
                (9.0, 3.0), (15.0, 3.0), (15.0, 5.0), (14.0, 5.0), (15.5, 12.0), (18.0, 13.5),
                (18.0, 15.0), (6.0, 15.0), (6.0, 13.5), (8.5, 12.0), (10.0, 5.0), (9.0, 5.0),
            ];
            let r = rotate(&shape, FRAC_PI_4);
            // the outline isn't convex, so fill it in two convex halves
            g.fill(&[r[0], r[1], r[2], r[3], r[10], r[11]]);
            g.fill(&[r[3], r[4], r[5], r[6], r[7], r[8], r[9], r[10]]);
            let needle = rotate(&[(12.0, 15.0), (12.0, 21.5)], FRAC_PI_4);
            g.line(&needle, 2.0);
        }
        Icon::Gear => {
            g.ring(12.0, 12.0, 5.0, 3.0);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                g.line(&[(12.0 + 6.0 * a.cos(), 12.0 + 6.0 * a.sin()), (12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin())], 3.2);
            }
        }
        Icon::Mic => {
            g.rect(9.0, 2.5, 15.0, 14.0, 3.0);
            g.arc(12.0, 11.0, 6.0, 0.0, PI, 2.0);
            g.line(&[(12.0, 17.0), (12.0, 21.0)], 2.0);
            g.line(&[(8.5, 21.0), (15.5, 21.0)], 2.0);
        }
        Icon::Headphones => {
            g.arc(12.0, 13.0, 8.0, PI, TAU, 2.0);
            g.rect(3.0, 13.0, 8.0, 20.5, 2.0);
            g.rect(16.0, 13.0, 21.0, 20.5, 2.0);
        }
        Icon::Plus => {
            g.line(&[(12.0, 5.0), (12.0, 19.0)], 2.5);
            g.line(&[(5.0, 12.0), (19.0, 12.0)], 2.5);
        }
        Icon::Smile | Icon::AddReaction => {
            let (cx, cy, r) = if icon == Icon::AddReaction { (11.0, 13.0, 8.0) } else { (12.0, 12.0, 9.0) };
            g.ring(cx, cy, r, 2.0);
            g.circle(cx - r * 0.35, cy - r * 0.2, 1.4);
            g.circle(cx + r * 0.35, cy - r * 0.2, 1.4);
            g.arc(cx, cy, r * 0.5, PI * 0.15, PI * 0.85, 2.0);
            if icon == Icon::AddReaction {
                g.line(&[(19.5, 1.5), (19.5, 8.5)], 2.0);
                g.line(&[(16.0, 5.0), (23.0, 5.0)], 2.0);
            }
        }
        Icon::Search => {
            g.ring(10.5, 10.5, 6.0, 2.0);
            g.line(&[(15.0, 15.0), (20.5, 20.5)], 2.5);
        }
        Icon::Reply => {
            g.line(&[(9.5, 4.5), (4.0, 10.0), (9.5, 15.5)], 2.0);
            let mut pts = vec![(4.0, 10.0), (14.0, 10.0)];
            for i in 1..=8 {
                let a = -PI / 2.0 + i as f32 * (PI / 2.0) / 8.0;
                pts.push((14.0 + 6.0 * a.cos(), 16.0 + 6.0 * a.sin()));
            }
            pts.push((20.0, 20.0));
            g.line(&pts, 2.0);
        }
        Icon::Pencil => {
            g.line(&[(7.0, 17.0), (17.0, 7.0)], 3.5);
            g.line(&[(18.0, 6.0), (19.0, 5.0)], 3.5);
            g.fill(&[(3.5, 20.5), (5.0, 15.5), (8.5, 19.0)]);
        }
        Icon::Trash => {
            g.line(&[(4.5, 6.0), (19.5, 6.0)], 2.0);
            g.line(&[(9.5, 6.0), (9.5, 3.5), (14.5, 3.5), (14.5, 6.0)], 2.0);
            g.line(&[(6.5, 6.0), (7.5, 20.5), (16.5, 20.5), (17.5, 6.0)], 2.0);
            g.line(&[(10.0, 10.0), (10.0, 17.0)], 1.8);
            g.line(&[(14.0, 10.0), (14.0, 17.0)], 1.8);
        }
        Icon::Close => {
            g.line(&[(6.0, 6.0), (18.0, 18.0)], 2.0);
            g.line(&[(18.0, 6.0), (6.0, 18.0)], 2.0);
        }
        Icon::Check => g.line(&[(5.0, 12.5), (10.0, 17.5), (19.0, 7.0)], 2.5),
        Icon::ChevronDown => g.line(&[(7.0, 9.5), (12.0, 14.5), (17.0, 9.5)], 2.0),
        Icon::ChevronRight => g.line(&[(9.5, 7.0), (14.5, 12.0), (9.5, 17.0)], 2.0),
        Icon::Folder => {
            g.fill(&[(3.0, 5.5), (9.0, 5.5), (11.0, 7.5), (11.0, 9.0), (3.0, 9.0)]);
            g.rect(3.0, 8.0, 21.0, 19.0, 2.0);
        }
        Icon::File => {
            g.line(&[(14.0, 2.5), (5.0, 2.5), (5.0, 21.5), (19.0, 21.5), (19.0, 7.5), (14.0, 2.5), (14.0, 7.5), (19.0, 7.5)], 1.8);
        }
        Icon::Play => g.fill(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]),
        Icon::Image => {
            g.rect_line(3.0, 4.5, 21.0, 19.5, 2.0, 2.0);
            g.fill(&[(5.5, 17.5), (10.0, 11.0), (13.5, 15.5)]);
            g.fill(&[(11.5, 17.5), (15.5, 12.5), (19.0, 17.5)]);
            g.circle(16.0, 8.5, 1.6);
        }
        Icon::Gem => {
            g.fill(&[(6.0, 4.0), (18.0, 4.0), (22.0, 9.0), (12.0, 21.0), (2.0, 9.0)]);
        }
        Icon::ArrowRight => {
            g.line(&[(4.0, 12.0), (19.0, 12.0)], 2.0);
            g.line(&[(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)], 2.0);
        }
        Icon::ArrowLeft => {
            g.line(&[(20.0, 12.0), (5.0, 12.0)], 2.0);
            g.line(&[(11.0, 6.0), (5.0, 12.0), (11.0, 18.0)], 2.0);
        }
        Icon::Send => {
            g.fill(&[(3.0, 4.0), (21.0, 12.0), (3.0, 20.0), (6.0, 12.0)]);
        }
        Icon::Bubble => {
            g.rect(3.0, 4.0, 21.0, 17.0, 4.0);
            g.fill(&[(6.0, 16.0), (11.0, 16.0), (6.0, 21.0)]);
        }
    }
}
