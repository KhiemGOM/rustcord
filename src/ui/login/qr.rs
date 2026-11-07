use egui::{Align2, Color32, CornerRadius, Rect, RichText, Ui, pos2, vec2};

use crate::api;
use crate::auth::Qr;
use crate::net::Cmd;
use crate::theme::*;
use crate::ui::{App, avatar, icons};

#[derive(Default)]
pub struct State {
    pub on: bool,
    pub state: Qr,
    code: Option<(String, usize, Vec<bool>)>,
}

impl State {
    fn modules(&mut self, text: &str) -> (usize, &[bool]) {
        if self.code.as_ref().is_none_or(|c| c.0 != text) {
            let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::Q).ok();
            let cells = code.as_ref().map(|c| c.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect()).unwrap_or_default();
            self.code = Some((text.to_string(), code.map_or(0, |c| c.width()), cells));
        }
        let c = self.code.as_ref().unwrap();
        (c.1, &c.2)
    }
}

fn code(ui: &mut Ui, qr: &mut State, text: &str, ready: bool) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(176.0, 176.0), egui::Sense::hover());
    let ink = if ready { Color32::BLACK } else { Color32::from_gray(0xc4) };
    ui.painter().rect_filled(rect, CornerRadius::same(2), Color32::WHITE);
    let (n, cells) = qr.modules(text);
    let inner = rect.shrink(8.0);
    let cell = inner.width() / n.max(1) as f32;
    for (i, dark) in cells.iter().enumerate() {
        if *dark {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            let a = pos2(inner.left() + (x * cell).round(), inner.top() + (y * cell).round());
            let b = pos2(inner.left() + ((x + 1.0) * cell).round(), inner.top() + ((y + 1.0) * cell).round());
            ui.painter().rect_filled(Rect::from_min_max(a, b), 0.0, ink);
        }
    }
    let badge = Rect::from_center_size(rect.center(), vec2(44.0, 44.0));
    ui.painter().rect_filled(badge, CornerRadius::same(8), Color32::WHITE);
    icons::draw(ui.painter(), icons::Icon::Discord, Rect::from_center_size(rect.center(), vec2(32.0, 32.0)), BLURPLE);
    rect
}

fn texts(ui: &mut Ui, title: &str, body: &str) {
    ui.add_space(16.0);
    ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
    ui.add_space(8.0);
    ui.add(egui::Label::new(RichText::new(body).font(reg(16.0)).color(pal().text_normal)).wrap());
}

fn passkey(app: &mut App, ui: &mut Ui) {
    ui.add_space(8.0);
    let busy = app.login.busy;
    ui.vertical_centered(|ui| {
        let r = ui.add_enabled_ui(!busy, |ui| super::link(ui, "Or sign in with a passkey")).inner;
        app.login.rects.insert("passkey", r.rect);
        if r.clicked() && !busy {
            app.login.err = None;
            app.login.busy = true;
            app.net.send(Cmd::Auth(crate::auth::Cmd::Passkey));
        }
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let qr = &mut app.login.qr;
    match &qr.state {
        Qr::Init => {
            let r = code(ui, qr, "placeholder", false);
            app.login.rects.insert("qr", r);
            texts(ui, "Log in with QR Code", "Scan this with the Discord mobile app to log in instantly.");
            passkey(app, ui);
        }
        Qr::Ready(fp) => {
            let text = format!("https://discord.com/ra/{fp}");
            let r = code(ui, qr, &text, true);
            app.login.rects.insert("qr", r);
            texts(ui, "Log in with QR Code", "Scan this with the Discord mobile app to log in instantly.");
            passkey(app, ui);
        }
        Qr::Ticket { id, avatar: hash, name } => {
            let url = match hash {
                Some(h) => format!("{}/avatars/{id}/{h}.png?size=128", api::cdn()),
                None => format!("{}/embed/avatars/{}.png", api::cdn(), id.parse::<u64>().unwrap_or(0) >> 22 % 6),
            };
            let (name, shown) = (name.clone(), ui.allocate_exact_size(vec2(120.0, 120.0), egui::Sense::hover()).0);
            avatar(ui, shown, &url);
            app.login.rects.insert("qr_user", shown);
            texts(ui, "Check your phone!", &format!("Logging in as {name}"));
            ui.add_space(16.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), egui::Sense::click());
            let col = crate::ui::motion::mix(pal().link, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp));
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Not me, start over", reg(14.0), col);
            app.login.rects.insert("qr_cancel", rect);
            if resp.clicked() {
                app.net.send(Cmd::QrLogin(true));
            }
        }
        Qr::Working => {
            ui.add_space(70.0);
            ui.label(RichText::new("Logging in…").font(semi(24.0)).color(pal().header_primary));
        }
    }
}
