//! Discord's easings, react-spring springs and looping animations, with Reduced Motion
//! honoured in one place: `theme::anim` turns every duration into zero, so a transition lands
//! at once. Nothing here asks for a repaint unless it is mid-flight.

use egui::emath::TSTransform;
use egui::{Color32, Context, Frame, Id, Response, Ui};
use std::time::Duration;

use crate::theme::anim;

#[derive(Clone, Copy)]
pub enum Curve {
    Bezier(f32, f32, f32, f32),
    Spring { k: f32, c: f32, m: f32, clamp: bool },
}

pub const EASE: Curve = Curve::Bezier(0.25, 0.1, 0.25, 1.0);
pub const EASE_IN: Curve = Curve::Bezier(0.42, 0.0, 1.0, 1.0);
pub const EASE_OUT: Curve = Curve::Bezier(0.0, 0.0, 0.58, 1.0);
pub const EASE_IN_OUT: Curve = Curve::Bezier(0.42, 0.0, 0.58, 1.0);
pub const POP: Curve = Curve::Spring { k: 500.0, c: 18.0, m: 1.0, clamp: true };
pub const SWITCH: Curve = Curve::Spring { k: 800.0, c: 32.0, m: 1.0, clamp: false };
pub const TOOLTIP: Curve = Curve::Spring { k: 2400.0, c: 52.0, m: 1.0, clamp: false };

fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let at = |a: f32, b: f32, s: f32| 3.0 * a * s * (1.0 - s) * (1.0 - s) + 3.0 * b * s * s * (1.0 - s) + s * s * s;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        if at(x1, x2, mid) < x { lo = mid } else { hi = mid }
    }
    at(y1, y2, (lo + hi) / 2.0)
}

fn spring(k: f32, c: f32, m: f32, t: f32) -> f32 {
    let w = (k / m).sqrt();
    let z = c / (2.0 * (k * m).sqrt());
    if z < 1.0 {
        let wd = w * (1.0 - z * z).sqrt();
        1.0 - (-z * w * t).exp() * ((wd * t).cos() + z * w / wd * (wd * t).sin())
    } else if z > 1.0 {
        let s = (z * z - 1.0).sqrt();
        let (r1, r2) = (-w * (z - s), -w * (z + s));
        1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
    } else {
        1.0 - (-w * t).exp() * (1.0 + w * t)
    }
}

impl Curve {
    /// How long the curve runs: a CSS transition's own length, or a spring's time to rest
    /// (react-spring: first crossing of 1 when clamped, else position within .001 and velocity
    /// within .1 a second of rest).
    pub fn duration(self, css: f32) -> f32 {
        let Curve::Spring { k, c, m, clamp } = self else { return css };
        let w = (k / m).sqrt();
        let z = c / (2.0 * (k * m).sqrt());
        if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            if clamp {
                return (std::f32::consts::PI - ((1.0 - z * z).sqrt() / z).atan()) / wd;
            }
            let pos = (1.0 + (z * w / wd).powi(2)).sqrt() / 0.001;
            let vel = w / (1.0 - z * z).sqrt() / 0.1;
            return pos.max(vel).ln() / (z * w);
        }
        let slow = w * (z - (z * z - 1.0).max(0.0).sqrt());
        (w / 0.1).max(1000.0).ln() / slow
    }

    /// Progress for `u` in 0..=1 of the curve's length.
    pub fn at(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Curve::Bezier(a, b, c, d) => bezier(a, b, c, d, u),
            Curve::Spring { k, c, m, clamp } => {
                if u >= 1.0 {
                    return 1.0;
                }
                let x = spring(k, c, m, u * self.duration(0.0));
                if clamp { x.min(1.0) } else { x }
            }
        }
    }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// Eases 0 to 1 as `on` flips and back again, over `secs` (CSS transition length).
pub fn tween(ctx: &Context, id: Id, on: bool, secs: f32, curve: Curve) -> f32 {
    curve.at(ctx.animate_bool_with_time(id, on, anim(curve.duration(secs))))
}

/// A number that glides to `target` whenever it changes, like a CSS transition on height.
pub fn follow(ctx: &Context, id: Id, target: f32, secs: f32, curve: Curve) -> f32 {
    let secs = anim(curve.duration(secs));
    let now = ctx.input(|i| i.time);
    let (from, to, start) = ctx.data(|d| d.get_temp::<(f32, f32, f64)>(id)).unwrap_or((target, target, now - 1e3));
    let u = if secs == 0.0 { 1.0 } else { ((now - start) as f32 / secs).clamp(0.0, 1.0) };
    let here = from + (to - from) * curve.at(u);
    if to == target {
        if u < 1.0 {
            ctx.request_repaint();
        }
        return here;
    }
    ctx.data_mut(|d| d.insert_temp(id, (here, target, now)));
    if secs == 0.0 {
        return target;
    }
    ctx.request_repaint();
    here
}

/// A hover transition on a widget's own response.
pub fn hover(ctx: &Context, resp: &Response, secs: f32) -> f32 {
    tween(ctx, resp.id.with("hover"), resp.hovered(), secs, EASE)
}

/// Discord's button hover: colours move over .05s ease-in.
pub fn hot(ctx: &Context, resp: &Response) -> f32 {
    tween(ctx, resp.id.with("hot"), resp.hovered(), 0.05, EASE_IN)
}

/// Seconds-long enter progress for something drawn every frame while it is open, restarting
/// when a frame went by without it. Returns 1 once it has settled.
pub fn enter(ctx: &Context, id: Id, secs: f32, curve: Curve) -> f32 {
    let secs = anim(curve.duration(secs));
    if secs == 0.0 {
        return 1.0;
    }
    let now = ctx.input(|i| i.time);
    let key = id.with("enter");
    let frame = ctx.cumulative_frame_nr();
    let (start, last) = ctx.data(|d| d.get_temp::<(f64, u64)>(key)).unwrap_or((now, frame));
    let start = if frame > last + 1 { now } else { start };
    ctx.data_mut(|d| d.insert_temp(key, (start, frame)));
    let u = ((now - start) as f32 / secs).clamp(0.0, 1.0);
    if u < 1.0 {
        ctx.request_repaint();
    }
    curve.at(u)
}

/// Position in a loop of `period` seconds, 0..1. Draws every frame; a caller whose loop is
/// not on screen must not call it. Reduced Motion freezes it at 0.
pub fn cycle(ctx: &Context, period: f32) -> f32 {
    if anim(1.0) == 0.0 {
        return 0.0;
    }
    ctx.request_repaint_after_secs(1.0 / 60.0);
    (ctx.input(|i| i.time) as f32 % period) / period
}

/// Discord's three pulsing dots (the typing line, "loading"): opacity and scale of dot `i`.
/// `steps(12)` runs inside each keyframe gap, so it moves in 25ms jumps and rests half the loop;
/// frames come every 50ms while it moves, since a step is under 6% of the opacity, and not at all
/// while it rests.
pub fn dots(ctx: &Context, i: usize) -> (f32, f32) {
    let period = if anim(1.0) == 0.0 { 2.4 } else { 1.2 };
    let t = ctx.input(|i| i.time) as f32;
    let p = (t - 0.15 * i as f32).rem_euclid(period) / period;
    ctx.request_repaint_after_secs(if p < 0.5 { 0.05 } else { (period - p * period).max(0.001) });
    let (gap, local) = if p < 0.25 { (0.25, p) } else if p < 0.5 { (0.25, p - 0.25) } else { (0.5, 0.0) };
    let stepped = (local / gap * 12.0).floor() / 12.0;
    let q = if p < 0.5 { (p - local) + stepped * gap } else { p };
    (keys(&[(0.0, 0.32), (0.25, 1.0), (0.5, 0.32), (1.0, 0.32)], q), keys(&[(0.0, 0.8), (0.25, 1.0), (0.5, 0.8), (1.0, 0.8)], q))
}

/// Discord's popout entrance: the card starts 10px off towards `away` and slides home in
/// 0.2s ease-out. egui fades it in on its own.
pub fn popout(ctx: &Context, area: egui::Area, away: egui::Vec2) -> egui::Area {
    slide(ctx, area.layer(), away);
    area
}

pub fn slide(ctx: &Context, layer: egui::LayerId, away: egui::Vec2) {
    let t = enter(ctx, layer.id, 0.2, EASE_OUT);
    ctx.set_transform_layer(layer, TSTransform::from_translation(away * 10.0 * (1.0 - t)));
}

/// Discord's full-screen layer (server and channel settings): scale 1.1 down to 1 about the
/// middle of `rect`, 0.2s ease-in-out; egui fades it in on its own.
pub fn layer(ctx: &Context, area: egui::Area, rect: egui::Rect) -> egui::Area {
    let k = 1.0 + 0.1 * (1.0 - enter(ctx, area.layer().id, 0.2, EASE_IN_OUT));
    ctx.set_transform_layer(area.layer(), TSTransform::new(rect.center().to_vec2() * (1.0 - k), k));
    area
}

/// Discord's tooltip: scale .95 to 1 on a {tension 2400, friction 52} spring, over egui's own
/// fade. Only the tooltips egui raises for hovered widgets; the app's own cards have their ids.
pub fn tooltips(ctx: &Context) {
    const OWN: [&str; 7] = ["toast", "discodo", "role_picker", "tag_pop", "role_pop", "rail_ghost", "titlebar_over"];
    let layers: Vec<egui::LayerId> =
        ctx.memory(|m| m.layer_ids().filter(|l| l.order == egui::Order::Tooltip && m.areas().is_visible(l)).collect());
    for l in layers {
        if OWN.iter().any(|n| l.id == Id::new(*n)) {
            continue;
        }
        let Some(r) = ctx.memory(|m| m.area_rect(l.id)) else { continue };
        let s = 0.95 + 0.05 * enter(ctx, l.id.with("tip"), 0.2, TOOLTIP);
        ctx.set_transform_layer(l, TSTransform::new(r.center().to_vec2() * (1.0 - s), s));
    }
}

/// A disclosure chevron: points down open, turns to point right over .2s ease-out.
pub fn chevron(ui: &Ui, id: Id, open: bool, rect: egui::Rect, color: Color32) {
    let turn = (1.0 - tween(ui.ctx(), id.with("chev"), open, 0.2, EASE_OUT)) * -std::f32::consts::FRAC_PI_2;
    crate::ui::icons::draw_turned(ui.painter(), crate::ui::icons::Icon::ChevronDown, rect, color, turn);
}

/// A hover wash that fades in and out instead of switching.
pub fn wash(ui: &Ui, resp: &Response, rect: egui::Rect, radius: impl Into<egui::CornerRadius>, color: Color32, secs: f32) {
    let k = hover(ui.ctx(), resp, secs);
    if k > 0.0 {
        ui.painter().rect_filled(rect, radius, color.gamma_multiply(k));
    }
}

/// Discord's spinning circle: a ring that turns once every 2s while its arc grows and then
/// chases its own tail. Reduced Motion swaps it for a still arc that pulses.
pub fn spinner(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint_spinner(ui, rect, color);
    resp
}

pub fn paint_spinner(ui: &Ui, rect: egui::Rect, color: Color32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let size = rect.width();
    let ctx = ui.ctx();
    let (turn, a, b, alpha) = if anim(1.0) == 0.0 {
        ctx.request_repaint_after(Duration::from_millis(50));
        (0.0, 0.0, 0.75, keys(&[(0.0, 1.0), (0.5, 0.6), (1.0, 1.0)], (ctx.input(|i| i.time) as f32 % 1.4) / 1.4))
    } else {
        let p = cycle(ctx, 2.0);
        let len = 125.66;
        let (dash, off) = if p < 0.5 { (1.0 + 129.0 * EASE_IN_OUT.at(p * 2.0), 0.0) } else { (130.0, 124.0 * EASE_IN_OUT.at(p * 2.0 - 1.0)) };
        (p, off / len, ((off + dash) / len).min(1.0), 1.0)
    };
    let r = size * 0.4;
    let pts: Vec<egui::Pos2> = (0..=40)
        .map(|i| {
            let ang = (turn + a + (b - a) * i as f32 / 40.0) * std::f32::consts::TAU;
            rect.center() + egui::vec2(ang.cos(), ang.sin()) * r
        })
        .collect();
    ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(size * 0.08, color.gamma_multiply(alpha))));
}

/// Discord's pulsing ellipsis ("connecting"): three 6px dots, 2px apart, each pulsing 1.4s
/// ease-in-out and 0.2s behind the one before. Reduced Motion only dims them.
pub fn ellipsis(ui: &Ui, left_center: egui::Pos2, color: Color32) {
    let ctx = ui.ctx();
    let low = anim(1.0) == 0.0;
    ctx.request_repaint_after(Duration::from_millis(32));
    for i in 0..3 {
        let p = (ctx.input(|i| i.time) as f32 - 0.2 * i as f32).rem_euclid(1.4) / 1.4;
        let u = if p < 0.5 { EASE_IN_OUT.at(p * 2.0) } else { 1.0 - EASE_IN_OUT.at(p * 2.0 - 1.0) };
        let (alpha, scale) = if low { (1.0 - 0.4 * u, 1.0) } else { (1.0 - 0.7 * u, 1.0 - 0.2 * u) };
        let c = left_center + egui::vec2(3.0 + 8.0 * i as f32, 0.0);
        ui.painter().rect_filled(egui::Rect::from_center_size(c, egui::vec2(6.0, 6.0) * scale), 3.0, color.gamma_multiply(alpha));
    }
}

/// A keyframe track of (offset, value) pairs, linear between them.
pub fn keys(frames: &[(f32, f32)], u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    for w in frames.windows(2) {
        let ((a, va), (b, vb)) = (w[0], w[1]);
        if u <= b {
            return if b == a { vb } else { va + (vb - va) * (u - a) / (b - a) };
        }
    }
    frames.last().map_or(0.0, |f| f.1)
}

/// Discord's modal: a dimmed backdrop that fades and a card that scales up from the middle,
/// in place of `egui::Modal`, which can only fade both together.
pub struct Modal {
    id: Id,
    frame: Option<Frame>,
    dim: Color32,
    from: f32,
    secs: f32,
    curve: Curve,
}

pub struct Shown {
    pub response: Response,
    closing: bool,
}

impl Shown {
    pub fn should_close(&self) -> bool {
        self.closing
    }
}

impl Modal {
    pub fn new(id: Id) -> Self {
        Self { id, frame: None, dim: Color32::from_black_alpha(100), from: 0.85, secs: 0.1, curve: POP }
    }

    pub fn frame(mut self, frame: Frame) -> Self {
        self.frame = Some(frame);
        self
    }

    pub fn backdrop_color(mut self, dim: Color32) -> Self {
        self.dim = dim;
        self
    }

    /// Discord's full-screen layers (settings): scale 1.1 down to 1, ease-in-out.
    pub fn layer(mut self) -> Self {
        self.from = 1.1;
        self.curve = EASE_IN_OUT;
        self.secs = 0.2;
        self
    }

    pub fn show<T>(self, ctx: &Context, add: impl FnOnce(&mut Ui) -> T) -> Shown {
        let t = enter(ctx, self.id, self.secs, self.curve);
        ctx.data_mut(|d| d.insert_temp(self.id.with("t"), t));
        let screen = ctx.content_rect();
        let area = egui::Modal::default_area(self.id).fade_in(false);
        let layer = area.layer();
        let top = ctx.memory_mut(|m| {
            m.set_modal_layer(layer);
            m.top_modal_layer() == Some(layer)
        });
        let popup_open = egui::Popup::is_any_open(ctx);
        let s = self.from + (1.0 - self.from) * t;
        let c = screen.center();
        ctx.set_transform_layer(layer, TSTransform::new((c.to_vec2()) * (1.0 - s), s));
        egui::Area::new(self.id.with("dim"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(screen.min)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, self.dim.gamma_multiply(t.clamp(0.0, 1.0)));
            });
        let frame = self.frame;
        let out = area.show(ctx, |ui| {
            ui.multiply_opacity(t.clamp(0.0, 1.0));
            let frame = frame.unwrap_or_else(|| Frame::popup(ui.style()));
            frame.show(ui, add);
        });
        let outside = |p: Option<egui::Pos2>| p.is_some_and(|p| !out.response.rect.contains(p));
        let key = self.id.with("press");
        if ctx.input(|i| i.pointer.any_pressed()) {
            let began = ctx.input(|i| outside(i.pointer.interact_pos()));
            ctx.data_mut(|d| d.insert_temp(key, began));
        }
        let pressed_out = ctx.input(|i| i.pointer.any_released()) && {
            let began = ctx.data_mut(|d| d.remove_temp::<bool>(key)).unwrap_or(false);
            began && ctx.input(|i| outside(i.pointer.interact_pos()))
        };
        let esc = top && !popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let closing = (pressed_out && top && !popup_open) || esc || out.response.should_close();
        Shown { response: out.response, closing }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressing_with_a_modal_open_does_not_hang() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ctx = Context::default();
            let press = egui::Event::PointerButton { pos: egui::pos2(1.0, 1.0), button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() };
            let raw = egui::RawInput { events: vec![press], ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| {
                Modal::new(Id::new("m")).show(ui.ctx(), |_| {});
            });
            let _ = tx.send(());
        });
        assert!(rx.recv_timeout(Duration::from_secs(30)).is_ok());
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn css_curves_match_their_control_points() {
        for (c, mid) in [(EASE, 0.8024), (EASE_IN, 0.3154), (EASE_OUT, 0.6846), (EASE_IN_OUT, 0.5)] {
            assert!(close(c.at(0.0), 0.0, 1e-4) && close(c.at(1.0), 1.0, 1e-4));
            assert!(close(c.at(0.5), mid, 1e-3), "{} vs {mid}", c.at(0.5));
        }
        assert!(close(EASE_OUT.at(0.25), 0.3781, 1e-3) && close(EASE_OUT.at(0.75), 0.9065, 1e-3));
    }

    #[test]
    fn pop_spring_first_reaches_one_at_its_clamp() {
        let (w, z) = (500f32.sqrt(), 18.0 / (2.0 * 500f32.sqrt()));
        let wd = w * (1.0 - z * z).sqrt();
        assert!(close(z, 0.4025, 1e-4) && close(wd, 20.469, 1e-3));
        let t = POP.duration(0.0);
        assert!(close(t, 0.09698, 1e-4), "{t}");
        assert!(close(spring(500.0, 18.0, 1.0, t), 1.0, 1e-4));
        assert!(spring(500.0, 18.0, 1.0, t - 0.005) < 0.99);
        let peak = std::f32::consts::PI / wd;
        assert!(close(peak, 0.15348, 1e-4));
        assert!(close(spring(500.0, 18.0, 1.0, peak), 1.2513, 1e-3));
        assert!(POP.at(0.99) <= 1.0);
    }

    #[test]
    fn unclamped_springs_overshoot_then_rest() {
        let peak = 1.0 + (-0.5657f32 * std::f32::consts::PI / (1.0 - 0.5657f32.powi(2)).sqrt()).exp();
        assert!(close(peak, 1.1159, 1e-3));
        let top = (0..300).map(|i| SWITCH.at(i as f32 / 300.0)).fold(0.0f32, f32::max);
        assert!(close(top, peak, 5e-3), "{top}");
        assert!(close(SWITCH.duration(0.0), 0.4426, 5e-3), "{}", SWITCH.duration(0.0));
        assert!(close(TOOLTIP.at(1.0), 1.0, 1e-6));
    }

    fn at(ctx: &Context, time: f64, draw: bool) -> f32 {
        let mut out = 0.0;
        let _ = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| {
            if draw {
                out = enter(ui.ctx(), Id::new("probe"), 0.2, EASE);
            }
        });
        out
    }

    #[test]
    fn enter_settles_across_idle_gaps_and_restarts_after_a_missed_frame() {
        let ctx = Context::default();
        assert!(at(&ctx, 0.0, true) < 0.01);
        assert!(close(at(&ctx, 0.5, true), 1.0, 1e-6));
        assert!(close(at(&ctx, 1.0, true), 1.0, 1e-6));
        at(&ctx, 1.1, false);
        at(&ctx, 1.2, false);
        assert!(at(&ctx, 1.3, true) < 0.01);
    }

    #[test]
    fn every_dialog_opens_through_motion_and_closes_on_its_backdrop() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") && p.file_name().is_some_and(|n| n != "motion.rs") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut bad = Vec::new();
        let mut seen = 0;
        for f in &files {
            let src = std::fs::read_to_string(f).unwrap();
            let lines: Vec<&str> = src.lines().collect();
            for (i, l) in lines.iter().enumerate().filter(|(_, l)| l.contains("Modal::new(")) {
                seen += 1;
                if !l.contains("motion::Modal::new(") {
                    bad.push(format!("{}:{}: not motion::Modal", f.display(), i + 1));
                }
                let var = l.trim().strip_prefix("let ").map(|r| r.trim_start_matches("mut ").split([' ', ':']).next().unwrap_or(""));
                let till = lines[i + 1..].iter().position(|l| l.contains("Modal::new(")).map_or(lines.len(), |n| i + 1 + n);
                if !var.is_some_and(|v| lines[i..till].iter().any(|l| l.contains(&format!("{v}.should_close()")))) {
                    bad.push(format!("{}:{}: never reads should_close", f.display(), i + 1));
                }
            }
        }
        assert!(seen > 30, "only found {seen} dialogs");
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn keys_interpolate() {
        let f = [(0.0, 0.32), (0.25, 1.0), (0.5, 0.32), (1.0, 0.32)];
        assert!(close(keys(&f, 0.125), 0.66, 1e-3));
        assert!(close(keys(&f, 0.9), 0.32, 1e-3));
    }
}
