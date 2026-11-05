//! Discord's keyboard: its table of shortcuts, what Escape does once nothing open has taken it,
//! and the Ctrl+/ sheet with the easter eggs hiding in it.

use egui::{Align2, Color32, CornerRadius, Frame, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::time::{Duration, Instant};

use super::icons::{self, Icon};
use super::{App, picker, side, take_key};
use crate::state::Id;
use crate::theme::*;

/// The Ctrl+/ sheet: when each DDR arrow was last pressed, and the last keys typed in it.
#[derive(Default)]
pub struct Combos {
    lit: [Option<Instant>; 4],
    typed: Vec<char>,
}

/// Ctrl+Alt+Shift+W: breathing with Wumpus. When it began, and when it was paused if it is.
pub struct Breathe {
    at: Instant,
    paused: Option<Instant>,
}

impl App {
    /// A dialog over everything, whose Enter and Escape nothing underneath may take.
    pub fn modal_open(&self) -> bool {
        self.confirm.is_some()
            || self.forward.is_some()
            || self.reactors.is_some()
            || self.report.is_some()
            || self.invite.is_some()
            || self.made_invite.is_some()
            || self.links.modal()
            || self.status_edit.is_some()
            || self.share.is_some()
            || self.switcher.is_some()
            || self.new_thread.is_some()
            || self.poll.is_some()
            || self.follow.is_some()
            || self.modify.is_some()
            || self.group_pick.is_some()
            || self.topic.is_some()
            || self.leaving.is_some()
            || self.timeout.is_some()
            || self.combos.is_some()
            || self.breathe.is_some()
            || self.demon.is_some()
            || self.create.is_some()
            || self.folders.edit.is_some()
            || self.wizard.is_some()
            || self.guild_modal.is_some()
            || self.active_threads.is_some()
            || self.server.is_some()
            || self.chan_settings.is_some()
            || self.events.is_some()
            || self.guild_notifs.is_some()
            || self.settings_page.is_some()
    }

    /// Whatever's open over the app that takes Escape (and most shortcuts) for itself.
    pub fn layered(&self, ctx: &egui::Context) -> bool {
        self.viewer.is_some()
            || self.folders.edit.is_some()
            || self.wizard.is_some()
            || self.guild_modal.is_some()
            || self.profile.is_some()
            || self.picker.is_some()
            || self.stickers.is_some()
            || self.gifs.is_some()
            || self.menu.is_some()
            || self.user_menu.is_some()
            || self.confirm.is_some()
            || self.forward.is_some()
            || self.invite.is_some()
            || self.made_invite.is_some()
            || self.links.modal()
            || self.status_edit.is_some()
            || self.share.is_some()
            || self.switcher.is_some()
            || self.pins.is_some()
            || self.inbox.is_some()
            || self.guild_notifs.is_some()
            || self.new_thread.is_some()
            || self.poll.is_some()
            || self.launcher.is_some()
            || self.follow.is_some()
            || self.modify.is_some()
            || self.group_pick.is_some()
            || self.topic.is_some()
            || self.leaving.is_some()
            || self.emoji_pop.is_some()
            || self.create.is_some()
            || self.active_threads.is_some()
            || self.server.is_some()
            || self.chan_settings.is_some()
            || self.events.is_some()
            || self.combos.is_some()
            || self.breathe.is_some()
            || self.demon.is_some()
            || self.timeout.is_some()
            || self.settings_page.is_some()
            || egui::Popup::is_any_open(ctx)
    }

    /// Discord's keybinds. `layered` is whether something was open as the frame began: most
    /// keys leave it alone then, and Escape is its business.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context, layered: bool) {
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| take_key(i, m, key));
        let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let ctrl_alt = Modifiers::COMMAND | Modifiers::ALT;
        let all3 = Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT;
        if !layered {
            super::folders::keys(self, ctx);
        }

        // a call ringing comes before anything: Ctrl+Enter answers, Escape declines
        if let Some(ch) = self.voice.ringing {
            if k(Modifiers::COMMAND, Key::Enter) {
                self.answer(ch);
                return;
            }
            if k(Modifiers::NONE, Key::Escape) {
                self.decline(ch);
                return;
            }
        }
        // the popouts' own keys toggle them shut again
        if k(Modifiers::COMMAND, Key::P) {
            match (&self.pins, self.cur()) {
                (Some(_), _) => self.pins = None,
                (None, Some(c)) if !layered => self.open_pins(c),
                _ => {}
            }
        }
        if k(Modifiers::COMMAND, Key::I) && (self.inbox.is_some() || !layered) {
            self.toggle_inbox();
        }
        let tab = if k(Modifiers::COMMAND, Key::E) {
            Some(picker::Tab::Emoji)
        } else if k(Modifiers::COMMAND, Key::G) {
            Some(picker::Tab::Gifs)
        } else if k(Modifiers::COMMAND, Key::S) {
            Some(picker::Tab::Stickers)
        } else {
            None
        };
        if let Some(t) = tab
            && self.rects.contains_key("composer")
            && (!layered || self.picker.is_some() || self.gifs.is_some() || self.stickers.is_some())
        {
            picker::toggle(self, t);
        }
        if k(Modifiers::COMMAND, Key::Slash) {
            self.combos = if self.combos.is_some() { None } else { Some(Combos::default()) };
        }
        if k(all3, Key::W) && self.breathe.is_none() {
            self.breathe = Some(Breathe { at: Instant::now(), paused: None });
        }
        if layered {
            return;
        }
        if k(Modifiers::NONE, Key::Escape) || k(Modifiers::SHIFT, Key::PageDown) {
            self.escape();
        }

        let (up, down, uup, udown, mup, mdown) =
            (k(Modifiers::ALT, Key::ArrowUp), k(Modifiers::ALT, Key::ArrowDown), k(Modifiers::ALT | Modifiers::SHIFT, Key::ArrowUp), k(Modifiers::ALT | Modifiers::SHIFT, Key::ArrowDown), k(all3, Key::ArrowUp), k(all3, Key::ArrowDown));
        if up || down || uup || udown || mup || mdown {
            self.walk_channels(down || udown || mdown, if uup || udown { 1 } else if mup || mdown { 2 } else { 0 });
        }
        let prev = k(ctrl_alt, Key::ArrowUp) || k(ctrl_shift, Key::OpenBracket) || k(ctrl_shift, Key::Tab);
        let next = k(ctrl_alt, Key::ArrowDown) || k(ctrl_shift, Key::CloseBracket) || k(Modifiers::COMMAND, Key::Tab);
        if prev || next {
            self.walk_servers(next);
        }
        if k(ctrl_alt, Key::ArrowRight) {
            match self.ui.guild {
                Some(_) => self.select_guild(None),
                None => {
                    if let Some(g) = self.prev_guild.filter(|g| self.store.guilds.contains_key(g)) {
                        self.select_guild(Some(g));
                    }
                }
            }
        }
        if (k(ctrl_alt, Key::ArrowLeft) || k(all3, Key::V))
            && let Some(c) = self.voice.chan
        {
            let g = self.store.channels.get(&c).map_or(0, |c| c.guild);
            self.ui.guild = (g != 0).then_some(g);
            self.open(c);
        }
        if k(ctrl_shift, Key::M) {
            let on = !self.voice.mute;
            self.voice.set_mute(&self.net, on);
        }
        if k(ctrl_shift, Key::D) {
            let on = !self.voice.deaf;
            self.voice.set_deaf(&self.net, on);
        }
        if k(Modifiers::COMMAND, Key::Quote)
            && let Some(c) = self.cur().filter(|c| self.store.channels.get(c).is_some_and(|c| c.guild == 0))
        {
            self.join_voice(c, true);
        }
        if k(ctrl_shift, Key::E)
            && let Some(&c) = self.inbox_unreads().first()
        {
            self.mark_read(&[c]);
        }
        if k(Modifiers::COMMAND, Key::U) {
            let dm = self.cur().and_then(|c| self.store.channels.get(&c)).is_some_and(|c| c.kind == 1);
            if dm {
                self.ui.hide_dm_profile = !self.ui.hide_dm_profile;
            } else {
                self.ui.hide_members = !self.ui.hide_members;
            }
            self.search = None;
        }
        let find = k(Modifiers::COMMAND, Key::F);
        if find || k(ctrl_shift, Key::F) {
            // ctrl+f starts the search off in this channel, ctrl+shift+f the whole server
            let here = self.cur().and_then(|c| self.store.channels.get(&c)).filter(|c| c.guild != 0 && find).map(|c| format!("in:{} ", c.name));
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("search_q"), here.unwrap_or_default()));
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("search_box")));
        }
        if k(ctrl_shift, Key::A)
            && let Some(g) = self.ui.guild
        {
            let cats: Vec<Id> = self.store.channels.values().filter(|c| c.guild == g && c.kind == 4).map(|c| c.id).collect();
            if cats.iter().all(|c| self.ui.collapsed.contains(c)) {
                self.ui.collapsed.retain(|c| !cats.contains(c));
            } else {
                self.ui.collapsed.extend(cats);
            }
        }
        if k(ctrl_shift, Key::N) {
            self.invite = Some(String::new());
        }
        if k(ctrl_shift, Key::T) {
            self.select_guild(None);
            self.switcher = Some(String::new());
        }
        if k(all3, Key::N)
            && let Some(c) = self.cur().filter(|c| self.store.channels.get(c).is_some_and(|c| c.guild != 0 && matches!(c.kind, 0 | 5)))
        {
            self.new_thread = Some(super::popups::NewThread::blank(c));
        }
        if k(ctrl_shift, Key::L)
            && let Some(c) = self.cur()
        {
            let g = self.store.channels.get(&c).map(|c| c.guild).filter(|g| *g != 0).map_or("@me".to_string(), |g| g.to_string());
            ctx.copy_text(format!("https://discord.com/channels/{g}/{c}"));
            self.toast("Channel link copied.");
        }
        if k(ctrl_shift, Key::H) || k(Modifiers::NONE, Key::F1) {
            ctx.open_url(egui::OpenUrl::new_tab("https://support.discord.com"));
        }
        if k(Modifiers::SHIFT, Key::Escape)
            && let Some(g) = self.ui.guild
        {
            // the whole server read, in one bulk ack
            let all: Vec<Id> = self.store.channels.values().filter(|c| c.guild == g).map(|c| c.id).collect();
            self.mark_read(&all);
            self.marker = None;
        }
        if k(Modifiers::SHIFT, Key::PageUp)
            && let Some((c, after)) = self.marker.filter(|(c, _)| Some(*c) == self.cur())
        {
            let first = self.store.msgs.get(&c).and_then(|e| e.list.iter().find(|m| m.id > after)).map(|m| m.id);
            if let Some(m) = first {
                self.jump_to(c, m);
            }
        }
        if k(ctrl_shift, Key::U)
            && let Some(files) = rfd::FileDialog::new().pick_files()
        {
            self.files.extend(files.into_iter().map(super::composer::Upload::new));
        }
    }

    /// Escape, once nothing open took it: search results close; a chat scrolled well back
    /// jumps to the present; an unread one is marked read; otherwise it scrolls to the bottom.
    fn escape(&mut self) {
        if self.search.is_some() {
            self.search = None;
            return;
        }
        let Some(ch) = self.cur() else { return };
        let detached = self.store.msgs.get(&ch).is_some_and(|e| e.detached);
        let unread = self.store.channels.get(&ch).is_some_and(|c| self.store.unread(c));
        if detached || self.far_up == Some(ch) {
            self.jump_to_present(ch);
        } else if unread || self.marker.is_some_and(|(c, _)| c == ch) {
            self.mark_read(&[ch]);
            self.marker = None;
            self.hold_ack = None;
        } else if self.side_thread.take().is_none() {
            self.jump_to_present(ch);
        }
    }

    /// Alt+↑/↓ and friends: the channel above or below, or (`only` 1) the next unread one, or
    /// (2) the next with a mention.
    fn walk_channels(&mut self, down: bool, only: u8) {
        let list: Vec<Id> = match self.ui.guild {
            Some(g) => side::tree(&self.store, g, self.cur())
                .into_iter()
                .flat_map(|(_, k)| k)
                .filter(|c| c.is_text() || c.is_forum())
                .map(|c| c.id)
                .collect(),
            None => self.store.dms.clone(),
        };
        let n = list.len();
        if n == 0 {
            return;
        }
        let at = self.cur().and_then(|c| list.iter().position(|x| *x == c));
        let target = if only > 0 {
            let order: Vec<usize> = if down { (1..=n).collect() } else { (1..=n).rev().collect() };
            order.into_iter().map(|k| (at.unwrap_or(0) + k) % n).find(|i| {
                let c = &list[*i];
                match only {
                    1 => self.store.channels.get(c).is_some_and(|c| self.store.unread(c)),
                    _ => self.store.mentions(*c) > 0,
                }
            })
        } else {
            Some(match (at, down) {
                (Some(i), true) => (i + 1) % n,
                (Some(i), false) => (i + n - 1) % n,
                (None, _) => 0,
            })
        };
        if let Some(i) = target {
            self.open(list[i]);
        }
    }

    /// Ctrl+Alt+↑/↓: the server above or below in the rail, home at the top, each opening on
    /// the channel it was left on.
    fn walk_servers(&mut self, down: bool) {
        let rail: Vec<Option<Id>> = std::iter::once(None).chain(self.store.guild_order.iter().copied().map(Some)).collect();
        let at = rail.iter().position(|g| *g == self.ui.guild).unwrap_or(0);
        let n = rail.len();
        let next = if down { (at + 1) % n } else { (at + n - 1) % n };
        self.select_guild(rail[next]);
    }

    pub fn answer(&mut self, ch: Id) {
        self.voice.ringing = None;
        self.voice.join(&self.net, &self.store, ch, false);
    }

    pub fn decline(&mut self, ch: Id) {
        self.voice.ringing = None;
        let me = self.store.me.to_string();
        self.rest("POST", format!("/channels/{ch}/call/stop-ringing"), Some(serde_json::json!({ "recipients": [me] })), crate::net::Tag::Quiet);
    }

    /// Into a voice channel or call. From another one discord asks first, unless told not to.
    pub fn join_voice(&mut self, ch: Id, ring: bool) {
        let here = self.voice.chan.filter(|c| *c != ch);
        if here.is_none() || self.settings.voice_switch_ok {
            self.voice.join(&self.net, &self.store, ch, ring);
            return;
        }
        let name = self.store.channels.get(&ch).map(|c| super::channel_title(&self.store, c.id)).unwrap_or_default();
        let body = format!("Looks like you're in another voice channel. Are you sure you want to switch to **{name}**?");
        let c = super::Confirm::new("You sure?", body, "Confirm", move |app: &mut App| app.voice.join(&app.net, &app.store, ch, ring))
            .blurple()
            .dont_ask(|app: &mut App| {
                app.settings.voice_switch_ok = true;
                crate::cache::set_settings(&app.settings);
            });
        self.confirm = Some(c);
    }

    /// The home button clicked sixteen times, each within half a second of the last, turns on
    /// discord's hidden ディスコード sounds; sixteen more turn them off.
    pub fn home_clicked(&mut self) {
        let now = Instant::now();
        let quick = self.home_clicks.1.is_some_and(|t| now - t < Duration::from_millis(500));
        self.home_clicks = (if quick { self.home_clicks.0 + 1 } else { 1 }, Some(now));
        if self.home_clicks.0 >= 16 {
            self.home_clicks = (0, None);
            self.settings.discodo = !self.settings.discodo;
            crate::cache::set_settings(&self.settings);
            crate::sound::cue(if self.settings.discodo { "discodo" } else { "leave" });
            self.discodo_at = Some(now);
            // the font for the katakana comes in on demand
            crate::theme::CJK_WANTED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// One of the sheet's dance arrows, pointing up turned `turn` quarters clockwise: a thick white
/// outline, filled while its key is down.
fn ddr_arrow(p: &egui::Painter, r: Rect, turn: f32, col: Color32, lit: bool) {
    let a = turn * std::f32::consts::FRAC_PI_2;
    let shape = [(0.5, 0.06), (0.94, 0.5), (0.68, 0.5), (0.68, 0.94), (0.32, 0.94), (0.32, 0.5), (0.06, 0.5)];
    let pts: Vec<egui::Pos2> = shape
        .iter()
        .map(|(x, y)| {
            let v = vec2(x - 0.5, y - 0.5) * r.width();
            r.center() + vec2(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos())
        })
        .collect();
    if lit {
        // not convex, so the head and the shaft go in as two pieces
        p.add(egui::Shape::convex_polygon(vec![pts[0], pts[1], pts[6]], col, Stroke::NONE));
        p.add(egui::Shape::convex_polygon(vec![pts[2], pts[3], pts[4], pts[5]], col, Stroke::NONE));
    }
    p.add(egui::Shape::closed_line(pts, Stroke::new(3.0, col)));
}

/// A key as discord draws it in the sheet: a dark rounded cap with the name in caps.
fn keycap(ui: &mut Ui, label: &str) {
    let arrow = match label {
        "↑" => Some(Icon::ChevronUp),
        "↓" => Some(Icon::ChevronDown),
        "←" => Some(Icon::NavBack),
        "→" => Some(Icon::ChevronRight),
        _ => None,
    };
    let g = ui.painter().layout_no_wrap(label.to_uppercase(), bold(12.0), pal().header_primary);
    let w = if arrow.is_some() { 22.0 } else { (g.size().x + 12.0).max(22.0) };
    let (r, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    ui.painter().rect(r, 4, pal().bg_tertiary, Stroke::new(1.0, pal().border_normal), egui::StrokeKind::Inside);
    ui.painter().hline(r.x_range().shrink(2.0), r.bottom() - 1.0, Stroke::new(2.0, pal().border_normal));
    match arrow {
        Some(i) => icons::draw(ui.painter(), i, Rect::from_center_size(r.center() - vec2(0.0, 1.0), vec2(12.0, 12.0)), pal().header_primary),
        None => {
            ui.painter().galley(r.center() - g.size() / 2.0 - vec2(0.0, 1.0), g, pal().header_primary);
        }
    }
}

/// Discord's sheet, section by section: a card for each combo, its keys under the name.
const SHEET: [(&str, Option<&str>, &[(&str, &[&[&str]])]); 5] = [
    (
        "Messages",
        Some("These shortcuts work when focused on a message."),
        &[
            ("Edit Message", &[&["e"]]),
            ("Delete Message", &[&["backspace"]]),
            ("Pin Message", &[&["p"]]),
            ("Add Reaction", &[&["+"]]),
            ("Reply", &[&["r"]]),
            ("Forward Message", &[&["f"]]),
            ("Speak Message", &[&["s"]]),
            ("Copy Text", &[&["ctrl", "c"]]),
            ("Mark Unread", &[&["alt", "enter"]]),
            ("Focus text area", &[&["esc"]]),
        ],
    ),
    (
        "Navigation",
        None,
        &[
            ("Navigate between servers", &[&["ctrl", "alt", "↑"], &["ctrl", "alt", "↓"]]),
            ("Navigate between channels", &[&["alt", "↑"], &["alt", "↓"]]),
            ("Navigate forward and backward in page history", &[&["alt", "←"], &["alt", "→"]]),
            ("Navigate between unread channels", &[&["alt", "shift", "↑"], &["alt", "shift", "↓"]]),
            ("Navigate between unread channels with mentions", &[&["ctrl", "shift", "alt", "↑"], &["ctrl", "shift", "alt", "↓"]]),
            ("Navigate to current call", &[&["ctrl", "shift", "alt", "v"]]),
            ("Toggle between last server and DMs", &[&["ctrl", "alt", "→"]]),
            ("Toggle QuickSwitcher", &[&["ctrl", "k"]]),
            ("Create or join a server", &[&["ctrl", "shift", "n"]]),
        ],
    ),
    (
        "Voice and Video",
        None,
        &[
            ("Toggle mute", &[&["ctrl", "shift", "m"]]),
            ("Toggle deafen", &[&["ctrl", "shift", "d"]]),
            ("Answer incoming call", &[&["ctrl", "enter"]]),
            ("Decline incoming call", &[&["esc"]]),
            ("Start call in private message or group", &[&["ctrl", "'"]]),
            ("Toggle soundboard", &[&["ctrl", "shift", "b"]]),
        ],
    ),
    (
        "Chat",
        None,
        &[
            ("Mark server read", &[&["shift", "esc"]]),
            ("Mark channel as read", &[&["esc"]]),
            ("Create a private group", &[&["ctrl", "shift", "t"]]),
            ("Create a thread", &[&["ctrl", "shift", "alt", "n"]]),
            ("Toggle pins popout", &[&["ctrl", "p"]]),
            ("Toggle inbox popout", &[&["ctrl", "i"]]),
            ("Mark top inbox channel read", &[&["ctrl", "shift", "e"]]),
            ("Toggle channel member list or voice text chat", &[&["ctrl", "u"]]),
            ("Toggle emoji picker", &[&["ctrl", "e"]]),
            ("Toggle GIF picker", &[&["ctrl", "g"]]),
            ("Toggle sticker picker", &[&["ctrl", "s"]]),
            ("Scroll chat up or down", &[&["page up"], &["page down"]]),
            ("Jump to oldest unread message", &[&["shift", "page up"]]),
            ("Focus text area", &[&["any key"]]),
            ("Upload a file", &[&["ctrl", "shift", "u"]]),
            ("Copy channel link", &[&["ctrl", "shift", "l"]]),
        ],
    ),
    (
        "Miscellaneous",
        None,
        &[
            ("Get help", &[&["ctrl", "shift", "h"]]),
            ("Search", &[&["ctrl", "f"]]),
            ("Open context menu", &[&["shift", "f10"]]),
            ("Raging Demon", &[&["h", "h", "→", "n", "k"]]),
            ("Listen to Lofi Beats with Wumpus", &[&["ctrl", "alt", "shift", "w"]]),
        ],
    ),
];

/// The Ctrl+/ sheet over the chat. Its arrow keys light discord's four DDR arrows, and H H → N K
/// typed in it sets off the Raging Demon.
pub fn combos(app: &mut App, ctx: &egui::Context) {
    let Some(c) = &mut app.combos else { return };
    let now = Instant::now();
    let keys: Vec<Key> = ctx.input(|i| i.events.iter().filter_map(|e| match e {
        egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
        _ => None,
    }).collect());
    for key in keys {
        if let Some(i) = [Key::ArrowLeft, Key::ArrowDown, Key::ArrowUp, Key::ArrowRight].iter().position(|k| *k == key) {
            c.lit[i] = Some(now);
        }
        let ch = match key {
            Key::H => Some('h'),
            Key::N => Some('n'),
            Key::K => Some('k'),
            Key::ArrowRight => Some('>'),
            _ => Some('.'),
        };
        c.typed.extend(ch);
        if c.typed.len() > 5 {
            c.typed.remove(0);
        }
    }
    if c.typed.iter().collect::<String>() == "hh>nk" {
        app.combos = None;
        app.demon = Some(now);
        crate::theme::CJK_WANTED.store(true, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let lit = c.lit;
    let screen = ctx.content_rect();
    let area = Rect::from_min_max(pos2(screen.left() + 440.0, screen.top() + super::titlebar::HEIGHT + 56.0), pos2(screen.right() - 160.0, screen.bottom() - 90.0));
    let mut close = false;
    let modal = egui::Modal::new("combos".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)))
        .show(ctx, |ui| {
            ui.set_width(area.width().clamp(560.0, 1100.0));
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 36, right: 36, top: 32, bottom: 24 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Keyboard Combos").font(medium(28.0)).color(pal().header_primary));
                            ui.add_space(12.0);
                            ui.spacing_mut().item_spacing.x = 4.0;
                            keycap(ui, "ctrl");
                            keycap(ui, "/");
                        });
                        ui.add_space(6.0);
                        ui.label(RichText::new("Master these to be the greatest Discord Discord Revolution player").font(reg(16.0)).color(pal().text_normal));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // right to left, so the arrows go in reversed: → ↑ ↓ ←, each a quarter turn on
                        for (i, turn) in [(3, 1.0), (2, 0.0), (1, 2.0), (0, 3.0)] {
                            let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                            let glow = lit[i].map_or(0.0, |t| (1.0 - t.elapsed().as_secs_f32() / 0.3).max(0.0));
                            let col = Color32::WHITE.lerp_to_gamma(BLURPLE, glow);
                            ddr_arrow(ui.painter(), r.shrink(6.0), turn, col, glow > 0.0);
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "ddr", i)), (glow > 0.0, r)));
                        }
                    });
                });
            });
            let max_h = (screen.height() - 240.0).max(200.0);
            egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin { left: 36, right: 36, top: 8, bottom: 32 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (n, (title, sub, cards)) in SHEET.iter().enumerate() {
                        if n > 0 {
                            ui.add_space(24.0);
                            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().border_subtle));
                            ui.add_space(24.0);
                        }
                        ui.label(RichText::new(*title).font(semi(20.0)).color(pal().header_primary));
                        if let Some(s) = sub {
                            ui.add_space(6.0);
                            ui.label(RichText::new(*s).font(reg(14.0)).color(pal().text_normal));
                        }
                        ui.add_space(16.0);
                        let cols = 4;
                        let w = ((ui.available_width() - 16.0 * (cols - 1) as f32) / cols as f32).floor();
                        for row in cards.chunks(cols) {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 16.0;
                                for (label, combos) in row {
                                    Frame::new().fill(pal().bg_tertiary).corner_radius(8).inner_margin(12).show(ui, |ui| {
                                        ui.set_width(w - 24.0);
                                        ui.set_min_height(70.0);
                                        ui.spacing_mut().item_spacing.y = 8.0;
                                        ui.label(RichText::new(*label).font(medium(14.0)).color(pal().header_primary));
                                        for combo in combos.iter() {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                for key in combo.iter() {
                                                    keycap(ui, key);
                                                }
                                            });
                                        }
                                    });
                                }
                            });
                            ui.add_space(16.0);
                        }
                    }
                });
            });
        });
    if lit.iter().flatten().any(|t| t.elapsed() < Duration::from_millis(300)) {
        ctx.request_repaint();
    }
    close |= modal.should_close();
    if close {
        app.combos = None;
    }
}

/// The Raging Demon: the screen goes dark, the app shakes while eight explosions go off near the
/// middle, then a great 天 hangs over dark rays until it all closes.
pub fn demon(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.demon else { return };
    let t = at.elapsed().as_secs_f32();
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("demon")));
    let fade = (t / 0.4).min(1.0);
    p.rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha((200.0 * fade) as u8));
    // the shake: the app itself, everything drawn on the background layer
    let shaking = (1.8..4.2).contains(&t);
    let jolt = if shaking { vec2((t * 97.0).sin() * 9.0, (t * 131.0).cos() * 7.0) } else { egui::Vec2::ZERO };
    ctx.set_transform_layer(egui::LayerId::background(), egui::emath::TSTransform::from_translation(jolt));
    let c = screen.center();
    for i in 0..8 {
        let start = 1.8 + i as f32 * 0.24;
        let k = (t - start) / 0.5;
        if !(0.0..1.0).contains(&k) {
            continue;
        }
        let spot = c + vec2(((i * 73) % 9) as f32 * 30.0 - 120.0, ((i * 41) % 7) as f32 * 26.0 - 80.0);
        let size = 40.0 + 120.0 * k;
        let alpha = ((1.0 - k) * 255.0) as u8;
        let rays: Vec<egui::Pos2> = (0..24)
            .map(|j| {
                let a = j as f32 * std::f32::consts::TAU / 24.0;
                let r = if j % 2 == 0 { size } else { size * 0.45 };
                spot + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        for w in rays.windows(2) {
            p.add(egui::Shape::convex_polygon(vec![spot, w[0], w[1]], Color32::from_rgba_unmultiplied(0x5d, 0xb8, 0xff, alpha), Stroke::NONE));
        }
        p.circle_stroke(spot, size * 0.8, Stroke::new(4.0, Color32::from_rgba_unmultiplied(0xff, 0x40, 0x40, alpha)));
        p.circle_stroke(spot, size * 1.05, Stroke::new(3.0, Color32::from_rgba_unmultiplied(0x40, 0x90, 0xff, alpha)));
    }
    let shown = 4.95;
    if t >= shown {
        // dark rays out from the middle behind the character
        for j in 0..16 {
            let a = j as f32 * std::f32::consts::TAU / 16.0 + t * 0.2;
            let b = a + std::f32::consts::TAU / 32.0;
            let far = screen.width().max(screen.height());
            p.add(egui::Shape::convex_polygon(vec![c, c + vec2(a.cos(), a.sin()) * far, c + vec2(b.cos(), b.sin()) * far], Color32::from_rgba_unmultiplied(0x10, 0x10, 0x30, 220), Stroke::NONE));
        }
        let font = egui::FontId::new(320.0, egui::FontFamily::Proportional);
        for d in [vec2(-6.0, 0.0), vec2(6.0, 0.0), vec2(0.0, -6.0), vec2(0.0, 6.0)] {
            p.text(c + d, Align2::CENTER_CENTER, "天", font.clone(), Color32::from_rgb(0x0a, 0x0a, 0x20));
        }
        p.text(c, Align2::CENTER_CENTER, "天", font, Color32::from_rgb(0x5d, 0xb8, 0xff));
    }
    ctx.request_repaint();
    if t > shown + 3.0 {
        app.demon = None;
        ctx.set_transform_layer(egui::LayerId::background(), egui::emath::TSTransform::IDENTITY);
    }
}

/// "Chats can be a lot": Wumpus breathing in and out over a glow that swells with him, stars
/// drifting past behind, until Back to conversation.
pub fn breathe(app: &mut App, ctx: &egui::Context) {
    let Some(b) = &mut app.breathe else { return };
    let screen = ctx.content_rect();
    let t = b.paused.unwrap_or_else(Instant::now).duration_since(b.at).as_secs_f32();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("breathe_sky")));
    p.rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha(200));
    for i in 0..12 {
        let speed = 30.0 + (i * 7 % 11) as f32 * 6.0;
        let x = screen.left() + ((i * 211) % 997) as f32 / 997.0 * screen.width();
        let y = screen.top() + ((i as f32 * 97.0 + t * speed) % screen.height());
        let head = pos2(x - y * 0.15, y);
        p.line_segment([head, head - vec2(-12.0, 40.0)], Stroke::new(2.0, Color32::from_rgba_unmultiplied(0x5d, 0xd8, 0xe8, 40)));
        p.circle_filled(head, 5.0, Color32::from_rgb(0x5d, 0xd8, 0xe8));
    }
    let (mut close, mut pause) = (false, false);
    let modal = egui::Modal::new("breathe".into())
        .backdrop_color(Color32::TRANSPARENT)
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                close |= super::icon_button(ui, Icon::Close, 24.0, "Close").clicked();
            });
            let (r, _) = ui.allocate_exact_size(vec2(392.0, 170.0), Sense::hover());
            // eight seconds a breath: four in, four out
            let k = 0.5 - 0.5 * (t * std::f32::consts::TAU / 8.0).cos();
            let c = r.center() + vec2(0.0, 10.0);
            for (grow, a) in [(1.0, 18), (0.75, 26), (0.5, 34)] {
                ui.painter().circle_filled(c, 50.0 + 30.0 * k * grow, Color32::from_rgba_unmultiplied(0x58, 0x65, 0xf2, a));
            }
            let img = egui::Image::new(egui::include_image!("../../assets/wumpus.png"));
            let h = 150.0 * (0.96 + 0.04 * k);
            img.paint_at(ui, Rect::from_center_size(c, vec2(h * 131.0 / 202.0, h)));
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Chats can be a lot").font(semi(24.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(RichText::new("Let's practice breathing with Wumpus.").font(reg(16.0)).color(pal().header_secondary));
            });
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= super::grey_button(ui, "Back to conversation", 192.0).clicked();
                let label = if b.paused.is_some() { "Resume" } else { "Pause" };
                pause = super::button(ui, label, BLURPLE, 192.0).clicked();
            });
        });
    if pause {
        b.paused = match b.paused {
            // picking up where it stopped
            Some(p) => {
                b.at += p.elapsed();
                None
            }
            None => Some(Instant::now()),
        };
    }
    ctx.request_repaint();
    if close || modal.should_close() {
        app.breathe = None;
    }
}

/// The ディスコード tooltip over the home button for a second after it's switched.
pub fn discodo(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.discodo_at else { return };
    if at.elapsed() > Duration::from_secs(1) {
        app.discodo_at = None;
        return;
    }
    let Some(home) = app.rects.get("guild:home").copied() else { return };
    let text = if app.settings.discodo { "ディスコード enabled" } else { "ディスコード disabled" };
    egui::Area::new(egui::Id::new("discodo")).order(egui::Order::Tooltip).fixed_pos(home.right_center() + vec2(12.0, -18.0)).show(ctx, |ui| {
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.label(RichText::new(text).font(semi(16.0)).color(pal().header_primary));
        });
    });
    app.rects.insert("discodo".into(), Rect::from_min_size(home.right_center(), vec2(1.0, 1.0)));
    ctx.request_repaint_after(Duration::from_millis(100));
}
