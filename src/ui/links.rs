//! What the local services ask of the window (see `crate::rpc`): a game's rich presence going
//! out as our status, an invite or a server template a website wants to show, and
//! `discord://` links.

use std::sync::mpsc::Receiver;

use egui::{Align, Frame, Key, Layout, RichText, Sense, vec2};
use serde_json::{Value, json};

use super::{App, button};
use crate::games::Game;
use crate::net::Tag;
use crate::rpc::Msg;
use crate::rpc::link::Link;
use crate::state::Id;
use crate::theme::*;

struct Act {
    conn: u64,
    game: Game,
    activity: Value,
}

pub struct Invite {
    code: String,
    pub info: Option<Value>,
    failed: bool,
}

pub struct Template {
    code: String,
    pub info: Option<Value>,
    failed: bool,
    name: String,
    busy: bool,
}

#[derive(Default)]
pub struct State {
    pub rx: Option<Receiver<Msg>>,
    acts: Vec<Act>,
    rev: u64,
    waiting: Vec<Msg>,
    pub invite: Option<Invite>,
    pub template: Option<Template>,
}

impl State {
    pub fn modal(&self) -> bool {
        self.invite.is_some() || self.template.is_some()
    }

    pub fn clear(&mut self) {
        self.acts.clear();
        self.waiting.clear();
        self.invite = None;
        self.template = None;
        self.rev += 1;
    }
}

impl App {
    pub(super) fn links_pump(&mut self, ctx: &egui::Context) {
        let msgs: Vec<Msg> = self.links.rx.as_ref().map(|r| r.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                Msg::Activity { conn, app, name, icon, activity } => {
                    self.links.acts.retain(|a| a.conn != conn);
                    if let Some(activity) = activity {
                        let start = activity["timestamps"]["start"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                        self.links.acts.push(Act { conn, game: Game { id: app, name, icon, start }, activity });
                    }
                    self.links.rev += 1;
                    self.refresh_game();
                }
                Msg::CloseJoinRequest(user) => {
                    let dm = self.store.dms.iter().copied().find(|c| self.store.channels.get(c).is_some_and(|c| c.recipients == [user]));
                    if let Some(dm) = dm {
                        self.mark_read(&[dm]);
                    }
                }
                other => self.links.waiting.push(other),
            }
        }
        if self.store.me != 0 && !self.links.waiting.is_empty() {
            for m in std::mem::take(&mut self.links.waiting) {
                self.handle(ctx, m);
            }
        }
    }

    /// The game the account card shows: a game's rich presence over a program we spotted.
    pub(super) fn refresh_game(&mut self) {
        self.game = self.links.acts.first().map(|a| a.game.clone()).or_else(|| self.detected.clone());
    }

    /// Rich presence first, then the detected game unless a rich presence is already for it.
    pub(super) fn activities(&self) -> Vec<Value> {
        let mut out: Vec<Value> = self.links.acts.iter().map(|a| a.activity.clone()).collect();
        if let Some(g) = &self.detected
            && !self.links.acts.iter().any(|a| a.game.id == g.id)
        {
            out.push(json!({ "name": g.name, "type": 0, "application_id": g.id.to_string(), "timestamps": { "start": g.start } }));
        }
        out
    }

    /// What the gateway has from us: the detected game, and how many times rich presence changed.
    pub(super) fn presence_key(&self) -> (Option<Id>, u64) {
        if self.store.hide_game { (None, 0) } else { (self.detected.as_ref().map(|g| g.id), self.links.rev) }
    }

    fn handle(&mut self, ctx: &egui::Context, m: Msg) {
        let link = match m {
            Msg::Invite(c) => Link::Invite(c),
            Msg::Template(c) => Link::Template(c),
            Msg::Open(l) => l,
            _ => return,
        };
        self.show_window(ctx);
        match link {
            Link::Focus => {}
            Link::Channel { guild, channel, message } if self.store.channels.contains_key(&channel) => {
                if message != 0 {
                    self.jump_to(channel, message);
                } else {
                    self.ui.guild = (guild != 0).then_some(guild);
                    self.open(channel);
                }
            }
            Link::Channel { .. } => self.toast("You can't open that channel."),
            Link::Invite(code) => {
                self.rest("GET", format!("/invites/{code}?with_counts=true&with_expiration=true"), None, Tag::Link("invite"));
                self.links.invite = Some(Invite { code, info: None, failed: false });
            }
            Link::Template(code) => {
                self.rest("GET", format!("/guilds/templates/{code}"), None, Tag::Link("template"));
                self.links.template = Some(Template { code, info: None, failed: false, name: String::new(), busy: false });
            }
        }
    }

    pub(super) fn on_link_rest(&mut self, k: &'static str, res: Result<Value, String>) {
        match (k, res) {
            ("invite", r) => {
                if let Some(i) = &mut self.links.invite {
                    i.failed = r.is_err();
                    i.info = r.ok();
                }
            }
            ("template", r) => {
                let name = self.store.user(self.store.me).map(|u| format!("{}'s server", u.display())).unwrap_or_default();
                if let Some(t) = &mut self.links.template {
                    t.failed = r.is_err();
                    t.info = r.ok();
                    t.name = name;
                }
            }
            (_, Ok(v)) => {
                self.links.template = None;
                self.toast(format!("Created {}", v["name"].as_str().unwrap_or("your server")));
            }
            (_, Err(e)) => {
                if let Some(t) = &mut self.links.template {
                    t.busy = false;
                }
                self.toast(e);
            }
        }
    }
}

fn modal_frame() -> Frame {
    Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16)
}

fn heading(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
        ui.add_space(8.0);
        ui.label(RichText::new(body).color(pal().header_secondary));
    });
}

fn decline(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(egui::Label::new(RichText::new(text).color(pal().header_primary)).sense(Sense::click())).clicked()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    invite(app, ctx);
    template(app, ctx);
}

fn invite(app: &mut App, ctx: &egui::Context) {
    let Some(inv) = &app.links.invite else { return };
    let (mut accept, mut close) = (None, false);
    let mut rect = None;
    let modal = egui::Modal::new("link_invite".into()).frame(modal_frame()).show(ctx, |ui| {
        ui.set_width(440.0);
        match (&inv.info, inv.failed) {
            (Some(v), _) => {
                let name = v["guild"]["name"].as_str().or(v["channel"]["name"].as_str()).unwrap_or("a server");
                let who = v["inviter"]["global_name"].as_str().or(v["inviter"]["username"].as_str());
                ui.vertical_centered(|ui| {
                    if let (Some(g), Some(icon)) = (v["guild"]["id"].as_str(), v["guild"]["icon"].as_str()) {
                        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                        egui::Image::from_uri(format!("{}/icons/{g}/{icon}.png?size=128", crate::api::cdn())).corner_radius(16).show_loading_spinner(false).paint_at(ui, r);
                        ui.add_space(12.0);
                    }
                    let line = who.map_or("You've been invited to join".to_string(), |w| format!("{w} invited you to join"));
                    ui.label(RichText::new(line).font(medium(12.0)).color(pal().header_secondary));
                    ui.label(RichText::new(name).font(semi(24.0)).color(pal().header_primary));
                    if let (Some(on), Some(all)) = (v["approximate_presence_count"].as_u64(), v["approximate_member_count"].as_u64()) {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("{on} Online    {all} Members")).font(medium(14.0)).color(pal().text_muted));
                    }
                });
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = button(ui, "Accept Invite", BLURPLE, 140.0);
                    rect = Some(b.rect);
                    if b.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        accept = Some(inv.code.clone());
                    }
                    close = decline(ui, "No thanks");
                });
            }
            (None, true) => {
                heading(ui, "Invite Invalid", "The invite is invalid or has expired.");
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close = button(ui, "Okay", BLURPLE, 120.0).clicked();
                });
            }
            (None, false) => {
                ui.vertical_centered(|ui| ui.add(egui::Spinner::new()));
            }
        }
    });
    if let Some(r) = rect {
        app.rects.insert("link_invite:accept".into(), r);
    }
    if let Some(code) = accept {
        app.rest("POST", format!("/invites/{code}"), Some(json!({})), Tag::Invite);
        close = true;
    }
    if close || modal.should_close() {
        app.links.invite = None;
    }
}

fn template(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &mut app.links.template else { return };
    let (mut make, mut close) = (None, false);
    let mut rect = None;
    let modal = egui::Modal::new("link_template".into()).frame(modal_frame()).show(ctx, |ui| {
        ui.set_width(440.0);
        match (&t.info, t.failed) {
            (Some(v), _) => {
                heading(ui, "Create Your Server", v["name"].as_str().unwrap_or("Server template"));
                if let Some(d) = v["description"].as_str().filter(|d| !d.is_empty()) {
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new(d).color(pal().text_muted)));
                }
                ui.add_space(16.0);
                ui.label(RichText::new("SERVER NAME").font(semi(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                input_frame().corner_radius(3).inner_margin(10).show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut t.name).frame(Frame::NONE).desired_width(ui.available_width()));
                });
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = button(ui, "Create", BLURPLE, 120.0);
                    rect = Some(b.rect);
                    if (b.clicked() || ui.input(|i| i.key_pressed(Key::Enter))) && !t.busy && !t.name.trim().is_empty() {
                        make = Some((t.code.clone(), t.name.trim().to_string()));
                        t.busy = true;
                    }
                    close = decline(ui, "No Thanks");
                });
            }
            (None, true) => {
                heading(ui, "Invalid server template", "This template link is invalid or has expired.");
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close = button(ui, "Okay", BLURPLE, 120.0).clicked();
                });
            }
            (None, false) => {
                ui.vertical_centered(|ui| ui.add(egui::Spinner::new()));
            }
        }
    });
    if let Some(r) = rect {
        app.rects.insert("link_template:create".into(), r);
    }
    if let Some((code, name)) = make {
        app.rest("POST", format!("/guilds/templates/{code}"), Some(json!({ "name": name, "icon": null })), Tag::Link("made"));
    }
    if close || modal.should_close() {
        app.links.template = None;
    }
}
