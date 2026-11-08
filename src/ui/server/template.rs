//! Server Template: what a template copies, and making, syncing or deleting the server's one.

use egui::{Align2, Color32, Frame, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use super::faint;
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

fn bullets(ui: &mut Ui, head: &str, items: [&str; 3], ok: bool) {
    ui.vertical(|ui| {
        ui.set_width(300.0);
        ui.label(RichText::new(head).font(bold(12.0)).color(pal().header_primary));
        ui.add_space(12.0);
        for it in items {
            let (r, _) = ui.allocate_exact_size(vec2(300.0, 24.0), Sense::hover());
            let c = pos2(r.left() + 12.0, r.center().y);
            ui.painter().circle_filled(c, 10.0, if ok { pal().green } else { pal().red });
            icons::draw(ui.painter(), if ok { Icon::Check } else { Icon::Close }, egui::Rect::from_center_size(c, vec2(12.0, 12.0)), Color32::WHITE);
            ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, it, reg(14.0), pal().text_normal);
            ui.add_space(18.0);
        }
    });
}

fn area(ui: &mut Ui, v: &mut String, hint: &str, rows: usize) {
    Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
        let r = ui.add(egui::TextEdit::multiline(v).frame(Frame::NONE).margin(Margin::ZERO).desired_rows(rows).font(reg(16.0)).text_color(pal().text_normal).hint_text(RichText::new(hint).color(pal().text_muted)).desired_width(ui.available_width()));
        super::super::settings::mark(ui, &format!("field:{hint}"), r.rect);
    });
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    ui.set_max_width(660.0);
    ui.label(RichText::new("Server Template").font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.label(RichText::new("A server template is an easy way to share your server setup and help anyone create a server instantly.").font(medium(14.0)).color(pal().text_normal));
    ui.add_space(12.0);
    ui.label(RichText::new("When someone uses your server template link, they create a new server pre-filled with the same channels, roles, permissions, and settings as yours.").font(medium(14.0)).line_height(Some(18.0)).color(pal().text_normal));
    ui.add_space(32.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 48.0;
        ui.add_space(16.0);
        bullets(ui, "TEMPLATES WILL COPY:", ["Channels and channel topics", "Roles and permissions", "Default server settings"], true);
        bullets(ui, "TEMPLATES WILL NOT COPY:", ["Messages or any content", "Members or bots", "Your server icon, Boosts, or other perks"], false);
    });
    form::line(ui);
    let Some(list) = app.server_data("template", format!("/guilds/{gid}/templates")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let tpl = list.as_array().and_then(|l| l.first()).cloned();
    let key = egui::Id::new(("template_form", gid));
    let (mut name, mut desc) = ui.data_mut(|d| d.get_temp::<(String, String)>(key)).unwrap_or_else(|| match &tpl {
        Some(t) => (t["name"].as_str().unwrap_or_default().to_string(), t["description"].as_str().unwrap_or_default().to_string()),
        None => Default::default(),
    });
    if let Some(t) = &tpl {
        let code = t["code"].as_str().unwrap_or_default().to_string();
        ui.label(RichText::new("Template Link").font(medium(16.0)).color(pal().header_primary));
        ui.add_space(8.0);
        let link = format!("https://discord.new/{code}");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let mut shown = link.clone();
            ui.allocate_ui(vec2(ui.available_width() - 100.0, 40.0), |ui| {
                ui.add_enabled_ui(false, |ui| super::super::settings::input(ui, &mut shown, "", false));
            });
            if form::button(ui, "Copy", true).clicked() {
                ui.ctx().copy_text(link.clone());
                app.toast("Copied to clipboard");
            }
        });
        ui.add_space(8.0);
        form::small(ui, &format!("Used {} times", t["usage_count"].as_u64().unwrap_or(0)));
        ui.add_space(24.0);
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Template Title").font(medium(16.0)).color(pal().header_primary));
        ui.label(RichText::new("*").font(medium(16.0)).color(pal().red));
    });
    ui.add_space(8.0);
    let r = super::super::settings::input(ui, &mut name, "Who is this server for? E.g. School Club, Artists' Community", false);
    super::super::settings::mark(ui, "field:template_title", r.rect);
    name.truncate(100);
    ui.add_space(16.0);
    ui.label(RichText::new("Template Description").font(medium(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    area(ui, &mut desc, "What can people do in this server?", 3);
    desc.truncate(120);
    ui.add_space(16.0);
    let ready = !name.trim().is_empty();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        match &tpl {
            None => {
                ui.add_enabled_ui(ready, |ui| {
                    if form::button(ui, "Generate Template", true).clicked() {
                        app.rest("POST", format!("/guilds/{gid}/templates"), Some(json!({ "name": name.trim(), "description": if desc.trim().is_empty() { Value::Null } else { json!(desc.trim()) } })), Tag::Server("template_made"));
                    }
                });
            }
            Some(t) => {
                let code = t["code"].as_str().unwrap_or_default().to_string();
                let changed = name != t["name"].as_str().unwrap_or_default() || desc != t["description"].as_str().unwrap_or_default();
                ui.add_enabled_ui(ready && changed, |ui| {
                    if form::button(ui, "Save Changes", true).clicked() {
                        app.rest("PATCH", format!("/guilds/{gid}/templates/{code}"), Some(json!({ "name": name.trim(), "description": desc.trim() })), Tag::Server("template_made"));
                    }
                });
                if t["is_dirty"].as_bool().unwrap_or(false) {
                    if form::button(ui, "Sync Template", false).clicked() {
                        app.rest("PUT", format!("/guilds/{gid}/templates/{code}"), None, Tag::Server("template_made"));
                    }
                }
                if form::button(ui, "Delete Template", false).clicked() {
                    app.confirm = Some(super::super::Confirm::new("Delete Template", "Are you sure you want to delete your template? This will prevent anyone with the link from using it.", "Delete Template", move |app: &mut App| {
                        app.rest("DELETE", format!("/guilds/{gid}/templates/{code}"), None, Tag::Server("template_made"));
                    }));
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(key, (name, desc)));
}
