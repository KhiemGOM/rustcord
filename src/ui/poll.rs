use egui::{Align, Align2, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, Pick, button, icon_button, text};
use crate::net::Tag;
use crate::state::{Id, ReactEmoji, make_nonce};
use crate::theme::*;

const DURATIONS: [(u32, &str); 7] = [(1, "1 hour"), (4, "4 hours"), (8, "8 hours"), (24, "24 hours"), (72, "3 days"), (168, "1 week"), (336, "2 weeks")];

pub struct Draft {
    pub ch: Id,
    question: String,
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
    ui.label(RichText::new(s).font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, h: f32, left: f32, right: f32, focus: bool) -> egui::Response {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(r, 8, pal().input_bg);
    let inner = Rect::from_min_max(pos2(r.left() + left, r.top() + (h - 20.0) / 2.0), pos2(r.right() - right, r.bottom() - (h - 20.0) / 2.0));
    let e = ui.put(inner, egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
    ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if e.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
    if focus {
        e.request_focus();
    }
    e
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(d) = &mut app.poll else { return };
    let (mut post, mut close) = (false, false);
    let mut pick_emoji = None;
    let mut remove = None;
    let ready = d.ready();
    let modal = egui::Modal::new("poll".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Create a Poll").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(24.0);
            label(ui, "Question");
            let fresh = std::mem::take(&mut d.fresh);
            if d.question.chars().count() > 300 {
                d.question = d.question.chars().take(300).collect();
            }
            field(ui, &mut d.question, "What question do you want to ask?", 44.0, 12.0, 12.0, fresh);
            ui.add_space(4.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.label(RichText::new(format!("{} / 300", d.question.chars().count())).font(reg(12.0)).color(pal().text_muted));
            });
            ui.add_space(16.0);
            label(ui, "Answers");
            let many = d.answers.len() > 2;
            for (i, (t, e)) in d.answers.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(8.0);
                }
                if t.chars().count() > 55 {
                    *t = t.chars().take(55).collect();
                }
                let r = field(ui, t, "Type your answer", 40.0, 44.0, 44.0, false).rect.expand2(vec2(0.0, 0.0));
                let row = Rect::from_x_y_ranges(r.x_range(), r.y_range());
                let face = Rect::from_center_size(pos2(row.left() + 24.0, row.center().y), vec2(24.0, 24.0));
                let fr = ui.interact(face, ui.id().with(("poll_emoji", i)), Sense::click());
                match e {
                    Some(em) if em.id != 0 => text::image(ui.painter(), &format!("{}/emojis/{}.png?size=48", crate::api::emoji_cdn(), em.id), face.shrink(2.0)),
                    Some(em) => text::image(ui.painter(), &crate::markdown::twemoji(&em.name), face.shrink(2.0)),
                    None => icons::draw(ui.painter(), Icon::Smile, face.shrink(2.0), if fr.hovered() { pal().interactive_hover } else { pal().header_secondary }),
                }
                let at = fr.rect.left_top();
                if fr.on_hover_text("Select Emoji").clicked() {
                    pick_emoji = Some((i, at));
                }
                let tr = Rect::from_center_size(pos2(row.right() - 22.0, row.center().y), vec2(24.0, 24.0));
                let tresp = ui.interact(tr, ui.id().with(("poll_del", i)), Sense::click());
                let col = if many { if tresp.hovered() { pal().interactive_hover } else { pal().header_secondary } } else { pal().text_muted.gamma_multiply(0.5) };
                icons::draw(ui.painter(), Icon::Trash, tr.shrink(2.0), col);
                if many && tresp.on_hover_text("Remove Answer").clicked() {
                    remove = Some(i);
                }
            }
            ui.add_space(8.0);
            if d.answers.len() < 10 {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(166.0, 40.0), Sense::click());
                    ui.painter().rect(r, 8, if resp.hovered() { pal().control_hover } else { pal().control }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
                    ui.painter().text(pos2(r.left() + 38.0, r.center().y), Align2::LEFT_CENTER, "Add another answer", medium(14.0), pal().header_primary);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        d.answers.push(Default::default());
                    }
                });
            }
            ui.add_space(16.0);
            label(ui, "Duration");
            let cur = DURATIONS.iter().find(|(h, _)| *h == d.hours).map_or("24 hours", |(_, n)| n);
            egui::ComboBox::from_id_salt("poll_duration").selected_text(cur).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                for (h, n) in DURATIONS {
                    ui.selectable_value(&mut d.hours, h, n);
                }
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut d.multi, RichText::new("Allow Multiple Answers").font(reg(16.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = ui.add_enabled_ui(ready, |ui| button(ui, "Post", BLURPLE, 100.0)).inner;
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
        app.rest("POST", format!("/channels/{}/messages", d.ch), Some(d.body()), Tag::Quiet);
        app.jump_to_present(d.ch);
    } else if (close || modal.should_close()) && app.picker.is_none() {
        app.poll = None;
    }
}
