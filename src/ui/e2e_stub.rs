use std::time::{Duration, Instant};

use super::App;
use crate::state::Id;

pub struct Run {
    pub tour: bool,
    pub opened: Vec<String>,
    pub pinged: Vec<Id>,
    pub pings: Vec<(String, (Id, Id))>,
    pub call_toasts: Vec<String>,
    pub said: Vec<String>,
    pub heard: Vec<serde_json::Value>,
}

impl Run {
    pub fn from_env(_: Instant) -> Option<Run> {
        None
    }
    pub fn hidden(&mut self, _: &mut App, _: &egui::Context) {}
    pub fn cost(&mut self, _: &str, _: Duration) {}
    pub fn mark(&mut self, _: &str) {}
    pub fn mark_once(&mut self, _: &str) {}
    pub fn screenshots(&mut self, _: &egui::Context) {}
    pub fn memory(&mut self) {}
    pub fn step(&mut self, _: &mut App, _: &egui::Context) {}
    pub fn frame_time(&mut self, _: Duration) {}
    pub fn inject(&mut self, _: &mut egui::RawInput) {}
}

pub fn beat() {}

pub fn scrolled<T>(_: &egui::Context, _: &egui::scroll_area::ScrollAreaOutput<T>) {}

pub mod stress {
    pub fn record(_: &egui::Context, _: &str, _: egui::Rect) {}
}
