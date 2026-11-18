//! Steps 5310 to 5319: the start of a DM. `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5310` runs
//! just this group.

use std::time::{Duration, Instant};

use egui::{Event, Modifiers, vec2};

use super::Run;
use crate::ui::App;

const ALICE_DM: u64 = 3000;

impl Run {
    pub(super) fn dmopen_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5310 => {
                app.select_guild(None);
                app.open(ALICE_DM);
                self.tries = 0;
                self.next(900);
            }
            5311 => {
                let screen = ctx.content_rect();
                let seen = |key: &str| Self::at(ctx, &format!("dm_start:{key}")).filter(|r| screen.contains_rect(*r));
                let first = ["Remove Friend", "Add Friend", "Accept Friend Request", "Friend Request Sent"].into_iter().find_map(seen);
                let (Some(first), Some(block), Some(servers)) = (first, seen("Block"), seen("servers")) else {
                    // the block sits above the oldest message
                    if self.tries < 30 {
                        self.tries += 1;
                        self.queue.push_back(vec![Event::PointerMoved(screen.center())]);
                        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 150.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                        self.wait = Instant::now() + Duration::from_millis(250);
                    } else {
                        self.shot(ctx, "dm_start_missing.png");
                        self.step = 5313;
                    }
                    return;
                };
                let near = |a: f32, b: f32| (a - b).abs() < 0.6;
                if near(block.height(), 32.0) && near(first.height(), 32.0) && near(block.left() - first.right(), 7.0) && servers.right() < first.left() && near(servers.center().y, first.center().y) {
                    self.mark("dm_start_row");
                }
                self.shot(ctx, "dm_start.png");
                self.click(block.center());
                self.next(500);
            }
            5312 => {
                if app.confirm.as_ref().is_some_and(|c| c.title.starts_with("Block ")) {
                    self.mark("dm_start_block_asks");
                } else if !stuck {
                    return;
                }
                app.confirm = None;
                self.next(200);
            }
            _ => {
                app.confirm = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5400 };
            }
        }
    }
}
