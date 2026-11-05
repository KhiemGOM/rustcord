use egui::{Align, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Ui, vec2};
use serde_json::{Value, json};

use super::{input, link, primary, refetch, secondary};
use crate::net::{Cmd, Tag};
use crate::theme::*;
use crate::ui::App;
use super::account::standing;

#[derive(Clone, Copy, PartialEq)]
pub enum Edit {
    Username,
    Email,
    Phone,
    Password,
    Disable,
    Delete,
    Owner,
    Standing,
    Totp,
    TotpOff,
    Codes,
    Key,
    AddKey,
    SmsOff,
}

pub struct Modal {
    pub kind: Edit,
    pub step: u8,
    pub f: [String; 3],
    pub err: Option<String>,
    pub busy: bool,
    pub token: String,
    pub nonce: [String; 2],
    pub codes: Vec<(String, bool)>,
    pub regenerate: bool,
    pub secret: String,
    pub key: Option<(String, String)>,
}

pub struct Web {
    rx: std::sync::mpsc::Receiver<Result<String, String>>,
    create: bool,
}

pub struct Req {
    method: &'static str,
    path: String,
    body: Option<Value>,
    done: Done,
}

pub(super) enum Done {
    Modal,
    Refresh(&'static str),
    Sms,
}

pub struct Verify {
    ticket: String,
    methods: Vec<&'static str>,
    at: usize,
    challenge: String,
    sent: bool,
    code: String,
    err: Option<String>,
    busy: bool,
}

pub(super) const SMS_FLAG: u64 = 1 << 4;
const REASON: &str = "user_settings_update";

pub fn new_modal(kind: Edit) -> Modal {
    Modal {
        kind,
        step: 0,
        f: Default::default(),
        err: None,
        busy: false,
        token: String::new(),
        nonce: Default::default(),
        codes: Vec::new(),
        regenerate: false,
        secret: String::new(),
        key: None,
    }
}

pub fn open(app: &mut App, kind: Edit) {
    let mut m = new_modal(kind);
    match kind {
        Edit::Username => m.f[0] = app.store.user(app.store.me).map(|u| u.username.clone()).unwrap_or_default(),
        Edit::Phone => m.step = if app.store.me_raw["phone"].is_string() { 0 } else { 1 },
        Edit::Totp => m.secret = new_secret(),
        Edit::Disable | Edit::Delete if owns_servers(app) => m.kind = Edit::Owner,
        _ => {}
    }
    app.sform.modal = Some(m);
}

fn owns_servers(app: &App) -> bool {
    app.store.guilds.values().any(|g| g.owner == app.store.me)
}

fn new_secret() -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| A[(*x & 31) as usize] as char).collect()
}

// the second factor ---------------------------------------------------------------------

pub(super) fn send(app: &mut App, done: Done, method: &'static str, path: &str, body: Option<Value>) {
    app.net.send(Cmd::Sudo { method, path: path.to_string(), body: body.clone(), mfa: None, tag: Tag::Remote("guard") });
    app.sform.sudo = Some(Req { method, path: path.to_string(), body, done });
}

pub fn guard_done(app: &mut App, res: Result<Value, String>) {
    if let Err(e) = &res
        && let Some(raw) = e.strip_prefix("mfa:")
    {
        let mfa: Value = serde_json::from_str(raw).unwrap_or_default();
        let methods: Vec<&'static str> = ["totp", "sms", "backup", "password", "webauthn"]
            .into_iter()
            .filter(|m| mfa["methods"].as_array().is_some_and(|l| l.iter().any(|x| x["type"] == *m)))
            .collect();
        if let Some(m) = &mut app.sform.modal {
            m.busy = false;
        }
        let challenge = mfa["methods"].as_array().into_iter().flatten().find(|x| x["type"] == "webauthn").and_then(|x| x["challenge"].as_str()).unwrap_or("").to_string();
        if methods.is_empty() {
            app.sform.sudo = None;
            app.toast("Discord asked for a second factor Rustcord doesn't know.");
        } else {
            app.sform.verify = Some(Verify { ticket: mfa["ticket"].as_str().unwrap_or("").to_string(), methods, at: 0, challenge, sent: false, code: String::new(), err: None, busy: false });
        }
        return;
    }
    let Some(req) = app.sform.sudo.take() else { return };
    match req.done {
        Done::Modal => modal_done(app, res),
        Done::Refresh(k) => match res {
            Ok(_) => refetch(app, k),
            Err(e) => app.toast(e),
        },
        Done::Sms => match res {
            Ok(_) => set_sms(app, true),
            Err(e) => app.toast(e),
        },
    }
}

pub fn finish_done(app: &mut App, res: Result<Value, String>) {
    let Some(v) = &mut app.sform.verify else { return };
    v.busy = false;
    match res {
        Err(e) => v.err = Some(e),
        Ok(r) => {
            app.sform.verify = None;
            if let Some(req) = &app.sform.sudo {
                app.net.send(Cmd::Sudo {
                    method: req.method,
                    path: req.path.clone(),
                    body: req.body.clone(),
                    mfa: r["token"].as_str().map(str::to_string),
                    tag: Tag::Remote("guard"),
                });
            }
        }
    }
}

fn start_web(ctx: &egui::Context, create: bool, options: String) -> Web {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let res = if create { crate::webauthn::make(&options) } else { crate::webauthn::get(&options) };
        let _ = tx.send(res);
        ctx.request_repaint();
    });
    Web { rx, create }
}

fn poll_web(app: &mut App) {
    let Some(w) = &app.sform.web else { return };
    let Ok(res) = w.rx.try_recv() else { return };
    let create = w.create;
    app.sform.web = None;
    if create {
        let Some(m) = &mut app.sform.modal else { return };
        match res {
            Err(e) => {
                m.err = Some(e);
                m.step = 0;
            }
            Ok(cred) => {
                let ticket = m.key.as_ref().map(|k| k.0.clone()).unwrap_or_default();
                let body = json!({ "name": m.f[0].trim(), "ticket": ticket, "credential": cred });
                m.step = 2;
                send(app, Done::Modal, "POST", "/users/@me/mfa/webauthn/credentials", Some(body));
            }
        }
    } else if let Some(v) = &mut app.sform.verify {
        match res {
            Err(e) => v.err = Some(e),
            Ok(cred) => {
                v.busy = true;
                let body = json!({ "ticket": v.ticket, "mfa_type": "webauthn", "data": cred });
                app.net.send(Cmd::Rest { method: "POST", path: "/mfa/finish".into(), body: Some(body), tag: Tag::Remote("mfa_finish") });
            }
        }
    }
}

fn verify_modal(app: &mut App, ctx: &egui::Context) {
    let waiting = app.sform.web.is_some();
    let Some(v) = &mut app.sform.verify else { return };
    let method = v.methods[v.at];
    let (head, sub, hint) = match method {
        "totp" => ("Verify with your authenticator app", "Enter the 6-digit code from your authenticator app.", "6-digit authentication code"),
        "sms" => ("Enter Discord SMS Code", "An SMS was sent to the phone number connected to this account.", "6-digit authentication code"),
        "backup" => ("Verify with a backup code", "If you do not have access to any other methods, enter your backup code and use the button below to proceed.", "8-character backup code"),
        "webauthn" => ("Use a passkey or security key", "Authenticate with a passkey or security key.", ""),
        _ => ("Verify your password", "Enter your password to continue.", "Password"),
    };
    if !v.sent && !v.busy {
        match method {
            "sms" => {
                v.sent = true;
                app.net.send(Cmd::Rest { method: "POST", path: "/auth/mfa/sms/send".into(), body: Some(json!({ "ticket": v.ticket })), tag: Tag::Remote("action") });
            }
            "webauthn" if !waiting => {
                v.sent = true;
                app.sform.web = Some(start_web(ctx, false, v.challenge.clone()));
            }
            _ => {}
        }
    }
    let alt: Vec<&'static str> = v.methods.iter().copied().filter(|m| *m != method).collect();
    let mut swap = None;
    let go = if method == "webauthn" { "Try Again" } else { "Verify" };
    let act = dialog(ctx, "settings_verify", head, sub, go, BLURPLE, None, |ui| {
        if method == "webauthn" {
            if waiting {
                para(ui, "Follow the prompts from Windows to use your passkey or security key.");
            }
        } else {
            let r = input(ui, &mut v.code, hint, method == "password");
            super::mark(ui, "verify_input", r.rect);
        }
        if let Some(e) = &v.err {
            ui.add_space(6.0);
            ui.label(RichText::new(e).font(reg(14.0)).color(RED));
        }
        ui.add_space(8.0);
        for m in &alt {
            let l = match *m {
                "totp" => "Use your authenticator app",
                "sms" => "Receive auth code from SMS",
                "backup" => "Use a backup code",
                "webauthn" => "Use a passkey or security key",
                _ => "Use your password",
            };
            if link(ui, l).clicked() {
                swap = Some(*m);
            }
        }
    });
    if let Some(m) = swap {
        v.at = v.methods.iter().position(|x| *x == m).unwrap_or(0);
        v.code.clear();
        v.err = None;
        v.sent = false;
    }
    if act == Act::Close {
        app.sform.verify = None;
        app.sform.sudo = None;
        app.sform.web = None;
        if let Some(m) = &mut app.sform.modal {
            m.busy = false;
        }
        return;
    }
    if act == Act::Go && !v.busy && method == "webauthn" {
        if !waiting {
            v.err = None;
            v.sent = false;
        }
    } else if act == Act::Go && !v.busy {
        v.busy = true;
        v.err = None;
        let body = json!({ "ticket": v.ticket, "mfa_type": method, "data": v.code.trim() });
        app.rest("POST", "/mfa/finish".into(), Some(body), Tag::Remote("mfa_finish"));
    }
}

// dialogs -------------------------------------------------------------------------------

#[derive(PartialEq)]
enum Act {
    None,
    Go,
    Alt,
    Close,
}

#[allow(clippy::too_many_arguments)]
fn dialog(ctx: &egui::Context, id: &'static str, head: &str, sub: &str, go: &str, fill: Color32, alt: Option<(&str, Color32)>, body: impl FnOnce(&mut Ui)) -> Act {
    let mut act = Act::None;
    let at = if id == "settings_modal" { "modal" } else { "verify" };
    let r = egui::Modal::new(id.into()).frame(Frame::new().fill(pal().bg_secondary).corner_radius(8)).show(ctx, |ui| {
        ui.set_width(440.0);
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(head).font(semi(24.0)).color(pal().header_primary));
                if !sub.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
                }
            });
            ui.add_space(12.0);
            body(ui);
        });
        Frame::new().fill(pal().bg_secondary).corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }).inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !go.is_empty() {
                    let w = ui.painter().layout_no_wrap(go.into(), semi(14.0), Color32::WHITE).size().x + 32.0;
                    let b = crate::ui::button(ui, go, fill, w);
                    super::mark(ui, &format!("{at}_done"), b.rect);
                    if b.clicked() {
                        act = Act::Go;
                    }
                    ui.add_space(8.0);
                }
                if let Some((l, c)) = alt {
                    let w = ui.painter().layout_no_wrap(l.into(), semi(14.0), Color32::WHITE).size().x + 32.0;
                    let b = crate::ui::button(ui, l, c, w);
                    super::mark(ui, &format!("{at}_alt"), b.rect);
                    if b.clicked() {
                        act = Act::Alt;
                    }
                    ui.add_space(8.0);
                }
                let c = ui.add(egui::Label::new(RichText::new(if go.is_empty() { "Done" } else { "Cancel" }).color(pal().header_primary)).sense(Sense::click()));
                super::mark(ui, &format!("{at}_cancel"), c.rect);
                if c.clicked() {
                    act = Act::Close;
                }
            });
        });
    });
    if r.should_close() {
        act = Act::Close;
    }
    act
}

fn label(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s.to_uppercase()).font(semi(12.0)).color(pal().header_secondary));
    ui.add_space(8.0);
}

fn para(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
}

fn qr(ui: &mut Ui, data: &str) {
    let Ok(code) = qrcode::QrCode::new(data.as_bytes()) else { return };
    let n = code.width();
    let cell = 4.0;
    let side = (n as f32 + 4.0) * cell;
    let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(4), Color32::WHITE);
    for (i, c) in code.to_colors().into_iter().enumerate() {
        if c == qrcode::Color::Dark {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            ui.painter().rect_filled(Rect::from_min_size(r.min + vec2((x + 2.0) * cell, (y + 2.0) * cell), vec2(cell, cell)), 0, Color32::BLACK);
        }
    }
}

pub fn modals(app: &mut App, ctx: &egui::Context) {
    poll_web(app);
    if let Some(m) = &mut app.sform.modal
        && m.kind == Edit::AddKey
        && m.step == 1
        && !m.secret.is_empty()
    {
        app.sform.web = Some(start_web(ctx, true, std::mem::take(&mut m.secret)));
    }
    modal(app, ctx);
    verify_modal(app, ctx);
}

fn codes_text(email: &str, codes: &[(String, bool)]) -> String {
    let mut s = format!("These are your Discord backup codes for account {email}.  Keep them safe!\n\n");
    for (c, used) in codes {
        s += &format!("* {c}{}\n", if *used { " (used)" } else { "" });
    }
    s
}

fn parse_codes(v: &Value) -> Vec<(String, bool)> {
    v["backup_codes"].as_array().into_iter().flatten().map(|c| (c["code"].as_str().unwrap_or("").to_string(), c["consumed"].as_bool().unwrap_or(false))).collect()
}

fn modal(app: &mut App, ctx: &egui::Context) {
    let email = app.store.email.clone().unwrap_or_default();
    let old_phone = app.store.me_raw["phone"].as_str().unwrap_or("").to_string();
    let standing_state = match app.remote.get("safety") {
        Some(Some(Ok(v))) => v.clone(),
        _ => Value::Null,
    };
    let Some(m) = &mut app.sform.modal else { return };
    let step = m.step;
    let (head, sub) = match (m.kind, step) {
        (Edit::Username, _) => ("Change your username", "Enter a new username and your existing password."),
        (Edit::Email, 0) => ("Verify email address", ""),
        (Edit::Email, 1) => ("Enter verification code", "Enter the 6-digit code we sent to your old email address."),
        (Edit::Email, _) => ("Enter an email address", "Enter a new email address and your existing password."),
        (Edit::Phone, 0) => ("Edit your phone number", ""),
        (Edit::Phone, 1) => ("Enter your phone number", "Discord will text you a code to verify it."),
        (Edit::Phone, 2) => ("Enter verification code", "Enter the code we texted to your phone."),
        (Edit::Phone, 3) => ("Enter your password", "Confirm it's you to save your phone number."),
        (Edit::Phone, _) => ("Remove phone number", "Enter your password to remove your phone number."),
        (Edit::Password, _) => ("Update your password", "Enter your current password and a new password."),
        (Edit::Disable, _) => ("Disable Account", "Are you sure that you want to disable your account? This will immediately log you out and make your account inaccessible to anyone."),
        (Edit::Delete, _) => ("Delete Account", "Are you sure that you want to delete your account? This will immediately log you out of your account and you will not be able to log in again."),
        (Edit::Owner, _) => ("You own servers!", "In order to delete or disable your account you must first transfer ownership of all servers that you own."),
        (Edit::Standing, _) => ("Account Standing", ""),
        (Edit::Totp, 0) => ("Enable Authenticator App", "Protect your account with a second step when you log in."),
        (Edit::Totp, _) => ("Backup codes", "Keep these somewhere safe. Each one works once."),
        (Edit::TotpOff, _) => ("Remove Authenticator App", "Are you sure you want to remove your Authenticator App?"),
        (Edit::Codes, 0) => ("View backup codes", "Enter your password and we'll email you a key."),
        (Edit::Codes, 1) => ("Enter your key", "Enter the verification key we emailed you."),
        (Edit::Codes, _) => ("Backup codes", "Keep these somewhere safe. Each one works once."),
        (Edit::Key, _) => ("Rename security key", ""),
        (Edit::AddKey, 0) => ("Name Security Key", "Give your passkey or security key a name you will recognise."),
        (Edit::AddKey, 3) => ("Backup codes", "Keep these somewhere safe. Each one works once."),
        (Edit::AddKey, _) => ("Register Security Key", "Follow the prompts from Windows to add your passkey or security key."),
        (Edit::SmsOff, _) => ("Remove SMS Authentication", "This removes the ability to log in with an authentication code through SMS. Are you sure you want to remove this?"),
    };
    let (go, fill, alt) = match (m.kind, step) {
        (Edit::Username, _) | (Edit::Password, _) => ("Done", BLURPLE, None),
        (Edit::Totp, 0) => ("Activate", BLURPLE, None),
        (Edit::Email, 0) => ("Send Verification Code", BLURPLE, None),
        (Edit::Email, 1) | (Edit::Phone, 2) | (Edit::Codes, 1) => ("Verify", BLURPLE, None),
        (Edit::Email, _) | (Edit::Phone, 3) => ("Done", BLURPLE, None),
        (Edit::Phone, 0) => ("Edit", BLURPLE, Some(("Remove", RED))),
        (Edit::Phone, 1) => ("Send", BLURPLE, None),
        (Edit::Phone, _) | (Edit::Codes, 0) | (Edit::Key, _) => ("Done", if m.kind == Edit::Phone { RED } else { BLURPLE }, None),
        (Edit::Disable, _) => ("Disable Account", RED, None),
        (Edit::Delete, _) => ("Delete Account", RED, None),
        (Edit::Owner, _) => ("Okay", BLURPLE, None),
        (Edit::Standing, _) => ("", BLURPLE, None),
        (Edit::TotpOff, _) => ("Remove", RED, None),
        (Edit::Totp, _) | (Edit::Codes, _) | (Edit::AddKey, 1..) => ("", BLURPLE, None),
        (Edit::AddKey, _) => ("Continue", BLURPLE, None),
        (Edit::SmsOff, _) => ("Remove", RED, None),
    };
    let act = dialog(ctx, "settings_modal", head, sub, if m.busy && !go.is_empty() { "…" } else { go }, fill, alt, |ui| {
        match (m.kind, step) {
            (Edit::Username, _) => {
                label(ui, "Username");
                let r = input(ui, &mut m.f[0], "", false);
                super::mark(ui, "modal_input", r.rect);
                para(ui, "Please only use numbers, letters, underscores _ , or periods.");
                ui.add_space(8.0);
                label(ui, "Current Password");
                let r = input(ui, &mut m.f[1], "", true);
                super::mark(ui, "modal_password", r.rect);
            }
            (Edit::Email, 0) => {
                para(ui, &format!("We’ll need to verify your old email address, {email}, in order to change it."));
                ui.add_space(8.0);
                para(ui, "Lost access to your email? Please contact your email provider to regain access.");
            }
            (Edit::Email, 1) | (Edit::Phone, 2) => {
                label(ui, "Verification code");
                let r = input(ui, &mut m.f[0], "", false);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Email, _) => {
                label(ui, "Email");
                let r = input(ui, &mut m.f[0], "", false);
                super::mark(ui, "modal_input", r.rect);
                ui.add_space(8.0);
                label(ui, "Current Password");
                input(ui, &mut m.f[1], "", true);
            }
            (Edit::Phone, 0) => {
                para(ui, "Your current phone number is");
                ui.label(RichText::new(&old_phone).font(medium(16.0)).color(pal().header_primary));
            }
            (Edit::Phone, 1) => {
                label(ui, "Phone number");
                let r = input(ui, &mut m.f[1], "+1 555 555 0100", false);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Phone, _) => {
                label(ui, "Password");
                let r = input(ui, &mut m.f[2], "", true);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Codes, 0) | (Edit::Disable, _) | (Edit::Delete, _) => {
                label(ui, "Password");
                let r = input(ui, &mut m.f[0], "", true);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Password, _) => {
                label(ui, "Current Password");
                let r = input(ui, &mut m.f[0], "", true);
                super::mark(ui, "modal_input", r.rect);
                ui.add_space(8.0);
                label(ui, "New Password");
                input(ui, &mut m.f[1], "", true);
                ui.add_space(8.0);
                label(ui, "Confirm New Password");
                input(ui, &mut m.f[2], "", true);
            }
            (Edit::Owner | Edit::TotpOff, _) => {}
            (Edit::AddKey, 0) => {
                label(ui, "Name");
                let r = input(ui, &mut m.f[0], "", false);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::AddKey, 1 | 2) => {
                ui.add(egui::Spinner::new().color(pal().text_muted));
            }
            (Edit::SmsOff, _) => {
                label(ui, "Password");
                let r = input(ui, &mut m.f[0], "", true);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Standing, _) => {
                let (name, _, _) = standing(standing_state["account_standing"]["state"].as_u64().unwrap_or(100));
                ui.label(RichText::new(name).font(semi(16.0)).color(pal().header_primary));
                let list = standing_state["classifications"].as_array().cloned().unwrap_or_default();
                if list.is_empty() {
                    para(ui, "You have no violations on your account.");
                }
                for c in &list {
                    para(ui, c["description"].as_str().or(c["id"].as_str()).unwrap_or("Violation"));
                }
            }
            (Edit::Totp, 0) => {
                para(ui, "1. Download an authenticator app such as Google Authenticator or Authy.");
                ui.add_space(8.0);
                para(ui, "2. Scan this QR code with it, or type the 2FA Key (Manual entry).");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    qr(ui, &format!("otpauth://totp/Discord:{email}?secret={}&issuer=Discord", m.secret));
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        label(ui, "2FA Key (Manual entry)");
                        ui.label(RichText::new(&m.secret).font(mono(14.0)).color(pal().header_primary));
                        if link(ui, "Copy key").clicked() {
                            ui.ctx().copy_text(m.secret.clone());
                        }
                    });
                });
                ui.add_space(8.0);
                para(ui, "3. Enter the 6-digit code it shows.");
                ui.add_space(8.0);
                let r = input(ui, &mut m.f[0], "6-digit authentication code", false);
                super::mark(ui, "modal_input", r.rect);
            }
            (Edit::Totp, _) | (Edit::Codes, _) | (Edit::AddKey, _) => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 6.0);
                    for (c, used) in &m.codes {
                        let t = RichText::new(c).font(mono(16.0)).color(if *used { pal().text_muted } else { pal().header_primary });
                        ui.label(if *used { t.strikethrough() } else { t });
                    }
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if primary(ui, "Download Backup Codes").clicked()
                        && let Some(p) = rfd::FileDialog::new().set_file_name("discord_backup_codes.txt").save_file()
                    {
                        let _ = std::fs::write(p, codes_text(&email, &m.codes));
                    }
                    ui.add_space(8.0);
                    if m.kind == Edit::Codes && secondary(ui, "Generate New Backup Codes").clicked() {
                        m.regenerate = true;
                        m.step = 0;
                        m.f = Default::default();
                    }
                });
                if m.kind == Edit::Codes {
                    ui.add_space(8.0);
                    para(ui, "Generating new backup codes will invalidate any old backup codes you have saved.");
                }
            }
            (Edit::Key, _) => {
                label(ui, "Name");
                input(ui, &mut m.f[0], "", false);
            }
        }
        if let Some(e) = &m.err {
            ui.add_space(6.0);
            ui.label(RichText::new(e).font(reg(14.0)).color(RED));
        }
    });
    match act {
        Act::Close => app.sform.modal = None,
        Act::Go if !m.busy => submit(app, false),
        Act::Alt if !m.busy => submit(app, true),
        _ => {}
    }
}

fn submit(app: &mut App, alt: bool) {
    let Some(m) = &mut app.sform.modal else { return };
    m.err = None;
    let (method, path, body): (&'static str, String, Option<Value>) = match (m.kind, m.step) {
        (Edit::Username, _) => ("PATCH", "/users/@me".into(), Some(json!({ "username": m.f[0].trim(), "password": m.f[1] }))),
        (Edit::Email, 0) => ("PUT", "/users/@me/email".into(), None),
        (Edit::Email, 1) => ("POST", "/users/@me/email/verify-code".into(), Some(json!({ "code": m.f[0].trim() }))),
        (Edit::Email, _) => ("PATCH", "/users/@me".into(), Some(json!({ "email": m.f[0].trim(), "password": m.f[1], "email_token": m.token }))),
        (Edit::Phone, 0) if alt => {
            m.step = 4;
            return;
        }
        (Edit::Phone, 0) => {
            m.step = 1;
            return;
        }
        (Edit::Phone, 1) => ("POST", "/users/@me/phone".into(), Some(json!({ "phone": m.f[1].trim(), "change_phone_reason": REASON }))),
        (Edit::Phone, 2) => ("POST", "/phone-verifications/verify".into(), Some(json!({ "phone": m.f[1].trim(), "code": m.f[0].trim() }))),
        (Edit::Phone, 3) => ("POST", "/users/@me/phone".into(), Some(json!({ "phone_token": m.token, "password": m.f[2], "change_phone_reason": REASON }))),
        (Edit::Phone, _) => ("DELETE", "/users/@me/phone".into(), Some(json!({ "password": m.f[2], "change_phone_reason": REASON }))),
        (Edit::Password, _) => {
            if m.f[1] != m.f[2] {
                m.err = Some("Passwords do not match!".into());
                return;
            }
            ("PATCH", "/users/@me".into(), Some(json!({ "password": m.f[0], "new_password": m.f[1] })))
        }
        (Edit::Disable, _) => ("POST", "/users/@me/disable".into(), Some(json!({ "password": m.f[0] }))),
        (Edit::Delete, _) => ("POST", "/users/@me/delete".into(), Some(json!({ "password": m.f[0] }))),
        (Edit::Owner | Edit::Standing, _) => {
            app.sform.modal = None;
            return;
        }
        (Edit::Totp, 0) => ("POST", "/users/@me/mfa/totp/enable".into(), Some(json!({ "code": m.f[0].trim(), "secret": m.secret }))),
        (Edit::TotpOff, _) => ("POST", "/users/@me/mfa/totp/disable".into(), None),
        (Edit::SmsOff, _) => ("POST", "/users/@me/mfa/sms/disable".into(), Some(json!({ "password": m.f[0] }))),
        (Edit::AddKey, 0) => {
            if m.f[0].trim().is_empty() {
                m.err = Some("Give it a name first.".into());
                return;
            }
            ("POST", "/users/@me/mfa/webauthn/credentials".into(), Some(json!({})))
        }
        (Edit::AddKey, _) => return,
        (Edit::Codes, 0) => ("POST", "/auth/verify/view-backup-codes-challenge".into(), Some(json!({ "password": m.f[0] }))),
        (Edit::Codes, _) => {
            let nonce = if m.regenerate { &m.nonce[1] } else { &m.nonce[0] };
            ("POST", "/users/@me/mfa/codes-verification".into(), Some(json!({ "key": m.f[1].trim(), "nonce": nonce, "regenerate": m.regenerate })))
        }
        (Edit::Totp, _) => {
            app.sform.modal = None;
            return;
        }
        (Edit::Key, _) => {
            let id = m.key.as_ref().map(|k| k.0.clone()).unwrap_or_default();
            let name = m.f[0].trim().to_string();
            app.sform.modal = None;
            send(app, Done::Refresh("keys"), "PATCH", &format!("/users/@me/mfa/webauthn/credentials/{id}"), Some(json!({ "name": name })));
            return;
        }
    };
    m.busy = true;
    send(app, Done::Modal, method, &path, body);
}

pub fn modal_done(app: &mut App, res: Result<Value, String>) {
    let Some(m) = &mut app.sform.modal else { return };
    m.busy = false;
    let v = match res {
        Err(e) => {
            m.err = Some(e);
            if m.kind == Edit::AddKey {
                m.step = 0;
            }
            return;
        }
        Ok(v) => v,
    };
    let close = |app: &mut App, msg: &str| {
        app.sform.modal = None;
        app.toast(msg.to_string());
    };
    match (m.kind, m.step) {
        (Edit::Disable | Edit::Delete, _) => {
            app.sform.modal = None;
            app.settings_page = None;
            app.logout();
        }
        (Edit::Email, 0) => m.step = 1,
        (Edit::Email, 1) => {
            m.token = v["token"].as_str().unwrap_or("").to_string();
            m.f = Default::default();
            m.step = 2;
        }
        (Edit::Phone, 1) => {
            m.f[0].clear();
            m.step = 2;
        }
        (Edit::Phone, 2) => {
            m.token = v["token"].as_str().unwrap_or("").to_string();
            m.step = 3;
        }
        (Edit::Phone, 3) => {
            let p = m.f[1].trim().to_string();
            app.store.me_raw["phone"] = json!(p);
            close(app, "Saved.");
        }
        (Edit::Phone, _) => {
            app.store.me_raw["phone"] = Value::Null;
            close(app, "Phone number removed.");
        }
        (Edit::Totp, 0) => {
            adopt_token(app, &v);
            app.store.me_raw["mfa_enabled"] = json!(true);
            if let Some(m) = &mut app.sform.modal {
                m.codes = parse_codes(&v);
                m.step = 1;
            }
        }
        (Edit::AddKey, 0) => {
            m.key = Some((v["ticket"].as_str().unwrap_or("").to_string(), String::new()));
            m.secret = v["challenge"].as_str().unwrap_or("").to_string();
            m.step = 1;
        }
        (Edit::AddKey, _) => {
            app.store.me_raw["mfa_enabled"] = json!(true);
            refetch(app, "keys");
            let codes = parse_codes(&v);
            if codes.is_empty() {
                close(app, "Security key added.");
            } else if let Some(m) = &mut app.sform.modal {
                m.codes = codes;
                m.step = 3;
            }
        }
        (Edit::SmsOff, _) => {
            set_sms(app, false);
            close(app, "SMS authentication removed.");
        }
        (Edit::TotpOff, _) => {
            adopt_token(app, &v);
            app.store.me_raw["mfa_enabled"] = json!(false);
            close(app, "Authenticator app removed.");
        }
        (Edit::Codes, 0) => {
            m.nonce = [v["nonce"].as_str().unwrap_or("").to_string(), v["regenerate_nonce"].as_str().unwrap_or("").to_string()];
            m.step = 1;
            m.f = Default::default();
        }
        (Edit::Codes, _) => {
            m.codes = parse_codes(&v);
            m.step = 2;
        }
        _ => {
            if v["id"].is_string() {
                app.store.apply("USER_UPDATE", &v);
                if let Some(e) = v["email"].as_str() {
                    app.store.email = Some(e.to_string());
                }
            }
            adopt_token(app, &v);
            close(app, "Saved.");
        }
    }
}

pub(super) fn set_sms(app: &mut App, on: bool) {
    let f = app.store.me_raw["flags"].as_u64().unwrap_or(0);
    app.store.me_raw["flags"] = json!(if on { f | SMS_FLAG } else { f & !SMS_FLAG });
}

/// A password or second-factor change hands back a fresh token; the old one stops working.
fn adopt_token(app: &mut App, v: &Value) {
    if let Some(t) = v["token"].as_str().filter(|t| app.token.as_deref() != Some(*t)) {
        crate::cache::set_token(Some(t));
        app.net.send(Cmd::Start(t.to_string()));
        app.token = Some(t.to_string());
    }
}

