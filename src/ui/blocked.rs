//! Runs of messages from blocked or ignored people, folded into one "N blocked messages — Show" row.

use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::state::{BLOCKED, Id, Message, Store};
use crate::theme::*;

pub fn hides(store: &Store, m: &Message) -> bool {
    m.author != store.me && matches!(m.kind, 0 | 19 | 20 | 23) && (store.relationships.get(&m.author) == Some(&BLOCKED) || store.ignored.contains(&m.author))
}

/// The messages of the run that starts at `list[i]`: their ids, and whether every one is from an ignored person.
pub fn run(store: &Store, list: &[Message], i: usize) -> (Vec<Id>, bool) {
    let run: Vec<&Message> = list[i..].iter().take_while(|m| hides(store, m)).collect();
    let ignored = run.iter().all(|m| store.relationships.get(&m.author) != Some(&BLOCKED));
    (run.iter().map(|m| m.id).collect(), ignored)
}

/// The row over a run, true when its Show or Hide was clicked.
pub fn row(ui: &mut Ui, count: usize, ignored: bool, shown: bool, id: Id) -> bool {
    let left = ui.max_rect().left();
    let top = ui.cursor().top() + 2.0;
    let kind = if ignored { "ignored" } else { "blocked" };
    let text = format!("{count} {kind} message{} —", if count == 1 { "" } else { "s" });
    let p = ui.painter().clone();
    let muted = pal().chat_muted;
    let c = pos2(left + 36.0, top + 11.0);
    p.circle_stroke(c, 6.0, Stroke::new(1.5, muted));
    p.line_segment([c + vec2(-4.2, 4.2), c + vec2(4.2, -4.2)], Stroke::new(1.5, muted));
    let g = p.layout_no_wrap(text, reg(16.0), pal().text_normal);
    let tx = left + 72.0;
    p.galley(pos2(tx, top + 11.0 - g.size().y / 2.0), g.clone(), pal().text_normal);
    let label = p.layout_no_wrap(if shown { "Hide" } else { "Show" }.to_string(), reg(16.0), muted);
    let b = Rect::from_min_size(pos2(tx + g.size().x + 4.0, top), vec2(label.size().x, 22.0));
    let resp = ui.interact(b, egui::Id::new(("blocked_run", id)), Sense::click());
    super::settings::mark(ui, &format!("blocked:{count}:{kind}:{}", if shown { "hide" } else { "show" }), b);
    let col = if resp.hovered() { pal().text_normal } else { muted };
    p.galley(pos2(b.left(), top + 11.0 - label.size().y / 2.0), label, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        p.hline(b.x_range(), b.bottom() - 2.0, Stroke::new(1.0, col));
    }
    ui.advance_cursor_after_rect(Rect::from_min_size(pos2(left, top - 2.0), vec2(ui.available_width(), 26.0)));
    resp.clicked()
}

