use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::icons::{self, Icon};
use super::{App, avatar, badge, button, channel_title, grey_button, status_dot, text};
use crate::net::Tag;
use crate::state::{Channel, Id, Store, sid};
use crate::theme::*;

#[derive(Default)]
pub struct State {
    pub tab: u8,
    pub fetched: bool,
}

pub fn count(store: &Store) -> u32 {
    store.dms.iter().filter(|id| store.channels.get(id).is_some_and(|c| c.is_message_request() && !c.is_spam())).count() as u32
}

pub fn spam_count(store: &Store) -> u32 {
    store.dms.iter().filter(|id| store.channels.get(id).is_some_and(|c| c.is_spam())).count() as u32
}

pub fn list(store: &Store, spam: bool) -> Vec<Id> {
    let mut ids: Vec<Id> = store
        .dms
        .iter()
        .copied()
        .filter(|id| {
            store.channels.get(id).is_some_and(|c| {
                if spam { c.is_spam() } else { c.is_message_request() && !c.is_spam() }
            })
        })
        .collect();
    ids.sort_by(|a, b| {
        let la = store.channels.get(a).map_or(0, |c| c.last_msg.max(c.id));
        let lb = store.channels.get(b).map_or(0, |c| c.last_msg.max(c.id));
        lb.cmp(&la)
    });
    ids
}

pub fn parse_supplemental(v: &Value) -> HashMap<Id, String> {
    let mut out = HashMap::new();
    let arr = v["supplemental_data"].as_array().or_else(|| v.as_array());
    for item in arr.into_iter().flatten() {
        let ch = sid(&item["channel_id"]);
        if ch == 0 {
            continue;
        }
        let msg = &item["message_preview"];
        let text = msg["content"]
            .as_str()
            .and_then(|s| s.lines().next())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                msg["attachments"]
                    .as_array()
                    .filter(|a| !a.is_empty())
                    .map(|a| format!("Sent {} attachment{}", a.len(), if a.len() == 1 { "" } else { "s" }))
            });
        if let Some(t) = text {
            out.insert(ch, t);
        }
    }
    out
}

fn accept(app: &mut App, ch: Id) {
    if let Some(c) = app.store.channels.get_mut(&ch) {
        c.clear_message_request();
    }
    app.rest("PUT", format!("/channels/{ch}/recipients/@me"), Some(json!({ "consent_status": 2 })), Tag::Quiet);
    app.open(ch);
}

fn ignore(app: &mut App, ch: Id) {
    app.store.dms.retain(|id| *id != ch);
    app.store.channels.remove(&ch);
    app.rest("DELETE", format!("/channels/{ch}/recipients/@me"), None, Tag::Quiet);
    if app.cur() == Some(ch) {
        app.ui.chan.remove(&0);
        if app.msg_requests.is_none() {
            app.friends = true;
        }
    }
}

fn batch_reject(app: &mut App, ids: &[Id]) {
    if ids.is_empty() {
        return;
    }
    let str_ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    for &ch in ids {
        app.store.dms.retain(|id| *id != ch);
        app.store.channels.remove(&ch);
        if app.cur() == Some(ch) {
            app.ui.chan.remove(&0);
        }
    }
    app.rest("PUT", "/channels/recipients/@me/batch-reject".into(), Some(json!({ "channel_ids": str_ids })), Tag::Quiet);
}

pub fn banner(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let spam = chan.is_spam();
    let mut do_accept = false;
    let mut do_ignore = false;
    egui::Panel::top("msg_request_banner")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_secondary).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), Stroke::new(1.0, pal().border_subtle));
            ui.horizontal_centered(|ui| {
                let msg = if spam {
                    "This message request was filtered as spam."
                } else {
                    "Message request: accept to move this conversation to your Direct Messages."
                };
                ui.label(RichText::new(msg).font(reg(14.0)).color(if spam { pal().critical } else { pal().text_normal }));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if grey_button(ui, "Ignore", 72.0).clicked() {
                        do_ignore = true;
                    }
                    if button(ui, "Accept", pal().blurple, 72.0).clicked() {
                        do_accept = true;
                    }
                });
            });
        });
    if do_accept {
        accept(app, chan.id);
    } else if do_ignore {
        ignore(app, chan.id);
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let reqs = count(&app.store);
    let spams = spam_count(&app.store);
    let tab = app.msg_requests.as_ref().map_or(0, |s| s.tab);
    let mut next_tab = tab;
    egui::Panel::top("requests_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom(), Stroke::new(1.0, pal().border_subtle));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (ir, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Bubble, ir, pal().channel_default);
                ui.label(RichText::new("Message Requests").font(medium(16.0)).color(pal().header_primary));
                let (dot, _) = ui.allocate_exact_size(vec2(4.0, 4.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61));
                for (i, (label, n)) in [("Requests", reqs), ("Spam", spams)].iter().enumerate() {
                    let sel = tab == i as u8;
                    let g = ui.painter().layout_no_wrap((*label).into(), medium(14.0), pal().header_primary);
                    let extra = if *n > 0 { 24.0 } else { 0.0 };
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0 + extra, 30.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_mod_hover);
                    }
                    let col = if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                    ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, *label, medium(14.0), col);
                    if *n > 0 {
                        badge(ui, pos2(r.right() - 8.0, r.center().y), *n, None);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        next_tab = i as u8;
                    }
                }
            });
        });
    if let Some(s) = &mut app.msg_requests {
        s.tab = next_tab;
    }

    let ids = list(&app.store, next_tab == 1);
    let need_fetch = app.msg_requests.as_ref().is_some_and(|s| !s.fetched) && !ids.is_empty();
    if need_fetch {
        if let Some(s) = &mut app.msg_requests {
            s.fetched = true;
        }
        let query: Vec<String> = ids.iter().take(25).map(|id| format!("channel_ids={id}")).collect();
        app.rest("GET", format!("/users/@me/message-requests/supplemental-data?{}", query.join("&")), None, Tag::Keyed("msg_req_supp".into()));
    }
    let supp = app.keyed.get("msg_req_supp").and_then(|v| v.as_ref()).and_then(|r| r.as_ref().ok()).map(parse_supplemental).unwrap_or_default();

    let mut do_preview = None;
    let mut do_accept = None;
    let mut do_ignore = None;
    let mut do_clear_all = false;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
            if ids.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(64.0);
                    let title = if next_tab == 1 { "No filtered spam messages." } else { "No pending message requests." };
                    ui.label(RichText::new(title).font(semi(16.0)).color(pal().header_primary));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Messages from people outside your friends list will appear here.").font(reg(14.0)).color(pal().header_secondary));
                });
                return;
            }
            ui.horizontal(|ui| {
                let heading = if next_tab == 1 { format!("SPAM — {}", ids.len()) } else { format!("REQUESTS — {}", ids.len()) };
                ui.label(RichText::new(heading).font(semi(12.0)).color(pal().header_secondary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if grey_button(ui, "Clear All", 84.0).clicked() {
                        do_clear_all = true;
                    }
                });
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for &id in &ids {
                    let Some(c) = app.store.channels.get(&id) else { continue };
                    let title = channel_title(&app.store, id);
                    let person = (c.kind == 1).then(|| c.recipients.first().and_then(|u| app.store.user(*u))).flatten();
                    let sub = app
                        .store
                        .msgs
                        .get(&id)
                        .and_then(|m| m.list.last())
                        .map(|m| m.content.lines().next().unwrap_or("").to_string())
                        .filter(|s| !s.is_empty())
                        .or_else(|| supp.get(&id).cloned())
                        .unwrap_or_else(|| person.map(|u| format!("@{}", u.username)).unwrap_or_else(|| "Pending conversation".into()));
                    let card = Frame::new()
                        .fill(pal().bg_secondary)
                        .corner_radius(CornerRadius::same(8))
                        .stroke(Stroke::new(1.0, pal().border_subtle))
                        .inner_margin(Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (av, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                                if let Some(u) = person {
                                    avatar(ui, av, &u.avatar_url());
                                    status_dot(ui, av, app.store.status(u.id), pal().bg_secondary);
                                } else {
                                    ui.painter().circle_filled(av.center(), 18.0, pal().blurple);
                                    icons::draw(ui.painter(), Icon::People, Rect::from_center_size(av.center(), vec2(20.0, 20.0)), Color32::WHITE);
                                }
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    text::label(ui, &title, semi(15.0), pal().header_primary, Sense::hover());
                                    ui.label(RichText::new(&sub).font(reg(13.0)).color(pal().text_muted));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if grey_button(ui, "Ignore", 72.0).clicked() {
                                        do_ignore = Some(id);
                                    }
                                    if button(ui, "Accept", pal().blurple, 72.0).clicked() {
                                        do_accept = Some(id);
                                    }
                                    if grey_button(ui, "Preview", 76.0).clicked() {
                                        do_preview = Some(id);
                                    }
                                });
                            });
                        });
                    let _ = card;
                    ui.add_space(8.0);
                }
            });
        });
    });

    if do_clear_all {
        batch_reject(app, &ids);
    } else if let Some(ch) = do_accept {
        accept(app, ch);
    } else if let Some(ch) = do_ignore {
        ignore(app, ch);
    } else if let Some(ch) = do_preview {
        app.open(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_supplemental_preview_text_and_attachments() {
        let v = json!({
            "supplemental_data": [
                { "channel_id": "42", "message_preview": { "content": "hey there\nsecond line" } },
                { "channel_id": "99", "message_preview": { "content": "", "attachments": [{ "id": "1" }] } }
            ]
        });
        let map = parse_supplemental(&v);
        assert_eq!(map.get(&42).map(String::as_str), Some("hey there"));
        assert_eq!(map.get(&99).map(String::as_str), Some("Sent 1 attachment"));
    }

    #[test]
    fn separates_regular_requests_and_spam_channels() {
        let mut s = Store::default();
        let normal = Channel::parse(&json!({ "id": "1", "type": 1, "last_message_id": "10" }), 0);
        let req = Channel::parse(&json!({ "id": "2", "type": 1, "last_message_id": "20", "is_message_request": true }), 0);
        let spam = Channel::parse(&json!({ "id": "3", "type": 1, "last_message_id": "30", "is_message_request": true, "is_spam": true }), 0);
        assert!(normal.extra.is_none());
        assert!(req.is_message_request() && !req.is_spam());
        assert!(spam.is_spam());
        for c in [normal, req, spam] {
            s.dms.push(c.id);
            s.channels.insert(c.id, c);
        }
        assert_eq!(count(&s), 1);
        assert_eq!(spam_count(&s), 1);
        assert_eq!(list(&s, false), vec![2]);
        assert_eq!(list(&s, true), vec![3]);
        s.channels.get_mut(&2).unwrap().clear_message_request();
        assert_eq!(count(&s), 0);
        assert!(s.channels[&2].extra.is_none());
    }
}
