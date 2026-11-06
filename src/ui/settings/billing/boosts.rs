use egui::{Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::{Value, json};

use super::{Look, Step, act, card, confirm, epoch, faint, fetch, get, gradient, link, long, mark, now, put, remote, settle, small, solid, SUPPORT};
use crate::state::{Guild, Id as Gid};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};

const MAGENTA: Color32 = Color32::from_rgb(0xff, 0x73, 0xfa);
const SOUNDBOARD: [u32; 3] = [24, 36, 48];

const PERKS: [(&str, [&str; 3]); 12] = [
    ("Emoji Slots", ["100", "150", "250"]),
    ("Sticker Slots", ["15", "30", "60"]),
    ("Soundboard Slots", ["24", "36", "48"]),
    ("Stream Quality", ["720p & 60fps", "1080p & 60fps", "1080p & 60fps"]),
    ("Audio Quality", ["128kbps", "256kbps", "384kbps"]),
    ("Upload Size Limit", ["10MB", "50MB", "100MB"]),
    ("Video Stage Seats", ["50", "150", "300"]),
    ("Animated Server Icon", ["yes", "yes", "yes"]),
    ("Server Invite Background", ["yes", "yes", "yes"]),
    ("Server Banner", ["no", "Static", "Animated"]),
    ("Custom Role Icons", ["no", "yes", "yes"]),
    ("Custom Invite Link", ["no", "no", "yes"]),
];

const RECOGNITION: [(Icon, &str); 4] = [
    (Icon::Gem, "Display your support with a Member List icon"),
    (Icon::Star, "Unlock a profile badge that evolves over time"),
    (Icon::Shield, "Get an exclusive Booster role"),
    (Icon::People, "Show support for your community"),
];

const FAQ: [(&str, &str); 9] = [
    ("What is a Server Boost?", "Boosts allow you to support your favorite Servers, by unlocking perks for everyone in them. The more Boosts a server has, the more epic the perks that can be unlocked. See the chart above for more details."),
    ("How do I Boost a server?", "You can Boost any server directly on this page! If you want step-by-step instructions, you can find them in our Help Center."),
    ("Can I Boost a Server multiple times?", "Yes, you can Boost a server as many times as you want! The more Boosts the server gets, the better the perks that will be unlocked."),
    ("Can I continue Boosting a server after it reaches Level 3?", "Absolutely! Server Boosts are a great way to support your favorite servers. When you Boost a server, you'll be recognized with a special Member List Icon, a Boosting role, and a Profile Badge."),
    ("What are 'additional perks' and how are they different from Levels?", "Additional Perks are new features you can unlock directly with Boosts, separate from your server's Boost Level. Currently, there are two available: Enhanced Role Styles (which add gradient and holographic effects to roles) and Server Tags (4-character identifiers members can display on their profiles across Discord). Unlike traditional server levels, these perks can be unlocked individually without reaching specific level thresholds."),
    ("How much do the new additional perks cost?", "Enhanced Role Styles and Server Tags each cost 3 Boosts. If you want both perks, you'll need 6 Boosts total (3 for each perk)."),
    ("Are additional perks replacing the current server level system?", "No, this supplements the existing level system. All current server level perks (emoji slots, file upload limits, etc.) remain exactly the same. This new system simply gives you additional ways to use your Boosts for targeted perks without affecting your server's level progression."),
    ("Do additional perks use separate Boosts from my server level?", "Yes, Boosts used for additional perks are dedicated specifically to those perks and don't count toward your server's level progression. Similarly, Boosts allocated to your server level can't be used for additional perks. You'll need separate Boosts for each - so if you want Enhanced Role Styles, you need 3 Boosts just for that perk, plus whatever Boosts you want to maintain your current server level."),
    ("Are additional perks a one-time purchase, or do they require ongoing Boosts?", "Each additional perk requires an ongoing allocation of its required Boosts to stay active, just like levels. If your server drops below the required number of Boosts, the perk will be disabled until the necessary Boosts are reallocated."),
];

#[derive(Clone, Default)]
struct Pick {
    slot: String,
    from: Option<(String, String)>,
}

struct Slot {
    id: String,
    sub: Option<(String, String)>,
    cooldown: i64,
}

fn slot_of(v: &Value) -> Option<Slot> {
    let sub = &v["premium_guild_subscription"];
    let applied = sub.is_object() && sub["ended"] != true;
    if v["canceled"] == true && !applied {
        return None;
    }
    Some(Slot {
        id: v["id"].as_str()?.to_string(),
        sub: applied.then(|| (sub["guild_id"].as_str().unwrap_or("").to_string(), sub["id"].as_str().unwrap_or("").to_string())),
        cooldown: epoch(v["cooldown_ends_at"].as_str().unwrap_or("")),
    })
}

fn gicon(ui: &Ui, g: &Guild, r: Rect) {
    match g.icon_url() {
        Some(url) => crate::gif::paint_still(ui, &url, r, CornerRadius::same(10), false),
        None => {
            ui.painter().rect_filled(r, 10, pal().bg_mod_selected);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, g.acronym(), semi(14.0), pal().text_normal);
        }
    }
}

fn gem(ui: &Ui, c: egui::Pos2, size: f32) {
    icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(c, vec2(size, size)), MAGENTA);
}

fn h2(ui: &mut Ui, s: &str) {
    ui.add_space(48.0);
    ui.add(egui::Label::new(RichText::new(s).font(reg(24.0)).color(pal().header_primary)).selectable(false));
    ui.add_space(24.0);
}

fn levels(boosts: u32, tier: u8) -> String {
    let n = format!("{boosts} Boost{}", if boosts == 1 { "" } else { "s" });
    if boosts == 0 { "No Boosts".into() } else if tier == 0 { n } else { format!("{n} • Level {tier}") }
}

fn apply(slot: &str, gid: &str) -> Step {
    ("PUT", format!("/guilds/{gid}/premium/subscriptions"), Some(json!({ "user_premium_guild_subscription_slot_ids": [slot] })))
}

fn guilds(app: &App, skip: &[String]) -> Vec<Gid> {
    let mut all: Vec<Gid> = app.store.guild_order.iter().copied().filter(|g| app.store.guilds.contains_key(g) && !skip.contains(&g.to_string())).collect();
    all.sort_by_key(|g| std::cmp::Reverse(app.store.guilds[g].boosts));
    all
}

pub fn boost(app: &mut App, ui: &mut Ui) -> bool {
    settle(app, ui);
    let Some(raw) = fetch(app, ui, "slots", "/users/@me/guilds/premium/subscription-slots") else { return false };
    let global = match remote(app, "cooldown", "/users/@me/guilds/premium/subscriptions/cooldown") {
        Some(Ok(v)) => epoch(v["ends_at"].as_str().unwrap_or("")),
        _ => 0,
    };
    let slots: Vec<Slot> = raw.iter().filter_map(slot_of).collect();
    let t = now();
    let free: Vec<&Slot> = slots.iter().filter(|s| s.sub.is_none()).collect();
    let ready = free.iter().find(|s| s.cooldown.max(global) <= t).map(|s| s.id.clone());
    let mut pick: Option<Pick> = get(ui, "pick");

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (r, _) = ui.allocate_exact_size(vec2(32.0, 36.0), Sense::hover());
        gem(ui, r.center(), 32.0);
        ui.label(RichText::new("Server Boosts").font(reg(28.0)).color(pal().header_primary));
    });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Boosting a server unlocks perks for everyone in that server. You can manage your Boosts here (add, subtract, cancel — just beware the 7 day cooldown).").font(reg(14.0)).color(pal().header_secondary));
        if link(ui, "Learn more").clicked() {
            let _ = open::that_detached(format!("{SUPPORT}360028038352"));
        }
    });
    ui.add_space(32.0);

    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 80.0), Sense::hover());
    gradient(ui, r, Color32::from_rgb(0x3b, 0x1c, 0x5e), Color32::from_rgb(0x12, 0x10, 0x1a), 12);
    gem(ui, pos2(r.left() + 44.0, r.center().y), 36.0);
    let dot = pos2(r.left() + 58.0, r.center().y + 14.0);
    ui.painter().circle_filled(dot, 9.0, Color32::BLACK);
    ui.painter().text(dot, Align2::CENTER_CENTER, free.len().to_string(), semi(12.0), Color32::WHITE);
    let title = match free.len() {
        0 => "You have no unused Boosts".to_string(),
        1 => "You have 1 unused Boost".to_string(),
        n => format!("You have {n} unused Boosts"),
    };
    ui.painter().text(pos2(r.left() + 84.0, r.center().y - 12.0), Align2::LEFT_CENTER, title, semi(20.0), Color32::WHITE);
    icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(r.left() + 92.0, r.center().y + 14.0), vec2(14.0, 14.0)), Color32::WHITE);
    ui.painter().text(pos2(r.left() + 106.0, r.center().y + 14.0), Align2::LEFT_CENTER, "Nitro users get 2 Boosts included with their membership.", medium(12.0), Color32::WHITE);
    let btn = Rect::from_min_size(pos2(r.right() - 180.0, r.center().y - 20.0), vec2(160.0, 40.0));
    let label = if free.is_empty() { "Get more Boosts" } else { "Boost any server" };
    mark(ui, "billing:boost_banner", btn);
    if solid(ui, btn, label, BLURPLE, Color32::WHITE) {
        match (ready.as_ref(), free.is_empty()) {
            (Some(slot), _) => pick = Some(Pick { slot: slot.clone(), from: None }),
            (None, true) => crate::ui::settings::open_web("/settings/premium-guild-subscriptions"),
            _ => {}
        }
    }
    ui.add_space(32.0);

    super::super::sub(ui, "Active Boosts");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("These are your purchased Boosts, powering your favorite servers.").font(reg(14.0)).color(pal().header_secondary));
        if link(ui, "Learn more").clicked() {
            let _ = open::that_detached(format!("{SUPPORT}360028038352"));
        }
    });
    ui.add_space(16.0);

    let mut applied: Vec<(String, Vec<&Slot>)> = Vec::new();
    for s in slots.iter().filter(|s| s.sub.is_some()) {
        let gid = s.sub.as_ref().map(|x| x.0.clone()).unwrap_or_default();
        match applied.iter_mut().find(|(g, _)| *g == gid) {
            Some((_, v)) => v.push(s),
            None => applied.push((gid, vec![s])),
        }
    }
    let mut remove: Option<(String, String)> = get(ui, "remove");
    for (gid, mine) in &applied {
        let guild = gid.parse::<Gid>().ok().and_then(|g| app.store.guilds.get(&g)).cloned();
        let name = guild.as_ref().map_or("Unknown server".to_string(), |g| g.name.clone());
        let (boosts, tier) = guild.as_ref().map_or((0, 0), |g| (g.boosts, g.tier));
        card(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            let tile = Rect::from_min_size(head.left_top() + vec2(0.0, 4.0), vec2(40.0, 40.0));
            match &guild {
                Some(g) => gicon(ui, g, tile),
                None => {
                    ui.painter().rect_filled(tile, 10, pal().bg_mod_selected);
                }
            }
            ui.painter().text(pos2(head.left() + 56.0, head.center().y - 10.0), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary);
            gem(ui, pos2(head.left() + 62.0, head.center().y + 12.0), 14.0);
            ui.painter().text(pos2(head.left() + 74.0, head.center().y + 12.0), Align2::LEFT_CENTER, levels(boosts, tier), medium(12.0), pal().header_secondary);
            let mut c = ui.new_child(UiBuilder::new().max_rect(head).layout(Layout::right_to_left(Align::Center)));
            if small(&mut c, "Go to server", Look::Secondary).clicked() {
                if let Ok(g) = gid.parse::<Gid>() {
                    app.settings_page = None;
                    app.select_guild(Some(g));
                }
            }
            for s in mine {
                ui.add_space(12.0);
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, faint(51)));
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
                gem(ui, pos2(row.left() + 16.0, row.center().y), 20.0);
                let text = match tier {
                    0 => format!("This Boost is helping {name} get to Level 1"),
                    t => format!("This Boost helped unlock {} soundboard slots for everyone", SOUNDBOARD[(t.min(3) - 1) as usize]),
                };
                ui.painter().text(pos2(row.left() + 40.0, row.center().y), Align2::LEFT_CENTER, text, reg(14.0), pal().text_normal);
                let wait = s.cooldown.max(global);
                let mut c = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)));
                let more = crate::ui::icon_button(&mut c, Icon::More, 24.0, "More Options");
                mark(ui, "billing:boost_menu", more.rect);
                let Some((_, sub)) = &s.sub else { continue };
                egui::Popup::menu(&more).width(200.0).show(|ui| {
                    if wait > t {
                        ui.label(RichText::new(format!("On cooldown until {}", long(&epoch_iso(wait)))).font(reg(14.0)).color(pal().text_muted));
                    } else {
                        if crate::ui::menu_item(ui, None, "Transfer Boost", false) {
                            pick = Some(Pick { slot: s.id.clone(), from: Some((gid.clone(), sub.clone())) });
                        }
                        if crate::ui::menu_item(ui, None, "Remove Boost", true) {
                            remove = Some((gid.clone(), sub.clone()));
                        }
                    }
                });
                if wait > t {
                    ui.painter().text(pos2(row.right() - 36.0, row.center().y), Align2::RIGHT_CENTER, format!("Cooldown ends {}", long(&epoch_iso(wait))), reg(12.0), pal().text_muted);
                }
            }
        });
    }
    for s in &free {
        let wait = s.cooldown.max(global);
        let blocked = wait > t;
        card(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
            gem(ui, pos2(head.left() + 24.0, head.center().y), 36.0);
            ui.painter().text(pos2(head.left() + 64.0, head.center().y - 11.0), Align2::LEFT_CENTER, "Your Boost is waiting!", semi(18.0), pal().header_primary);
            let sub = if blocked { format!("You can use this Boost again on {}", long(&epoch_iso(wait))) } else { "Just waiting here until you choose a server to give it to!".to_string() };
            ui.painter().text(pos2(head.left() + 64.0, head.center().y + 12.0), Align2::LEFT_CENTER, sub, reg(12.0), pal().header_secondary);
            let b = Rect::from_center_size(pos2(head.right() - 80.0, head.center().y), vec2(160.0, 40.0));
            if solid(ui, b, "Boost any server", if blocked { faint(61) } else { BLURPLE }, Color32::WHITE) && !blocked {
                pick = Some(Pick { slot: s.id.clone(), from: None });
            }
            ui.add_space(12.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, faint(51)));
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            gem(ui, pos2(row.left() + 12.0, row.center().y), 18.0);
            ui.painter().text(pos2(row.left() + 32.0, row.center().y), Align2::LEFT_CENTER, "Looks like this Boost could use a new home!", reg(14.0), pal().text_normal);
        });
    }
    if applied.is_empty() && free.is_empty() {
        super::super::note(ui, "You don't have any Boosts yet.");
    }

    let mine: Vec<String> = applied.iter().map(|(g, _)| g.clone()).collect();
    let suggested: Vec<Gid> = guilds(app, &mine).into_iter().take(3).collect();
    if !suggested.is_empty() {
        super::super::sub(ui, "These servers could use a Boost");
        ui.add_space(16.0);
        for gid in suggested {
            let Some(g) = app.store.guilds.get(&gid).cloned() else { continue };
            card(ui, |ui| {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                let tile = Rect::from_min_size(row.left_top() + vec2(0.0, 4.0), vec2(40.0, 40.0));
                gicon(ui, &g, tile);
                ui.painter().text(pos2(row.left() + 56.0, row.center().y - 10.0), Align2::LEFT_CENTER, &g.name, medium(16.0), pal().header_primary);
                gem(ui, pos2(row.left() + 62.0, row.center().y + 12.0), 14.0);
                let text = levels(g.boosts, g.tier);
                ui.painter().text(pos2(row.left() + 74.0, row.center().y + 12.0), Align2::LEFT_CENTER, text, medium(12.0), pal().header_secondary);
                let b = Rect::from_center_size(pos2(row.right() - 70.0, row.center().y), vec2(140.0, 40.0));
                if solid(ui, b, "Boost this server", BLURPLE, Color32::WHITE) {
                    match &ready {
                        Some(slot) => {
                            act(app, ui, vec![apply(slot, &gid.to_string())]);
                        }
                        None => app.open_buy(crate::ui::buy::What::Boost(gid)),
                    }
                }
            });
        }
    }

    h2(ui, "Perks");
    perks(ui);
    h2(ui, "Give your community a Boost & be recognized");
    let half = (ui.available_width() - 16.0) / 2.0;
    for row in RECOGNITION.chunks(2) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 112.0), Sense::hover());
        for (i, (icon, text)) in row.iter().enumerate() {
            let tile = Rect::from_min_size(pos2(r.left() + i as f32 * (half + 16.0), r.top()), vec2(half, 112.0));
            ui.painter().rect(tile, CornerRadius::same(12), pal().bg_primary, Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
            icons::draw(ui.painter(), *icon, Rect::from_center_size(tile.center() - vec2(0.0, 18.0), vec2(40.0, 40.0)), if *icon == Icon::Gem { MAGENTA } else { pal().header_primary });
            ui.painter().text(tile.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, *text, medium(13.0), pal().header_secondary);
        }
        ui.add_space(16.0);
    }
    h2(ui, "Boosting FAQ");
    let mut open: Vec<usize> = get(ui, "faq");
    for (i, (q, a)) in FAQ.iter().enumerate() {
        let on = open.contains(&i);
        let body = on.then(|| ui.painter().layout(a.to_string(), reg(14.0), pal().header_secondary, ui.available_width() - 32.0));
        let h = 54.0 + body.as_ref().map_or(0.0, |g| g.size().y + 16.0);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
        ui.painter().rect(r, CornerRadius::same(8), crate::ui::motion::mix(pal().bg_primary, faint(20), crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        ui.painter().text(pos2(r.left() + 16.0, r.top() + 27.0), Align2::LEFT_CENTER, *q, medium(16.0), pal().header_primary);
        icons::draw(ui.painter(), if on { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() - 24.0, r.top() + 27.0), vec2(20.0, 20.0)), pal().header_primary);
        if let Some(g) = body {
            ui.painter().galley(pos2(r.left() + 16.0, r.top() + 54.0), g, pal().header_secondary);
        }
        mark(ui, &format!("billing:faq{i}"), r);
        if resp.clicked() {
            if on {
                open.retain(|x| *x != i);
            } else {
                open.push(i);
            }
        }
        ui.add_space(8.0);
    }
    put(ui, "faq", open);

    put(ui, "remove", remove.clone());
    if let Some((gid, sub)) = remove {
        let name = gid.parse::<Gid>().ok().and_then(|g| app.store.guilds.get(&g)).map_or("this server".to_string(), |g| g.name.clone());
        let body = format!("This Boost will stop powering {name} and its perks will be taken away. You won't be able to move it again for 7 days.");
        match confirm(ui.ctx(), "remove_boost", "Remove Boost", &body, "Remove Boost", true) {
            Some(true) => {
                put(ui, "remove", None::<(String, String)>);
                act(app, ui, vec![("DELETE", format!("/guilds/{gid}/premium/subscriptions/{sub}"), None)]);
            }
            Some(false) => put(ui, "remove", None::<(String, String)>),
            None => {}
        }
    }
    if let Some(p) = pick.clone() {
        let (steps, done) = picker(app, ui, &p);
        if let Some(steps) = steps {
            act(app, ui, steps);
        }
        pick = if done { None } else { Some(p) };
    }
    put(ui, "pick", pick);
    false
}

fn epoch_iso(t: i64) -> String {
    let days = t.div_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

fn perks(ui: &mut Ui) {
    let w = ui.available_width();
    let col = (w - 240.0) / 3.0;
    let (head, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
    for (i, (name, need)) in [("Level 1", 2), ("Level 2", 7), ("Level 3", 14)].iter().enumerate() {
        let x = head.left() + 240.0 + col * (i as f32 + 0.5);
        ui.painter().text(pos2(x, head.center().y - 10.0), Align2::CENTER_CENTER, *name, semi(16.0), pal().header_primary);
        gem(ui, pos2(x - 26.0, head.center().y + 12.0), 14.0);
        ui.painter().text(pos2(x - 14.0, head.center().y + 12.0), Align2::LEFT_CENTER, format!("{need} Boosts"), medium(12.0), pal().header_secondary);
    }
    ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, faint(51)));
    for (name, vals) in &PERKS {
        let (r, _) = ui.allocate_exact_size(vec2(w, 62.0), Sense::hover());
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, *name, reg(16.0), pal().header_secondary);
        for (i, v) in vals.iter().enumerate() {
            let c = pos2(r.left() + 240.0 + col * (i as f32 + 0.5), r.center().y);
            match *v {
                "yes" => icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(c, vec2(24.0, 24.0)), pal().header_primary),
                "no" => icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(c, vec2(20.0, 20.0)), pal().text_muted),
                t => {
                    ui.painter().text(c, Align2::CENTER_CENTER, t, semi(16.0), pal().header_primary);
                }
            }
        }
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, faint(51)));
    }
}

fn picker(app: &mut App, ui: &mut Ui, p: &Pick) -> (Option<Vec<Step>>, bool) {
    let skip: Vec<String> = p.from.iter().map(|f| f.0.clone()).collect();
    let list = guilds(app, &skip);
    let mut steps = None;
    let mut close = false;
    let head = if p.from.is_some() { "Transfer Server Boost?" } else { "Boost any server" };
    let sub = if p.from.is_some() { "Select a server to transfer to" } else { "Select a server to give this Boost to" };
    let go = if p.from.is_some() { "Transfer here" } else { "Boost" };
    let r = crate::ui::motion::Modal::new(Id::new("billing_boost_pick")).frame(Frame::new().fill(pal().bg_secondary).corner_radius(8)).show(ui.ctx(), |ui| {
        ui.set_width(440.0);
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(408.0);
            ui.label(RichText::new(head).font(semi(20.0)).color(pal().header_primary));
            ui.label(RichText::new(sub).font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(12.0);
            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                for gid in &list {
                    let Some(g) = app.store.guilds.get(gid) else { continue };
                    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                    gicon(ui, g, Rect::from_min_size(row.left_top() + vec2(0.0, 4.0), vec2(40.0, 40.0)));
                    ui.painter().text(pos2(row.left() + 52.0, row.center().y - 8.0), Align2::LEFT_CENTER, &g.name, medium(16.0), pal().header_primary);
                    let text = levels(g.boosts, g.tier);
                    ui.painter().text(pos2(row.left() + 52.0, row.center().y + 12.0), Align2::LEFT_CENTER, text, reg(12.0), pal().header_secondary);
                    let b = Rect::from_center_size(pos2(row.right() - 56.0, row.center().y), vec2(112.0, 32.0));
                    if solid(ui, b, go, BLURPLE, Color32::WHITE) {
                        let mut s = Vec::new();
                        if let Some((og, osub)) = &p.from {
                            s.push(("DELETE", format!("/guilds/{og}/premium/subscriptions/{osub}"), None));
                        }
                        s.push(apply(&p.slot, &gid.to_string()));
                        steps = Some(s);
                    }
                }
            });
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Label::new(RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                    close = true;
                }
            });
        });
    });
    (steps.clone(), close || steps.is_some() || r.should_close())
}
