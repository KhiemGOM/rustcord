use chrono::{Datelike, Local, TimeZone};
use egui::text::CCursor;
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Image, Key, Label, Layout, Margin, Modifiers, Pos2, Rect, RichText,
    Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use super::{App, Pick, avatar, channel_title, icon_button};
use super::icons::{self, Icon};
use super::{side, system, text};
use crate::gif;
use crate::markdown::{self, Block, Cx, Link};
use crate::net::Cmd;
use crate::state::{Channel, Comp, Embed, Id, Message, Page, ReactEmoji, Set, Store};
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
    if let Some(p) = &m.poll {
        h += 92.0 + p.answers.len() as f32 * 50.0;
    }
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
    if app.ui.guild.is_none() && app.quests.is_some() {
        return super::quests::show(app, ui);
    }
    if let Some(g) = app.boosts.filter(|g| app.ui.guild == Some(*g)) {
        return super::boosts::show(app, ui, g);
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
    } else if chan.guild != 0 && side.is_none() && !app.ui.hide_members && room >= members_w + 420.0 {
        let r = egui::Panel::right("members")
            .default_size(members_w)
            .size_range(200.0..=(room - 420.0).clamp(200.0, 480.0))
            .resizable(true)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| super::members::show(app, ui, chan.guild, chan.is_thread().then_some(chan.id)));
        let mr = r.response.rect;
        ui.painter().vline(mr.left() + 0.5, mr.y_range(), Stroke::new(1.0, pal().border_subtle));
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

/// A voice channel's own text chat beside the call.
fn voice_chat(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let mut close = false;
    side_card(app, ui, chan, "voice_chat", |app, ui, h| {
        let y = h.top() + 25.0;
        icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(pos2(h.left() + 26.0, y), vec2(20.0, 20.0)), pal().channel_default);
        text::paint_em(ui.painter(), pos2(h.left() + 45.0, y), Align2::LEFT_CENTER, &chan.name, medium(16.0), pal().header_primary, h.width() - 100.0, 1.0);
        let x = Rect::from_center_size(pos2(h.right() - 25.0, y), vec2(20.0, 20.0));
        let r = ui.interact(x.expand(4.0), ui.id().with("voice_chat_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if r.hovered() { pal().interactive_hover } else { pal().header_secondary });
        app.rects.insert("voice_chat:close".into(), x);
        close = r.on_hover_text("Close").clicked();
    });
    if close {
        app.voice_chat = false;
    }
}

/// A channel's chat in discord's side card, rounded at the top left with a hairline down its
/// left and along its top: the header `head` draws into, then the channel as any other.
pub fn side_card(app: &mut App, ui: &mut Ui, chan: &Channel, key: &'static str, head: impl FnOnce(&mut App, &mut Ui, Rect)) {
    let card = ui.max_rect();
    let round = CornerRadius { nw: 12, ..Default::default() };
    ui.painter().rect_filled(card, round, pal().bg_primary);
    let edge = Rect::from_min_max(card.min, card.max + vec2(4.0, 4.0));
    ui.painter().with_clip_rect(card).rect_stroke(edge, round, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    egui::Panel::top(egui::Id::new((key, "head")))
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let h = ui.max_rect();
            ui.painter().hline(h.left() + 1.0..=h.right(), h.bottom() - 0.5, Stroke::new(1.0, pal().border_subtle));
            head(app, ui, h);
        });
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::Panel::bottom(egui::Id::new((key, "composer")))
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 0 }))
            .show(ui, |ui| super::composer::show(app, ui, chan));
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| messages(app, ui, chan));
    });
    app.rects.insert(key.into(), card);
}

fn header(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("chat_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        // discord's search box sits 8 from the window's edge
        // and everything centres in the 48 above the border
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.left() - 16.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            ui.horizontal_centered(|ui| {
                let title = channel_title(&app.store, chan.id);
                // discord's header icons are 20, the name 9 past them
                let glyph = |ui: &mut Ui, icon: Icon| {
                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    icons::draw(ui.painter(), icon, r, pal().channel_default);
                    ui.add_space(4.0);
                };
                let chan_glyph = |ui: &mut Ui, store: &crate::state::Store, c: &Channel| {
                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    side::chan_icon(store, ui.painter(), c, r, pal().channel_default);
                    ui.add_space(4.0);
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
                            let r = text::label(ui, &p.name, medium(16.0), pal().header_secondary, Sense::click());
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
                    // discord draws the header's emoji with the system font, as in the channel list
                    let t = text::layout_em(ui.ctx(), &title, medium(16.0), pal().header_primary, ui.available_width(), 1.0);
                    let (r, _) = ui.allocate_exact_size(t.size(), Sense::hover());
                    t.paint(ui.painter(), pos2(r.min.x, r.min.y.floor()));
                    if super::follow::can_follow(app, chan) {
                        super::follow::header_button(app, ui, chan);
                    }
                });
                if let Some(topic) = chan.topic.as_ref().filter(|t| !t.is_empty()) {
                    let (sep, _) = ui.allocate_exact_size(vec2(17.0, 24.0), Sense::hover());
                    ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, pal().bg_accent));
                    let w = (ui.available_width() - tools).max(0.0);
                    ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                        let r = text::label(ui, &topic.replace('\n', " "), reg(14.0), pal().header_secondary, Sense::click());
                        app.rects.insert("header:topic".into(), r.rect);
                        if r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            app.topic = Some(chan.id);
                        }
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
                                .inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 5 })
                                .show(ui, |ui| {
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut q)
                                            .id(egui::Id::new("search_box"))
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
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if chan.is_thread() {
                        super::thread::more(app, ui, chan, false);
                    }
                    if chan.kind == 1 {
                        let tip = if app.ui.hide_dm_profile { "Show User Profile" } else { "Hide User Profile" };
                        if super::toggle_button(ui, Icon::UserProfile, 32.0, tip, !app.ui.hide_dm_profile).clicked() {
                            app.ui.hide_dm_profile = !app.ui.hide_dm_profile;
                            app.search = None;
                        }
                    }
                    if chan.guild != 0 || chan.kind == 3 {
                        let tip = if app.ui.hide_members { "Show Member List" } else { "Hide Member List" };
                        if super::toggle_button(ui, Icon::People, 32.0, tip, !app.ui.hide_members).clicked() {
                            app.ui.hide_members = !app.ui.hide_members;
                            app.search = None;
                        }
                    }
                    if chan.guild == 0 {
                        let add = icon_button(ui, Icon::PersonAdd, 32.0, "Add Friends to DM");
                        app.rects.insert("dm:add".into(), add.rect);
                        if add.clicked() {
                            app.group_pick = Some(super::dmpick::Pick::new(Some(chan.id)));
                        }
                    }
                    if !chan.is_forum() {
                        let pins = super::toggle_button(ui, Icon::Pin, 32.0, "Pinned Messages", app.pins.is_some());
                        if app.store.pins_unread(chan.id) {
                            // discord's new-pins dot sits on the icon's lower right, cut out of it
                            let c = pins.rect.center() + vec2(4.5, 4.5);
                            ui.painter().circle_filled(c, 6.0, pal().bg_primary);
                            if pins.hovered() {
                                ui.painter().circle_filled(c, 6.0, pal().control_hover);
                            }
                            ui.painter().circle_filled(c, 4.0, BLURPLE);
                            app.rects.insert("pins_dot".into(), Rect::from_center_size(c, vec2(8.0, 8.0)));
                        }
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
                    if chan.guild != 0 && (matches!(chan.kind, 0 | 5) || chan.is_thread()) {
                        let t = icon_button(ui, Icon::Thread, 32.0, "Threads");
                        app.rects.insert("threads_button".into(), t.rect);
                        if std::mem::take(&mut app.open_threads) {
                            egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&t));
                        }
                        super::popups::threads_menu(app, &t, if chan.is_thread() { chan.parent } else { chan.id });
                    }
                    if chan.guild == 0 {
                        super::call::video_button(app, ui, chan.id);
                        super::call::call_button(app, ui, chan.id);
                    }
                });
            });
        });
}

/// The header's notification bell: Discord's menu to mute the channel or pick its level.
pub fn bell(app: &mut App, ui: &mut Ui, chan: &Channel) {
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
    // shift+click mutes or unmutes straight away, no menu
    if b.clicked() && ui.input(|i| i.modifiers.shift) {
        egui::Popup::close_all(ui.ctx());
        app.mute_for(chan.id, false, !muted, None);
        return;
    }
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
    nudge += std::mem::take(&mut app.wheel.chat_delta);
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

    let mut anchor = 0.0f32;
    let target = app.scroll_to.filter(|(c, _)| *c == ch).map(|(_, id)| id);
    let mut target_y = None;
    // like discord, a channel too short to scroll sits on the composer, not under the header;
    // last frame's height says how much room to leave above it
    let fill_id = egui::Id::new(("chat_fill", ch));
    let natural: f32 = ctx.data(|d| d.get_temp(fill_id)).unwrap_or(f32::MAX);
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
            if loading || list.is_empty() {
                crate::ui::motion::paint_spinner(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().text_muted);
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
            if d.focus == Some(m.id) {
                focus_at = Some((y, h));
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
    }));

    let D { acts, bar, rects, .. } = d;
    app.bar = bar;
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
    if anchor.abs() > 0.5 && !at_bottom && !was_bottom {
        app.scroll_fix = Some((ch, offset + anchor));
        ui.ctx().request_discard("scroll anchor");
    }

    // read first: taking the store mutably every frame would throw away its cached sums
    let page = app.store.msgs.get(&ch).filter(|e| !e.loading).and_then(|e| {
        if offset < 800.0 && !e.at_start && e.fresh {
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
            Act::EditChannel => app.open_chan_settings(ch),
            Act::Resend(id) => app.resend(ch, id),
            Act::Reactors(id, em, burst, n) => app.ask_reactors(ch, id, &em, burst, n),
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
            Act::Emoji(id, name, animated, at) => app.emoji_pop = Some((id, name, animated, at)),
            Act::Role(r, at) => app.role_pop = Some((chan.guild, r, at)),
            Act::Tag(g, at) => app.tag_pop = Some((g, at)),
            Act::React(id, em, add) => app.react(ch, id, em, add),
            Act::Picker(id, at) => app.picker = Some((Pick::React(ch, id), at)),
            Act::Menu(id, at) => app.menu = Some((ch, id, at)),
            Act::Url(u) => app.open_link(ui.ctx(), &u, false),
            Act::Masked(u) => app.open_link(ui.ctx(), &u, true),
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
        Some("p") if manage => app.pin_msg(ch, id, !shift),
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
        p.rect_filled(jr, 8, super::motion::mix(BLURPLE, BLURPLE.gamma_multiply(0.85), super::motion::hot(ui.ctx(), &resp)));
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

/// A rule across the chat at `y`, takes no room: the date in the middle if there is one, and
/// the red NEW tab on the end for the first unread message.
fn divider(ui: &Ui, y: f32, text: &str, unread: bool) {
    let r = ui.max_rect();
    let left = r.left() + 16.0;
    let right = r.right() - 14.0;
    let y = y + 0.5;
    let color = if unread { RED } else { pal().bg_accent };
    if text.is_empty() {
        ui.painter().hline(left..=right, y, Stroke::new(1.0, color));
    } else {
        let g = ui.painter().layout_no_wrap(text.to_string(), semi(12.0), Color32::WHITE);
        let gw = g.size().x + 8.0;
        let mid = r.center().x;
        ui.painter().hline(left..=mid - gw / 2.0, y, Stroke::new(1.0, color));
        ui.painter().hline(mid + gw / 2.0..=right, y, Stroke::new(1.0, color));
        let ink = if unread { RED } else { pal().text_muted };
        ui.painter().galley(pos2(mid - g.size().x / 2.0, y - g.size().y / 2.0), g, ink);
    }
    if unread {
        let g = ui.painter().layout_no_wrap("NEW".into(), bold(10.0), Color32::WHITE);
        let pr = Rect::from_min_size(pos2(right - g.size().x - 5.0, y - 7.0), vec2(g.size().x + 5.0, 13.0));
        ui.painter().rect_filled(pr, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, RED);
        // the tab's pointed end
        let tip = vec![pos2(pr.left() - 7.0, pr.center().y), pos2(pr.left() + 0.5, pr.top()), pos2(pr.left() + 0.5, pr.bottom())];
        ui.painter().add(egui::Shape::convex_polygon(tip, RED, Stroke::NONE));
        ui.painter().galley(pos2(pr.left() + 1.0, pr.center().y - g.size().y / 2.0), g, Color32::WHITE);
    }
}

fn message(ui: &mut Ui, d: &mut D, m: &Message, prev: Option<&Message>, start: bool) {
    if m.kind == 21 && let Some(r) = m.reply.and_then(|r| d.store.refs.get(&r)) {
        return message(ui, d, r, None, true);
    }
    let sys = if normal(m.kind) {
        None
    } else {
        let Some(sys) = system::describe(d.store, d.guild, m) else { return };
        Some(sys)
    };
    let compact = d.s.compact;
    let font = d.s.font_size;
    // discord's group spacing N is N+1 above a group
    let gs = d.s.group_space;
    let mut gap = gs + 1.0;
    let unread = d.marker.is_some_and(|after| prev.is_some_and(|p| p.id <= after) && m.id > after);
    if let Some(p) = prev
        && day(p.ts()) != day(m.ts())
    {
        // the date sits 24 under the last message, then 24-N or the group's gap, whichever's more
        ui.add_space(24.0);
        let label = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format("%B %-d, %Y").to_string()).unwrap_or_default();
        divider(ui, ui.cursor().top(), &label, unread);
        ui.add_space(1.0);
        gap = gap.max(24.0 - gs);
    } else if unread && start {
        // over the gap above a group, 8 under the last message
        divider(ui, ui.cursor().top() + 8.0, "", true);
    } else if unread {
        ui.add_space(4.0);
        divider(ui, ui.cursor().top(), "", true);
        ui.add_space(5.0);
    }
    if start {
        ui.add_space(gap);
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
    let mut tail = 0.0;
    if let Some(sys) = sys {
        system_line(&mut cui, d, m, sys);
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
            let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: font, color, revealed, edited: m.edited && !m.content.trim().is_empty(), embed: false };
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
        // discord's accessories: 2 under the text, 4 apart, 2 under the last
        let mut acc = 2.0;
        for a in &m.attachments {
            cui.add_space(acc);
            acc = 4.0;
            if a.is_image() && d.s.inline_uploads {
                let sep = if a.proxy.contains('?') { '&' } else { '?' };
                let (w, h) = fit(a.w, a.h, 550.0, 350.0, cui.available_width());
                let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                let r = media(&mut cui, d, &url, &a.url, w, h, None);
                if d.s.image_descriptions && !a.description.is_empty() {
                    alt_badge(&cui, &r, &a.description);
                }
                if cui.clip_rect().contains(r.rect.center()) {
                    d.rects.push(("image".into(), r.rect.intersect(cui.clip_rect())));
                }
                if r.clicked() {
                    d.acts.push(Act::View(a.proxy.clone(), a.url.clone(), a.w, a.h));
                }
            } else if a.is_audio() {
                let voice = m.flags & (1 << 13) != 0 || !a.waveform.is_empty();
                audio_card(&mut cui, d, a, voice);
            } else if file_card(&mut cui, &a.filename, a.size).clicked() {
                d.acts.push(Act::Url(a.url.clone()));
            }
        }
        for (id, name, fmt) in &m.stickers {
            cui.add_space(acc);
            acc = 4.0;
            if *fmt == 3 {
                cui.label(RichText::new(format!("[sticker: {name}]")).color(pal().text_muted));
            } else {
                let ext = if *fmt == 4 { "gif" } else { "png" };
                let url = format!("https://media.discordapp.net/stickers/{id}.{ext}?size=160");
                media(&mut cui, d, &url, &url, 160.0, 160.0, None).on_hover_text(name);
            }
        }
        let embeds = if d.embeds { m.embeds.as_slice() } else { &[] };
        for (i, e) in embeds.iter().enumerate() {
            cui.add_space(acc);
            acc = 4.0;
            let r = embed(&mut cui, d, e, m.id);
            // discord's ✕ beside the first of your own embeds, to take them all off for everyone
            if let Some(r) = r.filter(|_| i == 0 && hot && m.author == me && !d.popout) {
                let x = Rect::from_min_size(pos2(r.right() + 4.0, r.top()), vec2(16.0, 16.0));
                let resp = cui.interact(x, cui.id().with(("clear_embeds", m.id)), Sense::click());
                icons::draw(cui.painter(), Icon::Close, x, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
                d.rects.push(("clear_embeds".into(), x));
                if resp.on_hover_text("Remove all embeds").clicked() {
                    d.acts.push(Act::ClearEmbeds(m.id));
                }
            }
        }
        if m.poll.is_some() {
            cui.add_space(acc);
            acc = 4.0;
            poll(&mut cui, d, m);
        }
        if let Some(t) = m.thread.and_then(|t| d.store.channels.get(&t)) {
            cui.add_space(acc);
            acc = 4.0;
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
            cui.add_space(acc);
            acc = 4.0;
            components(&mut cui, d, m);
        }
        if m.edited && m.content.trim().is_empty() {
            cui.add_space(2.0);
            cui.label(RichText::new("(edited)").font(reg(10.0)).color(pal().text_muted));
        }
        if !m.reactions.is_empty() && d.s.reactions {
            cui.add_space(acc);
            acc = 4.0;
            reactions(&mut cui, d, m);
        }
        if acc > 2.0 {
            tail = 2.0;
        }
    }

    let bottom = cui.min_rect().bottom().max(top + 22.0) + 2.0 + tail;
    let row = Rect::from_min_max(pos2(left, top), pos2(left + w, bottom));
    ui.advance_cursor_after_rect(row);
    ui.data_mut(|s| s.insert_temp(row_id, row));
    // on screen top to bottom; a panel dragged to a fractional width can leave the row a hair
    // wider than the clip
    let clip = ui.clip_rect();
    if row.top() >= clip.top() && row.bottom() <= clip.bottom() && normal(m.kind) && !m.pending {
        d.rects.push(("msg:last".into(), row));
        if m.author != me {
            d.rects.push(("msg:other".into(), row));
        }
    }

    let hovered = match d.hover_lock {
        Some(id) => id == m.id,
        None => ui.rect_contains_pointer(row),
    };
    let fill = if d.store.mentions_me(m) && m.author != me {
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(2.0, row.height())), CornerRadius::ZERO, pal().mention_bar);
        // discord's hover over a mention is the lighter of the two
        if hovered { pal().highlight_hover } else { pal().highlight_bg }
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
    if d.focus == Some(m.id) {
        ui.painter().rect_stroke(row.shrink(1.0), CornerRadius::same(4), Stroke::new(2.0, FOCUS), egui::StrokeKind::Inside);
        d.rects.push(("msg:focus".into(), row));
    }

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
                d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, ar.right_top() + vec2(8.0, 0.0)) });
            }
            if r.secondary_clicked() {
                d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(ar.center())));
            }
        }
    } else if hovered && normal(m.kind) {
        let t = Local.timestamp_millis_opt(m.ts()).single().map(|t| t.format(markdown::hm()).to_string()).unwrap_or_default();
        ui.painter().text(pos2(left + 56.0, top + 13.0), Align2::RIGHT_CENTER, t, medium(11.0), pal().text_muted);
    }

    if hovered && !m.pending {
        if m.failed {
            failed_bar(d, m, row);
        } else {
            // alt+click marks unread from this message on, a long-standing discord shortcut
            if ui.input(|i| i.pointer.primary_clicked() && i.modifiers.alt) {
                d.acts.push(Act::MarkUnread(m.id));
            }
            action_bar(d, m, row);
        }
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
pub(super) struct Ink {
    pub stops: Vec<Color32>,
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
        self.layout_em(ctx, name, font, max_w, hot, 1.25)
    }

    /// `em` as in `text::layout_em`: 1.0 draws emoji with the system font, as the lists do.
    pub(super) fn layout_em(&self, ctx: &egui::Context, name: &str, font: egui::FontId, max_w: f32, hot: bool, em: f32) -> text::Rich {
        if self.stops.len() < 2 {
            return text::layout_em(ctx, name, font, self.stops[0], max_w, em);
        }
        let rich = text::layout_em(ctx, name, font, Color32::WHITE, max_w, em);
        let phase = if hot && self.tile { ctx.input(|i| i.time) as f32 * 100.0 / 1.5 } else { 0.0 };
        if phase > 0.0 {
            ctx.request_repaint();
        }
        let w = rich.size().x.max(1.0);
        rich.tint(self.at(w, phase))
    }

    pub(super) fn paint(&self, p: &egui::Painter, rich: &text::Rich, pos: Pos2, hot: bool) {
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

/// A gradient role's colours, repeating along the name.
pub(super) fn grad_ink(r: &crate::state::Role) -> Ink {
    // the light theme darkens gradient names to 85% brightness
    let k = if pal().dark { 1.0 } else { 0.85 };
    let dim = |c: u32| {
        let c = role_color(c);
        Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8)
    };
    let mut stops: Vec<Color32> = r.grad.iter().map(|c| dim(*c)).collect();
    stops.push(stops[0]);
    Ink { stops, tile: true, glow: false }
}

/// The ink for an author's name, and the role colour for the dot some people prefer.
fn name_ink(d: &D, who: Id) -> (Ink, Option<Color32>) {
    use crate::cache::RoleColors::*;
    // discord paints a nitro style's colours only outside servers; in one, roles colour the name
    // and the style lends just its font
    if d.guild == 0
        && d.s.name_styles
        && let Some((fx, cols)) = d.store.user(who).and_then(|u| u.style.clone())
    {
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
            Some(r) if r.grad.len() >= 2 => (grad_ink(r), None),
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
    let face = name_font(d.store.user(m.author).map_or(0, |u| u.font), font);
    let n = ink.layout(ui.ctx(), name, face, row.width() - 160.0, hot);
    // an odd-height galley centred in the 22 row lands on a half pixel and rounds a row low
    let ny = (row.center().y - n.size().y / 2.0).floor();
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
    let cy = row.center().y;
    let user = d.store.user(m.author);
    if let Some(r) = user.filter(|u| !u.bot).and_then(|u| super::tag_chip(ui.painter(), u, pos2(x, cy), 1.0)) {
        if super::tagpop::hit(ui, r, ("msg_tag", m.id))
            && let Some((_, g, _)) = user.and_then(|u| u.tag.as_ref())
        {
            d.acts.push(Act::Tag(*g, r.left_bottom() + vec2(0.0, 8.0)));
        }
        x = r.right() + 4.0;
    }
    if let Some((r, url)) = d.store.role_icon(d.guild, m.author).and_then(|r| Some((r, r.icon_url(40)?))) {
        let ir = Rect::from_min_size(pos2(x, row.top() + 1.0), vec2(20.0, 20.0));
        text::image(ui.painter(), &url, ir);
        ui.interact(ir, ui.id().with(("role_icon", m.id)), Sense::hover()).on_hover_text(&r.name);
        if vis {
            d.rects.push((format!("role_icon:{}", m.author), ir));
        }
        x = ir.right();
    }
    if let Some(u) = user.filter(|u| u.bot) {
        x = super::app_tag(ui.painter(), pos2(x, cy), u.flags & (1 << 16) != 0, 1.0).right() + 4.0;
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
        d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, name_rect.right_top() + vec2(8.0, 0.0)) });
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
        d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, r.rect.right_top() + vec2(8.0, 0.0)) });
    }
    if r.secondary_clicked() {
        d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(r.rect.center())));
    }
}

fn small_button(ui: &mut Ui, text: &str) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), medium(14.0), pal().header_primary);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 32.0), Sense::click());
    let fill = if resp.hovered() { pal().control_hover } else { pal().control };
    ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn system_line(ui: &mut Ui, d: &mut D, m: &Message, sys: system::Sys) {
    let left = ui.max_rect().left();
    let y = ui.cursor().top() + 11.0;
    icons::draw(ui.painter(), sys.icon, Rect::from_center_size(pos2(left - 36.0, y), vec2(16.0, 16.0)), sys.tint);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in &sys.spans {
            let (text, color, go) = match s {
                system::Span::Text(t) => (t, pal().header_secondary, None),
                system::Span::Bold(t) => (t, pal().header_primary, None),
                system::Span::Name(t, id) => (t, pal().header_primary, Some(system::Go::Profile(*id))),
                system::Span::Link(t, go) => (t, pal().link, Some(go.clone())),
            };
            let font = if matches!(s, system::Span::Text(_)) { reg(16.0) } else { medium(16.0) };
            let r = ui.add(Label::new(RichText::new(text).font(font).color(color)).selectable(false).sense(if go.is_some() { Sense::click() } else { Sense::hover() }));
            if let Some(go) = go {
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.clicked() {
                    d.acts.push(Act::System(go, r.rect.left_bottom()));
                }
            }
        }
        if let Some((label, go)) = &sys.button {
            ui.add_space(8.0);
            if small_button(ui, label).clicked() {
                d.acts.push(Act::System(go.clone(), Pos2::ZERO));
            }
        }
        ui.add_space(4.0);
        ui.add(Label::new(RichText::new(stamp(m.ts())).font(medium(12.0)).color(pal().chat_muted)).selectable(false));
    });
    match sys.card {
        system::Card::None => {}
        system::Card::Poll { winner, poll } => {
            ui.add_space(4.0);
            Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(432.0));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&winner.0).font(semi(16.0)).color(pal().header_primary));
                            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::CircleCheck, r, GREEN);
                        });
                        ui.label(RichText::new(format!("Winning answer \u{2022} {}%", winner.1)).font(reg(12.0)).color(pal().header_secondary));
                    });
                    if let Some((c, id)) = poll {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_button(ui, "View Poll").clicked() {
                                d.acts.push(Act::System(system::Go::Message(c, id), Pos2::ZERO));
                            }
                        });
                    }
                });
            });
        }
        system::Card::AutoMod { content, keyword, rule } => {
            ui.add_space(4.0);
            Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(520.0));
                if !content.is_empty() {
                    ui.label(RichText::new(content).font(reg(14.0)).color(pal().text_normal));
                }
                for (k, v) in [("Keyword", keyword), ("Rule", rule)] {
                    if let Some(v) = v {
                        ui.label(RichText::new(format!("{k}: {v}")).font(reg(12.0)).color(pal().header_secondary));
                    }
                }
            });
        }
    }
}

fn reply_line(ui: &mut Ui, d: &mut D, rid: Id, row_left: f32) {
    // discord's reply is 18 tall with 4 under it; the spine's 2px border starts at its middle and
    // runs down to its bottom
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
    ui.add_space(4.0);
    let spine = Stroke::new(2.0, pal().spine);
    let x = row_left + 36.0;
    let y = r.center().y;
    let sy = y + 1.0;
    let p = ui.painter();
    p.line_segment([pos2(x, r.bottom()), pos2(x, sy + 6.0)], spine);
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [pos2(x, sy + 6.0), pos2(x, sy), pos2(x, sy), pos2(x + 6.0, sy)],
        false,
        Color32::TRANSPARENT,
        spine,
    ));
    p.line_segment([pos2(x + 6.0, sy), pos2(r.left() - 4.0, sy)], spine);

    let Some(rm) = d.store.find_msg(d.ch, rid) else {
        // discord: a small reply arrow, then the notice in italics
        icons::draw(p, Icon::Reply, Rect::from_center_size(pos2(r.left() + 6.0, y), vec2(12.0, 12.0)), pal().text_muted);
        let mut job = egui::text::LayoutJob::default();
        let lost = if d.popout { "Message could not be loaded" } else { "Original message was deleted" };
        job.append(lost, 0.0, egui::TextFormat { font_id: italic(14.0), color: pal().text_muted, ..Default::default() });
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
    if let Some(r) = d.store.user(rm.author).filter(|u| !u.bot).and_then(|u| super::tag_chip(p, u, pos2(cx, y), 1.0)) {
        cx = r.right() + 4.0;
    }
    let excerpt = markdown::plain(&rm.content, d.store, d.guild);
    if excerpt.is_empty() {
        let ir = Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0));
        icons::draw(p, Icon::Image, ir, pal().header_secondary);
        cx += 20.0;
        p.text(pos2(cx, y), Align2::LEFT_CENTER, "Click to see attachment", reg(14.0), pal().header_secondary);
    } else {
        let t = text::paint(p, pos2(cx, y), Align2::LEFT_CENTER, &excerpt, reg(14.0), pal().header_secondary, r.right() - cx - 50.0);
        cx = t.right() + 4.0;
    }
    if rm.edited {
        p.text(pos2(cx, y + 1.0), Align2::LEFT_CENTER, "(edited)", reg(10.0), pal().text_muted);
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
                let under = ui.painter().add(egui::Shape::Noop);
                let resp = ui.add(Label::new(galley.clone()).sense(Sense::click()));
                let origin = resp.rect.min;
                let mut fills = Vec::new();
                for (r, fill, line, h) in &t.pills {
                    let a = galley.pos_from_cursor(CCursor { prefer_next_row: true, ..CCursor::new(r.start) });
                    let b = galley.pos_from_cursor(CCursor::new(r.end));
                    let piece = |l: f32, r: f32, row: Rect| {
                        let top = origin.y + row.center().y - line / 2.0;
                        egui::Shape::rect_filled(Rect::from_min_max(pos2(origin.x + l, top), pos2(origin.x + r, top + h)), CornerRadius::same(3), *fill)
                    };
                    if a.min.y == b.min.y {
                        fills.push(piece(a.min.x - 2.0, b.min.x + 2.0, a));
                    } else if let (Some(ra), Some(rb)) = (
                        galley.rows.iter().find(|row| row.min_y() == a.min.y).map(|row| row.rect()),
                        galley.rows.iter().find(|row| row.min_y() == b.min.y).map(|row| row.rect()),
                    ) {
                        // wrapped onto the next row: padded only at its two ends
                        fills.push(piece(a.min.x - 2.0, ra.max.x, a));
                        fills.push(piece(rb.min.x, b.min.x + 2.0, b));
                    }
                }
                ui.painter().set(under, egui::Shape::Vec(fills));
                // where links sit, for the e2e driver to click
                for (r, l) in &t.links {
                    if matches!(l, Link::Url(_) | Link::Masked(_)) {
                        let a = galley.pos_from_cursor(CCursor::new(r.start));
                        let at = Rect::from_min_size(origin + a.min.to_vec2(), vec2(8.0, a.height()));
                        if ui.clip_rect().contains_rect(at) {
                            d.rects.push(("link".into(), at));
                        }
                    }
                }
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
                        if let Link::Emoji(_, name, _) = l {
                            egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), ui.id().with(("emoji_tip", msg)), egui::PopupAnchor::Pointer).show(|ui| {
                                ui.label(RichText::new(format!(":{name}:")).font(semi(14.0)).color(pal().header_primary));
                                ui.label(RichText::new("Click to learn more").font(reg(12.0)).color(pal().header_secondary));
                            });
                        }
                        if resp.clicked() {
                            match l {
                                Link::Url(u) => d.acts.push(Act::Url(u.clone())),
                                Link::Masked(u) => d.acts.push(Act::Masked(u.clone())),
                                Link::Channel(c) => d.acts.push(Act::Channel(*c)),
                                Link::Role(r) => d.acts.push(Act::Role(*r, p + vec2(8.0, 8.0))),
                                Link::Spoiler => d.acts.push(Act::Reveal(msg)),
                                Link::User(u) if ui.input(|i| i.modifiers.shift) => d.acts.push(Act::Mention(*u)),
                                Link::User(u) => {
                                    let at = p + vec2(8.0, 0.0);
                                    d.acts.push(Act::Profile(*u, at));
                                }
                                Link::Emoji(id, name, animated) => d.acts.push(Act::Emoji(*id, name.clone(), *animated, p + vec2(8.0, 8.0))),
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

/// The ALT tag on an image that has a description; hovering it reads the description out.
fn alt_badge(ui: &Ui, image: &egui::Response, text: &str) {
    let tag = Rect::from_min_size(image.rect.left_bottom() + vec2(8.0, -28.0), vec2(36.0, 20.0));
    ui.painter().rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(160));
    ui.painter().text(tag.center(), Align2::CENTER_CENTER, "ALT", bold(11.0), Color32::WHITE);
    ui.interact(tag, ui.id().with(("alt", image.id)), Sense::hover()).on_hover_text(text);
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
    let (auto, on_hover) = if url.contains("/stickers/") {
        use crate::cache::Animate::*;
        match d.s.stickers {
            Always => (true, true),
            OnInteraction => (false, true),
            Never => (false, false),
        }
    } else {
        (d.s.gif_autoplay, true)
    };
    let play = (auto || (on_hover && resp.hovered())) && ui.is_rect_visible(rect);
    Image::from_uri(gif::uri(ui.ctx(), url, play))
        .corner_radius(CornerRadius::same(8))
        .show_loading_spinner(false)
        .paint_at(ui, rect);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Discord's audio / voice-message player: a pill with a play-pause button, the amplitude
/// waveform (or a flat track for plain audio) that fills as it plays, and the running time.
fn audio_card(ui: &mut Ui, d: &mut D, a: &crate::state::Attachment, voice: bool) {
    let w = ui.available_width().min(if voice { 340.0 } else { 420.0 });
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, 31));
    // play / pause button
    let btn = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(32.0, 32.0));
    let pr = ui.interact(btn, ui.id().with(("aud", &a.url)), Sense::click());
    ui.painter().circle_filled(btn.center(), 16.0, if pr.hovered() { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE });
    let playing = crate::audioplay::is_playing(&a.url);
    if playing {
        for dx in [-3.5, 3.5] {
            ui.painter().rect_filled(Rect::from_center_size(btn.center() + vec2(dx, 0.0), vec2(3.5, 13.0)), CornerRadius::same(1), Color32::WHITE);
        }
    } else {
        icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(btn.center() + vec2(1.0, 0.0), vec2(18.0, 18.0)), Color32::WHITE);
    }
    let prog = crate::audioplay::progress(&a.url).unwrap_or(0.0);
    // the waveform / track
    let bar = Rect::from_min_max(pos2(btn.right() + 10.0, rect.top() + 14.0), pos2(rect.right() - 54.0, rect.bottom() - 14.0));
    let bars = decode_waveform(&a.waveform);
    if voice && !bars.is_empty() {
        let n = bars.len();
        let step = bar.width() / n as f32;
        for (i, amp) in bars.iter().enumerate() {
            let h = (amp / 255.0 * bar.height()).max(2.0);
            let x = bar.left() + i as f32 * step;
            let past = (i as f32 + 0.5) / n as f32 <= prog;
            let c = if past { Color32::WHITE } else { pal().text_muted };
            ui.painter().rect_filled(Rect::from_center_size(pos2(x + step / 2.0, bar.center().y), vec2((step - 1.5).max(1.5), h)), CornerRadius::same(1), c);
        }
    } else {
        ui.painter().rect_filled(Rect::from_min_size(bar.left_center() - vec2(0.0, 2.0), vec2(bar.width(), 4.0)), CornerRadius::same(2), pal().text_muted.gamma_multiply(0.4));
        ui.painter().rect_filled(Rect::from_min_size(bar.left_center() - vec2(0.0, 2.0), vec2(bar.width() * prog, 4.0)), CornerRadius::same(2), Color32::WHITE);
        ui.painter().circle_filled(pos2(bar.left() + bar.width() * prog, bar.center().y), 5.0, Color32::WHITE);
    }
    // time: counts up while playing, else the clip length
    let secs = if prog > 0.0 && a.duration > 0.0 { a.duration * prog } else { a.duration };
    let t = format!("{}:{:02}", secs as u32 / 60, secs as u32 % 60);
    ui.painter().text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, t, medium(12.0), pal().header_secondary);
    if pr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if pr.clicked() || (resp.clicked() && !voice) {
        // ogg/opus plays inline; anything else (an mp3 upload) opens in the browser
        d.acts.push(if a.playable() { Act::PlayAudio(a.url.clone()) } else { Act::Url(a.url.clone()) });
    }
    if crate::audioplay::progress(&a.url).is_some() {
        ui.ctx().request_repaint();
    }
}

fn decode_waveform(b64: &str) -> Vec<f32> {
    use base64::Engine;
    if b64.is_empty() {
        return Vec::new();
    }
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map(|bytes| bytes.iter().map(|b| *b as f32).collect())
        .unwrap_or_default()
}

fn human_size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} bytes"),
        1024..1_048_576 => format!("{:.2} KB", n as f64 / 1024.0),
        _ => format!("{:.2} MB", n as f64 / 1_048_576.0),
    }
}

/// Discord's card for a file it doesn't show inline, videos it can't play included: its
/// file glyph, the name as a link that underlines under the pointer, and the size.
fn file_card(ui: &mut Ui, name: &str, size: u64) -> egui::Response {
    let w = ui.available_width().min(432.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 66.0), Sense::click());
    ui.painter().rect(rect, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), Icon::File, Rect::from_min_size(rect.min + vec2(16.0, 21.0), vec2(24.0, 24.0)), BLURPLE);
    let n = text::paint(ui.painter(), pos2(rect.left() + 52.0, rect.top() + 23.0), Align2::LEFT_CENTER, name, reg(14.0), pal().link, rect.width() - 64.0);
    if resp.hovered() {
        ui.painter().hline(n.x_range(), n.bottom() - 1.0, Stroke::new(1.0, pal().link));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().with_clip_rect(rect.shrink(8.0)).text(pos2(rect.left() + 52.0, rect.top() + 44.0), Align2::LEFT_CENTER, human_size(size), reg(12.0), pal().text_muted);
    resp
}

/// Everything in a message the image viewer can step through: its pictures, then its embeds'.
fn gallery(m: &Message) -> Vec<(String, String, u32, u32)> {
    let pics = m.attachments.iter().filter(|a| a.is_image()).map(|a| (a.proxy.clone(), a.url.clone(), a.w, a.h));
    let embeds = m.embeds.iter().filter_map(|e| {
        let x = e.image.as_ref().or(e.thumb.as_ref())?;
        Some((x.url.clone(), e.url.clone().unwrap_or_else(|| x.url.clone()), x.w, x.h))
    });
    pics.chain(embeds).collect()
}

/// Discord drops the text of a message that is nothing but the link its image or gifv embed came from.
fn link_only(m: &Message) -> bool {
    let c = m.content.trim();
    m.embeds.iter().any(|e| matches!(e.kind.as_str(), "image" | "gifv") && e.url.as_deref() == Some(c))
}

fn embed(ui: &mut Ui, d: &mut D, e: &Embed, msg: Id) -> Option<Rect> {
    let bare = e.title.is_none() && e.desc.is_none() && e.author.is_none() && e.fields.is_empty();
    let media_ok = d.s.inline_links;
    // gifv is always just the looping clip, whatever title the provider gave it
    let gifv = e.kind == "gifv";
    if gifv || (bare && e.kind == "image") {
        if !media_ok {
            return None;
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
            return Some(r.rect);
        }
        return None;
    }
    // discord's: a 4px bar, a 1px border round the rest, the text 12 in from the bar and 16 from
    // the right, 2 from the top and 16 from the bottom, each part 8 under the last. It's as wide
    // as its longest unwrapped line, up to 488 of text.
    let inner = (ui.available_width() - 33.0).min(488.0);
    let bar = e.color.map(role_color).unwrap_or(pal().border_normal);
    let start = ui.cursor().min;
    let thumb = e.thumb.as_ref().filter(|_| media_ok && e.kind != "video" && e.image.is_none());
    let text_w = inner - if thumb.is_some() { 96.0 } else { 0.0 };
    let desc = e.desc.as_ref().map(|t| {
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false, embed: true };
        markdown::parse(t, &cx)
    });
    let widest = desc
        .iter()
        .flatten()
        .map(|b| match b {
            Block::Text(t) => ui.fonts_mut(|f| f.layout_job(t.job.clone())).size().x,
            _ => 0.0,
        })
        .chain(e.title.iter().map(|t| ui.fonts_mut(|f| f.layout_no_wrap(t.clone(), semi(16.0), Color32::WHITE)).size().x))
        .fold(0.0, f32::max);
    let r = Frame::new()
        .fill(pal().bg_secondary)
        .corner_radius(4)
        .inner_margin(Margin { left: 16, right: 17, top: 3, bottom: 17 })
        .show(ui, |ui| {
            ui.set_max_width(inner);
            ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(text_w);
                    if widest > text_w {
                        ui.set_min_width(text_w);
                    }
                    if let Some(p) = &e.provider {
                        ui.add_space(8.0);
                        ui.label(RichText::new(p).font(reg(12.0)).line_height(Some(16.0)).color(pal().text_normal));
                    }
                    if let Some(a) = &e.author {
                        ui.add_space(8.0);
                        ui.label(RichText::new(a).font(semi(14.0)).line_height(Some(22.0)).color(pal().header_primary));
                    }
                    if let Some(t) = &e.title {
                        ui.add_space(8.0);
                        let col = if e.url.is_some() { pal().link } else { pal().header_primary };
                        let r = ui.add(Label::new(RichText::new(t).font(semi(16.0)).line_height(Some(22.0)).color(col)).sense(Sense::click()));
                        if e.url.is_some() && r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked()
                            && let Some(u) = &e.url
                        {
                            d.acts.push(Act::Url(u.clone()));
                        }
                    }
                    if let Some(blocks) = &desc {
                        ui.add_space(8.0);
                        blocks_ui(ui, blocks, d, msg);
                    }
                    // the fields are a grid 8 apart, three inline ones to a row
                    let mut row: Vec<&(String, String, bool)> = Vec::new();
                    let flush = |ui: &mut Ui, d: &mut D, row: &mut Vec<&(String, String, bool)>| {
                        if row.is_empty() {
                            return;
                        }
                        ui.add_space(8.0);
                        let cw = ((ui.available_width() + 8.0) / row.len() as f32).max(80.0);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
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
                            ui.add_space(8.0);
                            field(ui, d, f, msg);
                        }
                    }
                    flush(ui, d, &mut row);
                });
                if let Some(t) = thumb {
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        let (w, h) = fit(t.w, t.h, 80.0, 80.0, 80.0);
                        media(ui, d, &t.url, &t.url, w, h, None);
                    });
                }
            });
            if let Some(m) = e.image.as_ref().or(if e.kind == "video" { e.thumb.as_ref() } else { None }).filter(|_| media_ok) {
                ui.add_space(16.0);
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
            let when = e.timestamp.map(|t| {
                let s = stamp(t);
                if s.contains(',') || s.starts_with("Yesterday") { s } else { format!("Today at {s}") }
            });
            let foot = match (&e.footer, when) {
                (Some(f), Some(w)) => Some(format!("{f} • {w}")),
                (f, w) => f.clone().or(w),
            };
            if let Some(f) = foot {
                ui.add_space(8.0);
                ui.label(RichText::new(f).font(reg(12.0)).line_height(Some(16.0)).color(pal().text_normal));
            }
        });
    let rect = r.response.rect;
    // over the fill, as css draws it, so the border picks up the embed's colour
    ui.painter().rect_stroke(rect, 4, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().rect_filled(
        Rect::from_min_max(start, pos2(start.x + 4.0, rect.bottom())),
        CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
        bar,
    );
    Some(rect)
}

fn field(ui: &mut Ui, d: &mut D, f: &(String, String, bool), msg: Id) {
    ui.vertical(|ui| {
        ui.label(RichText::new(&f.0).font(semi(14.0)).line_height(Some(18.0)).color(pal().header_primary));
        ui.add_space(2.0);
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false, embed: true };
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
        // a lone components-v2 node (text, gallery, container, …) lays out on its own;
        // everything else is a v1 action row of buttons and selects
        if let [c] = row.as_slice() {
            if !matches!(c, Comp::Button { .. } | Comp::Select { .. }) {
                render_comp(ui, d, m, c, ui.available_width().min(520.0));
                continue;
            }
        }
        render_row(ui, d, m, row);
    }
}

/// One components-v2 node, laid out like discord's: text as markdown, galleries as images,
/// containers as a bordered box with an optional accent bar, sections with a side accessory.
fn render_comp(ui: &mut Ui, d: &mut D, m: &Message, c: &Comp, width: f32) {
    match c {
        Comp::Row(children) => render_row(ui, d, m, children),
        Comp::Text(s) => {
            let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 16.0, color: pal().text_normal, revealed: true, edited: false, embed: false };
            let blocks = markdown::parse(s, &cx);
            blocks_ui(ui, &blocks, d, m.id);
        }
        Comp::Gallery(items) => {
            for (url, iw, ih) in items {
                let (w, h) = if *iw > 0 { fit(*iw, *ih, width.min(400.0), 300.0, width) } else { (width.min(300.0), 200.0) };
                let r = media(ui, d, url, url, w, h, None);
                if r.clicked() {
                    d.acts.push(Act::View(url.clone(), url.clone(), *iw, *ih));
                }
                ui.add_space(4.0);
            }
        }
        Comp::Thumbnail(url) => {
            let r = media(ui, d, url, url, 80.0, 80.0, None);
            if r.clicked() {
                d.acts.push(Act::View(url.clone(), url.clone(), 0, 0));
            }
        }
        Comp::FileCard { name, size, url } => {
            if file_card(ui, name, *size).clicked() {
                d.acts.push(Act::Url(url.clone()));
            }
        }
        Comp::Separator { divider } => {
            ui.add_space(4.0);
            if *divider {
                let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().border_muted));
            }
            ui.add_space(4.0);
        }
        Comp::Section { children, accessory } => {
            ui.horizontal_top(|ui| {
                let aw = if accessory.is_some() { 96.0 } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_width((width - aw - 12.0).max(80.0));
                    for ch in children {
                        render_comp(ui, d, m, ch, width - aw - 12.0);
                    }
                });
                if let Some(a) = accessory {
                    render_comp(ui, d, m, a, aw);
                }
            });
        }
        Comp::Container { accent, children } => {
            let col = accent.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_muted))
                .corner_radius(8)
                .inner_margin(Margin { left: if col.is_some() { 12 } else { 16 }, right: 16, top: 12, bottom: 12 })
                .show(ui, |ui| {
                    ui.set_width(width - 32.0);
                    if let Some(col) = col {
                        let g = ui.min_rect();
                        ui.painter().rect_filled(Rect::from_min_size(pos2(g.left() - 8.0, g.top() - 12.0), vec2(4.0, g.height() + 24.0)), CornerRadius::same(2), col);
                    }
                    for (i, ch) in children.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                        }
                        render_comp(ui, d, m, ch, width - 32.0);
                    }
                });
        }
        Comp::Button { .. } | Comp::Select { .. } => render_row(ui, d, m, std::slice::from_ref(c)),
    }
}

fn render_row(ui: &mut Ui, d: &mut D, m: &Message, row: &[Comp]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        {
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
                        // the edge goes over the fill, as discord's border does, not beside it
                        ui.painter().rect_filled(r, CornerRadius::same(8), fill.gamma_multiply(fade));
                        ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, stroke.gamma_multiply(fade)), egui::StrokeKind::Inside);
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
                        ui.painter().galley(pos2(x, (r.center().y - g.size().y / 2.0).floor()), g, tc);
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
                    _ => {}
                }
            }
        }
    });
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
            let burst = r.burst > 0 && r.burst == r.count;
            if resp.hovered() {
                d.acts.push(Act::Reactors(m.id, r.emoji.clone(), burst, r.count));
            }
            resp.on_hover_ui(|ui| {
                ui.label(super::reactions::tooltip(d.store, d.guild, m, r, burst));
            });
        }
    });
}

fn poll_left(expiry: i64) -> String {
    let ms = expiry - Local::now().timestamp_millis();
    if ms <= 0 {
        return "Poll closed".into();
    }
    let mins = ms / 60_000;
    if mins >= 1440 {
        format!("{}d left", mins / 1440)
    } else if mins >= 60 {
        format!("{}h left", mins / 60)
    } else {
        format!("{}m left", mins.max(1))
    }
}

/// A poll, laid out like discord's: question, answers you pick with a Vote button, then the
/// result bars once you've voted, asked to see them, or the poll's closed.
fn poll(ui: &mut Ui, d: &mut D, m: &Message) {
    let Some(p) = &m.poll else { return };
    let key = egui::Id::new(("poll", m.id));
    let (mut pending, mut show): (Vec<u8>, bool) = ui.data(|s| s.get_temp(key)).unwrap_or_default();
    let voted = p.answers.iter().any(|a| a.me);
    let results = p.finalized || voted || show;
    let total = p.total().max(1);
    let w = ui.available_width().min(440.0);

    Frame::new()
        .fill(pal().bg_secondary)
        .stroke(Stroke::new(1.0, pal().border_muted))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            text::label(ui, &p.question, semi(16.0), pal().header_primary, Sense::hover());
            if !results {
                ui.label(RichText::new(if p.multi { "Select one or more answers" } else { "Select one answer" }).font(reg(14.0)).color(pal().text_muted));
            }
            ui.add_space(8.0);
            for a in &p.answers {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                if !results && ui.clip_rect().contains_rect(row) {
                    d.rects.push((format!("poll_opt:{}", a.id), row));
                }
                let picked = if results { a.me } else { pending.contains(&a.id) };
                if results {
                    let frac = a.count as f32 / total as f32;
                    ui.painter().rect_filled(row, CornerRadius::same(8), pal().input_bg);
                    let bar = Rect::from_min_size(row.min, vec2(row.width() * frac, row.height()));
                    let fill = if a.me { Color32::from_rgb(0x3d, 0x45, 0xa5) } else { pal().control };
                    ui.painter().rect_filled(bar, CornerRadius::same(8), fill);
                } else {
                    let fill = if resp.hovered() { pal().bg_accent } else { pal().input_bg };
                    ui.painter().rect(row, CornerRadius::same(8), fill, Stroke::new(1.0, if picked { BLURPLE } else { Color32::TRANSPARENT }), egui::StrokeKind::Inside);
                }
                let mut x = row.left() + 14.0;
                if let Some(em) = &a.emoji {
                    let er = Rect::from_min_size(pos2(x, row.center().y - 9.0), vec2(18.0, 18.0));
                    Image::from_uri(gif::uri(ui.ctx(), &emoji_url(em), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                    x += 26.0;
                }
                ui.painter().text(pos2(x, row.center().y), Align2::LEFT_CENTER, &a.text, reg(16.0), pal().header_primary);
                if results {
                    let pct = (a.count as f32 / total as f32 * 100.0).round() as u32;
                    ui.painter().text(pos2(row.right() - 14.0, row.center().y), Align2::RIGHT_CENTER, format!("{pct}%"), medium(14.0), pal().header_secondary);
                } else {
                    let c = pos2(row.right() - 24.0, row.center().y);
                    if picked {
                        ui.painter().circle_filled(c, 10.0, BLURPLE);
                        ui.painter().circle_filled(c, 4.0, Color32::WHITE);
                    } else {
                        ui.painter().circle_stroke(c, 9.0, Stroke::new(2.0, pal().text_muted));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        if p.multi {
                            if let Some(i) = pending.iter().position(|x| *x == a.id) {
                                pending.remove(i);
                            } else {
                                pending.push(a.id);
                            }
                        } else {
                            pending = vec![a.id];
                        }
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let left = if p.finalized {
                    format!("{} vote{} \u{2022} Poll closed", p.total(), if p.total() == 1 { "" } else { "s" })
                } else {
                    format!("{} vote{} \u{2022} {}", p.total(), if p.total() == 1 { "" } else { "s" }, poll_left(p.expiry))
                };
                ui.label(RichText::new(left).font(reg(14.0)).color(pal().text_muted));
                if !p.finalized {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if results {
                            if voted && link_label(ui, "Remove your vote").clicked() {
                                d.acts.push(Act::Vote(m.id, vec![]));
                            } else if !voted && show && link_label(ui, "Back to vote").clicked() {
                                show = false;
                            }
                        } else {
                            let btn = Frame::new().fill(if pending.is_empty() { pal().control } else { BLURPLE }).corner_radius(8).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
                                ui.label(RichText::new("Vote").font(medium(14.0)).color(if pending.is_empty() { pal().text_muted } else { Color32::WHITE }));
                            });
                            let r = btn.response.interact(Sense::click());
                            if ui.clip_rect().contains_rect(r.rect) {
                                d.rects.push(("poll_vote".into(), r.rect));
                            }
                            if !pending.is_empty() {
                                if r.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if r.clicked() {
                                    d.acts.push(Act::Vote(m.id, std::mem::take(&mut pending)));
                                }
                            }
                            ui.add_space(8.0);
                            if link_label(ui, "Show results").clicked() {
                                show = true;
                            }
                        }
                    });
                }
            });
        });
    ui.data_mut(|s| s.insert_temp(key, (pending, show)));
}

fn link_label(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(Label::new(RichText::new(text).font(medium(14.0)).color(pal().header_secondary)).selectable(false).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

fn action_bar(d: &mut D, m: &Message, row: Rect) {
    let own = m.author == d.store.me;
    if d.ctx.input(|i| i.modifiers.shift) {
        return shift_bar(d, m, row, own);
    }
    // discord's bar: three one-click reactions, a rule, then react, edit (own), reply, forward, more
    let quick: Vec<ReactEmoji> = d.quick.iter().take(3).cloned().collect();
    let n = quick.len() as f32 + if own { 5.0 } else { 4.0 };
    let size = vec2(n * 32.0 + 9.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    let rect = Rect::from_min_size(pos, size);
    d.bar = Some((m.id, rect));
    // under popouts and menus, which would otherwise lose clicks to a toolbar left over them
    let bar_in = super::motion::enter(d.ctx, egui::Id::new(("msg_bar", m.id)), 0.15, super::motion::EASE);
    let bar = egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).fade_in(false);
    d.ctx.set_transform_layer(bar.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, 2.0 * (1.0 - bar_in))));
    bar.show(d.ctx, |ui| {
            ui.multiply_opacity(bar_in);
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
                            crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
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

/// A message that never went out gets two buttons: send it again, or throw it away.
fn failed_bar(d: &mut D, m: &Message, row: Rect) {
    let size = vec2(2.0 * 32.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    d.bar = Some((m.id, Rect::from_min_size(pos, size)));
    egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).show(d.ctx, |ui| {
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(1).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let r = icon_button(ui, Icon::Refresh, 32.0, "Resend message");
                d.rects.push(("bar_resend".into(), r.rect));
                if r.clicked() {
                    d.acts.push(Act::Resend(m.id));
                }
                let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
                icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), RED);
                d.rects.push(("bar_delete".into(), r));
                if resp.on_hover_text("Delete Message").clicked() {
                    d.acts.push(Act::Delete(m.id));
                }
            });
        });
    });
}

/// What the bar turns into while shift is held: discord's long row of one-click actions in
/// place of the reactions, ending in a red delete that doesn't ask.
fn shift_bar(d: &mut D, m: &Message, row: Rect, own: bool) {
    let manage = d.guild == 0 || d.moderator;
    let threads = d.guild != 0 && d.store.channels.get(&d.ch).is_some_and(|c| matches!(c.kind, 0 | 5)) && m.thread.is_none();
    let saved = d.bookmarks.contains(&m.id);
    let mut items: Vec<(Icon, &str, Act)> = Vec::new();
    if d.s.dev {
        items.push((Icon::Id, "Copy Message ID", Act::CopyId(m.id)));
    }
    items.push((Icon::Link, "Copy Link", Act::CopyLink(m.id)));
    items.push((Icon::MarkUnread, "Mark Unread", Act::MarkUnread(m.id)));
    items.push((if saved { Icon::Bookmarked } else { Icon::Bookmark }, if saved { "Remove Bookmark" } else { "Bookmark Message" }, Act::Bookmark(m.id)));
    if manage {
        items.push((Icon::Pin, if m.pinned { "Unpin Message" } else { "Pin Message" }, Act::Pin(m.id)));
    }
    if threads {
        items.push((Icon::ThreadPlus, "Create Thread", Act::Thread(m.id)));
    }
    items.push((Icon::Reply, "Reply", Act::Reply(m.id)));
    items.push((Icon::AddReaction, "Add Reaction", Act::Picker(m.id, Pos2::ZERO)));
    if own {
        items.push((Icon::Pencil, "Edit", Act::Edit(m.id)));
    }
    items.push((Icon::Forward, "Forward", Act::Forward(m.id)));
    let delete = own || manage;
    let n = items.len() as f32 + if delete { 1.0 } else { 0.0 };
    let size = vec2(n * 32.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    d.bar = Some((m.id, Rect::from_min_size(pos, size)));
    let bar_in = super::motion::enter(d.ctx, egui::Id::new(("msg_bar", m.id)), 0.15, super::motion::EASE);
    let bar = egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).fade_in(false);
    d.ctx.set_transform_layer(bar.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, 2.0 * (1.0 - bar_in))));
    bar.show(d.ctx, |ui| {
            ui.multiply_opacity(bar_in);
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (icon, tip, act) in items {
                            let r = icon_button(ui, icon, 32.0, tip);
                            if r.clicked() {
                                d.acts.push(match act {
                                    Act::Picker(id, _) => Act::Picker(id, r.rect.left_bottom()),
                                    a => a,
                                });
                            }
                        }
                        if delete {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
                            icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), RED);
                            d.rects.push(("bar_delete".into(), r));
                            if resp.on_hover_text("Delete").clicked() {
                                d.acts.push(Act::Delete(m.id));
                            }
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

enum MenuPick {
    Publish,
    React,
    Edit,
    Reply,
    Forward,
    Thread,
    Copy,
    Pin,
    Bookmark,
    Remind(chrono::DateTime<chrono::Utc>),
    App(serde_json::Value),
    Unread,
    Link,
    Speak,
    ClearReactions,
    ClearEmoji(ReactEmoji),
    ViewReactions,
    Resend,
    SaveFile,
    Configure,
    Profile(Id),
    Delete,
    Report,
    CopyId,
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
    let reacted = !m.reactions.is_empty();
    let reaction_list: Vec<crate::state::Reaction> = m.reactions.iter().filter(|r| r.count > r.burst || r.burst == 0).cloned().collect();
    let has_thread = m.thread.is_some();
    let kind = m.kind;
    let failed = m.failed;
    let sent = !m.failed && !m.pending;
    let interaction = m.interaction.clone();
    let author_name = app.store.user(m.author).map(|u| u.display().to_string()).unwrap_or_default();
    let voice = (m.flags & (1 << 13) != 0).then(|| m.attachments.first().map(|a| (a.url.clone(), a.filename.clone()))).flatten();
    let editable = matches!(kind, 0 | 19 | 20 | 23);
    let pinnable = editable;
    let forwardable = matches!(kind, 0 | 19 | 20 | 23 | 35 | 69) && m.poll.is_none();
    let replyable = matches!(kind, 0 | 7 | 19 | 20 | 23 | 24 | 25 | 32..=42 | 45 | 46 | 69);
    let published = m.flags & 1 != 0;
    let dev = app.settings.dev;
    let mut frequent: Vec<ReactEmoji> = app.ui.quick.clone();
    for n in ["✅", "❤️", "🔥", "😂", "👍", "😭", "💀", "👀", "🙏", "😊"] {
        if !frequent.iter().any(|q| q.id == 0 && q.name == n) {
            frequent.push(ReactEmoji { id: 0, name: n.to_string(), animated: false });
        }
    }
    frequent.truncate(10);
    let quick = app.quick_reactions();
    let Some(chan) = app.store.channels.get(&ch).cloned() else { return };
    // pinning and deleting other people's messages take manage messages, except in dms
    let manage = chan.guild == 0 || app.store.perms(&chan) & crate::state::MANAGE_MESSAGES != 0;
    let threads = chan.guild != 0 && matches!(chan.kind, 0 | 5) && !has_thread && editable;
    let configure = chan.guild != 0 && kind == 7 && app.store.perms(&chan) & crate::state::MANAGE_GUILD != 0;
    let publishable = chan.kind == 5 && (own || manage);
    let saved = app.bookmarks.contains(&id);
    // the apps' message commands, for the Apps submenu
    app.ensure_commands(&chan);
    let scope = if chan.guild != 0 { chan.guild } else { ch };
    let apps: Vec<serde_json::Value> = app.commands.get(&scope).map(|c| c.list.iter().filter(|c| c["type"].as_u64() == Some(3)).cloned().collect()).unwrap_or_default();
    let app_store = &app.store;
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
        let mut started = false;
        let mut group = |ui: &mut Ui, on: bool| {
            if on && started {
                menu_sep(ui);
            }
            started |= on;
            on
        };
        if group(ui, true) {
            let add = menu_row(ui, Some(Icon::AddReaction), "Add Reaction", None, false, Trail::Caret);
            if add.clicked() {
                pick = Some(MenuPick::React);
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
                    pick = Some(MenuPick::React);
                }
            });
            if reacted && menu_item(ui, Some(Icon::Smile), "View Reactions", false) {
                pick = Some(MenuPick::ViewReactions);
            }
        }
        if group(ui, sent && (own && editable || replyable || forwardable || publishable && !published) || threads) {
            if sent && own && editable && menu_item(ui, Some(Icon::Pencil), "Edit Message", false) {
                pick = Some(MenuPick::Edit);
            }
            if sent && replyable && menu_item(ui, Some(Icon::Reply), "Reply", false) {
                pick = Some(MenuPick::Reply);
            }
            if sent && forwardable && menu_item(ui, Some(Icon::Forward), "Forward", false) {
                pick = Some(MenuPick::Forward);
            }
            if threads && menu_item(ui, Some(Icon::ThreadPlus), "Create Thread", false) {
                pick = Some(MenuPick::Thread);
            }
            if sent && publishable && !published && menu_item(ui, Some(Icon::Megaphone), "Publish", false) {
                pick = Some(MenuPick::Publish);
            }
        }
        group(ui, true);
        if !content.is_empty() && menu_item(ui, Some(Icon::Copy), "Copy Text", false) {
            pick = Some(MenuPick::Copy);
        }
        if sent && manage && pinnable && menu_item(ui, Some(Icon::Pin), if pinned { "Unpin Message" } else { "Pin Message" }, false) {
            pick = Some(MenuPick::Pin);
        }
        let mark = if saved { (Icon::Bookmarked, "Remove Bookmark") } else { (Icon::Bookmark, "Bookmark Message") };
        if menu_item(ui, Some(mark.0), mark.1, false) {
            pick = Some(MenuPick::Bookmark);
        }
        let rem = menu_row(ui, Some(Icon::Clock), "Create Reminder", None, false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &rem, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (label, due) in App::reminder_times() {
                if menu_item(ui, None, &label, false) {
                    pick = Some(MenuPick::Remind(due));
                }
            }
        });
        let a = menu_row(ui, Some(Icon::Launched), "Apps", None, false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &a, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if apps.is_empty() {
                ui.add_enabled_ui(false, |ui| menu_item(ui, None, "No Apps", false));
            }
            for c in &apps {
                if menu_item(ui, None, c["name"].as_str().unwrap_or(""), false) {
                    pick = Some(MenuPick::App(c.clone()));
                }
            }
        });
        if failed && menu_item(ui, Some(Icon::Refresh), "Resend message", false) {
            pick = Some(MenuPick::Resend);
        }
        if menu_item(ui, Some(Icon::MarkUnread), "Mark Unread", false) {
            pick = Some(MenuPick::Unread);
        }
        if sent && configure && menu_item(ui, Some(Icon::CircleCheck), "Configure", false) {
            pick = Some(MenuPick::Configure);
        }
        if menu_item(ui, Some(Icon::Link), "Copy Message Link", false) {
            pick = Some(MenuPick::Link);
        }
        if !content.is_empty() && menu_item(ui, Some(Icon::Bubble), "Speak Message", false) {
            pick = Some(MenuPick::Speak);
        }
        if let Some(i) = &interaction {
            let info = menu_row(ui, Some(Icon::Help), "View Interaction Info", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &info, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (owner, url) = if i.in_guild {
                    let g = app_store.guilds.get(&i.owner);
                    (g.map(|g| g.name.clone()).unwrap_or_default(), g.and_then(|g| g.icon_url()))
                } else {
                    let u = app_store.user(i.owner);
                    (u.map(|u| u.display().to_string()).unwrap_or_default(), u.map(|u| u.avatar_url()))
                };
                if i.owner != 0 {
                    let (r, lead) = super::menu_row_at(ui, true, &owner, Some(&format!("Installed {author_name}")), false, Trail::None);
                    if let Some(url) = url {
                        Image::from_uri(url).corner_radius(CornerRadius::same(10)).show_loading_spinner(false).paint_at(ui, lead);
                    }
                    let _ = r;
                }
                if let Some(u) = app_store.user(i.user) {
                    let (r, lead) = super::menu_row_at(ui, true, u.display(), Some("Triggered this message"), false, Trail::None);
                    avatar(ui, lead, &u.avatar_url());
                    if r.clicked() {
                        pick = Some(MenuPick::Profile(i.user));
                    }
                }
            });
        }
        let can_delete = (own || manage) && !matches!(kind, 1..=5 | 21 | 35 | 56 | 57 | 64 | 68);
        let can_report = !own && sent && kind != 67;
        if group(ui, sent && manage && chan.guild != 0 && reacted || can_delete || can_report) {
            if sent && manage && chan.guild != 0 && reacted {
                let rm = menu_row(ui, Some(Icon::Close), "Remove Reactions", None, true, Trail::Caret);
                egui::containers::menu::SubMenu::new().show(ui, &rm, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for r in &reaction_list {
                        let url = gif::uri(ui.ctx(), &emoji_url(&r.emoji), true);
                        if menu_row(ui, None, &super::reactions::colon_name(&r.emoji), None, false, Trail::Image(url)).clicked() {
                            pick = Some(MenuPick::ClearEmoji(r.emoji.clone()));
                        }
                    }
                });
                if menu_item(ui, Some(Icon::Trash), "Remove All Reactions", true) {
                    pick = Some(MenuPick::ClearReactions);
                }
            }
            if can_delete && menu_item(ui, Some(Icon::Trash), "Delete Message", true) {
                pick = Some(MenuPick::Delete);
            }
            if can_report && menu_item(ui, Some(Icon::Flag), "Report Message", true) {
                pick = Some(MenuPick::Report);
            }
        }
        if group(ui, voice.is_some()) && menu_item(ui, Some(Icon::Download), "Save File", false) {
            pick = Some(MenuPick::SaveFile);
        }
        if dev {
            menu_sep(ui);
            if menu_item(ui, Some(Icon::Id), "Copy Message ID", false) {
                pick = Some(MenuPick::CopyId);
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
    let shift = ctx.input(|i| i.modifiers.shift);
    let picked = pick.is_some();
    match pick {
        Some(MenuPick::React) => app.picker = Some((Pick::React(ch, id), at)),
        Some(MenuPick::Edit) => app.editing = Some((id, content)),
        Some(MenuPick::Reply) => {
            app.reply = Some(id);
            app.reply_ping = !shift && !own;
            app.focus_composer = true;
        }
        Some(MenuPick::Forward) => app.forward = Some(super::popups::Forward::new(ch, id)),
        Some(MenuPick::Thread) => app.new_thread = Some(super::popups::NewThread::from(app, ch, id)),
        Some(MenuPick::Publish) => app.publish_msg(ch, id),
        Some(MenuPick::Copy) => ctx.copy_text(content),
        Some(MenuPick::Pin) => app.pin_msg(ch, id, !shift),
        Some(MenuPick::Bookmark) => app.bookmark(ch, id),
        Some(MenuPick::Remind(due)) => app.remind(ch, id, due),
        Some(MenuPick::App(c)) => app.message_command(&chan, &c, id),
        Some(MenuPick::Unread) => app.mark_unread(ch, id),
        Some(MenuPick::Link) => ctx.copy_text(app.message_link(ch, id)),
        Some(MenuPick::Speak) => app.speak(ch, id),
        Some(MenuPick::ClearReactions) => app.clear_reactions(ch, id, !shift),
        Some(MenuPick::ClearEmoji(e)) => app.clear_emoji(ch, id, e, !shift),
        Some(MenuPick::ViewReactions) => app.reactors = Some(super::reactions::Reactors::new(ch, id)),
        Some(MenuPick::Resend) => app.resend(ch, id),
        Some(MenuPick::SaveFile) => {
            if let Some((url, name)) = voice
                && let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file()
            {
                app.net.send(Cmd::Download { url, path });
            }
        }
        Some(MenuPick::Configure) => app.configure_joins(chan.guild),
        Some(MenuPick::Profile(u)) => app.open_profile(u, chan.guild, at),
        Some(MenuPick::Delete) => app.delete_msg(ch, id, !shift),
        Some(MenuPick::Report) => app.open_report(ch, id),
        Some(MenuPick::CopyId) => ctx.copy_text(id.to_string()),
        None => {}
    }
    if picked || !open {
        app.menu = None;
    }
}

/// Compact read-only rendering used by pins and search results.
pub fn preview(ui: &mut Ui, store: &Store, m: &Message) -> egui::Response {
    let ctx = ui.ctx().clone();
    let guild = store.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
    let revealed = Set::default();
    let mut editing = None;
    let settings = crate::cache::Settings::default();
    let none = Set::default();
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
        focus: None,
        bookmarks: &none,
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
                let cx = Cx { store, ctx: &ctx, guild, size: 15.0, color: pal().text_normal, revealed: false, edited: m.edited, embed: false };
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
                        file_card(ui, &a.filename, a.size);
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
