use egui::{Align2, Color32, CornerRadius, Image, Rect, Sense, Ui, pos2, vec2};

use super::{App, badge};
use crate::theme::*;

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(12.0);

            let dm_mentions: u32 = app.store.dms.iter().map(|c| app.store.mentions(*c)).sum();
            let home = app.ui.guild.is_none();
            let r = item(ui, egui::Id::new("home"), home, false, |ui, rect, radius, hot| {
                let fill = if hot { BLURPLE } else { BG_PRIMARY };
                ui.painter().rect_filled(rect, CornerRadius::same(radius), fill);
                clyde(ui, rect.center(), if hot { Color32::WHITE } else { INTERACTIVE_HOVER });
            });
            if dm_mentions > 0 {
                badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), dm_mentions, Some(BG_TERTIARY));
            }
            app.rects.insert("guild:home".into(), r.rect);
            let r = r.on_hover_text("Direct Messages");
            if r.clicked() {
                app.select_guild(None);
            }

            let (sep, _) = ui.allocate_exact_size(vec2(72.0, 2.0), Sense::hover());
            ui.painter().rect_filled(
                Rect::from_center_size(sep.center(), vec2(32.0, 2.0)),
                CornerRadius::same(1),
                BG_MOD_HOVER,
            );

            let order = app.store.guild_order.clone();
            for gid in order {
                let Some(g) = app.store.guilds.get(&gid) else { continue };
                let (unread, mentions) = app.store.guild_badge(gid);
                let selected = app.ui.guild == Some(gid);
                let icon = g.icon_url();
                let acr = g.acronym();
                let name = g.name.clone();
                let r = item(ui, egui::Id::new(("guild", gid)), selected, unread, |ui, rect, radius, hot| match &icon {
                    Some(url) => {
                        ui.painter().rect_filled(rect, CornerRadius::same(radius), BG_PRIMARY);
                        Image::from_uri(url.clone())
                            .corner_radius(CornerRadius::same(radius))
                            .show_loading_spinner(false)
                            .paint_at(ui, rect);
                    }
                    None => {
                        let fill = if hot { BLURPLE } else { BG_PRIMARY };
                        ui.painter().rect_filled(rect, CornerRadius::same(radius), fill);
                        let size = match acr.chars().count() {
                            0..=2 => 18.0,
                            3 => 16.0,
                            _ => 12.0,
                        };
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            &acr,
                            semi(size),
                            if hot { Color32::WHITE } else { TEXT_NORMAL },
                        );
                    }
                });
                if mentions > 0 {
                    badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(BG_TERTIARY));
                }
                app.rects.insert(format!("guild:{gid}"), r.rect);
                if r.on_hover_text(name).clicked() {
                    app.select_guild(Some(gid));
                }
            }
            ui.add_space(12.0);
        });
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
