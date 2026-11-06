mod mfa;
mod qr;
mod register;

use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Ui, text::LayoutJob, vec2};
use std::collections::HashMap;

use super::App;
use crate::auth::{self, Cmd as Auth};
use crate::net::Cmd;
use crate::theme::*;

#[derive(Default, PartialEq)]
pub enum Page {
    #[default]
    Login,
    Register,
    Forgot,
    Sent,
    Mfa,
    Disabled(String),
    Deletion,
}

#[derive(Default)]
pub struct Form {
    pub page: Page,
    pub login: String,
    pub password: String,
    pub token: String,
    pub use_token: bool,
    pub errs: Vec<(String, String)>,
    pub err: Option<String>,
    pub busy: bool,
    pub captcha: bool,
    pub rects: HashMap<&'static str, Rect>,
    pub mfa: mfa::State,
    pub reg: register::State,
    pub qr: qr::State,
    focus: Option<&'static str>,
    card_h: f32,
}

impl Form {
    pub fn expired(&mut self) {
        self.err = Some("Your session expired. Log in again.".into());
    }

    pub fn event(&mut self, ev: auth::Ev, net: &crate::net::Net) {
        use auth::{Ev, Fail};
        match ev {
            Ev::Token(_) => {}
            Ev::Mfa(c) => {
                self.busy = false;
                self.captcha = false;
                self.err = None;
                self.errs.clear();
                self.page = Page::Mfa;
                if self.mfa.start(c) {
                    net.send(Cmd::Auth(Auth::Sms(self.mfa.challenge.ticket.clone())));
                }
            }
            Ev::Fail(f) => {
                self.busy = false;
                self.captcha = false;
                self.mfa.busy = false;
                match f {
                    Fail::Fields(list) if self.page == Page::Mfa => self.mfa.err = list.into_iter().next().map(|(_, m)| m),
                    Fail::Msg(m) if self.page == Page::Mfa => self.mfa.err = Some(m),
                    Fail::Fields(list) => self.errs = list,
                    Fail::Msg(m) => self.err = Some(m),
                    Fail::Disabled(m) => self.page = Page::Disabled(m),
                    Fail::Deletion => self.page = Page::Deletion,
                }
            }
            Ev::Captcha => self.captcha = true,
            Ev::Qr(q) => self.qr.state = q,
            Ev::Sms(r) => self.mfa.sms_sent(r),
            Ev::Name { name, taken } => self.reg.checked(&name, taken),
            Ev::Forgot => {
                self.busy = false;
                self.captcha = false;
                self.page = Page::Sent;
            }
        }
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.errs.clear();
        self.err = None;
        self.busy = false;
    }

    fn error(&self, field: &str) -> Option<&str> {
        self.errs.iter().find(|(f, _)| f == field).map(|(_, m)| m.as_str())
    }
}

fn backdrop(ui: &Ui) {
    let r = ui.max_rect();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), Color32::from_rgb(0x40, 0x4e, 0xed));
    mesh.colored_vertex(r.right_top(), Color32::from_rgb(0x5a, 0x3c, 0xc8));
    mesh.colored_vertex(r.right_bottom(), Color32::from_rgb(0x1b, 0x1f, 0x5c));
    mesh.colored_vertex(r.left_bottom(), Color32::from_rgb(0x12, 0x16, 0x3d));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    let logo = egui::Image::new(egui::include_image!("../../../assets/login/wordmark.svg"));
    logo.paint_at(ui, Rect::from_min_size(r.left_top() + vec2(32.0, 32.0), vec2(130.0, 36.0)));
}

pub(super) fn heading(ui: &mut Ui, title: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
        if !sub.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
        }
    });
    ui.add_space(20.0);
}

pub(super) fn label(ui: &mut Ui, text: &str, required: bool, err: Option<&str>) {
    let mut job = LayoutJob::default();
    let col = if err.is_some() { RED } else { pal().header_secondary };
    let fmt = |c: Color32| egui::TextFormat { font_id: medium(14.0), color: c, ..Default::default() };
    job.append(text, 0.0, fmt(col));
    if let Some(e) = err {
        job.append(&format!(" - {e}"), 0.0, egui::TextFormat { font_id: reg(14.0), color: RED, italics: true, ..Default::default() });
    } else if required {
        job.append(" *", 0.0, fmt(RED));
    }
    ui.add(egui::Label::new(job).wrap());
    ui.add_space(8.0);
}

pub(super) fn input(ui: &mut Ui, value: &mut String, password: bool, hint: &str) -> egui::Response {
    Frame::new()
        .fill(pal().input_bg)
        .corner_radius(3)
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .password(password)
                    .hint_text(RichText::new(hint).color(pal().text_muted))
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .desired_width(ui.available_width()),
            )
        })
        .inner
}

impl Form {
    pub(super) fn field(&mut self, ui: &mut Ui, id: &'static str, text: &str, required: bool, password: bool) -> egui::Response {
        let mut value = std::mem::take(match id {
            "login" => &mut self.login,
            "password" => &mut self.password,
            "token" => &mut self.token,
            _ => unreachable!(),
        });
        label(ui, text, required, self.error(id));
        let r = input(ui, &mut value, password, "");
        if self.focus == Some(id) {
            r.request_focus();
            self.focus = None;
        }
        self.rects.insert(id, r.rect);
        *match id {
            "login" => &mut self.login,
            "password" => &mut self.password,
            _ => &mut self.token,
        } = value;
        r
    }
}

pub(super) fn link(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().link)).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let y = r.rect.bottom() - 1.0;
        ui.painter().hline(r.rect.x_range(), y, egui::Stroke::new(1.0, pal().link));
    }
    r
}

pub(super) fn primary(ui: &mut Ui, text: &str, busy: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let on = ui.is_enabled() && !busy;
    let fill = if !on { BLURPLE.gamma_multiply(0.6) } else { crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x48, 0x54, 0xc4), crate::ui::motion::hot(ui.ctx(), &resp)) };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    let shown = if busy { "…" } else { text };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, shown, medium(16.0), if on { Color32::WHITE } else { Color32::from_white_alpha(170) });
    if resp.hovered() && on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(super) fn secondary(ui: &mut Ui, text: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, medium(16.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(super) fn muted_then_link(ui: &mut Ui, muted: &str, action: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(muted).font(reg(14.0)).color(pal().text_muted));
        link(ui, action)
    })
    .inner
}

pub(super) fn problem(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(RED)).wrap());
    ui.add_space(12.0);
}

pub fn show(app: &mut App, ui: &mut Ui) {
    backdrop(ui);
    app.login.rects.clear();
    let wide = app.login.page == Page::Login && !app.login.use_token;
    let want_qr = wide;
    if want_qr != app.login.qr.on {
        app.login.qr.on = want_qr;
        app.net.send(Cmd::QrLogin(want_qr));
    }
    let top = ((ui.available_height() - app.login.card_h) / 2.0).max(16.0);
    let mut h = 0.0;
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add_space(top);
        let card = Frame::new()
            .fill(pal().bg_secondary)
            .corner_radius(4)
            .inner_margin(32)
            .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(80) })
            .show(ui, |ui| {
                ui.set_width(if wide { 720.0 } else { 416.0 });
                ui.with_layout(Layout::top_down(Align::Min), |ui| match app.login.page {
                    Page::Login => login_page(app, ui),
                    Page::Register => register::show(app, ui),
                    Page::Forgot => forgot(app, ui),
                    Page::Sent => sent(app, ui),
                    Page::Mfa => mfa::show(app, ui),
                    Page::Disabled(_) | Page::Deletion => closed(app, ui),
                });
            });
        h = card.response.rect.height();
    });
    if (h - app.login.card_h).abs() > 0.5 {
        ui.ctx().request_repaint();
    }
    app.login.card_h = h;
}

fn login_page(app: &mut App, ui: &mut Ui) {
    let mut cmd = None;
    ui.horizontal_top(|ui| {
        let wide = !app.login.use_token;
        ui.allocate_ui_with_layout(vec2(if wide { 448.0 } else { 416.0 }, 0.0), Layout::top_down(Align::Min), |ui| {
            let f = &mut app.login;
            heading(ui, "Welcome back!", "We're so excited to see you again!");
            let mut enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if f.use_token {
                f.field(ui, "token", "Token", true, true);
                ui.add_space(20.0);
            } else {
                let who = f.field(ui, "login", "Email or Phone Number", true, false);
                ui.add_space(20.0);
                let pw = f.field(ui, "password", "Password", true, true);
                if enter && who.lost_focus() {
                    pw.request_focus();
                    enter = false;
                }
                ui.add_space(4.0);
                let r = link(ui, "Forgot your password?");
                f.rects.insert("forgot", r.rect);
                if r.clicked() {
                    f.go(Page::Forgot);
                    f.focus = Some("login");
                }
                ui.add_space(20.0);
            }
            if let Some(e) = f.err.clone() {
                problem(ui, &e);
            }
            if f.captcha {
                problem(ui, "Finish the captcha in your browser to continue.");
            }
            let r = primary(ui, "Log In", f.busy);
            f.rects.insert("submit", r.rect);
            if (r.clicked() || enter) && !f.busy {
                cmd = Some(());
            }
            ui.add_space(8.0);
            if !f.use_token {
                let r = muted_then_link(ui, "Need an account?", "Register");
                f.rects.insert("register", r.rect);
                if r.clicked() {
                    f.go(Page::Register);
                    f.focus = Some("email");
                }
                ui.add_space(4.0);
            }
            let alt = if f.use_token { "Log in with email instead" } else { "Log in with a token instead" };
            if link(ui, alt).clicked() {
                f.use_token = !f.use_token;
                f.err = None;
                f.errs.clear();
            }
        });
        if !app.login.use_token {
            ui.add_space(32.0);
            ui.allocate_ui_with_layout(vec2(240.0, 0.0), Layout::top_down(Align::Center), |ui| qr::show(app, ui));
        }
    });
    if cmd.is_some() {
        submit(app);
    }
}

fn submit(app: &mut App) {
    let f = &mut app.login;
    f.err = None;
    f.errs.clear();
    if f.use_token {
        let t = f.token.trim().trim_matches('"').to_string();
        if t.is_empty() {
            f.err = Some("Paste your token first.".into());
            return;
        }
        app.sign_in(t);
        return;
    }
    let mut missing = |field: &str, v: &str| {
        if v.trim().is_empty() {
            f.errs.push((field.into(), "This field is required".into()));
        }
    };
    missing("login", &f.login.clone());
    missing("password", &f.password.clone());
    if !f.errs.is_empty() {
        return;
    }
    f.busy = true;
    app.net.send(Cmd::Auth(Auth::Login { login: f.login.trim().to_string(), password: f.password.clone(), undelete: false }));
}

fn forgot(app: &mut App, ui: &mut Ui) {
    let f = &mut app.login;
    heading(ui, "Forgot your password?", "Enter your email or phone number and we'll send you instructions to reset it.");
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    f.field(ui, "login", "Email or Phone Number", true, false);
    ui.add_space(20.0);
    if let Some(e) = f.err.clone() {
        problem(ui, &e);
    }
    if f.captcha {
        problem(ui, "Finish the captcha in your browser to continue.");
    }
    let r = primary(ui, "Send Reset Instructions", f.busy);
    f.rects.insert("submit", r.rect);
    if (r.clicked() || enter) && !f.busy {
        f.errs.clear();
        f.err = None;
        if f.login.trim().is_empty() {
            f.errs.push(("login".into(), "This field is required".into()));
        } else {
            f.busy = true;
            app.net.send(Cmd::Auth(Auth::Forgot(f.login.trim().to_string())));
        }
    }
    ui.add_space(8.0);
    if link(ui, "Back to Login").clicked() {
        f.go(Page::Login);
    }
}

fn sent(app: &mut App, ui: &mut Ui) {
    heading(ui, "Instructions Sent", "Check your email or phone for a link to reset your password.");
    let r = primary(ui, "Return to Login", false);
    app.login.rects.insert("submit", r.rect);
    if r.clicked() {
        app.login.go(Page::Login);
    }
}

fn closed(app: &mut App, ui: &mut Ui) {
    let f = &mut app.login;
    let (title, body, deletion) = match &f.page {
        Page::Disabled(m) => ("Account Disabled", if m.is_empty() { "This account has been disabled." } else { m.as_str() }, false),
        _ => ("Account Scheduled for Deletion", "Your account is scheduled to self-destruct soon.", true),
    };
    heading(ui, title, body);
    if deletion {
        let r = primary(ui, "Re-Activate Account", f.busy);
        f.rects.insert("reactivate", r.rect);
        if r.clicked() && !f.busy {
            f.busy = true;
            app.net.send(Cmd::Auth(Auth::Login { login: f.login.trim().to_string(), password: f.password.clone(), undelete: true }));
        }
        ui.add_space(10.0);
    }
    let r = secondary(ui, "Return to Login");
    f.rects.insert("back", r.rect);
    if r.clicked() {
        f.go(Page::Login);
    }
}
