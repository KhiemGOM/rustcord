//! Animation checks, steps 2600 and up: the curves against their control points, a modal's
//! scale-in seen mid-flight and settling, and Reduced Motion landing it at once.

use super::*;
use crate::ui::motion;

impl Run {
    pub(super) fn motion_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let t = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("settings").with("t")));
        match self.step {
            2600 => {
                let near = |a: f32, b: f32| (a - b).abs() < 2e-3;
                if near(motion::EASE_OUT.at(0.5), 0.6846) && near(motion::POP.duration(0.0), 0.097) {
                    self.mark("motion_curves");
                }
                app.settings_page = Some(crate::ui::settings::Page::Account);
                self.next(0);
            }
            2601 => match t {
                Some(t) if t > 0.05 && t < 0.95 => self.mark_once("motion_modal_mid"),
                Some(t) if t >= 0.999 && self.marks.iter().any(|(n, _)| n == "motion_modal_mid") => {
                    self.mark("motion_modal_settled");
                    app.settings_page = None;
                    self.next(400);
                }
                _ => {}
            },
            2602 => {
                app.settings.sync_motion = false;
                app.settings.reduced_motion = true;
                app.apply_settings(ctx);
                app.settings_page = Some(crate::ui::settings::Page::Account);
                self.next(0);
            }
            2603 => {
                let Some(t) = t else { return };
                if t >= 0.999 {
                    self.mark("motion_reduced_instant");
                }
                app.settings.reduced_motion = false;
                app.settings.sync_motion = true;
                app.apply_settings(ctx);
                app.settings_page = None;
                self.step = 3500;
            }
            _ => self.step = 99,
        }
    }
}
