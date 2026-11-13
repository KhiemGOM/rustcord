use super::*;

pub(super) const SIDE_W: f32 = 304.0;

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        e2e::beat();
        let focused = ctx.input(|i| i.focused);
        if self.ui_priority != Some(focused) {
            self.ui_priority = Some(focused);
            crate::winsys::set_ui_priority(focused);
        }
        self.pump(ctx);
        self.status_expiry(ctx);
        activity::logic(self, ctx);
        self.os_settings(ctx, frame);
        self.trim_memory(ctx);
        if self.away(ctx) {
            self.update_title(ctx);
            if let Some(mut e) = self.e2e.take() {
                e.hidden(self, ctx);
                self.e2e = Some(e);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        if self.away(&ctx) {
            return;
        }
        let want_cjk = crate::theme::CJK_WANTED.load(std::sync::atomic::Ordering::Relaxed);
        if (want_cjk && !self.cjk_loaded) || crate::fonts::take_fresh() {
            self.cjk_loaded = want_cjk;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::theme::fonts(want_cjk));
            });
            self.fonts_rx = Some(rx);
        }
        if let Some(f) = self.fonts_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            let took = Instant::now();
            ctx.set_fonts(f);
            self.fonts_rx = None;
            if let Some(e) = &mut self.e2e {
                e.cost("cjk_fonts", took.elapsed());
            }
        }
        crate::theme::set_me(self.store.me);
        crate::theme::backdrop(&ctx);
        // rebuilt every frame by whatever gets drawn, so nothing points at last frame's layout
        self.rects.clear();
        if self.frames == 0
            && let Some(e) = &mut self.e2e
        {
            e.mark("first_frame");
        }
        if let Some(e) = &mut self.e2e {
            e.screenshots(&ctx);
        }

        if self.frames == 0 {
            crate::winsys::frameless(frame);
        }
        let (zin, zout, zreset) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                take_key(i, Modifiers::COMMAND, Key::Equals) || take_key(i, Modifiers::COMMAND, Key::Plus),
                take_key(i, Modifiers::COMMAND, Key::Minus),
                take_key(i, Modifiers::COMMAND, Key::Num0),
            )
        });
        if zin || zout {
            self.step_zoom(&ctx, zin);
        }
        if zreset {
            ctx.set_zoom_factor(1.0);
            self.toast("Zoom 100%");
        }
        // F11, as in a browser
        if ctx.input_mut(|i| take_key(i, egui::Modifiers::NONE, egui::Key::F11)) {
            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            titlebar::fullscreen(&ctx, !full);
        }
        // what was open as the frame began owns Escape and most keys this frame
        let layered = self.layered(&ctx);
        titlebar::show(self, ui);
        if self.token.is_none() {
            login::show(self, ui);
        } else if splash::waiting(self) {
            splash::show(ui);
        } else {
            self.streamer_banner(ui);
            settings::test_mode_banner(self, ui);
            viewas::bar(self, ui);
            // a modal over the app; shown first so it takes ESC before the chat does
            let settings_open = self.settings_page.is_some();
            if settings_open {
                settings::show(self, ui);
            } else if self.profed.is_some() {
                profed::show(self, &ctx);
            }
            let w = ctx.content_rect().width();
            if (w - self.heights_w).abs() > 0.5 {
                self.heights.clear();
                self.heights_w = w;
            }
            let full = self.call.full && self.voice.chan.is_some() && self.cur() == self.voice.chan;
            self.call.full = full;
            self.apply_look(&ctx);
            let rail = if full {
                Rect::NOTHING
            } else {
                egui::Panel::left("rail")
                    .exact_size(72.0)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(Frame::new().fill(pal().bg_tertiary))
                    .show(ui, |ui| rail::show(self, ui))
                    .response
                    .rect
            };
            if !full {
                themes::show(self, ui);
            }
            // discord: 375 wide with the rail, then a 1px line before the chat, draggable between 264 and 432
            let side_w = if self.ui.side_w > 0.0 { self.ui.side_w } else { SIDE_W };
            let cr = if full {
                egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| chat::show(self, ui)).response.rect
            } else if self.directory.is_some() {
                // discord's discover page brings its own sidebar
                let c = egui::CentralPanel::default()
                    .frame(Frame::new().fill(pal().bg_primary).inner_margin(egui::Margin { top: 1, ..egui::Margin::ZERO }))
                    .show(ui, |ui| directory::show(self, ui));
                let side = egui::Rect::from_min_size(c.response.rect.min, vec2(side_w, c.response.rect.height()));
                side_edge(ui.painter(), side);
                side::card(self, ui, rail, side);
                c.response.rect
            } else {
                nav::keep_size(&ctx, "side", 0, side_w);
                // discord's dm list sits a pixel right of where a server's channels do
                let dms = self.ui.guild.is_none() as i8;
                let r = egui::Panel::left("side")
                    .default_size(side_w)
                    .size_range(193.0..=(w - 72.0 - 420.0).clamp(193.0, 361.0))
                    .resizable(true)
                    .show_separator_line(false)
                    .frame(Frame::new().fill(pal().bg_primary).inner_margin(egui::Margin { left: dms, right: 2 - dms, ..egui::Margin::ZERO }))
                    .show(ui, |ui| side::show(self, ui));
                side_edge(ui.painter(), r.response.rect);
                side::card(self, ui, rail, r.response.rect);
                if let Some(w) = self.dragged(&ctx, "side", r.response.rect.width(), side_w) {
                    self.ui.side_w = w;
                }
                // discord's frame line carries on from the sidebar's top over the chat and members
                let c = egui::CentralPanel::default()
                    .frame(Frame::new().fill(pal().bg_primary).inner_margin(egui::Margin { top: 1, ..egui::Margin::ZERO }))
                    .show(ui, |ui| chat::show(self, ui));
                c.response.rect
            };
            ui.painter().hline(cr.x_range(), cr.top() + 0.5, egui::Stroke::new(1.0, pal().frame_border));
            self.overlays(ui);
            callbar::popout(self, &ctx);
            self.update_title(&ctx);
            if !settings_open {
                self.ack_current(&ctx);
                if !layered && ctx.input_mut(|i| take_key(i, egui::Modifiers::COMMAND, egui::Key::K)) {
                    self.switcher = Some(String::new());
                }
                self.shortcuts(&ctx, layered);
            }
            if self.store.typing.values().flatten().any(|(_, t)| t.elapsed().as_secs() >= 10) {
                self.store.prune_typing();
            }
            if let Some(e) = &mut self.e2e
                && self.frames > 0
                && !self.store.guilds.is_empty()
            {
                e.mark_once("content");
            }
        }

        if self.token.is_some() {
            titlebar::inbox(self, &ctx);
        }

        // discord refreshes its activity feed now and then; ten minutes is close enough
        if self.feed_at.is_some_and(|t| Instant::now() >= t) {
            self.feed_at = Some(Instant::now() + Duration::from_secs(600));
            self.rest("GET", "/content-inventory/users/@me?for_game_profile=false&feature=inbox".into(), None, Tag::Feed);
        }
        favs::tick(self, &ctx);
        if self.save_at.is_some_and(|t| Instant::now() >= t) {
            self.save_at = None;
            let took = Instant::now();
            cache::save(&self.store, &self.ui, false);
            if let Some(e) = &mut self.e2e {
                e.cost("cache_save", took.elapsed());
            }
        }
        // images off screen for two minutes go, animations (all their frames in memory) after ten seconds
        if self.sweep_at.elapsed() >= Duration::from_secs(5) {
            self.sweep_at = Instant::now();
            for uri in self.textures.stale(Duration::from_secs(120), Duration::from_secs(10)) {
                ctx.forget_image(&uri);
            }
        }
        autoscroll::paint(&ctx, &self.autoscroll);
        // eframe opens links only with a feature we don't build, so hand them to the shell; under
        // e2e they're only written down
        let urls: Vec<String> = ctx.output_mut(|o| {
            let mut urls = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) => {
                    urls.push(u.url.clone());
                    false
                }
                _ => true,
            });
            urls
        });
        for u in urls {
            match &mut self.e2e {
                Some(e) => e.opened.push(u),
                None => {
                    let _ = open::that_detached(u);
                }
            }
        }
        if let Some(mut e) = self.e2e.take() {
            e.step(self, &ctx);
            e.frame_time(started.elapsed());
            self.e2e = Some(e);
        }
        motion::tooltips(&ctx);
        self.frames += 1;
        // fullscreen's size isn't the window's; keep the one to come back to
        if !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
            self.win = ctx.input(|i| i.viewport().inner_rect).map(|r| [r.width(), r.height()]);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // egui wakes a delayed repaint this much early, then spins a frame per vsync until it's
        // due: five frames for every blink of the caret
        raw.predicted_dt = 0.0;
        if let Some(e) = &mut self.e2e {
            e.inject(raw);
        }
        autoscroll::filter(&mut self.autoscroll, raw);
        let nav = self.focus_msg.is_none() && !self.modal_open() && !self.layered(ctx);
        self.wheel.filter(ctx, raw, nav);
        // ctrl+wheel: a zoom level a notch
        let mut notches = 0.0;
        raw.events.retain(|e| match e {
            egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.command => {
                notches += match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                    egui::MouseWheelUnit::Page => delta.y * 3.0,
                };
                false
            }
            _ => true,
        });
        self.zoom_acc += notches;
        while self.zoom_acc.abs() >= 1.0 {
            let up = self.zoom_acc > 0.0;
            self.zoom_acc -= if up { 1.0 } else { -1.0 };
            self.step_zoom(ctx, up);
        }
        // text pasted past the message limit goes up as message.txt instead, as in discord
        if ctx.memory(|m| m.has_focus(egui::Id::new("composer"))) {
            let limit = if self.store.nitro() { 4000 } else { 2000 };
            let mut long = Vec::new();
            raw.events.retain(|e| match e {
                egui::Event::Paste(t) if t.chars().count() > limit => {
                    long.push(t.clone());
                    false
                }
                _ => true,
            });
            for t in long {
                self.attach_text(&t);
            }
        }
        // dropping files on the window attaches them to the next message
        let dropped = !raw.dropped_files.is_empty();
        for f in raw.dropped_files.drain(..) {
            if self.interactions.modal.as_mut().is_some_and(|m| m.drop_file(f.path())) {
                continue;
            }
            if !server::dropped(self, f.path()) {
                self.attach_files(vec![f.path().to_path_buf()]);
            }
        }
        if dropped && key_down(0x10) && let Some(ch) = self.cur() {
            let draft = self.drafts.remove(&ch);
            self.post(ch, false);
            if let Some(d) = draft {
                self.drafts.insert(ch, d);
            }
        }
        if raw.focused && crate::winsys::take_paste() && !raw.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
            self.paste_files();
        }
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        let c = crate::theme::clear();
        c.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self) {
        crate::tray::remove();
        if self.token.is_some() {
            cache::save(&self.store, &self.ui, true);
        }
        if let Some(w) = self.win {
            let [r, g, b, _] = pal().bg_primary.to_array();
            cache::set_window(w, [r, g, b]);
        }
    }
}
