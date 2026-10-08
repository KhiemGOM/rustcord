use egui::{Align, Color32, Frame, Layout, Margin, RichText, Sense, Ui};

use super::{App, button};
use crate::net::Cmd;
use crate::theme::*;

#[derive(Default)]
pub struct Form {
    pub login: String,
    pub password: String,
    pub token: String,
    pub code: String,
    pub mfa: Option<String>,
    pub err: Option<String>,
    pub busy: bool,
    pub use_token: bool,
}

fn backdrop(ui: &Ui) {
    let r = ui.max_rect();
    let mut mesh = egui::Mesh::default();
    let top = Color32::from_rgb(0x40, 0x4e, 0xed);
    let bottom = Color32::from_rgb(0x1b, 0x1f, 0x5c);
    mesh.colored_vertex(r.left_top(), top);
    mesh.colored_vertex(r.right_top(), Color32::from_rgb(0x5a, 0x3c, 0xc8));
    mesh.colored_vertex(r.right_bottom(), bottom);
    mesh.colored_vertex(r.left_bottom(), Color32::from_rgb(0x12, 0x16, 0x3d));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
}

fn field(ui: &mut Ui, label: &str, value: &mut String, password: bool) -> egui::Response {
    ui.label(RichText::new(label).font(medium(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let r = Frame::new()
        .fill(pal().bg_tertiary)
        .corner_radius(3)
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .password(password)
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .desired_width(ui.available_width()),
            )
        })
        .inner;
    ui.add_space(20.0);
    r
}

pub fn show(app: &mut App, ui: &mut Ui) {
    backdrop(ui);
    let f = &mut app.login;
    let mut submit = false;
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        let h = ui.available_height();
        ui.add_space(((h - 420.0) / 2.0).max(16.0));
        Frame::new()
            .fill(pal().bg_primary)
            .corner_radius(5)
            .inner_margin(32)
            .shadow(egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(60) })
            .show(ui, |ui| {
                ui.set_width(416.0);
                ui.vertical_centered(|ui| {
                    let title = if f.mfa.is_some() { "Multi-Factor Authentication" } else { "Welcome back!" };
                    ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
                    ui.add_space(8.0);
                    let sub = if f.mfa.is_some() {
                        "Enter the 6-digit code from your authenticator app."
                    } else {
                        "We're so excited to see you again!"
                    };
                    ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
                });
                ui.add_space(20.0);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if f.mfa.is_some() {
                    let r = field(ui, "Enter Discord Auth Code", &mut f.code, false);
                    if !r.has_focus() && f.code.is_empty() {
                        r.request_focus();
                    }
                } else if f.use_token {
                    field(ui, "Token", &mut f.token, true);
                } else {
                    field(ui, "Email or Phone Number", &mut f.login, false);
                    field(ui, "Password", &mut f.password, true);
                }
                if let Some(e) = &f.err {
                    ui.label(RichText::new(e).font(reg(14.0)).color(RED));
                    ui.add_space(12.0);
                }
                let label = if f.busy { "…" } else if f.mfa.is_some() { "Confirm" } else { "Log In" };
                if button(ui, label, BLURPLE, ui.available_width()).clicked() || enter {
                    submit = !f.busy;
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let alt = if f.use_token { "Log in with email instead" } else { "Log in with a token instead" };
                    let r = ui.add(egui::Label::new(RichText::new(alt).font(reg(14.0)).color(pal().link)).sense(Sense::click()));
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if r.clicked() {
                        f.use_token = !f.use_token;
                        f.err = None;
                    }
                });
            });
    });

    if !submit {
        return;
    }
    f.err = None;
    if let Some(ticket) = &f.mfa {
        f.busy = true;
        app.net.send(Cmd::Mfa { ticket: ticket.clone(), code: f.code.trim().to_string() });
    } else if f.use_token {
        let t = f.token.trim().trim_matches('"').to_string();
        if t.is_empty() {
            f.err = Some("Paste your token first.".into());
            return;
        }
        crate::cache::set_token(Some(&t));
        app.net.send(Cmd::Start(t.clone()));
        app.token = Some(t);
        app.login = Default::default();
    } else {
        f.busy = true;
        app.net.send(Cmd::Login { login: f.login.trim().to_string(), password: f.password.clone() });
    }
}
