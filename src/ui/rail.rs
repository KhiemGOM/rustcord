use egui::{Align2, Color32, CornerRadius, Frame, Image, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, avatar, badge, motion, text};
use super::folders::{self, Drop, Src};
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
            // discord's home tile sits flush with the top of the rail
            ui.spacing_mut().item_spacing.y = 8.0;

            let incoming = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            let home = app.ui.guild.is_none();
            let r = item(ui, egui::Id::new("home"), home, false, |ui, rect, radius, _, k| {
                ui.painter().rect_filled(rect, CornerRadius::same(radius), motion::mix(subtle(), BLURPLE, k));
                icons::draw(ui.painter(), Icon::Discord, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)), motion::mix(pal().interactive_hover, Color32::WHITE, k));
            });
            if incoming > 0 {
                badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), incoming, Some(pal().bg_tertiary));
            }
            app.rects.insert("guild:home".into(), r.rect);
            let r = tip(r, "Direct Messages");
            if r.clicked() {
                app.select_guild(None);
                app.home_clicked();
            }

            // dms with something waiting or a call going get their own bubble under home, like the
            // official client; a muted one waits quietly in the list instead
            let waiting: Vec<(Id, u32)> = app
                .store
                .dms
                .iter()
                .filter(|c| !app.store.dm_muted(**c) || app.voice.chan == Some(**c))
                .map(|c| (*c, app.store.mentions(*c)))
                .filter(|(c, n)| *n > 0 || app.voice.chan == Some(*c) || app.voice.calls.get(c).is_some_and(|u| !u.is_empty()) || app.voice.ringing == Some(*c))
                .take(8)
                .collect();
            for (c, n) in waiting {
                let Some(ch) = app.store.channels.get(&c) else { continue };
                let url = match ch.kind {
                    1 => ch.recipients.first().and_then(|u| app.store.user(*u)).map(|u| u.avatar_url()),
                    _ => ch.icon.as_ref().map(|i| format!("{}/channel-icons/{c}/{i}.png?size=96", crate::api::cdn())),
                };
                let title = super::channel_title(&app.store, c);
                let r = item(ui, egui::Id::new(("dm", c)), false, false, |ui, rect, radius, _, _| {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius), BLURPLE);
                    if let Some(u) = &url {
                        Image::from_uri(u.clone()).corner_radius(CornerRadius::same(radius)).show_loading_spinner(false).paint_at(ui, rect);
                    } else {
                        icons::draw(ui.painter(), Icon::People, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), Color32::WHITE);
                    }
                });
                if n > 0 {
                    badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), n, Some(pal().bg_tertiary));
                }
                let mine = app.voice.chan == Some(c);
                if mine || app.voice.ringing == Some(c) || app.voice.calls.get(&c).is_some_and(|u| !u.is_empty()) {
                    let at = upper_badge(ui, Rect::from_center_size(r.rect.center(), vec2(ICON, ICON)), Icon::Speaker, mine, pal().bg_tertiary);
                    app.rects.insert(format!("dmcall:{c}"), Rect::from_center_size(at, vec2(16.0, 16.0)));
                }
                if app.voice.ringing == Some(c) {
                    let k = (ui.input(|i| i.time) as f32 * 1.5).fract();
                    ui.painter().rect_stroke(r.rect.expand(2.0 + 4.0 * k), CornerRadius::same(RADIUS + 2), egui::Stroke::new(2.0, GREEN.gamma_multiply(1.0 - k)), egui::StrokeKind::Outside);
                    ui.ctx().request_repaint();
                }
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
                    guild(app, ui, f.guilds[0], pal().bg_tertiary);
                } else {
                    folder(app, ui, f);
                }
            }
            drag(app, ui, &folders);

            separator(ui);
            let r = button(ui, egui::Id::new("add_server"), false, |ui, rect, radius, _, k| {
                ui.painter().rect_filled(rect, CornerRadius::same(radius), motion::mix(pal().bg_primary, GREEN, k));
                icons::draw(ui.painter(), Icon::CirclePlus, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), motion::mix(pal().interactive_hover, Color32::WHITE, k));
            });
            let r = tip(r, "Add a Server");
            if r.clicked() {
                app.wizard = Some(super::newserver::Wizard::new(app));
            }
            let mut add = None;
            egui::Popup::context_menu(&r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| {
                super::menu_body(ui, |ui| {
                    if super::menu_item(ui, None, "Join a server", false) {
                        add = Some(false);
                    }
                    if super::menu_item(ui, None, "Create a server", false) {
                        add = Some(true);
                    }
                })
            });
            match add {
                Some(false) => app.invite = Some(String::new()),
                Some(true) => app.wizard = Some(super::newserver::Wizard::new(app)),
                None => {}
            }
            // discord's last two rail buttons: discover opens in the app, the downloads page in the browser
            for (key, icon, tip, url) in [("discover", Icon::Discover, "Discover", ""), ("download", Icon::Download, "Download Apps", "https://discord.com/download")] {
                let on = key == "discover" && app.directory.is_some();
                let r = button(ui, egui::Id::new(key), on, |ui, rect, radius, _, k| {
                    let k = if on { 1.0 } else { k };
                    ui.painter().rect_filled(rect, CornerRadius::same(radius), motion::mix(pal().bg_primary, GREEN, k));
                    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), motion::mix(pal().interactive_hover, Color32::WHITE, k));
                });
                if self::tip(r, tip).clicked() {
                    if url.is_empty() {
                        app.open_servers();
                    } else {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                }
            }
            ui.add_space(12.0);
        });
}

fn separator(ui: &mut Ui) {
    let (sep, _) = ui.allocate_exact_size(vec2(72.0, 1.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(sep.center(), vec2(32.0, 1.0)), CornerRadius::same(1), pal().frame_border);
}

fn guild_icon(app: &App, ui: &Ui, gid: Id, rect: Rect, radius: impl Into<CornerRadius>, hot: bool, k: f32) {
    let Some(g) = app.store.guilds.get(&gid) else { return };
    let radius = radius.into();
    match g.icon_url() {
        Some(url) => {
            crate::gif::paint_still(ui, &url, rect, radius, hot);
        }
        None => {
            ui.painter().rect_filled(rect, radius, motion::mix(pal().bg_mod_selected, BLURPLE, k));
            let acr = g.acronym();
            let size = match acr.chars().count() {
                0..=2 => 17.0,
                3 => 15.0,
                4..=5 => 13.0,
                _ => 11.0,
            } * rect.width()
                / ICON;
            let col = motion::mix(pal().text_normal, Color32::WHITE, k);
            text::paint(ui.painter(), rect.center(), Align2::CENTER_CENTER, &acr, semi(size), col, rect.width() - 4.0);
        }
    }
}

/// `hole` is whatever is behind the icon, the rail or a folder, for the badges' cut-outs.
fn guild(app: &mut App, ui: &mut Ui, gid: Id, hole: Color32) {
    let Some(name) = app.store.guilds.get(&gid).map(|g| g.name.clone()) else { return };
    let (unread, mentions) = app.store.guild_badge(gid);
    let selected = app.ui.guild == Some(gid);
    let r = item(ui, egui::Id::new(("guild", gid)), selected, unread, |ui, rect, radius, hot, k| {
        guild_icon(app, ui, gid, rect, radius, hot, k)
    });
    if mentions > 0 {
        super::badge_in(ui, egui::Id::new(("rail_badge", gid)), r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(hole));
    }
    if let Some((icon, mine)) = voice_badge(app, gid) {
        let at = upper_badge(ui, Rect::from_center_size(r.rect.center(), vec2(ICON, ICON)), icon, mine, hole);
        app.rects.insert(format!("voicebadge:{gid}"), Rect::from_center_size(at, vec2(16.0, 16.0)));
    }
    app.rects.insert(format!("guild:{gid}"), r.rect);
    app.rects.insert(format!("guildicon:{gid}"), Rect::from_center_size(r.rect.center(), vec2(ICON, ICON)));
    let talking = in_voice(app, gid);
    let r = guild_tip(app, r, &name, &talking);
    if r.clicked() {
        app.select_guild(Some(gid));
    }
    grab(ui, &r, Held::Guild(gid));
    guild_menu(app, &r, gid, unread);
}

enum Act {
    Read,
    Invite(Id),
    Mute(bool, Option<i64>),
    Notifs,
    HideMuted,
    Privacy,
    Profile,
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
    egui::Popup::context_menu(r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| super::menu_body(ui, |ui| {
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
        if menu_item(ui, None, "Privacy Settings", false) {
            act = Some(Act::Privacy);
        }
        if menu_item(ui, None, "Edit Per-server Profile", false) {
            act = Some(Act::Profile);
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
            app.invite_people(c);
        }
        Some(Act::Mute(m, secs)) => app.mute_for(gid, true, m, secs),
        Some(Act::Notifs) => app.guild_notifs = Some(gid),
        Some(Act::HideMuted) => super::side::toggle_hide_muted(app, gid),
        Some(Act::Privacy) => app.guild_modal = Some(super::guildmodal::Modal::Privacy(gid)),
        Some(Act::Profile) => app.edit_server_profile(gid),
        Some(Act::Leave) => super::side::leave(app, gid),
        Some(Act::CopyId) => r.ctx.copy_text(gid.to_string()),
        None => {}
    }
}

/// The icon over a server's corner, like discord's: a stream beats a camera beats people talking,
/// and it's green when the call is ours. Muted servers, the AFK channel, stages, and channels we
/// can't see or have muted don't count.
fn voice_badge(app: &App, gid: Id) -> Option<(Icon, bool)> {
    let store = &app.store;
    let v = &app.voice;
    let g = store.guilds.get(&gid)?;
    if let Some(c) = v.chan.filter(|c| store.channels.get(c).is_some_and(|c| c.guild == gid)) {
        let icon = if v.live.contains(&store.me) {
            Icon::Screen
        } else if g.voice.iter().any(|(u, ch)| *ch == c && v.cams.contains(u)) {
            Icon::Video
        } else {
            Icon::Speaker
        };
        return Some((icon, true));
    }
    if store.muted.contains(&gid) {
        return None;
    }
    let here: Vec<Id> = g
        .voice
        .iter()
        .filter(|(u, c)| {
            **c != g.afk
                && store.relationships.get(*u) != Some(&crate::state::BLOCKED)
                && store.channels.get(*c).is_some_and(|c| c.kind != 13 && store.can_view(c) && !store.is_muted(c))
        })
        .map(|(u, _)| *u)
        .collect();
    let icon = if here.iter().any(|u| v.live.contains(u)) {
        Icon::Screen
    } else if here.iter().any(|u| v.cams.contains(u)) {
        Icon::Video
    } else if !here.is_empty() {
        Icon::Speaker
    } else {
        return None;
    };
    Some((icon, false))
}

/// A 16 disc in the icon's top right corner, cut 2 into the icon. Returns its centre.
fn upper_badge(ui: &Ui, icon_rect: Rect, icon: Icon, mine: bool, hole: Color32) -> egui::Pos2 {
    let at = icon_rect.right_top() + vec2(-8.0, 8.0);
    ui.painter().circle_filled(at, 10.0, hole);
    ui.painter().circle_filled(at, 8.0, if mine { GREEN } else { Color32::BLACK });
    icons::draw(ui.painter(), icon, Rect::from_center_size(at, vec2(12.0, 12.0)), Color32::WHITE);
    at
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

    // closed, discord's folder is a 48 tile of four 19px icons; open, its header is a 40 button in
    // a 48 row that sits 4 over the first server
    if open {
        ui.spacing_mut().item_spacing.y = 4.0;
    }
    let r = item_sized(ui, egui::Id::new(("folder", f.id)), 48.0, selected_inside && !open, unread && !open, 0.1, |ui, rect, radius, hot, _| {
        if open {
            let rect = rect.shrink(4.0);
            if hot {
                ui.painter().rect_filled(rect, CornerRadius::same(radius), subtle());
            }
            icons::draw(ui.painter(), Icon::Folder, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), color.unwrap_or(BLURPLE));
            return;
        }
        let tint = color.map_or(subtle(), |c| c.gamma_multiply(0.4));
        ui.painter().rect_filled(rect, CornerRadius::same(16), tint);
        for (i, g) in f.guilds.iter().take(4).enumerate() {
            let cell = Rect::from_min_size(
                rect.min + vec2(4.0 + (i % 2) as f32 * 21.0, 4.0 + (i / 2) as f32 * 21.0),
                vec2(19.0, 19.0),
            );
            // the corner on the folder's outside edge is the round one
            let mut cr = CornerRadius::same(4);
            match i {
                0 => cr.nw = 10,
                1 => cr.ne = 10,
                2 => cr.sw = 10,
                _ => cr.se = 10,
            }
            guild_icon(app, ui, *g, cell, cr, false, 0.0);
        }
    });
    ui.spacing_mut().item_spacing.y = 8.0;
    if mentions > 0 && !open {
        badge(ui, r.rect.right_bottom() - vec2(12.0, 8.0), mentions, Some(pal().bg_tertiary));
    }
    // closed, the folder carries our call's badge for the server inside it
    if !open && let Some((icon, _)) = f.guilds.iter().find_map(|g| voice_badge(app, *g).filter(|b| b.1)) {
        upper_badge(ui, Rect::from_center_size(r.rect.center(), vec2(48.0, 48.0)), icon, true, pal().bg_tertiary);
    }
    app.rects.insert(format!("folder:{}", f.id), r.rect);
    let name = f.name.clone().unwrap_or_else(|| {
        f.guilds.iter().filter_map(|g| app.store.guilds.get(g)).map(|g| g.name.clone()).collect::<Vec<_>>().join(", ")
    });
    let r = tip(r, &name);
    if r.clicked() && !app.ui.open_folders.remove(&f.id) {
        app.ui.open_folders.insert(f.id);
    }
    grab(ui, &r, Held::Folder(f.id));
    use super::{Trail, menu_item, menu_row, menu_sep};
    let mut act = None;
    egui::Popup::context_menu(&r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| {
        super::menu_body(ui, |ui| {
            if ui.add_enabled_ui(unread, |ui| menu_item(ui, None, "Mark Folder As Read", false)).inner {
                act = Some(0);
            }
            menu_sep(ui);
            let m = menu_row(ui, None, "Move to...", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                if menu_item(ui, None, "First in List", false) {
                    act = Some(1);
                }
                if menu_item(ui, None, "Last in List", false) {
                    act = Some(2);
                }
            });
            menu_sep(ui);
            if menu_item(ui, None, "Folder Settings", false) {
                act = Some(3);
            }
        })
    });
    match act {
        Some(0) => {
            let chans: Vec<Id> = f.guilds.iter().filter_map(|g| app.store.guilds.get(g)).flat_map(|g| g.channels.iter().copied()).collect();
            app.mark_read(&chans);
        }
        Some(n @ (1 | 2)) => super::folders::move_folder(app, f.id, n == 2),
        Some(_) => super::folders::settings(app, f.id),
        None => {}
    }
    if open {
        let fill = color.map_or(subtle(), |c| c.gamma_multiply(0.15));
        for g in &f.guilds {
            guild(app, ui, *g, pal().bg_tertiary.blend(fill));
        }
        // 4 under the last server, and the usual 8 to whatever follows
        let back = Rect::from_min_max(pos2(12.0, top), pos2(60.0, ui.cursor().top() - 4.0));
        let back = back.translate(vec2(ui.max_rect().left(), 0.0));
        ui.add_space(4.0);
        app.rects.insert(format!("folderbg:{}", f.id), back);
        ui.painter().set(
            bg,
            egui::epaint::RectShape::new(back, CornerRadius::same(16), fill, egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x97, 0x97, 0x9f, 66)), egui::StrokeKind::Inside),
        );
    }
}

// discord's background-mod-subtle: a faint grey wash that works on any rail colour
fn subtle() -> Color32 {
    Color32::from_rgba_unmultiplied(0x97, 0x97, 0x9f, 31)
}

fn item(ui: &mut Ui, id: egui::Id, selected: bool, unread: bool, paint: impl FnOnce(&Ui, Rect, u8, bool, f32)) -> egui::Response {
    item_sized(ui, id, ICON, selected, unread, 0.1, paint)
}

fn button(ui: &mut Ui, id: egui::Id, selected: bool, paint: impl FnOnce(&Ui, Rect, u8, bool, f32)) -> egui::Response {
    item_sized(ui, id, ICON, selected, false, 0.15, paint)
}

fn item_sized(
    ui: &mut Ui,
    id: egui::Id,
    size: f32,
    selected: bool,
    unread: bool,
    fade: f32,
    paint: impl FnOnce(&Ui, Rect, u8, bool, f32),
) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(72.0, size), Sense::click_and_drag());
    let rect = Rect::from_center_size(row.center(), vec2(size, size));
    let hovered = resp.hovered();
    let hot = hovered || selected;
    let radius = RADIUS;
    let pill_target = if selected {
        40.0
    } else if hovered {
        20.0
    } else {
        8.0
    };
    let ctx = ui.ctx();
    let pill = motion::follow(ctx, id.with("p"), pill_target, 0.2, motion::EASE_OUT);
    let seen = motion::tween(ctx, id.with("seen"), hot || unread, 0.2, motion::EASE_OUT);
    if seen > 0.0 {
        let p = Rect::from_min_size(pos2(row.left() - 4.0 * (1.0 - seen), row.center().y - pill / 2.0), vec2(4.0, pill));
        ui.painter().rect_filled(p, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, pal().header_primary.gamma_multiply(seen));
    }
    let k = motion::tween(ctx, id.with("hot"), hot, fade, motion::EASE_OUT);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint(ui, rect, radius, hot, k);
    resp
}

/// Who's in the server's voice channels, by name: the AFK channel, stages, channels we can't see
/// and people we've blocked left out.
fn in_voice(app: &App, gid: Id) -> Vec<(Id, String)> {
    let store = &app.store;
    let Some(g) = store.guilds.get(&gid) else { return Vec::new() };
    let mut v: Vec<(Id, String)> = g
        .voice
        .iter()
        .filter(|(u, c)| {
            **c != g.afk
                && store.relationships.get(*u) != Some(&crate::state::BLOCKED)
                && store.channels.get(*c).is_some_and(|c| c.kind != 13 && store.can_view(c))
        })
        .filter_map(|(u, _)| store.user(*u).map(|x| (*u, x.avatar_url())))
        .collect();
    v.sort_by_cached_key(|(u, _)| store.name_in(gid, *u).to_lowercase());
    v
}

/// Discord's server tooltip: the name, and while anyone's talking a speaker with their avatars
/// overlapping beside it, six at most and a count for the rest.
fn guild_tip(app: &mut App, r: egui::Response, name: &str, talking: &[(Id, String)]) -> egui::Response {
    let rects = &mut app.rects;
    beside(r, |ui| {
        ui.set_max_width(196.0);
        text::label(ui, name, semi(16.0), pal().text_normal, Sense::hover());
        if talking.is_empty() {
            return;
        }
        ui.add_space(8.0);
        let shown = talking.len().min(6);
        let more = (talking.len() > shown).then(|| ui.painter().layout_no_wrap(format!("+{}", talking.len() - shown), semi(12.0), pal().header_secondary));
        let stack = 24.0 + (shown - 1) as f32 * 18.0;
        let w = 28.0 + stack + more.as_ref().map_or(0.0, |g| 6.0 + g.size().x);
        let (row, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
        icons::draw(ui.painter(), Icon::Speaker, Rect::from_center_size(pos2(row.left() + 10.0, row.center().y), vec2(20.0, 20.0)), pal().header_secondary);
        for (i, (_, url)) in talking.iter().take(shown).enumerate() {
            let c = pos2(row.left() + 40.0 + i as f32 * 18.0, row.center().y);
            // each one sits on the last with a ring cut out of it, like discord's stack
            if i > 0 {
                ui.painter().circle_filled(c, 14.0, ui.visuals().window_fill);
            }
            avatar(ui, Rect::from_center_size(c, vec2(24.0, 24.0)), url);
        }
        if let Some(g) = more {
            let x = row.left() + 28.0 + stack + 6.0;
            ui.painter().galley(pos2(x, row.center().y - g.size().y / 2.0), g, pal().header_secondary);
        }
        rects.insert("guildtip_voice".into(), row);
    })
}

// the server-name bubble to the right of the rail, with emoji
fn tip(r: egui::Response, name: &str) -> egui::Response {
    beside(r, |ui| {
        text::label(ui, name, semi(16.0), pal().header_primary, Sense::hover());
    })
}

fn beside(r: egui::Response, content: impl FnOnce(&mut Ui)) -> egui::Response {
    if !r.hovered() || r.dragged() {
        return r;
    }
    let ctx = r.ctx.clone();
    let frame = Frame::popup(&ctx.global_style());
    let (fill, edge) = (frame.fill, frame.stroke);
    let gap = 10.0;
    let shown = egui::Popup::from_response(&r)
        .id(r.id.with("rail_tip"))
        .kind(egui::PopupKind::Tooltip)
        .align(egui::RectAlign::RIGHT)
        .gap(gap)
        .open(true)
        .frame(frame)
        .show(content);
    if let Some(s) = shown {
        let b = s.response.rect;
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "rail_tip")), (r.rect, b)));
        let tip = pos2(b.left() - 6.0, b.center().y);
        let p = ctx.layer_painter(s.response.layer_id);
        let tri = [pos2(b.left() + 1.0, b.center().y - 6.0), tip, pos2(b.left() + 1.0, b.center().y + 6.0)];
        p.add(egui::Shape::convex_polygon(tri.to_vec(), fill, Stroke::NONE));
        p.line_segment([tri[0], tri[1]], edge);
        p.line_segment([tri[1], tri[2]], edge);
    }
    r
}

#[derive(Clone, Copy)]
enum Held {
    Guild(Id),
    Folder(Id),
}

fn grab(ui: &Ui, r: &egui::Response, held: Held) {
    if r.drag_started() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("rail_held"), held));
    }
}

fn spot(app: &App, folders: &[Folder], held: Held, p: egui::Pos2) -> Option<Drop> {
    let band = |r: &Rect| r.expand2(vec2(0.0, 4.0));
    for (i, f) in folders.iter().enumerate() {
        let lone = f.id == 0 && f.guilds.len() == 1;
        let head = app.rects.get(&if lone { format!("guild:{}", f.guilds[0]) } else { format!("folder:{}", f.id) })?;
        let open = !lone && app.ui.open_folders.contains(&f.id);
        if band(head).contains(p) {
            let rel = (p.y - head.top()) / head.height();
            return match held {
                Held::Folder(_) => Some(Drop::Before(i + (rel >= 0.5) as usize)),
                Held::Guild(g) if lone && f.guilds[0] == g => None,
                Held::Guild(_) if open => Some(if rel < 0.5 { Drop::Before(i) } else { Drop::In(i, 0) }),
                Held::Guild(_) if rel < 0.28 => Some(Drop::Before(i)),
                Held::Guild(_) if rel > 0.72 => Some(Drop::Before(i + 1)),
                Held::Guild(_) => Some(Drop::Combine(i)),
            };
        }
        if open && let Held::Guild(_) = held {
            for (n, g) in f.guilds.iter().enumerate() {
                let Some(r) = app.rects.get(&format!("guild:{g}")) else { continue };
                if band(r).contains(p) {
                    return Some(Drop::In(i, n + ((p.y - r.top()) / r.height() >= 0.5) as usize));
                }
            }
        }
    }
    None
}

fn drag(app: &mut App, ui: &mut Ui, folders: &[Folder]) {
    let ctx = ui.ctx().clone();
    if let Some((g, _)) = app.folders.pick
        && let Some(r) = app.rects.get(&format!("guild:{g}"))
    {
        let ring = Rect::from_center_size(r.center(), vec2(ICON + 8.0, ICON + 8.0));
        ui.painter().rect_stroke(ring, CornerRadius::same(16), egui::Stroke::new(2.0, BLURPLE), egui::StrokeKind::Outside);
    }
    let key = egui::Id::new("rail_held");
    let Some(held) = ctx.data(|d| d.get_temp::<Held>(key)) else { return };
    let src = match held {
        Held::Guild(g) => Src::Guild(g),
        Held::Folder(id) => match folders.iter().position(|f| f.id == id) {
            Some(i) => Src::Folder(i),
            None => return,
        },
    };
    let pos = ctx.pointer_interact_pos();
    let target = pos.and_then(|p| spot(app, folders, held, p));
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    let top = ui.clip_rect();
    if let Some(p) = pos {
        if p.y < top.top() + 48.0 {
            ui.scroll_with_delta(vec2(0.0, 14.0));
        } else if p.y > top.bottom() - 48.0 {
            ui.scroll_with_delta(vec2(0.0, -14.0));
        }
        egui::Area::new(egui::Id::new("rail_ghost")).order(egui::Order::Tooltip).fixed_pos(p - vec2(20.0, 20.0)).interactable(false).show(&ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ICON, ICON), Sense::hover());
            match held {
                Held::Guild(g) => guild_icon(app, ui, g, r, RADIUS, true, 1.0),
                Held::Folder(_) => {
                    ui.painter().rect_filled(r, CornerRadius::same(16), subtle());
                    icons::draw(ui.painter(), Icon::Folder, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), BLURPLE);
                }
            }
        });
    }
    let paint = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("rail_drop")));
    let bar = |y: f32, x: f32| paint.rect_filled(Rect::from_center_size(pos2(x, y), vec2(32.0, 4.0)), CornerRadius::same(2), BLURPLE);
    let head = |i: usize| folders.get(i).and_then(|f| app.rects.get(&if f.id == 0 && f.guilds.len() == 1 { format!("guild:{}", f.guilds[0]) } else { format!("folder:{}", f.id) }));
    match target {
        Some(Drop::Before(i)) => {
            if let Some(r) = head(i) {
                bar(r.top() - 4.0, r.center().x);
            } else if let Some(f) = folders.last() {
                let end = app.rects.get(&format!("folderbg:{}", f.id)).map(|r| r.bottom() + 4.0).or_else(|| head(folders.len() - 1).map(|r| r.bottom() + 4.0));
                if let (Some(y), Some(r)) = (end, head(folders.len() - 1)) {
                    bar(y, r.center().x);
                }
            }
        }
        Some(Drop::Combine(i)) => {
            if let Some(r) = head(i) {
                paint.rect_stroke(r.shrink2(vec2(12.0, 0.0)).expand(2.0), CornerRadius::same(16), egui::Stroke::new(2.0, BLURPLE), egui::StrokeKind::Outside);
            }
        }
        Some(Drop::In(i, n)) => {
            if let Some(f) = folders.get(i) {
                let at = f.guilds.get(n).and_then(|g| app.rects.get(&format!("guild:{g}"))).map(|r| (r.top() - 4.0, r.center().x)).or_else(|| {
                    f.guilds.last().and_then(|g| app.rects.get(&format!("guild:{g}"))).map(|r| (r.bottom() + 4.0, r.center().x))
                });
                if let Some((y, x)) = at {
                    bar(y, x);
                }
            }
        }
        None => {}
    }
    let done = !ctx.input(|i| i.pointer.primary_down());
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if done || esc {
        ctx.data_mut(|d| d.remove::<Held>(key));
        if done && !esc && let Some(t) = target {
            folders::drop(app, src, t);
        }
    }
}
