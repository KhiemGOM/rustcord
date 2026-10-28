use egui::text_selection::CCursorRange;
use egui::text::CCursor;
use egui::{Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Instant;

use super::icons::{self, Icon};
use super::{App, avatar, channel_title, icon_button, text};
use crate::net::{Cmd, Tag};
use crate::state::{Channel, Id, make_nonce, sid};
use crate::theme::*;

pub struct Upload {
    pub path: PathBuf,
    pub name: String,
    pub spoiler: bool,
    // kept for the thumbnail; it gets read for the upload anyway
    thumb: Option<egui::load::Bytes>,
}

impl Upload {
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let mut u = Upload { path, name, spoiler: false, thumb: None };
        if u.is_image() && std::fs::metadata(&u.path).is_ok_and(|m| m.len() < 25 << 20) {
            u.thumb = std::fs::read(&u.path).ok().map(Into::into);
        }
        u
    }

    fn is_image(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| n.ends_with(e))
    }
}

/// Application commands for a guild (or DM), as returned by the command index.
#[derive(Default)]
pub struct Commands {
    pub apps: Vec<(Id, String, Option<String>)>,
    pub list: Vec<Value>,
}

enum Item {
    User(Id, String, String),
    Role(Id, String, u32),
    Everyone(&'static str),
    Channel(Id, String),
    Emoji(Em),
    Command(usize),
    BuiltIn(usize),
    // a Vencord plugin's command: name, description, plugin
    Plugin(String, String, String),
}

/// Discord's own commands, listed after the apps' as "Built-In": name, options, description.
const BUILT_IN: [(&str, &str, &str); 13] = [
    ("shrug", "message", "Appends ¯\\_(ツ)_/¯ to your message."),
    ("tableflip", "message", "Appends (╯°□°)╯︵ ┻━┻ to your message."),
    ("unflip", "message", "Appends ┬─┬ ノ( ゜-゜ノ) to your message."),
    ("tts", "message", "Use text-to-speech to read the message to all members currently viewing the channel."),
    ("me", "message", "Displays text with emphasis."),
    ("spoiler", "message", "Marks your message as a spoiler."),
    ("nick", "new_nick", "Change nickname on this server."),
    ("thread", "name message", "Start new thread"),
    ("kick", "user reason", "Kick user"),
    ("ban", "user delete_messages reason", "Ban user"),
    ("timeout", "user duration reason", "Timeout user"),
    ("msg", "user message", "Message user"),
    ("leave", "silent", "Leave Group"),
];

/// Whether a built-in command makes sense here, as discord's predicates decide.
fn built_in_here(app: &App, chan: &Channel, name: &str) -> bool {
    use crate::state::{BAN_MEMBERS, KICK_MEMBERS, MODERATE_MEMBERS};
    let perms = app.store.perms(chan);
    match name {
        "tts" | "nick" => chan.guild != 0,
        "thread" => chan.guild != 0 && matches!(chan.kind, 0 | 5),
        "kick" => chan.guild != 0 && perms & KICK_MEMBERS != 0,
        "ban" => chan.guild != 0 && perms & BAN_MEMBERS != 0,
        "timeout" => chan.guild != 0 && perms & MODERATE_MEMBERS != 0,
        "leave" => chan.kind == 3,
        _ => true,
    }
}

struct Em {
    name: String,
    // what goes into the message: `<:name:id>`, or the glyph
    raw: String,
    url: Option<String>,
    server: Option<String>,
    locked: bool,
}

/// The word the cursor sits in, when it's one autocomplete cares about.
fn token(draft: &str, cursor: usize) -> Option<(usize, char, String)> {
    let bytes: Vec<(usize, char)> = draft.char_indices().collect();
    let cur_b = bytes.get(cursor).map(|(b, _)| *b).unwrap_or(draft.len());
    let before = &draft[..cur_b];
    let start = before.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    let word = &before[start..];
    let first = word.chars().next()?;
    match first {
        '@' | '#' => Some((start, first, word[1..].to_string())),
        ':' if word.chars().count() >= 3 && !word[1..].contains(':') => Some((start, ':', word[1..].to_string())),
        '/' if start == 0 => Some((0, '/', word[1..].to_string())),
        _ => None,
    }
}

fn score(hay: &str, q: &str) -> Option<u8> {
    let h = hay.to_lowercase();
    if h.starts_with(q) {
        Some(0)
    } else if h.contains(q) {
        Some(1)
    } else {
        None
    }
}

fn candidates(app: &App, chan: &Channel, kind: char, q: &str) -> Vec<Item> {
    let q = q.to_lowercase();
    let store = &app.store;
    let mut out: Vec<(u16, Item)> = Vec::new();
    match kind {
        '@' => {
            let mut ids: Vec<Id> = if chan.guild == 0 {
                chan.recipients.clone()
            } else {
                store.members.get(&chan.guild).map(|m| m.keys().copied().collect()).unwrap_or_default()
            };
            ids.sort();
            ids.dedup();
            for u in ids {
                let Some(user) = store.user(u) else { continue };
                let name = store.name_in(chan.guild, u);
                if let Some(s) = score(&name, &q).or_else(|| score(&user.username, &q)) {
                    out.push((s as u16, Item::User(u, name, user.username.clone())));
                }
            }
            if chan.guild != 0 {
                for e in ["everyone", "here"] {
                    if e.starts_with(&q) {
                        out.push((2, Item::Everyone(e)));
                    }
                }
                if let Some(g) = store.guilds.get(&chan.guild) {
                    for r in g.roles.values().filter(|r| r.id != g.id) {
                        if let Some(s) = score(&r.name, &q) {
                            out.push(((s + 2) as u16, Item::Role(r.id, r.name.clone(), r.color)));
                        }
                    }
                }
            }
        }
        '#' => {
            if let Some(g) = store.guilds.get(&chan.guild) {
                for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
                    if c.kind == 4 || c.is_thread() || !store.can_view(c) {
                        continue;
                    }
                    if let Some(s) = score(&c.name, &q) {
                        out.push((s as u16, Item::Channel(c.id, c.name.clone())));
                    }
                }
            }
        }
        ':' => {
            // current server first, then prefix hits anywhere, then substring hits; locked ones trail their group
            let rank = |name: &str, home: bool, locked: bool, unicode: bool| {
                let m = if name.eq_ignore_ascii_case(&q) { 0 } else { score(name, &q)? + 1 };
                let tier = if home { 0 } else if m < 2 { 1 } else { 2 };
                Some(tier * 100 + locked as u16 * 30 + m as u16 * 10 + unicode as u16)
            };
            for (g, e, ok) in store.emojis_for(chan.guild) {
                if let Some(k) = rank(&e.name, g.id == chan.guild, !ok, false) {
                    let raw = format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id);
                    out.push((k, Item::Emoji(Em { name: e.name.clone(), raw, url: Some(e.url()), server: Some(g.name.clone()), locked: !ok })));
                }
            }
            let mut seen = std::collections::HashSet::new();
            for (glyph, name) in super::picker::common() {
                if let Some(k) = rank(&name, false, false, true)
                    && seen.insert(glyph)
                {
                    out.push((k, Item::Emoji(Em { name, raw: glyph.to_string(), url: None, server: None, locked: false })));
                }
            }
        }
        '/' => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            if let Some(cmds) = app.commands.get(&scope) {
                for (i, c) in cmds.list.iter().enumerate() {
                    if c["type"].as_u64().unwrap_or(1) != 1 {
                        continue;
                    }
                    if let Some(s) = score(c["name"].as_str().unwrap_or(""), &q) {
                        out.push((s as u16, Item::Command(i)));
                    }
                }
            }
            for (name, desc, plugin) in app.vc_command_list() {
                if let Some(s) = score(&name, &q) {
                    out.push((s as u16 + 1, Item::Plugin(name, desc, plugin)));
                }
            }
            // after the apps' own, so an app's /me still comes first as in discord
            for (i, (name, ..)) in BUILT_IN.iter().enumerate() {
                if built_in_here(app, chan, name)
                    && let Some(s) = score(name, &q)
                {
                    out.push((s as u16 + 1, Item::BuiltIn(i)));
                }
            }
        }
        _ => {}
    }
    out.sort_by_key(|(s, _)| *s);
    out.into_iter().map(|(_, i)| i).take(if kind == '/' { 25 } else { 10 }).collect()
}

pub fn show(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let ch = chan.id;
    let tid = egui::Id::new("composer");
    // the reply bar is part of the composer, so the typing line goes above it
    let stack_top = ui.cursor().top();

    if let Some(rid) = app.reply {
        let name = app.store.find_msg(ch, rid).map(|m| app.store.name_in(chan.guild, m.author)).unwrap_or_default();
        // replies to your own messages never ping, so there's no switch for them
        let own = app.store.find_msg(ch, rid).is_some_and(|m| m.author == app.store.me);
        Frame::new()
            .fill(pal().bg_secondary)
            .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
            .inner_margin(Margin::symmetric(16, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Replying to").font(reg(14.0)).color(pal().header_secondary));
                    text::label(ui, &name, semi(14.0), pal().header_secondary, Sense::hover());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 20.0, "Cancel reply").clicked() {
                            app.reply = None;
                        }
                        ui.add_space(8.0);
                        if !own {
                            // the "@ ON" toggle: whether the reply pings its author
                            let (label, col, tip) = if app.reply_ping {
                                ("@ ON", pal().link, "Click to disable pinging the original author")
                            } else {
                                ("@ OFF", pal().header_secondary, "Click to enable pinging the original author.")
                            };
                            let r = ui.add(egui::Label::new(RichText::new(label).font(medium(14.0)).color(col)).sense(Sense::click()));
                            app.rects.insert("ping_toggle".into(), r.rect);
                            if r.on_hover_text(tip).clicked() {
                                app.reply_ping = !app.reply_ping;
                            }
                        }
                    });
                });
            });
    }

    let focused = ui.memory(|m| m.has_focus(tid));
    let nothing_focused = ui.memory(|m| m.focused().is_none());
    let typed = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_))));
    // tab with nothing focused lands in the composer too, as in discord
    let modal = app.modal_open();
    let tabbed = nothing_focused && !modal && app.focus_msg.is_none() && !egui::Popup::is_any_open(ui.ctx()) && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Tab));
    if app.editing.is_none() && !modal && (app.focus_composer || tabbed || (nothing_focused && typed)) {
        ui.memory_mut(|m| m.request_focus(tid));
        app.focus_composer = false;
        app.focus_msg = None;
    }
    if std::mem::take(&mut app.caret_end)
        && let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), tid)
    {
        let end = app.drafts.get(&ch).map_or(0, |d| d.chars().count());
        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(end))));
        st.store(ui.ctx(), tid);
    }

    // autocomplete runs off the text and cursor from the previous frame
    let draft_now = app.drafts.get(&ch).cloned().unwrap_or_default();
    let cursor = egui::TextEdit::load_state(ui.ctx(), tid)
        .and_then(|s| s.cursor.char_range())
        .map(|r| r.primary.index.0)
        .unwrap_or(draft_now.chars().count());
    let tok = token(&draft_now, cursor).filter(|t| app.ac_dismissed.as_ref() != Some(&t.2) || t.1 == '/');
    if let Some((_, '/', _)) = &tok {
        app.ensure_commands(chan);
    }
    let items = tok.as_ref().map(|(_, k, q)| candidates(app, chan, *k, q)).unwrap_or_default();
    let open = !items.is_empty() && (focused || typed);
    app.ac_open = open;
    if !open {
        app.ac_sel = 0;
    }
    app.ac_sel = app.ac_sel.min(items.len().saturating_sub(1));

    let mut send = false;
    let mut accept = None;
    let selection = egui::TextEdit::load_state(ui.ctx(), tid).and_then(|s| s.cursor.char_range()).filter(|r| !r.is_empty());
    // inside a ``` block left open, Enter is a new line and Tab a tab
    let in_code = draft_now.chars().take(cursor).collect::<String>().matches("```").count() % 2 == 1;
    let limit = if app.store.nitro() { 4000 } else { 2000 };
    if (focused || (nothing_focused && typed)) && !modal {
        if open {
            // ctrl+n walks the list too; ctrl+p is pins, which never reaches it
            if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::ArrowDown) || super::take_key(i, Modifiers::COMMAND, Key::N)) {
                app.ac_sel = (app.ac_sel + 1) % items.len();
            }
            if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::ArrowUp)) {
                app.ac_sel = (app.ac_sel + items.len() - 1) % items.len();
            }
            if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Tab) || super::take_key(i, Modifiers::NONE, Key::Enter)) {
                accept = Some(app.ac_sel);
            }
            if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape)) {
                app.ac_dismissed = tok.as_ref().map(|t| t.2.clone());
            }
        }
        if accept.is_none() && !in_code && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Enter)) {
            send = true;
        }
        if !in_code {
            ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Tab));
        }
        // shift+tab steps out of the composer onto the newest message, for discord's message keys
        if ui.input_mut(|i| super::take_key(i, Modifiers::SHIFT, Key::Tab))
            && let Some(m) = app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| !m.pending))
        {
            app.focus_msg = Some(m.id);
            app.focus_scroll = true;
            ui.memory_mut(|m| m.surrender_focus(tid));
        }
        if draft_now.is_empty() && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::ArrowUp)) {
            let me = app.store.me;
            if let Some(m) = app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.author == me && !m.pending)) {
                app.editing = Some((m.id, m.content.clone()));
            }
        }
        // ctrl+b and ctrl+u wrap the selection in ** and __, as discord's editor does
        if let Some(r) = selection {
            let mark = ui.input_mut(|i| {
                if super::take_key(i, Modifiers::COMMAND, Key::B) {
                    Some("**")
                } else if super::take_key(i, Modifiers::COMMAND, Key::U) {
                    Some("__")
                } else {
                    None
                }
            });
            if let Some(m) = mark {
                wrap(app, ui.ctx(), ch, tid, r, m);
            }
        }
    }
    // escape takes every upload card off, then drops a reply, wherever focus is (egui lets go of
    // the focus on escape before anything sees it), unless a popout or menu is open to take it
    let popped = app.layered(ui.ctx());
    if !app.files.is_empty() && !popped && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape)) {
        app.files.clear();
    }
    if app.reply.is_some() && app.editing.is_none() && !popped && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape)) {
        app.reply = None;
    }

    let hint = match chan.kind {
        1 | 3 => format!("Message @{}", channel_title(&app.store, ch)),
        2 | 13 => format!("Message {}", chan.name),
        _ => format!("Message #{}", chan.name),
    };
    let top_r = if app.reply.is_some() { 0 } else { 8 };
    let mut attach = false;
    let mut tab = None;
    let mut apps_click = false;
    let mut gift_click = false;
    let plugin_bar = app.vc_chatbar(ch);
    let mut plugin_click: Option<String> = None;
    let mut send_click = false;
    let mut gif_rect = None;
    let mut remove = None;
    let mut new_cursor = None;
    let mut colon_at = None;
    // where a selection starts on screen, for the formatting toolbar over it
    let mut sel_at = None;
    let colon = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == ":")));
    let before = draft_now.len();
    let radius = CornerRadius { nw: top_r, ne: top_r, sw: 8, se: 8 };
    let r = Frame::new()
        .fill(pal().bg_input)
        .inner_margin(1)
        .corner_radius(radius)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !app.files.is_empty() {
                Frame::new().inner_margin(Margin { left: 10, right: 10, top: 20, bottom: 10 }).show(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt("uploads").show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 24.0;
                            for (i, f) in app.files.iter_mut().enumerate() {
                                match upload_card(ui, f) {
                                    Some(CardAct::Remove) => remove = Some(i),
                                    Some(CardAct::Spoiler) => f.spoiler = !f.spoiler,
                                    None => {}
                                }
                            }
                        });
                    });
                });
                let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(sep.x_range(), sep.center().y, Stroke::new(1.0, pal().bg_accent.gamma_multiply(0.5)));
            }
            Frame::new().inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 0 }).show(ui, |ui| {
                // discord's 56px text area, its 22px line centred
                ui.set_min_height(56.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (pr, presp) = ui.allocate_exact_size(vec2(24.0, 56.0), Sense::click());
                    let col = if presp.hovered() { pal().interactive_hover } else { pal().header_secondary };
                    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pr.center(), vec2(20.0, 20.0)), col);
                    if presp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if presp.on_hover_text("Upload a File").clicked() {
                        attach = true;
                    }
                    // discord's text starts 64 in: 16, the 24 button, then 24 more
                    ui.add_space(23.0);
                    let draft = app.drafts.entry(ch).or_default();
                    // the text stops short of the buttons: five 40 wide, six with the send button
                    let buttons = if app.settings.send_button { 240.0 } else { 200.0 };
                    let w = (ui.available_width() - buttons - 8.0).max(60.0);
                    let mut text_pos = egui::Pos2::ZERO;
                    egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.5).id_salt("composer_scroll").show(ui, |ui| {
                        // the row is horizontal; padding above and below the text has to be vertical
                        ui.vertical(|ui| {
                            ui.add_space(17.0);
                            let out = egui::TextEdit::multiline(draft)
                                .id(tid)
                                .frame(Frame::NONE)
                                .desired_rows(1)
                                .margin(Margin::ZERO)
                                .font(reg(16.0))
                                .text_color(pal().text_normal)
                                .desired_width(w)
                                .lock_focus(true)
                                .show(ui);
                            text_pos = out.galley_pos;
                            if let Some(r) = out.cursor_range.filter(|r| !r.is_empty() && out.response.has_focus()) {
                                let [a, _] = r.sorted_cursors();
                                sel_at = Some(out.galley_pos + out.galley.pos_from_cursor(a).left_top().to_vec2());
                            }
                            if draft.is_empty() {
                                // painted here rather than as the hint so its emoji come from the system font, as discord's do
                                let t = super::text::layout_em(ui.ctx(), &hint, reg(16.0), pal().channel_default, w, 1.0);
                                t.paint(ui.painter(), out.galley_pos);
                            }
                            if colon {
                                colon_at = out.cursor_range.map(|r| r.primary.index.0);
                            }
                            ui.add_space(17.0);
                        });
                    });
                    app.rects.insert("composer_text".into(), Rect::from_min_size(text_pos, vec2(1.0, 1.0)));
                    // discord's row, right to left: apps, emoji, sticker, gif, gift; 20px icons 40 apart
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        let slot = |ui: &mut Ui, icon: Icon, tip: &str| {
                            let (r, resp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
                            let col = if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
                            icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), col);
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            resp.on_hover_text(tip)
                        };
                        if app.settings.send_button {
                            let (sr, sresp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
                            let col = if sresp.hovered() { BLURPLE } else { pal().header_secondary };
                            icons::draw(ui.painter(), Icon::Send, Rect::from_center_size(sr.center(), vec2(20.0, 20.0)), col);
                            if sresp.on_hover_text("Send Message").clicked() {
                                send_click = true;
                            }
                        }
                        apps_click = slot(ui, Icon::Apps, "Apps").clicked();
                        let er = slot(ui, Icon::Smile, "Select emoji");
                        app.rects.insert("emoji_button".into(), er.rect);
                        if er.clicked() {
                            tab = Some(super::picker::Tab::Emoji);
                        }
                        let sr = slot(ui, Icon::Sticker, "Open sticker picker");
                        app.rects.insert("sticker_button".into(), sr.rect);
                        if sr.clicked() {
                            tab = Some(super::picker::Tab::Stickers);
                        }
                        let gr = slot(ui, Icon::Gif, "Open GIF picker");
                        gif_rect = Some(gr.rect);
                        if gr.clicked() {
                            tab = Some(super::picker::Tab::Gifs);
                        }
                        gift_click = slot(ui, Icon::Gift, "Send a gift").clicked();
                        for b in plugin_bar.as_array().into_iter().flatten() {
                            let (r, resp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
                            let col = if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
                            match b["icon"].as_str() {
                                Some(svg) => super::vencord::svg(ui, svg, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), col),
                                None => {
                                    ui.painter().text(r.center(), Align2::CENTER_CENTER, b["tooltip"].as_str().unwrap_or("?").chars().next().unwrap_or('?'), semi(14.0), col);
                                }
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            app.rects.insert(format!("chatbar:{}", b["key"].as_str().unwrap_or("")), r);
                            let resp = resp.on_hover_text(b["tooltip"].as_str().unwrap_or(""));
                            let a = if resp.secondary_clicked() { b["menu"].as_str() } else if resp.clicked() { b["click"].as_str() } else { None };
                            if let Some(a) = a {
                                plugin_click = Some(a.to_string());
                            }
                        }
                    });
                });
            });
        });
    let crect = r.response.rect;
    // discord's border sits on the box's own fill, not beside it
    ui.painter().rect_stroke(crect, radius, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    app.rects.insert("composer".into(), crect);
    if let (Some(at), Some(r)) = (sel_at, selection) {
        if let Some(m) = format_bar(ui.ctx(), at) {
            wrap(app, ui.ctx(), ch, tid, r, m);
        }
    }
    // discord's count of what's left, once the end is near; past it the number goes red and
    // negative, and Enter asks to send the lot as a file instead
    let used = app.drafts.get(&ch).map_or(0, |d| d.chars().count());
    let left = limit as i64 - used as i64;
    if left <= 200 || (limit > 2000 && used > 2000) {
        let col = if left < 0 { RED } else { pal().text_muted };
        let g = ui.painter().layout_no_wrap(left.to_string(), reg(12.0), col);
        let at = pos2(crect.right() - 16.0 - g.size().x, crect.bottom() - 4.0 - g.size().y);
        let r = Rect::from_min_size(at, g.size());
        ui.painter().galley(at, g, col);
        app.rects.insert("char_count".into(), r);
        let tip = if limit > 2000 {
            format!("Nitro is powering you with {limit} characters per message! {left} characters remaining")
        } else {
            format!("{left} characters remaining")
        };
        ui.interact(r, egui::Id::new("char_count"), Sense::hover()).on_hover_text(tip);
    }

    if open {
        let picked = popup(app, ui.ctx(), chan, &items, crect, tok.as_ref().map(|t| t.1).unwrap_or('@'));
        if picked.is_some() {
            accept = picked;
        }
    }
    if let (Some(i), Some((start, _, _))) = (accept, &tok) {
        new_cursor = complete(app, chan, &items[i], *start, cursor);
    }
    if let Some(c) = colon_at {
        new_cursor = new_cursor.or(close_colon(app, chan, c));
    }
    if let Some(c) = new_cursor {
        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), tid) {
            st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c))));
            st.store(ui.ctx(), tid);
        }
        ui.memory_mut(|m| m.request_focus(tid));
    }
    if let Some(i) = remove {
        app.files.remove(i);
    }
    if attach && let Some(files) = rfd::FileDialog::new().pick_files() {
        app.files.extend(files.into_iter().map(Upload::new));
    }

    if let Some(a) = plugin_click {
        app.vc_act(&a, ui.input(|i| i.modifiers.shift));
    }
    if gift_click {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://discord.com/nitro"));
    }
    // the apps button opens what fastcord has of the app launcher: the slash command list
    if apps_click {
        let d = app.drafts.entry(ch).or_default();
        if d.is_empty() {
            d.push('/');
        }
        app.focus_composer = true;
    }
    if let Some(r) = gif_rect {
        app.rects.insert("gif_button".into(), r);
    }
    if let Some(t) = tab {
        super::picker::toggle(app, t);
    }
    let changed = app.drafts.get(&ch).is_some_and(|d| d.len() != before && !d.is_empty() && !d.starts_with('/'));
    if changed && app.last_typing.is_none_or(|(c, t)| c != ch || t.elapsed().as_secs() >= 8) {
        app.last_typing = Some((ch, Instant::now()));
        app.net.send(Cmd::Typing { channel: ch });
    }
    if app.drafts.get(&ch).is_some_and(|d| d.is_empty()) {
        app.ac_dismissed = None;
    }
    if (send || send_click) && used > limit {
        app.confirm = Some(
            super::Confirm::new("Your message is too long...", "Your message is too long.", "Upload as File", move |app: &mut App| {
                let text = app.drafts.remove(&ch).unwrap_or_default();
                app.attach_text(&text);
                app.send_message(ch);
            })
            .blurple(),
        );
    } else if (send || send_click) && !app.quick_react(chan) && !app.substitute(chan) && !app.try_command(chan) && !app.vc_command(chan) && !app.built_in(chan) {
        app.send_message(ch);
    }
    typing_line(app, ui, chan, Rect::from_min_max(pos2(crect.left(), stack_top), crect.max));
}

enum CardAct {
    Remove,
    Spoiler,
}

fn upload_card(ui: &mut Ui, f: &Upload) -> Option<CardAct> {
    let (r, _) = ui.allocate_exact_size(vec2(216.0, 216.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(4), pal().bg_secondary);
    let thumb = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(200.0, 160.0));
    if let Some(bytes) = &f.thumb {
        let uri = format!("bytes://upload/{}", f.path.to_string_lossy());
        egui::Image::from_bytes(uri, bytes.clone())
            .corner_radius(CornerRadius::same(3))
            .maintain_aspect_ratio(true)
            .show_loading_spinner(false)
            .paint_at(ui, thumb);
        if f.spoiler {
            p.rect_filled(thumb, CornerRadius::same(3), Color32::from_black_alpha(230));
            p.text(thumb.center(), Align2::CENTER_CENTER, "SPOILER", semi(14.0), Color32::WHITE);
        }
    } else {
        icons::draw(p, Icon::File, Rect::from_center_size(thumb.center(), vec2(72.0, 72.0)), BLURPLE);
    }
    text::paint(p, pos2(r.left() + 8.0, r.bottom() - 24.0), Align2::LEFT_CENTER, &f.name, reg(14.0), pal().text_normal, 200.0);

    // the little action strip that floats over the top-right corner
    let bar = Rect::from_min_size(pos2(r.right() - 76.0, r.top() - 12.0), vec2(80.0, 32.0));
    p.rect(bar, CornerRadius::same(4), pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let mut act = None;
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink(2.0)).layout(Layout::left_to_right(Align::Center)));
    b.spacing_mut().item_spacing.x = 0.0;
    if f.is_image() && icon_button(&mut b, Icon::Image, 28.0, "Spoiler Attachment").clicked() {
        act = Some(CardAct::Spoiler);
    }
    let del = b.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let col = if del.1.hovered() { RED } else { pal().header_secondary };
    icons::draw(b.painter(), Icon::Trash, Rect::from_center_size(del.0.center(), vec2(20.0, 20.0)), col);
    if del.1.on_hover_text("Remove Attachment").clicked() {
        act = Some(CardAct::Remove);
    }
    act
}

fn popup(app: &mut App, ctx: &egui::Context, chan: &Channel, items: &[Item], composer: Rect, kind: char) -> Option<usize> {
    let mut picked = None;
    let title = match kind {
        '@' => "Members".to_string(),
        '#' => "Text Channels".to_string(),
        ':' => "Emoji Matching".to_string(),
        _ => "Commands".to_string(),
    };
    let row_h = if kind == '/' { 52.0 } else { 34.0 };
    egui::Area::new(egui::Id::new("autocomplete"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(composer.left_top())
        .show(ctx, |ui| {
            Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                ui.set_width(composer.width() - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                ui.painter().text(pos2(hr.left() + 8.0, hr.center().y), Align2::LEFT_CENTER, title, medium(14.0), pal().header_secondary);
                // the area only offers the space below the composer, so the list has to ask for its height
                let list_h = row_h * items.len().min(10) as f32;
                egui::ScrollArea::vertical().min_scrolled_height(list_h).max_height(list_h).show(ui, |ui| {
                    for (i, it) in items.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                            app.ac_sel = i;
                        }
                        if i == app.ac_sel {
                            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                        }
                        row(app, ui, chan, it, r);
                        if matches!(it, Item::Emoji(e) if e.locked) {
                            resp.clone().on_hover_text("Get Nitro, or the role this emoji needs, to use it here");
                        }
                        if resp.clicked() {
                            picked = Some(i);
                        }
                    }
                });
            });
        });
    picked
}

fn row(app: &App, ui: &Ui, chan: &Channel, it: &Item, r: Rect) {
    let p = ui.painter();
    let y = r.center().y;
    let x = r.left() + 8.0;
    match it {
        Item::User(u, name, username) => {
            if let Some(user) = app.store.user(*u) {
                avatar(ui, Rect::from_center_size(pos2(x + 12.0, y), vec2(24.0, 24.0)), &user.avatar_url());
            }
            let nr = text::paint(p, pos2(x + 32.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary, r.width() * 0.5);
            p.text(pos2(nr.right() + 8.0, y), Align2::LEFT_CENTER, username, reg(14.0), pal().header_secondary);
        }
        Item::Role(_, name, color) => {
            let col = if *color == 0 { pal().header_primary } else { role_color(*color) };
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{name}"), semi(15.0), col);
        }
        Item::Everyone(e) => {
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{e}"), semi(15.0), pal().header_primary);
            let note = if *e == "here" { "Notify everyone online in this channel." } else { "Notify everyone who has permission to view this channel." };
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, note, reg(13.0), pal().header_secondary);
        }
        Item::Channel(id, name) => {
            let ir = Rect::from_center_size(pos2(x + 10.0, y), vec2(18.0, 18.0));
            match app.store.channels.get(id) {
                Some(c) => super::side::chan_icon(&app.store, p, c, ir, pal().channel_default),
                None => icons::draw(p, Icon::Hash, ir, pal().channel_default),
            }
            text::paint(p, pos2(x + 28.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary, r.width() - 160.0);
            let cat = app
                .store
                .channels
                .get(id)
                .and_then(|c| app.store.channels.get(&c.parent))
                .map(|c| c.name.to_uppercase())
                .unwrap_or_default();
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, cat, semi(12.0), pal().header_secondary);
        }
        Item::Emoji(e) => {
            let er = Rect::from_center_size(pos2(x + 12.0, y), vec2(22.0, 22.0));
            let fade = if e.locked { 0.4 } else { 1.0 };
            match &e.url {
                Some(u) => text::image_tint(p, u, er, Color32::WHITE.gamma_multiply(fade)),
                None => text::image(p, &crate::markdown::twemoji(&e.raw), er),
            }
            p.text(pos2(x + 32.0, y), Align2::LEFT_CENTER, format!(":{}:", e.name), semi(15.0), pal().header_primary.gamma_multiply(fade));
            if let Some(srv) = &e.server {
                text::paint(p, pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, srv, semi(12.0), pal().header_secondary.gamma_multiply(fade), r.width() * 0.4);
            }
        }
        Item::Command(i) => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            let Some(c) = app.commands.get(&scope).and_then(|c| c.list.get(*i)) else { return };
            let app_id = sid(&c["application_id"]);
            let (app_name, icon) = app
                .commands
                .get(&scope)
                .and_then(|cs| cs.apps.iter().find(|a| a.0 == app_id))
                .map(|a| (a.1.clone(), a.2.clone()))
                .unwrap_or_default();
            let ir = Rect::from_center_size(pos2(x + 16.0, y), vec2(32.0, 32.0));
            match icon {
                Some(h) => avatar(ui, ir, &format!("{}/app-icons/{app_id}/{h}.png?size=64", crate::api::cdn())),
                None => {
                    p.circle_filled(ir.center(), 16.0, BLURPLE);
                }
            }
            let name = c["name"].as_str().unwrap_or("");
            let mut args = String::new();
            for o in c["options"].as_array().into_iter().flatten() {
                if o["required"].as_bool() == Some(true) {
                    args.push_str(&format!(" {}", o["name"].as_str().unwrap_or("")));
                }
            }
            let nr = p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
            p.text(pos2(nr.right() + 6.0, y - 9.0), Align2::LEFT_CENTER, args, reg(14.0), pal().header_secondary);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, c["description"].as_str().unwrap_or(""), reg(13.0), pal().header_secondary, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, app_name, semi(13.0), pal().header_secondary);
        }
        Item::BuiltIn(i) => {
            let (name, args, desc) = BUILT_IN[*i];
            let ir = Rect::from_center_size(pos2(x + 16.0, y), vec2(32.0, 32.0));
            p.circle_filled(ir.center(), 16.0, BLURPLE);
            icons::draw(p, Icon::Discord, ir.shrink(8.0), Color32::WHITE);
            let nr = p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
            p.text(pos2(nr.right() + 6.0, y - 9.0), Align2::LEFT_CENTER, args, reg(14.0), pal().header_secondary);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, desc, reg(13.0), pal().header_secondary, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, "Built-In", semi(13.0), pal().header_secondary);
        }
        Item::Plugin(name, desc, plugin) => {
            let ir = Rect::from_center_size(pos2(x + 16.0, y), vec2(32.0, 32.0));
            p.circle_filled(ir.center(), 16.0, BLURPLE);
            p.text(ir.center(), Align2::CENTER_CENTER, "V", semi(15.0), Color32::WHITE);
            p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, desc, reg(13.0), pal().header_secondary, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, plugin.as_str(), semi(13.0), pal().header_secondary);
        }
    }
}

/// Puts `mark` either side of the selection, or takes it off when it's already there, the words
/// staying selected.
fn wrap(app: &mut App, ctx: &egui::Context, ch: Id, tid: egui::Id, r: CCursorRange, mark: &str) {
    let Some(draft) = app.drafts.get_mut(&ch) else { return };
    let [a, b] = r.sorted_cursors();
    let (a, b) = (a.index.0, b.index.0);
    let byte = |d: &str, c: usize| d.char_indices().nth(c).map_or(d.len(), |(i, _)| i);
    let (ba, bb) = (byte(draft, a), byte(draft, b));
    let n = mark.chars().count();
    let (na, nb) = if draft[..ba].ends_with(mark) && draft[bb..].starts_with(mark) {
        draft.replace_range(bb..bb + mark.len(), "");
        draft.replace_range(ba - mark.len()..ba, "");
        (a - n, b - n)
    } else {
        draft.insert_str(bb, mark);
        draft.insert_str(ba, mark);
        (a + n, b + n)
    };
    if let Some(mut st) = egui::TextEdit::load_state(ctx, tid) {
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(na), CCursor::new(nb))));
        st.store(ctx, tid);
    }
    ctx.memory_mut(|m| m.request_focus(tid));
}

/// Discord's formatting bar over selected text: bold, italic, underline, strikethrough, then
/// spoiler past a rule. A press, not a click, so the composer keeps its selection.
fn format_bar(ctx: &egui::Context, at: Pos2) -> Option<&'static str> {
    let mut pick = None;
    egui::Area::new(egui::Id::new("format_bar")).order(egui::Order::Foreground).fixed_pos(at - vec2(4.0, 48.0)).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(4).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (icon, tip, mark) in [
                    (Icon::Bold, "Bold", "**"),
                    (Icon::Italic, "Italic", "*"),
                    (Icon::Underline, "Underline", "__"),
                    (Icon::Strike, "Strikethrough", "~~"),
                    (Icon::Eye, "Spoiler", "||"),
                ] {
                    if mark == "||" {
                        let (r, _) = ui.allocate_exact_size(vec2(13.0, 32.0), Sense::hover());
                        ui.painter().vline(r.center().x, r.y_range().shrink(6.0), Stroke::new(1.0, pal().border_subtle));
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 4, pal().control_hover);
                    }
                    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "format", tip)), r));
                    if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                        pick = Some(mark);
                    }
                    resp.on_hover_text(tip);
                }
            });
        });
    });
    pick
}

/// Replace the token with the chosen completion, returning the new cursor (in chars).
fn complete(app: &mut App, chan: &Channel, it: &Item, start: usize, cursor: usize) -> Option<usize> {
    let ch = chan.id;
    let draft = app.drafts.entry(ch).or_default();
    let cur_b = draft.char_indices().nth(cursor).map(|(b, _)| b).unwrap_or(draft.len());
    let (shown, raw) = match it {
        Item::User(u, name, _) => (format!("@{name}"), format!("<@{u}>")),
        Item::Role(id, name, _) => (format!("@{name}"), format!("<@&{id}>")),
        Item::Everyone(e) => (format!("@{e}"), format!("@{e}")),
        Item::Channel(id, name) => (format!("#{name}"), format!("<#{id}>")),
        Item::Emoji(e) if e.locked => (format!(":{}:", e.name), String::new()),
        Item::Emoji(e) if e.url.is_some() => (format!(":{}:", e.name), e.raw.clone()),
        Item::Emoji(e) => (e.raw.clone(), e.raw.clone()),
        Item::Command(i) => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            let name = app.commands.get(&scope).and_then(|c| c.list.get(*i)).and_then(|c| c["name"].as_str()).unwrap_or("").to_string();
            (format!("/{name}"), String::new())
        }
        Item::BuiltIn(i) => (format!("/{}", BUILT_IN[*i].0), String::new()),
        Item::Plugin(name, ..) => (format!("/{name}"), String::new()),
    };
    let insert = format!("{shown} ");
    draft.replace_range(start..cur_b, &insert);
    if shown != raw && !raw.is_empty() {
        app.aliases.entry(ch).or_default().push((shown.clone(), raw));
    }
    let new_cursor = draft[..start + insert.len()].chars().count();
    app.ac_sel = 0;
    Some(new_cursor)
}

/// The `:name:` that ends `before`, as (byte offset of the opening colon, name).
fn typed_name(before: &str) -> Option<(usize, &str)> {
    let body = before.strip_suffix(':')?;
    let start = body.rfind(':')?;
    let name = &body[start + 1..];
    let fine = name.len() >= 2 && name.chars().all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c));
    (fine && !body[..start].chars().last().is_some_and(char::is_alphanumeric)).then_some((start, name))
}

/// Typing the closing colon of a unicode name swaps in the glyph, like discord. Custom names
/// stay as typed and turn into the emoji when the message goes out.
fn close_colon(app: &mut App, chan: &Channel, cursor: usize) -> Option<usize> {
    let draft = app.drafts.get(&chan.id)?;
    let end = draft.char_indices().nth(cursor).map(|(b, _)| b).unwrap_or(draft.len());
    let (start, name) = typed_name(&draft[..end])?;
    if draft[..start].matches('`').count() % 2 == 1 {
        return None;
    }
    let glyph = match app.emoji_named(chan.guild, name)? {
        (_, true) => return None,
        (glyph, false) => glyph,
    };
    let draft = app.drafts.get_mut(&chan.id)?;
    draft.replace_range(start..end, &glyph);
    Some(draft[..start + glyph.len()].chars().count())
}

/// Runs `f` over the stretches of `text` that aren't code, so nothing gets rewritten inside
/// `inline` spans or fenced blocks.
fn outside_code(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        out.push_str(&f(&rest[..i]));
        let n = rest[i..].chars().take_while(|c| *c == '`').count();
        let after = &rest[i + n..];
        // a span closes on the next run of the same length; with no closer the ticks are plain text
        let mut at = 0;
        let close = loop {
            let Some(j) = after[at..].find('`') else { break None };
            let run = after[at + j..].chars().take_while(|c| *c == '`').count();
            if run == n {
                break Some(at + j + n);
            }
            at += j + run;
        };
        let end = close.unwrap_or(0);
        out.push_str(&rest[i..i + n + end]);
        rest = &after[end..];
    }
    out.push_str(&f(rest));
    out
}

/// What a reaction is made of: a ready-made `<:name:id>`, or a unicode emoji.
fn react_emoji(raw: &str) -> Option<crate::state::ReactEmoji> {
    if custom_len(raw) == Some(raw.len()) {
        let animated = raw.starts_with("<a:");
        let inner = raw.trim_start_matches("<a:").trim_start_matches("<:").trim_end_matches('>');
        let (name, id) = inner.split_once(':')?;
        return Some(crate::state::ReactEmoji { id: id.parse().ok()?, name: name.to_string(), animated });
    }
    emojis::get(raw).map(|_| crate::state::ReactEmoji { id: 0, name: raw.to_string(), animated: false })
}

/// Length of a ready-made `<:name:id>` / `<a:name:id>` at the start of `s`.
fn custom_len(s: &str) -> Option<usize> {
    let body = s.strip_prefix('<')?;
    let end = body.find('>')?;
    let inner = body[..end].strip_prefix('a').unwrap_or(&body[..end]);
    let mut parts = inner.strip_prefix(':')?.split(':');
    let (name, id) = (parts.next()?, parts.next()?);
    let ok = parts.next().is_none() && !name.is_empty() && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    ok.then_some(end + 2)
}

/// Discord's sits in the 24 above the composer, over the bottom of the messages, which fade out
/// under it. Below the composer there's only discord's 8.
fn typing_line(app: &App, ui: &mut Ui, chan: &Channel, composer: Rect) {
    ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    let users: Vec<Id> = app.store.typing.get(&chan.id).map(|v| v.iter().map(|(u, _)| *u).collect()).unwrap_or_default();
    if users.is_empty() {
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("typing")));
    let wide = ui.max_rect().x_range().expand(8.0);
    let fade = Rect::from_x_y_ranges(wide, composer.top() - 16.0..=composer.top());
    let mut m = egui::Mesh::default();
    let (clear, bg) = (pal().bg_primary.gamma_multiply(0.0), pal().bg_primary);
    m.colored_vertex(fade.left_top(), clear);
    m.colored_vertex(fade.right_top(), clear);
    m.colored_vertex(fade.right_bottom(), bg);
    m.colored_vertex(fade.left_bottom(), bg);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(m);

    let y = composer.top() - 12.0;
    let t = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((t * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        p.circle_filled(pos2(composer.left() + 19.5 + i as f32 * 8.75, y), 3.5, pal().text_normal.gamma_multiply(0.3 + 0.7 * phase));
    }
    let names: Vec<String> = users.iter().map(|u| app.store.name_in(chan.guild, *u)).collect();
    let parts: Vec<(&str, bool)> = match names.as_slice() {
        [a] => vec![(a, true), (" is typing...", false)],
        [a, b] => vec![(a, true), (" and ", false), (b, true), (" are typing...", false)],
        [a, b, c] => vec![(a, true), (", ", false), (b, true), (", and ", false), (c, true), (" are typing...", false)],
        _ => vec![("Several people are typing...", false)],
    };
    let mut job = egui::text::LayoutJob::default();
    for (s, name) in parts {
        let (font, color) = if name { (semi(12.0), pal().text_normal) } else { (medium(12.0), pal().header_secondary) };
        job.append(s, 0.0, egui::TextFormat::simple(font, color));
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(composer.width() - 80.0);
    let g = ui.painter().layout_job(job);
    p.galley(pos2(composer.left() + 64.5, y - g.size().y / 2.0), g, pal().text_normal);
}

impl App {
    pub fn ensure_commands(&mut self, chan: &Channel) {
        let scope = if chan.guild != 0 { chan.guild } else { chan.id };
        if self.commands.contains_key(&scope) {
            return;
        }
        self.commands.insert(scope, Commands::default());
        let path = if chan.guild != 0 {
            format!("/guilds/{}/application-command-index", chan.guild)
        } else {
            format!("/channels/{}/application-command-index", chan.id)
        };
        self.rest("GET", path, None, Tag::Commands(scope));
    }

    pub fn set_commands(&mut self, scope: Id, v: &Value) {
        let apps = v["applications"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| (sid(&a["id"]), a["name"].as_str().unwrap_or("").to_string(), a["icon"].as_str().map(str::to_string)))
            .collect();
        let list = v["application_commands"].as_array().cloned().unwrap_or_default();
        self.commands.insert(scope, Commands { apps, list });
    }

    /// Send "/name args" as an application command if it names one we know.
    pub fn try_command(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some(rest) = draft.trim().strip_prefix('/') else { return false };
        let scope = if chan.guild != 0 { chan.guild } else { chan.id };
        let mut words = rest.splitn(2, char::is_whitespace);
        let name = words.next().unwrap_or("");
        let args = words.next().unwrap_or("").trim().to_string();
        let Some(cmd) = self.commands.get(&scope).and_then(|c| c.list.iter().find(|c| c["name"] == name)).cloned() else {
            return false;
        };
        let args = self.resolve_aliases(ch, &args);
        let (options, err) = build_options(&cmd["options"], &args);
        if let Some(e) = err {
            self.toast(e);
            return true;
        }
        let mut payload = json!({
            "type": 2,
            "application_id": cmd["application_id"],
            "channel_id": ch.to_string(),
            "session_id": self.store.session,
            "nonce": make_nonce(),
            "analytics_location": "slash_ui",
            "data": {
                "version": cmd["version"],
                "id": cmd["id"],
                "name": name,
                "type": 1,
                "options": options,
                "application_command": cmd,
                "attachments": [],
            }
        });
        if chan.guild != 0 {
            payload["guild_id"] = json!(chan.guild.to_string());
        }
        self.net.send(Cmd::Interact(payload));
        self.drafts.remove(&ch);
        self.aliases.remove(&ch);
        self.jump_to_present(ch);
        true
    }

    /// Click a button or pick a select option on a bot message.
    pub fn press(&mut self, ch: Id, msg: Id, custom_id: &str, values: Option<Vec<String>>) {
        let Some(m) = self.store.find_msg(ch, msg) else { return };
        let guild = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
        let data = match values {
            Some(v) => json!({ "component_type": 3, "custom_id": custom_id, "type": 3, "values": v }),
            None => json!({ "component_type": 2, "custom_id": custom_id }),
        };
        let mut payload = json!({
            "type": 3,
            "nonce": make_nonce(),
            "channel_id": ch.to_string(),
            "message_flags": m.flags,
            "message_id": msg.to_string(),
            "application_id": m.app_id.to_string(),
            "session_id": self.store.session,
            "data": data,
        });
        if guild != 0 {
            payload["guild_id"] = json!(guild.to_string());
        }
        self.net.send(Cmd::Interact(payload));
    }

    /// `+:fire:` then Enter: a reaction on the newest message rather than a message.
    pub fn quick_react(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).map(|d| d.trim().to_string()).unwrap_or_default();
        let Some(rest) = draft.strip_prefix('+') else { return false };
        let raw = match rest.strip_prefix(':').and_then(|r| r.strip_suffix(':')) {
            Some(name) => self.emoji_named(chan.guild, name).map(|(raw, _)| raw),
            None => Some(rest.to_string()),
        };
        let Some(em) = raw.and_then(|r| react_emoji(&r)) else { return false };
        let Some(last) = self.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| !m.pending)).map(|m| m.id) else { return false };
        self.react(ch, last, em, true);
        self.drafts.remove(&ch);
        true
    }

    /// `s/old/new` then Enter: your newest message with its first "old" turned into "new".
    pub fn substitute(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some((old, new)) = draft.trim().strip_prefix("s/").and_then(|r| r.split_once('/')) else { return false };
        if old.is_empty() {
            return false;
        }
        let new = new.strip_suffix('/').unwrap_or(new);
        let me = self.store.me;
        let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().rev().find(|m| m.author == me && !m.pending)) else { return false };
        if let Some(at) = m.content.find(old) {
            let mut text = m.content.clone();
            text.replace_range(at..at + old.len(), new);
            m.content = text.clone();
            m.edited = true;
            self.net.send(Cmd::Edit { channel: ch, id: m.id, content: text });
        }
        self.drafts.remove(&ch);
        true
    }

    /// Discord's built-in commands, typed out: the text ones send what they'd send, the rest
    /// do their thing and say how it went.
    pub fn built_in(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some(rest) = draft.trim().strip_prefix('/') else { return false };
        let (name, args) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(n, a)| (n, a.trim()));
        if name == "gif" {
            self.drafts.remove(&ch);
            let at = self.rects.get("gif_button").map_or(pos2(800.0, 600.0), |r| r.left_top());
            self.open_gifs(at);
            self.search_gifs(args.to_string());
            return true;
        }
        let Some(&(_, opts, _)) = BUILT_IN.iter().find(|(n, ..)| *n == name).filter(|_| built_in_here(self, chan, name)) else { return false };
        let args = self.resolve_aliases(ch, args);
        // the options as discord types them, so the usual parsing fills them in
        let kinds: Vec<Value> = opts
            .split(' ')
            .map(|o| {
                let kind = match o {
                    "user" => 6,
                    "duration" | "delete_messages" => 4,
                    "silent" => 5,
                    _ => 3,
                };
                let required = matches!((name, o), ("me" | "spoiler" | "tts" | "thread" | "msg", _) | (_, "user" | "duration"));
                json!({ "name": o, "type": kind, "required": required })
            })
            .collect();
        let (got, err) = build_options(&json!(kinds), &args);
        if let Some(e) = err {
            self.toast(e);
            return true;
        }
        let get = |k: &str| got.iter().find(|o| o["name"] == k).map(|o| o["value"].clone());
        let text = |k: &str| get(k).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let user = get("user").and_then(|v| v.as_str().and_then(|s| s.parse::<Id>().ok()));
        let who = user.map(|u| self.store.name_in(chan.guild, u)).unwrap_or_default();
        let g = chan.guild;
        self.drafts.remove(&ch);
        self.aliases.remove(&ch);
        let say = |app: &mut App, out: String, tts: bool| {
            app.drafts.insert(ch, out);
            app.post(ch, tts);
        };
        match name {
            "shrug" => say(self, format!("{} ¯\\_(ツ)_/¯", text("message")).trim().to_string(), false),
            "tableflip" => say(self, format!("{} (╯°□°)╯︵ ┻━┻", text("message")).trim().to_string(), false),
            "unflip" => say(self, format!("{} ┬─┬ノ( º _ ºノ)", text("message")).trim().to_string(), false),
            "me" => say(self, format!("_{}_", text("message")), false),
            "spoiler" => say(self, format!("||{}||", text("message")), false),
            "tts" => say(self, text("message"), true),
            "nick" => {
                let nick = text("new_nick");
                self.rest("PATCH", format!("/guilds/{g}/members/@me"), Some(json!({ "nick": nick })), Tag::Quiet);
                self.toast(if nick.is_empty() { "Your nickname on this server has been reset.".to_string() } else { format!("Your nickname on this server has been changed to {nick}.") });
            }
            "thread" => {
                self.rest("POST", format!("/channels/{ch}/threads"), Some(json!({ "name": text("name"), "type": 11, "auto_archive_duration": 4320 })), Tag::Thread(text("message")));
            }
            "kick" => {
                if let Some(u) = user {
                    self.rest("DELETE", format!("/guilds/{g}/members/{u}"), None, Tag::Quiet);
                    self.toast(format!("{who} was kicked from the server."));
                }
            }
            "ban" => {
                if let Some(u) = user {
                    let secs = get("delete_messages").and_then(|v| v.as_i64()).unwrap_or(0);
                    self.rest("PUT", format!("/guilds/{g}/bans/{u}"), Some(json!({ "delete_message_seconds": secs })), Tag::Quiet);
                    self.toast(format!("{who} was banned from the server."));
                }
            }
            "timeout" => {
                if let Some(u) = user {
                    let secs = get("duration").and_then(|v| v.as_i64()).unwrap_or(60);
                    self.time_out(g, u, secs);
                    self.toast(format!("{who} timed out for {secs} seconds."));
                }
            }
            "msg" => {
                if let Some(u) = user {
                    self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [u.to_string()] })), Tag::InviteDm(text("message")));
                }
            }
            "leave" => {
                let silent = get("silent").and_then(|v| v.as_bool()).unwrap_or(false);
                self.rest("DELETE", format!("/channels/{ch}?silent={silent}"), None, Tag::Quiet);
                self.store.dms.retain(|d| *d != ch);
                self.friends = true;
            }
            _ => {}
        }
        true
    }

    /// Text too long to send as a message goes up as message.txt, as discord does with a paste.
    pub fn attach_text(&mut self, text: &str) {
        let dir = std::env::temp_dir().join("fastcord").join(chrono::Utc::now().timestamp_millis().to_string());
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("message.txt");
        if std::fs::write(&path, text).is_ok() {
            self.files.push(Upload::new(path));
        }
    }

    /// One of an app's message commands from a message's Apps submenu.
    pub fn message_command(&mut self, chan: &Channel, cmd: &Value, target: Id) {
        let mut payload = json!({
            "type": 2,
            "application_id": cmd["application_id"],
            "channel_id": chan.id.to_string(),
            "session_id": self.store.session,
            "nonce": make_nonce(),
            "analytics_location": "message_context_menu",
            "data": {
                "version": cmd["version"],
                "id": cmd["id"],
                "name": cmd["name"],
                "type": 3,
                "target_id": target.to_string(),
                "options": [],
                "application_command": cmd,
                "attachments": [],
            }
        });
        if chan.guild != 0 {
            payload["guild_id"] = json!(chan.guild.to_string());
        }
        self.net.send(Cmd::Interact(payload));
    }

    /// A member timed out until `secs` from now.
    pub fn time_out(&mut self, guild: Id, user: Id, secs: i64) {
        let until = (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.rest("PATCH", format!("/guilds/{guild}/members/{user}"), Some(json!({ "communication_disabled_until": until })), Tag::Quiet);
    }

    pub fn resolve_aliases(&self, ch: Id, text: &str) -> String {
        let Some(list) = self.aliases.get(&ch) else { return text.to_string() };
        // longest first so "@ann" never eats half of "@anna"
        let mut list = list.clone();
        list.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
        outside_code(text, |part| list.iter().fold(part.to_string(), |acc, (shown, raw)| acc.replace(shown, raw)))
    }

    /// What `:name:` turns into here: the custom emoji we may use (this server's first), else a
    /// unicode glyph. The flag says which of the two it was.
    pub fn emoji_named(&self, guild: Id, name: &str) -> Option<(String, bool)> {
        let custom = self.store.emojis_for(guild).into_iter().find(|(_, e, ok)| *ok && e.name.eq_ignore_ascii_case(name));
        if let Some((_, e, _)) = custom {
            return Some((format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id), true));
        }
        super::picker::unicode(name).map(|g| (g.to_string(), false))
    }

    /// Typed `:name:`s that name an emoji we may send become the real thing; anything else,
    /// and anything in code, stays as typed.
    pub fn convert_emoji(&self, guild: Id, text: &str) -> String {
        outside_code(text, |part| {
            let mut out = String::new();
            let mut i = 0;
            while i < part.len() {
                let rest = &part[i..];
                if let Some(n) = custom_len(rest) {
                    out.push_str(&rest[..n]);
                    i += n;
                    continue;
                }
                let c = rest.chars().next().unwrap_or(':');
                if c == ':'
                    && let Some(end) = rest[1..].find(':').map(|e| i + e + 2)
                    && let Some((start, name)) = typed_name(&part[..end])
                    && start == i
                    && let Some((raw, _)) = self.emoji_named(guild, name)
                {
                    out.push_str(&raw);
                    i = end;
                    continue;
                }
                out.push(c);
                i += c.len_utf8();
            }
            out
        })
    }
}

/// Map typed arguments onto command options. `key:value` pairs win, the rest fill
/// required options in order and the last string option soaks up what's left.
fn build_options(opts: &Value, args: &str) -> (Vec<Value>, Option<String>) {
    let list: Vec<&Value> = opts.as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if let Some(first) = list.first()
        && matches!(first["type"].as_u64(), Some(1) | Some(2))
    {
        let mut it = args.splitn(2, char::is_whitespace);
        let sub = it.next().unwrap_or("");
        let rest = it.next().unwrap_or("");
        let Some(s) = list.iter().find(|o| o["name"] == sub) else {
            let names: Vec<&str> = list.iter().filter_map(|o| o["name"].as_str()).collect();
            return (vec![], Some(format!("Pick a subcommand: {}", names.join(", "))));
        };
        let (inner, err) = build_options(&s["options"], rest);
        return (vec![json!({ "type": s["type"], "name": sub, "options": inner })], err);
    }
    let mut out = Vec::new();
    let mut loose: Vec<&str> = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if let Some((k, v)) = w.split_once(':')
            && let Some(o) = list.iter().find(|o| o["name"] == k)
        {
            // a key:value string option keeps going until the next key:
            let mut val = v.to_string();
            if o["type"].as_u64() == Some(3) {
                while i + 1 < words.len() && !words[i + 1].split_once(':').is_some_and(|(k2, _)| list.iter().any(|o| o["name"] == k2)) {
                    i += 1;
                    val.push(' ');
                    val.push_str(words[i]);
                }
            }
            if let Some(v) = convert(o, &val) {
                out.push(json!({ "type": o["type"], "name": k, "value": v }));
                used.push(o["name"].as_str().unwrap_or(""));
            }
        } else {
            loose.push(w);
        }
        i += 1;
    }
    let free: Vec<&&Value> = list.iter().filter(|o| !used.contains(&o["name"].as_str().unwrap_or(""))).collect();
    let mut li = 0;
    for (n, o) in free.iter().enumerate() {
        if li >= loose.len() {
            break;
        }
        let last_string = o["type"].as_u64() == Some(3) && free[n + 1..].iter().all(|x| x["required"].as_bool() != Some(true));
        let val = if last_string { loose[li..].join(" ") } else { loose[li].to_string() };
        li = if last_string { loose.len() } else { li + 1 };
        match convert(o, &val) {
            Some(v) => out.push(json!({ "type": o["type"], "name": o["name"], "value": v })),
            None => return (vec![], Some(format!("“{val}” isn't a valid {}", o["name"].as_str().unwrap_or("value")))),
        }
    }
    let missing: Vec<&str> = list
        .iter()
        .filter(|o| o["required"].as_bool() == Some(true) && !out.iter().any(|x| x["name"] == o["name"]))
        .filter_map(|o| o["name"].as_str())
        .collect();
    if !missing.is_empty() {
        return (vec![], Some(format!("Missing required option: {}", missing.join(", "))));
    }
    (out, None)
}

fn convert(o: &Value, v: &str) -> Option<Value> {
    let digits = |s: &str| -> Option<String> {
        let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        (!d.is_empty()).then_some(d)
    };
    match o["type"].as_u64()? {
        4 => v.parse::<i64>().ok().map(Value::from),
        10 => v.parse::<f64>().ok().map(Value::from),
        5 => match v.to_lowercase().as_str() {
            "true" | "yes" | "y" | "on" | "1" => Some(json!(true)),
            "false" | "no" | "n" | "off" | "0" => Some(json!(false)),
            _ => None,
        },
        6..=9 => digits(v).map(Value::from),
        _ => Some(json!(v)),
    }
}
