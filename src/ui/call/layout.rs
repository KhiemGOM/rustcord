use egui::{Rect, pos2, vec2};

pub(super) const WIDE: f32 = 16.0 / 9.0;
pub(super) const STRIP_TILE: egui::Vec2 = egui::vec2(195.0, 112.0);

/// Discord's call grid: the widest 16:9 tile, on whole points, that still lets every tile fit 8
/// apart inside 8 from the sides and 64 clear of the bars at the top and bottom. Rows are centred,
/// and the block sits in the middle of what's left.
pub(super) fn grid(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let (w, h) = (area.width() - 16.0, area.height() - 128.0);
    let cols = |t: f32| (((w - t) / (8.0 + t)).floor() + 1.0).max(1.0);
    let fits = |t: f32| {
        let th = t / WIDE;
        cols(t) * (((h - th) / (8.0 + th)).floor() + 1.0) >= n as f32
    };
    let (mut lo, mut count) = ((w / 25.0).floor(), w - (w / 25.0).floor());
    while count > 0.0 {
        let step = (count / 2.0).floor();
        if fits(lo + step) {
            lo += step + 1.0;
            count -= step + 1.0;
        } else {
            count = step;
        }
    }
    let t = (lo - 1.0).max(1.0);
    let per = cols(t) as usize;
    let rows = n.div_ceil(per);
    let pitch = (t / WIDE).floor() + 8.0;
    let top = area.top() + 64.0 + (h - pitch * rows as f32).max(0.0) / 2.0;
    (0..n)
        .map(|i| {
            let (row, k) = (i / per, (n - i / per * per).min(per));
            let width = k as f32 * t + (k - 1) as f32 * 8.0;
            let x = area.left() + 8.0 + (w - width) / 2.0 + (i % per) as f32 * (t + 8.0);
            Rect::from_min_size(pos2(x, top + row as f32 * pitch), vec2(t, t / WIDE))
        })
        .collect()
}

/// Discord's focused call: the picked tile as big as fits at its own shape, 60 above the middle,
/// over a centred strip of 195x112 tiles that ends 80 above the bottom; with more than fit, up to
/// 12 shrink to share the width. A call 384 tall or less has no strip, and the picked tile takes
/// the whole height.
pub(super) fn focus(area: Rect, aspect: f32, n: usize) -> (Rect, Vec<Rect>) {
    let strip = area.height() > 384.0;
    let (pad, lift) = if strip { (140.0, 60.0) } else { (0.0, 0.0) };
    let w = ((area.height() - 2.0 * pad) * aspect).min(area.width());
    let big = Rect::from_center_size(area.center() - vec2(0.0, lift), vec2(w, (w / aspect + 0.001).floor()));
    if !strip {
        return (big, Vec::new());
    }
    let fit = (((area.width() - 8.0) / 132.0).floor() as usize).min(12).min(n);
    let w = ((area.width() - 16.0 - 8.0 * fit.saturating_sub(1) as f32) / fit.max(1) as f32).min(STRIP_TILE.x);
    let x0 = area.center().x - (fit as f32 * (w + 8.0) - 8.0) / 2.0;
    let tiles = (0..fit).map(|i| Rect::from_min_size(pos2(x0 + i as f32 * (w + 8.0), area.bottom() - 192.0), vec2(w, STRIP_TILE.y))).collect();
    (big, tiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALL_AT_1280X800: Rect = Rect { min: pos2(375.0, 32.0), max: pos2(1280.0, 800.0) };

    #[test]
    fn three_tiles_make_two_rows_of_440_like_discord() {
        let t = grid(CALL_AT_1280X800, 3);
        assert_eq!(t[0].width(), 440.0);
        assert_eq!((t[0].left(), t[0].top()), (383.5, 161.0));
        assert_eq!(t[1].left(), 831.5);
        assert_eq!((t[2].left(), t[2].top()), (607.5, 416.0));
    }

    #[test]
    fn two_tiles_stack_when_that_makes_them_bigger() {
        let t = grid(CALL_AT_1280X800, 2);
        assert_eq!(t[0].left(), t[1].left());
        assert!(t[0].width() > 540.0);
    }

    #[test]
    fn a_watched_stream_sits_over_the_strip_like_discord() {
        let (big, strip) = focus(CALL_AT_1280X800, WIDE, 3);
        assert_eq!((big.width().round(), big.height()), (868.0, 488.0));
        assert_eq!(big.top(), 112.0);
        assert_eq!(strip.len(), 3);
        assert_eq!((strip[0].left(), strip[0].top()), (527.0, 608.0));
        let (_, many) = focus(CALL_AT_1280X800, WIDE, 20);
        assert_eq!(many.len(), 6);
        assert!(many[0].width() < 195.0 && many[5].right() <= CALL_AT_1280X800.right() - 8.0);
        let (_, none) = focus(Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 380.0)), WIDE, 3);
        assert!(none.is_empty());
    }
}
