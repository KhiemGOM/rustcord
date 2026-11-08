//! Add App from the directory: the consent screen Discord shows for an install link, done in the
//! app. Bot installs pick a server; commands-only ones install to the account.

use egui::{Align2, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, button, grey_button, motion};
use crate::net::Tag;
use crate::state::{Id, MANAGE_GUILD, sid};
use crate::theme::*;

pub struct AddApp {
    id: Id,
    name: String,
    icon: Option<String>,
    scopes: Vec<String>,
    perms: u64,
    guilds: Option<Vec<(Id, String)>>,
    pick: Option<Id>,
    busy: bool,
    err: Option<String>,
}

fn query(id: Id, scopes: &[String], perms: u64) -> String {
    let mut u = reqwest::Url::parse("http://q/").expect("static url");
    u.query_pairs_mut().append_pair("client_id", &id.to_string()).append_pair("scope", &scopes.join(" ")).append_pair("permissions", &perms.to_string());
    u.query().unwrap_or_default().to_string()
}

fn scope_line(s: &str) -> &'static str {
    match s {
        "bot" => "Add a bot to a server",
        "applications.commands" => "Use commands in a server",
        "identify" => "Access your username, avatar, and banner",
        "email" => "Access your email address",
        "guilds" => "Know what servers you're in",
        "guilds.join" => "Join servers for you",
        "webhook.incoming" => "Create a webhook in a channel",
        _ => "",
    }
}

pub fn open(app: &mut App, card: &Value) {
    let id = sid(&card["id"]);
    let scopes: Vec<String> = card["install_params"]["scopes"].as_array().map(|s| s.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).filter(|s: &Vec<String>| !s.is_empty()).unwrap_or_else(|| vec!["applications.commands".into()]);
    let perms = card["install_params"]["permissions"].as_str().and_then(|p| p.parse().ok()).unwrap_or(0);
    let icon = card["icon"].as_str().map(|h| format!("{}/app-icons/{id}/{h}.png?size=128", crate::api::cdn()));
    let pick = app.ui.guild.filter(|g| app.store.my_perms(*g) & MANAGE_GUILD != 0);
    app.rest("GET", format!("/oauth2/authorize?{}", query(id, &scopes, perms)), None, Tag::AddApp(true));
    app.add_app = Some(AddApp { id, name: card["name"].as_str().unwrap_or("this app").to_string(), icon, scopes, perms, guilds: None, pick, busy: false, err: None });
}

pub fn info(app: &mut App, v: &Value) {
    let Some(a) = &mut app.add_app else { return };
    let mut list: Vec<(Id, String)> = v["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["permissions"].as_str().and_then(|p| p.parse::<u64>().ok()).is_some_and(|p| p & (MANAGE_GUILD | crate::state::ADMINISTRATOR) != 0))
        .map(|g| (sid(&g["id"]), g["name"].as_str().unwrap_or_default().to_string()))
        .collect();
    if v["guilds"].is_null() {
        list = app.store.guild_order.iter().filter(|g| app.store.my_perms(**g) & MANAGE_GUILD != 0).filter_map(|g| app.store.guilds.get(g).map(|g| (g.id, g.name.clone()))).collect();
    }
    if a.pick.is_some_and(|p| !list.iter().any(|(g, _)| *g == p)) {
        a.pick = None;
    }
    a.guilds = Some(list);
}

pub fn done(app: &mut App, ok: Result<(), String>) {
    let Some(a) = &mut app.add_app else { return };
    a.busy = false;
    match ok {
        Ok(()) => {
            let msg = match a.pick.and_then(|g| app.store.guilds.get(&g)) {
                Some(g) => format!("{} was added to {}.", a.name, g.name),
                None => format!("{} was added to your account.", a.name),
            };
            app.add_app = None;
            app.toast(msg);
        }
        Err(e) => a.err = Some(e),
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(a) = &mut app.add_app else { return };
    let bot = a.scopes.iter().any(|s| s == "bot");
    let (mut cancel, mut go) = (false, false);
    let frame = Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(12).inner_margin(Margin::same(24));
    let m = motion::Modal::new("add_app".into()).backdrop_color(egui::Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(12), pal().bg_mod_selected);
            if let Some(u) = &a.icon {
                egui::Image::from_uri(u.clone()).corner_radius(CornerRadius::same(12)).show_loading_spinner(false).paint_at(ui, r);
            }
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(&a.name).font(semi(20.0)).color(pal().header_primary));
                ui.label(RichText::new(if bot { "wants to join your server" } else { "wants to access your Discord account" }).font(reg(14.0)).color(pal().header_secondary));
            });
        });
        ui.add_space(16.0);
        if bot {
            ui.label(RichText::new("ADD TO SERVER").font(bold(12.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let (r, resp) = ui.allocate_exact_size(vec2(400.0, 40.0), Sense::click());
            super::settings::mark(ui, "addapp:server", r);
            ui.painter().rect(r, 8, egui::Color32::from_black_alpha(31), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let name = a.pick.and_then(|p| a.guilds.as_ref()?.iter().find(|(g, _)| *g == p).map(|(_, n)| n.as_str()));
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, name.unwrap_or("Select a server"), reg(16.0), if name.is_some() { pal().text_normal } else { pal().text_muted });
            icons::draw(ui.painter(), Icon::ChevronDown, egui::Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), pal().header_secondary);
            egui::Popup::menu(&resp).width(400.0).show(|ui| {
                super::menu_body(ui, |ui| match &a.guilds {
                    None => {
                        ui.label(RichText::new("Loading…").font(reg(14.0)).color(pal().text_muted));
                    }
                    Some(l) if l.is_empty() => {
                        ui.label(RichText::new("You don't manage any servers.").font(reg(14.0)).color(pal().text_muted));
                    }
                    Some(l) => {
                        for (g, n) in l {
                            if super::menu_row(ui, None, n, None, false, super::Trail::None).clicked() {
                                a.pick = Some(*g);
                            }
                        }
                    }
                });
            });
            ui.add_space(16.0);
        }
        ui.label(RichText::new(format!("This will allow {} to:", a.name)).font(medium(14.0)).color(pal().header_primary));
        ui.add_space(8.0);
        for s in a.scopes.iter().filter(|s| !scope_line(s).is_empty()) {
            check(ui, scope_line(s));
        }
        if bot && a.perms != 0 {
            for (_, perms) in super::server::perms::SECTIONS {
                for (_, name, _) in perms.iter().filter(|(b, _, _)| a.perms & b != 0) {
                    check(ui, name);
                }
            }
        }
        if let Some(e) = &a.err {
            ui.add_space(8.0);
            ui.label(RichText::new(e).font(reg(14.0)).color(pal().critical));
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            cancel = grey_button(ui, "Cancel", 192.0).clicked();
            let ready = !a.busy && (!bot || a.pick.is_some());
            let ok = ui.add_enabled_ui(ready, |ui| button(ui, "Authorize", pal().blurple, 192.0)).inner;
            super::settings::mark(ui, "addapp:authorize", ok.rect);
            go = ok.clicked();
        });
    });
    let (id, scopes, perms, pick) = (a.id, a.scopes.clone(), a.perms, a.pick);
    if go {
        a.busy = true;
        a.err = None;
        let body = match pick.filter(|_| bot) {
            Some(g) => json!({ "guild_id": g.to_string(), "permissions": perms.to_string(), "authorize": true }),
            None => json!({ "permissions": "0", "authorize": true, "integration_type": 1 }),
        };
        app.rest("POST", format!("/oauth2/authorize?{}", query(id, &scopes, perms)), Some(body), Tag::AddApp(false));
    }
    if cancel || m.should_close() {
        app.add_app = None;
    }
}

fn check(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        icons::draw(ui.painter(), Icon::Check, r, pal().green);
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().text_normal)).wrap());
    });
    ui.add_space(4.0);
}
