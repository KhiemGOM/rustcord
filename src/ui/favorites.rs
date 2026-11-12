//! Discord's Favorites server: the channel menu's Add to and Remove from Favorites, the order its
//! channel list takes, and opening a favourite inside it. What is in it lives in the account's
//! settings proto (`proto::favorites`); every change sends the whole message back, as Discord's does.

use super::App;
use crate::net::Tag;
use crate::proto::favorites::{self as fav, Fav, Favorites};
use crate::state::{Channel, Id, Store};

/// The Favorites server's place in `ui.guild` and `ui.chan`; Discord's stand-in guild has this id too.
pub const ID: Id = 373;
const MOST: usize = 200;

pub fn has(store: &Store, ch: Id) -> bool {
    store.favorites.channels.contains_key(&ch)
}

/// Discord's useAddToFavoritesItem: anything but a category, in a server you have joined.
pub fn can_add(store: &Store, c: &Channel) -> bool {
    c.kind != 4 && c.guild != 0 && !store.lurking.contains(&c.guild) && !has(store, c.id)
}

/// The rail shows it while it is open, or when the account's setting (or, unset, having any) says so.
pub fn shown(app: &App) -> bool {
    app.ui.guild == Some(ID) || app.store.favorites.enabled()
}

/// Nitro gets 200, everyone else the free tier's 3; past 200 entries, categories included, nothing more goes in.
fn full(store: &Store, f: &Favorites) -> Option<usize> {
    if f.channels.len() >= MOST {
        return Some(MOST);
    }
    let most = if store.me_raw["premium_type"].as_u64() == Some(2) { MOST } else { 3 };
    (f.channels.values().filter(|c| c.kind != fav::CATEGORY).count() >= most).then_some(most)
}

/// Drops favourites of channels that are gone or can't be seen (an unloaded thread stays), as Discord
/// does before each change, and fills in a missing channel type.
fn prune(store: &Store, f: &mut Favorites) {
    f.channels.retain(|id, c| {
        if c.kind == fav::CATEGORY {
            return true;
        }
        match store.channels.get(id) {
            None => matches!(c.chan_kind, Some(10..=12)),
            Some(ch) => {
                c.chan_kind.get_or_insert(ch.kind as u32);
                ch.guild == 0 || store.can_view(ch)
            }
        }
    });
    let cats: Vec<Id> = f.channels.iter().filter(|(_, c)| c.kind == fav::CATEGORY).map(|(id, _)| *id).collect();
    for c in f.channels.values_mut() {
        if c.parent != 0 && !cats.contains(&c.parent) {
            c.parent = 0;
        }
    }
}

fn save(app: &mut App, f: Favorites) {
    let was = std::mem::replace(&mut app.store.favorites, f);
    let bytes = fav::patch(&app.store.favorites);
    app.proto_patch(bytes, Tag::undo(super::SAVE_FAILED, move |app: &mut App| app.store.favorites = was.clone()));
}

/// Into the list, or into one of its categories.
pub fn add(app: &mut App, ch: Id, parent: Id) {
    if has(&app.store, ch) {
        return;
    }
    let mut f = app.store.favorites.clone();
    prune(&app.store, &mut f);
    if let Some(n) = full(&app.store, &f) {
        return app.toast(format!("You cannot have more than {n} favorites."));
    }
    let chan_kind = app.store.channels.get(&ch).map(|c| c.kind as u32);
    f.channels.insert(ch, Fav { kind: fav::CHANNEL, chan_kind, position: f.next_position(), parent, ..Default::default() });
    prune(&app.store, &mut f);
    if !app.store.favorites.enabled() {
        f.visible = Some(true);
    }
    save(app, f);
    app.toast("Added to Favorites!");
}

pub fn remove(app: &mut App, ch: Id) {
    let mut f = app.store.favorites.clone();
    let Some(gone) = f.channels.remove(&ch) else { return };
    if app.ui.guild == Some(ID) && app.cur() == Some(ch) {
        let order: Vec<Id> = sections(&app.store).into_iter().flat_map(|(_, k)| k).map(|c| c.id).collect();
        let next = match order.iter().position(|c| *c == ch) {
            Some(0) => order.get(1),
            Some(i) => order.get(i - 1),
            None => None,
        };
        match next.copied() {
            Some(c) => open(app, c),
            None => {
                app.ui.chan.remove(&ID);
            }
        }
    }
    if gone.kind == fav::CATEGORY {
        for c in f.channels.values_mut().filter(|c| c.parent == ch) {
            c.parent = 0;
        }
    }
    prune(&app.store, &mut f);
    save(app, f);
}

pub fn toggle_category(app: &mut App, cat: Id) {
    let mut f = app.store.favorites.clone();
    let Some(c) = f.channels.get_mut(&cat) else { return };
    c.collapsed = !c.collapsed;
    save(app, f);
}

/// The account's own categories in the Favorites server, in order, by name.
pub fn categories(store: &Store) -> Vec<(Id, String)> {
    let mut cats: Vec<(&Id, &Fav)> = store.favorites.channels.iter().filter(|(_, c)| c.kind == fav::CATEGORY).collect();
    cats.sort_by_key(|(id, c)| (c.position, **id));
    cats.into_iter().map(|(id, c)| (*id, c.nickname.clone())).collect()
}

/// Discord's favourites list: the ones outside a category first, then each category by position;
/// inside each, by position with voice channels last. Nicknames stand in for names. Only server
/// channels: how Discord draws a favourite dm there isn't known.
pub fn sections(store: &Store) -> Vec<(Option<Id>, Vec<Channel>)> {
    let favs = &store.favorites.channels;
    let cats = categories(store);
    let mut out: Vec<(Option<Id>, Vec<Channel>)> = std::iter::once(None).chain(cats.iter().map(|(id, _)| Some(*id))).map(|c| (c, Vec::new())).collect();
    for (id, f) in favs.iter().filter(|(_, f)| f.kind != fav::CATEGORY) {
        let Some(c) = store.channels.get(id).filter(|c| c.guild != 0 && store.can_view(c)) else { continue };
        let at = cats.iter().position(|(k, _)| *k == f.parent).map_or(0, |i| i + 1);
        let mut c = c.clone();
        if !f.nickname.is_empty() {
            c.name = f.nickname.clone();
        }
        out[at].1.push(c);
    }
    for (_, kids) in &mut out {
        kids.sort_by_key(|c| (favs[&c.id].position + if c.is_voice() { 10_000 } else { 0 }, c.id));
    }
    out
}

/// Whether anything in it is unread, and its mentions, for the rail.
pub fn badge(store: &Store) -> (bool, u32) {
    let chans = store.favorites.channels.keys().filter_map(|id| store.channels.get(id)).filter(|c| c.guild != 0 && !store.is_muted(c));
    chans.fold((false, 0), |(u, m), c| (u || (c.is_text() && store.unread(c)), m + store.mentions(c.id)))
}

/// Opens a favourite and keeps the Favorites server around it.
pub fn open(app: &mut App, ch: Id) {
    app.open(ch);
    app.ui.chan.insert(ID, ch);
}

pub fn select(app: &mut App) {
    if app.ui.chan.get(&ID).is_some_and(|c| !has(&app.store, *c)) {
        app.ui.chan.remove(&ID);
    }
    app.select_guild(Some(ID));
    if app.cur().is_none()
        && let Some(c) = sections(&app.store).into_iter().flat_map(|(_, k)| k).find(|c| c.is_text())
    {
        open(app, c.id);
    }
}
