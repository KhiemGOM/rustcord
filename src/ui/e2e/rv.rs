//! Steps 5400 to 5449: a tester's reports, each driven through the real control.
//! `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5400` runs just this group.

use std::time::{Duration, Instant};

use egui::{Event, pos2};

use super::Run;
use crate::ui::App;

const ALICE: u64 = 101;
const ALICE_DM: u64 = 3000;

impl Run {
    fn busy_voice(app: &App, gid: u64) -> Option<u64> {
        let g = app.store.guilds.get(&gid)?;
        let mut v: Vec<&crate::state::Channel> = g.channels.iter().filter_map(|c| app.store.channels.get(c)).filter(|c| c.is_voice() && app.store.can_view(c)).collect();
        v.sort_by_key(|c| (c.position, c.id));
        v.first().map(|c| c.id)
    }

    pub(super) fn rv_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let (gid, general) = self.target;
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5400 => {
                app.profile = None;
                app.inbox = None;
                app.ui.hide_members = false;
                app.select_guild(Some(gid));
                app.open(general);
                self.sub = 0;
                self.next(1000);
            }
            5401 => {
                let Some(row) = app.rects.get(&format!("member:{ALICE}")).copied() else {
                    if stuck {
                        self.next(0);
                    }
                    return;
                };
                match self.sub {
                    0 => {
                        self.click(row.center());
                        self.sub = 1;
                    }
                    1 => {
                        if app.profile.is_some_and(|p| p.0 == ALICE) {
                            self.mark("rv_popout_open");
                        }
                        self.click(row.center());
                        self.sub = 2;
                    }
                    _ => {
                        if app.profile.is_none() {
                            self.mark("rv_popout_closed_by_its_name");
                        }
                        app.profile = None;
                        self.sub = 0;
                        self.next(300);
                        return;
                    }
                }
                self.wait = Instant::now() + Duration::from_millis(800);
            }
            5402 => {
                app.open_profile(app.store.me, 0, pos2(300.0, 160.0));
                self.next(1200);
            }
            5403 => {
                app.profile = None;
                app.drafts.insert(general, "words that run on and on ".repeat(40));
                self.next(600);
            }
            5404 => {
                if let (Some(text), Some(strip)) = (app.rects.get("composer_text_box").copied(), app.rects.get("composer:buttons").copied()) {
                    self.rects.insert("rv_composer".into(), serde_json::json!([text.right(), strip.left()]));
                    if text.width() > 100.0 && text.right() <= strip.left() + 0.5 {
                        self.mark("rv_composer_clear_of_buttons");
                    }
                }
                app.drafts.remove(&general);
                app.select_guild(None);
                app.store.pinned.insert(ALICE_DM);
                self.queue.push_back(vec![Event::PointerMoved(pos2(900.0, 400.0))]);
                self.next(900);
            }
            5405 => {
                if Self::at(ctx, &format!("dm_mark:{ALICE_DM}")).is_some() {
                    self.mark("rv_dm_pin");
                } else if !stuck {
                    return;
                }
                app.store.pinned.remove(&ALICE_DM);
                app.open_quests();
                self.next(2500);
            }
            5406 => {
                app.quests = None;
                app.select_guild(Some(gid));
                app.open(general);
                let Some(vc) = Self::busy_voice(app, gid) else {
                    self.step = 5414;
                    return;
                };
                app.store.opt_in.insert(gid);
                if let Some(g) = app.store.guilds.get_mut(&gid) {
                    g.voice.insert(ALICE, vc);
                    g.streaming.insert(ALICE);
                    if let Some(r) = g.roles.get_mut(&gid) {
                        self.rects.insert("rv_perms".into(), serde_json::json!(r.perms));
                        r.perms |= crate::state::CONNECT;
                    }
                }
                app.ui.voice_open.remove(&gid);
                self.rects.insert("rv_vc".into(), serde_json::json!(vc));
                self.next(1200);
            }
            5407 => {
                let vc = self.rects.get("rv_vc").and_then(|v| v.as_u64()).unwrap_or(0);
                let Some(t) = Self::at(ctx, "side:voice_toggle") else {
                    if stuck {
                        self.step = 5413;
                    }
                    return;
                };
                self.mark("rv_voice_toggle");
                if app.rects.contains_key(&format!("chan:{vc}")) {
                    self.mark("rv_active_now_lists_busy");
                }
                self.click(t.center());
                self.next(900);
            }
            5408 => {
                if app.ui.voice_open.contains(&gid) {
                    self.mark("rv_voice_shown_all");
                }
                let Some(row) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("voice_row", ALICE)))) else {
                    if stuck {
                        self.step = 5413;
                    }
                    return;
                };
                self.queue.push_back(vec![Event::PointerMoved(row.center())]);
                self.next(1800);
            }
            5409 => {
                if Self::at(ctx, "stream_tip").is_some() {
                    self.mark("rv_stream_tip");
                    self.shot(ctx, "rv_stream_tip.png");
                } else if !stuck {
                    return;
                }
                self.step = 5413;
            }
            5413 => {
                app.store.opt_in.remove(&gid);
                let perms = self.rects.get("rv_perms").and_then(|v| v.as_u64());
                if let Some(g) = app.store.guilds.get_mut(&gid) {
                    g.voice.remove(&ALICE);
                    g.streaming.remove(&ALICE);
                    if let (Some(r), Some(p)) = (g.roles.get_mut(&gid), perms) {
                        r.perms = p;
                    }
                }
                app.ui.voice_open.remove(&gid);
                self.queue.push_back(vec![Event::PointerMoved(pos2(900.0, 400.0))]);
                self.next(500);
            }
            5414 => {
                let path = self.dir.join("studio_pick.png");
                let _ = image::RgbaImage::from_pixel(64, 64, image::Rgba([88, 101, 242, 255])).save(&path);
                app.select_guild(None);
                crate::ui::emojistudio::start(app, path, 0);
                self.next(900);
            }
            5415 => {
                if app.emoji_studio.is_none() || Self::at(ctx, "studio:finish").is_none() {
                    if stuck {
                        self.step = 5419;
                    }
                    return;
                }
                self.mark("rv_studio_open_in_dm");
                let Some(r) = Self::at(ctx, "studio:server") else { return };
                self.click(r.center());
                self.next(600);
            }
            5416 => {
                let mine = app.store.guild_order.iter().copied().find(|g| app.store.my_perms(*g) & (crate::state::CREATE_GUILD_EXPRESSIONS | crate::state::MANAGE_GUILD_EXPRESSIONS) != 0);
                let Some(r) = mine.and_then(|g| Self::at(ctx, &format!("studio:server:{g}"))) else {
                    if stuck {
                        self.shot(ctx, "rv_studio_no_menu.png");
                        self.step = 5419;
                    }
                    return;
                };
                self.click(r.center());
                self.next(600);
            }
            5417 => {
                let Some(r) = Self::at(ctx, "studio:finish") else {
                    if stuck {
                        self.step = 5419;
                    }
                    return;
                };
                self.click(r.center());
                self.next(1500);
            }
            5418 => {
                if app.emoji_studio.is_none() {
                    self.mark("rv_studio_uploaded");
                } else if !stuck {
                    return;
                }
                self.step = 5419;
            }
            _ => {
                app.emoji_studio = None;
                app.profile = None;
                app.quests = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5450 };
            }
        }
    }
}
