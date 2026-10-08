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
                        ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
                    }
                    // discord fades the whole offline row to .3, except your own
                    let fade = if offline && !hovered && *uid != store.me { 0.3 } else { 1.0 };
                    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
                    if let Some(u) = store.user(*uid) {
                        avatar(ui, av, &u.avatar_url());
                        if let Some(d) = u.decoration_url() {
                            super::text::image(ui.painter(), &d, Rect::from_center_size(av.center(), av.size() * 1.2));
                        }
                        if fade < 1.0 {
                            ui.painter().circle_filled(av.center(), 19.5, pal().bg_primary.gamma_multiply(0.7));
                        }
                    }
                    let st = store.status(*uid);
                    if st != Status::Offline {
                        status_dot(ui, av, st, if hovered { pal().bg_mod_hover } else { pal().bg_primary });
                    }
                    let color = store.role_color(gid, *uid).map(role_color).unwrap_or(pal().header_secondary);
                    let name = store.name_in(gid, *uid);
                    let bot = store.user(*uid).is_some_and(|u| u.bot);
                    let tw = inner.right() - 8.0 - av.right() - 12.0 - if bot { 48.0 } else { 0.0 };
                    // with an activity the name rides up and the line sits under it
                    let line = if offline { None } else { store.activity_line(*uid) };
                    let ny = if line.is_some() { row.center().y - 8.0 } else { row.center().y };
                    let nr = super::text::paint_em(ui.painter(), pos2(av.right() + 12.0, ny), Align2::LEFT_CENTER, &name, medium(16.0), color.gamma_multiply(fade), tw, 1.0);
                    if let Some((emoji, plain, bold)) = line {
                        let mut x = av.right() + 12.0;
                        let y = row.center().y + 10.0;
                        if let Some(url) = emoji {
                            super::text::image(ui.painter(), &url, Rect::from_center_size(pos2(x + 7.0, y), vec2(14.0, 14.0)));
                            x += 18.0;
                        }
                        let mut job = egui::text::LayoutJob::default();
                        job.wrap.max_width = inner.right() - 8.0 - x;
                        job.wrap.max_rows = 1;
                        job.wrap.break_anywhere = true;
                        job.append(&plain, 0.0, egui::TextFormat { font_id: medium(12.0), color: pal().text_muted, ..Default::default() });
                        job.append(&bold, 0.0, egui::TextFormat { font_id: semi(12.0), color: pal().text_muted, ..Default::default() });
                        let g = ui.painter().layout_job(job);
                        ui.painter().galley(pos2(x, y - g.size().y / 2.0), g, pal().text_muted);
                    }
                    let mut tx = nr.right() + 4.0;
                    // discord leaves the crown off while the owner sits in the offline group
                    if !offline && store.guilds.get(&gid).is_some_and(|g| g.owner == *uid) {
                        let cr = Rect::from_min_size(pos2(tx, ny - 7.0), vec2(14.0, 14.0));
                        super::icons::draw(ui.painter(), super::icons::Icon::Crown, cr, egui::Color32::from_rgb(0xf0, 0xb2, 0x32).gamma_multiply(fade));
                        ui.interact(cr, ui.id().with(("owner", *uid)), Sense::hover()).on_hover_text("Server Owner");
                        tx = cr.right() + 4.0;
                    }
                    if let Some(u) = store.user(*uid) {
                        if bot {
                            super::app_tag(ui.painter(), pos2(tx, ny), u.flags & (1 << 16) != 0, fade);
                        } else {
                            super::tag_chip(ui.painter(), u, pos2(tx, ny), fade);
                        }
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
