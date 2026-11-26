use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::state::{Id, Map, Set, Store};

mod persist;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct UiState {
    pub guild: Option<Id>,
    pub chan: Map<Id, Id>,
    pub collapsed: Set<Id>,
    pub recent: Vec<Id>,
    // apps whose commands were used from the launcher, latest first
    pub recent_apps: Vec<Id>,
    // the toolbar's one-click reactions, most recent first
    pub quick: Vec<crate::state::ReactEmoji>,
    pub hide_members: bool,
    // the profile card beside one-on-one dms
    pub hide_dm_profile: bool,
    pub open_folders: Set<Id>,
    pub voice_open: Set<Id>,
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
    pub fav_stickers: Vec<crate::state::Id>,
    // discord's survey notice: when it last asked for one and pinged one seen (ms), and the ones closed
    pub survey_fetched: i64,
    pub survey_seen: i64,
    pub surveys_hidden: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    // notifications
    pub notify: bool,
    pub flash: bool,
    pub badge: bool,
    pub sounds: bool,
    pub muted_sounds: Vec<String>,
    // the message sound for every message in the channel that's open, not only pings
    pub notify_selected: bool,
    // speak incoming messages out loud: 0 never, 1 in the open channel, 2 in every channel
    pub tts_notify: u8,
    // the account's notification choices: reactions (0 all, 1 dms only, 2 never) and who may notify
    pub reaction_notify: u8,
    pub stream_notify: bool,
    pub friend_anniv: bool,
    pub friend_online: bool,
    pub server_events: bool,
    pub profile_updates: bool,
    // seconds idle on the desktop before the phone gets pushes, and /tts playback
    pub push_delay: u32,
    pub tts_command: bool,
    // video goes through the card's encoder and decoder
    pub hw_accel: bool,
    // appearance
    pub theme: crate::theme::Theme,
    pub custom_theme: Option<u8>,
    pub own_theme: Option<crate::theme::Custom>,
    // the in-app icon's id, as discord names them
    pub app_icon: String,
    // servers that made a theme show it to people who haven't picked "Use my theme"
    pub server_themes: bool,
    pub dark_sidebar: bool,
    pub hub_seen: bool,
    // the appearance message the account last sent, so an update keeps the fields we don't know
    #[serde(skip)]
    pub appearance_raw: Vec<u8>,
    pub sync_theme: bool,
    pub own_theme_on_profiles: bool,
    pub compact: bool,
    pub compact_avatars: bool,
    pub thread_sidebar: bool,
    pub font_size: f32,
    pub group_space: f32,
    pub zoom: f32,
    // 0 follows windows, 1 is 12-hour, 2 is 24-hour: the account's timestamp hour cycle
    pub hour_cycle: u8,
    // accessibility
    pub underline_links: bool,
    pub name_styles: bool,
    pub density: Density,
    pub saturation: f32,
    pub saturate_custom: bool,
    pub high_contrast: bool,
    pub sync_contrast: bool,
    pub switch_icons: bool,
    pub role_colors: RoleColors,
    pub reduced_motion: bool,
    pub sync_motion: bool,
    pub min_toast_secs: f32,
    pub stickers: Animate,
    pub tts_rate: f32,
    pub image_descriptions: bool,
    pub send_button: bool,
    // the shop's own pointer, the plain chat input, games in the autocomplete
    pub shop_cursor: bool,
    pub legacy_input: bool,
    pub game_suggestions: bool,
    // stream details on video tiles under the pointer, and the application put in test mode
    pub stream_info: bool,
    pub test_app: Option<TestApp>,
    pub md_preview: bool,
    pub sticker_suggestions: bool,
    pub hide_activity_cards: bool,
    pub search_all_dms: bool,
    // chat
    pub embeds: bool,
    pub inline_links: bool,
    pub inline_uploads: bool,
    pub reactions: bool,
    // the message menu's one-click reactions when picked by hand; empty follows recent use
    pub quick_reactions: Vec<crate::state::ReactEmoji>,
    pub emoticons: bool,
    pub spoilers: Spoilers,
    /// The sensitive content filters for mature and for graphic media: what to do with it in a
    /// friend's DM, anyone else's, and in servers (1 show, 2 blur, 3 block).
    pub explicit: [u8; 3],
    pub gore: [u8; 3],
    pub animate_emoji: bool,
    pub gif_autoplay: bool,
    // the inbox's filter: @here and @everyone, role mentions, and every server rather than the one open
    pub inbox_everyone: bool,
    pub inbox_roles: bool,
    pub inbox_all: bool,
    // the unreads tab's first-visit card has been dismissed
    pub inbox_tip: bool,
    pub media_volume: f32,
    pub media_muted: bool,
    // advanced
    pub dev: bool,
    // voice channels whose people show as avatars only
    pub hide_names: Vec<crate::state::Id>,
    // voice
    pub mic: Option<String>,
    pub speaker: Option<String>,
    pub camera: Option<String>,
    pub preview_video: bool,
    pub video_bg: VideoBg,
    pub custom_bg: Option<String>,
    pub vad_db: i32,
    pub auto_sensitivity: bool,
    pub ptt: bool,
    pub ptt_delay_ms: u32,
    pub input_volume: f32,
    pub output_volume: f32,
    pub input_profile: InputProfile,
    pub noise: Noise,
    pub echo_cancel: bool,
    pub agc: bool,
    pub adv_vad: bool,
    pub silence_warning: bool,
    // how far other apps duck, in percent, and when
    pub attenuation: u32,
    pub atten_self: bool,
    pub atten_others: bool,
    pub qos: bool,
    pub openh264: bool,
    pub debug_log: bool,
    pub aec_dump: bool,
    pub stream_previews: bool,
    pub soundboard_volume: f32,
    // (guild, sound, the guild it came from) to play on joining that guild's calls; guild 0 is all servers
    pub entrance: Vec<(Id, Id, Id)>,
    pub theme_prefs: Vec<(Id, u8)>,
    pub guilds_raw: Vec<u8>,
    // keybinds: (action, windows virtual-key codes)
    pub keybinds: Vec<(String, Vec<u32>)>,
    // language
    pub locale: String,
    // the composer's red squiggles, and the words added from their menu (lowercase)
    pub spellcheck: bool,
    pub learned_words: Vec<String>,
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
    // go live: discord's stream mode (1 gaming, 2 screenshare, 3 custom), and custom's tallest
    // picture (0 for the source's own size) and frame rate
    pub stream_preset: u8,
    pub stream_height: usize,
    pub stream_fps: u32,
    // clips: on or off, seconds kept, the reminder after a stream, and where they are written ("" is the default folder)
    pub clips: bool,
    pub clips_secs: u32,
    pub clips_reminders: bool,
    pub clips_dir: String,
    pub clips_voice: bool,
    // sites whose links no longer ask "Leaving Discord", ticked in that dialog
    pub trusted: Vec<String>,
    // "Don't ask again" on switching voice channels
    pub voice_switch_ok: bool,
    // what each person in a call is played at on this device
    pub peers: Map<Id, Peer>,
    pub alerts_off: Map<Id, [bool; 2]>,
    // the call's More Options: names on the tiles, and tiles for people without video
    pub hide_call_names: bool,
    pub hide_idle_tiles: bool,
    // the activities launched last, newest first, for the shelf's Recents
    pub recent_activities: Vec<Id>,
    // the first-visit modal on a stage channel
    pub stage_welcome: bool,
    // the per-server profile notice in the profile editor, dismissed
    pub profed_notice: bool,
    // Mute Music in a stage's tray, kept on this device
    pub stage_music_muted: bool,
    // emoji and gifs starred in the pickers
    pub fav_emoji: Vec<crate::state::ReactEmoji>,
    pub fav_gifs: Vec<(String, String, f32, f32)>,
    // the hidden ディスコード sounds, from clicking home sixteen times
    pub discodo: bool,
    // discord's halloween sound pack, while it runs
    pub halloween_sounds: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct Peer {
    pub volume: f32,
    pub muted: bool,
    pub soundboard_muted: bool,
    pub modified: u64,
}

impl Default for Peer {
    fn default() -> Self {
        Peer { volume: 100.0, muted: false, soundboard_muted: false, modified: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum VideoBg {
    #[default]
    None,
    Blur,
    Preset(u8),
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum RoleColors {
    #[default]
    InNames,
    NextToNames,
    Hidden,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Density {
    Compact,
    #[default]
    Default,
    Spacious,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Animate {
    #[default]
    Always,
    OnInteraction,
    Never,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum InputProfile {
    VoiceIsolation,
    Studio,
    #[default]
    Custom,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TestApp {
    pub id: String,
    pub proxy: bool,
    pub port: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum Noise {
    #[default]
    Krisp,
    Standard,
    None,
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
            flash: true,
            badge: true,
            sounds: true,
            muted_sounds: Vec::new(),
            notify_selected: false,
            tts_notify: 0,
            reaction_notify: 0,
            stream_notify: true,
            friend_anniv: true,
            friend_online: true,
            server_events: true,
            profile_updates: true,
            push_delay: 60,
            tts_command: true,
            hw_accel: true,
            theme: Default::default(),
            custom_theme: None,
            own_theme: None,
            app_icon: "Rustcord".into(),
            server_themes: true,
            dark_sidebar: false,
            hub_seen: false,
            appearance_raw: Vec::new(),
            sync_theme: true,
            own_theme_on_profiles: false,
            compact: false,
            compact_avatars: false,
            thread_sidebar: true,
            font_size: 16.0,
            group_space: 16.0,
            zoom: 1.0,
            hour_cycle: 0,
            underline_links: false,
            name_styles: true,
            density: Density::Default,
            saturation: 1.0,
            saturate_custom: false,
            high_contrast: false,
            sync_contrast: true,
            switch_icons: false,
            role_colors: RoleColors::InNames,
            reduced_motion: false,
            sync_motion: true,
            min_toast_secs: 5.0,
            stickers: Animate::Always,
            tts_rate: 1.0,
            image_descriptions: false,
            shop_cursor: true,
            legacy_input: false,
            game_suggestions: true,
            stream_info: false,
            test_app: None,
            send_button: false,
            md_preview: true,
            sticker_suggestions: false,
            hide_activity_cards: false,
            search_all_dms: false,
            embeds: true,
            inline_links: true,
            inline_uploads: true,
            reactions: true,
            quick_reactions: Vec::new(),
            emoticons: true,
            spoilers: Spoilers::OnClick,
            explicit: [1, 2, 1],
            gore: [2, 3, 2],
            animate_emoji: true,
            gif_autoplay: true,
            inbox_everyone: true,
            inbox_roles: true,
            inbox_all: true,
            inbox_tip: false,
            media_volume: 1.0,
            media_muted: false,
            dev: false,
            hide_names: Vec::new(),
            mic: None,
            speaker: None,
            camera: None,
            preview_video: false,
            video_bg: VideoBg::None,
            custom_bg: None,
            vad_db: -50,
            auto_sensitivity: true,
            ptt: false,
            ptt_delay_ms: 20,
            input_volume: 1.0,
            output_volume: 1.0,
            input_profile: InputProfile::Custom,
            noise: Noise::Krisp,
            echo_cancel: true,
            agc: true,
            adv_vad: true,
            silence_warning: true,
            attenuation: 0,
            atten_self: false,
            atten_others: true,
            qos: false,
            openh264: true,
            debug_log: false,
            aec_dump: false,
            stream_previews: true,
            soundboard_volume: 1.0,
            entrance: Vec::new(),
            theme_prefs: Vec::new(),
            guilds_raw: Vec::new(),
            keybinds: Vec::new(),
            locale: "en-US".into(),
            spellcheck: true,
            learned_words: Vec::new(),
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
            stream_preset: 1,
            stream_height: 720,
            stream_fps: 30,
            clips: false,
            clips_secs: 30,
            clips_reminders: true,
            clips_dir: String::new(),
            clips_voice: true,
            trusted: Vec::new(),
            voice_switch_ok: false,
            peers: Map::default(),
            alerts_off: Map::default(),
            hide_call_names: false,
            hide_idle_tiles: false,
            recent_activities: Vec::new(),
            stage_welcome: false,
            profed_notice: false,
            stage_music_muted: false,
            fav_emoji: Vec::new(),
            fav_gifs: Vec::new(),
            discodo: false,
            halloween_sounds: true,
        }
    }
}

impl Settings {
    pub fn clock24(&self) -> bool {
        match self.hour_cycle {
            0 => crate::winsys::system_24h(),
            c => c == 2,
        }
    }

    pub fn hiding_personal(&self) -> bool {
        self.streamer && self.hide_personal
    }

    pub fn noise_on(&self) -> bool {
        match self.input_profile {
            InputProfile::VoiceIsolation => true,
            InputProfile::Studio => false,
            InputProfile::Custom => self.noise != Noise::None,
        }
    }

    pub fn set_noise(&mut self, on: bool) {
        self.input_profile = InputProfile::Custom;
        self.noise = if on { Noise::Krisp } else { Noise::None };
    }

    /// Back to Discord's defaults for everything on Voice & Video, devices included.
    pub fn reset_voice(&mut self) {
        let d = Settings::default();
        self.mic = d.mic;
        self.speaker = d.speaker;
        self.vad_db = d.vad_db;
        self.auto_sensitivity = d.auto_sensitivity;
        self.ptt = d.ptt;
        self.ptt_delay_ms = d.ptt_delay_ms;
        self.input_volume = d.input_volume;
        self.output_volume = d.output_volume;
        self.input_profile = d.input_profile;
        self.noise = d.noise;
        self.echo_cancel = d.echo_cancel;
        self.agc = d.agc;
        self.adv_vad = d.adv_vad;
        self.silence_warning = d.silence_warning;
        self.attenuation = d.attenuation;
        self.atten_self = d.atten_self;
        self.atten_others = d.atten_others;
        self.qos = d.qos;
        self.openh264 = d.openh264;
        self.debug_log = d.debug_log;
        self.aec_dump = d.aec_dump;
        self.stream_previews = d.stream_previews;
        self.soundboard_volume = d.soundboard_volume;
        self.voice_switch_ok = d.voice_switch_ok;
        self.muted_sounds.retain(|m| !crate::sound::VOICE_EVENTS.iter().any(|(k, _)| k == m));
    }
}

pub fn settings() -> Settings {
    persist::flush();
    let mut s: Settings = std::fs::read(dir().join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    rename_muted(&mut s.muted_sounds);
    s
}

fn rename_muted(muted: &mut [String]) {
    for m in muted {
        match m.as_str() {
            "join" => *m = "user_join".into(),
            "leave" => *m = "user_leave".into(),
            "ring" => *m = "call_ringing".into(),
            _ => {}
        }
    }
}

pub fn set_settings(s: &Settings) {
    persist::write_settings(dir(), s);
}

pub fn dir() -> PathBuf {
    #[cfg(any(test, feature = "e2e"))]
    if let Some(d) = std::env::var_os("RUSTCORD_DATA") {
        return PathBuf::from(d);
    }
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Rustcord")
}

pub fn load() -> Option<(Store, UiState)> {
    persist::load_dir(&dir())
}

pub fn save(store: &Store, ui: &UiState, wait: bool) {
    persist::save_snapshot(dir(), store, ui, wait);
}

pub fn save_ui(ui: &UiState) {
    persist::save_ui(dir(), ui);
}

pub fn append_journal(store: &Store, ui: &UiState, reads: &Set<Id>, last_msgs: &Set<Id>, msgs: &Set<Id>, include_ui: bool) {
    persist::append_journal(dir(), store, ui, reads, last_msgs, msgs, include_ui);
}

pub fn compact_due() -> bool {
    persist::compact_due()
}

pub fn flush() {
    persist::flush();
}

const DPAPI_PREFIX: &str = "dpapi:v1:";

fn seal_token(plain: &str) -> Option<String> {
    use base64::Engine;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData};
    let input = CRYPT_INTEGER_BLOB { cbData: u32::try_from(plain.len()).ok()?, pbData: plain.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) };
    if ok == 0 || output.pbData.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let encoded = format!("{DPAPI_PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(slice));
    unsafe { LocalFree(output.pbData as _) };
    Some(encoded)
}

fn unseal_token(stored: &str) -> Option<(String, bool)> {
    use base64::Engine;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData};
    let trimmed = stored.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Some(b64) = trimmed.strip_prefix(DPAPI_PREFIX) else {
        return Some((trimmed.to_string(), true));
    };
    let raw = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let input = CRYPT_INTEGER_BLOB { cbData: u32::try_from(raw.len()).ok()?, pbData: raw.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) };
    if ok == 0 || output.pbData.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as _) };
    let plain = String::from_utf8(bytes).ok()?;
    let plain = plain.trim().to_string();
    (!plain.is_empty()).then_some((plain, false))
}

fn write_token_file(path: &std::path::Path, token: &str) {
    match seal_token(token) {
        Some(sealed) => {
            let _ = std::fs::write(path, sealed);
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn token() -> Option<String> {
    #[cfg(any(test, feature = "e2e"))]
    if let Ok(t) = std::env::var("RUSTCORD_TOKEN") {
        return Some(t);
    }
    let path = dir().join("token");
    let raw = std::fs::read_to_string(&path).ok()?;
    let (plain, legacy) = unseal_token(&raw)?;
    if legacy {
        write_token_file(&path, &plain);
    }
    Some(plain)
}

pub fn set_token(t: Option<&str>) {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    match t {
        Some(t) => write_token_file(&d.join("token"), t),
        None => {
            persist::clear_session(d);
        }
    }
}

/// An account signed in on this machine besides the one in `token`: who it is, kept beside its
/// token in the data folder the way the main token is.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedAccount {
    pub id: Id,
    pub name: String,
    pub avatar: Option<String>,
}

fn accounts_dir() -> PathBuf {
    dir().join("accounts")
}

pub fn accounts() -> Vec<SavedAccount> {
    let Ok(list) = std::fs::read_dir(accounts_dir()) else { return Vec::new() };
    let mut out: Vec<SavedAccount> = list
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    out.sort_by_key(|a| a.id);
    out
}

pub fn account_token(id: Id) -> Option<String> {
    let path = accounts_dir().join(format!("{id}.token"));
    let raw = std::fs::read_to_string(&path).ok()?;
    let (plain, legacy) = unseal_token(&raw)?;
    if legacy {
        write_token_file(&path, &plain);
    }
    Some(plain)
}

pub fn remember_account(a: &SavedAccount, token: &str) {
    let d = accounts_dir();
    let _ = std::fs::create_dir_all(&d);
    write_token_file(&d.join(format!("{}.token", a.id)), token);
    if let Ok(b) = serde_json::to_vec(a) {
        let _ = std::fs::write(d.join(format!("{}.json", a.id)), b);
    }
}

pub fn forget_account(id: Id) {
    let d = accounts_dir();
    let _ = std::fs::remove_file(d.join(format!("{id}.token")));
    let _ = std::fs::remove_file(d.join(format!("{id}.json")));
}

pub fn window() -> Option<[f32; 2]> {
    let s = std::fs::read_to_string(dir().join("window")).ok()?;
    let mut it = s.split_whitespace().filter_map(|x| x.parse().ok());
    Some([it.next()?, it.next()?])
}

/// The theme's background as it last was, to open the window in before anything can be drawn.
pub fn window_color() -> [u8; 3] {
    let s = std::fs::read_to_string(dir().join("window")).unwrap_or_default();
    let mut it = s.split_whitespace().skip(2).filter_map(|x| x.parse().ok());
    match (it.next(), it.next(), it.next()) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [0x1a, 0x1a, 0x1e],
    }
}

pub fn set_window(size: [f32; 2], color: [u8; 3]) {
    let _ = std::fs::write(dir().join("window"), format!("{} {} {} {} {}", size[0], size[1], color[0], color[1], color[2]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_muted_sound_names_move_to_discords() {
        let mut m = ["join", "leave", "ring", "mute"].map(String::from);
        rename_muted(&mut m);
        assert_eq!(m, ["user_join", "user_leave", "call_ringing", "mute"]);
    }

    #[test]
    fn callbar_noise_switch_follows_the_profile() {
        let mut s = Settings::default();
        assert!(s.noise_on());
        s.set_noise(false);
        assert!(s.input_profile == InputProfile::Custom && s.noise == Noise::None && !s.noise_on());
        s.input_profile = InputProfile::Studio;
        assert!(!s.noise_on());
        s.input_profile = InputProfile::VoiceIsolation;
        assert!(s.noise_on());
        s.input_profile = InputProfile::Studio;
        s.set_noise(true);
        assert!(s.input_profile == InputProfile::Custom && s.noise_on());
    }

    #[test]
    fn dpapi_token_round_trips_and_never_stores_plaintext() {
        let secret = "mfa.test-secret-token-value";
        let sealed = seal_token(secret).expect("dpapi seal");
        assert!(sealed.starts_with(DPAPI_PREFIX));
        assert!(!sealed.contains(secret));
        assert_eq!(unseal_token(&sealed), Some((secret.to_string(), false)));
        assert_eq!(unseal_token(secret), Some((secret.to_string(), true)));
    }
}
