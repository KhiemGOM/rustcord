use egui::{Context, Event, MouseWheelUnit, RawInput, TouchPhase, Vec2, vec2};
use std::time::Instant;
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWHEELSCROLLLINES, SystemParametersInfoW};

// Chromium's scroll animation (cc/animation/scroll_offset_animation_curve.cc): an ease-in-out
// cubic bezier, and a segment of 6 to 12 frames at 60Hz that gets shorter the further it goes.
// A wheel notch arriving mid-animation retargets it and keeps the speed it had.
const X1: f32 = 0.42;
const X2: f32 = 0.58;
const FRAME: f32 = 1.0 / 60.0;
// how long after the last motion egui is left believing a scroll is going on
const LINGER: f64 = 0.12;

#[derive(Clone, Copy)]
pub enum Kind {
    Wheel,
    Key,
}

impl Kind {
    // `px` is in physical pixels, which is what the ramp is tuned in
    fn duration(self, px: f32) -> f32 {
        match self {
            Kind::Wheel => (14.0 - px.abs() / 60.0).clamp(6.0, 12.0) * FRAME,
            Kind::Key => 9.0 * FRAME,
        }
    }
}

fn bez(a: f32, b: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
}

fn bez_slope(a: f32, b: f32, s: f32) -> f32 {
    let u = 1.0 - s;
    3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
}

fn solve(x: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if bez(X1, X2, mid) < x { lo = mid } else { hi = mid }
    }
    (lo + hi) * 0.5
}

#[derive(Clone, Copy)]
struct Curve {
    from: f32,
    to: f32,
    t0: f64,
    dur: f32,
    y1: f32,
}

impl Curve {
    fn u(&self, t: f64) -> f32 {
        ((t - self.t0) as f32 / self.dur).clamp(0.0, 1.0)
    }

    fn value(&self, t: f64) -> f32 {
        let u = self.u(t);
        if u >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * bez(self.y1, 1.0, solve(u))
    }

    fn velocity(&self, t: f64) -> f32 {
        let s = solve(self.u(t));
        bez_slope(self.y1, 1.0, s) / bez_slope(X1, X2, s) * (self.to - self.from) / self.dur
    }

    fn done(&self, t: f64) -> bool {
        t >= self.t0 + self.dur as f64
    }
}

#[derive(Default)]
pub struct Axis {
    curve: Option<Curve>,
    sent: f32,
}

impl Axis {
    pub fn push(&mut self, d: f32, t: f64, kind: Kind, ppp: f32) {
        let live = self.curve.filter(|c| !c.done(t));
        let Some(c) = live else {
            self.sent = 0.0;
            self.curve = Some(Curve { from: 0.0, to: d, t0: t, dur: kind.duration(d * ppp), y1: 0.0 });
            return;
        };
        let cur = c.value(t);
        let to = c.to + d;
        let rest = to - cur;
        if rest.abs() < 0.01 {
            self.curve = Some(Curve { from: to, to, t0: t, dur: 0.001, y1: 0.0 });
            return;
        }
        let vel = c.velocity(t);
        let bound = if vel.abs() < 0.01 || rest / vel < 0.0 { f32::MAX } else { rest / vel * 2.5 };
        let dur = kind.duration(rest * ppp).min(bound);
        if dur < 0.01 {
            self.curve = Some(Curve { from: to, to, t0: t, dur: 0.001, y1: 0.0 });
            return;
        }
        let slope = (vel * dur / rest).clamp(-1000.0, 1000.0);
        self.curve = Some(Curve { from: cur, to, t0: t, dur, y1: X1 * slope });
    }

    pub fn step(&mut self, t: f64) -> f32 {
        let Some(c) = self.curve else { return 0.0 };
        let v = c.value(t);
        let d = v - self.sent;
        if c.done(t) {
            self.curve = None;
            self.sent = 0.0;
        } else {
            self.sent = v;
        }
        d
    }

    pub fn animating(&self) -> bool {
        self.curve.is_some()
    }
}

pub struct Wheel {
    epoch: Instant,
    x: Axis,
    y: Axis,
    live: bool,
    last: f64,
}

impl Wheel {
    pub fn new() -> Self {
        Self { epoch: Instant::now(), x: Axis::default(), y: Axis::default(), live: false, last: 0.0 }
    }

    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Takes the line-unit wheel events winit reports and puts Chromium's scrolling in their
    /// place: a notch is lines*100/3 px animated with the curve above, shift turns it sideways,
    /// and the fractions a precision touchpad sends go through as they come, unsmoothed. egui
    /// is told they come from a touch so it adds no smoothing of its own.
    pub fn filter(&mut self, ctx: &Context, raw: &mut RawInput) {
        let t = self.now();
        let ppp = ctx.pixels_per_point();
        let mut out = Vec::with_capacity(raw.events.len() + 2);
        let mut direct = Vec2::ZERO;
        for e in raw.events.drain(..) {
            let Event::MouseWheel { unit: MouseWheelUnit::Line, delta, modifiers, .. } = e else {
                out.push(e);
                continue;
            };
            if modifiers.command {
                out.push(e);
                continue;
            }
            let notch = match lines() {
                u32::MAX => raw.screen_rect.map_or(600.0, |r| r.height() * 0.875),
                n => n as f32 * 100.0 / 3.0 / ctx.zoom_factor(),
            };
            let mut d = delta * notch;
            if modifiers.shift {
                d = vec2(d.x + d.y, 0.0);
            }
            if delta.abs().max_elem() < 1.0 {
                direct += d;
            } else {
                self.x.push(d.x, t, Kind::Wheel, ppp);
                self.y.push(d.y, t, Kind::Wheel, ppp);
            }
        }
        let d = direct + vec2(self.x.step(t), self.y.step(t));
        if d != Vec2::ZERO {
            if !self.live {
                self.live = true;
                out.push(Event::MouseWheel { unit: MouseWheelUnit::Point, delta: Vec2::ZERO, phase: TouchPhase::Start, modifiers: Default::default() });
            }
            self.last = t;
            out.push(Event::MouseWheel { unit: MouseWheelUnit::Point, delta: d, phase: TouchPhase::Move, modifiers: Default::default() });
        }
        if self.x.animating() || self.y.animating() {
            ctx.request_repaint();
        } else if self.live {
            if t - self.last > LINGER {
                self.live = false;
                out.push(Event::MouseWheel { unit: MouseWheelUnit::Point, delta: Vec2::ZERO, phase: TouchPhase::End, modifiers: Default::default() });
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(LINGER - (t - self.last) + 0.005));
            }
        }
        raw.events = out;
    }
}

fn lines() -> u32 {
    let mut n = 3u32;
    unsafe { SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &mut n as *mut u32 as *mut _, 0) };
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ax: &mut Axis, from: f64, to: f64) -> Vec<f32> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            out.push(ax.step(t));
            t += 1.0 / 60.0;
        }
        out
    }

    #[test]
    fn one_notch_lands_whole_in_200ms() {
        let mut ax = Axis::default();
        ax.push(100.0, 0.0, Kind::Wheel, 1.5);
        let steps = run(&mut ax, 0.0, 0.5);
        assert!((steps.iter().sum::<f32>() - 100.0).abs() < 0.01);
        assert!(!ax.animating());
        let at_200 = steps.iter().take(12).sum::<f32>();
        assert!(at_200 < 100.0 && at_200 > 90.0, "{at_200}");
    }

    #[test]
    fn ramp_matches_chromium() {
        assert!((Kind::Wheel.duration(100.0) - 0.2).abs() < 1e-4);
        assert!((Kind::Wheel.duration(300.0) - 9.0 / 60.0).abs() < 1e-4);
        assert!((Kind::Wheel.duration(900.0) - 0.1).abs() < 1e-4);
        assert!((Kind::Key.duration(5000.0) - 0.15).abs() < 1e-4);
    }

    #[test]
    fn retarget_keeps_moving_and_lands_on_the_sum() {
        let mut ax = Axis::default();
        ax.push(100.0, 0.0, Kind::Wheel, 1.5);
        let mut total = 0.0;
        let mut t = 0.0;
        while t < 0.1 {
            total += ax.step(t);
            t += 1.0 / 60.0;
        }
        ax.push(100.0, t, Kind::Wheel, 1.5);
        let rest = run(&mut ax, t, t + 0.6);
        assert!(rest[1] > 0.0);
        assert!(rest.iter().all(|d| *d >= 0.0));
        assert!((total + rest.iter().sum::<f32>() - 200.0).abs() < 0.01);
    }
}
