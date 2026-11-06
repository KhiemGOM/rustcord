//! Server Tag: the four-letter tag members can wear, its pixel badge and colours. Locked until
//! the server has the Server Tag boost perk.

use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::form;
use super::{faint, title};
use crate::state::Id as Snow;
use crate::theme::*;

// (badge id, picture), in the order discord lists them
const BADGES: [(u64, &str, &[u8]); 10] = [
    (6, "leaf", include_bytes!("../../../assets/tag_badges/leaf.svg")),
    (0, "sword", include_bytes!("../../../assets/tag_badges/sword.svg")),
    (7, "heart", include_bytes!("../../../assets/tag_badges/heart.svg")),
    (8, "fire", include_bytes!("../../../assets/tag_badges/fire.svg")),
    (1, "drop", include_bytes!("../../../assets/tag_badges/drop.svg")),
    (2, "skull", include_bytes!("../../../assets/tag_badges/skull.svg")),
    (4, "moon", include_bytes!("../../../assets/tag_badges/moon.svg")),
    (5, "lightning", include_bytes!("../../../assets/tag_badges/lightning.svg")),
    (9, "compass", include_bytes!("../../../assets/tag_badges/compass.svg")),
    (3, "toadstool", include_bytes!("../../../assets/tag_badges/toadstool.svg")),
];

const COLORS: [(&str, &str, &str); 13] = [
    ("Hot pink", "#ff1b90", "#ff7fc0"),
    ("Bright orange", "#ff6a1a", "#ffb27a"),
    ("Golden yellow", "#ffb84b", "#ffe361"),
    ("Teal", "#1abc9c", "#7fffd4"),
    ("Blue teal", "#0abbff", "#87e0ff"),
    ("Purple", "#816bee", "#b79cf8"),
    ("Violet", "#9b4fd1", "#d19ae6"),
    ("Deep purple", "#5b2a9c", "#c878d8"),
    ("Red", "#e8363c", "#ff9a8a"),
    ("Brown", "#7a4a2a", "#d2a679"),
    ("Olive green", "#6b7f2a", "#a5a79b"),
    ("Burgundy", "#8a1538", "#e88aa5"),
    ("Dark gray", "#57595f", "#d1cdd5"),
];

fn hex(s: &str) -> Color32 {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0x5865f2);
    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
}

fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn norm(p: &Value) -> Value {
    json!({
        "tag": p["tag"].as_str().unwrap_or_default(),
        "badge": p["badge"].as_u64().unwrap_or(6),
        "badge_color_primary": p["badge_color_primary"].as_str().unwrap_or(COLORS[5].1),
        "badge_color_secondary": p["badge_color_secondary"].as_str().unwrap_or(COLORS[5].2),
    })
}

pub fn badge(ui: &Ui, r: Rect, id: u64) {
    if let Some((_, name, svg)) = BADGES.iter().find(|b| b.0 == id) {
        egui::Image::from_bytes(format!("bytes://tag-badge-{name}.svg"), *svg).show_loading_spinner(false).paint_at(ui, r);
    }
}

fn info(ui: &mut Ui, w: f32, text: &str, link: Option<&str>) -> bool {
    let mut clicked = false;
    Frame::new().fill(Color32::from_rgba_unmultiplied(0, 168, 252, 14)).stroke(Stroke::new(1.0, FOCUS)).corner_radius(4).inner_margin(Margin::symmetric(10, 10)).show(ui, |ui| {
        ui.set_width(w - 22.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 11.0, FOCUS);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "i", italic(15.0), Color32::WHITE);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(text).font(medium(12.0)).color(Color32::WHITE));
                if let Some(l) = link {
                    clicked = super::super::settings::link(ui, l).clicked();
                }
            });
        });
    });
    clicked
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    let unlocked = app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_TAGS"));
    let Some(raw) = app.server_data("profile", format!("/guilds/{gid}/profile")).cloned() else {
        title(ui, "Server Tag", "");
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let orig = norm(&raw);
    let mut d = form::take(app, "tag", &orig);
    ui.set_max_width(332.0);
    title(ui, "Server Tag", "");
    ui.label(RichText::new("Create a tag that your server members can display next to their name! Anyone outside your server can view your Server Profile through the Server Tag, and if applications are enabled, they can apply to join. Your Server Profile is private. Users will not be able to see it from a Server Tag.").font(reg(14.0)).line_height(Some(18.0)).color(pal().text_normal));
    ui.add_space(16.0);
    if raw["visibility"].as_u64() != Some(2) && info(ui, 332.0, "Your Server Profile is private. Users will not be able to see it from a Server Tag.", Some("Edit Setting")) {
        if let Some(s) = &mut app.server {
            s.page = super::Page::Profile;
        }
    }
    ui.add_space(16.0);
    if !unlocked {
        let (r, resp) = ui.allocate_exact_size(vec2(220.0, 40.0), Sense::click());
        ui.painter().rect_filled(r.expand(1.0), CornerRadius::same(8), Color32::from_rgba_unmultiplied(0x9b, 0x5c, 0xff, 90));
        ui.painter().rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(Color32::WHITE, Color32::from_gray(235), crate::ui::motion::hot(ui.ctx(), &resp)));
        super::super::icons::draw(ui.painter(), super::super::icons::Icon::BoostGem, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(18.0, 18.0)), Color32::BLACK);
        ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, "Unlock with Boosting", medium(16.0), Color32::BLACK);
        super::super::settings::mark(ui, "button:Unlock with Boosting", r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.server = None;
            app.ui.guild = Some(gid);
            app.open_boosts(gid);
            return;
        }
        ui.add_space(8.0);
    }
    ui.scope(|ui| {
        if !unlocked {
            ui.set_opacity(0.35);
        }
        ui.add_enabled_ui(unlocked, |ui| body(ui, &mut d));
    });
    preview(ui.ctx(), &d);
    form::put(app, "tag", d);
    let ctx = ui.ctx().clone();
    form::bar(app, &ctx, "tag", "profile", &orig, format!("/guilds/{gid}/profile"), Value::Object);
}

fn body(ui: &mut Ui, d: &mut Value) {
    let label = |ui: &mut Ui, t: &str| {
        ui.label(RichText::new(t).font(medium(16.0)).color(pal().header_primary));
        ui.add_space(8.0);
    };
    label(ui, "Choose Name");
    let id = d["badge"].as_u64().unwrap_or(6);
    let mut tag = d["tag"].as_str().unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(14, 14)).show(ui, |ui| {
            ui.set_width(138.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                badge(ui, r, id);
                let resp = ui.add(egui::TextEdit::singleline(&mut tag).frame(Frame::NONE).margin(Margin::ZERO).char_limit(4).font(reg(28.0)).text_color(pal().text_normal).hint_text(RichText::new("WUMP").color(pal().text_muted)).desired_width(ui.available_width()));
                super::super::settings::mark(ui, "field:tag", resp.rect);
            });
        });
        ui.label(RichText::new("You may use max 4 characters, alphabet and numbers.").font(reg(12.0)).color(pal().header_secondary));
    });
    tag.retain(|c| c.is_ascii_alphanumeric());
    d["tag"] = json!(tag);
    ui.add_space(16.0);
    info(ui, 332.0, "Updating your Server Tag will require all of your members to manually reapply the tag to their profile. We do this to prevent abuse.", Some("Learn more"));
    ui.add_space(32.0);
    label(ui, "Choose Badge");
    ui.label(RichText::new("Changing just the badge won't remove the Server Tag from members.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
    for row in BADGES.chunks(5) {
        ui.horizontal(|ui| {
            for (bid, name, _) in row {
                let (r, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
                ui.painter().rect(r, CornerRadius::same(4), faint(20), Stroke::new(1.0, if *bid == id { BLURPLE } else { faint(51) }), egui::StrokeKind::Inside);
                badge(ui, r.shrink(14.0), *bid);
                super::super::settings::mark(ui, &format!("badge:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    d["badge"] = json!(bid);
                }
            }
        });
    }
    ui.add_space(24.0);
    label(ui, "Choose Color");
    let (p1, p2) = (d["badge_color_primary"].as_str().unwrap_or_default().to_lowercase(), d["badge_color_secondary"].as_str().unwrap_or_default().to_lowercase());
    for row in COLORS.chunks(5) {
        ui.horizontal(|ui| {
            for (name, a, b) in row {
                let (r, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
                let on = *a == p1 && *b == p2;
                pair(ui, r, hex(a), hex(b), on);
                super::super::settings::mark(ui, &format!("color:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    d["badge_color_primary"] = json!(a);
                    d["badge_color_secondary"] = json!(b);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
        let custom = !COLORS.iter().any(|(_, a, b)| *a == p1 && *b == p2);
        pair(ui, r, hex(&p1), hex(&p2), custom);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        egui::Popup::menu(&resp).show(|ui| {
            for (k, c) in [("badge_color_primary", "Primary"), ("badge_color_secondary", "Secondary")] {
                ui.horizontal(|ui| {
                    let mut col = hex(d[k].as_str().unwrap_or("#5865f2"));
                    ui.label(RichText::new(c).font(medium(14.0)).color(pal().text_normal));
                    if egui::color_picker::color_edit_button_srgba(ui, &mut col, egui::color_picker::Alpha::Opaque).changed() {
                        d[k] = json!(to_hex(col));
                    }
                });
            }
        });
        let (r, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
        ui.painter().rect(r, CornerRadius::same(4), faint(20), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
        super::super::icons::draw(ui.painter(), super::super::icons::Icon::Refresh, r.shrink(20.0), pal().header_secondary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            d["badge_color_primary"] = json!(COLORS[5].1);
            d["badge_color_secondary"] = json!(COLORS[5].2);
        }
    });
    ui.add_space(8.0);
    ui.label(RichText::new("These preset color pairs set your badge's primary and secondary colors.").font(reg(12.0)).color(pal().header_secondary));
}

fn pair(ui: &Ui, r: Rect, a: Color32, b: Color32, on: bool) {
    ui.painter().rect(r, CornerRadius::same(4), faint(20), Stroke::new(1.0, if on { BLURPLE } else { faint(51) }), egui::StrokeKind::Inside);
    let c = r.shrink(16.0);
    ui.painter().rect_filled(c, CornerRadius::same(4), a);
    ui.painter().rect_filled(Rect::from_min_max(pos2(c.center().x, c.top()), c.max), CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, b);
}

/// The chat sample at the right: how the tag looks next to a name.
fn preview(ctx: &egui::Context, d: &Value) {
    let Some(area) = ctx.data(|m| m.get_temp::<Rect>(Id::new("server_area"))) else { return };
    let at = pos2(area.center().x - 228.0 + 385.0, area.top() + 60.0);
    let tag = d["tag"].as_str().filter(|t| !t.is_empty()).unwrap_or("WUMP").to_uppercase();
    let (c1, c2) = (hex(d["badge_color_primary"].as_str().unwrap_or("#816bee")), hex(d["badge_color_secondary"].as_str().unwrap_or("#b79cf8")));
    let id = d["badge"].as_u64().unwrap_or(6);
    let shown = egui::Area::new(Id::new("server_tag_preview")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        let rows = [("Roka", "anyone down for aram", Color32::from_rgb(0x75, 0x7e, 0xf6), false), ("hongo", "count me in", Color32::from_rgb(0x2d, 0xc7, 0x70), false), ("Lily", "check out my tag!", Color32::from_rgb(0xf0, 0x8a, 0x36), true), ("sharon", "woah how did you get that", Color32::from_rgb(0x8a, 0xd0, 0x6a), false), ("Lily", "anyone here can get it!", Color32::from_rgb(0xf0, 0x8a, 0x36), false)];
        for (name, msg, col, mine) in rows {
            let (r, _) = ui.allocate_exact_size(vec2(300.0, 52.0), Sense::hover());
            let dim = if mine { 1.0 } else { 0.55 };
            ui.painter().circle_filled(pos2(r.left() + 20.0, r.center().y), 20.0, col.gamma_multiply(0.6 * dim + 0.2));
            let n = ui.painter().text(pos2(r.left() + 52.0, r.top() + 13.0), Align2::LEFT_CENTER, name, semi(16.0), col.gamma_multiply(dim));
            if mine {
                let g = ui.painter().layout_no_wrap(tag.clone(), semi(12.0), Color32::WHITE);
                let chip = Rect::from_min_size(pos2(n.right() + 6.0, r.top() + 3.0), vec2(g.size().x + 30.0, 20.0));
                ui.painter().rect_filled(chip, CornerRadius::same(4), Color32::from_rgba_unmultiplied(c1.r(), c1.g(), c1.b(), 70));
                badge(ui, Rect::from_min_size(chip.min + vec2(4.0, 3.0), vec2(14.0, 14.0)), id);
                ui.painter().galley(pos2(chip.left() + 22.0, chip.center().y - g.size().y / 2.0), g, c2);
            }
            ui.painter().text(pos2(r.left() + 52.0, r.top() + 34.0), Align2::LEFT_CENTER, msg, medium(16.0), pal().text_normal.gamma_multiply(dim));
        }
    });
    ctx.move_to_top(shown.response.layer_id);
}
