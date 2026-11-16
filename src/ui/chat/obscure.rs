use super::*;
use crate::cache::Spoilers;
use crate::state::Attachment;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Why {
    Spoiler,
    Sensitive,
}

const EXPLICIT: u32 = 16;
const GORE: u32 = 64 | 128;
const SHOW: u8 = 1;

/// Which of the account's three filter settings a message falls under: a friend's DM, anyone
/// else's, or a server.
pub(super) fn context(store: &Store, ch: Id, author: Id) -> usize {
    match store.channels.get(&ch) {
        Some(c) if c.guild == 0 && store.relationships.get(&author) == Some(&crate::state::FRIEND) => 0,
        Some(c) if c.guild == 0 => 1,
        _ => 2,
    }
}

/// Whether discord's filters hide media with these flags from this viewer; never their own.
pub(crate) fn filtered(flags: u32, own: bool, explicit: u8, gore: u8) -> bool {
    !own && ((flags & EXPLICIT != 0 && explicit != SHOW) || (flags & GORE != 0 && gore != SHOW))
}

/// Why a file would be covered, whether or not it has been clicked open.
fn reason(d: &D, m: &Message, a: &Attachment) -> Option<Why> {
    let at = context(d.store, m.channel, m.author);
    if filtered(a.flags, m.author == d.store.me, d.s.explicit[at], d.s.gore[at]) {
        return Some(Why::Sensitive);
    }
    let shown = d.s.spoilers == Spoilers::Always || (d.s.spoilers == Spoilers::Moderated && d.moderator);
    (a.spoiler() && !shown).then_some(Why::Spoiler)
}

pub(super) fn why(d: &D, m: &Message, a: &Attachment) -> Option<Why> {
    reason(d, m, a).filter(|_| !d.unveiled.contains(&a.id))
}

/// Discord's eye on sensitive media that was opened: a click covers it again.
pub(super) fn eye(ui: &mut Ui, d: &mut D, rect: Rect, m: &Message, a: &Attachment) {
    if d.unveiled.contains(&a.id) && reason(d, m, a) == Some(Why::Sensitive) {
        eye_button(ui, d, rect, a.id);
    }
}

fn embed_key(msg: Id, idx: usize) -> Id {
    (1 << 63) | msg.wrapping_mul(31).wrapping_add(idx as u64)
}

fn embed_flagged(d: &D, msg: Id, flags: u32) -> bool {
    let author = d.store.find_msg(d.ch, msg).map_or(0, |m| m.author);
    let at = context(d.store, d.ch, author);
    filtered(flags, author == d.store.me, d.s.explicit[at], d.s.gore[at])
}

/// A flagged embed's pictures stay covered until opened; hands back what opening it records.
pub(super) fn embed_hidden(d: &D, msg: Id, idx: usize, flags: u32) -> Option<Id> {
    let key = embed_key(msg, idx);
    (embed_flagged(d, msg, flags) && !d.unveiled.contains(&key)).then_some(key)
}

pub(super) fn embed_eye(ui: &mut Ui, d: &mut D, rect: Rect, msg: Id, idx: usize, flags: u32) {
    let key = embed_key(msg, idx);
    if d.unveiled.contains(&key) && embed_flagged(d, msg, flags) {
        eye_button(ui, d, rect, key);
    }
}

fn eye_button(ui: &mut Ui, d: &mut D, rect: Rect, key: Id) {
    let b = Rect::from_min_size(pos2(rect.right() - 40.0, rect.top() + 8.0), vec2(32.0, 32.0));
    let resp = ui.interact(b, ui.id().with(("veil", key)), Sense::click());
    ui.painter().circle_filled(b.center(), 16.0, Color32::from_black_alpha(if resp.hovered() { 204 } else { 153 }));
    icons::draw(ui.painter(), Icon::Eye, b.shrink(6.0), Color32::WHITE);
    d.rects.push((format!("veil:{key}"), b));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        d.acts.push(Act::Unveil(key, false));
    }
}

/// The cover over one picture or video; true when it was clicked away. The blur is the picture
/// asked for at 16 pixels and stretched.
pub(super) fn cover(ui: &mut Ui, d: &mut D, rect: Rect, radius: CornerRadius, a: &Attachment, why: Why, alone: bool, key: (Id, usize)) -> bool {
    let resp = ui.interact(rect, ui.id().with(("obscured", key)), Sense::click());
    ui.painter().rect_filled(rect, radius, pal().bg_tertiary);
    if a.w > 0 {
        let sep = if a.proxy.contains('?') { '&' } else { '?' };
        let still = if a.is_video() { "format=webp&" } else { "" };
        egui::Image::from_uri(format!("{}{sep}{still}width=16&height=16", a.proxy)).corner_radius(radius).show_loading_spinner(false).paint_at(ui, rect);
    }
    ui.painter().rect_filled(rect, radius, Color32::from_black_alpha(if resp.hovered() { 100 } else { 130 }));
    match why {
        Why::Spoiler => {
            let g = ui.painter().layout_no_wrap("SPOILER".into(), semi(15.0), Color32::WHITE);
            let pill = Rect::from_center_size(rect.center(), g.size() + vec2(24.0, 16.0));
            ui.painter().rect_filled(pill, pill.height() / 2.0, Color32::from_black_alpha(if resp.hovered() { 230 } else { 153 }));
            ui.painter().galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
        }
        Why::Sensitive => {
            let says = alone && rect.width() >= 240.0 && rect.height() >= 90.0;
            let mid = rect.center() - vec2(0.0, if says { 12.0 } else { 0.0 });
            icons::draw(ui.painter(), Icon::EyeOff, Rect::from_center_size(mid, vec2(32.0, 32.0)), Color32::WHITE);
            if says {
                ui.painter().text(mid + vec2(0.0, 30.0), Align2::CENTER_CENTER, "May contain sensitive content", reg(14.0), Color32::WHITE);
            }
        }
    }
    d.rects.push((format!("obscured:{}:{}", key.0, key.1), rect));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(if why == Why::Spoiler { "Spoiler" } else { "Content Warning: This may contain sensitive content" }).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_hide_flagged_media_unless_set_to_show_or_your_own() {
        assert!(filtered(EXPLICIT, false, 2, 1));
        assert!(filtered(EXPLICIT, false, 3, 1));
        assert!(!filtered(EXPLICIT, false, SHOW, 3));
        assert!(!filtered(EXPLICIT, true, 2, 2));
        assert!(filtered(64, false, SHOW, 2));
        assert!(filtered(128, false, SHOW, 3));
        assert!(!filtered(8, false, 2, 2));
    }
}
