use super::*;
use crate::state::{BLOCKED, FRIEND, INCOMING, OUTGOING};

pub(super) enum Start {
    Want(Id),
    Friend(Id),
    Unfriend(Id),
    Block(Id),
    Unblock(Id),
    Servers(Id),
}

/// Which buttons discord puts under the start of a DM, by what the person is to you: the label,
/// whether it is the filled one, and whether it can be pressed.
pub(super) fn buttons(rel: Option<u8>, bot: bool) -> Vec<(&'static str, bool, bool)> {
    match rel {
        _ if bot => Vec::new(),
        Some(BLOCKED) => vec![("Unblock", false, true)],
        Some(FRIEND) => vec![("Remove Friend", false, true), ("Block", false, true)],
        Some(INCOMING) => vec![("Accept Friend Request", true, true), ("Block", false, true)],
        Some(OUTGOING) => vec![("Friend Request Sent", true, false), ("Block", false, true)],
        _ => vec![("Add Friend", true, true), ("Block", false, true)],
    }
}

fn button(ui: &mut Ui, text: &str, filled: bool, on: bool) -> egui::Response {
    let ink = if filled { Color32::WHITE } else { pal().text_normal };
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), ink);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), if on { Sense::click() } else { Sense::hover() });
    let hot = on && resp.hovered();
    if filled {
        let fill = if hot { pal().blurple.gamma_multiply(0.85) } else { pal().blurple };
        ui.painter().rect_filled(r, 8, if on { fill } else { fill.gamma_multiply(0.5) });
    } else {
        let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if hot { 51 } else { 31 });
        ui.painter().rect(r, 8, fill, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
    }
    ui.painter().galley(pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, ink);
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The top of a DM: who it is with, the servers you share, and the friend and block buttons.
pub(super) fn show(ui: &mut Ui, d: &mut D, chan: &Channel, mutual: Option<&[Id]>) {
    let Some(u) = chan.recipients.first().and_then(|u| d.store.user(*u)) else { return };
    let title = channel_title(d.store, chan.id);
    if mutual.is_none() {
        d.acts.push(Act::Start(Start::Want(u.id)));
    }
    ui.add_space(16.0);
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
        let (r, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
        avatar(ui, r, &u.avatar_url());
        ui.add_space(6.0);
        ui.label(RichText::new(&title).font(bold(32.0)).color(pal().header_primary));
        ui.add_space(7.0);
        ui.label(RichText::new(&u.username).font(medium(24.0)).color(pal().header_primary));
        ui.add_space(19.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(RichText::new("This is the beginning of your direct message history with ").font(reg(16.0)).color(pal().text_normal));
            ui.label(RichText::new(&title).font(semi(16.0)).color(pal().text_normal));
            ui.label(RichText::new(".").font(reg(16.0)).color(pal().text_normal));
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.set_height(32.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            if let Some(list) = mutual {
                let shown: Vec<&crate::state::Guild> = list.iter().filter_map(|g| d.store.guilds.get(g)).take(3).collect();
                if !shown.is_empty() {
                    let (strip, _) = ui.allocate_exact_size(vec2(24.0 + 19.0 * (shown.len() - 1) as f32, 24.0), Sense::hover());
                    for (i, g) in shown.iter().enumerate().rev() {
                        let at = Rect::from_min_size(pos2(strip.left() + 19.0 * i as f32, strip.top()), vec2(24.0, 24.0));
                        ui.painter().rect_filled(at.expand(2.0), 8, pal().bg_primary);
                        match g.icon_url() {
                            Some(url) => egui::Image::from_uri(url).corner_radius(6).show_loading_spinner(false).paint_at(ui, at),
                            None => {
                                ui.painter().rect_filled(at, 6, pal().bg_accent);
                                ui.painter().text(at.center(), Align2::CENTER_CENTER, g.acronym().chars().take(2).collect::<String>(), semi(9.0), pal().text_normal);
                            }
                        }
                    }
                    ui.add_space(8.0);
                }
                let label = match list.len() {
                    0 => "No servers in common".to_string(),
                    1 => "1 Mutual Server".to_string(),
                    n => format!("{n} Mutual Servers"),
                };
                let resp = ui.add(egui::Label::new(RichText::new(label).font(reg(14.0)).color(pal().header_secondary)).sense(if list.is_empty() { Sense::hover() } else { Sense::click() }));
                if ui.clip_rect().contains_rect(resp.rect) {
                    crate::ui::settings::mark(ui, "dm_start:servers", resp.rect);
                }
                if !list.is_empty() && resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    d.acts.push(Act::Start(Start::Servers(u.id)));
                }
            }
            let set = buttons(d.store.relationships.get(&u.id).copied(), u.bot);
            if mutual.is_some() && !set.is_empty() {
                let (dot, _) = ui.allocate_exact_size(vec2(28.0, 32.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.0, pal().text_muted);
            }
            for (i, (text, filled, on)) in set.into_iter().enumerate() {
                if i > 0 {
                    ui.add_space(7.0);
                }
                let resp = button(ui, text, filled, on);
                if ui.clip_rect().contains_rect(resp.rect) {
                    crate::ui::settings::mark(ui, &format!("dm_start:{text}"), resp.rect);
                }
                if resp.clicked() {
                    d.acts.push(Act::Start(match text {
                        "Unblock" => Start::Unblock(u.id),
                        "Remove Friend" => Start::Unfriend(u.id),
                        "Block" => Start::Block(u.id),
                        _ => Start::Friend(u.id),
                    }));
                }
            }
        });
    });
}

pub(super) fn act(app: &mut App, what: Start) {
    let name = |app: &App, u: Id| app.store.name_in(0, u);
    match what {
        Start::Want(u) => {
            let key = format!("dm_start:{u}");
            if !app.keyed.contains_key(&key) {
                app.keyed.insert(key, None);
                app.rest("GET", format!("/users/{u}/profile?with_mutual_guilds=true&with_mutual_friends=true"), None, crate::net::Tag::Profile(u, 0));
            }
        }
        Start::Friend(u) => app.rest_in("PUT", format!("/users/@me/relationships/{u}"), Some(serde_json::json!({})), "DM Channel", crate::net::Tag::Friend),
        Start::Unfriend(u) => {
            let name = name(app, u);
            let body = format!("Are you sure you want to remove {name} from your friends?");
            app.confirm = Some(crate::ui::Confirm::new(format!("Remove '{name}'"), body, "Remove Friend", move |app: &mut App| {
                app.rest_in("DELETE", format!("/users/@me/relationships/{u}"), None, "DM Channel", crate::net::Tag::reason())
            }));
        }
        Start::Block(u) => {
            let body = "They won't be able to message you, and you won't see their messages unless you choose to.";
            app.confirm = Some(crate::ui::Confirm::new(format!("Block {}?", name(app, u)), body, "Block", move |app: &mut App| {
                app.rest_in("PUT", format!("/users/@me/relationships/{u}"), Some(serde_json::json!({ "type": 2 })), "DM Channel", crate::net::Tag::reason())
            }));
        }
        Start::Unblock(u) => app.rest_in("DELETE", format!("/users/@me/relationships/{u}"), None, "DM Channel", crate::net::Tag::reason()),
        Start::Servers(u) => app.open_full(u, 0, crate::ui::profile::Tab::Servers),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dm_start_offers_what_fits_the_relationship() {
        let labels = |rel, bot| buttons(rel, bot).into_iter().map(|b| b.0).collect::<Vec<_>>();
        assert_eq!(labels(None, false), ["Add Friend", "Block"]);
        assert_eq!(labels(Some(FRIEND), false), ["Remove Friend", "Block"]);
        assert_eq!(labels(Some(BLOCKED), false), ["Unblock"]);
        assert_eq!(labels(Some(INCOMING), false), ["Accept Friend Request", "Block"]);
        assert!(buttons(Some(OUTGOING), false)[0] == ("Friend Request Sent", true, false));
        assert!(labels(None, true).is_empty());
    }
}
