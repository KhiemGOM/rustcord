//! Developer's Application Test Mode: the application id (and where its embedded app is served from)
//! kept while test mode is on.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Frame, Margin, Rect, Sense, Ui, pos2, vec2};

use super::modals::{Act, dialog as show, label};
use super::{dropdown, mark, toggle};
use crate::cache::TestApp;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

pub struct Form {
    id: String,
    proxy: bool,
    port: String,
}

pub fn row(app: &mut App, ui: &mut Ui) -> bool {
    let mut on = app.settings.test_app.is_some();
    if !toggle(ui, &mut on, "Application Test Mode", "Enter your Discord application ID to enable test mode for that application.") {
        return false;
    }
    if on {
        let at = app.settings.test_app.clone();
        app.sform.test_mode = Some(Form { id: at.as_ref().map(|a| a.id.clone()).unwrap_or_default(), proxy: at.as_ref().is_some_and(|a| a.proxy), port: at.map_or("8080".into(), |a| a.port) });
        false
    } else {
        app.settings.test_app = None;
        true
    }
}

pub fn dialog(app: &mut App, ctx: &egui::Context) {
    let Some(f) = &mut app.sform.test_mode else { return };
    let active = app.settings.test_app.as_ref().is_some_and(|a| a.id == f.id);
    let valid = f.id.bytes().all(|b| b.is_ascii_digit());
    let ready = valid && !f.id.is_empty() && (f.proxy || !f.port.is_empty());
    let (go, fill) = if active { ("Clear", RED) } else { ("Activate", if ready { BLURPLE } else { BLURPLE.gamma_multiply(0.5) }) };
    let act = show(
        ctx,
        "settings_test_mode",
        "Application Test Mode",
        "Enter your Discord application ID to enable test mode for that application. An application in test mode will allow you to view related unpublished SKUs and bypass related purchases to make development easier.",
        go,
        fill,
        None,
        |ui| {
            label(ui, "Application ID *");
            let r = super::input(ui, &mut f.id, "", false);
            f.id.truncate(19);
            mark(ui, "test_mode:id", r.rect);
            if !valid {
                ui.label(egui::RichText::new("Invalid application ID").font(reg(14.0)).color(RED));
            }
            ui.add_space(16.0);
            label(ui, "Select URL Origin Type (Embedded Apps Only)");
            let shown = if f.proxy { "Discord Proxy" } else { "Localhost" };
            let w = ui.available_width();
            let mut proxy = f.proxy;
            dropdown(ui, w, None, shown, |ui| {
                for (v, name) in [(false, "Localhost"), (true, "Discord Proxy")] {
                    if ui.selectable_label(proxy == v, name).clicked() {
                        proxy = v;
                    }
                }
            });
            f.proxy = proxy;
            if !f.proxy {
                ui.add_space(16.0);
                label(ui, "Localhost Port Number *");
                let r = super::input(ui, &mut f.port, "", false);
                f.port.truncate(5);
                mark(ui, "test_mode:port", r.rect);
            }
        },
    );
    match act {
        Act::Go if active => {
            app.settings.test_app = None;
            app.sform.test_mode = None;
            app.save_settings(ctx);
        }
        Act::Go if ready => {
            app.settings.test_app = Some(TestApp { id: f.id.clone(), proxy: f.proxy, port: f.port.clone() });
            app.sform.test_mode = None;
            app.save_settings(ctx);
        }
        Act::Close => app.sform.test_mode = None,
        _ => {}
    }
}

/// The strip across the top while an application is in test mode; its close button ends the mode.
pub fn banner(app: &mut App, ui: &mut Ui) {
    let Some(name) = app.settings.test_app.as_ref().map(|a| a.id.clone()) else { return };
    let [fill, ink] = pal().notice;
    let mut job = LayoutJob::default();
    job.halign = Align::Center;
    for (text, font) in [
        ("Test mode is currently active for ", medium(14.0)),
        (name.as_str(), semi(14.0)),
        (". Unpublished SKUs will appear and you will not be charged for any related payments.", medium(14.0)),
    ] {
        job.append(text, 0.0, TextFormat::simple(font, ink));
    }
    let mut closed = false;
    egui::Panel::top("test_mode_banner")
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(fill).inner_margin(Margin { left: 40, right: 40, top: 9, bottom: 9 }))
        .show(ui, |ui| {
            let w = ui.available_width();
            job.wrap.max_width = w;
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let (r, _) = ui.allocate_exact_size(vec2(w, galley.size().y.max(18.0)), Sense::hover());
            ui.painter().galley(pos2(r.center().x - galley.size().x / 2.0, r.top()), galley, ink);
            let full = ui.max_rect();
            let hit = Rect::from_min_size(pos2(full.right() + 40.0 - 36.0, full.top() - 9.0), vec2(36.0, 36.0));
            let resp = ui.interact(hit, egui::Id::new("test_mode_close"), Sense::click());
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(hit.center(), vec2(16.0, 16.0)), ink);
            mark(ui, "test_mode:close", hit);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            closed = resp.clicked();
        });
    if closed {
        app.settings.test_app = None;
        app.save_settings(ui.ctx());
    }
}
