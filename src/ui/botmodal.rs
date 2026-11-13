use egui::{Align, Frame, Key, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};
use std::path::PathBuf;

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
    Choice(Choice),
    Files(Files),
}

/// A radio group (21), a checkbox group (22) or a lone checkbox (23): rows you tick.
pub struct Choice {
    pub label: String,
    pub desc: Option<String>,
    pub custom_id: String,
    pub kind: u8,
    pub options: Vec<Opt>,
    pub min: usize,
    pub max: usize,
    pub required: bool,
    pub picked: Vec<String>,
}

/// A file upload (19): the files picked here go up with the interaction, listed under `attachments`.
pub struct Files {
    pub label: String,
    pub desc: Option<String>,
    pub custom_id: String,
    pub min: usize,
    pub max: usize,
    pub required: bool,
    pub types: Vec<String>,
    pub picked: Vec<PathBuf>,
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
        19 => Some(Field::Files(Files {
            label,
            desc,
            custom_id: text_of(c, "custom_id"),
            min: count(c, "min_values", 1),
            max: count(c, "max_values", 1),
            required: c["required"].as_bool().unwrap_or(false),
            types: c["file_types"].as_array().into_iter().flatten().filter_map(|t| t.as_str().map(str::to_string)).collect(),
            picked: Vec::new(),
        })),
        k @ (21..=23) => {
            let options: Vec<Opt> = if k == 23 {
                vec![Opt { label: label.clone(), value: "true".into(), desc: None, lead: selectmenu::Lead::None }]
            } else {
                c["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| Opt { label: text_of(o, "label"), value: text_of(o, "value"), desc: o["description"].as_str().map(str::to_string), lead: selectmenu::Lead::None })
                    .collect()
            };
            let picked = if k == 23 {
                c["default"].as_bool().unwrap_or(false).then(|| "true".to_string()).into_iter().collect()
            } else {
                c["options"].as_array().into_iter().flatten().filter(|o| o["default"].as_bool() == Some(true)).map(|o| text_of(o, "value")).collect()
            };
            Some(Field::Choice(Choice {
                label: if k == 23 { String::new() } else { label },
                desc,
                custom_id: text_of(c, "custom_id"),
                kind: k as u8,
                options,
                min: if k == 22 { count(c, "min_values", 1) } else { 0 },
                max: if k == 22 { count(c, "max_values", options_len(c)) } else { 1 },
                required: if k == 22 { count(c, "min_values", 1) > 0 } else { c["required"].as_bool().unwrap_or(k == 21) },
                picked,
            }))
        }
        _ => None,
    }
}

fn options_len(c: &Value) -> usize {
    c["options"].as_array().map_or(1, Vec::len)
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

    /// A file dropped on the window while the form is up: the first upload that has room takes it.
    pub fn drop_file(&mut self, path: &std::path::Path) -> bool {
        let slot = self.fields.iter_mut().find_map(|f| match f {
            Field::Files(f) if f.picked.len() < f.max => Some(f),
            _ => None,
        });
        slot.is_some_and(|f| {
            f.picked.push(path.to_path_buf());
            true
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
                Field::Choice(c) if c.picked.is_empty() && !c.required => None,
                Field::Choice(c) if c.kind == 21 => c.picked.is_empty().then(|| "This field is required.".to_string()),
                Field::Choice(c) => selectmenu::validate(c.picked.len(), c.min, c.max),
                Field::Files(f) if f.picked.is_empty() => f.required.then(|| "This field is required.".to_string()),
                Field::Files(f) if f.picked.len() < f.min => Some(format!("Upload at least {} files.", f.min)),
                Field::Files(f) => (f.picked.len() > f.max).then(|| format!("Upload at most {} files.", f.max)),
            })
            .collect()
    }

    /// The components as they go out, and the files they refer to by index.
    fn components(&self) -> (Vec<Value>, Vec<Value>) {
        let mut files = Vec::new();
        let comps = self
            .fields
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
                Field::Choice(c) => {
                    let inner = match c.kind {
                        21 => json!({ "type": 21, "custom_id": c.custom_id, "value": c.picked.first() }),
                        22 => json!({ "type": 22, "custom_id": c.custom_id, "values": c.picked }),
                        _ => json!({ "type": 23, "custom_id": c.custom_id, "value": !c.picked.is_empty() }),
                    };
                    json!({ "type": 18, "component": inner })
                }
                Field::Files(f) => {
                    let ids: Vec<String> = f
                        .picked
                        .iter()
                        .map(|p| {
                            let id = files.len().to_string();
                            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            files.push(json!({ "id": id, "filename": name, "path": p.to_string_lossy() }));
                            id
                        })
                        .collect();
                    json!({ "type": 18, "component": { "type": 19, "custom_id": f.custom_id, "values": ids } })
                }
            })
            .collect();
        (comps, files)
    }
}

fn label(ui: &mut Ui, text: &str, required: bool, desc: &Option<String>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(text).font(semi(12.0)).color(pal().header_secondary));
        if required {
            ui.label(RichText::new("*").font(semi(12.0)).color(pal().red));
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
            ui.painter().rect_stroke(r, 8, Stroke::new(1.0, if bad { pal().red } else if e.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
            ui.advance_cursor_after_rect(r);
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
        Field::Files(f) => {
            label(ui, &f.label, f.required, &f.desc);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
            super::settings::mark(ui, &format!("bot_files:{}", f.custom_id), r);
            let over = ui.rect_contains_pointer(r);
            ui.painter().rect(r, 8, pal().input_bg, Stroke::new(1.0, if shown.is_some() { pal().red } else if over { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
            super::icons::draw(ui.painter(), Icon::Upload, egui::Rect::from_center_size(egui::pos2(r.center().x, r.top() + 24.0), vec2(20.0, 20.0)), pal().header_secondary);
            let hint = match f.types.as_slice() {
                [] => "Drag and drop or click to upload file".to_string(),
                t => format!("Drag and drop or click to upload {} file", t.iter().map(|t| t.trim_start_matches('.').to_uppercase()).collect::<Vec<_>>().join(", ")),
            };
            ui.painter().text(egui::pos2(r.center().x, r.top() + 50.0), egui::Align2::CENTER_CENTER, hint, reg(14.0), pal().header_secondary);
            if over {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let mut added: Vec<PathBuf> = Vec::new();
            if resp.clicked() && f.picked.len() < f.max {
                let mut dialog = rfd::FileDialog::new();
                let exts: Vec<&str> = f.types.iter().map(|t| t.trim_start_matches('.')).filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric())).collect();
                if !exts.is_empty() {
                    dialog = dialog.add_filter("Files", &exts);
                }
                added.extend(if f.max > 1 { dialog.pick_files().unwrap_or_default() } else { dialog.pick_file().into_iter().collect() });
            }
            for p in added {
                if f.picked.len() < f.max && !f.picked.contains(&p) {
                    f.picked.push(p);
                }
            }
            let mut gone = None;
            for (n, p) in f.picked.iter().enumerate() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    super::icons::draw(ui.painter(), Icon::File, egui::Rect::from_min_size(ui.cursor().min, vec2(20.0, 20.0)), pal().header_secondary);
                    ui.add_space(24.0);
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.label(RichText::new(name).font(reg(14.0)).color(pal().text_normal));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let x = icon_button(ui, Icon::Close, 24.0, "Remove");
                        super::settings::mark(ui, &format!("bot_file_x:{}:{n}", f.custom_id), x.rect);
                        if x.clicked() {
                            gone = Some(n);
                        }
                    });
                });
            }
            if let Some(n) = gone {
                f.picked.remove(n);
            }
            error_line(ui, shown);
        }
        Field::Choice(c) => {
            if !c.label.is_empty() {
                label(ui, &c.label, c.required, &c.desc);
            }
            for (n, o) in c.options.iter().enumerate() {
                if n > 0 {
                    ui.add_space(8.0);
                }
                let on = c.picked.contains(&o.value);
                if choice_row(ui, c.kind == 21, on, o, &format!("bot_choice:{}:{}", c.custom_id, o.value)) {
                    match c.kind {
                        21 => c.picked = vec![o.value.clone()],
                        _ if on => c.picked.retain(|v| *v != o.value),
                        _ if c.kind == 22 && c.picked.len() >= c.max => {}
                        _ => c.picked.push(o.value.clone()),
                    }
                }
            }
            error_line(ui, shown);
        }
    }
}

fn choice_row(ui: &mut Ui, round: bool, on: bool, o: &Opt, mark: &str) -> bool {
    let g = ui.painter().layout_no_wrap(o.label.clone(), reg(16.0), pal().header_primary);
    let h = if o.desc.is_some() { 44.0 } else { 24.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    super::settings::mark(ui, mark, r);
    let cb = egui::Rect::from_min_size(r.min, vec2(24.0, 24.0));
    let ink = if on { pal().blurple } else { pal().header_secondary };
    if round {
        ui.painter().circle_stroke(cb.center(), 10.0, Stroke::new(2.0, ink));
        if on {
            ui.painter().circle_filled(cb.center(), 5.0, pal().blurple);
        }
    } else {
        ui.painter().rect(cb, 4, if on { pal().blurple } else { egui::Color32::TRANSPARENT }, Stroke::new(2.0, ink), egui::StrokeKind::Inside);
        if on {
            super::icons::draw(ui.painter(), Icon::Check, cb.shrink(3.0), egui::Color32::WHITE);
        }
    }
    ui.painter().galley(egui::pos2(r.left() + 36.0, r.top() + 12.0 - g.size().y / 2.0), g, pal().header_primary);
    if let Some(d) = &o.desc {
        ui.painter().text(egui::pos2(r.left() + 36.0, r.top() + 34.0), egui::Align2::LEFT_CENTER, d, reg(12.0), pal().text_muted);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut m) = app.interactions.modal.take() else { return };
    let guild = app.store.channels.get(&m.channel).map_or(0, |c| c.guild);
    let errors = m.errors();
    let dropped = (0..m.fields.len()).any(|i| ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("bot_modal_select", i)).with("open")).unwrap_or(false)));
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
            let body = egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.6).id_salt("bot_modal_scroll").show(ui, |ui| {
                for (i, f) in m.fields.iter_mut().enumerate() {
                    if i > 0 {
                        ui.add_space(16.0);
                    }
                    field(ui, &app.store, guild, m.channel, i, f, &errors[i], tried, &mut submit);
                }
            });
            super::settings::mark(ui, "bot_modal:scroll", body.inner_rect);
            ui.add_space(24.0);
            if let Some(e) = &m.error {
                ui.label(RichText::new(e).font(reg(14.0)).color(pal().critical));
                ui.add_space(12.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = ui.add_enabled_ui(!m.busy, |ui| button(ui, "Submit", pal().blurple, 96.0)).inner;
                super::settings::mark(ui, "bot_modal:submit", b.rect);
                submit |= b.clicked();
                ui.add_space(8.0);
                close |= grey_button(ui, "Cancel", 92.0).clicked();
            });
        });
    if close || (shown.should_close() && !dropped) {
        return;
    }
    if submit && !m.busy {
        m.tried = true;
        m.error = None;
        if errors.iter().all(Option::is_none) {
            let nonce = make_nonce();
            let (components, attachments) = m.components();
            let mut data = json!({ "id": m.id, "custom_id": m.custom_id, "components": components });
            if !attachments.is_empty() {
                data["attachments"] = json!(attachments);
            }
            let mut payload = json!({
                "type": 5,
                "application_id": m.app.to_string(),
                "channel_id": m.channel.to_string(),
                "data": data,
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
