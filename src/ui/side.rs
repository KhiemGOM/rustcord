use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar, badge, channel_title, icon_button, status_dot};
use super::icons::{self, Icon};
use super::menus::{ChanAct, GuildAct, Move, channel_menu, guild_menu};
use super::text;
use crate::net::Cmd;
use crate::state::{Channel, FRIEND, Id, Status, Store, snowflake_ms};
use crate::theme::*;
mod guildlist;
use guildlist::*;
mod dmlist;
use dmlist::*;

fn sort_key(c: &Channel) -> (bool, i32, Id) {
    (c.is_voice(), c.position, c.id)
}

/// Visible channels grouped under their categories, in sidebar order. `keep` stays in even when
/// onboarding's opt-in would leave it out, the way discord keeps the open channel.
pub fn tree(store: &Store, gid: Id, keep: Option<Id>) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
    let Some(g) = store.guilds.get(&gid) else { return Vec::new() };
    let all: Vec<&Channel> = g.channels.iter().filter_map(|c| store.channels.get(c)).collect();
    let visible: Vec<&Channel> = all
        .iter()
        .copied()
        .filter(|c| c.kind != 4 && !matches!(c.kind, 10..=12) && store.can_view(c) && (keep == Some(c.id) || store.listed(c)))
        .collect();
    let opt_in = store.opt_in.contains(&gid);
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
        if kids.is_empty() && (opt_in || !store.can_view(cat)) {
            continue;
        }
        kids.sort_by_key(|c| sort_key(c));
        out.push((Some(cat), kids));
    }
    out
}

pub fn first_channel(store: &Store, gid: Id) -> Option<Id> {
    tree(store, gid, None).into_iter().flat_map(|(_, k)| k).find(|c| c.is_text()).map(|c| c.id)
}

/// How much of the bottom the floating account card takes, its 8 margin included. The rail and
/// the sidebar both stop short of it.
pub fn foot(app: &App) -> f32 {
    66.0 + if app.voice.chan.is_some() { 102.0 } else { 0.0 } + if app.game.is_some() { 57.0 } else { 0.0 }
}

/// Discord's account card, floating 8 in from the left of the server rail, the bottom, and the
/// sidebar's right edge. A game we detected gets the top of it, then the voice controls while
/// connected.
pub fn card(app: &mut App, ui: &mut Ui, rail: Rect, side: Rect) {
    let card = Rect::from_min_max(pos2(rail.left() + 8.0, side.bottom() - foot(app)), pos2(side.right() - 8.0, side.bottom() - 8.0));
    ui.painter().rect(card, 8, pal().bg_secondary_alt, egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    let user = Rect::from_min_max(pos2(card.left(), card.bottom() - 58.0), card.max);
    let mut top = card.top();
    if app.game.is_some() {
        let area = Rect::from_min_size(card.min + vec2(1.0, 1.0), vec2(card.width() - 2.0, 56.0));
        game_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)), area);
        top = area.bottom();
    }
    if app.voice.chan.is_some() {
        ui.painter().hline(card.x_range().shrink(1.0), user.top() - 0.5, egui::Stroke::new(1.0, pal().border_muted));
        let area = Rect::from_min_max(pos2(card.left() + 1.0, top + 1.0), pos2(card.right() - 1.0, user.top() - 1.0)).shrink(12.0);
        // children that take no room from the window's layout; the chat panel comes after
        super::call::panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)));
    }
    app.rects.insert("userpanel".into(), user);
    user_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(user.shrink2(vec2(12.0, 0.0)))));
}

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::Panel::bottom("me").exact_size(foot(app)).resizable(false).show_separator_line(false).frame(Frame::NONE).show(ui, |_| {});

    match app.ui.guild {
        Some(gid) => guild(app, ui, gid),
        None => dms(app, ui),
    }
}

fn header(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    header_ruled(ui, true, add);
}

/// The header with or without its hairline; a banner runs under it without one.
fn header_ruled(ui: &mut Ui, rule: bool, add: impl FnOnce(&mut Ui)) {
    egui::Panel::top("side_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(egui::Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            if rule {
                ui.painter().hline(r.x_range().expand(16.0), r.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
            }
            ui.horizontal_centered(add);
        });
}

/// Out of onboarding's pick-your-channels list, or back into it; the unread bits of the
/// server's flags stay as they were.
fn show_all_channels(app: &mut App, gid: Id) {
    let was_in = app.store.opt_in.contains(&gid);
    if !app.store.opt_in.remove(&gid) {
        app.store.opt_in.insert(gid);
    }
    let flags = app.guild_flags(gid);
    app.guild_settings(gid, serde_json::json!({ "flags": flags }), crate::net::Tag::undo("Failed to update your settings. Please try again.", move |app: &mut App| {
        if was_in {
            app.store.opt_in.insert(gid);
        } else {
            app.store.opt_in.remove(&gid);
        }
    }));
}

pub fn toggle_hide_muted(app: &mut App, gid: Id) {
    let hide = !app.store.hide_muted.remove(&gid);
    if hide {
        app.store.hide_muted.insert(gid);
    }
    let body = serde_json::json!({ "hide_muted_channels": hide });
    app.guild_settings(gid, body, crate::net::Tag::undo("Failed to update your settings. Please try again.", move |app: &mut App| {
        if hide {
            app.store.hide_muted.remove(&gid);
        } else {
            app.store.hide_muted.insert(gid);
        }
    }));
}

/// Discord's question before a channel or category goes.
pub fn delete_channel(app: &mut App, ch: Id) {
    let Some(c) = app.store.channels.get(&ch) else { return };
    let title = if c.kind == 4 { "Delete Category" } else { "Delete Channel" };
    let name = if c.kind == 4 || c.is_voice() { c.name.clone() } else { format!("#{}", c.name) };
    let body = format!("Are you sure you want to delete **{name}**? This cannot be undone.");
    app.confirm = Some(super::Confirm::new(title, body, title, move |app: &mut App| {
        app.rest("DELETE", format!("/channels/{ch}"), None, crate::net::Tag::reason());
        app.chan_settings = None;
    }));
}

/// Asks first, like discord.
pub fn leave(app: &mut App, gid: Id) {
    let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    let body = format!("Are you sure you want to leave {name}? You won't be able to rejoin this server unless you are re-invited.");
    app.confirm = Some(super::Confirm::new(format!("Leave '{name}'"), body, "Leave Server", move |app: &mut App| {
        app.rest("DELETE", format!("/users/@me/guilds/{gid}"), Some(serde_json::json!({ "lurking": app.store.lurking.contains(&gid) })), crate::net::Tag::reason());
        app.select_guild(None);
    }));
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

/// Discord's boost goal pill: boosts against the next level, then against everything boosts can
/// unlock (levels 1-3 at 2+5+7, the perks at 3+3+3+5+3+2), or once that's covered a full bar
/// and the count. It opens the server's boost page, in the browser here.
fn boost_goal(ui: &mut Ui, boosts: u32, goal: u32, under_banner: bool) -> bool {
    ui.add_space(if under_banner { 19.0 } else { 4.0 });
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let pill = Rect::from_min_max(pos2(row.left() + 12.0, row.top()), pos2(row.right() - 13.0, row.bottom()));
    let p = ui.painter();
    p.rect_filled(pill, 13, pal().bg_tertiary);
    // the goal is every level and perk together, 33 boosts in all
    let next = (boosts < goal).then_some(goal);
    let fill = next.map_or(1.0, |n| boosts as f32 / n as f32);
    let bar = pill.shrink(2.0);
    let done = Rect::from_min_size(bar.min, vec2(bar.width() * fill, bar.height()));
    if done.width() > 0.0 {
        // discord's fill: the boost pink at .1 rising to .4, round ended, with a soft pink glow
        let rad = done.height() / 2.0;
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: 5, spread: 0, color: Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 51) }.as_shape(done, rad));
        let (a, b) = (Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 26), Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 102));
        let rx = rad.min(done.width() / 2.0);
        // a strip along the pill, its ends rounded off, coloured by how far along it is
        let mut edge: Vec<(f32, f32)> = (0..=8).map(|i| (i as f32 / 8.0) * std::f32::consts::FRAC_PI_2).map(|t| (done.left() + rx - rx * t.cos(), rad * t.sin())).collect();
        edge.extend((0..=8).map(|i| (i as f32 / 8.0) * std::f32::consts::FRAC_PI_2).map(|t| (done.right() - rx + rx * t.sin(), rad * t.cos())));
        let mut m = egui::Mesh::default();
        for (k, (x, h)) in edge.iter().enumerate() {
            let c = a.lerp_to_gamma(b, ((x - done.left()) / done.width().max(1.0)).clamp(0.0, 1.0));
            m.colored_vertex(pos2(*x, done.center().y - h), c);
            m.colored_vertex(pos2(*x, done.center().y + h), c);
            if k > 0 {
                let i = (k * 2) as u32;
                m.add_triangle(i - 2, i - 1, i);
                m.add_triangle(i - 1, i, i + 1);
            }
        }
        p.add(m);
    }
    let left = if next.is_none() { "Boost Goal 🎉".to_string() } else { "Boost Goal".to_string() };
    text::paint(p, pos2(bar.left() + 6.0, bar.center().y), Align2::LEFT_CENTER, &left, semi(12.0), pal().text_normal, 160.0);
    let caret = Rect::from_center_size(pos2(bar.right() - 12.0, bar.center().y), vec2(12.0, 12.0));
    // the count and caret are the text colour at .7
    let dim = pal().text_normal.gamma_multiply(0.7);
    icons::draw(p, Icon::ChevronRight, caret, dim);
    let count = match next {
        Some(n) => format!("{boosts}/{n} Boosts"),
        None => format!("{boosts} Boosts"),
    };
    p.text(pos2(caret.left() - 2.0, bar.center().y), Align2::RIGHT_CENTER, count, semi(12.0), dim);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(16.0), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
    ui.add_space(7.0);
    resp.clicked()
}

/// Discord's "Get Started" bar over a server guide: how many of the new member steps are done,
/// with a bar filling green, then a rule. It opens the guide, which lives in the browser here.
fn get_started(ui: &mut Ui, total: usize, done: usize) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let p = ui.painter();
    let x = r.left() + 16.0;
    let right = r.right() - 9.0;
    p.text(pos2(x, r.top() + 5.0), Align2::LEFT_TOP, "Get Started", bold(14.0), pal().header_primary);
    let arrow = Rect::from_min_size(pos2(right - 16.0, r.top() + 6.0), vec2(16.0, 16.0));
    icons::draw(p, Icon::ChevronRight, arrow, pal().text_normal);
    let n = p.text(pos2(arrow.left(), arrow.center().y), Align2::RIGHT_CENTER, total.to_string(), bold(12.0), pal().text_normal);
    let of = p.text(pos2(n.left() - 4.0, arrow.center().y), Align2::RIGHT_CENTER, "of", medium(12.0), pal().text_muted);
    p.text(pos2(of.left() - 4.0, arrow.center().y), Align2::RIGHT_CENTER, done.to_string(), bold(12.0), pal().text_normal);
    let track = Rect::from_min_max(pos2(x, r.top() + 31.0), pos2(right, r.top() + 35.0));
    p.rect_filled(track, 3, pal().border_subtle);
    let filled = Rect::from_min_size(track.min, vec2(track.width() * done as f32 / total.max(1) as f32, track.height()));
    p.rect_filled(filled, 3, pal().green);
    p.hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(1.0, pal().border_subtle));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(8.0);
    resp.clicked()
}

/// The rows over the channels, in discord's order and on its conditions. Server Boosts, Members
/// and Channels & Roles are pages in place of the chat, Events is a window, and what's left opens
/// discord's own page in the browser. Hands back the row clicked
/// and which page is open.
fn home_rows(ui: &mut Ui, g: &crate::state::Guild, perms: u64, open: Option<&str>, ev: EventsRow, ev_act: &mut Option<EventsAct>) -> Option<&'static str> {
    use crate::state::{ADMINISTRATOR, BAN_MEMBERS, CREATE_EVENTS, KICK_MEMBERS, MANAGE_EVENTS, MANAGE_GUILD, MODERATE_MEMBERS};
    let has = |f: &str| g.features.iter().any(|x| x == f);
    let community = has("COMMUNITY");
    let perms = if perms & ADMINISTRATOR != 0 { u64::MAX } else { perms };
    let mut rows: Vec<(Icon, &'static str, String)> = Vec::new();
    if has("GUILD_SERVER_GUIDE") {
        rows.push((Icon::ServerGuide, "Server Guide", String::new()));
    }
    // events only once there are some or you could make one
    if g.events > 0 || perms & (MANAGE_EVENTS | CREATE_EVENTS) != 0 {
        rows.push((Icon::Events, "Events", String::new()));
    }
    if community {
        rows.push((Icon::ChannelsRoles, if has("GUILD_ONBOARDING") { "Channels & Roles" } else { "Browse Channels" }, String::new()));
    }
    // servers selling role subscriptions get discord's shop row
    if has("ROLE_SUBSCRIPTIONS_AVAILABLE_FOR_PURCHASE") {
        rows.push((Icon::Shop, "Server Shop", String::new()));
    }
    // the member table is for moderators, in community servers or ones with discord's tools on
    let moderator = perms & (BAN_MEMBERS | KICK_MEMBERS | MODERATE_MEMBERS | MANAGE_GUILD) != 0;
    if (moderator && (community || has("ENABLED_MODERATION_EXPERIENCE_FOR_NON_COMMUNITY"))) || (moderator && has("MEMBER_VERIFICATION_MANUAL_APPROVAL")) {
        rows.push((Icon::People, "Members", String::new()));
    }
    if perms & MANAGE_GUILD != 0 {
        rows.push((Icon::Gem, "Server Boosts", String::new()));
    }
    if rows.is_empty() {
        return None;
    }
    let mut clicked = None;
    for (icon, label, url) in rows {
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let inner = row.shrink2(vec2(8.0, 0.0));
        let selected = open == Some(label);
        super::settings::mark(ui, &format!("home_row:{label}"), row);
        if selected {
            ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let is_events = label == "Events";
        let lit = is_events && ev.unread;
        let col = if selected || lit { pal().header_primary } else if is_events && ev.muted { pal().channel_muted } else { crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
        if lit && !selected {
            ui.painter().rect_filled(Rect::from_min_size(pos2(row.left(), row.center().y - 4.0), vec2(4.0, 8.0)), CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, pal().header_primary);
        }
        icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(inner.left() + 18.0, row.center().y), vec2(20.0, 20.0)), col);
        let shown = if is_events && g.events > 0 { format!("{} Event{}", g.events, if g.events == 1 { "" } else { "s" }) } else { label.to_string() };
        let room = inner.width() - 42.0 - if is_events && ev.mentions > 0 { 32.0 } else { 0.0 };
        text::paint(ui.painter(), pos2(inner.left() + 36.0, row.center().y), Align2::LEFT_CENTER, &shown, if lit { semi(16.0) } else { medium(16.0) }, col, room);
        if is_events {
            if ev.mentions > 0 {
                super::badge(ui, pos2(inner.right() - 8.0, row.center().y), ev.mentions, None);
            }
            events_menu(&resp, ev, ev_act);
        }
        if resp.clicked() {
            if url.is_empty() {
                clicked = Some(label);
            } else {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
    }
    ui.add_space(9.0);
    let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    // drawn a row under its slot so the first category's 15 lands 17 below the line
    ui.painter().hline(sep.x_range().shrink(16.0), sep.center().y + 1.0, egui::Stroke::new(1.0, pal().border_subtle));
    clicked
}

#[derive(Clone, Copy)]
struct EventsRow {
    muted: bool,
    unread: bool,
    mentions: u32,
}

enum EventsAct {
    Mute(bool),
    Read,
}

/// The Events row's right-click menu: mark its notifications read, mute new events.
fn events_menu(r: &egui::Response, ev: EventsRow, out: &mut Option<EventsAct>) {
    use super::{Trail, menu_row, menu_sep};
    egui::Popup::context_menu(r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if ui.add_enabled_ui(ev.unread || ev.mentions > 0, |ui| super::menu_item(ui, None, "Mark As Read", false)).inner {
            *out = Some(EventsAct::Read);
        }
        menu_sep(ui);
        let mute = menu_row(ui, None, "Mute New Events", None, false, Trail::Check(ev.muted));
        super::settings::mark(ui, "events_menu:mute", mute.rect);
        if mute.clicked() {
            *out = Some(EventsAct::Mute(!ev.muted));
        }
    });
}

/// A category header, and whether its create-channel plus was clicked.
fn category(ui: &mut Ui, name: &str, collapsed: bool, can_add: bool) -> (egui::Response, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 23.0), Sense::click_and_drag());
    let col = crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    let turn = (1.0 - super::motion::tween(ui.ctx(), resp.id.with("chev"), !collapsed, 0.2, super::motion::EASE_OUT)) * -std::f32::consts::FRAC_PI_2;
    let y = rect.top() + 12.0;
    // the refresh's type consolidation: sentence case at 14 medium, caret after the name
    let t = text::paint_em(ui.painter(), pos2(rect.left() + 16.0, y), Align2::LEFT_CENTER, name, medium(14.0), col, rect.width() - 60.0, 1.0);
    icons::draw_turned(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(t.right() + 9.0, y), vec2(12.0, 12.0)), col, turn);
    let mut add = false;
    if can_add {
        let pr = Rect::from_center_size(pos2(rect.right() - 24.0, y), vec2(16.0, 16.0));
        let hit = ui.interact(pr.expand(2.0), ui.id().with(("add_channel", name)), Sense::click());
        icons::draw(ui.painter(), Icon::Plus, pr, crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &hit)));
        add = hit.on_hover_text("Create Channel").clicked();
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, add)
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

fn voice_full(store: &Store, c: &Channel) -> bool {
    c.is_voice() && store.full(c) && store.guilds.get(&c.guild).and_then(|g| g.voice.get(&store.me)) != Some(&c.id)
}

fn kind_tip(store: &Store, c: &Channel) -> &'static str {
    let limited = store.limited(c);
    let age = c.nsfw;
    match c.kind {
        2 if store.perms(c) & crate::state::CONNECT == 0 => "Voice (Locked)",
        2 if age => "Voice (Age-Restricted)",
        2 if limited => "Voice (Limited)",
        2 => "Voice",
        13 if store.perms(c) & crate::state::CONNECT == 0 => "Stage (Locked)",
        13 if limited => "Stage (Limited)",
        13 => "Stage",
        5 if age => "Announcements (Age-Restricted)",
        5 if limited => "Announcements (Limited)",
        5 => "Announcements",
        15 if age => "Forum (Age-Restricted)",
        15 if limited => "Forum (Limited)",
        15 => "Forum",
        16 if age => "Media Channel (Age-Restricted)",
        16 if limited => "Media Channel (Limited)",
        16 => "Media Channel",
        12 => "Private Thread",
        10 | 11 => "Thread",
        _ if age => "Text (Age-Restricted)",
        _ if limited => "Text (Limited)",
        _ => "Text",
    }
}

pub fn glyph(c: &Channel) -> Icon {
    match c.kind {
        2 => Icon::Speaker,
        13 => Icon::Stage,
        5 => Icon::Megaphone,
        15 => Icon::Forum,
        16 => Icon::Media,
        10..=12 => Icon::Thread,
        _ => Icon::Hash,
    }
}

/// A channel in the list. Besides the row, hands back where its name's emoji went and whether
/// the invite or edit buttons were clicked.
/// What a voice channel's row adds: green while we're in it, how long its call has gone on, and a
/// second line with its status or, in our own call, discord's prompt to set one.
struct VoiceRow {
    here: bool,
    timer: Option<String>,
    sub: Option<String>,
    prompt: bool,
}

fn elapsed(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// The bools are clicks on invite, edit, "Set a channel status" and a voice channel's Open Chat.
#[allow(clippy::too_many_arguments)]
/// A list row at its design height, centred in the height the density setting gives it. Gives the
/// row, its response, and the rectangle a highlight should fill.
pub(super) fn dense_row(ui: &mut Ui, h: f32) -> (Rect, egui::Response, Rect) {
    let (slot, resp) = ui.allocate_exact_size(vec2(ui.available_width(), crate::theme::row(h)), Sense::click());
    (Rect::from_center_size(slot.center(), vec2(slot.width(), h)), resp, slot.shrink2(vec2(8.0, 0.0)))
}

fn channel_row(ui: &mut Ui, store: &Store, c: &Channel, selected: bool, unread: bool, mentions: u32, muted: bool, new: usize, voice: Option<&VoiceRow>) -> (egui::Response, Option<Rect>, bool, bool, bool, bool) {
    let sub = voice.and_then(|v| v.sub.as_ref());
    let (row, resp, hl) = dense_row(ui, if sub.is_some() { 48.0 } else { 32.0 });
    let resp = resp.interact(Sense::click_and_drag());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let y = row.top() + 16.0;
    let here = voice.is_some_and(|v| v.here);
    // not hovered(): egui unhovers the row while its own button is pressed, which would take the
    // button away mid-click and hand the click to the row
    let hovered = resp.contains_pointer();
    if selected {
        ui.painter().rect_filled(hl, CornerRadius::same(4), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(hl, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let col = if selected || here || (unread && !muted) {
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
            Rect::from_min_size(pos2(row.left(), y - 4.0), vec2(4.0, 8.0)),
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            pal().header_primary,
        );
    }
    let icon_col = if here {
        pal().green
    } else if selected {
        pal().header_primary
    } else if muted {
        pal().channel_muted
    } else {
        pal().channel_default
    };
    let ir = Rect::from_center_size(pos2(inner.left() + 18.0, y), vec2(20.0, 20.0));
    chan_icon(store, ui.painter(), c, ir, icon_col);
    let icon_hit = ui.interact(ir, ui.id().with(("kind", c.id)), Sense::hover());
    if icon_hit.hovered() {
        icon_hit.show_tooltip_text(kind_tip(store, c));
    } else if hovered && voice_full(store, c) {
        resp.show_tooltip_text("This channel is full.");
    }
    // the invite and edit buttons discord shows on the hovered or selected row; a running call
    // keeps its timer there until the row is hovered
    let timer = voice.and_then(|v| v.timer.as_ref());
    let perms = store.perms(c);
    let lit = (hovered || (selected && timer.is_none())) && !c.is_thread() && mentions == 0;
    let (mut invite, mut edit, mut chat) = (false, false, false);
    let mut right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let mut x = inner.right() - 16.0;
    let voice_room = c.is_voice();
    for (icon, tip, shown, out) in [
        (Icon::Gear, "Edit Channel", perms & crate::state::MANAGE_CHANNELS != 0, &mut edit),
        (Icon::PersonAdd, if voice_room { "Invite to Voice" } else { "Invite to Channel" }, perms & crate::state::CREATE_INVITE != 0, &mut invite),
        // a voice channel's own text chat, open beside the call
        (Icon::Bubble, "Open Chat", voice_room, &mut chat),
    ] {
        if !lit || !shown {
            continue;
        }
        let ir = Rect::from_center_size(pos2(x, y), vec2(16.0, 16.0));
        let hit = ui.interact(ir.expand(2.0), ui.id().with((tip, c.id)), Sense::click());
        super::settings::mark(ui, &format!("row_btn:{tip}:{}", c.id), ir);
        icons::draw(ui.painter(), icon, ir, crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &hit)));
        *out = hit.on_hover_text(tip).clicked();
        right = ir.left() - 6.0;
        x -= 22.0;
    }
    if new > 0 && mentions == 0 && x == inner.right() - 16.0 {
        let n = ui.painter().text(pos2(inner.right() - 8.0, y), Align2::RIGHT_CENTER, format!("{new} New"), semi(12.0), Color32::from_rgb(0x63, 0x74, 0xf4));
        right = n.left() - 6.0;
    }
    if let Some(t) = timer.filter(|_| x == inner.right() - 16.0 && mentions == 0) {
        let r = ui.painter().text(pos2(inner.right() - 8.0, y), Align2::RIGHT_CENTER, t, mono(13.0), pal().green);
        right = r.left() - 6.0;
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    let font = medium(16.0);
    let left = inner.left() + 36.0;
    let name = text::layout_em(ui.ctx(), &c.name, font, col, right - left, 1.0);
    let at = pos2(left, y - name.size().y / 2.0);
    name.paint(ui.painter(), at);
    let emoji = name.emoji_rects(at).next().map(|(_, r)| r);
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, y), mentions, None);
    }
    let mut set = false;
    if let (Some(sub), Some(v)) = (sub, voice) {
        let sy = row.top() + 36.0;
        let r = text::paint_em(ui.painter(), pos2(left, sy), Align2::LEFT_CENTER, sub, reg(14.0), pal().channel_default, inner.right() - 8.0 - left - if v.prompt { 20.0 } else { 0.0 }, 1.0);
        if v.prompt {
            let pen = Rect::from_center_size(pos2(r.right() + 10.0, sy), vec2(14.0, 14.0));
            let hit = ui.interact(r.union(pen), ui.id().with(("set_status", c.id)), Sense::click());
            icons::draw(ui.painter(), Icon::Pencil, pen, crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &hit)));
            set = hit.on_hover_text("Set a channel status").clicked();
        }
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, emoji, invite, edit, set, chat)
}

fn voice_users(ui: &mut Ui, store: &Store, voice: &crate::voice::VoiceUi, gid: Id, ch: Id, hide_names: bool) -> Option<(Id, bool, egui::Pos2)> {
    let g = store.guilds.get(&gid)?;
    let mut hit = None;
    let mut users: Vec<Id> = g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, _)| *u).collect();
    // discord lists them by name, 32 to a row, in their own display name font and server tag
    users.sort_by_cached_key(|u| store.name_in(gid, *u).to_lowercase());
    let live = voice.stage.live.contains_key(&ch);
    let before = users.len();
    if live {
        users.retain(|u| voice.stage.seats.get(u).is_none_or(|s| !s.audience));
    }
    let listeners = before - users.len();
    if hide_names {
        let per = (((ui.available_width() - 52.0) / 28.0) as usize).max(1);
        for line in users.chunks(per) {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
            for (i, u) in line.iter().enumerate() {
                let av = Rect::from_min_size(pos2(row.left() + 44.0 + i as f32 * 28.0, row.center().y - 12.0), vec2(24.0, 24.0));
                if let Some(user) = store.user(*u) {
                    avatar(ui, av, &user.avatar_url());
                }
                super::call::ring(ui, av, *u, voice.is_speaking(*u));
            }
        }
        return None;
    }
    for u in users {
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(row.shrink2(vec2(8.0, 0.0)), CornerRadius::same(4), pal().bg_mod_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() || resp.secondary_clicked() {
            hit = Some((u, resp.secondary_clicked(), resp.interact_pointer_pos().unwrap_or(row.center())));
        }
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("voice_row", u)), row));
        let av = Rect::from_min_size(pos2(row.left() + 44.0, row.center().y - 12.0), vec2(24.0, 24.0));
        if let Some(user) = store.user(u) {
            avatar(ui, av, &user.avatar_url());
        }
        let speaking = voice.is_speaking(u);
        super::call::ring(ui, av, u, speaking);
        let used = super::call::badges(ui, voice, u, row);
        // discord's usernameSpeaking: the name goes bright while they talk
        let col = if speaking { pal().interactive_hover } else { pal().channel_default };
        let room = row.right() - av.right() - 16.0 - used;
        let face = name_font(store.user(u).map_or(0, |x| x.font), 14.0);
        let r = text::paint_em(ui.painter(), pos2(av.right() + 8.0, row.center().y), Align2::LEFT_CENTER, &store.name_in(gid, u), face, col, room, 1.0);
        if let Some(user) = store.user(u) {
            let clip = Rect::from_min_max(row.min, pos2(av.right() + 8.0 + room, row.bottom()));
            super::tag_chip(&ui.painter().with_clip_rect(clip), user, pos2(r.right() + 4.0, row.center().y), 1.0);
        }
    }
    if listeners > 0 {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        ui.painter().text(pos2(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, format!("{listeners} in audience"), reg(14.0), pal().channel_default);
    }
    hit
}

/// The game's panel: its icon and name, "Not Sharing" while activity sharing is off (a click
/// there opens the setting), and the button that streams it.
fn game_panel(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(g) = app.game.clone() else { return };
    ui.painter().rect_filled(area, CornerRadius { nw: 7, ne: 7, ..Default::default() }, pal().bg_secondary);
    ui.painter().hline(area.x_range(), area.bottom() + 0.5, egui::Stroke::new(1.0, pal().border_subtle));
    app.rects.insert("game:panel".into(), area);
    let inner = area.shrink(12.0);
    let icon = Rect::from_min_size(inner.min, vec2(32.0, 32.0));
    match &g.icon {
        Some(h) => {
            egui::Image::from_uri(format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), g.id)).corner_radius(8).show_loading_spinner(false).paint_at(ui, icon);
        }
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(8), pal().bg_mod_selected);
            icons::draw(ui.painter(), Icon::Controller, Rect::from_center_size(icon.center(), vec2(20.0, 20.0)), pal().header_secondary);
        }
    }
    app.rects.insert("game:icon".into(), icon);

    let live = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let btn = Rect::from_center_size(pos2(inner.right() - 14.5, inner.center().y), vec2(32.0, 32.0));
    let resp = ui.interact(btn, ui.id().with("game_live"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(btn, CornerRadius::same(8), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let col = if live { pal().green } else { crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
    icons::draw(ui.painter(), Icon::ScreenArrow, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), col);
    app.rects.insert("game:live".into(), btn);
    let tip = match (app.voice.chan.is_some(), live) {
        (_, true) => "Stop Streaming".to_string(),
        (true, false) => format!("Stream {}", g.name),
        // discord offers its channel picker here; rustcord streams from a call
        (false, false) => "Join a voice channel to stream".to_string(),
    };
    if resp.on_hover_text(tip).clicked() && app.voice.chan.is_some() {
        if live {
            app.voice.end_stream(&app.net);
        } else {
            app.share = Some(super::share::Picker::new(ui.ctx()));
        }
    }

    // discord says nothing under the name while it's being shared
    let x = icon.right() + 8.0;
    let room = btn.left() - 8.0 - x;
    let hidden = app.store.hide_game || app.store.presence.get(&app.store.me) == Some(&Status::Offline);
    let y = if hidden { inner.top() + 8.0 } else { inner.center().y };
    text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, &g.name, medium(16.0), pal().text_normal, room);
    if hidden {
        let galley = ui.painter().layout_no_wrap("Not Sharing".into(), medium(12.0), pal().header_secondary);
        let r = Rect::from_min_size(pos2(x, inner.top() + 16.0), vec2(galley.size().x, 16.0));
        let resp = ui.interact(r, ui.id().with("game_sharing"), Sense::click());
        let col = crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp));
        ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, "Not Sharing", medium(12.0), col);
        app.rects.insert("game:not_sharing".into(), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.settings_page = Some(super::settings::Page::Activity);
        }
    }
}

fn user_panel(app: &mut App, ui: &mut Ui) {
    let me = app.store.me;
    let user = app.store.user(me).cloned();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let bg = ui.painter().add(egui::Shape::Noop);
        let (av, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        if let Some(u) = &user {
            avatar(ui, av, &u.avatar_url());
        }
        let status = match app.conn {
            crate::net::Conn::Online => app.store.presence.get(&me).copied().unwrap_or(Status::Online),
            _ => Status::Offline,
        };
        status_dot(ui, av, status, pal().bg_secondary_alt);
        // leave room for the three buttons so long names elide instead of running under them
        let col = (ui.available_width() - 146.0 - if app.settings.clips { 32.0 } else { 0.0 }).max(40.0);
        let hot = ui.rect_contains_pointer(ui.max_rect());
        let names = ui.allocate_ui_with_layout(vec2(col, 40.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(col);
            ui.set_max_width(col);
            ui.spacing_mut().item_spacing.y = -2.0;
            let name = user.as_ref().map(|u| u.display().to_string()).unwrap_or_default();
            text::label(ui, &name, medium(16.0), pal().header_primary, Sense::hover());
            let sub = match app.conn {
                crate::net::Conn::Online if app.settings.hiding_personal() => "Streamer Mode".into(),
                // discord shows the custom status, or the status itself, and the username on hover
                crate::net::Conn::Online if hot => user.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
                crate::net::Conn::Online if app.store.custom_emoji.as_ref().is_some_and(|e| e.id == 0) => {
                    format!("{} {}", app.store.custom_emoji.as_ref().map_or("", |e| e.name.as_str()), app.store.custom_status.as_deref().unwrap_or_default()).trim().to_string()
                }
                crate::net::Conn::Online if app.store.custom_emoji.as_ref().is_some_and(|e| e.id != 0) => app.store.custom_status.clone().unwrap_or_default(),
                crate::net::Conn::Online => app.store.custom_status.clone().unwrap_or_else(|| {
                    match status {
                        Status::Online => "Online",
                        Status::Idle => "Idle",
                        Status::Dnd => "Do Not Disturb",
                        Status::Offline => "Invisible",
                    }
                    .into()
                }),
                crate::net::Conn::Connecting => "Connecting…".into(),
                crate::net::Conn::Resuming => "Reconnecting…".into(),
                crate::net::Conn::Offline => "Offline".into(),
            };
            // in a call discord swaps the line for a green speaker and "In voice"
            let in_voice = matches!(app.conn, crate::net::Conn::Online) && app.voice.chan.is_some() && !hot && !app.settings.hiding_personal();
            if in_voice {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    let stage = app.voice.chan.and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.kind == 13);
                    icons::draw(ui.painter(), if stage { Icon::Stage } else { Icon::Speaker }, r, pal().green);
                    ui.add(egui::Label::new(egui::RichText::new(if stage { "In a stage" } else { "In voice" }).font(medium(12.0)).color(pal().header_secondary)).selectable(false));
                });
            } else if let Some(e) = app.store.custom_emoji.clone().filter(|e| e.id != 0 && matches!(app.conn, crate::net::Conn::Online) && !hot && !app.settings.hiding_personal()) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    egui::Image::from_uri(crate::markdown::emoji_url(e.id, e.animated)).show_loading_spinner(false).paint_at(ui, r);
                    app.rects.insert("side:status_emoji".into(), r);
                    ui.add(egui::Label::new(egui::RichText::new(sub).font(medium(12.0)).color(pal().header_secondary)).truncate().selectable(false));
                });
            } else {
                ui.add(egui::Label::new(egui::RichText::new(sub).font(medium(12.0)).color(pal().header_secondary)).truncate().selectable(false));
            }
        });
        let hit = Rect::from_x_y_ranges(av.left() - 4.0..=names.response.rect.right() + 4.0, av.center().y - 20.0..=av.center().y + 20.0);
        let resp = ui.interact(hit, ui.id().with("account"), Sense::click());
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
        if open || resp.hovered() {
            ui.painter().set(bg, egui::Shape::rect_filled(hit, CornerRadius::same(8), MENU_HOVER));
        }
        egui::Popup::menu(&resp)
            .align(egui::RectAlign::TOP_START)
            .gap(12.0)
            .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(ui.style().visuals.popup_shadow))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| super::popups::account(app, ui));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let gear = icon_button(ui, Icon::Gear, 32.0, "User Settings");
            app.rects.insert("gear".into(), gear.rect);
            if gear.clicked() {
                app.settings_page = Some(super::settings::Page::Account);
            }
            if app.settings.clips {
                super::clips::panel_button(app, ui);
            }
            super::call::user_buttons(app, ui);
        });
    });
}
