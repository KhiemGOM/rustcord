//! Developer's Application Test Mode: the application id (and where its embedded app is served from)
//! kept while test mode is on.

use egui::Ui;

use super::modals::{Act, dialog as show, label};
use super::{dropdown, mark, toggle};
use crate::cache::TestApp;
use crate::theme::*;
use crate::ui::App;

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
