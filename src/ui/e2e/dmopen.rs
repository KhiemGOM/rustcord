//! Steps 5310 to 5319: the start of a DM. `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5310` runs
//! just this group.

use std::time::{Duration, Instant};

use egui::{Event, Modifiers, vec2};

use super::Run;
use crate::ui::App;

const ALICE: u64 = 101;
const ALICE_DM: u64 = 3000;
const BOB: u64 = 102;
const GROUP_DM: u64 = 3001;

impl Run {
    pub(super) fn dmopen_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5310 => {
                if let Some(u) = app.store.users.get_mut(&ALICE) {
                    u.font = 7;
                }
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
                if Self::said(ctx, "header:font").as_deref() == Some("name7") {
                    self.mark("dm_header_name_font");
                }
                if Self::said(ctx, "dm_start:styled").is_some() {
                    self.mark("dm_start_styled_heading");
                }
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
            5313 => {
                let who = app.store.channels.get(&GROUP_DM).and_then(|c| c.recipients.first().copied()).unwrap_or(BOB);
                self.loaded = who as usize;
                if let Some(u) = app.store.users.get_mut(&who) {
                    u.nameplate = Some("nameplates/mock/7002/".into());
                    u.decoration = Some("a_mock_decoration".into());
                }
                app.ui.hide_members = false;
                app.open(GROUP_DM);
                self.next(900);
            }
            5314 => {
                let who = self.loaded;
                let (Some(plate), Some(_)) = (Self::at(ctx, &format!("group:plate:{who}")), Self::at(ctx, &format!("group:decoration:{who}"))) else {
                    if stuck {
                        self.shot(ctx, "group_cosmetics_missing.png");
                        self.step = 5315;
                    }
                    return;
                };
                if plate.right() > ctx.content_rect().right() - 20.0 {
                    self.mark("group_member_cosmetics");
                }
                self.shot(ctx, "group_cosmetics.png");
                self.tries = 0;
                self.next(200);
            }
            5315 => {
                let screen = ctx.content_rect();
                let seen = |key: &str| Self::at(ctx, &format!("group_start:{key}")).filter(|r| screen.contains_rect(*r));
                let (Some(invite), Some(edit)) = (seen("invite"), seen("edit")) else {
                    if self.tries < 30 {
                        self.tries += 1;
                        self.queue.push_back(vec![Event::PointerMoved(screen.center())]);
                        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 150.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                        self.wait = Instant::now() + Duration::from_millis(250);
                    } else {
                        self.shot(ctx, "group_start_missing.png");
                        self.step = 5317;
                    }
                    return;
                };
                if (invite.center().y - edit.center().y).abs() < 0.6 && invite.right() < edit.left() {
                    self.mark("group_start");
                }
                self.shot(ctx, "group_start.png");
                self.click(edit.center());
                self.next(500);
            }
            5316 => {
                if matches!(app.user_dialog, Some(crate::ui::userdlg::Dialog::Group { .. })) {
                    self.mark("group_start_edit_opens");
                } else if !stuck {
                    return;
                }
                app.user_dialog = None;
                self.next(200);
            }
            _ => {
                if let Some(u) = app.store.users.get_mut(&(self.loaded as u64)).filter(|_| self.loaded != 0) {
                    u.nameplate = None;
                    u.decoration = None;
                }
                self.loaded = 0;
                if let Some(u) = app.store.users.get_mut(&ALICE) {
                    u.font = 0;
                }
                app.confirm = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5320 };
            }
        }
    }
}
