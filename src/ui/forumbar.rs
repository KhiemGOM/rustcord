//! A forum post's toolbar: the post's reactions and React to Post on the left, Follow, Copy Link
//! and, once the first message is out of view, Jump To Top on the right.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::chat::emoji_url;
use super::icons::{self, Icon};
use crate::state::{Channel, Message, ReactEmoji};
use crate::theme::*;

pub enum Do {
    React(ReactEmoji, bool),
    Picker(Pos2),
    Follow(bool),
    Link,
    Top,
}

fn secondary(ui: &mut Ui, r: Rect, id: egui::Id) -> egui::Response {
    let resp = ui.interact(r, id, Sense::click());
    let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn show(ui: &mut Ui, post: &Channel, forum: &Channel, first: Option<&Message>, stuck: bool, copied: bool) -> Option<Do> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    let row = row.shrink2(vec2(16.0, 0.0));
    let id = ui.id().with(("forum_bar", post.id));
    let ink = pal().text_normal;
    let mut act = None;
    let mut x = row.left();
    let mid = row.center().y;
    let reactions = first.map(|m| m.reactions.as_slice()).unwrap_or_default();
    for r in reactions.iter().take(((row.width() - 340.0) / 58.0).max(0.0) as usize) {
        let g = ui.painter().layout_no_wrap(r.count.to_string(), semi(14.0), if r.me { pal().reacted_fg } else { pal().header_secondary });
        let b = Rect::from_min_size(pos2(x, mid - 15.0), vec2(6.0 + 20.0 + 6.0 + g.size().x + 8.0, 30.0));
        let resp = ui.interact(b, id.with(("react", r.emoji.id, &r.emoji.name)), Sense::click());
        let (fill, edge) = if r.me { (wash(pal().blurple, 61), pal().blurple) } else { (pal().control, if resp.hovered() { pal().bg_accent } else { Color32::TRANSPARENT }) };
        ui.painter().rect(b, 8, fill, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
        let er = Rect::from_min_size(pos2(b.left() + 7.0, mid - 10.0), vec2(20.0, 20.0));
        egui::Image::from_uri(crate::gif::uri(ui.ctx(), &emoji_url(&r.emoji), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
        ui.painter().galley(pos2(er.right() + 6.0, mid - g.size().y / 2.0), g.clone(), ink);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            act = Some(Do::React(r.emoji.clone(), !r.me));
        }
        x = b.right() + 4.0;
    }
    if reactions.is_empty()
        && let Some(e) = &forum.default_react
    {
        let b = Rect::from_min_size(pos2(x, mid - 15.0), vec2(36.0, 30.0));
        let resp = secondary(ui, b, id.with("default"));
        let er = Rect::from_center_size(b.center(), vec2(20.0, 20.0));
        egui::Image::from_uri(emoji_url(e)).show_loading_spinner(false).paint_at(ui, er);
        if resp.clicked() {
            act = Some(Do::React(e.clone(), true));
        }
        x = b.right() + 4.0;
    }
    let label = if reactions.is_empty() { "React to Post" } else { "" };
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), ink);
    let w = if label.is_empty() { 36.0 } else { 9.0 + 18.0 + 8.0 + g.size().x + 12.0 };
    let b = Rect::from_min_size(pos2(x, mid - 15.0), vec2(w, 30.0));
    let resp = secondary(ui, b, id.with("add"));
    super::settings::mark(ui, "forum_bar:react", b);
    icons::draw(ui.painter(), Icon::AddReaction, Rect::from_center_size(pos2(if label.is_empty() { b.center().x } else { b.left() + 18.0 }, mid), vec2(18.0, 18.0)), ink);
    if !label.is_empty() {
        ui.painter().galley(pos2(b.left() + 35.0, mid - g.size().y / 2.0), g, ink);
    }
    if resp.clicked() {
        act = Some(Do::Picker(b.left_bottom()));
    }

    let mut right = row.right();
    if stuck {
        let b = Rect::from_min_size(pos2(right - 32.0, row.top()), vec2(32.0, 32.0));
        let resp = secondary(ui, b, id.with("top"));
        super::settings::mark(ui, "forum_bar:top", b);
        icons::draw(ui.painter(), Icon::ArrowUp, Rect::from_center_size(b.center(), vec2(16.0, 16.0)), ink);
        if resp.on_hover_text("Jump To Top").clicked() {
            act = Some(Do::Top);
        }
        right = b.left() - 4.0;
    }
    let text = if copied { "Copied" } else { "" };
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), ink);
    let w = if copied { 12.0 + 16.0 + 4.0 + g.size().x + 12.0 } else { 32.0 };
    let b = Rect::from_min_size(pos2(right - w, row.top()), vec2(w, 32.0));
    let resp = secondary(ui, b, id.with("link"));
    super::settings::mark(ui, if copied { "forum_bar:copied" } else { "forum_bar:link" }, b);
    let at = if copied { pos2(b.left() + 20.0, mid) } else { b.center() };
    icons::draw(ui.painter(), if copied { Icon::Check } else { Icon::Link }, Rect::from_center_size(at, vec2(16.0, 16.0)), ink);
    if copied {
        ui.painter().galley(pos2(b.left() + 12.0 + 20.0, mid - g.size().y / 2.0), g, ink);
    }
    if resp.on_hover_text("Copy Link").clicked() {
        act = Some(Do::Link);
    }
    right = b.left() - 4.0;
    let following = post.joined;
    let text = if following { "Following" } else { "Follow" };
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), ink);
    let b = Rect::from_min_size(pos2(right - (12.0 + 16.0 + 4.0 + g.size().x + 12.0), row.top()), vec2(12.0 + 16.0 + 4.0 + g.size().x + 12.0, 32.0));
    let resp = secondary(ui, b, id.with("follow"));
    super::settings::mark(ui, "forum_bar:follow", b);
    icons::draw(ui.painter(), if following { Icon::Check } else { Icon::Bell }, Rect::from_center_size(pos2(b.left() + 20.0, mid), vec2(16.0, 16.0)), ink);
    ui.painter().galley(pos2(b.left() + 32.0, mid - g.size().y / 2.0), g, ink);
    if resp.on_hover_text("Add this post to your channel list and receive notifications about it.").clicked() {
        act = Some(Do::Follow(!following));
    }
    act
}
