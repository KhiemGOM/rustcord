//! Favorites, PreloadedUserSettings field 15: the channels on the account's Favorites server.
//! Field numbers and wire types are from Discord's client (discord_users.v1.Favorites and
//! FavoriteChannel); a change sends the whole message back, as the client does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Val, fields, message, put, tag, wrapped};
use crate::state::Id;

/// FavoriteChannelType: a channel the favourite points at, or a category of the Favorites server.
pub const CHANNEL: u32 = 1;
pub const CATEGORY: u32 = 2;

#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Fav {
    pub nickname: String,
    pub kind: u32,
    pub position: u32,
    pub parent: Id,
    pub chan_kind: Option<u32>,
    pub collapsed: bool,
}

#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Favorites {
    pub channels: BTreeMap<Id, Fav>,
    pub muted: bool,
    pub visible: Option<bool>,
    pub auto_threads: bool,
}

impl Favorites {
    /// Discord's favoriteGuildEnabled: the setting when there is one, else whether anything is in it.
    pub fn enabled(&self) -> bool {
        self.visible.unwrap_or(!self.channels.is_empty())
    }

    /// The next position, one past the highest.
    pub fn next_position(&self) -> u32 {
        self.channels.values().map(|f| f.position).max().unwrap_or(0) + 1
    }
}

fn fav(b: &[u8]) -> Fav {
    let mut f = Fav::default();
    for (n, v) in fields(b) {
        match (n, v) {
            (1, Val::Bytes(s)) => f.nickname = String::from_utf8_lossy(s).into_owned(),
            (2, Val::Int(i)) => f.kind = i as u32,
            (3, Val::Int(i)) => f.position = i as u32,
            (4, Val::Fixed(p)) => f.parent = p,
            (5, Val::Bytes(w)) => {
                f.chan_kind = Some(match wrapped(w) {
                    Some(Val::Int(k)) => k as u32,
                    _ => 0,
                })
            }
            (6, Val::Int(i)) => f.collapsed = i != 0,
            _ => {}
        }
    }
    f
}

/// The favourites in a settings blob, None when it doesn't carry them.
pub fn read(settings: &[u8]) -> Option<Favorites> {
    let body = fields(settings).into_iter().rev().find_map(|(n, v)| match (n, v) {
        (15, Val::Bytes(b)) => Some(b),
        _ => None,
    })?;
    let mut out = Favorites::default();
    for (n, v) in fields(body) {
        match (n, v) {
            (1, Val::Bytes(entry)) => {
                let (mut id, mut f) = (0, Fav::default());
                for (n, v) in fields(entry) {
                    match (n, v) {
                        (1, Val::Fixed(k)) => id = k,
                        (2, Val::Bytes(b)) => f = fav(b),
                        _ => {}
                    }
                }
                out.channels.insert(id, f);
            }
            (2, Val::Int(i)) => out.muted = i != 0,
            (3, Val::Bytes(w)) => out.visible = Some(matches!(wrapped(w), Some(Val::Int(1..)))),
            (4, Val::Int(i)) => out.auto_threads = i != 0,
            _ => {}
        }
    }
    Some(out)
}

fn flag(out: &mut Vec<u8>, field: u64, on: bool) {
    if on {
        tag(out, field, 0);
        put(out, 1);
    }
}

/// A PreloadedUserSettings update carrying the whole favourites message.
pub fn patch(f: &Favorites) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, c) in &f.channels {
        let mut v = Vec::new();
        if !c.nickname.is_empty() {
            message(&mut v, 1, c.nickname.as_bytes());
        }
        if c.kind != 0 {
            tag(&mut v, 2, 0);
            put(&mut v, c.kind as u64);
        }
        if c.position != 0 {
            tag(&mut v, 3, 0);
            put(&mut v, c.position as u64);
        }
        if c.parent != 0 {
            tag(&mut v, 4, 1);
            v.extend_from_slice(&c.parent.to_le_bytes());
        }
        if let Some(k) = c.chan_kind {
            let mut w = Vec::new();
            if k != 0 {
                tag(&mut w, 1, 0);
                put(&mut w, k as u64);
            }
            message(&mut v, 5, &w);
        }
        flag(&mut v, 6, c.collapsed);
        let mut entry = Vec::new();
        tag(&mut entry, 1, 1);
        entry.extend_from_slice(&id.to_le_bytes());
        message(&mut entry, 2, &v);
        message(&mut body, 1, &entry);
    }
    flag(&mut body, 2, f.muted);
    if let Some(on) = f.visible {
        let mut w = Vec::new();
        flag(&mut w, 1, on);
        message(&mut body, 3, &w);
    }
    flag(&mut body, 4, f.auto_threads);
    let mut out = Vec::new();
    message(&mut out, 15, &body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Favorites {
        let mut f = Favorites { visible: Some(true), ..Default::default() };
        f.channels.insert(11, Fav { kind: CATEGORY, position: 1, nickname: "Mine".into(), collapsed: true, ..Default::default() });
        f.channels.insert(22, Fav { kind: CHANNEL, position: 2, parent: 11, chan_kind: Some(0), ..Default::default() });
        f.channels.insert(33, Fav { kind: CHANNEL, position: 3, nickname: "short".into(), chan_kind: Some(2), ..Default::default() });
        f
    }

    #[test]
    fn favorites_round_trip() {
        let f = sample();
        assert_eq!(read(&patch(&f)), Some(f));
        let hidden = Favorites { visible: Some(false), muted: true, auto_threads: true, ..Default::default() };
        assert_eq!(read(&patch(&hidden)), Some(hidden));
    }

    #[test]
    fn favorites_absent_or_empty() {
        let mut other = Vec::new();
        message(&mut other, 14, &[]);
        assert_eq!(read(&other), None);
        assert_eq!(read(&patch(&Favorites::default())), Some(Favorites::default()));
        assert!(!Favorites::default().enabled());
        assert!(sample().enabled());
        assert!(!Favorites { visible: Some(false), ..sample() }.enabled());
    }

    #[test]
    fn favorites_wire_matches_discord() {
        let mut f = Favorites::default();
        f.channels.insert(5, Fav { kind: CHANNEL, position: 1, chan_kind: Some(0), ..Default::default() });
        // 15 { 1 { 1: fixed64 5, 2 { 2: 1, 3: 1, 5 {} } } }
        let want = [0x7a, 0x13, 0x0a, 0x11, 0x09, 5, 0, 0, 0, 0, 0, 0, 0, 0x12, 0x06, 0x10, 0x01, 0x18, 0x01, 0x2a, 0x00];
        assert_eq!(patch(&f), want);
        assert_eq!(f.next_position(), 2);
    }
}
