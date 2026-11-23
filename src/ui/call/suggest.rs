//! The tile Discord adds to the end of a server call's grid: three activities to start once two
//! or more people are in, or the way to invite someone while you're alone. Closing it hides it in
//! that channel until you look at another; Don't show again is a synced dismissed tip.

use egui::{Align2, Color32, CornerRadius, Mesh, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::state::{CREATE_INVITE, Channel, Id};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

const TIP: usize = 167;

fn closed_id() -> egui::Id {
    egui::Id::new("call_tile_closed")
}

fn dismissed(app: &App) -> bool {
    app.store.dismissed_bits.get(TIP / 8).is_some_and(|b| b & (1 << (TIP % 8)) != 0)
}

fn can_launch(app: &App, chan: &Channel) -> bool {
    app.store.embedded.get(&chan.id).is_none_or(|l| l.is_empty())
}

/// Whether the grid ends with the tile, given how many people are in the call.
pub(super) fn wanted(app: &App, ctx: &egui::Context, chan: &Channel, people: usize) -> bool {
    let Some(g) = app.store.guilds.get(&chan.guild) else { return false };
    let closed = ctx.data(|d| d.get_temp::<Id>(closed_id()));
    if closed.is_some_and(|c| c != chan.id) {
        ctx.data_mut(|d| d.remove::<Id>(closed_id()));
    }
    if g.afk == chan.id {
        return false;
    }
    if people >= 2 && can_launch(app, chan) && !dismissed(app) && closed != Some(chan.id) {
        return true;
    }
    people == 1 && chan.user_limit != 1 && app.store.perms(chan) & CREATE_INVITE != 0
}

pub(super) fn tile(app: &mut App, ui: &mut Ui, chan: &Channel, r: Rect, people: usize) {
    app.rects.insert("voice:suggest".into(), r);
    if r.width() < 250.0 {
        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_primary);
        let b = Rect::from_center_size(r.center(), vec2(40.0, 40.0));
        if ui.interact(r, egui::Id::new("call_tile_shelf"), Sense::click()).on_hover_text("Start An Activity").clicked() {
            crate::ui::activity::open_shelf(app);
        }
        icons::draw(ui.painter(), Icon::Apps, b, Color32::WHITE);
        return;
    }
    if people > 1 {
        suggestions(app, ui, chan, r);
    } else {
        invite(app, ui, chan, r);
    }
}

fn suggestions(app: &mut App, ui: &mut Ui, chan: &Channel, r: Rect) {
    crate::ui::activity::want_shelf(app, chan.guild);
    let picks: Vec<(Id, Option<String>, String)> = app
        .activity
        .entries
        .get(&chan.guild)
        .into_iter()
        .flatten()
        .take(3)
        .map(|e| {
            let info = app.activity.apps.get(&e.app);
            (e.app, info.and_then(|i| i.cover.clone().or(i.banner.clone())), info.map_or_else(String::new, |i| i.name.clone()))
        })
        .collect();
    let p = ui.painter().clone();
    p.rect_filled(r, CornerRadius::same(8), pal().bg_primary);
    let heading = r.width() > 300.0;
    let font = if r.width() > 550.0 { semi(16.0) } else { semi(14.0) };
    let line = if r.width() > 550.0 { 20.0 } else { 18.0 };
    let gap = if r.width() > 300.0 { 16.0 } else { 8.0 };
    let cell = ((r.width() - 32.0 - 2.0 * gap) / 3.0).min(250.0);
    let pic = (cell * 9.0 / 16.0).ceil();
    let tall = if heading { 16.0 + 2.0 * line } else { 0.0 } + 16.0 + pic + 16.0 + 24.0;
    let mut y = r.top() + (r.height() - tall) / 2.0;
    if heading {
        for (i, text) in ["Looking for something to do?", "Start a game or video playlist instantly."].into_iter().enumerate() {
            p.text(pos2(r.center().x, y + 16.0 + i as f32 * line + line / 2.0), Align2::CENTER_CENTER, text, font.clone(), pal().text_normal);
        }
        y += 16.0 + 2.0 * line;
    }
    y += 16.0;
    let x0 = r.center().x - (3.0 * cell + 2.0 * gap) / 2.0;
    for (i, (id, url, name)) in picks.into_iter().enumerate() {
        let c = Rect::from_min_size(pos2(x0 + i as f32 * (cell + gap), y), vec2(cell, pic));
        p.rect_filled(c, CornerRadius::same(12), pal().bg_secondary);
        if let Some(u) = url {
            egui::Image::from_uri(u).corner_radius(12).show_loading_spinner(false).paint_at(ui, c);
        }
        let hit = ui.interact(c, egui::Id::new(("call_tile_pick", id)), Sense::click());
        if hit.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if hit.on_hover_text(name).clicked() {
            crate::ui::activity::launch(app, id);
        }
    }
    y += pic + 16.0;
    let g = p.layout_no_wrap("Don't show again".into(), medium(14.0), pal().text_normal);
    let row = Rect::from_center_size(pos2(r.center().x, y + 12.0), vec2(32.0 + g.size().x, 24.0));
    let hit = ui.interact(row, egui::Id::new("call_tile_dismiss"), Sense::click());
    let box_ = Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(18.0, 18.0));
    p.rect_stroke(box_, CornerRadius::same(4), Stroke::new(1.5, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &hit))), egui::StrokeKind::Inside);
    p.galley(pos2(row.left() + 32.0, row.center().y - g.size().y / 2.0), g, pal().text_normal);
    if hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if hit.clicked() {
        let mut bits = app.store.dismissed_bits.clone();
        bits.resize(bits.len().max(TIP / 8 + 1), 0);
        bits[TIP / 8] |= 1 << (TIP % 8);
        app.store.dismissed_bits = bits.clone();
        app.proto_patch(crate::proto::dismissed_contents_patch(&bits), crate::net::Tag::Quiet);
    }
    let x = Rect::from_min_size(pos2(r.right() - 32.0, r.top() + 8.0), vec2(24.0, 24.0));
    let hit = ui.interact(x, egui::Id::new("call_tile_close"), Sense::click());
    icons::draw(&p, Icon::Close, x, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &hit)));
    app.rects.insert("voice:suggest_close".into(), x);
    if hit.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(closed_id(), chan.id));
    }
}

/// Discord's pink wash: a fade to pink toward the left along the top, under an ellipse of the
/// tile's own colour rising from the bottom middle.
fn wash(ui: &Ui, r: Rect) {
    let base = pal().bg_primary;
    let (start, end) = (Color32::from_rgb(179, 38, 156), Color32::from_rgb(102, 0, 102));
    let mix = |a: Color32, b: Color32, t: f32| crate::ui::motion::mix(a, b, t.clamp(0.0, 1.0));
    let (cols, rows) = (24u32, 12u32);
    let mut m = Mesh::default();
    for j in 0..=rows {
        for i in 0..=cols {
            let (u, v) = (i as f32 / cols as f32, j as f32 / rows as f32);
            let pink = mix(end, start, (1.0 - u) / 0.8);
            let tinted = mix(base, pink, 0.3);
            let d = (((u - 0.5) / 1.1079).powi(2) + (v - 1.0).powi(2)).sqrt();
            m.colored_vertex(pos2(r.left() + r.width() * u, r.top() + r.height() * v), mix(base, tinted, (d - 0.6) / 0.4));
        }
    }
    for j in 0..rows {
        for i in 0..cols {
            let a = j * (cols + 1) + i;
            let c = a + cols + 1;
            m.add_triangle(a, a + 1, c + 1);
            m.add_triangle(a, c + 1, c);
        }
    }
    let corners = [(r.left_top(), vec2(8.0, 8.0)), (r.right_top(), vec2(-8.0, 8.0)), (r.right_bottom(), vec2(-8.0, -8.0)), (r.left_bottom(), vec2(8.0, -8.0))];
    for (corner, inward) in corners {
        let center = corner + inward;
        let base = m.vertices.len() as u32;
        m.colored_vertex(corner, Color32::BLACK);
        for k in 0..=8 {
            let a = k as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            m.colored_vertex(center + vec2(-inward.x * a.cos(), -inward.y * a.sin()), Color32::BLACK);
            if k > 0 {
                m.add_triangle(base, base + k, base + k + 1);
            }
        }
    }
    ui.painter().with_clip_rect(r).add(m);
}

fn invite(app: &mut App, ui: &mut Ui, chan: &Channel, r: Rect) {
    wash(ui, r);
    let art_h = (r.height() - 112.0).clamp(0.0, 210.0);
    let top = r.top() + 16.0 + (r.height() - 112.0 - art_h).max(0.0) / 2.0;
    let art = Rect::from_center_size(pos2(r.center().x, top + art_h / 2.0), vec2(art_h * 388.0 / 204.0, art_h));
    crate::ui::webart::draw(ui.painter(), crate::ui::webart::CALL_INVITE, art);
    let cy = art.bottom() + 16.0 + 24.0 + 20.0;
    let shelf = can_launch(app, chan);
    let words = |t: &str| ui.painter().layout_no_wrap(t.into(), medium(16.0), Color32::WHITE).size().x + 56.0;
    let (a, b) = (words("Invite to Voice"), if shelf { words("Choose Activity") + 8.0 } else { 0.0 });
    let left = r.center().x - (a + b) / 2.0;
    if super::room::button(ui, "call_tile_invite", pos2(left + a / 2.0, cy), Some(Icon::PersonAdd), "Invite to Voice").clicked() {
        app.invite_people(chan.id, "");
    }
    if shelf && super::room::button(ui, "call_tile_choose", pos2(left + a + 8.0 + (b - 8.0) / 2.0, cy), Some(Icon::Apps), "Choose Activity").clicked() {
        crate::ui::activity::open_shelf(app);
    }
}
