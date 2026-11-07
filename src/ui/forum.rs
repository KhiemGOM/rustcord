use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::json;
use std::collections::HashSet;

use super::icons::{self, Icon};
use super::{App, Trail, avatar, menu_row, menu_sep, text};
use crate::net::Tag;
use crate::state::{Channel, Id, snowflake_ms};
use crate::theme::*;

fn ago(id: Id) -> String {
    let secs = (chrono::Utc::now().timestamp_millis() - snowflake_ms(id)) / 1000;
    match secs {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86400 => format!("{}h ago", secs / 3600),
        86400..2592000 => format!("{}d ago", secs / 86400),
        _ => ">30d ago".into(),
    }
}

pub struct Post {
    pub title: String,
    pub body: String,
    pub tags: Vec<Id>,
    pub files: Vec<std::path::PathBuf>,
}

/// What Sort & View, the tag pills and the search bar are set to, per forum for the session.
#[derive(Clone, Default)]
struct View {
    gallery: bool,
    by_date: bool,
    match_all: bool,
    tags: HashSet<Id>,
    query: String,
}

pub fn show(app: &mut App, ui: &mut Ui, forum: &Channel) {
    let key = egui::Id::new(("forum_view", forum.id));
    let mut v: View = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| View { gallery: forum.layout == 2, ..Default::default() });
    let can_post = app.store.perms(forum) & crate::state::SEND_MESSAGES != 0;
    app.rects.retain(|k, _| !k.starts_with("forum:"));
    let mut open: Option<Id> = None;
    let mut submit: Option<Option<(String, String, Vec<Id>, Vec<std::path::PathBuf>)>> = None;
    let mut rects: Vec<(String, Rect)> = Vec::new();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(24).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            match &mut app.post {
                None => {
                    // discord starts the post's title from whatever was typed in the search
                    if search_bar(ui, &mut v.query, can_post, &mut rects) {
                        app.post = Some(Post { title: v.query.trim().to_string(), body: String::new(), tags: Vec::new(), files: Vec::new() });
                    }
                }
                Some(post) => {
                    Frame::new().fill(pal().bg_primary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_muted)).inner_margin(Margin::symmetric(12, 12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let media = Rect::from_min_size(pos2(ui.max_rect().right() - 78.0, ui.cursor().top()), vec2(78.0, 78.0));
                        let add = ui.interact(media, ui.id().with("post_media"), Sense::click());
                        let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &add));
                        ui.painter().rect(media, 8, fill, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
                        icons::draw(ui.painter(), Icon::AddMedia, Rect::from_center_size(media.center(), vec2(32.0, 32.0)), pal().header_primary);
                        rects.push(("forum:post_media".into(), media));
                        if add.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if add.on_hover_text("Add Media").clicked()
                            && let Some(picked) = rfd::FileDialog::new().pick_files()
                        {
                            post.files.extend(picked);
                        }
                        ui.horizontal(|ui| {
                            let (x, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                            icons::draw(ui.painter(), Icon::Close, x, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
                            rects.push(("forum:post_close".into(), x));
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.on_hover_text("Clear").clicked() {
                                submit = Some(None);
                            }
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut post.title)
                                    .frame(Frame::NONE)
                                    .font(semi(20.0))
                                    .hint_text(RichText::new("Title").font(semi(20.0)).color(pal().text_muted))
                                    .desired_width(ui.available_width() - 90.0),
                            );
                            rects.push(("forum:post_title".into(), r.rect));
                        });
                        ui.add_space(4.0);
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut post.body)
                                .frame(Frame::NONE)
                                .font(reg(16.0))
                                .margin(Margin { left: 34, ..Margin::ZERO })
                                .desired_rows(2)
                                .hint_text("Enter a message...")
                                .desired_width(ui.available_width() - 90.0),
                        );
                        rects.push(("forum:post_body".into(), r.rect));
                        if !post.files.is_empty() {
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                                let mut gone = None;
                                for (i, f) in post.files.iter().enumerate() {
                                    let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                    let g = ui.painter().layout_no_wrap(name, medium(14.0), pal().header_primary);
                                    let (chip, _) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 28.0), Sense::hover());
                                    ui.painter().rect_filled(chip, 6, pal().control);
                                    ui.painter().galley(pos2(chip.left() + 10.0, chip.center().y - g.size().y / 2.0), g, pal().header_primary);
                                    let x = Rect::from_center_size(pos2(chip.right() - 14.0, chip.center().y), vec2(14.0, 14.0));
                                    let resp = ui.interact(x.expand(4.0), ui.id().with(("post_file_x", i)), Sense::click());
                                    icons::draw(ui.painter(), Icon::Close, x, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
                                    rects.push((format!("forum:post_file_x:{i}"), x));
                                    if resp.on_hover_text("Remove Attachment").clicked() {
                                        gone = Some(i);
                                    }
                                }
                                if let Some(i) = gone {
                                    post.files.remove(i);
                                }
                            });
                        }
                        ui.add_space(12.0);
                        post_tags(ui, forum, &mut post.tags, &mut rects);
                        ui.add_space(12.0);
                        let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                        ui.painter().hline(rule.x_range(), rule.center().y, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
                        ui.add_space(8.0);
                        let filled = !post.title.trim().is_empty() && !post.body.trim().is_empty();
                        let needs_tag = forum.require_tag && post.tags.is_empty();
                        ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::right_to_left(Align::Center), |ui| {
                            let ok = filled && !needs_tag;
                            let send = post_button(ui, ok);
                            rects.push(("forum:post_send".into(), send.rect));
                            if send.clicked() && ok {
                                submit = Some(Some((post.title.trim().to_string(), post.body.trim().to_string(), post.tags.clone(), post.files.clone())));
                            }
                            if forum.rate > 0 {
                                ui.add_space(8.0);
                                ui.label(RichText::new("Slowmode is enabled.").font(reg(14.0)).color(pal().header_secondary));
                                let (t, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                icons::draw(ui.painter(), Icon::Timer, t, pal().header_secondary);
                            }
                            if filled && needs_tag {
                                let r = ui.add(egui::Label::new(RichText::new("A tag is required to create a post in this channel").font(medium(14.0)).color(RED)));
                                rects.push(("forum:post_tag_required".into(), r.rect));
                            }
                        });
                    });
                }
            }
            guidelines(ui, forum, &mut rects);
            ui.add_space(16.0);
            filters(ui, forum, &mut v);
            ui.add_space(20.0);

            let Some(posts) = app.store.forum_posts.get(&forum.id) else {
                crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                return;
            };
            let q = v.query.trim().to_lowercase();
            let mut posts: Vec<&Channel> = posts
                .iter()
                .filter_map(|p| app.store.channels.get(p))
                .filter(|p| v.tags.is_empty() || if v.match_all { v.tags.iter().all(|t| p.applied.contains(t)) } else { p.applied.iter().any(|t| v.tags.contains(t)) })
                .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                .collect();
            if v.by_date {
                posts.sort_by_key(|p| std::cmp::Reverse(p.id));
            }
            if posts.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("There are no posts yet").font(semi(20.0)).color(pal().header_primary));
                    ui.label(RichText::new("Be the first to start a conversation.").color(pal().header_secondary));
                });
            }
            let seen = app.store.reads.get(&forum.id).map(|r| r.last_acked);
            let fresh = |p: &Channel| seen.is_some_and(|a| p.id > a);
            if v.gallery {
                // discord's grid: as many columns of at least 400 as fit, 16 apart, 3:2 previews
                let w = ui.available_width();
                let cols = ((w + 16.0) / 416.0).floor().max(1.0) as usize;
                let cw = (w - 16.0 * (cols - 1) as f32) / cols as f32;
                let ch = 120.0 + (cw - 26.0) * 2.0 / 3.0;
                for row in posts.chunks(cols) {
                    let (band, _) = ui.allocate_exact_size(vec2(w, ch), Sense::hover());
                    for (i, p) in row.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(band.left() + i as f32 * (cw + 16.0), band.top()), vec2(cw, ch));
                        let resp = tile(app, ui, forum, p, r, fresh(p));
                        rects.push((format!("post:{}", p.id), r));
                        if resp.clicked() {
                            open = Some(p.id);
                        }
                    }
                    ui.add_space(16.0);
                }
            } else {
                ui.spacing_mut().item_spacing.y = 8.0;
                for p in posts {
                    let r = card(app, ui, forum, p);
                    rects.push((format!("post:{}", p.id), r.rect));
                    if r.clicked() {
                        open = Some(p.id);
                    }
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(key, v));
    for (k, r) in rects {
        app.rects.insert(k, r);
    }
    if let Some(sent) = submit {
        if let Some((name, body, tags, files)) = sent {
            let archive = if forum.archive_after > 0 { forum.archive_after } else { 4320 };
            let payload = json!({ "name": name, "auto_archive_duration": archive, "applied_tags": tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(), "message": { "content": body } });
            if files.is_empty() {
                app.rest("POST", format!("/channels/{}/threads?use_nested_fields=true", forum.id), Some(payload), Tag::reason());
            } else {
                let files = files.into_iter().map(|p| (p.clone(), p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), String::new())).collect();
                app.net.send(crate::net::Cmd::ForumPost { channel: forum.id, body: payload, files, tag: Tag::reason() });
            }
        }
        app.post = None;
    }
    if let Some(p) = open {
        let shift = ui.input(|i| i.modifiers.shift);
        app.open_thread(p, false, shift);
    }
}

/// The forum's topic as a card under the post box, until it's closed.
fn guidelines(ui: &mut Ui, forum: &Channel, rects: &mut Vec<(String, Rect)>) {
    let Some(topic) = forum.topic.as_ref().filter(|t| !t.trim().is_empty()) else { return };
    let hidden = egui::Id::new(("forum_guidelines_closed", forum.id));
    if ui.data(|d| d.get_temp::<bool>(hidden)).unwrap_or(false) {
        return;
    }
    let body: String = topic
        .lines()
        .map(|l| match l.trim_start().strip_prefix("- ").or_else(|| l.trim_start().strip_prefix("* ")) {
            Some(item) => format!("•  {item}"),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("
");
    ui.add_space(16.0);
    Frame::new().fill(pal().bg_primary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_muted)).inner_margin(16).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        icons::draw(ui.painter(), Icon::Forum, Rect::from_center_size(pos2(head.left() + 10.0, head.center().y), vec2(20.0, 20.0)), pal().header_primary);
        ui.painter().text(pos2(head.left() + 32.0, head.center().y), Align2::LEFT_CENTER, "Post Guidelines", semi(20.0), pal().header_primary);
        let x = Rect::from_center_size(pos2(head.right() - 8.0, head.center().y), vec2(16.0, 16.0));
        let resp = ui.interact(x.expand(6.0), ui.id().with("guidelines_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
        rects.push(("forum:guidelines_close".into(), x));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text("Close").clicked() {
            ui.data_mut(|d| d.insert_temp(hidden, true));
        }
        ui.add_space(8.0);
        Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let r = ui.add(egui::Label::new(RichText::new(body).font(reg(14.0)).color(pal().text_normal)));
            rects.push(("forum:guidelines".into(), r.rect));
        });
    });
}

/// The bar over the posts: a search that narrows them by title, New Post at its right end
/// (dimmed without permission to post). True when New Post was clicked.
fn search_bar(ui: &mut Ui, q: &mut String, can_post: bool, rects: &mut Vec<(String, Rect)>) -> bool {
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    ui.painter().rect(bar, 8, pal().bg_primary, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(bar.left() + 26.0, bar.center().y), vec2(26.0, 26.0)), pal().header_secondary);
    let ink = if can_post { Color32::WHITE } else { Color32::from_white_alpha(128) };
    let label = ui.painter().layout_no_wrap("New Post".into(), medium(14.0), ink);
    let b = Rect::from_min_max(pos2(bar.right() - 9.0 - label.size().x - 44.0, bar.center().y - 16.0), pos2(bar.right() - 9.0, bar.center().y + 16.0));
    let resp = ui.interact(b, ui.id().with("new_post"), if can_post { Sense::click() } else { Sense::hover() });
    rects.push(("forum:new_post".into(), b));
    let fill = match (can_post, resp.hovered()) {
        (false, _) => BLURPLE.gamma_multiply(0.5),
        (true, true) => BLURPLE.gamma_multiply(0.85),
        (true, false) => BLURPLE,
    };
    ui.painter().rect_filled(b, 8, fill);
    icons::draw(ui.painter(), Icon::Bubble, Rect::from_min_size(pos2(b.left() + 12.0, b.center().y - 8.0), vec2(16.0, 16.0)), ink);
    ui.painter().galley(pos2(b.left() + 32.0, b.center().y - label.size().y / 2.0), label, ink);
    if can_post && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let field = Rect::from_min_max(pos2(bar.left() + 52.0, bar.center().y - 13.0), pos2(b.left() - 12.0, bar.center().y + 13.0));
    // shift+enter in the search makes what was typed the new post's title
    let shift_enter = can_post && ui.input_mut(|i| super::take_key(i, egui::Modifiers::SHIFT, egui::Key::Enter));
    let edit = ui.new_child(UiBuilder::new().max_rect(field)).add(
        egui::TextEdit::singleline(q)
            .frame(Frame::NONE)
            .font(medium(20.0))
            .margin(Margin::ZERO)
            .desired_width(field.width())
            .hint_text(RichText::new("Search").font(medium(20.0)).color(pal().text_muted)),
    );
    if can_post && !q.trim().is_empty() {
        ui.add_space(6.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), egui::Align2::LEFT_CENTER, "SHIFT + ENTER to create a new post", semi(12.0), pal().text_muted);
    }
    resp.clicked() || (shift_enter && (edit.has_focus() || edit.lost_focus()))
}

/// The post form's tag row: a tag icon, the tags that fit as toggles (five at most), and the
/// rest under "See more tags".
fn post_tags(ui: &mut Ui, forum: &Channel, picked: &mut Vec<Id>, rects: &mut Vec<(String, Rect)>) {
    if forum.tags.is_empty() {
        return;
    }
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    icons::draw(ui.painter(), Icon::Tag, Rect::from_center_size(pos2(row.left() + 8.0, row.center().y), vec2(16.0, 16.0)), pal().header_secondary);
    let more_w = ui.painter().layout_no_wrap("See more tags".into(), medium(14.0), Color32::WHITE).size().x + 46.0;
    let at_limit = picked.len() >= 5;
    let mut x = row.left() + 24.0;
    let mut full = false;
    let mut flip = None;
    for t in &forum.tags {
        let w = chip_width(ui, t);
        full |= x + w > row.right() - more_w - 4.0;
        if full {
            continue;
        }
        let on = picked.contains(&t.id);
        let r = chip(ui, Rect::from_min_size(pos2(x, row.center().y - 12.0), vec2(w, 24.0)), t, on, true, ui.id().with(("post_tag", t.id)));
        rects.push((format!("forum:post_tag:{}", t.id), r.rect));
        if r.clicked() && (on || !at_limit) {
            flip = Some(t.id);
        }
        x = r.rect.right() + 4.0;
    }
    if full {
        let more = pill(ui, pos2(x, row.top()), None, None, "See more tags", true, false, ui.id().with("post_more_tags"));
        rects.push(("forum:post_more_tags".into(), more.rect));
        egui::Popup::menu(&more).gap(4.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for t in &forum.tags {
                let on = picked.contains(&t.id);
                let row = menu_row(ui, None, &tag_label(t), None, false, Trail::Check(on));
                rects.push((format!("forum:more_tag:{}", t.id), row.rect));
                if row.clicked() && (on || !at_limit) {
                    flip = Some(t.id);
                }
            }
        });
    }
    if let Some(t) = flip {
        match picked.iter().position(|p| *p == t) {
            Some(i) => {
                picked.remove(i);
            }
            None => picked.push(t),
        }
    }
}

fn post_button(ui: &mut Ui, ok: bool) -> egui::Response {
    let ink = if ok { Color32::WHITE } else { Color32::from_white_alpha(128) };
    let g = ui.painter().layout_no_wrap("Post".into(), medium(14.0), ink);
    let (b, resp) = ui.allocate_exact_size(vec2(12.0 + 16.0 + 4.0 + g.size().x + 12.0, 32.0), if ok { Sense::click() } else { Sense::hover() });
    let fill = if ok { crate::ui::motion::mix(BLURPLE, BLURPLE.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)) } else { BLURPLE.gamma_multiply(0.5) };
    ui.painter().rect_filled(b, 8, fill);
    icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(pos2(b.left() + 20.0, b.center().y), vec2(16.0, 16.0)), ink);
    ui.painter().galley(pos2(b.left() + 32.0, b.center().y - g.size().y / 2.0), g, ink);
    if ok && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn chip_width(ui: &Ui, t: &crate::state::Tag) -> f32 {
    18.0 + ui.painter().layout_no_wrap(t.name.clone(), semi(12.0), Color32::WHITE).size().x + if tag_lead(t).is_some() { 18.0 } else { 0.0 }
}

/// A tag as the small rounded chip the post form and a post's top use: 24 tall, its emoji at 12.
fn chip(ui: &mut Ui, r: Rect, t: &crate::state::Tag, on: bool, click: bool, id: egui::Id) -> egui::Response {
    let resp = ui.interact(r, id, if click { Sense::click() } else { Sense::hover() });
    let (fill, edge) = if on {
        (BLURPLE.gamma_multiply(0.3), BLURPLE)
    } else {
        (Color32::from_rgba_unmultiplied(151, 151, 159, if click && resp.hovered() { 51 } else { 31 }), Color32::from_rgba_unmultiplied(151, 151, 159, 51))
    };
    ui.painter().rect(r, 12, fill, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    let g = ui.painter().layout_no_wrap(t.name.clone(), semi(12.0), pal().header_primary);
    let mut x = r.left() + 9.0;
    if let Some(u) = tag_lead(t) {
        egui::Image::from_uri(u).show_loading_spinner(false).paint_at(ui, Rect::from_min_size(pos2(x, r.center().y - 6.0), vec2(12.0, 12.0)));
        x += 18.0;
    }
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, pal().header_primary);
    if click && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn tag_label(t: &crate::state::Tag) -> String {
    match &t.emoji {
        Some(e) if t.emoji_id == 0 => format!("{e} {}", t.name),
        _ => t.name.clone(),
    }
}

fn tag_lead(t: &crate::state::Tag) -> Option<String> {
    match (&t.emoji, t.emoji_id) {
        (_, id) if id != 0 => Some(crate::markdown::emoji_url(id, false)),
        (Some(e), _) => Some(crate::markdown::twemoji(e)),
        _ => None,
    }
}

/// The top of a post: a bubble, the title large, then the tags it was posted with.
pub fn post_intro(ui: &mut Ui, post: &Channel, forum: &Channel) {
    let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
    icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(r.center(), vec2(40.0, 40.0)), pal().header_primary);
    ui.add_space(8.0);
    ui.label(RichText::new(&post.name).font(bold(32.0)).color(pal().header_primary));
    let tags: Vec<_> = post.applied.iter().filter_map(|t| forum.tags.iter().find(|x| x.id == *t)).collect();
    if tags.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for t in tags {
            let (b, _) = ui.allocate_exact_size(vec2(chip_width(ui, t), 24.0), Sense::hover());
            chip(ui, b, t, false, false, ui.id().with(("intro_tag", t.id)));
        }
    });
}

fn tag_width(ui: &Ui, t: &crate::state::Tag) -> f32 {
    ui.painter().layout_no_wrap(t.name.clone(), semi(14.0), Color32::WHITE).size().x + 26.0 + if tag_lead(t).is_some() { 20.0 } else { 0.0 }
}

/// One of the filter row's rounded buttons. Sort & View and All are quieter and carry a caret.
fn pill(ui: &mut Ui, at: Pos2, icon: Option<Icon>, lead: Option<&str>, label: &str, caret: bool, on: bool, id: egui::Id) -> egui::Response {
    let quiet = icon.is_some() || caret;
    let (font, ink) = if quiet { (medium(14.0), pal().header_secondary) } else { (semi(14.0), pal().header_primary) };
    let g = ui.painter().layout_no_wrap(label.to_string(), font, ink);
    let w = 13.0 + if icon.is_some() { 24.0 } else { 0.0 } + if lead.is_some() { 20.0 } else { 0.0 } + g.size().x + if caret { 33.0 } else { 13.0 };
    let r = Rect::from_min_size(at, vec2(w, 32.0));
    let resp = ui.interact(r, id, Sense::click());
    let (fill, edge) = if on {
        (BLURPLE.gamma_multiply(0.3), BLURPLE)
    } else if resp.hovered() {
        (Color32::from_rgba_unmultiplied(151, 151, 159, 51), pal().border_subtle)
    } else {
        (Color32::from_rgba_unmultiplied(151, 151, 159, 31), pal().border_subtle)
    };
    let p = ui.painter();
    p.rect(r, 16, fill, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    let mut x = r.left() + 13.0;
    if let Some(i) = icon {
        icons::draw(p, i, Rect::from_min_size(pos2(x, r.center().y - 8.0), vec2(16.0, 16.0)), ink);
        x += 24.0;
    }
    if let Some(u) = lead {
        egui::Image::from_uri(u.to_string()).show_loading_spinner(false).paint_at(ui, Rect::from_min_size(pos2(x, r.center().y - 7.0), vec2(14.0, 14.0)));
        x += 20.0;
    }
    let p = ui.painter();
    let gw = g.size().x;
    p.galley(pos2(x, r.center().y - g.size().y / 2.0), g, ink);
    if caret {
        icons::draw(p, Icon::ChevronDown, Rect::from_min_size(pos2(x + gw + 4.0, r.center().y - 10.0), vec2(20.0, 20.0)), ink);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Sort & View, a rule, then the forum's tags; the ones that don't fit wait under "All".
fn filters(ui: &mut Ui, forum: &Channel, v: &mut View) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    let sv = pill(ui, row.min, Some(Icon::Sort), None, "Sort & View", true, false, ui.id().with("sort_view"));
    egui::Popup::menu(&sv).gap(4.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let head = |ui: &mut Ui, t: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(200.0), 28.0), Sense::hover());
            ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, t, semi(12.0), pal().header_secondary);
        };
        head(ui, "Sort by");
        if menu_row(ui, None, "Recently Active", None, false, Trail::Radio(!v.by_date)).clicked() {
            v.by_date = false;
        }
        if menu_row(ui, None, "Date Posted", None, false, Trail::Radio(v.by_date)).clicked() {
            v.by_date = true;
        }
        menu_sep(ui);
        head(ui, "View as");
        if menu_row(ui, None, "List", None, false, Trail::Radio(!v.gallery)).clicked() {
            v.gallery = false;
        }
        if menu_row(ui, None, "Gallery", None, false, Trail::Radio(v.gallery)).clicked() {
            v.gallery = true;
        }
        if !forum.tags.is_empty() {
            menu_sep(ui);
            head(ui, "Tag Matching");
            if menu_row(ui, None, "Match Some", None, false, Trail::Radio(!v.match_all)).clicked() {
                v.match_all = false;
            }
            if menu_row(ui, None, "Match All", None, false, Trail::Radio(v.match_all)).clicked() {
                v.match_all = true;
            }
        }
        menu_sep(ui);
        if menu_row(ui, None, "Reset to default", None, false, Trail::None).clicked() {
            v.by_date = false;
            v.match_all = false;
            v.gallery = forum.layout == 2;
        }
    });
    if forum.tags.is_empty() {
        return;
    }
    let mut x = sv.rect.right() + 16.0;
    ui.painter().rect_filled(Rect::from_min_size(pos2(x, row.center().y - 12.0), vec2(2.0, 24.0)), 1, pal().border_subtle);
    x += 18.0;
    let mut flip = None;
    let mut full = false;
    for t in &forum.tags {
        let w = tag_width(ui, t);
        // leave room for "All" once anything spills over
        full |= x + w > row.right() - 72.0;
        if full {
            continue;
        }
        let r = pill(ui, pos2(x, row.top()), None, tag_lead(t).as_deref(), &t.name, false, v.tags.contains(&t.id), ui.id().with(("tag", t.id)));
        if r.clicked() {
            flip = Some(t.id);
        }
        x = r.rect.right() + 8.0;
    }
    if full {
        let all = pill(ui, pos2(x, row.top()), None, None, "All", true, false, ui.id().with("all_tags"));
        egui::Popup::menu(&all).gap(4.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for t in &forum.tags {
                if menu_row(ui, None, &tag_label(t), None, false, Trail::Check(v.tags.contains(&t.id))).clicked() {
                    flip = Some(t.id);
                }
            }
        });
    }
    if let Some(t) = flip
        && !v.tags.remove(&t)
    {
        v.tags.insert(t);
    }
}

/// A gallery card: author and age (NEW if it came after your last visit), the title, the first
/// image fitted into a 3:2 preview with the tags over its foot, and the message count.
fn tile(app: &App, ui: &mut Ui, forum: &Channel, p: &Channel, r: Rect, fresh: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("tile", p.id)), Sense::click());
    let first = app.store.refs.get(&p.id);
    let paint = ui.painter().with_clip_rect(r.intersect(ui.clip_rect()));
    paint.rect(r, 8, crate::ui::motion::mix(pal().bg_primary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    let x = r.left() + 13.0;
    let who = first.map_or(p.owner, |m| m.author);
    let color = app.store.role_color(p.guild, who).map(role_color).unwrap_or(pal().header_primary);
    let n = text::paint(&paint, pos2(x, r.top() + 22.5), Align2::LEFT_CENTER, &app.store.name_in(p.guild, who), semi(14.0), color, r.width() / 2.0);
    paint.text(pos2(n.right() + 6.0, r.top() + 22.5), Align2::LEFT_CENTER, ago(p.id), reg(12.0), pal().text_normal);
    if fresh {
        let g = paint.layout_no_wrap("NEW".into(), bold(12.0), Color32::WHITE);
        let b = Rect::from_min_max(pos2(r.right() - 11.0 - g.size().x - 16.0, r.top() + 13.5), pos2(r.right() - 11.0, r.top() + 29.5));
        paint.rect_filled(b, 8, BLURPLE);
        paint.galley(b.center() - g.size() / 2.0, g, Color32::WHITE);
    }
    let title = if p.joined && app.store.unread(p) { pal().header_primary } else { pal().text_muted };
    text::paint(&paint, pos2(x, r.top() + 46.5), Align2::LEFT_CENTER, &p.name, bold(16.0), title, r.width() - 26.0);

    let media = Rect::from_min_size(pos2(x, r.top() + 66.5), vec2(r.width() - 26.0, (r.width() - 26.0) * 2.0 / 3.0));
    let pic = first.and_then(|m| {
        m.attachments
            .iter()
            .find(|a| a.is_image())
            .map(|a| (a.proxy.clone(), a.w, a.h))
            .or_else(|| m.embeds.iter().find_map(|e| e.image.as_ref().or(e.thumb.as_ref())).map(|i| (i.url.clone(), i.w, i.h)))
    });
    match (pic, first) {
        (Some((url, w, h)), _) => {
            let s = (media.width() / w.max(1) as f32).min(media.height() / h.max(1) as f32);
            let fit = Rect::from_center_size(media.center(), vec2(w as f32 * s, h as f32 * s));
            let sep = if url.contains('?') { '&' } else { '?' };
            let url = format!("{url}{sep}width={}&height={}", (fit.width() * 2.0) as u32, (fit.height() * 2.0) as u32);
            egui::Image::from_uri(url).show_loading_spinner(false).paint_at(&ui.new_child(UiBuilder::new().max_rect(r)), fit);
        }
        (None, Some(m)) => {
            let body = crate::markdown::plain(&m.content, &app.store, p.guild);
            let mut job = egui::text::LayoutJob::simple(body, reg(14.0), pal().header_secondary, media.width());
            job.wrap.max_rows = (media.height() / 18.0) as usize;
            paint.galley(media.min, ui.painter().layout_job(job), pal().header_secondary);
        }
        _ => {}
    }
    let mut tx = media.left() + 13.0;
    for t in p.applied.iter().filter_map(|t| forum.tags.iter().find(|x| x.id == *t)) {
        let g = paint.layout_no_wrap(tag_label(t), semi(12.0), pal().header_primary);
        let b = Rect::from_min_size(pos2(tx, media.bottom() - 33.0), vec2(g.size().x + 18.0, 24.0));
        if b.right() > media.right() {
            break;
        }
        paint.rect(b, 12, pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        paint.galley(pos2(b.left() + 9.0, b.center().y - g.size().y / 2.0), g, pal().header_primary);
        tx = b.right() + 8.0;
    }
    let cy = r.bottom() - 30.0;
    icons::draw(&paint, Icon::Bubble, Rect::from_center_size(pos2(x + 8.0, cy), vec2(16.0, 16.0)), pal().text_muted);
    paint.text(pos2(x + 20.0, cy), Align2::LEFT_CENTER, p.msg_count.to_string(), medium(14.0), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn card(app: &App, ui: &mut Ui, forum: &Channel, p: &Channel) -> egui::Response {
    let first = app.store.refs.get(&p.id);
    let r = Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        let tags: Vec<_> = p.applied.iter().filter_map(|t| forum.tags.iter().find(|x| x.id == *t)).collect();
        if !tags.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for t in tags {
                    Frame::new().fill(pal().bg_mod_hover).corner_radius(8).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                        ui.label(RichText::new(tag_label(t)).font(semi(12.0)).color(pal().header_primary));
                    });
                }
            });
        }
        super::text::label(ui, &p.name, semi(16.0), pal().header_primary, Sense::hover());
        if let Some(m) = first {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let color = app.store.role_color(p.guild, m.author).map(role_color).unwrap_or(pal().header_primary);
                ui.label(RichText::new(format!("{}:", app.store.name_in(p.guild, m.author))).font(semi(14.0)).color(color));
                let excerpt: String = m.content.replace('\n', " ").chars().take(160).collect();
                ui.add(egui::Label::new(RichText::new(excerpt).font(reg(14.0)).color(pal().header_secondary)).truncate());
            });
            if let Some(a) = m.attachments.iter().find(|a| a.is_image()) {
                let (r, _) = ui.allocate_exact_size(vec2(96.0, 72.0), Sense::hover());
                egui::Image::from_uri(format!("{}{}width=192&height=144", a.proxy, if a.proxy.contains('?') { '&' } else { '?' }))
                    .corner_radius(CornerRadius::same(4))
                    .show_loading_spinner(false)
                    .paint_at(ui, r);
            }
        }
        ui.horizontal(|ui| {
            if let Some(u) = app.store.user(p.owner) {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                avatar(ui, r, &u.avatar_url());
            }
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            super::icons::draw(ui.painter(), super::icons::Icon::Bubble, r, pal().header_secondary);
            ui.label(RichText::new(p.msg_count.to_string()).font(reg(13.0)).color(pal().header_secondary));
            ui.label(RichText::new("•").color(pal().text_muted));
            ui.label(RichText::new(ago(p.last_msg.max(p.id))).font(reg(13.0)).color(pal().header_secondary));
        });
    });
    let resp = r.response.interact(Sense::click());
    if resp.hovered() {
        ui.painter().rect_stroke(resp.rect, CornerRadius::same(8), egui::Stroke::new(1.0, pal().bg_accent), egui::StrokeKind::Inside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
