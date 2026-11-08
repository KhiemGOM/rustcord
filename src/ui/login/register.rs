use chrono::{Datelike, NaiveDate};
use egui::{RichText, Ui, vec2};
use std::time::{Duration, Instant};

use super::{Page, Form, input, label, link, muted_then_link, primary, problem};
use crate::auth::{Cmd as Auth, Register};
use crate::net::Cmd;
use crate::theme::*;
use crate::ui::App;

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

pub struct State {
    pub email: String,
    pub global_name: String,
    pub username: String,
    pub password: String,
    pub month: u32,
    pub day: u32,
    pub year: i32,
    pub consent: bool,
    pub promo: bool,
    sent: String,
    changed: Instant,
    known: Option<(String, bool)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            email: String::new(),
            global_name: String::new(),
            username: String::new(),
            password: String::new(),
            month: 0,
            day: 0,
            year: 0,
            consent: false,
            promo: false,
            sent: String::new(),
            changed: Instant::now(),
            known: None,
        }
    }
}

impl State {
    pub fn checked(&mut self, name: &str, taken: bool) {
        self.known = Some((name.to_string(), taken));
    }

    pub fn taken(&self) -> Option<bool> {
        self.known.as_ref().filter(|(n, _)| *n == self.username).map(|(_, t)| *t)
    }

    fn birthday(&self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, self.month, self.day)
    }
}

fn name_ok(n: &str) -> bool {
    (2..=32).contains(&n.chars().count()) && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.') && !n.contains("..")
}

fn status(ui: &mut Ui, text: &str, col: egui::Color32) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).font(reg(14.0)).color(col));
}

fn text_field(f: &mut Form, ui: &mut Ui, id: &'static str, text: &str, required: bool, password: bool, hint: &str) -> egui::Response {
    let mut value = std::mem::take(match id {
        "email" => &mut f.reg.email,
        "global_name" => &mut f.reg.global_name,
        "username" => &mut f.reg.username,
        _ => &mut f.reg.password,
    });
    label(ui, text, required, f.error(id));
    let r = input(ui, &mut value, password, hint);
    if f.focus == Some(id) {
        r.request_focus();
        f.focus = None;
    }
    f.rects.insert(id, r.rect);
    *match id {
        "email" => &mut f.reg.email,
        "global_name" => &mut f.reg.global_name,
        "username" => &mut f.reg.username,
        _ => &mut f.reg.password,
    } = value;
    r
}

fn check(ui: &mut Ui, on: &mut bool, text: &str) -> egui::Response {
    ui.horizontal_top(|ui| {
        let (rect, r) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::click());
        let p = ui.painter();
        if *on {
            p.rect_filled(rect, 4.0, pal().blurple);
            let c = rect.center();
            let stroke = egui::Stroke::new(2.0, egui::Color32::WHITE);
            p.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.5)], stroke);
            p.line_segment([c + vec2(-1.5, 3.5), c + vec2(5.0, -3.5)], stroke);
        } else {
            p.rect_stroke(rect, 4.0, egui::Stroke::new(2.0, pal().text_muted), egui::StrokeKind::Inside);
        }
        if r.clicked() {
            *on = !*on;
        }
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().text_muted)).wrap());
        r
    })
    .inner
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let f = &mut app.login;
    super::heading(ui, "Create an account", "");
    let mut go = ui.input(|i| i.key_pressed(egui::Key::Enter));
    text_field(f, ui, "email", "Email", true, false, "");
    ui.add_space(20.0);
    text_field(f, ui, "global_name", "Display Name", false, false, "");
    ui.add_space(20.0);
    let who = text_field(f, ui, "username", "Username", true, false, "");
    if who.changed() {
        f.reg.changed = Instant::now();
    }
    let name = f.reg.username.clone();
    if !name.is_empty() && f.error("username").is_none() {
        if !name_ok(&name) {
            status(ui, "Please only use numbers, letters, underscores _ , or periods.", pal().red);
        } else if let Some((n, taken)) = f.reg.known.as_ref().filter(|(n, _)| *n == name) {
            let _ = n;
            if *taken {
                status(ui, "Username is unavailable. Try adding numbers, letters, underscores _ , or periods.", pal().red);
            } else {
                status(ui, "Username is available. Nice!", pal().positive);
            }
        } else {
            status(ui, "Checking if username is available", pal().text_muted);
            if f.reg.sent != name {
                let wait = Duration::from_millis(400);
                if f.reg.changed.elapsed() >= wait {
                    f.reg.sent = name.clone();
                    app.net.send(Cmd::Auth(Auth::Name(name)));
                } else {
                    ui.ctx().request_repaint_after(wait - f.reg.changed.elapsed());
                }
            }
        }
    }
    ui.add_space(20.0);
    text_field(f, ui, "password", "Password", true, true, "");
    ui.add_space(20.0);
    label(ui, "Date of Birth", true, f.error("date_of_birth"));
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.spacing_mut().button_padding = vec2(10.0, 10.0);
        let w = &mut ui.visuals_mut().widgets;
        for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            v.weak_bg_fill = pal().input_bg;
            v.bg_fill = pal().input_bg;
            v.corner_radius = 3.into();
            v.bg_stroke = egui::Stroke::NONE;
            v.fg_stroke.color = pal().text_normal;
        }
        let s = &mut f.reg;
        let month = if s.month == 0 { "Month" } else { MONTHS[s.month as usize - 1] };
        egui::ComboBox::from_id_salt("dob_month").width(150.0).selected_text(month).show_ui(ui, |ui| {
            for (i, m) in MONTHS.iter().enumerate() {
                ui.selectable_value(&mut s.month, i as u32 + 1, *m);
            }
        });
        let day = if s.day == 0 { "Day".into() } else { s.day.to_string() };
        egui::ComboBox::from_id_salt("dob_day").width(90.0).selected_text(day).show_ui(ui, |ui| {
            for d in 1..=31 {
                ui.selectable_value(&mut s.day, d, d.to_string());
            }
        });
        let year = if s.year == 0 { "Year".into() } else { s.year.to_string() };
        egui::ComboBox::from_id_salt("dob_year").width(110.0).selected_text(year).show_ui(ui, |ui| {
            for y in (1900..=chrono::Local::now().year()).rev() {
                ui.selectable_value(&mut s.year, y, y.to_string());
            }
        });
    });
    ui.add_space(20.0);
    let c = check(ui, &mut f.reg.consent, "I have read and agree to Discord's Terms of Service and Privacy Policy.");
    f.rects.insert("consent", c.rect);
    if let Some(e) = f.error("consent") {
        status(ui, e, pal().red);
    }
    ui.add_space(8.0);
    check(ui, &mut f.reg.promo, "(Optional) It's okay to send me emails with Discord updates, tips, and special offers. You can opt out at any time.");
    ui.add_space(20.0);
    if let Some(e) = f.err.clone() {
        problem(ui, &e);
    }
    if f.captcha {
        problem(ui, "Finish the captcha in the window that opened.");
    }
    let r = primary(ui, "Continue", f.busy);
    f.rects.insert("submit", r.rect);
    ui.add_space(8.0);
    if muted_then_link(ui, "Already have an account?", "Log In").clicked() {
        f.go(Page::Login);
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("By registering, you agree to Discord's").font(reg(12.0)).color(pal().text_muted));
        if link(ui, "Terms of Service").clicked() {
            let _ = open::that_detached("https://discord.com/terms");
        }
        ui.label(RichText::new("and").font(reg(12.0)).color(pal().text_muted));
        if link(ui, "Privacy Policy").clicked() {
            let _ = open::that_detached("https://discord.com/privacy");
        }
    });
    go &= !f.busy;
    if (r.clicked() && !f.busy) || go {
        submit(app);
    }
}

fn submit(app: &mut App) {
    let f = &mut app.login;
    f.errs.clear();
    f.err = None;
    let required = "This field is required";
    let s = &f.reg;
    let mut bad: Vec<(String, String)> = Vec::new();
    if s.email.trim().is_empty() {
        bad.push(("email".into(), required.into()));
    }
    if s.username.is_empty() {
        bad.push(("username".into(), required.into()));
    } else if !name_ok(&s.username) {
        bad.push(("username".into(), "Please only use numbers, letters, underscores _ , or periods.".into()));
    }
    if s.password.is_empty() {
        bad.push(("password".into(), required.into()));
    }
    match s.birthday() {
        None => bad.push(("date_of_birth".into(), required.into())),
        Some(b) if chrono::Local::now().date_naive().years_since(b).unwrap_or(0) < 13 => {
            bad.push(("date_of_birth".into(), "You must be at least 13 years old to use Discord.".into()))
        }
        _ => {}
    }
    if !s.consent {
        bad.push(("consent".into(), "You need to agree to our terms of service to continue".into()));
    }
    if !bad.is_empty() {
        f.errs = bad;
        return;
    }
    let birthday = s.birthday().map(|b| b.format("%Y-%m-%d").to_string()).unwrap_or_default();
    let form = Register {
        email: s.email.trim().to_string(),
        global_name: s.global_name.trim().to_string(),
        username: s.username.clone(),
        password: s.password.clone(),
        birthday,
        consent: s.consent,
        promo: s.promo,
    };
    f.busy = true;
    app.net.send(Cmd::Auth(Auth::Register(form)));
}
