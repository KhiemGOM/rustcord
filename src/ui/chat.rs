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
    Jump(Id),
    Component(Id, String, Option<Vec<String>>),
    View(String, String, u32, u32),
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
    if d.date_naive() == now.date_naive() {
        format!("Today at {time}")
    } else if d.date_naive().succ_opt() == Some(now.date_naive()) {
        format!("Yesterday at {time}")
    } else {
        format!("{} {}", d.format("%m/%d/%Y"), d.format(markdown::hm()))
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.ui.guild.is_none() && app.friends {
        return super::friends::show(app, ui);
    }
    let Some(ch) = app.cur() else {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("No text channels here yet.").color(TEXT_MUTED));
        });
        return;
    };
    let chan = app.store.channels[&ch].clone();
    header(app, ui, &chan);
    if app.search.is_some() {
        egui::Panel::right("search")
            .exact_size(420.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG_SECONDARY))
            .show(ui, |ui| super::popups::search_panel(app, ui));
    } else if chan.guild != 0 && !app.ui.hide_members && !chan.is_forum() {
        egui::Panel::right("members")
            .exact_size(240.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG_SECONDARY))
            .show(ui, |ui| super::members::show(app, ui, chan.guild));
    }
    if chan.is_forum() {
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| super::forum::show(app, ui, &chan));
        return;
    }
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::Panel::bottom("composer")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 0 }))
            .show(ui, |ui| super::composer::show(app, ui, &chan));
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| messages(app, ui, &chan));
    });
}

fn header(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("chat_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), Stroke::new(1.0, BG_TERTIARY));
            ui.horizontal_centered(|ui| {
                let title = channel_title(&app.store, chan.id);
                let glyph = |ui: &mut Ui, icon: Icon| {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    icons::draw(ui.painter(), icon, r, CHANNEL_DEFAULT);
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
                            glyph(ui, side::glyph(&p));
                            let r = text::label(ui, &p.name, semi(16.0), HEADER_SECONDARY, Sense::click());
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                app.open(p.id);
                            }
                            let (r, _) = ui.allocate_exact_size(vec2(16.0, 24.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::ChevronRight, r, CHANNEL_DEFAULT);
                        }
                        glyph(ui, Icon::Thread);
                    }
                    _ => glyph(ui, side::glyph(chan)),
                }
                text::label(ui, &title, semi(16.0), HEADER_PRIMARY, Sense::hover());
                let tools = if chan.guild != 0 { 330.0 } else { 220.0 };
                if let Some(topic) = &chan.topic {
                    let (sep, _) = ui.allocate_exact_size(vec2(17.0, 24.0), Sense::hover());
                    ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, BG_ACCENT));
                    let w = (ui.available_width() - tools).max(0.0);
                    ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                        text::label(ui, &topic.replace('\n', " "), reg(14.0), HEADER_SECONDARY, Sense::hover());
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let qid = egui::Id::new("search_q");
                    let mut q: String = ui.data(|d| d.get_temp(qid).unwrap_or_default());
                    let sw = if app.search.is_some() { 240.0 } else { 144.0 };
                    let r = ui
                        .allocate_ui_with_layout(vec2(sw, 24.0), Layout::left_to_right(Align::Center), |ui| {
                            Frame::new()
                                .fill(BG_TERTIARY)
                                .corner_radius(4)
                                .inner_margin(Margin::symmetric(6, 3))
                                .show(ui, |ui| {
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut q)
                                            .frame(Frame::NONE)
                                            .font(reg(14.0))
                                            .hint_text(RichText::new("Search").color(TEXT_MUTED))
                                            .desired_width(sw - 30.0),
                                    );
                                    let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                    icons::draw(ui.painter(), Icon::Search, ir, HEADER_SECONDARY);
                                    r
                                })
                                .inner
                        })
                        .inner;
                    app.rects.insert("search".into(), r.rect);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !q.trim().is_empty() {
                        app.start_search(chan.id, q.trim().to_string());
                    }
                    ui.data_mut(|d| d.insert_temp(qid, q));
                    ui.add_space(8.0);
                    if chan.guild != 0 && !chan.is_forum() {
                        let tip = if app.ui.hide_members { "Show Member List" } else { "Hide Member List" };
                        if icon_button(ui, Icon::People, 28.0, tip).clicked() {
                            app.ui.hide_members = !app.ui.hide_members;
                            app.search = None;
                        }
                    }
                    if !chan.is_forum() {
                        let pins = icon_button(ui, Icon::Pin, 28.0, "Pinned Messages");
                        app.rects.insert("pins_button".into(), pins.rect);
                        if pins.clicked() {
                            match app.pins {
                                Some(_) => app.pins = None,
                                None => app.open_pins(chan.id),
                            }
                        }
                    }
                    if chan.guild != 0 && matches!(chan.kind, 0 | 5) {
                        let t = icon_button(ui, Icon::Thread, 28.0, "Threads");
                        super::popups::threads_menu(app, &t, chan.id);
                    }
                });
            });
        });
}

fn messages(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let ch = chan.id;
    let mut sa = egui::ScrollArea::vertical()
        .id_salt(("chat", ch))
        .stick_to_bottom(true)
        .auto_shrink([false, false]);
    let mut to_bottom = false;
    if let Some((_, off)) = app.scroll_fix.take_if(|(c, _)| *c == ch) {
        // f32::MAX means "the bottom", which only the content itself knows
        if off == f32::MAX {
            to_bottom = true;
        } else {
            sa = sa.vertical_scroll_offset(off);
        }
    }

    let pointer = ui.ctx().pointer_hover_pos();
    let hover_lock = app.bar.and_then(|(id, r)| pointer.filter(|p| r.contains(*p)).map(|_| id));
    let marker = app.marker.filter(|(c, _)| *c == ch).map(|(_, m)| m);
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
    let out = sa.show_viewport(ui, |ui, vp| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let origin = ui.max_rect().top();
        let w = ui.available_width();
        if at_start {
            welcome(ui, d.store, chan);
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
            if loading || list.is_empty() {
                egui::Spinner::new().size(20.0).color(TEXT_MUTED).paint_at(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)));
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
        ui.add_space(12.0);
        if to_bottom {
            ui.scroll_to_cursor(Some(Align::BOTTOM));
        }
    });

    let D { acts, bar, rects, .. } = d;
    app.bar = bar;
    app.rects.extend(rects);
    let offset = out.state.offset.y;
    app.offsets.insert(ch, offset);
    let at_bottom = offset + out.inner_rect.height() >= out.content_size.y - 4.0;
    if anchor.abs() > 0.5 && !at_bottom {
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
                let text = text.trim().to_string();
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
            Act::Jump(id) => app.jump_to(ch, id),
            Act::Component(id, custom_id, values) => app.press(ch, id, &custom_id, values),
            Act::View(img, open, w, h) => app.viewer = Some((img, open, w, h)),
        }
    }
}

/// The "N new messages" strip on top and the "Jump To Present" strip at the bottom.
fn bars(app: &mut App, ui: &mut Ui, ch: Id, area: Rect, offset: f32, content_h: f32) {
    let detached = app.store.msgs.get(&ch).is_some_and(|e| e.detached);
    if let Some(after) = app.marker.filter(|(c, _)| *c == ch).map(|(_, m)| m) {
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
        p.rect_filled(r, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, BG_SECONDARY_ALT);
        p.text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "You're viewing older messages", semi(14.0), HEADER_SECONDARY);
        let jr = Rect::from_min_max(pos2(r.right() - 150.0, r.top()), r.max);
        let resp = ui.interact(jr, egui::Id::new("jump_present"), Sense::click());
        let col = if resp.hovered() { Color32::WHITE } else { HEADER_PRIMARY };
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
                ui.label(RichText::new(&title).font(bold(32.0)).color(HEADER_PRIMARY));
                ui.label(RichText::new(&u.username).font(reg(20.0)).color(HEADER_PRIMARY));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("This is the beginning of your direct message history with {title}."))
                        .color(HEADER_SECONDARY),
                );
            }
        } else {
            let (r, _) = ui.allocate_exact_size(vec2(68.0, 68.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 34.0, BG_ACCENT);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "#", reg(44.0), Color32::WHITE);
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Welcome to #{title}!")).font(bold(32.0)).color(HEADER_PRIMARY));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("This is the start of the #{title} channel.")).color(HEADER_SECONDARY));
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
    let g = ui.painter().layout_no_wrap(text.to_string(), semi(12.0), TEXT_MUTED);
    let gw = g.size().x + 8.0;
    let mid = r.center().x;
    ui.painter().hline(left..=mid - gw / 2.0, y, Stroke::new(1.0, color));
    ui.painter().hline(mid + gw / 2.0..=right, y, Stroke::new(1.0, color));
    ui.painter().galley(pos2(mid - g.size().x / 2.0, y - g.size().y / 2.0), g, TEXT_MUTED);
}

fn message(ui: &mut Ui, d: &mut D, m: &Message, prev: Option<&Message>, start: bool) {
    if let Some(p) = prev
        && day(p.ts()) != day(m.ts())
    {
        ui.add_space(8.0);
        let label = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format("%B %-d, %Y").to_string()).unwrap_or_default();
        divider(ui, &label, BG_ACCENT, false);
    }
    if let Some(after) = d.marker
        && prev.is_some_and(|p| p.id <= after)
        && m.id > after
    {
        divider(ui, "", RED, true);
    }
    if start {
        ui.add_space(14.0);
    }

    let left = ui.max_rect().left();
    let w = ui.available_width();
    let top = ui.cursor().top();
    let bg = ui.painter().add(egui::Shape::Noop);
    let content = Rect::from_min_size(pos2(left + 72.0, top + 2.0), vec2((w - 72.0 - 48.0).max(120.0), 100_000.0));
    let mut cui = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    cui.spacing_mut().item_spacing = vec2(4.0, 0.0);

    let me = d.store.me;
    let mut head_top = top + 2.0;
    if !normal(m.kind) {
        system_line(&mut cui, d, m);
    } else {
        if let Some(r) = m.reply {
            reply_line(&mut cui, d, r, left);
            head_top = cui.cursor().top();
        }
        if start {
            let name = d.store.name_in(d.guild, m.author);
            let color = d.store.role_color(d.guild, m.author).map(role_color).unwrap_or(HEADER_PRIMARY);
            let bot = d.store.user(m.author).is_some_and(|u| u.bot);
            if let Some(at) = author_line(&mut cui, &name, color, bot, &stamp(m.ts())) {
                d.acts.push(Act::Profile(m.author, at));
            }
        }
        let editing = d.editing.as_ref().is_some_and(|(id, _)| *id == m.id);
        if editing {
            edit_box(&mut cui, d, m.id);
        } else if !m.content.is_empty() {
            let color = if m.failed {
                RED
            } else if m.pending {
                TEXT_MUTED
            } else {
                TEXT_NORMAL
            };
            let cx = Cx {
                store: d.store,
                ctx: d.ctx,
                guild: d.guild,
                size: 16.0,
                color,
                revealed: d.revealed.contains(&m.id),
                edited: m.edited,
            };
            let blocks = markdown::parse(&m.content, &cx);
            blocks_ui(&mut cui, &blocks, d, m.id);
        }
        for a in &m.attachments {
            cui.add_space(4.0);
            if a.is_image() {
                let sep = if a.proxy.contains('?') { '&' } else { '?' };
                let (w, h) = fit(a.w, a.h, 550.0, 350.0, cui.available_width());
                let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                let r = media(&mut cui, &url, w, h);
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
                cui.label(RichText::new(format!("[sticker: {name}]")).color(TEXT_MUTED));
            } else {
                let ext = if *fmt == 4 { "gif" } else { "png" };
                let url = format!("https://media.discordapp.net/stickers/{id}.{ext}?size=160");
                media(&mut cui, &url, 160.0, 160.0).on_hover_text(name);
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
                .fill(BG_SECONDARY)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(12, 8))
                .show(&mut cui, |ui| {
                    ui.set_max_width(400.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&t.name).font(semi(14.0)).color(HEADER_PRIMARY));
                        ui.label(RichText::new(format!("{} Messages ›", t.msg_count)).font(semi(14.0)).color(LINK));
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
        if !m.reactions.is_empty() {
            cui.add_space(4.0);
            reactions(&mut cui, d, m);
        }
    }

    let bottom = cui.min_rect().bottom().max(top + 22.0) + 2.0;
    let row = Rect::from_min_max(pos2(left, top), pos2(left + w, bottom));
    ui.advance_cursor_after_rect(row);

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
        if hovered { Color32::from_rgba_premultiplied(30, 23, 8, 36) } else { HIGHLIGHT_BG }
    } else if let Some((_, t)) = d.flash.filter(|(id, _)| *id == m.id) {
        // jumped-to message glows and fades out over two seconds
        let a = (1.0 - t.elapsed().as_secs_f32() / 2.0).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(88, 101, 242, (40.0 * a) as u8)
    } else if hovered || d.editing.as_ref().is_some_and(|(id, _)| *id == m.id) {
        BG_MSG_HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().set(bg, egui::epaint::RectShape::filled(row, CornerRadius::ZERO, fill));

    if start && normal(m.kind) {
        if let Some(u) = d.store.user(m.author) {
            let ar = Rect::from_min_size(pos2(left + 16.0, head_top), vec2(40.0, 40.0));
            avatar(ui, ar, &u.avatar_url());
            let r = ui.interact(ar, egui::Id::new(("avatar", m.id)), Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                d.acts.push(Act::Profile(m.author, ar.right_top() + vec2(8.0, 0.0)));
            }
        }
    } else if hovered && normal(m.kind) {
        let t = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format(markdown::hm()).to_string()).unwrap_or_default();
        ui.painter().text(pos2(left + 64.0, top + 13.0), Align2::RIGHT_CENTER, t, reg(11.0), TEXT_MUTED);
    }

    if hovered && !m.pending && !m.failed {
        action_bar(d, m, row);
        if ui.input(|i| i.pointer.secondary_clicked())
            && let Some(p) = ui.ctx().pointer_interact_pos()
        {
            d.acts.push(Act::Menu(m.id, p));
        }
    }
}

/// Name, optional APP tag and timestamp on one 22px line. The timestamp shares the
/// name's baseline rather than its centre, which is what makes discord's header read right.
/// Returns where to open the profile popout when the name was clicked.
fn author_line(ui: &mut Ui, name: &str, color: Color32, bot: bool, when: &str) -> Option<Pos2> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let n = text::layout(ui.ctx(), name, semi(16.0), color, row.width() - 160.0);
    let ny = row.center().y - n.size().y / 2.0;
    let name_rect = Rect::from_min_size(pos2(row.left(), ny), n.size());
    let resp = ui.interact(name_rect, ui.id().with(("author", name)), Sense::click());
    n.paint(ui.painter(), name_rect.min);
    if resp.hovered() {
        ui.painter().hline(name_rect.x_range(), name_rect.bottom() - 2.0, Stroke::new(1.0, color));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut x = name_rect.right() + 4.0;
    // ascent is ~81% of a segoe ui row, use it to put both baselines on the same y
    let base = ny + n.size().y * 0.81;
    if bot {
        let g = ui.painter().layout_no_wrap("APP".into(), semi(10.0), Color32::WHITE);
        let r = Rect::from_min_size(pos2(x, base - 12.5), vec2(g.size().x + 8.0, 15.0));
        ui.painter().rect_filled(r, CornerRadius::same(3), BLURPLE);
        ui.painter().galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
        x = r.right() + 4.0;
    }
    let t = ui.painter().layout_no_wrap(when.to_string(), reg(12.0), TEXT_MUTED);
    let ty = base - t.size().y * 0.81;
    ui.painter().galley(pos2(x + 2.0, ty), t, TEXT_MUTED);
    resp.clicked().then(|| name_rect.right_top() + vec2(8.0, 0.0))
}

fn system_line(ui: &mut Ui, d: &mut D, m: &Message) {
    let name = d.store.name_in(d.guild, m.author);
    let (icon, text, col) = match m.kind {
        7 => (Icon::ArrowRight, format!("{name} joined the server."), GREEN),
        6 => (Icon::Pin, format!("{name} pinned a message to this channel."), HEADER_SECONDARY),
        8..=11 => (Icon::Gem, format!("{name} just boosted the server!"), Color32::from_rgb(0xff, 0x73, 0xfa)),
        1 => (Icon::ArrowRight, format!("{name} added someone to the group."), GREEN),
        2 => (Icon::ArrowLeft, format!("{name} removed someone from the group."), RED),
        3 => (Icon::Speaker, format!("{name} started a call."), GREEN),
        4 => (Icon::Pencil, format!("{name} changed the channel name."), HEADER_SECONDARY),
        18 => (Icon::Thread, format!("{name} started a thread: {}", m.content), HEADER_SECONDARY),
        _ => (Icon::ArrowRight, if m.content.is_empty() { format!("{name} did something.") } else { m.content.clone() }, HEADER_SECONDARY),
    };
    let left = ui.max_rect().left();
    let y = ui.cursor().top() + 11.0;
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(left - 36.0, y), vec2(16.0, 16.0)), col);
    ui.horizontal_wrapped(|ui| {
        text::label(ui, &text, reg(16.0), HEADER_SECONDARY, Sense::hover());
        ui.add(Label::new(RichText::new(stamp(m.ts())).font(reg(12.0)).color(TEXT_MUTED)).selectable(false));
    });
}

fn reply_line(ui: &mut Ui, d: &mut D, rid: Id, row_left: f32) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    let spine = Stroke::new(2.0, BG_ACCENT);
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
        p.text(pos2(r.left(), y), Align2::LEFT_CENTER, "Original message was deleted", reg(14.0), TEXT_MUTED);
        return;
    };
    let mut cx = r.left();
    if let Some(u) = d.store.user(rm.author) {
        avatar(ui, Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0)), &u.avatar_url());
        cx += 20.0;
    }
    let name = format!("@{}", d.store.name_in(d.guild, rm.author));
    let color = d.store.role_color(d.guild, rm.author).map(role_color).unwrap_or(HEADER_PRIMARY);
    let nr = text::paint(p, pos2(cx, y), Align2::LEFT_CENTER, &name, semi(14.0), color, 200.0);
    cx = nr.right() + 6.0;
    let excerpt = markdown::plain(&rm.content, d.store, d.guild);
    if excerpt.is_empty() {
        let ir = Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0));
        icons::draw(p, Icon::Image, ir, HEADER_SECONDARY);
        cx += 20.0;
        p.text(pos2(cx, y), Align2::LEFT_CENTER, "Click to see attachment", reg(14.0), HEADER_SECONDARY);
    } else {
        text::paint(p, pos2(cx, y), Align2::LEFT_CENTER, &excerpt, reg(14.0), HEADER_SECONDARY, r.right() - cx);
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
                    Image::from_uri(url.clone()).show_loading_spinner(false).paint_at(ui, rect);
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
                ui.add_space(4.0);
                Frame::new()
                    .fill(BG_SECONDARY)
                    .stroke(Stroke::new(1.0, BG_TERTIARY))
                    .corner_radius(4)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(Label::new(RichText::new(code).font(mono(14.0)).color(TEXT_NORMAL)).wrap());
                    });
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
                    BG_ACCENT,
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

fn media(ui: &mut Ui, url: &str, w: f32, h: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(8), BG_SECONDARY);
    Image::from_uri(url.to_string())
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
    ui.painter().rect(rect, CornerRadius::same(8), BG_SECONDARY, Stroke::new(1.0, BG_TERTIARY), egui::StrokeKind::Inside);
    let ir = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(30.0, 36.0));
    icons::draw(ui.painter(), Icon::File, ir, BLURPLE);
    if video {
        icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(ir.center() + vec2(0.0, 3.0), vec2(12.0, 12.0)), BLURPLE);
    }
    let clip = ui.painter().with_clip_rect(rect.shrink(8.0));
    let col = if resp.hovered() { LINK.gamma_multiply(1.2) } else { LINK };
    text::paint(ui.painter(), pos2(rect.left() + 52.0, rect.center().y - 9.0), Align2::LEFT_CENTER, name, reg(16.0), col, rect.width() - 64.0);
    clip.text(pos2(rect.left() + 52.0, rect.center().y + 11.0), Align2::LEFT_CENTER, human_size(size), reg(12.0), TEXT_MUTED);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn embed(ui: &mut Ui, d: &mut D, e: &Embed, msg: Id) {
    let bare = e.title.is_none() && e.desc.is_none() && e.author.is_none() && e.fields.is_empty();
    if bare && matches!(e.kind.as_str(), "image" | "gifv") {
        if let Some(m) = e.thumb.as_ref().or(e.image.as_ref()) {
            let (w, h) = fit(m.w, m.h, 400.0, 300.0, ui.available_width());
            if media(ui, &m.url, w, h).clicked() {
                let open = e.url.clone().unwrap_or_else(|| m.url.clone());
                d.acts.push(Act::View(m.url.clone(), open, m.w, m.h));
            }
        }
        return;
    }
    let maxw = (ui.available_width() - 4.0).min(516.0);
    let bar = e.color.map(role_color).unwrap_or(BG_TERTIARY);
    let start = ui.cursor().min;
    let thumb = e.thumb.as_ref().filter(|_| e.kind != "video" && e.image.is_none());
    let r = Frame::new()
        .fill(BG_SECONDARY)
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
                        ui.label(RichText::new(p).font(reg(12.0)).color(HEADER_SECONDARY));
                    }
                    if let Some(a) = &e.author {
                        ui.label(RichText::new(a).font(semi(14.0)).color(HEADER_PRIMARY));
                    }
                    if let Some(t) = &e.title {
                        let col = if e.url.is_some() { LINK } else { HEADER_PRIMARY };
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
                        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: TEXT_NORMAL, revealed: d.revealed.contains(&msg), edited: false };
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
                    media(ui, &t.url, w, h);
                }
            });
            if let Some(m) = e.image.as_ref().or(if e.kind == "video" { e.thumb.as_ref() } else { None }) {
                ui.add_space(8.0);
                let (w, h) = fit(m.w, m.h, 400.0, 300.0, ui.available_width());
                let r = media(ui, &m.url, w, h);
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
                ui.label(RichText::new(f).font(reg(12.0)).color(HEADER_SECONDARY));
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
        ui.label(RichText::new(&f.0).font(semi(14.0)).color(HEADER_PRIMARY));
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: TEXT_NORMAL, revealed: d.revealed.contains(&msg), edited: false };
        let blocks = markdown::parse(&f.1, &cx);
        blocks_ui(ui, &blocks, d, msg);
    });
}

pub fn emoji_url(e: &ReactEmoji) -> String {
    if e.id != 0 {
        format!("{}/emojis/{}.{}?size=48", crate::api::cdn(), e.id, if e.animated { "gif" } else { "png" })
    } else {
        markdown::twemoji(&e.name)
    }
}

fn components(ui: &mut Ui, d: &mut D, m: &Message) {
    for row in &m.components {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for c in row {
                match c {
                    Comp::Button { style, label, emoji, custom_id, url, disabled } => {
                        let (fill, hover) = match style {
                            1 => (BLURPLE, Color32::from_rgb(0x47, 0x52, 0xc4)),
                            3 => (Color32::from_rgb(0x24, 0x80, 0x46), Color32::from_rgb(0x1a, 0x63, 0x34)),
                            4 => (Color32::from_rgb(0xda, 0x37, 0x3c), Color32::from_rgb(0xa1, 0x28, 0x28)),
                            _ => (BG_ACCENT, Color32::from_rgb(0x6d, 0x6f, 0x78)),
                        };
                        let g = ui.painter().layout_no_wrap(label.clone(), semi(14.0), Color32::WHITE);
                        let ew = if emoji.is_some() { 22.0 } else { 0.0 };
                        let lw = if url.is_some() { 18.0 } else { 0.0 };
                        let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + ew + lw + 32.0, 32.0), Sense::click());
                        let fill = if *disabled {
                            fill.gamma_multiply(0.5)
                        } else if resp.hovered() {
                            hover
                        } else {
                            fill
                        };
                        ui.painter().rect_filled(r, CornerRadius::same(3), fill);
                        if ui.clip_rect().contains_rect(r) {
                            d.rects.push((format!("button:{custom_id}"), r));
                        }
                        let mut x = r.left() + 16.0;
                        if let Some(e) = emoji {
                            text::image(ui.painter(), &emoji_url(e), Rect::from_min_size(pos2(x, r.center().y - 9.0), vec2(18.0, 18.0)));
                            x += ew;
                        }
                        let tc = if *disabled { Color32::from_white_alpha(140) } else { Color32::WHITE };
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
                        ui.painter().rect(r, CornerRadius::same(4), BG_TERTIARY, Stroke::new(1.0, if open { BLURPLE } else { BG_TERTIARY }), egui::StrokeKind::Inside);
                        ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, placeholder, reg(15.0), TEXT_MUTED);
                        icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(18.0, 18.0)), HEADER_SECONDARY);
                        if resp.clicked() && !disabled {
                            open = !open;
                        }
                        if open {
                            for (label, value, desc) in options {
                                let h = if desc.is_some() { 48.0 } else { 36.0 };
                                let (or, oresp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                                ui.painter().rect_filled(or, CornerRadius::ZERO, if oresp.hovered() { BG_MOD_HOVER } else { BG_SECONDARY });
                                ui.painter().text(pos2(or.left() + 12.0, or.top() + 18.0), Align2::LEFT_CENTER, label, semi(15.0), HEADER_PRIMARY);
                                if let Some(dsc) = desc {
                                    ui.painter().text(pos2(or.left() + 12.0, or.top() + 36.0), Align2::LEFT_CENTER, dsc, reg(13.0), HEADER_SECONDARY);
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
            let g = ui.painter().layout_no_wrap(count, semi(14.0), if r.me { MENTION_FG } else { HEADER_SECONDARY });
            let size = vec2(6.0 + 16.0 + 6.0 + g.size().x + 8.0, 24.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let (fill, stroke) = if r.me {
                (Color32::from_rgba_premultiplied(13, 15, 36, 38), BLURPLE)
            } else if resp.hovered() {
                (BG_SECONDARY, BG_ACCENT)
            } else {
                (BG_SECONDARY, Color32::TRANSPARENT)
            };
            ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            let er = Rect::from_min_size(pos2(rect.left() + 7.0, rect.center().y - 8.0), vec2(16.0, 16.0));
            Image::from_uri(emoji_url(&r.emoji)).show_loading_spinner(false).paint_at(ui, er);
            ui.painter().galley(pos2(er.right() + 6.0, rect.center().y - g.size().y / 2.0), g, HEADER_SECONDARY);
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
    let n = if own { 4.0 } else { 2.0 };
    let size = vec2(n * 32.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    let rect = Rect::from_min_size(pos, size);
    d.bar = Some((m.id, rect));
    egui::Area::new(egui::Id::new("msg_bar"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(d.ctx, |ui| {
            Frame::new()
                .fill(BG_PRIMARY)
                .stroke(Stroke::new(1.0, BG_TERTIARY))
                .corner_radius(4)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let r = icon_button(ui, Icon::AddReaction, 32.0, "Add Reaction");
                        if r.clicked() {
                            d.acts.push(Act::Picker(m.id, r.rect.left_bottom()));
                        }
                        if own {
                            if icon_button(ui, Icon::Pencil, 32.0, "Edit").clicked() {
                                d.acts.push(Act::Edit(m.id));
                            }
                        }
                        if icon_button(ui, Icon::Reply, 32.0, "Reply").clicked() {
                            d.acts.push(Act::Reply(m.id));
                        }
                        if own && icon_button(ui, Icon::Trash, 32.0, "Delete").clicked() {
                            d.acts.push(Act::Delete(m.id));
                        }
                    });
                });
        });
}

fn edit_box(ui: &mut Ui, d: &mut D, id: Id) {
    let Some((_, text)) = d.editing.as_mut() else { return };
    let tid = egui::Id::new("edit_box");
    let enter = ui.memory(|m| m.has_focus(tid)) && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let esc = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    ui.add_space(4.0);
    Frame::new().fill(BG_INPUT).corner_radius(8).inner_margin(Margin::symmetric(16, 11)).show(ui, |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(text)
                .id(tid)
                .frame(Frame::NONE)
                .desired_rows(1)
                .font(reg(16.0))
                .text_color(TEXT_NORMAL)
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
        ui.label(small("escape to", TEXT_NORMAL));
        if ui.add(Label::new(small("cancel", LINK)).sense(Sense::click())).clicked() {
            d.acts.push(Act::CancelEdit);
        }
        ui.label(small("• enter to", TEXT_NORMAL));
        if ui.add(Label::new(small("save", LINK)).sense(Sense::click())).clicked() {
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
    let author = m.author;
    let pinned = m.pinned;
    let dev = app.settings.dev;
    let mut pick = None;
    let area = egui::Area::new(egui::Id::new("msg_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(BG_FLOATING).corner_radius(4).inner_margin(6).show(ui, |ui| {
                ui.set_width(188.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                let item = |ui: &mut Ui, label: &str, danger: bool| {
                    let (r, resp) = ui.allocate_exact_size(vec2(188.0, 32.0), Sense::click());
                    let hot = resp.hovered();
                    if hot {
                        ui.painter().rect_filled(r, CornerRadius::same(2), if danger { RED } else { BLURPLE });
                    }
                    let col = if hot { Color32::WHITE } else if danger { RED } else { TEXT_NORMAL };
                    ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, label, reg(14.0), col);
                    resp.clicked()
                };
                if item(ui, "Add Reaction", false) {
                    pick = Some(0);
                }
                if own && item(ui, "Edit Message", false) {
                    pick = Some(1);
                }
                if item(ui, "Reply", false) {
                    pick = Some(2);
                }
                if item(ui, if pinned { "Unpin Message" } else { "Pin Message" }, false) {
                    pick = Some(6);
                }
                if item(ui, "Mark Unread", false) {
                    pick = Some(7);
                }
                if item(ui, "Copy Text", false) {
                    pick = Some(3);
                }
                if item(ui, "Copy Message Link", false) {
                    pick = Some(8);
                }
                if dev && item(ui, "Copy Message ID", false) {
                    pick = Some(4);
                }
                if dev && item(ui, "Copy User ID", false) {
                    pick = Some(9);
                }
                if own && item(ui, "Delete Message", true) {
                    pick = Some(5);
                }
            });
        });
    let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() && pick.is_none();
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
        Some(7) => {
            // ack the message before this one so it shows as the first unread
            let prev = app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.id < id)).map(|m| m.id).unwrap_or(id - 1);
            let r = app.store.reads.entry(ch).or_default();
            r.last_acked = prev;
            app.rest("POST", format!("/channels/{ch}/messages/{prev}/ack"), Some(serde_json::json!({ "manual": true, "token": null })), crate::net::Tag::Quiet);
            app.marker = Some((ch, prev));
        }
        Some(8) => {
            let g = app.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0);
            let g = g.map(|g| g.to_string()).unwrap_or_else(|| "@me".into());
            ctx.copy_text(format!("https://discord.com/channels/{g}/{ch}/{id}"));
        }
        Some(9) => ctx.copy_text(author.to_string()),
        _ => {}
    }
    if pick.is_some() || clicked_out || ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.menu = None;
    }
}

/// Compact read-only rendering used by pins and search results.
pub fn preview(ui: &mut Ui, store: &Store, m: &Message) -> egui::Response {
    let ctx = ui.ctx().clone();
    let guild = store.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
    let revealed = HashSet::new();
    let mut editing = None;
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
    };
    let r = Frame::new().fill(BG_PRIMARY).corner_radius(4).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            let (av, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            if let Some(u) = store.user(m.author) {
                avatar(ui, av, &u.avatar_url());
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let color = store.role_color(guild, m.author).map(role_color).unwrap_or(HEADER_PRIMARY);
                    ui.label(RichText::new(store.name_in(guild, m.author)).font(semi(16.0)).color(color));
                    ui.label(RichText::new(stamp(m.ts())).font(reg(12.0)).color(TEXT_MUTED));
                });
                let cx = Cx { store, ctx: &ctx, guild, size: 15.0, color: TEXT_NORMAL, revealed: false, edited: m.edited };
                let blocks = markdown::parse(&m.content, &cx);
                blocks_ui(ui, &blocks, &mut d, m.id);
                for a in &m.attachments {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::draw(ui.painter(), Icon::File, r, LINK);
                        text::label(ui, &a.filename, reg(14.0), LINK, Sense::hover());
                    });
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
