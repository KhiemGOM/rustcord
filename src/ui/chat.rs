use chrono::{Datelike, Local, TimeZone};
use egui::text::CCursor;
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Image, Key, Label, Layout, Margin, Modifiers, Pos2, Rect, RichText,
    Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};
use std::collections::HashSet;

use super::{App, Pick, avatar, channel_title, icon_button};
use super::icons::{self, Icon};
use super::{side, text};
use crate::gif;
use crate::markdown::{self, Block, Cx, Link};
use crate::net::Cmd;
use crate::state::{Channel, Comp, Embed, Id, Message, Page, ReactEmoji, Store};
use crate::theme::*;

enum Act {
    Reply(Id),
    Edit(Id),
    Delete(Id),
    React(Id, ReactEmoji, bool),
    Picker(Id, Pos2),
    Menu(Id, Pos2),
    Url(String),
    Channel(Id),
    Reveal(Id),
    SaveEdit(Id, String),
    CancelEdit,
    Profile(Id, Pos2),
    UserMenu(Id, Pos2),
    Forward(Id),
    Jump(Id),
    Component(Id, String, Option<Vec<String>>),
    View(String, String, u32, u32),
    MarkUnread(Id),
    Save(String),
}

struct D<'a> {
    store: &'a Store,
    ctx: &'a egui::Context,
    guild: Id,
    ch: Id,
    revealed: &'a HashSet<Id>,
    editing: &'a mut Option<(Id, String)>,
    marker: Option<Id>,
    hover_lock: Option<Id>,
    bar: Option<(Id, Rect)>,
    acts: Vec<Act>,
    embeds: bool,
    flash: Option<(Id, std::time::Instant)>,
    rects: Vec<(String, Rect)>,
    s: &'a crate::cache::Settings,
    quick: &'a [ReactEmoji],
    moderator: bool,
    // a copy of the message shown outside its channel (pins, search, inbox)
    popout: bool,
}

fn normal(kind: u8) -> bool {
    matches!(kind, 0 | 19 | 20 | 23)
}

fn day(ts: i64) -> (i32, u32) {
    let d = Local.timestamp_millis_opt(ts).single().unwrap_or_default();
    (d.year(), d.ordinal())
}

fn group_start(prev: Option<&Message>, m: &Message) -> bool {
    match prev {
        None => true,
        Some(p) => {
            p.author != m.author
                || m.ts() - p.ts() > 7 * 60 * 1000
                || m.reply.is_some()
                || !normal(p.kind)
                || !normal(m.kind)
                || day(p.ts()) != day(m.ts())
        }
    }
}

/// Rough height before a message has been laid out once.
pub fn estimate(m: &Message) -> f32 {
    let lines = m.content.lines().count().max(1) as f32 + m.content.len() as f32 / 90.0;
    let mut h = 22.0 * lines + 4.0;
    for a in &m.attachments {
        h += if a.is_image() { (a.h as f32).min(350.0) + 8.0 } else { 80.0 };
    }
    h += m.embeds.len() as f32 * 120.0;
    if !m.reactions.is_empty() {
        h += 30.0;
    }
    h
}

fn stamp(ts: i64) -> String {
    let Some(d) = Local.timestamp_millis_opt(ts).single() else { return String::new() };
    let now = Local::now();
    let time = d.format(markdown::hm());
    // discord's headers drop "Today at" and show just the time
    if d.date_naive() == now.date_naive() {
        time.to_string()
    } else if d.date_naive().succ_opt() == Some(now.date_naive()) {
        format!("Yesterday at {time}")
    } else {
        format!("{}, {}", d.format("%-m/%-d/%y"), d.format(markdown::hm()))
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.ui.guild.is_none() && app.friends {
        return super::friends::show(app, ui);
    }
    let Some(ch) = app.cur() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("No text channels here yet.").color(pal().text_muted));
        });
        return;
    };
    let chan = app.store.channels[&ch].clone();
    if chan.is_voice() {
        return super::call::room(app, ui, &chan);
    }
    header(app, ui, &chan);
    if chan.guild == 0 {
        super::call::dm_strip(app, ui, &chan);
    }
    let ctx = ui.ctx().clone();
    let room = ui.available_width();
    let members_w = if app.ui.members_w > 0.0 { app.ui.members_w } else { 264.0 };
    if app.search.is_some() {
        let search_w = if app.ui.search_w > 0.0 { app.ui.search_w } else { 420.0 };
        let r = egui::Panel::right("search")
            .default_size(search_w)
            .size_range(300.0..=(room - 320.0).clamp(300.0, 640.0))
            .resizable(true)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::popups::search_panel(app, ui));
        if let Some(w) = app.dragged(&ctx, r.response.rect.width(), search_w) {
            app.ui.search_w = w;
        }
    } else if chan.kind == 1 && !app.ui.hide_dm_profile && room >= 320.0 + 420.0 {
        if let Some(&uid) = chan.recipients.first() {
            egui::Panel::right("dm_profile")
                .exact_size(320.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(pal().bg_primary).inner_margin(8))
                .show(ui, |ui| super::popups::dm_profile(app, ui, uid));
        }
    } else if chan.kind == 3 && !app.ui.hide_members && room >= members_w + 420.0 {
        egui::Panel::right("group_members")
            .exact_size(members_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::members::group(app, ui, &chan));
    } else if chan.guild != 0 && !app.ui.hide_members && room >= members_w + 420.0 {
        let r = egui::Panel::right("members")
            .default_size(members_w)
            .size_range(200.0..=(room - 420.0).clamp(200.0, 480.0))
            .resizable(true)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::members::show(app, ui, chan.guild));
        if let Some(w) = app.dragged(&ctx, r.response.rect.width(), members_w) {
            app.ui.members_w = w;
        }
    }
    // a channel opened from elsewhere in a server that only lists the ones you picked
    if chan.guild != 0 && !chan.is_thread() && !app.store.listed(&chan) {
        egui::Panel::top("unlisted")
            .exact_size(49.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_secondary).inner_margin(Margin { left: 28, right: 16, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("This channel is not on your channel list").font(reg(14.0)).color(pal().text_normal));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let g = ui.painter().layout_no_wrap("Add to Channel List".into(), medium(14.0), Color32::WHITE);
                        let (b, resp) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 32.0), Sense::click());
                        app.rects.insert("add_to_list".into(), b);
                        ui.painter().rect_filled(b, 8, if resp.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE });
                        icons::draw(ui.painter(), Icon::Plus, Rect::from_min_size(pos2(b.left() + 12.0, b.center().y - 8.0), vec2(16.0, 16.0)), Color32::WHITE);
                        ui.painter().galley(pos2(b.left() + 32.0, b.center().y - g.size().y / 2.0), g, Color32::WHITE);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            app.opt_in(chan.id);
                        }
                    });
                });
            });
    }
    if chan.is_forum() {
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| super::forum::show(app, ui, &chan));
        return;
    }
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::Panel::bottom("composer")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 0 }))
            .show(ui, |ui| super::composer::show(app, ui, &chan));
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| messages(app, ui, &chan));
    });
}

fn header(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("chat_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom() - 0.5, Stroke::new(1.0, pal().border_subtle));
            ui.horizontal_centered(|ui| {
                let title = channel_title(&app.store, chan.id);
                let glyph = |ui: &mut Ui, icon: Icon| {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    icons::draw(ui.painter(), icon, r, pal().channel_default);
                };
                let chan_glyph = |ui: &mut Ui, store: &crate::state::Store, c: &Channel| {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    side::chan_icon(store, ui.painter(), c, r, pal().channel_default);
                    r
                };
                match chan.kind {
                    1 => {
                        if let Some(u) = chan.recipients.first().and_then(|u| app.store.user(*u)) {
                            let (av, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                            avatar(ui, av, &u.avatar_url());
                        }
                    }
                    3 => glyph(ui, Icon::People),
                    10..=12 => {
                        let parent = app.store.channels.get(&chan.parent).cloned();
                        if let Some(p) = parent {
                            chan_glyph(ui, &app.store, &p);
                            let r = text::label(ui, &p.name, semi(16.0), pal().header_secondary, Sense::click());
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                app.open(p.id);
                            }
                            let (r, _) = ui.allocate_exact_size(vec2(16.0, 24.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::ChevronRight, r, pal().channel_default);
                        }
                        glyph(ui, Icon::Thread);
                    }
                    _ => {
                        let r = chan_glyph(ui, &app.store, chan);
                        app.rects.insert("header:icon".into(), r);
                    }
                }
                let tools = if chan.guild != 0 { 420.0 } else { 300.0 };
                // a narrow window elides the name rather than running it under the buttons
                let w = (ui.available_width() - tools).max(24.0);
                ui.allocate_ui_with_layout(vec2(w, 24.0), Layout::left_to_right(Align::Center), |ui| {
                    text::label(ui, &title, semi(16.0), pal().header_primary, Sense::hover());
                });
                if let Some(topic) = &chan.topic {
                    let (sep, _) = ui.allocate_exact_size(vec2(17.0, 24.0), Sense::hover());
                    ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, pal().bg_accent));
                    let w = (ui.available_width() - tools).max(0.0);
                    ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                        text::label(ui, &topic.replace('\n', " "), reg(14.0), pal().header_secondary, Sense::hover());
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let qid = egui::Id::new("search_q");
                    let mut q: String = ui.data(|d| d.get_temp(qid).unwrap_or_default());
                    // discord's 244x32 box, placeholder naming the server
                    let sw = 244.0;
                    let hint = match app.store.guilds.get(&chan.guild) {
                        Some(g) => format!("Search {}", g.name.split_whitespace().collect::<Vec<_>>().join(" ")),
                        None => "Search".to_string(),
                    };
                    let mut clear = false;
                    let r = ui
                        .allocate_ui_with_layout(vec2(sw, 32.0), Layout::left_to_right(Align::Center), |ui| {
                            Frame::new()
                                .fill(pal().input_bg)
                                .stroke(Stroke::new(1.0, pal().border_subtle))
                                .corner_radius(8)
                                .inner_margin(Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut q)
                                            .frame(Frame::NONE)
                                            .font(medium(14.0))
                                            .hint_text(RichText::new(hint).color(pal().text_muted))
                                            .desired_width(sw - 42.0),
                                    );
                                    // the magnifier turns into a clear button once there's a query
                                    let (ir, x) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                                    if q.is_empty() {
                                        icons::draw(ui.painter(), Icon::Search, ir, pal().header_secondary);
                                    } else {
                                        icons::draw(ui.painter(), Icon::Close, ir, if x.hovered() { pal().interactive_hover } else { pal().header_secondary });
                                        clear = x.clicked();
                                    }
                                    r
                                })
                                .inner
                        })
                        .inner;
                    app.rects.insert("search".into(), r.rect);
                    if clear {
                        q.clear();
                        app.search = None;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !q.trim().is_empty() {
                        app.start_search(chan.id, q.trim().to_string());
                    }
                    if let Some(past) = super::popups::search_options(app, ui.ctx(), &r, &mut q, chan.guild) {
                        app.start_search(chan.id, past.clone());
                        q = past;
                    }
                    ui.data_mut(|d| d.insert_temp(qid, q));
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if chan.kind == 1 {
                        let tip = if app.ui.hide_dm_profile { "Show User Profile" } else { "Hide User Profile" };
                        if icon_button(ui, Icon::UserProfile, 32.0, tip).clicked() {
                            app.ui.hide_dm_profile = !app.ui.hide_dm_profile;
                            app.search = None;
                        }
                    }
                    if chan.guild != 0 || chan.kind == 3 {
                        let tip = if app.ui.hide_members { "Show Member List" } else { "Hide Member List" };
                        if icon_button(ui, Icon::People, 32.0, tip).clicked() {
                            app.ui.hide_members = !app.ui.hide_members;
                            app.search = None;
                        }
                    }
                    if !chan.is_forum() {
                        let pins = icon_button(ui, Icon::Pin, 32.0, "Pinned Messages");
                        app.rects.insert("pins_button".into(), pins.rect);
                        if pins.clicked() {
                            match app.pins {
                                Some(_) => app.pins = None,
                                None => app.open_pins(chan.id),
                            }
                        }
                    }
                    if chan.guild != 0 {
                        bell(app, ui, chan);
                    }
                    if chan.guild != 0 && matches!(chan.kind, 0 | 5) {
                        let t = icon_button(ui, Icon::Thread, 32.0, "Threads");
                        app.rects.insert("threads_button".into(), t.rect);
                        super::popups::threads_menu(app, &t, chan.id);
                    }
                    if chan.guild == 0 {
                        super::call::call_button(app, ui, chan.id);
                    }
                });
            });
        });
}

/// The header's notification bell: Discord's menu to mute the channel or pick its level.
fn bell(app: &mut App, ui: &mut Ui, chan: &Channel) {
    use super::{Trail, menu_item, menu_row, menu_sep};
    let muted = app.store.muted.contains(&chan.id);
    let level = app.store.levels.get(&chan.id).copied();
    let inherited = app.store.inherited_level(chan);
    // discord slashes the bell whenever not every message notifies
    let quiet = app.store.is_muted(chan) || level.unwrap_or(inherited) != 0;
    let b = icon_button(ui, if quiet { Icon::BellOff } else { Icon::Bell }, 32.0, "Notification Settings");
    app.rects.insert("bell".into(), b.rect);
    let mut pick: Option<Option<u8>> = None;
    let mut mute = None;
    egui::Popup::menu(&b).gap(4.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if muted {
            if menu_item(ui, None, "Unmute Channel", false) {
                mute = Some((false, None));
            }
        } else {
            let m = menu_row(ui, None, "Mute Channel", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                if let Some(secs) = super::mute_menu(ui) {
                    mute = Some((true, secs));
                }
            });
        }
        menu_sep(ui);
        let default = if chan.parent != 0 && !chan.is_thread() { "Use Category Default" } else { "Use Server Default" };
        pick = super::level_menu(ui, level, inherited, default);
    });
    if let Some((m, secs)) = mute {
        app.mute_for(chan.id, false, m, secs);
    }
    if let Some(v) = pick {
        app.set_level(chan.id, v);
    }
}

fn messages(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let ch = chan.id;
    let mut sa = egui::ScrollArea::vertical()
        .id_salt(("chat", ch))
        .stick_to_bottom(true)
        .auto_shrink([false, false]);
    let mut to_bottom = false;
    // a scroll by delta, for when egui's stick-to-bottom would undo a set offset
    let mut nudge = 0.0;
    let fixed = app.scroll_fix.is_some_and(|(c, _)| c == ch);
    if let Some((_, off)) = app.scroll_fix.take_if(|(c, _)| *c == ch) {
        // f32::MAX means "the bottom", which only the content itself knows
        if off == f32::MAX {
            to_bottom = true;
        } else if app.pinned.contains(&ch) {
            nudge = app.offsets.get(&ch).copied().unwrap_or(0.0) - off;
        } else {
            sa = sa.vertical_scroll_offset(off);
        }
    }

    let pointer = ui.ctx().pointer_hover_pos();
    let hover_lock = app.bar.and_then(|(id, r)| pointer.filter(|p| r.contains(*p)).map(|_| id));
    // the hover toolbar floats on its own layer; a wheel over it still scrolls the chat, like discord
    let over_bar = pointer.and_then(|p| ui.ctx().layer_id_at(p)) == Some(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("msg_bar")));
    if (hover_lock.is_some() || over_bar) && !fixed {
        nudge += ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta.y));
    }
    let marker = app.marker.filter(|(c, _)| *c == ch).map(|(_, m)| m);
    let quick = app.quick_reactions();
    let ctx = ui.ctx().clone();
    let mut d = D {
        store: &app.store,
        ctx: &ctx,
        guild: chan.guild,
        ch,
        revealed: &app.revealed,
        editing: &mut app.editing,
        marker,
        hover_lock,
        bar: None,
        acts: Vec::new(),
        embeds: app.settings.embeds,
        flash: app.flash.filter(|(_, t)| t.elapsed().as_secs_f32() < 2.0),
        rects: Vec::new(),
        s: &app.settings,
        quick: &quick,
        moderator: app.store.perms(chan) & crate::state::MANAGE_MESSAGES != 0,
        popout: false,
    };
    if d.flash.is_some() {
        ctx.request_repaint();
    }
    let heights = &mut app.heights;
    let empty = Vec::new();
    let entry = d.store.msgs.get(&ch);
    let list = entry.map(|e| &e.list).unwrap_or(&empty);
    let at_start = entry.is_some_and(|e| e.at_start);
    let loading = entry.is_some_and(|e| e.loading);

    let mut anchor = 0.0f32;
    let target = app.scroll_to.filter(|(c, _)| *c == ch).map(|(_, id)| id);
    let mut target_y = None;
    // like discord, a channel too short to scroll sits on the composer, not under the header;
    // last frame's height says how much room to leave above it
    let fill_id = egui::Id::new(("chat_fill", ch));
    let natural: f32 = ctx.data(|d| d.get_temp(fill_id)).unwrap_or(f32::MAX);
    let out = sa.show_viewport(ui, |ui, vp| {
        if nudge != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, nudge), egui::style::ScrollAnimation::none());
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let fill = (vp.height() - natural).max(0.0);
        ui.add_space(fill);
        let origin = ui.max_rect().top();
        let w = ui.available_width();
        if at_start {
            welcome(ui, d.store, chan);
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
            if loading || list.is_empty() {
                egui::Spinner::new().size(20.0).color(pal().text_muted).paint_at(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)));
            }
        }
        let mut prev: Option<&Message> = None;
        for m in list {
            let y = ui.cursor().top() - origin;
            let start = group_start(prev, m);
            let known = heights.get(&(m.id, start)).copied();
            let h = known.unwrap_or_else(|| estimate(m));
            if target == Some(m.id) {
                target_y = Some(y);
            }
            if y + h < vp.top() - 400.0 || y > vp.bottom() + 400.0 {
                ui.allocate_space(vec2(w, h));
            } else {
                let top = ui.cursor().top();
                message(ui, &mut d, m, prev, start);
                let real = ui.cursor().top() - top;
                if known.is_none_or(|k| (k - real).abs() > 0.5) {
                    if y + real <= vp.top() {
                        anchor += real - h;
                    }
                    heights.insert((m.id, start), real);
                }
            }
            prev = Some(m);
        }
        // discord's spacer: room for the typing line that sits over the bottom of the list
        ui.add_space(24.0);
        let grown = ui.cursor().top() - origin - fill;
        ctx.data_mut(|d| d.insert_temp(fill_id, grown));
        if to_bottom {
            // instant: images still loading above would bend a smooth scroll through the anchor fix
            ui.scroll_to_cursor_animation(Some(Align::BOTTOM), egui::style::ScrollAnimation::none());
        }
    });

    let D { acts, bar, rects, .. } = d;
    app.bar = bar;
    app.rects.extend(rects);
    let offset = out.state.offset.y;
    app.offsets.insert(ch, offset);
    let at_bottom = offset + out.inner_rect.height() >= out.content_size.y - 4.0;
    // a message arriving grows the content in the same frame something above may change height;
    // if the list was at the bottom it should stay there, not hold the old spot
    let was_bottom = to_bottom || app.pinned.contains(&ch);
    if at_bottom {
        app.pinned.insert(ch);
    } else {
        app.pinned.remove(&ch);
    }
    if anchor.abs() > 0.5 && !at_bottom && !was_bottom {
        app.scroll_fix = Some((ch, offset + anchor));
        ui.ctx().request_discard("scroll anchor");
    }

    if offset < 800.0
        && let Some(e) = app.store.msgs.get_mut(&ch)
        && !e.at_start
        && !e.loading
        && e.fresh
        && let Some(oldest) = e.list.iter().find(|m| !m.pending).map(|m| m.id)
    {
        e.loading = true;
        app.net.send(Cmd::Messages { channel: ch, page: Page::Before(oldest) });
    }
    let near_end = offset + out.inner_rect.height() > out.content_size.y - 800.0;
    if near_end
        && let Some(e) = app.store.msgs.get_mut(&ch)
        && e.detached
        && !e.loading
        && let Some(newest) = e.list.iter().rev().find(|m| !m.pending).map(|m| m.id)
    {
        e.loading = true;
        app.net.send(Cmd::Messages { channel: ch, page: Page::After(newest) });
    }

    if let Some((_, id)) = app.scroll_to.filter(|(c, _)| *c == ch)
        && let Some(ty) = target_y
    {
        let want = (ty - 80.0).max(0.0);
        if (offset - want).abs() > 2.0 && app.scroll_tries < 8 {
            app.scroll_tries += 1;
            app.scroll_fix = Some((ch, want));
            ui.ctx().request_discard("jump");
        } else {
            app.scroll_to = None;
            app.flash = Some((id, std::time::Instant::now()));
        }
    }
    // scrolling down to the newest message reads it: the bar goes, the red line stays
    let bottom = offset + out.inner_rect.height() >= out.content_size.y - 4.0;
    let wheel_down = ui.input(|i| i.raw.events.iter().any(|e| matches!(e, egui::Event::MouseWheel { delta, .. } if delta.y < 0.0)));
    if bottom && wheel_down && pointer.is_some_and(|p| out.inner_rect.contains(p)) {
        app.marker_bar = false;
    }
    bars(app, ui, ch, out.inner_rect, offset, out.content_size.y);

    for a in acts {
        match a {
            Act::Reply(id) => {
                app.reply = Some(id);
                app.focus_composer = true;
            }
            Act::Edit(id) => {
                if let Some(m) = app.store.find_msg(ch, id) {
                    app.editing = Some((id, m.content.clone()));
                }
            }
            Act::Delete(id) => {
                if ui.input(|i| i.modifiers.shift) {
                    app.net.send(Cmd::Delete { channel: ch, id });
                } else {
                    app.confirm_delete = Some((ch, id));
                }
            }
            Act::React(id, em, add) => app.react(ch, id, em, add),
            Act::Picker(id, at) => app.picker = Some((Pick::React(ch, id), at)),
            Act::Menu(id, at) => app.menu = Some((ch, id, at)),
            Act::Url(u) => {
                let _ = open::that_detached(u);
            }
            Act::Channel(c) => app.open(c),
            Act::Reveal(id) => {
                app.revealed.insert(id);
            }
            Act::SaveEdit(id, text) => {
                app.editing = None;
                let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
                let text = app.convert_emoji(guild, text.trim());
                if text.is_empty() {
                    app.confirm_delete = Some((ch, id));
                } else if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                    if m.content != text {
                        m.content = text.clone();
                        m.edited = true;
                        app.net.send(Cmd::Edit { channel: ch, id, content: text });
                    }
                }
                app.focus_composer = true;
            }
            Act::CancelEdit => {
                app.editing = None;
                app.focus_composer = true;
            }
            Act::Profile(u, at) => app.open_profile(u, chan.guild, at),
            Act::UserMenu(u, at) => app.user_menu = Some((u, chan.guild, None, at)),
            Act::Forward(id) => app.forward = Some(super::popups::Forward::new(ch, id)),
            Act::Jump(id) => app.jump_to(ch, id),
            Act::Component(id, custom_id, values) => app.press(ch, id, &custom_id, values),
            Act::View(img, open, w, h) => app.viewer = Some((img, open, w, h)),
            Act::MarkUnread(id) => app.mark_unread(ch, id),
            Act::Save(url) => {
                let name = url.split('?').next().unwrap_or("").rsplit('/').next().unwrap_or("image.png").to_string();
                if let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                    app.net.send(Cmd::Download { url, path });
                }
            }
        }
    }
}

/// The "N new messages" strip on top and the "Jump To Present" strip at the bottom.
fn bars(app: &mut App, ui: &mut Ui, ch: Id, area: Rect, offset: f32, content_h: f32) {
    let detached = app.store.msgs.get(&ch).is_some_and(|e| e.detached);
    if let Some(after) = app.marker.filter(|(c, _)| *c == ch && app.marker_bar).map(|(_, m)| m) {
        let list = app.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default();
        let n = list.iter().filter(|m| m.id > after && !m.pending).count();
        let first = list.iter().find(|m| m.id > after).map(|m| m.id);
        if n > 0 {
            let r = Rect::from_min_size(area.min + vec2(16.0, 0.0), vec2(area.width() - 32.0, 24.0));
            let p = ui.painter();
            p.rect_filled(r, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, Color32::from_rgb(0x58, 0x65, 0xf2));
            let since = Local
                .timestamp_millis_opt(crate::state::snowflake_ms(after))
                .single()
                .map(|t| t.format(markdown::hm()).to_string())
                .unwrap_or_default();
            let more = if list.first().is_some_and(|m| m.id > after) { "+" } else { "" };
            p.text(
                pos2(r.left() + 12.0, r.center().y),
                Align2::LEFT_CENTER,
                format!("{n}{more} new message{} since {since}", if n == 1 { "" } else { "s" }),
                semi(14.0),
                Color32::WHITE,
            );
            let mr = Rect::from_min_max(pos2(r.right() - 130.0, r.top()), r.max);
            p.text(pos2(mr.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, "Mark As Read", semi(14.0), Color32::WHITE);
            let left = Rect::from_min_max(r.min, pos2(mr.left(), r.bottom()));
            if ui.interact(mr, egui::Id::new("mark_read_bar"), Sense::click()).clicked() {
                app.marker = None;
            } else if ui.interact(left, egui::Id::new("jump_new_bar"), Sense::click()).clicked()
                && let Some(f) = first
            {
                app.jump_to(ch, f);
            }
        }
    }
    let scrolled_up = offset + area.height() < content_h - 1500.0;
    if detached || scrolled_up {
        let r = Rect::from_min_size(pos2(area.left() + 16.0, area.bottom() - 32.0), vec2(area.width() - 32.0, 32.0));
        let p = ui.painter();
        p.rect_filled(r, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, pal().bg_secondary_alt);
        p.text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "You're viewing older messages", semi(14.0), pal().header_secondary);
        let jr = Rect::from_min_max(pos2(r.right() - 150.0, r.top()), r.max);
        let resp = ui.interact(jr, egui::Id::new("jump_present"), Sense::click());
        let col = if resp.hovered() { Color32::WHITE } else { pal().header_primary };
        p.text(pos2(jr.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, "Jump To Present", semi(14.0), col);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.jump_to_present(ch);
        }
    }
}

fn welcome(ui: &mut Ui, store: &Store, chan: &Channel) {
    ui.add_space(16.0);
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
        let title = channel_title(store, chan.id);
        if chan.guild == 0 {
            if let Some(u) = chan.recipients.first().and_then(|u| store.user(*u)) {
                let (r, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
                avatar(ui, r, &u.avatar_url());
                ui.add_space(8.0);
                ui.label(RichText::new(&title).font(semi(32.0)).color(pal().header_primary));
                ui.label(RichText::new(&u.username).font(reg(20.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("This is the beginning of your direct message history with {title}."))
                        .color(pal().header_secondary),
                );
            }
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(68.0, 68.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 34.0, pal().bg_accent);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "#", reg(44.0), Color32::WHITE);
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Welcome to #{title}!")).font(semi(32.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("This is the start of the #{title} channel.")).color(pal().header_secondary));
        }
    });
}

fn divider(ui: &mut Ui, text: &str, color: Color32, pill: bool) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
    let y = r.center().y;
    let left = r.left() + 16.0;
    let right = r.right() - 14.0;
    if pill {
        ui.painter().hline(left..=right, y, Stroke::new(1.0, color));
        let g = ui.painter().layout_no_wrap("NEW".into(), bold(10.0), Color32::WHITE);
        let pr = Rect::from_min_size(pos2(right - g.size().x - 8.0, y - 7.0), vec2(g.size().x + 8.0, 14.0));
        ui.painter().rect_filled(pr, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, color);
        ui.painter().galley(pr.center() - g.size() / 2.0, g, Color32::WHITE);
        return;
    }
    let g = ui.painter().layout_no_wrap(text.to_string(), semi(12.0), pal().text_muted);
    let gw = g.size().x + 8.0;
    let mid = r.center().x;
    ui.painter().hline(left..=mid - gw / 2.0, y, Stroke::new(1.0, color));
    ui.painter().hline(mid + gw / 2.0..=right, y, Stroke::new(1.0, color));
    ui.painter().galley(pos2(mid - g.size().x / 2.0, y - g.size().y / 2.0), g, pal().text_muted);
}

fn message(ui: &mut Ui, d: &mut D, m: &Message, prev: Option<&Message>, start: bool) {
    if let Some(p) = prev
        && day(p.ts()) != day(m.ts())
    {
        ui.add_space(8.0);
        let label = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format("%B %-d, %Y").to_string()).unwrap_or_default();
        divider(ui, &label, pal().bg_accent, false);
    }
    if let Some(after) = d.marker
        && prev.is_some_and(|p| p.id <= after)
        && m.id > after
    {
        divider(ui, "", RED, true);
    }
    let compact = d.s.compact;
    let font = d.s.font_size;
    if start {
        ui.add_space((d.s.group_space - 2.0).max(0.0));
    }

    let left = ui.max_rect().left();
    let w = ui.available_width();
    let top = ui.cursor().top();
    let bg = ui.painter().add(egui::Shape::Noop);
    let content = Rect::from_min_size(pos2(left + 72.0, top + 2.0), vec2((w - 72.0 - 48.0).max(120.0), 100_000.0));
    let mut cui = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    cui.spacing_mut().item_spacing = vec2(4.0, 0.0);

    let me = d.store.me;
    // last frame's row decides whether the pointer is over this message yet
    let row_id = egui::Id::new(("row", m.id));
    let hot = ui.data(|s| s.get_temp::<Rect>(row_id)).is_some_and(|r| ui.rect_contains_pointer(r));
    let mut head_top = top + 2.0;
    if !normal(m.kind) {
        system_line(&mut cui, d, m);
    } else {
        if let Some(r) = m.reply {
            reply_line(&mut cui, d, r, left);
            head_top = cui.cursor().top();
        }
        let name = d.store.name_in(d.guild, m.author);
        let (ink, dot) = name_ink(d, m.author);
        let color = ink.stops[0];
        if start && !compact {
            author_line(&mut cui, d, m, &name, &ink, dot, font, hot);
        }
        let editing = d.editing.as_ref().is_some_and(|(id, _)| *id == m.id);
        if editing {
            edit_box(&mut cui, d, m.id);
        } else if !m.content.is_empty() && !(d.embeds && d.s.inline_links && link_only(m)) {
            let color = if m.failed {
                RED
            } else if m.pending {
                pal().text_muted
            } else {
                pal().text_normal
            };
            let revealed = d.revealed.contains(&m.id)
                || d.s.spoilers == crate::cache::Spoilers::Always
                || (d.s.spoilers == crate::cache::Spoilers::Moderated && d.moderator);
            // with no text the marker goes under the embeds and buttons instead
            let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: font, color, revealed, edited: m.edited && !m.content.trim().is_empty() };
            let mut blocks = markdown::parse(&m.content, &cx);
            if compact {
                compact_prefix(d, m, &mut blocks, &name, color, font);
                if !matches!(blocks.first(), Some(Block::Text(_))) {
                    compact_name_line(&mut cui, d, m, &name, color, font);
                }
            }
            blocks_ui(&mut cui, &blocks, d, m.id);
        } else if compact {
            compact_name_line(&mut cui, d, m, &name, color, font);
        }
        for a in &m.attachments {
            cui.add_space(4.0);
            if a.is_image() && d.s.inline_uploads {
                let sep = if a.proxy.contains('?') { '&' } else { '?' };
                let (w, h) = fit(a.w, a.h, 550.0, 350.0, cui.available_width());
                let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                let r = media(&mut cui, d, &url, &a.url, w, h, None);
                if cui.clip_rect().contains(r.rect.center()) {
                    d.rects.push(("image".into(), r.rect.intersect(cui.clip_rect())));
                }
                if r.clicked() {
                    d.acts.push(Act::View(a.proxy.clone(), a.url.clone(), a.w, a.h));
                }
            } else if file_card(&mut cui, &a.filename, a.size, a.ctype.starts_with("video/")).clicked() {
                d.acts.push(Act::Url(a.url.clone()));
            }
        }
        for (id, name, fmt) in &m.stickers {
            cui.add_space(4.0);
            if *fmt == 3 {
                cui.label(RichText::new(format!("[sticker: {name}]")).color(pal().text_muted));
            } else {
                let ext = if *fmt == 4 { "gif" } else { "png" };
                let url = format!("https://media.discordapp.net/stickers/{id}.{ext}?size=160");
                media(&mut cui, d, &url, &url, 160.0, 160.0, None).on_hover_text(name);
            }
        }
        let embeds = if d.embeds { m.embeds.as_slice() } else { &[] };
        for e in embeds {
            cui.add_space(4.0);
            embed(&mut cui, d, e, m.id);
        }
        if let Some(t) = m.thread.and_then(|t| d.store.channels.get(&t)) {
            cui.add_space(4.0);
            let r = Frame::new()
                .fill(pal().bg_secondary)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(12, 8))
                .show(&mut cui, |ui| {
                    ui.set_max_width(400.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&t.name).font(semi(14.0)).color(pal().header_primary));
                        ui.label(RichText::new(format!("{} Messages ›", t.msg_count)).font(semi(14.0)).color(pal().link));
                    });
                })
                .response
                .interact(Sense::click());
            if r.hovered() {
                cui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                d.acts.push(Act::Channel(t.id));
            }
        }
        if !m.components.is_empty() {
            cui.add_space(4.0);
            components(&mut cui, d, m);
        }
        if m.edited && m.content.trim().is_empty() {
            cui.add_space(2.0);
            cui.label(RichText::new("(edited)").font(reg(10.0)).color(pal().text_muted));
        }
        if !m.reactions.is_empty() && d.s.reactions {
            cui.add_space(4.0);
            reactions(&mut cui, d, m);
        }
    }

    let bottom = cui.min_rect().bottom().max(top + 22.0) + 2.0;
    let row = Rect::from_min_max(pos2(left, top), pos2(left + w, bottom));
    ui.advance_cursor_after_rect(row);
    ui.data_mut(|s| s.insert_temp(row_id, row));
    if ui.clip_rect().contains_rect(row) && normal(m.kind) && !m.pending {
        d.rects.push(("msg:last".into(), row));
    }

    let hovered = match d.hover_lock {
        Some(id) => id == m.id,
        None => ui.rect_contains_pointer(row),
    };
    let fill = if d.store.mentions_me(m) && m.author != me {
        ui.painter().rect_filled(
            Rect::from_min_size(row.min, vec2(2.0, row.height())),
            CornerRadius::ZERO,
            YELLOW,
        );
        if hovered { Color32::from_rgba_premultiplied(30, 23, 8, 36) } else { pal().highlight_bg }
    } else if let Some((_, t)) = d.flash.filter(|(id, _)| *id == m.id) {
        // jumped-to message glows and fades out over two seconds
        let a = (1.0 - t.elapsed().as_secs_f32() / 2.0).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(88, 101, 242, (40.0 * a) as u8)
    } else if hovered || d.editing.as_ref().is_some_and(|(id, _)| *id == m.id) {
        pal().bg_msg_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().set(bg, egui::epaint::RectShape::filled(row, CornerRadius::ZERO, fill));

    if compact && normal(m.kind) {
        // compact mode keeps the time in the gutter on every line
        let t = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format(markdown::hm()).to_string()).unwrap_or_default();
        ui.painter().text(pos2(left + 64.0, head_top + font * 0.69), Align2::RIGHT_CENTER, t, medium((11.0 * font / 16.0).max(10.0)), pal().text_muted);
    } else if start && normal(m.kind) {
        if let Some(u) = d.store.user(m.author) {
            let ar = Rect::from_min_size(pos2(left + 16.0, head_top + 2.0), vec2(40.0, 40.0));
            avatar(ui, ar, &u.avatar_url());
            if ui.clip_rect().contains_rect(ar) {
                d.rects.push((format!("avatar:{}", m.author), ar));
            }
            let r = ui.interact(ar, egui::Id::new(("avatar", m.id)), Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                d.acts.push(Act::Profile(m.author, ar.right_top() + vec2(8.0, 0.0)));
            }
            if r.secondary_clicked() {
                d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(ar.center())));
            }
        }
    } else if hovered && normal(m.kind) {
        let t = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format(markdown::hm()).to_string()).unwrap_or_default();
        ui.painter().text(pos2(left + 56.0, top + 13.0), Align2::RIGHT_CENTER, t, medium(11.0), pal().text_muted);
    }

    if hovered && !m.pending && !m.failed {
        // alt+click marks unread from this message on, a long-standing discord shortcut
        if ui.input(|i| i.pointer.primary_clicked() && i.modifiers.alt) {
            d.acts.push(Act::MarkUnread(m.id));
        }
        action_bar(d, m, row);
        if ui.input(|i| i.pointer.secondary_clicked())
            && !d.acts.iter().any(|a| matches!(a, Act::UserMenu(..)))
            && let Some(p) = ui.ctx().pointer_interact_pos()
        {
            d.acts.push(Act::Menu(m.id, p));
        }
    }
}

/// What a name is painted with. A role gradient tiles every 100px and drifts while
/// the message is hovered, glowing as it goes; a nitro display style stretches its
/// colours over the whole name.
struct Ink {
    stops: Vec<Color32>,
    tile: bool,
    glow: bool,
}

impl Ink {
    fn flat(c: Color32) -> Self {
        Ink { stops: vec![c], tile: false, glow: false }
    }

    fn at(&self, w: f32, phase: f32) -> impl Fn(f32) -> Color32 + '_ {
        move |x| {
            let t = if self.tile { (x - phase).rem_euclid(100.0) / 100.0 } else { (x / w - 0.1) / 0.8 };
            ramp(&self.stops, t)
        }
    }

    fn layout(&self, ctx: &egui::Context, name: &str, font: egui::FontId, max_w: f32, hot: bool) -> text::Rich {
        if self.stops.len() < 2 {
            return text::layout(ctx, name, font, self.stops[0], max_w);
        }
        let rich = text::layout(ctx, name, font, Color32::WHITE, max_w);
        let phase = if hot && self.tile { ctx.input(|i| i.time) as f32 * 100.0 / 1.5 } else { 0.0 };
        if phase > 0.0 {
            ctx.request_repaint();
        }
        let w = rich.size().x.max(1.0);
        rich.tint(self.at(w, phase))
    }

    fn paint(&self, p: &egui::Painter, rich: &text::Rich, pos: Pos2, hot: bool) {
        if self.glow || (self.tile && hot) {
            // css blurs a copy of the name by 4px; a few faint rings of it look about the same
            let phase = if hot && self.tile { p.ctx().input(|i| i.time) as f32 * 100.0 / 1.5 } else { 0.0 };
            let w = rich.size().x.max(1.0);
            let at = self.at(w, phase);
            for (r, a) in [(1.5, 0.03), (3.0, 0.025), (4.5, 0.018), (6.0, 0.012)] {
                let tinted = rich.clone().tint(|x| at(x).gamma_multiply(a));
                for k in 0..12 {
                    let ang = k as f32 * std::f32::consts::TAU / 12.0;
                    p.galley(pos + vec2(ang.cos(), ang.sin()) * r, tinted.galley.clone(), Color32::WHITE);
                }
            }
        }
        rich.paint(p, pos);
    }
}

/// The ink for an author's name, and the role colour for the dot some people prefer.
fn name_ink(d: &D, who: Id) -> (Ink, Option<Color32>) {
    use crate::cache::RoleColors::*;
    if let Some((fx, cols)) = d.store.user(who).and_then(|u| u.style.clone()) {
        let min = if matches!(fx, 2 | 6 | 7) { 2.5 } else { 3.0 };
        let stops: Vec<Color32> = cols.iter().map(|c| contrast(role_color(*c), pal().bg_primary, min)).collect();
        // gradient, glow and prism spread their colours; the other effects sit flat on the first
        return match fx {
            2 | 6 | 7 => (Ink { stops, tile: false, glow: fx == 6 }, None),
            _ => (Ink::flat(stops[0]), None),
        };
    }
    let role = d.store.role_look(d.guild, who);
    let solid = role.map(|r| role_color(r.color));
    match d.s.role_colors {
        InNames => match role {
            Some(r) if r.grad.len() >= 2 => {
                // the light theme darkens gradient names to 85% brightness
                let k = if pal().dark { 1.0 } else { 0.85 };
                let dim = |c: u32| {
                    let c = role_color(c);
                    Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8)
                };
                let mut stops: Vec<Color32> = r.grad.iter().map(|c| dim(*c)).collect();
                stops.push(stops[0]);
                (Ink { stops, tile: true, glow: false }, None)
            }
            _ => (Ink::flat(solid.unwrap_or(pal().header_primary)), None),
        },
        NextToNames => (Ink::flat(pal().header_primary), solid),
        Hidden => (Ink::flat(pal().header_primary), None),
    }
}

/// Name, role icon, optional APP tag and timestamp on one 22px line. The timestamp shares the
/// name's baseline rather than its centre, which is what makes discord's header read right.
#[allow(clippy::too_many_arguments)]
fn author_line(ui: &mut Ui, d: &mut D, m: &Message, name: &str, ink: &Ink, dot: Option<Color32>, font: f32, hot: bool) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), (font * 1.375).round()), Sense::hover());
    let n = ink.layout(ui.ctx(), name, medium(font), row.width() - 160.0, hot);
    let ny = row.center().y - n.size().y / 2.0;
    let name_rect = Rect::from_min_size(pos2(row.left(), ny), n.size());
    let resp = ui.interact(name_rect, ui.id().with(("author", name)), Sense::click());
    ink.paint(ui.painter(), &n, name_rect.min, hot);
    if resp.hovered() {
        ui.painter().hline(name_rect.x_range(), name_rect.bottom() - 2.0, Stroke::new(1.0, ink.stops[0]));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let vis = ui.clip_rect().contains_rect(row);
    if vis {
        d.rects.push((format!("name:{}", m.author), name_rect));
    }
    let base = ny + n.baseline();
    // headerText's 4px margin; the icon (which has none after it) and the timestamp bring their own
    let mut x = name_rect.right() + 4.0;
    if let Some(c) = dot {
        ui.painter().circle_filled(pos2(x + 5.0, row.center().y), 5.0, c);
        x += 14.0;
    }
    if let Some(r) = d.store.role_icon(d.guild, m.author) {
        let url = match &r.icon {
            Some(h) => format!("{}/role-icons/{}/{h}.png?size=40", crate::api::cdn(), r.id),
            None => markdown::twemoji(r.emoji.as_deref().unwrap_or_default()),
        };
        let ir = Rect::from_min_size(pos2(x, row.top() + 1.0), vec2(20.0, 20.0));
        text::image(ui.painter(), &url, ir);
        ui.interact(ir, ui.id().with(("role_icon", m.id)), Sense::hover()).on_hover_text(&r.name);
        if vis {
            d.rects.push((format!("role_icon:{}", m.author), ir));
        }
        x = ir.right();
    }
    if let Some(u) = d.store.user(m.author) {
        let cy = row.center().y;
        if u.bot {
            x = super::app_tag(ui.painter(), pos2(x, cy), u.flags & (1 << 16) != 0, 1.0).right() + 4.0;
        } else if let Some(r) = super::tag_chip(ui.painter(), u, pos2(x, cy), 1.0) {
            x = r.right() + 4.0;
        }
    }
    let t = ui.painter().layout_no_wrap(stamp(m.ts()), medium(12.0 * font / 16.0), pal().chat_muted);
    let ty = base - text::baseline(&t);
    let tr = Rect::from_min_size(pos2(x + 4.0, ty), t.size());
    ui.painter().galley(tr.min, t, pal().chat_muted);
    let full = Local
        .timestamp_millis_opt(m.ts())
        .single()
        .map(|t| format!("{} {}", t.format("%A, %B %-d, %Y"), t.format(markdown::hm())))
        .unwrap_or_default();
    ui.interact(tr, ui.id().with(("stamp", name, m.id)), Sense::hover()).on_hover_text(full);
    if vis {
        d.rects.push((format!("stamp:{}", m.author), tr));
    }
    if resp.clicked() {
        d.acts.push(Act::Profile(m.author, name_rect.right_top() + vec2(8.0, 0.0)));
    }
    if resp.secondary_clicked() {
        d.acts.push(Act::UserMenu(m.author, resp.interact_pointer_pos().unwrap_or(name_rect.center())));
    }
}

// compact mode: "Name  message text" on one line, the name clickable like everywhere else
fn compact_prefix(d: &D, m: &Message, blocks: &mut [Block], name: &str, color: Color32, font: f32) {
    let Some(Block::Text(t)) = blocks.first_mut() else { return };
    let fmt = egui::text::TextFormat {
        font_id: medium(font),
        color,
        line_height: Some((font * 1.375).round()),
        valign: Align::Center,
        ..Default::default()
    };
    markdown::prepend(t, &format!("{name}  "), fmt, Some(Link::User(m.author)));
    if d.s.compact_avatars
        && let Some(u) = d.store.user(m.author)
    {
        let size = (font * 1.1).round();
        let w = d.ctx.fonts_mut(|f| f.glyph_width(&reg(font), '\u{a0}'));
        let gap = egui::text::TextFormat { font_id: reg(font), color: Color32::TRANSPARENT, valign: Align::Center, ..Default::default() };
        markdown::prepend(t, "\u{a0}", gap, None);
        t.job.sections[0].leading_space = size - w + 4.0;
        t.emojis.push((0, u.avatar_url(), size, size - w + 4.0));
    }
}

fn compact_name_line(ui: &mut Ui, d: &mut D, m: &Message, name: &str, color: Color32, font: f32) {
    let r = text::label(ui, name, medium(font), color, Sense::click());
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        d.acts.push(Act::Profile(m.author, r.rect.right_top() + vec2(8.0, 0.0)));
    }
    if r.secondary_clicked() {
        d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(r.rect.center())));
    }
}

fn system_line(ui: &mut Ui, d: &mut D, m: &Message) {
    let name = d.store.name_in(d.guild, m.author);
    let (icon, text, col) = match m.kind {
        7 => (Icon::ArrowRight, format!("{name} joined the server."), GREEN),
        6 => (Icon::Pin, format!("{name} pinned a message to this channel."), pal().header_secondary),
        8..=11 => (Icon::Gem, format!("{name} just boosted the server!"), Color32::from_rgb(0xff, 0x73, 0xfa)),
        1 => (Icon::ArrowRight, format!("{name} added someone to the group."), GREEN),
        2 => (Icon::ArrowLeft, format!("{name} removed someone from the group."), RED),
        3 => (Icon::Speaker, format!("{name} started a call."), GREEN),
        4 => (Icon::Pencil, format!("{name} changed the channel name."), pal().header_secondary),
        18 => (Icon::Thread, format!("{name} started a thread: {}", m.content), pal().header_secondary),
        _ => (Icon::ArrowRight, if m.content.is_empty() { format!("{name} did something.") } else { m.content.clone() }, pal().header_secondary),
    };
    let left = ui.max_rect().left();
    let y = ui.cursor().top() + 11.0;
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(left - 36.0, y), vec2(16.0, 16.0)), col);
    ui.horizontal_wrapped(|ui| {
        text::label(ui, &text, reg(16.0), pal().header_secondary, Sense::hover());
        ui.add(Label::new(RichText::new(stamp(m.ts())).font(medium(12.0)).color(pal().chat_muted)).selectable(false));
    });
}

fn reply_line(ui: &mut Ui, d: &mut D, rid: Id, row_left: f32) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    let spine = Stroke::new(2.0, pal().spine);
    let x = row_left + 36.0;
    let y = r.center().y;
    let p = ui.painter();
    p.line_segment([pos2(x, r.bottom() + 2.0), pos2(x, y + 6.0)], spine);
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [pos2(x, y + 6.0), pos2(x, y), pos2(x, y), pos2(x + 6.0, y)],
        false,
        Color32::TRANSPARENT,
        spine,
    ));
    p.line_segment([pos2(x + 6.0, y), pos2(r.left() - 4.0, y)], spine);

    let Some(rm) = d.store.find_msg(d.ch, rid) else {
        // discord: a small reply arrow, then the notice in italics
        icons::draw(p, Icon::Reply, Rect::from_center_size(pos2(r.left() + 6.0, y), vec2(12.0, 12.0)), pal().text_muted);
        let mut job = egui::text::LayoutJob::default();
        let lost = if d.popout { "Message could not be loaded" } else { "Original message was deleted" };
        job.append(lost, 0.0, egui::TextFormat { font_id: reg(14.0), color: pal().text_muted, italics: true, ..Default::default() });
        let g = ui.painter().layout_job(job);
        p.galley(pos2(r.left() + 16.0, y - g.size().y / 2.0), g, pal().text_muted);
        return;
    };
    let mut cx = r.left();
    if let Some(u) = d.store.user(rm.author) {
        avatar(ui, Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0)), &u.avatar_url());
        cx += 20.0;
    }
    let name = format!("@{}", d.store.name_in(d.guild, rm.author));
    let (ink, _) = name_ink(d, rm.author);
    let rich = ink.layout(ui.ctx(), &name, medium(14.0), 200.0, false);
    let nr = Align2::LEFT_CENTER.anchor_size(pos2(cx, y), rich.size());
    ink.paint(p, &rich, nr.min, false);
    cx = nr.right() + 4.0;
    let excerpt = markdown::plain(&rm.content, d.store, d.guild);
    if excerpt.is_empty() {
        let ir = Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0));
        icons::draw(p, Icon::Image, ir, pal().header_secondary);
        cx += 20.0;
        p.text(pos2(cx, y), Align2::LEFT_CENTER, "Click to see attachment", reg(14.0), pal().header_secondary);
    } else {
        text::paint(p, pos2(cx, y), Align2::LEFT_CENTER, &excerpt, reg(14.0), pal().header_secondary, r.right() - cx);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.clip_rect().contains_rect(r) {
        d.rects.push(("reply".into(), r));
    }
    if resp.clicked() {
        d.acts.push(Act::Jump(rid));
    }
}

fn blocks_ui(ui: &mut Ui, blocks: &[Block], d: &mut D, msg: Id) {
    for b in blocks {
        match b {
            Block::Text(t) => {
                let mut job = t.job.clone();
                job.wrap.max_width = ui.available_width();
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                let resp = ui.add(Label::new(galley.clone()).sense(Sense::click()));
                let origin = resp.rect.min;
                for (idx, url, size, lead) in &t.emojis {
                    let c = galley.pos_from_cursor(CCursor::new(*idx));
                    let x = origin.x + (c.min.x - lead).max(0.0);
                    let rect = Rect::from_min_size(pos2(x, origin.y + c.center().y - size / 2.0), vec2(*size, *size));
                    Image::from_uri(gif::uri(ui.ctx(), url, ui.is_rect_visible(rect))).show_loading_spinner(false).paint_at(ui, rect);
                }
                if !t.links.is_empty()
                    && let Some(p) = resp.hover_pos()
                {
                    // cursor_from_pos snaps to the nearest gap; we want the glyph under the pointer
                    let mut idx = galley.cursor_from_pos(p - origin).index.0;
                    if idx > 0 && origin.x + galley.pos_from_cursor(CCursor::new(idx)).min.x > p.x {
                        idx -= 1;
                    }
                    if let Some((_, l)) = t.links.iter().find(|(r, _)| r.contains(&idx)) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if resp.clicked() {
                            match l {
                                Link::Url(u) => d.acts.push(Act::Url(u.clone())),
                                Link::Channel(c) => d.acts.push(Act::Channel(*c)),
                                Link::Spoiler => d.acts.push(Act::Reveal(msg)),
                                Link::User(u) => {
                                    let at = p + vec2(8.0, 0.0);
                                    d.acts.push(Act::Profile(*u, at));
                                }
                            }
                        }
                    }
                }
            }
            Block::Code(code) => {
                ui.add_space(6.0);
                let r = Frame::new()
                    .fill(pal().bg_code)
                    .stroke(Stroke::new(1.0, pal().border_normal))
                    .corner_radius(4)
                    .inner_margin(7)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(Label::new(RichText::new(code).font(mono(14.0)).color(pal().text_normal)).wrap());
                    });
                if ui.clip_rect().contains_rect(r.response.rect) {
                    d.rects.push(("codeblock".into(), r.response.rect));
                }
                ui.add_space(4.0);
            }
            Block::Quote(inner) => {
                let top = ui.cursor().top();
                let x = ui.cursor().left();
                let r = ui
                    .scope_builder(
                        UiBuilder::new().max_rect(Rect::from_min_size(pos2(x + 16.0, top), vec2(ui.available_width() - 16.0, 100_000.0))),
                        |ui| blocks_ui(ui, inner, d, msg),
                    )
                    .response
                    .rect;
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(x, top), pos2(x + 4.0, r.bottom())),
                    CornerRadius::same(2),
                    pal().bg_accent,
                );
            }
        }
    }
}

fn fit(w: u32, h: u32, max_w: f32, max_h: f32, avail: f32) -> (f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let s = (max_w.min(avail) / w).min(max_h / h).min(1.0);
    ((w * s).round(), (h * s).round())
}

/// `poster` is a still drawn underneath while the animation downloads and decodes.
fn media(ui: &mut Ui, d: &mut D, url: &str, full: &str, w: f32, h: f32, poster: Option<&str>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    resp.context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if super::menu_item(ui, None, "Save Image", false) {
            d.acts.push(Act::Save(full.to_string()));
        }
        super::menu_sep(ui);
        if super::menu_item(ui, None, "Copy Link", false) {
            ui.ctx().copy_text(full.to_string());
        }
        if super::menu_item(ui, None, "Open Link", false) {
            d.acts.push(Act::Url(full.to_string()));
        }
    });
    ui.painter().rect_filled(rect, CornerRadius::same(8), pal().bg_secondary);
    if let Some(p) = poster {
        Image::from_uri(p.to_string()).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, rect);
    }
    let play = (d.s.gif_autoplay || resp.hovered()) && ui.is_rect_visible(rect);
    Image::from_uri(gif::uri(ui.ctx(), url, play))
        .corner_radius(CornerRadius::same(8))
        .show_loading_spinner(false)
        .paint_at(ui, rect);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn human_size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} bytes"),
        1024..1_048_576 => format!("{:.2} KB", n as f64 / 1024.0),
        _ => format!("{:.2} MB", n as f64 / 1_048_576.0),
    }
}

fn file_card(ui: &mut Ui, name: &str, size: u64, video: bool) -> egui::Response {
    let w = ui.available_width().min(432.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 66.0), Sense::click());
    ui.painter().rect(rect, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let ir = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(30.0, 36.0));
    icons::draw(ui.painter(), Icon::File, ir, BLURPLE);
    if video {
        icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(ir.center() + vec2(0.0, 3.0), vec2(12.0, 12.0)), BLURPLE);
    }
    let clip = ui.painter().with_clip_rect(rect.shrink(8.0));
    let col = if resp.hovered() { pal().link.gamma_multiply(1.2) } else { pal().link };
    text::paint(ui.painter(), pos2(rect.left() + 52.0, rect.center().y - 9.0), Align2::LEFT_CENTER, name, reg(16.0), col, rect.width() - 64.0);
    clip.text(pos2(rect.left() + 52.0, rect.center().y + 11.0), Align2::LEFT_CENTER, human_size(size), reg(12.0), pal().text_muted);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Discord drops the text of a message that is nothing but the link its image or gifv embed came from.
fn link_only(m: &Message) -> bool {
    let c = m.content.trim();
    m.embeds.iter().any(|e| matches!(e.kind.as_str(), "image" | "gifv") && e.url.as_deref() == Some(c))
}

fn embed(ui: &mut Ui, d: &mut D, e: &Embed, msg: Id) {
    let bare = e.title.is_none() && e.desc.is_none() && e.author.is_none() && e.fields.is_empty();
    let media_ok = d.s.inline_links;
    // gifv is always just the looping clip, whatever title the provider gave it
    let gifv = e.kind == "gifv";
    if gifv || (bare && e.kind == "image") {
        if !media_ok {
            return;
        }
        let still = e.thumb.as_ref().or(e.image.as_ref());
        if let Some(m) = e.video.as_ref().filter(|_| gifv).or(still) {
            let (mw, mh) = if m.w > 0 { (m.w, m.h) } else { still.map_or((0, 0), |s| (s.w, s.h)) };
            let (w, h) = fit(mw, mh, 400.0, 300.0, ui.available_width());
            let poster = still.filter(|s| gifv && s.url != m.url).map(|s| s.url.as_str());
            let r = media(ui, d, &m.url, e.url.as_deref().unwrap_or(&m.url), w, h, poster);
            if ui.clip_rect().contains(r.rect.center()) {
                d.rects.push((format!("{}:{msg}", if gifv { "gifv" } else { "embed_image" }), r.rect.intersect(ui.clip_rect())));
            }
            if r.clicked() {
                let open = e.url.clone().unwrap_or_else(|| m.url.clone());
                d.acts.push(Act::View(m.url.clone(), open, mw, mh));
            }
        }
        return;
    }
    let maxw = (ui.available_width() - 4.0).min(516.0);
    let bar = e.color.map(role_color).unwrap_or(pal().border_normal);
    let start = ui.cursor().min;
    let thumb = e.thumb.as_ref().filter(|_| media_ok && e.kind != "video" && e.image.is_none());
    let r = Frame::new()
        .fill(pal().bg_secondary)
        .corner_radius(CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 })
        .inner_margin(Margin { left: 12, right: 16, top: 8, bottom: 16 })
        .show(ui, |ui| {
            ui.set_max_width(maxw - 28.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(maxw - 28.0 - if thumb.is_some() { 96.0 } else { 0.0 });
                    ui.add_space(2.0);
                    if let Some(p) = &e.provider {
                        ui.label(RichText::new(p).font(reg(12.0)).color(pal().header_secondary));
                    }
                    if let Some(a) = &e.author {
                        ui.label(RichText::new(a).font(semi(14.0)).color(pal().header_primary));
                    }
                    if let Some(t) = &e.title {
                        let col = if e.url.is_some() { pal().link } else { pal().header_primary };
                        let r = ui.add(Label::new(RichText::new(t).font(semi(16.0)).color(col)).sense(Sense::click()));
                        if e.url.is_some() && r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked()
                            && let Some(u) = &e.url
                        {
                            d.acts.push(Act::Url(u.clone()));
                        }
                    }
                    if let Some(desc) = &e.desc {
                        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false };
                        let blocks = markdown::parse(desc, &cx);
                        blocks_ui(ui, &blocks, d, msg);
                    }
                    let mut row: Vec<&(String, String, bool)> = Vec::new();
                    let flush = |ui: &mut Ui, d: &mut D, row: &mut Vec<&(String, String, bool)>| {
                        if row.is_empty() {
                            return;
                        }
                        let cw = (ui.available_width() / row.len() as f32).max(80.0);
                        ui.horizontal_top(|ui| {
                            for f in row.iter() {
                                ui.allocate_ui(vec2(cw - 8.0, 0.0), |ui| {
                                    ui.set_min_width(cw - 8.0);
                                    field(ui, d, f, msg)
                                });
                            }
                        });
                        row.clear();
                    };
                    for f in &e.fields {
                        if f.2 {
                            row.push(f);
                            if row.len() == 3 {
                                flush(ui, d, &mut row);
                            }
                        } else {
                            flush(ui, d, &mut row);
                            field(ui, d, f, msg);
                        }
                    }
                    flush(ui, d, &mut row);
                });
                if let Some(t) = thumb {
                    let (w, h) = fit(t.w, t.h, 80.0, 80.0, 80.0);
                    media(ui, d, &t.url, &t.url, w, h, None);
                }
            });
            if let Some(m) = e.image.as_ref().or(if e.kind == "video" { e.thumb.as_ref() } else { None }).filter(|_| media_ok) {
                ui.add_space(8.0);
                let (w, h) = fit(m.w, m.h, 400.0, 300.0, ui.available_width());
                let r = media(ui, d, &m.url, &m.url, w, h, None);
                if e.kind == "video" {
                    ui.painter().circle_filled(r.rect.center(), 24.0, Color32::from_black_alpha(160));
                    icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(r.rect.center() + vec2(2.0, 0.0), vec2(22.0, 22.0)), Color32::WHITE);
                }
                if r.clicked() {
                    match (&e.url, e.kind == "video") {
                        (Some(u), true) => d.acts.push(Act::Url(u.clone())),
                        _ => d.acts.push(Act::View(m.url.clone(), e.url.clone().unwrap_or_else(|| m.url.clone()), m.w, m.h)),
                    }
                }
            }
            if let Some(f) = &e.footer {
                ui.add_space(2.0);
                ui.label(RichText::new(f).font(reg(12.0)).color(pal().header_secondary));
            }
        });
    let rect = r.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_max(start, pos2(start.x + 4.0, rect.bottom())),
        CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
        bar,
    );
}

fn field(ui: &mut Ui, d: &mut D, f: &(String, String, bool), msg: Id) {
    ui.vertical(|ui| {
        ui.label(RichText::new(&f.0).font(semi(14.0)).color(pal().header_primary));
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false };
        let blocks = markdown::parse(&f.1, &cx);
        blocks_ui(ui, &blocks, d, msg);
    });
}

pub fn emoji_url(e: &ReactEmoji) -> String {
    if e.id != 0 {
        markdown::emoji_url(e.id, e.animated)
    } else {
        markdown::twemoji(&e.name)
    }
}

fn components(ui: &mut Ui, d: &mut D, m: &Message) {
    for (i, row) in m.components.iter().enumerate() {
        if i > 0 {
            ui.add_space(8.0);
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for c in row {
                match c {
                    Comp::Button { style, label, emoji, custom_id, url, disabled } => {
                        // discord's small button: 32 tall, 1px border, 11 padding inside it, radius 8
                        let edge = Color32::from_white_alpha(20);
                        let (fill, hover, ink, stroke) = match style {
                            1 => (BLURPLE, Color32::from_rgb(0x44, 0x52, 0xbb), Color32::WHITE, edge),
                            3 => (Color32::from_rgb(0x00, 0x85, 0x45), Color32::from_rgb(0x00, 0x6c, 0x37), Color32::WHITE, edge),
                            4 => (Color32::from_rgb(0xd2, 0x2d, 0x39), Color32::from_rgb(0xa9, 0x23, 0x2e), Color32::WHITE, edge),
                            _ => (pal().control, pal().control_hover, pal().header_primary, pal().border_muted),
                        };
                        let g = ui.painter().layout_no_wrap(label.clone(), medium(14.0), Color32::WHITE);
                        let ew = if emoji.is_some() { 22.0 } else { 0.0 };
                        let lw = if url.is_some() { 18.0 } else { 0.0 };
                        let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + ew + lw + 24.0).max(60.0), 32.0), Sense::click());
                        // disabled is opacity .5 on the whole button
                        let fade = if *disabled { 0.5 } else { 1.0 };
                        let fill = if resp.hovered() && !disabled { hover } else { fill };
                        ui.painter().rect(r, CornerRadius::same(8), fill.gamma_multiply(fade), Stroke::new(1.0, stroke.gamma_multiply(fade)), egui::StrokeKind::Inside);
                        if ui.clip_rect().contains_rect(r) {
                            d.rects.push((format!("button:{custom_id}"), r));
                        }
                        let mut x = r.center().x - (g.size().x + ew + lw) / 2.0;
                        if let Some(e) = emoji {
                            text::image(ui.painter(), &emoji_url(e), Rect::from_min_size(pos2(x, r.center().y - 9.0), vec2(18.0, 18.0)));
                            x += ew;
                        }
                        let tc = ink.gamma_multiply(fade);
                        let gw = g.size().x;
                        ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, tc);
                        if url.is_some() {
                            icons::draw(ui.painter(), Icon::ArrowRight, Rect::from_center_size(pos2(x + gw + 12.0, r.center().y), vec2(14.0, 14.0)), tc);
                        }
                        if resp.hovered() && !disabled {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() && !disabled {
                            match url {
                                Some(u) => d.acts.push(Act::Url(u.clone())),
                                None => d.acts.push(Act::Component(m.id, custom_id.clone(), None)),
                            }
                        }
                    }
                    Comp::Select { custom_id, placeholder, options, disabled, .. } => {
                        let w = ui.available_width().min(400.0);
                        let id = ui.id().with(("select", m.id, custom_id));
                        let mut open: bool = ui.data(|dd| dd.get_temp(id).unwrap_or(false));
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
                        ui.painter().rect(r, CornerRadius::same(4), pal().input_bg, Stroke::new(1.0, if open { BLURPLE } else { pal().border_normal }), egui::StrokeKind::Inside);
                        ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, placeholder, reg(15.0), pal().text_muted);
                        icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary);
                        if resp.clicked() && !disabled {
                            open = !open;
                        }
                        if open {
                            for (label, value, desc) in options {
                                let h = if desc.is_some() { 48.0 } else { 36.0 };
                                let (or, oresp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                                ui.painter().rect_filled(or, CornerRadius::ZERO, if oresp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary });
                                ui.painter().text(pos2(or.left() + 12.0, or.top() + 18.0), Align2::LEFT_CENTER, label, semi(15.0), pal().header_primary);
                                if let Some(dsc) = desc {
                                    ui.painter().text(pos2(or.left() + 12.0, or.top() + 36.0), Align2::LEFT_CENTER, dsc, reg(13.0), pal().header_secondary);
                                }
                                if oresp.clicked() {
                                    d.acts.push(Act::Component(m.id, custom_id.clone(), Some(vec![value.clone()])));
                                    open = false;
                                }
                            }
                        }
                        ui.data_mut(|dd| dd.insert_temp(id, open));
                    }
                }
            }
        });
    }
}

fn reactions(ui: &mut Ui, d: &mut D, m: &Message) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for r in &m.reactions {
            let count = r.count.to_string();
            let g = ui.painter().layout_no_wrap(count, semi(14.0), if r.me { pal().mention_fg } else { pal().header_secondary });
            let size = vec2(6.0 + 16.0 + 6.0 + g.size().x + 8.0, 24.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let (fill, stroke) = if r.me {
                (Color32::from_rgba_premultiplied(13, 15, 36, 38), BLURPLE)
            } else if resp.hovered() {
                (pal().bg_secondary, pal().bg_accent)
            } else {
                (pal().bg_secondary, Color32::TRANSPARENT)
            };
            ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            let er = Rect::from_min_size(pos2(rect.left() + 7.0, rect.center().y - 8.0), vec2(16.0, 16.0));
            Image::from_uri(gif::uri(ui.ctx(), &emoji_url(&r.emoji), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
            ui.painter().galley(pos2(er.right() + 6.0, rect.center().y - g.size().y / 2.0), g, pal().header_secondary);
            if resp.clicked() {
                d.acts.push(Act::React(m.id, r.emoji.clone(), !r.me));
            }
            if r.emoji.id != 0 {
                resp.on_hover_text(format!(":{}:", r.emoji.name));
            }
        }
    });
}

fn action_bar(d: &mut D, m: &Message, row: Rect) {
    let own = m.author == d.store.me;
    // discord's bar: three one-click reactions, a rule, then react, edit (own), reply, forward, more
    let quick: Vec<ReactEmoji> = d.quick.iter().take(3).cloned().collect();
    let n = quick.len() as f32 + if own { 5.0 } else { 4.0 };
    let size = vec2(n * 32.0 + 9.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    let rect = Rect::from_min_size(pos, size);
    d.bar = Some((m.id, rect));
    egui::Area::new(egui::Id::new("msg_bar"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(d.ctx, |ui| {
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for e in quick {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(8), pal().control_hover);
                            }
                            let er = Rect::from_center_size(r.center(), vec2(20.0, 20.0));
                            Image::from_uri(gif::uri(ui.ctx(), &emoji_url(&e), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                            if resp.on_hover_text("Click to react").clicked() {
                                d.acts.push(Act::React(m.id, e.clone(), true));
                            }
                        }
                        let (rule, _) = ui.allocate_exact_size(vec2(9.0, 32.0), Sense::hover());
                        ui.painter().vline(rule.center().x, rule.y_range().shrink(8.0), Stroke::new(1.0, pal().border_subtle));
                        let r = icon_button(ui, Icon::AddReaction, 32.0, "Add Reaction");
                        if r.clicked() {
                            d.acts.push(Act::Picker(m.id, r.rect.left_bottom()));
                        }
                        if own && icon_button(ui, Icon::Pencil, 32.0, "Edit").clicked() {
                            d.acts.push(Act::Edit(m.id));
                        }
                        let rr = icon_button(ui, Icon::Reply, 32.0, "Reply");
                        d.rects.push(("bar_reply".into(), rr.rect));
                        if rr.clicked() {
                            d.acts.push(Act::Reply(m.id));
                        }
                        // holding shift on your own message offers delete without the confirm, like discord
                        if own && ui.input(|i| i.modifiers.shift) {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), RED);
                            if resp.on_hover_text("Delete").clicked() {
                                d.acts.push(Act::Delete(m.id));
                            }
                        }
                        if icon_button(ui, Icon::Forward, 32.0, "Forward").clicked() {
                            d.acts.push(Act::Forward(m.id));
                        }
                        let more = icon_button(ui, Icon::More, 32.0, "More");
                        if more.clicked() {
                            d.acts.push(Act::Menu(m.id, more.rect.left_bottom()));
                        }
                    });
                });
        });
}

fn edit_box(ui: &mut Ui, d: &mut D, id: Id) {
    let Some((_, text)) = d.editing.as_mut() else { return };
    let tid = egui::Id::new("edit_box");
    let enter = ui.memory(|m| m.has_focus(tid)) && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Enter));
    let esc = ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape));
    ui.add_space(4.0);
    Frame::new().fill(pal().bg_input).corner_radius(8).inner_margin(Margin::symmetric(16, 11)).show(ui, |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(text)
                .id(tid)
                .frame(Frame::NONE)
                .desired_rows(1)
                .font(reg(16.0))
                .text_color(pal().text_normal)
                .desired_width(ui.available_width()),
        );
        if !r.has_focus() && !enter {
            r.request_focus();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let small = |s: &str, c: Color32| RichText::new(s).font(reg(12.0)).color(c);
        ui.label(small("escape to", pal().text_normal));
        if ui.add(Label::new(small("cancel", pal().link)).sense(Sense::click())).clicked() {
            d.acts.push(Act::CancelEdit);
        }
        ui.label(small("• enter to", pal().text_normal));
        if ui.add(Label::new(small("save", pal().link)).sense(Sense::click())).clicked() {
            d.acts.push(Act::SaveEdit(id, text.clone()));
        }
    });
    if enter {
        d.acts.push(Act::SaveEdit(id, text.clone()));
    } else if esc {
        d.acts.push(Act::CancelEdit);
    }
}

pub fn context_menu(app: &mut App, ctx: &egui::Context) {
    let Some((ch, id, at)) = app.menu else { return };
    let Some(m) = app.store.find_msg(ch, id) else {
        app.menu = None;
        return;
    };
    let own = m.author == app.store.me;
    let content = m.content.clone();
    let pinned = m.pinned;
    let dev = app.settings.dev;
    let mut frequent: Vec<ReactEmoji> = app.ui.quick.clone();
    for n in ["✅", "❤️", "🔥", "😂", "👍", "😭", "💀", "👀", "🙏", "😊"] {
        if !frequent.iter().any(|q| q.id == 0 && q.name == n) {
            frequent.push(ReactEmoji { id: 0, name: n.to_string(), animated: false });
        }
    }
    frequent.truncate(10);
    let quick = app.quick_reactions();
    let chan = app.store.channels.get(&ch);
    // pinning and deleting other people's messages take manage messages, except in dms
    let manage = chan.is_some_and(|c| c.guild == 0 || app.store.perms(c) & crate::state::MANAGE_MESSAGES != 0);
    let mut pick = None;
    let mut react = None;
    let mut swap = None;
    let mut tiles = Vec::new();
    // discord's message menu: one-click reactions on top, then its groups
    let open = super::float_menu(ctx, &format!("msg_menu{id}"), at, |ui| {
        use super::{Trail, menu_item, menu_row, menu_sep};
        ui.set_min_width(204.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (i, e) in quick.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(46.0, 40.0), Sense::click());
                tiles.push(r);
                ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { pal().control_hover } else { pal().control });
                let er = Rect::from_center_size(r.center(), vec2(24.0, 24.0));
                Image::from_uri(gif::uri(ui.ctx(), &emoji_url(e), true)).show_loading_spinner(false).paint_at(ui, er);
                // not discord's: right-click a slot to put another emoji there
                if resp.secondary_clicked() {
                    swap = Some((i, r.left_bottom()));
                }
                if resp.on_hover_text("Right-click to change").clicked() {
                    react = Some(e.clone());
                }
            }
        });
        menu_sep(ui);
        let add = menu_row(ui, Some(Icon::AddReaction), "Add Reaction", None, false, Trail::Caret);
        if add.clicked() {
            pick = Some(0);
        }
        egui::containers::menu::SubMenu::new().show(ui, &add, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for e in &frequent {
                let name = if e.id != 0 { e.name.clone() } else { emojis::get(&e.name).and_then(|x| x.shortcode()).unwrap_or(&e.name).to_string() };
                let url = gif::uri(ui.ctx(), &emoji_url(e), true);
                if menu_row(ui, None, &format!(":{name}:"), None, false, Trail::Image(url)).clicked() {
                    react = Some(e.clone());
                }
            }
            menu_sep(ui);
            if menu_item(ui, None, "View More", false) {
                pick = Some(0);
            }
        });
        menu_sep(ui);
        if own && menu_item(ui, Some(Icon::Pencil), "Edit Message", false) {
            pick = Some(1);
        }
        if menu_item(ui, Some(Icon::Reply), "Reply", false) {
            pick = Some(2);
        }
        if menu_item(ui, Some(Icon::Forward), "Forward", false) {
            pick = Some(11);
        }
        menu_sep(ui);
        if menu_item(ui, Some(Icon::Copy), "Copy Text", false) {
            pick = Some(3);
        }
        if manage && menu_item(ui, Some(Icon::Pin), if pinned { "Unpin Message" } else { "Pin Message" }, false) {
            pick = Some(6);
        }
        if menu_item(ui, Some(Icon::MarkUnread), "Mark Unread", false) {
            pick = Some(7);
        }
        if menu_item(ui, Some(Icon::Link), "Copy Message Link", false) {
            pick = Some(8);
        }
        menu_sep(ui);
        if (own || manage) && menu_item(ui, Some(Icon::Trash), "Delete Message", true) {
            pick = Some(5);
        }
        if !own && menu_item(ui, Some(Icon::Flag), "Report Message", true) {
            pick = Some(10);
        }
        if dev {
            menu_sep(ui);
            if menu_item(ui, Some(Icon::Id), "Copy Message ID", false) {
                pick = Some(4);
            }
        }
    });
    if let Some(e) = react {
        app.react(ch, id, e, true);
        app.menu = None;
        return;
    }
    for (i, r) in tiles.into_iter().enumerate() {
        app.rects.insert(format!("quick:{i}"), r);
    }
    if let Some((slot, under)) = swap {
        app.menu = None;
        app.picker = Some((Pick::Quick(slot), under));
        return;
    }
    match pick {
        Some(0) => app.picker = Some((Pick::React(ch, id), at)),
        Some(1) => app.editing = Some((id, content)),
        Some(2) => {
            app.reply = Some(id);
            app.focus_composer = true;
        }
        Some(3) => ctx.copy_text(content),
        Some(4) => ctx.copy_text(id.to_string()),
        Some(5) => app.confirm_delete = Some((ch, id)),
        Some(6) => {
            let method = if pinned { "DELETE" } else { "PUT" };
            app.rest(method, format!("/channels/{ch}/pins/{id}"), None, crate::net::Tag::Quiet);
            if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                m.pinned = !pinned;
            }
        }
        Some(7) => app.mark_unread(ch, id),
        Some(8) => {
            let g = app.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0);
            let g = g.map(|g| g.to_string()).unwrap_or_else(|| "@me".into());
            ctx.copy_text(format!("https://discord.com/channels/{g}/{ch}/{id}"));
        }
        Some(11) => app.forward = Some(super::popups::Forward::new(ch, id)),
        // reporting has no flow of its own here; discord's web client handles it
        Some(10) => {
            let g = app.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0);
            let g = g.map(|g| g.to_string()).unwrap_or_else(|| "@me".into());
            ctx.open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{g}/{ch}/{id}")));
        }
        _ => {}
    }
    if pick.is_some() || !open {
        app.menu = None;
    }
}

/// Compact read-only rendering used by pins and search results.
pub fn preview(ui: &mut Ui, store: &Store, m: &Message) -> egui::Response {
    let ctx = ui.ctx().clone();
    let guild = store.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
    let revealed = HashSet::new();
    let mut editing = None;
    let settings = crate::cache::Settings::default();
    let mut d = D {
        store,
        ctx: &ctx,
        guild,
        ch: m.channel,
        revealed: &revealed,
        editing: &mut editing,
        marker: None,
        hover_lock: None,
        bar: None,
        acts: Vec::new(),
        embeds: false,
        flash: None,
        rects: Vec::new(),
        s: &settings,
        quick: &[],
        moderator: false,
        popout: true,
    };
    // discord's cards: a raised fill and a hairline, the reply it answers above the author
    let r = Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin { left: 12, right: 16, top: 12, bottom: 16 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(rid) = m.reply {
            let left = ui.max_rect().left();
            ui.horizontal(|ui| {
                ui.add_space(56.0);
                ui.vertical(|ui| reply_line(ui, &mut d, rid, left - 16.0));
            });
        }
        ui.horizontal_top(|ui| {
            let (av, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            if let Some(u) = store.user(m.author) {
                avatar(ui, av, &u.avatar_url());
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let color = store.role_color(guild, m.author).map(role_color).unwrap_or(pal().header_primary);
                    ui.label(RichText::new(store.name_in(guild, m.author)).font(medium(16.0)).color(color));
                    ui.label(RichText::new(stamp(m.ts())).font(medium(12.0)).color(pal().chat_muted));
                });
                let cx = Cx { store, ctx: &ctx, guild, size: 15.0, color: pal().text_normal, revealed: false, edited: m.edited };
                let blocks = markdown::parse(&m.content, &cx);
                blocks_ui(ui, &blocks, &mut d, m.id);
                for a in &m.attachments {
                    ui.add_space(4.0);
                    if a.is_image() {
                        let sep = if a.proxy.contains('?') { '&' } else { '?' };
                        let (w, h) = fit(a.w, a.h, 550.0, 350.0, ui.available_width());
                        let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                        media(ui, &mut d, &url, &a.url, w, h, None);
                    } else {
                        file_card(ui, &a.filename, a.size, a.ctype.starts_with("video/"));
                    }
                }
            });
        });
    });
    for a in d.acts {
        if let Act::Url(u) = a {
            let _ = open::that_detached(u);
        }
    }
    let resp = r.response.interact(Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
