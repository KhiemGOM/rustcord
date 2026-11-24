//! The server menu's own windows: Privacy Settings, Security Actions, Report Raid and the Server
//! Tag popout.

use egui::{Align2, Color32, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::App;
use super::create::{modal, plate};
use super::settings::switch;
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;

pub enum Modal {
    Privacy(Id),
    Security { gid: Id, invites: bool, dms: bool },
    Raid(Id),
    Tag(Id),
}

const PRIVACY: [(u64, &str, &str, Option<&str>); 4] = [
    (3, "Direct Messages", "Allow DMs from other members in this server", None),
    (16, "Message requests", "Filter messages from server members you may not know.", Some("https://support.discord.com/hc/en-us/articles/7924992471191")),
    (13, "Share my activity", "Share activity information from games and connected apps I use, including when and how I engage.", None),
    (15, "Activity joining", "Allow users to join your activity in this server.", Some("https://support.discord.com/hc/en-us/articles/7931156448919")),
];
const POLICIES: &str = "https://discord.com/safety/360043709612-our-policies";

fn link(ui: &mut Ui, text: &str, url: &str) {
    let r = ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().link)).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        let _ = open::that_detached(url);
    }
}

/// One of Privacy Settings' switches: discord's 350 wide column, the copy wrapping short of the
/// switch, and Learn more after it.
fn privacy_row(ui: &mut Ui, title: &str, desc: &str, more: Option<&str>, on: bool) -> bool {
    let top = ui.cursor().left_top();
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(top.x + 302.0, top.y + 1.0), vec2(48.0, 24.0))));
    let flipped = switch(&mut c, on).clicked();
    ui.label(RichText::new(title).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.scope(|ui| {
        ui.set_max_width(286.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
            ui.label(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary));
            if let Some(url) = more {
                link(ui, "Learn more", url);
            }
        });
    });
    ui.add_space(15.0);
    flipped
}

/// Privacy Settings is narrower than the other server dialogs: 400 wide, its title wrapping
/// beside the close button, and Done across the foot.
fn privacy(ctx: &egui::Context, name: &str, community: bool, add: impl FnOnce(&mut Ui)) -> (bool, bool) {
    let (mut close, mut done) = (false, false);
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(150, 150, 160, 51))).inner_margin(24);
    let m = crate::ui::motion::Modal::new("guild_privacy".into()).frame(frame).show(ctx, |ui| {
        ui.set_width(350.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let title = ui.painter().layout(format!("Privacy Settings\u{2014}{name}"), semi(20.0), pal().header_primary, 318.0);
        let (t, _) = ui.allocate_exact_size(vec2(350.0, title.size().y), Sense::hover());
        ui.painter().galley(t.min, title, pal().header_primary);
        let x = Rect::from_min_size(pos2(t.right() - 24.0, t.top() - 15.0), vec2(40.0, 40.0));
        let resp = ui.interact(x, ui.id().with("close"), Sense::click());
        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
        close = resp.clicked();
        ui.add_space(22.0);
        add(ui);
        if community {
            ui.add_space(1.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                ui.label(RichText::new("Learn about enhanced safety measures in Community Servers").font(reg(14.0)).color(pal().text_normal));
                link(ui, "here", POLICIES);
            });
            ui.add_space(24.0);
        }
        let (r, _) = ui.allocate_exact_size(vec2(350.0, 40.0), Sense::hover());
        done = plate(ui, r, "Done", true, true);
    });
    crate::ui::settings::mark_in(ctx, "modal:guild_privacy", m.response.rect);
    (close || m.should_close(), done)
}

fn row(ui: &mut Ui, title: &str, desc: &str, on: bool) -> bool {
    let w = ui.available_width();
    let g = ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, w - 80.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 22.0 + g.size().y + 16.0), Sense::hover());
    ui.painter().text(r.left_top() + vec2(0.0, 11.0), Align2::LEFT_CENTER, title, medium(16.0), pal().header_primary);
    ui.painter().galley(r.left_top() + vec2(0.0, 24.0), g, pal().header_secondary);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0))));
    switch(&mut c, on).clicked()
}

fn pause(app: &mut App, gid: Id, invites: bool, dms: bool) {
    let until = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let body = json!({
        "invites_disabled_until": invites.then_some(&until),
        "dms_disabled_until": dms.then_some(&until),
        "lockdown_duration_hours": (invites || dms).then_some(24),
    });
    app.rest("PUT", format!("/guilds/{gid}/incident-actions"), Some(body), Tag::reason());
}

pub fn privacy_set(app: &mut App, gid: Id, field: u64, off: bool) {
    let list = app.store.privacy.entry(field).or_default();
    let was = list.contains(&gid);
    if off {
        list.insert(gid);
    } else {
        list.remove(&gid);
    }
    let ids: Vec<Id> = list.iter().copied().collect();
    let raw = crate::proto::privacy_update(field, &ids);
    use base64::Engine;
    let body = json!({ "settings": base64::engine::general_purpose::STANDARD.encode(raw) });
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(body), Tag::undo(super::SAVE_FAILED, move |app: &mut App| {
        let list = app.store.privacy.entry(field).or_default();
        if was {
            list.insert(gid);
        } else {
            list.remove(&gid);
        }
    }));
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(m) = app.guild_modal.as_mut() else { return };
    let close;
    let mut toggled = None;
    let mut act = false;
    match m {
        Modal::Privacy(gid) => {
            let (name, community) = app.store.guilds.get(gid).map(|g| (g.name.clone(), g.features.iter().any(|f| f == "COMMUNITY"))).unwrap_or_default();
            let gid = *gid;
            (close, act) = privacy(ctx, &name, community, |ui| {
                for (field, title, desc, more) in PRIVACY {
                    let on = !app.store.privacy.get(&field).is_some_and(|l| l.contains(&gid));
                    if privacy_row(ui, title, desc, more, on) {
                        toggled = Some((field, on));
                    }
                }
            });
            if let Some((field, was_on)) = toggled {
                privacy_set(app, gid, field, was_on);
            }
        }
        Modal::Security { gid, invites, dms } => {
            let gid = *gid;
            close = modal(ctx, "guild_security", Some("Security Actions"), |ui| {
                if row(ui, "Pause Invites", "Stop new members joining with invites for 24 hours.", *invites) {
                    *invites = !*invites;
                }
                if row(ui, "Pause DMs", "Stop members sending direct messages to each other for 24 hours.", *dms) {
                    *dms = !*dms;
                }
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                act = plate(ui, Rect::from_min_size(pos2(r.right() - 96.0, r.top()), vec2(96.0, 40.0)), "Save", true, true);
            });
            if act {
                let (i, d) = (*invites, *dms);
                pause(app, gid, i, d);
            }
        }
        Modal::Raid(gid) => {
            let gid = *gid;
            close = modal(ctx, "guild_raid", Some("Report a Raid"), |ui| {
                ui.label(RichText::new("Takes automated actions to safeguard your server from potential raids or other suspicious activity. Report a raid that we miss to improve raid protection.").font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(24.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                act = plate(ui, Rect::from_min_size(pos2(r.right() - 140.0, r.top()), vec2(140.0, 40.0)), "Report Raid", true, true);
            });
            if act {
                app.rest("POST", format!("/guilds/{gid}/auto-moderation/report-raid"), None, Tag::reason());
                app.toast("Raid report submitted");
            }
        }
        Modal::Tag(gid) => {
            let gid = *gid;
            let mine = app.store.user(app.store.me).and_then(|u| u.tag.as_ref()).is_some_and(|t| t.1 == gid);
            let wearer = app.store.users.values().find(|u| u.tag.as_ref().is_some_and(|t| t.1 == gid)).cloned();
            close = modal(ctx, "guild_tag", Some("Server Tag"), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                if let Some(u) = &wearer {
                    super::tag_chip(ui.painter(), u, r.left_center(), 1.0);
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Adopt this tag to display it next to your name across Discord.").font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(24.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                act = plate(ui, Rect::from_min_size(pos2(r.right() - 140.0, r.top()), vec2(140.0, 40.0)), if mine { "Remove Tag" } else { "Adopt Tag" }, true, true);
            });
            if act {
                app.rest("PUT", "/users/@me/clan".into(), Some(json!({ "identity_guild_id": gid.to_string(), "identity_enabled": !mine })), Tag::reason());
            }
        }
    }
    if act || close {
        app.guild_modal = None;
    }
}
