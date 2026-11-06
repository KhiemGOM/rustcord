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

pub struct Consent {
    app: Id,
    name: String,
    icon: Option<String>,
    scopes: Vec<String>,
    reply: Option<tokio::sync::oneshot::Sender<bool>>,
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
    pub consent: Option<Consent>,
    pub template: Option<Template>,
}

impl State {
    pub fn modal(&self) -> bool {
        self.consent.is_some() || self.template.is_some()
    }

    pub fn clear(&mut self) {
        self.acts.clear();
        self.waiting.clear();
        self.consent = None;
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
        if let Msg::Consent { app, name, icon, scopes, reply } = m {
            self.show_window(ctx);
            if self.links.consent.is_none() {
                self.links.consent = Some(Consent { app, name, icon, scopes, reply: Some(reply) });
            } else {
                let _ = reply.send(false);
            }
            return;
        }
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
            Link::Channel { guild, channel, .. } if guild != 0 && !self.store.guilds.contains_key(&guild) => super::lurk::start(self, guild, channel),
            Link::Channel { .. } => self.toast("You can't open that channel."),
            Link::Invite(code) => self.invite = Some(format!("https://discord.gg/{code}")),
            Link::Template(code) => {
                self.rest("GET", format!("/guilds/templates/{code}"), None, Tag::Link("template"));
                self.links.template = Some(Template { code, info: None, failed: false, name: String::new(), busy: false });
            }
        }
    }

    pub(super) fn on_link_rest(&mut self, k: &'static str, res: Result<Value, String>) {
        match (k, res) {
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
    consent(app, ctx);
    template(app, ctx);
}

fn consent(app: &mut App, ctx: &egui::Context) {
    let Some(c) = &app.links.consent else { return };
    let mut answer = None;
    let mut rect = None;
    let modal = crate::ui::motion::Modal::new("link_consent".into()).frame(modal_frame()).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.vertical_centered(|ui| {
            if let Some(icon) = &c.icon {
                let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                egui::Image::from_uri(format!("{}/app-icons/{}/{icon}.png?size=128", crate::api::cdn(), c.app)).corner_radius(16).show_loading_spinner(false).paint_at(ui, r);
                ui.add_space(12.0);
            }
            ui.label(RichText::new(&c.name).font(semi(24.0)).color(pal().header_primary));
            ui.label(RichText::new("wants to access your Discord account").color(pal().header_secondary));
        });
        ui.add_space(16.0);
        ui.label(RichText::new("THIS WILL ALLOW THE APPLICATION TO").font(semi(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        for s in &c.scopes {
            let line = crate::rpc::scope_text(s).unwrap_or(s);
            ui.label(RichText::new(line).color(pal().text_normal));
            ui.add_space(4.0);
        }
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let b = button(ui, "Authorize", BLURPLE, 120.0);
            rect = Some(b.rect);
            if b.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                answer = Some(true);
            }
            if decline(ui, "Cancel") {
                answer = Some(false);
            }
        });
    });
    if let Some(r) = rect {
        app.rects.insert("link_consent:authorize".into(), r);
    }
    if modal.should_close() {
        answer = Some(false);
    }
    if let Some(yes) = answer
        && let Some(mut c) = app.links.consent.take()
        && let Some(reply) = c.reply.take()
    {
        let _ = reply.send(yes);
    }
}

fn template(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &mut app.links.template else { return };
    let (mut make, mut close) = (None, false);
    let mut rect = None;
    let modal = crate::ui::motion::Modal::new("link_template".into()).frame(modal_frame()).show(ctx, |ui| {
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
