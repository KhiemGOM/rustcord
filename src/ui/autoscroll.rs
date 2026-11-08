use egui::{CursorIcon, Event, Id, Key, LayerId, MouseWheelUnit, Order, PointerButton, Pos2, Shape, Stroke, TouchPhase, pos2, vec2};
use std::time::{Duration, Instant};

use crate::theme::*;

/// Middle-click autoscroll the way browsers do it: press the wheel, then move away from where
/// it went down to scroll that way, faster the further out. A quick click leaves it on until the
/// next click, Escape or a turn of the wheel; holding the wheel down scrolls only while held.
pub struct Auto {
    anchor: Pos2,
    at: Pos2,
    since: Instant,
    tick: Instant,
    toggled: bool,
}

// how far the pointer can stray from the anchor before anything moves
const DEAD: f32 = 15.0;

/// Runs on the raw input before egui sees it. While scrolling, egui is told the pointer stays at
/// the anchor, so the pane it started in keeps getting the scroll wherever the cursor wanders.
pub fn filter(st: &mut Option<Auto>, raw: &mut egui::RawInput) {
    let now = Instant::now();
    let mut out = Vec::with_capacity(raw.events.len() + 1);
    for e in raw.events.drain(..) {
        let Some(a) = st else {
            if let Event::PointerButton { button: PointerButton::Middle, pressed: true, pos, .. } = e {
                *st = Some(Auto { anchor: pos, at: pos, since: now, tick: now, toggled: false });
            } else {
                out.push(e);
            }
            continue;
        };
        match e {
            Event::PointerMoved(p) => {
                a.at = p;
                out.push(Event::PointerMoved(a.anchor));
            }
            // leaving the window doesn't stop it either
            Event::PointerGone => {}
            Event::PointerButton { button: PointerButton::Middle, pressed: false, pos, .. } if !a.toggled => {
                // a click rather than a drag: it stays on until the next click
                if now - a.since < Duration::from_millis(300) && pos.distance(a.anchor) < 8.0 {
                    a.toggled = true;
                } else {
                    *st = None;
                }
            }
            // the click that ends it is spent on that
            Event::PointerButton { pressed: true, .. } => *st = None,
            Event::Key { key: Key::Escape, pressed: true, .. } => *st = None,
            Event::MouseWheel { .. } => {
                *st = None;
                out.push(e);
            }
            _ => out.push(e),
        }
    }
    if let Some(a) = st {
        let dt = (now - a.tick).as_secs_f32().min(0.1);
        a.tick = now;
        let dy = a.at.y - a.anchor.y;
        if dy.abs() > DEAD {
            // chromium's autoscroll: 8e-6 * distance^2.2 px a millisecond
            let speed = dy.abs().powf(2.2) * 0.008;
            out.push(Event::MouseWheel { unit: MouseWheelUnit::Point, delta: vec2(0.0, -dy.signum() * speed * dt), phase: TouchPhase::Move, modifiers: egui::Modifiers::NONE });
        }
    }
    raw.events = out;
}

/// The marker where it started and the arrow cursor, on top of everything.
pub fn paint(ctx: &egui::Context, st: &Option<Auto>) {
    let Some(a) = st else { return };
    ctx.request_repaint();
    let dy = a.at.y - a.anchor.y;
    ctx.set_cursor_icon(if dy < -DEAD {
        CursorIcon::ResizeNorth
    } else if dy > DEAD {
        CursorIcon::ResizeSouth
    } else {
        CursorIcon::AllScroll
    });
    let p = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("autoscroll")));
    let c = a.anchor;
    p.circle(c, 14.0, pal().bg_floating, Stroke::new(1.0, pal().border_normal));
    p.circle_filled(c, 2.0, pal().header_primary);
    for dir in [-1.0, 1.0] {
        let tri = vec![pos2(c.x, c.y + dir * 10.0), pos2(c.x + 4.0, c.y + dir * 5.0), pos2(c.x - 4.0, c.y + dir * 5.0)];
        p.add(Shape::convex_polygon(tri, pal().header_primary, Stroke::NONE));
    }
}
