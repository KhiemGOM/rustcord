use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar, badge, channel_title, icon_button, status_dot};
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
    egui::Panel::bottom("me")
        .exact_size(52.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BG_SECONDARY_ALT).inner_margin(egui::Margin::symmetric(8, 0)))
        .show(ui, |ui| user_panel(app, ui));

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
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), egui::Stroke::new(1.0, BG_TERTIARY));
            ui.horizontal_centered(add);
        });
}

fn guild(app: &mut App, ui: &mut Ui, gid: Id) {
    let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    header(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(name).font(bold(16.0)).color(HEADER_PRIMARY)).truncate().selectable(false));
    });

    let cur = app.cur();
    let mut open: Option<Id> = None;
    let mut toggle: Option<Id> = None;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(8.0);
            let store = &app.store;
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
                    let r = channel_row(ui, c, selected, unread, store.mentions(c.id), store.muted.contains(&c.id));
                    app.rects.insert(format!("chan:{}", c.id), r.rect);
                    if r.clicked() {
                        open = Some(c.id);
                    }
                    if c.is_voice() && !collapsed {
                        voice_users(ui, store, gid, c.id);
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
    if let Some(c) = open {
        if app.store.channels.get(&c).is_some_and(|c| c.is_voice()) {
            app.toast("Voice isn't wired up yet in this build.");
        } else {
            app.open(c);
        }
    }
}

fn category(ui: &mut Ui, name: &str, collapsed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let col = if resp.hovered() { INTERACTIVE_HOVER } else { CHANNEL_DEFAULT };
    let chev = if collapsed { "›" } else { "⌄" };
    let y = rect.center().y;
    ui.painter().text(pos2(rect.left() + 10.0, y - if collapsed { 1.0 } else { 3.0 }), Align2::CENTER_CENTER, chev, semi(12.0), col);
    ui.painter().with_clip_rect(rect.shrink2(vec2(8.0, 0.0))).text(
        pos2(rect.left() + 18.0, y),
        Align2::LEFT_CENTER,
        name.to_uppercase(),
        semi(12.0),
        col,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn glyph(c: &Channel) -> &'static str {
    match c.kind {
        2 => "🔊",
        13 => "📡",
        5 => "📢",
        15 | 16 => "💬",
        _ => "#",
    }
}

fn channel_row(ui: &mut Ui, c: &Channel, selected: bool, unread: bool, mentions: u32, muted: bool) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let hovered = resp.hovered();
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), BG_MOD_SELECTED);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), BG_MOD_HOVER);
    }
    let col = if selected || (unread && !muted) {
        HEADER_PRIMARY
    } else if hovered {
        INTERACTIVE_HOVER
    } else if muted {
        CHANNEL_MUTED
    } else {
        CHANNEL_DEFAULT
    };
    if unread && !selected && !muted {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(row.left(), row.center().y - 4.0), vec2(4.0, 8.0)),
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            HEADER_PRIMARY,
        );
    }
    let g = glyph(c);
    let gfont = if g == "#" { reg(22.0) } else { reg(15.0) };
    ui.painter().text(pos2(inner.left() + 16.0, row.center().y), Align2::CENTER_CENTER, g, gfont, if selected { HEADER_SECONDARY } else { CHANNEL_DEFAULT.gamma_multiply(if muted { 0.6 } else { 1.0 }) });
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let font = if selected || unread { semi(16.0) } else { reg(16.0) };
    ui.painter()
        .with_clip_rect(Rect::from_x_y_ranges(inner.left()..=right, row.y_range()))
        .text(pos2(inner.left() + 32.0, row.center().y), Align2::LEFT_CENTER, &c.name, font, col);
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, row.center().y), mentions, None);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn voice_users(ui: &mut Ui, store: &Store, gid: Id, ch: Id) {
    let Some(g) = store.guilds.get(&gid) else { return };
    let mut users: Vec<Id> = g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, _)| *u).collect();
    users.sort();
    for u in users {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        let av = Rect::from_min_size(pos2(row.left() + 44.0, row.center().y - 12.0), vec2(24.0, 24.0));
        if let Some(user) = store.user(u) {
            avatar(ui, av, &user.avatar_url());
        }
        ui.painter().text(
            pos2(av.right() + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            store.name_in(gid, u),
            reg(14.0),
            CHANNEL_DEFAULT,
        );
    }
}

fn dms(app: &mut App, ui: &mut Ui) {
    header(ui, |ui| {
        let w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(w, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            Frame::new().fill(BG_TERTIARY).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.dm_filter)
                        .frame(Frame::NONE)
                        .hint_text(egui::RichText::new("Find or start a conversation").color(TEXT_MUTED))
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
            ui.add_space(16.0);
            let (h, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
            ui.painter().text(pos2(h.left() + 18.0, h.center().y), Align2::LEFT_CENTER, "DIRECT MESSAGES", semi(12.0), CHANNEL_DEFAULT);
            ui.add_space(4.0);
            let filter = app.dm_filter.to_lowercase();
            let store = &app.store;
            for &id in &store.dms {
                let Some(c) = store.channels.get(&id) else { continue };
                let title = channel_title(store, id);
                if !filter.is_empty() && !title.to_lowercase().contains(&filter) {
                    continue;
                }
                let r = dm_row(ui, store, c, &title, cur == Some(id));
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
        ui.painter().rect_filled(inner, CornerRadius::same(4), BG_MOD_SELECTED);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), BG_MOD_HOVER);
    }
    let unread = store.unread(c);
    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
    let ring = if selected { BG_MOD_SELECTED } else if hovered { BG_MOD_HOVER } else { BG_SECONDARY };
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
                ui.painter().text(av.center(), Align2::CENTER_CENTER, "👥", reg(15.0), Color32::WHITE);
            }
        }
    }
    let col = if selected || unread { HEADER_PRIMARY } else if hovered { INTERACTIVE_HOVER } else { CHANNEL_DEFAULT };
    let mentions = store.mentions(c.id);
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let clip = ui.painter().with_clip_rect(Rect::from_x_y_ranges(av.right()..=right, row.y_range()));
    let font = if unread { semi(16.0) } else { reg(16.0) };
    if c.kind == 3 {
        clip.text(pos2(av.right() + 12.0, row.center().y - 8.0), Align2::LEFT_CENTER, title, font, col);
        clip.text(
            pos2(av.right() + 12.0, row.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("{} Members", c.recipients.len() + 1),
            reg(12.0),
            TEXT_MUTED,
        );
    } else {
        clip.text(pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, title, font, col);
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
    let mut logout = false;
    ui.horizontal_centered(|ui| {
        let (av, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        if let Some(u) = &user {
            avatar(ui, av, &u.avatar_url());
        }
        let status = match app.conn {
            crate::net::Conn::Online => app.store.presence.get(&me).copied().unwrap_or(Status::Online),
            _ => Status::Offline,
        };
        status_dot(ui, av, status, BG_SECONDARY_ALT);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            let name = user.as_ref().map(|u| u.display().to_string()).unwrap_or_default();
            ui.add(egui::Label::new(egui::RichText::new(name).font(semi(14.0)).color(HEADER_PRIMARY)).truncate().selectable(false));
            let sub = match app.conn {
                crate::net::Conn::Online => user.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
                crate::net::Conn::Connecting => "Connecting…".into(),
                crate::net::Conn::Resuming => "Reconnecting…".into(),
                crate::net::Conn::Offline => "Offline".into(),
            };
            ui.add(egui::Label::new(egui::RichText::new(sub).font(reg(12.0)).color(HEADER_SECONDARY)).truncate().selectable(false));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let gear = icon_button(ui, "⚙", 32.0, "User Settings");
            egui::Popup::menu(&gear).show(|ui| {
                ui.set_min_width(160.0);
                if ui.add(egui::Button::new(egui::RichText::new("Log Out").color(RED))).clicked() {
                    logout = true;
                }
            });
            icon_button(ui, "🎧", 32.0, "Deafen");
            icon_button(ui, "🎤", 32.0, "Mute");
        });
    });
    if logout {
        app.logout();
    }
}
