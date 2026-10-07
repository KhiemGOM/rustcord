use egui::text_selection::CCursorRange;
use egui::text::CCursor;
use egui::{Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Modifiers, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Instant;

use super::icons::{self, Icon};
use super::{App, Pick, avatar, channel_title, icon_button, text};
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
    Emoji(String, String, Option<String>),
    Command(usize),
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
    let mut out: Vec<(u8, Item)> = Vec::new();
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
                    out.push((s, Item::User(u, name, user.username.clone())));
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
                            out.push((s + 2, Item::Role(r.id, r.name.clone(), r.color)));
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
                        out.push((s, Item::Channel(c.id, c.name.clone())));
                    }
                }
            }
        }
        ':' => {
            if let Some(g) = store.guilds.get(&chan.guild) {
                for e in &g.emojis {
                    if let Some(s) = score(&e.name, &q) {
                        let raw = format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id);
                        out.push((s, Item::Emoji(e.name.clone(), raw, Some(e.url()))));
                    }
                }
            }
            for (glyph, name) in super::picker::common() {
                if let Some(s) = score(name, &q) {
                    out.push((s, Item::Emoji(name.to_string(), glyph.to_string(), None)));
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
                        out.push((s, Item::Command(i)));
                    }
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

    if let Some(rid) = app.reply {
        let name = app.store.find_msg(ch, rid).map(|m| app.store.name_in(chan.guild, m.author)).unwrap_or_default();
        Frame::new()
            .fill(BG_SECONDARY)
            .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
            .inner_margin(Margin::symmetric(16, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Replying to").font(reg(14.0)).color(HEADER_SECONDARY));
                    text::label(ui, &name, semi(14.0), HEADER_SECONDARY, Sense::hover());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 20.0, "Cancel reply").clicked() {
                            app.reply = None;
                        }
                        ui.add_space(8.0);
                        // the "@ ON" toggle: whether the reply pings its author
                        let (label, col) = if app.reply_ping { ("@ ON", LINK) } else { ("@ OFF", HEADER_SECONDARY) };
                        let r = ui.add(egui::Label::new(RichText::new(label).font(bold(12.0)).color(col)).sense(Sense::click()));
                        app.rects.insert("ping_toggle".into(), r.rect);
                        if r.on_hover_text("Click to toggle mentioning the author").clicked() {
                            app.reply_ping = !app.reply_ping;
                        }
                    });
                });
            });
    }

    let focused = ui.memory(|m| m.has_focus(tid));
    let nothing_focused = ui.memory(|m| m.focused().is_none());
    let typed = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_))));
    if app.editing.is_none() && app.switcher.is_none() && (app.focus_composer || (nothing_focused && typed)) {
        ui.memory_mut(|m| m.request_focus(tid));
        app.focus_composer = false;
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
    if focused || (nothing_focused && typed) {
        if open {
            if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::ArrowDown)) {
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
        if accept.is_none() && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Enter)) {
            send = true;
        }
        if draft_now.is_empty() && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::ArrowUp)) {
            let me = app.store.me;
            if let Some(m) = app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.author == me && !m.pending)) {
                app.editing = Some((m.id, m.content.clone()));
            }
        }
        if ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape)) {
            if app.reply.is_some() {
                app.reply = None;
            } else if let Some(c) = app.cur() {
                // escape marks the channel read, same as the official client
                app.mark_read(&[c]);
                app.marker = None;
            }
        }
    }

    let hint = match chan.kind {
        1 | 3 => format!("Message @{}", channel_title(&app.store, ch)),
        _ => format!("Message #{}", chan.name),
    };
    let top_r = if app.reply.is_some() { 0 } else { 8 };
    let mut attach = false;
    let mut emoji_at = None;
    let mut gif_at = None;
    let mut gif_rect = None;
    let mut remove = None;
    let mut new_cursor = None;
    let before = draft_now.len();
    let r = Frame::new()
        .fill(BG_INPUT)
        .corner_radius(CornerRadius { nw: top_r, ne: top_r, sw: 8, se: 8 })
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
                ui.painter().hline(sep.x_range(), sep.center().y, Stroke::new(1.0, BG_ACCENT.gamma_multiply(0.5)));
            }
            Frame::new().inner_margin(Margin { left: 16, right: 10, top: 0, bottom: 0 }).show(ui, |ui| {
                ui.set_min_height(44.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (pr, presp) = ui.allocate_exact_size(vec2(24.0, 44.0), Sense::click());
                    let col = if presp.hovered() { HEADER_PRIMARY } else { HEADER_SECONDARY };
                    ui.painter().circle_filled(pr.center(), 12.0, col);
                    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pr.center(), vec2(16.0, 16.0)), BG_INPUT);
                    if presp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if presp.on_hover_text("Upload a File").clicked() {
                        attach = true;
                    }
                    ui.add_space(16.0);
                    let draft = app.drafts.entry(ch).or_default();
                    let w = ui.available_width() - 44.0;
                    let mut text_pos = egui::Pos2::ZERO;
                    egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.5).id_salt("composer_scroll").show(ui, |ui| {
                        // the row is horizontal; padding above and below the text has to be vertical
                        ui.vertical(|ui| {
                            ui.add_space(11.0);
                            text_pos = egui::TextEdit::multiline(draft)
                                .id(tid)
                                .frame(Frame::NONE)
                                .desired_rows(1)
                                .margin(Margin::ZERO)
                                .font(reg(16.0))
                                .text_color(TEXT_NORMAL)
                                .hint_text(RichText::new(&hint).color(CHANNEL_DEFAULT))
                                .desired_width(w)
                                .lock_focus(true)
                                .show(ui)
                                .galley_pos;
                            ui.add_space(11.0);
                        });
                    });
                    app.rects.insert("composer_text".into(), Rect::from_min_size(text_pos, vec2(1.0, 1.0)));
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        let (er, eresp) = ui.allocate_exact_size(vec2(36.0, 44.0), Sense::click());
                        let col = if eresp.hovered() { INTERACTIVE_HOVER } else { HEADER_SECONDARY };
                        icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(er.center(), vec2(24.0, 24.0)), col);
                        if eresp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if eresp.on_hover_text("Select emoji").clicked() {
                            emoji_at = Some(er.left_top());
                        }
                        let (gr, gresp) = ui.allocate_exact_size(vec2(40.0, 44.0), Sense::click());
                        let col = if gresp.hovered() { INTERACTIVE_HOVER } else { HEADER_SECONDARY };
                        let badge = Rect::from_center_size(gr.center(), vec2(28.0, 20.0));
                        ui.painter().rect_stroke(badge, CornerRadius::same(4), Stroke::new(2.0, col), egui::StrokeKind::Inside);
                        ui.painter().text(badge.center(), Align2::CENTER_CENTER, "GIF", bold(11.0), col);
                        gif_rect = Some(gr);
                        if gresp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if gresp.on_hover_text("Open GIF picker").clicked() {
                            gif_at = Some(gr.right_top());
                        }
                    });
                });
            });
        });
    let crect = r.response.rect;
    app.rects.insert("composer".into(), crect);

    if open {
        let picked = popup(app, ui.ctx(), chan, &items, crect, tok.as_ref().map(|t| t.1).unwrap_or('@'));
        if picked.is_some() {
            accept = picked;
        }
    }
    if let (Some(i), Some((start, _, _))) = (accept, &tok) {
        new_cursor = complete(app, chan, &items[i], *start, cursor);
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
    if let Some(at) = emoji_at {
        app.picker = Some((Pick::Composer, at - vec2(380.0, 440.0)));
    }
    if let Some(r) = gif_rect {
        app.rects.insert("gif_button".into(), r);
    }
    if let Some(at) = gif_at {
        match app.gifs {
            Some(_) => app.gifs = None,
            None => app.open_gifs(at + vec2(36.0, -8.0)),
        }
    }
    let changed = app.drafts.get(&ch).is_some_and(|d| d.len() != before && !d.is_empty() && !d.starts_with('/'));
    if changed && app.last_typing.is_none_or(|(c, t)| c != ch || t.elapsed().as_secs() >= 8) {
        app.last_typing = Some((ch, Instant::now()));
        app.net.send(Cmd::Typing { channel: ch });
    }
    if app.drafts.get(&ch).is_some_and(|d| d.is_empty()) {
        app.ac_dismissed = None;
    }
    if send {
        if !app.try_command(chan) {
            app.send_message(ch);
        }
    }
    typing_line(app, ui, chan);
}

enum CardAct {
    Remove,
    Spoiler,
}

fn upload_card(ui: &mut Ui, f: &Upload) -> Option<CardAct> {
    let (r, _) = ui.allocate_exact_size(vec2(216.0, 216.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(4), BG_SECONDARY);
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
            p.text(thumb.center(), Align2::CENTER_CENTER, "SPOILER", bold(14.0), Color32::WHITE);
        }
    } else {
        icons::draw(p, Icon::File, Rect::from_center_size(thumb.center(), vec2(72.0, 72.0)), BLURPLE);
    }
    text::paint(p, pos2(r.left() + 8.0, r.bottom() - 24.0), Align2::LEFT_CENTER, &f.name, reg(14.0), TEXT_NORMAL, 200.0);

    // the little action strip that floats over the top-right corner
    let bar = Rect::from_min_size(pos2(r.right() - 76.0, r.top() - 12.0), vec2(80.0, 32.0));
    p.rect(bar, CornerRadius::same(4), BG_PRIMARY, Stroke::new(1.0, BG_TERTIARY), egui::StrokeKind::Inside);
    let mut act = None;
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink(2.0)).layout(Layout::left_to_right(Align::Center)));
    b.spacing_mut().item_spacing.x = 0.0;
    if f.is_image() && icon_button(&mut b, Icon::Image, 28.0, "Spoiler Attachment").clicked() {
        act = Some(CardAct::Spoiler);
    }
    let del = b.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let col = if del.1.hovered() { RED } else { HEADER_SECONDARY };
    icons::draw(b.painter(), Icon::Trash, Rect::from_center_size(del.0.center(), vec2(20.0, 20.0)), col);
    if del.1.on_hover_text("Remove Attachment").clicked() {
        act = Some(CardAct::Remove);
    }
    act
}

fn popup(app: &mut App, ctx: &egui::Context, chan: &Channel, items: &[Item], composer: Rect, kind: char) -> Option<usize> {
    let mut picked = None;
    let title = match kind {
        '@' => "MEMBERS".to_string(),
        '#' => "TEXT CHANNELS".to_string(),
        ':' => "EMOJI MATCHING".to_string(),
        _ => "COMMANDS".to_string(),
    };
    let row_h = if kind == '/' { 52.0 } else { 34.0 };
    egui::Area::new(egui::Id::new("autocomplete"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(composer.left_top())
        .show(ctx, |ui| {
            Frame::new().fill(BG_SECONDARY).corner_radius(8).inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                ui.set_width(composer.width() - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                ui.painter().text(pos2(hr.left() + 8.0, hr.center().y), Align2::LEFT_CENTER, title, bold(12.0), HEADER_SECONDARY);
                egui::ScrollArea::vertical().max_height(row_h * 10.0).show(ui, |ui| {
                    for (i, it) in items.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                            app.ac_sel = i;
                        }
                        if i == app.ac_sel {
                            ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_SELECTED);
                        }
                        row(app, ui, chan, it, r);
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
            let nr = text::paint(p, pos2(x + 32.0, y), Align2::LEFT_CENTER, name, semi(15.0), HEADER_PRIMARY, r.width() * 0.5);
            p.text(pos2(nr.right() + 8.0, y), Align2::LEFT_CENTER, username, reg(14.0), HEADER_SECONDARY);
        }
        Item::Role(_, name, color) => {
            let col = if *color == 0 { HEADER_PRIMARY } else { role_color(*color) };
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{name}"), semi(15.0), col);
        }
        Item::Everyone(e) => {
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{e}"), semi(15.0), HEADER_PRIMARY);
            let note = if *e == "here" { "Notify everyone online in this channel." } else { "Notify everyone who has permission to view this channel." };
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, note, reg(13.0), HEADER_SECONDARY);
        }
        Item::Channel(id, name) => {
            let icon = app.store.channels.get(id).map(super::side::glyph).unwrap_or(Icon::Hash);
            icons::draw(p, icon, Rect::from_center_size(pos2(x + 10.0, y), vec2(18.0, 18.0)), CHANNEL_DEFAULT);
            text::paint(p, pos2(x + 28.0, y), Align2::LEFT_CENTER, name, semi(15.0), HEADER_PRIMARY, r.width() - 160.0);
            let cat = app
                .store
                .channels
                .get(id)
                .and_then(|c| app.store.channels.get(&c.parent))
                .map(|c| c.name.to_uppercase())
                .unwrap_or_default();
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, cat, semi(12.0), HEADER_SECONDARY);
        }
        Item::Emoji(name, raw, url) => {
            let er = Rect::from_center_size(pos2(x + 12.0, y), vec2(22.0, 22.0));
            match url {
                Some(u) => text::image(p, u, er),
                None => text::image(p, &crate::markdown::twemoji(raw), er),
            }
            p.text(pos2(x + 32.0, y), Align2::LEFT_CENTER, format!(":{name}:"), semi(15.0), HEADER_PRIMARY);
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
            let nr = p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), HEADER_PRIMARY);
            p.text(pos2(nr.right() + 6.0, y - 9.0), Align2::LEFT_CENTER, args, reg(14.0), HEADER_SECONDARY);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, c["description"].as_str().unwrap_or(""), reg(13.0), HEADER_SECONDARY, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, app_name, semi(13.0), HEADER_SECONDARY);
        }
    }
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
        Item::Emoji(name, raw, Some(_)) => (format!(":{name}:"), raw.clone()),
        Item::Emoji(_, glyph, None) => (glyph.clone(), glyph.clone()),
        Item::Command(i) => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            let name = app.commands.get(&scope).and_then(|c| c.list.get(*i)).and_then(|c| c["name"].as_str()).unwrap_or("").to_string();
            (format!("/{name}"), String::new())
        }
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

fn typing_line(app: &App, ui: &mut Ui, chan: &Channel) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let users: Vec<Id> = app.store.typing.get(&chan.id).map(|v| v.iter().map(|(u, _)| *u).collect()).unwrap_or_default();
    if users.is_empty() {
        return;
    }
    let t = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((t * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        ui.painter().circle_filled(
            pos2(r.left() + 6.0 + i as f32 * 7.0, r.center().y),
            2.5,
            HEADER_PRIMARY.gamma_multiply(0.4 + 0.6 * phase),
        );
    }
    let names: Vec<String> = users.iter().map(|u| app.store.name_in(chan.guild, *u)).collect();
    let text = match names.len() {
        1 => format!("{} is typing…", names[0]),
        2 => format!("{} and {} are typing…", names[0], names[1]),
        3 => format!("{}, {}, and {} are typing…", names[0], names[1], names[2]),
        _ => "Several people are typing…".to_string(),
    };
    text::paint(ui.painter(), pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, &text, reg(13.0), TEXT_NORMAL, r.width() - 40.0);
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

    pub fn resolve_aliases(&self, ch: Id, text: &str) -> String {
        let mut out = text.to_string();
        if let Some(list) = self.aliases.get(&ch) {
            // longest first so "@ann" never eats half of "@anna"
            let mut list = list.clone();
            list.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
            for (shown, raw) in list {
                out = out.replace(&shown, &raw);
            }
        }
        out
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
