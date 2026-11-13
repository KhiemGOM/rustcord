use chrono::Datelike;
use egui::text::CCursor;
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Image, Key, Label, Layout, Margin, Modifiers, Pos2, Rect, RichText,
    Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

mod components;
mod interaction;

use super::interact::Press;
use super::{App, Pick, avatar, channel_title, icon_button};
use super::icons::{self, Icon};
use super::{side, system, text};
use crate::gif;
use crate::markdown::{self, Block, Cx, Link};
use crate::net::Cmd;
use crate::state::{Channel, Embed, Id, Message, Page, ReactEmoji, Set, Store};
use crate::theme::*;

mod forward;
mod mosaic;
mod head;
use head::*;
pub(crate) use head::side_card;
pub(crate) use head::bell;
mod row;
use row::*;
mod author;
use author::*;
pub(super) use author::grad_ink;
mod body;
use body::*;
mod embeds;
use embeds::*;
pub(super) use embeds::human_size;
pub(crate) use embeds::emoji_url;
mod polls;
use polls::*;
mod actions;
use actions::*;
mod ctxmenu;
pub(crate) use ctxmenu::context_menu;
pub(crate) use ctxmenu::preview;

enum Act {
    Reply(Id),
    Edit(Id),
    Delete(Id),
    React(Id, ReactEmoji, bool),
    Picker(Id, Pos2),
    Post(super::forumbar::Do),
    Menu(Id, Pos2),
    Url(String),
    // the poster of a video embed clicked: which one, and the address of its player
    Frame(super::embedplay::Key, String),
    // a link opened in the browser as it is, as a middle click does
    Browser(String),
    Channel(Id),
    Reveal(Id),
    SaveEdit(Id, String),
    CancelEdit,
    Profile(Id, Pos2),
    UserMenu(Id, Pos2),
    Forward(Id),
    Jump(Id),
    Component(Id, String, Press),
    Premium(String),
    View(String, String, u32, u32),
    MarkUnread(Id),
    Save(String),
    Pin(Id),
    Bookmark(Id),
    CopyId(Id),
    CopyLink(Id),
    Thread(Id),
    // shift on a name: @them into the composer
    Mention(Id),
    // a [text](url) link, which asks before leaving
    Masked(String),
    ClearEmbeds(Id),
    // a poll vote: the message and the answer ids now picked (empty clears the vote)
    Vote(Id, Vec<u8>),
    // play/pause an audio attachment (voice message, soundboard) by its url
    PlayAudio(String),
    Player(super::player::Do),
    // a custom emoji clicked: its id, name, whether animated, and where
    Emoji(Id, String, bool, Pos2),
    Role(Id, Pos2),
    // a server tag chip: its server and where it sits
    Tag(Id, Pos2),
    EditChannel,
    System(system::Go, Pos2),
    // a chip hovered: who reacted wants fetching
    Reactors(Id, ReactEmoji, bool, u32),
    Resend(Id),
    Invite(super::chatinvite::Go),
    CopyImage(String),
    Votes(Id),
    Sticker(Id, String, u8, Pos2),
    CancelUpload(String),
    // a run of blocked messages shown or folded again: their ids
    Blocked(Vec<Id>, bool),
}

struct D<'a> {
    store: &'a Store,
    ctx: &'a egui::Context,
    guild: Id,
    ch: Id,
    revealed: &'a Set<Id>,
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
    // the message keyboard focus is on, ringed and kept in view
    focus: Option<Id>,
    bookmarks: &'a Set<Id>,
    invites: &'a mut super::chatinvite::Cache,
    unblocked: &'a Set<Id>,
    // a right click landed on a picture, whose own menu opens instead of the message's
    image_menu: bool,
    playing: Option<super::embedplay::Key>,
    frame_at: Option<(super::embedplay::Key, Rect, Rect)>,
}

impl D<'_> {
    fn card(&mut self, name: &str, ui: &Ui, r: Rect) {
        if ui.clip_rect().contains(r.center()) {
            self.rects.push((format!("{name}:{}", self.ch), r.intersect(ui.clip_rect())));
        }
    }

    fn player(&mut self, outs: Vec<super::player::Do>) {
        use super::player::Do;
        self.acts.extend(outs.into_iter().map(|o| match o {
            Do::Save(u) => Act::Save(u),
            o => Act::Player(o),
        }));
    }
}

fn normal(kind: u8) -> bool {
    matches!(kind, 0 | 19 | 20 | 23)
}

fn day(ts: i64) -> (i32, u32) {
    let d = markdown::local(ts);
    (d.year(), d.ordinal())
}

pub(super) fn group_start(prev: Option<&Message>, m: &Message) -> bool {
    match prev {
        None => true,
        Some(p) => {
            p.author != m.author
                || m.ts() - p.ts() > 7 * 60 * 1000
                || m.reply.is_some()
                || m.interaction.as_ref().is_some_and(|i| !i.name.is_empty())
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
    h += super::chatinvite::codes(&m.content).len() as f32 * 240.0;
    if let Some(p) = &m.poll {
        h += 92.0 + p.answers.len() as f32 * 50.0;
    }
    if !m.reactions.is_empty() {
        h += 30.0;
    }
    h
}

fn stamp(ts: i64) -> String {
    let d = markdown::local(ts);
    let now = markdown::local(chrono::Utc::now().timestamp_millis());
    let time = d.format(markdown::hm());
    // discord's headers drop "Today at" and show just the time
    if d.date_naive() == now.date_naive() {
        time.to_string()
    } else if d.date_naive().succ_opt() == Some(now.date_naive()) {
        format!("Yesterday at {time}")
    } else {
        format!("{}, {}", d.format(&markdown::short_date("%y", false)), d.format(markdown::hm()))
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.ui.guild.is_none() && app.friends {
        return super::friends::show(app, ui);
    }
    if app.ui.guild.is_none() && app.quests.is_some() {
        return super::quests::show(app, ui);
    }
    if app.ui.guild.is_none() && app.shop.is_some() {
        return super::shop::show(app, ui);
    }
    if let Some(g) = app.boosts.filter(|g| app.ui.guild == Some(*g)) {
        return super::boosts::show(app, ui, g);
    }
    if let Some(g) = app.server_shop.as_ref().map(|s| s.guild).filter(|g| app.ui.guild == Some(*g)) {
        return super::buy::server_shop::show(app, ui, g);
    }
    if let Some(g) = app.home.as_ref().map(|h| h.guild).filter(|g| app.ui.guild == Some(*g)) {
        return super::community::show(app, ui, g);
    }
    let Some(ch) = app.cur() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("No text channels here yet.").color(pal().text_muted));
        });
        return;
    };
    let chan = app.store.channels[&ch].clone();
    if chan.is_voice() {
        if app.voice_chat {
            egui::Panel::right("voice_chat")
                .exact_size(488.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(Color32::BLACK).inner_margin(Margin { left: 8, ..Default::default() }))
                .show(ui, |ui| voice_chat(app, ui, &chan));
        }
        if app.call.pop && app.voice.chan == Some(chan.id) {
            return super::callbar::popped(app, ui, &chan);
        }
        return super::call::room(app, ui, &chan);
    }
    let side = app.side_thread.filter(|t| app.store.channels.get(t).is_some_and(|t| t.parent == chan.id) && ui.available_width() >= 488.0 + 400.0);
    if let Some(t) = side {
        let thread = app.store.channels[&t].clone();
        egui::Panel::right("side_thread")
            .exact_size(488.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::BLACK).inner_margin(Margin { left: 8, ..Default::default() }))
            .show(ui, |ui| super::thread::side(app, ui, &thread));
    }
    header(app, ui, &chan);
    if chan.guild == 0 {
        super::call::dm_strip(app, ui, &chan);
    }
    // before the composer, which would otherwise take the letters as typing
    message_keys(app, ui, &chan);
    let ctx = ui.ctx().clone();
    let room = ui.available_width();
    let members_w = if app.ui.members_w > 0.0 { app.ui.members_w } else { 264.0 };
    if app.search.is_some() {
        let search_w = if app.ui.search_w > 0.0 { app.ui.search_w } else { 420.0 };
        super::nav::keep_size(&ctx, "search", 0, search_w);
        let r = egui::Panel::right("search")
            .default_size(search_w)
            .size_range(300.0..=(room - 320.0).clamp(300.0, 640.0))
            .resizable(true)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::popups::search_panel(app, ui));
        if let Some(w) = app.dragged(&ctx, "search", r.response.rect.width(), search_w) {
            app.ui.search_w = w;
        }
    } else if chan.kind == 1 && !app.ui.hide_dm_profile && !dm_profile_unavailable(app, &ctx, &chan) {
        if let Some(&uid) = chan.recipients.first() {
            let r = egui::Panel::right("dm_profile")
                .exact_size(320.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(pal().bg_primary).inner_margin(8))
                .show(ui, |ui| super::popups::dm_profile(app, ui, uid));
            let pr = r.response.rect;
            ui.painter().vline(pr.left() + 0.5, pr.y_range(), hairline(ui.pixels_per_point(), pal().border_subtle));
        }
    } else if chan.kind == 3 && !app.ui.hide_members {
        let r = egui::Panel::right("group_members")
            .exact_size(members_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::members::group(app, ui, &chan));
        let gr = r.response.rect;
        ui.painter().vline(gr.left() + 0.5, gr.y_range(), hairline(ui.pixels_per_point(), pal().border_subtle));
    } else if chan.guild != 0 && side.is_none() && !app.ui.hide_members {
        super::nav::keep_size(&ctx, "members", 0, members_w);
        let r = egui::Panel::right("members")
            .default_size(members_w)
            .size_range(200.0..=(room - 420.0).clamp(200.0, 480.0))
            .resizable(true)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::members::show(app, ui, chan.guild, chan.is_thread().then_some(chan.id)));
        let mr = r.response.rect;
        ui.painter().vline(mr.left() + 0.5, mr.y_range(), hairline(ui.pixels_per_point(), pal().border_subtle));
        if let Some(w) = app.dragged(&ctx, "members", r.response.rect.width(), members_w) {
            app.ui.members_w = w;
        }
    }
    // a channel opened from elsewhere in a server that only lists the ones you picked
    if chan.guild != 0 && !chan.is_thread() && !chan.resource && !app.store.listed(&chan) {
        egui::Panel::top("unlisted")
            .exact_size(49.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_secondary).inner_margin(Margin { left: 28, right: 16, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("This channel is not on your channel list.").font(reg(14.0)).color(pal().text_normal));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let g = ui.painter().layout_no_wrap("Add to Channel List".into(), medium(14.0), Color32::WHITE);
                        let (b, resp) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 32.0), Sense::click());
                        app.rects.insert("add_to_list".into(), b);
                        ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
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

fn messages(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let ch = chan.id;
    let mut sa = egui::ScrollArea::vertical()
        .id_salt(("chat", ch))
        .animated(false)
        .stick_to_bottom(true)
        .auto_shrink([false, false]);
    let mut to_bottom = false;
    let mut nudge = std::mem::take(&mut app.wheel.chat_delta);
    let fixed = app.scroll_fix.is_some_and(|(c, _)| c == ch);
    if let Some((_, off)) = app.scroll_fix.take_if(|(c, _)| *c == ch) {
        // f32::MAX means "the bottom", which only the content itself knows
        if off == f32::MAX {
            to_bottom = true;
        } else {
            sa = sa.vertical_scroll_offset(off).stick_to_bottom(false);
        }
    }

    let pointer = ui.ctx().pointer_hover_pos();
    app.wheel.chat = Some(ui.max_rect());
    let hover_lock = app.bar.and_then(|(id, r)| pointer.filter(|p| r.contains(*p)).map(|_| id));
    // the hover toolbar floats on its own layer; a wheel over it still scrolls the chat, like discord
    let over_bar = pointer.and_then(|p| ui.ctx().layer_id_at(p)) == Some(egui::LayerId::new(egui::Order::Middle, egui::Id::new("msg_bar")));
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
        focus: app.focus_msg,
        bookmarks: &app.bookmarks,
        invites: &mut app.chat_invites,
        unblocked: &app.unblocked,
        image_menu: false,
        playing: app.embed.as_ref().map(|p| p.key),
        frame_at: None,
    };
    let mut focus_at = None;
    if d.flash.is_some() {
        ctx.request_repaint();
    }
    let heights = &mut app.heights;
    let empty = Vec::new();
    let entry = d.store.msgs.get(&ch);
    let list = entry.map(|e| &e.list).unwrap_or(&empty);
    let at_start = entry.is_some_and(|e| e.at_start);
    let loading = entry.is_some_and(|e| e.loading);
    let failed = entry.is_some_and(|e| e.failed.is_some());

    let mut anchor = 0.0f32;
    let target = app.scroll_to.filter(|(c, _)| *c == ch).map(|(_, id)| id);
    let mut target_y = None;
    // like discord, a channel too short to scroll sits on the composer, not under the header;
    // last frame's height says how much room to leave above it
    let fill_id = egui::Id::new(("chat_fill", ch));
    let natural: f32 = ctx.data(|d| d.get_temp(fill_id)).unwrap_or(f32::MAX);
    let post_forum = d.store.channels.get(&chan.parent).filter(|f| chan.is_thread() && f.is_forum());
    let copied_id = egui::Id::new(("post_copied", ch));
    let copied = ctx.data(|s| s.get_temp::<std::time::Instant>(copied_id)).is_some_and(|t| t.elapsed().as_secs() < 2);
    let mut op_bottom = None;
    let out = crate::theme::auto_scroll(ui, |ui| sa.show_viewport(ui, |ui, vp| {
        if nudge != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, nudge), egui::style::ScrollAnimation::none());
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let fill = (vp.height() - natural).max(0.0);
        ui.add_space(fill);
        let origin = ui.max_rect().top();
        let w = ui.available_width();
        if at_start {
            if welcome(ui, d.store, chan) {
                d.acts.push(Act::EditChannel);
            }
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
            if (loading || list.is_empty()) && !failed {
                crate::ui::motion::paint_spinner(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().text_muted);
            }
        }
        let mut prev: Option<&Message> = None;
        for (i, m) in list.iter().enumerate() {
            let hidden = super::blocked::hides(d.store, m);
            let first = hidden && (i == 0 || !super::blocked::hides(d.store, &list[i - 1]));
            let folded = hidden && !d.unblocked.contains(&m.id);
            if folded && !first {
                continue;
            }
            let y = ui.cursor().top() - origin;
            let start = group_start(prev, m);
            let known = heights.get(&(m.id, start)).copied();
            let h = known.unwrap_or_else(|| if folded { 26.0 } else { estimate(m) });
            if target == Some(m.id) {
                target_y = Some(y);
            }
            if d.focus == Some(m.id) {
                focus_at = Some((y, h));
            }
            if y + h < vp.top() - 400.0 || y > vp.bottom() + 400.0 {
                ui.allocate_space(vec2(w, h));
            } else {
                let top = ui.cursor().top();
                if first {
                    let (ids, ignored) = super::blocked::run(d.store, list, i);
                    if super::blocked::row(ui, ids.len(), ignored, !folded, m.id) {
                        d.acts.push(Act::Blocked(ids, folded));
                    }
                }
                if !folded {
                    message(ui, &mut d, m, prev, start);
                    if let Some(f) = post_forum.filter(|_| m.id == ch) {
                        ui.add_space(8.0);
                        let a = super::forumbar::show(ui, chan, f, Some(m), false, copied);
                        d.acts.extend(a.map(Act::Post));
                        ui.add_space(8.0);
                    }
                }
                if m.id == ch {
                    op_bottom = Some(y + h);
                }
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
    }));

    let stuck = post_forum.is_some() && op_bottom.map_or(!list.is_empty() && !at_start, |b| b <= out.state.offset.y);
    ctx.data_mut(|s| s.insert_temp(egui::Id::new(("op_stuck", ch)), stuck));
    let D { acts, bar, rects, frame_at, .. } = d;
    app.bar = bar;
    if frame_at.is_some() {
        app.embed_at = frame_at;
    }
    app.rects.remove("emoji_tip");
    app.rects.extend(rects);
    let offset = out.state.offset.y;
    app.offsets.insert(ch, offset);
    // the keyboard's message comes into view as it moves, a little room left around it
    if std::mem::take(&mut app.focus_scroll)
        && let Some((y, h)) = focus_at
    {
        let view = out.inner_rect.height();
        let want = if y < offset + 24.0 { Some(y - 24.0) } else if y + h > offset + view - 24.0 { Some(y + h - view + 24.0) } else { None };
        if let Some(w) = want {
            app.scroll_fix = Some((ch, w.max(0.0)));
            ui.ctx().request_discard("focus");
        }
    }
    let at_bottom = offset + out.inner_rect.height() >= out.content_size.y - 4.0;
    // a message arriving grows the content in the same frame something above may change height;
    // if the list was at the bottom it should stay there, not hold the old spot
    let was_bottom = to_bottom || app.pinned.contains(&ch);
    if at_bottom {
        app.pinned.insert(ch);
    } else {
        app.pinned.remove(&ch);
    }
    if anchor.abs() > 0.5 && !at_bottom && (!was_bottom || fixed && !to_bottom) {
        app.scroll_fix = Some((ch, offset + anchor));
        ui.ctx().request_discard("scroll anchor");
    }

    // read first: taking the store mutably every frame would throw away its cached sums
    let page = app.store.msgs.get(&ch).filter(|e| !e.loading).and_then(|e| {
        if offset < 800.0 && !e.at_start && e.fresh && e.failed.is_none() {
            return e.list.iter().find(|m| !m.pending).map(|m| Page::Before(m.id));
        }
        let near_end = offset + out.inner_rect.height() > out.content_size.y - 800.0;
        if near_end && e.detached { e.list.iter().rev().find(|m| !m.pending).map(|m| Page::After(m.id)) } else { None }
    });
    if let Some(page) = page
        && let Some(e) = app.store.msgs.get_mut(&ch)
    {
        e.loading = true;
        app.net.send(Cmd::Messages { channel: ch, page });
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

    let shift = ui.input(|i| i.modifiers.shift);
    for a in acts {
        match a {
            Act::Reply(id) => {
                app.reply = Some(id);
                // shift+click replies without the ping; your own messages never ping
                app.reply_ping = !shift && app.store.find_msg(ch, id).is_some_and(|m| m.author != app.store.me);
                app.focus_composer = true;
            }
            Act::Edit(id) => {
                if let Some(m) = app.store.find_msg(ch, id) {
                    app.editing = Some((id, m.content.clone()));
                }
            }
            Act::Delete(id) => app.delete_msg(ch, id, !shift),
            Act::Pin(id) => app.pin_msg(ch, id, !shift),
            Act::Bookmark(id) => app.bookmark(ch, id),
            Act::CopyId(id) => ui.ctx().copy_text(id.to_string()),
            Act::CopyLink(id) => ui.ctx().copy_text(app.message_link(ch, id)),
            Act::Thread(id) => app.new_thread = Some(super::popups::NewThread::from(app, ch, id)),
            Act::Mention(u) => app.mention(u, chan.guild),
            Act::ClearEmbeds(id) => app.clear_embeds(ch, id, !shift),
            Act::Vote(id, picks) => app.vote(ch, id, picks),
            Act::PlayAudio(url) => app.play_audio(url),
            Act::Player(d) => {
                if matches!(d, super::player::Do::Open { .. }) {
                    app.embed = None;
                }
                super::player::apply(app, ui.ctx(), d)
            }
            Act::EditChannel => app.open_chan_settings(ch),
            Act::Resend(id) => app.resend(ch, id),
            Act::Invite(go) => app.chat_invite(go),
            Act::CopyImage(url) => app.copy_image(url),
            Act::CancelUpload(nonce) => crate::upload::cancel(&nonce),
            Act::Votes(id) => app.votes = Some(super::pollvotes::Votes::new(ch, id)),
            Act::Blocked(ids, show) => {
                for id in ids {
                    if show {
                        app.unblocked.insert(id);
                    } else {
                        app.unblocked.remove(&id);
                    }
                }
            }
            Act::Reactors(id, em, burst, n) => app.ask_reactors(ch, id, &em, burst, n),
            Act::Post(a) => post_do(app, ui.ctx(), chan, a),
            Act::System(go, at) => match go {
                system::Go::Profile(u) => app.open_profile(u, chan.guild, at),
                system::Go::Message(c, id) => app.jump_to(c, id),
                system::Go::Pins => app.open_pins(ch),
                system::Go::Threads => app.open_threads = true,
                system::Go::Channel(c) => app.open(c),
                system::Go::JoinCall(c) => app.join_voice(c, false),
                system::Go::Guild(g) => app.select_guild(Some(g)),
                system::Go::GuildSettings(g) => app.open_server_settings(g),
            },
            Act::Emoji(id, name, animated, at) => app.open_source(super::sourcepop::Pop { id, name, sticker: false, animated, format: 0, at }),
            Act::Sticker(id, name, format, at) => app.open_source(super::sourcepop::Pop { id, name, sticker: true, animated: false, format, at }),
            Act::Role(r, at) => app.role_pop = Some((chan.guild, r, at)),
            Act::Tag(g, at) => app.tag_pop = Some((g, at)),
            Act::React(id, em, add) => app.react(ch, id, em, add),
            Act::Picker(id, at) => app.picker = Some((Pick::React(ch, id), at)),
            Act::Menu(id, at) => app.menu = Some((ch, id, at)),
            Act::Url(u) => app.open_link(ui.ctx(), &u, false),
            Act::Frame(key, u) => super::embedplay::start(app, ui.ctx(), key, &u),
            Act::Masked(u) => app.open_link(ui.ctx(), &u, true),
            Act::Browser(u) => ui.ctx().open_url(egui::OpenUrl::new_tab(u)),
            Act::Channel(c) => app.open_thread(c, false, shift),
            Act::Reveal(id) => {
                app.revealed.insert(id);
            }
            Act::SaveEdit(id, text) => {
                app.editing = None;
                let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
                let text = app.convert_emoji(guild, text.trim());
                if text.is_empty() {
                    app.delete_msg(ch, id, true);
                } else if app.store.find_msg(ch, id).is_some_and(|m| m.content != text) {
                    app.edit_msg(ch, id, text);
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
            Act::Component(id, custom_id, what) => app.press(ch, id, &custom_id, what),
            Act::Premium(sku) => app.open_buy(super::buy::What::Listing(serde_json::json!({ "id": sku }))),
            Act::View(img, open, w, h) => {
                // the message's other pictures come along, for the viewer's arrows
                let all = app.store.msgs.get(&ch).and_then(|e| e.list.iter().find(|m| m.attachments.iter().any(|a| a.proxy == img) || m.embeds.iter().any(|e| e.image.as_ref().or(e.thumb.as_ref()).or(e.video.as_ref()).is_some_and(|x| x.url == img))));
                app.gallery = all.map(gallery).unwrap_or_default();
                app.viewer = Some((img, open, w, h));
            }
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

/// Discord's message keys, while a message has keyboard focus (shift+tab out of the composer):
/// ↑/↓ walk the messages, R replies, E edits, + reacts, P pins, Backspace deletes, F forwards,
/// S reads it out, Ctrl+C copies it, Alt+Enter marks it unread, Shift+F10 opens its menu and
/// Escape or Tab goes back to the composer. Shift skips the pin and delete dialogs. Any other
/// typing goes to the composer, which takes the focus back.
/// Discord greys the profile toggle below 1132 px and while the DM's call fills the top of the chat.
fn dm_profile_unavailable(app: &App, ctx: &egui::Context, chan: &Channel) -> bool {
    ctx.content_rect().width() < 1132.0 || app.voice.chan == Some(chan.id)
}

fn message_keys(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let Some(id) = app.focus_msg else { return };
    if app.layered(ui.ctx()) {
        return;
    }
    // the mouse takes over from the keyboard
    if ui.input(|i| i.pointer.any_pressed()) {
        app.focus_msg = None;
        return;
    }
    let ch = chan.id;
    let list: Vec<(Id, Id)> = app.store.msgs.get(&ch).map(|e| e.list.iter().filter(|m| !m.pending).map(|m| (m.id, m.author)).collect()).unwrap_or_default();
    let Some(at) = list.iter().position(|(m, _)| *m == id) else {
        app.focus_msg = None;
        return;
    };
    let own = list[at].1 == app.store.me;
    let manage = chan.guild == 0 || app.store.perms(chan) & crate::state::MANAGE_MESSAGES != 0;
    let pin = app.store.can_pin(chan);
    let row = ui.data(|s| s.get_temp::<Rect>(egui::Id::new(("row", id)))).unwrap_or(Rect::from_min_size(pos2(400.0, 300.0), vec2(1.0, 1.0)));
    let shift = ui.input(|i| i.modifiers.shift);
    let key = |ui: &mut Ui, m: Modifiers, k: Key| ui.input_mut(|i| super::take_key(i, m, k));
    let typed = ui.input(|i| i.events.iter().find_map(|e| match e {
        egui::Event::Text(t) => Some(t.clone()),
        _ => None,
    }));
    let letter = typed.as_deref().filter(|t| ["r", "e", "+", "p", "f", "s"].contains(&t.to_lowercase().as_str())).map(str::to_string);
    if let Some(t) = &letter {
        ui.input_mut(|i| super::take_text(i, t));
    }
    let moved = if key(ui, Modifiers::NONE, Key::ArrowUp) {
        list.get(at.wrapping_sub(1)).map(|m| m.0)
    } else if key(ui, Modifiers::NONE, Key::ArrowDown) {
        list.get(at + 1).map(|m| m.0)
    } else {
        None
    };
    if let Some(m) = moved {
        app.focus_msg = Some(m);
        app.focus_scroll = true;
        return;
    }
    if key(ui, Modifiers::NONE, Key::Escape) || key(ui, Modifiers::NONE, Key::Tab) {
        app.focus_msg = None;
        app.focus_composer = true;
        return;
    }
    if key(ui, Modifiers::ALT, Key::Enter) {
        app.mark_unread(ch, id);
    } else if key(ui, Modifiers::COMMAND, Key::C) || ui.input_mut(|i| i.events.iter().position(|e| matches!(e, egui::Event::Copy)).map(|p| i.events.remove(p)).is_some()) {
        if let Some(m) = app.store.find_msg(ch, id) {
            ui.ctx().copy_text(m.content.clone());
        }
    } else if key(ui, Modifiers::SHIFT, Key::F10) {
        app.menu = Some((ch, id, row.left_top() + vec2(row.width() * 0.5, 16.0)));
    } else if (own || manage) && (key(ui, Modifiers::NONE, Key::Backspace) || key(ui, Modifiers::SHIFT, Key::Backspace) || key(ui, Modifiers::NONE, Key::Delete) || key(ui, Modifiers::SHIFT, Key::Delete)) {
        app.delete_msg(ch, id, !shift);
    }
    match letter.as_ref().map(|t| t.to_lowercase()).as_deref() {
        Some("r") => {
            app.reply = Some(id);
            app.reply_ping = !own;
            app.focus_msg = None;
            app.focus_composer = true;
        }
        Some("e") if own => {
            if let Some(m) = app.store.find_msg(ch, id) {
                app.editing = Some((id, m.content.clone()));
            }
            app.focus_msg = None;
        }
        Some("+") => app.picker = Some((Pick::React(ch, id), row.right_top() - vec2(360.0, 0.0))),
        Some("p") if pin => app.pin_msg(ch, id, !shift),
        Some("f") => app.forward = Some(super::popups::Forward::new(ch, id)),
        Some("s") => app.speak(ch, id),
        _ => {
            // anything else typed belongs to the composer
            if typed.is_some() && letter.is_none() {
                app.focus_msg = None;
            }
        }
    }
}

/// The thread card's bottom line: the last message in it, or why there's none to show.
fn thread_line(store: &Store, t: &Channel, guild: Id) -> String {
    if t.archived {
        return "There are no recent messages in this thread.".into();
    }
    if t.msg_count == 0 {
        return "There are no messages in this thread yet.".into();
    }
    match store.msgs.get(&t.id).and_then(|e| e.list.last()) {
        Some(m) => format!("{}: {}", store.name_in(guild, m.author), markdown::plain(&m.content, store, guild)),
        None => "There are no recent messages in this thread.".into(),
    }
}

fn post_do(app: &mut App, ctx: &egui::Context, post: &Channel, a: super::forumbar::Do) {
    use super::forumbar::Do;
    let ch = post.id;
    match a {
        Do::React(em, add) => app.react(ch, ch, em, add),
        Do::Picker(at) => app.picker = Some((Pick::React(ch, ch), at)),
        Do::Follow(on) => app.follow_thread(ch, on),
        Do::Link => {
            ctx.copy_text(format!("https://discord.com/channels/{}/{ch}", post.guild));
            ctx.data_mut(|s| s.insert_temp(egui::Id::new(("post_copied", ch)), std::time::Instant::now()));
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        Do::Top => app.thread_top(ch),
    }
}

/// The "N new messages" strip on top and the "Jump To Present" strip at the bottom.
fn bars(app: &mut App, ui: &mut Ui, ch: Id, area: Rect, offset: f32, content_h: f32) {
    if ui.ctx().data(|s| s.get_temp::<bool>(egui::Id::new(("op_stuck", ch)))).unwrap_or(false)
        && let Some(post) = app.store.channels.get(&ch).cloned()
        && let Some(forum) = app.store.channels.get(&post.parent).cloned()
    {
        let r = Rect::from_min_size(area.min, vec2(area.width(), 48.0));
        ui.painter().rect_filled(r, 0, pal().bg_primary);
        let first = app.store.msgs.get(&ch).and_then(|e| e.list.iter().find(|m| m.id == ch)).cloned();
        let copied = ui.ctx().data(|s| s.get_temp::<std::time::Instant>(egui::Id::new(("post_copied", ch)))).is_some_and(|t| t.elapsed().as_secs() < 2);
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(0.0, 8.0))));
        let act = super::forumbar::show(&mut bar, &post, &forum, first.as_ref(), true, copied);
        if let Some(a) = act {
            post_do(app, ui.ctx(), &post, a);
        }
    }
    let detached = app.store.msgs.get(&ch).is_some_and(|e| e.detached);
    if let Some(after) = app.marker.filter(|(c, _)| *c == ch && app.marker_bar).map(|(_, m)| m) {
        let list = app.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default();
        let n = list.iter().filter(|m| m.id > after && !m.pending).count();
        let first = list.iter().find(|m| m.id > after).map(|m| m.id);
        if n > 0 {
            let r = Rect::from_min_size(area.min + vec2(16.0, 0.0), vec2(area.width() - 32.0, 24.0));
            let p = ui.painter();
            p.rect_filled(r, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, pal().blurple);
            let since = markdown::local(crate::state::snowflake_ms(after)).format(markdown::hm()).to_string();
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
    if app.store.msgs.get(&ch).is_some_and(|e| e.failed.is_some()) {
        let red = Color32::from_rgb(255, 160, 155);
        let p = ui.painter();
        let note = p.layout_no_wrap("Messages Failed To Load".into(), medium(14.0), red);
        let label = p.layout_no_wrap("Try Again".into(), semi(14.0), red);
        let w = (16.0 + note.size().x + 12.0 + label.size().x + 16.0 + 8.0).max(284.0);
        let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 34.0), vec2(w, 44.0));
        p.rect(r, 16, Color32::from_rgb(35, 0, 4), Stroke::new(1.0, Color32::from_rgb(235, 95, 94)), egui::StrokeKind::Inside);
        p.galley(pos2(r.left() + 16.0, r.center().y - note.size().y / 2.0), note, red);
        let br = Rect::from_min_max(pos2(r.right() - 8.0 - label.size().x - 16.0, r.top() + 8.0), r.max - vec2(8.0, 8.0));
        let resp = ui.interact(br, egui::Id::new("messages_retry"), Sense::click());
        let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
        p.rect_filled(br, 4, fill);
        p.galley(br.center() - label.size() / 2.0, label, red);
        app.rects.insert("messages:retry".into(), br);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.retry_messages(ch);
        }
        return;
    }
    let scrolled_up = offset + area.height() < content_h - 1500.0;
    app.far_up = scrolled_up.then_some(ch);
    if detached || scrolled_up {
        // discord's pill over the composer: the notice, then a blurple Jump To Present
        let p = ui.painter();
        let note = p.layout_no_wrap("You're Viewing Older Messages".into(), medium(14.0), pal().header_primary);
        let label = p.layout_no_wrap("Jump To Present".into(), semi(14.0), Color32::WHITE);
        let w = 12.0 + note.size().x + 12.0 + label.size().x + 20.0 + 6.0;
        let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 34.0), vec2(w, 44.0));
        p.rect(r, 12, pal().bg_floating, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        p.galley(pos2(r.left() + 12.0, r.center().y - note.size().y / 2.0), note, pal().header_primary);
        let jr = Rect::from_min_max(pos2(r.right() - 6.0 - label.size().x - 20.0, r.top() + 6.0), r.max - vec2(6.0, 6.0));
        let resp = ui.interact(jr, egui::Id::new("jump_present"), Sense::click());
        p.rect_filled(jr, 8, super::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), super::motion::hot(ui.ctx(), &resp)));
        p.galley(jr.center() - label.size() / 2.0, label, Color32::WHITE);
        app.rects.insert("jump_present".into(), jr);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.jump_to_present(ch);
        }
    }
}

/// The top of a channel's history. Hands back a click on its Edit Channel button.
fn welcome(ui: &mut Ui, store: &Store, chan: &Channel) -> bool {
    let mut edit = false;
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
        } else if let Some(forum) = store.channels.get(&chan.parent).filter(|f| chan.is_thread() && f.is_forum()) {
            super::forum::post_intro(ui, chan, forum);
        } else {
            // a voice channel's chat has a bubble and no # before the name
            let voice = chan.is_voice();
            let (r, _) = ui.allocate_exact_size(vec2(68.0, 68.0), Sense::hover());
            if voice {
                ui.painter().circle_filled(r.center(), 34.0, Color32::from_rgba_unmultiplied(151, 151, 159, 20));
                icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(r.center(), vec2(42.0, 42.0)), Color32::WHITE);
            } else {
                ui.painter().circle_filled(r.center(), 34.0, pal().bg_accent);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "#", reg(44.0), Color32::WHITE);
            }
            let hash = if voice { "" } else { "#" };
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Welcome to {hash}{title}!")).font(semi(32.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("This is the start of the {hash}{title} channel.")).color(pal().header_secondary));
            if store.perms(chan) & crate::state::MANAGE_CHANNELS != 0 {
                ui.add_space(12.0);
                let g = ui.painter().layout_no_wrap("Edit Channel".into(), medium(14.0), pal().text_normal);
                let (b, resp) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 32.0), Sense::click());
                let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
                ui.painter().rect(b, 8, fill, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Pencil, Rect::from_min_size(pos2(b.left() + 12.0, b.center().y - 8.0), vec2(16.0, 16.0)), pal().text_normal);
                ui.painter().galley(pos2(b.left() + 32.0, b.center().y - g.size().y / 2.0), g, pal().text_normal);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                edit = resp.clicked();
            }
        }
    });
    edit
}
