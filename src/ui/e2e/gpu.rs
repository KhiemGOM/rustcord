//! Steps 5450 to 5459: the graphics device is lost and the window comes back as it was.
//! `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5450` runs just this group.

use serde_json::json;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn gpu_steps(&mut self, _app: &mut App, ctx: &egui::Context) {
        match self.step {
            5450 => {
                self.shot(ctx, "gpu_before.png");
                self.next(500);
            }
            5451 => {
                if self.pending_shots > 0 {
                    return;
                }
                self.rects.insert("gpu_resets_before".into(), json!(eframe::device_resets()));
                eframe::lose_device();
                ctx.request_repaint();
                self.next(1500);
            }
            5452 => {
                self.shot(ctx, "gpu_after.png");
                self.next(500);
            }
            5453 => {
                if self.pending_shots > 0 {
                    return;
                }
                let before = self.rects.get("gpu_resets_before").and_then(|v| v.as_u64()).unwrap_or(0);
                let resets = eframe::device_resets() as u64 - before;
                let ppp = ctx.pixels_per_point();
                let off = self.changed("gpu_before.png", "gpu_after.png", ctx.content_rect(), ppp);
                let area = (ctx.content_rect().area() * ppp * ppp) as usize;
                self.rects.insert("gpu_reset".into(), json!({ "resets": resets, "pixels_changed": off, "pixels": area }));
                if resets == 1 && off * 200 < area {
                    self.mark("gpu_reset_redraws");
                }
                self.next(0);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
        }
    }
}
