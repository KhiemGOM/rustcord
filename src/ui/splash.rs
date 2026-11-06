//! What's up until the first READY of a session with nothing cached: the logo, one of discord's
//! "Did you know" tips and where to go when it won't connect.

use egui::{Align2, Color32, Frame, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use crate::theme::*;

const TIPS: [&str; 5] = [
    "Press Ctrl+K to jump to any server, channel or DM.",
    "Shift+Enter starts a new line without sending your message.",
    "Right-click a message and pick Mark Unread to come back to it later.",
    "Type @ in the quick switcher to search only people.",
    "Hold Shift when you click Delete Message to skip the confirmation.",
];

pub fn waiting(app: &super::App) -> bool {
    app.token.is_some() && app.store.me == 0
}

pub fn show(ui: &mut Ui) {
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_tertiary)).show(ui, |ui| {
        let r = ui.max_rect();
        let c = r.center();
        let t = ui.input(|i| i.time) as f32;
        let pulse = 0.65 + 0.35 * (t * 2.4).sin().abs();
        icons::draw(ui.painter(), Icon::Discord, Rect::from_center_size(c - vec2(0.0, 60.0), vec2(96.0, 96.0)), Color32::WHITE.gamma_multiply(pulse));
        ui.painter().text(pos2(c.x, c.y + 20.0), Align2::CENTER_CENTER, "DID YOU KNOW", bold(12.0), GREEN);
        let tip = TIPS[std::process::id() as usize % TIPS.len()];
        let g = ui.painter().layout(tip.into(), medium(16.0), pal().text_normal, 420.0_f32.min(r.width() - 48.0));
        ui.painter().galley(pos2(c.x - g.size().x / 2.0, c.y + 40.0), g, pal().text_normal);
        let y = r.bottom() - 40.0;
        let hint = ui.painter().text(pos2(c.x - 90.0, y), Align2::CENTER_CENTER, "Connection problems? Let us know!", reg(14.0), pal().text_muted);
        let mut x = hint.right() + 12.0;
        for (label, url) in [("Tweet Us", "https://twitter.com/discord_support"), ("Server Status", "https://discordstatus.com")] {
            let b = ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, label, medium(14.0), pal().link);
            let resp = ui.interact(b.expand(4.0), ui.id().with(label), Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().hline(b.x_range(), b.bottom(), egui::Stroke::new(1.0, pal().link));
            }
            if resp.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            x = b.right() + 12.0;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    });
}
