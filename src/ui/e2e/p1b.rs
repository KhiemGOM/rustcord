//! The rest of the P1 round: the forum's Post Guidelines card, the DM list's Wave button, Listen
//! Along on a song in a profile popout and the built-in commands as pills. Steps 4360 and up.

use super::*;

const FORUM: Id = 2030;
const FRIEND: Id = 102;
const SINGER: Id = 101;

impl Run {
    fn wave_dm(app: &App) -> Option<Id> {
        app.store.dms.iter().copied().find(|c| app.store.channels.get(c).is_some_and(|c| c.kind == 1 && c.recipients == [FRIEND]))
    }

    pub(super) fn p1b_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        match self.step {
            4360 => {
                app.select_guild(Some(self.target.0));
                app.open(FORUM);
                self.next(1200);
            }
            4361 => {
                let Some(card) = app.rects.get("forum:guidelines").copied() else { return };
                let Some(x) = app.rects.get("forum:guidelines_close").copied() else { return };
                self.mark("forum_guidelines_card");
                self.rects.insert("forum_guidelines".into(), json!([card.min.x, card.min.y, card.width(), card.height()]));
                self.shot(ctx, "forum_guidelines.png");
                self.click(x.center());
                self.next(500);
            }
            4362 => {
                if app.rects.contains_key("forum:guidelines") {
                    return;
                }
                self.mark("forum_guidelines_closed");
                self.shot(ctx, "forum_guidelines_closed.png");
                self.next(300);
                self.step = 4370;
            }
            4370 => {
                app.select_guild(None);
                app.friends = true;
                self.next(900);
            }
            4371 => {
                let Some(dm) = Self::wave_dm(app) else { return };
                let Some(b) = Self::at(ctx, &format!("dm_wave:{dm}")) else { return };
                if Self::at(ctx, "dm_wave:3000").is_none() {
                    self.mark("wave_only_new_dm");
                }
                self.shot(ctx, "wave_button.png");
                self.queue.push_back(vec![Event::PointerMoved(b.center() + vec2(1.0, 0.0))]);
                self.queue.push_back(vec![Event::PointerMoved(b.center())]);
                self.next(1100);
            }
            4372 => {
                let Some(dm) = Self::wave_dm(app) else { return };
                let Some(b) = Self::at(ctx, &format!("dm_wave:{dm}")) else { return };
                if Self::tooltip_near(ctx, b) {
                    self.mark("wave_tooltip");
                }
                self.shot(ctx, "wave_tooltip.png");
                self.click(b.center());
                self.next(900);
            }
            4373 => {
                let Some(dm) = Self::wave_dm(app) else { return };
                if app.cur() != Some(dm) || app.store.channels.get(&dm).is_none_or(|c| c.last_msg == 0) {
                    return;
                }
                self.mark("wave_sent");
                self.shot(ctx, "wave_sent.png");
                self.next(400);
                self.step = 4375;
            }
            4375 => {
                let now = chrono::Utc::now().timestamp_millis();
                app.store.apply("PRESENCE_UPDATE", &json!({ "user": { "id": SINGER.to_string() }, "status": "online", "activities": [
                    { "type": 2, "name": "Spotify", "details": "Farewell", "state": "marQ", "sync_id": "abc", "timestamps": { "start": now - 99000, "end": now + 14000 }, "assets": { "large_image": "spotify:xyz" } },
                ] }));
                app.friends = false;
                app.select_guild(Some(self.target.0));
                app.open(general);
                app.open_profile(SINGER, self.target.0, pos2(300.0, 160.0));
                self.next(1200);
            }
            4376 => {
                if app.profiles.get(&SINGER).is_none_or(|p| p.is_null()) {
                    return;
                }
                let Some(b) = Self::at(ctx, "spotify:listen") else { return };
                self.shot(ctx, "listen_along_popout.png");
                self.loaded = self.opened.len();
                self.queue.push_back(vec![Event::PointerMoved(b.center() + vec2(1.0, 0.0))]);
                self.queue.push_back(vec![Event::PointerMoved(b.center())]);
                self.next(1100);
            }
            4377 => {
                let Some(b) = Self::at(ctx, "spotify:listen") else { return };
                if Self::said(ctx, "listen_tip").is_some_and(|t| t.starts_with("Listen along with ")) && Self::tooltip_near(ctx, b) {
                    self.mark("listen_along_tip");
                }
                self.shot(ctx, "listen_along_tip.png");
                self.click(b.center());
                self.next(600);
            }
            4378 => {
                if self.opened.len() > self.loaded {
                    self.mark("listen_along_click");
                }
                self.key(Key::Escape);
                self.next(400);
                self.step = 4380;
            }
            4380 => {
                app.open(general);
                self.next(700);
            }
            4381 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("/shrug".into())]);
                self.next(600);
            }
            4382 => {
                if !app.ac_open {
                    return;
                }
                self.key(Key::Enter);
                self.next(600);
            }
            4383 => {
                if !app.slash.get(&general).is_some_and(|s| s.cmd["builtin"] == true) {
                    return;
                }
                self.mark("builtin_pill");
                self.shot(ctx, "builtin_pill.png");
                self.queue.push_back(vec![Event::Text("pill hello".into())]);
                self.key(Key::Enter);
                self.next(1000);
            }
            4384 => {
                if !app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.content.starts_with("pill hello") && m.content.contains("¯\\_(ツ)_/¯"))) {
                    return;
                }
                self.mark("builtin_pill_sent");
                self.next(300);
            }
            4385 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("/gif".into())]);
                self.next(600);
            }
            4386 => {
                if !app.ac_open {
                    return;
                }
                self.key(Key::Enter);
                self.next(500);
            }
            4387 => {
                if !app.slash.get(&general).is_some_and(|s| s.cmd["name"] == "gif") {
                    return;
                }
                self.queue.push_back(vec![Event::Text("cat".into())]);
                self.key(Key::Enter);
                self.next(900);
            }
            4388 => {
                if app.gifs.is_none() {
                    return;
                }
                self.mark("builtin_gif_pill");
                self.shot(ctx, "builtin_gif_pill.png");
                self.key(Key::Escape);
                self.next(400);
            }
            _ => self.step = 4400,
        }
    }
}
