use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::state::{Id, Message, Msgs, Store};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct UiState {
    pub guild: Option<Id>,
    pub chan: HashMap<Id, Id>,
    pub collapsed: HashSet<Id>,
    pub recent: Vec<Id>,
    pub hide_members: bool,
    pub open_folders: HashSet<Id>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub notify: bool,
    pub notify_all_dms: bool,
    pub flash: bool,
    pub zoom: f32,
    pub clock24: bool,
    pub dev: bool,
    pub embeds: bool,
    pub mic: Option<String>,
    pub speaker: Option<String>,
    pub vad_db: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { notify: true, notify_all_dms: true, flash: true, zoom: 1.0, clock24: false, dev: false, embeds: true, mic: None, speaker: None, vad_db: -50 }
    }
}

pub fn settings() -> Settings {
    std::fs::read(dir().join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn set_settings(s: &Settings) {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(b) = serde_json::to_vec_pretty(s) {
        let _ = std::fs::write(d.join("settings.json"), b);
    }
}

#[derive(Serialize)]
struct Out<'a> {
    store: &'a Store,
    msgs: Vec<(Id, &'a [Message])>,
    ui: &'a UiState,
}

#[derive(Deserialize)]
struct In {
    store: Store,
    msgs: Vec<(Id, Vec<Message>)>,
    ui: UiState,
}

pub fn dir() -> PathBuf {
    match std::env::var_os("FASTCORD_DATA") {
        Some(d) => PathBuf::from(d),
        None => dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Fastcord"),
    }
}

pub fn load() -> Option<(Store, UiState)> {
    let bytes = std::fs::read(dir().join("state.json")).ok()?;
    let mut i: In = serde_json::from_slice(&bytes).ok()?;
    for (ch, list) in i.msgs {
        i.store.msgs.insert(ch, Msgs { list, ..Default::default() });
    }
    Some((i.store, i.ui))
}

pub fn save(store: &Store, ui: &UiState) {
    let msgs = ui
        .recent
        .iter()
        .filter_map(|c| store.msgs.get(c).map(|m| (*c, m)))
        .map(|(c, m)| {
            let sent = m.list.iter().take_while(|x| !x.pending && !x.failed).count();
            (c, &m.list[sent.saturating_sub(50)..sent])
        })
        .collect();
    let Ok(bytes) = serde_json::to_vec(&Out { store, msgs, ui }) else { return };
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    let tmp = d.join("state.json.part");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(tmp, d.join("state.json"));
    }
}

pub fn token() -> Option<String> {
    if let Ok(t) = std::env::var("FASTCORD_TOKEN") {
        return Some(t);
    }
    let t = std::fs::read_to_string(dir().join("token")).ok()?;
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

pub fn set_token(t: Option<&str>) {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    match t {
        Some(t) => {
            let _ = std::fs::write(d.join("token"), t);
        }
        None => {
            let _ = std::fs::remove_file(d.join("token"));
            let _ = std::fs::remove_file(d.join("state.json"));
        }
    }
}

pub fn window() -> Option<[f32; 2]> {
    let s = std::fs::read_to_string(dir().join("window")).ok()?;
    let mut it = s.split_whitespace().filter_map(|x| x.parse().ok());
    Some([it.next()?, it.next()?])
}

pub fn set_window(size: [f32; 2]) {
    let _ = std::fs::write(dir().join("window"), format!("{} {}", size[0], size[1]));
}
