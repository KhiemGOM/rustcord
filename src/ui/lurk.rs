//! Previewing a server before joining: a link into a server you're not in lurks there, the
//! composer turns into a Join Server bar.

use egui::{Align2, CornerRadius, Margin, Sense, Ui, vec2};

use super::App;
use crate::net::Tag;
use crate::state::{Channel, Id};
use crate::theme::*;

pub fn start(app: &mut App, gid: Id, ch: Id) {
    app.lurk_goto = Some((gid, ch));
    app.rest("PUT", format!("/guilds/{gid}/members/@me?lurker=true"), Some(serde_json::json!({})), Tag::Lurk(gid));
}

pub fn tick
(app: &mut App) {
    let Some((gid, ch)) = app.lurk_goto else { return };
    let ready = if ch == 0 { app.store.guilds.contains_key(&gid) } else { app.store.channels.contains_key(&ch) };
    if ready {
        app.lurk_goto = None;
        app.select_guild(Some(gid));
        if ch != 0 {
            app.open(ch);
        }
    }
}


pub fn bar(app: &mut App, ui: &mut Ui, chan: &Channel) {
    ui.add_space(8.0);
    let mut join = false;
    egui::Frame::new().fill(pal().bg_input).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        let b = egui::Rect::from_center_size(egui::pos2(r.right() - 56.0, r.center().y), vec2(112.0, 32.0));
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "You are in preview mode. Join this server to talk.", reg(16.0), pal().text_normal);
        let resp = ui.interact(b, ui.id().with("lurk_join"), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(4), if resp.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Join Server", semi(14.0), egui::Color32::WHITE);
        app.rects.insert("lurk:join".into(), b);
        join = resp.clicked();
    });
    ui.add_space(24.0);
    if join {
        let gid = chan.guild;
        app.rest("PUT", format!("/guilds/{gid}/members/@me?lurker=false"), Some(serde_json::json!({})), Tag::Joined(gid));
    }
}
