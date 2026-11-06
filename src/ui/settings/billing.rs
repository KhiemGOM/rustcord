use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{LOGOUT, Look, card, danger, end, faint, input, lead, link, loading, mark, primary, refetch, remote, secondary, section};
use crate::net::Tag;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

mod boosts;
mod gifts;

pub use boosts::boost;
pub use gifts::gifts;

type Step = (&'static str, String, Option<Value>);

pub const KEYS: [&str; 9] = ["subs", "sources", "payments", "slots", "cooldown", "credits", "gifts", "gift_codes", "badge"];
pub const NITRO_SKU: &str = "521842831262875670";
pub const BASIC_SKU: &str = "978380692553465865";
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const COUNTRIES: [(&str, &str); 36] = [
    ("US", "United States"), ("GB", "United Kingdom"), ("VN", "Vietnam"), ("CA", "Canada"), ("AU", "Australia"), ("DE", "Germany"),
    ("FR", "France"), ("ES", "Spain"), ("IT", "Italy"), ("NL", "Netherlands"), ("BR", "Brazil"), ("MX", "Mexico"), ("JP", "Japan"),
    ("KR", "South Korea"), ("IN", "India"), ("PH", "Philippines"), ("ID", "Indonesia"), ("TH", "Thailand"), ("SG", "Singapore"),
    ("PL", "Poland"), ("SE", "Sweden"), ("NO", "Norway"), ("DK", "Denmark"), ("FI", "Finland"), ("IE", "Ireland"), ("NZ", "New Zealand"),
    ("TR", "Turkey"), ("UA", "Ukraine"), ("PT", "Portugal"), ("AT", "Austria"), ("CH", "Switzerland"), ("BE", "Belgium"),
    ("AR", "Argentina"), ("CL", "Chile"), ("ZA", "South Africa"), ("MY", "Malaysia"),
];
const SUPPORT: &str = "https://support.discord.com/hc/en-us/articles/";

fn parts(s: &str) -> Option<[u32; 6]> {
    let n = |a: usize, l: usize| s.get(a..a + l).and_then(|x| x.parse::<u32>().ok());
    Some([n(0, 4)?, n(5, 2)?, n(8, 2)?, n(11, 2).unwrap_or(0), n(14, 2).unwrap_or(0), n(17, 2).unwrap_or(0)])
}

pub fn mdy(s: &str) -> String {
    parts(s).map_or(String::new(), |p| format!("{:02}/{:02}/{}", p[1], p[2], p[0]))
}

pub fn long(s: &str) -> String {
    parts(s).map_or(String::new(), |p| format!("{} {}, {}", MONTHS[(p[1].clamp(1, 12) - 1) as usize], p[2], p[0]))
}

pub fn epoch(s: &str) -> i64 {
    let Some(p) = parts(s) else { return 0 };
    let (y, m, d) = (p[0] as i64, p[1] as i64, p[2] as i64);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    days * 86400 + p[3] as i64 * 3600 + p[4] as i64 * 60 + p[5] as i64
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn money(amount: i64, cur: &str) -> String {
    let cur = cur.to_lowercase();
    let zero = matches!(cur.as_str(), "vnd" | "jpy" | "krw" | "clp");
    let sym = match cur.as_str() {
        "usd" => "$".to_string(),
        "eur" => "€".to_string(),
        "gbp" => "£".to_string(),
        "vnd" => "₫".to_string(),
        "jpy" => "¥".to_string(),
        "krw" => "₩".to_string(),
        "inr" => "₹".to_string(),
        "brl" => "R$".to_string(),
        "cad" => "CA$".to_string(),
        "aud" => "A$".to_string(),
        c => format!("{} ", c.to_uppercase()),
    };
    let (whole, cents) = if zero { (amount.abs(), 0) } else { (amount.abs() / 100, amount.abs() % 100) };
    let digits = whole.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let sign = if amount < 0 { "-" } else { "" };
    if zero { format!("{sign}{sym}{out}") } else { format!("{sign}{sym}{out}.{cents:02}") }
}

fn country(code: &str) -> String {
    COUNTRIES.iter().find(|(c, _)| c.eq_ignore_ascii_case(code)).map_or(code.to_string(), |(_, n)| n.to_string())
}

pub fn plan_name(id: &str) -> &'static str {
    match id {
        "511651871736201216" => "Nitro Classic Monthly",
        "511651876987469824" => "Nitro Classic Yearly",
        "511651880837840896" => "Nitro Monthly",
        "511651885459963904" => "Nitro Yearly",
        "978380692553465866" => "Nitro Basic Monthly",
        "1024422698568122368" => "Nitro Basic Yearly",
        "590665532894740483" => "Server Boost Monthly",
        "590665538238152709" => "Server Boost Yearly",
        _ => "Subscription",
    }
}

fn get<T: Clone + Default + Send + Sync + 'static>(ui: &Ui, key: &str) -> T {
    ui.data(|d| d.get_temp(Id::new(("billing", key)))).unwrap_or_default()
}

fn put<T: Clone + Send + Sync + 'static>(ui: &Ui, key: &str, v: T) {
    ui.data_mut(|d| d.insert_temp(Id::new(("billing", key)), v));
}

fn fetch(app: &mut App, ui: &mut Ui, key: &'static str, path: &str) -> Option<Vec<Value>> {
    loading(ui, remote(app, key, path)).map(|v| v.as_array().cloned().unwrap_or_default())
}

fn quiet(app: &mut App, key: &'static str, path: &str) -> Vec<Value> {
    match remote(app, key, path) {
        Some(Ok(v)) => v.as_array().cloned().unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn act(app: &mut App, ui: &Ui, mut steps: Vec<Step>) {
    if steps.is_empty() || app.remote.contains_key("bill_act") {
        return;
    }
    let (method, path, body) = steps.remove(0);
    app.remote.insert("bill_act", None);
    app.rest(method, path, body, Tag::Remote("bill_act"));
    put(ui, "queue", steps);
}

fn settle(app: &mut App, ui: &Ui) {
    let Some(Some(res)) = app.remote.get("bill_act").cloned() else { return };
    app.remote.remove("bill_act");
    let next: Vec<Step> = get(ui, "queue");
    match res {
        Ok(_) if !next.is_empty() => act(app, ui, next),
        Ok(_) => KEYS.iter().for_each(|k| refetch(app, k)),
        Err(e) => {
            put(ui, "queue", Vec::<Step>::new());
            KEYS.iter().for_each(|k| refetch(app, k));
            app.toast(e);
        }
    }
}

fn confirm(ctx: &egui::Context, id: &str, head: &str, body: &str, ok: &str, red: bool) -> Option<bool> {
    let mut out = None;
    let r = crate::ui::motion::Modal::new(Id::new(("billing_modal", id))).frame(Frame::new().fill(pal().bg_secondary).corner_radius(8)).show(ctx, |ui| {
        ui.set_width(440.0);
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.label(RichText::new(head).font(semi(20.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.label(RichText::new(body).font(reg(16.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let w = ui.painter().layout_no_wrap(ok.into(), semi(14.0), Color32::WHITE).size().x + 32.0;
                let b = crate::ui::button(ui, ok, if red { RED } else { BLURPLE }, w);
                mark(ui, "billing_confirm", b.rect);
                if b.clicked() {
                    out = Some(true);
                }
                ui.add_space(8.0);
                if ui.add(egui::Label::new(RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                    out = Some(false);
                }
            });
        });
    });
    if r.should_close() {
        out = Some(false);
    }
    out
}

pub fn pill(p: &egui::Painter, at: egui::Pos2, text: &str, fill: Color32) -> f32 {
    let g = p.layout_no_wrap(text.into(), semi(10.0), Color32::WHITE);
    let r = Rect::from_min_size(at, vec2(g.size().x + 10.0, 16.0));
    p.rect_filled(r, 3, fill);
    p.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    r.width()
}

pub fn solid(ui: &mut Ui, r: Rect, text: &str, fill: Color32, ink: Color32) -> bool {
    let resp = ui.interact(r, Id::new(("billing_btn", text, r.min.x as i32, r.min.y as i32)), Sense::click());
    let fill = if resp.hovered() { fill.gamma_multiply(0.85) } else { fill };
    ui.painter().rect_filled(r, 8, fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(16.0), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("billing:{text}"), r);
    resp.clicked()
}

pub fn small(ui: &mut Ui, text: &str, look: Look) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + 32.0).max(60.0), 32.0), Sense::click());
    let (fill, ink) = match look {
        Look::Primary => (BLURPLE, Color32::WHITE),
        Look::Danger => (faint(31), LOGOUT),
        _ => (faint(31), pal().header_primary),
    };
    let fill = if resp.hovered() { if look == Look::Primary { fill.gamma_multiply(0.85) } else { faint(51) } } else { fill };
    ui.painter().rect(r, CornerRadius::same(8), fill, Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(14.0), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("billing:{text}"), r);
    resp
}

fn gradient(ui: &Ui, r: Rect, a: Color32, b: Color32, radius: u8) {
    const N: usize = 48;
    for i in 0..N {
        let t = i as f32 / (N - 1) as f32;
        let x0 = r.left() + r.width() * i as f32 / N as f32;
        let x1 = (r.left() + r.width() * (i + 1) as f32 / N as f32 + 0.5).min(r.right());
        let cr = CornerRadius {
            nw: if i == 0 { radius } else { 0 },
            sw: if i == 0 { radius } else { 0 },
            ne: if i == N - 1 { radius } else { 0 },
            se: if i == N - 1 { radius } else { 0 },
        };
        ui.painter().rect_filled(Rect::from_x_y_ranges(x0..=x1, r.y_range()), cr, a.lerp_to_gamma(b, t));
    }
}

pub fn brand_tile(p: &egui::Painter, r: Rect, brand: &str) {
    let fill = match brand.to_lowercase().as_str() {
        "visa" => Color32::from_rgb(0x1a, 0x1f, 0x71),
        "mastercard" => Color32::from_rgb(0xeb, 0x00, 0x1b),
        "amex" | "american express" => Color32::from_rgb(0x2e, 0x77, 0xbc),
        "paypal" => Color32::from_rgb(0x00, 0x30, 0x87),
        _ => Color32::from_rgb(0x6d, 0x6f, 0x78),
    };
    p.rect_filled(r, 4, fill);
    p.rect_filled(Rect::from_min_size(pos2(r.left(), r.top() + r.height() * 0.22), vec2(r.width(), r.height() * 0.2)), 0, Color32::from_white_alpha(200));
    p.rect_filled(Rect::from_min_size(pos2(r.left() + 5.0, r.bottom() - 8.0), vec2(10.0, 3.0)), 1, Color32::from_white_alpha(170));
}

fn plans(app: &mut App, key: &'static str, sku: &str) -> Vec<Value> {
    quiet(app, key, &format!("/store/published-listings/skus/{sku}/subscription-plans?revenue_surface=0"))
}

fn plan_price(plan: &Value) -> String {
    let each = if plan["interval"] == 2 { "year" } else { "month" };
    format!("{}/{each}", money(plan["price"].as_i64().unwrap_or(0), plan["currency"].as_str().unwrap_or("usd")))
}

fn subscribe(app: &mut App, plans: &[Value]) {
    if let Some(id) = plans.iter().find(|p| p["interval"] == 1 && p["interval_count"] == 1).and_then(|p| p["id"].as_str()) {
        app.open_buy(crate::ui::buy::What::Nitro(id.to_string()));
    }
}

fn monthly(plans: &[Value]) -> String {
    plans.iter().find(|p| p["interval"] == 1 && p["interval_count"] == 1).map_or("—".into(), plan_price)
}

const FEATURES: [(&str, &str, &str); 20] = [
    ("Custom emoji anywhere and make them animated", "Included", "Included"),
    ("Custom stickers anywhere", "Included", "Included"),
    ("Custom App Icons", "Included", "Included"),
    ("Super Reactions", "Included", "Included"),
    ("Larger File Uploads", "50 MB", "1 GB"),
    ("HD streaming", "Not included", "Up to 4K and 60fps"),
    ("2 Boosts + 30% off extra Boosts", "Not included", "Included"),
    ("Animated avatar, banner, and profile theme", "Not included", "Included"),
    ("Display Name Styles", "Not included", "Included"),
    ("Custom per-server profiles", "Not included", "Included"),
    ("Shop member pricing, and exclusive items", "Not included", "Included"),
    ("Colors for your Discord theme", "Not included", "Included"),
    ("Nitro badge on your profile", "Included", "Included"),
    ("Custom video backgrounds", "Included", "Included"),
    ("Custom sounds anywhere", "Not included", "Included"),
    ("Custom entrance sounds", "Not included", "Included"),
    ("Join up to 200 servers", "Not included", "Included"),
    ("Longer messages up to 4000 characters", "Not included", "Included"),
    ("3 Nitro friend passes", "Not included", "Included"),
    ("Keep Quest avatar decorations rewards longer", "Not included", "Included"),
];

pub const NITRO_PERKS: [(Icon, &str); 6] = [
    (Icon::Upload, "1GB uploads"),
    (Icon::Smile, "Custom emoji anywhere"),
    (Icon::AddReaction, "Unlimited Super Reactions"),
    (Icon::Screen, "HD video streaming"),
    (Icon::Gem, "2 Server Boosts"),
    (Icon::UserProfile, "Custom profiles and more!"),
];

pub const BASIC_PERKS: [(Icon, &str); 4] = [
    (Icon::Upload, "50MB uploads"),
    (Icon::Smile, "Custom emoji anywhere"),
    (Icon::Sticker, "Custom stickers anywhere"),
    (Icon::AddReaction, "Unlimited Super Reactions"),
];

fn hero(ui: &mut Ui, name: &str, price: &str, perks: &[(Icon, &str)], buttons: &[(&str, Color32, Color32)]) -> Option<usize> {
    lead(ui);
    let w = ui.available_width();
    let h = 32.0 + 52.0 + 30.0 + 16.0 + perks.len() as f32 * 32.0 + 16.0 + buttons.len() as f32 * 48.0 + 24.0;
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    gradient(ui, r, Color32::from_rgb(0x93, 0x3f, 0xcf), Color32::from_rgb(0xc2, 0x5e, 0xc0), 12);
    let p = ui.painter().clone();
    p.text(r.min + vec2(32.0, 32.0), Align2::LEFT_TOP, name, semi(44.0), Color32::WHITE);
    if !price.is_empty() {
        p.text(r.min + vec2(32.0, 88.0), Align2::LEFT_TOP, price, medium(18.0), Color32::WHITE);
    }
    let mut y = r.top() + 134.0;
    for (icon, text) in perks {
        icons::draw(&p, *icon, Rect::from_center_size(pos2(r.left() + 44.0, y + 12.0), vec2(24.0, 24.0)), Color32::WHITE);
        p.text(pos2(r.left() + 68.0, y + 12.0), Align2::LEFT_CENTER, *text, reg(16.0), Color32::WHITE);
        y += 32.0;
    }
    y += 16.0;
    let mut hit = None;
    for (i, (text, fill, ink)) in buttons.iter().enumerate() {
        let b = Rect::from_min_size(pos2(r.left() + 32.0, y), vec2(320.0, 40.0));
        if solid(ui, b, text, *fill, *ink) {
            hit = Some(i);
        }
        y += 48.0;
    }
    end(ui, 40.0);
    hit
}

fn features(app: &mut App, ui: &mut Ui, basic_plans: &[Value], nitro_plans: &[Value], buy: bool) {
    let (basic, nitro) = (monthly(basic_plans), monthly(nitro_plans));
    lead(ui);
    let w = ui.available_width();
    let cols = [w - 320.0, w - 160.0];
    let (head, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
    let p = ui.painter().clone();
    p.text(head.left_center(), Align2::LEFT_CENTER, "Pricing and Features", semi(20.0), pal().header_primary);
    p.text(pos2(head.left() + cols[0] + 80.0, head.center().y), Align2::CENTER_CENTER, "Nitro Basic", semi(16.0), pal().header_primary);
    p.text(pos2(head.left() + cols[1] + 80.0, head.center().y), Align2::CENTER_CENTER, "Nitro", semi(16.0), pal().header_primary);
    let line = |ui: &Ui, y: f32| ui.painter().hline(head.x_range(), y, Stroke::new(1.0, faint(51)));
    line(ui, head.bottom());
    let mut rows = vec![("Price", basic.to_string(), nitro.to_string())];
    rows.extend(FEATURES.iter().map(|(a, b, c)| (*a, b.to_string(), c.to_string())));
    for (name, b, n) in &rows {
        let g = ui.painter().layout(name.to_string(), reg(16.0), pal().header_primary, cols[0] - 16.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, (g.size().y + 24.0).max(48.0)), Sense::hover());
        p.galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, pal().header_primary);
        for (x, v) in [(cols[0], b), (cols[1], n)] {
            let c = pos2(r.left() + x + 80.0, r.center().y);
            match v.as_str() {
                "Included" => icons::draw(&p, Icon::Check, Rect::from_center_size(c, vec2(24.0, 24.0)), pal().header_primary),
                "Not included" => icons::draw(&p, Icon::Close, Rect::from_center_size(c, vec2(20.0, 20.0)), pal().text_muted),
                t => {
                    p.text(c, Align2::CENTER_CENTER, t, medium(14.0), pal().header_primary);
                }
            }
        }
        line(ui, r.bottom());
    }
    if buy {
        let (r, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
        for (x, label, plans) in [(cols[0], "Nitro Basic", basic_plans), (cols[1], "Nitro", nitro_plans)] {
            let b = Rect::from_center_size(pos2(r.left() + x + 80.0, r.center().y), vec2(144.0, 40.0));
            if solid(ui, b, &format!("Subscribe to {label}"), BLURPLE, Color32::WHITE) {
                subscribe(app, plans);
            }
        }
    }
    end(ui, 24.0);
}

pub fn nitro(app: &mut App, ui: &mut Ui) -> bool {
    settle(app, ui);
    let tier = app.store.me_raw["premium_type"].as_u64().unwrap_or(0);
    let nitro_plans = plans(app, "plans_nitro", NITRO_SKU);
    let basic_plans = plans(app, "plans_basic", BASIC_SKU);
    let subs = quiet(app, "subs", "/users/@me/billing/subscriptions?sync_level=1");
    let mine = subs.iter().find(|s| s["type"] == 1 && matches!(s["status"].as_u64(), Some(1..=3)));
    let plan_id = mine.and_then(|s| s["items"][0]["plan_id"].as_str()).unwrap_or("");
    let own = nitro_plans.iter().chain(&basic_plans).find(|p| p["id"] == plan_id);
    let price = own.map_or_else(|| monthly(if tier == 3 { &basic_plans } else { &nitro_plans }), plan_price);
    let price_line = price.replace('/', " / ");
    if tier == 0 {
        let hit = hero(ui, "NITRO", &format!("{} / month", monthly(&nitro_plans).trim_end_matches("/month")), &NITRO_PERKS, &[("Subscribe", Color32::WHITE, Color32::from_rgb(0x2e, 0x33, 0x38))]);
        if hit == Some(0) {
            subscribe(app, &nitro_plans);
        }
    } else {
        let name = ["", "NITRO CLASSIC", "NITRO", "NITRO BASIC"][tier.min(3) as usize];
        let perks: &[(Icon, &str)] = if tier == 2 { &NITRO_PERKS } else { &BASIC_PERKS };
        let hit = hero(
            ui,
            name,
            &price_line,
            perks,
            &[("Learn More About My Nitro", Color32::WHITE, Color32::from_rgb(0x2e, 0x33, 0x38)), ("Manage Plan in Subscriptions", Color32::from_black_alpha(110), Color32::WHITE)],
        );
        match hit {
            Some(0) => {
                let _ = open::that_detached(format!("{SUPPORT}115002812632"));
            }
            Some(1) => app.settings_page = Some(super::Page::Subscriptions),
            _ => {}
        }
    }
    if tier == 2 {
        section(ui, "Share Nitro with Friends");
        for n in 1..=3 {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.label(RichText::new(n.to_string()).font(semi(24.0)).color(pal().header_primary));
                    let text = if n == 3 {
                        "Either you have more trials to send or this trial offer has expired. You can resend or share with another friend. Send three friends a 2-week Nitro trial and become the MVP of the group chat."
                    } else {
                        "Either you have more trials to send or this trial offer has expired. You can resend or share with another friend."
                    };
                    ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().header_secondary)).wrap());
                });
            });
        }
        lead(ui);
        ui.horizontal(|ui| {
            if link(ui, "Learn more").clicked() {
                let _ = open::that_detached(format!("{SUPPORT}22567595858327"));
            }
        });
        end(ui, 16.0);
        if primary(ui, "Select Friends").clicked() {
            super::open_web("/settings/premium");
        }
    }
    if tier != 0 {
        section(ui, "What You’ve Got");
    }
    features(app, ui, &basic_plans, &nitro_plans, tier == 0);
    false
}

pub fn pay_label(s: &Value) -> String {
    if s["type"] == 2 {
        return format!("PayPal {}", s["email"].as_str().unwrap_or(""));
    }
    let brand = s["brand"].as_str().unwrap_or("Card");
    let brand = brand.get(..1).map_or(String::new(), |f| f.to_uppercase() + &brand[1..]);
    format!("{brand} ending in {}", s["last_4"].as_str().unwrap_or(""))
}

pub fn expires(s: &Value) -> String {
    match (s["expires_month"].as_u64(), s["expires_year"].as_u64()) {
        (Some(m @ 1..=12), Some(y)) => format!("Expires {} {y}", MONTHS[m as usize - 1]),
        _ => String::new(),
    }
}

pub fn subscriptions(app: &mut App, ui: &mut Ui) -> bool {
    settle(app, ui);
    let sources = quiet(app, "sources", "/users/@me/billing/payment-sources");
    let Some(subs) = fetch(app, ui, "subs", "/users/@me/billing/subscriptions?sync_level=1") else { return false };
    let credits = quiet(app, "credits", "/users/@me/entitlements?with_sku=false&with_application=false&entitlement_type=11&exclude_ended=true");
    let mut cancel: Option<(String, String)> = get(ui, "cancel");
    for s in subs.iter().filter(|s| s["status"].as_u64() != Some(4)) {
        let id = s["id"].as_str().unwrap_or("");
        let status = s["status"].as_u64().unwrap_or(1);
        let ending = !s["canceled_at"].is_null() || status == 3;
        let ends = long(s["current_period_end"].as_str().unwrap_or(""));
        let name = match s["type"].as_u64() {
            Some(2) => "Server Boost".to_string(),
            _ => plan_name(s["items"][0]["plan_id"].as_str().unwrap_or("")).to_string(),
        };
        let source = sources.iter().find(|x| x["id"] == s["payment_source_id"]).map(pay_label);
        let (word, fill) = match (status, ending) {
            (2, _) => ("PAST DUE", RED),
            (_, true) => ("CANCELED", Color32::from_rgb(0x6d, 0x6f, 0x78)),
            _ => ("ACTIVE", GREEN),
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).font(semi(20.0)).color(pal().header_primary));
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(72.0, 16.0), Sense::hover());
                pill(ui.painter(), r.min, word, fill);
            });
            ui.add_space(4.0);
            let when = match (status, ending) {
                (2, _) => "Your current subscription is past due. Please update your payment method.".to_string(),
                (_, true) => format!("Ends {ends}. You can resubscribe any time before then."),
                _ => format!("Renews {ends}"),
            };
            ui.label(RichText::new(when).font(reg(14.0)).color(pal().header_secondary));
            if let Some(src) = source {
                ui.label(RichText::new(format!("Payment method: {src}")).font(reg(14.0)).color(pal().header_secondary));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ending {
                    if primary(ui, "Resubscribe").clicked() {
                        crate::webwin::manage();
                    }
                } else {
                    if secondary(ui, "Change Payment Method").clicked() {
                        crate::webwin::manage();
                    }
                    if danger(ui, "Cancel Subscription", false).clicked() {
                        cancel = Some((id.to_string(), ends.clone()));
                    }
                }
            });
        });
    }
    lead(ui);
    ui.label(RichText::new("Subscription Credit").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.add(egui::Label::new(RichText::new("When you accept a gift while you have a running subscription, redeem a promotion, or accept a gift that differs from your current subscription, it will appear here as credit.").font(reg(14.0)).color(pal().header_secondary)).wrap());
    ui.add_space(16.0);
    if credits.is_empty() {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        let tile = Rect::from_center_size(pos2(r.left() + 28.0, r.center().y), vec2(32.0, 32.0));
        ui.painter().rect(tile, CornerRadius::same(6), pal().bg_tertiary, Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        ui.painter().text(tile.center(), Align2::CENTER_CENTER, "?", semi(18.0), pal().header_primary);
        ui.painter().text(pos2(r.left() + 64.0, r.center().y), Align2::LEFT_CENTER, "You have no unused credits", medium(16.0), pal().header_primary);
    }
    for c in &credits {
        let ends = long(c["ends_at"].as_str().unwrap_or(""));
        card(ui, |ui| {
            ui.label(RichText::new("Nitro Credit").font(medium(16.0)).color(pal().header_primary));
            if !ends.is_empty() {
                ui.label(RichText::new(format!("Expires {ends}")).font(reg(14.0)).color(pal().header_secondary));
            }
        });
    }
    ui.add_space(24.0);
    ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, faint(51)));
    put(ui, "cancel", cancel.clone());
    if let Some((id, ends)) = cancel {
        let body = format!("Your plan stays active until {ends}. After that you will lose your Nitro perks. You can resubscribe any time before then.");
        match confirm(ui.ctx(), "cancel_sub", "Cancel Subscription", &body, "Cancel Subscription", true) {
            Some(true) => {
                put(ui, "cancel", None::<(String, String)>);
                act(app, ui, vec![("DELETE", format!("/users/@me/billing/subscriptions/{id}"), None)]);
            }
            Some(false) => put(ui, "cancel", None::<(String, String)>),
            None => {}
        }
    }
    false
}

fn address_lines(s: &Value) -> Vec<String> {
    let a = &s["billing_address"];
    let f = |k: &str| a[k].as_str().filter(|x| !x.is_empty()).map(str::to_string);
    let city = [f("city"), f("state"), f("postal_code")].into_iter().flatten().collect::<Vec<_>>().join(", ");
    [f("name"), f("line_1"), f("line_2"), Some(city).filter(|c| !c.is_empty()), f("country").map(|c| country(&c))].into_iter().flatten().collect()
}

fn source_row(ui: &mut Ui, s: &Value, in_use: bool) -> bool {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 76.0), Sense::hover());
    let p = ui.painter().clone();
    brand_tile(&p, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(40.0, 28.0)), s["brand"].as_str().unwrap_or(if s["type"] == 2 { "paypal" } else { "" }));
    let label = pay_label(s);
    let g = p.layout_no_wrap(label, medium(16.0), pal().header_primary);
    let top = r.center().y - if expires(s).is_empty() { 10.0 } else { 20.0 };
    let w = g.size().x;
    p.galley(pos2(r.left() + 56.0, top), g, pal().header_primary);
    let mut x = r.left() + 56.0 + w + 8.0;
    for (on, text, fill) in [(s["default"] == true, "DEFAULT", Color32::from_rgb(0x4e, 0x50, 0x58)), (in_use, "SUBSCRIPTION", BLURPLE), (s["invalid"] == true, "INVALID", RED)] {
        if on {
            x += pill(&p, pos2(x, top + 3.0), text, fill) + 6.0;
        }
    }
    p.text(pos2(r.left() + 56.0, top + 24.0), Align2::LEFT_TOP, expires(s), reg(14.0), pal().header_secondary);
    p.hline(r.x_range(), r.bottom(), Stroke::new(1.0, faint(51)));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
    small(&mut c, "Edit", Look::Secondary).clicked()
}

fn edit_source(app: &mut App, ui: &mut Ui, sources: &[Value]) {
    let Some(id) = get::<Option<String>>(ui, "edit") else { return };
    let Some(s) = sources.iter().find(|s| s["id"] == id.as_str()) else { return put(ui, "edit", None::<String>) };
    if get::<bool>(ui, "delete") {
        let body = format!("Remove {} from your account? Subscriptions that use it will need another payment method.", pay_label(s));
        match confirm(ui.ctx(), "delete_source", "Delete Payment Method", &body, "Delete Payment Method", true) {
            Some(true) => {
                put(ui, "delete", false);
                put(ui, "edit", None::<String>);
                act(app, ui, vec![("DELETE", format!("/users/@me/billing/payment-sources/{id}"), None)]);
            }
            Some(false) => put(ui, "delete", false),
            None => {}
        }
        return;
    }
    let default = s["default"] == true;
    let (mut close, mut make, mut delete) = (false, false, false);
    let r = crate::ui::motion::Modal::new(Id::new("billing_edit_source")).frame(Frame::new().fill(pal().bg_secondary).corner_radius(8)).show(ui.ctx(), |ui| {
        ui.set_width(440.0);
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.label(RichText::new("Edit Payment Method").font(semi(24.0)).color(pal().header_primary));
            ui.add_space(16.0);
            source_summary(ui, s);
            ui.add_space(16.0);
            ui.label(RichText::new("BILLING ADDRESS").font(semi(12.0)).color(pal().header_secondary));
            ui.add_space(6.0);
            let lines = address_lines(s);
            if lines.is_empty() {
                ui.label(RichText::new("No billing address on file.").font(reg(14.0)).color(pal().text_muted));
            }
            for l in lines {
                ui.label(RichText::new(l).font(reg(16.0)).color(pal().text_normal));
            }
            if !default {
                ui.add_space(16.0);
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                let boxr = Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(20.0, 20.0));
                ui.painter().rect(boxr, CornerRadius::same(4), Color32::TRANSPARENT, Stroke::new(2.0, pal().header_secondary), egui::StrokeKind::Inside);
                ui.painter().text(pos2(row.left() + 32.0, row.center().y), Align2::LEFT_CENTER, "Make this my default payment method.", reg(16.0), pal().text_normal);
                mark(ui, "billing:make_default", row);
                make = resp.clicked();
            }
        });
        Frame::new().fill(pal().bg_secondary).corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }).inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = crate::ui::button(ui, "Done", BLURPLE, 80.0);
                close = b.clicked();
                ui.add_space(8.0);
                let d = crate::ui::button(ui, "Delete Payment Method", RED, 170.0);
                mark(ui, "billing:delete_source", d.rect);
                delete = d.clicked();
            });
        });
    });
    if make {
        put(ui, "edit", None::<String>);
        act(app, ui, vec![("PATCH", format!("/users/@me/billing/payment-sources/{id}"), Some(json!({ "default": true })))]);
    } else if delete {
        put(ui, "delete", true);
    } else if close || r.should_close() {
        put(ui, "edit", None::<String>);
    }
}

fn source_summary(ui: &mut Ui, s: &Value) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    brand_tile(ui.painter(), Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(40.0, 28.0)), s["brand"].as_str().unwrap_or(""));
    ui.painter().text(pos2(r.left() + 56.0, r.top() + 4.0), Align2::LEFT_TOP, pay_label(s), medium(16.0), pal().header_primary);
    ui.painter().text(pos2(r.left() + 56.0, r.top() + 24.0), Align2::LEFT_TOP, expires(s), reg(14.0), pal().header_secondary);
}

const PAGE: usize = 10;

fn payment_row(ui: &mut Ui, p: &Value, sources: &[Value], open: bool) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 66.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 0, faint(20));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let pt = ui.painter().clone();
    let cy = r.center().y;
    pt.text(pos2(r.left() + 20.0, cy), Align2::LEFT_CENTER, mdy(p["created_at"].as_str().unwrap_or("")), reg(14.0), pal().text_normal);
    let tile = Rect::from_center_size(pos2(r.left() + 162.0, cy), vec2(28.0, 28.0));
    pt.rect_filled(tile, 6, BLURPLE);
    icons::draw(&pt, Icon::Nitro, tile.shrink(5.0), Color32::WHITE);
    let name = p["description"].as_str().unwrap_or("Payment");
    let g = pt.layout_no_wrap(name.into(), reg(16.0), pal().header_primary);
    let gw = g.size().x;
    let gift = p["gift"] == true || p["is_gift"] == true;
    pt.galley(pos2(r.left() + 186.0, cy - if gift { 18.0 } else { g.size().y / 2.0 }), g, pal().header_primary);
    if gift {
        icons::draw(&pt, Icon::Gift, Rect::from_center_size(pos2(r.left() + 194.0, cy + 8.0), vec2(14.0, 14.0)), pal().header_secondary);
        pt.text(pos2(r.left() + 206.0, cy + 8.0), Align2::LEFT_CENTER, "This was a gift", reg(12.0), pal().header_secondary);
    }
    if p["status"] == 2 {
        pill(&pt, pos2(r.left() + 196.0 + gw, cy - 8.0), "FAILED", RED);
    }
    let amount = money(p["amount"].as_i64().unwrap_or(0), p["currency"].as_str().unwrap_or("usd"));
    pt.text(pos2(r.right() - 64.0, cy), Align2::RIGHT_CENTER, amount, reg(16.0), pal().header_primary);
    icons::draw(&pt, if open { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() - 30.0, cy), vec2(24.0, 24.0)), pal().header_primary);
    if !open {
        pt.hline(r.x_range(), r.bottom(), Stroke::new(1.0, faint(51)));
        return resp.clicked();
    }
    let cur = p["currency"].as_str().unwrap_or("usd");
    let total = p["amount"].as_i64().unwrap_or(0);
    let tax = p["tax"].as_i64().unwrap_or(0);
    let refunded = p["amount_refunded"].as_i64().unwrap_or(0);
    let source = sources.iter().find(|s| s["id"] == p["payment_source"]["id"]).or(Some(&p["payment_source"])).filter(|s| s["last_4"].is_string()).map(pay_label);
    let mut rows = vec![("Date", long(p["created_at"].as_str().unwrap_or("")))];
    rows.push(("Payment Method", source.unwrap_or_else(|| "Not available".into())));
    rows.push(("Status", ["Pending", "Completed", "Failed", "Canceled", "Refunded"][p["status"].as_u64().unwrap_or(1).min(4) as usize].into()));
    rows.push(("Subtotal", money(if p["tax_inclusive"] == true { total - tax } else { total }, cur)));
    rows.push(("Tax", money(tax, cur)));
    if refunded > 0 {
        rows.push(("Refunded", money(refunded, cur)));
    }
    rows.push(("Total", money(total, cur)));
    if let Some(id) = p["id"].as_str() {
        rows.push(("Order ID", id.to_string()));
    }
    let (d, _) = ui.allocate_exact_size(vec2(w, rows.len() as f32 * 28.0 + 24.0), Sense::hover());
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = d.top() + 4.0 + i as f32 * 28.0;
        pt.text(pos2(d.left() + 20.0, y), Align2::LEFT_TOP, k, medium(14.0), pal().header_secondary);
        pt.text(pos2(d.left() + 186.0, y), Align2::LEFT_TOP, v, reg(14.0), pal().text_normal);
    }
    pt.hline(d.x_range(), d.bottom(), Stroke::new(1.0, faint(51)));
    resp.clicked()
}

pub fn billing(app: &mut App, ui: &mut Ui) -> bool {
    settle(app, ui);
    section(ui, "Payment Methods");
    let subs = quiet(app, "subs", "/users/@me/billing/subscriptions?sync_level=1");
    let Some(sources) = fetch(app, ui, "sources", "/users/@me/billing/payment-sources") else { return false };
    if let Some(c) = sources.iter().find_map(|s| s["billing_address"]["country"].as_str().or(s["country"].as_str())) {
        ui.add_space(-17.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Country/Region: {}", country(c))).font(reg(14.0)).color(pal().header_secondary));
            let (r, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 7.0, pal().header_secondary);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "i", semi(11.0), pal().bg_primary);
            resp.on_hover_text("Your Country/Region determines your pricing, available payment methods, and which gift cards you can redeem. You can change it once every 90 days.");
        });
        ui.add_space(24.0);
    }
    if sources.is_empty() {
        super::note(ui, "No payment method saved");
    }
    let mut edit = None;
    for s in &sources {
        let in_use = subs.iter().any(|x| x["payment_source_id"] == s["id"] && matches!(x["status"].as_u64(), Some(1..=3)));
        if source_row(ui, s, in_use) {
            edit = s["id"].as_str().map(str::to_string);
        }
    }
    if edit.is_some() {
        put(ui, "edit", edit);
    }
    ui.add_space(24.0);
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        if primary(ui, "Add Payment Method").clicked() {
            crate::webwin::add_card();
        }
    });
    edit_source(app, ui, &sources);

    section(ui, "Transaction History");
    let Some(payments) = fetch(app, ui, "payments", "/users/@me/billing/payments?limit=30") else { return false };
    if payments.is_empty() {
        super::note(ui, "No transactions yet.");
        return false;
    }
    let mut page: usize = get(ui, "page");
    page = page.min((payments.len() - 1) / PAGE);
    let mut open: Vec<String> = get(ui, "open");
    Frame::new().stroke(Stroke::new(1.0, faint(51))).corner_radius(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
        for (x, text, a) in [(20.0, "Date", Align2::LEFT_CENTER), (146.0, "Description", Align2::LEFT_CENTER)] {
            ui.painter().text(pos2(head.left() + x, head.center().y), a, text, semi(12.0), pal().header_primary);
        }
        ui.painter().text(pos2(head.right() - 64.0, head.center().y), Align2::RIGHT_CENTER, "Amount", semi(12.0), pal().header_primary);
        ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, faint(51)));
        for p in payments.iter().skip(page * PAGE).take(PAGE) {
            let id = p["id"].as_str().unwrap_or("").to_string();
            let is_open = open.contains(&id);
            if payment_row(ui, p, &sources, is_open) {
                if is_open {
                    open.retain(|o| *o != id);
                } else {
                    open.push(id);
                }
            }
        }
    });
    put(ui, "open", open);
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        if small(ui, "Previous", Look::Secondary).clicked() && page > 0 {
            page -= 1;
        }
        ui.label(RichText::new(format!("Page {}", page + 1)).font(reg(14.0)).color(pal().header_secondary));
        if small(ui, "Next", Look::Secondary).clicked() && (page + 1) * PAGE < payments.len() {
            page += 1;
        }
    });
    put(ui, "page", page);
    false
}
