use egui::{Sense, Ui, vec2};
use std::time::{Duration, Instant};

use super::{Form, Page, input, label, link, primary, problem, secondary};
use crate::auth::{Challenge, Cmd as Auth};
use crate::net::Cmd;
use crate::theme::*;
use crate::ui::App;

pub struct State {
    pub challenge: Challenge,
    pub kind: &'static str,
    pub code: String,
    pub phone: Option<String>,
    pub sending: bool,
    pub err: Option<String>,
    pub busy: bool,
    warned: Instant,
}

impl Default for State {
    fn default() -> Self {
        State { challenge: Challenge::default(), kind: "totp", code: String::new(), phone: None, sending: false, err: None, busy: false, warned: Instant::now() }
    }
}

impl State {
    fn methods(&self) -> Vec<&'static str> {
        let c = &self.challenge;
        [
            (c.webauthn.is_some(), "webauthn"),
            (c.totp, "totp"),
            (c.sms, "sms"),
            (c.backup, "backup"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, k)| k)
        .collect()
    }

    pub fn start(&mut self, c: Challenge) -> bool {
        *self = State { challenge: c, ..State::default() };
        let first = self.methods().first().copied().unwrap_or("totp");
        self.pick(first)
    }

    pub fn sms_sent(&mut self, r: Result<String, String>) {
        self.sending = false;
        match r {
            Ok(phone) => self.phone = Some(phone),
            Err(e) => self.err = Some(e),
        }
    }

    fn pick(&mut self, kind: &'static str) -> bool {
        self.kind = if kind == "backup" { "backup_warn" } else { kind };
        self.code.clear();
        self.err = None;
        self.warned = Instant::now();
        kind == "sms" && self.phone.is_none() && !self.sending && {
            self.sending = true;
            true
        }
    }
}

fn names(kind: &str) -> &'static str {
    match kind {
        "webauthn" => "Use a passkey or security key",
        "totp" => "Use your authenticator app",
        "sms" => "Use a code sent to your phone",
        _ => "Use a backup code",
    }
}

fn row(ui: &mut Ui, text: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().bg_mod_hover, pal().bg_mod_selected, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().text(rect.left_center() + vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, text, semi(16.0), pal().text_normal);
    ui.painter().text(rect.right_center() - vec2(16.0, 0.0), egui::Align2::RIGHT_CENTER, "›", semi(20.0), pal().text_muted);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let f = &mut app.login;
    let methods = f.mfa.methods();
    let mut send_sms = false;
    let mut confirm = false;
    let mut back = false;
    match f.mfa.kind {
        "select" => {
            super::heading(ui, "Multi-Factor Authentication", "Verify with something else");
            for m in methods.iter().copied() {
                let r = row(ui, names(m));
                f.rects.insert(m, r.rect);
                if r.clicked() {
                    send_sms |= f.mfa.pick(m);
                }
                ui.add_space(8.0);
            }
        }
        "backup_warn" => {
            super::heading(ui, "Multi-Factor Authentication", "Slow down! Are you sure?");
            let left = 10u64.saturating_sub(f.mfa.warned.elapsed().as_secs());
            ui.add(note("Backup codes can only be used once each and are meant for emergency access to your account!"));
            ui.add_space(8.0);
            let more = if left > 0 { format!("Please wait {left} seconds before continuing.") } else { "If you do not have access to any other methods, use the other button below to proceed.".into() };
            ui.add(note(&more));
            ui.add_space(20.0);
            if left > 0 {
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            }
            let r = primary(ui, "I understand", left > 0);
            f.rects.insert("ack", r.rect);
            if r.clicked() && left == 0 {
                f.mfa.kind = "backup";
            }
            if methods.len() > 1 {
                ui.add_space(10.0);
                if secondary(ui, "Verify with something else").clicked() {
                    f.mfa.kind = "select";
                }
            }
        }
        "webauthn" => {
            super::heading(ui, "Multi-Factor Authentication", "");
            if let Some(e) = f.mfa.err.clone() {
                problem(ui, &e);
            }
            let r = primary(ui, "Authenticate with a passkey or security key", f.mfa.busy);
            f.rects.insert("webauthn", r.rect);
            if r.clicked() && !f.mfa.busy {
                f.mfa.err = None;
                if let Some(options) = f.mfa.challenge.webauthn.clone() {
                    f.mfa.busy = true;
                    app.net.send(Cmd::Auth(Auth::Key { ticket: f.mfa.challenge.ticket.clone(), options }));
                }
            }
            switcher(ui, f, methods.len());
        }
        kind => {
            let sub = match (kind, &f.mfa.phone) {
                ("sms", None) => "Sending...".to_string(),
                ("sms", Some(p)) => format!("A text message with a 6-digit verification code was just sent to {p}"),
                _ => String::new(),
            };
            super::heading(ui, "Multi-Factor Authentication", &sub);
            let (text, hint, max) = match kind {
                "backup" => ("Enter Discord Backup Code", "8-digit backup code", 11),
                _ => ("Enter Discord Auth Code", "6-digit authentication code", if kind == "sms" { 10 } else { 6 }),
            };
            label(ui, text, false, None);
            let r = input(ui, &mut f.mfa.code, false, hint);
            f.mfa.code.truncate(max);
            f.rects.insert("code", r.rect);
            if !r.has_focus() && f.mfa.code.is_empty() && !f.mfa.busy {
                r.request_focus();
            }
            ui.add_space(8.0);
            if kind == "sms" && link(ui, "Resend SMS?").clicked() && !f.mfa.sending {
                f.mfa.sending = true;
                send_sms = true;
            }
            if let Some(e) = f.mfa.err.clone() {
                problem(ui, &e);
            }
            ui.add_space(12.0);
            let need = match kind {
                "backup" => 8,
                _ => 6,
            };
            let ready = f.mfa.code.replace('-', "").len() >= need;
            let busy = f.mfa.busy;
            let r = ui.add_enabled_ui(ready && !busy, |ui| primary(ui, "Confirm", busy)).inner;
            f.rects.insert("confirm", r.rect);
            confirm = (r.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))) && ready && !f.mfa.busy;
            switcher(ui, f, methods.len());
        }
    }
    ui.add_space(12.0);
    if link(ui, "Go back").clicked() {
        back = true;
    }
    if back {
        f.go(Page::Login);
    }
    if send_sms {
        app.net.send(Cmd::Auth(Auth::Sms(f.mfa.challenge.ticket.clone())));
    }
    if confirm {
        let kind = match f.mfa.kind {
            "backup" => "backup",
            "sms" => "sms",
            _ => "totp",
        };
        f.mfa.busy = true;
        f.mfa.err = None;
        let code = f.mfa.code.replace('-', "");
        app.net.send(Cmd::Auth(Auth::Mfa { kind, ticket: f.mfa.challenge.ticket.clone(), code }));
    }
}

fn switcher(ui: &mut Ui, f: &mut Form, count: usize) {
    if count < 2 {
        return;
    }
    ui.add_space(10.0);
    let r = secondary(ui, "Verify with something else");
    f.rects.insert("other", r.rect);
    if r.clicked() {
        f.mfa.err = None;
        f.mfa.kind = "select";
    }
}

fn note(s: &str) -> egui::Label {
    egui::Label::new(egui::RichText::new(s).font(reg(14.0)).color(pal().text_normal)).wrap()
}
