use egui::{Align2, Painter, Pos2, Rect, Sense, Ui, pos2, vec2};

use super::App;
use crate::plugins;
use crate::state::{BLOCKED, Channel, Id, Store};
use crate::theme::*;

fn dots(p: &Painter, at: Pos2, t: f32, still: bool) {
    for i in 0..3 {
        let phase = if still { 1.0 } else { ((t * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5).clamp(0.0, 1.0) };
        p.circle_filled(at + vec2(i as f32 * 6.0, 0.0), 2.0, pal().header_primary.gamma_multiply(0.3 + 0.7 * phase));
    }
}

/// TypingIndicator: who is typing in a channel, as the plugin's options allow. `current` and
/// `muted` say whether the row is the open channel or a muted one.
pub fn typers(store: &Store, ch: Id, current: bool, muted: bool) -> Vec<Id> {
    const P: &str = "TypingIndicator";
    if !plugins::on(P) || (current && !plugins::flag(P, "includeCurrentChannel")) || (muted && !plugins::flag(P, "includeMutedChannels")) {
        return Vec::new();
    }
    let blocked = plugins::flag(P, "includeBlockedUsers");
    store.typing.get(&ch).into_iter().flatten().map(|(u, _)| *u).filter(|u| *u != store.me && (blocked || store.relationships.get(u) != Some(&BLOCKED))).collect()
}

/// Paints the indicator ending at `right` (the row's centre line) and returns the room it took.
pub fn row(ui: &Ui, store: &Store, guild: Id, who: &[Id], right: Pos2, id: egui::Id) -> f32 {
    if who.is_empty() {
        return 0.0;
    }
    let mode = plugins::num("TypingIndicator", "indicatorMode") as u8;
    let t = ui.input(|i| i.time) as f32;
    let still = plugins::on("NoTypingAnimation");
    let mut x = right.x;
    if mode & 1 != 0 {
        x -= 18.0;
        dots(ui.painter(), pos2(x + 3.0, right.y), t, still);
    }
    if mode & 2 != 0 {
        for u in who.iter().take(3) {
            x -= 14.0;
            if let Some(user) = store.user(*u) {
                super::avatar(ui, Rect::from_center_size(pos2(x + 8.0, right.y), vec2(16.0, 16.0)), &user.avatar_url());
            }
        }
        x -= 4.0;
    }
    let names: Vec<String> = who.iter().map(|u| store.name_in(guild, *u)).collect();
    let tip = match names.as_slice() {
        [a] => format!("{a} is typing..."),
        [a, b] => format!("{a} and {b} are typing..."),
        [a, b, c] => format!("{a}, {b} and {c} are typing..."),
        _ => "Several people are typing...".to_string(),
    };
    ui.interact(Rect::from_min_max(pos2(x, right.y - 8.0), pos2(right.x, right.y + 8.0)), id, Sense::hover()).on_hover_text(tip);
    if !still {
        ui.ctx().request_repaint();
    }
    right.x - x + 4.0
}

/// TypingTweaks: the composer's typing line with avatars, role colours, and a count for crowds.
pub fn tweaked(app: &App, ui: &mut Ui, p: &Painter, chan: &Channel, users: &[Id], composer: Rect, y: f32) {
    const P: &str = "TypingTweaks";
    let store = &app.store;
    let avatars = plugins::flag(P, "showAvatars");
    let colors = plugins::flag(P, "showRoleColors");
    let alt = plugins::flag(P, "alternativeFormatting");
    let shown = if users.len() > 3 && alt { 2 } else { users.len().min(3) };
    let lay = ui.new_child(egui::UiBuilder::new().layer_id(egui::LayerId::new(egui::Order::Middle, egui::Id::new("typing"))).max_rect(composer));
    let mut x = composer.left() + 64.5;
    let mut text = |p: &Painter, x: &mut f32, s: &str, name: bool, col: Option<egui::Color32>| {
        let (font, color) = if name { (semi(12.0), col.unwrap_or(pal().text_normal)) } else { (medium(12.0), pal().header_secondary) };
        *x = p.text(pos2(*x, y), Align2::LEFT_CENTER, s, font, color).right();
    };
    if users.len() > 3 && !alt {
        text(p, &mut x, "Several people are typing...", false, None);
        return;
    }
    for (i, u) in users.iter().take(shown).enumerate() {
        if i > 0 {
            let sep = if i + 1 == users.len() { if users.len() == 2 { " and " } else { ", and " } } else { ", " };
            text(p, &mut x, sep, false, None);
        }
        if avatars && let Some(user) = store.user(*u) {
            super::avatar(&lay, Rect::from_center_size(pos2(x + 8.0, y), vec2(16.0, 16.0)), &user.avatar_url());
            x += 20.0;
        }
        let col = colors.then(|| store.role_color(chan.guild, *u)).flatten().map(role_color);
        text(p, &mut x, &store.name_in(chan.guild, *u), true, col);
    }
    if users.len() > shown {
        text(p, &mut x, &format!(", and {} others are typing...", users.len() - shown), false, None);
    } else {
        text(p, &mut x, if users.len() == 1 { " is typing..." } else { " are typing..." }, false, None);
    }
}
