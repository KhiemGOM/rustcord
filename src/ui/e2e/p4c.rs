//! Steps 5050 to 5099: P4c. What each person in a call is played at, synced with the account
//! (audio_context_settings): a click on the User Volume slider reaches the account, and another
//! device's change reaches the mixer. `python e2e/from_step.py 5050` runs just this group.

use std::time::Duration;

use egui::{ViewportCommand, pos2, vec2};
use serde_json::json;

use super::Run;
use crate::ui::App;
use crate::ui::usermenu::Origin;

const ALICE: u64 = 101;
const BOB: u64 = 102;

fn gain(app: &App, user: u64) -> Option<f32> {
    app.voice.shared.peers.lock().ok().and_then(|p| p.get(&user).copied())
}

impl Run {
    fn volume_click(&mut self, ctx: &egui::Context, along: f32) -> bool {
        let Some(r) = Self::at(ctx, "menu_row:User Volume") else { return false };
        self.click(pos2(r.left() + 8.0 + along * (r.width() - 16.0), r.top() + 42.0));
        true
    }

    pub(super) fn p4c_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(8);
        match self.step {
            5050 => {
                if app.conn != crate::ui::Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.select_guild(Some(self.target.0));
                app.user_menu = Some((ALICE, self.target.0, None, pos2(640.0, 300.0)));
                app.menu_from = Origin::Voice(2021);
                self.next(700);
            }
            5051 => {
                if self.volume_click(ctx, 0.75) {
                    self.next(600);
                }
            }
            5052 => {
                let heard = app.settings.peers.get(&ALICE).map(|p| p.volume);
                if !heard.is_some_and(|v| (140.0..142.0).contains(&v)) || !gain(app, ALICE).is_some_and(|g| (1.4..1.42).contains(&g)) {
                    if stuck {
                        self.fail(format!("a click on the User Volume slider left alice at {heard:?}, mixer {:?}", gain(app, ALICE)));
                    }
                    return;
                }
                self.mark("peer_volume_clicked");
                self.shot(ctx, "peer_volume.png");
                self.next(300);
            }
            5053 => {
                if self.volume_click(ctx, 0.5) {
                    self.next(600);
                }
            }
            5054 => {
                if app.settings.peers.contains_key(&ALICE) {
                    if stuck {
                        self.fail("the slider at 100% kept alice among the peoples' volumes".into());
                    }
                    return;
                }
                self.mark("peer_volume_reset");
                app.user_menu = None;
                app.menu_from = Origin::Plain;
                app.rest("POST", "/mock/peers".into(), Some(json!({ "users": [{ "id": BOB, "volume": 35.0, "muted": true }] })), crate::net::Tag::Quiet);
                self.next(800);
            }
            5055 => {
                let p = app.settings.peers.get(&BOB).copied();
                if !p.is_some_and(|p| p.volume == 35.0 && p.muted) || gain(app, BOB) != Some(0.0) {
                    if stuck {
                        self.fail(format!("another device muted bob at 35% and this one has {:?}", p.map(|p| (p.volume, p.muted))));
                    }
                    return;
                }
                self.mark("peer_remote_applied");
                app.rest("POST", "/mock/peers".into(), Some(json!({ "users": [] })), crate::net::Tag::Quiet);
                self.next(800);
            }
            5056 => {
                if !app.settings.peers.is_empty() || gain(app, BOB).is_some() {
                    if stuck {
                        self.fail("another device resetting every volume left some here".into());
                    }
                    return;
                }
                self.mark("peer_remote_cleared");
                self.next(300);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
        }
    }
}
