use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Sense, Ui, vec2};
use serde_json::json;

use super::{App, avatar, button};
use crate::net::Tag;
use crate::state::{Channel, Id, snowflake_ms};
use crate::theme::*;

fn ago(id: Id) -> String {
    let secs = (chrono::Utc::now().timestamp_millis() - snowflake_ms(id)) / 1000;
    match secs {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

pub fn show(app: &mut App, ui: &mut Ui, forum: &Channel) {
    let mut open: Option<Id> = None;
    let mut submit = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            match &mut app.post {
                None => {
                    ui.horizontal(|ui| {
                        Frame::new().fill(pal().bg_tertiary).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                            ui.set_width(ui.available_width() - 130.0);
                            ui.label(RichText::new("Search or create a post…").color(pal().text_muted));
                        });
                        if button(ui, "New Post", BLURPLE, 110.0).clicked() {
                            app.post = Some((String::new(), String::new()));
                        }
                    });
                }
                Some((title, body)) => {
                    Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).show(ui, |ui| {
                        ui.set_width(ui.available_width());
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
            ui.add_space(8.0);

            let posts = app.store.forum_posts.get(&forum.id).cloned();
            let Some(posts) = posts else {
                ui.add(egui::Spinner::new().color(pal().text_muted));
                return;
            };
            if posts.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("There are no posts yet").font(bold(20.0)).color(pal().header_primary));
                    ui.label(RichText::new("Be the first to start a conversation.").color(pal().header_secondary));
                });
            }
            for pid in posts {
                let Some(p) = app.store.channels.get(&pid) else { continue };
                if card(app, ui, forum, p).clicked() {
                    open = Some(pid);
                }
            }
        });
    });
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
                    let text = match &t.emoji {
                        Some(e) => format!("{e} {}", t.name),
                        None => t.name.clone(),
                    };
                    Frame::new().fill(pal().bg_mod_hover).corner_radius(8).inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
                        ui.label(RichText::new(text).font(semi(12.0)).color(pal().header_primary));
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
