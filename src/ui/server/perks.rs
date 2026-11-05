//! Boost Perks: the boost progress bar, and the banner, invite background and vanity link that
//! boost levels unlock.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use super::{faint, title};
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    title(ui, "Boost Perks", "");
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let vanity = app.server_data("vanity", format!("/guilds/{gid}/vanity-url")).cloned().unwrap_or(Value::Null);
    ui.set_max_width(660.0);
    let has = |f: &str| g.features.iter().any(|x| x == f);

    let mut on = g.boost_bar;
    if super::super::settings::toggle(ui, &mut on, "Show Boost progress bar", "Servers with the Boost progress bar enabled get more Boosts. This progress bar will display in your channel list, attached to your server name (or server banner if you have one set).") {
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "premium_progress_bar_enabled": on })), Tag::Quiet);
        if let Some(g) = app.store.guilds.get_mut(&gid) {
            g.boost_bar = on;
        }
    }
    example(ui, &g.name);
    ui.add_space(32.0);

    let mut open_boost = false;
    let mut upload = None;
    perk(ui, "Server Banner Background", 2, "This image will display at the top of your channels list. The recommended minimum size is 960x540 and recommended aspect ratio is 16:9.", "https://support.discord.com/hc/en-us/articles/360028716472", has("BANNER"), g.banner.is_some(), "banner", &mut open_boost, &mut upload);
    ui.add_space(32.0);
    perk(ui, "Server Invite Background", 1, "This image will display when your server invite is viewed in a browser, as well as in invite confirmation screens and Server Onboarding. The recommended minimum size is 1920x1080 and recommended aspect ratio is 16:9.", "https://support.discord.com/hc/en-us/articles/4415841146391", has("INVITE_SPLASH"), false, "splash", &mut open_boost, &mut upload);
    ui.add_space(32.0);

    heading(ui, "Custom Invite Link", 3);
    ui.label(RichText::new("Bring others to your server easily with your own customized invite link. Heads up though, anyone with the link can join and you'll need at least one text channel that is open to all server members.").font(reg(14.0)).color(pal().text_normal));
    ui.add_space(4.0);
    learn(ui, "https://support.discord.com/hc/en-us/articles/115001542132");
    ui.add_space(12.0);
    if has("VANITY_URL") {
        let mut code = ui.data_mut(|d| d.get_temp::<(Snow, String)>(egui::Id::new("vanity_draft"))).filter(|(g, _)| *g == gid).map(|(_, c)| c).unwrap_or_else(|| vanity["code"].as_str().unwrap_or_default().to_string());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                ui.set_width(360.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("discord.gg/").font(reg(16.0)).color(pal().header_secondary));
                    let r = ui.add(egui::TextEdit::singleline(&mut code).frame(Frame::NONE).margin(Margin::ZERO).font(reg(16.0)).text_color(pal().text_normal).desired_width(ui.available_width()));
                    super::super::settings::mark(ui, "field:vanity", r.rect);
                });
            });
            code.retain(|c| c.is_ascii_alphanumeric() || c == '-');
            let changed = code != vanity["code"].as_str().unwrap_or_default();
            ui.add_enabled_ui(changed, |ui| {
                if form::button(ui, "Save", true).clicked() {
                    app.rest("PATCH", format!("/guilds/{gid}/vanity-url"), Some(json!({ "code": code })), Tag::Server("vanity"));
                }
            });
        });
        ui.data_mut(|d| d.insert_temp(egui::Id::new("vanity_draft"), (gid, code)));
    } else {
        open_boost |= unlock(ui);
    }

    if open_boost {
        app.server = None;
        app.ui.guild = Some(gid);
        app.open_boosts(gid);
        return;
    }
    if let Some((field, path)) = upload
        && let Some(uri) = form::data_uri(&path)
    {
        app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ (field): uri })), Tag::Quiet);
    }
}

fn heading(ui: &mut Ui, name: &str, lvl: u8) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(name).font(semi(16.0)).color(pal().header_primary));
        let g = ui.painter().layout_no_wrap(format!("LVL {lvl}"), bold(10.0), Color32::WHITE);
        let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 14.0, 16.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgb(0x87, 0x57, 0xd6));
        icons::draw(ui.painter(), Icon::BoostGem, Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(8.0, 8.0)), Color32::WHITE);
        ui.painter().galley(pos2(r.left() + 14.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    });
    ui.add_space(4.0);
}

fn learn(ui: &mut Ui, url: &str) {
    if super::super::settings::link(ui, "Learn more").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

fn unlock(ui: &mut Ui) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(220.0, 40.0), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { Color32::from_gray(235) } else { Color32::WHITE });
    icons::draw(ui.painter(), Icon::BoostGem, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(18.0, 18.0)), Color32::BLACK);
    ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, "Unlock with Boosting", medium(16.0), Color32::BLACK);
    super::super::settings::mark(ui, "button:Unlock with Boosting", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

#[allow(clippy::too_many_arguments)]
fn perk(ui: &mut Ui, name: &str, lvl: u8, desc: &str, url: &str, unlocked: bool, set: bool, field: &'static str, open_boost: &mut bool, upload: &mut Option<(&'static str, std::path::PathBuf)>) {
    heading(ui, name, lvl);
    ui.label(RichText::new(desc).font(reg(14.0)).color(pal().text_normal));
    ui.add_space(4.0);
    learn(ui, url);
    ui.add_space(12.0);
    if !unlocked {
        *open_boost |= unlock(ui);
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if form::button(ui, if set { "Change Image" } else { "Upload Image" }, true).clicked()
            && let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
        {
            *upload = Some((field, p));
        }
    });
}

fn example(ui: &mut Ui, name: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(240.0, 84.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_secondary);
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    ui.painter().text(pos2(r.left() + 16.0, r.top() + 22.0), Align2::LEFT_CENTER, name, bold(16.0), pal().header_primary);
    let bar = Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 44.0), vec2(r.width() - 32.0, 8.0));
    ui.painter().rect_filled(bar, CornerRadius::same(4), faint(51));
    ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * 0.45, 8.0)), CornerRadius::same(4), Color32::from_rgb(0xff, 0x73, 0xfa));
    ui.painter().text(pos2(r.left() + 16.0, r.top() + 64.0), Align2::LEFT_CENTER, "2/7 Boosts", medium(12.0), pal().header_secondary);
}
