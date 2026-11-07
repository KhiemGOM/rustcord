use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, CornerRadius, Rect, RichText, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, Trail, menu_item, menu_row, menu_sep};
use crate::cache::Peer;
use crate::state::{Channel, Id};
use crate::theme::*;

impl App {
    pub fn set_peer(&mut self, user: Id, peer: Peer, save: bool) {
        if peer.muted || peer.soundboard_muted || peer.volume != 100.0 {
            self.settings.peers.insert(user, peer);
        } else {
            self.settings.peers.remove(&user);
        }
        self.voice.configure(&self.settings);
        if save {
            crate::cache::set_settings(&self.settings);
        }
    }

    pub fn peer(&self, user: Id) -> Peer {
        self.settings.peers.get(&user).copied().unwrap_or_default()
    }
}

pub fn options(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, tile: Rect, shown: f32) {
    let resp = ui.interact(tile, egui::Id::new(("tile", u)), Sense::click());
    let me = app.store.me;
    let live = app.voice.live.contains(&u) && u != me && app.voice.chan.is_some();
    if live {
        app.rects.insert(format!("voice:watch:{u}"), tile);
        super::call::watch_hint(ui, tile, &resp);
        if resp.clicked() {
            app.voice.watch(&app.net, u);
        }
    }
    let btn = Rect::from_min_size(pos2(tile.right() - 12.0 - 30.0, tile.bottom() - 12.0 - 30.0), vec2(30.0, 30.0));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let hot = resp.hovered() || open || ui.input(|i| i.pointer.hover_pos().is_some_and(|p| btn.contains(p)));
    let b = ui.interact(btn, egui::Id::new(("tile_opts", u)), Sense::click());
    if hot {
        ui.painter().rect_filled(btn, CornerRadius::same(8), Color32::from_black_alpha(143));
        icons::draw(ui.painter(), Icon::More, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), Color32::WHITE.gamma_multiply(shown.max(0.5)));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        app.rects.insert(format!("voice:tile_opts:{u}"), btn);
    }
    let pos = btn.center();
    let b = b.on_hover_text("Options");
    let menu = |app: &mut App, ui: &mut Ui| body(app, ui, chan, u, pos);
    egui::Popup::context_menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| menu(app, ui));
    egui::Popup::menu(&b).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| menu(app, ui));
}

fn body(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, at: egui::Pos2) {
    let me = app.store.me;
    ui.set_width(220.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    if menu_item(ui, None, "Profile", false) {
        app.open_profile(u, chan.guild, at);
        ui.close();
    }
    if u != me && menu_item(ui, None, "Message", false) {
        app.open_dm(u);
        ui.close();
    }
    if u != me {
        menu_sep(ui);
        let mut peer = app.peer(u);
        let mut at = super::usermenu::vol_pos(peer.volume / 100.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("User Volume").font(medium(14.0)).color(pal().header_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{}%", peer.volume.round())).font(reg(12.0)).color(pal().text_muted));
            });
        });
        ui.add_space(4.0);
        let moved = ui
            .horizontal(|ui| {
                ui.add_space(8.0);
                let w = ui.available_width() - 8.0;
                super::settings::slider(ui, w, &mut at, 0.0, 1.0)
            })
            .inner;
        if moved {
            peer.volume = (super::usermenu::vol_gain(at) * 100.0).round();
            app.set_peer(u, peer, false);
        }
        if ui.input(|i| i.pointer.any_released()) && app.settings.peers.get(&u).is_some_and(|p| p.volume != 100.0) {
            crate::cache::set_settings(&app.settings);
        }
        ui.add_space(4.0);
        menu_sep(ui);
        if menu_row(ui, None, "Mute", None, false, Trail::Check(peer.muted)).clicked() {
            peer.muted = !peer.muted;
            app.set_peer(u, peer, true);
        }
    }
    let focused = app.call.focus == Some(u);
    if menu_item(ui, None, if focused { "Unfocus" } else { "Focus This Person" }, false) {
        app.call.focus = if focused { None } else { Some(u) };
        ui.close();
    }
    let full = app.call.full;
    if menu_item(ui, None, if full { "Exit Full Screen" } else { "Full Screen" }, false) {
        app.call.focus = Some(u);
        app.call.full = !full;
        ui.close();
    }
    super::stage::tile_items(app, ui, chan, u);
}

/// Developer mode's Stream Info Overlay: what the person's video is, shown over their tile under the pointer.
pub fn stream_info(ui: &Ui, app: &App, user: Id, tile: Rect) {
    let img = app.voice.video.get(&user);
    let size = img.map(|i| i.source_size).filter(|s| s.x > 0.0);
    let fps = app.voice.video_rate.get(&user).map_or(0, |r| r.2);
    let rows = [
        ("Video Codec: ", if size.is_some() { "H264".to_string() } else { "-".into() }),
        ("Resolution: ", size.map_or("-".into(), |s| format!("{} x {}", s.x as u32, s.y as u32))),
        ("FPS: ", if size.is_some() && fps > 0 { fps.to_string() } else { "-".into() }),
    ];
    let mut job = LayoutJob::default();
    job.append("Stream Info", 0.0, TextFormat::simple(semi(12.0), Color32::WHITE));
    for (name, value) in rows {
        job.append(&format!("
{name}"), 0.0, TextFormat::simple(reg(12.0), Color32::WHITE));
        job.append(&value, 0.0, TextFormat::simple(semi(12.0), Color32::WHITE));
    }
    let galley = ui.painter().layout_job(job);
    let r = Rect::from_min_size(tile.min + vec2(8.0, 8.0), galley.size() + vec2(16.0, 12.0));
    super::settings::mark(ui, "stream_info", r);
    ui.painter().rect_filled(r, CornerRadius::same(4), Color32::from_black_alpha(170));
    ui.painter().galley(r.min + vec2(8.0, 6.0), galley, Color32::WHITE);
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
}
