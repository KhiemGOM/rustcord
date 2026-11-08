//! FrecencyUserSettings, settings-proto type 2: favourite gifs, stickers and emoji and how
//! often each emoji and sticker is used. Field numbers and the scoring are from Discord's client.

use std::collections::BTreeMap;

use super::{Val, fields, put_bytes, put_varint, varint};
use crate::state::Id;

const SAMPLES: usize = 10;
pub const KEEP: usize = 100;

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Item {
    pub total: u32,
    pub recent: Vec<u64>,
    pub frecency: i32,
    pub score: i32,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Gif {
    pub url: String,
    pub src: String,
    pub w: u32,
    pub h: u32,
    pub order: u32,
}

/// Each field is None when the blob doesn't carry it, so a partial update leaves what it doesn't mention.
#[derive(Default, Debug)]
pub struct Frecency {
    pub gifs: Option<Vec<Gif>>,
    pub stickers: Option<Vec<Id>>,
    pub emojis: Option<Vec<String>>,
    pub emoji_use: Option<BTreeMap<String, Item>>,
    pub sticker_use: Option<BTreeMap<Id, Item>>,
}

impl Item {
    pub fn track(&mut self, now_ms: u64) {
        self.total += 1;
        self.recent.push(now_ms);
        self.trim();
    }

    pub fn merge(&mut self, other: &Item) {
        self.total = self.total.max(other.total);
        self.recent.extend(&other.recent);
        self.recent.sort_unstable();
        self.recent.dedup();
        self.trim();
    }

    fn trim(&mut self) {
        self.recent.sort_unstable();
        let extra = self.recent.len().saturating_sub(SAMPLES);
        self.recent.drain(..extra);
        self.frecency = -1;
    }

    pub fn compute(&mut self, now_ms: u64) {
        if self.frecency != -1 {
            return;
        }
        let weight = |days: u64| match days {
            0..=3 => 100,
            4..=15 => 70,
            16..=30 => 50,
            31..=45 => 30,
            46..=80 => 10,
            _ => 1,
        };
        let score: i32 = self.recent.iter().map(|t| weight(now_ms.saturating_sub(*t) / 86_400_000)).sum();
        self.score = score;
        self.frecency = if self.recent.is_empty() { 0 } else { (self.total as f64 * (score as f64 / self.recent.len() as f64)).ceil() as i32 };
    }
}

/// The keys of the `take` best-used entries, best first.
pub fn frequent<K: Clone + Ord>(uses: &mut BTreeMap<K, Item>, now_ms: u64, take: usize) -> Vec<K> {
    uses.values_mut().for_each(|i| i.compute(now_ms));
    let mut ranked: Vec<(&K, &Item)> = uses.iter().filter(|(_, i)| i.score > 0).collect();
    ranked.sort_by_key(|(_, i)| std::cmp::Reverse(i.frecency));
    ranked.into_iter().take(take).map(|(k, _)| k.clone()).collect()
}

fn item(b: &[u8]) -> Item {
    let mut it = Item { frecency: -1, ..Default::default() };
    for (n, v) in fields(b) {
        match (n, v) {
            (1, Val::Int(t)) => it.total = t as u32,
            (2, Val::Int(t)) => it.recent.push(t),
            (2, Val::Bytes(p)) => {
                let mut i = 0;
                while let Some(t) = varint(p, &mut i) {
                    it.recent.push(t);
                }
            }
            _ => {}
        }
    }
    it
}

fn gif(b: &[u8]) -> Gif {
    let mut g = Gif::default();
    for (n, v) in fields(b) {
        match (n, v) {
            (2, Val::Bytes(s)) => g.src = String::from_utf8_lossy(s).into_owned(),
            (3, Val::Int(w)) => g.w = w as u32,
            (4, Val::Int(h)) => g.h = h as u32,
            (5, Val::Int(o)) => g.order = o as u32,
            _ => {}
        }
    }
    g
}

type Entry<'a> = (Option<&'a [u8]>, Option<u64>, Option<&'a [u8]>);

fn entries(b: &[u8]) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    for (n, v) in fields(b) {
        let (1, Val::Bytes(e)) = (n, v) else { continue };
        let (mut key, mut id, mut val) = (None, None, None);
        for (n, v) in fields(e) {
            match (n, v) {
                (1, Val::Bytes(k)) => key = Some(k),
                (1, Val::Fixed(k)) => id = Some(k),
                (2, Val::Bytes(m)) => val = Some(m),
                _ => {}
            }
        }
        out.push((key, id, val));
    }
    out
}

pub fn parse(blob: &[u8]) -> Frecency {
    let mut out = Frecency::default();
    for (n, v) in fields(blob) {
        let Val::Bytes(b) = v else { continue };
        match n {
            2 => {
                let mut list: Vec<Gif> = entries(b)
                    .into_iter()
                    .filter_map(|(k, _, v)| {
                        let mut g = gif(v?);
                        g.url = String::from_utf8_lossy(k?).into_owned();
                        Some(g)
                    })
                    .collect();
                list.sort_by_key(|g| std::cmp::Reverse(g.order));
                out.gifs = Some(list);
            }
            3 => {
                let mut ids = Vec::new();
                for (n, v) in fields(b) {
                    match (n, v) {
                        (1, Val::Bytes(p)) => ids.extend(p.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap()) as Id)),
                        (1, Val::Fixed(i)) => ids.push(i as Id),
                        _ => {}
                    }
                }
                out.stickers = Some(ids);
            }
            4 => out.sticker_use = Some(entries(b).into_iter().filter_map(|(_, id, v)| Some((id? as Id, item(v?)))).collect()),
            5 => {
                let keys = fields(b).into_iter().filter_map(|(n, v)| match (n, v) {
                    (1, Val::Bytes(s)) => Some(String::from_utf8_lossy(s).into_owned()),
                    _ => None,
                });
                out.emojis = Some(keys.collect());
            }
            6 => out.emoji_use = Some(entries(b).into_iter().filter_map(|(k, _, v)| Some((String::from_utf8_lossy(k?).into_owned(), item(v?)))).collect()),
            _ => {}
        }
    }
    out
}

fn put_item(it: &Item) -> Vec<u8> {
    let mut b = Vec::new();
    put_varint(&mut b, 1 << 3);
    put_varint(&mut b, it.total as u64);
    let mut packed = Vec::new();
    it.recent.iter().for_each(|t| put_varint(&mut packed, *t));
    put_bytes(&mut b, 2, &packed);
    put_varint(&mut b, 3 << 3);
    put_varint(&mut b, it.frecency.max(0) as u64);
    put_varint(&mut b, 4 << 3);
    put_varint(&mut b, it.score.max(0) as u64);
    b
}

fn put_map(out: &mut Vec<u8>, field: u64, entries: Vec<(Vec<u8>, Vec<u8>)>) {
    let mut m = Vec::new();
    for (k, v) in entries {
        let mut e = k;
        put_bytes(&mut e, 2, &v);
        put_bytes(&mut m, 1, &e);
    }
    put_bytes(out, field, &m);
}

fn key_str(k: &str) -> Vec<u8> {
    let mut b = Vec::new();
    put_bytes(&mut b, 1, k.as_bytes());
    b
}

pub fn gifs_field(list: &[Gif]) -> Vec<u8> {
    let entries = list
        .iter()
        .map(|g| {
            let mut v = Vec::new();
            put_varint(&mut v, 1 << 3);
            put_varint(&mut v, if g.src.contains(".mp4") || g.src.contains(".webm") { 2 } else { 1 });
            put_bytes(&mut v, 2, g.src.as_bytes());
            for (n, x) in [(3, g.w), (4, g.h), (5, g.order)] {
                put_varint(&mut v, n << 3);
                put_varint(&mut v, x as u64);
            }
            (key_str(&g.url), v)
        })
        .collect();
    let mut out = Vec::new();
    put_map(&mut out, 2, entries);
    out
}

pub fn stickers_field(ids: &[Id]) -> Vec<u8> {
    let mut list = Vec::new();
    let packed: Vec<u8> = ids.iter().flat_map(|i| (*i as u64).to_le_bytes()).collect();
    put_bytes(&mut list, 1, &packed);
    let mut out = Vec::new();
    put_bytes(&mut out, 3, &list);
    out
}

pub fn emojis_field(keys: &[String]) -> Vec<u8> {
    let mut list = Vec::new();
    keys.iter().for_each(|k| put_bytes(&mut list, 1, k.as_bytes()));
    let mut out = Vec::new();
    put_bytes(&mut out, 5, &list);
    out
}

fn newest<K>(uses: &BTreeMap<K, Item>) -> Vec<(&K, &Item)> {
    let mut all: Vec<_> = uses.iter().collect();
    all.sort_by_key(|(_, i)| std::cmp::Reverse(i.recent.last().copied().unwrap_or(0)));
    all.truncate(KEEP);
    all
}

pub fn emoji_use_field(uses: &BTreeMap<String, Item>) -> Vec<u8> {
    let mut out = Vec::new();
    put_map(&mut out, 6, newest(uses).into_iter().map(|(k, i)| (key_str(k), put_item(i))).collect());
    out
}

pub fn sticker_use_field(uses: &BTreeMap<Id, Item>) -> Vec<u8> {
    let mut out = Vec::new();
    let entries = newest(uses)
        .into_iter()
        .map(|(k, i)| {
            let mut key = Vec::new();
            put_varint(&mut key, 1 << 3 | 1);
            key.extend_from_slice(&(*k as u64).to_le_bytes());
            (key, put_item(i))
        })
        .collect();
    put_map(&mut out, 4, entries);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400_000;

    #[test]
    fn a_patch_reads_back_as_written() {
        let gifs = vec![Gif { url: "https://tenor.com/a".into(), src: "https://x/a.mp4".into(), w: 320, h: 180, order: 1 }];
        let mut it = Item { frecency: -1, ..Default::default() };
        it.track(5 * DAY);
        it.track(6 * DAY);
        let emoji_use = BTreeMap::from([("123".to_string(), it.clone())]);
        let sticker_use = BTreeMap::from([(777, it.clone())]);
        let mut blob = gifs_field(&gifs);
        blob.extend(stickers_field(&[9, 10]));
        blob.extend(emojis_field(&["smile".into(), "55".into()]));
        blob.extend(emoji_use_field(&emoji_use));
        blob.extend(sticker_use_field(&sticker_use));
        let back = parse(&blob);
        assert_eq!(back.gifs.unwrap(), gifs);
        assert_eq!(back.stickers.unwrap(), vec![9, 10]);
        assert_eq!(back.emojis.unwrap(), vec!["smile", "55"]);
        assert_eq!(back.emoji_use.unwrap(), emoji_use);
        assert_eq!(back.sticker_use.unwrap(), sticker_use);
    }

    #[test]
    fn a_field_left_out_stays_none() {
        let back = parse(&stickers_field(&[1]));
        assert!(back.gifs.is_none() && back.emojis.is_none() && back.emoji_use.is_none() && back.stickers.is_some());
    }

    #[test]
    fn recent_use_outranks_old_use() {
        let now = 100 * DAY;
        let old = Item { total: 9, recent: vec![now - 90 * DAY; 9], frecency: -1, score: 0 };
        let fresh = Item { total: 2, recent: vec![now, now - DAY], frecency: -1, score: 0 };
        let mut uses = BTreeMap::from([("old".to_string(), old), ("fresh".to_string(), fresh)]);
        assert_eq!(frequent(&mut uses, now, 5), vec!["fresh", "old"]);
    }

    #[test]
    fn only_the_last_ten_uses_are_kept() {
        let mut it = Item::default();
        for i in 0..14 {
            it.track(i);
        }
        assert_eq!((it.total, it.recent.len(), it.recent[0]), (14, 10, 4));
    }
}
