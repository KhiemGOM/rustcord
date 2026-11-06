//! Who reacted: the chip tooltip, the View Reactions window, and taking reactions away.

use egui::{Align, Align2, CornerRadius, Frame, Image, Layout, Margin, Rect, RichText, Sense, pos2, vec2};
use super::icons::{self, Icon};
use super::{App, Confirm, avatar, icon_button};
use crate::gif;
use crate::net::Tag;
use crate::state::{Id, Message, ReactEmoji, Reaction, Store};
use crate::theme::*;

pub struct Reactors {
    ch: Id,
    msg: Id,
    sel: usize,
}

impl Reactors {
    pub fn new(ch: Id, msg: Id) -> Self {
        Reactors { ch, msg, sel: 0 }
    }
}

/// discord's `:name:` for an emoji
pub fn colon_name(e: &ReactEmoji) -> String {
    if e.id != 0 {
        format!(":{}:", e.name)
    } else {
        format!(":{}:", emojis::get(&e.name).and_then(|x| x.shortcode()).unwrap_or(&e.name))
    }
}

/// "reacted by A, B and 4 others", from the people loaded so far
pub fn tooltip(store: &Store, guild: Id, m: &Message, r: &Reaction, burst: bool) -> String {
    let name = colon_name(&r.emoji);
    let verb = if burst { "Super reacted" } else { "reacted" };
    let Some(users) = store.reactors.get(&(m.id, r.emoji.api_name(), burst)).and_then(|e| e.1.as_ref()) else { return name };
    let names: Vec<String> = users.iter().filter(|u| store.relationships.get(*u) != Some(&crate::state::BLOCKED)).take(3).map(|u| store.name_in(guild, *u)).collect();
    let total = if burst { r.burst } else { r.count };
    let more = (total as usize).saturating_sub(names.len());
    let others = |n: usize| if n == 1 { "1 other".to_string() } else { format!("{n} others") };
    let who = match (names.as_slice(), more) {
        ([], _) => return name,
        ([a], 0) => a.clone(),
        ([a], n) => format!("{a} and {}", others(n)),
        ([a, b], 0) => format!("{a} and {b}"),
        ([a, b], n) => format!("{a}, {b} and {}", others(n)),
        ([a, b, c], 0) => format!("{a}, {b}, and {c}"),
        ([a, b, c], n) => format!("{a}, {b}, {c} and {}", others(n)),
        _ => return name,
    };
    format!("{name} {verb} by {who}")
}

impl App {
    /// Fetches the people behind a reaction, again whenever its count has moved since.
    pub fn ask_reactors(&mut self, ch: Id, msg: Id, em: &ReactEmoji, burst: bool, count: u32) {
        let key = (msg, em.api_name(), burst);
        if self.store.reactors.get(&key).is_some_and(|e| e.0 == count) {
            return;
        }
        self.store.reactors.insert(key, (count, None));
        let path = format!("/channels/{ch}/messages/{msg}/reactions/{}?limit=100&type={}", crate::api::esc(&em.api_name()), burst as u8);
        self.rest("GET", path, None, Tag::Reactors(ch, msg, em.api_name(), burst));
    }

    /// Remove Reactions for one emoji, asking first unless shift is held.
    pub fn clear_emoji(&mut self, ch: Id, msg: Id, em: ReactEmoji, ask: bool) {
        if !ask {
            let kept = self.store.find_msg(ch, msg).map(|m| m.reactions.clone()).unwrap_or_default();
            self.rest("DELETE", format!("/channels/{ch}/messages/{msg}/reactions/{}", crate::api::esc(&em.api_name())), None, Tag::undo("Failed to remove reactions", move |app: &mut App| {
                if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg)) {
                    m.reactions = kept.clone();
                }
            }));
            if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg)) {
                m.reactions.retain(|r| r.emoji != em);
            }
            return;
        }
        let body = "Are you sure you want to remove all of the reactions for this emoji from this message?";
        self.confirm = Some(Confirm::new("Remove Reactions for Emoji", body, "Yes", move |app: &mut App| app.clear_emoji(ch, msg, em, false)));
    }

    fn remove_reactor(&mut self, ch: Id, msg: Id, em: &ReactEmoji, burst: bool, user: Id) {
        let path = format!("/channels/{ch}/messages/{msg}/reactions/{}/{user}?type={}", crate::api::esc(&em.api_name()), burst as u8);
        self.rest("DELETE", path, None, Tag::reason());
        if let Some(e) = self.store.reactors.get_mut(&(msg, em.api_name(), burst)) {
            e.0 = e.0.saturating_sub(1);
            if let Some(l) = &mut e.1 {
                l.retain(|u| *u != user);
            }
        }
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == msg)) {
            if let Some(r) = m.reactions.iter_mut().find(|r| r.emoji == *em) {
                if burst {
                    r.burst = r.burst.saturating_sub(1);
                }
                r.count = r.count.saturating_sub(1);
            }
            m.reactions.retain(|r| r.count > 0);
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(w) = &app.reactors else { return };
    let (ch, msg, mut sel) = (w.ch, w.msg, w.sel);
    let Some(m) = app.store.find_msg(ch, msg) else {
        app.reactors = None;
        return;
    };
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let manage = app.store.channels.get(&ch).is_some_and(|c| guild != 0 && app.store.perms(c) & crate::state::MANAGE_MESSAGES != 0);
    let tabs: Vec<(Reaction, bool)> = m.reactions.iter().flat_map(|r| [(r.clone(), false), (r.clone(), true)]).filter(|(r, b)| if *b { r.burst > 0 } else { r.count > r.burst || r.burst == 0 }).collect();
    if tabs.is_empty() {
        app.reactors = None;
        return;
    }
    sel = sel.min(tabs.len() - 1);
    let (cur, burst) = tabs[sel].clone();
    let total = if burst { cur.burst } else { cur.count.saturating_sub(cur.burst) };
    app.ask_reactors(ch, msg, &cur.emoji, burst, total.max(1));
    let users = app.store.reactors.get(&(msg, cur.emoji.api_name(), burst)).and_then(|e| e.1.clone());
    let (mut close, mut pick, mut profile, mut remove) = (false, None, None, None);
    let store = &app.store;
    let modal = crate::ui::motion::Modal::new("reactors".into())
        .frame(Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            Frame::new().inner_margin(Margin { left: 24, right: 16, top: 16, bottom: 12 }).show(ui, |ui| {
                ui.set_width(400.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Reactions").font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
            let (body, _) = ui.allocate_exact_size(vec2(440.0, 360.0), Sense::hover());
            ui.painter().hline(body.x_range(), body.top(), egui::Stroke::new(1.0, pal().border_subtle));
            let side = Rect::from_min_size(pos2(body.left() + 24.0, body.top() + 16.0), vec2(98.0, body.height() - 16.0));
            ui.painter().vline(side.right() + 8.0, egui::Rangef::new(body.top() + 16.0, body.bottom() - 16.0), egui::Stroke::new(1.0, pal().border_subtle));
            let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(side).layout(Layout::top_down(Align::Min)));
            egui::ScrollArea::vertical().id_salt("reactor_tabs").show(&mut tab_ui, |ui| {
                ui.set_width(90.0);
                for (i, (r, b)) in tabs.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(90.0, 40.0), Sense::click());
                    let fill = if i == sel { Some(pal().control_hover) } else if resp.hovered() { Some(MENU_HOVER) } else { None };
                    if let Some(f) = fill {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), f);
                    }
                    let er = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 12.0), vec2(24.0, 24.0));
                    Image::from_uri(gif::uri(ui.ctx(), &super::chat::emoji_url(&r.emoji), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                    let n = if *b { r.burst } else { r.count.saturating_sub(r.burst) };
                    ui.painter().text(pos2(er.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, n.to_string(), semi(14.0), pal().header_primary);
                    if *b {
                        icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), vec2(12.0, 12.0)), pal().header_secondary);
                    }
                    if resp.clicked() {
                        pick = Some(i);
                    }
                    ui.add_space(8.0);
                }
            });
            let list = Rect::from_min_max(pos2(side.right() + 16.0, body.top() + 12.0), pos2(body.right() - 16.0, body.bottom()));
            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list).layout(Layout::top_down(Align::Min)));
            egui::ScrollArea::vertical().id_salt("reactor_list").auto_shrink([false, false]).show(&mut list_ui, |ui| {
                ui.set_width(list.width());
                let Some(users) = &users else {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                    return;
                };
                for u in users {
                    let Some(user) = store.user(*u) else { continue };
                    let (r, resp) = ui.allocate_exact_size(vec2(list.width(), 52.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), MENU_HOVER, 0.05);
                    avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                    let display = store.name_in(guild, *u);
                    let sub = user.username.clone();
                    let two = display != sub;
                    let y = if two { r.center().y - 8.0 } else { r.center().y };
                    super::text::paint(ui.painter(), pos2(r.left() + 46.0, y), Align2::LEFT_CENTER, &display, medium(16.0), pal().header_primary, list.width() - 100.0);
                    if two {
                        super::text::paint(ui.painter(), pos2(r.left() + 46.0, r.center().y + 10.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().text_muted, list.width() - 100.0);
                    }
                    if resp.clicked() {
                        profile = Some((*u, r.right_top()));
                    }
                    if manage && resp.hovered() {
                        let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
                        let xr = ui.interact(x, ui.id().with(("remove_reactor", *u)), Sense::click());
                        icons::draw(ui.painter(), Icon::Close, x, crate::ui::motion::mix(pal().header_secondary, pal().critical, crate::ui::motion::hot(ui.ctx(), &xr)));
                        if xr.on_hover_text("Remove Reaction").clicked() {
                            remove = Some(*u);
                        }
                    }
                }
            });
        });
    if modal.should_close() {
        close = true;
    }
    if let Some(i) = pick {
        sel = i;
    }
    if let Some(u) = remove {
        app.remove_reactor(ch, msg, &cur.emoji, burst, u);
    }
    if let Some((u, at)) = profile {
        app.reactors = None;
        app.open_profile(u, guild, at);
        return;
    }
    match &mut app.reactors {
        Some(_) if close => app.reactors = None,
        Some(w) => w.sel = sel,
        None => {}
    }
}
