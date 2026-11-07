//! Report Message, the way discord's own client runs it: a menu the server sends (`/reporting/menu/message`)
//! as a graph of nodes, each with a header, choices, inputs and a button, walked one step at a time and
//! submitted with the path taken.

use egui::{Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Sense, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, button, grey_button, icon_button};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

pub struct Report {
    ch: Id,
    msg: Id,
    menu: Option<Value>,
    cur: String,
    path: Vec<Step>,
    inputs: Vec<(String, Value)>,
    busy: bool,
    err: Option<String>,
}

struct Step {
    node: String,
    inputs: Vec<(String, Value)>,
}

impl Report {
    /// On the page that follows a submitted report.
    pub fn done(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| m["success_node_id"] == self.cur.as_str())
    }

    pub fn new(ch: Id, msg: Id) -> Self {
        Report { ch, msg, menu: None, cur: String::new(), path: Vec::new(), inputs: Vec::new(), busy: false, err: None }
    }

    fn node(&self, id: &str) -> Option<&Value> {
        self.menu.as_ref().map(|m| &m["nodes"][id]).filter(|n| n.is_object())
    }

    fn input(&self, name: &str) -> Option<&Value> {
        self.inputs.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    fn set(&mut self, name: &str, v: Value) {
        match self.inputs.iter_mut().find(|(n, _)| n == name) {
            Some(e) => e.1 = v,
            None => self.inputs.push((name.to_string(), v)),
        }
    }

    fn enter(&mut self, id: &str) {
        let skip = self.node(id).filter(|n| n["elements"].as_array().is_some_and(|e| e.iter().any(|e| e["type"] == "skip")) && n["button"]["type"] == "next").and_then(|n| n["button"]["target"].as_str().map(str::to_string));
        self.path.push(Step { node: self.cur.clone(), inputs: std::mem::take(&mut self.inputs) });
        self.cur = id.to_string();
        self.err = None;
        if let Some(t) = skip {
            self.enter(&t);
        }
    }

    fn back(&mut self) {
        if let Some(s) = self.path.pop() {
            self.cur = s.node;
            self.inputs = s.inputs;
            self.err = None;
        }
    }

    fn body(&self) -> Value {
        let menu = self.menu.clone().unwrap_or_default();
        let mut elements = serde_json::Map::new();
        for (n, v) in self.path.iter().flat_map(|s| s.inputs.iter()).chain(self.inputs.iter()) {
            elements.insert(n.clone(), v.clone());
        }
        let mut crumbs: Vec<String> = self.path.iter().map(|s| s.node.clone()).collect();
        crumbs.push(self.cur.clone());
        json!({
            "version": menu["version"],
            "variant": menu["variant"],
            "language": menu["language"].as_str().unwrap_or("en"),
            "breadcrumbs": crumbs,
            "elements": elements,
            "name": "message",
            "channel_id": self.ch.to_string(),
            "message_id": self.msg.to_string(),
        })
    }
}

impl App {
    pub fn open_report(&mut self, ch: Id, msg: Id) {
        self.report = Some(Report::new(ch, msg));
        self.rest("GET", "/reporting/menu/message".into(), None, Tag::Report(false));
    }

    pub fn report_loaded(&mut self, submit: bool, v: Value) {
        let Some(r) = &mut self.report else { return };
        r.busy = false;
        if submit {
            let done = r.menu.as_ref().and_then(|m| m["success_node_id"].as_str()).map(str::to_string);
            if let Some(d) = done {
                r.enter(&d);
            }
            return;
        }
        let root = v["root_node_id"].as_str().unwrap_or("").to_string();
        r.menu = Some(v);
        r.cur = root.clone();
        if r.node(&root).is_none() {
            r.err = Some("This report couldn't be loaded.".into());
        }
    }

    pub fn report_failed(&mut self, e: String) {
        if let Some(r) = &mut self.report {
            r.busy = false;
            r.err = Some(e);
        }
    }
}

fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s.replace("**", "");
    while let Some(i) = rest.find('[') {
        let Some(j) = rest[i..].find("](").map(|j| i + j) else { break };
        let Some(k) = rest[j..].find(')').map(|k| j + k) else { break };
        out.push_str(&rest[..i]);
        out.push_str(&rest[i + 1..j]);
        rest = rest[k + 1..].to_string();
    }
    out.push_str(&rest);
    out
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(r) = &mut app.report else { return };
    let mut close = false;
    let mut go: Option<String> = None;
    let mut back = false;
    let mut submit = false;
    let store = &app.store;
    let preview = store.find_msg(r.ch, r.msg).cloned();
    let modal = crate::ui::motion::Modal::new("report".into())
        .frame(Frame::new().fill(pal().bg_floating).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            Frame::new().inner_margin(Margin { left: 24, right: 16, top: 24, bottom: 8 }).show(ui, |ui| {
                ui.set_width(400.0);
                ui.horizontal(|ui| {
                    let head = r.node(&r.cur).and_then(|n| n["header"].as_str()).unwrap_or("Report Message").to_string();
                    ui.label(RichText::new(plain(&head)).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
            let Some(node) = r.node(&r.cur).cloned() else {
                Frame::new().inner_margin(Margin::symmetric(24, 24)).show(ui, |ui| {
                    ui.set_width(392.0);
                    match &r.err {
                        Some(e) => ui.label(RichText::new(e).font(reg(14.0)).color(pal().critical)),
                        None => crate::ui::motion::spinner(ui, 24.0, pal().text_muted),
                    };
                });
                return;
            };
            egui::ScrollArea::vertical().max_height(440.0).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 0)).show(ui, |ui| {
                    ui.set_width(392.0);
                    if let Some(s) = node["subheader"].as_str().filter(|s| !s.is_empty()) {
                        ui.label(RichText::new(plain(s)).font(reg(14.0)).color(pal().header_secondary));
                        ui.add_space(12.0);
                    }
                    for e in node["elements"].as_array().into_iter().flatten() {
                        let name = e["name"].as_str().unwrap_or("").to_string();
                        match e["type"].as_str().unwrap_or("") {
                            "message_preview" => {
                                if let Some(m) = &preview {
                                    super::chat::preview(ui, store, m);
                                    ui.add_space(12.0);
                                }
                            }
                            "inline_notice" => {
                                let body = e["data"]["body"].as_str().unwrap_or("");
                                Frame::new().fill(pal().bg_secondary).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                                    ui.set_width(368.0);
                                    ui.label(RichText::new(plain(body)).font(reg(14.0)).color(pal().text_normal));
                                });
                                ui.add_space(12.0);
                            }
                            "text_line_resource" => {
                                if let Some(h) = e["data"]["header"].as_str() {
                                    ui.label(RichText::new(plain(h)).font(semi(14.0)).color(pal().header_primary));
                                }
                                if let Some(b) = e["data"]["body"].as_str() {
                                    ui.label(RichText::new(plain(b)).font(reg(14.0)).color(pal().header_secondary));
                                }
                                ui.add_space(12.0);
                            }
                            "checkbox" => {
                                let mut picked: Vec<String> = r.input(&name).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                                for (n, opt) in e["data"].as_array().into_iter().flatten().enumerate() {
                                    let id = opt[0].as_str().unwrap_or("");
                                    let label = opt[1].as_str().unwrap_or("");
                                    let on = picked.iter().any(|p| p == id);
                                    let (rect, resp) = ui.allocate_exact_size(vec2(392.0, if opt[2].is_string() { 52.0 } else { 40.0 }), Sense::click());
                                    super::settings::mark(ui, &format!("report_check{n}"), rect);
                                    crate::ui::motion::wash(ui, &resp, rect, CornerRadius::same(4), MENU_HOVER, 0.05);
                                    let b = egui::Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), vec2(20.0, 20.0));
                                    if on {
                                        ui.painter().rect_filled(b, 4, BLURPLE);
                                        icons::draw(ui.painter(), Icon::Check, b.shrink(3.0), egui::Color32::WHITE);
                                    } else {
                                        ui.painter().rect_stroke(b, 4, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                                    }
                                    let y = if opt[2].is_string() { rect.center().y - 9.0 } else { rect.center().y };
                                    super::text::paint(ui.painter(), egui::pos2(rect.left() + 40.0, y), Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary, 340.0);
                                    if let Some(sub) = opt[2].as_str() {
                                        super::text::paint(ui.painter(), egui::pos2(rect.left() + 40.0, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, reg(12.0), pal().text_muted, 340.0);
                                    }
                                    if resp.clicked() {
                                        if on {
                                            picked.retain(|p| p != id);
                                        } else {
                                            picked.push(id.to_string());
                                        }
                                        r.set(&name, json!(picked));
                                    }
                                }
                                ui.add_space(12.0);
                            }
                            "text" => {
                                let d = &e["data"];
                                if let Some(t) = d["title"].as_str() {
                                    ui.label(RichText::new(plain(t)).font(medium(14.0)).color(pal().header_primary));
                                    ui.add_space(8.0);
                                }
                                let mut text = r.input(&name).and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let rows = d["rows"].as_u64().unwrap_or(1).max(1) as usize;
                                let mut edit = if rows == 1 { egui::TextEdit::singleline(&mut text) } else { egui::TextEdit::multiline(&mut text).desired_rows(rows) };
                                edit = edit.desired_width(392.0).font(reg(14.0)).hint_text(RichText::new(d["placeholder"].as_str().unwrap_or("")).color(pal().text_muted));
                                if let Some(n) = d["character_limit"].as_u64() {
                                    edit = edit.char_limit(n as usize);
                                }
                                if ui.add(edit).changed() {
                                    r.set(&name, json!(text));
                                }
                                ui.add_space(12.0);
                            }
                            "dropdown" => {
                                let d = &e["data"];
                                ui.label(RichText::new(d["title"].as_str().unwrap_or("")).font(medium(14.0)).color(pal().header_primary));
                                ui.add_space(8.0);
                                let cur = r.input(&name).and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let shown = d["options"].as_array().into_iter().flatten().find(|o| o["value"] == cur.as_str()).and_then(|o| o["label"].as_str()).unwrap_or("Select").to_string();
                                egui::ComboBox::from_id_salt(("report", &name)).selected_text(shown).width(392.0).show_ui(ui, |ui| {
                                    for o in d["options"].as_array().into_iter().flatten() {
                                        let v = o["value"].as_str().unwrap_or("");
                                        if ui.selectable_label(cur == v, o["label"].as_str().unwrap_or(v)).clicked() {
                                            r.set(&name, json!(v));
                                        }
                                    }
                                });
                                ui.add_space(12.0);
                            }
                            "external_link" => {
                                let d = &e["data"];
                                if let Some(url) = d["url"].as_str() {
                                    let (rect, resp) = ui.allocate_exact_size(vec2(392.0, 40.0), Sense::click());
                                    crate::ui::motion::wash(ui, &resp, rect, CornerRadius::same(4), MENU_HOVER, 0.05);
                                    super::text::paint(ui.painter(), egui::pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, d["link_text"].as_str().unwrap_or(url), medium(14.0), pal().link, 340.0);
                                    icons::draw(ui.painter(), Icon::LinkExternal, egui::Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                                    if resp.clicked() {
                                        let _ = open::that_detached(url);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    for (n, c) in node["children"].as_array().into_iter().flatten().enumerate() {
                        let label = c[0].as_str().unwrap_or("");
                        let (rect, resp) = ui.allocate_exact_size(vec2(392.0, 44.0), Sense::click());
                        super::settings::mark(ui, &format!("report_child{n}"), rect);
                        crate::ui::motion::wash(ui, &resp, rect, CornerRadius::same(4), MENU_HOVER, 0.05);
                        super::text::paint(ui.painter(), egui::pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, label, reg(16.0), pal().header_primary, 340.0);
                        icons::draw(ui.painter(), Icon::ChevronRight, egui::Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(20.0, 20.0)), pal().header_secondary);
                        if resp.clicked() {
                            go = c[1].as_str().map(str::to_string);
                        }
                    }
                });
            });
            if let Some(e) = &r.err {
                Frame::new().inner_margin(Margin::symmetric(24, 8)).show(ui, |ui| {
                    ui.label(RichText::new(e).font(reg(14.0)).color(pal().critical));
                });
            }
            let kind = node["button"]["type"].as_str().unwrap_or("");
            Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
                ui.set_width(392.0);
                ui.horizontal(|ui| {
                    if !r.path.is_empty() && kind != "done" && grey_button(ui, "Back", 88.0).clicked() {
                        back = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = node["button"]["label"].as_str().map(str::to_string).unwrap_or_else(|| match kind {
                            "submit" => "Submit".into(),
                            "done" => "Done".into(),
                            "cancel" => "Cancel".into(),
                            _ => "Next".into(),
                        });
                        let required = node["elements"].as_array().into_iter().flatten().filter(|e| e["should_submit_data"] == true).all(|e| {
                            let v = r.input(e["name"].as_str().unwrap_or(""));
                            v.is_some_and(|v| v.as_array().map_or(!v.as_str().unwrap_or("").is_empty(), |a| !a.is_empty()))
                        });
                        if !kind.is_empty() {
                            let b = button(ui, &label, if kind == "cancel" { pal().bg_accent } else { BLURPLE }, 96.0);
                            super::settings::mark(ui, "report_button", b.rect);
                            if b.clicked() && required && !r.busy {
                                match kind {
                                    "next" => go = node["button"]["target"].as_str().map(str::to_string),
                                    "submit" => submit = true,
                                    _ => close = true,
                                }
                            }
                        }
                    });
                });
            });
        });
    if modal.should_close() {
        close = true;
    }
    if close {
        app.report = None;
        return;
    }
    if back {
        r.back();
    }
    if let Some(id) = go {
        r.enter(&id);
    }
    let auto = r.node(&r.cur).is_some_and(|n| n["is_auto_submit"] == true) && !r.busy && r.err.is_none();
    if submit || auto {
        r.busy = true;
        let body = r.body();
        app.rest("POST", "/reporting/message".into(), Some(body), Tag::Report(true));
    }
}
