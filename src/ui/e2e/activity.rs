use super::*;

const DEN: Id = 1008;
const HANGOUT: Id = 2801;
const QUIZ: Id = 9100;
const DRAW: Id = 9101;
const ALICE: Id = 101;

impl Run {
    pub(super) fn activity_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let join_as = |app: &mut App, who: Id, id: Id| {
            app.rest("POST", "/mock/activity/join".into(), Some(json!({ "channel": HANGOUT, "app": id, "user": who })), crate::net::Tag::Quiet);
        };
        let connected = |app: &App| app.voice.state == State::Connected && app.voice.chan == Some(HANGOUT);
        match self.step {
            2200 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&HANGOUT) {
                    return;
                }
                app.rest("POST", "/mock/activity/forget".into(), Some(json!({})), crate::net::Tag::Quiet);
                app.select_guild(Some(DEN));
                app.open(HANGOUT);
                app.join_voice(HANGOUT, false);
                self.next(0);
            }
            2201 => {
                if !connected(app) {
                    return;
                }
                let Some(r) = app.rects.get("voice:activity").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            2202 => {
                let Some(r) = app.rects.get(&format!("activity:card:{QUIZ}")).copied() else { return };
                self.mark("activity_shelf");
                self.shot(ctx, "activity_shelf.png");
                self.click(r.center());
                self.next(0);
            }
            2203 => {
                if app.activity.run.is_none() {
                    return;
                }
                self.mark("activity_launched");
                self.next(0);
            }
            2204 => {
                if app.activity.ready() {
                    self.mark_once("activity_ready");
                }
                if app.activity.consent.is_none() {
                    return;
                }
                self.mark("activity_consent");
                self.next(600);
            }
            2205 => {
                let Some(r) = app.rects.get("activity:authorize").copied() else { return };
                self.shot(ctx, "activity_consent.png");
                self.click(r.center());
                self.next(300);
            }
            2206 => {
                if !app.activity.authed() {
                    return;
                }
                self.mark("activity_authed");
                self.next(0);
            }
            2207 => {
                if app.leaving.as_ref().is_none_or(|(u, _)| !u.contains("example.com")) {
                    return;
                }
                self.mark("activity_link_prompt");
                self.key(Key::Escape);
                self.next(400);
            }
            2208 => {
                if app.leaving.is_some() {
                    return;
                }
                self.mark("activity_link_dismissed");
                join_as(app, ALICE, QUIZ);
                self.next(0);
            }
            2209 => {
                if app.store.embedded.get(&HANGOUT).and_then(|l| l.iter().find(|i| i.app == QUIZ)).is_none_or(|i| i.users.len() < 2) {
                    return;
                }
                self.mark("activity_participants");
                self.shot(ctx, "activity_running.png");
                self.next(800);
            }
            2210 => {
                let Some(r) = app.rects.get("activity:leave").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            2211 => {
                if app.activity.run.is_some() {
                    return;
                }
                self.mark("activity_left");
                join_as(app, ALICE, DRAW);
                self.next(0);
            }
            2212 => {
                if !app.rects.contains_key(&format!("activity:join_tile:{DRAW}")) {
                    return;
                }
                self.mark("activity_join_tile");
                self.next(600);
            }
            2213 => {
                let Some(r) = app.rects.get(&format!("activity:join_tile:{DRAW}")).copied() else { return };
                self.shot(ctx, "activity_join_tile.png");
                self.click(r.center());
                self.next(0);
            }
            2214 => {
                if app.activity.consent.is_none() {
                    return;
                }
                self.mark("activity_joined");
                self.key(Key::Escape);
                self.next(600);
            }
            2215 => {
                if app.activity.consent.is_some() {
                    return;
                }
                self.mark("activity_denied");
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(600);
            }
            2216 => {
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            2217 => {
                if app.activity.run.is_some() || app.voice.chan.is_some() {
                    return;
                }
                self.mark("activity_left_with_call");
                ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("activity_join_row_rect", QUIZ))));
                join_as(app, ALICE, QUIZ);
                self.next(600);
            }
            2218 => {
                if ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("activity_join_row_rect", QUIZ)))).is_none() {
                    return;
                }
                self.mark("activity_list_row");
                self.next(600);
            }
            2219 => {
                let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("activity_join_row_rect", QUIZ)))) else { return };
                self.click(r.center());
                self.next(0);
            }
            2220 => {
                if !connected(app) || !app.activity.authed() {
                    return;
                }
                self.mark("activity_list_join");
                self.next(0);
            }
            2221 => {
                if app.leaving.is_none() {
                    return;
                }
                self.key(Key::Escape);
                self.next(400);
            }
            2222 => {
                if app.leaving.is_some() {
                    return;
                }
                app.call.pop = true;
                self.next(1500);
            }
            2223 => {
                if !app.activity.in_popout {
                    return;
                }
                self.mark("activity_popout");
                app.call.pop = false;
                self.next(800);
            }
            2224 => {
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(600);
            }
            2225 => {
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            _ => {
                if app.voice.chan.is_none() && app.activity.run.is_none() {
                    let heard = crate::sound::heard();
                    if ["activity_launch", "activity_user_join", "activity_end"].iter().all(|c| heard.iter().any(|n| n == c)) {
                        self.mark("activity_cues");
                    }
                    self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 2099 };
                    self.next(300);
                }
            }
        }
    }
}
