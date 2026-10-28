//! Plugin UI, drawn natively: the element trees Vencord plugins render (html and fc-* controls)
//! laid out with egui. Block elements stack, inline ones flow, `display: flex` rows go across.
//! What the user does comes back as (handler, arguments) for the plugin's own handlers.

use egui::{Align, Color32, CornerRadius, Frame, Id, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::theme::*;

/// What a tree's controls were used for: a handler id and its arguments, or `#url` and a link.
pub type Events = Vec<(String, Value)>;

#[derive(Clone, Copy)]
struct Sty {
    size: f32,
    bold: bool,
    italic: bool,
    mono: bool,
    color: Color32,
}

const INLINE: &[&str] = &["span", "a", "strong", "b", "em", "i", "u", "s", "code", "small", "img", "svg", "br", "abbr", "time", "kbd", "sub", "sup", "mark"];

/// Draws `tree` (a node or a list) for the root `root`.
pub fn draw(ui: &mut Ui, root: &str, tree: &Value) -> Events {
    let mut out = Vec::new();
    let sty = Sty { size: 14.0, bold: false, italic: false, mono: false, color: pal().text_normal };
    let kids: Vec<Value> = match tree {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        v => vec![v.clone()],
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        children(ui, root, &kids, sty, "r", false, 4.0, &mut out);
    });
    out
}

/// Text of a node and everything under it.
pub fn text(n: &Value) -> String {
    match n {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(text).collect(),
        Value::Object(_) => n["c"].as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default(),
        _ => String::new(),
    }
}

fn inline(n: &Value) -> bool {
    match n {
        Value::String(_) => true,
        Value::Object(_) => {
            let t = n["t"].as_str().unwrap_or("");
            INLINE.contains(&t) || (t == "fc-tip" && n["c"].as_array().is_some_and(|c| c.iter().all(inline)))
        }
        _ => false,
    }
}

fn px(v: &Value, base: f32) -> Option<f32> {
    if let Some(n) = v.as_f64() {
        return Some(n as f32);
    }
    let s = v.as_str()?.trim();
    let num = |suffix: &str| s.strip_suffix(suffix).and_then(|x| x.trim().parse::<f32>().ok());
    num("px").or_else(|| num("rem").map(|r| r * 16.0)).or_else(|| num("em").map(|r| r * base)).or_else(|| s.parse().ok())
}

fn color(v: &Value) -> Option<Color32> {
    let s = v.as_str()?.trim();
    if let Some(var) = s.strip_prefix("var(--").and_then(|x| x.split([')', ',']).next()) {
        let p = pal();
        return Some(match var {
            "text-muted" | "interactive-normal" | "channels-default" => p.text_muted,
            "header-primary" | "text-strong" => p.header_primary,
            "header-secondary" | "text-subtle" => p.header_secondary,
            "text-danger" | "status-danger" | "red-400" | "red-500" => RED,
            "text-positive" | "status-positive" | "green-360" => GREEN,
            "status-warning" | "text-warning" | "yellow-300" => YELLOW,
            "text-link" | "brand-500" | "brand-experiment" => p.link,
            "background-secondary" | "background-base-low" => p.bg_secondary,
            "background-tertiary" | "background-base-lowest" => p.bg_tertiary,
            "background-primary" | "background-base-lower" => p.bg_primary,
            _ => p.text_normal,
        });
    }
    if let Some(hex) = s.strip_prefix('#') {
        let hex = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
        let v = u32::from_str_radix(&hex[..6.min(hex.len())], 16).ok()?;
        return Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    if let Some(inner) = s.strip_prefix("rgb(").or(s.strip_prefix("rgba(")).and_then(|x| x.strip_suffix(')')) {
        let p: Vec<f32> = inner.split([',', ' ', '/']).filter_map(|x| x.trim().parse().ok()).collect();
        if p.len() >= 3 {
            return Some(Color32::from_rgb(p[0] as u8, p[1] as u8, p[2] as u8));
        }
    }
    match s {
        "white" => Some(Color32::WHITE),
        "black" => Some(Color32::BLACK),
        "red" => Some(RED),
        "green" => Some(GREEN),
        "gray" | "grey" => Some(pal().text_muted),
        _ => None,
    }
}

fn styled(n: &Value, sty: Sty) -> Sty {
    let mut s = sty;
    match n["t"].as_str().unwrap_or("") {
        "h1" => (s.size, s.bold) = (24.0, true),
        "h2" => (s.size, s.bold) = (20.0, true),
        "h3" => (s.size, s.bold) = (18.0, true),
        "h4" => (s.size, s.bold) = (16.0, true),
        "h5" | "h6" => (s.size, s.bold, s.color) = (12.0, true, pal().header_secondary),
        "strong" | "b" | "th" => s.bold = true,
        "em" | "i" => s.italic = true,
        "code" | "pre" | "kbd" => s.mono = true,
        "small" => s.size = (s.size - 2.0).max(10.0),
        "a" => s.color = pal().link,
        _ => {}
    }
    let st = &n["p"]["style"];
    if let Some(sz) = px(&st["fontSize"], s.size) {
        s.size = sz.clamp(8.0, 48.0);
    }
    match &st["fontWeight"] {
        Value::Number(w) => s.bold = w.as_f64().unwrap_or(400.0) >= 600.0,
        Value::String(w) => s.bold = w == "bold" || w.parse::<f32>().is_ok_and(|w| w >= 600.0),
        _ => {}
    }
    if st["fontStyle"] == "italic" {
        s.italic = true;
    }
    if let Some(c) = color(&st["color"]) {
        s.color = c;
    }
    s
}

fn rich(t: &str, s: Sty) -> RichText {
    let font = if s.mono { egui::FontId::monospace(s.size) } else if s.bold { semi(s.size) } else { reg(s.size) };
    let r = RichText::new(t).font(font).color(s.color);
    if s.italic { r.italics() } else { r }
}

fn handler(n: &Value, name: &str) -> Option<String> {
    n["h"][name].as_str().map(str::to_string)
}

fn event(ui: &Ui, value: Value, checked: Option<bool>) -> Value {
    let m = ui.input(|i| i.modifiers);
    json!({ "$event": true, "value": value, "checked": checked, "shift": m.shift, "ctrl": m.ctrl, "alt": m.alt })
}

#[allow(clippy::too_many_arguments)]
fn children(ui: &mut Ui, root: &str, kids: &[Value], sty: Sty, path: &str, row: bool, gap: f32, out: &mut Events) {
    if row {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (i, k) in kids.iter().enumerate() {
                node(ui, root, k, sty, &format!("{path}.{i}"), out);
            }
        });
        return;
    }
    let mut i = 0;
    while i < kids.len() {
        if inline(&kids[i]) {
            let start = i;
            while i < kids.len() && inline(&kids[i]) {
                i += 1;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (j, k) in kids[start..i].iter().enumerate() {
                    node(ui, root, k, sty, &format!("{path}.{}", start + j), out);
                }
            });
        } else {
            node(ui, root, &kids[i], sty, &format!("{path}.{i}"), out);
            i += 1;
        }
    }
}

fn kids(n: &Value) -> &[Value] {
    n["c"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn node(ui: &mut Ui, root: &str, n: &Value, sty: Sty, path: &str, out: &mut Events) {
    let t = match n {
        Value::String(s) => {
            ui.label(rich(s, sty));
            return;
        }
        Value::Object(_) => n["t"].as_str().unwrap_or(""),
        _ => return,
    };
    let p = &n["p"];
    let id = Id::new((root, path));
    let sty = styled(n, sty);
    let st = &p["style"];
    match t {
        "br" => ui.end_row(),
        "hr" | "fc-sep" => {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
        }
        "svg" => {
            let w = px(&n["w"], sty.size).unwrap_or(sty.size + 4.0);
            let h = px(&n["h"], sty.size).unwrap_or(w);
            let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            super::vencord::svg(ui, n["s"].as_str().unwrap_or(""), r, sty.color);
        }
        "img" => {
            let Some(src) = p["src"].as_str() else { return };
            let w = px(&p["width"], 16.0).or_else(|| px(&st["width"], 16.0));
            let h = px(&p["height"], 16.0).or_else(|| px(&st["height"], 16.0));
            let img = egui::Image::from_uri(src.to_string()).show_loading_spinner(false);
            let resp = match (w, h) {
                (Some(w), Some(h)) => ui.add(img.fit_to_exact_size(vec2(w, h))),
                (Some(w), None) => ui.add(img.max_width(w)),
                _ => ui.add(img.max_width(ui.available_width().min(400.0)).max_height(300.0)),
            };
            if let Some(alt) = p["alt"].as_str().filter(|a| !a.is_empty()) {
                resp.on_hover_text(alt);
            }
        }
        "a" => {
            let label = text(n);
            let resp = ui.add(egui::Label::new(rich(&label, sty).underline()).sense(Sense::click()));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                match (handler(n, "onClick"), p["href"].as_str()) {
                    (Some(h), _) => out.push((h, json!([event(ui, Value::Null, None)]))),
                    (None, Some(href)) => out.push(("#url".into(), href.into())),
                    _ => {}
                }
            }
        }
        "button" | "fc-button" => {
            let label = text(n);
            let variant = p["variant"].as_str().unwrap_or(if t == "button" { "secondary" } else { "primary" });
            let (fill, ink) = match variant {
                "primary" | "overlayPrimary" => (BLURPLE, Color32::WHITE),
                "dangerPrimary" | "danger" => (RED, Color32::WHITE),
                "positive" => (GREEN, Color32::WHITE),
                "dangerSecondary" => (pal().control, RED),
                "link" | "none" => (Color32::TRANSPARENT, pal().link),
                _ => (pal().control, pal().header_primary),
            };
            let small = matches!(p["size"].as_str(), Some("small" | "xs" | "min" | "tiny"));
            let svg = n["c"].as_array().and_then(|c| c.iter().find(|k| k["t"] == "svg")).and_then(|k| k["s"].as_str());
            let resp = ui.add_enabled_ui(p["disabled"] != true, |ui| {
                if label.trim().is_empty()
                    && let Some(svg) = svg
                {
                    super::vencord::icon_button(ui, Some(svg), p["aria-label"].as_str().unwrap_or(""))
                } else {
                    let b = egui::Button::new(RichText::new(label.trim()).font(medium(if small { 13.0 } else { 14.0 })).color(ink)).fill(fill).corner_radius(CornerRadius::same(8));
                    let b = if small { b.min_size(vec2(0.0, 28.0)) } else { b.min_size(vec2(60.0, 34.0)) };
                    ui.add(b)
                }
            });
            let resp = resp.inner;
            super::settings::mark(ui, &format!("vc:{root}:{}", label.trim()), resp.rect);
            if resp.clicked() {
                match (handler(n, "onClick"), p["href"].as_str()) {
                    (Some(h), _) => out.push((h, json!([event(ui, Value::Null, None)]))),
                    (None, Some(href)) => out.push(("#url".into(), href.into())),
                    _ => {}
                }
            }
        }
        "fc-input" | "fc-textarea" | "input" | "textarea" => {
            let kind = p["type"].as_str().unwrap_or("text");
            if t == "input" && (kind == "checkbox" || kind == "radio") {
                let mut on = p["checked"] == true;
                if ui.checkbox(&mut on, "").changed()
                    && let Some(h) = handler(n, "onChange")
                {
                    out.push((h, json!([event(ui, Value::Null, Some(on))])));
                }
                return;
            }
            let controlled = p["value"].as_str().map(str::to_string).or_else(|| p["value"].as_f64().map(|v| v.to_string()));
            let mut s = controlled.clone().unwrap_or_else(|| ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| p["defaultValue"].as_str().unwrap_or("").to_string()));
            let multi = t.ends_with("textarea");
            let mut edit = if multi { egui::TextEdit::multiline(&mut s).desired_rows(p["rows"].as_u64().unwrap_or(3) as usize) } else { egui::TextEdit::singleline(&mut s) };
            edit = edit.id(id).password(kind == "password").desired_width(ui.available_width().min(500.0)).hint_text(p["placeholder"].as_str().unwrap_or("")).font(reg(14.0));
            let resp = Frame::new().fill(pal().input_bg).stroke(Stroke::new(1.0, pal().border_normal)).corner_radius(8).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| ui.add_enabled(p["disabled"] != true, edit)).inner;
            super::settings::mark(ui, &format!("vc:{root}:{}", p["placeholder"].as_str().unwrap_or("input")), resp.rect);
            if resp.changed() {
                if controlled.is_none() {
                    ui.data_mut(|d| d.insert_temp(id, s.clone()));
                }
                let fc = t.starts_with("fc-");
                for name in ["onChange", "onInput"] {
                    if let Some(h) = handler(n, name) {
                        out.push((h, if fc { json!([s]) } else { json!([event(ui, s.clone().into(), None)]) }));
                    }
                }
            }
            if let Some(e) = p["error"].as_str().filter(|e| !e.is_empty()) {
                ui.label(RichText::new(e).font(reg(12.0)).color(RED));
            }
        }
        "fc-switch" | "fc-checkbox" => {
            let mut on = p["checked"] == true;
            let label: String = p["label"].as_array().map(|a| a.iter().map(text).collect()).or_else(|| p["label"].as_str().map(str::to_string)).unwrap_or_default();
            let note: String = p["note"].as_array().map(|a| a.iter().map(text).collect()).or_else(|| p["note"].as_str().map(str::to_string)).unwrap_or_default();
            let changed = ui
                .add_enabled_ui(p["disabled"] != true, |ui| {
                    if t == "fc-checkbox" {
                        return ui.checkbox(&mut on, rich(&label, sty)).changed();
                    }
                    let mut flip = false;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if !label.is_empty() {
                                ui.label(rich(&label, Sty { size: 16.0, bold: true, color: pal().header_primary, ..sty }));
                            }
                            if !note.is_empty() {
                                ui.label(rich(&note, Sty { size: 14.0, color: pal().header_secondary, ..sty }));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            let r = toggle(ui, on);
                            super::settings::mark(ui, &format!("vc:{root}:{label}"), r.rect);
                            flip = r.clicked();
                        });
                    });
                    if flip {
                        on = !on;
                    }
                    flip
                })
                .inner;
            if changed && let Some(h) = handler(n, "onChange") {
                out.push((h, json!([on])));
            }
        }
        "fc-select" => {
            let opts: Vec<&Value> = p["options"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
            let shown = opts.iter().filter(|o| o["default"] == true).map(|o| o["label"].as_str().unwrap_or("")).collect::<Vec<_>>().join(", ");
            let shown = if shown.is_empty() { p["placeholder"].as_str().unwrap_or("Select...").to_string() } else { shown };
            egui::ComboBox::from_id_salt(id).selected_text(shown).width(ui.available_width().min(400.0)).show_ui(ui, |ui| {
                for o in &opts {
                    if ui.selectable_label(o["default"] == true, o["label"].as_str().unwrap_or("")).clicked()
                        && let Some(h) = handler(n, "onChange")
                    {
                        out.push((h, json!([o["value"]])));
                    }
                }
            });
        }
        "fc-slider" => {
            let (lo, hi) = (p["min"].as_f64().unwrap_or(0.0), p["max"].as_f64().unwrap_or(100.0));
            let mut v = ui.data(|d| d.get_temp::<f64>(id)).unwrap_or(p["value"].as_f64().unwrap_or(lo));
            let resp = ui.add_enabled(p["disabled"] != true, egui::Slider::new(&mut v, lo..=hi));
            if resp.changed() {
                let marks: Vec<f64> = p["markers"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
                if p["stickToMarkers"] == true
                    && let Some(m) = marks.iter().copied().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
                {
                    v = m;
                }
                ui.data_mut(|d| d.insert_temp(id, v));
                if let Some(h) = handler(n, "onChange") {
                    out.push((h, json!([v])));
                }
            }
        }
        "fc-color" => {
            let c = p["color"].as_u64().unwrap_or(0) as u32;
            let mut rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
            if ui.color_edit_button_srgb(&mut rgb).changed()
                && let Some(h) = handler(n, "onChange")
            {
                out.push((h, json!([((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32])));
            }
        }
        "fc-tabs" => {
            ui.horizontal(|ui| {
                for (i, tab) in kids(n).iter().enumerate().filter(|(_, k)| k["t"] == "fc-tab") {
                    let tid = &tab["p"]["id"];
                    let on = *tid == p["selected"];
                    let label = text(tab);
                    let resp = ui.add(egui::Button::selectable(on, rich(&label, Sty { bold: on, ..sty })).frame(false));
                    if resp.clicked() && !on && let Some(h) = handler(n, "onChange") {
                        out.push((h, json!([tid])));
                    }
                    let _ = i;
                }
            });
        }
        "fc-modal-header" => {
            ui.horizontal(|ui| {
                ui.vertical(|ui| children(ui, root, kids(n), Sty { size: 20.0, bold: true, color: pal().header_primary, ..sty }, path, false, 4.0, out));
            });
            ui.add_space(8.0);
        }
        "fc-modal-content" => {
            egui::ScrollArea::vertical().id_salt(id).max_height(ui.ctx().content_rect().height() * 0.6).auto_shrink([false, true]).show(ui, |ui| {
                children(ui, root, kids(n), sty, path, false, 4.0, out);
            });
        }
        "fc-modal-footer" => {
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| children(ui, root, kids(n), sty, path, true, 8.0, out));
        }
        "fc-modal-close" => {
            if super::icon_button(ui, super::icons::Icon::Close, 24.0, "Close").clicked()
                && let Some(h) = handler(n, "onClick")
            {
                out.push((h, json!([event(ui, Value::Null, None)])));
            }
        }
        _ => block(ui, root, n, t, sty, path, out),
    }
}

/// A container: its children laid out as its style says, clickable when it has a click handler,
/// with a tooltip when it's a tooltip.
fn block(ui: &mut Ui, root: &str, n: &Value, t: &str, sty: Sty, path: &str, out: &mut Events) {
    let p = &n["p"];
    let st = &p["style"];
    let row = (st["display"] == "flex" || st["display"] == "inline-flex") && !st["flexDirection"].as_str().is_some_and(|d| d.starts_with("column"));
    let gap = px(&st["gap"], sty.size).unwrap_or(if row { 8.0 } else { 4.0 });
    let pad = |k: &str| px(&st[k], sty.size).or_else(|| px(&st["padding"], sty.size)).unwrap_or(0.0);
    let fill = color(&st["backgroundColor"]).or_else(|| color(&st["background"]));
    let (mt, mb) = (px(&st["marginTop"], sty.size).unwrap_or(0.0), px(&st["marginBottom"], sty.size).unwrap_or(0.0));
    if mt > 0.0 {
        ui.add_space(mt.min(32.0));
    }
    let frame = Frame::new()
        .fill(fill.unwrap_or(Color32::TRANSPARENT))
        .corner_radius(px(&st["borderRadius"], sty.size).unwrap_or(0.0).min(24.0) as u8)
        .inner_margin(Margin { left: pad("paddingLeft") as i8, right: pad("paddingRight") as i8, top: pad("paddingTop") as i8, bottom: pad("paddingBottom") as i8 });
    let list = kids(n);
    let resp = frame
        .show(ui, |ui| {
            if t == "li" {
                ui.horizontal_wrapped(|ui| {
                    ui.label(rich("•", sty));
                    children(ui, root, list, sty, path, true, 4.0, out);
                });
            } else if t == "pre" {
                Frame::new().fill(pal().bg_code).corner_radius(4).inner_margin(8).show(ui, |ui| children(ui, root, list, sty, path, false, 0.0, out));
            } else {
                children(ui, root, list, sty, path, row, gap, out);
            }
        })
        .response;
    if let Some(h) = handler(n, "onClick").or_else(|| handler(n, "onMouseDown")) {
        let r = ui.interact(resp.rect, Id::new((root, path, "click")), Sense::click());
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            out.push((h, json!([event(ui, Value::Null, None)])));
        }
    }
    if let Some(h) = handler(n, "onContextMenu") {
        let r = ui.interact(resp.rect, Id::new((root, path, "menu")), Sense::click());
        if r.secondary_clicked() {
            out.push((h, json!([event(ui, Value::Null, None)])));
        }
    }
    let tip = match &p["tooltip"] {
        Value::String(s) => s.clone(),
        v @ Value::Array(_) => text(v),
        _ => String::new(),
    };
    if !tip.is_empty() {
        ui.interact(resp.rect, Id::new((root, path, "tip")), Sense::hover()).on_hover_text(tip);
    }
    if matches!(t, "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") || mb > 0.0 {
        ui.add_space(mb.clamp(4.0, 32.0));
    }
}

fn toggle(ui: &mut Ui, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.12);
    ui.painter().rect_filled(r, CornerRadius::same(12), if on { BLURPLE } else { pal().control });
    let x = egui::lerp(r.left() + 12.0..=r.right() - 12.0, t);
    ui.painter().circle_filled(egui::pos2(x, r.center().y), 9.0, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// What plugins drew, as the app last heard it, for drawing code with no `App` in reach.
#[derive(Clone, Default)]
pub struct Published {
    pub trees: std::sync::Arc<std::collections::HashMap<String, Value>>,
    // how many plugins add accessories, decorations and member decorators
    pub features: [u64; 3],
}

pub fn published(ctx: &egui::Context) -> Published {
    ctx.data(|d| d.get_temp::<Published>(Id::new("vc_published"))).unwrap_or_default()
}

/// Asks the app for a tree it doesn't have yet: `acc:<channel>:<message>` and the like.
pub fn want(ctx: &egui::Context, what: String) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(Id::new("vc_want")).push(what));
}

/// Hands the app something done with plugin UI drawn where it isn't in reach.
pub fn queue(ctx: &egui::Context, root: &str, handler: &str, args: Value) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<(String, String, Value)>>(Id::new("vc_events")).push((root.to_string(), handler.to_string(), args)));
}

/// Plugins' items for a menu drawn without the app: what was fetched, or asked for this frame.
pub fn menu(ui: &mut Ui, key: &str) {
    let items = published(ui.ctx()).trees.get(key).cloned();
    match items {
        Some(items) if items.as_array().is_some_and(|a| !a.is_empty()) => {
            super::menu_sep(ui);
            if let Some(a) = super::vencord::menu_items(ui, &items) {
                queue(ui.ctx(), "#act", &a, Value::Null);
            }
        }
        Some(_) => {}
        None => want(ui.ctx(), key.to_string()),
    }
}

/// Draws the published tree `root`, if there is one, sending what's done with it to the app.
pub fn slot(ui: &mut Ui, root: &str) -> bool {
    let Some(tree) = published(ui.ctx()).trees.get(root).cloned() else { return false };
    if tree.is_null() || tree.as_array().is_some_and(|a| a.is_empty()) {
        return true;
    }
    let evs = draw(ui, root, &tree);
    if !evs.is_empty() {
        let root = root.to_string();
        ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<(String, String, Value)>>(Id::new("vc_events")).extend(evs.into_iter().map(|(h, a)| (root.clone(), h, a))));
    }
    true
}

/// The first element of type `t` in a tree.
pub fn find<'a>(n: &'a Value, t: &str) -> Option<&'a Value> {
    if n["t"] == t {
        return Some(n);
    }
    n.as_array().or_else(|| n["c"].as_array())?.iter().find_map(|k| find(k, t))
}
