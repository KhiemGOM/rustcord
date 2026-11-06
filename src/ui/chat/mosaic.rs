//! Discord's attachment mosaic: two or more images and videos share one block as much as 550 wide.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use super::{Act, D, alt_badge, media_at};
use crate::state::{Attachment, Id};
use crate::theme::*;
use crate::ui::player::{self, Look};

const GAP: f32 = 4.0;
const MAX_W: f32 = 550.0;
const MAX_H: f32 = 350.0;
const WIDE_ROW: f32 = 0.508;
pub const SHOWN: usize = 10;

pub fn layout(n: usize, w: f32) -> Vec<Rect> {
    let at = |x: f32, y: f32, cw: f32, ch: f32| Rect::from_min_size(pos2(x, y), vec2(cw, ch));
    let half = (w - GAP) / 2.0;
    let tall = w.min(MAX_H);
    match n {
        0 | 1 => vec![at(0.0, 0.0, w, tall)],
        2 => vec![at(0.0, 0.0, half, half.min(MAX_H)), at(half + GAP, 0.0, half, half.min(MAX_H))],
        3 => {
            let (big, small, row) = ((w - GAP) * 2.0 / 3.0, (w - GAP) / 3.0, (tall - GAP) / 2.0);
            vec![at(0.0, 0.0, big, tall), at(big + GAP, 0.0, small, row), at(big + GAP, row + GAP, small, row)]
        }
        4 => {
            let row = (tall - GAP) / 2.0;
            (0..4).map(|i| at((i % 2) as f32 * (half + GAP), (i / 2) as f32 * (row + GAP), half, row)).collect()
        }
        _ => {
            let third = (w - GAP * 2.0) / 3.0;
            let first = match n % 3 {
                0 => 3,
                k => k,
            };
            let mut out = Vec::new();
            let mut y = 0.0;
            let mut left = n;
            let mut per = first;
            while left > 0 {
                let k = per.min(left);
                let cw = (w - GAP * (k as f32 - 1.0)) / k as f32;
                let ch = if k == 3 { third } else { w * WIDE_ROW };
                out.extend((0..k).map(|i| at(i as f32 * (cw + GAP), y, cw, ch)));
                y += ch + GAP;
                left -= k;
                per = 3;
            }
            out
        }
    }
}

/// Object-fit: cover; the part of the picture that shows in a cell, and the size the whole picture is drawn at.
fn cover(cell: Vec2, a: &Attachment) -> (Vec2, Rect) {
    let (nw, nh) = (a.w.max(1) as f32, a.h.max(1) as f32);
    let s = (cell.x / nw).max(cell.y / nh);
    let whole = vec2(nw * s, nh * s);
    let shown = vec2(cell.x / whole.x, cell.y / whole.y);
    let min = (Vec2::splat(1.0) - shown) / 2.0;
    (whole, Rect::from_min_size(min.to_pos2(), shown))
}

fn corners(cell: Rect, whole: Rect) -> CornerRadius {
    let (l, t, r, b) = ((cell.left() - whole.left()).abs() < 0.5, (cell.top() - whole.top()).abs() < 0.5, (whole.right() - cell.right()).abs() < 0.5, (whole.bottom() - cell.bottom()).abs() < 0.5);
    let pick = |on: bool| if on { 8 } else { 0 };
    CornerRadius { nw: pick(l && t), ne: pick(r && t), sw: pick(l && b), se: pick(r && b) }
}

pub fn show(ui: &mut Ui, d: &mut D, msg: Id, items: &[&Attachment]) {
    let w = ui.available_width().min(MAX_W);
    let shown = items.len().min(SHOWN);
    let cells = layout(shown, w);
    let height = cells.iter().map(|c| c.bottom()).fold(0.0, f32::max);
    let (whole, _) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
    d.rects.push((format!("mosaic:{msg}:{}", d.ch), whole));
    for (i, (a, cell)) in items.iter().zip(&cells).enumerate() {
        let cell = cell.translate(whole.min.to_vec2());
        let (full, uv) = cover(cell.size(), a);
        let look = Look { radius: corners(cell, whole), uv };
        d.rects.push((format!("mosaic:{msg}:{i}:{}", d.ch), cell));
        let sep = if a.proxy.contains('?') { '&' } else { '?' };
        if a.is_video() {
            let poster = format!("{}{sep}format=webp&width={}&height={}", a.proxy, (full.x * 2.0) as u32, (full.y * 2.0) as u32);
            let mut outs = Vec::new();
            let mut cell_ui = ui.new_child(UiBuilder::new().max_rect(cell));
            let r = player::video_in(&mut cell_ui, a, cell.size(), &poster, &mut outs, look);
            d.card("player_video", ui, r);
            d.player(outs);
        } else {
            let url = format!("{}{sep}width={}&height={}", a.proxy, (full.x * 2.0) as u32, (full.y * 2.0) as u32);
            let resp = ui.interact(cell, ui.id().with(("mosaic", i, &a.url)), Sense::click());
            media_at(ui, d, &resp, cell, &url, &a.url, None, look);
            if d.s.image_descriptions && !a.description.is_empty() {
                alt_badge(ui, &resp, &a.description);
            }
            if ui.clip_rect().contains(cell.center()) {
                d.rects.push(("image".into(), cell.intersect(ui.clip_rect())));
            }
            if resp.clicked() {
                d.acts.push(Act::View(a.proxy.clone(), a.url.clone(), a.w, a.h));
            }
        }
        if i == SHOWN - 1 && items.len() > SHOWN {
            ui.painter().rect_filled(cell, look.radius, Color32::from_black_alpha(153));
            ui.painter().text(cell.center(), Align2::CENTER_CENTER, format!("+{}", items.len() - SHOWN), bold(24.0), Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(r: Rect, x: f32, y: f32, w: f32, h: f32) -> bool {
        [(r.left(), x), (r.top(), y), (r.width(), w), (r.height(), h)].iter().all(|(a, b)| (a - b).abs() < 1.0)
    }

    #[test]
    fn two_are_squares_side_by_side() {
        let c = layout(2, 550.0);
        assert!(near(c[0], 0.0, 0.0, 273.0, 273.0) && near(c[1], 277.0, 0.0, 273.0, 273.0));
    }

    #[test]
    fn three_are_one_tall_and_two_stacked() {
        let c = layout(3, 550.0);
        assert!(near(c[0], 0.0, 0.0, 364.0, 350.0) && near(c[1], 368.0, 0.0, 182.0, 173.0) && near(c[2], 368.0, 177.0, 182.0, 173.0));
    }

    #[test]
    fn four_are_two_by_two() {
        let c = layout(4, 550.0);
        assert!(near(c[3], 277.0, 177.0, 273.0, 173.0));
    }

    #[test]
    fn seven_are_one_then_rows_of_three() {
        let c = layout(7, 550.0);
        assert!(near(c[0], 0.0, 0.0, 550.0, 279.4) && near(c[1], 0.0, 283.4, 180.7, 180.7) && near(c[6], 369.3, 468.1, 180.7, 180.7));
    }

    #[test]
    fn every_size_fits_the_width_it_is_given() {
        for w in [150.0, 300.0, 421.0, 550.0] {
            for n in 2..=SHOWN {
                let c = layout(n, w);
                assert_eq!(c.len(), n);
                assert!(c.iter().all(|r| r.left() >= -0.01 && r.right() <= w + 0.01 && r.width() > 0.0 && r.height() > 0.0), "{n} at {w}");
            }
        }
    }

    #[test]
    fn a_wide_picture_loses_its_sides() {
        let a = Attachment { w: 400, h: 100, ..Default::default() };
        let (whole, uv) = cover(vec2(100.0, 100.0), &a);
        assert!((whole.x - 400.0).abs() < 0.01 && (uv.width() - 0.25).abs() < 0.01 && (uv.height() - 1.0).abs() < 0.01);
    }
}
