use egui::{Align2, Color32, CornerRadius, Image, Rect, Sense, Ui, pos2, vec2};

use super::{App, badge};
use crate::state::{Folder, Id};
use crate::theme::*;

const FOLDER_DEFAULT: u32 = 0x5865f2;

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(12.0);

            let dm_mentions: u32 = app.store.dms.iter().map(|c| app.store.mentions(*c)).sum();
            let incoming = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            let home = app.ui.guild.is_none();
            let r = item(ui, egui::Id::new("home"), home, false, |ui, rect, radius, hot| {
                let fill = if hot { BLURPLE } else { BG_PRIMARY };
                ui.painter().rect_filled(rect, CornerRadius::same(radius), fill);
                clyde(ui, rect.center(), if hot { Color32::WHITE } else { INTERACTIVE_HOVER });
            });
            if dm_mentions + incoming > 0 {
                badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), dm_mentions + incoming, Some(BG_TERTIARY));
            }
            app.rects.insert("guild:home".into(), r.rect);
            let r = r.on_hover_text("Direct Messages");
            if r.clicked() {
                app.select_guild(None);
            }

            separator(ui);

            // caches written before folders existed only have the flat order
            let folders: Vec<Folder> = if app.store.folders.is_empty() {
                app.store.guild_order.iter().map(|g| Folder { guilds: vec![*g], ..Default::default() }).collect()
            } else {
                app.store.folders.clone()
            };
            for f in &folders {
                if f.id == 0 && f.guilds.len() == 1 {
                    guild(app, ui, f.guilds[0]);
                } else {
                    folder(app, ui, f);
                }
            }

            separator(ui);
            let r = item(ui, egui::Id::new("add_server"), false, false, |ui, rect, radius, hot| {
                ui.painter().rect_filled(rect, CornerRadius::same(radius), if hot { GREEN } else { BG_PRIMARY });
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", reg(28.0), if hot { Color32::WHITE } else { GREEN });
            });
            if r.on_hover_text("Join a Server").clicked() {
                app.invite = Some(String::new());
            }
            ui.add_space(12.0);
        });
}

fn separator(ui: &mut Ui) {
    let (sep, _) = ui.allocate_exact_size(vec2(72.0, 2.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(sep.center(), vec2(32.0, 2.0)), CornerRadius::same(1), BG_MOD_HOVER);
}

fn guild_icon(app: &App, ui: &Ui, gid: Id, rect: Rect, radius: u8, hot: bool) {
    let Some(g) = app.store.guilds.get(&gid) else { return };
    match g.icon_url() {
        Some(url) => {
            ui.painter().rect_filled(rect, CornerRadius::same(radius), BG_PRIMARY);
            Image::from_uri(url).corner_radius(CornerRadius::same(radius)).show_loading_spinner(false).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, CornerRadius::same(radius), if hot { BLURPLE } else { BG_PRIMARY });
            let acr = g.acronym();
            let size = match acr.chars().count() {
                0..=2 => 18.0,
                3 => 16.0,
                _ => 12.0,
            } * rect.width()
                / 48.0;
            let col = if hot { Color32::WHITE } else { TEXT_NORMAL };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, acr, semi(size), col);
        }
    }
}

fn guild(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(name) = app.store.guilds.get(&gid).map(|g| g.name.clone()) else { return };
    let (unread, mentions) = app.store.guild_badge(gid);
    let selected = app.ui.guild == Some(gid);
    let r = item(ui, egui::Id::new(("guild", gid)), selected, unread, |ui, rect, radius, hot| {
        guild_icon(app, ui, gid, rect, radius, hot)
    });
    if mentions > 0 {
        badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(BG_TERTIARY));
    }
    app.rects.insert(format!("guild:{gid}"), r.rect);
    let r = r.on_hover_text(name);
    if r.clicked() {
        app.select_guild(Some(gid));
    }
    let muted = app.store.muted.contains(&gid);
    r.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui.button("Mark As Read").clicked() {
            let chans = app.store.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default();
            app.mark_read(&chans);
            ui.close();
        }
        if ui.button(if muted { "Unmute Server" } else { "Mute Server" }).clicked() {
            app.set_muted(gid, true, !muted);
            ui.close();
        }
        if app.settings.dev && ui.button("Copy Server ID").clicked() {
            ui.ctx().copy_text(gid.to_string());
            ui.close();
        }
    });
}

fn folder(app: &mut App, ui: &mut Ui, f: &Folder) {
    let open = app.ui.open_folders.contains(&f.id);
    let color = role_color(f.color.unwrap_or(FOLDER_DEFAULT));
    let mut unread = false;
    let mut mentions = 0;
    for g in &f.guilds {
        let (u, m) = app.store.guild_badge(*g);
        unread |= u;
        mentions += m;
    }
    let selected_inside = f.guilds.iter().any(|g| app.ui.guild == Some(*g));
    let top = ui.cursor().top();
    let bg = ui.painter().add(egui::Shape::Noop);

    let r = item(ui, egui::Id::new(("folder", f.id)), selected_inside && !open, unread && !open, |ui, rect, radius, _hot| {
        if open {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "📁", reg(22.0), color);
            return;
        }
        ui.painter().rect_filled(rect, CornerRadius::same(radius), color.gamma_multiply(0.4));
        for (i, g) in f.guilds.iter().take(4).enumerate() {
            let cell = Rect::from_min_size(
                rect.min + vec2(6.0 + (i % 2) as f32 * 19.0, 6.0 + (i / 2) as f32 * 19.0),
                vec2(17.0, 17.0),
            );
            guild_icon(app, ui, *g, cell, 6, false);
        }
    });
    if mentions > 0 && !open {
        badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(BG_TERTIARY));
    }
    let name = f.name.clone().unwrap_or_else(|| {
        f.guilds.iter().filter_map(|g| app.store.guilds.get(g)).map(|g| g.name.clone()).collect::<Vec<_>>().join(", ")
    });
    if r.on_hover_text(name).clicked() && !app.ui.open_folders.remove(&f.id) {
        app.ui.open_folders.insert(f.id);
    }
    if open {
        for g in &f.guilds {
            guild(app, ui, *g);
        }
        let back = Rect::from_min_max(pos2(12.0, top), pos2(60.0, ui.cursor().top() - 8.0));
        let back = back.translate(vec2(ui.max_rect().left(), 0.0));
        ui.painter().set(
            bg,
            egui::epaint::RectShape::filled(back, CornerRadius::same(16), color.gamma_multiply(0.15)),
        );
    }
}

fn item(
    ui: &mut Ui,
    id: egui::Id,
    selected: bool,
    unread: bool,
    paint: impl FnOnce(&Ui, Rect, u8, bool),
) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(72.0, 48.0), Sense::click());
    let rect = Rect::from_center_size(row.center(), vec2(48.0, 48.0));
    let hovered = resp.hovered();
    let hot = hovered || selected;
    let t = ui.ctx().animate_bool_with_time(id.with("r"), hot, 0.15);
    let radius = (24.0 - 8.0 * t).round() as u8;
    let pill_target = if selected {
        40.0
    } else if hovered {
        20.0
    } else if unread {
        8.0
    } else {
        0.0
    };
    let pill = ui.ctx().animate_value_with_time(id.with("p"), pill_target, 0.15);
    if pill > 0.5 {
        let p = Rect::from_min_size(pos2(row.left(), row.center().y - pill / 2.0), vec2(4.0, pill));
        ui.painter().rect_filled(
            p,
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            HEADER_PRIMARY,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint(ui, rect, radius, hot);
    resp
}

// a little discord-ish face so the home button reads right at a glance
fn clyde(ui: &Ui, c: egui::Pos2, col: Color32) {
    let p = ui.painter();
    let body = Rect::from_center_size(c + vec2(0.0, 1.0), vec2(26.0, 18.0));
    p.rect_filled(body, CornerRadius { nw: 9, ne: 9, sw: 6, se: 6 }, col);
    let eye = if col == Color32::WHITE { BLURPLE } else { BG_PRIMARY };
    p.circle_filled(c + vec2(-5.5, 1.0), 2.6, eye);
    p.circle_filled(c + vec2(5.5, 1.0), 2.6, eye);
}
