use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, Hook, Sub, faint, heading, modal, rule};
use crate::net::Tag;
use crate::state::{Channel, Id, MANAGE_GUILD, sid};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{Confirm, menu_item, settings};

const HOURS: [(u64, &str); 4] = [(1, "1 Hour"), (6, "6 Hours"), (12, "12 Hours"), (24, "24 Hours")];

fn sm(ui: &mut Ui, text: &str, fill: Color32) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 28.0, 32.0), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { fill.gamma_multiply(0.85) } else { fill });
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(14.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn open_help(url: &str) {
    let _ = open::that_detached(url);
}

fn text_line(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
}

pub fn invites(app: &mut App, ui: &mut Ui, c: &Channel) {
    heading(ui, "Invites", "");
    let ch = c.id;
    let perms = app.store.perms(c);
    let paused = app.store.guilds.get(&c.guild).is_some_and(|g| g.features.iter().any(|f| f == "INVITES_DISABLED"));
    let mut list: Vec<Value> = app.chan_data("invites", format!("/channels/{ch}/invites")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    list.sort_by_key(|i| i["inviter"]["username"].as_str().unwrap_or_default().to_lowercase());
    let roles: Vec<(Id, String)> = app.store.guilds.get(&c.guild).map(|g| g.roles.values().map(|r| (r.id, r.name.clone())).collect()).unwrap_or_default();
    let can_create = perms & crate::state::CREATE_INVITE != 0;
    let mut create = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        text_line(ui, "Here's a list of all active invite links. You can revoke any one");
        if can_create {
            text_line(ui, " or ");
            create = settings::link(ui, "create one").clicked();
        }
        text_line(ui, ".");
    });
    ui.add_space(16.0);
    let mut pause = false;
    ui.horizontal(|ui| {
        if perms & MANAGE_GUILD != 0 {
            let b = if paused { sm(ui, "Enable Invites", BLURPLE) } else { sm(ui, "Pause Invites", RED) };
            settings::mark(ui, "chanset:pause", b.rect);
            pause = b.clicked();
        }
        if paused {
            ui.add_space(8.0);
            ui.label(RichText::new("⚠").font(reg(16.0)).color(Color32::from_rgb(0xf0, 0xb2, 0x32)));
            text_line(ui, "Invites to this server are currently paused.");
        }
    });
    ui.add_space(20.0);
    rule(ui);
    ui.add_space(16.0);
    if list.is_empty() {
        text_line(ui, "No active invite links");
    } else {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        for (x, h) in COLS {
            ui.painter().text(pos2(r.left() + r.width() * x, r.center().y), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary);
        }
    }
    let mut revoke = None;
    for inv in &list {
        let code = inv["code"].as_str().unwrap_or_default().to_string();
        let who = inv["inviter"]["global_name"].as_str().or(inv["inviter"]["username"].as_str()).unwrap_or("Unknown").to_string();
        let uses = match inv["max_uses"].as_u64().unwrap_or(0) {
            0 => inv["uses"].as_u64().unwrap_or(0).to_string(),
            m => format!("{}/{m}", inv["uses"].as_u64().unwrap_or(0)),
        };
        let expires = inv["expires_at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or("\u{221e}".to_string(), |t| {
            let s = (t.timestamp() - chrono::Utc::now().timestamp()).max(0);
            format!("{:02}:{:02}:{:02}:{:02}", s / 86400, s / 3600 % 24, s / 60 % 60, s % 60)
        });
        let granted = inv["roles"].as_array().into_iter().flatten().filter_map(|r| roles.iter().find(|x| x.0 == sid(&r["id"])).map(|x| x.1.clone())).collect::<Vec<_>>().join(", ");
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
        ui.painter().hline(r.x_range(), r.bottom() - 0.5, egui::Stroke::new(1.0, faint(31)));
        let cells = [(&who, pal().header_primary, 0.3), (&code, pal().text_normal, 0.22), (&uses, pal().text_normal, 0.12), (&expires, pal().text_normal, 0.18), (&granted, pal().text_normal, 0.18)];
        for ((text, col, w), (x, _)) in cells.into_iter().zip(COLS) {
            crate::ui::text::paint(ui.painter(), pos2(r.left() + r.width() * x + if x == 0.0 { 8.0 } else { 0.0 }, r.center().y), Align2::LEFT_CENTER, text, medium(16.0), col, r.width() * w - 12.0);
        }
        if resp.hovered() {
            let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
            ui.painter().circle_filled(x.center(), 10.0, RED);
            icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), Color32::WHITE);
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| q.x > r.right() - 32.0) {
                revoke = Some(code.clone());
            }
        }
    }
    if let Some(code) = revoke {
        app.rest("DELETE", format!("/invites/{code}"), None, Tag::Quiet);
        if let Some(s) = &mut app.chan_settings {
            s.data.remove("invites");
        }
    }
    if pause && let Some(s) = &mut app.chan_settings {
        s.pause = Some((!paused, false, 24));
    }
    if create {
        app.invite_people(ch);
    }
}

const COLS: [(f32, &str); 5] = [(0.0, "INVITER"), (0.3, "INVITE CODE"), (0.52, "USES"), (0.64, "EXPIRES"), (0.82, "ROLES")];

pub fn pause_modal(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.chan_settings.as_mut() else { return };
    let Some(p) = s.pause.as_mut() else { return };
    let gid = app.store.channels.get(&s.chan).map_or(0, |c| c.guild);
    let paused = app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "INVITES_DISABLED"));
    let (mut save, mut cancel) = (false, false);
    let close = modal(ctx, "chan_pause", "Security Actions", |ui| {
        let shown = HOURS.iter().find(|(h, _)| *h == p.2).map_or("24 Hours", |(_, l)| l);
        let w = ui.available_width();
        settings::dropdown(ui, w, None, shown, |ui| {
            for (h, l) in HOURS {
                if menu_item(ui, None, l, false) {
                    p.2 = h;
                }
            }
        });
        ui.add_space(16.0);
        settings::toggle(ui, &mut p.0, "Pause Invites", "Temporarily stop new members from joining this server via invite or vanity links.");
        settings::toggle(ui, &mut p.1, "Pause DMs", "Temporarily stop new direct messages from being sent between members in your server. Friends can still DM each other, moderators can still DM members, and your Apps can still DM members.");
        ui.add_space(8.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            save = ui.add_enabled(p.0 != paused || p.1, |ui: &mut Ui| settings::primary(ui, "Save")).clicked();
            ui.add_space(8.0);
            cancel = settings::secondary(ui, "Cancel").clicked();
        });
    });
    if save {
        let (invites, dms, hours) = *p;
        let until = (chrono::Utc::now() + chrono::Duration::hours(hours as i64)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let any = invites || dms;
        let body = json!({ "invites_disabled_until": invites.then_some(&until), "dms_disabled_until": dms.then_some(&until), "lockdown_duration_hours": any.then_some(hours) });
        app.rest("PUT", format!("/guilds/{gid}/incident-actions"), Some(body), Tag::Quiet);
    }
    if close || save || cancel {
        if let Some(s) = &mut app.chan_settings {
            s.pause = None;
        }
    }
}

fn card(ui: &mut Ui, icon: Icon, title: &str, sub: &str, key: &str, trailing: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut hit = false;
    let r = Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(28.0, 28.0)), pal().header_secondary);
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).font(medium(16.0)).color(pal().header_primary));
                ui.label(RichText::new(sub).font(reg(12.0)).color(pal().header_secondary));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| hit = trailing(ui));
        });
    });
    settings::mark(ui, key, r.response.rect);
    hit
}

fn chevron(ui: &mut Ui) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

pub fn integrations(app: &mut App, ui: &mut Ui, c: &Channel) {
    let ch = c.id;
    let sub = app.chan_settings.as_ref().map_or(Sub::Cards, |s| s.sub);
    let Some(all) = app.chan_data("webhooks", format!("/channels/{ch}/webhooks")).and_then(|v| v.as_array().cloned()) else {
        heading(ui, "Integrations", "");
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let (follow, hooks): (Vec<Value>, Vec<Value>) = all.into_iter().filter(|h| h["type"].as_u64() != Some(3)).partition(|h| h["type"].as_u64() == Some(2));
    match sub {
        Sub::Cards => {
            heading(ui, "Integrations", "");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                text_line(ui, "Customize your server with integrations. Manage webhooks and followed channels that post into this channel. ");
                if settings::link(ui, "Learn more about managing integrations.").clicked() {
                    open_help("https://support.discord.com/hc/en-us/articles/360045093012");
                }
            });
            ui.add_space(20.0);
            rule(ui);
            ui.add_space(20.0);
            let mut go = None;
            let wsub = plural(hooks.len(), "webhook");
            let made = card(ui, Icon::Link, "Webhooks", &wsub, "chanset:webhooks", |ui| {
                if hooks.is_empty() {
                    let b = sm(ui, "Create Webhook", BLURPLE);
                    settings::mark(ui, "chanset:create_webhook", b.rect);
                    b.clicked()
                } else {
                    go = chevron(ui).then_some(Sub::Webhooks);
                    false
                }
            });
            ui.add_space(16.0);
            let fsub = plural(follow.len(), "channel");
            let more = card(ui, Icon::Forward, "Channels Followed", &fsub, "chanset:followed", |ui| {
                if follow.is_empty() {
                    sm(ui, "Learn More", BLURPLE).clicked()
                } else {
                    go = chevron(ui).then_some(Sub::Followed);
                    false
                }
            });
            if more {
                open_help("https://support.discord.com/hc/en-us/articles/360028384531");
            }
            if made {
                app.rest("POST", format!("/channels/{ch}/webhooks"), Some(json!({ "name": "Captain Hook" })), Tag::Chan("hook_made"));
                go = Some(Sub::Webhooks);
            }
            if let (Some(g), Some(s)) = (go, app.chan_settings.as_mut()) {
                s.sub = g;
            }
        }
        Sub::Webhooks => webhooks(app, ui, c, &hooks),
        Sub::Followed => followed(app, ui, &follow),
    }
}

fn day(id: Id) -> String {
    chrono::DateTime::from_timestamp_millis(((id >> 22) + 1_420_070_400_000) as i64).map_or_else(String::new, |t| t.format("%m/%d/%Y").to_string())
}

pub fn hook_dirty(list: Option<&Value>, h: &Hook) -> bool {
    let Some(w) = list.and_then(|v| v.as_array()).and_then(|a| a.iter().find(|w| sid(&w["id"]) == h.id)) else { return false };
    w["name"].as_str() != Some(h.name.as_str()) || sid(&w["channel_id"]) != h.chan || h.avatar.is_some()
}

fn webhooks(app: &mut App, ui: &mut Ui, c: &Channel, hooks: &[Value]) {
    heading(ui, "Webhooks", "");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        text_line(ui, "Webhooks are a simple way to post messages from other apps and websites into Discord using internet magic. ");
        if settings::link(ui, "Learn more").clicked() {
            open_help("https://support.discord.com/hc/en-us/articles/228383668");
        }
        text_line(ui, " or try ");
        if settings::link(ui, "building one yourself.").clicked() {
            open_help("https://discord.com/developers/docs/resources/webhook");
        }
    });
    ui.add_space(20.0);
    rule(ui);
    ui.add_space(20.0);
    let ch = c.id;
    let mut make = false;
    if hooks.is_empty() {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("You have no webhooks!").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(12.0);
            make = sm(ui, "Create Webhook", BLURPLE).clicked();
        });
    } else {
        let b = sm(ui, "New Webhook", BLURPLE);
        settings::mark(ui, "chanset:new_webhook", b.rect);
        make = b.clicked();
        ui.add_space(16.0);
    }
    let chans: Vec<(Id, String)> = {
        let mut v: Vec<_> = app.store.channels.values().filter(|x| x.guild == c.guild && matches!(x.kind, 0 | 2 | 5 | 13 | 15 | 16)).map(|x| (x.id, x.name.clone())).collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v
    };
    let mut open = None;
    let mut delete = None;
    let Some(s) = app.chan_settings.as_mut() else { return };
    for h in hooks {
        let id = sid(&h["id"]);
        let name = h["name"].as_str().unwrap_or("Webhook").to_string();
        let editing = s.hook.as_ref().is_some_and(|x| x.id == id);
        let by = h["user"]["global_name"].as_str().or(h["user"]["username"].as_str()).map_or_else(String::new, |u| format!(" by {u}"));
        let resp = Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let top = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 20.0, BLURPLE);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, name.chars().next().unwrap_or('W'), semi(18.0), Color32::WHITE);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&name).font(medium(16.0)).color(pal().header_primary));
                    ui.label(RichText::new(format!("Created on {}{by}", day(id))).font(reg(12.0)).color(pal().header_secondary));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    crate::ui::motion::chevron(ui, ui.id().with(("hook_chev", id)), editing, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
                });
            });
            let head = ui.interact(top.response.rect, ui.id().with(("hook", id)), Sense::click());
            if head.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            settings::mark(ui, &format!("chanset:hook:{name}"), head.rect);
            if editing && let Some(e) = s.hook.as_mut() {
                ui.add_space(16.0);
                settings::h5(ui, "Name");
                settings::input(ui, &mut e.name, "", false);
                ui.add_space(16.0);
                settings::h5(ui, "Channel");
                let shown = chans.iter().find(|x| x.0 == e.chan).map_or("Unknown Channel".to_string(), |x| format!("#{}", x.1));
                let w = ui.available_width();
                settings::dropdown(ui, w, None, &shown, |ui| {
                    for (cid, n) in &chans {
                        if menu_item(ui, None, &format!("#{n}"), false) {
                            e.chan = *cid;
                        }
                    }
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if settings::secondary(ui, if e.avatar.is_some() { "Avatar Chosen" } else { "Change Avatar" }).clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
                        && let Ok(bytes) = std::fs::read(&path)
                    {
                        use base64::Engine;
                        let mime = match bytes.get(..4) {
                            Some([0x89, b'P', ..]) => "image/png",
                            Some([b'G', b'I', ..]) => "image/gif",
                            Some([b'R', b'I', ..]) => "image/webp",
                            _ => "image/jpeg",
                        };
                        e.avatar = Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
                    }
                    if let Some(t) = h["token"].as_str()
                        && settings::secondary(ui, "Copy Webhook URL").clicked()
                    {
                        ui.ctx().copy_text(format!("https://discord.com/api/webhooks/{id}/{t}"));
                    }
                    let d = settings::danger(ui, "Delete Webhook", false);
                    settings::mark(ui, "chanset:delete_webhook", d.rect);
                    if d.clicked() {
                        delete = Some((id, name.clone()));
                    }
                });
            }
            head.clicked()
        });
        if resp.inner {
            open = Some((id, name, sid(&h["channel_id"])));
        }
        ui.add_space(12.0);
    }
    if let Some((id, name, chan)) = open {
        s.hook = if s.hook.as_ref().is_some_and(|x| x.id == id) { None } else { Some(Hook { id, name, chan, avatar: None }) };
    }
    if make {
        app.rest("POST", format!("/channels/{ch}/webhooks"), Some(json!({ "name": "Captain Hook" })), Tag::Chan("hook_made"));
    }
    if let Some((id, name)) = delete {
        let body = format!("Are you sure you want to delete the **{name}** webhook? This action cannot be undone.");
        app.confirm = Some(Confirm::new("Delete Webhook", body, "Delete", move |app: &mut App| {
            app.rest("DELETE", format!("/webhooks/{id}"), None, Tag::Chan("hook_gone"));
        }));
    }
}

fn followed(app: &mut App, ui: &mut Ui, follow: &[Value]) {
    heading(ui, "Channels Followed", "");
    let mut unfollow = None;
    for h in follow {
        let id = sid(&h["id"]);
        let name = format!("{} · #{}", h["source_guild"]["name"].as_str().unwrap_or("Unknown Server"), h["source_channel"]["name"].as_str().unwrap_or("unknown"));
        Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).font(medium(16.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = settings::danger(ui, "Unfollow", false);
                    settings::mark(ui, "chanset:unfollow", b.rect);
                    if b.clicked() {
                        unfollow = Some((id, name.clone()));
                    }
                });
            });
        });
        ui.add_space(12.0);
    }
    if let Some((id, name)) = unfollow {
        let body = "Are you sure you want to unfollow this channel? You can refollow it in the future by visiting the server it's from.";
        app.confirm = Some(Confirm::new(format!("Unfollow {name}"), body, "Unfollow", move |app: &mut App| {
            app.rest("DELETE", format!("/webhooks/{id}"), None, Tag::Chan("hook_gone"));
        }));
    }
}
