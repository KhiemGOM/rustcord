//! Discord's Server Boosts page, in the app rather than the browser: the three levels and the
//! perks boosts buy, which the server has, its boost count, and who's been boosting.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashSet;

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, sid, snowflake_ms};
use crate::theme::*;

// discord's own application for powerups; its store listings are the catalogue
const POWERUPS_APP: &str = "1340102344645283891";
const PINK: Color32 = Color32::from_rgb(0xff, 0x73, 0xfa);
// the badge packs card's own picture while none is on, from discord's client
const PACKS_ART: &str = "8dd1f75ba9ef2e0587c9963de0964edc7577f98385e85372e9dc600d353dd3b1";

pub struct Powerup {
    pub sku: Id,
    pub name: String,
    pub desc: String,
    pub cost: u32,
    pub perk: bool,
    pub image: Option<String>,
    // played while the card's hovered
    pub animated: Option<String>,
}

pub struct Sub {
    pub user: Id,
    pub ended: bool,
    // unix ms: when it ends (0 if it isn't going to), and when it was given
    pub ends: i64,
    pub at: i64,
}

#[derive(Default)]
pub struct Info {
    pub catalog: Vec<Powerup>,
    pub applied: HashSet<Id>,
    pub subs: Vec<Sub>,
    pub loaded: u8,
}

impl Info {
    /// Every level and perk together, which is what discord's boost goal counts towards.
    pub fn goal(&self) -> u32 {
        self.catalog.iter().map(|p| p.cost).sum()
    }
}

impl App {
    pub fn open_boosts(&mut self, gid: Id) {
        self.boosts = Some(gid);
        self.home = None;
        self.boost_info.insert(gid, Info::default());
        self.rest("GET", format!("/store/published-listings/skus?application_id={POWERUPS_APP}&guild_id={gid}"), None, Tag::Boosts(gid, 0));
        self.rest("GET", format!("/guilds/{gid}/powerups?include_ends_at=true"), None, Tag::Boosts(gid, 1));
        self.rest("GET", format!("/guilds/{gid}/premium/subscriptions?include_ended=true"), None, Tag::Boosts(gid, 2));
    }
}

pub fn load(app: &mut App, gid: Id, part: u8, v: &serde_json::Value) {
    let iso = |v: &serde_json::Value| v.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map_or(0, |t| t.timestamp_millis());
    let mut users = Vec::new();
    let Some(info) = app.boost_info.get_mut(&gid) else { return };
    let list = v.as_array().into_iter().flatten();
    match part {
        0 => {
            info.catalog = list
                .map(|l| {
                    let meta = &l["sku"]["tenant_metadata"]["guild_monetization"]["powerup"];
                    Powerup {
                        sku: sid(&l["sku"]["id"]),
                        name: l["sku"]["name"].as_str().unwrap_or_default().to_string(),
                        desc: l["description"].as_str().unwrap_or_default().to_string(),
                        cost: meta["boost_price"].as_u64().unwrap_or(0) as u32,
                        perk: meta["category_type"].as_str() == Some("perk"),
                        image: meta["static_image_url"].as_str().map(str::to_string),
                        animated: meta["animated_image_url"].as_str().map(str::to_string),
                    }
                })
                .collect();
            info.catalog.sort_by_key(|p| (p.perk, p.cost, p.sku));
        }
        1 => info.applied = list.filter(|e| e["deleted"].as_bool() != Some(true)).map(|e| sid(&e["sku_id"])).collect(),
        _ => {
            info.subs = list
                .map(|s| {
                    users.push(s["user"].clone());
                    Sub { user: sid(&s["user_id"]), ended: s["ended"].as_bool().unwrap_or(false), ends: iso(&s["ends_at"]), at: snowflake_ms(sid(&s["id"])) }
                })
                .collect();
        }
    }
    info.loaded |= 1 << part;
    for u in users {
        app.store.apply("USER_UPDATE", &u);
    }
}

// discord's perk lines under each level
const LEVEL_PERKS: [[(Icon, &str); 4]; 3] = [
    [(Icon::Smile, "100 Emoji Slots"), (Icon::Soundboard, "24 Soundboard Slots"), (Icon::Gif, "Animated Server Icon"), (Icon::Headphones, "Better Quality Audio")],
    [(Icon::ScreenArrow, "HD Streaming"), (Icon::Upload, "50MB File Uploads"), (Icon::UserCircle, "Custom Role Icons"), (Icon::Picture, "Server Banner")],
    [(Icon::Star, "Custom Server Invite Link"), (Icon::Upload, "100MB File Uploads"), (Icon::Headphones, "Highest Quality Audio"), (Icon::Gif, "Animated Server Banner")],
];

pub fn show(app: &mut App, ui: &mut Ui, gid: Id) {
    egui::Panel::top("boosts_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.left() - 16.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            let p = ui.painter();
            icons::draw(p, Icon::Gem, Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(20.0, 20.0)), pal().channel_default);
            p.text(pos2(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, "Server Boosts", medium(16.0), pal().header_primary);
        });
    let wide = ui.available_width() >= 720.0 + 375.0;
    if wide {
        egui::Panel::right("boosts_side")
            .exact_size(375.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_primary))
            .show(ui, |ui| side(app, ui, gid));
    }
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(("boosts", gid)).auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 24, right: 16, top: 25, bottom: 32 }).show(ui, |ui| main(app, ui, gid));
        });
    });
}

fn heading(ui: &mut Ui, title: &str, sub: &str) {
    ui.label(egui::RichText::new(title).font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    ui.add_space(2.0);
    ui.label(egui::RichText::new(sub).font(reg(14.0)).line_height(Some(18.0)).color(pal().text_muted));
    ui.add_space(16.0);
}

/// A card's frame, rounded 16: raised with a green edge once it's enabled, flat otherwise, and
/// lifted 4 with a shadow under the pointer.
fn card(ui: &Ui, r: Rect, on: bool, hot: bool) {
    let p = ui.painter();
    if hot {
        p.add(egui::Shadow { offset: [0, 12], blur: 24, spread: 0, color: Color32::from_black_alpha(61) }.as_shape(r, CornerRadius::same(16)));
    }
    p.rect_filled(r, CornerRadius::same(16), if on { pal().bg_secondary } else { pal().bg_primary });
    let edge = if on { Color32::from_rgba_unmultiplied(61, 158, 96, 89) } else { pal().border_subtle };
    p.rect_stroke(r, CornerRadius::same(16), Stroke::new(1.0, edge), egui::StrokeKind::Inside);
}

/// What a hovered card's foot turns into: Configure once it's on, else Enable and its price.
fn hover_button(ui: &Ui, r: Rect, on: bool, cost: &str) -> egui::Response {
    let b = Rect::from_min_size(pos2(r.left() + 17.0, r.bottom() - 57.0), vec2(r.width() - 34.0, 40.0));
    let resp = ui.interact(b, ui.id().with(("perk_btn", r.left() as i32, r.top() as i32)), Sense::click());
    super::settings::mark(ui, "perk_btn", b);
    let p = ui.painter();
    p.rect(b, 8, if resp.hovered() { Color32::from_rgb(0x47, 0x52, 0xc4) } else { BLURPLE }, Stroke::new(1.0, Color32::from_white_alpha(20)), egui::StrokeKind::Inside);
    if on {
        p.text(b.center(), Align2::CENTER_CENTER, "Configure", medium(16.0), Color32::WHITE);
    } else {
        let g = p.layout_no_wrap("Enable  \u{2022}  ".to_string(), medium(16.0), Color32::WHITE);
        let c = p.layout_no_wrap(cost.to_string(), medium(16.0), Color32::WHITE);
        let w = g.size().x + 18.0 + c.size().x;
        let x = b.center().x - w / 2.0;
        let y = b.center().y;
        p.galley(pos2(x, y - g.size().y / 2.0), g.clone(), Color32::WHITE);
        icons::draw(p, Icon::Gem, Rect::from_center_size(pos2(x + g.size().x + 8.0, y), vec2(16.0, 16.0)), Color32::WHITE);
        p.galley(pos2(x + g.size().x + 18.0, y - c.size().y / 2.0), c, Color32::WHITE);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// "N Boosts" on the left of a card's foot and, once it's on, a green tick and "Enabled".
fn foot(ui: &Ui, r: Rect, cost: &str, on: bool) {
    let p = ui.painter();
    let y = r.bottom() - 26.0;
    icons::draw(p, Icon::GemOutline, Rect::from_center_size(pos2(r.left() + 26.0, y), vec2(18.0, 18.0)), PINK);
    p.text(pos2(r.left() + 35.0, y), Align2::LEFT_CENTER, format!("{cost} Boosts"), semi(14.0), pal().header_secondary);
    if on {
        let t = p.text(pos2(r.right() - 17.0, y), Align2::RIGHT_CENTER, "Enabled", semi(14.0), pal().positive);
        icons::draw(p, Icon::CircleCheck, Rect::from_center_size(pos2(t.left() - 13.0, y), vec2(18.0, 18.0)), pal().positive);
    }
}

fn main(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(info) = app.boost_info.get(&gid) else { return };
    if info.loaded & 3 != 3 {
        ui.add_space(40.0);
        ui.centered_and_justified(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
        return;
    }
    let mut rects = Vec::new();
    heading(ui, "Levels", "Your server will automatically \u{201c}level up\u{201d} once it has enough available Boosts.");
    let levels: Vec<&Powerup> = info.catalog.iter().filter(|p| !p.perk).take(3).collect();
    let w = ((ui.available_width() - 32.0) / 3.0).min(220.0);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 296.0), Sense::hover());
    for (i, l) in levels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(row.left() + i as f32 * (w + 16.0), row.top()), vec2(w, 296.0));
        let on = info.applied.contains(&l.sku);
        card(ui, r, on, false);
        let p = ui.painter();
        // the level line runs through every card, the gem riding on it
        let y = r.top() + 36.0;
        let c = pos2(r.left() + 33.0, y);
        let line = if on { PINK } else { pal().bg_accent };
        if i > 0 {
            p.line_segment([pos2(r.left() + 1.0, y), c], Stroke::new(6.0, line));
        }
        p.line_segment([c, pos2(r.right() - 1.0, y)], Stroke::new(6.0, line));
        p.circle_filled(c, 16.0, if on { Color32::from_rgb(230, 123, 228) } else { pal().bg_accent });
        icons::draw(p, Icon::Gem, Rect::from_center_size(c, vec2(16.0, 16.0)), Color32::from_rgb(203, 203, 208));
        p.text(pos2(r.left() + 17.0, r.top() + 81.0), Align2::LEFT_CENTER, &l.name, bold(16.0), pal().text_normal);
        for (k, (icon, label)) in LEVEL_PERKS.get(i).into_iter().flatten().enumerate() {
            let ly = r.top() + 125.0 + k as f32 * 26.0;
            icons::draw(p, *icon, Rect::from_center_size(pos2(r.left() + 27.0, ly), vec2(18.0, 18.0)), pal().text_normal);
            p.text(pos2(r.left() + 43.0, ly), Align2::LEFT_CENTER, *label, medium(14.0), pal().text_normal);
        }
        p.text(pos2(r.left() + 17.0, r.top() + 229.0), Align2::LEFT_CENTER, "...and more!", medium(14.0), pal().text_normal);
        foot(ui, r, &l.cost.to_string(), on);
        rects.push((format!("boosts:level:{}", i + 1), r));
    }

    ui.add_space(48.0);
    heading(ui, "Additional Perks", "Individual perks that can be unlocked using available Boosts. No level required.");
    // the badge packs share one card, as discord shows them: the packs that are on, or what the
    // cheapest costs and discord's picture of them all
    let perks: Vec<&Powerup> = info.catalog.iter().filter(|p| p.perk).collect();
    let packs: Vec<&&Powerup> = perks.iter().filter(|p| p.name.ends_with("Badge Pack")).collect();
    let mut cards: Vec<(String, String, String, bool, Option<String>, Option<String>)> = perks
        .iter()
        .filter(|p| !p.name.ends_with("Badge Pack"))
        .map(|p| (p.name.clone(), p.desc.clone(), p.cost.to_string(), info.applied.contains(&p.sku), p.image.clone(), p.animated.clone()))
        .collect();
    if !packs.is_empty() {
        let on: Vec<&&&Powerup> = packs.iter().filter(|p| info.applied.contains(&p.sku)).collect();
        let cost = if on.is_empty() { format!("{}+", packs.iter().map(|p| p.cost).min().unwrap_or(0)) } else { on.iter().map(|p| p.cost).sum::<u32>().to_string() };
        let (image, animated) = match on.first() {
            Some(p) => (p.image.clone(), p.animated.clone()),
            None => (Some(format!("{}/assets/content/{PACKS_ART}.png", crate::api::cdn())), None),
        };
        let desc = "Stand out with special, themed badge packs for your Server Tag.".to_string();
        cards.insert(cards.len().min(1), ("Server Tag Badge Packs".into(), desc, cost, !on.is_empty(), image, animated));
    }
    // discord's card: 326 tall, the art 180 across the top (fitted, a shade darker at its foot),
    // then the name, what it does and the price
    let w = (ui.available_width() - 32.0) / 3.0;
    let mut pick = None;
    for chunk in cards.chunks(3) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 330.0), Sense::hover());
        for (i, (name, desc, cost, on, image, animated)) in chunk.iter().enumerate() {
            let slot = Rect::from_min_size(pos2(row.left() + i as f32 * (w + 16.0), row.top() + 4.0), vec2(w, 326.0));
            let hot = ui.rect_contains_pointer(slot);
            let r = slot.translate(vec2(0.0, if hot { -4.0 } else { 0.0 }));
            card(ui, r, *on, hot);
            let art = Rect::from_min_size(r.min + vec2(1.0, 1.0), vec2(r.width() - 2.0, 180.0));
            let fit = if name == "Enhanced Role Styles" { art.shrink2(vec2(16.0, 0.0)) } else { art };
            let shown = if hot { animated.as_ref().or(image.as_ref()) } else { image.as_ref() };
            if let Some(url) = shown {
                let u = crate::gif::uri(ui.ctx(), url, hot);
                egui::Image::from_uri(u).fit_to_exact_size(fit.size()).maintain_aspect_ratio(true).show_loading_spinner(false).paint_at(ui, fit);
            }
            let mut wash = egui::Mesh::default();
            let (top, bottom) = (Color32::TRANSPARENT, Color32::from_rgba_unmultiplied(15, 16, 17, 102));
            wash.colored_vertex(art.left_top(), top);
            wash.colored_vertex(art.right_top(), top);
            wash.colored_vertex(art.right_bottom(), bottom);
            wash.colored_vertex(art.left_bottom(), bottom);
            wash.add_triangle(0, 1, 2);
            wash.add_triangle(0, 2, 3);
            ui.painter().add(wash);
            let ink = if *on { pal().text_normal } else { pal().header_secondary };
            let p = ui.painter();
            p.text(pos2(r.left() + 17.0, r.top() + 207.0), Align2::LEFT_CENTER, name, bold(16.0), ink);
            let g = p.layout(desc.clone(), medium(14.0), ink, r.width() - 34.0);
            p.galley(pos2(r.left() + 17.0, r.top() + 223.0), g, ink);
            if hot {
                if hover_button(ui, r, *on, cost).clicked() {
                    pick = Some((name.clone(), *on));
                }
            } else {
                foot(ui, r, cost, *on);
            }
            rects.push((format!("boosts:perk:{name}"), r));
        }
        ui.add_space(12.0);
    }
    app.rects.extend(rects);
    match pick {
        // configuring is the server settings page the perk lives on; turning one on spends the
        // server's boosts, which stays on discord.com with the rest of the boost checkout
        Some((name, true)) => {
            app.open_server_settings(gid);
            if let Some(s) = &mut app.server {
                s.page = if name == "Enhanced Role Styles" { super::server::Page::Roles } else { super::server::Page::Tag };
            }
        }
        Some((_, false)) => ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{gid}/boosts"))),
        None => {}
    }
}

fn side(app: &mut App, ui: &mut Ui, gid: Id) {
    let boosts = app.store.guilds.get(&gid).map_or(0, |g| g.boosts);
    let Some(info) = app.boost_info.get(&gid) else { return };
    let spent: u32 = info.catalog.iter().filter(|p| info.applied.contains(&p.sku)).map(|p| p.cost).sum();
    let total = if info.loaded & 4 != 0 { info.subs.iter().filter(|s| !s.ended).count() as u32 } else { boosts };
    let mut boost = false;
    egui::ScrollArea::vertical().id_salt(("boosts_side", gid)).auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(Margin { left: 16, right: 16, top: 25, bottom: 24 }).show(ui, |ui| {
            let w = ui.available_width();
            let (r, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
            let p = ui.painter();
            p.text(r.left_center(), Align2::LEFT_CENTER, "SERVER BOOSTS", bold(12.0), pal().header_secondary);
            let lr = p.text(r.right_center(), Align2::RIGHT_CENTER, "Learn More", semi(12.0), pal().link);
            let learn = ui.interact(lr, ui.id().with("boosts_learn"), Sense::click());
            if learn.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/articles/360028038352"));
            }
            ui.add_space(8.0);
            let tw = (w - 16.0) / 3.0;
            let (row, _) = ui.allocate_exact_size(vec2(w, 75.0), Sense::hover());
            for (i, (n, label)) in [(total.saturating_sub(spent), "Available"), (spent, "Spent"), (total, "Total")].into_iter().enumerate() {
                let t = Rect::from_min_size(pos2(row.left() + i as f32 * (tw + 8.0), row.top()), vec2(tw, 75.0));
                let p = ui.painter();
                p.rect_filled(t, CornerRadius::same(8), pal().bg_secondary);
                p.rect_stroke(t, CornerRadius::same(8), Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
                let ink = if i == 0 { pal().header_primary } else { pal().text_muted };
                let g = p.layout_no_wrap(n.to_string(), semi(20.0), ink);
                let x = t.center().x - (g.size().x + 18.0) / 2.0;
                icons::draw(p, Icon::Gem, Rect::from_center_size(pos2(x + 8.0, t.top() + 29.0), vec2(16.0, 16.0)), if i == 0 { PINK } else { pal().text_muted });
                p.galley(pos2(x + 18.0, t.top() + 29.0 - g.size().y / 2.0), g, ink);
                p.text(pos2(t.center().x, t.top() + 54.0), Align2::CENTER_CENTER, label, medium(14.0), pal().text_muted);
            }
            ui.add_space(8.0);
            let (b, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(b, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { Color32::from_rgba_unmultiplied(151, 151, 159, 20) });
            p.rect_stroke(b, CornerRadius::same(8), Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
            let g = p.layout_no_wrap("Boost This Server".into(), medium(14.0), pal().text_normal);
            let x = b.center().x - (g.size().x + 20.0) / 2.0;
            icons::draw(p, Icon::Gem, Rect::from_center_size(pos2(x + 8.0, b.center().y), vec2(16.0, 16.0)), PINK);
            p.galley(pos2(x + 20.0, b.center().y - g.size().y / 2.0), g, pal().text_normal);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            boost = resp.clicked();

            ui.add_space(24.0);
            let (r, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
            ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "RECENT ACTIVITY", bold(12.0), pal().header_secondary);
            ui.add_space(8.0);
            for (when, who, what, icon, color) in activity(info).into_iter().take(40) {
                let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
                let p = ui.painter();
                icons::draw(p, icon, Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(16.0, 16.0)), color);
                let date = day(when);
                let d = p.text(pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, date, reg(12.0), pal().channel_muted);
                let ink = app.store.role_color(gid, who).map(role_color).unwrap_or(pal().header_primary);
                let name = app.store.name_in(gid, who);
                let n = super::text::paint_em(p, pos2(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, &name, medium(16.0), ink, (d.left() - r.left() - 120.0).max(40.0), 1.0);
                p.text(pos2(n.right() + 4.0, r.center().y), Align2::LEFT_CENTER, what, reg(16.0), pal().text_muted);
            }
        });
    });
    if boost {
        // buying or moving a boost is discord's checkout, which stays on the web
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{gid}/boosts")));
    }
}

/// Boosts given, about to run out, and run out, newest first. Discord dates a coming expiry
/// three days ahead of it.
fn activity(info: &Info) -> Vec<(i64, Id, String, Icon, Color32)> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut out = Vec::new();
    for s in &info.subs {
        out.push((s.at, s.user, "gave 1 boost.".to_string(), Icon::GemOutline, pal().text_muted));
        if s.ends != 0 && s.ended {
            out.push((s.ends, s.user, "1 boost expired.".to_string(), Icon::GemSlash, pal().critical));
        } else if s.ends != 0 && s.ends - 3 * 86_400_000 <= now {
            let on = chrono::DateTime::from_timestamp_millis(s.ends).map(|t| t.with_timezone(&chrono::Local).format("%-m/%-d").to_string()).unwrap_or_default();
            out.push((s.ends - 3 * 86_400_000, s.user, format!("boost expiring {on}."), Icon::GemSlash, pal().mention_bar));
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.0));
    out
}

fn day(ms: i64) -> String {
    let Some(t) = chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&chrono::Local)) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    if t.date_naive() == today {
        "Today".into()
    } else if t.date_naive().succ_opt() == Some(today) {
        "Yesterday".into()
    } else {
        t.format("%-m/%-d/%y").to_string()
    }
}
