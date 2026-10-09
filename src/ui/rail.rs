use egui::{Align2, Color32, CornerRadius, Image, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, badge, text};
use crate::state::{Folder, Id};
use crate::theme::*;

const ICON: f32 = 40.0;
const RADIUS: u8 = 12;

pub fn show(app: &mut App, ui: &mut Ui) {
    // the account card floats over the bottom of the rail
    egui::Panel::bottom("rail_foot").exact_size(super::side::foot(app)).resizable(false).show_separator_line(false).frame(egui::Frame::NONE).show(ui, |_| {});
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(8.0);

            let incoming = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            let home = app.ui.guild.is_none();
            let r = item(ui, egui::Id::new("home"), home, false, |ui, rect, radius, hot| {
                let fill = if hot { BLURPLE } else { pal().bg_primary };
                ui.painter().rect_filled(rect, CornerRadius::same(radius), fill);
                icons::draw(ui.painter(), Icon::Discord, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)), if hot { Color32::WHITE } else { pal().interactive_hover });
            });
            if incoming > 0 {
                badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), incoming, Some(pal().bg_tertiary));
            }
            app.rects.insert("guild:home".into(), r.rect);
            let r = r.on_hover_text("Direct Messages");
            if r.clicked() {
                app.select_guild(None);
            }

            // dms with something waiting get their own bubble under home, like the official client
            let waiting: Vec<(Id, u32)> = app
                .store
                .dms
                .iter()
                .map(|c| (*c, app.store.mentions(*c)))
                .filter(|(_, n)| *n > 0)
                .take(8)
                .collect();
            for (c, n) in waiting {
                let Some(ch) = app.store.channels.get(&c) else { continue };
                let url = match ch.kind {
                    1 => ch.recipients.first().and_then(|u| app.store.user(*u)).map(|u| u.avatar_url()),
                    _ => ch.icon.as_ref().map(|i| format!("{}/channel-icons/{c}/{i}.png?size=96", crate::api::cdn())),
                };
                let title = super::channel_title(&app.store, c);
                let r = item(ui, egui::Id::new(("dm", c)), false, false, |ui, rect, radius, _| {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius), BLURPLE);
                    if let Some(u) = &url {
                        Image::from_uri(u.clone()).corner_radius(CornerRadius::same(radius)).show_loading_spinner(false).paint_at(ui, rect);
                    } else {
                        icons::draw(ui.painter(), Icon::People, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), Color32::WHITE);
                    }
                });
                badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), n, Some(pal().bg_tertiary));
                app.rects.insert(format!("dmbubble:{c}"), r.rect);
                if tip(r, &title).clicked() {
                    app.ui.guild = None;
                    app.open(c);
                }
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
                ui.painter().rect_filled(rect, CornerRadius::same(radius), if hot { GREEN } else { pal().bg_primary });
                icons::draw(ui.painter(), Icon::CirclePlus, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), if hot { Color32::WHITE } else { pal().interactive_hover });
            });
            if r.on_hover_text("Add a Server").clicked() {
                app.invite = Some(String::new());
            }
            // discord's last two rail buttons; fastcord leaves both pages to the browser
            for (key, icon, tip, url) in [
                ("discover", Icon::Discover, "Discover", "https://discord.com/discovery"),
                ("download", Icon::Download, "Download Apps", "https://discord.com/download"),
            ] {
                let r = item(ui, egui::Id::new(key), false, false, |ui, rect, radius, hot| {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius), if hot { GREEN } else { pal().bg_primary });
                    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), if hot { Color32::WHITE } else { pal().interactive_hover });
                });
                if r.on_hover_text(tip).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
            ui.add_space(12.0);
        });
}

fn separator(ui: &mut Ui) {
    let (sep, _) = ui.allocate_exact_size(vec2(72.0, 2.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(sep.center(), vec2(32.0, 1.0)), CornerRadius::same(1), pal().frame_border);
}

fn guild_icon(app: &App, ui: &Ui, gid: Id, rect: Rect, radius: impl Into<CornerRadius>, hot: bool) {
    let Some(g) = app.store.guilds.get(&gid) else { return };
    let radius = radius.into();
    match g.icon_url() {
        Some(url) => {
            crate::gif::paint_still(ui, &url, rect, radius, hot);
        }
        None => {
            ui.painter().rect_filled(rect, radius, if hot { BLURPLE } else { pal().bg_mod_selected });
            let acr = g.acronym();
            let size = match acr.chars().count() {
                0..=2 => 17.0,
                3 => 15.0,
                4..=5 => 13.0,
                _ => 11.0,
            } * rect.width()
                / ICON;
            let col = if hot { Color32::WHITE } else { pal().text_normal };
            text::paint(ui.painter(), rect.center(), Align2::CENTER_CENTER, &acr, semi(size), col, rect.width() - 4.0);
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
        badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(pal().bg_tertiary));
    }
    if let Some(mine) = voice_badge(app, gid) {
        let at = Rect::from_center_size(r.rect.center(), vec2(ICON, ICON)).right_top() + vec2(-7.0, 7.0);
        ui.painter().circle_filled(at, 11.0, pal().bg_tertiary);
        ui.painter().circle_filled(at, 9.0, if mine { GREEN } else { Color32::BLACK });
        icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(at, vec2(12.0, 12.0)), Color32::WHITE);
        app.rects.insert(format!("voicebadge:{gid}"), Rect::from_center_size(at, vec2(18.0, 18.0)));
    }
    app.rects.insert(format!("guild:{gid}"), r.rect);
    app.rects.insert(format!("guildicon:{gid}"), Rect::from_center_size(r.rect.center(), vec2(ICON, ICON)));
    let r = tip(r, &name);
    if r.clicked() {
        app.select_guild(Some(gid));
    }
    guild_menu(app, &r, gid, unread);
}

enum Act {
    Read,
    Invite(Id),
    Mute(bool, Option<i64>),
    Notifs,
    HideMuted,
    Web,
    Leave,
    CopyId,
}

/// Discord's right-click menu on a server icon.
fn guild_menu(app: &mut App, r: &egui::Response, gid: Id, unread: bool) {
    use super::{Trail, menu_item, menu_row, menu_sep};
    let store = &app.store;
    let muted = store.muted.contains(&gid);
    let hide = store.hide_muted.contains(&gid);
    let owner = store.guilds.get(&gid).is_some_and(|g| g.owner == store.me);
    // invites need a channel; discord picks the first one we may invite to
    let invite_to = super::side::first_channel(store, gid)
        .and_then(|c| store.channels.get(&c))
        .filter(|c| store.perms(c) & crate::state::CREATE_INVITE != 0)
        .map(|c| c.id);
    let dev = app.settings.dev;
    let mut act = None;
    r.context_menu(|ui| super::menu_body(ui, |ui| {
        if ui.add_enabled_ui(unread, |ui| menu_item(ui, None, "Mark As Read", false)).inner {
            act = Some(Act::Read);
        }
        if let Some(c) = invite_to {
            menu_sep(ui);
            if menu_item(ui, None, "Invite to Server", false) {
                act = Some(Act::Invite(c));
            }
        }
        menu_sep(ui);
        if muted {
            if menu_item(ui, None, "Unmute Server", false) {
                act = Some(Act::Mute(false, None));
            }
        } else {
            let m = menu_row(ui, None, "Mute Server", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                if let Some(secs) = super::mute_menu(ui) {
                    act = Some(Act::Mute(true, secs));
                }
            });
        }
        if menu_item(ui, None, "Notification Settings", false) {
            act = Some(Act::Notifs);
        }
        if menu_row(ui, None, "Hide Muted Channels", None, false, Trail::Check(hide)).clicked() {
            act = Some(Act::HideMuted);
        }
        menu_sep(ui);
        for label in ["Privacy Settings", "Edit Per-server Profile"] {
            if menu_item(ui, None, label, false) {
                act = Some(Act::Web);
            }
        }
        if !owner {
            menu_sep(ui);
            if menu_item(ui, None, "Leave Server", true) {
                act = Some(Act::Leave);
            }
        }
        if dev {
            menu_sep(ui);
            if menu_item(ui, Some(Icon::Id), "Copy Server ID", false) {
                act = Some(Act::CopyId);
            }
        }
    }));
    match act {
        Some(Act::Read) => {
            let chans = app.store.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default();
            app.mark_read(&chans);
        }
        Some(Act::Invite(c)) => {
            app.made_invite = Some((c, None));
            app.rest("POST", format!("/channels/{c}/invites"), Some(serde_json::json!({ "max_age": 604800, "max_uses": 0, "temporary": false })), crate::net::Tag::MadeInvite);
        }
        Some(Act::Mute(m, secs)) => app.mute_for(gid, true, m, secs),
        Some(Act::Notifs) => app.guild_notifs = Some(gid),
        Some(Act::HideMuted) => super::side::toggle_hide_muted(app, gid),
        Some(Act::Web) => r.ctx.open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{gid}"))),
        Some(Act::Leave) => super::side::leave(app, gid),
        Some(Act::CopyId) => r.ctx.copy_text(gid.to_string()),
        None => {}
    }
}

/// Some(true) when we're connected to voice in this server, Some(false) when someone else is.
fn voice_badge(app: &App, gid: Id) -> Option<bool> {
    let mine = app.voice.chan.and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.guild == gid);
    let busy = app.store.guilds.get(&gid).is_some_and(|g| g.voice.values().any(|c| *c != 0));
    (mine || busy).then_some(mine)
}

fn folder(app: &mut App, ui: &mut Ui, f: &Folder) {
    let open = app.ui.open_folders.contains(&f.id);
    let color = f.color.map(role_color);
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

    let r = item(ui, egui::Id::new(("folder", f.id)), selected_inside && !open, unread && !open, |ui, rect, radius, hot| {
        if open {
            if hot {
                ui.painter().rect_filled(rect, CornerRadius::same(radius), subtle());
            }
            icons::draw(ui.painter(), Icon::Folder, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), color.unwrap_or(BLURPLE));
            return;
        }
        let tint = color.map_or(subtle(), |c| c.gamma_multiply(0.4));
        ui.painter().rect_filled(rect, CornerRadius::same(radius), tint);
        for (i, g) in f.guilds.iter().take(4).enumerate() {
            let cell = Rect::from_min_size(
                rect.min + vec2(3.0 + (i % 2) as f32 * 18.0, 3.0 + (i / 2) as f32 * 18.0),
                vec2(16.0, 16.0),
            );
            // the corner on the folder's outside edge is the round one
            let mut cr = CornerRadius::same(4);
            match i {
                0 => cr.nw = 10,
                1 => cr.ne = 10,
                2 => cr.sw = 10,
                _ => cr.se = 10,
            }
            guild_icon(app, ui, *g, cell, cr, false);
        }
    });
    if mentions > 0 && !open {
        badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(pal().bg_tertiary));
    }
    app.rects.insert(format!("folder:{}", f.id), r.rect);
    let name = f.name.clone().unwrap_or_else(|| {
        f.guilds.iter().filter_map(|g| app.store.guilds.get(g)).map(|g| g.name.clone()).collect::<Vec<_>>().join(", ")
    });
    let r = tip(r, &name);
    if r.clicked() && !app.ui.open_folders.remove(&f.id) {
        app.ui.open_folders.insert(f.id);
    }
    let mut read = false;
    r.context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        read = ui.add_enabled_ui(unread, |ui| super::menu_item(ui, None, "Mark Folder As Read", false)).inner;
    });
    if read {
        let chans: Vec<Id> = f.guilds.iter().filter_map(|g| app.store.guilds.get(g)).flat_map(|g| g.channels.iter().copied()).collect();
        app.mark_read(&chans);
    }
    if open {
        for g in &f.guilds {
            guild(app, ui, *g);
        }
        let back = Rect::from_min_max(pos2(12.0, top - 4.0), pos2(60.0, ui.cursor().top() - 4.0));
        let back = back.translate(vec2(ui.max_rect().left(), 0.0));
        let fill = color.map_or(subtle(), |c| c.gamma_multiply(0.15));
        app.rects.insert(format!("folderbg:{}", f.id), back);
        ui.painter().set(
            bg,
            egui::epaint::RectShape::new(back, CornerRadius::same(16), fill, egui::Stroke::new(1.0, pal().bg_mod_hover), egui::StrokeKind::Inside),
        );
    }
}

// discord's background-mod-subtle: a faint grey wash that works on any rail colour
fn subtle() -> Color32 {
    Color32::from_rgba_unmultiplied(0x97, 0x97, 0x9f, 31)
}

fn item(
    ui: &mut Ui,
    id: egui::Id,
    selected: bool,
    unread: bool,
    paint: impl FnOnce(&Ui, Rect, u8, bool),
) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(72.0, ICON), Sense::click());
    let rect = Rect::from_center_size(row.center(), vec2(ICON, ICON));
    let hovered = resp.hovered();
    let hot = hovered || selected;
    let radius = RADIUS;
    let pill_target = if selected {
        40.0
    } else if hovered {
        20.0
    } else if unread {
        8.0
    } else {
        0.0
    };
    let pill = ui.ctx().animate_value_with_time(id.with("p"), pill_target, anim(0.15));
    if pill > 0.5 {
        let p = Rect::from_min_size(pos2(row.left(), row.center().y - pill / 2.0), vec2(4.0, pill));
        ui.painter().rect_filled(
            p,
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            pal().header_primary,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint(ui, rect, radius, hot);
    resp
}

// the server-name bubble to the right of the rail, with emoji
fn tip(r: egui::Response, name: &str) -> egui::Response {
    r.on_hover_ui(|ui| {
        text::label(ui, name, semi(16.0), pal().header_primary, Sense::hover());
    })
}
