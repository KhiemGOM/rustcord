use egui::{Frame, Margin, RichText, Ui, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::plugins;
use crate::state::{ADMINISTRATOR, Channel, Id, Store, VIEW_CHANNEL, snowflake_ms};
use crate::theme::*;

/// A channel ShowHiddenChannels lists although we can't see into it.
pub fn locked(store: &Store, c: &Channel) -> bool {
    plugins::on("ShowHiddenChannels") && c.guild != 0 && !c.is_thread() && c.kind != 4 && !store.can_view(c)
}

fn viewers(store: &Store, c: &Channel) -> Vec<(String, u32)> {
    let Some(g) = store.guilds.get(&c.guild) else { return Vec::new() };
    let ow = |id: Id| c.overwrites.iter().find(|o| o.id == id);
    let base = g.roles.get(&g.id).map_or(0, |r| r.perms);
    let mut roles: Vec<_> = g.roles.values().collect();
    roles.sort_by_key(|r| -r.position);
    let mut out: Vec<(String, u32)> = roles
        .into_iter()
        .filter(|r| {
            let mut p = base | r.perms;
            if p & ADMINISTRATOR != 0 {
                return true;
            }
            if let Some(o) = ow(g.id) {
                p = (p & !o.deny) | o.allow;
            }
            if r.id != g.id
                && let Some(o) = ow(r.id)
            {
                p = (p & !o.deny) | o.allow;
            }
            p & VIEW_CHANNEL != 0
        })
        .map(|r| (if r.id == g.id { "@everyone".into() } else { r.name.clone() }, r.color))
        .collect();
    out.extend(c.overwrites.iter().filter(|o| o.kind == 1 && o.allow & VIEW_CHANNEL != 0).map(|o| (store.name_in(c.guild, o.id), 0)));
    out
}

pub fn show(app: &mut App, ui: &mut Ui, c: &Channel) {
    let store = &app.store;
    let people = viewers(store, c);
    let kind = match c.kind {
        2 => "voice",
        13 => "stage",
        15 => "forum",
        5 => "announcement",
        _ => "text",
    };
    ui.vertical_centered(|ui| {
        ui.set_max_width(520.0);
        ui.add_space((ui.available_height() * 0.18).max(24.0));
        let (r, _) = ui.allocate_exact_size(vec2(80.0, 80.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 40.0, pal().bg_secondary);
        icons::draw(ui.painter(), Icon::Lock, egui::Rect::from_center_size(r.center(), vec2(40.0, 40.0)), pal().header_secondary);
        ui.add_space(16.0);
        ui.label(RichText::new(format!("This is a hidden {kind} channel.")).font(bold(24.0)).color(pal().header_primary));
        ui.add_space(8.0);
        ui.label(RichText::new("You cannot see the contents of this channel.").font(reg(16.0)).color(pal().header_secondary));
        if let Some(t) = c.topic.as_deref().filter(|t| !t.is_empty()) {
            ui.label(RichText::new(format!("However, you may see its topic: {t}")).font(reg(16.0)).color(pal().header_secondary));
        }
        ui.add_space(8.0);
        let made = chrono::DateTime::from_timestamp_millis(snowflake_ms(c.id)).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default();
        ui.label(RichText::new(format!("Channel created on {made}")).font(reg(14.0)).color(pal().text_muted));
        if c.nsfw {
            ui.label(RichText::new("Age-restricted channel").font(reg(14.0)).color(pal().text_muted));
        }
        ui.add_space(24.0);
        ui.label(RichText::new("Who can see this channel").font(semi(14.0)).color(pal().header_primary));
        ui.add_space(8.0);
        Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(480.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for (name, color) in &people {
                    let g = ui.painter().layout_no_wrap(name.clone(), reg(12.0), pal().header_primary);
                    let (pill, _) = ui.allocate_exact_size(vec2(32.0 + g.size().x, 24.0), egui::Sense::hover());
                    ui.painter().rect_stroke(pill, 8, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let col = if *color == 0 { egui::Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(*color) };
                    ui.painter().circle_filled(pos2(pill.left() + 12.0, pill.center().y), 6.0, col);
                    ui.painter().galley(pos2(pill.left() + 22.0, pill.center().y - g.size().y / 2.0), g, pal().header_primary);
                }
                if people.is_empty() {
                    ui.label(RichText::new("Nobody listed.").font(reg(14.0)).color(pal().text_muted));
                }
            });
        });
    });
}
