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
    for (i, id) in order.iter().enumerate() {
        if let Some(x) = app.store.channels.get_mut(id) {
            x.position = i as i32;
            if *id == ch {
                x.parent = parent;
            }
        }
    }
    app.rest("PATCH", format!("/guilds/{gid}/channels"), Some(json!(body)), Tag::Quiet);
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
