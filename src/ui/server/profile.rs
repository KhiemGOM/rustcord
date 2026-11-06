//! Server Profile: name, icon, banner colour, traits, description, games and who can see it.
//! The page edits a draft and the bar at the bottom saves it to `/guilds/<id>/profile`.

use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::sync::Arc;

use super::super::App;
use super::super::icons::{self, Icon};
use super::form::{self, Pick};
use super::{faint, title};
use crate::state::{Id as Snow, sid, snowflake_ms};
use crate::theme::*;

// discord's banner presets: the colour sent as brand_color_primary and the swatch's top and bottom
const BANNERS: [(&str, &str, [u8; 3], [u8; 3]); 9] = [
    ("Soul", "#ff1c90", [0xff, 0x4b, 0xb3], [0xff, 0x86, 0xdf]),
    ("Volcano", "#e81d1e", [0xf1, 0x49, 0x3d], [0xfd, 0x7f, 0x64]),
    ("Marsh", "#e86e1d", [0xf1, 0x91, 0x3f], [0xfd, 0xbd, 0x6b]),
    ("Thunder", "#e8c02f", [0xf1, 0xda, 0x54], [0xfd, 0xfa, 0x83]),
    ("Rising", "#71368a", [0x94, 0x57, 0xad], [0xbd, 0x7d, 0xd6]),
    ("Zephyr", "#029FFC", [0x3f, 0xc3, 0xfd], [0x86, 0xec, 0xfe]),
    ("Cascade", "#4fe2ca", [0x78, 0xed, 0xdf], [0xa7, 0xfb, 0xf9]),
    ("Earth", "#406601", [0x62, 0x88, 0x25], [0x89, 0xae, 0x4e]),
    ("Boulder", "#272727", [0x45, 0x45, 0x45], [0x67, 0x67, 0x67]),
];

fn blank() -> Value {
    json!({ "label": "", "emoji_name": null, "emoji_id": null })
}

/// What the form edits, out of what discord sends: a fixed five traits, ids as strings.
fn norm(p: &Value) -> Value {
    let mut traits: Vec<Value> = (0..5).map(|_| blank()).collect();
    for t in p["traits"].as_array().into_iter().flatten() {
        let i = t["position"].as_u64().unwrap_or(0) as usize;
        if i < 5 {
            traits[i] = json!({ "label": t["label"], "emoji_name": t["emoji_name"], "emoji_id": t["emoji_id"] });
        }
    }
    json!({
        "name": p["name"],
        "description": p["description"].as_str().unwrap_or_default(),
        "icon": p["icon_hash"].as_str().or(p["icon"].as_str()),
        "brand_color_primary": p["brand_color_primary"],
        "visibility": p["visibility"].as_u64().unwrap_or(1),
        "traits": traits,
        "game_application_ids": p["game_application_ids"].as_array().cloned().unwrap_or_default(),
    })
}

fn shape(mut c: serde_json::Map<String, Value>) -> Value {
    if let Some(t) = c.get("traits").and_then(|t| t.as_array()).cloned() {
        let list: Vec<Value> = t
            .iter()
            .enumerate()
            .filter(|(_, t)| t["label"].as_str().is_some_and(|l| !l.is_empty()))
            .map(|(i, t)| json!({ "label": t["label"], "position": i, "emoji_id": t["emoji_id"], "emoji_name": t["emoji_name"], "emoji_animated": false }))
            .collect();
        c.insert("traits".into(), json!(list));
    }
    Value::Object(c)
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let Some(raw) = app.server_data("profile", format!("/guilds/{gid}/profile")).cloned() else {
        title(ui, "Server Profile", "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages");
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let orig = norm(&raw);
    let mut d = form::take(app, "profile", &orig);
    ui.set_max_width(form_width(ui.ctx(), ui.available_width()));
    title(ui, "Server Profile", "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages");

    super::super::settings::h5(ui, "Name");
    let mut name = d["name"].as_str().unwrap_or_default().to_string();
    let r = super::super::settings::input(ui, &mut name, "", false);
    super::super::settings::mark(ui, "field:server_name", r.rect);
    d["name"] = json!(name);

    form::rule(ui);
    super::super::settings::h5(ui, "Icon");
    ui.label(RichText::new("We recommend an image of at least 512x512.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let mut pick = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        pick = form::button(ui, "Change Server Icon", true).clicked();
        if !d["icon"].is_null() && form::button(ui, "Remove Icon", false).clicked() {
            d["icon"] = Value::Null;
        }
    });
    if pick
        && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()
        && let Ok(bytes) = std::fs::read(&path)
    {
        let ctx = ui.ctx().clone();
        super::super::crop::open(app, bytes, 1.0, move |app: &mut App, uri: String| {
            use base64::Engine;
            if let Some(b) = uri.split_once(',').and_then(|(_, b)| base64::engine::general_purpose::STANDARD.decode(b).ok()) {
                ctx.data_mut(|m| m.insert_temp(Id::new("server_icon_bytes"), Arc::<[u8]>::from(b)));
            }
            if let Some(d) = app.server.as_mut().and_then(|s| s.draft.get_mut("profile")) {
                d["icon"] = json!(uri);
            }
        });
    }

    form::rule(ui);
    super::super::settings::h5(ui, "Banner");
    let cur = d["brand_color_primary"].as_str().map(str::to_lowercase);
    ui.spacing_mut().item_spacing = vec2(7.0, 7.0);
    let tiles: Vec<(&str, Option<&str>, [u8; 3], [u8; 3])> = std::iter::once(("Server Icon Color", None, [0x00, 0x00, 0x00], [0x44, 0x44, 0x44])).chain(BANNERS.iter().map(|(n, c, a, b)| (*n, Some(*c), *a, *b))).collect();
    let w = (ui.available_width() - 4.0 * 7.0) / 5.0;
    for row in tiles.chunks(5) {
        ui.horizontal(|ui| {
            for (name, color, top, bot) in row {
                let (r, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
                let on = color.map(str::to_lowercase) == cur;
                swatch(ui, r, *top, *bot);
                if on {
                    ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(6), Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
                }
                super::super::settings::mark(ui, &format!("banner:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    d["brand_color_primary"] = color.map_or(Value::Null, |c| json!(c));
                }
            }
        });
    }

    form::rule(ui);
    super::super::settings::h5(ui, "Traits");
    ui.label(RichText::new("Add up to 5 traits to show off your server\u{2019}s interests and personality.").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(12.0);
    let cw = (ui.available_width() - 2.0 * 8.0) / 3.0;
    let mut traits = d["traits"].as_array().cloned().unwrap_or_default();
    traits.resize_with(5, blank);
    for chunk in (0..5usize).collect::<Vec<_>>().chunks(3) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for &i in chunk {
                let t = &mut traits[i];
                let mut label = t["label"].as_str().unwrap_or_default().to_string();
                let cur: Pick = (t["emoji_name"].as_str().unwrap_or_default().to_string(), sid(&t["emoji_id"]), false);
                Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                    ui.set_width(cw - 18.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if let Some(p) = form::emoji_button(ui, app, gid, &cur) {
                            t["emoji_name"] = if p.0.is_empty() { Value::Null } else { json!(p.0) };
                            t["emoji_id"] = if p.1 == 0 { Value::Null } else { json!(p.1.to_string()) };
                        }
                        let r = ui.add(egui::TextEdit::singleline(&mut label).frame(Frame::NONE).margin(Margin::ZERO).char_limit(20).font(reg(16.0)).text_color(pal().text_normal).desired_width(ui.available_width()));
                        super::super::settings::mark(ui, &format!("trait:{}", i + 1), r.rect);
                    });
                });
                t["label"] = json!(label);
            }
        });
        ui.add_space(8.0);
    }
    d["traits"] = json!(traits);

    form::rule(ui);
    super::super::settings::h5(ui, "Description");
    ui.label(RichText::new("How did your server get started? Why should people join?").font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
    let mut desc = d["description"].as_str().unwrap_or_default().to_string();
    Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
        let r = ui.add(egui::TextEdit::multiline(&mut desc).frame(Frame::NONE).margin(Margin::ZERO).desired_rows(4).char_limit(300).font(reg(16.0)).text_color(pal().text_normal).hint_text(RichText::new("Tell the world a bit about this server.").color(pal().text_muted)).desired_width(ui.available_width()));
        super::super::settings::mark(ui, "field:description", r.rect);
    });
    d["description"] = json!(desc);

    form::rule(ui);
    games(app, ui, &raw, &mut d);

    form::rule(ui);
    let private = d["visibility"].as_u64() == Some(1);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        let text_w = (ui.available_width() - PRIVATE_CARD - 48.0 - 2.0 * 24.0 - 4.0).max(0.0);
        ui.vertical(|ui| {
            ui.set_width(text_w);
            ui.label(RichText::new("Private Profile").font(medium(16.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new("When enabled, only server members can view profile content. Non-members won't be able to see this content unless they have an invite.").font(reg(14.0)).color(pal().header_secondary));
        });
        let (r, resp) = ui.allocate_exact_size(vec2(48.0, 24.0), Sense::click());
        super::super::settings::mark(ui, "toggle:Private Profile", r);
        paint_switch(ui, r, private);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            d["visibility"] = json!(if private { 2 } else { 1 });
        }
        ui.add_space(4.0);
        private_card(ui);
    });

    preview(ui.ctx(), &g, &d, &raw);
    form::put(app, "profile", d);
    let ctx = ui.ctx().clone();
    form::bar(app, &ctx, "profile", "profile", &orig, format!("/guilds/{gid}/profile"), shape);
}

fn swatch(ui: &Ui, r: Rect, top: [u8; 3], bot: [u8; 3]) {
    let (t, b) = (Color32::from_rgb(top[0], top[1], top[2]), Color32::from_rgb(bot[0], bot[1], bot[2]));
    ui.painter().rect_filled(r, CornerRadius::same(4), t);
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), t);
    m.colored_vertex(r.right_top(), t);
    m.colored_vertex(r.right_bottom(), b);
    m.colored_vertex(r.left_bottom(), b);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
}

fn paint_switch(ui: &Ui, r: Rect, on: bool) {
    if on {
        ui.painter().rect_filled(r, CornerRadius::same(12), BLURPLE);
    } else {
        ui.painter().rect(r, CornerRadius::same(12), Color32::from_black_alpha(31), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    }
    let x = if on { r.right() - 13.0 } else { r.left() + 13.0 };
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
}

fn private_card(ui: &mut Ui) {
    Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51))).show(ui, |ui| {
        ui.set_width(PRIVATE_CARD);
        let (top, _) = ui.allocate_exact_size(vec2(PRIVATE_CARD, 80.0), Sense::hover());
        swatch(ui, top, [0x1c, 0x1c, 0x1f], [0x48, 0x48, 0x4c]);
        Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
            let ir = Rect::from_min_size(pos2(ui.max_rect().left(), top.bottom() - 32.0), vec2(64.0, 64.0));
            ui.painter().rect_filled(ir.expand(4.0), CornerRadius::same(20), pal().bg_secondary);
            ui.painter().rect_filled(ir, CornerRadius::same(16), pal().bg_accent);
            ui.painter().text(ir.center(), Align2::CENTER_CENTER, "?", bold(24.0), Color32::WHITE);
            ui.add_space(40.0);
            ui.label(RichText::new("Private Server").font(semi(20.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new("The server has limited who can see this profile.").font(reg(14.0)).color(pal().header_secondary));
        });
    });
}

fn games(app: &mut App, ui: &mut Ui, raw: &Value, d: &mut Value) {
    ui.label(RichText::new("Games").font(reg(20.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.label(RichText::new("What games does your server play?").font(reg(14.0)).color(pal().text_normal));
    ui.add_space(12.0);
    let known: Arc<Vec<(Snow, String, Option<String>)>> = ui.data_mut(|m| m.get_temp(Id::new("server_games"))).unwrap_or_else(|| {
        let list = detectable();
        ui.data_mut(|m| m.insert_temp(Id::new("server_games"), list.clone()));
        list
    });
    let mut ids: Vec<String> = d["game_application_ids"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let q = {
        let s = app.server.as_mut().unwrap();
        let r = super::super::settings::input(ui, &mut s.search, "Search for a game...", false);
        super::super::settings::mark(ui, "field:game", r.rect);
        s.search.trim().to_lowercase()
    };
    if !q.is_empty() {
        let hits: Vec<_> = known.iter().filter(|(id, n, _)| n.to_lowercase().contains(&q) && !ids.contains(&id.to_string())).take(8).collect();
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, faint(51))).corner_radius(8).inner_margin(Margin::same(4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if hits.is_empty() {
                ui.label(RichText::new("No games found.").font(reg(14.0)).color(pal().header_secondary));
            }
            for (id, n, icon) in hits {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
                if let Some(i) = icon {
                    super::super::text::image(ui.painter(), &format!("{}/app-icons/{id}/{i}.png?size=64", crate::api::cdn()), Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 12.0), vec2(24.0, 24.0)));
                }
                ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, n, medium(16.0), pal().text_normal);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    ids.push(id.to_string());
                    app.server.as_mut().unwrap().search.clear();
                }
            }
        });
    }
    if !ids.is_empty() {
        ui.add_space(8.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0, faint(51));
        ui.add_space(16.0);
        ui.label(RichText::new("Your Members Play").font(bold(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        let mut drop = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
            for id in &ids {
                let name = known.iter().find(|(k, _, _)| k.to_string() == *id).map(|(_, n, _)| n.clone()).or_else(|| raw["games"].as_array().into_iter().flatten().find(|g| g["id"].as_str() == Some(id)).and_then(|g| g["name"].as_str().map(str::to_string))).unwrap_or_default();
                let icon = known.iter().find(|(k, _, _)| k.to_string() == *id).and_then(|(_, _, i)| i.clone());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let (r, resp) = ui.allocate_exact_size(vec2(134.0, 134.0), Sense::click());
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_secondary);
                    if let Some(i) = icon {
                        super::super::text::image(ui.painter(), &format!("{}/app-icons/{id}/{i}.png?size=256", crate::api::cdn()), r);
                    }
                    if resp.hovered() {
                        let x = Rect::from_center_size(pos2(r.right() - 14.0, r.top() + 14.0), vec2(20.0, 20.0));
                        ui.painter().circle_filled(x.center(), 10.0, RED);
                        icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), Color32::WHITE);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if resp.clicked() {
                            drop = Some(id.clone());
                        }
                    }
                    ui.label(RichText::new(name).font(reg(12.0)).color(pal().header_secondary));
                });
            }
        });
        if let Some(id) = drop {
            ids.retain(|i| *i != id);
        }
    }
    d["game_application_ids"] = json!(ids);
}

/// The games discord says it can detect, from the list the game detector caches.
fn detectable() -> Arc<Vec<(Snow, String, Option<String>)>> {
    let list: Vec<(String, Snow, String, Option<String>)> = std::fs::read(crate::cache::dir().join("detectable.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<_> = list.into_iter().filter(|(_, id, _, _)| seen.insert(*id)).map(|(_, id, n, i)| (id, n, i)).collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    Arc::new(out)
}

const FORM_MAX: f32 = 560.0;
const PREVIEW: f32 = 300.0;
const PRIVATE_CARD: f32 = 263.0;
const GAP: f32 = 40.0;
// the preview column goes at this viewport width and below (@media (max-width: 1024px))
const PREVIEW_HIDDEN: f32 = 1024.0;

fn has_preview(ctx: &egui::Context) -> bool {
    ctx.content_rect().width() > PREVIEW_HIDDEN
}

/// The form column: it fills what the preview leaves, up to its max.
fn form_width(ctx: &egui::Context, row: f32) -> f32 {
    let taken = if has_preview(ctx) { PREVIEW + GAP } else { 0.0 };
    (row - taken).clamp(0.0, FORM_MAX)
}

/// The invite card, held in place at the right of the page while it scrolls.
fn preview(ctx: &egui::Context, g: &crate::state::Guild, d: &Value, raw: &Value) {
    let Some(area) = ctx.data(|m| m.get_temp::<Rect>(Id::new("server_area"))) else { return };
    let Some((left, right)) = ctx.data(|m| m.get_temp::<(f32, f32)>(Id::new("server_col"))) else { return };
    if !has_preview(ctx) {
        return;
    }
    let at = pos2(left + form_width(ctx, right - left) + GAP, area.top() + 44.0);
    let name = d["name"].as_str().unwrap_or_default();
    let color = d["brand_color_primary"].as_str().and_then(|c| BANNERS.iter().find(|b| b.1.eq_ignore_ascii_case(c))).map(|b| (b.2, b.3)).unwrap_or(([0x1c, 0x1c, 0x1f], [0x48, 0x48, 0x4c]));
    let bytes = ctx.data(|m| m.get_temp::<Arc<[u8]>>(Id::new("server_icon_bytes")));
    let picked = d["icon"].as_str().is_some_and(|i| i.starts_with("data:"));
    let shown = egui::Area::new(Id::new("server_preview")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51))).show(ui, |ui| {
            ui.set_width(PREVIEW);
            let (top, _) = ui.allocate_exact_size(vec2(PREVIEW, 120.0), Sense::hover());
            swatch(ui, top, color.0, color.1);
            Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                let ir = Rect::from_min_size(pos2(ui.max_rect().left(), top.bottom() - 40.0), vec2(80.0, 80.0));
                ui.painter().rect_filled(ir.expand(4.0), CornerRadius::same(24), pal().bg_secondary);
                let removed = d["icon"].is_null();
                match (picked, &bytes) {
                    (true, Some(b)) => {
                        egui::Image::from_bytes(format!("bytes://server-icon-{}", b.len()), b.to_vec()).corner_radius(CornerRadius::same(20)).paint_at(ui, ir);
                    }
                    _ => match g.icon_url().filter(|_| !removed) {
                        Some(u) => {
                            egui::Image::from_uri(u.replace("size=96", "size=160")).corner_radius(CornerRadius::same(20)).paint_at(ui, ir);
                        }
                        None => {
                            ui.painter().rect_filled(ir, CornerRadius::same(20), pal().bg_accent);
                            let acr: String = name.split_whitespace().filter_map(|w| w.chars().next()).collect();
                            ui.painter().text(ir.center(), Align2::CENTER_CENTER, acr, bold(24.0), Color32::WHITE);
                        }
                    },
                }
                ui.add_space(48.0);
                ui.label(RichText::new(name).font(bold(20.0)).color(pal().header_primary));
                ui.add_space(4.0);
                let online = raw["online_count"].as_u64().map(|n| n as usize).unwrap_or_else(|| g.list.iter().filter(|i| matches!(i, crate::state::ListItem::Member(_))).count());
                let members = raw["member_count"].as_u64().map(|n| n as u32).unwrap_or(g.member_count);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let dot = |ui: &mut Ui, c: Color32| {
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, c);
                    };
                    dot(ui, GREEN);
                    ui.label(RichText::new(format!("{online} Online")).font(reg(14.0)).color(pal().header_secondary));
                    dot(ui, GREY);
                    ui.label(RichText::new(format!("{members} Members")).font(reg(14.0)).color(pal().header_secondary));
                });
                let est = chrono::DateTime::from_timestamp_millis(snowflake_ms(g.id)).map(|t| t.format("Est. %b %Y").to_string()).unwrap_or_default();
                ui.label(RichText::new(est).font(reg(14.0)).color(pal().header_secondary));
                for t in d["traits"].as_array().into_iter().flatten().filter(|t| t["label"].as_str().is_some_and(|l| !l.is_empty())) {
                    ui.add_space(2.0);
                    ui.label(RichText::new(format!("{}  {}", t["emoji_name"].as_str().filter(|_| t["emoji_id"].is_null()).unwrap_or(""), t["label"].as_str().unwrap_or_default())).font(reg(14.0)).color(pal().text_normal));
                }
            });
        });
    });
    ctx.move_to_top(shown.response.layer_id);
}
