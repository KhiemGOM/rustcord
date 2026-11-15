use egui::{Align, Align2, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, Pick, menu_item, modal_button, text};
use crate::net::Tag;
use crate::state::{Id, ReactEmoji, make_nonce};
use crate::theme::*;

const NO_QUESTION: &str = "Oops, you forgot something. Please add a question.";
const NO_ANSWER: &str = "Oops, you forgot something. Please add at least one answer.";
const NO_TEXT: &str = "Don\u{2019}t leave the answer field hanging! Please add some text.";

const DURATIONS: [(u32, &str); 7] = [(1, "1 hour"), (4, "4 hours"), (8, "8 hours"), (24, "24 hours"), (72, "3 days"), (168, "1 week"), (336, "2 weeks")];

pub struct Draft {
    pub ch: Id,
    pub question: String,
    pub answers: Vec<(String, Option<ReactEmoji>)>,
    hours: u32,
    multi: bool,
    fresh: bool,
    q_err: Option<&'static str>,
    a_errs: Vec<Option<&'static str>>,
}

impl Draft {
    pub fn new(ch: Id) -> Self {
        Draft { ch, question: String::new(), answers: vec![Default::default(), Default::default()], hours: 24, multi: false, fresh: true, q_err: None, a_errs: vec![None, None] }
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

    fn check(&mut self) -> bool {
        self.q_err = self.question.trim().is_empty().then_some(NO_QUESTION);
        let none = self.answers.iter().all(|(t, _)| t.trim().is_empty());
        self.a_errs = self
            .answers
            .iter()
            .enumerate()
            .map(|(i, (t, e))| if e.is_some() && t.trim().is_empty() { Some(NO_TEXT) } else if i == 0 && none { Some(NO_ANSWER) } else { None })
            .collect();
        self.q_err.is_none() && self.a_errs.iter().all(Option::is_none)
    }

    pub fn errors(&self) -> Vec<&'static str> {
        self.q_err.iter().chain(self.a_errs.iter().flatten()).copied().collect()
    }

    fn pristine(&self) -> bool {
        self.question.is_empty() && self.answers.iter().all(|(t, _)| t.trim().is_empty())
    }
}

fn label(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(7.5);
}

fn error(ui: &Ui, at: egui::Pos2, msg: &str) {
    let c = pos2(at.x + 8.0, at.y + 8.0);
    let p = ui.painter();
    p.circle_filled(c, 6.7, pal().critical);
    p.line_segment([pos2(c.x, c.y - 3.4), pos2(c.x, c.y + 0.6)], Stroke::new(1.7, egui::Color32::WHITE));
    p.circle_filled(pos2(c.x, c.y + 3.1), 1.0, egui::Color32::WHITE);
    p.text(pos2(at.x + 20.0, c.y), Align2::LEFT_CENTER, msg, reg(12.0), pal().critical);
}

pub fn check_row(ui: &mut Ui, on: &mut bool, label: &str) {
    let g = ui.painter().layout_no_wrap(label.into(), reg(16.0), pal().header_primary);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 24.0), Sense::click());
    let cb = Rect::from_min_size(pos2(r.left(), r.center().y - 10.0), vec2(20.0, 20.0));
    if *on {
        ui.painter().rect_filled(cb, 4, pal().blurple);
        icons::draw(ui.painter(), Icon::Check, cb.shrink(2.0), egui::Color32::WHITE);
    } else {
        ui.painter().rect(cb, 4, egui::Color32::from_black_alpha(20), Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(151, 151, 159, 163)), egui::StrokeKind::Inside);
    }
    ui.painter().galley(pos2(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
    }
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, h: f32, left: f32, right: f32, focus: bool, err: bool) -> (egui::Response, Rect) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(r, 8, if err { egui::Color32::from_rgba_unmultiplied(204, 51, 51, 10) } else { pal().input_bg });
    let inner = Rect::from_min_max(pos2(r.left() + left, r.top() + (h - 20.0) / 2.0), pos2(r.right() - right, r.bottom() - (h - 20.0) / 2.0));
    let e = ui.put(inner, egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
    let edge = if err { pal().critical } else if e.has_focus() { pal().blurple } else { pal().border_subtle };
    ui.painter().rect_stroke(r, 8, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
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
    let modal = crate::ui::motion::Modal::new("poll".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(430.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = ui.max_rect().top();
            ui.label(RichText::new("Create a Poll").font(semi(20.0)).color(pal().header_primary));
            let x = Rect::from_min_size(pos2(ui.max_rect().right() - 24.0, top - 16.0), vec2(40.0, 40.0));
            let xr = ui.interact(x, ui.id().with("poll_close"), Sense::click());
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &xr)));
            close |= xr.on_hover_text("Close").clicked();
            ui.add_space(22.0);
            label(ui, "Question");
            let fresh = std::mem::take(&mut d.fresh);
            if d.question.chars().count() > 300 {
                d.question = d.question.chars().take(300).collect();
            }
            if field(ui, &mut d.question, "What question do you want to ask?", 48.0, 12.0, 12.0, fresh, d.q_err.is_some()).0.changed() {
                d.q_err = None;
            }
            ui.add_space(4.0);
            let (row, _) = ui.allocate_exact_size(vec2(430.0, 16.0), Sense::hover());
            if let Some(m) = d.q_err {
                error(ui, row.min, m);
            }
            ui.painter().text(row.right_center(), Align2::RIGHT_CENTER, format!("{} / 300", d.question.chars().count()), reg(12.0), pal().text_muted);
            ui.add_space(25.0);
            label(ui, "Answers");
            let many = d.answers.len() > 1;
            d.a_errs.resize(d.answers.len(), None);
            for (i, (t, e)) in d.answers.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(4.0);
                }
                if t.chars().count() > 55 {
                    *t = t.chars().take(55).collect();
                }
                let (er, row) = field(ui, t, "Type your answer", 40.0, 44.0, 44.0, false, d.a_errs[i].is_some());
                if er.changed() {
                    d.a_errs[i] = None;
                }
                if let Some(m) = d.a_errs[i] {
                    ui.add_space(4.0);
                    let (r, _) = ui.allocate_exact_size(vec2(430.0, 16.0), Sense::hover());
                    error(ui, r.min, m);
                }
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
                let tr = Rect::from_center_size(pos2(row.right() - 21.0, row.center().y), vec2(24.0, 24.0));
                let tresp = ui.interact(tr, ui.id().with(("poll_del", i)), Sense::click());
                let col = if many { crate::ui::motion::mix(pal().header_primary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &tresp)) } else { pal().text_muted.gamma_multiply(0.5) };
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
                    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(r.left() + 26.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
                    ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, "Add another answer", medium(16.0), pal().header_primary);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        d.answers.push(Default::default());
                        d.a_errs.push(None);
                    }
                });
            }
            ui.add_space(16.5);
            label(ui, "Duration");
            let cur = DURATIONS.iter().find(|(h, _)| *h == d.hours).map_or("24 hours", |(_, n)| n);
            let mut hours = d.hours;
            super::settings::dropdown(ui, 430.0, None, cur, |ui| {
                for (h, n) in DURATIONS {
                    if menu_item(ui, (h == hours).then_some(Icon::Check), n, false) {
                        hours = h;
                    }
                }
            });
            d.hours = hours;
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.set_min_height(40.0);
                check_row(ui, &mut d.multi, "Allow Multiple Answers");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = modal_button(ui, "Post", Some(pal().blurple), 100.0);
                    super::settings::mark(ui, "poll:post", b.rect);
                    post = b.clicked();
                });
            });
        });
    if let Some(i) = remove {
        d.answers.remove(i);
        if i < d.a_errs.len() {
            d.a_errs.remove(i);
        }
    }
    let post = post && d.check();
    let pristine = d.pristine();
    if let Some((i, at)) = pick_emoji {
        app.picker = Some((Pick::Poll(i), at - vec2(0.0, 360.0)));
    }
    if post && let Some(d) = app.poll.take() {
        app.rest("POST", format!("/channels/{}/messages", d.ch), Some(d.body()), Tag::fail(super::SEND_FAILED));
        app.jump_to_present(d.ch);
    } else if (close || modal.should_close()) && app.picker.is_none() && app.confirm.is_none() {
        if pristine {
            app.poll = None;
        } else {
            let body = "Are you sure you want to exit? Any changes made will be lost if you exit now.";
            app.confirm = Some(super::Confirm::new("Are you sure?", body, "Exit", |app: &mut App| app.poll = None).cancel("Back to editing"));
        }
    }
}
