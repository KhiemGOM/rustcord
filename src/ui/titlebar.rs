//! Discord's own title bar in place of the system one: history arrows, where
//! you are, inbox, help and the window buttons.

use egui::{Align2, Color32, CornerRadius, Frame, Image, Key, Margin, Modifiers, PointerButton, Rect, RichText, Sense, Ui, ViewportCommand, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, badge, chat, popups, take_key, text};
use crate::net::Tag;
use crate::state::{Id, Message, Store};
use crate::theme::*;

pub const HEIGHT: f32 = 32.0;

// --interactive-background-hover and --border-subtle, which the palette doesn't carry
const HOVER: Color32 = Color32::from_rgba_unmultiplied_const(0x97, 0x97, 0x9f, 31);
// background-mod-normal
const NAV_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(0x97, 0x97, 0x9f, 51);

const INBOX_W: f32 = 560.0;
const INBOX_TABS: [(&str, &str); 5] = [("unreads", "Unreads"), ("mentions", "Mentions"), ("bookmarks", "Bookmarks"), ("reminders", "Reminders"), ("scheduled", "Scheduled")];

#[derive(Clone, Copy, PartialEq)]
pub enum Loc {
    Friends,
    Chan(Id, Option<Id>),
}

#[derive(Default)]
pub struct Nav {
    stack: Vec<Loc>,
    at: usize,
    // set while a history jump replays itself through `open`
    quiet: bool,
}

impl Nav {
    pub fn visit(&mut self, loc: Loc) {
        if self.quiet || self.stack.get(self.at) == Some(&loc) {
            return;
        }
        self.stack.truncate(self.at + 1);
        self.stack.push(loc);
        self.at = self.stack.len() - 1;
    }

    /// A jump into another channel has already been recorded as that channel by `open`.
    pub fn jumped(&mut self, ch: Id, id: Id, moved: bool) {
        if moved && !self.quiet && let Some(top) = self.stack.get_mut(self.at) {
            *top = Loc::Chan(ch, Some(id));
            return;
        }
        self.visit(Loc::Chan(ch, Some(id)));
    }

    fn can_back(&self) -> bool {
        self.at > 0
    }

    fn can_forward(&self) -> bool {
        self.at + 1 < self.stack.len()
    }
}

pub struct Inbox {
    // discord's five tabs: unreads, mentions, bookmarks, reminders, scheduled
    pub tab: u8,
    pub mentions: Option<Vec<Message>>,
    // what the bookmarks or reminders tab lists, once it's loaded
    pub saved: Option<Vec<Saved>>,
    // the friend requests list instead of the tab's
    pub friends: bool,
}

/// A message saved for later: where it is, the message itself when discord sent it, and when a
/// reminder is due.
pub struct Saved {
    pub ch: Id,
    pub id: Id,
    pub msg: Option<Message>,
    pub due: Option<i64>,
}

/// GET /users/@me/saved-messages: 0 the ids bookmarked and the reminders, 1 the bookmarks.
pub fn saved(app: &mut App, k: u8, v: &serde_json::Value) {
    let list = |items: &serde_json::Value, app: &mut App| -> Vec<Saved> {
        items
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                let d = &s["save_data"];
                let due = d["due_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis());
                let msg = s["message"].is_object().then(|| app.store.parse_message(&s["message"]));
                Saved { ch: crate::state::sid(&d["channel_id"]), id: crate::state::sid(&d["message_id"]), msg, due }
            })
            .collect()
    };
    if k == 0 {
        for b in v["bookmark_ids"].as_array().into_iter().flatten() {
            let id = if b.is_object() { crate::state::sid(&b["message_id"]) } else { crate::state::sid(b) };
            app.bookmarks.insert(id);
        }
    }
    let items = if k == 0 { list(&v["reminders"], app) } else { list(&v["results"], app) };
    if let Some(i) = &mut app.inbox
        && i.tab == if k == 0 { 3 } else { 2 }
    {
        i.saved = Some(items);
    }
}

impl App {
    pub fn go(&mut self, back: bool) {
        loop {
            let at = if back { self.nav.at.checked_sub(1) } else { Some(self.nav.at + 1).filter(|a| *a < self.nav.stack.len()) };
            let Some(at) = at else { return };
            self.nav.at = at;
            let loc = self.nav.stack[at];
            self.nav.quiet = true;
            let found = match loc {
                Loc::Friends => {
                    self.ui.guild = None;
                    self.friends = true;
                    true
                }
                Loc::Chan(ch, msg) => match self.store.channels.get(&ch).map(|c| c.guild) {
                    // left the server or lost the channel since, step over it
                    None => false,
                    Some(g) => {
                        self.ui.guild = (g != 0).then_some(g);
                        match msg {
                            Some(m) => self.jump_to(ch, m),
                            None => self.open(ch),
                        }
                        true
                    }
                },
            };
            self.nav.quiet = false;
            if found {
                return;
            }
        }
    }

    pub(super) fn toggle_inbox(&mut self) {
        if self.inbox.take().is_some() {
            return;
        }
        self.inbox = Some(Inbox { tab: 1, mentions: None, saved: None, friends: false });
        self.fetch_mentions();
    }

    fn fetch_mentions(&mut self) {
        let s = &self.settings;
        let mut path = format!("/users/@me/mentions?limit=25&roles={}&everyone={}", s.inbox_roles, s.inbox_everyone);
        if let Some(g) = self.ui.guild.filter(|_| !s.inbox_all) {
            path.push_str(&format!("&guild_id={g}"));
        }
        if let Some(i) = &mut self.inbox {
            i.mentions = None;
        }
        self.rest("GET", path, None, Tag::Inbox);
    }

    /// The unread channels the inbox lists, newest first: nothing muted, nothing hidden.
    pub fn inbox_unreads(&self) -> Vec<Id> {
        let s = &self.store;
        let here = self.ui.guild.filter(|_| !self.settings.inbox_all);
        let mut list: Vec<_> = s.channels.values().filter(|c| c.is_text() && !s.muted.contains(&c.guild) && !s.is_muted(c) && s.unread(c) && s.can_view(c) && here.is_none_or(|g| c.guild == g || c.guild == 0)).collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.last_msg));
        list.into_iter().map(|c| c.id).collect()
    }

    /// Mark All as Read on the inbox's unreads; shift skips discord's question.
    pub fn read_everything(&mut self, ask: bool) {
        if ask {
            let body = "This will mark all your unmuted channels as read. Are you sure you want to continue?";
            self.confirm = Some(super::Confirm::new("Mark all as read?", body, "Mark As Read", |app: &mut App| app.read_everything(false)).blurple());
            return;
        }
        let all = self.inbox_unreads();
        self.mark_read(&all);
    }
}

fn guild_mark(ui: &Ui, store: &Store, gid: Id, rect: Rect) {
    let radius = CornerRadius::same((rect.width() * 0.25) as u8);
    let Some(g) = store.guilds.get(&gid) else { return };
    ui.painter().rect_filled(rect, radius, pal().bg_accent);
    match g.icon_url() {
        Some(url) => {
            Image::from_uri(url).corner_radius(radius).show_loading_spinner(false).paint_at(ui, rect);
        }
        None => {
            let size = if g.acronym().chars().count() > 2 { 7.0 } else { 8.0 } * rect.width() / 16.0;
            text::paint(ui.painter(), rect.center(), Align2::CENTER_CENTER, &g.acronym(), reg(size), pal().text_normal, rect.width());
        }
    }
}

enum Mark {
    Icon(Icon),
    Guild(Id),
    Page(Icon),
    People,
    None,
}

fn place(app: &App) -> (Mark, String) {
    let s = &app.store;
    if app.directory.is_some() {
        return (Mark::Page(Icon::Discover), "Discover".into());
    }
    if app.ui.guild.is_none() && app.friends {
        return (Mark::People, "Friends".into());
    }
    if app.ui.guild.is_none() && app.quests.is_some() {
        return (Mark::Page(Icon::Quests), "Quests".into());
    }
    if let Some(s) = app.shop.as_ref().filter(|_| app.ui.guild.is_none()) {
        return match s.page {
            super::shop::Page::Nitro => (Mark::Icon(Icon::Nitro), "Nitro".into()),
            super::shop::Page::Shop => (Mark::Icon(Icon::Shop), "Shop".into()),
        };
    }
    if let Some(g) = app.ui.guild.and_then(|g| s.guilds.get(&g)) {
        return (Mark::Guild(g.id), g.name.clone());
    }
    if app.cur().and_then(|c| s.channels.get(&c)).is_none() {
        return (Mark::None, "Rustcord".into());
    }
    (Mark::Icon(Icon::Discord), "Direct Messages".into())
}

/// Double-clicking the bar or its middle button: out of fullscreen if in it, else maximise or restore.
fn toggle_maximized(ctx: &egui::Context) {
    let (full, on) = ctx.input(|i| (i.viewport().fullscreen.unwrap_or(false), i.viewport().maximized.unwrap_or(false)));
    if full {
        fullscreen(ctx, false);
    } else {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!on));
    }
}

/// A maximised window is put back to normal before going fullscreen, and maximised again after:
/// winit keeps a maximised window's client area to the work area, so fullscreen over one left
/// the taskbar's height of it black.
pub(super) fn fullscreen(ctx: &egui::Context, on: bool) {
    crate::winsys::set_fullscreen(on);
    ctx.send_viewport_cmd(ViewportCommand::Resizable(!on));
    let was = egui::Id::new("maximized_before_fullscreen");
    if on {
        let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.data_mut(|d| d.insert_temp(was, max));
        if max {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
    } else {
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        if ctx.data_mut(|d| d.remove_temp::<bool>(was)).unwrap_or(false) {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let signed_in = app.token.is_some() && app.settings_page.is_none();
    if signed_in {
        if app.friends && app.ui.guild.is_none() {
            app.nav.visit(Loc::Friends);
        }
        keys(app, &ctx);
    }
    let (mark, title) = place(app);
    crate::ui::settings::said(&ctx, "title", &title);
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false));
    egui::Panel::top("titlebar")
        .exact_size(HEIGHT)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let bar = ui.max_rect();
            // above the order modals use, so dialogs can't swallow the window buttons
            egui::Area::new(egui::Id::new("titlebar_over")).order(egui::Order::Tooltip).fixed_pos(bar.min).show(&ctx, |ui| {
                ui.painter().rect_filled(bar, CornerRadius::ZERO, pal().frame_bg);
                app.rects.insert("title:bar".into(), bar);
                let drag = ui.interact(bar, egui::Id::new("title_drag"), Sense::click_and_drag());
                let mid = bar.center().y;
                let at = |x: f32, w: f32, h: f32| Rect::from_min_size(pos2(x, mid - h / 2.0), vec2(w, h));
                let press = |ui: &mut Ui, app: &mut App, name: &str, rect: Rect, enabled: bool| {
                    app.rects.insert(format!("title:{name}"), rect);
                    let resp = ui.interact(rect, egui::Id::new(("title", name)), Sense::click());
                    let on = enabled && resp.hovered();
                    if on {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    (resp.clicked() && enabled, on)
                };
                let glyph = |ui: &Ui, icon: Icon, rect: Rect, size: f32, col: Color32| {
                    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(size, size)), col);
                };
                let idle = pal().header_secondary;
                let lit = pal().interactive_hover;

                // discord's pair is one 54 wide pill split in two, centred over the rail
                let back = signed_in && app.nav.can_back();
                let r = at(bar.left() + 9.0, 26.0, 24.0);
                let (hit, on) = press(ui, app, "back", r, back);
                if on {
                    ui.painter().rect_filled(r, CornerRadius { nw: 6, sw: 6, ..Default::default() }, NAV_HOVER);
                }
                let col = if !back { idle.gamma_multiply(0.3) } else if on { lit } else { idle };
                glyph(ui, Icon::NavBack, r.translate(vec2(2.0, 0.0)), 18.0, col);
                if hit {
                    app.go(true);
                }
                let fwd = signed_in && app.nav.can_forward();
                let r = at(bar.left() + 37.0, 26.0, 24.0);
                let (hit, on) = press(ui, app, "forward", r, fwd);
                if on {
                    ui.painter().rect_filled(r, CornerRadius { ne: 6, se: 6, ..Default::default() }, NAV_HOVER);
                }
                let col = if !fwd { idle.gamma_multiply(0.3) } else if on { lit } else { idle };
                glyph(ui, Icon::NavForward, r.translate(vec2(-2.0, 0.0)), 18.0, col);
                if hit {
                    app.go(false);
                }

                // the window buttons are flush with the right edge
                let mut x = bar.right();
                for (name, icon) in [("close", Icon::Close), ("max", if maximized { Icon::Restore } else { Icon::Maximize }), ("min", Icon::Minimize)] {
                    let rect = at(x - 32.0, 32.0, HEIGHT);
                    let (hit, on) = press(ui, app, name, rect, true);
                    let (fill, col) = match (name, on) {
                        ("close", true) => (pal().danger, Color32::WHITE),
                        (_, true) => (HOVER, lit),
                        _ => (Color32::TRANSPARENT, idle),
                    };
                    ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
                    glyph(ui, icon, rect, 18.0, col);
                    if hit {
                        match name {
                            "close" if app.settings.minimize_to_tray => app.hide_to_tray(&ctx),
                            "close" => ctx.send_viewport_cmd(ViewportCommand::Close),
                            "min" => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                            _ => toggle_maximized(&ctx),
                        }
                    }
                    x -= 36.0;
                }
                // what's left of the 4px gap, then the divider with its margin
                let div = x - 4.0 - 4.0;
                ui.painter().vline(div + 0.5, (mid - 9.0)..=(mid + 9.0), egui::Stroke::new(1.0, pal().frame_border));
                let help = at(div - 4.0 - 12.0 - 24.0, 24.0, 24.0);
                let inbox = at(help.left() - 12.0 - 24.0, 24.0, 24.0);

                let (hit, on) = press(ui, app, "help", help, true);
                glyph(ui, Icon::Help, help, 16.0, if on { lit } else { idle });
                if hit && app.e2e.is_none() {
                    let _ = open::that_detached("https://support.discord.com");
                }

                let (hit, on) = press(ui, app, "inbox", inbox, signed_in);
                let open = app.inbox.is_some();
                if signed_in {
                    glyph(ui, Icon::Inbox, inbox, 16.0, if on || open { lit } else { idle });
                    let n = app.store.mention_count();
                    if n > 0 {
                        badge(ui, inbox.center() + vec2(22.0, -6.0), n, Some(pal().frame_bg));
                    }
                }
                if hit {
                    app.toggle_inbox();
                }

                // centred on the window, not on the space between the buttons
                let half = (bar.center().x - (bar.left() + 72.0)).min(inbox.left() - 12.0 - bar.center().x);
                let label = text::layout(&ctx, &title, medium(14.0), pal().text_normal, (half * 2.0 - 32.0).max(0.0));
                let width = 16.0 + 8.0 + label.size().x;
                let left = bar.center().x - width / 2.0;
                let icon = Rect::from_min_size(pos2(left, mid - 8.0), vec2(16.0, 16.0));
                let bare = matches!(mark, Mark::None);
                match mark {
                    Mark::Guild(g) => guild_mark(ui, &app.store, g, icon),
                    Mark::Page(i) => glyph(ui, i, icon, 16.0, pal().header_secondary),
                    Mark::People => glyph(ui, Icon::People, icon, 16.0, pal().header_secondary),
                    Mark::Icon(i) => glyph(ui, i, icon, 16.0, pal().header_secondary),
                    Mark::None => {}
                }
                let name_at = if bare { bar.center().x - label.size().x / 2.0 } else { left + 24.0 };
                label.paint(ui.painter(), pos2(name_at, mid - label.size().y / 2.0));
                app.rects.insert("title:name".into(), Rect::from_min_size(pos2(name_at, mid - 8.0), vec2(label.size().x, 16.0)));

                if drag.drag_started_by(PointerButton::Primary) && !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    toggle_maximized(&ctx);
                }
            });
        });
}

/// Back and forward: Alt+←/→ and Ctrl+[ / ] like the official client, plus the mouse side buttons.
fn keys(app: &mut App, ctx: &egui::Context) {
    let (back, fwd) = ctx.input_mut(|i| {
        let back = take_key(i, Modifiers::ALT, Key::ArrowLeft) | take_key(i, Modifiers::COMMAND, Key::OpenBracket);
        let fwd = take_key(i, Modifiers::ALT, Key::ArrowRight) | take_key(i, Modifiers::COMMAND, Key::CloseBracket);
        (back | i.pointer.button_pressed(PointerButton::Extra1), fwd | i.pointer.button_pressed(PointerButton::Extra2))
    });
    if back {
        app.go(true);
    } else if fwd {
        app.go(false);
    }
}

/// Discord's inbox tabs share the width 8 apart, the open one underlined in brand.
fn tab(ui: &mut Ui, app: &mut App, name: &str, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2((INBOX_W - 8.0 * (INBOX_TABS.len() - 1) as f32) / INBOX_TABS.len() as f32, 36.0), Sense::click());
    let col = if on { pal().blurple } else { crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
    let galley = ui.painter().layout_no_wrap(label.into(), semi(14.0), col);
    app.rects.insert(format!("inbox:{name}"), rect);
    if on {
        let w = galley.size().x + 32.0;
        ui.painter().rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 1.0), vec2(w, 2.0)), CornerRadius::same(1), pal().blurple);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn inbox(app: &mut App, ctx: &egui::Context) {
    let Some(state) = &app.inbox else { return };
    let tab = state.tab;
    let button = app.rects.get("title:inbox").copied();
    let at = button.map(|r| pos2(r.right() - INBOX_W, r.bottom() + 8.0)).unwrap_or(pos2(100.0, HEIGHT + 8.0));
    let room = (ctx.content_rect().bottom() - at.y - 16.0).clamp(160.0, 560.0);
    let body = body_height(app);
    let mut jump = None;
    let mut open_ch = None;
    let mut switch = None;
    let mut read_all = None;
    let mut filter = None;
    let mut friends = None;
    let area = super::motion::popout(ctx, egui::Area::new(egui::Id::new("inbox"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true), vec2(0.0, 1.0))
        .show(ctx, |ui| {
            // an area only offers last frame's size; the scrolled list has to ask for its room
            ui.set_max_height(room + 120.0);
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(INBOX_W);
                ui.spacing_mut().item_spacing.y = 0.0;
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 17 }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.set_min_height(32.0);
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        icons::draw(ui.painter(), Icon::Inbox, r, pal().header_primary);
                        ui.label(RichText::new("Inbox").font(semi(20.0)).color(pal().header_primary));
                        {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let pending = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count();
                                let r = head_button(ui, Icon::PersonAdd, "View Friend Requests", pending);
                                app.rects.insert("inbox:friends".into(), r.rect);
                                if r.clicked() {
                                    friends = Some(!app.inbox.as_ref().is_some_and(|i| i.friends));
                                }
                                if tab < 2 {
                                    let r = head_button(ui, Icon::Filter, "Filter", 0);
                                    app.rects.insert("inbox:filter".into(), r.rect);
                                    let frame = Frame::menu(&ui.ctx().global_style());
                                    egui::Popup::menu(&r).align(egui::RectAlign::BOTTOM_END).gap(4.0).frame(frame).show(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        let s = &app.settings;
                                        let rows: &[(&str, bool)] = if tab == 1 {
                                            &[("@here and @everyone", s.inbox_everyone), ("Show Role Mentions", s.inbox_roles), ("Include all servers", s.inbox_all)]
                                        } else {
                                            &[("Include all servers", s.inbox_all)]
                                        };
                                        for (i, (label, on)) in rows.iter().enumerate() {
                                            if super::menu_row(ui, None, label, None, false, super::Trail::Check(*on)).clicked() {
                                                filter = Some(if tab == 1 { i } else { 2 });
                                            }
                                        }
                                    });
                                }
                                if tab == 0 {
                                let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                                ui.painter().rect(r, 8, if resp.hovered() { NAV_HOVER } else { HOVER }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                icons::draw(ui.painter(), Icon::CheckAll, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
                                app.rects.insert("inbox:read_all".into(), r);
                                if resp.on_hover_text("Mark All as Read").clicked() {
                                    read_all = Some(!ui.input(|i| i.modifiers.shift));
                                }
                                }
                            });
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (i, (name, label)) in INBOX_TABS.into_iter().enumerate() {
                        if self::tab(ui, app, name, label, tab == i as u8) {
                            switch = Some(i as u8);
                        }
                    }
                });
                let r = ui.min_rect();
                ui.painter().hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
                let mut list = |ui: &mut Ui| {
                    Frame::new().inner_margin(16).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if app.inbox.as_ref().is_some_and(|i| i.friends) {
                            friend_requests(ui, app);
                            return;
                        }
                        match tab {
                            0 => open_ch = unread_list(ui, app),
                            1 => jump = mention_list(ui, app),
                            4 => tip(ui, "Send it at the perfect moment", "Draft your message then click the plus button and ", "Schedule Message", " to choose when to send. We'll handle the rest."),
                            _ => jump = saved_list(ui, app, tab == 3),
                        }
                    });
                };
                if body > room {
                    egui::ScrollArea::vertical().max_height(room).auto_shrink(false).show(ui, |ui| list(ui));
                } else {
                    list(ui);
                }
            });
        });
    if let Some(f) = friends
        && let Some(i) = &mut app.inbox
    {
        i.friends = f;
    }
    if let Some(n) = filter {
        let s = &mut app.settings;
        match n {
            0 => s.inbox_everyone = !s.inbox_everyone,
            1 => s.inbox_roles = !s.inbox_roles,
            _ => s.inbox_all = !s.inbox_all,
        }
        if tab == 1 {
            app.fetch_mentions();
        }
    }
    if let Some(t) = switch
        && let Some(i) = &mut app.inbox
        && i.tab != t
    {
        i.tab = t;
        i.friends = false;
        i.saved = None;
        match t {
            2 => app.rest("GET", "/users/@me/saved-messages/bookmarks?limit=25".into(), None, Tag::Saved(1)),
            3 => app.rest("GET", "/users/@me/saved-messages?bookmark_ids=true".into(), None, Tag::Saved(0)),
            _ => {}
        }
    }
    if let Some(ask) = read_all {
        app.read_everything(ask);
    }
    let (target, close) = match (jump, open_ch) {
        (Some((c, m)), _) => (Some((c, Some(m))), true),
        (_, Some(c)) => (Some((c, None)), true),
        _ => (None, false),
    };
    if let Some((c, m)) = target {
        app.settings_page = None;
        match m {
            Some(m) => app.jump_to(c, m),
            None => {
                let g = app.store.channels.get(&c).map(|c| c.guild).unwrap_or(0);
                app.ui.guild = (g != 0).then_some(g);
                app.open(c);
            }
        }
    }
    let on_button = button.is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.contains(p)));
    if close || (popups::dismissed(ctx, &area.response) && !on_button) {
        app.inbox = None;
    }
}

// a shrink-to-fit scroll area stayed at its spinner-sized first frame once the list arrived,
// so only scroll when the entries (by a rough height) would overflow
fn body_height(app: &App) -> f32 {
    let Some(i) = &app.inbox else { return 0.0 };
    if i.friends {
        return 100.0 + 56.0 * app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as f32;
    }
    if i.tab >= 2 {
        return match &i.saved {
            None => 90.0,
            Some(l) if l.is_empty() => 100.0,
            Some(l) => l.len() as f32 * 200.0,
        };
    }
    if i.tab == 0 {
        let n = app.store.channels.values().filter(|c| c.is_text() && app.store.unread(c)).count().min(40);
        return 24.0 + 48.0 * n.max(1) as f32;
    }
    match &i.mentions {
        None => 90.0,
        Some(l) if l.is_empty() => 100.0,
        Some(l) => l.iter().map(|m| 58.0 + 98.0 + 20.0 * (m.content.lines().count().max(1) - 1 + m.content.len() / 60) as f32 + 24.0 * m.attachments.len() as f32).sum(),
    }
}

fn head_button(ui: &mut Ui, icon: Icon, tip: &str, count: usize) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    ui.painter().rect(r, 8, if resp.hovered() { NAV_HOVER } else { HOVER }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
    if count > 0 {
        badge(ui, pos2(r.right() + 2.0, r.top() + 2.0), count as u32, None);
    }
    resp.on_hover_text(tip)
}

/// The people who've asked to be your friend, to take or turn down without leaving the inbox.
fn friend_requests(ui: &mut Ui, app: &mut App) {
    let mut asked: Vec<Id> = app.store.relationships.iter().filter(|(_, t)| **t == crate::state::INCOMING).map(|(u, _)| *u).collect();
    asked.sort();
    ui.label(RichText::new("Friend Requests").font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    if asked.is_empty() {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No Incoming Friend Requests").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new("Once you have incoming friend requests, you can access them here!").font(reg(14.0)).color(pal().header_secondary));
        });
        ui.add_space(16.0);
        return;
    }
    let mut accept = None;
    let mut ignore = None;
    for u in asked {
        let Some(user) = app.store.user(u) else { continue };
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
        super::avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
        text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y - 8.0), egui::Align2::LEFT_CENTER, user.display(), semi(16.0), pal().header_primary, 280.0);
        text::paint(ui.painter(), pos2(r.left() + 48.0, r.center().y + 10.0), egui::Align2::LEFT_CENTER, "Incoming Friend Request", reg(12.0), pal().text_muted, 280.0);
        for (i, (icon, tip)) in [(Icon::Close, "Ignore"), (Icon::Check, "Accept")].into_iter().enumerate() {
            let b = Rect::from_center_size(pos2(r.right() - 20.0 - 40.0 * i as f32, r.center().y), vec2(32.0, 32.0));
            let resp = ui.interact(b, ui.id().with(("friend_req", u, i)), Sense::click());
            ui.painter().circle_filled(b.center(), 16.0, crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(16.0, 16.0)), if i == 1 { pal().green } else { pal().header_secondary });
            if resp.on_hover_text(tip).clicked() {
                if i == 1 { accept = Some(u) } else { ignore = Some(u) }
            }
        }
    }
    if let Some(u) = accept {
        app.store.relationships.insert(u, crate::state::FRIEND);
        app.rest_in("PUT", format!("/users/@me/relationships/{u}"), Some(serde_json::json!({})), "friends-popout", Tag::rollback(move |app: &mut App| { app.store.relationships.remove(&u); }));
    }
    if let Some(u) = ignore {
        let was = app.store.relationships.remove(&u);
        app.rest_in("DELETE", format!("/users/@me/relationships/{u}"), None, "friends-popout", Tag::rollback(move |app: &mut App| {
            if let Some(t) = was {
                app.store.relationships.insert(u, t);
            }
        }));
    }
}

fn empty(ui: &mut Ui, what: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(what).color(pal().header_secondary)));
    ui.add_space(24.0);
}

/// How many of a channel's mentions its card shows before "Load more messages".
const CARD_SHOWN: usize = 3;

fn card_button(ui: &mut Ui, r: Rect, icon: Icon, tip: &str, mark: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(mark), egui::Sense::click());
    ui.painter().rect(r, 8, if resp.hovered() { NAV_HOVER } else { HOVER }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
    super::settings::mark(ui, mark, r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip).clicked()
}

fn mention_list(ui: &mut Ui, app: &mut App) -> Option<(Id, Id)> {
    let Some(list) = app.inbox.as_ref().and_then(|i| i.mentions.as_ref()) else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
        ui.add_space(24.0);
        return None;
    };
    if list.is_empty() {
        empty(ui, "You don't have any mentions.");
        return None;
    }
    let s = &app.store;
    let mut jump = None;
    let mut read = None;
    let mut order: Vec<Id> = Vec::new();
    for m in list {
        if !order.contains(&m.channel) {
            order.push(m.channel);
        }
    }
    for ch in order {
        let c = s.channels.get(&ch);
        let msgs: Vec<&Message> = list.iter().filter(|m| m.channel == ch).collect();
        let more_id = egui::Id::new(("inbox_more", ch));
        let all = ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
        let shown = if all { msgs.len() } else { msgs.len().min(CARD_SHOWN) };
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
            let g = c.map(|c| c.guild).unwrap_or(0);
            let p = ui.painter();
            let icon = Rect::from_min_size(hr.left_top() + vec2(12.0, 14.0), vec2(32.0, 32.0));
            if g != 0 {
                guild_mark(ui, s, g, icon);
            } else {
                icons::draw(ui.painter(), Icon::People, icon.shrink(4.0), pal().header_secondary);
            }
            icons::draw(p, Icon::ChevronRight, Rect::from_center_size(hr.left_top() + vec2(58.0, 24.0), vec2(12.0, 12.0)), pal().header_secondary);
            let (name, hash) = match c {
                Some(c) if c.guild != 0 => (c.name.clone(), true),
                Some(c) => (super::channel_title(s, c.id), false),
                None => ("Unknown channel".into(), false),
            };
            let mut x = hr.left() + 72.0;
            if hash {
                icons::draw(p, Icon::Hash, Rect::from_center_size(pos2(x + 6.0, hr.top() + 22.0), vec2(12.0, 12.0)), pal().channel_default);
                x += 16.0;
            }
            let room = hr.width() - (x - hr.left()) - 100.0;
            text::paint(p, pos2(x, hr.top() + 22.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, room);
            let sub = match (s.guilds.get(&g), c.and_then(|c| s.channels.get(&c.parent))) {
                (Some(g), Some(cat)) if cat.kind == 4 => format!("{} \u{203a} {}", g.name, cat.name),
                (Some(g), _) => g.name.clone(),
                _ => String::new(),
            };
            text::paint(p, pos2(hr.left() + 72.0, hr.top() + 41.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().header_secondary, hr.width() - 172.0);
            let done = Rect::from_min_size(pos2(hr.right() - 12.0 - 32.0, hr.top() + 14.0), vec2(32.0, 32.0));
            let go = done.translate(vec2(-40.0, 0.0));
            if card_button(ui, go, Icon::Jump, "Jump to Message", &format!("inbox:jump:{ch}")) {
                jump = msgs.first().map(|m| (m.channel, m.id));
            }
            if card_button(ui, done, Icon::Check, "Mark As Read", &format!("inbox:read:{ch}")) {
                read = Some(ch);
            }
            ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, egui::Stroke::new(1.0, pal().border_subtle));
            for m in &msgs[..shown] {
                Frame::new().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let r = chat::preview(ui, s, m);
                    app.rects.insert(format!("inbox:mention:{}", m.id), r.rect);
                    if r.clicked() {
                        jump = Some((m.channel, m.id));
                    }
                });
            }
            if shown < msgs.len() {
                let (br, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                ui.painter().hline(br.x_range(), br.top() + 0.5, egui::Stroke::new(1.0, pal().border_subtle));
                let col = if resp.hovered() { pal().header_primary } else { pal().link };
                ui.painter().text(br.center(), Align2::CENTER_CENTER, "Load more messages", medium(14.0), col);
                super::settings::mark(ui, &format!("inbox:more:{ch}"), br);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    ui.data_mut(|d| d.insert_temp(more_id, true));
                }
            }
        });
        ui.add_space(16.0);
    }
    if let Some(ch) = read {
        app.mark_read(&[ch]);
        if let Some(l) = app.inbox.as_mut().and_then(|i| i.mentions.as_mut()) {
            l.retain(|m| m.channel != ch);
        }
    }
    jump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_disables_resizing_and_restores_it_on_exit() {
        let ctx = egui::Context::default();
        let mut entered = ctx.run_ui(Default::default(), |ui| fullscreen(ui.ctx(), true));
        entered.textures_delta.clear();
        let commands = &entered.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.iter().any(|c| matches!(c, ViewportCommand::Resizable(false))));
        let mut exited = ctx.run_ui(Default::default(), |ui| fullscreen(ui.ctx(), false));
        exited.textures_delta.clear();
        let commands = &exited.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.iter().any(|c| matches!(c, ViewportCommand::Resizable(true))));
    }
}

fn unread_list(ui: &mut Ui, app: &mut App) -> Option<Id> {
    let ids = app.inbox_unreads();
    let s = &app.store;
    let list: Vec<_> = ids.iter().filter_map(|c| s.channels.get(c)).collect();
    if !app.settings.inbox_tip {
        Frame::new().fill(pal().bg_primary).corner_radius(8).inner_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Inbox, r, pal().header_primary);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(RichText::new("Stay up to date with latest messages!").font(semi(16.0)).color(pal().header_primary));
                    ui.label(RichText::new("Unread messages from all your unmuted channels will show up here. Time to get to that zero inbox!").color(pal().text_normal));
                    ui.add_space(4.0);
                    let g = ui.painter().layout_no_wrap("Got it!".into(), medium(14.0), Color32::WHITE);
                    let (b, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 32.0), Sense::click());
                    app.rects.insert("inbox:tip".into(), b);
                    ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
                    ui.painter().galley(b.center() - g.size() / 2.0, g, Color32::WHITE);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        app.settings.inbox_tip = true;
                    }
                });
            });
        });
        ui.add_space(16.0);
    }
    if list.is_empty() {
        empty(ui, "You're all caught up.");
        return None;
    }
    let mut open = None;
    let mut read = None;
    let mut seen = Vec::new();
    for c in list.into_iter().take(40) {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
        seen.push((c.id, r));
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let icon = Rect::from_min_size(r.left_top() + vec2(8.0, 8.0), vec2(32.0, 32.0));
        let sub = match s.guilds.get(&c.guild) {
            Some(g) => {
                guild_mark(ui, s, g.id, icon);
                g.name.clone()
            }
            None => {
                match c.recipients.first().and_then(|u| s.user(*u)) {
                    Some(u) if c.kind == 1 => super::avatar(ui, icon, &u.avatar_url()),
                    _ => icons::draw(ui.painter(), Icon::People, icon.shrink(4.0), pal().header_secondary),
                }
                "Direct Messages".into()
            }
        };
        let title = if c.guild != 0 { format!("# {}", c.name) } else { super::channel_title(s, c.id) };
        text::paint(ui.painter(), r.left_top() + vec2(52.0, 16.0), Align2::LEFT_CENTER, &title, semi(15.0), pal().header_primary, r.width() - 110.0);
        text::paint(ui.painter(), r.left_top() + vec2(52.0, 34.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().text_muted, r.width() - 110.0);
        // discord's check on each row: read without opening it
        let tick = Rect::from_center_size(r.right_center() - vec2(24.0, 0.0), vec2(32.0, 32.0));
        let t = ui.interact(tick, ui.id().with(("inbox_read", c.id)), Sense::click());
        ui.painter().rect(tick, 8, if t.hovered() { NAV_HOVER } else { HOVER }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(tick.center(), vec2(16.0, 16.0)), pal().header_primary);
        let n = s.mentions(c.id);
        if n > 0 {
            badge(ui, tick.left_center() - vec2(8.0, 0.0), n, None);
        }
        if t.on_hover_text("Mark As Read").clicked() {
            read = Some(c.id);
        } else if resp.clicked() {
            open = Some(c.id);
        }
    }
    for (id, r) in seen {
        app.rects.insert(format!("inbox:unread:{id}"), r);
    }
    if let Some(c) = read {
        app.mark_read(&[c]);
    }
    open
}

/// Bookmarks or reminders: each message as a card under where it was said, a reminder's time
/// over it, the message jumped to on a click.
/// A tab's empty state: a heading, then how to fill it with the action in bold.
fn tip(ui: &mut Ui, head: &str, how: &str, action: &str, tail: &str) {
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(head).font(semi(24.0)).color(pal().text_normal));
        ui.add_space(8.0);
        let mut job = egui::text::LayoutJob::default();
        let fmt = |font| egui::TextFormat { font_id: font, color: pal().header_secondary, ..Default::default() };
        job.append(how, 0.0, fmt(medium(14.0)));
        job.append(action, 0.0, fmt(semi(14.0)));
        job.append(tail, 0.0, fmt(medium(14.0)));
        job.halign = egui::Align::Center;
        ui.label(job);
    });
    ui.add_space(24.0);
}

fn saved_list(ui: &mut Ui, app: &mut App, reminders: bool) -> Option<(Id, Id)> {
    let Some(list) = app.inbox.as_ref().and_then(|i| i.saved.as_ref()) else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
        ui.add_space(24.0);
        return None;
    };
    let (head, how, action, tail) = if reminders {
        ("Stay organized, never forget a follow-up", "Right-click any message and pick ", "Create Reminder", ". We'll nudge you here when it's due.")
    } else {
        ("Collect your favorite memories", "Right-click any message and pick ", "Bookmark Message", ". Find it here, for your eyes only.")
    };
    tip(ui, head, how, action, tail);
    if list.is_empty() {
        return None;
    }
    let s = &app.store;
    let mut jump = None;
    for it in list {
        Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let where_ = match s.channels.get(&it.ch) {
                Some(c) if c.guild != 0 => format!("# {}", c.name),
                Some(c) => super::channel_title(s, c.id),
                None => "Unknown channel".into(),
            };
            ui.label(RichText::new(where_).font(semi(16.0)).color(pal().header_primary));
            if let Some(due) = it.due {
                let when = chrono::DateTime::from_timestamp_millis(due).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %-I:%M %p").to_string()).unwrap_or_default();
                ui.label(RichText::new(format!("Reminder · {when}")).font(reg(12.0)).color(pal().header_secondary));
            }
            ui.add_space(8.0);
            match &it.msg {
                Some(m) => {
                    let r = chat::preview(ui, s, m);
                    app.rects.insert(format!("inbox:saved:{}", m.id), r.rect);
                    if r.clicked() {
                        jump = Some((it.ch, it.id));
                    }
                }
                None => {
                    let lost = if reminders { "A message you set a reminder for could not be found." } else { "A message you bookmarked could not be found." };
                    ui.label(RichText::new(lost).color(pal().text_muted));
                }
            }
        });
        ui.add_space(16.0);
    }
    jump
}
