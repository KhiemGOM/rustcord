use egui::{Align2, CornerRadius, Frame, Rect, Sense, Stroke, pos2, vec2};

use super::App;
use crate::net::Tag;
use crate::state::{CONNECT, Id};
use crate::theme::*;

pub fn hover(ui: &egui::Ui, gid: Id, ch: Id, user: Id, row: Rect) {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("stream_tip"), (gid, ch, user, row, pass)));
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((gid, ch, user, row, pass)) = ctx.data(|d| d.get_temp::<(Id, Id, Id, Rect, u64)>(egui::Id::new("stream_tip"))) else { return };
    if pass != ctx.cumulative_pass_nr() {
        return;
    }
    let url = preview_url(app, ctx, gid, ch, user);
    let can = app.voice.chan == Some(ch) || app.store.channels.get(&ch).is_some_and(|c| app.store.perms(c) & CONNECT != 0);
    let game = app.store.activities.get(&user).and_then(|l| l.iter().find(|a| a.kind == 0).cloned());
    let title = game.as_ref().map_or("Streaming".to_string(), |a| format!("Streaming {}", a.name));
    let icon = game.as_ref().and_then(|a| app.store.app_icons.get(&a.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=32", crate::api::cdn(), a.app)));
    let frame = Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle));
    egui::Area::new(egui::Id::new("stream_tip_area")).order(egui::Order::Tooltip).fixed_pos(pos2(row.right() + 8.0, row.top() - 4.0)).interactable(false).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_width(252.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let (head, _) = ui.allocate_exact_size(vec2(252.0, 34.0), Sense::hover());
            let mut x = head.left() + 10.0;
            let y = head.top() + 18.0;
            if let Some(i) = &icon {
                super::text::image(ui.painter(), i, Rect::from_center_size(pos2(x + 8.0, y), vec2(16.0, 16.0)));
                x += 22.0;
            }
            super::text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, &title, semi(12.0), pal().header_primary, head.right() - 10.0 - x);
            let (r, _) = ui.allocate_exact_size(vec2(252.0, 142.0), Sense::hover());
            let cr = CornerRadius { sw: 7, se: 7, ..Default::default() };
            ui.painter().rect_filled(r, cr, pal().bg_secondary_alt);
            match &url {
                Some(u) => egui::Image::from_uri(u.clone()).corner_radius(cr).show_loading_spinner(false).paint_at(ui, r),
                None if can => egui::Spinner::new().size(24.0).paint_at(ui, Rect::from_center_size(r.center(), vec2(24.0, 24.0))),
                None => {}
            }
            super::settings::mark(ui, "stream_tip", r);
        });
    });
}

/// The picture Discord keeps of someone's stream, asked for again every two minutes and retried
/// a few times while there's none yet.
pub fn preview_url(app: &mut App, ctx: &egui::Context, gid: Id, ch: Id, user: Id) -> Option<String> {
    let key = if gid != 0 { format!("guild:{gid}:{ch}:{user}") } else { format!("call:{ch}:{user}") };
    let slot = format!("stream_preview:{key}");
    let seen = egui::Id::new(("stream_preview_url", &key));
    let asked = egui::Id::new(("stream_preview_at", &key));
    let answered = matches!(app.keyed.get(&slot), Some(Some(_)));
    if answered && let Some(Some(res)) = app.keyed.remove(&slot) {
        let now = ctx.input(|i| i.time);
        let tries = ctx.data(|d| d.get_temp::<(f64, u32)>(asked)).map_or(0, |t| t.1);
        match res.ok().and_then(|v| v["url"].as_str().map(str::to_string)) {
            Some(u) => ctx.data_mut(|d| {
                d.insert_temp(seen, u);
                d.insert_temp(asked, (now + 120.0, 0));
            }),
            None => ctx.data_mut(|d| {
                d.insert_temp(asked, (now + 10.0 * (tries + 1) as f64, tries + 1));
            }),
        }
    }
    let can = app.voice.chan == Some(ch) || app.store.channels.get(&ch).is_some_and(|c| app.store.perms(c) & CONNECT != 0);
    let (due, tries) = ctx.data(|d| d.get_temp::<(f64, u32)>(asked)).unwrap_or((f64::MIN, 0));
    if can && !app.keyed.contains_key(&slot) && tries < 5 && ctx.input(|i| i.time) >= due {
        app.keyed.insert(slot.clone(), None);
        let ms = chrono::Utc::now().timestamp_millis();
        app.rest("GET", format!("/streams/{key}/preview?version={ms}"), None, Tag::Keyed(slot));
    }
    ctx.data(|d| d.get_temp(seen))
}
