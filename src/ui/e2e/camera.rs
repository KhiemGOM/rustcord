//! Voice & Video's Camera section (Test Video, the device list, video backgrounds) and the
//! call's camera button with its preview modal and menu, in steps 2800 and up. 2800 is entered
//! from step 60, 2820 from step 63 (as 2819); each ends by going back to the main chain.

use super::*;
use crate::voice::{effects, preview};
use std::sync::atomic::Ordering::Relaxed;

const SECOND: &str = effects::FAKE_CAMERAS[1];

impl Run {
    pub(super) fn camera_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let base = egui::Id::new("e2e_camera_base");
        let saved = |ctx: &egui::Context| ctx.data(|d| d.get_temp::<(u64, u64)>(base)).unwrap_or_default();
        let note = |ctx: &egui::Context| ctx.data_mut(|d| d.insert_temp(base, (effects::PLACED.load(Relaxed), effects::rev())));
        let goto = |ctx: &egui::Context, to: &str| ctx.data_mut(|d| d.insert_temp(egui::Id::new("settings_goto"), to.to_string()));
        let placed = |ctx: &egui::Context| effects::PLACED.load(Relaxed) > saved(ctx).0 + 5;
        match self.step {
            2800 => {
                app.settings.camera = None;
                app.settings.preview_video = false;
                app.settings.video_bg = crate::cache::VideoBg::None;
                app.settings.custom_bg = None;
                app.save_settings(ctx);
                goto(ctx, "Video Background");
                self.next(700);
            }
            2801 => {
                let Some(r) = Self::at(ctx, "camera:bg:blur") else { return };
                self.click(r.center());
                self.next(1800);
            }
            2802 => {
                if app.settings.video_bg == crate::cache::VideoBg::Blur
                    && effects::mode() == "blur"
                    && effects::LOADS.load(Relaxed) == 0
                    && effects::PLACED.load(Relaxed) == 0
                    && preview::RUNNING.load(Relaxed) == 0
                    && !app.voice.camera
                {
                    self.mark("camera_idle_no_load");
                }
                goto(ctx, "Camera");
                self.next(700);
            }
            2803 => {
                let Some(r) = Self::at(ctx, "camera:test") else { return };
                note(ctx);
                self.click(r.center());
                self.next(0);
            }
            2804 => {
                if app.voice.tex.contains_key("settings") && placed(ctx) && effects::LOADS.load(Relaxed) >= 1 {
                    self.mark("camera_test_video");
                    self.mark("camera_background");
                    self.shot(ctx, "settings_camera.png");
                    self.next(0);
                }
            }
            2805 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(0);
            }
            2806 => {
                if preview::RUNNING.load(Relaxed) == 0 {
                    self.mark("camera_preview_stops");
                    self.next(0);
                }
            }
            2807 => {
                if !self.open_page(app, "Voice & Video") {
                    return;
                }
                goto(ctx, "Video Background");
                self.next(1200);
            }
            2808 => {
                let path = self.dir.join("custom_bg.png");
                let _ = image::RgbImage::from_pixel(64, 36, image::Rgb([200, 40, 40])).save(&path);
                note(ctx);
                if super::super::settings::camera::use_custom(app, &path) {
                    app.save_settings(ctx);
                    self.next(0);
                }
            }
            2809 => {
                goto(ctx, "Camera");
                self.next(700);
            }
            2810 => {
                let Some(r) = Self::at(ctx, "camera:test") else { return };
                self.click(r.center());
                self.next(0);
            }
            2811 => {
                if effects::mode() == "picture" && effects::rev() > saved(ctx).1 && placed(ctx) {
                    self.mark("camera_bg_custom");
                    goto(ctx, "Video Background");
                    self.next(700);
                }
            }
            2812 => {
                let Some(r) = Self::at(ctx, "camera:bg:Cybercity") else { return };
                note(ctx);
                self.click(r.center());
                self.next(0);
            }
            2813 => {
                if app.settings.video_bg == crate::cache::VideoBg::Preset(0) && effects::mode() == "picture" && effects::rev() > saved(ctx).1 && placed(ctx) {
                    self.mark("camera_bg_preset");
                    goto(ctx, "Camera");
                    self.next(700);
                }
            }
            2814 => {
                let Some(r) = Self::at(ctx, "camera:device") else { return };
                self.click(r.center());
                self.next(400);
            }
            2815 => {
                let Some(r) = Self::at(ctx, &format!("camera:dev:{SECOND}")) else { return };
                self.click(r.center());
                self.next(0);
            }
            2816 => {
                if app.settings.camera.as_deref() == Some(SECOND) {
                    self.next(0);
                }
            }
            2817 => {
                let Some(r) = Self::at(ctx, "toggle:Always preview video") else { return };
                self.click(r.center());
                self.next(0);
            }
            2818 => {
                if app.settings.preview_video {
                    self.mark("camera_always_preview");
                    self.shot(ctx, "settings_voice.png");
                    self.step = 61;
                }
            }
            2819 => {
                let Some(_) = Self::at(ctx, "camera:modal:on") else { return };
                if app.voice.camera {
                    return self.fail("the camera went on before the preview modal was answered".into());
                }
                self.mark("camera_preview_modal");
                note(ctx);
                effects::note_open(String::new());
                self.next(700);
            }
            2820 => {
                let Some(r) = Self::at(ctx, "camera:modal:on") else { return };
                self.click(r.center());
                self.next(0);
            }
            2821 => {
                let sh = &app.voice.shared;
                if app.voice.camera && app.voice.video.contains_key(&app.store.me) && sh.video_out.load(Relaxed) > 10 {
                    self.mark("camera_on");
                    if Self::said(ctx, "voice_status").as_deref() == Some("Video Connected") {
                        self.mark("video_connected_text");
                    }
                    if effects::last_open().as_deref() == Some(SECOND) {
                        self.mark("camera_device");
                    }
                    if effects::mode() == "picture" && placed(ctx) {
                        self.mark("camera_stream_bg");
                    }
                    app.settings.preview_video = false;
                    self.next(0);
                }
            }
            2822 => {
                let Some(r) = app.rects.get("voice:camera_more").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            2823 => {
                let Some(r) = Self::at(ctx, "callcam:row") else { return };
                self.click(r.center());
                self.next(400);
            }
            2824 => {
                let Some(r) = Self::at(ctx, &format!("callcam:dev:{}", effects::FAKE_CAMERAS[0])) else { return };
                self.click(r.center());
                self.next(0);
            }
            2825 => {
                if effects::last_open().as_deref() == Some(effects::FAKE_CAMERAS[0]) && app.voice.camera {
                    let sent = app.voice.shared.video_out.load(Relaxed);
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("e2e_camera_sent"), sent));
                    self.next(1500);
                }
            }
            2826 => {
                if let Some(r) = app.rects.get("voice:status").copied() {
                    if self.loaded != 88 {
                        self.loaded = 88;
                        self.queue.push_back(vec![Event::PointerMoved(r.center() + vec2(1.0, 0.0))]);
                        self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                        return;
                    }
                    if Self::said(ctx, "voice_status").as_deref() == Some("Video Details") {
                        self.mark("video_details_hover");
                    }
                }
                let before = ctx.data(|d| d.get_temp::<u64>(egui::Id::new("e2e_camera_sent"))).unwrap_or(0);
                if app.voice.camera && app.voice.shared.video_out.load(Relaxed) > before + 5 {
                    self.mark("camera_callbar_menu");
                    self.key(Key::Escape);
                    self.step = 65;
                }
            }
            _ => self.step = 99,
        }
    }
}
