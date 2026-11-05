//! The server behind the link in the Join a Server dialog: its name, icon and member counts.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar};
use crate::net::Tag;
use crate::theme::*;

const SETTLE_SECS: f64 = 0.4;

struct Server {
    name: String,
    icon: Option<String>,
    online: u64,
    members: u64,
}

#[derive(Default)]
pub struct Card {
    code: String,
    since: f64,
    asked: bool,
    server: Option<Server>,
}

/// An invite's code from a pasted link or a bare code; anything else isn't worth asking about.
pub fn code_of(text: &str) -> &str {
    let code = text.trim().trim_end_matches('/').rsplit('/').next().unwrap_or("");
    code.split(['?', '#']).next().filter(|c| c.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).unwrap_or("")
}

pub fn tick(app: &mut App, ctx: &egui::Context) {
    let Some(text) = &app.invite else {
        app.invite_card = Card::default();
        return;
    };
    let code = code_of(text).to_string();
    let now = ctx.input(|i| i.time);
    if code != app.invite_card.code {
        app.invite_card = Card { code: code.clone(), since: now, ..Default::default() };
    }
    if code.is_empty() || app.invite_card.asked {
        return;
    }
    let waited = now - app.invite_card.since;
    if waited < SETTLE_SECS {
        ctx.request_repaint_after_secs((SETTLE_SECS - waited) as f32);
        return;
    }
    app.invite_card.asked = true;
    app.rest("GET", format!("/invites/{code}?with_counts=true"), None, Tag::InvitePreview(code));
}

impl Card {
    pub fn loaded(&self) -> bool {
        self.server.is_some()
    }

    pub fn got(&mut self, code: &str, v: &serde_json::Value) {
        if code != self.code {
            return;
        }
        let g = &v["guild"];
        self.server = g["name"].as_str().map(|name| Server {
            name: name.to_string(),
            icon: g["icon"].as_str().map(|h| format!("{}/icons/{}/{h}.png?size=96", crate::api::cdn(), g["id"].as_str().unwrap_or(""))),
            online: v["approximate_presence_count"].as_u64().unwrap_or(0),
            members: v["approximate_member_count"].as_u64().unwrap_or(0),
        });
    }

    pub fn draw(&self, ui: &mut Ui) {
        let Some(s) = &self.server else { return };
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_tertiary);
        let icon = Rect::from_center_size(pos2(r.left() + 36.0, r.center().y), vec2(48.0, 48.0));
        match &s.icon {
            Some(url) => avatar(ui, icon, url),
            None => {
                ui.painter().circle_filled(icon.center(), 24.0, BLURPLE);
                let letter: String = s.name.chars().take(1).collect();
                ui.painter().text(icon.center(), Align2::CENTER_CENTER, letter, medium(18.0), Color32::WHITE);
            }
        }
        let x = icon.right() + 12.0;
        let width = r.right() - x - 12.0;
        super::text::paint(ui.painter(), pos2(x, r.center().y - 10.0), Align2::LEFT_CENTER, &s.name, semi(16.0), pal().header_primary, width);
        let counts = format!("{} Online  ·  {} Members", s.online, s.members);
        super::text::paint(ui.painter(), pos2(x, r.center().y + 12.0), Align2::LEFT_CENTER, &counts, reg(14.0), pal().header_secondary, width);
        ui.add_space(16.0);
    }
}

#[cfg(test)]
mod tests {
    use super::code_of;

    #[test]
    fn code_from_links_and_bare_codes() {
        assert_eq!(code_of("https://discord.gg/e2eRpc1"), "e2eRpc1");
        assert_eq!(code_of(" https://discord.com/invite/abc-1?event=1 "), "abc-1");
        assert_eq!(code_of("abc-1"), "abc-1");
        assert_eq!(code_of("../../users/@me"), "");
    }
}
