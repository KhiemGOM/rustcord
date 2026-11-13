//! Create Your Server: the template list, the question about who it's for, then the name and icon.

use base64::Engine;
use egui::{Align2, Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::App;
use super::create::{faint, field, label, modal, plate};
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::theme::*;

const TEMPLATES: [(Icon, &str, &str); 7] = [
    (Icon::Plus, "Create My Own", "2TffvPucqHkN"),
    (Icon::Stage, "Gaming", "hvtBQMfw6uSJ"),
    (Icon::Friends, "Friends", "hgM48av5Q69A"),
    (Icon::Bookmark, "Study Group", "FbwUwRp4j8Es"),
    (Icon::Rules, "School Club", "Ctg7PUHcQmZu"),
    (Icon::Discover, "Local Community", "64UDvRNCC52Y"),
    (Icon::Pencil, "Artists & Creators", "6exdzMgjZgah"),
];

pub struct Wizard {
    step: u8,
    code: &'static str,
    name: String,
    icon: Option<String>,
    pub busy: bool,
}

impl Wizard {
    pub fn new(app: &App) -> Self {
        let me = app.store.user(app.store.me).map(|u| u.display().to_string()).unwrap_or_default();
        Wizard { step: 0, code: TEMPLATES[0].2, name: format!("{me}'s server"), icon: None, busy: false }
    }
}

fn row(ui: &mut Ui, icon: Icon, text: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let fill = crate::ui::motion::mix(Color32::from_black_alpha(31), faint(31), crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.left() + 32.0, r.center().y), vec2(24.0, 24.0)), pal().header_primary);
    ui.painter().text(pos2(r.left() + 62.0, r.center().y), Align2::LEFT_CENTER, text, medium(16.0), pal().header_primary);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(8.0);
    resp.clicked()
}

fn heading(ui: &mut Ui, title: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
        ui.add_space(8.0);
        ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
    });
    ui.add_space(24.0);
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(w) = app.wizard.as_mut() else { return };
    let (mut close, mut join, mut back, mut create, mut pick) = (false, false, false, false, false);
    close |= modal(ctx, "new_server", Some(""), |ui| match w.step {
        0 => {
            heading(ui, "Create Your Server", "Your server is where you and your friends hang out. Make yours and start talking.");
            for (i, (icon, name, code)) in TEMPLATES.iter().enumerate() {
                if i == 1 {
                    ui.add_space(8.0);
                    ui.label(RichText::new("START FROM A TEMPLATE").font(semi(12.0)).color(pal().header_secondary));
                    ui.add_space(8.0);
                }
                if row(ui, *icon, name) {
                    w.code = code;
                    w.step = 1;
                }
            }
            ui.add_space(16.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("Have an invite already?").font(semi(20.0)).color(pal().header_primary)));
            ui.add_space(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            join = plate(ui, r, "Join a Server", false, true);
        }
        1 => {
            heading(ui, "Tell Us More About Your Server", "In order to help you with your setup, is your new server for just a few friends or a larger community?");
            if row(ui, Icon::Friends, "For me and my friends") | row(ui, Icon::People, "For a club or community") {
                w.step = 2;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 140.0);
                ui.label(RichText::new("Not sure? You can").color(pal().text_normal));
                if ui.add(egui::Label::new(RichText::new("skip this question").color(pal().link)).sense(Sense::click())).clicked() {
                    w.step = 2;
                }
                ui.label(RichText::new("for now.").color(pal().text_normal));
            });
            ui.add_space(40.0);
            back = ui.add(egui::Label::new(RichText::new("Back").font(medium(14.0)).color(pal().header_primary)).sense(Sense::click())).clicked();
        }
        _ => {
            heading(ui, "Customize Your Server", "Give your new server a personality with a name and an icon. You can always change it later.");
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 80.0), Sense::hover());
            let r = Rect::from_center_size(row.center(), vec2(80.0, 80.0));
            ui.painter().circle_stroke(r.center(), 40.0, Stroke::new(1.5, pal().header_secondary));
            icons::draw(ui.painter(), Icon::Image, Rect::from_center_size(r.center() - vec2(0.0, 8.0), vec2(24.0, 24.0)), pal().header_secondary);
            ui.painter().text(r.center() + vec2(0.0, 16.0), Align2::CENTER_CENTER, if w.icon.is_some() { "CHANGE" } else { "UPLOAD" }, semi(12.0), pal().header_secondary);
            ui.painter().circle_filled(r.right_top() - vec2(8.0, -8.0), 12.0, pal().blurple);
            icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(r.right_top() - vec2(8.0, -8.0), vec2(16.0, 16.0)), Color32::WHITE);
            pick = ui.interact(r, ui.id().with("icon"), Sense::click()).clicked();
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                label(ui, "Server Name");
            });
            field(ui, &mut w.name, "", None, 1, None);
            ui.add_space(8.0);
            ui.label(RichText::new("By creating a server, you agree to Discord's Community Guidelines.").font(reg(12.0)).color(pal().header_secondary));
            ui.add_space(24.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            back = ui.interact(Rect::from_min_size(r.min, vec2(64.0, 40.0)), ui.id().with("back"), Sense::click()).clicked();
            ui.painter().text(pos2(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, "Back", medium(14.0), pal().header_primary);
            create = plate(ui, Rect::from_min_size(pos2(r.right() - 96.0, r.top()), vec2(96.0, 40.0)), "Create", true, !w.name.trim().is_empty() && !w.busy);
        }
    });
    if back {
        w.step -= 1;
    }
    if pick && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
        if let Ok(bytes) = std::fs::read(&path) {
            let mime = match bytes.get(..4) {
                Some([0x89, b'P', ..]) => "image/png",
                Some([b'G', b'I', ..]) => "image/gif",
                Some([b'R', b'I', ..]) => "image/webp",
                _ => "image/jpeg",
            };
            w.icon = Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
        }
    }
    if create {
        w.busy = true;
        let mut body = json!({ "name": w.name.trim(), "icon": w.icon, "guild_template_code": w.code });
        if w.code == TEMPLATES[0].2 {
            body["channels"] = json!([]);
            body["system_channel_id"] = json!(null);
        }
        app.rest("POST", "/guilds".into(), Some(body), Tag::NewServer);
    }
    if join {
        app.wizard = None;
        app.invite = Some(String::new());
    } else if close {
        app.wizard = None;
    }
}

pub fn created(app: &mut App, v: &serde_json::Value) {
    app.wizard = None;
    if v["id"].is_string() {
        let gid = crate::state::sid(&v["id"]);
        if !app.store.guilds.contains_key(&gid) {
            app.store.apply("GUILD_CREATE", v);
        }
        app.select_guild(Some(gid));
    }
}