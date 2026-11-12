//! The channel list keeps its scroll per server, as Discord's does, so a row the script is about
//! to click can be scrolled out of sight: it is wheeled back into view first.

use std::time::{Duration, Instant};

use egui::{Event, Modifiers, MouseWheelUnit, Rect, TouchPhase, pos2, vec2};

use super::Run;
use crate::ui::App;

impl Run {
    /// A sidebar row's rect once it can be clicked. None while it is missing or being scrolled to.
    pub(super) fn reach_row(&mut self, app: &App, key: &str) -> Option<Rect> {
        let r = app.rects.get(key).copied()?;
        let top = crate::ui::titlebar::HEIGHT + 57.0;
        let bottom = ["game:panel", "userpanel"].iter().filter_map(|k| app.rects.get(*k)).map(|p| p.top()).fold(f32::INFINITY, f32::min) - 8.0;
        let y = r.center().y;
        if (top..=bottom).contains(&y) {
            return Some(r);
        }
        let delta = if y < top { top + 60.0 - y } else { bottom - 60.0 - y };
        self.queue.push_back(vec![Event::PointerMoved(pos2(r.center().x, top + 100.0))]);
        self.queue.push_back(vec![Event::MouseWheel { unit: MouseWheelUnit::Point, delta: vec2(0.0, delta), phase: TouchPhase::Move, modifiers: Modifiers::NONE }]);
        self.wait = Instant::now() + Duration::from_millis(350);
        None
    }
}
