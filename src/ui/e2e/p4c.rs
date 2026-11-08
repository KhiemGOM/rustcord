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
const DEN: u64 = 1008;
const PANEL: u64 = 2802;
const TOWN_GUILD: u64 = 1003;
const TOWN_HALL: u64 = 2311;
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
                self.step = 5058;
                self.next(300);
            }
            5059 => {
                app.settings.stage_welcome = true;
                app.select_guild(Some(DEN));
                app.open(PANEL);
                self.next(800);
            }
            5060 => {
                app.join_voice(PANEL, true);
                self.next(500);
            }
            5061 => {
                if app.voice.chan != Some(PANEL) {
                    return;
                }
                let at = chrono::Utc::now().to_rfc3339();
                for u in [ALICE, BOB, 103] {
                    let d = json!({ "guild_id": DEN.to_string(), "channel_id": PANEL.to_string(), "user_id": u.to_string(), "session_id": format!("s{u}"), "suppress": true, "request_to_speak_timestamp": null, "self_mute": false, "self_deaf": false, "mute": false, "deaf": false });
                    let _ = &at;
                    app.rest("POST", "/mock/event".into(), Some(json!({ "t": "VOICE_STATE_UPDATE", "d": d })), crate::net::Tag::Quiet);
                }
                self.next(800);
            }
            5062 => {
                let me = app.store.me;
                let heard = crate::sound::heard().iter().any(|n| n == "stage_waiting");
                let audience = app.voice.stage.seats.get(&me).is_some_and(|s| s.audience);
                if !(app.stage_music() && heard && audience) || app.rects.get("stage:ctl:Mute Music").copied().is_none() || app.rects.get("voice:music").copied().is_none() {
                    if stuck {
                        self.shot(ctx, "stage_music_fail.png");
                        self.fail(format!(
                            "waiting in an unstarted stage: music {}, heard {heard}, audience {audience}, tray button {}, panel button {}, chan {:?}, stage_room {}, deaf {}, waiting {:?}, live {:?}",
                            app.stage_music(),
                            app.rects.get("stage:ctl:Mute Music").copied().is_some(),
                            app.rects.get("voice:music").copied().is_some(),
                            app.voice.chan,
                            app.voice.stage_room,
                            app.voice.deaf,
                            app.stage_waiting(),
                            app.voice.stage.live.keys().collect::<Vec<_>>()
                        ));
                    }
                    return;
                }
                self.mark("stage_music_plays");
                self.shot(ctx, "stage_tray_waiting.png");
                self.next(300);
            }
            5063 => {
                let Some(r) = app.rects.get("stage:ctl:Mute Music").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            5064 => {
                if !app.settings.stage_music_muted || app.stage_music() || app.rects.get("stage:ctl:Unmute Music").copied().is_none() {
                    if stuck {
                        self.fail("Mute Music in the tray left the music on".into());
                    }
                    return;
                }
                self.mark("stage_music_muted");
                self.shot(ctx, "stage_tray_muted.png");
                self.next(300);
            }
            5065 => {
                let Some(r) = app.rects.get("voice:music").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            5066 => {
                if app.settings.stage_music_muted || !app.stage_music() {
                    if stuck {
                        self.fail("Unmute Music in the voice panel left the music off".into());
                    }
                    return;
                }
                self.mark("stage_music_panel_unmuted");
                app.rest("POST", "/stage-instances".into(), Some(json!({ "channel_id": PANEL.to_string(), "topic": "music", "privacy_level": 2 })), crate::net::Tag::Quiet);
                self.next(800);
            }
            5067 => {
                if !app.voice.stage.live.contains_key(&PANEL) || app.stage_music() || app.rects.get("stage:ctl:Unmute Music").copied().is_none() {
                    if stuck {
                        self.fail("a live stage kept the waiting music, or its tray button isn't Unmute Music".into());
                    }
                    return;
                }
                self.mark("stage_music_stops_live");
                let r = app.rects.get("stage:ctl:Unmute Music").copied().unwrap();
                self.click(r.center());
                self.next(500);
            }
            5068 => {
                if !app.stage_music() || app.rects.get("stage:ctl:Mute Music").copied().is_none() {
                    if stuck {
                        self.fail("Unmute Music in a live stage didn't play the music".into());
                    }
                    return;
                }
                self.mark("stage_music_forced");
                self.shot(ctx, "stage_tray_live_music.png");
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            5069 => {
                if app.voice.chan.is_some() {
                    return;
                }
                if !app.stage_music() && app.call.music_forced.is_none() {
                    self.mark("stage_music_ends_on_leave");
                }
                app.rest("DELETE", format!("/stage-instances/{PANEL}"), None, crate::net::Tag::Quiet);
                app.select_guild(Some(TOWN_GUILD));
                app.open(TOWN_HALL);
                self.next(800);
            }
            5070 => {
                app.join_voice(TOWN_HALL, true);
                self.next(1200);
            }
            5071 => {
                let Some(r) = app.rects.get("stage:ctl:Request to Speak").copied() else {
                    if stuck {
                        self.fail("the stage tray's hand never read Request to Speak for a member with REQUEST_TO_SPEAK".into());
                    }
                    return;
                };
                self.mark("stage_hand_enabled");
                let _ = r;
                if let Some(g) = app.store.guilds.get_mut(&TOWN_GUILD)
                    && let Some(e) = g.roles.get_mut(&TOWN_GUILD)
                {
                    e.perms &= !(1 << 32);
                }
                self.next(300);
            }
            5072 => {
                let Some(r) = app.rects.get("stage:ctl:Raise Hand Disabled").copied() else {
                    if stuck {
                        self.fail("without REQUEST_TO_SPEAK the hand button never read Raise Hand Disabled".into());
                    }
                    return;
                };
                self.shot(ctx, "stage_hand_disabled.png");
                self.click(r.center());
                self.next(500);
            }
            5073 => {
                if !app.call.asked && !app.voice.stage.seats.get(&app.store.me).is_some_and(|s| s.hand.is_some()) {
                    self.mark("stage_hand_disabled");
                }
                if let Some(g) = app.store.guilds.get_mut(&TOWN_GUILD)
                    && let Some(e) = g.roles.get_mut(&TOWN_GUILD)
                {
                    e.perms |= 1 << 32;
                }
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            5074 => {
                if app.voice.chan.is_some() {
                    return;
                }
                for u in [ALICE, BOB, 103] {
                    app.store.apply("VOICE_STATE_UPDATE", &json!({ "guild_id": DEN.to_string(), "user_id": u.to_string(), "channel_id": null }));
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.select_guild(None);
                app.open(3000);
                self.next(1200);
            }
            5075 => {
                if app.rects.contains_key("dm:profile_off") {
                    self.fail("the DM's profile toggle was greyed at 1280px with no call".into());
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1100.0, 720.0)));
                self.next(900);
            }
            5076 => {
                if !app.rects.contains_key("dm:profile_off") {
                    if stuck {
                        self.fail("the DM's profile toggle stayed live below 1132px".into());
                    }
                    return;
                }
                self.mark("dm_profile_unavailable");
                self.shot(ctx, "dm_profile_off.png");
                self.next(500);
            }
            5077 => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(600);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5100 },
        }
    }
}
