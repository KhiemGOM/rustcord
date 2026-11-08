//! Steps 5100 to 5199: the controls nobody had clicked, the Active Now stream row, and the rest
//! of the leftovers list. `python e2e/from_step.py 5100` runs just this group.

use std::time::Duration;

use egui::{ViewportCommand, pos2, vec2};
use serde_json::json;

use super::Run;
use crate::ui::{App, applications, community};

const STREAMER: u64 = 103;
const STREAM_CHAN: u64 = 2021;
const OWNED: u64 = 1001;
const JOINS_FEATURE: &str = "MEMBER_VERIFICATION_MANUAL_APPROVAL";

fn menu(ctx: &egui::Context, label: &str) -> Option<egui::Rect> {
    Run::at(ctx, &format!("menu_row:{label}"))
}

fn first_more(app: &App) -> Option<egui::Rect> {
    let id = app.rects.keys().filter_map(|k| k.strip_prefix("joins:more:")).min()?;
    app.rects.get(&format!("joins:more:{id}")).copied()
}

impl Run {
    pub(super) fn p5_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5100 => {
                if app.voice.chan.is_some() {
                    app.voice.leave(&app.net);
                }
                app.store.apply("VOICE_STATE_UPDATE", &json!({ "guild_id": self.target.0.to_string(), "user_id": STREAMER.to_string(), "channel_id": STREAM_CHAN.to_string(), "self_stream": true }));
                app.select_guild(None);
                app.friends = true;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                self.next(1300);
            }
            5101 => {
                let Some(shot) = Self::at(ctx, &format!("now_stream:{STREAMER}")) else { return };
                let screen = ctx.content_rect();
                if shot.bottom() > screen.bottom() - 8.0 || shot.top() < screen.top() + 40.0 {
                    if stuck {
                        self.shot(ctx, "active_now_stuck.png");
                        return self.fail(format!("the Active Now stream preview {shot:?} never scrolled into {screen:?}"));
                    }
                    self.scroll_by(pos2(screen.right() - 150.0, screen.center().y), if shot.top() < screen.top() + 40.0 { 240.0 } else { -240.0 });
                    self.wait = std::time::Instant::now() + Duration::from_millis(500);
                    return;
                }
                self.queue.push_back(vec![egui::Event::PointerMoved(shot.center() + vec2(0.0, -40.0))]);
                self.next(500);
            }
            5102 => {
                let Some(b) = Self::at(ctx, &format!("now_watch:{STREAMER}")) else {
                    if stuck {
                        self.shot(ctx, "active_now_stuck.png");
                        self.fail("hovering the Active Now stream preview showed no Watch Stream button".into());
                    }
                    return;
                };
                self.shot(ctx, "active_now_watch_hover.png");
                self.click(b.center());
                self.next(800);
            }
            5103 => {
                let watching = app.voice.stream.as_ref().is_some_and(|l| l.user == STREAMER && !l.mine);
                if app.voice.chan != Some(STREAM_CHAN) || !watching {
                    if stuck {
                        self.fail(format!("Watch Stream left us in {:?}, watching {}", app.voice.chan, watching));
                    }
                    return;
                }
                self.mark("active_now_watch_stream");
                self.next(1500);
            }
            5104 => {
                self.shot(ctx, "active_now_watching.png");
                app.voice.leave(&app.net);
                app.store.apply("VOICE_STATE_UPDATE", &json!({ "guild_id": self.target.0.to_string(), "user_id": STREAMER.to_string(), "channel_id": null }));
                self.next(600);
                self.step = 5110;
            }
            5110 => {
                if let Some(g) = app.store.guilds.get_mut(&OWNED)
                    && !g.features.iter().any(|f| f == JOINS_FEATURE)
                {
                    g.features.push(JOINS_FEATURE.into());
                }
                app.server = None;
                app.select_guild(Some(OWNED));
                app.open_home(OWNED, community::Page::Members);
                self.next(600);
            }
            5111 => {
                let Some(r) = app.rects.get("joins:tab:1").copied() else { return };
                self.click(r.center());
                app.rects.retain(|k, _| !k.starts_with("joins:more:"));
                self.next(800);
            }
            5112 => {
                if applications::counts(app, OWNED).is_none_or(|c| c.0[0] < 2) {
                    return;
                }
                let Some(r) = first_more(app) else {
                    if stuck {
                        self.fail("a pending join request row had no More Options button".into());
                    }
                    return;
                };
                self.click(r.center());
                self.next(500);
            }
            5113 => {
                let (Some(m), Some(b), Some(c)) = (menu(ctx, "Open in Mod View"), menu(ctx, "Ban User"), menu(ctx, "Copy User ID")) else {
                    if stuck {
                        self.fail("More Options on a join request did not open Open in Mod View, Ban User and Copy User ID".into());
                    }
                    return;
                };
                let _ = (m, c);
                self.mark("joins_more_menu");
                self.shot(ctx, "joins_more_menu.png");
                self.click(b.center());
                self.next(600);
            }
            5114 => {
                if app.kickban.is_none() {
                    if stuck {
                        self.fail("Ban User in a join request's menu opened no dialog".into());
                    }
                    return;
                }
                self.mark("joins_more_ban");
                self.shot(ctx, "joins_more_ban.png");
                app.kickban = None;
                let Some(r) = first_more(app) else { return };
                self.click(r.center());
                self.next(500);
            }
            5115 => {
                let Some(m) = menu(ctx, "Open in Mod View") else { return };
                self.click(m.center());
                self.next(700);
            }
            5116 => {
                if app.mod_view.is_none() {
                    if stuck {
                        self.fail("Open in Mod View in a join request's menu opened nothing".into());
                    }
                    return;
                }
                self.mark("joins_more_mod_view");
                app.mod_view = None;
                self.next(300);
            }
            5117 => {
                app.store.custom_emoji = Some(crate::state::ReactEmoji { id: 5000, name: "ferris".into(), animated: false });
                app.store.custom_status = Some("writing rust".into());
                self.next(1200);
            }
            5118 => {
                if !app.rects.contains_key("side:status_emoji") {
                    if stuck {
                        self.fail("a custom emoji status drew no emoji in the side panel".into());
                    }
                    return;
                }
                self.mark("side_status_custom_emoji");
                self.shot(ctx, "side_status_emoji.png");
                self.next(500);
            }
            5119 => {
                app.store.custom_emoji = None;
                app.store.custom_status = None;
                self.next(300);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5120 },
        }
    }
}
