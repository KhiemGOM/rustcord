use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::state::{Id, Map, Message, Msgs, Set, Store};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct UiState {
    pub guild: Option<Id>,
    pub chan: Map<Id, Id>,
    pub collapsed: Set<Id>,
    pub recent: Vec<Id>,
    // the toolbar's one-click reactions, most recent first
    pub quick: Vec<crate::state::ReactEmoji>,
    pub hide_members: bool,
    // the profile card beside one-on-one dms
    pub hide_dm_profile: bool,
    pub open_folders: Set<Id>,
    // new keys: beta 2 saved its 240 defaults whether or not anyone dragged
    #[serde(rename = "channels_w")]
    pub side_w: f32,
    #[serde(rename = "member_list_w")]
    pub members_w: f32,
    pub search_w: f32,
    pub dm_call_h: f32,
    // past searches, newest first, for the search box's history
    pub searches: Vec<String>,
    // the picker's frequently used emoji, newest first, and its skin tone (0 is the default yellow)
    pub frequent: Vec<crate::state::ReactEmoji>,
    pub skin: u8,
    pub frequent_stickers: Vec<crate::state::Id>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    // notifications
    pub notify: bool,
    pub notify_all_dms: bool,
    pub flash: bool,
    pub badge: bool,
    pub sounds: bool,
    pub muted_sounds: Vec<String>,
    // appearance
    pub theme: crate::theme::Theme,
    pub compact: bool,
    pub compact_avatars: bool,
    pub thread_sidebar: bool,
    pub font_size: f32,
    pub group_space: f32,
    pub zoom: f32,
    pub clock24: bool,
    // accessibility
    pub underline_links: bool,
    pub role_colors: RoleColors,
    pub reduced_motion: bool,
    pub send_button: bool,
    // chat
    pub embeds: bool,
    pub inline_links: bool,
    pub inline_uploads: bool,
    pub reactions: bool,
    // the message menu's one-click reactions when picked by hand; empty follows recent use
    pub quick_reactions: Vec<crate::state::ReactEmoji>,
    pub emoticons: bool,
    pub spoilers: Spoilers,
    pub animate_emoji: bool,
    pub gif_autoplay: bool,
    // advanced
    pub dev: bool,
    // voice
    pub mic: Option<String>,
    pub speaker: Option<String>,
    pub vad_db: i32,
    pub auto_sensitivity: bool,
    pub ptt: bool,
    pub ptt_delay_ms: u32,
    pub input_volume: f32,
    pub output_volume: f32,
    // krisp-style background noise suppression on the mic
    pub noise_suppression: bool,
    // keybinds: (action, windows virtual-key codes)
    pub keybinds: Vec<(String, Vec<u32>)>,
    // language
    pub locale: String,
    // windows
    pub open_on_startup: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    // the tray's "runs in the background" balloon, shown once
    pub tray_hint: bool,
    // streamer mode
    pub streamer: bool,
    pub streamer_auto: bool,
    pub hide_personal: bool,
    pub hide_invites: bool,
    pub streamer_no_sounds: bool,
    pub streamer_no_notifications: bool,
    pub hide_window: bool,
    // go live: the tallest the stream gets (0 for the source's own size) and its frame rate
    pub stream_height: usize,
    pub stream_fps: u32,
    // sites whose links no longer ask "Leaving Discord", ticked in that dialog
    pub trusted: Vec<String>,
    // "Don't ask again" on switching voice channels
    pub voice_switch_ok: bool,
    // emoji and gifs starred in the pickers
    pub fav_emoji: Vec<crate::state::ReactEmoji>,
    pub fav_gifs: Vec<(String, String, f32, f32)>,
    // the hidden ディスコード sounds, from clicking home sixteen times
    pub discodo: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum RoleColors {
    #[default]
    InNames,
    NextToNames,
    Hidden,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Spoilers {
    #[default]
    OnClick,
    Moderated,
    Always,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            notify: true,
            notify_all_dms: true,
            flash: true,
            badge: true,
            sounds: true,
            muted_sounds: Vec::new(),
            theme: Default::default(),
            compact: false,
            compact_avatars: false,
            thread_sidebar: true,
            font_size: 16.0,
            group_space: 16.0,
            zoom: 1.0,
            clock24: false,
            underline_links: false,
            role_colors: RoleColors::InNames,
            reduced_motion: false,
            send_button: false,
            embeds: true,
            inline_links: true,
            inline_uploads: true,
            reactions: true,
            quick_reactions: Vec::new(),
            emoticons: true,
            spoilers: Spoilers::OnClick,
            animate_emoji: true,
            gif_autoplay: true,
            dev: false,
            mic: None,
            speaker: None,
            vad_db: -50,
            auto_sensitivity: true,
            ptt: false,
            ptt_delay_ms: 20,
            input_volume: 1.0,
            output_volume: 1.0,
            noise_suppression: true,
            keybinds: Vec::new(),
            locale: "en-US".into(),
            open_on_startup: false,
            start_minimized: false,
            minimize_to_tray: true,
            tray_hint: false,
            streamer: false,
            streamer_auto: false,
            hide_personal: true,
            hide_invites: true,
            streamer_no_sounds: false,
            streamer_no_notifications: true,
            hide_window: false,
            stream_height: 720,
            stream_fps: 30,
            trusted: Vec::new(),
            voice_switch_ok: false,
            fav_emoji: Vec::new(),
            fav_gifs: Vec::new(),
            discodo: false,
        }
    }
}

impl Settings {
    pub fn hiding_personal(&self) -> bool {
        self.streamer && self.hide_personal
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
    match std::env::var_os("RUSTCORD_DATA") {
        Some(d) => PathBuf::from(d),
        None => dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Rustcord"),
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

/// Serialized here, written on a thread of its own: at a big account's size the snapshot is
/// 12 MB, too much to put on disk from inside a frame. `wait` writes it before returning, for exit.
pub fn save(store: &Store, ui: &UiState, wait: bool) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
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
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let write = move || {
        // a newer snapshot already made it to disk
        let mut last = WRITTEN.lock().unwrap();
        if *last > n {
            return;
        }
        let d = dir();
        let _ = std::fs::create_dir_all(&d);
        let tmp = d.join("state.json.part");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, d.join("state.json"));
        }
        *last = n;
    };
    if wait {
        write();
    } else {
        std::thread::spawn(write);
    }
}

pub fn token() -> Option<String> {
    if let Ok(t) = std::env::var("RUSTCORD_TOKEN") {
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
