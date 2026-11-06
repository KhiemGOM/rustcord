//! Favourite gifs, stickers and emoji and the use counts behind Frequently Used, kept in step with
//! the account's frecency settings (settings-proto type 2): fetched after READY, written back a few
//! seconds after a change, and re-read when another client changes them.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};

use super::App;
use crate::net::Tag;
use crate::proto::frecency::{self, Frecency, Gif, Item};
use crate::state::{Id, ReactEmoji};

const DELAY: Duration = Duration::from_secs(3);
const FREQUENT: usize = 27;
const FREQUENT_STICKERS: usize = 12;
const MAX_FAVOURITES: usize = 250;

type LocalGif = (String, String, f32, f32);

#[derive(Default)]
struct Dirty {
    gifs: bool,
    stickers: bool,
    emojis: bool,
    emoji_use: bool,
    sticker_use: bool,
}

impl Dirty {
    fn any(&self) -> bool {
        self.gifs || self.stickers || self.emojis || self.emoji_use || self.sticker_use
    }
}

#[derive(Default)]
pub struct Favs {
    loaded: bool,
    emoji_use: BTreeMap<String, Item>,
    sticker_use: BTreeMap<Id, Item>,
    kept: Vec<String>,
    gifs: Vec<LocalGif>,
    stickers: Vec<Id>,
    emojis: Vec<ReactEmoji>,
    dirty: Dirty,
    flush_at: Option<Instant>,
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn key(e: &ReactEmoji) -> Option<String> {
    if e.id != 0 {
        return Some(e.id.to_string());
    }
    emojis::get(&e.name).and_then(|u| u.shortcode()).map(str::to_string)
}

fn resolve(app: &App, k: &str) -> Option<ReactEmoji> {
    if let Ok(id) = k.parse::<Id>() {
        let e = app.store.guilds.values().flat_map(|g| &g.emojis).find(|e| e.id == id)?;
        return Some(ReactEmoji { id, name: e.name.clone(), animated: e.animated });
    }
    let u = emojis::get_by_shortcode(k)?;
    Some(ReactEmoji { id: 0, name: u.as_str().to_string(), animated: false })
}

fn local_gifs(list: &[Gif]) -> Vec<LocalGif> {
    list.iter().map(|g| (g.url.clone(), g.src.clone(), g.w as f32, g.h as f32)).collect()
}

fn wire_gifs(list: &[LocalGif]) -> Vec<Gif> {
    list.iter().enumerate().map(|(i, (url, src, w, h))| Gif { url: url.clone(), src: src.clone(), w: *w as u32, h: *h as u32, order: (list.len() - i) as u32 }).collect()
}

pub fn fetch(app: &App) {
    app.rest("GET", "/users/@me/settings-proto/2".into(), None, Tag::Favs);
}

/// What the picker showed before the account was read, as uses of its own, so none of it is lost.
fn seed(app: &mut App) {
    let now = now_ms();
    for (i, e) in app.ui.frequent.clone().iter().enumerate() {
        if let Some(k) = key(e) {
            app.favs.emoji_use.entry(k).or_insert(Item { total: 1, recent: vec![now - i as u64 * 1000], frecency: -1, score: 0 });
        }
    }
    for (i, id) in app.ui.frequent_stickers.clone().iter().enumerate() {
        app.favs.sticker_use.entry(*id).or_insert(Item { total: 1, recent: vec![now - i as u64 * 1000], frecency: -1, score: 0 });
    }
}

pub fn used_emoji(app: &mut App, e: &ReactEmoji) {
    let Some(k) = key(e) else { return };
    if !app.favs.loaded {
        seed(app);
    }
    app.favs.emoji_use.entry(k).or_insert_with(|| Item { frecency: -1, ..Default::default() }).track(now_ms());
    app.favs.dirty.emoji_use = true;
    refresh_frequent(app);
}

pub fn used_sticker(app: &mut App, id: Id) {
    if !app.favs.loaded {
        seed(app);
    }
    app.favs.sticker_use.entry(id).or_insert_with(|| Item { frecency: -1, ..Default::default() }).track(now_ms());
    app.favs.dirty.sticker_use = true;
    refresh_frequent(app);
}

fn refresh_frequent(app: &mut App) {
    let now = now_ms();
    let keys = frecency::frequent(&mut app.favs.emoji_use, now, FREQUENT * 2);
    let list: Vec<ReactEmoji> = keys.iter().filter_map(|k| resolve(app, k)).take(FREQUENT).collect();
    app.ui.frequent = list;
    app.ui.frequent_stickers = frecency::frequent(&mut app.favs.sticker_use, now, FREQUENT_STICKERS);
}

fn merge_uses<K: Ord + Clone>(mine: &mut BTreeMap<K, Item>, theirs: BTreeMap<K, Item>, union: bool) {
    if !union {
        *mine = theirs;
        return;
    }
    for (k, t) in theirs {
        match mine.get_mut(&k) {
            Some(m) => m.merge(&t),
            None => {
                mine.insert(k, t);
            }
        }
    }
}

/// What the account says. The first read keeps what was starred offline; after that a field with a
/// change still waiting to go out is left alone and the rest follow the account.
fn apply(app: &mut App, f: Frecency) {
    let first = !app.favs.loaded;
    if first {
        seed(app);
    }
    if let Some(list) = f.gifs.filter(|_| !app.favs.dirty.gifs) {
        let theirs = local_gifs(&list);
        app.favs.gifs = theirs.clone();
        let extra = app.settings.fav_gifs.iter().filter(|g| first && !theirs.iter().any(|t| t.0 == g.0)).cloned();
        app.settings.fav_gifs = theirs.iter().cloned().chain(extra).collect();
    }
    if let Some(ids) = f.stickers.filter(|_| !app.favs.dirty.stickers) {
        let theirs: Vec<Id> = ids.into_iter().rev().collect();
        app.favs.stickers = theirs.clone();
        let extra: Vec<Id> = app.ui.fav_stickers.iter().filter(|s| first && !theirs.contains(s)).copied().collect();
        app.ui.fav_stickers = theirs.into_iter().chain(extra).collect();
    }
    if let Some(keys) = f.emojis.filter(|_| !app.favs.dirty.emojis) {
        let theirs: Vec<ReactEmoji> = keys.iter().filter_map(|k| resolve(app, k)).collect();
        app.favs.kept = keys.into_iter().filter(|k| resolve(app, k).is_none()).collect();
        app.favs.emojis = theirs.clone();
        let extra: Vec<ReactEmoji> = app.settings.fav_emoji.iter().filter(|e| first && !theirs.contains(e)).cloned().collect();
        app.settings.fav_emoji = theirs.into_iter().chain(extra).collect();
    }
    if let Some(u) = f.emoji_use.filter(|_| !app.favs.dirty.emoji_use) {
        merge_uses(&mut app.favs.emoji_use, u, first);
        app.favs.dirty.emoji_use |= first;
    }
    if let Some(u) = f.sticker_use.filter(|_| !app.favs.dirty.sticker_use) {
        merge_uses(&mut app.favs.sticker_use, u, first);
        app.favs.dirty.sticker_use |= first;
    }
    app.favs.loaded = true;
    refresh_frequent(app);
    crate::cache::set_settings(&app.settings);
}

pub fn loaded(app: &mut App, v: &Value) {
    let Some(raw) = v["settings"].as_str().and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok()) else { return };
    apply(app, frecency::parse(&raw));
}

/// A USER_SETTINGS_PROTO_UPDATE for type 2: a partial one carries only the fields that changed.
pub fn updated(app: &mut App, d: &Value) {
    if d["settings"]["type"].as_u64() != Some(2) || !app.favs.loaded {
        return;
    }
    let Some(raw) = d["settings"]["proto"].as_str().and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok()) else { return };
    apply(app, frecency::parse(&raw));
}

pub fn tick(app: &mut App, ctx: &egui::Context) {
    if !app.favs.loaded {
        return;
    }
    let f = &mut app.favs;
    f.dirty.gifs |= app.settings.fav_gifs != f.gifs;
    f.dirty.stickers |= app.ui.fav_stickers != f.stickers;
    f.dirty.emojis |= app.settings.fav_emoji != f.emojis;
    if !f.dirty.any() {
        return;
    }
    let at = *f.flush_at.get_or_insert_with(|| Instant::now() + DELAY);
    if Instant::now() < at {
        ctx.request_repaint_after(at - Instant::now());
        return;
    }
    f.flush_at = None;
    let mut patch = Vec::new();
    if f.dirty.gifs {
        patch.extend(frecency::gifs_field(&wire_gifs(&app.settings.fav_gifs)));
        f.gifs = app.settings.fav_gifs.clone();
    }
    if f.dirty.stickers {
        let oldest_first: Vec<Id> = app.ui.fav_stickers.iter().rev().copied().take(MAX_FAVOURITES).collect();
        patch.extend(frecency::stickers_field(&oldest_first));
        f.stickers = app.ui.fav_stickers.clone();
    }
    if f.dirty.emojis {
        let mut keys: Vec<String> = app.settings.fav_emoji.iter().filter_map(key).collect();
        keys.extend(f.kept.iter().filter(|k| !keys.contains(k)).cloned().collect::<Vec<_>>());
        keys.truncate(MAX_FAVOURITES);
        patch.extend(frecency::emojis_field(&keys));
        f.emojis = app.settings.fav_emoji.clone();
    }
    if f.dirty.emoji_use {
        f.emoji_use.values_mut().for_each(|i| i.compute(now_ms()));
        patch.extend(frecency::emoji_use_field(&f.emoji_use));
    }
    if f.dirty.sticker_use {
        f.sticker_use.values_mut().for_each(|i| i.compute(now_ms()));
        patch.extend(frecency::sticker_use_field(&f.sticker_use));
    }
    f.dirty = Dirty::default();
    let body = json!({ "settings": base64::engine::general_purpose::STANDARD.encode(&patch) });
    app.rest("PATCH", "/users/@me/settings-proto/2".into(), Some(body), Tag::Favs);
}

impl App {
    pub fn favs_settled(&self) -> bool {
        self.favs.loaded && !self.favs.dirty.any() && self.favs.flush_at.is_none()
    }
}
