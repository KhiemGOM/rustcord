use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::json;
use std::collections::HashSet;

use super::icons::{self, Icon};
use super::{App, Trail, avatar, button, menu_row, menu_sep, text};
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

/// What Sort & View, the tag pills and the search bar are set to, per forum for the session.
#[derive(Clone, Default)]
struct View {
    gallery: bool,
    by_date: bool,
    tags: HashSet<Id>,
    query: String,
}

pub fn show(app: &mut App, ui: &mut Ui, forum: &Channel) {
    let key = egui::Id::new(("forum_view", forum.id));
    let mut v: View = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| View { gallery: forum.layout == 2, ..Default::default() });
    let can_post = app.store.perms(forum) & crate::state::SEND_MESSAGES != 0;
    let mut open: Option<Id> = None;
    let mut submit = None;
    let mut rects = Vec::new();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(24).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            match &mut app.post {
                None => {
                    // discord starts the post's title from whatever was typed in the search
                    if search_bar(ui, &mut v.query, can_post) {
                        app.post = Some((v.query.trim().to_string(), String::new()));
                    }
                }
                Some((title, body)) => {
                    Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.add(
                            egui::TextEdit::singleline(title)
                                .frame(Frame::NONE)
                                .font(semi(20.0))
                                .hint_text("Post title")
                                .desired_width(ui.available_width()),
                        );
                        ui.add_space(6.0);
                        Frame::new().fill(pal().bg_input).corner_radius(8).inner_margin(12).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(body)
                                    .frame(Frame::NONE)
                                    .font(reg(16.0))
                                    .desired_rows(3)
                                    .hint_text("Enter a message to start the conversation")
                                    .desired_width(ui.available_width()),
                            );
                        });
                        ui.add_space(6.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let ok = !title.trim().is_empty() && !body.trim().is_empty();
                            if button(ui, "Post", if ok { BLURPLE } else { pal().bg_accent }, 90.0).clicked() && ok {
                                submit = Some((title.trim().to_string(), body.trim().to_string()));
                            }
                            if ui.add(egui::Label::new(RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                                submit = Some((String::new(), String::new()));
                            }
                        });
                    });
                }
            }
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
                .filter(|p| v.tags.is_empty() || p.applied.iter().any(|t| v.tags.contains(t)))
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
                        rects.push((p.id, r));
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
                    rects.push((p.id, r.rect));
                    if r.clicked() {
                        open = Some(p.id);
                    }
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(key, v));
    for (p, r) in rects {
        app.rects.insert(format!("post:{p}"), r);
    }
    if let Some((title, body)) = submit {
        if !title.is_empty() {
            let payload = json!({ "name": title, "message": { "content": body }, "applied_tags": [] });
            app.rest("POST", format!("/channels/{}/threads", forum.id), Some(payload), Tag::Quiet);
        }
        app.post = None;
    }
    if let Some(p) = open {
        app.open(p);
    }
}

/// The bar over the posts: a search that narrows them by title, New Post at its right end
/// (dimmed without permission to post). True when New Post was clicked.
fn search_bar(ui: &mut Ui, q: &mut String, can_post: bool) -> bool {
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    ui.painter().rect(bar, 8, pal().bg_primary, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(bar.left() + 26.0, bar.center().y), vec2(26.0, 26.0)), pal().header_secondary);
    let ink = if can_post { Color32::WHITE } else { Color32::from_white_alpha(128) };
    let label = ui.painter().layout_no_wrap("New Post".into(), medium(14.0), ink);
    let b = Rect::from_min_max(pos2(bar.right() - 9.0 - label.size().x - 44.0, bar.center().y - 16.0), pos2(bar.right() - 9.0, bar.center().y + 16.0));
    let resp = ui.interact(b, ui.id().with("new_post"), if can_post { Sense::click() } else { Sense::hover() });
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

fn tag_label(t: &crate::state::Tag) -> String {
    match &t.emoji {
        Some(e) => format!("{e} {}", t.name),
        None => t.name.clone(),
    }
}

/// One of the filter row's rounded buttons. Sort & View and All are quieter and carry a caret.
fn pill(ui: &mut Ui, at: Pos2, icon: Option<Icon>, label: &str, caret: bool, on: bool, id: egui::Id) -> egui::Response {
    let quiet = icon.is_some() || caret;
    let (font, ink) = if quiet { (medium(14.0), pal().header_secondary) } else { (semi(14.0), pal().header_primary) };
    let g = ui.painter().layout_no_wrap(label.to_string(), font, ink);
    let w = 13.0 + if icon.is_some() { 24.0 } else { 0.0 } + g.size().x + if caret { 33.0 } else { 13.0 };
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
    let sv = pill(ui, row.min, Some(Icon::Sort), "Sort & View", true, false, ui.id().with("sort_view"));
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
        let label = tag_label(t);
        let w = ui.painter().layout_no_wrap(label.clone(), semi(14.0), Color32::WHITE).size().x + 26.0;
        // leave room for "All" once anything spills over
        full |= x + w > row.right() - 72.0;
        if full {
            continue;
        }
        let r = pill(ui, pos2(x, row.top()), None, &label, false, v.tags.contains(&t.id), ui.id().with(("tag", t.id)));
        if r.clicked() {
            flip = Some(t.id);
        }
        x = r.rect.right() + 8.0;
    }
    if full {
        let all = pill(ui, pos2(x, row.top()), None, "All", true, false, ui.id().with("all_tags"));
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
    paint.rect(r, 8, if resp.hovered() { pal().bg_mod_hover } else { pal().bg_primary }, Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
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
