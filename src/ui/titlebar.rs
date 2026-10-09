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
const CLOSE: Color32 = Color32::from_rgb(0xd2, 0x2d, 0x39);

const INBOX_W: f32 = 597.0;

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
    pub unreads: bool,
    pub mentions: Option<Vec<Message>>,
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

    fn toggle_inbox(&mut self) {
        if self.inbox.take().is_some() {
            return;
        }
        self.inbox = Some(Inbox { unreads: false, mentions: None });
        self.rest("GET", "/users/@me/mentions?limit=25&roles=true&everyone=true".into(), None, Tag::Inbox);
    }
}

fn mentions(store: &Store) -> u32 {
    store
        .reads
        .iter()
        .filter(|(ch, _)| store.channels.get(ch).is_some_and(|c| !store.muted.contains(&c.id) && !store.muted.contains(&c.guild)))
        .map(|(_, r)| r.mentions)
        .sum()
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
            text::paint(ui.painter(), rect.center(), Align2::CENTER_CENTER, &g.acronym(), semi(size), pal().text_normal, rect.width());
        }
    }
}

enum Mark {
    Guild(Id),
    Avatar(String),
    People,
    None,
}

fn place(app: &App) -> (Mark, String) {
    let s = &app.store;
    if app.ui.guild.is_none() && app.friends {
        return (Mark::People, "Friends".into());
    }
    if let Some(g) = app.ui.guild.and_then(|g| s.guilds.get(&g)) {
        return (Mark::Guild(g.id), g.name.clone());
    }
    let Some(c) = app.cur().and_then(|c| s.channels.get(&c)) else { return (Mark::None, "Fastcord".into()) };
    let mark = match c.kind {
        1 => c.recipients.first().and_then(|u| s.user(*u)).map(|u| Mark::Avatar(u.avatar_url())).unwrap_or(Mark::None),
        _ => Mark::People,
    };
    (mark, super::channel_title(s, c.id))
}

fn toggle_maximized(ctx: &egui::Context) {
    let on = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!on));
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
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
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

                let back = signed_in && app.nav.can_back();
                let (hit, on) = press(ui, app, "back", at(bar.left() + 12.0, 24.0, 24.0), back);
                let col = if !back { idle.gamma_multiply(0.4) } else if on { lit } else { idle };
                glyph(ui, Icon::ArrowLeft, at(bar.left() + 12.0, 24.0, 24.0), 22.0, col);
                if hit {
                    app.go(true);
                }
                let fwd = signed_in && app.nav.can_forward();
                let (hit, on) = press(ui, app, "forward", at(bar.left() + 36.0, 24.0, 24.0), fwd);
                let col = if !fwd { idle.gamma_multiply(0.4) } else if on { lit } else { idle };
                glyph(ui, Icon::ArrowRight, at(bar.left() + 36.0, 24.0, 24.0), 22.0, col);
                if hit {
                    app.go(false);
                }

                // the window buttons are flush with the right edge
                let mut x = bar.right();
                for (name, icon) in [("close", Icon::Close), ("max", if maximized { Icon::Restore } else { Icon::Maximize }), ("min", Icon::Minimize)] {
                    let rect = at(x - 32.0, 32.0, HEIGHT);
                    let (hit, on) = press(ui, app, name, rect, true);
                    let (fill, col) = match (name, on) {
                        ("close", true) => (CLOSE, Color32::WHITE),
                        (_, true) => (HOVER, lit),
                        _ => (Color32::TRANSPARENT, idle),
                    };
                    ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
                    glyph(ui, icon, rect, 18.0, col);
                    if hit {
                        ctx.send_viewport_cmd(match name {
                            "close" => ViewportCommand::Close,
                            "min" => ViewportCommand::Minimized(true),
                            _ => ViewportCommand::Maximized(!maximized),
                        });
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
                    let n = mentions(&app.store);
                    if n > 0 {
                        badge(ui, inbox.center() + vec2(22.0, -6.0), n, Some(pal().frame_bg));
                    }
                }
                if hit {
                    app.toggle_inbox();
                }

                // centred on the window, not on the space between the buttons
                let half = (bar.center().x - (bar.left() + 72.0)).min(inbox.left() - 12.0 - bar.center().x);
                let label = text::layout(&ctx, &title, semi(14.0), pal().header_primary, (half * 2.0 - 32.0).max(0.0));
                let width = 16.0 + 8.0 + label.size().x;
                let left = bar.center().x - width / 2.0;
                let icon = Rect::from_min_size(pos2(left, mid - 8.0), vec2(16.0, 16.0));
                let bare = matches!(mark, Mark::None);
                match mark {
                    Mark::Guild(g) => guild_mark(ui, &app.store, g, icon),
                    Mark::Avatar(url) => super::avatar(ui, icon, &url),
                    Mark::People => glyph(ui, Icon::People, icon, 16.0, pal().header_secondary),
                    Mark::None => {}
                }
                let name_at = if bare { bar.center().x - label.size().x / 2.0 } else { left + 24.0 };
                label.paint(ui.painter(), pos2(name_at, mid - label.size().y / 2.0));
                app.rects.insert("title:name".into(), Rect::from_min_size(pos2(name_at, mid - 8.0), vec2(label.size().x, 16.0)));

                if drag.drag_started_by(PointerButton::Primary) {
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

/// Discord's inbox tabs each take a quarter of the width, the open one underlined in brand.
fn tab(ui: &mut Ui, app: &mut App, name: &str, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(INBOX_W / 4.0, 40.0), Sense::click());
    let col = if on { BLURPLE } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    let galley = ui.painter().layout_no_wrap(label.into(), medium(14.0), col);
    app.rects.insert(format!("inbox:{name}"), rect);
    if on {
        let w = galley.size().x + 32.0;
        ui.painter().rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 1.0), vec2(w, 2.0)), CornerRadius::same(1), BLURPLE);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn inbox(app: &mut App, ctx: &egui::Context) {
    let Some(state) = &app.inbox else { return };
    let unreads = state.unreads;
    let button = app.rects.get("title:inbox").copied();
    let at = button.map(|r| pos2(r.right() - INBOX_W, r.bottom() + 6.0)).unwrap_or(pos2(100.0, HEIGHT + 6.0));
    let room = (ctx.content_rect().bottom() - at.y - 16.0).clamp(160.0, 560.0);
    let body = body_height(app);
    let mut jump = None;
    let mut open_ch = None;
    let mut switch = None;
    let area = egui::Area::new(egui::Id::new("inbox"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            // an area only offers last frame's size; the scrolled list has to ask for its room
            ui.set_max_height(room + 120.0);
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(INBOX_W);
                ui.spacing_mut().item_spacing.y = 0.0;
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 4 }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        icons::draw(ui.painter(), Icon::Inbox, r, pal().header_primary);
                        ui.label(RichText::new("Inbox").font(semi(20.0)).color(pal().header_primary));
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if tab(ui, app, "unreads", "Unreads", unreads) {
                        switch = Some(true);
                    }
                    if tab(ui, app, "mentions", "Mentions", !unreads) {
                        switch = Some(false);
                    }
                });
                let r = ui.min_rect();
                ui.painter().hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
                let mut list = |ui: &mut Ui| {
                    Frame::new().inner_margin(16).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if unreads {
                            open_ch = unread_list(ui, app);
                        } else {
                            jump = mention_list(ui, app);
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
    if let Some(u) = switch
        && let Some(i) = &mut app.inbox
    {
        i.unreads = u;
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
    if i.unreads {
        let n = app.store.channels.values().filter(|c| c.is_text() && app.store.unread(c)).count().min(40);
        return 24.0 + 48.0 * n.max(1) as f32;
    }
    match &i.mentions {
        None => 90.0,
        Some(l) if l.is_empty() => 100.0,
        Some(l) => l.iter().map(|m| 58.0 + 98.0 + 20.0 * (m.content.lines().count().max(1) - 1 + m.content.len() / 60) as f32 + 24.0 * m.attachments.len() as f32).sum(),
    }
}

fn empty(ui: &mut Ui, what: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(what).color(pal().header_secondary)));
    ui.add_space(24.0);
}

fn mention_list(ui: &mut Ui, app: &mut App) -> Option<(Id, Id)> {
    let Some(list) = app.inbox.as_ref().and_then(|i| i.mentions.as_ref()) else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.add(egui::Spinner::new().color(pal().text_muted)));
        ui.add_space(24.0);
        return None;
    };
    if list.is_empty() {
        empty(ui, "You don't have any mentions.");
        return None;
    }
    let s = &app.store;
    let mut jump = None;
    for m in list {
        let c = s.channels.get(&m.channel);
        Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let g = c.map(|c| c.guild).unwrap_or(0);
                if g != 0 {
                    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                    guild_mark(ui, s, g, r);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let name = match c {
                        Some(c) if c.guild != 0 => format!("# {}", c.name),
                        Some(c) => super::channel_title(s, c.id),
                        None => "Unknown channel".into(),
                    };
                    ui.label(RichText::new(name).font(semi(16.0)).color(pal().header_primary));
                    if let Some(g) = s.guilds.get(&g) {
                        ui.label(RichText::new(&g.name).font(reg(12.0)).color(pal().header_secondary));
                    }
                });
            });
            ui.add_space(8.0);
            let r = chat::preview(ui, s, m);
            app.rects.insert(format!("inbox:mention:{}", m.id), r.rect);
            if r.clicked() {
                jump = Some((m.channel, m.id));
            }
        });
        ui.add_space(16.0);
    }
    jump
}

fn unread_list(ui: &mut Ui, app: &mut App) -> Option<Id> {
    let s = &app.store;
    let mut list: Vec<_> = s.channels.values().filter(|c| c.is_text() && !s.muted.contains(&c.guild) && s.unread(c) && s.can_view(c)).collect();
    list.sort_by_key(|c| std::cmp::Reverse(c.last_msg));
    if list.is_empty() {
        empty(ui, "You're all caught up.");
        return None;
    }
    let mut open = None;
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
        let n = s.mentions(c.id);
        if n > 0 {
            badge(ui, r.right_center() - vec2(8.0, 0.0), n, None);
        }
        if resp.clicked() {
            open = Some(c.id);
        }
    }
    for (id, r) in seen {
        app.rects.insert(format!("inbox:unread:{id}"), r);
    }
    open
}
