use egui::{Align, Frame, Key, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use super::icons::Icon;
use super::selectmenu::{self, Opt, Spec};
use super::{App, button, grey_button, icon_button};
use crate::state::{Id, make_nonce, sid};
use crate::theme::*;

pub struct Modal {
    pub id: String,
    pub channel: Id,
    pub custom_id: String,
    pub app: Id,
    pub title: String,
    pub fields: Vec<Field>,
    pub busy: bool,
    pub error: Option<String>,
    tried: bool,
}

pub enum Field {
    Text(String),
    Input(Input),
    Select(Select),
}

pub struct Input {
    pub label: String,
    pub desc: Option<String>,
    pub custom_id: String,
    pub long: bool,
    pub min: usize,
    pub max: usize,
    pub required: bool,
    pub placeholder: String,
    pub value: String,
    pub wrapped: bool,
}

pub struct Select {
    pub label: String,
    pub desc: Option<String>,
    pub custom_id: String,
    pub kind: u8,
    pub placeholder: String,
    pub options: Vec<Opt>,
    pub min: usize,
    pub max: usize,
    pub required: bool,
    pub picked: Vec<String>,
    pub types: Vec<u8>,
}

fn text_of(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_string()
}

fn count(v: &Value, k: &str, d: usize) -> usize {
    v[k].as_u64().map_or(d, |n| n as usize)
}

fn leaf(c: &Value, label: String, desc: Option<String>, wrapped: bool) -> Option<Field> {
    match c["type"].as_u64()? {
        4 => Some(Field::Input(Input {
            label,
            desc,
            custom_id: text_of(c, "custom_id"),
            long: c["style"].as_u64() == Some(2),
            min: count(c, "min_length", 0),
            max: count(c, "max_length", 4000),
            required: c["required"].as_bool().unwrap_or(true),
            placeholder: text_of(c, "placeholder"),
            value: text_of(c, "value"),
            wrapped,
        })),
        k @ (3 | 5..=8) => {
            let options: Vec<Opt> = c["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|o| Opt { label: text_of(o, "label"), value: text_of(o, "value"), desc: o["description"].as_str().map(str::to_string), lead: selectmenu::Lead::None })
                .collect();
            let picked = c["options"].as_array().into_iter().flatten().filter(|o| o["default"].as_bool() == Some(true)).map(|o| text_of(o, "value")).collect();
            Some(Field::Select(Select {
                label,
                desc,
                custom_id: text_of(c, "custom_id"),
                kind: k as u8,
                placeholder: c["placeholder"].as_str().unwrap_or("Make a selection").to_string(),
                options,
                min: count(c, "min_values", 1),
                max: count(c, "max_values", 1),
                required: c["required"].as_bool().unwrap_or(true),
                picked,
                types: c["channel_types"].as_array().into_iter().flatten().filter_map(|t| t.as_u64().map(|t| t as u8)).collect(),
            }))
        }
        _ => None,
    }
}

impl Modal {
    pub fn open(d: &Value) -> Option<Modal> {
        let mut fields = Vec::new();
        for c in d["components"].as_array()? {
            match c["type"].as_u64() {
                Some(1) => {
                    for inner in c["components"].as_array().into_iter().flatten() {
                        fields.extend(leaf(inner, text_of(inner, "label"), None, false));
                    }
                }
                Some(18) => fields.extend(leaf(&c["component"], text_of(c, "label"), c["description"].as_str().map(str::to_string), true)),
                Some(10) => fields.push(Field::Text(text_of(c, "content"))),
                _ => {}
            }
        }
        Some(Modal {
            id: text_of(d, "id"),
            channel: sid(&d["channel_id"]),
            custom_id: text_of(d, "custom_id"),
            app: sid(&d["application"]["id"]),
            title: text_of(d, "title"),
            fields,
            busy: false,
            error: None,
            tried: false,
        })
    }

    pub fn invalid(&self) -> usize {
        if self.tried { self.errors().iter().flatten().count() } else { 0 }
    }

    pub fn failed(&mut self) {
        self.busy = false;
        self.error = Some("Something went wrong. Try again.".into());
    }

    fn errors(&self) -> Vec<Option<String>> {
        self.fields
            .iter()
            .map(|f| match f {
                Field::Text(_) => None,
                Field::Input(i) => {
                    let n = i.value.chars().count();
                    if n == 0 {
                        i.required.then(|| "This field is required.".to_string())
                    } else if n < i.min || n > i.max {
                        Some(format!("Must be between {} and {} in length.", i.min, i.max))
                    } else {
                        None
                    }
                }
                Field::Select(s) => {
                    if s.picked.is_empty() && !s.required {
                        None
                    } else {
                        selectmenu::validate(s.picked.len(), s.min, s.max)
                    }
                }
            })
            .collect()
    }

    fn components(&self) -> Vec<Value> {
        self.fields
            .iter()
            .map(|f| match f {
                Field::Text(_) => json!({ "type": 10 }),
                Field::Input(i) => {
                    let inner = json!({ "type": 4, "custom_id": i.custom_id, "value": i.value });
                    if i.wrapped { json!({ "type": 18, "component": inner }) } else { json!({ "type": 1, "components": [inner] }) }
                }
                Field::Select(s) => {
                    let inner = json!({ "type": s.kind, "custom_id": s.custom_id, "values": s.picked });
                    json!({ "type": 18, "component": inner })
                }
            })
            .collect()
    }
}

fn label(ui: &mut Ui, text: &str, required: bool, desc: &Option<String>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(text).font(semi(12.0)).color(pal().header_secondary));
        if required {
            ui.label(RichText::new("*").font(semi(12.0)).color(RED));
        }
    });
    if let Some(d) = desc {
        ui.label(RichText::new(d).font(reg(12.0)).color(pal().text_muted));
    }
    ui.add_space(8.0);
}

fn error_line(ui: &mut Ui, e: &Option<String>) {
    if let Some(e) = e {
        ui.add_space(4.0);
        ui.label(RichText::new(e).font(reg(12.0)).color(pal().critical));
    }
}

fn field(ui: &mut Ui, store: &crate::state::Store, guild: Id, m_channel: Id, i: usize, f: &mut Field, err: &Option<String>, tried: bool, submit: &mut bool) {
    let shown = if tried { err } else { &None };
    match f {
        Field::Text(t) => {
            ui.label(RichText::new(t.as_str()).font(reg(14.0)).color(pal().text_normal));
        }
        Field::Input(inp) => {
            label(ui, &inp.label, inp.required, &inp.desc);
            let w = ui.available_width();
            let h = if inp.long { 104.0 } else { 40.0 };
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().rect_filled(r, 8, pal().input_bg);
            let id = egui::Id::new(("bot_modal_input", i));
            let e = if inp.long {
                egui::TextEdit::multiline(&mut inp.value).desired_rows(4)
            } else {
                egui::TextEdit::singleline(&mut inp.value)
            };
            let e = ui.put(
                r.shrink2(vec2(12.0, 10.0)),
                e.id(id)
                    .hint_text(RichText::new(&inp.placeholder).color(pal().text_muted))
                    .font(reg(16.0))
                    .char_limit(inp.max.max(1))
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO),
            );
            super::settings::mark(ui, &format!("bot_input:{}", inp.custom_id), r);
            let bad = shown.is_some();
            ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if bad { RED } else if e.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
            if !inp.long && e.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                *submit = true;
            }
            error_line(ui, shown);
        }
        Field::Select(s) => {
            label(ui, &s.label, s.required, &s.desc);
            let id = egui::Id::new(("bot_modal_select", i));
            let spec = Spec {
                kind: s.kind,
                placeholder: &s.placeholder,
                min: s.min,
                max: s.max,
                disabled: false,
                loading: false,
                options: &s.options,
                channel_types: &s.types,
                guild,
                channel: m_channel,
                width: ui.available_width(),
                mark: &format!("bot_select:{}", s.custom_id),
            };
            selectmenu::show(ui, store, id, &spec, &mut s.picked);
            error_line(ui, shown);
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut m) = app.interactions.modal.take() else { return };
    let guild = app.store.channels.get(&m.channel).map_or(0, |c| c.guild);
    let errors = m.errors();
    let (mut close, mut submit) = (false, false);
    let shown = super::motion::Modal::new("bot_modal".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&m.title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(20.0);
            let tried = m.tried;
            egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.6).id_salt("bot_modal_scroll").show(ui, |ui| {
                for (i, f) in m.fields.iter_mut().enumerate() {
                    if i > 0 {
                        ui.add_space(16.0);
                    }
                    field(ui, &app.store, guild, m.channel, i, f, &errors[i], tried, &mut submit);
                }
            });
            ui.add_space(24.0);
            if let Some(e) = &m.error {
                ui.label(RichText::new(e).font(reg(14.0)).color(pal().critical));
                ui.add_space(12.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = ui.add_enabled_ui(!m.busy, |ui| button(ui, "Submit", BLURPLE, 96.0)).inner;
                super::settings::mark(ui, "bot_modal:submit", b.rect);
                submit |= b.clicked();
                ui.add_space(8.0);
                close |= grey_button(ui, "Cancel", 92.0).clicked();
            });
        });
    if close || shown.should_close() {
        return;
    }
    if submit && !m.busy {
        m.tried = true;
        m.error = None;
        if errors.iter().all(Option::is_none) {
            let nonce = make_nonce();
            let mut payload = json!({
                "type": 5,
                "application_id": m.app.to_string(),
                "channel_id": m.channel.to_string(),
                "data": { "id": m.id, "custom_id": m.custom_id, "components": m.components() },
                "session_id": app.store.session,
                "nonce": nonce,
            });
            if guild != 0 {
                payload["guild_id"] = json!(guild.to_string());
            }
            m.busy = true;
            app.submit_modal(payload, m.channel);
        }
    }
    app.interactions.modal = Some(m);
}
