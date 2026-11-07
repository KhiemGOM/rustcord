//! Discord's Prune Members dialog: how long since they were seen, optional roles to include, and the count it would kick.

use egui::{Align, Frame, Layout, RichText};
use serde_json::json;

use super::icons::Icon;
use super::{App, button, enter_submits, grey_button, icon_button, settings};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

const DAYS: [u8; 2] = [7, 30];

pub struct Dialog {
    guild: Id,
    days: u8,
    roles: Vec<Id>,
    pub count: Option<u64>,
}

impl Dialog {
    fn ask(&mut self, app: &App) {
        self.count = None;
        let roles = self.roles.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(",");
        let include = if roles.is_empty() { String::new() } else { format!("&include_roles={roles}") };
        app.rest("GET", format!("/guilds/{}/prune?days={}{include}", self.guild, self.days), None, Tag::Home("prune"));
    }
}

impl App {
    pub fn open_prune(&mut self, guild: Id) {
        let mut d = Dialog { guild, days: 7, roles: Vec::new(), count: None };
        d.ask(self);
        self.prune = Some(d);
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.prune.take() else { return };
    let server = app.store.guilds.get(&d.guild).map(|g| g.name.clone()).unwrap_or_default();
    let mut roles: Vec<(Id, String)> = app.store.guilds.get(&d.guild).map(|g| g.roles.values().filter(|r| r.id != d.guild && crate::ui::profile::can_manage(&app.store, d.guild, r.id)).map(|r| (r.id, r.name.clone())).collect()).unwrap_or_default();
    roles.sort_by_key(|(_, n)| n.to_lowercase());
    let (mut go, mut close, mut again) = (false, false, false);
    let modal = super::motion::Modal::new("prune".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Prune Members\u{2014}{server}")).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(20.0);
            ui.label(RichText::new("Last Seen").font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            settings::dropdown(ui, ui.available_width(), None, &format!("{} days", d.days), |ui| {
                for n in DAYS {
                    if super::menu_item(ui, (d.days == n).then_some(Icon::Check), &format!("{n} days"), false) && d.days != n {
                        d.days = n;
                        again = true;
                    }
                }
            });
            if !roles.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new("Also include members with these roles").font(semi(14.0)).color(pal().header_primary));
                ui.add_space(8.0);
                let shown = match d.roles.len() {
                    0 => "Select roles".to_string(),
                    n => format!("{n} selected"),
                };
                settings::dropdown(ui, ui.available_width(), None, &shown, |ui| {
                    for (id, name) in &roles {
                        if super::menu_item(ui, d.roles.contains(id).then_some(Icon::Check), name, false) {
                            match d.roles.iter().position(|r| r == id) {
                                Some(i) => drop(d.roles.remove(i)),
                                None => d.roles.push(*id),
                            }
                            again = true;
                        }
                    }
                });
            }
            ui.add_space(20.0);
            let says = match d.count {
                None => "Counting the members who would be pruned\u{2026}".to_string(),
                Some(n) => {
                    let who = if d.roles.is_empty() { "have no roles" } else { "have no roles, or only the ones picked above," };
                    format!("{n} member{} who haven\u{2019}t been seen in {} days and {who} will be kicked. They can rejoin with a new invite.", if n == 1 { "" } else { "s" }, d.days)
                }
            };
            ui.label(RichText::new(says).font(reg(14.0)).color(pal().text_normal));
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= grey_button(ui, "Cancel", 212.0).clicked();
                let b = button(ui, "Prune", super::DANGER, 212.0);
                settings::mark(ui, "prune:go", b.rect);
                go = b.clicked();
            });
        });
    go |= enter_submits(ctx, false);
    if go {
        let body = json!({ "days": d.days, "compute_prune_count": false, "include_roles": d.roles.iter().map(|r| r.to_string()).collect::<Vec<_>>() });
        app.rest("POST", format!("/guilds/{}/prune", d.guild), Some(body), Tag::reason());
    } else if !(close || modal.should_close()) {
        if again {
            d.ask(app);
        }
        app.prune = Some(d);
    }
}
