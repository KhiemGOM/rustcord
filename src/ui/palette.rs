//! The colour discord takes out of an avatar for call tiles: the copy of color-thief's quantize in
//! its bundle, quirks and all, asked for 5 colours from every 10th pixel, and the first one kept.

use egui::Color32;

fn at(r: i64, g: i64, b: i64) -> usize {
    ((r << 10) + (g << 5) + b) as usize
}

#[derive(Clone)]
struct VBox {
    // r g b, inclusive, on 5 bit channels
    lo: [i64; 3],
    hi: [i64; 3],
    count: i64,
}

impl VBox {
    fn new(lo: [i64; 3], hi: [i64; 3], h: &[u32]) -> Self {
        let mut count = 0;
        for r in lo[0]..=hi[0] {
            for g in lo[1]..=hi[1] {
                for b in lo[2]..=hi[2] {
                    count += h[at(r, g, b)] as i64;
                }
            }
        }
        VBox { lo, hi, count }
    }

    fn volume(&self) -> i64 {
        (0..3).map(|i| self.hi[i] - self.lo[i] + 1).product()
    }

    fn avg(&self, h: &[u32]) -> [u8; 3] {
        let (mut n, mut s) = (0.0, [0.0f64; 3]);
        for r in self.lo[0]..=self.hi[0] {
            for g in self.lo[1]..=self.hi[1] {
                for b in self.lo[2]..=self.hi[2] {
                    let k = h[at(r, g, b)] as f64;
                    n += k;
                    for (i, v) in [r, g, b].into_iter().enumerate() {
                        s[i] += k * (v as f64 + 0.5) * 8.0;
                    }
                }
            }
        }
        std::array::from_fn(|i| if n > 0.0 { (s[i] / n) as u8 } else { (8 * (self.lo[i] + self.hi[i] + 1) / 2) as u8 })
    }
}

/// The library's queue: sorted (stably) only when something is taken out.
struct Queue {
    v: Vec<VBox>,
    sorted: bool,
    by_volume: bool,
}

impl Queue {
    fn push(&mut self, b: VBox) {
        self.v.push(b);
        self.sorted = false;
    }

    fn pop(&mut self) -> Option<VBox> {
        if !self.sorted {
            let vol = self.by_volume;
            self.v.sort_by_key(|b| if vol { b.count * b.volume() } else { b.count });
            self.sorted = true;
        }
        self.v.pop()
    }
}

fn cut(h: &[u32], b: &VBox) -> Option<(VBox, Option<VBox>)> {
    if b.count == 0 {
        return None;
    }
    if b.count == 1 {
        return Some((b.clone(), None));
    }
    let w: [i64; 3] = std::array::from_fn(|i| b.hi[i] - b.lo[i] + 1);
    let m = w.into_iter().max().unwrap_or(0);
    let axis = if m == w[0] { 0 } else if m == w[1] { 1 } else { 2 };
    let (o1, o2) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    // running totals along the long side, and what's left after each slice
    let mut part = [None::<i64>; 33];
    let mut total = 0;
    for l in b.lo[axis]..=b.hi[axis] {
        let mut s = 0;
        for u in b.lo[o1]..=b.hi[o1] {
            for c in b.lo[o2]..=b.hi[o2] {
                let mut p = [0; 3];
                p[axis] = l;
                p[o1] = u;
                p[o2] = c;
                s += h[at(p[0], p[1], p[2])] as i64;
            }
        }
        total += s;
        part[l as usize] = Some(total);
    }
    let get = |i: i64| if (0..33).contains(&i) { part[i as usize] } else { None };
    let (lo, hi) = (b.lo[axis], b.hi[axis]);
    for l in lo..=hi {
        if (get(l).unwrap_or(0) as f64) <= total as f64 / 2.0 {
            continue;
        }
        let (left, right) = ((l - lo) as f64, (hi - l) as f64);
        let mut a = if left <= right { (hi - 1).min((l as f64 + right / 2.0) as i64) } else { lo.max((l as f64 - 1.0 - left / 2.0) as i64) };
        while get(a).unwrap_or(0) == 0 {
            a += 1;
        }
        let mut left_over = total - get(a).unwrap_or(0);
        while left_over == 0 && get(a - 1).unwrap_or(0) != 0 {
            a -= 1;
            left_over = total - get(a).unwrap_or(0);
        }
        let (mut hi1, mut lo2) = (b.hi, b.lo);
        hi1[axis] = a;
        lo2[axis] = a + 1;
        return Some((VBox::new(b.lo, hi1, h), Some(VBox::new(lo2, b.hi, h))));
    }
    None
}

fn split(q: &mut Queue, h: &[u32], target: f64) {
    let (mut n, mut tries) = (1, 0);
    while tries < 1000 {
        let Some(b) = q.pop() else { return };
        if b.count == 0 {
            q.push(b);
            tries += 1;
            continue;
        }
        let Some((one, two)) = cut(h, &b) else { return };
        q.push(one);
        if let Some(two) = two {
            q.push(two);
            n += 1;
        }
        if n as f64 >= target {
            return;
        }
        tries += 1;
        if tries > 1001 {
            return;
        }
    }
}

/// Discord's colour for an avatar, from its pixels as straight (not premultiplied) rgba.
pub fn dominant(px: impl Iterator<Item = [u8; 4]>) -> Color32 {
    let mut h = vec![0u32; 32768];
    let (mut lo, mut hi) = ([1_000_000i64; 3], [0i64; 3]);
    let mut any = false;
    for [r, g, b, a] in px.step_by(10) {
        if a < 125 || (r > 250 && g > 250 && b > 250) {
            continue;
        }
        any = true;
        let c = [(r >> 3) as i64, (g >> 3) as i64, (b >> 3) as i64];
        h[at(c[0], c[1], c[2])] += 1;
        // the library only grows the top when the value isn't a new bottom
        for i in 0..3 {
            if c[i] < lo[i] {
                lo[i] = c[i];
            } else if c[i] > hi[i] {
                hi[i] = c[i];
            }
        }
    }
    if !any {
        return Color32::BLACK;
    }
    let mut by_count = Queue { v: vec![VBox::new(lo, hi, &h)], sorted: false, by_volume: false };
    split(&mut by_count, &h, 0.75 * 5.0);
    let mut by_volume = Queue { v: Vec::new(), sorted: false, by_volume: true };
    while let Some(b) = by_count.pop() {
        by_volume.push(b);
    }
    let left = 5.0 - by_volume.v.len() as f64;
    split(&mut by_volume, &h, left);
    let [r, g, b] = by_volume.pop().map(|b| b.avg(&h)).unwrap_or([0; 3]);
    Color32::from_rgb(r, g, b)
}

/// The colour for a loaded avatar, worked out once per url.
pub fn avatar(ctx: &egui::Context, url: &str) -> Option<Color32> {
    let key = egui::Id::new(("avatar_colour", url));
    if let Some(c) = ctx.data(|d| d.get_temp::<Color32>(key)) {
        return Some(c);
    }
    let Ok(egui::load::ImagePoll::Ready { image }) = ctx.try_load_image(url, egui::SizeHint::default()) else { return None };
    let c = dominant(image.pixels.iter().map(|p| p.to_srgba_unmultiplied()));
    ctx.data_mut(|d| d.insert_temp(key, c));
    Some(c)
}
