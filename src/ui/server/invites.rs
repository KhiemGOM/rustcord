use egui::{Align2, Color32, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, row, title};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Invites", "Here's a list of all active invite links. You can revoke any one or create one.");
    let Some(g) = app.store.guilds.get(&gid) else { return };
    let paused = g.invites_paused();
    let first = g.channels.iter().filter_map(|c| app.store.channels.get(c)).filter(|c| c.kind == 0).min_by_key(|c| c.position).map(|c| c.id);
    let mut toggle = false;
    let mut create = false;
    ui.horizontal(|ui| {
        toggle = settings::danger(ui, if paused { "Resume Invites" } else { "Pause Invites" }, false).clicked();
        if ui.add_enabled_ui(!paused, |ui| settings::primary(ui, "Create Invite Link")).inner.clicked() {
            create = true;
        }
    });
    ui.add_space(24.0);
    if paused {
        ui.label(RichText::new("Invites to this server are paused. Nobody can join with an invite link until you resume them.").font(medium(14.0)).color(pal().header_secondary));
        ui.add_space(16.0);
    }
    let list: Vec<Value> = app.server_data("invites", format!("/guilds/{gid}/invites")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if list.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No active invite links").font(semi(20.0)).color(pal().header_primary));
        });
    } else {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        for (x, h) in [(8.0, "INVITER"), (0.4, "INVITE CODE"), (0.6, "USES"), (0.75, "EXPIRES")] {
            let px = if x > 1.0 { r.left() + x } else { r.left() + r.width() * x };
            ui.painter().text(pos2(px, r.center().y), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary);
        }
    }
    let mut revoke = None;
    let now = chrono::Utc::now().timestamp();
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    for inv in &list {
        let code = inv["code"].as_str().unwrap_or_default().to_string();
        let who = inv["inviter"]["global_name"].as_str().or(inv["inviter"]["username"].as_str()).unwrap_or("Unknown").to_string();
        let avatar = inv["inviter"]["avatar"].as_str().map(|h| format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), inv["inviter"]["id"].as_str().unwrap_or_default()));
        let uses = match inv["max_uses"].as_u64().unwrap_or(0) {
            0 => inv["uses"].as_u64().unwrap_or(0).to_string(),
            m => format!("{}/{m}", inv["uses"].as_u64().unwrap_or(0)),
        };
        let expires = inv["expires_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| {
            let s = (t.timestamp() - now).max(0);
            format!("{:02}:{:02}:{:02}:{:02}", s / 86400, s / 3600 % 24, s / 60 % 60, s % 60)
        });
        let chan = inv["channel"]["name"].as_str().map(|n| format!("#{n}"));
        let resp = row(ui, 56.0, |ui, r, hot| {
            if let Some(a) = &avatar {
                crate::ui::avatar(ui, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 12.0), vec2(24.0, 24.0)), a);
            }
            ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, &who, medium(16.0), pal().header_primary);
            ui.painter().text(pos2(r.left() + r.width() * 0.4, r.center().y - if chan.is_some() { 8.0 } else { 0.0 }), Align2::LEFT_CENTER, &code, medium(16.0), pal().text_normal);
            if let Some(c) = &chan {
                ui.painter().text(pos2(r.left() + r.width() * 0.4, r.center().y + 10.0), Align2::LEFT_CENTER, c, reg(12.0), pal().header_secondary);
            }
            ui.painter().text(pos2(r.left() + r.width() * 0.6, r.center().y), Align2::LEFT_CENTER, &uses, medium(16.0), pal().text_normal);
            ui.painter().text(pos2(r.left() + r.width() * 0.75, r.center().y), Align2::LEFT_CENTER, expires.clone().unwrap_or_else(|| "\u{221e}".into()), medium(16.0), pal().text_normal);
            if hot {
                let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
                ui.painter().circle_filled(x.center(), 10.0, pal().red);
                icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), Color32::WHITE);
            }
        });
        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x > resp.rect.right() - 32.0) {
            revoke = Some(code);
        }
    }
    if let Some(c) = revoke {
        app.confirm = Some(crate::ui::Confirm::new("Delete Invite Link", format!("Are you sure you want to revoke **{c}**? This cannot be undone."), "Revoke", move |app: &mut App| {
            app.rest("DELETE", format!("/invites/{c}"), None, Tag::rollback(crate::ui::failed::stale("invites")));
            if let Some(s) = &mut app.server {
                if let Some(Value::Array(a)) = s.data.get_mut("invites") {
                    a.retain(|i| i["code"].as_str() != Some(c.as_str()));
                }
            }
        }));
    }
    if toggle {
        let dms = app.store.guilds.get(&gid).is_some_and(|g| g.dms_paused());
        let until = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let body = json!({ "invites_disabled_until": (!paused).then_some(&until), "dms_disabled_until": dms.then_some(&until), "lockdown_duration_hours": (!paused || dms).then_some(24) });
        app.rest("PUT", format!("/guilds/{gid}/incident-actions"), Some(body), Tag::reason());
    }
    if create && let Some(ch) = first {
        app.invite_people(ch, "Settings Invite");
        if let Some(s) = &mut app.server {
            s.data.remove("invites");
        }
    }
}
