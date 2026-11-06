//! Moving channels and categories around: the menu's Move to... and dragging in the sidebar.
//! Both end in PATCH /guilds/{id}/channels with the new positions.

use serde_json::json;

use super::App;
use crate::net::Tag;
use crate::state::{Channel, Id, Store};

pub fn children(store: &Store, gid: Id, parent: Id) -> Vec<&Channel> {
    let mut v: Vec<&Channel> = store.channels.values().filter(|c| c.guild == gid && c.parent == parent && c.kind != 4 && !c.is_thread()).collect();
    v.sort_by_key(|c| (c.is_voice(), c.position, c.id));
    v
}

pub fn categories(store: &Store, gid: Id) -> Vec<&Channel> {
    let mut v: Vec<&Channel> = store.channels.values().filter(|c| c.guild == gid && c.kind == 4).collect();
    v.sort_by_key(|c| (c.position, c.id));
    v
}

/// The ones a channel or category is ordered among: its own kind, in its category.
pub fn peers(store: &Store, c: &Channel) -> Vec<Id> {
    if c.kind == 4 {
        categories(store, c.guild).iter().map(|x| x.id).collect()
    } else {
        children(store, c.guild, c.parent).iter().filter(|x| x.is_voice() == c.is_voice()).map(|x| x.id).collect()
    }
}

/// `sorted` is a category's channels as shown, `moved` goes `index` into its own kind (the end
/// if None); text stays above voice.
pub fn layout(sorted: &[(Id, bool)], moved: (Id, bool), index: Option<usize>) -> Vec<Id> {
    let rest: Vec<(Id, bool)> = sorted.iter().copied().filter(|c| c.0 != moved.0).collect();
    let mut same: Vec<Id> = rest.iter().filter(|c| c.1 == moved.1).map(|c| c.0).collect();
    let other: Vec<Id> = rest.iter().filter(|c| c.1 != moved.1).map(|c| c.0).collect();
    same.insert(index.unwrap_or(same.len()).min(same.len()), moved.0);
    if moved.1 { other.into_iter().chain(same).collect() } else { same.into_iter().chain(other).collect() }
}

/// Puts a channel `index` places in under `parent` (0 for none), or last. A category moves
/// among the categories and ignores the parent.
pub fn move_to(app: &mut App, ch: Id, parent: Id, index: Option<usize>) {
    let store = &app.store;
    let Some(c) = store.channels.get(&ch) else { return };
    let gid = c.guild;
    let (parent, order) = if c.kind == 4 {
        let sorted: Vec<(Id, bool)> = categories(store, gid).iter().map(|x| (x.id, false)).collect();
        (0, layout(&sorted, (ch, false), index))
    } else {
        let sorted: Vec<(Id, bool)> = children(store, gid, parent).iter().map(|x| (x.id, x.is_voice())).collect();
        (parent, layout(&sorted, (ch, c.is_voice()), index))
    };
    let mut body = Vec::new();
    for (i, id) in order.iter().enumerate() {
        let Some(x) = store.channels.get(id) else { continue };
        if *id == ch {
            body.push(json!({ "id": id.to_string(), "position": i, "parent_id": (parent != 0).then(|| parent.to_string()), "lock_permissions": false }));
        } else if x.position != i as i32 {
            body.push(json!({ "id": id.to_string(), "position": i }));
        }
    }
    if body.is_empty() {
        return;
    }
    let before: Vec<(Id, i32, Id)> = order.iter().filter_map(|id| app.store.channels.get(id).map(|x| (*id, x.position, x.parent))).collect();
    for (i, id) in order.iter().enumerate() {
        if let Some(x) = app.store.channels.get_mut(id) {
            x.position = i as i32;
            if *id == ch {
                x.parent = parent;
            }
        }
    }
    app.rest("PATCH", format!("/guilds/{gid}/channels"), Some(json!(body)), Tag::rollback(move |app: &mut App| {
        for (id, position, parent) in &before {
            if let Some(x) = app.store.channels.get_mut(id) {
                x.position = *position;
                x.parent = *parent;
            }
        }
    }));
}

pub fn grab(ctx: &egui::Context, r: &egui::Response, id: Id) {
    if r.drag_started() {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("chan_held"), id));
    }
}

fn band(app: &App, key: String, p: egui::Pos2) -> Option<(egui::Rect, bool)> {
    let r = app.rects.get(&key)?.expand2(egui::vec2(0.0, 1.0));
    r.contains(p).then(|| (r, (p.y - r.top()) / r.height() >= 0.5))
}

/// Where a held channel or category would land under the pointer: the parent, the index among
/// its own kind, and the line that shows it.
fn spot(app: &App, held: &Channel, p: egui::Pos2) -> Option<(Id, usize, (egui::Rect, f32))> {
    let store = &app.store;
    let gid = held.guild;
    let cats = categories(store, gid);
    let rect = |pre: &str, c: Id| app.rects.get(&format!("{pre}:{c}")).copied();
    let edge = |pre: &str, ids: &[Id], i: usize| {
        if let Some(r) = ids.get(i).and_then(|c| rect(pre, *c)) {
            return Some((r, r.top() - 1.0));
        }
        let all = store.channels.values().filter(|x| x.guild == gid).filter_map(|x| rect("chan", x.id).or_else(|| rect("cat", x.id)));
        let last = if pre == "cat" { all.max_by(|a, b| a.bottom().total_cmp(&b.bottom())) } else { ids.last().and_then(|c| rect(pre, *c)) }?;
        Some((last, last.bottom() + 1.0))
    };
    if held.kind == 4 {
        let ids: Vec<Id> = cats.iter().map(|c| c.id).filter(|c| *c != held.id).collect();
        let (at, low) = ids.iter().enumerate().find_map(|(i, c)| band(app, format!("cat:{c}"), p).map(|b| (i, b.1)))?;
        let i = at + low as usize;
        return Some((0, i, edge("cat", &ids, i)?));
    }
    for k in cats.iter().filter(|k| k.id != held.parent) {
        if let Some((r, _)) = band(app, format!("cat:{}", k.id), p) {
            return Some((k.id, 0, (r, r.bottom() + 1.0)));
        }
    }
    let parents = std::iter::once(0).chain(cats.iter().map(|k| k.id));
    for parent in parents {
        let ids: Vec<Id> = children(store, gid, parent).iter().filter(|x| x.is_voice() == held.is_voice() && x.id != held.id).map(|x| x.id).collect();
        if let Some((at, low)) = ids.iter().enumerate().find_map(|(i, c)| band(app, format!("chan:{c}"), p).map(|b| (i, b.1))) {
            let i = at + low as usize;
            return Some((parent, i, edge("chan", &ids, i)?));
        }
    }
    None
}

/// Drag in the sidebar: a line shows where the channel or category lands, letting go moves it.
pub fn drag(app: &mut App, ctx: &egui::Context) {
    let key = egui::Id::new("chan_held");
    let Some(held) = ctx.data(|d| d.get_temp::<Id>(key)) else { return };
    let Some(c) = app.store.channels.get(&held).cloned() else {
        ctx.data_mut(|d| d.remove::<Id>(key));
        return;
    };
    let pos = ctx.pointer_interact_pos();
    let target = pos.and_then(|p| spot(app, &c, p));
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    let paint = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("chan_drop")));
    if let Some((_, _, (r, y))) = target {
        paint.hline(r.x_range().shrink(8.0), y, egui::Stroke::new(2.0, crate::theme::BLURPLE));
    }
    if let Some(p) = pos {
        paint.text(p + egui::vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, &c.name, crate::theme::medium(14.0), crate::theme::pal().header_primary);
    }
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if !ctx.input(|i| i.pointer.primary_down()) || esc {
        ctx.data_mut(|d| d.remove::<Id>(key));
        if !esc && let Some((parent, i, _)) = target {
            move_to(app, held, parent, Some(i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_stays_above_voice() {
        let list = [(1, false), (2, false), (3, true), (4, true)];
        assert_eq!(layout(&list, (2, false), Some(0)), [2, 1, 3, 4]);
        assert_eq!(layout(&list, (1, false), None), [2, 1, 3, 4]);
        assert_eq!(layout(&list, (4, true), Some(0)), [1, 2, 4, 3]);
    }

    #[test]
    fn into_another_category() {
        let list = [(5, false), (6, true)];
        assert_eq!(layout(&list, (9, false), None), [5, 9, 6]);
        assert_eq!(layout(&list, (9, true), None), [5, 6, 9]);
        assert_eq!(layout(&[], (9, true), Some(3)), [9]);
    }
}
