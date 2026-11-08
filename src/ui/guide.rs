//! A server's guide page: the welcome message, the new member steps with their ticks, and the
//! resource channels, from `new-member-welcome` and `new-member-actions`.

use egui::{Align2, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::icons::{self, Icon};
use super::{App, text};
use crate::state::{Id, sid};
use crate::theme::*;

fn card(ui: &mut Ui, key: &str, emoji: &str, title: &str, desc: &str, done: Option<bool>) -> bool {
    let w = ui.available_width();
    let body = (!desc.is_empty()).then(|| ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 120.0));
    let h = 16.0 + 22.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y) + 16.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w, h.max(64.0)), Sense::click());
    super::settings::mark(ui, key, r);
    let fill = super::motion::mix(pal().bg_secondary, pal().bg_mod_hover, super::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    if !emoji.is_empty() {
        text::paint_em(ui.painter(), pos2(r.left() + 36.0, r.center().y), Align2::CENTER_CENTER, emoji, reg(24.0), pal().header_primary, 40.0, 1.0);
    }
    let x = r.left() + 72.0;
    ui.painter().text(pos2(x, r.top() + 16.0), Align2::LEFT_TOP, title, medium(16.0), pal().header_primary);
    if let Some(g) = body {
        ui.painter().galley(pos2(x, r.top() + 42.0), g, pal().header_secondary);
    }
    if let Some(done) = done {
        let c = pos2(r.right() - 28.0, r.center().y);
        if done {
            ui.painter().circle_filled(c, 11.0, pal().green);
            icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(c, vec2(14.0, 14.0)), egui::Color32::WHITE);
        } else {
            ui.painter().circle_stroke(c, 10.0, Stroke::new(2.0, pal().header_secondary));
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(8.0);
    resp.clicked()
}

fn heading(ui: &mut Ui, s: &str) {
    ui.add_space(24.0);
    ui.label(RichText::new(s).font(semi(20.0)).color(pal().header_primary));
    ui.add_space(12.0);
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    let data: Value = app.guide_raw.get(&gid).cloned().unwrap_or_default();
    let done = app.guides.get(&gid).map(|g| g.1.clone()).unwrap_or_default();
    let mut open: Option<Id> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let w = ui.available_width().min(720.0);
        let pad = ((ui.available_width() - w) / 2.0).max(16.0);
        Frame::new().inner_margin(Margin { left: pad as i8, right: pad as i8, top: 32, bottom: 48 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(format!("Welcome to {name}!")).font(bold(32.0)).color(pal().header_primary));
            if let Some(m) = data["welcome_message"]["message"].as_str().filter(|m| !m.is_empty()) {
                ui.add_space(12.0);
                ui.label(RichText::new(m).font(reg(16.0)).color(pal().text_normal));
            }
            let actions = data["new_member_actions"].as_array().cloned().unwrap_or_default();
            if !actions.is_empty() {
                heading(ui, "Get Started");
                for (i, a) in actions.iter().enumerate() {
                    let ch = sid(&a["channel_id"]);
                    let emoji = a["emoji_name"].as_str().unwrap_or_default();
                    if card(ui, &format!("guide:action:{i}"), emoji, a["title"].as_str().unwrap_or_default(), a["description"].as_str().unwrap_or_default(), Some(done.contains(&ch))) {
                        open = Some(ch);
                    }
                }
            }
            let resources = data["resource_channels"].as_array().cloned().unwrap_or_default();
            if !resources.is_empty() {
                heading(ui, "Resources");
                for (i, a) in resources.iter().enumerate() {
                    let ch = sid(&a["channel_id"]);
                    let emoji = a["emoji_name"].as_str().unwrap_or_default();
                    if card(ui, &format!("guide:resource:{i}"), emoji, a["title"].as_str().unwrap_or_default(), a["description"].as_str().unwrap_or_default(), None) {
                        open = Some(ch);
                    }
                }
            }
            if data.is_null() {
                ui.add_space(24.0);
                super::motion::spinner(ui, 24.0, pal().text_muted);
            }
        });
    });
    if let Some(c) = open.filter(|c| *c != 0) {
        app.home = None;
        app.open_thread(c, true, false);
    }
}
