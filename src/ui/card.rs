use egui::{Align2, Color32, CornerRadius, OpenUrl, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::Value;

use super::extra::{icon_url, usrbg};
use crate::plugins;
use crate::state::{Id, Store, User};
use crate::theme::*;

/// ViewIcons on a profile card: the picture a click on its avatar or banner opens.
pub fn click(ui: &Ui, top: Rect, user: &User, p: Option<&Value>, avatar: bool) -> Option<String> {
    if !plugins::on("ViewIcons") {
        return None;
    }
    let av = Rect::from_min_size(pos2(top.left() + 16.0, top.top() + 61.0), vec2(80.0, 80.0));
    if avatar {
        let hit = ui.interact(av, ui.id().with("view_avatar"), Sense::click());
        if hit.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if hit.clicked() {
            return Some(match &user.avatar {
                Some(h) => icon_url("avatars", user.id, h),
                None => user.avatar_url(),
            });
        }
    }
    let nitro = p.and_then(|p| p["user"]["banner"].as_str());
    let url = match (nitro, usrbg(user.id)) {
        (Some(h), u) if u.is_none() || plugins::flag("USRBG", "nitroFirst") => icon_url("banners", user.id, h),
        (_, Some(u)) => u,
        (Some(h), None) => icon_url("banners", user.id, h),
        (None, None) => return None,
    };
    let banner = Rect::from_min_max(top.min, pos2(top.right(), av.top()));
    let hit = ui.interact(banner, ui.id().with("view_banner"), Sense::click());
    if hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    hit.clicked().then_some(url)
}

/// ShowConnections: brand tiles for a profile's connected accounts.
pub fn connections(ui: &mut Ui, p: Option<&Value>) {
    if !plugins::on("ShowConnections") {
        return;
    }
    let Some(list) = p.and_then(|p| p["connected_accounts"].as_array()).filter(|l| !l.is_empty()) else { return };
    let size = plugins::num("ShowConnections", "iconSize").clamp(12.0, 64.0) as f32;
    let gap = match plugins::text("ShowConnections", "iconSpacing").as_str() {
        "0" => 2.0,
        "2" => 10.0,
        _ => 5.0,
    };
    ui.add_space(14.0);
    ui.label(RichText::new("Connections").font(semi(12.0)).color(pal().header_primary));
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for c in list {
            let kind = c["type"].as_str().unwrap_or("");
            let name = c["name"].as_str().unwrap_or("");
            let id = c["id"].as_str().unwrap_or("");
            let (brand, label, url): (u32, &str, Option<String>) = match kind {
                "github" => (0x24292f, "GH", Some(format!("https://github.com/{name}"))),
                "twitter" => (0x1d9bf0, "X", Some(format!("https://twitter.com/{name}"))),
                "youtube" => (0xff0000, "YT", Some(format!("https://www.youtube.com/channel/{id}"))),
                "twitch" => (0x9146ff, "TW", Some(format!("https://twitch.tv/{name}"))),
                "steam" => (0x1b2838, "ST", Some(format!("https://steamcommunity.com/profiles/{id}"))),
                "spotify" => (0x1db954, "SP", Some(format!("https://open.spotify.com/user/{id}"))),
                "reddit" => (0xff4500, "RD", Some(format!("https://reddit.com/u/{name}"))),
                "tiktok" => (0x000000, "TT", Some(format!("https://tiktok.com/@{name}"))),
                "bluesky" => (0x1185fe, "BS", Some(format!("https://bsky.app/profile/{name}"))),
                "instagram" => (0xe4405f, "IG", Some(format!("https://instagram.com/{name}"))),
                "xbox" => (0x107c10, "XB", None),
                "playstation" => (0x003791, "PS", None),
                "mastodon" => (0x6364ff, "MA", None),
                "facebook" => (0x1877f2, "FB", None),
                "epicgames" => (0x333333, "EG", None),
                "battlenet" => (0x148eff, "BN", None),
                _ => (0x5865f2, "?", None),
            };
            let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::same((size / 4.0) as u8), role_color(brand));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, bold(size * 0.38), Color32::WHITE);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let verified = if c["verified"].as_bool() == Some(true) { " (verified)" } else { "" };
            if resp.on_hover_text(format!("{name} \u{2014} {kind}{verified}")).clicked()
                && let Some(u) = url
            {
                ui.ctx().open_url(OpenUrl::new_tab(u));
            }
        }
    });
}

/// MutualGroupDMs: the group chats we share with someone.
pub fn mutual_groups(store: &Store, uid: Id) -> Vec<(Id, String)> {
    if !plugins::on("MutualGroupDMs") {
        return Vec::new();
    }
    store.dms.iter().filter_map(|c| store.channels.get(c)).filter(|c| c.kind == 3 && c.recipients.contains(&uid)).map(|c| (c.id, super::channel_title(store, c.id))).collect()
}

/// UserVoiceShow on a profile: where they are, if they're in a call.
pub fn voice_line(store: &Store, uid: Id) -> Option<String> {
    if !plugins::flag("UserVoiceShow", "showInUserProfileModal") {
        return None;
    }
    store.guilds.values().find_map(|g| {
        let c = store.channels.get(g.voice.get(&uid)?)?;
        Some(format!("In voice: {} \u{b7} {}", c.name, g.name))
    })
}

/// A row of small facts at the top of a card's body: the call someone is in, and our shared groups.
pub fn extras(ui: &mut Ui, store: &Store, uid: Id, groups: &[(Id, String)], open: &mut Option<Id>) {
    if let Some(l) = voice_line(store, uid) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            super::icons::draw(ui.painter(), super::icons::Icon::Speaker, r, GREEN);
            ui.label(RichText::new(l).font(medium(14.0)).color(pal().header_primary));
        });
    }
    if !groups.is_empty() {
        ui.add_space(14.0);
        ui.label(RichText::new(format!("Mutual Group DMs \u{2014} {}", groups.len())).font(semi(12.0)).color(pal().header_primary));
        ui.add_space(6.0);
        for (c, title) in groups.iter().take(5) {
            let (r, resp) = ui.allocate_exact_size(vec2(268.0, 24.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            super::icons::draw(ui.painter(), super::icons::Icon::People, Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(14.0, 14.0)), pal().header_secondary);
            super::text::paint(ui.painter(), pos2(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, title, reg(14.0), pal().header_primary, r.width() - 30.0);
            if resp.clicked() {
                *open = Some(*c);
            }
        }
    }
}
