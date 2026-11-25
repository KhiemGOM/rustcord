//! Discord's member verification: a server's rules to read and agree to before talking there.

use egui::{Color32, Frame, Margin, Sense, Stroke, pos2, vec2};
use serde_json::{Value, json};

use super::App;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

pub fn open(app: &mut App, guild: Id) {
    app.screening = Some((guild, false));
    app.remote.insert("screening", None);
    app.remote.remove("screening_sent");
    app.rest("GET", format!("/guilds/{guild}/member-verification?with_guild=false"), None, Tag::Remote("screening"));
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((guild, mut agreed)) = app.screening else { return };
    match app.remote.get("screening_sent") {
        Some(Some(Ok(_))) => {
            app.screening = None;
            app.remote.remove("screening_sent");
            return;
        }
        Some(Some(Err(e))) => {
            let e = e.clone();
            app.remote.remove("screening_sent");
            app.toast(e);
        }
        _ => {}
    }
    let form: Option<Value> = match app.remote.get("screening") {
        Some(Some(Ok(v))) => Some(v.clone()),
        _ => None,
    };
    let rules: Vec<String> = form
        .as_ref()
        .and_then(|f| f["form_fields"].as_array())
        .into_iter()
        .flatten()
        .filter(|f| f["field_type"] == "TERMS")
        .flat_map(|f| f["values"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)))
        .collect();
    let name = app.store.guilds.get(&guild).map(|g| g.name.clone()).unwrap_or_default();
    let sending = matches!(app.remote.get("screening_sent"), Some(None));
    let mut submit = false;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = super::motion::Modal::new("screening".into()).frame(frame).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(408.0);
            ui.label(egui::RichText::new(name).font(semi(20.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("You must complete a few more steps before you can start talking in this server.").font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            ui.label(egui::RichText::new("Agree to rules").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(8.0);
            if form.is_none() {
                super::motion::spinner(ui, 24.0, pal().text_muted);
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (i, rule) in rules.iter().enumerate() {
                    let n = ui.painter().layout_no_wrap(format!("{}.", i + 1), reg(16.0), pal().text_muted);
                    let body = ui.painter().layout(rule.clone(), reg(16.0), pal().text_normal, 408.0 - 28.0);
                    let (r, _) = ui.allocate_exact_size(vec2(408.0, body.size().y.max(n.size().y) + 8.0), Sense::hover());
                    ui.painter().galley(r.min, n, pal().text_muted);
                    ui.painter().galley(pos2(r.left() + 28.0, r.top()), body, pal().text_normal);
                }
            });
            ui.add_space(8.0);
            super::channel::overview::check(ui, &mut agreed, "I have read and agree to the rules", "screening:agree");
        });
        let ready = agreed && !rules.is_empty() && !sending;
        Frame::new().fill(pal().bg_tertiary).corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 }).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(408.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = ui.add_enabled_ui(ready, |ui| super::settings::primary(ui, "Submit")).inner;
                super::settings::mark(ui, "screening:submit", b.rect);
                submit = b.clicked();
            });
        });
    });
    if submit {
        let mut fields = form.as_ref().and_then(|f| f["form_fields"].as_array().cloned()).unwrap_or_default();
        for f in &mut fields {
            if f["field_type"] == "TERMS" {
                f["response"] = json!(true);
            }
        }
        let body = json!({ "version": form.as_ref().map_or(Value::Null, |f| f["version"].clone()), "form_fields": fields });
        app.remote.insert("screening_sent", None);
        app.rest("PUT", format!("/guilds/{guild}/requests/@me"), Some(body), Tag::Remote("screening_sent"));
    }
    app.screening = if m.should_close() { None } else { app.screening.map(|(g, _)| (g, agreed)) };
}
