use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar, badge, channel_title, icon_button, status_dot};
use super::icons::{self, Icon};
use super::text;
use crate::state::{Channel, Id, Status, Store};
use crate::theme::*;

fn sort_key(c: &Channel) -> (bool, i32, Id) {
    (c.is_voice(), c.position, c.id)
}

/// Visible channels grouped under their categories, in sidebar order.
pub fn tree(store: &Store, gid: Id) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
    let Some(g) = store.guilds.get(&gid) else { return Vec::new() };
    let all: Vec<&Channel> = g.channels.iter().filter_map(|c| store.channels.get(c)).collect();
    let visible: Vec<&Channel> = all
        .iter()
        .copied()
        .filter(|c| c.kind != 4 && !matches!(c.kind, 10..=12) && store.can_view(c))
        .collect();
    let mut cats: Vec<&Channel> = all.iter().copied().filter(|c| c.kind == 4).collect();
    cats.sort_by_key(|c| (c.position, c.id));

    let mut out = Vec::new();
    let mut loose: Vec<&Channel> = visible.iter().copied().filter(|c| c.parent == 0 || !cats.iter().any(|k| k.id == c.parent)).collect();
    loose.sort_by_key(|c| sort_key(c));
    if !loose.is_empty() {
        out.push((None, loose));
    }
    for cat in cats {
        let mut kids: Vec<&Channel> = visible.iter().copied().filter(|c| c.parent == cat.id).collect();
        if kids.is_empty() && !store.can_view(cat) {
            continue;
        }
        kids.sort_by_key(|c| sort_key(c));
        out.push((Some(cat), kids));
    }
    out
}

pub fn first_channel(store: &Store, gid: Id) -> Option<Id> {
    tree(store, gid).into_iter().flat_map(|(_, k)| k).find(|c| c.is_text()).map(|c| c.id)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    // discord floats the account card 8 above the bottom and in from the sides
    egui::Panel::bottom("me")
        .exact_size(64.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 8 }))
        .show(ui, |ui| {
            let card = ui.max_rect();
            ui.painter().rect(card, 8, pal().bg_secondary_alt, egui::Stroke::new(1.0, pal().bg_mod_hover), egui::StrokeKind::Inside);
            app.rects.insert("userpanel".into(), card);
            ui.scope_builder(egui::UiBuilder::new().max_rect(card.shrink2(vec2(8.0, 0.0))), |ui| user_panel(app, ui));
        });
    if app.voice.chan.is_some() {
        egui::Panel::bottom("voice")
            .exact_size(60.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_secondary_alt).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ui, |ui| super::call::panel(app, ui));
    }

    match app.ui.guild {
        Some(gid) => guild(app, ui, gid),
        None => dms(app, ui),
    }
}

fn header(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Panel::top("side_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(egui::Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), egui::Stroke::new(1.0, pal().bg_tertiary));
            ui.horizontal_centered(add);
        });
}

fn guild(app: &mut App, ui: &mut Ui, gid: Id) {
    let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    header(ui, |ui| {
        text::label(ui, &name, bold(16.0), pal().header_primary, Sense::hover());
    });

    let cur = app.cur();
    let mut open: Option<Id> = None;
    let mut toggle: Option<Id> = None;
    let mut menu: Option<(Id, ChanAct)> = None;
    let dev = app.settings.dev;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(8.0);
            let store = &app.store;
            let mut threads: Vec<&Channel> = store
                .channels
                .values()
                .filter(|c| c.guild == gid && c.is_thread() && c.joined && !c.archived)
                .collect();
            threads.sort_by_key(|t| t.id);
            for (cat, kids) in tree(store, gid) {
                let collapsed = cat.is_some_and(|c| app.ui.collapsed.contains(&c.id));
                if let Some(cat) = cat {
                    ui.add_space(14.0);
                    if category(ui, &cat.name, collapsed).clicked() {
                        toggle = Some(cat.id);
                    }
                }
                for c in kids {
                    let selected = cur == Some(c.id);
                    let unread = c.is_text() && store.unread(c);
                    if collapsed && !selected && !unread {
                        continue;
                    }
                    let muted = store.is_muted(c);
                    let (r, emoji) = channel_row(ui, store, c, selected, unread, store.mentions(c.id), muted);
                    app.rects.insert(format!("chan:{}", c.id), r.rect);
                    if let Some(e) = emoji {
                        app.rects.insert(format!("chanemoji:{}", c.id), e);
                    }
                    app.rects.insert(format!("chanicon:{}", c.id), Rect::from_center_size(pos2(r.rect.left() + 26.0, r.rect.center().y), vec2(20.0, 20.0)));
                    if r.clicked() {
                        open = Some(c.id);
                    }
                    channel_menu(&r, c.id, muted, dev, &mut menu);
                    let kids: Vec<&&Channel> = threads.iter().filter(|t| t.parent == c.id).collect();
                    let n = kids.len();
                    for (i, t) in kids.into_iter().enumerate() {
                        let r = thread_row(ui, t, cur == Some(t.id), store.unread(t), i + 1 == n);
                        app.rects.insert(format!("chan:{}", t.id), r.rect);
                        if r.clicked() {
                            open = Some(t.id);
                        }
                        channel_menu(&r, t.id, store.is_muted(t), dev, &mut menu);
                    }
                    if c.is_voice() && !collapsed {
                        voice_users(ui, store, &app.voice, gid, c.id);
                    }
                }
            }
            ui.add_space(12.0);
        });
    });
    if let Some(c) = toggle
        && !app.ui.collapsed.remove(&c)
    {
        app.ui.collapsed.insert(c);
    }
    match menu {
        Some((c, ChanAct::Read)) => app.mark_read(&[c]),
        Some((c, ChanAct::Mute(m))) => app.set_muted(c, false, m),
        Some((c, ChanAct::Link)) => ui.ctx().copy_text(format!("https://discord.com/channels/{gid}/{c}")),
        Some((c, ChanAct::CopyId)) => ui.ctx().copy_text(c.to_string()),
        None => {}
    }
    if let Some(c) = open {
        if app.store.channels.get(&c).is_some_and(|c| c.is_voice()) {
            app.voice.join(&app.net, &app.store, c);
        }
        app.open(c);
    }
}

enum ChanAct {
    Read,
    Mute(bool),
    Link,
    CopyId,
}

fn channel_menu(r: &egui::Response, id: Id, muted: bool, dev: bool, out: &mut Option<(Id, ChanAct)>) {
    r.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui.button("Mark As Read").clicked() {
            *out = Some((id, ChanAct::Read));
            ui.close();
        }
        if ui.button(if muted { "Unmute Channel" } else { "Mute Channel" }).clicked() {
            *out = Some((id, ChanAct::Mute(!muted)));
            ui.close();
        }
        if ui.button("Copy Link").clicked() {
            *out = Some((id, ChanAct::Link));
            ui.close();
        }
        if dev && ui.button("Copy Channel ID").clicked() {
            *out = Some((id, ChanAct::CopyId));
            ui.close();
        }
    });
}

fn thread_row(ui: &mut Ui, t: &Channel, selected: bool, unread: bool, last: bool) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let spine = egui::Stroke::new(1.5, pal().bg_accent);
    let x = row.left() + 24.0;
    // the little tree branch the official client draws from the parent channel
    ui.painter().line_segment([pos2(x, row.top() - 2.0), pos2(x, if last { row.center().y - 6.0 } else { row.bottom() + 2.0 })], spine);
    ui.painter().add(egui::epaint::CubicBezierShape::from_points_stroke(
        [pos2(x, row.center().y - 6.0), pos2(x, row.center().y), pos2(x, row.center().y), pos2(x + 8.0, row.center().y)],
        false,
        Color32::TRANSPARENT,
        spine,
    ));
    let inner = Rect::from_min_max(pos2(row.left() + 36.0, row.top()), pos2(row.right() - 8.0, row.bottom()));
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let col = if selected || unread {
        pal().header_primary
    } else if resp.hovered() {
        pal().interactive_hover
    } else {
        pal().channel_default
    };
    let font = if selected || unread { semi(15.0) } else { reg(15.0) };
    text::paint_em(ui.painter(), pos2(inner.left() + 8.0, row.center().y), Align2::LEFT_CENTER, &t.name, font, col, inner.width() - 16.0, 1.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn category(ui: &mut Ui, name: &str, collapsed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let col = if resp.hovered() { pal().interactive_hover } else { pal().channel_default };
    let chev = if collapsed { Icon::ChevronRight } else { Icon::ChevronDown };
    let y = rect.center().y;
    icons::draw(ui.painter(), chev, Rect::from_center_size(pos2(rect.left() + 10.0, y), vec2(12.0, 12.0)), col);
    text::paint(ui.painter(), pos2(rect.left() + 18.0, y), Align2::LEFT_CENTER, &name.to_uppercase(), semi(12.0), col, rect.width() - 26.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The channel's icon with the lock or warning discord adds for private and age-gated ones.
pub fn chan_icon(store: &Store, p: &egui::Painter, c: &Channel, rect: Rect, col: Color32) {
    let icon = if store.guilds.get(&c.guild).is_some_and(|g| g.rules == c.id) { Icon::Rules } else { glyph(c) };
    if store.locked(c) {
        icons::draw_marked(p, icon, icons::Mark::Lock, rect, col);
    } else if c.nsfw && !c.is_thread() {
        icons::draw_marked(p, icon, icons::Mark::Warn, rect, col);
    } else {
        icons::draw(p, icon, rect, col);
    }
}

pub fn glyph(c: &Channel) -> Icon {
    match c.kind {
        2 => Icon::Speaker,
        13 => Icon::Stage,
        5 => Icon::Megaphone,
        15 => Icon::Forum,
        16 => Icon::Image,
        10..=12 => Icon::Thread,
        _ => Icon::Hash,
    }
}

fn channel_row(ui: &mut Ui, store: &Store, c: &Channel, selected: bool, unread: bool, mentions: u32, muted: bool) -> (egui::Response, Option<Rect>) {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let hovered = resp.hovered();
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let col = if selected || (unread && !muted) {
        pal().header_primary
    } else if hovered {
        pal().interactive_hover
    } else if muted {
        pal().channel_muted
    } else {
        pal().channel_default
    };
    if unread && !selected && !muted {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(row.left(), row.center().y - 4.0), vec2(4.0, 8.0)),
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            pal().header_primary,
        );
    }
    let icon_col = if selected {
        pal().header_primary
    } else if muted {
        pal().channel_muted
    } else {
        pal().channel_default
    };
    let ir = Rect::from_center_size(pos2(inner.left() + 18.0, row.center().y), vec2(20.0, 20.0));
    chan_icon(store, ui.painter(), c, ir, icon_col);
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let font = if selected || unread { semi(16.0) } else { reg(16.0) };
    let left = inner.left() + 34.0;
    let name = text::layout_em(ui.ctx(), &c.name, font, col, right - left, 1.0);
    let at = pos2(left, row.center().y - name.size().y / 2.0);
    name.paint(ui.painter(), at);
    let emoji = name.emoji_rects(at).next().map(|(_, r)| r);
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, row.center().y), mentions, None);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, emoji)
}

fn voice_users(ui: &mut Ui, store: &Store, voice: &crate::voice::VoiceUi, gid: Id, ch: Id) {
    let Some(g) = store.guilds.get(&gid) else { return };
    let mut users: Vec<Id> = g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, _)| *u).collect();
    users.sort();
    for u in users {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        let av = Rect::from_min_size(pos2(row.left() + 44.0, row.center().y - 12.0), vec2(24.0, 24.0));
        if let Some(user) = store.user(u) {
            avatar(ui, av, &user.avatar_url());
        }
        if voice.is_speaking(u) {
            super::call::ring(ui, av);
        }
        let used = super::call::badges(ui, voice, u, row);
        text::paint_em(ui.painter(), pos2(av.right() + 8.0, row.center().y), Align2::LEFT_CENTER, &store.name_in(gid, u), reg(14.0), pal().channel_default, row.right() - av.right() - 16.0 - used, 1.0);
    }
}

fn dms(app: &mut App, ui: &mut Ui) {
    header(ui, |ui| {
        let w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(w, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            Frame::new().fill(pal().bg_tertiary).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.dm_filter)
                        .frame(Frame::NONE)
                        .hint_text(egui::RichText::new("Find or start a conversation").color(pal().text_muted))
                        .font(reg(14.0))
                        .desired_width(ui.available_width()),
                );
            });
        });
    });
    let cur = app.cur();
    let mut open = None;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(8.0);
            let pending = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
            let inner = row.shrink2(vec2(8.0, 0.0));
            if app.friends {
                ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
            }
            let col = if app.friends { pal().header_primary } else if resp.hovered() { pal().interactive_hover } else { pal().channel_default };
            icons::draw(ui.painter(), Icon::People, Rect::from_center_size(pos2(inner.left() + 24.0, row.center().y), vec2(24.0, 24.0)), col);
            ui.painter().text(pos2(inner.left() + 52.0, row.center().y), Align2::LEFT_CENTER, "Friends", semi(16.0), col);
            if pending > 0 {
                badge(ui, pos2(inner.right() - 8.0, row.center().y), pending, None);
            }
            app.rects.insert("friends".into(), row);
            if resp.clicked() {
                app.friends = true;
            }
            ui.add_space(8.0);
            let (h, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
            ui.painter().text(pos2(h.left() + 18.0, h.center().y), Align2::LEFT_CENTER, "DIRECT MESSAGES", semi(12.0), pal().channel_default);
            ui.add_space(4.0);
            let filter = app.dm_filter.to_lowercase();
            let store = &app.store;
            for &id in &store.dms {
                let Some(c) = store.channels.get(&id) else { continue };
                let title = channel_title(store, id);
                if !filter.is_empty() && !title.to_lowercase().contains(&filter) {
                    continue;
                }
                let r = dm_row(ui, store, c, &title, cur == Some(id) && !app.friends);
                app.rects.insert(format!("chan:{id}"), r.rect);
                if r.clicked() {
                    open = Some(id);
                }
            }
        });
    });
    if let Some(c) = open {
        app.open(c);
    }
}

fn dm_row(ui: &mut Ui, store: &Store, c: &Channel, title: &str, selected: bool) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let hovered = resp.hovered();
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let unread = store.unread(c);
    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
    let ring = if selected { pal().bg_mod_selected } else if hovered { pal().bg_mod_hover } else { pal().bg_secondary };
    if c.kind == 1 {
        if let Some(u) = c.recipients.first().and_then(|u| store.user(*u)) {
            avatar(ui, av, &u.avatar_url());
            status_dot(ui, av, store.status(u.id), ring);
        }
    } else {
        match &c.icon {
            Some(icon) => avatar(ui, av, &format!("{}/channel-icons/{}/{}.png?size=64", crate::api::cdn(), c.id, icon)),
            None => {
                ui.painter().circle_filled(av.center(), 16.0, group_color(c.id));
                icons::draw(ui.painter(), Icon::People, Rect::from_center_size(av.center(), vec2(20.0, 20.0)), Color32::WHITE);
            }
        }
    }
    let col = if selected || unread { pal().header_primary } else if hovered { pal().interactive_hover } else { pal().channel_default };
    let mentions = store.mentions(c.id);
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let font = if unread { semi(16.0) } else { reg(16.0) };
    let tw = right - av.right() - 12.0;
    let clip = ui.painter().with_clip_rect(Rect::from_x_y_ranges(av.right()..=right, row.y_range()));
    if c.kind == 3 {
        text::paint_em(ui.painter(), pos2(av.right() + 12.0, row.center().y - 8.0), Align2::LEFT_CENTER, title, font, col, tw, 1.0);
        clip.text(
            pos2(av.right() + 12.0, row.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("{} Members", c.recipients.len() + 1),
            reg(12.0),
            pal().text_muted,
        );
    } else {
        text::paint_em(ui.painter(), pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, title, font, col, tw, 1.0);
    }
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, row.center().y), mentions, None);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn group_color(id: Id) -> Color32 {
    const C: [Color32; 5] = [
        Color32::from_rgb(0x58, 0x65, 0xf2),
        Color32::from_rgb(0x3b, 0xa5, 0x5c),
        Color32::from_rgb(0xfa, 0xa6, 0x1a),
        Color32::from_rgb(0xed, 0x42, 0x45),
        Color32::from_rgb(0xeb, 0x45, 0x9e),
    ];
    C[(id % 5) as usize]
}

fn user_panel(app: &mut App, ui: &mut Ui) {
    let me = app.store.me;
    let user = app.store.user(me).cloned();
    let mut status_pick: Option<&str> = None;
    ui.horizontal_centered(|ui| {
        let (av, avr) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
        egui::Popup::menu(&avr).show(|ui| {
            ui.set_min_width(200.0);
            for (key, label, st) in [
                ("online", "Online", Status::Online),
                ("idle", "Idle", Status::Idle),
                ("dnd", "Do Not Disturb", Status::Dnd),
                ("invisible", "Invisible", Status::Offline),
            ] {
                let (r, resp) = ui.allocate_exact_size(vec2(200.0, 32.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(2), BLURPLE);
                }
                ui.painter().circle_filled(pos2(r.left() + 14.0, r.center().y), 5.0, crate::theme::status_color(st));
                ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, reg(14.0), pal().header_primary);
                if resp.clicked() {
                    status_pick = Some(key);
                    ui.close();
                }
            }
        });
        if let Some(u) = &user {
            avatar(ui, av, &u.avatar_url());
        }
        let status = match app.conn {
            crate::net::Conn::Online => app.store.presence.get(&me).copied().unwrap_or(Status::Online),
            _ => Status::Offline,
        };
        status_dot(ui, av, status, pal().bg_secondary_alt);
        // leave room for the three buttons so long names elide instead of running under them
        let col = (ui.available_width() - 100.0).max(40.0);
        ui.vertical(|ui| {
            ui.set_width(col);
            ui.set_max_width(col);
            ui.add_space(8.0);
            let name = user.as_ref().map(|u| u.display().to_string()).unwrap_or_default();
            text::label(ui, &name, semi(14.0), pal().header_primary, Sense::hover());
            let sub = match app.conn {
                crate::net::Conn::Online if app.settings.hiding_personal() => "Streamer Mode".into(),
                crate::net::Conn::Online => user.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
                crate::net::Conn::Connecting => "Connecting…".into(),
                crate::net::Conn::Resuming => "Reconnecting…".into(),
                crate::net::Conn::Offline => "Offline".into(),
            };
            ui.add(egui::Label::new(egui::RichText::new(sub).font(reg(12.0)).color(pal().header_secondary)).truncate().selectable(false));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let gear = icon_button(ui, Icon::Gear, 32.0, "User Settings");
            app.rects.insert("gear".into(), gear.rect);
            if gear.clicked() {
                app.settings_page = Some(super::settings::Page::Account);
            }
            super::call::user_buttons(app, ui);
        });
    });
    if let Some(s) = status_pick {
        app.set_status(s);
    }
}
