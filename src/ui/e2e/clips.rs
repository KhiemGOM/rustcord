use egui::{Event, Key, Modifiers};
use std::time::Duration;

use super::Run;
use crate::clips;
use crate::net::Conn;
use crate::state::Id;
use crate::ui::App;
use crate::ui::settings::Page;
use crate::voice::{Quality, Source, State};

const DEN: Id = 1008;
const HANGOUT: Id = 2801;
const SPEAKER: Id = 101;

impl Run {
    /// Clips against the mock's call and the fake screen: the settings page, a real Go Live
    /// filling the buffer, the button's save, the gallery, the trimmer, sharing, the consent flags
    /// and the warning toast, then everything freed when the stream ends. Needs the fake screen.
    pub(super) fn clips_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let me = app.store.me;
        let dir = self.dir.join("clips");
        let rect = |key: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("e2e", key))));
        let mp4s = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
            std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "mp4")).collect()
        };
        let waited = |run: &Self, secs: u64| run.step_at.elapsed() > Duration::from_secs(secs);
        let toast = |app: &App, text: &str| app.toast.as_ref().is_some_and(|(t, _)| t.starts_with(text));
        match self.step {
            2900 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&HANGOUT) {
                    return;
                }
                if std::env::var_os("RUSTCORD_FAKE_VIDEO").is_none() {
                    self.step = 4000;
                    return;
                }
                app.settings.clips = true;
                app.settings.clips_secs = 5;
                app.settings.clips_dir = dir.display().to_string();
                app.save_settings(ctx);
                app.settings_page = Some(Page::Clips);
                self.next(600);
            }
            2901 => {
                if rect("toggle:Enable Clipping").is_some() && rect("action:Clips").is_some() {
                    self.mark("clips_page");
                }
                if app.clip_keys().as_deref() == Some("ALT + C") && app.clip_tip() == "Stream to Start Clipping" {
                    self.mark("clips_default_keybind");
                }
                self.shot(ctx, "clips_settings.png");
                self.next(300);
            }
            2902 => {
                app.settings_page = None;
                app.select_guild(Some(DEN));
                app.open(HANGOUT);
                app.join_voice(HANGOUT, false);
                self.next(0);
            }
            2903 => {
                if app.voice.state == State::Connected && app.voice.chan == Some(HANGOUT) {
                    self.next(1000);
                }
            }
            2904 => {
                app.voice.go_live(&app.net, me, Source::Monitor(0), Quality { height: 360, fps: 15 });
                self.next(0);
            }
            2905 => {
                if app.voice.stream.as_ref().is_some_and(|l| l.mine && l.state == State::Connected && l.frame.is_some()) {
                    if clips::active() && clips::buffered() > 0 {
                        self.mark("clips_buffering");
                    }
                    if crate::hotkeys::watching() {
                        self.mark("clips_hotkey_on");
                    }
                    self.next(6500);
                } else if waited(self, 15) {
                    self.next(0);
                }
            }
            2906 => {
                let Some(b) = app.rects.get("clips:button").copied() else { return };
                if app.clip_tip() == "Save Clip (ALT + C)" {
                    self.mark("clips_tooltip");
                }
                self.shot(ctx, "clips_button.png");
                self.click(b.center());
                self.next(300);
            }
            2907 => {
                if !mp4s(&dir).is_empty() && clips::saving() == 0 || waited(self, 15) {
                    self.next(500);
                }
            }
            2908 => {
                if let Some(f) = mp4s(&dir).first() {
                    self.mark("clip_saved");
                    let bytes = std::fs::read(f).unwrap_or_default();
                    let secs: f64 = crate::mp4::frames(&bytes, 64 << 20).unwrap_or_default().iter().map(|p| p.1.as_secs_f64()).sum();
                    if (4.5..12.0).contains(&secs) && bytes.windows(4).any(|w| w == b"mp4a") {
                        self.mark("clip_length_and_audio");
                    }
                    if f.with_extension("json").exists() && f.with_extension("jpg").exists() {
                        self.mark("clip_metadata");
                    }
                }
                if toast(app, "You recently captured a clip!") {
                    self.mark("clip_toast");
                }
                self.next(0);
            }
            2909 => {
                app.settings_page = Some(Page::Privacy);
                if rect("toggle:Allow my voice to be recorded in Clips").is_some() {
                    self.mark("clips_privacy_switch");
                    app.settings_page = None;
                    self.next(300);
                } else if waited(self, 8) {
                    app.settings_page = None;
                    self.next(0);
                }
            }
            2910 => {
                app.open_clips();
                self.next(500);
            }
            2911 => {
                if app.clips.list.as_ref().is_some_and(|l| !l.is_empty()) {
                    self.mark("clips_gallery");
                    self.shot(ctx, "clips_gallery.png");
                    let c = app.clips.list.as_ref().and_then(|l| l.first().cloned());
                    if let (Some(c), Some(g)) = (c, app.clips.gallery.as_mut()) {
                        g.edit = Some(crate::ui::clips_trim::Trim::new(&c));
                    }
                    self.next(1500);
                } else if waited(self, 8) {
                    self.next(0);
                }
            }
            2912 => {
                self.shot(ctx, "clips_trimmer.png");
                for (key, modifiers) in [(Key::ArrowRight, Modifiers::NONE), (Key::ArrowRight, Modifiers::SHIFT), (Key::Tab, Modifiers::NONE), (Key::ArrowLeft, Modifiers::NONE)] {
                    self.queue.push_back(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
                }
                self.next(600);
            }
            2913 => {
                let first = app.clips.list.as_ref().and_then(|l| l.first().cloned());
                let t = app.clips.gallery.as_ref().and_then(|g| g.edit.as_ref());
                if let (Some(t), Some(c)) = (t, &first)
                    && t.start == c.trim_start_ms + 1100
                    && t.end == c.length_ms - 100
                {
                    self.mark("clip_trim_keys");
                }
                if let Some(mut c) = first {
                    c.edit = Some((1000, 3000));
                    let out = crate::ui::clips_trim::export(&c);
                    let secs = out.and_then(|p| std::fs::read(p).ok()).and_then(|b| crate::mp4::frames(&b, 64 << 20).ok()).map_or(0.0, |f| f.iter().map(|p| p.1.as_secs_f64()).sum::<f64>());
                    if (secs - 2.0).abs() < 0.2 {
                        self.mark("clip_trim_export");
                    }
                }
                if let Some(g) = app.clips.gallery.as_mut() {
                    g.edit = None;
                }
                self.step = 2929;
                self.next(300);
            }
            2930 => {
                let c = app.clips.list.as_ref().and_then(|l| l.first().cloned());
                if let (Some(c), Some(mut g)) = (c, app.clips.gallery.take()) {
                    crate::ui::clips_gallery::play(app, ctx, &mut g, c);
                    app.clips.gallery = Some(g);
                }
                self.next(0);
            }
            2931 => {
                let c = app.clips.list.as_ref().and_then(|l| l.first().cloned());
                let s = c.and_then(|c| crate::media::current(&crate::ui::clips_gallery::clip_url(&c)));
                if s.is_some_and(|s| s.view().playing && s.pos() > 0.5) {
                    self.mark("clip_plays_inline");
                    self.shot(ctx, "clips_player.png");
                    self.key(Key::Escape);
                    self.next(500);
                } else if waited(self, 8) {
                    self.next(0);
                }
            }
            2932 => {
                if app.clips.gallery.as_ref().is_some_and(|g| g.playing.is_none()) {
                    self.mark("clip_player_closed");
                }
                self.step = 2913;
                self.next(0);
            }
            2914 => {
                if let Some(c) = app.clips.list.as_ref().and_then(|l| l.first().cloned()) {
                    crate::ui::clips_gallery::send_clips(ctx, HANGOUT, "my clip".into(), &[c]);
                }
                app.clips.gallery = None;
                self.next(1000);
            }
            2915 => {
                if clips::others_clipping(me) && clips::allowed(SPEAKER) {
                    self.mark("clips_consent_flags");
                }
                if app.clips.warned_for(HANGOUT) {
                    self.mark("clips_call_warning");
                }
                if app.rects.contains_key("clips:badge") {
                    self.mark("clips_badge");
                }
                self.next(0);
            }
            2916 => {
                app.voice.end_stream(&app.net);
                self.next(600);
            }
            2917 => {
                if (app.voice.stream.is_some() || clips::streaming()) && !waited(self, 8) {
                    return;
                }
                if !clips::active() && !clips::streaming() && clips::buffered() == 0 {
                    self.mark("clips_freed");
                }
                self.next(200);
            }
            2918 => {
                if toast(app, "You captured clips during your recent") {
                    self.mark("clips_reminder");
                }
                if app.settings.keybinds.iter().filter(|(a, _)| crate::ui::clips::is_save_clip(a)).count() == 1 {
                    self.mark("clips_keybind_once");
                }
                app.settings.clips = false;
                app.save_settings(ctx);
                self.next(300);
            }
            2919 => {
                if !crate::hotkeys::watching() {
                    self.mark("clips_hotkey_off");
                }
                app.voice.leave(&app.net);
                self.step = 4000;
            }
            _ => self.step = 99,
        }
    }
}
