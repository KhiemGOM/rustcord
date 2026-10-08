use egui::{Align2, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar, status_dot};
use crate::state::{Id, ListItem, Status};
use crate::theme::*;

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    let mut clicked = None;
    let store = &app.store;
    let mut rects = Vec::new();
    let Some(g) = store.guilds.get(&gid) else { return };

    // until the gateway sends a member list, fall back to people we've seen talk
    let fallback: Vec<ListItem>;
    let items: &[ListItem] = if g.list.is_empty() {
        let mut seen: Vec<Id> = store.members.get(&gid).map(|m| m.keys().copied().collect()).unwrap_or_default();
        seen.sort_by_key(|u| store.name_in(gid, *u).to_lowercase());
        let (on, off): (Vec<Id>, Vec<Id>) = seen.into_iter().partition(|u| store.status(*u) != Status::Offline);
        let mut v = Vec::new();
        if !on.is_empty() {
            v.push(ListItem::Group { id: "online".into(), count: on.len() as u32 });
            v.extend(on.into_iter().map(ListItem::Member));
        }
        if !off.is_empty() {
            v.push(ListItem::Group { id: "offline".into(), count: off.len() as u32 });
            v.extend(off.into_iter().map(ListItem::Member));
        }
        fallback = v;
        &fallback
    } else {
        &g.list
    };

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let mut offline = false;
        let mut first = true;
        for it in items {
            match it {
                ListItem::Group { id, count } => {
                    offline = id == "offline";
                    let name = match id.as_str() {
                        "online" => "Online".to_string(),
                        "offline" => "Offline".to_string(),
                        r => r.parse::<Id>().ok().and_then(|r| g.roles.get(&r)).map(|r| r.name.clone()).unwrap_or_default(),
                    };
                    ui.add_space(if first { 16.0 } else { 20.0 });
                    first = false;
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
                    ui.painter().text(
                        pos2(r.left() + 16.0, r.center().y),
                        Align2::LEFT_CENTER,
                        format!("{} — {}", name, count),
                        medium(14.0),
                        pal().channel_default,
                    );
                }
                ListItem::Member(uid) => {
                    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
                    rects.push((*uid, row));
                    if resp.clicked() {
                        clicked = Some((*uid, row.left_top() - vec2(310.0, 0.0)));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let inner = row.shrink2(vec2(8.0, 0.0));
                    let hovered = resp.hovered();
                    if hovered {
                        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
                    }
                    let fade = if offline && !hovered { 0.3 } else { 1.0 };
                    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
                    if let Some(u) = store.user(*uid) {
                        avatar(ui, av, &u.avatar_url());
                        if fade < 1.0 {
                            ui.painter().circle_filled(av.center(), 16.5, pal().bg_primary.gamma_multiply(0.7));
                        }
                    }
                    let st = store.status(*uid);
                    if st != Status::Offline {
                        status_dot(ui, av, st, if hovered { pal().bg_mod_hover } else { pal().bg_primary });
                    }
                    let color = store.role_color(gid, *uid).map(role_color).unwrap_or(pal().header_secondary);
                    let name = store.name_in(gid, *uid);
                    let bot = store.user(*uid).is_some_and(|u| u.bot);
                    let tw = inner.right() - 8.0 - av.right() - 12.0 - if bot { 36.0 } else { 0.0 };
                    let nr = super::text::paint_em(ui.painter(), pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, &name, reg(16.0), color.gamma_multiply(fade), tw, 1.0);
                    if bot {
                        let tag = Rect::from_min_size(pos2(nr.right() + 4.0, row.center().y - 7.5), vec2(30.0, 15.0));
                        ui.painter().rect_filled(tag, CornerRadius::same(3), BLURPLE.gamma_multiply(fade.max(0.5)));
                        ui.painter().text(tag.center(), Align2::CENTER_CENTER, "APP", semi(10.0), egui::Color32::WHITE);
                    }
                }
            }
        }
        ui.add_space(16.0);
    });
    for (u, r) in rects {
        app.rects.insert(format!("member:{u}"), r);
    }
    if let Some((u, at)) = clicked {
        app.open_profile(u, gid, at);
    }
}
