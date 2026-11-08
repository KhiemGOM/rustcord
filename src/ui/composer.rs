pub mod slash;
mod suggest;
use suggest::*;
pub(crate) use suggest::Commands;
mod popup;
use popup::*;
mod edit;
use edit::*;
mod send;

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
    pub alt: String,
    // kept for the thumbnail; it gets read for the upload anyway
    thumb: Option<egui::load::Bytes>,
}

impl Upload {
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let mut u = Upload { path, name, spoiler: false, alt: String::new(), thumb: None };
        if u.is_image() && std::fs::metadata(&u.path).is_ok_and(|m| m.len() < 25 << 20) {
            u.thumb = std::fs::read(&u.path).ok().map(Into::into);
        }
        u
    }

    pub fn thumb_bytes(&self) -> Option<egui::load::Bytes> {
        self.thumb.clone()
    }

    fn is_image(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| n.ends_with(e))
    }
}

pub fn show(app: &mut App, ui: &mut Ui, chan: &Channel) {
    if app.store.lurking.contains(&chan.guild) {
        return super::lurk::bar(app, ui, chan);
    }
    let ch = chan.id;
    let side = app.side_thread == Some(ch);
    let tid = egui::Id::new(if side { "composer_side" } else { "composer" });
    let reply = app.reply.filter(|r| app.store.msgs.get(&ch).is_some_and(|e| e.list.iter().any(|m| m.id == *r)));
    // the reply bar is part of the composer, so the typing line goes above it
    let stack_top = ui.cursor().top();
    let gate = super::gate::check(app, chan);
    if let super::gate::Gate::Bar(b) = &gate {
        super::gate::bar(app, ui, b);
        return;
    }
    let off = gate.off();
    super::gate::banner(app, ui, chan);
    super::gate::notice(app, ui, chan);

    if let Some(rid) = reply {
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

    if !app.slash.contains_key(&ch) {
        app.slash_from_draft(chan);
    }
    let slashing = app.slash.contains_key(&ch);
    if slashing {
        slash::bar(app, ui, chan);
    }
    let focused = ui.memory(|m| m.has_focus(tid));
    let nothing_focused = ui.memory(|m| m.focused().is_none());
    let typed = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_))));
    // tab with nothing focused lands in the composer too, as in discord
    let modal = app.modal_open();
    // egui has already moved the focus on by the time this runs, so it's last frame's that counts
    let idle_id = egui::Id::new(("composer_idle", side));
    let was_idle = ui.data(|d| d.get_temp::<bool>(idle_id)).unwrap_or(true);
    ui.data_mut(|d| d.insert_temp(idle_id, nothing_focused));
    let tabbed = was_idle && !modal && app.focus_msg.is_none() && !egui::Popup::is_any_open(ui.ctx()) && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Tab));
    // the push-to-talk key held in a call is for talking, not typing
    let talking = app.settings.ptt && app.voice.chan.is_some() && crate::hotkeys::PTT_HELD.load(std::sync::atomic::Ordering::Relaxed);
    // Tab (not the mouse) into the composer rings it in discord's keyboard blue
    let ring_id = egui::Id::new(("composer_ring", side));
    // asked again next frame too, in case egui's own Tab move won this one
    let retry_id = egui::Id::new(("composer_tab_retry", side));
    let retry = ui.data_mut(|d| d.remove_temp::<bool>(retry_id)).unwrap_or(false);
    if tabbed {
        ui.data_mut(|d| d.insert_temp(retry_id, true));
    }
    if tabbed || (!modal && ui.input(|i| i.key_pressed(Key::Tab))) {
        ui.data_mut(|d| d.insert_temp(ring_id, true));
    } else if ui.input(|i| i.pointer.any_pressed()) {
        ui.data_mut(|d| d.insert_temp(ring_id, false));
    }
    if !side && app.editing.is_none() && !modal && !off && (app.focus_composer || tabbed || retry || (nothing_focused && typed && !talking)) {
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
    let tok = token(&draft_now, cursor).filter(|t| !slashing && (app.ac_dismissed.as_ref() != Some(&t.2) || t.1 == '/'));
    if let Some((_, '/', _)) = &tok
        && !app.settings.legacy_input
    {
        app.ensure_commands(chan);
    }
    if matches!(&tok, Some((_, ':', _))) && app.settings.sticker_suggestions {
        app.load_packs();
    }
    if let Some((_, 'G', q)) = &tok
        && app.settings.game_suggestions
    {
        game_search(app, ui.ctx(), &game_query(q));
    }
    let items = tok.as_ref().map(|(_, k, q)| candidates(app, chan, *k, q)).unwrap_or_default();
    let open = !items.is_empty() && (focused || typed);
    app.ac_open = open;
    if !open {
        app.ac_sel = 0;
    }
    app.ac_sel = app.ac_sel.min(items.len().saturating_sub(1));

    let slow = app.store.slow_left(chan);
    let mut send = false;
    let mut accept = None;
    let selection = egui::TextEdit::load_state(ui.ctx(), tid).and_then(|s| s.cursor.char_range()).filter(|r| !r.is_empty());
    // inside a ``` block left open, Enter is a new line and Tab a tab
    let in_code = draft_now.chars().take(cursor).collect::<String>().matches("```").count() % 2 == 1;
    let limit = if app.store.nitro() { 4000 } else { 2000 };
    if slashing && (focused || (nothing_focused && typed)) && !modal && !off {
        app.slash_keys(ui, chan);
    }
    if (focused || (nothing_focused && typed)) && !modal && !off {
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
    if reply.is_some() && app.editing.is_none() && !popped && ui.input_mut(|i| super::take_key(i, Modifiers::NONE, Key::Escape)) {
        app.reply = None;
    }

    let hint = match chan.kind {
        1 | 3 => format!("Message @{}", channel_title(&app.store, ch)),
        2 | 13 => format!("Message {}", chan.name),
        _ => format!("Message #{}", chan.name),
    };
    let hint = super::gate::placeholder(chan, &gate).map_or(hint, str::to_string);
    let top_r = if reply.is_some() || slashing { 0 } else { 8 };
    let mut attach = false;
    let mut tab = None;
    let mut apps_click = false;
    let mut gift_click = false;
    let mut send_click = false;
    let mut gif_rect = None;
    let mut remove = None;
    let mut plus = None;
    let mut modify = None;
    let mut card_focus = None;
    let mut new_cursor = None;
    let mut colon_at = None;
    // where a selection starts on screen, for the formatting toolbar over it
    let mut sel_at = None;
    let mut spell_changed = false;
    let colon = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == ":")));
    let before = draft_now.len();
    let radius = CornerRadius { nw: top_r, ne: top_r, sw: 8, se: 8 };
    // below 450 discord folds the gif, sticker and emoji buttons into one
    let wide = ui.available_width() > 450.0;
    let r = Frame::new()
        .fill(pal().bg_input)
        .inner_margin(1)
        .corner_radius(radius)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let inside = ui.max_rect();
            if !app.files.is_empty() {
                Frame::new().inner_margin(Margin { left: 10, right: 10, top: 20, bottom: 10 }).show(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt(("uploads", side)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 24.0;
                            for (i, f) in app.files.iter_mut().enumerate() {
                                let (act, focused) = upload_card(ui, f, i);
                                match act {
                                    Some(CardAct::Remove) => remove = Some(i),
                                    Some(CardAct::Spoiler) => f.spoiler = !f.spoiler,
                                    Some(CardAct::Modify) => modify = Some(i),
                                    None => {}
                                }
                                if focused {
                                    card_focus = Some(i);
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
                    let slashing = app.slash.contains_key(&ch);
                    let row_left = ui.cursor().left();
                    // the text stops short of the buttons, 40 wide each, and gives way to them first
                    let slots = if off { 0 } else { (if wide { 5 } else { 3 }) + usize::from(app.settings.send_button) };
                    let buttons = slots as f32 * 40.0;
                    let w = (ui.available_width() - buttons - 8.0).max(8.0);
                    let mut text_pos = egui::Pos2::ZERO;
                    if slashing {
                        slash::row(app, ui, chan, tid, w, off);
                    } else {
                        let (pr, presp) = ui.allocate_exact_size(vec2(24.0, 56.0), Sense::click());
                        let col = crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &presp));
                        icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pr.center(), vec2(20.0, 20.0)), if off { col.gamma_multiply(0.4) } else { col });
                        if presp.hovered() && !off {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        app.rects.insert("plus_button".into(), pr);
                        if !off {
                            plus = super::plus::menu(app, &presp, chan);
                        }
                        // discord's text starts 64 in: 16, the 24 button, then 24 more
                        ui.add_space(23.0);
                        let preview = app.settings.md_preview && !app.settings.legacy_input;
                        let draft = app.drafts.entry(ch).or_default();
                        egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.5).id_salt(("composer_scroll", side)).show(ui, |ui| {
                            // the row is horizontal; padding above and below the text has to be vertical
                            ui.vertical(|ui| {
                                ui.add_space(17.0);
                                let mut styled = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| super::mdpreview::layout(ui, text.as_str(), wrap);
                                let mut edit = egui::TextEdit::multiline(draft);
                                if preview {
                                    edit = edit.layouter(&mut styled);
                                }
                                let out = edit
                                    .id(tid)
                                    .frame(Frame::NONE)
                                    .desired_rows(1)
                                    .margin(Margin::ZERO)
                                    .font(reg(16.0))
                                    .interactive(!off)
                                    .text_color(pal().text_normal)
                                    .desired_width(w)
                                    .lock_focus(true)
                                    .show(ui);
                                text_pos = out.galley_pos;
                                spell_changed = super::spelling::composer(&mut app.spell, &mut app.settings, ui, &out, draft, tid);
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
                    }
                    if spell_changed {
        app.save_settings(ui.ctx());
    }
    app.rects.insert("composer_text".into(), Rect::from_min_size(text_pos, vec2(1.0, 1.0)));
                    // discord's row: gift, gif, sticker, emoji, apps; 20px icons 40 apart. It sits flush right
                    // when it fits and runs off the box's edge, clipped, when it doesn't
                    let x0 = (ui.max_rect().right() - buttons).max(row_left + if slashing { 0.0 } else { 24.0 + 23.0 } + 8.0);
                    let strip = Rect::from_min_size(pos2(x0, ui.min_rect().top()), vec2(buttons, 56.0));
                    app.rects.insert("composer:buttons".into(), strip);
                    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(strip).layout(Layout::left_to_right(Align::Min)));
                    row.set_clip_rect(ui.clip_rect().intersect(inside));
                    {
                        let ui = &mut row;
                        if off {
                            return;
                        }
                        let slot = |ui: &mut Ui, icon: Icon, tip: &str| {
                            let (r, resp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
                            let col = crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp));
                            icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), col);
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            resp.on_hover_text(tip)
                        };
                        gift_click = slot(ui, Icon::Gift, "Send a gift").clicked();
                        if wide {
                            let gr = slot(ui, Icon::Gif, "Open GIF picker");
                            gif_rect = Some(gr.rect);
                            if gr.clicked() {
                                tab = Some(super::picker::Tab::Gifs);
                            }
                            let sr = slot(ui, Icon::Sticker, "Open sticker picker");
                            app.rects.insert("sticker_button".into(), sr.rect);
                            if sr.clicked() {
                                tab = Some(super::picker::Tab::Stickers);
                            }
                            let er = slot(ui, Icon::Smile, "Select emoji");
                            app.rects.insert("emoji_button".into(), er.rect);
                            if er.clicked() {
                                tab = Some(super::picker::Tab::Emoji);
                            }
                        } else {
                            let xr = slot(ui, Icon::Smile, "Open expression picker");
                            app.rects.insert("expression_button".into(), xr.rect);
                            gif_rect = Some(xr.rect);
                            if xr.clicked() {
                                tab = Some(super::picker::Tab::Emoji);
                            }
                        }
                        let ar = slot(ui, Icon::Apps, "Apps");
                        app.rects.insert("apps_button".into(), ar.rect);
                        apps_click = ar.clicked();
                        if app.settings.send_button {
                            let (sr, sresp) = ui.allocate_exact_size(vec2(40.0, 56.0), Sense::click());
                            let col = crate::ui::motion::mix(pal().header_secondary, pal().blurple, crate::ui::motion::hot(ui.ctx(), &sresp));
                            icons::draw(ui.painter(), Icon::Send, Rect::from_center_size(sr.center(), vec2(20.0, 20.0)), col);
                            if sresp.on_hover_text("Send Message").clicked() {
                                send_click = true;
                            }
                        }
                    }
                });
            });
        });
    let crect = r.response.rect;
    // discord's border sits on the box's own fill, not beside it
    ui.painter().rect_stroke(crect, radius, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    if ui.memory(|m| m.has_focus(tid)) && ui.data(|d| d.get_temp::<bool>(ring_id)).unwrap_or(false) {
        ui.painter().rect_stroke(crect.expand(2.0), radius, Stroke::new(2.0, Color32::from_rgb(0x00, 0xa8, 0xfc)), egui::StrokeKind::Inside);
        app.rects.insert("composer_ring".into(), crect);
    }
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
        let col = if left < 0 { pal().red } else { pal().text_muted };
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
        ui.interact(r, egui::Id::new(("char_count", side)), Sense::hover()).on_hover_text(tip);
    }

    if app.slash.contains_key(&ch) {
        slash::list(app, ui.ctx(), chan, crect);
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
    match plus {
        Some(super::plus::Plus::Upload) => attach = true,
        Some(super::plus::Plus::Clip) => {
            app.open_clips();
            if let Some(g) = &mut app.clips.gallery {
                g.pick = Some(ch);
            }
        }
        Some(super::plus::Plus::Thread) => app.new_thread = Some(super::popups::NewThread::blank(ch)),
        Some(super::plus::Plus::Poll) => app.poll = Some(super::poll::Draft::new(ch)),
        Some(super::plus::Plus::Apps) => app.launcher = Some(super::plus::Launcher::new()),
        None => {}
    }
    if let Some(i) = modify
        && let Some(f) = app.files.get(i)
    {
        app.modify = Some(super::attach::Modify::new(i, f));
    }
    if let Some(i) = card_focus {
        super::attach::card_keys(app, ui.ctx(), i);
    }
    if attach && let Some(files) = rfd::FileDialog::new().pick_files() {
        app.attach_files(files);
    }

    if gift_click {
        app.open_buy(super::buy::What::NitroGift);
    }
    if apps_click {
        app.launcher = if app.launcher.is_some() { None } else { Some(super::plus::Launcher::new()) };
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
    let (send, send_click) = (send && !off && slow == 0, send_click && !off && slow == 0);
    if (send || send_click) && used > limit {
        app.confirm = Some(
            super::Confirm::new("Your message is too long...", "Your message is too long.", "Upload as File", move |app: &mut App| {
                let text = app.drafts.remove(&ch).unwrap_or_default();
                app.attach_text(&text);
                app.send_message(ch);
            })
            .blurple(),
        );
    } else if (send || send_click) && slashing {
        app.slash_submit(chan);
    } else if (send || send_click) && !app.quick_react(chan) && !app.substitute(chan) && !app.built_in(chan) {
        app.send_message(ch);
    }
    super::gate::cooldown(app, ui, chan, crect);
    typing_line(app, ui, chan, Rect::from_min_max(pos2(crect.left(), stack_top), crect.max));
}

enum CardAct {
    Remove,
    Spoiler,
    Modify,
}

fn upload_card(ui: &mut Ui, f: &Upload, i: usize) -> (Option<CardAct>, bool) {
    let (r, card) = ui.allocate_exact_size(vec2(218.0, 218.0), Sense::click());
    if card.clicked() {
        card.request_focus();
    }
    super::settings::mark(ui, &format!("card:{i}"), r);
    let p = ui.painter();
    p.rect(r, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    if card.has_focus() {
        p.rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, pal().blurple), egui::StrokeKind::Inside);
    }
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
        icons::draw(p, Icon::File, Rect::from_center_size(thumb.center(), vec2(72.0, 72.0)), pal().blurple);
        if f.spoiler {
            p.text(thumb.center() + vec2(0.0, 52.0), Align2::CENTER_CENTER, "SPOILER", semi(12.0), pal().header_secondary);
        }
    }
    text::paint(p, pos2(r.left() + 8.0, r.bottom() - 24.0), Align2::LEFT_CENTER, &f.name, reg(14.0), pal().text_normal, 200.0);

    let bar = Rect::from_min_size(pos2(r.right() - 81.0, r.top() + 1.0), vec2(96.0, 32.0));
    ui.painter().add(egui::Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha(36) }.as_shape(bar, CornerRadius::same(4)));
    ui.painter().rect(bar, CornerRadius::same(4), pal().bg_floating, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
    let mut act = None;
    let buttons = [
        (if f.spoiler { Icon::EyeOff } else { Icon::Eye }, "Spoiler Attachment", CardAct::Spoiler, pal().header_secondary),
        (Icon::Pencil, "Modify Attachment", CardAct::Modify, pal().header_secondary),
        (Icon::Trash, "Remove Attachment", CardAct::Remove, pal().red),
    ];
    for (k, (icon, tip, what, ink)) in buttons.into_iter().enumerate() {
        let cell = Rect::from_min_size(bar.min + vec2(32.0 * k as f32, 0.0), vec2(32.0, 32.0));
        let resp = ui.interact(cell, ui.id().with(("upload_btn", i, k)), Sense::click());
        let ink = if ink == pal().red { ink } else { crate::ui::motion::mix(ink, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
        icons::draw(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(20.0, 20.0)), ink);
        super::settings::mark(ui, &format!("card:{i}:{k}"), cell);
        if resp.hovered() {
            super::settings::said(ui.ctx(), "card_tip", tip);
        }
        if resp.on_hover_text(tip).clicked() {
            act = Some(what);
        }
    }
    (act, card.has_focus())
}
