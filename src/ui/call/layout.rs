use egui::{Rect, pos2, vec2};

use crate::state::Id;

pub(super) fn focused(area: Rect, members: &[Id], who: Id) -> Vec<Rect> {
    let rest = members.len() - 1;
    let strip = if rest == 0 { 0.0 } else { 120.0 };
    let big = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - strip - if rest == 0 { 0.0 } else { 8.0 }));
    let (w, h) = (192.0, 108.0);
    let x0 = area.center().x - (rest as f32 * (w + 8.0) - 8.0) / 2.0;
    let mut k = 0;
    members
        .iter()
        .map(|u| {
            if *u == who {
                return big;
            }
            k += 1;
            Rect::from_min_size(pos2(x0 + (k - 1) as f32 * (w + 8.0), area.bottom() - h - 6.0), vec2(w, h))
        })
        .collect()
}

pub(super) fn grid(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let gap = 8.0;
    // try every column count and keep whichever gives the biggest 16:9 tiles
    let mut best = (0.0, 1, 1);
    for cols in 1..=n {
        let rows = n.div_ceil(cols);
        let w = (area.width() - gap * (cols - 1) as f32) / cols as f32;
        let h = (area.height() - gap * (rows - 1) as f32) / rows as f32;
        let tw = w.min(h * 16.0 / 9.0);
        if tw > best.0 {
            best = (tw, cols, rows);
        }
    }
    let (tw, cols, rows) = best;
    let th = tw * 9.0 / 16.0;
    let total_h = rows as f32 * th + (rows - 1) as f32 * gap;
    let mut out = Vec::with_capacity(n);
    for row in 0..rows {
        let in_row = (n - row * cols).min(cols);
        let row_w = in_row as f32 * tw + (in_row - 1) as f32 * gap;
        let x0 = area.center().x - row_w / 2.0;
        let y = area.center().y - total_h / 2.0 + row as f32 * (th + gap);
        for k in 0..in_row {
            out.push(Rect::from_min_size(pos2(x0 + k as f32 * (tw + gap), y), vec2(tw, th)));
        }
    }
    out
}
