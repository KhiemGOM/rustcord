use egui::{Align, Color32, Frame, Layout, Margin, RichText, Sense, Ui, vec2};
use serde_json::Value;

use super::icons::Icon;
use super::{App, icon_button};
use crate::net::Tag;
use crate::plugins;
use crate::state::{ADMINISTRATOR, Id, snowflake_ms};
use crate::theme::*;

const PERMS: [(u64, &str); 49] = [
    (1 << 3, "Administrator"),
    (1 << 4, "Manage Channels"),
    (1 << 5, "Manage Server"),
    (1 << 28, "Manage Roles"),
    (1 << 29, "Manage Webhooks"),
    (1 << 30, "Manage Expressions"),
    (1 << 43, "Create Expressions"),
    (1 << 7, "View Audit Log"),
    (1 << 19, "View Server Insights"),
    (1 << 41, "View Creator Monetization Analytics"),
    (1 << 1, "Kick Members"),
    (1 << 2, "Ban Members"),
    (1 << 40, "Timeout Members"),
    (1 << 27, "Manage Nicknames"),
    (1 << 26, "Change Nickname"),
    (1 << 0, "Create Invite"),
    (1 << 10, "View Channel"),
    (1 << 11, "Send Messages"),
    (1 << 38, "Send Messages in Threads"),
    (1 << 35, "Create Public Threads"),
    (1 << 36, "Create Private Threads"),
    (1 << 34, "Manage Threads"),
    (1 << 13, "Manage Messages"),
    (1 << 51, "Pin Messages"),
    (1 << 52, "Bypass Slowmode"),
    (1 << 14, "Embed Links"),
    (1 << 15, "Attach Files"),
    (1 << 16, "Read Message History"),
    (1 << 6, "Add Reactions"),
    (1 << 18, "Use External Emoji"),
    (1 << 37, "Use External Stickers"),
    (1 << 17, "Mention @everyone, @here, and All Roles"),
    (1 << 12, "Send Text-to-Speech Messages"),
    (1 << 46, "Send Voice Messages"),
    (1 << 49, "Create Polls"),
    (1 << 31, "Use Application Commands"),
    (1 << 50, "Use External Apps"),
    (1 << 39, "Use Activities"),
    (1 << 20, "Connect"),
    (1 << 21, "Speak"),
    (1 << 9, "Video"),
    (1 << 25, "Use Voice Activity"),
    (1 << 8, "Priority Speaker"),
    (1 << 22, "Mute Members"),
    (1 << 23, "Deafen Members"),
    (1 << 24, "Move Members"),
    (1 << 42, "Use Soundboard"),
    (1 << 45, "Use External Sounds"),
    (1 << 33, "Manage Events"),
];

const OWNER: u32 = 0xf0b232;

pub struct Section {
    head: String,
    color: u32,
    lines: Vec<(String, u32, Option<bool>)>,
}

pub struct Perms {
    title: String,
    sections: Vec<Section>,
}

impl App {
    /// PermissionsViewer on a member: each permission they have and the roles that give it.
    pub fn view_user_perms(&mut self, gid: Id, uid: Id) {
        let store = &self.store;
        let (Some(g), Some(m)) = (store.guilds.get(&gid), store.members.get(&gid).and_then(|m| m.get(&uid))) else { return };
        let mut roles: Vec<_> = m.roles.iter().filter_map(|r| g.roles.get(r)).chain(g.roles.get(&gid)).collect();
        roles.sort_by_key(|r| r.position);
        if plugins::text("PermissionsViewer", "permissionsSortOrder") != "1" {
            roles.reverse();
        }
        let owner = g.owner == uid;
        let mut sections = Vec::new();
        for (bit, name) in PERMS {
            let mut lines: Vec<(String, u32, Option<bool>)> = Vec::new();
            if owner {
                lines.push(("Server Owner".into(), OWNER, None));
            }
            lines.extend(roles.iter().filter(|r| r.perms & (bit | ADMINISTRATOR) != 0).map(|r| (r.name.clone(), r.color, None)));
            if !lines.is_empty() {
                sections.push(Section { head: name.into(), color: 0, lines });
            }
        }
        self.ext.perms = Some(Perms { title: format!("{}'s Permissions", store.name_in(gid, uid)), sections });
    }

    /// PermissionsViewer on a channel: what each role and member has allowed or denied there.
    pub fn view_channel_perms(&mut self, ch: Id) {
        let store = &self.store;
        let Some(c) = store.channels.get(&ch) else { return };
        let g = store.guilds.get(&c.guild);
        let mut sections = Vec::new();
        for o in &c.overwrites {
            let (head, color) = if o.kind == 0 {
                g.and_then(|g| g.roles.get(&o.id)).map(|r| (if r.id == c.guild { "@everyone".to_string() } else { r.name.clone() }, r.color)).unwrap_or(("Unknown role".into(), 0))
            } else {
                (store.name_in(c.guild, o.id), 0)
            };
            let names = |bits: u64| PERMS.iter().filter(move |(b, _)| bits & b != 0).map(|(_, n)| n.to_string());
            let mut lines: Vec<(String, u32, Option<bool>)> = names(o.allow).map(|n| (n, 0, Some(true))).collect();
            lines.extend(names(o.deny).map(|n| (n, 0, Some(false))));
            sections.push(Section { head, color, lines });
        }
        self.ext.perms = Some(Perms { title: format!("Permissions for #{}", c.name), sections });
    }

    pub fn open_server_info(&mut self, gid: Id) {
        self.ext.info = Some(gid);
        self.remote.insert("server_info", None);
        self.rest("GET", format!("/guilds/{gid}?with_counts=true"), None, Tag::Remote("server_info"));
    }
}

fn modal(ctx: &egui::Context, id: &str, title: &str, body: impl FnOnce(&mut Ui)) -> bool {
    let mut close = false;
    let m = egui::Modal::new(egui::Id::new(id))
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(Margin::same(24)))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(16.0);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 160.0).max(120.0)).show(ui, |ui| {
                ui.set_width(480.0);
                body(ui)
            });
        });
    close || m.should_close()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if let Some(p) = &app.ext.perms {
        let close = modal(ctx, "perms_viewer", &p.title, |ui| {
            if p.sections.is_empty() {
                ui.label(RichText::new("Nothing here.").font(reg(14.0)).color(pal().header_secondary));
            }
            for s in &p.sections {
                ui.add_space(12.0);
                let col = if s.color == 0 { pal().header_primary } else { role_color(s.color) };
                ui.label(RichText::new(&s.head).font(semi(14.0)).color(col));
                ui.add_space(4.0);
                for (text, color, allow) in &s.lines {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                        let dot = match allow {
                            Some(true) => GREEN,
                            Some(false) => RED,
                            None if *color == 0 => Color32::from_rgb(0xc4, 0xc9, 0xce),
                            None => role_color(*color),
                        };
                        ui.painter().circle_filled(r.center(), 5.0, dot);
                        ui.label(RichText::new(text).font(reg(14.0)).color(pal().text_normal));
                    });
                }
            }
        });
        if close {
            app.ext.perms = None;
        }
    }
    if let Some(gid) = app.ext.info {
        let v = match app.remote.get("server_info") {
            Some(Some(Ok(v))) => v.clone(),
            _ => Value::Null,
        };
        let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
        let store = &app.store;
        let g = store.guilds.get(&gid);
        let owner = g.map(|g| store.name_in(gid, g.owner)).unwrap_or_default();
        let date = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y, %-I:%M %p").to_string()).unwrap_or_default();
        let level = ["None", "Low", "Medium", "High", "Highest"].get(v["verification_level"].as_u64().unwrap_or(9) as usize).copied().unwrap_or("-");
        let mut rows: Vec<(&str, String)> = vec![
            ("Server Owner", owner),
            ("Created At", date(snowflake_ms(gid))),
            ("Joined At", g.filter(|g| g.joined > 0).map_or("-".into(), |g| date(g.joined * 1000))),
            ("Preferred Locale", v["preferred_locale"].as_str().unwrap_or("-").to_string()),
            ("Verification Level", level.to_string()),
            ("Server Boosts", g.map_or(String::new(), |g| format!("{} (Level {})", g.boosts, g.tier))),
            ("Members", v["approximate_member_count"].as_u64().or(g.map(|g| g.member_count as u64)).map_or("-".into(), |n| n.to_string())),
            ("Online", v["approximate_presence_count"].as_u64().map_or("-".into(), |n| n.to_string())),
            ("Roles", g.map_or(0, |g| g.roles.len()).to_string()),
            ("Channels", g.map_or(0, |g| g.channels.len()).to_string()),
            ("Emojis", g.map_or(0, |g| g.emojis.len()).to_string()),
            ("Stickers", g.map_or(0, |g| g.stickers.len()).to_string()),
        ];
        if let Some(d) = v["description"].as_str().filter(|d| !d.is_empty()) {
            rows.insert(1, ("Description", d.to_string()));
        }
        let features = g.map(|g| g.features.iter().map(|f| f.to_lowercase().replace('_', " ")).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        if !features.is_empty() {
            rows.push(("Features", features));
        }
        let close = modal(ctx, "server_info", &name, |ui| {
            for (k, val) in &rows {
                ui.label(RichText::new(k.to_uppercase()).font(bold(12.0)).color(pal().header_secondary));
                ui.add_space(4.0);
                ui.label(RichText::new(val).font(reg(16.0)).color(pal().text_normal));
                ui.add_space(16.0);
            }
        });
        if close {
            app.ext.info = None;
            app.remote.remove("server_info");
        }
    }
}
