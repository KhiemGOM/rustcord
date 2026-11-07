use chrono::Timelike;
use egui::{Align2, Color32, CornerRadius, Frame, Painter, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::{NIGHT, PLATE, RADIUS_LG, SPACE_12, SPACE_16, SPACE_20, SPACE_24, SPACE_32, SPACE_8};
use super::{button, get, list, round_button, shade, text_width, want};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{Page as Settings, billing};

const COLUMN_MAX: f32 = 1200.0;
const COLUMN_INSET: f32 = 64.0;
const GRID_GAP: f32 = SPACE_24;
const TWO_COLUMNS: f32 = 720.0;
const THREE_COLUMNS: f32 = 960.0;
const CARD_HEIGHT: f32 = 380.0;
const CARD_PAD: f32 = SPACE_20;
const ART_MAX: f32 = 180.0;
const CTA_HEIGHT: f32 = 36.0;
const SECTION_GAP: f32 = 88.0;
const PERK_WIDTH: f32 = 280.0;
const TAB_HEIGHT: f32 = 32.0;
const SPACE_64: f32 = 64.0;
const BACKDROP_HEIGHT: f32 = 984.0;

const TENURE: [(f64, &str); 5] = [(1.0, "Bronze"), (3.0, "Silver"), (6.0, "Gold"), (12.0, "Platinum"), (24.0, "Diamond")];

type Perk = (&'static str, &'static str, Option<Settings>);

const FAVORITES: [(&str, [Perk; 5]); 4] = [
    (
        "Best of Nitro",
        [
            ("2 Server Boosts + 30% off", "Support the servers you love with 2 included Boosts and 30% off when you buy more.", Some(Settings::Boost)),
            ("Customize your profile", "Explore all the ways to personalize your profile including animated avatars, color themes, and per-server profiles.", Some(Settings::Profiles)),
            ("HD streaming", "Share every clutch moment in up to 4K at 60fps.", None),
            ("Custom emojis anywhere", "Hype, roast, and meme with custom emojis anywhere.", None),
            ("Larger file uploads", "Send files up to 1GB. Perfect for game clips, art, and more.", None),
        ],
    ),
    (
        "Appearance & Style",
        [
            ("Display Name Styles", "Mix and match styles to create an unforgettable display name.", Some(Settings::Profiles)),
            ("Animated avatar, banner, and profile theme", "Make your profile move with an animated avatar and banner.", Some(Settings::Profiles)),
            ("Colors for your Discord theme", "Create your perfect theme, or choose from 20+ themes like Cotton Candy, Midnight Blurple, and more.", Some(Settings::Appearance)),
            ("Custom per-server profiles", "Show a different avatar, banner and bio in each server.", Some(Settings::Profiles)),
            ("Custom App Icons", "Pick from a range of app icons for Discord.", Some(Settings::Appearance)),
        ],
    ),
    (
        "Discord Upgrades",
        [
            ("Larger file uploads", "Send files up to 1GB. Perfect for game clips, art, and more.", None),
            ("HD streaming", "Share every clutch moment in up to 4K at 60fps.", None),
            ("Longer messages", "Say more with messages up to 4000 characters.", None),
            ("Join up to 200 servers", "Keep every community you love in one place.", None),
            ("Super Reactions", "Make your reactions stand out with animated effects.", None),
        ],
    ),
    (
        "VIP Extras",
        [
            ("Shop member pricing", "Get member pricing and exclusive items in the Shop.", None),
            ("Custom sounds anywhere", "Use your soundboard sounds in any server.", None),
            ("Custom entrance sounds", "Walk into a voice channel with a sound of your own.", None),
            ("3 Nitro friend passes", "Share a week of Nitro with friends.", None),
            ("Keep Quest rewards longer", "Keep Quest avatar decoration rewards for longer.", Some(Settings::Profiles)),
        ],
    ),
];

enum Art {
    Medal(Color32),
    Orbs,
    Upload,
    Quests,
    Name,
    Themes,
    Nitro,
}

#[derive(Clone)]
enum Act {
    Subscribe(String),
    Gift,
    Settings(Settings),
    Quests,
    Shop,
    Wishlist,
    Perks(u8),
}

struct Card {
    art: Art,
    tag: Option<&'static str>,
    eyebrow: Option<String>,
    title: String,
    body: String,
    button: Option<&'static str>,
    act: Option<Act>,
}

fn card_of(art: Art, tag: Option<&'static str>, eyebrow: Option<String>, title: impl Into<String>, body: impl Into<String>, button: Option<&'static str>, act: Option<Act>) -> Card {
    Card { art, tag, eyebrow, title: title.into(), body: body.into(), button, act }
}

fn glow(p: &Painter, c: Pos2, rad: f32, col: Color32, alpha: u8) {
    const SEGMENTS: usize = 48;
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha));
    for i in 0..SEGMENTS {
        let a = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        m.colored_vertex(c + vec2(a.cos(), a.sin()) * rad, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 0));
        m.add_triangle(0, 1 + i as u32, 1 + ((i + 1) % SEGMENTS) as u32);
    }
    p.add(egui::Shape::mesh(m));
}

fn art(p: &Painter, r: Rect, kind: &Art) {
    let c = r.center();
    let k = r.width().min(r.height());
    let icon = Rect::from_center_size(c, vec2(k * 0.5, k * 0.5));
    match kind {
        Art::Medal(gold) => {
            glow(p, c, k * 0.6, *gold, 90);
            p.circle_filled(c, k * 0.36, gold.gamma_multiply(0.9));
            p.circle_filled(c, k * 0.28, Color32::from_rgb(0x3a, 0x22, 0x08));
            icons::draw(p, Icon::Nitro, Rect::from_center_size(c, vec2(k * 0.3, k * 0.3)), *gold);
        }
        Art::Orbs => {
            glow(p, c, k * 0.65, Color32::from_rgb(0xc0, 0x3a, 0xe8), 110);
            p.circle_filled(c, k * 0.28, Color32::from_rgb(0x58, 0x65, 0xf2));
            icons::draw(p, Icon::Gem, Rect::from_center_size(c, vec2(k * 0.35, k * 0.35)), Color32::WHITE);
        }
        Art::Upload => {
            glow(p, c, k * 0.65, Color32::from_rgb(0x58, 0x65, 0xf2), 110);
            icons::draw(p, Icon::Upload, icon, Color32::from_rgb(0xb8, 0xc0, 0xff));
        }
        Art::Quests => {
            glow(p, c, k * 0.65, Color32::from_rgb(0xeb, 0x45, 0x9e), 100);
            icons::draw(p, Icon::Quests, icon, Color32::from_rgb(0xff, 0xc2, 0xe6));
        }
        Art::Name => {
            glow(p, c, k * 0.65, Color32::from_rgb(0x9b, 0x4d, 0xff), 100);
            p.text(c, Align2::CENTER_CENTER, "Aa", bold_italic(k * 0.35), Color32::from_rgb(0xe8, 0xd5, 0xff));
        }
        Art::Themes => {
            for (i, col) in [Color32::from_rgb(0xff, 0x9a, 0xd5), Color32::from_rgb(0x58, 0x65, 0xf2), Color32::from_rgb(0x4b, 0xd6, 0xc8)].into_iter().enumerate() {
                p.circle_filled(c + vec2((i as f32 - 1.0) * k * 0.28, 0.0), k * 0.22, col);
            }
        }
        Art::Nitro => {
            glow(p, c, k * 0.65, Color32::from_rgb(0xc0, 0x3a, 0xe8), 110);
            icons::draw(p, Icon::Nitro, icon, Color32::WHITE);
        }
    }
}

fn card(ui: &mut Ui, r: Rect, c: &Card, key: &str) -> bool {
    let hover = ui.rect_contains_pointer(r);
    crate::ui::settings::mark(ui, &format!("shop:nitro:{}", c.title), r);
    let p = ui.painter().clone();
    p.rect(r, CornerRadius::same(RADIUS_LG), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
    let inner = Rect::from_min_max(r.min + vec2(CARD_PAD, CARD_PAD), r.max - vec2(CARD_PAD, CARD_PAD));
    let cta = c.button.filter(|_| hover);
    let mut bottom = inner.bottom();
    if cta.is_some() {
        bottom -= CTA_HEIGHT + SPACE_16;
    }
    let width = inner.width();
    let body = p.layout(c.body.clone(), reg(14.0), pal().header_secondary, width);
    let shown = if hover { body.size().y } else { 20.0_f32.min(body.size().y) };
    let mut y = bottom - shown;
    let clip = p.with_clip_rect(Rect::from_min_size(pos2(inner.left(), y), vec2(width, shown)));
    clip.galley(pos2(inner.left(), y), body, pal().header_secondary);
    y -= SPACE_8 / 2.0 + 24.0;
    p.text(pos2(inner.left(), y), Align2::LEFT_TOP, &c.title, semi(20.0), pal().header_primary);
    if let Some(e) = &c.eyebrow {
        y -= 22.0;
        p.text(pos2(inner.left(), y), Align2::LEFT_TOP, e, reg(14.0), pal().text_muted);
    }
    let art_area = Rect::from_min_max(r.min, pos2(r.right(), y - SPACE_16));
    let side = art_area.height().min(art_area.width()).min(ART_MAX * 1.3);
    art(&p.with_clip_rect(r), Rect::from_center_size(pos2(art_area.center().x, art_area.center().y), vec2(side, side)), &c.art);
    if let Some(t) = c.tag {
        let g = p.layout_no_wrap(t.to_string(), bold(12.0), Color32::from_rgb(0x2f, 0x30, 0x35));
        let pill = Rect::from_min_size(r.min + vec2(SPACE_24, SPACE_24), vec2(g.size().x + SPACE_16, 20.0));
        p.rect_filled(pill, CornerRadius::same(10), Color32::WHITE);
        p.galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
    }
    match cta {
        Some(label) => {
            let b = Rect::from_min_size(pos2(inner.left(), inner.bottom() - CTA_HEIGHT), vec2(text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16, CTA_HEIGHT));
            button(ui, b, label, Color32::from_rgba_unmultiplied(151, 151, 159, 40), Color32::WHITE, key)
        }
        None => false,
    }
}

fn months_since(created: &str) -> f64 {
    (billing::now() - billing::epoch(created)).max(0) as f64 / (30.44 * 86_400.0)
}

fn tenure(created: &str) -> (Color32, String, String) {
    let m = months_since(created);
    let at = TENURE.iter().rposition(|(t, _)| m >= *t);
    let tints = [Color32::from_rgb(0xcd, 0x7f, 0x32), Color32::from_rgb(0xc0, 0xc6, 0xd0), Color32::from_rgb(0xf0, 0xa5, 0x1c), Color32::from_rgb(0xb7, 0xe4, 0xff), Color32::from_rgb(0x8f, 0xb4, 0xff)];
    let (tint, name) = at.map_or((tints[0], "Nitro"), |i| (tints[i], TENURE[i].1));
    let next = match at {
        Some(i) if i + 1 < TENURE.len() => format!("{} months to Nitro {}", (TENURE[i + 1].0 - m).ceil() as i64, TENURE[i + 1].1),
        Some(_) => "You're at the top tier".to_string(),
        None => format!("{} months to Nitro {}", (TENURE[0].0 - m).ceil().max(1.0) as i64, TENURE[0].1),
    };
    (tint, format!("Nitro {name}"), next)
}

fn short_date(s: &str) -> String {
    use chrono::Datelike;
    chrono::DateTime::parse_from_rfc3339(s).map_or_else(|_| billing::mdy(s), |t| format!("{}/{}/{}", t.month(), t.day(), t.year() % 100))
}

fn greeting() -> &'static str {
    match chrono::Local::now().hour() {
        0..=11 => "GOOD MORNING,",
        12..=17 => "GOOD AFTERNOON,",
        _ => "GOOD EVENING,",
    }
}

fn monthly(plans: &[serde_json::Value]) -> Option<&serde_json::Value> {
    plans.iter().find(|p| p["interval"] == 1 && p["interval_count"] == 1)
}

fn price(plan: Option<&serde_json::Value>) -> String {
    plan.map_or("—".into(), |p| format!("{}/month", billing::money(p["price"].as_i64().unwrap_or(0), p["currency"].as_str().unwrap_or("usd"))))
}

fn hero_art(p: &Painter, r: Rect) {
    let k = r.width().min(r.height() * 1.3);
    let c = pos2(r.right() - k * 0.4, r.top() + k * 0.4);
    glow(p, c, k * 0.42, Color32::from_rgb(0x58, 0x65, 0xf2), 70);
    glow(p, c + vec2(-k * 0.34, k * 0.26), k * 0.24, Color32::from_rgb(0xeb, 0x45, 0x9e), 70);
    icons::draw(p, Icon::Nitro, Rect::from_center_size(c, vec2(k * 0.25, k * 0.25)), Color32::from_rgb(0xdd, 0xe3, 0xff));
    for (dx, dy, s, col) in [(-0.27, -0.29, 0.075, 0xc2a0ff_u32), (0.31, -0.31, 0.04, 0x8aa0ff), (0.4, 0.04, 0.06, 0xff7ad9), (-0.4, 0.15, 0.045, 0xc2a0ff)] {
        icons::draw(p, Icon::Star, Rect::from_center_size(c + vec2(dx * k, dy * k), vec2(s * k, s * k)), Color32::from_rgb((col >> 16) as u8, (col >> 8) as u8, col as u8));
    }
}

fn perk_height(ui: &Ui, perks: &[Perk]) -> f32 {
    perks
        .iter()
        .map(|(_, body, go)| {
            let g = ui.painter().layout(body.to_string(), reg(14.0), Color32::WHITE, PERK_WIDTH - 2.0 * CARD_PAD);
            2.0 * CARD_PAD + 24.0 + SPACE_8 + g.size().y + if go.is_some() { SPACE_16 + CTA_HEIGHT } else { 0.0 }
        })
        .fold(0.0, f32::max)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    want(app, "nitro_subs", "/users/@me/billing/subscriptions?sync_level=1");
    want(app, "nitro_plans", &format!("/store/published-listings/skus/{}/subscription-plans?revenue_surface=0", billing::NITRO_SKU));
    want(app, "nitro_basic", &format!("/store/published-listings/skus/{}/subscription-plans?revenue_surface=0", billing::BASIC_SKU));
    let tier = app.store.me_raw["premium_type"].as_u64().unwrap_or(0);
    let (name, avatar) = app.store.user(app.store.me).map(|u| (u.display().to_string(), u.avatar_url())).unwrap_or_default();
    let nitro_plans = list(get(app, "nitro_plans")).to_vec();
    let basic_plans = list(get(app, "nitro_basic")).to_vec();
    let sub = list(get(app, "nitro_subs")).iter().find(|s| s["type"] == 1 && matches!(s["status"].as_u64(), Some(1..=3))).cloned();
    let since = sub.as_ref().and_then(|s| s["created_at"].as_str()).map(str::to_string);
    let tab = app.shop.as_ref().map_or(0, |s| s.perks);
    let mut acts: Vec<Act> = Vec::new();
    egui::CentralPanel::default().frame(Frame::NONE.fill(NIGHT)).show(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width();
            let top = ui.cursor().min;
            let cw = (w - COLUMN_INSET).clamp(0.0, COLUMN_MAX);
            let left = top.x + (w - cw) / 2.0;
            let p = ui.painter().clone();
            let band = p.with_clip_rect(Rect::from_min_size(top, vec2(w, BACKDROP_HEIGHT)));
            shade(ui, Rect::from_min_size(top, vec2(w, BACKDROP_HEIGHT)), Color32::from_rgb(0x34, 0x16, 0x6a), 200, 0);
            glow(&band, pos2(top.x, top.y), w * 0.3, Color32::from_rgb(0xc0, 0x2a, 0xc0), 90);
            if cw >= TWO_COLUMNS {
                hero_art(&band, Rect::from_min_size(pos2(left, top.y), vec2(cw, SPACE_64 * 7.0)));
            }
            let logo = Rect::from_min_size(pos2(left, top.y + SPACE_8), vec2(24.0, 24.0));
            icons::draw(&p, Icon::Nitro, logo, Color32::WHITE);
            p.text(pos2(logo.right() + SPACE_8, logo.center().y), Align2::LEFT_CENTER, "NITRO", bold_italic(20.0), Color32::WHITE);
            let gift_w = text_width(ui, "Gift Nitro", medium(14.0)) + 2.0 * SPACE_16 + 16.0;
            let gift = Rect::from_min_size(pos2(left + cw - gift_w, top.y + SPACE_8 / 2.0), vec2(gift_w, 32.0));
            let heart = Rect::from_min_size(pos2(gift.left() - SPACE_8 - 32.0, gift.top()), vec2(32.0, 32.0));
            if round_button(ui, heart, Icon::Heart, Color32::WHITE, "wishlist") {
                acts.push(Act::Wishlist);
            }
            let hover = ui.interact(gift, ui.id().with("nitro_gift_top"), Sense::click());
            p.rect_filled(gift, CornerRadius::same(RADIUS_LG), Color32::from_black_alpha(if hover.hovered() { 170 } else { 143 }));
            icons::draw(&p, Icon::Gift, Rect::from_center_size(pos2(gift.left() + SPACE_16 + 2.0, gift.center().y), vec2(16.0, 16.0)), Color32::WHITE);
            p.text(pos2(gift.left() + SPACE_16 + 16.0 + SPACE_8, gift.center().y), Align2::LEFT_CENTER, "Gift Nitro", medium(14.0), Color32::WHITE);
            crate::ui::settings::mark(ui, "shop:gift_top", gift);
            if hover.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hover.clicked() {
                acts.push(Act::Gift);
            }

            let mut y = top.y + SPACE_64 + SPACE_32;
            p.text(pos2(left, y), Align2::LEFT_TOP, greeting(), bold_italic(32.0), Color32::WHITE);
            y += 32.0 + SPACE_16;
            let face = Rect::from_min_size(pos2(left, y), vec2(SPACE_64, SPACE_64));
            crate::ui::avatar(ui, face, &avatar);
            p.text(pos2(face.right() + SPACE_24, face.center().y), Align2::LEFT_CENTER, &name, reg(44.0), Color32::from_gray(0xe6));
            y += SPACE_64 + SPACE_64;

            let cols = if cw >= THREE_COLUMNS { 3 } else if cw >= TWO_COLUMNS { 2 } else { 1 };
            let cell = (cw - (cols - 1) as f32 * GRID_GAP) / cols as f32;
            let mut cards: Vec<Card> = Vec::new();
            let heading = if tier == 0 || tier == 1 {
                let (nitro, basic) = (monthly(&nitro_plans), monthly(&basic_plans));
                let id = |p: Option<&serde_json::Value>| p.and_then(|p| p["id"].as_str()).unwrap_or_default().to_string();
                cards.push(card_of(Art::Nitro, Some("Most popular"), Some(price(nitro)), "Nitro", "1GB uploads, HD streaming, 2 Server Boosts, custom profiles, Display Name Styles and everything in Nitro Basic.", Some("Subscribe"), Some(Act::Subscribe(id(nitro)))));
                cards.push(card_of(Art::Upload, None, Some(price(basic)), "Nitro Basic", "50MB uploads, custom emoji and stickers anywhere, and unlimited Super Reactions.", Some("Subscribe"), Some(Act::Subscribe(id(basic)))));
                cards.push(card_of(Art::Orbs, None, None, "Orbs on repeat", "Nitro members earn Orbs every month to spend in the Shop.", Some("Open the Shop"), Some(Act::Shop)));
                "CHOOSE YOUR PLAN"
            } else {
                let (tint, level, next) = since.as_deref().map_or((Color32::from_rgb(0xf0, 0xa5, 0x1c), "Nitro".to_string(), String::new()), tenure);
                let from = since.as_deref().map(|s| format!("Subscriber since {}", short_date(s)));
                let (upload, limit) = if tier == 3 { ("50MB Uploads", "Your upload limit just jumped to 50MB.") } else { ("1GB Uploads", "Your upload limit just jumped to 1GB.") };
                cards.push(card_of(Art::Medal(tint), Some("Reward"), from, level, next, Some("Take me there"), Some(Act::Settings(Settings::Nitro))));
                cards.push(card_of(Art::Orbs, None, None, "Orbs on repeat", "Enjoy 250 Orbs every month, just for having Nitro.", Some("Spend Orbs"), Some(Act::Shop)));
                cards.push(card_of(Art::Upload, None, None, upload, limit, None, None));
                cards.push(card_of(Art::Quests, None, None, "Win more Orbs with Nitro", "Now your Quest wins become mega wins with 1.2x rewards when you subscribe to Nitro. Cancel anytime", Some("Take me there"), Some(Act::Quests)));
                cards.push(card_of(Art::Name, None, None, "Display Name Styles", "Mix and match styles to create an unforgettable display name.", Some("Take me there"), Some(Act::Settings(Settings::Profiles))));
                cards.push(card_of(Art::Themes, None, None, "Color Themes", "Create your perfect theme, or choose from 20+ themes like Cotton Candy, Midnight Blurple, and more.", Some("Take me there"), Some(Act::Settings(Settings::Appearance))));
                "EXPLORE WHAT'S NEW"
            };
            p.text(pos2(left, y), Align2::LEFT_TOP, heading, bold_italic(20.0), Color32::from_gray(0xe6));
            y += 24.0 + SPACE_24;
            for row in cards.chunks(cols) {
                for (i, c) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(left + i as f32 * (cell + GRID_GAP), y), vec2(cell, CARD_HEIGHT));
                    if card(ui, r, c, &format!("card:{}", c.title))
                        && let Some(a) = &c.act
                    {
                        acts.push(a.clone());
                    }
                }
                y += CARD_HEIGHT + GRID_GAP;
            }

            y += SECTION_GAP - GRID_GAP;
            p.text(pos2(left, y), Align2::LEFT_TOP, "CHECK OUT THESE NITRO FAVORITES", bold_italic(20.0), Color32::from_gray(0xe6));
            y += 24.0 + SPACE_32;
            let mut x = left;
            for (i, (name, _)) in FAVORITES.iter().enumerate() {
                let r = Rect::from_min_size(pos2(x, y), vec2(text_width(ui, name, medium(14.0)) + 2.0 * SPACE_12, TAB_HEIGHT));
                let resp = ui.interact(r, ui.id().with(("nitro_tab", i)), Sense::click());
                let on = tab as usize == i;
                let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 60 } else { 40 });
                p.rect(r, CornerRadius::same(TAB_HEIGHT as u8), fill, Stroke::new(1.0, if on { BLURPLE } else { Color32::TRANSPARENT }), egui::StrokeKind::Inside);
                p.text(r.center(), Align2::CENTER_CENTER, *name, medium(14.0), Color32::WHITE);
                crate::ui::settings::mark(ui, &format!("shop:perks:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    acts.push(Act::Perks(i as u8));
                }
                x = r.right() + SPACE_8;
                if x + 80.0 > left + cw && i + 1 < FAVORITES.len() {
                    x = left;
                    y += TAB_HEIGHT + SPACE_8;
                }
            }
            y += TAB_HEIGHT + SPACE_20 + SPACE_16;
            let perks = &FAVORITES[(tab as usize).min(FAVORITES.len() - 1)].1;
            let ph = perk_height(ui, perks);
            let strip = Rect::from_min_size(pos2(left, y), vec2(cw, ph));
            ui.scope_builder(UiBuilder::new().max_rect(strip), |ui| {
                ScrollArea::horizontal().id_salt(("nitro_perks", tab)).auto_shrink([false, false]).show(ui, |ui| {
                    let origin = ui.cursor().min;
                    for (i, (title, body, go)) in perks.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(origin.x + i as f32 * (PERK_WIDTH + GRID_GAP), origin.y), vec2(PERK_WIDTH, ph));
                        let pp = ui.painter().clone();
                        pp.rect(r, CornerRadius::same(RADIUS_LG), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
                        pp.text(r.min + vec2(CARD_PAD, CARD_PAD), Align2::LEFT_TOP, *title, semi(20.0), pal().header_primary);
                        let g = pp.layout(body.to_string(), reg(14.0), pal().header_secondary, PERK_WIDTH - 2.0 * CARD_PAD);
                        pp.galley(r.min + vec2(CARD_PAD, CARD_PAD + 24.0 + SPACE_8), g, pal().header_secondary);
                        if let Some(s) = go {
                            let label = "Take me there";
                            let b = Rect::from_min_size(pos2(r.left() + CARD_PAD, r.bottom() - CARD_PAD - CTA_HEIGHT), vec2(text_width(ui, label, medium(16.0)) + 2.0 * SPACE_16, CTA_HEIGHT));
                            if button(ui, b, label, Color32::from_rgba_unmultiplied(151, 151, 159, 40), Color32::WHITE, &format!("perk:{title}")) {
                                acts.push(Act::Settings(*s));
                            }
                        }
                    }
                    ui.allocate_exact_size(vec2(perks.len() as f32 * (PERK_WIDTH + GRID_GAP) - GRID_GAP, ph), Sense::hover());
                });
            });
            y += ph + SECTION_GAP;

            let gift_h = SPACE_64 * 3.0 + SPACE_20;
            let box_ = Rect::from_min_size(pos2(left, y), vec2(cw, gift_h));
            p.rect(box_, CornerRadius::same(RADIUS_LG + 8), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
            let art_c = pos2(box_.right() - gift_h * 0.55, box_.center().y);
            glow(&p.with_clip_rect(box_), art_c, gift_h * 0.62, Color32::from_rgb(0xeb, 0x45, 0x9e), 80);
            icons::draw(&p, Icon::Gift, Rect::from_center_size(art_c, vec2(gift_h * 0.45, gift_h * 0.45)), Color32::from_rgb(0xff, 0xc2, 0xe6));
            p.text(box_.min + vec2(SPACE_32, SPACE_32), Align2::LEFT_TOP, "Gift Nitro", bold_italic(32.0), Color32::WHITE);
            let g = p.layout("Gift someone Nitro to give them access to profile customization, emoji, and superpowered chat features.".into(), reg(16.0), pal().header_secondary, (cw - gift_h - 2.0 * SPACE_32).max(CARD_HEIGHT * 0.5));
            p.galley(box_.min + vec2(SPACE_32, SPACE_32 + 32.0 + SPACE_12), g, pal().header_secondary);
            let label = "Gift Nitro";
            if button(ui, Rect::from_min_size(pos2(box_.left() + SPACE_32, box_.bottom() - SPACE_32 - 40.0), vec2(text_width(ui, label, medium(16.0)) + 2.0 * SPACE_24, 40.0)), label, Color32::WHITE, Color32::BLACK, "gift") {
                acts.push(Act::Gift);
            }
            y += gift_h + SECTION_GAP;
            ui.allocate_exact_size(vec2(w, y - top.y), Sense::hover());
        });
    });
    for a in acts {
        match a {
            Act::Subscribe(id) => app.open_buy(crate::ui::buy::What::Nitro(id)),
            Act::Gift => app.open_buy(crate::ui::buy::What::NitroGift),
            Act::Settings(s) => app.settings_page = Some(s),
            Act::Quests => app.open_quests(),
            Act::Shop => app.open_shop(super::Page::Shop),
            Act::Wishlist => {
                app.open_shop(super::Page::Shop);
                if let Some(s) = &mut app.shop {
                    s.tab = 3;
                }
            }
            Act::Perks(i) => {
                if let Some(s) = &mut app.shop {
                    s.perks = i;
                }
            }
        }
    }
}
