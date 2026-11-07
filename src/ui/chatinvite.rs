//! The card under a message with an invite link: the server's banner, icon, counts and a Join button.

use std::collections::{HashMap, HashSet};

use chrono::TimeZone;
use egui::{Align2, Color32, CornerRadius, Rect, Sense, Shape, Stroke, StrokeKind, Ui, epaint::Vertex, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, Store, sid};
use crate::theme::*;

const WIDTH: f32 = 302.0;
const JOIN: Color32 = Color32::from_rgb(0, 133, 69);

pub struct Invite {
    guild: Id,
    name: String,
    icon: Option<String>,
    banner: Option<String>,
    color: Option<Color32>,
    online: u64,
    members: u64,
    desc: String,
    badge: Option<Icon>,
}

enum Entry {
    Asked,
    Found(Invite),
    Gone,
}

#[derive(Default)]
pub struct Cache {
    map: HashMap<String, Entry>,
    joining: HashSet<String>,
}

pub enum Go {
    Ask(String),
    Join(String),
    Open(Id),
}

/// The invite codes a message links to, once each, in order.
pub fn codes(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for host in ["discord.gg/", "discord.com/invite/", "discordapp.com/invite/"] {
        for (at, _) in text.match_indices(host) {
            let rest = &text[at + host.len()..];
            let code: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
            if !code.is_empty() && !out.contains(&code) {
                out.push(code);
            }
        }
    }
    out
}

impl Cache {
    pub fn got(&mut self, code: &str, v: &serde_json::Value) {
        let g = &v["guild"];
        let p = &v["profile"];
        let pick = |a: &serde_json::Value, b: &serde_json::Value| a.as_str().or(b.as_str()).map(str::to_string);
        let guild = sid(&g["id"]);
        let name = pick(&p["name"], &g["name"]);
        let (Some(name), true) = (name, guild != 0) else {
            self.map.insert(code.to_string(), Entry::Gone);
            return;
        };
        let cdn = crate::api::cdn();
        let features: Vec<&str> = p["features"].as_array().or(g["features"].as_array()).into_iter().flatten().filter_map(|f| f.as_str()).collect();
        let badge = if features.contains(&"PARTNERED") {
            Some(Icon::BadgePartner)
        } else if features.contains(&"VERIFIED") {
            Some(Icon::Verified)
        } else if features.contains(&"COMMUNITY") {
            Some(Icon::BadgeCommunity)
        } else {
            None
        };
        let color = p["brand_color_primary"].as_str().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()).map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
        let count = |a: &serde_json::Value, b: &serde_json::Value| a.as_u64().or(b.as_u64()).unwrap_or(0);
        let inv = Invite {
            guild,
            name,
            icon: pick(&p["icon_hash"], &g["icon"]).map(|h| format!("{cdn}/icons/{guild}/{h}.png?size=128")),
            banner: pick(&p["banner_hash"], &g["banner"]).map(|h| format!("{cdn}/banners/{guild}/{h}.png?size=480")),
            color,
            online: count(&p["online_count"], &v["approximate_presence_count"]),
            members: count(&p["member_count"], &v["approximate_member_count"]),
            desc: pick(&p["description"], &g["description"]).unwrap_or_default(),
            badge,
        };
        self.map.insert(code.to_string(), Entry::Found(inv));
    }

    pub fn failed(&mut self, code: &str) {
        self.map.insert(code.to_string(), Entry::Gone);
    }

    pub fn joined(&mut self, code: &str) {
        self.joining.remove(code);
    }

    pub fn joining(&mut self, code: &str) {
        self.joining.insert(code.to_string());
    }
}

impl App {
    pub fn chat_invite(&mut self, go: Go) {
        match go {
            Go::Ask(code) => self.rest("GET", format!("/invites/{code}?with_counts=true&with_expiration=true&with_permissions=true"), None, Tag::ChatInvite(code)),
            Go::Join(code) => {
                self.chat_invites.joining(&code);
                let session = self.store.session.clone();
                self.rest_in("POST", format!("/invites/{code}"), Some(serde_json::json!({ "session_id": session })), "Invite Button Embed", Tag::ChatJoin(code));
            }
            Go::Open(g) => self.select_guild(Some(g)),
        }
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn est(guild: Id) -> String {
    let ms = (guild >> 22) as i64 + 1_420_070_400_000;
    chrono::Local.timestamp_millis_opt(ms).single().map(|d| format!("Est. {}", d.format("%b %Y"))).unwrap_or_default()
}

pub fn card(ui: &mut Ui, store: &Store, cache: &mut Cache, code: &str, own: bool) -> Option<Go> {
    match cache.map.get(code) {
        None => {
            cache.map.insert(code.to_string(), Entry::Asked);
            Some(Go::Ask(code.to_string()))
        }
        Some(Entry::Asked) => {
            ui.ctx().request_repaint_after_secs(0.5);
            let (r, _) = ui.allocate_exact_size(vec2(WIDTH, 120.0), Sense::hover());
            frame(ui, r);
            super::motion::paint_spinner(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().text_muted);
            None
        }
        Some(Entry::Gone) => {
            let r = gone(ui, own);
            super::settings::mark(ui, &format!("invite:{code}:gone"), r);
            None
        }
        Some(Entry::Found(inv)) => {
            let member = store.guilds.contains_key(&inv.guild);
            let joining = cache.joining.contains(code) && !member;
            live(ui, inv, member, joining, code)
        }
    }
}

fn frame(ui: &Ui, r: Rect) {
    ui.painter().rect_filled(r, CornerRadius::same(16), pal().bg_secondary);
    ui.painter().rect_stroke(r, CornerRadius::same(16), Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
}

fn gone(ui: &mut Ui, own: bool) -> Rect {
    let head = if own { "You sent an invite, but" } else { "You received an invite, but" };
    let sub = if own { "Try sending a new invite!" } else { "Ask for a new invite" };
    let (r, _) = ui.allocate_exact_size(vec2(WIDTH, 100.0), Sense::hover());
    frame(ui, r);
    let p = ui.painter();
    super::text::paint(p, pos2(r.left() + 16.0, r.top() + 22.0), Align2::LEFT_CENTER, head, medium(12.0), pal().text_muted, WIDTH - 32.0);
    let icon = Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 40.0), vec2(40.0, 40.0));
    p.rect_filled(icon, CornerRadius::same(12), pal().bg_accent);
    p.text(icon.center(), Align2::CENTER_CENTER, "?", semi(18.0), pal().text_muted);
    let x = icon.right() + 12.0;
    super::text::paint(p, pos2(x, r.top() + 50.0), Align2::LEFT_CENTER, "Invalid Invite", medium(16.0), pal().header_primary, r.right() - x - 16.0);
    super::text::paint(p, pos2(x, r.top() + 70.0), Align2::LEFT_CENTER, sub, reg(14.0), pal().text_muted, r.right() - x - 16.0);
    r
}

fn live(ui: &mut Ui, inv: &Invite, member: bool, joining: bool, code: &str) -> Option<Go> {
    let font = reg(14.0);
    let desc = (!inv.desc.is_empty()).then(|| {
        let mut job = egui::text::LayoutJob::simple(inv.desc.clone(), font.clone(), pal().text_muted, WIDTH - 34.0);
        job.wrap.max_rows = 5;
        ui.painter().layout_job(job)
    });
    let footer = desc.as_ref().map_or(189.0, |g| 197.0 + g.size().y);
    let (r, _) = ui.allocate_exact_size(vec2(WIDTH, footer + 49.0), Sense::hover());
    let p = ui.painter().clone();
    frame(ui, r);
    let banner = Rect::from_min_size(r.min + vec2(1.0, 1.0), vec2(WIDTH - 2.0, 72.0));
    let top = CornerRadius { nw: 15, ne: 15, sw: 0, se: 0 };
    match &inv.banner {
        Some(url) => {
            p.rect_filled(banner, top, pal().bg_accent);
            crate::gif::paint_still(ui, url, banner, top, false);
        }
        None => {
            let c = inv.color.unwrap_or(pal().bg_accent);
            let dim = super::motion::mix(c, pal().bg_secondary, 0.55);
            p.rect_filled(banner, top, c);
            let mut mesh = egui::Mesh::default();
            for (pos, col) in [(banner.left_top(), dim), (banner.right_top(), dim), (banner.right_bottom(), c), (banner.left_bottom(), c)] {
                mesh.vertices.push(Vertex { pos, uv: egui::epaint::WHITE_UV, color: col });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            p.add(Shape::mesh(mesh));
        }
    }
    let wrap = Rect::from_min_size(r.min + vec2(17.0, 39.0), vec2(70.0, 70.0));
    p.rect_filled(wrap, CornerRadius::same(20), Color32::from_rgb(0x17, 0x18, 0x1b));
    let icon = wrap.shrink(3.0);
    match &inv.icon {
        Some(url) => crate::gif::paint_still(ui, url, icon, CornerRadius::same(17), false),
        None => {
            p.rect_filled(icon, CornerRadius::same(17), pal().bg_accent);
            let acr: String = inv.name.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect();
            p.text(icon.center(), Align2::CENTER_CENTER, acr, semi(22.0), pal().text_normal);
        }
    }
    let x = r.left() + 17.0;
    let w = WIDTH - 34.0;
    let name = super::text::paint(&p, pos2(x, r.top() + 127.0), Align2::LEFT_CENTER, &inv.name, semi(16.0), pal().header_primary, w - 22.0);
    if let Some(b) = inv.badge {
        icons::draw(&p, b, Rect::from_min_size(pos2(name.right() + 4.0, r.top() + 118.0), vec2(18.0, 18.0)), Color32::WHITE);
    }
    let dot = |at: f32, col: Color32| p.circle_filled(pos2(x + at + 4.0, r.top() + 150.0), 4.0, col);
    dot(0.0, pal().positive);
    let online = format!("{} Online", thousands(inv.online));
    let a = super::text::paint(&p, pos2(x + 12.0, r.top() + 146.0), Align2::LEFT_CENTER, &online, font.clone(), pal().text_muted, w);
    let at = a.right() + 8.0 - x;
    dot(at, pal().text_muted);
    super::text::paint(&p, pos2(x + at + 12.0, r.top() + 146.0), Align2::LEFT_CENTER, &format!("{} Members", thousands(inv.members)), font.clone(), pal().text_muted, w);
    super::text::paint(&p, pos2(x, r.top() + 164.0), Align2::LEFT_CENTER, &est(inv.guild), font, pal().text_muted, w);
    if let Some(g) = desc {
        p.galley(pos2(x, r.top() + 181.0), g, pal().text_muted);
    }
    let b = Rect::from_min_size(pos2(x, r.bottom() - 49.0), vec2(w, 32.0));
    let resp = ui.interact(b, egui::Id::new(("invite_join", code)), Sense::click());
    super::settings::mark(ui, &format!("invite:{code}:join"), b);
    let hot = resp.hovered() && !joining;
    let fill = if member { pal().control } else { super::motion::mix(JOIN, JOIN.gamma_multiply(0.85), super::motion::hot(ui.ctx(), &resp)) };
    p.rect_filled(b, CornerRadius::same(8), if member && hot { pal().control_hover } else { fill });
    p.rect_stroke(b, CornerRadius::same(8), Stroke::new(1.0, Color32::from_white_alpha(20)), StrokeKind::Inside);
    let label = if member { "Joined" } else if joining { "Joining..." } else { "Join" };
    p.text(b.center(), Align2::CENTER_CENTER, label, medium(14.0), Color32::WHITE);
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        return if member { Some(Go::Open(inv.guild)) } else if joining { None } else { Some(Go::Join(code.to_string())) };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{codes, thousands};

    #[test]
    fn invite_codes_in_a_message() {
        assert_eq!(codes("join https://discord.gg/abc-1 or discord.com/invite/XyZ9, https://discord.gg/abc-1"), vec!["abc-1", "XyZ9"]);
        assert!(codes("https://example.com/discord.gg").is_empty());
    }

    #[test]
    fn counts_get_commas() {
        assert_eq!(thousands(4668), "4,668");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
