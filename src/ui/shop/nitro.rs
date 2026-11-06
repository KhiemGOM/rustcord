use chrono::Timelike;
use egui::{Align2, Color32, CornerRadius, Frame, Painter, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};

use super::{NIGHT, PLATE, button, get, list, round_button, want};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{Page as Settings, billing};

const COLUMN: f32 = 1200.0;
const GAP: f32 = 24.0;

const TENURE: [(f64, &str); 5] = [(1.0, "Bronze"), (3.0, "Silver"), (6.0, "Gold"), (12.0, "Platinum"), (24.0, "Diamond")];

const FAVORITES: [(&str, [(&str, &str, Option<Settings>); 5]); 4] = [
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
    for i in 0..10 {
        p.circle_filled(c, rad * (1.0 - i as f32 / 11.0), Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha / 5));
    }
}

fn art(p: &Painter, r: Rect, kind: &Art) {
    let c = r.center();
    match kind {
        Art::Medal(gold) => {
            glow(p, c, 110.0, *gold, 90);
            p.circle_filled(c, 66.0, gold.gamma_multiply(0.9));
            p.circle_filled(c, 52.0, Color32::from_rgb(0x3a, 0x22, 0x08));
            icons::draw(p, Icon::Nitro, Rect::from_center_size(c, vec2(56.0, 56.0)), *gold);
        }
        Art::Orbs => {
            glow(p, c, 120.0, Color32::from_rgb(0xc0, 0x3a, 0xe8), 110);
            p.circle_filled(c, 52.0, Color32::from_rgb(0x58, 0x65, 0xf2));
            icons::draw(p, Icon::Gem, Rect::from_center_size(c, vec2(64.0, 64.0)), Color32::WHITE);
        }
        Art::Upload => {
            glow(p, c, 120.0, Color32::from_rgb(0x58, 0x65, 0xf2), 110);
            icons::draw(p, Icon::Upload, Rect::from_center_size(c, vec2(96.0, 96.0)), Color32::from_rgb(0xb8, 0xc0, 0xff));
        }
        Art::Quests => {
            glow(p, c, 120.0, Color32::from_rgb(0xeb, 0x45, 0x9e), 100);
            icons::draw(p, Icon::Quests, Rect::from_center_size(c, vec2(96.0, 96.0)), Color32::from_rgb(0xff, 0xc2, 0xe6));
        }
        Art::Name => {
            glow(p, c, 120.0, Color32::from_rgb(0x9b, 0x4d, 0xff), 100);
            p.text(c, Align2::CENTER_CENTER, "Aa", bold_italic(64.0), Color32::from_rgb(0xe8, 0xd5, 0xff));
        }
        Art::Themes => {
            for (i, col) in [Color32::from_rgb(0xff, 0x9a, 0xd5), Color32::from_rgb(0x58, 0x65, 0xf2), Color32::from_rgb(0x4b, 0xd6, 0xc8)].into_iter().enumerate() {
                p.circle_filled(c + vec2((i as f32 - 1.0) * 52.0, 0.0), 40.0, col);
            }
        }
        Art::Nitro => {
            glow(p, c, 120.0, Color32::from_rgb(0xc0, 0x3a, 0xe8), 110);
            icons::draw(p, Icon::Nitro, Rect::from_center_size(c, vec2(96.0, 96.0)), Color32::WHITE);
        }
    }
}

fn card(ui: &mut Ui, r: Rect, c: &Card, key: &str) -> bool {
    let p = ui.painter();
    p.rect(r, CornerRadius::same(16), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
    art(&p.with_clip_rect(r), Rect::from_min_size(r.min, vec2(r.width(), 260.0)), &c.art);
    if let Some(t) = c.tag {
        let g = p.layout_no_wrap(t.to_string(), bold(12.0), Color32::from_rgb(0x2f, 0x30, 0x35));
        let pill = Rect::from_min_size(r.min + vec2(24.0, 24.0), vec2(g.size().x + 16.0, 20.0));
        p.rect_filled(pill, CornerRadius::same(10), Color32::WHITE);
        p.galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
    }
    let x = r.left() + 20.0;
    let mut y = r.top() + 272.0;
    if let Some(e) = &c.eyebrow {
        p.text(pos2(x, y), Align2::LEFT_TOP, e, reg(14.0), pal().text_muted);
        y += 22.0;
    }
    p.text(pos2(x, y), Align2::LEFT_TOP, &c.title, semi(20.0), pal().header_primary);
    let g = p.layout(c.body.clone(), reg(14.0), pal().header_secondary, r.width() - 40.0);
    p.galley(pos2(x, y + 30.0), g, pal().header_secondary);
    match c.button {
        Some(label) => {
            let g = ui.painter().layout_no_wrap(label.to_string(), medium(16.0), Color32::WHITE);
            let b = Rect::from_min_size(pos2(x, r.bottom() - 20.0 - 40.0), vec2(g.size().x + 32.0, 40.0));
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
    let c = pos2(r.right() - 250.0, r.top() + 230.0);
    glow(p, c, 230.0, Color32::from_rgb(0x58, 0x65, 0xf2), 80);
    glow(p, c + vec2(-170.0, 120.0), 120.0, Color32::from_rgb(0xeb, 0x45, 0x9e), 70);
    icons::draw(p, Icon::Nitro, Rect::from_center_size(c, vec2(170.0, 170.0)), Color32::from_rgb(0xdd, 0xe3, 0xff));
    for (dx, dy, s, col) in [(-130.0, -140.0, 36.0, 0xc2a0ff_u32), (150.0, -150.0, 18.0, 0x8aa0ff), (190.0, 20.0, 28.0, 0xff7ad9), (-190.0, 70.0, 22.0, 0xc2a0ff)] {
        icons::draw(p, Icon::Star, Rect::from_center_size(c + vec2(dx, dy), vec2(s, s)), Color32::from_rgb((col >> 16) as u8, (col >> 8) as u8, col as u8));
    }
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
            let cw = (w - 64.0).clamp(300.0, COLUMN);
            let left = top.x + (w - cw) / 2.0;
            let p = ui.painter().clone();
            let page = Rect::from_min_size(top, vec2(w, 640.0));
            let band = p.with_clip_rect(page);
            glow(&band, pos2(top.x - 80.0, top.y - 40.0), 520.0, Color32::from_rgb(0xb0, 0x2a, 0xb0), 70);
            glow(&band, pos2(top.x + w * 0.45, top.y - 160.0), 560.0, Color32::from_rgb(0x2a, 0x1a, 0x7a), 90);
            if w > 760.0 {
                hero_art(&band, Rect::from_min_size(pos2(left, top.y), vec2(cw, 480.0)));
            }
            icons::draw(&p, Icon::Nitro, Rect::from_min_size(pos2(left - 4.0, top.y + 8.0), vec2(24.0, 24.0)), Color32::WHITE);
            p.text(pos2(left + 30.0, top.y + 20.0), Align2::LEFT_CENTER, "NITRO", bold_italic(20.0), Color32::WHITE);
            let gift = Rect::from_min_size(pos2(left + cw - 101.0, top.y + 4.0), vec2(101.0, 32.0));
            let heart = Rect::from_min_size(pos2(gift.left() - 8.0 - 32.0, top.y + 4.0), vec2(32.0, 32.0));
            if round_button(ui, heart, Icon::Heart, Color32::WHITE, "wishlist") {
                acts.push(Act::Wishlist);
            }
            let hover = ui.interact(gift, ui.id().with("nitro_gift_top"), Sense::click());
            p.rect_filled(gift, CornerRadius::same(16), Color32::from_black_alpha(if hover.hovered() { 170 } else { 143 }));
            icons::draw(&p, Icon::Gift, Rect::from_center_size(pos2(gift.left() + 18.0, gift.center().y), vec2(16.0, 16.0)), Color32::WHITE);
            p.text(pos2(gift.left() + 32.0, gift.center().y), Align2::LEFT_CENTER, "Gift Nitro", medium(14.0), Color32::WHITE);
            crate::ui::settings::mark(ui, "shop:gift_top", gift);
            if hover.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hover.clicked() {
                acts.push(Act::Gift);
            }

            let mut y = top.y + 130.0;
            p.text(pos2(left, y), Align2::LEFT_TOP, greeting(), bold_italic(32.0), Color32::WHITE);
            y += 48.0;
            let face = Rect::from_min_size(pos2(left, y), vec2(64.0, 64.0));
            crate::ui::avatar(ui, face, &avatar);
            p.text(pos2(left + 88.0, face.center().y), Align2::LEFT_CENTER, &name, reg(44.0), Color32::from_gray(0xe6));
            y += 64.0 + 56.0;

            let cols = (((cw + GAP) / (300.0 + GAP)).floor() as usize).clamp(1, 3);
            let cell = (cw - (cols - 1) as f32 * GAP) / cols as f32;
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
            y += 44.0;
            for row in cards.chunks(cols) {
                for (i, c) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(left + i as f32 * (cell + GAP), y), vec2(cell, 440.0));
                    if card(ui, r, c, &format!("card:{}", c.title))
                        && let Some(a) = &c.act
                    {
                        acts.push(a.clone());
                    }
                }
                y += 440.0 + GAP;
            }

            y += 24.0;
            p.text(pos2(left, y), Align2::LEFT_TOP, "CHECK OUT THESE NITRO FAVORITES", bold_italic(20.0), Color32::from_gray(0xe6));
            y += 44.0;
            let mut x = left;
            for (i, (name, _)) in FAVORITES.iter().enumerate() {
                let g = p.layout_no_wrap(name.to_string(), medium(14.0), Color32::WHITE);
                let r = Rect::from_min_size(pos2(x, y), vec2(g.size().x + 32.0, 36.0));
                let resp = ui.interact(r, ui.id().with(("nitro_tab", i)), Sense::click());
                let on = tab as usize == i;
                p.rect_filled(r, CornerRadius::same(18), if on { Color32::WHITE } else if resp.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(22) });
                p.galley(r.center() - g.size() / 2.0, g, if on { Color32::BLACK } else { Color32::WHITE });
                crate::ui::settings::mark(ui, &format!("shop:perks:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    acts.push(Act::Perks(i as u8));
                }
                x = r.right() + 8.0;
            }
            y += 36.0 + 20.0;
            let perks = &FAVORITES[(tab as usize).min(3)].1;
            let pc = (((cw + GAP) / (280.0 + GAP)).floor() as usize).clamp(1, 3);
            let pw = (cw - (pc - 1) as f32 * GAP) / pc as f32;
            for row in perks.chunks(pc) {
                for (i, (title, body, go)) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(left + i as f32 * (pw + GAP), y), vec2(pw, 200.0));
                    p.rect(r, CornerRadius::same(16), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
                    p.text(r.min + vec2(20.0, 22.0), Align2::LEFT_TOP, *title, semi(20.0), pal().header_primary);
                    let g = p.layout(body.to_string(), reg(14.0), pal().header_secondary, pw - 40.0);
                    p.galley(r.min + vec2(20.0, 56.0), g, pal().header_secondary);
                    if let Some(s) = go {
                        let b = Rect::from_min_size(pos2(r.left() + 20.0, r.bottom() - 60.0), vec2(124.0, 40.0));
                        if button(ui, b, "Take me there", Color32::from_rgba_unmultiplied(151, 151, 159, 40), Color32::WHITE, &format!("perk:{title}")) {
                            acts.push(Act::Settings(*s));
                        }
                    }
                }
                y += 200.0 + GAP;
            }

            y += 24.0;
            let band = Rect::from_min_size(pos2(left, y), vec2(cw, 240.0));
            p.rect(band, CornerRadius::same(24), PLATE, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
            glow(&p.with_clip_rect(band), pos2(band.right() - 130.0, band.center().y), 150.0, Color32::from_rgb(0xeb, 0x45, 0x9e), 80);
            icons::draw(&p, Icon::Gift, Rect::from_center_size(pos2(band.right() - 130.0, band.center().y), vec2(110.0, 110.0)), Color32::from_rgb(0xff, 0xc2, 0xe6));
            p.text(band.min + vec2(32.0, 36.0), Align2::LEFT_TOP, "Gift Nitro", bold_italic(32.0), Color32::WHITE);
            let g = p.layout("Gift someone Nitro to give them access to profile customization, emoji, and superpowered chat features.".into(), reg(16.0), pal().header_secondary, (band.width() - 340.0).max(200.0));
            p.galley(band.min + vec2(32.0, 84.0), g, pal().header_secondary);
            if button(ui, Rect::from_min_size(pos2(band.left() + 32.0, band.bottom() - 32.0 - 40.0), vec2(120.0, 40.0)), "Gift Nitro", Color32::WHITE, Color32::BLACK, "gift") {
                acts.push(Act::Gift);
            }
            y += 240.0 + 48.0;
            ui.allocate_exact_size(vec2(w, y - top.y), Sense::hover());
        });
    });
    for a in acts {
        match a {
            Act::Subscribe(id) => crate::webwin::subscribe(&id, false),
            Act::Gift => crate::webwin::gift_nitro(),
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
