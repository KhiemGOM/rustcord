use egui::{Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, faint, title};
use crate::net::Tag;
use crate::state::{Id, sid, snowflake_ms};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

#[derive(Default)]
pub struct Apps {
    query: String,
    hook: Option<Hook>,
    manage: Option<Id>,
}

struct Hook {
    id: Id,
    name: String,
    channel: Id,
    avatar: Option<Option<String>>,
}

pub fn created(app: &mut App, v: Value) {
    let Some(s) = &mut app.server else { return };
    s.data.remove("webhooks");
    s.pages.apps.hook = Some(Hook { id: sid(&v["id"]), name: v["name"].as_str().unwrap_or_default().into(), channel: sid(&v["channel_id"]), avatar: None });
}

impl Apps {
    pub fn deep(&self) -> bool {
        self.manage.is_some()
    }
}

fn h2(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(semi(20.0)).color(pal().header_primary));
}

fn count(ui: &mut Ui, n: usize, one: &str) {
    ui.label(RichText::new(format!("{n} {one}{}", if n == 1 { "" } else { "s" })).font(reg(14.0)).color(pal().header_secondary));
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    if app.server.as_ref().is_some_and(|s| s.pages.apps.manage.is_some()) {
        return manage(app, ui, gid);
    }
    title(ui, "Integrations", "Customize your server with integrations. Manage webhooks, followed channels, and apps, as well as Twitch and YouTube settings for creators.");
    if settings::link(ui, "Learn more about managing integrations.").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360045093012"));
    }
    ui.add_space(24.0);
    let hooks: Vec<Value> = app.server_data("webhooks", format!("/guilds/{gid}/webhooks")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let (follows, hooks): (Vec<Value>, Vec<Value>) = hooks.into_iter().partition(|h| h["type"].as_u64() == Some(2));
    h2(ui, "Webhooks");
    ui.horizontal(|ui| {
        count(ui, hooks.len(), "webhook");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if settings::primary(ui, "Create Webhook").clicked() {
                let ch = app.store.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default().into_iter().filter_map(|c| app.store.channels.get(&c)).filter(|c| c.kind == 0).min_by_key(|c| c.position).map(|c| c.id);
                if let Some(ch) = ch {
                    app.rest("POST", format!("/channels/{ch}/webhooks"), Some(json!({ "name": "Spidey Bot" })), Tag::Server("webhook_new"));
                }
            }
        });
    });
    ui.add_space(12.0);
    for h in &hooks {
        webhook(app, ui, gid, h);
    }
    ui.add_space(24.0);
    h2(ui, "Channels Followed");
    ui.horizontal(|ui| {
        count(ui, follows.len(), "channel");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if settings::secondary(ui, "Learn More").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360028384531"));
            }
        });
    });
    ui.add_space(12.0);
    let mut unfollow = None;
    for f in &follows {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        ui.painter().rect(r, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
        let from = f["source_channel"]["name"].as_str().unwrap_or("channel");
        let guild = f["source_guild"]["name"].as_str().unwrap_or("a server");
        ui.painter().text(pos2(r.left() + 16.0, r.center().y - 9.0), Align2::LEFT_CENTER, format!("#{from}"), semi(16.0), pal().header_primary);
        ui.painter().text(pos2(r.left() + 16.0, r.center().y + 11.0), Align2::LEFT_CENTER, format!("from {guild}"), reg(12.0), pal().header_secondary);
        let x = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(28.0, 28.0));
        let xr = ui.interact(x, ui.id().with(("unfollow", sid(&f["id"]))), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x.shrink(6.0), if xr.hovered() || resp.hovered() { RED } else { pal().header_secondary });
        if xr.on_hover_text("Unfollow").clicked() {
            unfollow = Some(sid(&f["id"]));
        }
        ui.add_space(8.0);
    }
    if let Some(id) = unfollow {
        app.rest("DELETE", format!("/webhooks/{id}"), None, Tag::Quiet);
        if let Some(s) = &mut app.server {
            s.data.remove("webhooks");
        }
    }
    ui.add_space(24.0);
    h2(ui, "Bots and Apps");
    ui.add_space(12.0);
    {
        let q = &mut app.server.as_mut().unwrap().pages.apps.query;
        settings::input(ui, q, "Search installed apps", false);
    }
    ui.add_space(16.0);
    bots(app, ui, gid);
}

fn bots(app: &mut App, ui: &mut Ui, gid: Id) {
    let list: Vec<Value> = app.server_data("integrations", format!("/guilds/{gid}/integrations?include_applications=true&include_role_connections_metadata=true")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let cmds: std::collections::HashSet<Id> = app.server_data("command_index", format!("/guilds/{gid}/application-command-index")).and_then(|v| v["application_commands"].as_array()).map(|a| a.iter().map(|c| sid(&c["application_id"])).collect()).unwrap_or_default();
    let q = app.server.as_ref().map(|s| s.pages.apps.query.trim().to_lowercase()).unwrap_or_default();
    let mut open = None;
    for i in list.iter().filter(|i| i["application"].is_object()) {
        let a = &i["application"];
        let name = a["name"].as_str().unwrap_or_default();
        if !q.is_empty() && !name.to_lowercase().contains(&q) {
            continue;
        }
        let aid = sid(&a["id"]);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::click());
        ui.painter().rect(r, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
        if let Some(ic) = a["icon"].as_str() {
            egui::Image::from_uri(format!("{}/app-icons/{aid}/{ic}.png?size=96", crate::api::cdn())).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 16.0), vec2(64.0, 64.0)));
        } else {
            ui.painter().rect_filled(Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 16.0), vec2(64.0, 64.0)), CornerRadius::same(8), pal().bg_accent);
        }
        ui.painter().text(pos2(r.left() + 96.0, r.top() + 24.0), Align2::LEFT_CENTER, name, semi(16.0), pal().header_primary);
        let by = i["user"]["username"].as_str().map(|u| format!(" by {u}")).unwrap_or_default();
        let when = chrono::DateTime::from_timestamp_millis(snowflake_ms(sid(&i["id"]))).map(|t| t.format("%b %-d, %Y").to_string()).unwrap_or_default();
        ui.painter().text(pos2(r.left() + 96.0, r.top() + 46.0), Align2::LEFT_CENTER, format!("Added on {when}{by}"), reg(12.0), pal().header_secondary);
        let mut x = r.left() + 96.0;
        let mut feats = Vec::new();
        if a["bot"]["public_flags"].as_u64().unwrap_or(0) & 65536 != 0 || a["flags"].as_u64().unwrap_or(0) & (1 << 23) != 0 {
            feats.push("Verified Bot");
        } else if a["bot"].is_object() {
            feats.push("Bot");
        }
        if cmds.contains(&aid) {
            feats.push("Commands");
        }
        for f in feats {
            let g = ui.painter().layout_no_wrap(f.into(), medium(12.0), pal().text_normal);
            let chip = Rect::from_min_size(pos2(x, r.top() + 62.0), vec2(g.size().x + 16.0, 22.0));
            ui.painter().rect_filled(chip, CornerRadius::same(4), faint(41));
            ui.painter().galley(pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0), g, pal().text_normal);
            x = chip.right() + 6.0;
        }
        ui.painter().text(pos2(r.right() - 24.0, r.center().y), Align2::RIGHT_CENTER, "Manage", medium(14.0), pal().link);
        if resp.clicked() {
            open = Some(i.clone());
        }
        ui.add_space(8.0);
    }
    if list.is_empty() {
        ui.label(RichText::new("No apps have been installed in this server yet.").font(reg(14.0)).color(pal().header_secondary));
    }
    if let Some(i) = open {
        if let Some(s) = &mut app.server {
            s.pages.apps.manage = Some(sid(&i["id"]));
        }
    }
}

fn manage(app: &mut App, ui: &mut Ui, gid: Id) {
    let id = app.server.as_ref().and_then(|s| s.pages.apps.manage).unwrap_or(0);
    let list: Vec<Value> = app.server_data("integrations", format!("/guilds/{gid}/integrations?include_applications=true&include_role_connections_metadata=true")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let Some(i) = list.iter().find(|i| sid(&i["id"]) == id).cloned() else {
        if let Some(s) = &mut app.server {
            s.pages.apps.manage = None;
        }
        return;
    };
    if ui.add(egui::Label::new(RichText::new("\u{2190} Back").font(semi(14.0)).color(pal().header_secondary)).sense(Sense::click())).clicked()
        && let Some(s) = &mut app.server
    {
        s.pages.apps.manage = None;
    }
    ui.add_space(16.0);
    let a = &i["application"];
    let name = a["name"].as_str().unwrap_or_default().to_string();
    title(ui, &name, a["description"].as_str().unwrap_or_default());
    let bot = sid(&a["bot"]["id"]);
    let role = app.store.guilds.get(&gid).and_then(|g| g.roles.values().find(|r| r.name == name && r.managed)).map(|r| r.name.clone());
    ui.label(RichText::new("Added").font(bold(12.0)).color(pal().header_secondary));
    let when = chrono::DateTime::from_timestamp_millis(snowflake_ms(sid(&i["id"]))).map(|t| t.format("%b %-d, %Y").to_string()).unwrap_or_default();
    ui.label(RichText::new(format!("{when} by {}", i["user"]["username"].as_str().unwrap_or("unknown"))).font(reg(16.0)).color(pal().text_normal));
    ui.add_space(16.0);
    if let Some(r) = role {
        ui.label(RichText::new("Role").font(bold(12.0)).color(pal().header_secondary));
        ui.label(RichText::new(r).font(reg(16.0)).color(pal().text_normal));
        ui.add_space(16.0);
    }
    ui.label(RichText::new("Scopes").font(bold(12.0)).color(pal().header_secondary));
    let scopes: Vec<&str> = i["scopes"].as_array().into_iter().flatten().filter_map(|s| s.as_str()).collect();
    ui.label(RichText::new(if scopes.is_empty() { "bot".to_string() } else { scopes.join(", ") }).font(reg(16.0)).color(pal().text_normal));
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        if settings::secondary(ui, "Open in App Directory").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://discord.com/application-directory/{}", sid(&a["id"]))));
        }
        if bot != 0 && settings::danger(ui, "Remove App", false).clicked() {
            app.confirm = Some(crate::ui::Confirm::new(format!("Remove {name}"), format!("Are you sure you want to remove **{name}** from this server? Its commands and role will go with it."), "Remove", move |app: &mut App| {
                app.rest("DELETE", format!("/guilds/{gid}/members/{bot}"), None, Tag::Quiet);
                if let Some(s) = &mut app.server {
                    s.pages.apps.manage = None;
                    s.data.remove("integrations");
                }
            }));
        }
    });
}

fn webhook(app: &mut App, ui: &mut Ui, gid: Id, h: &Value) {
    let id = sid(&h["id"]);
    let open = app.server.as_ref().and_then(|s| s.pages.apps.hook.as_ref()).is_some_and(|k| k.id == id);
    let name = h["name"].as_str().unwrap_or("Webhook").to_string();
    let channel = sid(&h["channel_id"]);
    let cname = app.store.channels.get(&channel).map(|c| format!("#{}", c.name)).unwrap_or_default();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    ui.painter().rect(r, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    let av = Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 12.0), vec2(40.0, 40.0));
    match h["avatar"].as_str() {
        Some(a) => crate::ui::avatar(ui, av, &format!("{}/avatars/{id}/{a}.png?size=64", crate::api::cdn())),
        None => {
            ui.painter().circle_filled(av.center(), 20.0, BLURPLE);
            icons::draw(ui.painter(), Icon::Discord, av.shrink(8.0), egui::Color32::WHITE);
        }
    }
    ui.painter().text(pos2(r.left() + 72.0, r.center().y - 9.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary);
    ui.painter().text(pos2(r.left() + 72.0, r.center().y + 11.0), Align2::LEFT_CENTER, &cname, reg(12.0), pal().header_secondary);
    icons::draw(ui.painter(), if open { Icon::ChevronDown } else { Icon::ChevronRight }, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    if resp.clicked() {
        if let Some(s) = &mut app.server {
            let k = &mut s.pages.apps.hook;
            *k = if open { None } else { Some(Hook { id, name: name.clone(), channel, avatar: None }) };
        }
    }
    if open {
        edit_hook(app, ui, gid, h);
    }
    ui.add_space(8.0);
}

fn edit_hook(app: &mut App, ui: &mut Ui, gid: Id, h: &Value) {
    let Some(mut k) = app.server.as_mut().and_then(|s| s.pages.apps.hook.take()) else { return };
    let id = k.id;
    let (orig_name, orig_chan) = (h["name"].as_str().unwrap_or("").to_string(), sid(&h["channel_id"]));
    let chans: Vec<(Id, String)> = app.store.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default().into_iter().filter_map(|c| app.store.channels.get(&c)).filter(|c| matches!(c.kind, 0 | 5)).map(|c| (c.id, c.name.clone())).collect();
    let (mut save, mut delete, mut copy) = (false, false, false);
    Frame::new().fill(pal().bg_primary).stroke(Stroke::new(1.0, faint(51))).corner_radius(8).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::click());
                ui.painter().circle_filled(r.center(), 40.0, BLURPLE);
                match (&k.avatar, h["avatar"].as_str()) {
                    (Some(Some(_)), _) => icons::draw(ui.painter(), Icon::Image, r.shrink(24.0), egui::Color32::WHITE),
                    (Some(None), _) | (None, None) => icons::draw(ui.painter(), Icon::Discord, r.shrink(20.0), egui::Color32::WHITE),
                    (None, Some(a)) => crate::ui::avatar(ui, r, &format!("{}/avatars/{id}/{a}.png?size=128", crate::api::cdn())),
                }
                if resp.on_hover_text("Change avatar").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
                    && let Ok(bytes) = std::fs::read(&p)
                {
                    use base64::Engine;
                    let mime = if bytes.starts_with(b"GIF") { "image/gif" } else if bytes.starts_with(&[0x89, b'P']) { "image/png" } else { "image/jpeg" };
                    k.avatar = Some(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))));
                }
                if (h["avatar"].is_string() || matches!(k.avatar, Some(Some(_)))) && settings::link(ui, "Remove").clicked() {
                    k.avatar = Some(None);
                }
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                settings::h5(ui, "Name");
                settings::input(ui, &mut k.name, "", false);
                ui.add_space(12.0);
                settings::h5(ui, "Channel");
                let shown = chans.iter().find(|c| c.0 == k.channel).map(|c| format!("#{}", c.1)).unwrap_or_default();
                settings::dropdown(ui, ui.available_width().min(360.0), Some(Icon::Hash), &shown, |ui| {
                    crate::ui::menu_body(ui, |ui| {
                        for (c, n) in &chans {
                            if crate::ui::menu_item(ui, (*c == k.channel).then_some(Icon::Check), n, false) {
                                k.channel = *c;
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(16.0);
        let dirty = k.name.trim() != orig_name || k.channel != orig_chan || k.avatar.is_some();
        ui.horizontal(|ui| {
            if dirty && settings::primary(ui, "Save Changes").clicked() {
                save = true;
            }
            copy = settings::secondary(ui, "Copy Webhook URL").clicked();
            delete = settings::danger(ui, "Delete Webhook", false).clicked();
        });
    });
    if copy {
        ui.ctx().copy_text(format!("https://discord.com/api/webhooks/{id}/{}", h["token"].as_str().unwrap_or_default()));
        app.toast("Copied");
    }
    if save {
        let mut body = json!({ "name": k.name.trim(), "channel_id": k.channel.to_string() });
        if let Some(a) = &k.avatar {
            body["avatar"] = json!(a);
        }
        app.rest("PATCH", format!("/webhooks/{id}"), Some(body), Tag::Quiet);
        if let Some(s) = &mut app.server {
            s.data.remove("webhooks");
        }
        return;
    }
    if delete {
        let name = orig_name.clone();
        app.confirm = Some(crate::ui::Confirm::new("Delete Webhook", format!("Are you sure you want to delete **{name}**? This cannot be undone."), "Delete", move |app: &mut App| {
            app.rest("DELETE", format!("/webhooks/{id}"), None, Tag::Quiet);
            if let Some(s) = &mut app.server {
                s.pages.apps.hook = None;
                s.data.remove("webhooks");
            }
        }));
    }
    if let Some(s) = &mut app.server {
        s.pages.apps.hook = Some(k);
    }
}
