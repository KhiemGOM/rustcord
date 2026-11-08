//! A forwarded message: "Forwarded" over a 4-wide bar, the original's content indented 16,
//! and where it came from underneath.

use chrono::{Local, TimeZone};
use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};

use super::{Act, D, Icon, icons, medium, semi};
use crate::state::Message;
use crate::theme::*;

pub struct Open {
    outer: Ui,
    top: f32,
}

pub fn begin(cui: &mut Ui) -> Open {
    let top = cui.cursor().top();
    let (head, _) = cui.allocate_exact_size(vec2(cui.available_width(), 18.0), Sense::hover());
    icons::draw(cui.painter(), Icon::Forward, Rect::from_min_size(pos2(head.left() + 16.0, head.top() + 1.0), vec2(16.0, 16.0)), pal().text_muted);
    cui.painter().text(pos2(head.left() + 36.0, head.center().y), egui::Align2::LEFT_CENTER, "Forwarded", semi(14.0), pal().text_muted);
    cui.add_space(14.0);
    let body = Rect::from_min_max(pos2(cui.max_rect().left() + 16.0, cui.cursor().top()), cui.max_rect().right_bottom());
    let inner = cui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
    Open { outer: std::mem::replace(cui, inner), top }
}

pub fn end(cui: &mut Ui, d: &mut D, m: &Message, open: Open) {
    cui.add_space(7.0);
    let when = m.forward.and_then(|ms| Local.timestamp_millis_opt(ms).single()).map(|t| t.format("%-m/%-d/%y").to_string()).unwrap_or_default();
    let name = m.refer.and_then(|(c, _)| d.store.channels.get(&c)).map(|c| format!("# {}  •  ", c.name)).unwrap_or_default();
    let (foot, resp) = cui.allocate_exact_size(vec2(cui.available_width().min(400.0), 18.0), Sense::click());
    cui.painter().text(foot.left_center(), egui::Align2::LEFT_CENTER, format!("{name}{when}"), medium(14.0), pal().text_muted);
    if resp.clicked()
        && let Some((_, id)) = m.refer.filter(|r| r.1 != 0)
    {
        d.acts.push(Act::Jump(id));
    }
    let inner = std::mem::replace(cui, open.outer);
    cui.advance_cursor_after_rect(inner.min_rect());
    let bar = Rect::from_min_max(pos2(cui.max_rect().left(), open.top - 2.0), pos2(cui.max_rect().left() + 4.0, inner.min_rect().bottom() + 1.0));
    cui.painter().rect_filled(bar, 2.0, pal().border_subtle);
}
