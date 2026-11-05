use super::*;

const DEN: Id = 1008;
const HANGOUT: Id = 2801;
const PANEL: Id = 2802;
const TOWN_HALL: Id = 2311;
const LOUNGE: Id = 2310;
const ALICE: Id = 101;
const HANDS_UP: Id = 102;

impl Run {
    pub(super) fn stage_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let me = app.store.me;
        let hover = |run: &mut Self, at: Pos2| run.queue.push_back(vec![Event::PointerMoved(at)]);
        let row = |label: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))));
        let connected = |app: &App, ch: Id| app.voice.state == State::Connected && app.voice.chan == Some(ch);
        let idle = |app: &App| app.voice.state == State::Idle && app.voice.chan.is_none();
        match self.step {
            700 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&HANGOUT) {
                    return;
                }
                app.select_guild(Some(DEN));
                app.open(HANGOUT);
                app.join_voice(HANGOUT, false);
                self.next(0);
            }
            701 => {
                if connected(app, HANGOUT) {
                    self.next(1500);
                }
            }
            702 => {
                let Some(r) = app.rects.get(&format!("voice:tile:{ALICE}")).copied() else { return };
                hover(self, r.center());
                self.next(300);
            }
            703 => {
                let Some(r) = app.rects.get(&format!("voice:tile_opts:{ALICE}")).copied() else { return };
                self.shot(ctx, "call_tile.png");
                self.click(r.center());
                self.next(400);
            }
            704 => {
                let Some(r) = row("Mute") else { return };
                self.shot(ctx, "call_tile_menu.png");
                self.click(r.center());
                self.next(300);
            }
            705 => {
                let silenced = app.voice.shared.peers.lock().is_ok_and(|p| p.get(&ALICE) == Some(&0.0));
                if !silenced || !app.settings.peers.get(&ALICE).is_some_and(|p| p.muted) {
                    return;
                }
                self.mark("tile_mute");
                app.set_peer(ALICE, Default::default(), true);
                let Some(r) = row("Focus This Person") else { return };
                self.click(r.center());
                self.next(300);
            }
            706 => {
                if app.call.focus != Some(ALICE) {
                    return;
                }
                self.mark("tile_focus");
                self.shot(ctx, "call_focus.png");
                app.call.focus = None;
                self.key(Key::Escape);
                self.next(300);
            }
            707 => {
                let Some(r) = app.rects.get("voice:mic_more").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            708 => {
                if row("Input Device").is_none() || row("Input Profile").is_none() || row("Voice Settings").is_none() {
                    return;
                }
                self.mark("call_mic_menu");
                self.shot(ctx, "call_mic_menu.png");
                self.key(Key::Escape);
                self.next(300);
            }
            709 => {
                let Some(r) = app.rects.get("voice:krisp").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            710 => {
                let Some(r) = app.rects.get("krisp:test").copied() else { return };
                self.mark("krisp_popout");
                self.shot(ctx, "krisp.png");
                self.click(r.center());
                self.next(300);
            }
            711 => {
                if !app.call.mic_test {
                    return;
                }
                self.mark("krisp_test");
                self.key(Key::Escape);
                self.next(300);
            }
            712 => {
                let Some(r) = app.rects.get("voice:invite").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            713 => {
                if app.made_invite.is_none() {
                    return;
                }
                self.mark("call_invite");
                self.key(Key::Escape);
                self.next(400);
            }
            714 => {
                let Some(r) = app.rects.get("voice:fullscreen").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            715 => {
                if !app.call.full {
                    return;
                }
                self.mark("call_full");
                self.shot(ctx, "call_full.png");
                self.key(Key::Escape);
                self.next(300);
            }
            716 => {
                if app.call.full {
                    return;
                }
                self.mark("call_unfull");
                app.call.pop = true;
                self.next(900);
            }
            717 => {
                if ctx.input(|i| i.raw.viewports.len()) < 2 {
                    return;
                }
                self.mark("call_pop");
                app.call.pop = false;
                self.next(400);
            }
            718 => {
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            719 => {
                if idle(app) {
                    app.open(PANEL);
                    self.next(800);
                }
            }
            720 => {
                if let Some(r) = app.rects.get("stage:welcome_ok").copied() {
                    self.shot(ctx, "stage_welcome.png");
                    self.click(r.center());
                    self.next(400);
                } else if app.settings.stage_welcome {
                    self.next(0);
                }
            }
            721 => {
                let Some(r) = app.rects.get("stage:join").copied() else { return };
                if !app.settings.stage_welcome {
                    return;
                }
                self.mark("stage_welcome");
                self.shot(ctx, "stage_landing.png");
                self.click(r.center());
                self.next(500);
            }
            722 => {
                if !app.rects.contains_key("stage:start_ok") {
                    return;
                }
                self.shot(ctx, "stage_start.png");
                self.queue.push_back(vec![Event::Text("Panel talk".into())]);
                self.next(300);
            }
            723 => {
                let Some(r) = app.rects.get("stage:start_ok").copied() else { return };
                self.click(r.center());
                self.next(1000);
            }
            724 => {
                let speaker = app.voice.stage.seats.get(&me).is_some_and(|s| !s.audience);
                if connected(app, PANEL) && speaker && app.voice.stage.live.contains_key(&PANEL) {
                    self.mark("stage_started");
                    self.next(500);
                }
            }
            725 => {
                let Some(r) = app.rects.get(&format!("stage:invite_speak:{HANDS_UP}")).copied() else { return };
                self.shot(ctx, "stage_view.png");
                self.click(r.center());
                self.next(600);
            }
            726 => {
                if app.voice.stage.seats.get(&HANDS_UP).is_some_and(|s| !s.audience) {
                    self.mark("stage_invite");
                    self.next(300);
                }
            }
            727 => {
                let Some(r) = app.rects.get("stage:ctl:End Stage").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            728 => {
                if app.confirm.is_none() {
                    return;
                }
                self.shot(ctx, "stage_end.png");
                self.key(Key::Enter);
                self.next(600);
            }
            729 => {
                if !app.voice.stage.live.is_empty() {
                    return;
                }
                self.mark("stage_ended");
                let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            730 => {
                if idle(app) {
                    app.select_guild(Some(1003));
                    app.open(TOWN_HALL);
                    self.next(1000);
                }
            }
            731 => {
                let Some(r) = app.rects.get("stage:join").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            732 => {
                if connected(app, TOWN_HALL) {
                    self.next(800);
                }
            }
            733 => {
                let Some(r) = app.rects.get("stage:ctl:Request to Speak").copied() else { return };
                self.shot(ctx, "stage_audience.png");
                self.click(r.center());
                self.next(600);
            }
            734 => {
                if !app.voice.stage.seats.get(&me).is_some_and(|s| s.hand.is_some()) {
                    return;
                }
                self.mark("stage_hand");
                let Some(r) = app.rects.get("stage:ctl:Lower Hand").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            735 => {
                if app.voice.stage.seats.get(&me).is_some_and(|s| s.hand.is_none()) {
                    self.mark("stage_hand_down");
                    app.join_voice(LOUNGE, false);
                    self.next(400);
                }
            }
            736 => {
                if !app.confirm.as_ref().is_some_and(|c| c.title == "You sure?") {
                    return;
                }
                self.mark("voice_switch_warn");
                self.shot(ctx, "voice_switch.png");
                self.key(Key::Escape);
                self.next(400);
            }
            737 => {
                if app.confirm.is_none() && app.voice.chan == Some(TOWN_HALL) {
                    self.mark("voice_switch_kept");
                    let Some(r) = app.rects.get("voice:leave_big").copied() else { return };
                    self.click(r.center());
                    self.next(0);
                }
            }
            _ => {
                if idle(app) {
                    self.step = 2099;
                    self.next(300);
                }
            }
        }
    }
}
