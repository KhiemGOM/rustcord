//! The soundboard popout over a guild call's controls: a grid of the server's sounds, each a
//! tile you click to preview-play.

use egui::{Align2, CornerRadius, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::theme::*;

pub fn popout(app: &mut App, ui: &mut Ui, anchor: Rect) {
    let Some((_, slot)) = app.soundboard.clone() else { return };
    let resp = super::motion::popout(ui.ctx(), egui::Area::new(egui::Id::new("soundboard_pop"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(anchor.center().x - 180.0, anchor.top() - 236.0)), vec2(0.0, -1.0))
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
                                        ui.painter().rect(r, CornerRadius::same(8), crate::ui::motion::mix(pal().bg_secondary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
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
                                            app.play_audio(format!("{}/soundboard-sounds/{}", crate::api::cdn(), s.id));
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
