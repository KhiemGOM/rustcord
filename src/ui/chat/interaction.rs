use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use std::time::Duration;

use super::{D, avatar, name_ink, spine, text};
use crate::state::{Interaction, Message, Stage};
use crate::theme::*;
use crate::ui::interact::LOADING;

pub enum Status {
    Sending,
    Thinking(String),
    Failed(String),
}

const GRACE: Duration = Duration::from_secs(3);

pub fn status(d: &D, m: &Message) -> Option<Status> {
    let app = d.store.name_in(d.guild, m.author);
    match m.busy.as_ref().filter(|b| b.component.is_empty()).map(|b| &b.stage) {
        Some(Stage::Sending) if m.interaction.is_some() => return Some(Status::Sending),
        Some(Stage::Created) if m.interaction.is_some() => return Some(Status::Thinking(app)),
        Some(Stage::Failed(why, sent)) if m.interaction.is_some() => {
            return Some(Status::Failed(if sent.elapsed() < GRACE { why.clone() } else { "The application did not respond".into() }));
        }
        _ => {}
    }
    if m.flags & LOADING != 0 && m.interaction.is_some() {
        return Some(if m.pending { Status::Sending } else { Status::Thinking(app) });
    }
    None
}

pub fn line(ui: &mut Ui, ctx: &egui::Context, font: f32, dots: bool, text_: &str) {
    let h = (font * 1.375).round();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let y = r.center().y;
    let (col, mut x) = (if dots { pal().text_normal } else { pal().critical }, r.left());
    if dots {
        for i in 0..3 {
            let (a, s) = crate::ui::motion::dots(ctx, i);
            ui.painter().circle_filled(pos2(x + 3.5 + i as f32 * 8.75, y), 3.5 * s, pal().text_normal.gamma_multiply(a));
        }
        x += 8.75 * 3.0 + 4.0;
    } else {
        ui.painter().circle_filled(pos2(x + 8.0, y), 7.0, col);
        ui.painter().text(pos2(x + 8.0, y), Align2::CENTER_CENTER, "!", semi(11.0), Color32::WHITE);
        x += 24.0;
    }
    text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, text_, reg(font), col, r.right() - x);
}

pub fn status_line(ui: &mut Ui, d: &mut D, font: f32, s: &Status) {
    match s {
        Status::Sending => line(ui, d.ctx, font, true, "Sending command..."),
        Status::Thinking(app) => line(ui, d.ctx, font, true, &format!("{app} is thinking...")),
        Status::Failed(why) => {
            d.ctx.request_repaint_after(GRACE);
            line(ui, d.ctx, font, false, why)
        }
    }
}

/// "Name used /command" over a bot's reply, with the spine of a reply.
pub fn executed(ui: &mut Ui, d: &mut D, i: &Interaction, row_left: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    ui.add_space(4.0);
    let p = ui.painter();
    spine(p, row_left, r);
    let y = r.center().y;
    let mut cx = r.left();
    if let Some(u) = d.store.user(i.user) {
        avatar(ui, Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0)), &u.avatar_url());
        cx += 20.0;
    }
    let name = d.store.name_in(d.guild, i.user);
    let (ink, _) = name_ink(d, i.user);
    let rich = ink.layout(ui.ctx(), &name, medium(14.0), 200.0, false);
    let nr = Align2::LEFT_CENTER.anchor_size(pos2(cx, y), rich.size());
    ink.paint(p, &rich, nr.min, false);
    let used = p.text(pos2(nr.right() + 4.0, y), Align2::LEFT_CENTER, "used", reg(14.0), pal().text_normal);
    let g = p.layout_no_wrap(format!("/{}", i.name), medium(14.0), pal().mention_fg);
    let pill = Rect::from_min_size(pos2(used.right() + 4.0, y - 9.0), vec2(g.size().x + 20.0, 18.0));
    let hot = ui.rect_contains_pointer(pill);
    p.rect_filled(pill, CornerRadius::same(3), if hot { pal().mention_fg.gamma_multiply(0.3) } else { pal().mention_bg });
    for (dx, dy) in [(0.0, 0.0), (5.0, 0.0), (0.0, 5.0), (5.0, 5.0)] {
        p.circle_filled(pill.left_center() + vec2(8.0 + dx - 2.5, dy - 2.5), 2.0, pal().mention_fg);
    }
    p.galley(pos2(pill.left() + 19.0, y - g.size().y / 2.0), g, pal().mention_fg);
}
