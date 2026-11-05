use egui::{Align, Layout, RichText, Ui};
use serde_json::json;

use super::icons::Icon;
use super::{App, button, icon_button};
use crate::net::Tag;
use crate::state::{Channel, Id, MANAGE_WEBHOOKS};
use crate::theme::*;

pub struct Follow {
    pub guild: Id,
    pub chan: Id,
}

impl Follow {
    pub fn new() -> Self {
        Follow { guild: 0, chan: 0 }
    }
}

fn targets(app: &App, gid: Id) -> Vec<&Channel> {
    let s = &app.store;
    let Some(g) = s.guilds.get(&gid) else { return Vec::new() };
    g.channels.iter().filter_map(|c| s.channels.get(c)).filter(|c| c.kind == 0 && s.perms(c) & MANAGE_WEBHOOKS != 0).collect()
}

fn pick(ui: &mut Ui, id: &str, label: &str, enabled: bool, items: Vec<(Id, String)>, cur: &mut Id) {
    ui.add_space(8.0);
    ui.label(RichText::new(label).font(semi(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
    let shown = items.iter().find(|(i, _)| i == cur).map_or("Select...".to_string(), |(_, n)| n.clone());
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id).selected_text(shown).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
            for (i, n) in items {
                ui.selectable_value(cur, i, n);
            }
        });
    });
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(f) = &app.follow else { return };
    let Some(src) = app.cur() else { return };
    let (mut gid, mut chan) = (f.guild, f.chan);
    let guilds: Vec<(Id, String)> = app.store.guild_order.iter().filter(|g| app.store.my_perms(**g) & MANAGE_WEBHOOKS != 0).filter_map(|g| app.store.guilds.get(g).map(|g| (g.id, g.name.clone()))).collect();
    let chans: Vec<(Id, String)> = targets(app, gid).into_iter().map(|c| (c.id, format!("# {}", c.name))).collect();
    let title = app.store.channels.get(&src).map(|c| c.name.clone()).unwrap_or_default();
    let (mut go, mut close) = (false, false);
    let modal = egui::Modal::new("follow".into())
        .frame(egui::Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("# {title}")).font(medium(16.0)).color(pal().header_secondary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Add this channel's updates to your server!").font(semi(20.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(RichText::new("Select where you want these updates to go. You can remove this at any time in Server Settings > Integrations > Channels Followed.").font(reg(14.0)).color(pal().header_secondary));
            });
            ui.add_space(16.0);
            let before = gid;
            pick(ui, "follow_guild", "Send to", true, guilds, &mut gid);
            if gid != before {
                chan = 0;
            }
            ui.add_space(16.0);
            pick(ui, "follow_chan", "Select channel", gid != 0, chans, &mut chan);
            ui.add_space(16.0);
            ui.label(RichText::new("Only servers and channels you have webhook permissions in are shown.").font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = button(ui, "Follow", BLURPLE, 100.0);
                super::settings::mark(ui, "follow:go", b.rect);
                go = chan != 0 && b.clicked();
                ui.add_space(8.0);
                close |= super::grey_button(ui, "Cancel", 100.0).clicked();
            });
        });
    if go {
        app.rest("POST", format!("/channels/{src}/followers"), Some(json!({ "webhook_channel_id": chan.to_string() })), Tag::Quiet);
        app.toast("Following channel".to_string());
        close = true;
    }
    if close || modal.should_close() {
        app.follow = None;
    } else if let Some(f) = &mut app.follow {
        f.guild = gid;
        f.chan = chan;
    }
}
