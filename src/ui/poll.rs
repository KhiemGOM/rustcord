use egui::{Align, Align2, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, Pick, icon_button, menu_item, modal_button, text};
use crate::net::Tag;
use crate::state::{Id, ReactEmoji, make_nonce};
use crate::theme::*;

const DURATIONS: [(u32, &str); 7] = [(1, "1 hour"), (4, "4 hours"), (8, "8 hours"), (24, "24 hours"), (72, "3 days"), (168, "1 week"), (336, "2 weeks")];

pub struct Draft {
    pub ch: Id,
    pub question: String,
    pub answers: Vec<(String, Option<ReactEmoji>)>,
    hours: u32,
    multi: bool,
    fresh: bool,
}

impl Draft {
    pub fn new(ch: Id) -> Self {
        Draft { ch, question: String::new(), answers: vec![Default::default(), Default::default()], hours: 24, multi: false, fresh: true }
    }

    fn body(&self) -> Value {
        let answers: Vec<Value> = self
            .answers
            .iter()
            .filter(|(t, _)| !t.trim().is_empty())
            .map(|(t, e)| {
                let mut media = json!({ "text": t.trim() });
                if let Some(e) = e {
                    media["emoji"] = if e.id != 0 { json!({ "id": e.id.to_string(), "name": e.name }) } else { json!({ "name": e.name }) };
                }
                json!({ "poll_media": media })
            })
            .collect();
        json!({
            "content": "",
            "nonce": make_nonce(),
            "tts": false,
            "flags": 0,
            "poll": { "question": { "text": self.question.trim() }, "answers": answers, "allow_multiselect": self.multi, "duration": self.hours, "layout_type": 1 },
        })
    }

    fn ready(&self) -> bool {
        !self.question.trim().is_empty() && self.answers.iter().filter(|(t, _)| !t.trim().is_empty()).count() >= 2
    }
}

fn label(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(9.0);
}

pub fn check_row(ui: &mut Ui, on: &mut bool, label: &str) {
    let g = ui.painter().layout_no_wrap(label.into(), reg(16.0), pal().header_primary);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 24.0), Sense::click());
    let cb = Rect::from_min_size(r.min, vec2(24.0, 24.0));
    ui.painter().rect(cb, 4, if *on { pal().blurple } else { egui::Color32::TRANSPARENT }, Stroke::new(2.0, if *on { pal().blurple } else { pal().header_secondary }), egui::StrokeKind::Inside);
    if *on {
        icons::draw(ui.painter(), Icon::Check, cb.shrink(3.0), egui::Color32::WHITE);
    }
    ui.painter().galley(pos2(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
    }
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, h: f32, left: f32, right: f32, focus: bool) -> (egui::Response, Rect) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(r, 8, pal().input_bg);
    let inner = Rect::from_min_max(pos2(r.left() + left, r.top() + (h - 20.0) / 2.0), pos2(r.right() - right, r.bottom() - (h - 20.0) / 2.0));
    let e = ui.put(inner, egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
    ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if e.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
    ui.advance_cursor_after_rect(r);
    if focus {
        e.request_focus();
    }
    (e, r)
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(d) = &mut app.poll else { return };
    let (mut post, mut close) = (false, false);
    let mut pick_emoji = None;
    let mut remove = None;
    let ready = d.ready();
    let modal = crate::ui::motion::Modal::new("poll".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Create a Poll").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(22.0);
            label(ui, "Question");
            let fresh = std::mem::take(&mut d.fresh);
            if d.question.chars().count() > 300 {
                d.question = d.question.chars().take(300).collect();
            }
            field(ui, &mut d.question, "What question do you want to ask?", 46.0, 12.0, 12.0, fresh);
            ui.add_space(4.5);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.label(RichText::new(format!("{} / 300", d.question.chars().count())).font(reg(12.0)).color(pal().text_muted));
            });
            ui.add_space(25.0);
            label(ui, "Answers");
            let many = d.answers.len() > 2;
            for (i, (t, e)) in d.answers.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(4.0);
                }
                if t.chars().count() > 55 {
                    *t = t.chars().take(55).collect();
                }
                let (_, row) = field(ui, t, "Type your answer", 40.0, 44.0, 44.0, false);
                let face = Rect::from_center_size(pos2(row.left() + 28.0, row.center().y), vec2(24.0, 24.0));
                let fr = ui.interact(face, ui.id().with(("poll_emoji", i)), Sense::click());
                match e {
                    Some(em) if em.id != 0 => text::image(ui.painter(), &format!("{}/emojis/{}.png?size=48", crate::api::emoji_cdn(), em.id), face.shrink(3.0)),
                    Some(em) => text::image(ui.painter(), &crate::markdown::twemoji(&em.name), face.shrink(3.0)),
                    None => icons::draw(ui.painter(), Icon::Smile, face.shrink(3.0), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &fr))),
                }
                let at = fr.rect.left_top();
                if fr.on_hover_text("Select Emoji").clicked() {
                    pick_emoji = Some((i, at));
                }
                let tr = Rect::from_center_size(pos2(row.right() - 22.0, row.center().y), vec2(24.0, 24.0));
                let tresp = ui.interact(tr, ui.id().with(("poll_del", i)), Sense::click());
                let col = if many { crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &tresp)) } else { pal().text_muted.gamma_multiply(0.5) };
                icons::draw(ui.painter(), Icon::Trash, tr.shrink(4.0), col);
                if many && tresp.on_hover_text("Remove Answer").clicked() {
                    remove = Some(i);
                }
            }
            ui.add_space(4.0);
            if d.answers.len() < 10 {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(196.0, 40.0), Sense::click());
                    ui.painter().rect(r, 8, crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
                    ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, "Add another answer", medium(16.0), pal().header_primary);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        d.answers.push(Default::default());
                    }
                });
            }
            ui.add_space(16.5);
            label(ui, "Duration");
            let cur = DURATIONS.iter().find(|(h, _)| *h == d.hours).map_or("24 hours", |(_, n)| n);
            let mut hours = d.hours;
            super::settings::dropdown(ui, 432.0, None, cur, |ui| {
                for (h, n) in DURATIONS {
                    if menu_item(ui, (h == hours).then_some(Icon::Check), n, false) {
                        hours = h;
                    }
                }
            });
            d.hours = hours;
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                check_row(ui, &mut d.multi, "Allow Multiple Answers");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = ui.add_enabled_ui(ready, |ui| modal_button(ui, "Post", Some(pal().blurple), 100.0)).inner;
                    super::settings::mark(ui, "poll:post", b.rect);
                    post = b.clicked();
                });
            });
        });
    if let Some(i) = remove {
        d.answers.remove(i);
    }
    if let Some((i, at)) = pick_emoji {
        app.picker = Some((Pick::Poll(i), at - vec2(0.0, 360.0)));
    }
    if post && let Some(d) = app.poll.take() {
        app.rest("POST", format!("/channels/{}/messages", d.ch), Some(d.body()), Tag::fail(super::SEND_FAILED));
        app.jump_to_present(d.ch);
    } else if (close || modal.should_close()) && app.picker.is_none() {
        app.poll = None;
    }
}
