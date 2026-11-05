//! The inline players against the mock's clip.mp4, tone.mp3 and hello.rs in #media.

use super::Run;
use crate::media;
use crate::state::Id;
use crate::ui::{App, textfile};
use egui::{Event, Key, Modifiers, Rect, pos2, vec2};
use serde_json::json;
use std::time::{Duration, Instant};

const MEDIA: Id = 2060;
const VIDEO: &str = "clip.mp4";
const AUDIO: &str = "tone.mp3";
const TEXT: &str = "hello.rs";

fn url(app: &App, file: &str) -> Option<String> {
    app.store.msgs.get(&MEDIA)?.list.iter().flat_map(|m| &m.attachments).find(|a| a.filename == file).map(|a| a.url.clone())
}

fn msg_with(app: &App, file: &str) -> Option<Id> {
    app.store.msgs.get(&MEDIA)?.list.iter().find(|m| m.attachments.iter().any(|a| a.filename == file)).map(|m| m.id)
}

fn card(app: &App, kind: &str) -> Option<Rect> {
    app.rects.iter().find(|(k, _)| k.starts_with(&format!("{kind}:"))).map(|(_, r)| *r)
}

fn bar(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<(Rect, Rect)>(egui::Id::new(("player_bar", false)))).map(|b| b.0)
}

fn state(url: &str) -> Option<media::View> {
    media::current(url).map(|s| s.view())
}

impl Run {
    pub(super) fn player_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let ready = app.cur() == Some(MEDIA) && app.store.msgs.get(&MEDIA).is_some_and(|e| e.list.len() >= 8 && e.fresh && !e.loading);
        match self.step {
            1800 => {
                app.select_guild(Some(self.target.0));
                app.open(MEDIA);
                self.next(800);
            }
            1801 => {
                let Some(id) = msg_with(app, VIDEO).filter(|_| ready) else { return };
                app.scroll_to = Some((MEDIA, id));
                self.next(700);
            }
            1802 => {
                let (Some(r), Some(u)) = (card(app, "player_video"), url(app, VIDEO)) else { return };
                if media::current(&u).is_none() {
                    self.mark("player_poster");
                }
                self.click(r.center());
                self.next(300);
            }
            1803 => {
                let Some(v) = url(app, VIDEO).and_then(|u| state(&u)) else { return };
                if v.playing && v.pos > 0.3 && matches!(v.phase, media::Phase::Ready) {
                    self.mark("player_video_plays");
                    self.rects.insert("player_video_info".into(), json!({ "w": v.info.w, "h": v.info.h, "dur": v.info.dur, "fps": v.info.fps }));
                    self.shot(ctx, "player_t0.png");
                    self.next(0);
                }
            }
            1804 => {
                if self.pending_shots == 0 {
                    self.rects.insert("player_uploads_0".into(), json!(media::uploads()));
                    self.next(250);
                }
            }
            1805 => {
                self.shot(ctx, "player_t1.png");
                self.next(0);
            }
            1806 => {
                let Some(r) = card(app, "player_video").filter(|_| self.pending_shots == 0) else { return };
                let n = self.changed("player_t0.png", "player_t1.png", r.shrink2(vec2(0.0, 40.0)), ctx.pixels_per_point());
                self.rects.insert("player_pixels_changed".into(), json!(n));
                let before = self.rects.get("player_uploads_0").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
                if n > 0 && media::uploads() > before {
                    self.mark("player_video_advances");
                }
                self.click(r.center());
                self.next(300);
            }
            1807 => {
                let Some(v) = url(app, VIDEO).and_then(|u| state(&u)) else { return };
                if !v.playing {
                    self.mark("player_video_pauses");
                    self.rects.insert("player_paused_at".into(), json!([media::uploads(), v.pos]));
                    self.next(900);
                }
            }
            1808 => {
                let Some(v) = url(app, VIDEO).and_then(|u| state(&u)) else { return };
                let at = self.rects.get("player_paused_at").cloned().unwrap_or_default();
                if at[0].as_u64() == Some(media::uploads()) && (v.pos - at[1].as_f64().unwrap_or(-1.0)).abs() < 0.01 && !v.playing {
                    self.mark("player_pause_idle");
                }
                let Some(b) = bar(ctx) else { return self.fail("no seek bar while a video was open".into()) };
                self.click(pos2(b.left() + b.width() * 0.75, b.center().y));
                self.next(500);
            }
            1809 => {
                let Some(v) = url(app, VIDEO).and_then(|u| state(&u)) else { return };
                self.rects.insert("player_seek_pos".into(), json!([v.pos, v.info.dur]));
                if (v.pos - v.info.dur * 0.75).abs() < 0.4 && !v.playing {
                    self.mark("player_seek");
                }
                self.key(Key::K);
                self.next(300);
            }
            1810 => {
                if url(app, VIDEO).and_then(|u| state(&u)).is_some_and(|v| v.playing) {
                    self.rects.insert("player_k_played".into(), json!(true));
                }
                self.key(Key::M);
                self.next(300);
            }
            1811 => {
                if media::volume().1 && app.settings.media_muted && self.rects.contains_key("player_k_played") {
                    self.mark("player_keys");
                }
                self.key(Key::M);
                self.next(300);
            }
            1812 => {
                if media::volume().1 {
                    return;
                }
                self.key(Key::F);
                self.next(400);
            }
            1813 => {
                if app.media_fs.is_some() {
                    self.mark("player_fullscreen");
                    self.shot(ctx, "player_fullscreen.png");
                    self.key(Key::Escape);
                    self.next(400);
                }
            }
            1814 => {
                if app.media_fs.is_none() && self.pending_shots == 0 {
                    self.mark("player_fullscreen_exit");
                    if let Some(s) = url(app, VIDEO).and_then(|u| media::current(&u)) {
                        s.play();
                    }
                    for _ in 0..40 {
                        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -400.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                    }
                    self.next(900);
                }
            }
            1815 => {
                let Some(u) = url(app, VIDEO) else { return };
                if card(app, "player_video").is_none() && state(&u).is_some_and(|v| !v.playing) {
                    self.mark("player_offscreen_pause");
                    let Some(id) = msg_with(app, AUDIO) else { return };
                    app.scroll_to = Some((MEDIA, id));
                    self.next(800);
                }
            }
            1816 => {
                let Some(r) = card(app, "player_audio") else { return };
                self.click(pos2(r.left() + 33.0, r.bottom() - 33.0));
                self.next(400);
            }
            1817 => {
                let Some(u) = url(app, AUDIO) else { return };
                if state(&u).is_some_and(|v| v.playing && v.pos > 0.3 && matches!(v.phase, media::Phase::Ready)) {
                    self.mark("player_audio_plays");
                    if url(app, VIDEO).is_some_and(|v| media::current(&v).is_none()) {
                        self.mark("player_one_at_a_time");
                    }
                    let Some(b) = bar(ctx) else { return self.fail("no seek bar while audio was open".into()) };
                    self.click(pos2(b.left() + b.width() * 0.5, b.center().y));
                    self.next(400);
                }
            }
            1818 => {
                let Some(v) = url(app, AUDIO).and_then(|u| state(&u)) else { return };
                self.rects.insert("player_audio_seek_pos".into(), json!([v.pos, v.info.dur]));
                if (v.pos - v.info.dur * 0.5).abs() < 0.8 {
                    self.mark("player_audio_seek");
                }
                if let Some(s) = url(app, AUDIO).and_then(|u| media::current(&u)) {
                    s.pause();
                }
                let Some(id) = msg_with(app, TEXT) else { return };
                app.scroll_to = Some((MEDIA, id));
                self.next(800);
            }
            1819 => {
                let (Some(r), Some(u)) = (card(app, "player_text"), url(app, TEXT)) else { return };
                if !textfile::loaded(&u) {
                    return;
                }
                self.mark("player_text_loads");
                self.rects.insert("player_text_h0".into(), json!(r.height()));
                self.shot(ctx, "player_text.png");
                self.click(pos2(r.left() + 33.0, r.bottom() - 33.0));
                self.next(400);
            }
            1820 => {
                let Some(r) = card(app, "player_text") else { return };
                if r.height() > self.rects.get("player_text_h0").and_then(|v| v.as_f64()).unwrap_or(f64::MAX) as f32 + 20.0 {
                    self.mark("player_text_expands");
                    self.click(pos2(r.left() + 60.0, r.top() + 40.0));
                    self.next(200);
                }
            }
            1821 => {
                self.queue.push_back(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
                self.next(300);
            }
            1822 => {
                let Some(u) = url(app, TEXT) else { return };
                if textfile::selected(ctx, &u) {
                    self.mark("player_text_selects");
                }
                self.shot(ctx, "player_text_selected.png");
                self.next(0);
            }
            1823 => {
                if self.pending_shots == 0 {
                    self.step = 1900;
                    self.wait = Instant::now() + Duration::from_millis(100);
                }
            }
            _ => self.step = 99,
        }
    }
}
