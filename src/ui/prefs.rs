use super::*;

impl App {
    pub fn save_settings(&self, ctx: &egui::Context) {
        self.apply_settings(ctx);
        cache::set_settings(&self.settings);
    }

    /// The account's values win at login and whenever one of them changes; a
    /// value the account still agrees on with last time leaves a local edit alone.
    pub(super) fn sync_account(&mut self, ctx: &egui::Context) {
        let now = self.store.account.clone();
        let changed = now.changed(self.synced.as_ref());
        self.synced = Some(now);
        // sync with computer is a local choice the account doesn't know about
        let (theme, preset, own) = (self.settings.theme, self.settings.custom_theme, self.settings.own_theme.clone());
        let custom_bg = self.settings.video_bg == crate::cache::VideoBg::Custom && self.settings.custom_bg.is_some();
        changed.apply(&mut self.settings);
        if custom_bg {
            self.settings.video_bg = crate::cache::VideoBg::Custom;
        }
        if theme == crate::theme::Theme::Sync || !self.settings.sync_theme {
            self.settings.theme = theme;
        }
        if !self.settings.sync_theme {
            (self.settings.custom_theme, self.settings.own_theme) = (preset, own);
        }
        self.heights.clear();
        self.save_settings(ctx);
    }

    /// Push every setting that lives outside `App` (theme, zoom, sounds, hotkeys...) to where it acts.
    pub(super) fn apply_settings(&self, ctx: &egui::Context) {
        let s = &self.settings;
        ctx.set_zoom_factor(s.zoom);
        crate::tray::set_close_hides(s.minimize_to_tray && std::env::var_os("RUSTCORD_BENCH").is_none());
        crate::media::set_volume(s.media_volume, s.media_muted);
        crate::media::set_gpu(s.hw_accel);
        crate::markdown::CLOCK24.store(s.clock24(), std::sync::atomic::Ordering::Relaxed);
        crate::markdown::set_date_order(&s.locale);
        crate::theme::set_reduced_motion(self.reduced_motion());
        crate::theme::set_view(crate::theme::View {
            underline_links: s.underline_links,
            animate_emoji: s.animate_emoji,
            hide_invites: s.streamer && s.hide_invites,
            hide_personal: s.hiding_personal(),
            name_styles: s.name_styles,
            own_profiles: s.own_theme_on_profiles,
            switch_icons: s.switch_icons,
            density: s.density,
        });
        self.apply_look(ctx);
        themes::icon::put(ctx, themes::icon::index(&s.app_icon));
        crate::speech::set_speed(s.tts_rate);
        crate::toast::set_long(s.min_toast_secs > 5.0);
        crate::theme::style(ctx);
        crate::sound::configure(s);
        self.sync_hotkeys();
        crate::clips::configure(s.clips, s.clips_secs);
        crate::api::set_locale(&s.locale);
        self.voice.configure(s);
    }

    pub(super) fn reduced_motion(&self) -> bool {
        if self.settings.sync_motion { !self.system.animations } else { self.settings.reduced_motion }
    }

    pub(super) fn apply_look(&self, ctx: &egui::Context) {
        let s = &self.settings;
        crate::theme::set_look(
            ctx,
            &crate::theme::Look {
                theme: s.theme,
                fill: themes::shown(self),
                saturation: s.saturation,
                saturate_custom: s.saturate_custom,
                contrast: s.high_contrast || (s.sync_contrast && self.system.high_contrast),
            },
        );
    }

    pub(super) fn speak_incoming(&self, m: &crate::state::Message, tts: bool) {
        let here = self.cur() == Some(m.channel);
        let s = &self.settings;
        let wanted = (s.tts_command && tts && here) || match s.tts_notify {
            2 => true,
            1 => here,
            _ => false,
        };
        if !wanted || m.author == self.store.me || self.voice.deaf {
            return;
        }
        let guild = self.store.channels.get(&m.channel).map_or(0, |c| c.guild);
        let text = crate::markdown::plain(&m.content, &self.store, guild);
        if !text.is_empty() {
            crate::speech::say(format!("{} says {text}", self.store.name_in(guild, m.author)));
        }
    }

    pub(super) fn notify(&mut self, d: &serde_json::Value, ctx: &egui::Context) {
        let m = self.store.parse_message(d);
        self.speak_incoming(&m, d["tts"].as_bool() == Some(true));
        if m.author == self.store.me || !self.settings.notify || (self.settings.streamer && self.settings.streamer_no_notifications) {
            return;
        }
        // do not disturb: no desktop notifications, no sound, no flashing, as discord promises
        if self.store.presence.get(&self.store.me) == Some(&Status::Dnd) {
            return;
        }
        let Some(c) = self.store.channels.get(&m.channel) else { return };
        if self.store.muted.contains(&c.id) || self.store.muted.contains(&c.guild) {
            return;
        }
        let focused = ctx.input(|i| i.raw.focused);
        if focused && self.cur() == Some(c.id) {
            if self.settings.notify_selected {
                crate::sound::cue("message");
            }
            return;
        }
        let dm = c.guild == 0;
        if !(self.store.mentions_me(&m) || dm) {
            return;
        }
        let who = self.store.name_in(c.guild, m.author);
        let title = match self.store.guilds.get(&c.guild) {
            Some(g) => format!("{who} (#{}, {})", c.name, g.name),
            None if c.kind == 3 => format!("{who} ({})", channel_title(&self.store, c.id)),
            None => who,
        };
        // mentions and channels by name, the markdown taken off, as discord's toasts read
        let mut body: String = crate::markdown::plain(&m.content, &self.store, c.guild).chars().take(200).collect();
        if body.is_empty() && !m.attachments.is_empty() {
            body = "Uploaded a file".into();
        }
        let avatar = self.store.user(m.author).map(|u| u.avatar_url()).unwrap_or_default();
        let ping = crate::toast::Ping { title, body, avatar, at: (c.id, m.id), call: false };
        crate::sound::cue("message");
        if let Some(e) = &mut self.e2e {
            e.mark_once("notified");
            e.pinged.push(c.id);
            e.pings.push((crate::toast::xml(&ping, None), ping.at));
        } else {
            crate::toast::send(ping);
        }
        if !focused && self.settings.flash && self.e2e.is_none() {
            crate::winsys::flash();
        }
    }

    /// Settings that react to the outside world every frame: system theme,
    /// ctrl+wheel zoom, global keybinds, OBS starting, capture exclusion.
    pub(super) fn os_settings(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let now = ctx.input(|i| i.time);
        if now >= self.system_at {
            self.system_at = now + 2.0;
            let sys = crate::winsys::preferences();
            if (sys.animations, sys.high_contrast) != (self.system.animations, self.system.high_contrast) {
                self.system = sys;
                self.apply_settings(ctx);
            }
        }
        if self.settings.theme == crate::theme::Theme::Sync {
            self.apply_look(ctx);
        }
        let z = ctx.zoom_factor();
        if (z - self.settings.zoom).abs() > 0.001 {
            self.settings.zoom = z;
            cache::set_settings(&self.settings);
        }
        let events: Vec<(String, bool)> = self.hotkey_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        crate::hotkeys::pause(self.settings_page == Some(settings::Page::System));
        for (action, pressed) in events {
            if action == "ptt" && self.settings.ptt && self.voice.chan.is_some() && !self.voice.mute {
                crate::sound::cue(if pressed { "ptt_start" } else { "ptt_stop" });
            }
            if action == "soundboard_hold" && !pressed {
                self.soundboard = None;
            }
            if !pressed {
                continue;
            }
            match action.as_str() {
                "voice_mode" => {
                    self.settings.ptt = !self.settings.ptt;
                    self.save_settings(ctx);
                }
                "voice_chat" if self.voice.chan.is_some() => self.voice_chat = !self.voice_chat,
                "camera" if self.voice.chan.is_some() => {
                    let on = !self.voice.camera;
                    self.voice.set_camera(&self.net, self.store.me, on);
                }
                "screenshare" if self.voice.chan.is_some() => {
                    if self.voice.stream.as_ref().is_some_and(|l| l.mine) {
                        self.voice.end_stream(&self.net);
                    } else if self.share.is_none() {
                        self.show_window(ctx);
                        self.share = Some(share::Picker::new(ctx));
                    }
                }
                "back" => self.go(true),
                "forward" => self.go(false),
                "goto_voice" => {
                    if let Some(c) = self.voice.chan {
                        self.show_window(ctx);
                        let g = self.store.channels.get(&c).map_or(0, |c| c.guild);
                        self.ui.guild = (g != 0).then_some(g);
                        self.open(c);
                    }
                }
                "disconnect" => self.voice.leave(&self.net),
                "soundboard" | "soundboard_hold" => {
                    let guild = self.voice.chan.and_then(|c| self.store.channels.get(&c)).map_or(0, |c| c.guild);
                    if self.soundboard.is_some() && action == "soundboard" {
                        self.soundboard = None;
                    } else if guild != 0 {
                        self.rest("GET", format!("/guilds/{guild}/soundboard-sounds"), None, crate::net::Tag::Soundboard(guild));
                        self.soundboard = Some((guild, None));
                    }
                }
                "mute" => {
                    let on = !self.voice.mute;
                    self.voice.set_mute(&self.net, on);
                }
                "deafen" => {
                    let on = !self.voice.deaf;
                    self.voice.set_deaf(&self.net, on);
                }
                "streamer" => {
                    self.settings.streamer = !self.settings.streamer;
                    self.save_settings(ctx);
                }
                "save_clip" => self.save_clip(ctx),
                "open" => self.show_window(ctx),
                "badge" => self.badge = None,
                "hide" => self.hide_to_tray(ctx),
                "voice_settings" => {
                    self.show_window(ctx);
                    self.settings_page = Some(settings::Page::Voice);
                }
                "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                _ => {}
            }
        }
        self.clips_tick(ctx);
        let obs = crate::winsys::OBS_RUNNING.load(std::sync::atomic::Ordering::Relaxed);
        if obs != self.obs_seen {
            self.obs_seen = obs;
            if self.settings.streamer_auto {
                self.settings.streamer = obs;
                self.save_settings(ctx);
            }
        }
        let hide = self.settings.streamer && self.settings.hide_window;
        if hide != self.capture_hidden {
            self.capture_hidden = hide;
            crate::winsys::exclude_from_capture(frame, hide);
        }
    }

    // the purple strip the official client shows across the top in streamer mode
    pub(super) fn streamer_banner(&mut self, ui: &mut Ui) {
        if !self.settings.streamer {
            return;
        }
        let mut off = false;
        egui::Panel::top("streamer_banner")
            .exact_size(36.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::from_rgb(0x59, 0x3b, 0xd3)))
            .show(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                let msg = p.layout_no_wrap("Streamer Mode is enabled. Stay safe, friend.".into(), semi(14.0), Color32::WHITE);
                let link = p.layout_no_wrap("Disable".into(), semi(14.0), Color32::WHITE);
                let total = msg.size().x + 10.0 + link.size().x;
                let x = r.center().x - total / 2.0;
                let y = r.center().y - msg.size().y / 2.0;
                let lr = Rect::from_min_size(pos2(x + msg.size().x + 10.0, y), link.size());
                p.galley(pos2(x, y), msg, Color32::WHITE);
                p.galley(lr.min, link, Color32::WHITE);
                p.hline(lr.x_range(), lr.bottom(), egui::Stroke::new(1.0, Color32::WHITE));
                let resp = ui.interact(lr, egui::Id::new("streamer_off"), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                off = resp.clicked();
            });
        if off {
            self.settings.streamer = false;
            self.settings.streamer_auto = false;
            self.save_settings(ui.ctx());
        }
    }

    pub(super) fn update_title(&mut self, ctx: &egui::Context) {
        let n = self.store.mention_count()
            + self.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
        if self.title_count != Some(n) {
            self.title_count = Some(n);
            let t = if n > 0 { format!("({n}) Rustcord") } else { "Rustcord".into() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
        // discord's taskbar badge: the count, or with nothing mentioning you and the unread badge
        // on, a dot while anything's unread
        let mut badge = n.min(i32::MAX as u32) as i32;
        if badge == 0 && self.settings.badge && self.store.any_unread() {
            badge = -1;
        }
        if self.badge != Some(badge) && self.store.me != 0 {
            crate::winsys::set_badge(badge);
            if crate::winsys::badge() == badge {
                self.badge = Some(badge);
            }
        }
        let v = &self.voice;
        let me = self.store.me;
        let m = v.muted.get(&me).copied().unwrap_or_default();
        crate::tray::set(crate::tray::State {
            unread: self.settings.badge && badge != 0,
            connected: v.state == crate::voice::State::Connected,
            speaking: v.speaking.contains(&me),
            mute: v.mute || v.deaf,
            server_mute: m[1],
            deaf: v.deaf,
            server_deaf: m[3],
        });
    }

    /// Pages nothing touches go back to Windows: what startup left behind (READY's parse, setting
    /// up) within the first half minute, and everything once the window has sat hidden for 5 s or out of focus
    /// for a minute. Drawing and the gateway touch about 40 MB; the rest comes back if it's needed.
    pub(super) fn trim_memory(&mut self, ctx: &egui::Context) {
        let away = self.away(ctx);
        let wait = if away { Duration::from_secs(5) } else { Duration::from_secs(60) };
        if !(away || !ctx.input(|i| i.focused)) {
            self.quiet = None;
        } else if self.quiet.is_none() {
            self.quiet = Some((Instant::now(), false));
        }
        let startup = [12, 30].get(self.startup_trims).map(|s| self.born + Duration::from_secs(*s));
        let quiet = self.quiet.filter(|q| !q.1).map(|(at, _)| at + wait);
        let Some(due) = startup.into_iter().chain(quiet).min() else { return };
        let now = Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        let settled = self.quiet.is_some_and(|(at, done)| !done && at.elapsed() >= wait);
        if settled {
            crate::sound::forget();
            textfile::forget();
            if away {
                self.spell.release();
            }
        }
        crate::winsys::trim();
        if startup.is_some_and(|at| at <= now) {
            self.startup_trims += 1;
        }
        if settled {
            self.quiet = self.quiet.map(|(at, _)| (at, true));
        }
    }

    /// Minimized or put away in the tray: eframe stops calling ui() for the first and we skip
    /// drawing for the second, so logic() keeps the badge and the tray icon up to date.
    pub(super) fn away(&self, ctx: &egui::Context) -> bool {
        self.frames > 0 && (ctx.input(|i| i.viewport().visible() == Some(false)) || !crate::winsys::shown())
    }

    pub(super) fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        // winit focuses by faking an alt press, which the e2e mustn't do to whoever's at the machine
        if self.e2e.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    pub(crate) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if !self.settings.tray_hint && self.e2e.is_none() {
            crate::tray::hint();
            self.settings.tray_hint = true;
            cache::set_settings(&self.settings);
        }
    }

    /// A clicked toast: the window back up and the message on screen.
    pub(crate) fn open_ping(&mut self, ctx: &egui::Context, ch: Id, msg: Id) {
        self.show_window(ctx);
        if self.store.channels.contains_key(&ch) {
            self.jump_to(ch, msg);
        }
    }
}
