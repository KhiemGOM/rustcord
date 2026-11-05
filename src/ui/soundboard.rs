//! The soundboard popout over a guild call's controls: a grid of the server's sounds, each a
//! tile you click to preview-play.

use egui::{Align2, CornerRadius, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use crate::net::Tag;
use crate::state::Id;
use serde_json::json;
use super::icons::{self, Icon};
use crate::theme::*;

pub fn popout(app: &mut App, ui: &mut Ui, anchor: Rect) {
    let Some((guild, slot)) = app.soundboard.clone() else { return };
    let resp = egui::Area::new(egui::Id::new("soundboard_pop"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(anchor.center().x - 180.0, anchor.top() - 236.0))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(pal().bg_floating)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(336.0);
                    ui.set_height(210.0);
                    ui.label(RichText::new("Soundboard").font(semi(14.0)).color(pal().header_secondary));
                    ui.add_space(8.0);
                    match &slot {
                        None => {
                            ui.label(RichText::new("Loading…").font(reg(14.0)).color(pal().text_muted));
                        }
                        Some(sounds) if sounds.is_empty() => {
                            ui.label(RichText::new("This server has no soundboard sounds.").font(reg(14.0)).color(pal().text_muted));
                        }
                        Some(sounds) => {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                                    for s in sounds {
                                        let (r, resp) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::click());
                                        app.rects.insert(format!("sound:{}", s.id), r);
                                        ui.painter().rect(r, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                        let er = Rect::from_center_size(pos2(r.center().x, r.top() + 26.0), vec2(24.0, 24.0));
                                        match &s.emoji {
                                            Some(e) if e.id != 0 => super::text::image(ui.painter(), &super::chat::emoji_url(e), er),
                                            Some(e) => {
                                                ui.painter().text(er.center(), Align2::CENTER_CENTER, &e.name, reg(22.0), pal().header_primary);
                                            }
                                            None => icons::draw(ui.painter(), Icon::Soundboard, er, pal().header_secondary),
                                        }
                                        super::text::paint(ui.painter(), pos2(r.center().x, r.bottom() - 14.0), Align2::CENTER_CENTER, &s.name, reg(11.0), pal().header_secondary, 68.0);
                                        if resp.hovered() {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if resp.clicked() {
                                            app.play_sound(s.id, Some(guild));
                                        }
                                    }
                                });
                            });
                        }
                    }
                });
        })
        .response;
    app.rects.insert("voice:soundboard_pop".into(), resp.rect);
    if super::popups::dismissed(ui.ctx(), &resp) {
        app.soundboard = None;
    }
}

impl App {
    /// Plays a soundboard sound for you, at your soundboard volume, and sends it to the call.
    /// `source` is the server it belongs to, None for one of Discord's own.
    pub fn play_sound(&mut self, sound: Id, source: Option<Id>) {
        self.play_audio(format!("{}/soundboard-sounds/{}", crate::api::cdn(), sound));
        let Some(ch) = self.voice.chan else { return };
        let mut body = json!({ "sound_id": sound.to_string() });
        if let Some(g) = source {
            body["source_guild_id"] = json!(g.to_string());
        }
        self.rest("POST", format!("/channels/{ch}/send-soundboard-sound"), Some(body), Tag::Quiet);
    }

    /// Whatever sound is set to play when you join this server's calls, or any server's.
    pub fn entrance_sound(&mut self) {
        let g = self.voice.guild;
        let pick = self.settings.entrance.iter().find(|e| e.0 == g).or_else(|| self.settings.entrance.iter().find(|e| e.0 == 0)).copied();
        if let Some((_, sound, source)) = pick.filter(|_| g != 0) {
            self.play_sound(sound, Some(source).filter(|s| *s != 0));
        }
    }

    /// Someone's soundboard sound in the call we're in, unless we muted their soundboard or deafened.
    pub fn soundboard_effect(&mut self, d: &serde_json::Value) {
        let uid = crate::state::sid(&d["user_id"]);
        let sound = crate::state::sid(&d["sound_id"]);
        let here = self.voice.chan == Some(crate::state::sid(&d["channel_id"]));
        if sound == 0 || uid == self.store.me || !here || self.voice.deaf || self.peer(uid).soundboard_muted {
            return;
        }
        self.play_audio(format!("{}/soundboard-sounds/{sound}", crate::api::cdn()));
    }
}
