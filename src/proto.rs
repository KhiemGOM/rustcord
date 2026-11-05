//! Just enough protobuf to pull guild folders and the settings Rustcord has a
//! twin for out of the READY settings blob (PreloadedUserSettings: 6 = text and
//! images, 12 = localization, 13 = appearance, 14 = guild folders). Field numbers
//! are from dolfies/discord-protos.

use crate::cache::{Settings, Spoilers};
use crate::state::{Folder, Id};
use crate::theme::Theme;

enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Fixed(u64),
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i)?;
        *i += 1;
        out |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

fn fields(b: &[u8]) -> Vec<(u64, Val<'_>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let v = match key & 7 {
            0 => match varint(b, &mut i) {
                Some(v) => Val::Int(v),
                None => break,
            },
            1 => match b.get(i..i + 8) {
                Some(s) => {
                    i += 8;
                    Val::Fixed(u64::from_le_bytes(s.try_into().unwrap()))
                }
                None => break,
            },
            2 => {
                let Some(len) = varint(b, &mut i) else { break };
                let Some(s) = b.get(i..i + len as usize) else { break };
                i += len as usize;
                Val::Bytes(s)
            }
            5 => {
                i += 4;
                continue;
            }
            _ => break,
        };
        out.push((key >> 3, v));
    }
    out
}

// google.protobuf.*Value wrappers keep their payload in field 1
fn wrapped(b: &[u8]) -> Option<Val<'_>> {
    fields(b).into_iter().find(|(f, _)| *f == 1).map(|(_, v)| v)
}

fn folder(b: &[u8]) -> Folder {
    let mut f = Folder::default();
    for (n, v) in fields(b) {
        match (n, v) {
            (1, Val::Bytes(packed)) => {
                f.guilds.extend(packed.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap()) as Id))
            }
            (1, Val::Fixed(g)) => f.guilds.push(g),
            (2, Val::Bytes(w)) => {
                if let Some(Val::Int(id)) = wrapped(w) {
                    f.id = id;
                }
            }
            (3, Val::Bytes(w)) => {
                if let Some(Val::Bytes(s)) = wrapped(w) {
                    f.name = Some(String::from_utf8_lossy(s).into_owned()).filter(|s| !s.is_empty());
                }
            }
            (4, Val::Bytes(w)) => {
                if let Some(Val::Int(c)) = wrapped(w) {
                    f.color = Some(c as u32);
                }
            }
            _ => {}
        }
    }
    f
}

pub fn guild_folders(settings: &[u8]) -> Vec<Folder> {
    let mut out = Vec::new();
    for (n, v) in fields(settings) {
        if let (14, Val::Bytes(gf)) = (n, v) {
            for (n2, v2) in fields(gf) {
                if let (1, Val::Bytes(fb)) = (n2, v2) {
                    out.push(folder(fb));
                }
            }
        }
    }
    out
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_bytes(out: &mut Vec<u8>, field: u64, b: &[u8]) {
    put_varint(out, field << 3 | 2);
    put_varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn put_ids(out: &mut Vec<u8>, field: u64, ids: &[Id]) {
    let packed: Vec<u8> = ids.iter().flat_map(|g| (*g as u64).to_le_bytes()).collect();
    put_bytes(out, field, &packed);
}

fn put_wrapped(out: &mut Vec<u8>, field: u64, v: u64) {
    let mut w = Vec::new();
    put_varint(&mut w, 1 << 3);
    put_varint(&mut w, v);
    put_bytes(out, field, &w);
}

/// A PreloadedUserSettings holding just the guild folders (14), for a partial update.
pub fn folders_update(folders: &[Folder]) -> Vec<u8> {
    let mut list = Vec::new();
    for f in folders {
        let mut b = Vec::new();
        put_ids(&mut b, 1, &f.guilds);
        if f.id != 0 {
            put_wrapped(&mut b, 2, f.id as u64);
        }
        if let Some(n) = &f.name {
            let mut w = Vec::new();
            put_bytes(&mut w, 1, n.as_bytes());
            put_bytes(&mut b, 3, &w);
        }
        if let Some(c) = f.color {
            put_wrapped(&mut b, 4, c as u64);
        }
        put_bytes(&mut list, 1, &b);
    }
    let mut out = Vec::new();
    put_bytes(&mut out, 14, &list);
    out
}

/// The privacy settings' (8) lists of servers: 3 direct messages off, 13 activity status off, 15
/// activity joining off, 16 message requests off. Only the lists the blob carries.
pub fn privacy(settings: &[u8]) -> Vec<(u64, Vec<Id>)> {
    let mut out = Vec::new();
    for (n, v) in fields(settings) {
        let (8, Val::Bytes(b)) = (n, v) else { continue };
        for (n, v) in fields(b) {
            if !matches!(n, 3 | 13 | 15 | 16) {
                continue;
            }
            let ids = match v {
                Val::Bytes(p) => p.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap()) as Id).collect(),
                Val::Fixed(g) => vec![g as Id],
                _ => continue,
            };
            out.push((n, ids));
        }
    }
    out
}

/// One of those lists, set to `ids`, as a partial update.
pub fn privacy_update(field: u64, ids: &[Id]) -> Vec<u8> {
    let mut p = Vec::new();
    put_ids(&mut p, field, ids);
    let mut out = Vec::new();
    put_bytes(&mut out, 8, &p);
    out
}

/// The account's own custom status text (status settings 11, custom_status 2), unless it ran out.
/// None when the blob has no status settings at all, so a partial update leaves it be.
pub fn custom_status(settings: &[u8], now_ms: u64) -> Option<Option<String>> {
    let status = fields(settings).into_iter().find_map(|(n, v)| match (n, v) {
        (11, Val::Bytes(b)) => Some(b),
        _ => None,
    })?;
    let Some(custom) = fields(status).into_iter().find_map(|(n, v)| match (n, v) {
        (2, Val::Bytes(b)) => Some(b),
        _ => None,
    }) else {
        return Some(None);
    };
    let mut text = None;
    for (n, v) in fields(custom) {
        match (n, v) {
            (1, Val::Bytes(t)) => text = Some(String::from_utf8_lossy(t).into_owned()),
            (4, Val::Fixed(exp)) if exp != 0 && exp <= now_ms => return Some(None),
            _ => {}
        }
    }
    Some(text.filter(|t| !t.is_empty()))
}

/// From the status settings (11): the status (1) and whether games are shared (3, a BoolValue
/// that's empty when false). Each None when the blob doesn't say.
pub fn status(settings: &[u8]) -> (Option<String>, Option<bool>) {
    let mut out = (None, None);
    for (n, v) in fields(settings) {
        let (11, Val::Bytes(b)) = (n, v) else { continue };
        for (n, v) in fields(b) {
            match (n, v) {
                (1, Val::Bytes(w)) => out.0 = text(w).filter(|s| !s.is_empty()),
                (3, Val::Bytes(w)) => out.1 = Some(flag(w)),
                _ => {}
            }
        }
    }
    out
}

macro_rules! account {
    ($($f:ident: $t:ty),* $(,)?) => {
        /// What the account says for the settings we mirror; None = not in the blob.
        #[derive(Default, Clone, PartialEq)]
        pub struct Account {
            $(pub $f: Option<$t>),*
        }

        impl Account {
            pub fn merge(&mut self, o: Account) {
                $(self.$f = o.$f.or(self.$f.take());)*
            }

            /// Only the values that differ from what we saw last time.
            pub fn changed(&self, old: Option<&Account>) -> Account {
                Account { $($f: self.$f.clone().filter(|n| old.is_none_or(|o| o.$f.as_ref() != Some(n)))),* }
            }

            pub fn apply(&self, s: &mut Settings) {
                $(if let Some(v) = &self.$f {
                    s.$f = v.clone();
                })*
            }
        }
    };
}

account! {
    theme: Theme,
    dev: bool,
    clock24: bool,
    compact: bool,
    compact_avatars: bool,
    thread_sidebar: bool,
    inline_links: bool,
    inline_uploads: bool,
    embeds: bool,
    reactions: bool,
    emoticons: bool,
    animate_emoji: bool,
    gif_autoplay: bool,
    spoilers: Spoilers,
    locale: String,
}

fn flag(w: &[u8]) -> bool {
    matches!(wrapped(w), Some(Val::Int(v)) if v != 0)
}

fn text(w: &[u8]) -> Option<String> {
    match wrapped(w) {
        Some(Val::Bytes(s)) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

/// `full` = the blob is a whole snapshot, so a missing developer_mode (a plain
/// bool, never written when false) means off rather than "not mentioned".
pub fn account(settings: &[u8], full: bool) -> Account {
    let mut a = Account::default();
    for (n, v) in fields(settings) {
        let Val::Bytes(b) = v else { continue };
        match n {
            6 => {
                for (n, v) in fields(b) {
                    let Val::Bytes(w) = v else { continue };
                    match n {
                        3 => a.thread_sidebar = Some(flag(w)),
                        4 => {
                            a.spoilers = match text(w).as_deref() {
                                Some("ALWAYS") => Some(Spoilers::Always),
                                Some("ON_CLICK") => Some(Spoilers::OnClick),
                                Some("IF_MODERATOR") => Some(Spoilers::Moderated),
                                _ => None,
                            }
                        }
                        9 => a.inline_uploads = Some(flag(w)),
                        10 => a.inline_links = Some(flag(w)),
                        11 => a.gif_autoplay = Some(flag(w)),
                        12 => a.embeds = Some(flag(w)),
                        13 => a.reactions = Some(flag(w)),
                        14 => a.animate_emoji = Some(flag(w)),
                        17 => a.compact = Some(flag(w)),
                        21 => a.emoticons = Some(flag(w)),
                        41 => a.compact_avatars = Some(flag(w)),
                        _ => {}
                    }
                }
            }
            12 => {
                for (n, v) in fields(b) {
                    if let (1, Val::Bytes(w)) = (n, v) {
                        a.locale = text(w).filter(|l| !l.is_empty());
                    }
                }
            }
            13 => {
                if full {
                    a.dev = Some(false);
                }
                for (n, v) in fields(b) {
                    let Val::Int(i) = v else { continue };
                    match n {
                        // 1 dark, 2 light, 3 darker, 4 midnight; the 2025 names are Ash, Light, Dark, Onyx
                        1 => {
                            a.theme = match i {
                                1 => Some(Theme::Ash),
                                2 => Some(Theme::Light),
                                3 => Some(Theme::Dark),
                                4 => Some(Theme::Onyx),
                                _ => None,
                            }
                        }
                        2 => a.dev = Some(i != 0),
                        9 => {
                            a.clock24 = match i {
                                1 => Some(false),
                                2 => Some(true),
                                _ => None,
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    a
}
