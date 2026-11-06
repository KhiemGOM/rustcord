//! A poll's View Votes window: its answers down the left, who picked the one selected on the right.

use std::collections::HashMap;

use egui::{Align, Align2, CornerRadius, Frame, Image, Layout, Margin, Rect, RichText, Sense, pos2, vec2};

use super::icons::Icon;
use super::{App, avatar, icon_button};
use crate::gif;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

pub struct Votes {
    ch: Id,
    msg: Id,
    sel: usize,
    // answer id -> the count it was fetched at, and who picked it once that's back
    data: HashMap<u8, (u32, Option<Vec<Id>>)>,
}

impl Votes {
    pub fn new(ch: Id, msg: Id) -> Self {
        Votes { ch, msg, sel: 0, data: HashMap::new() }
    }

    pub fn msg(&self) -> Id {
        self.msg
    }

    pub fn got(&mut self, ans: u8, users: Vec<Id>) {
        if let Some(e) = self.data.get_mut(&ans) {
            e.1 = Some(users);
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(w) = &app.votes else { return };
    let (ch, msg, mut sel) = (w.ch, w.msg, w.sel);
    let Some(p) = app.store.find_msg(ch, msg).and_then(|m| m.poll.clone()) else {
        app.votes = None;
        return;
    };
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    sel = sel.min(p.answers.len().saturating_sub(1));
    let Some(cur) = p.answers.get(sel) else {
        app.votes = None;
        return;
    };
    if let Some(w) = &mut app.votes
        && w.data.get(&cur.id).is_none_or(|e| e.0 != cur.count)
    {
        w.data.insert(cur.id, (cur.count, None));
        let path = format!("/channels/{ch}/polls/{msg}/answers/{}?limit=100", cur.id);
        app.rest("GET", path, None, Tag::PollVotes(ch, msg, cur.id));
    }
    let users = app.votes.as_ref().and_then(|w| w.data.get(&cur.id)).and_then(|e| e.1.clone());
    let (mut close, mut pick, mut profile) = (false, None, None);
    let store = &app.store;
    let modal = crate::ui::motion::Modal::new("poll_votes".into())
        .frame(Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            Frame::new().inner_margin(Margin { left: 24, right: 16, top: 16, bottom: 12 }).show(ui, |ui| {
                ui.set_width(400.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Votes").font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
            let (body, _) = ui.allocate_exact_size(vec2(440.0, 360.0), Sense::hover());
            super::settings::mark(ui, "poll_votes", body);
            ui.painter().hline(body.x_range(), body.top(), egui::Stroke::new(1.0, pal().border_subtle));
            let side = Rect::from_min_size(pos2(body.left() + 24.0, body.top() + 16.0), vec2(150.0, body.height() - 16.0));
            ui.painter().vline(side.right() + 8.0, egui::Rangef::new(body.top() + 16.0, body.bottom() - 16.0), egui::Stroke::new(1.0, pal().border_subtle));
            let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(side).layout(Layout::top_down(Align::Min)));
            egui::ScrollArea::vertical().id_salt("vote_tabs").show(&mut tab_ui, |ui| {
                ui.set_width(142.0);
                for (i, a) in p.answers.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(142.0, 40.0), Sense::click());
                    super::settings::mark(ui, &format!("poll_votes:tab:{}", a.id), rect);
                    let fill = if i == sel { Some(pal().control_hover) } else if resp.hovered() { Some(MENU_HOVER) } else { None };
                    if let Some(f) = fill {
                        ui.painter().rect_filled(rect, CornerRadius::same(8), f);
                    }
                    let mut x = rect.left() + 8.0;
                    if let Some(em) = &a.emoji {
                        let er = Rect::from_min_size(pos2(x, rect.center().y - 9.0), vec2(18.0, 18.0));
                        Image::from_uri(gif::uri(ui.ctx(), &super::chat::emoji_url(em), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                        x += 24.0;
                    }
                    super::text::paint(ui.painter(), pos2(x, rect.center().y), Align2::LEFT_CENTER, &a.text, medium(14.0), pal().header_primary, rect.right() - x - 34.0);
                    ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, a.count.to_string(), semi(14.0), pal().text_muted);
                    if resp.clicked() {
                        pick = Some(i);
                    }
                    ui.add_space(8.0);
                }
            });
            let list = Rect::from_min_max(pos2(side.right() + 16.0, body.top() + 12.0), pos2(body.right() - 16.0, body.bottom()));
            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list).layout(Layout::top_down(Align::Min)));
            egui::ScrollArea::vertical().id_salt("vote_list").auto_shrink([false, false]).show(&mut list_ui, |ui| {
                ui.set_width(list.width());
                let Some(users) = &users else {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                    return;
                };
                if users.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("There are no votes for this answer.").font(reg(14.0)).color(pal().text_muted)));
                }
                for u in users {
                    let Some(user) = store.user(*u) else { continue };
                    let (r, resp) = ui.allocate_exact_size(vec2(list.width(), 52.0), Sense::click());
                    super::settings::mark(ui, &format!("poll_votes:user:{u}"), r);
                    crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), MENU_HOVER, 0.05);
                    avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                    let display = store.name_in(guild, *u);
                    let sub = user.username.clone();
                    let two = display != sub;
                    let y = if two { r.center().y - 8.0 } else { r.center().y };
                    super::text::paint(ui.painter(), pos2(r.left() + 46.0, y), Align2::LEFT_CENTER, &display, medium(16.0), pal().header_primary, list.width() - 60.0);
                    if two {
                        super::text::paint(ui.painter(), pos2(r.left() + 46.0, r.center().y + 10.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().text_muted, list.width() - 60.0);
                    }
                    if resp.clicked() {
                        profile = Some((*u, r.right_top()));
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
    if let Some((u, at)) = profile {
        app.votes = None;
        app.open_profile(u, guild, at);
        return;
    }
    match &mut app.votes {
        Some(_) if close => app.votes = None,
        Some(w) => w.sel = sel,
        None => {}
    }
}
