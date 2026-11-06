use egui::{Ui, vec2};
use serde_json::{Value, json};

use super::{App, rule, title};
use crate::net::Tag;
use crate::state::Id;
use crate::ui::settings;

// .contentColumn's padding-block-end
const PAGE_END: f32 = 80.0;
const FEATURE: &str = "ENABLED_MODERATION_EXPERIENCE_FOR_NON_COMMUNITY";

pub fn loaded(app: &mut App, v: Value) {
    let Some(s) = &mut app.server else { return };
    for u in super::super::community::fill(&mut s.pages.members, &v) {
        app.store.apply("USER_UPDATE", &u);
    }
}

pub fn list_toggle(app: &mut App, ui: &mut Ui, gid: Id, label: &str) -> bool {
    let Some(g) = app.store.guilds.get(&gid) else { return false };
    if g.features.iter().any(|f| f == "COMMUNITY") {
        return false;
    }
    let mut on = g.features.iter().any(|f| f == FEATURE);
    if settings::toggle(ui, &mut on, label, "Enabling this will show the members page in the channel list, allowing you to quickly see who's recently joined your server, and find any users flagged for unusual activity.") {
        let mut feats = g.features.clone();
        feats.retain(|f| f != FEATURE);
        if on {
            feats.push(FEATURE.into());
        }
        if let Some(g) = app.store.guilds.get_mut(&gid) {
            g.features = feats.clone();
        }
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "features": feats })), Tag::Quiet);
    }
    true
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Server Members", "");
    if list_toggle(app, ui, gid, "Show Members In Channel List") {
        rule(ui);
    }
    let Some(s) = app.server.as_mut() else { return };
    let mut m = std::mem::take(&mut s.pages.members);
    if m.sort == 0 {
        m.sort = 1;
    }
    let h = (ui.ctx().content_rect().bottom() - ui.cursor().top() - PAGE_END).max(0.0);
    ui.allocate_ui(vec2(ui.available_width(), h), |ui| {
        crate::ui::community::table(app, ui, gid, &mut m, Tag::Server("members"), 0.0);
    });
    if let Some(s) = &mut app.server {
        s.pages.members = m;
    }
}
