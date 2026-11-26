//! Server Onboarding editor: configure enabled state, mode, default channels, and prompts
//! via GET/PUT /guilds/{id}/onboarding.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui};
use serde_json::{Value, json};

use super::super::App;
use super::form;
use super::title;
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

pub fn build_payload(d: &Value) -> Value {
    json!({
        "prompts": d["prompts"].as_array().cloned().unwrap_or_default(),
        "default_channel_ids": d["default_channel_ids"].as_array().cloned().unwrap_or_default(),
        "enabled": d["enabled"].as_bool().unwrap_or(false),
        "mode": d["mode"].as_u64().unwrap_or(0),
    })
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    title(ui, "Onboarding", "Set default channels and questions new members see when joining this server.");
    let Some(raw) = app.server_data("onboarding", format!("/guilds/{gid}/onboarding")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    ui.set_max_width(660.0);
    let orig = build_payload(&raw);
    let mut d = form::take(app, "onboarding", &orig);

    let mut enabled = d["enabled"].as_bool().unwrap_or(false);
    if super::super::settings::toggle(ui, &mut enabled, "Enable Onboarding", "Show default channels and customization questions to new members.") {
        d["enabled"] = json!(enabled);
    }

    let mut mode = d["mode"].as_u64().unwrap_or(0);
    super::super::settings::h5(ui, "Onboarding Mode");
    if super::super::settings::radio(
        ui,
        &mut mode,
        &[
            (0, "Default Channels Only", "Counts only default channels toward the 5-channel requirement."),
            (1, "Default Channels + Questions", "Counts both default channels and prompt channels toward onboarding."),
        ],
    ) {
        d["mode"] = json!(mode);
    }
    form::line(ui);

    super::super::settings::h5(ui, "Default Channels");
    form::small(ui, "Channels every new member sees immediately after joining.");
    ui.add_space(8.0);
    let mut defaults: Vec<String> = d["default_channel_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let chans: Vec<(Snow, String)> = app
        .store
        .guilds
        .get(&gid)
        .map(|g| {
            g.channels
                .iter()
                .filter_map(|id| app.store.channels.get(id))
                .filter(|c| !c.is_thread() && c.kind != 4)
                .map(|c| (c.id, c.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    egui::ScrollArea::vertical().id_salt("onboarding_defaults").max_height(180.0).show(ui, |ui| {
        for (cid, name) in &chans {
            let sid = cid.to_string();
            let mut picked = defaults.contains(&sid);
            if ui.checkbox(&mut picked, format!("#{name}")).changed() {
                if picked {
                    defaults.push(sid);
                } else {
                    defaults.retain(|x| x != &sid);
                }
            }
        }
    });
    d["default_channel_ids"] = json!(defaults);
    form::line(ui);

    super::super::settings::h5(ui, "Customization Prompts");
    form::small(ui, "Questions that let members pick roles and channels during onboarding.");
    ui.add_space(8.0);
    let mut prompts = d["prompts"].as_array().cloned().unwrap_or_default();
    let mut remove_prompt = None;
    for (pi, p) in prompts.iter_mut().enumerate() {
        Frame::new()
            .fill(pal().bg_secondary)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, pal().border_subtle))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Prompt {}", pi + 1)).font(semi(14.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if super::super::grey_button(ui, "Remove", 72.0).clicked() {
                            remove_prompt = Some(pi);
                        }
                    });
                });
                ui.add_space(6.0);
                let mut q = p["title"].as_str().unwrap_or("").to_string();
                if super::super::settings::input(ui, &mut q, "Question title", false).changed() {
                    p["title"] = json!(q);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let mut single = p["single_select"].as_bool().unwrap_or(false);
                    if ui.checkbox(&mut single, "Single choice").changed() {
                        p["single_select"] = json!(single);
                    }
                    let mut req = p["required"].as_bool().unwrap_or(false);
                    if ui.checkbox(&mut req, "Required").changed() {
                        p["required"] = json!(req);
                    }
                    let mut in_onb = p["in_onboarding"].as_bool().unwrap_or(true);
                    if ui.checkbox(&mut in_onb, "Show in Onboarding").changed() {
                        p["in_onboarding"] = json!(in_onb);
                    }
                });
                ui.add_space(6.0);
                let mut opts = p["options"].as_array().cloned().unwrap_or_default();
                let mut remove_opt = None;
                for (oi, o) in opts.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let mut ot = o["title"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut ot).hint_text("Option title").desired_width(220.0)).changed() {
                            o["title"] = json!(ot);
                        }
                        let mut od = o["description"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut od).hint_text("Description").desired_width(240.0)).changed() {
                            o["description"] = json!(od);
                        }
                        if super::super::grey_button(ui, "X", 28.0).clicked() {
                            remove_opt = Some(oi);
                        }
                    });
                }
                if let Some(oi) = remove_opt {
                    opts.remove(oi);
                }
                ui.add_space(4.0);
                if super::super::grey_button(ui, "+ Add Option", 108.0).clicked() {
                    opts.push(json!({
                        "id": "0",
                        "title": "New Option",
                        "description": "",
                        "channel_ids": [],
                        "role_ids": [],
                    }));
                }
                p["options"] = json!(opts);
            });
        ui.add_space(8.0);
    }
    if let Some(pi) = remove_prompt {
        prompts.remove(pi);
    }
    if super::super::grey_button(ui, "+ Add Prompt", 112.0).clicked() {
        prompts.push(json!({
            "id": "0",
            "type": 0,
            "title": "What brings you here?",
            "single_select": false,
            "required": false,
            "in_onboarding": true,
            "options": [],
        }));
    }
    d["prompts"] = json!(prompts);

    let payload = build_payload(&d);
    form::put(app, "onboarding", payload.clone());
    let ctx = ui.ctx().clone();
    form::bar_with(app, &ctx, "onboarding", &orig, move |app, _| {
        app.rest("PUT", format!("/guilds/{gid}/onboarding"), Some(payload), Tag::Server("onboarding"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_payload_normalizes_onboarding_fields() {
        let raw = json!({
            "guild_id": "123",
            "enabled": true,
            "mode": 1,
            "default_channel_ids": ["10", "20"],
            "prompts": [{ "id": "1", "title": "Pick roles", "options": [] }],
            "extra_ignored": 99
        });
        let p = build_payload(&raw);
        assert_eq!(p["enabled"], json!(true));
        assert_eq!(p["mode"], json!(1));
        assert_eq!(p["default_channel_ids"], json!(["10", "20"]));
        assert_eq!(p["prompts"].as_array().unwrap().len(), 1);
        assert!(p.get("extra_ignored").is_none());
    }
}
