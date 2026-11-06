//! Discord's Quests hub: the quests on offer as cards, the ones claimed, and the Orbs balance.
//! Accepting a quest and claiming its reward go to discord; playing, streaming or watching one
//! through is how progress is made, and that stays outside the client.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, text};
use crate::net::Tag;
use crate::theme::*;

const HOME: u64 = 11;

#[derive(Default)]
pub struct Quests {
    // 0 All Quests, 1 Claimed Quests
    pub tab: u8,
    pub offered: Option<Vec<Value>>,
    pub claimed: Option<Vec<Value>>,
    pub orbs: Option<u64>,
    // 0 suggested, 1 most recent, 2 expiring soon
    pub sort: u8,
    // quest types hidden by the filter: watch, play, stream
    pub hide: [bool; 3],
    // quests with a request out
    pub busy: Vec<String>,
}

impl App {
    pub fn open_quests(&mut self) {
        self.quests = Some(Quests::default());
        self.ui.guild = None;
        self.directory = None;
        self.friends = false;
        self.fetch_quests();
    }

    fn fetch_quests(&mut self) {
        self.rest("GET", "/quests/@me".into(), None, Tag::Quests(0));
        self.rest("GET", "/quests/@me/claimed".into(), None, Tag::Quests(1));
        self.rest("GET", "/users/@me/virtual-currency/balance".into(), None, Tag::Quests(2));
    }
}

pub fn loaded(app: &mut App, part: u8, v: &Value) {
    if part == 3 {
        if let Some(q) = &mut app.quests {
            q.busy.clear();
        }
        return app.fetch_quests();
    }
    let Some(q) = &mut app.quests else { return };
    let list = || v["quests"].as_array().cloned().unwrap_or_default();
    match part {
        0 => q.offered = Some(list()),
        1 => q.claimed = Some(list()),
        _ => q.orbs = v["balance"].as_u64(),
    }
}

fn config(q: &Value) -> &Value {
    &q["config"]
}

fn task(q: &Value) -> Option<(&str, &Value)> {
    config(q)["task_config_v2"]["tasks"].as_object()?.iter().next().map(|(k, v)| (k.as_str(), v))
}

/// 0 watch, 1 play, 2 stream.
fn kind(q: &Value) -> u8 {
    match task(q).map(|t| t.0) {
        Some(t) if t.starts_with("WATCH") => 0,
        Some("STREAM_ON_DESKTOP") => 2,
        _ => 1,
    }
}

fn reward(q: &Value) -> String {
    let r = &config(q)["rewards_config"]["rewards"][0];
    if r["type"].as_u64() == Some(4) {
        return format!("{} Orbs", r["orb_quantity"].as_u64().unwrap_or(0));
    }
    let name = r["messages"]["name_with_article"].as_str().or(r["messages"]["name"].as_str()).or(r["name"].as_str()).unwrap_or("a reward");
    name.to_string()
}

fn describe(q: &Value) -> String {
    let reward = reward(q);
    let game = config(q)["messages"]["game_title"].as_str().unwrap_or_default();
    let minutes = task(q).and_then(|t| t.1["target"].as_u64()).unwrap_or(900) / 60;
    match kind(q) {
        0 => format!("Watch the video to win {reward}!"),
        2 => format!("Stream {game} for {minutes} minutes and win {reward}."),
        _ => format!("Play {game} for {minutes} minutes and win {reward}."),
    }
}

fn colour(v: &Value, fallback: Color32) -> Color32 {
    let Some(h) = v.as_str().map(|s| s.trim_start_matches('#')) else { return fallback };
    u32::from_str_radix(h, 16).map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).unwrap_or(fallback)
}

fn date(v: &Value, fmt: &str) -> String {
    v.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Local).format(fmt).to_string()).unwrap_or_default()
}

fn asset(q: &Value, name: &str) -> Option<String> {
    let file = config(q)["assets"][name].as_str().filter(|f| !f.is_empty())?;
    Some(format!("{}/quests/{}/{file}", crate::api::cdn(), q["id"].as_str().unwrap_or_default()))
}

/// How far along it is: 0 to 1, from the task's progress against its target.
fn progress(q: &Value) -> f32 {
    let Some((name, t)) = task(q) else { return 0.0 };
    let done = q["user_status"]["progress"][name]["value"].as_f64().unwrap_or(0.0);
    let target = t["target"].as_f64().unwrap_or(1.0).max(1.0);
    (done / target).clamp(0.0, 1.0) as f32
}

enum Act {
    Enroll(String),
    Claim(String),
    Copy(String),
    Open(&'static str),
}

fn plate(ui: &mut Ui, r: Rect, label: &str, primary: bool, on: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(("quest_btn", label, r.min.x as i32, r.min.y as i32)), if on { Sense::click() } else { Sense::hover() });
    super::settings::mark(ui, &format!("quest:{label}"), r);
    let fade = if on { 1.0 } else { 0.5 };
    let fill = if primary {
        if resp.hovered() && on { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE }
    } else if resp.hovered() && on {
        Color32::from_rgba_unmultiplied(151, 151, 159, 51)
    } else {
        Color32::from_rgba_unmultiplied(151, 151, 159, 31)
    };
    ui.painter().rect_filled(r, CornerRadius::same(8), fill.gamma_multiply(fade));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(14.0), if primary { Color32::WHITE } else { pal().header_primary }.gamma_multiply(fade));
    if resp.hovered() && on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn card(ui: &mut Ui, r: Rect, q: &Value, claimed: bool, busy: bool, acts: &mut Vec<Act>) {
    let id = q["id"].as_str().unwrap_or_default().to_string();
    let cfg = config(q);
    let accent = colour(&cfg["colors"]["primary"], BLURPLE);
    let edge = colour(&cfg["colors"]["secondary"], Color32::from_rgb(0x40, 0x4a, 0x7b)).gamma_multiply(0.7);
    ui.painter().rect_filled(r, CornerRadius::same(10), pal().bg_floating);
    let banner = Rect::from_min_size(r.min, vec2(r.width(), 150.0));
    let top = CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 };
    ui.painter().rect_filled(banner, top, accent.gamma_multiply(0.35));
    if let Some(url) = asset(q, "hero").or_else(|| asset(q, "quest_bar_hero")) {
        egui::Image::from_uri(url).corner_radius(top).show_loading_spinner(false).paint_at(ui, banner);
    }
    let shade = Rect::from_min_max(pos2(banner.left(), banner.bottom() - 56.0), banner.max);
    ui.painter().rect_filled(shade, CornerRadius::ZERO, Color32::from_black_alpha(110));
    let status = &q["user_status"];
    let (enrolled, done) = (status["enrolled_at"].is_string(), status["completed_at"].is_string());
    if let Some(url) = asset(q, "game_tile") {
        text::image(ui.painter(), &url, Rect::from_min_size(pos2(r.left() + 13.0, banner.bottom() - 40.0), vec2(28.0, 28.0)));
    }
    let y = banner.bottom() - 26.0;
    let promoted = ui.painter().text(pos2(r.left() + 49.0, y), Align2::LEFT_CENTER, "Promoted by", medium(14.0), pal().text_muted);
    icons::draw(ui.painter(), Icon::Verified, Rect::from_center_size(pos2(promoted.right() + 14.0, y), vec2(16.0, 16.0)), GREEN);
    let who = cfg["messages"]["game_publisher"].as_str().unwrap_or_default();
    text::paint(ui.painter(), pos2(promoted.right() + 26.0, y), Align2::LEFT_CENTER, who, medium(14.0), pal().header_primary, r.width() - 49.0 - promoted.width() - 130.0);
    let when = if claimed { format!("Claimed {}", date(&status["claimed_at"], "%-m/%-d")) } else { format!("Ends {}", date(&cfg["expires_at"], "%-m/%-d")) };
    ui.painter().text(pos2(r.right() - 13.0, y), Align2::RIGHT_CENTER, when, medium(14.0), pal().text_muted);
    let more = Rect::from_center_size(pos2(r.right() - 24.0, r.top() + 24.0), vec2(28.0, 28.0));
    let mr = ui.interact(more, ui.id().with(("quest_more", &id)), Sense::click());
    ui.painter().circle_filled(more.center(), 14.0, Color32::from_black_alpha(if mr.hovered() { 170 } else { 120 }));
    icons::draw(ui.painter(), Icon::More, Rect::from_center_size(more.center(), vec2(16.0, 16.0)), Color32::WHITE);
    egui::Popup::menu(&mr).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if super::menu_item(ui, Some(Icon::Link), "Copy Quest Link", false) {
            acts.push(Act::Copy(format!("https://discord.com/quests/{id}")));
        }
        if super::menu_item(ui, Some(Icon::Id), "Copy Quest ID", false) {
            ui.ctx().copy_text(id.clone());
        }
    });
    let body = Rect::from_min_max(pos2(r.left(), banner.bottom()), r.max);
    let tile = Rect::from_min_size(pos2(body.left() + 13.0, body.top() + 16.0), vec2(64.0, 64.0));
    if let Some(url) = asset(q, "logotype").or_else(|| asset(q, "game_tile")) {
        text::image(ui.painter(), &url, tile);
    }
    let x = tile.right() + 16.0;
    let w = r.right() - 13.0 - x;
    let name = cfg["messages"]["quest_name"].as_str().unwrap_or_default();
    text::paint(ui.painter(), pos2(x, body.top() + 22.0), Align2::LEFT_CENTER, &name.to_uppercase(), bold(12.0), accent, w);
    text::paint(ui.painter(), pos2(x, body.top() + 44.0), Align2::LEFT_CENTER, &format!("Claim {}", reward(q)), semi(16.0), pal().header_primary, w);
    let d = ui.painter().layout(describe(q), reg(14.0), pal().header_secondary, w);
    ui.painter().galley(pos2(x, body.top() + 62.0), d, pal().header_secondary);
    let foot = body.bottom() - 13.0 - 40.0;
    if enrolled && !done && !claimed {
        let bar = Rect::from_min_size(pos2(x, foot - 14.0), vec2(w, 6.0));
        ui.painter().rect_filled(bar, CornerRadius::same(3), pal().bg_accent);
        let fill = Rect::from_min_size(bar.min, vec2(bar.width() * progress(q), 6.0));
        ui.painter().rect_filled(fill, CornerRadius::same(3), GREEN);
    }
    let btn = Rect::from_min_size(pos2(r.right() - 13.0 - 148.0, foot), vec2(148.0, 40.0));
    if claimed {
        plate(ui, btn, "Claimed", false, false);
    } else if done {
        if plate(ui, btn, "Claim Reward", true, !busy) {
            acts.push(Act::Claim(id.clone()));
        }
    } else if enrolled {
        let label = if kind(q) == 0 { "Watch Video" } else { "In Progress" };
        if plate(ui, btn, label, false, kind(q) == 0) {
            acts.push(Act::Open("https://discord.com/quest-home"));
        }
    } else if plate(ui, btn, "Accept Quest", true, !busy) {
        acts.push(Act::Enroll(id.clone()));
    }
    ui.painter().rect_stroke(r, CornerRadius::same(10), Stroke::new(1.0, edge), egui::StrokeKind::Inside);
}

fn hero(ui: &mut Ui, acts: &mut Vec<Act>) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 420.0), Sense::hover());
    let round = CornerRadius::same(24);
    ui.painter().rect_filled(r, round, Color32::from_rgb(0x0b, 0x0b, 0x1f));
    for (c, rad, a) in [(Color32::from_rgb(0x58, 0x65, 0xf2), 260.0, 70u8), (Color32::from_rgb(0xeb, 0x45, 0x9e), 150.0, 40)] {
        let at = pos2(r.right() - 220.0 - if rad > 200.0 { 0.0 } else { 240.0 }, r.center().y - if rad > 200.0 { 20.0 } else { -120.0 });
        for i in 0..8 {
            ui.painter().with_clip_rect(r).circle_filled(at, rad * (1.0 - i as f32 / 9.0), Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a / 4));
        }
    }
    icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(r.right() - 220.0, r.center().y - 20.0), vec2(150.0, 150.0)), Color32::from_rgb(0xdd, 0xe3, 0xff));
    let x = r.left() + 38.0;
    ui.painter().text(pos2(x, r.bottom() - 190.0), Align2::LEFT_BOTTOM, "INTRODUCING", bold(56.0), Color32::WHITE);
    ui.painter().text(pos2(x, r.bottom() - 140.0), Align2::LEFT_BOTTOM, "DISCORD ORBS", bold(56.0), Color32::WHITE);
    ui.painter().text(pos2(x, r.bottom() - 100.0), Align2::LEFT_CENTER, "Reward Your Play. Earn through Quests. Spend in the Shop.", reg(18.0), Color32::from_rgb(0xdc, 0xdc, 0xdf));
    let a = Rect::from_min_size(pos2(x, r.bottom() - 74.0), vec2(190.0, 46.0));
    if white(ui, a, "Explore Orbs Exclusives", true) {
        acts.push(Act::Open("https://discord.com/shop"));
    }
    let b = Rect::from_min_size(pos2(a.right() + 12.0, a.top()), vec2(160.0, 46.0));
    if white(ui, b, "Discord Orbs Terms", false) {
        acts.push(Act::Open("https://discord.com/terms/orbs"));
    }
}

fn white(ui: &mut Ui, r: Rect, label: &str, solid: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(("hero_btn", label)), Sense::click());
    let fill = if solid { Color32::WHITE } else { Color32::from_black_alpha(if resp.hovered() { 170 } else { 120 }) };
    ui.painter().rect_filled(r, CornerRadius::same(8), if solid && resp.hovered() { Color32::from_gray(0xe6) } else { fill });
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(16.0), if solid { Color32::BLACK } else { Color32::WHITE });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(q) = app.quests.as_ref() else { return };
    let tab = q.tab;
    let orbs = q.orbs;
    let mut set_tab = None;
    egui::Panel::top("quests_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range().expand(16.0), r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            icons::draw(ui.painter(), Icon::Quests, Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(24.0, 24.0)), pal().channel_default);
            let mut x = r.left() + 56.0;
            for (i, name) in ["All Quests", "Claimed Quests"].iter().enumerate() {
                let g = ui.painter().layout_no_wrap(name.to_string(), medium(16.0), pal().header_primary);
                let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x, 48.0));
                let resp = ui.interact(tr, ui.id().with(("quest_tab", i)), Sense::click());
                super::settings::mark(ui, &format!("quest_tab:{name}"), tr);
                let on = tab == i as u8;
                let col = if on || resp.hovered() { pal().header_primary } else { pal().header_secondary };
                ui.painter().galley(pos2(x, tr.center().y - g.size().y / 2.0), g, col);
                if on {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(x, tr.bottom() - 2.0), vec2(tr.width(), 2.0)), 1, pal().header_primary);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    set_tab = Some(i as u8);
                }
                x = tr.right() + 20.0;
            }
            if let Some(n) = orbs {
                let g = ui.painter().layout_no_wrap(n.to_string(), semi(16.0), pal().header_primary);
                let chip = Rect::from_min_size(pos2(r.right() - 40.0 - g.size().x - 24.0, r.center().y - 16.0), vec2(g.size().x + 64.0, 32.0));
                ui.painter().rect(chip, 8, Color32::from_rgba_unmultiplied(151, 151, 159, 31), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(chip.left() + 20.0, chip.center().y), vec2(20.0, 20.0)), pal().header_primary);
                ui.painter().galley(pos2(chip.left() + 38.0, chip.center().y - g.size().y / 2.0), g, pal().header_primary);
            }
        });
    let mut acts: Vec<Act> = Vec::new();
    let mut sort = None;
    let mut hide = None;
    let (offered, claimed, busy) = {
        let q = app.quests.as_ref().unwrap();
        (q.offered.clone(), q.claimed.clone(), q.busy.clone())
    };
    let (cur_sort, cur_hide) = {
        let q = app.quests.as_ref().unwrap();
        (q.sort, q.hide)
    };
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 32, right: 32, top: 24, bottom: 32 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let list = if tab == 0 { offered.as_ref() } else { claimed.as_ref() };
                if tab == 0 {
                    hero(ui, &mut acts);
                    ui.add_space(40.0);
                }
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                let title = if tab == 0 { "Available Quests" } else { "Claimed Quests" };
                ui.painter().text(hr.left_center(), Align2::LEFT_CENTER, title, medium(20.0), pal().header_primary);
                if tab == 0 {
                    let sort_label = ["Suggested", "Most Recent", "Expiring Soon"][cur_sort as usize];
                    let width = |ui: &Ui, t: &str| ui.painter().layout_no_wrap(t.to_string(), medium(14.0), pal().header_primary).size().x + 44.0;
                    let f = Rect::from_min_size(pos2(hr.right() - width(ui, "Filters"), hr.top() + 4.0), vec2(width(ui, "Filters"), 32.0));
                    let s = Rect::from_min_size(pos2(f.left() - 8.0 - width(ui, sort_label), hr.top() + 4.0), vec2(width(ui, sort_label), 32.0));
                    let sr = chip(ui, s, sort_label, true, "quest_sort");
                    egui::Popup::menu(&sr).gap(4.0).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, n) in ["Suggested", "Most Recent", "Expiring Soon"].iter().enumerate() {
                            if super::menu_row(ui, None, n, None, false, super::Trail::Radio(cur_sort == i as u8)).clicked() {
                                sort = Some(i as u8);
                            }
                        }
                    });
                    let fr = chip(ui, f, "Filters", false, "quest_filters");
                    egui::Popup::menu(&fr).gap(4.0).show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, n) in ["Watch a video", "Play a game", "Stream a game"].iter().enumerate() {
                            if super::menu_row(ui, None, n, None, false, super::Trail::Check(!cur_hide[i])).clicked() {
                                hide = Some(i);
                            }
                        }
                    });
                }
                ui.add_space(12.0);
                let Some(list) = list else {
                    ui.add_space(80.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                    return;
                };
                let mut shown: Vec<&Value> = list.iter().filter(|q| tab == 1 || !cur_hide[kind(q) as usize]).collect();
                match cur_sort {
                    1 if tab == 0 => shown.sort_by(|a, b| config(b)["starts_at"].as_str().cmp(&config(a)["starts_at"].as_str())),
                    2 if tab == 0 => shown.sort_by(|a, b| config(a)["expires_at"].as_str().cmp(&config(b)["expires_at"].as_str())),
                    _ => {}
                }
                if shown.is_empty() {
                    ui.add_space(60.0);
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 80.0), Sense::hover());
                    ui.painter().text(r.center_top() + vec2(0.0, 16.0), Align2::CENTER_CENTER, if tab == 0 { "No Quests Available" } else { "No Claimed Quests" }, semi(24.0), pal().header_primary);
                    if tab == 0 {
                        ui.painter().text(r.center_top() + vec2(0.0, 50.0), Align2::CENTER_CENTER, "Hang tight, adventurer! New quests will be available soon.", reg(16.0), pal().header_secondary);
                    }
                    return;
                }
                let avail = ui.available_width();
                let cols = (((avail + 24.0) / (340.0 + 24.0)).floor() as usize).clamp(1, 3);
                let cw = (avail - (cols - 1) as f32 * 24.0) / cols as f32;
                for row in shown.chunks(cols) {
                    let (strip, _) = ui.allocate_exact_size(vec2(avail, 316.0), Sense::hover());
                    for (i, q) in row.iter().enumerate() {
                        let r = Rect::from_min_size(pos2(strip.left() + i as f32 * (cw + 24.0), strip.top()), vec2(cw, 316.0));
                        let id = q["id"].as_str().unwrap_or_default();
                        card(ui, r, q, tab == 1, busy.iter().any(|b| b == id), &mut acts);
                        super::settings::mark(ui, &format!("quest_card:{id}"), r);
                    }
                    ui.add_space(24.0);
                }
            });
        });
    });
    if let Some(t) = set_tab
        && let Some(q) = &mut app.quests
    {
        q.tab = t;
    }
    if let Some(s) = sort
        && let Some(q) = &mut app.quests
    {
        q.sort = s;
    }
    if let Some(i) = hide
        && let Some(q) = &mut app.quests
    {
        q.hide[i] = !q.hide[i];
    }
    for a in acts {
        match a {
            Act::Enroll(id) => {
                if let Some(q) = &mut app.quests {
                    q.busy.push(id.clone());
                }
                let body = json!({ "location": HOME, "is_targeted": false, "metadata_sealed": null, "traffic_metadata_sealed": null });
                app.rest("POST", format!("/quests/{id}/enroll"), Some(body), Tag::Quests(3));
            }
            Act::Claim(id) => {
                if let Some(q) = &mut app.quests {
                    q.busy.push(id.clone());
                }
                let body = json!({ "platform": 0, "location": HOME, "is_targeted": false, "metadata_sealed": null, "traffic_metadata_sealed": null });
                app.rest("POST", format!("/quests/{id}/claim-reward"), Some(body), Tag::Quests(3));
            }
            Act::Copy(u) => {
                ui.ctx().copy_text(u);
                app.toast("Link copied to clipboard.");
            }
            Act::Open(u) => ui.ctx().open_url(egui::OpenUrl::new_tab(u)),
        }
    }
}

fn chip(ui: &mut Ui, r: Rect, label: &str, caret: bool, key: &'static str) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(key), Sense::click());
    let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let t = ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary);
    let ic = if caret { Icon::ChevronDown } else { Icon::Sort };
    icons::draw(ui.painter(), ic, Rect::from_center_size(pos2(t.right() + 16.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
