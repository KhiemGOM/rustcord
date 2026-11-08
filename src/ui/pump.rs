use super::*;

impl App {
    pub(super) fn pump(&mut self, ctx: &egui::Context) {
        let clicked: Vec<crate::toast::Click> = self.pings.as_ref().map(|r| r.try_iter().collect()).unwrap_or_default();
        for click in clicked {
            match click {
                crate::toast::Click::Open(ch, msg) => self.open_ping(ctx, ch, msg),
                crate::toast::Click::Answer(ch) => {
                    self.show_window(ctx);
                    self.answer(ch);
                }
                crate::toast::Click::Decline(ch) => self.decline(ch),
            }
        }
        self.call_fx(ctx);
        if let Some(g) = self.games.as_ref().and_then(|r| r.try_iter().last()) {
            self.detected = g;
            self.refresh_game();
        }
        self.links_pump(ctx);
        // a game opening or closing, rich presence changing, or sharing switched, goes out once we're connected
        let want = self.presence_key();
        if want != self.shared && self.conn == crate::net::Conn::Online && self.store.me != 0 {
            self.send_presence();
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Dispatch(t, d) => {
                    if t == "MESSAGE_CREATE" {
                        crate::rpc::on_message(&d);
                        self.notify(&d, ctx);
                    }
                    if t.starts_with("INTERACTION_") || t == "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE" {
                        self.on_interaction(&t, &d);
                    }
                    if t == "READY" {
                        if let Some(e) = &mut self.e2e {
                            e.mark("ready");
                        }
                        self.shared = (None, 0);
                        if let Some(tk) = &self.token {
                            crate::rpc::signed_in(tk, &d["user"], d["session_id"].as_str().unwrap_or(""));
                        }
                        self.info_asked.clear();
                        self.members_asked.clear();
                        self.feed_at = Some(Instant::now());
                        self.save_at = Some(Instant::now() + Duration::from_secs(5));
                        // which messages are bookmarked, for the menus' Remove Bookmark
                        self.rest("GET", "/users/@me/saved-messages?bookmark_ids=true".into(), None, Tag::Saved(0));
                    }
                    if t.starts_with("STREAM_")
                        && let Some(e) = &mut self.e2e
                    {
                        e.heard.push(json!({ "t": t, "d": d }));
                    }
                    self.voice_cues(&t, &d);
                    if t == "VOICE_CHANNEL_EFFECT_SEND" {
                        self.soundboard_effect(&d);
                    }
                    self.voice.dispatch(&self.net, self.store.me, &t, &d);
                    let took = Instant::now();
                    self.store.apply(&t, &d);
                    self.live(&t, &d);
                    if matches!(t.as_str(), "GUILD_MEMBER_REMOVE" | "GUILD_MEMBER_UPDATE" | "GUILD_BAN_ADD" | "GUILD_BAN_REMOVE") {
                        community::refresh(self);
                    }
                    if t == "READY" || t == "USER_SETTINGS_PROTO_UPDATE" {
                        self.sync_account(ctx);
                    }
                    if t == "READY" {
                        favs::fetch(self);
                        settings::backfill(self);
                    }
                    if t == "USER_SETTINGS_PROTO_UPDATE" {
                        favs::updated(self, &d);
                    }
                    if t == "READY"
                        && let Some(e) = &mut self.e2e
                    {
                        e.cost("ready_apply", took.elapsed());
                    }
                    if t == "READY" {
                        self.remember_me();
                        // cached channel lists can be stale, refetch whatever is open
                        for m in self.store.msgs.values_mut() {
                            m.fresh = false;
                        }
                        match self.cur() {
                            Some(c) => self.open(c),
                            None => self.select_guild(self.ui.guild.filter(|g| self.store.guilds.contains_key(g))),
                        }
                    }
                }
                Ev::Messages { channel, page, list } => {
                    let n = self.store.set_messages(channel, page, &list);
                    self.request_members(channel);
                    if self.jump_top == Some(channel) {
                        self.jump_top = None;
                        self.scroll_fix = Some((channel, 0.0));
                    }
                    if matches!(page, Page::Before(_)) && n > 0 {
                        let est: f32 = self.store.msgs[&channel].list[..n].iter().map(chat::estimate).sum();
                        let off = self.offsets.get(&channel).copied().unwrap_or(0.0);
                        self.scroll_fix = Some((channel, off + est));
                    }
                }
                Ev::MessagesFailed { channel, page, err } => {
                    if let Some(m) = self.store.msgs.get_mut(&channel) {
                        m.loading = false;
                        m.fresh = true;
                        m.failed = Some((page, err));
                    }
                }
                Ev::SendCancelled { channel, nonce } => {
                    self.store.drop_pending(channel, &nonce);
                    self.unsent.remove(&nonce);
                }
                Ev::SendFailed { channel, nonce, err, again } => {
                    // a retry going out stays red without saying so again
                    let first = self.store.msgs.get(&channel).is_some_and(|e| e.list.iter().any(|m| m.pending && m.nonce.as_deref() == Some(nonce.as_str())));
                    self.store.fail_pending(channel, &nonce);
                    let low = err.to_lowercase();
                    let why = if low.contains("cannot send messages to this user") {
                        Some("You cannot message this user")
                    } else if low.contains("slowmode") {
                        Some("You are sending messages too quickly")
                    } else if low.contains("missing permissions") || low.contains("missing access") {
                        Some("You cannot send messages in this channel")
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        self.notice = Some((channel, why.into()));
                    } else if first || !again {
                        self.error_toast(format!("Message failed to send: {err}"));
                    }
                }
                Ev::Conn(c) => self.conn = c,
                Ev::AuthFailed => {
                    self.logout();
                    self.login.expired();
                }
                Ev::Auth(auth::Ev::Token(t)) => self.sign_in(t),
                Ev::Auth(e) => self.login.event(e, &self.net),
                Ev::Error(e) => self.error_toast(e),
                Ev::Toast(e) => self.toast(e),
                Ev::Fail { req, fields, raw } => self.fail = Some((fields, format!("{req}\n{raw}"))),
                Ev::InteractFailed { nonce, status, message } => self.interact_failed(&nonce, status, message),
                Ev::Rest { tag, res } => {
                    self.on_rest(tag, res);
                    self.fail = None;
                }
                Ev::Voice(e) => {
                    if let Some(err) = self.voice.event(e) {
                        if let Some(e) = &mut self.e2e {
                            e.heard.push(json!({ "t": "voice error", "d": err }));
                        }
                        if err == crate::voice::CAMERA_FAILED {
                            self.voice.camera = false;
                            self.toast(err);
                        } else {
                            self.toast(format!("Voice disconnected: {err}"));
                        }
                    }
                    if let Some((key, img)) = self.voice.preview_job.take() {
                        self.stream_preview(key, img);
                    }
                    if std::mem::take(&mut self.voice.entrance_due) {
                        self.entrance_sound();
                    }
                    if self.voice.shared.no_audio.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        self.toast("No audio detected from your mic. Check the input device in Voice & Video settings.");
                    }
                    if self.voice.video_call && self.voice.state == crate::voice::State::Connected {
                        self.voice.video_call = false;
                        self.voice.set_camera(&self.net, self.store.me, true);
                    }
                }
            }
        }
    }

    pub(super) fn ack_current(&mut self, ctx: &egui::Context) {
        if self.e2e.as_ref().is_some_and(|e| e.tour) {
            return;
        }
        if !ctx.input(|i| i.focused) {
            return;
        }
        let Some(ch) = self.cur() else { return };
        let side = self.side_thread.filter(|t| self.store.channels.get(t).is_some_and(|t| t.parent == ch));
        for ch in std::iter::once(ch).chain(side) {
            if Some(ch) == self.cur() {
                if self.hold_ack == Some(ch) {
                    continue;
                }
                self.hold_ack = None;
            }
            let Some((last, flags)) = self.store.channels.get(&ch).map(|c| (c.last_msg, if c.is_thread() { 2 } else { (c.guild != 0) as u8 })) else { continue };
            let (acked, mentions) = self.store.reads.get(&ch).map_or((0, 0), |r| (r.last_acked, r.mentions));
            if last <= acked && mentions == 0 {
                continue;
            }
            let r = self.store.reads.entry(ch).or_default();
            r.last_acked = last.max(r.last_acked);
            r.mentions = 0;
            if last != 0 {
                let flags = (r.flags != Some(flags)).then_some(flags);
                r.flags = r.flags.or(flags);
                let last_viewed = ((chrono::Utc::now().timestamp_millis() - 1_420_070_400_000) as f64 / 86_400_000.0).ceil() as u64;
                self.net.send(Cmd::Ack { channel: ch, id: last, last_viewed, flags });
            }
        }
    }

    pub(super) fn overlays(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        create::show(self, &ctx);
        lurk::tick(self);
        folders::show(self, &ctx);
        newserver::show(self, &ctx);
        guildmodal::show(self, &ctx);
        active::show(self, &ctx);
        server::show(self, &ctx);
        channel::show(self, &ctx);
        events::show(self, &ctx);
        applications::modals(self, &ctx);
        stage::modals(self, &ctx);
        settings::verify(self, &ctx);
        share::show(self, &ctx);
        let mut show_detail = false;
        clips_gallery::show(self, &ctx);
        if let Some((text, at)) = &self.toast {
            let e = at.elapsed().as_secs_f32();
            if e > 4.0 {
                self.toast = None;
            } else {
                let text = text.clone();
                let k = motion::EASE_OUT.at(e / 0.3) - motion::EASE_IN.at((e - 3.7) / 0.3);
                let slide = if crate::theme::anim(1.0) > 0.0 { 24.0 * (1.0 - k) } else { 0.0 };
                let area = egui::Area::new("toast".into()).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -90.0)).order(egui::Order::Tooltip).fade_in(false);
                ctx.set_transform_layer(area.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, slide)));
                area.show(&ctx, |ui| {
                    ui.multiply_opacity(k);
                    let r = Frame::new()
                        .fill(pal().bg_floating)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(text).color(pal().text_normal).font(reg(14.0))).wrap_mode(egui::TextWrapMode::Extend)));
                    self.rects.insert("toast".into(), r.response.rect);
                    let r = ui.interact(r.response.rect, egui::Id::new("toast_click"), Sense::click());
                    if r.clicked() && ui.input(|i| i.modifiers.shift) {
                        show_detail = true;
                    }
                });
                if e < 0.3 || e >= 3.7 {
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(Duration::from_secs_f32(3.7 - e));
                }
            }
        }
        if show_detail && let Some(e) = self.toast_detail.take() {
            self.toast = None;
            self.confirm = Some(Confirm::new("Error Details", e, "Okay", |_: &mut App| {}).blurple());
        }
        self.confirm_modal(&ctx);
        settings::camera::frame(self, &ctx);
        notifs::show(self, &ctx);
        addapp::show(self, &ctx);
        crop::show(self, &ctx);
        cstatus::show(self, &ctx);
        picker::show(self, &ctx);
        picker::stickers(self, &ctx);
        chat::context_menu(self, &ctx);
        usermenu::user_menu(self, &ctx);
        userdlg::show(self, &ctx);
        popups::show(self, &ctx);
        buy::show(self, &ctx);
        attach::dragging(self, &ctx);
        links::show(self, &ctx);
        botmodal::show(self, &ctx);
        self.interactions_tick();
        popups::viewer(self, &ctx);
        player::overlay(self, &ctx);
        crate::media::tick(ctx.cumulative_frame_nr());
        call::incoming(self, &ctx);
        activity::overlays(self, &ctx);
        embedplay::place(self, &ctx);
        keys::combos(self, &ctx);
        keys::breathe(self, &ctx);
        keys::demon(self, &ctx);
        keys::discodo(self, &ctx);
    }

    /// Discord's confirm dialog: the title with a ✕, the question, the message it's about and a
    /// protip, then Cancel and the action. With a message they split the width; without, they
    /// sit at the right, "Don't ask again" across from them. Enter goes ahead, as in discord.
    pub(super) fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(c) = &mut self.confirm else { return };
        let (mut close, mut go) = (false, false);
        let pinned = |ch: Id, id: Id| self.pins.as_ref().filter(|(c, _)| *c == ch).and_then(|(_, l)| l.as_ref()?.iter().find(|m| m.id == id).cloned());
        let msg = c.msg.and_then(|(ch, id)| self.store.find_msg(ch, id).cloned().or_else(|| pinned(ch, id)));
        let store = &self.store;
        let modal = crate::ui::motion::Modal::new("confirm".into())
            .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
            .show(ctx, |ui| {
                ui.set_width(432.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&c.title).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= icon_button(ui, icons::Icon::Close, 24.0, "Close").clicked();
                    });
                });
                ui.add_space(8.0);
                ui.label(ui.painter().layout_job(bolded(&c.body, 16.0, pal().text_normal, 432.0)));
                if let Some(m) = &msg {
                    ui.add_space(16.0);
                    chat::preview(ui, store, m);
                }
                if let Some([a, b, rest]) = c.tip {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("PROTIP:").font(bold(12.0)).color(pal().green));
                    ui.add_space(2.0);
                    ui.label(ui.painter().layout_job(bolded(&format!("{a}**{b}**{rest}"), 14.0, pal().text_normal, 432.0)));
                }
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if msg.is_some() {
                        close |= grey_button(ui, "Cancel", 212.0).clicked();
                        go = button(ui, c.button, c.fill, 212.0).clicked();
                        return;
                    }
                    if let Some((on, _)) = &mut c.check {
                        let (r, resp) = ui.allocate_exact_size(vec2(160.0, 38.0), Sense::click());
                        let b = Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(24.0, 24.0));
                        if *on {
                            ui.painter().rect_filled(b, 6, pal().blurple);
                            icons::draw(ui.painter(), icons::Icon::Check, b.shrink(4.0), Color32::WHITE);
                        } else {
                            ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                        }
                        ui.painter().text(pos2(b.right() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, "Don't ask again", reg(16.0), pal().text_normal);
                        if resp.clicked() {
                            *on = !*on;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let w = ui.painter().layout_no_wrap(c.button.to_string(), medium(16.0), Color32::WHITE).size().x + 32.0;
                        go = button(ui, c.button, c.fill, w.max(60.0)).clicked();
                        close |= grey_button(ui, "Cancel", 92.0).clicked();
                    });
                });
            });
        go |= c.shown && enter_submits(ctx, true);
        c.shown = true;
        if go && let Some(c) = self.confirm.take() {
            if let Some((true, then)) = c.check {
                then(self);
            }
            (c.run)(self);
        } else if close || modal.should_close() {
            self.confirm = None;
        }
    }
}
