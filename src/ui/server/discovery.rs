//! Server Discovery: whether the server passes Discord's requirements for being listed, and the
//! switch that lists it.

use egui::{Align2, Color32, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

fn check(ui: &mut Ui, ok: bool, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let c = pos2(r.left() + 12.0, r.center().y);
    ui.painter().circle_filled(c, 10.0, if ok { GREEN } else { RED });
    icons::draw(ui.painter(), if ok { Icon::Check } else { Icon::Close }, egui::Rect::from_center_size(c, vec2(12.0, 12.0)), Color32::WHITE);
    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, text, reg(14.0), pal().text_normal);
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    ui.set_max_width(660.0);
    ui.label(RichText::new("Server Discovery").font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    ui.add_space(8.0);
    ui.label(RichText::new("This lists your server in Server Discovery, which allows people to find your server via search or recommendations.").font(medium(14.0)).color(pal().text_normal));
    ui.add_space(24.0);
    let features: Vec<String> = app.store.guilds.get(&gid).map(|g| g.features.clone()).unwrap_or_default();
    if !features.iter().any(|f| f == "COMMUNITY") {
        form::small(ui, "Enable Community to access Server Discovery");
        return;
    }
    let listed = features.iter().any(|f| f == "DISCOVERABLE");
    let Some(req) = app.server_data("discovery", format!("/guilds/{gid}/discovery-requirements")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let sufficient = req["sufficient"].as_bool().unwrap_or(false);
    form::card(ui, |ui| {
        let min = req["minimum_size"].as_u64().unwrap_or(1000);
        let size = req["size"].as_u64().unwrap_or(0);
        check(ui, req["safe_environment"].as_bool().unwrap_or(false), "Your server assets look appropriate for Discovery");
        check(ui, req["healthy"].as_bool().unwrap_or(false), "Your server is active enough");
        check(ui, size >= min, &format!("Your server has more than {min} members"));
        check(ui, req["age"].as_bool().unwrap_or(false), "Your server is old enough");
    });
    ui.add_space(16.0);
    if sufficient {
        form::small(ui, "Great work! Your server passes all the requirements to be in Server Discovery.");
    } else if !listed {
        form::small(ui, "Your server doesn't pass all the requirements to be in Server Discovery yet.");
    }
    ui.add_space(16.0);
    let label = if listed { "Disable Discovery" } else { "Enable Discovery" };
    let resp = ui.add_enabled_ui(listed || sufficient, |ui| form::button(ui, label, !listed)).inner;
    if resp.clicked() {
        let mut next: Vec<Value> = features.iter().filter(|f| *f != "DISCOVERABLE").map(|f| json!(f)).collect();
        if !listed {
            next.push(json!("DISCOVERABLE"));
        }
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "features": next })), Tag::Server("guild"));
    }
}
