//! Discord's own sounds. They're fetched from discord.com on first start, cached next to the
//! fonts and decoded once, off the ui thread. The sine tones at the bottom are only what plays
//! while a file isn't on disk yet.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;

use crate::{api, cache};

/// (sound, asset): the sounds the client plays, with the asset each had in the build these
/// were read from. Asset names change with every discord deploy; they only save the lookup
/// while they're current.
const CLASSIC: [(&str, &str); 23] = [
    ("message1", "3ed22d14f3c30bc4"), ("deafen", "529ff198eac567af"), ("undeafen", "b150f03c89944403"), ("mute", "2d3b4ba32c34c862"),
    ("unmute", "e74c4a06134a20e4"), ("disconnect", "dd0911025fe00b2e"), ("ptt_start", "f5c8ec31889c5317"), ("ptt_stop", "34b5969be3a063ba"),
    ("user_join", "b135ff6c8e091b43"), ("user_leave", "7b9a183742515fc2"), ("user_moved", "4003230e4086b680"), ("call_calling", "5146737af413d88e"),
    ("call_ringing", "c2a7111bb44b8da0"), ("call_ringing_beat", "574220f72e718dad"), ("call_ringing_snow_halation", "82954088613583bc"),
    ("stream_started", "abe52a3c92953edb"), ("stream_ended", "cbff07b5a83e0321"), ("stream_user_joined", "3d2af2764581bb2e"),
    ("stream_user_left", "24ae9dc0184bd570"), ("reconnect", "7dfc49b36dea3a6c"), ("camera_on", "855607d0932ea396"),
    ("camera_off", "b1dd204729fe78b4"), ("discodo", "cb85be48267eb1b4"),
];

const HALLOWEEN: [(&str, &str); 10] = [
    ("halloween_call_calling", "ef24e229a3413c8a"), ("halloween_call_ringing", "7a287e2f88531f25"), ("halloween_message1", "c269de500ca3cb7b"),
    ("halloween_deafen", "514663b657bc4c7d"), ("halloween_undeafen", "4dfa756a71484219"), ("halloween_mute", "d5e69d2c2aac0a27"),
    ("halloween_unmute", "6f0d538ed066a1a6"), ("halloween_disconnect", "9b8ef0a59d731dd6"), ("halloween_user_join", "00b48ea9fc787786"),
    ("halloween_user_leave", "c9b6b9d1ef992f1b"),
];

/// The sounds the halloween pack swaps out; anything else keeps its classic file.
const HALLOWEEN_SWAPS: [&str; 10] = ["call_calling", "call_ringing", "message1", "deafen", "undeafen", "mute", "unmute", "disconnect", "user_join", "user_leave"];

/// Discord's holiday window for the pack, in unix ms (7 Oct to 2 Nov 2026).
const HALLOWEEN_FROM: u64 = 1_791_388_800_000;
const HALLOWEEN_TO: u64 = 1_793_638_800_000;

/// The call sounds Voice & Video lets you switch off one by one, as (cue, Discord's label).
pub const VOICE_EVENTS: [(&str, &str); 17] = [
    ("deafen", "Deafen"),
    ("undeafen", "Undeafen"),
    ("mute", "Mute"),
    ("unmute", "Unmute"),
    ("camera_on", "Camera On"),
    ("camera_off", "Camera Off"),
    ("disconnect", "Voice Disconnected"),
    ("ptt_start", "PTT Activate"),
    ("ptt_stop", "PTT Deactivate"),
    ("user_join", "User Join"),
    ("user_leave", "User Leave"),
    ("user_moved", "User Moved"),
    ("call_calling", "Outgoing Ring"),
    ("stream_started", "Stream Started"),
    ("stream_ended", "Stream Stopped"),
    ("stream_user_joined", "Viewer Join"),
    ("stream_user_left", "Viewer Leave"),
];

/// Volume discord plays its sounds at before the speaker volume scales it.
const LEVEL: f32 = 0.4;

struct Config {
    on: bool,
    muted: Vec<String>,
    discodo: bool,
    halloween: bool,
    speaker: Option<String>,
    volume: f32,
}

static CFG: Mutex<Config> = Mutex::new(Config { on: true, muted: Vec::new(), discodo: false, halloween: false, speaker: None, volume: 1.0 });
static LOOPS: Mutex<Vec<(String, Arc<AtomicBool>)>> = Mutex::new(Vec::new());
static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CLIPS: Mutex<Option<HashMap<String, Arc<Clip>>>> = Mutex::new(None);
static FETCHED: AtomicUsize = AtomicUsize::new(0);
static DECODED: AtomicUsize = AtomicUsize::new(0);
static QUEUED: AtomicUsize = AtomicUsize::new(0);

struct Clip {
    pcm: Vec<f32>,
    ch: usize,
    rate: usize,
}

fn origin() -> String {
    std::env::var("RUSTCORD_SOUNDS").unwrap_or_else(|_| "https://discord.com".into())
}

fn path(name: &str) -> PathBuf {
    cache::dir().join("sounds").join(format!("{name}.mp3"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// True while discord is shipping the halloween pack, which is when the settings page offers it.
pub fn halloween_season() -> bool {
    (HALLOWEEN_FROM..=HALLOWEEN_TO).contains(&now_ms())
}

/// Every cue that was allowed to play, oldest first, for the e2e run to check.
pub fn heard() -> Vec<String> {
    HEARD.lock().map(|h| h.clone()).unwrap_or_default()
}

fn note(cue: &str) {
    if let Ok(mut h) = HEARD.lock() {
        h.push(cue.to_string());
        if h.len() > 256 {
            h.remove(0);
        }
    }
}

/// (files fetched, clips decoded, plays queued) since start, for the e2e checks.
pub fn stats() -> (usize, usize, usize) {
    (FETCHED.load(Relaxed), DECODED.load(Relaxed), QUEUED.load(Relaxed))
}

/// What the settings say about sounds.
pub fn configure(s: &cache::Settings) {
    if let Ok(mut c) = CFG.lock() {
        *c = Config {
            on: s.sounds && !(s.streamer && s.streamer_no_sounds),
            muted: s.muted_sounds.clone(),
            discodo: s.discodo,
            halloween: s.halloween_sounds && halloween_season(),
            speaker: s.speaker.clone(),
            volume: s.output_volume,
        };
    }
}

/// Cues are discord's sound names, the settings list included; only `message` differs.
fn sound_of(cue: &str) -> &str {
    if cue == "message" { "message1" } else { cue }
}

/// The file behind a cue under the current pack: discodo turns every message into the
/// ディスコード jingle, halloween swaps ten sounds. A one in a thousand ring is a secret tune.
fn file_of(cue: &str, c: &Config, dice: u32) -> String {
    let s = sound_of(cue);
    if s == "message1" && c.discodo {
        return "discodo".into();
    }
    if c.halloween && HALLOWEEN_SWAPS.contains(&s) {
        return format!("halloween_{s}");
    }
    if s == "call_ringing" && dice % 1000 + 1 == 500 {
        return if dice / 1000 % 2 == 0 { "call_ringing_beat" } else { "call_ringing_snow_halation" }.into();
    }
    s.into()
}

fn dice() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(now_ms());
    h.finish() as u32
}

fn allowed(cue: &str) -> bool {
    CFG.lock().map(|c| c.on && !c.muted.iter().any(|m| m == cue)).unwrap_or(true)
}

/// Play a cue unless the user turned it (or all sounds) off.
pub fn cue(name: &str) {
    if allowed(name) {
        note(name);
        play(name);
    }
}

/// Keep a cue looping on its own length until `release`. Called every frame while a call rings.
pub fn hold(name: &str) {
    let Ok(mut loops) = LOOPS.lock() else { return };
    let at = loops.iter().position(|(n, _)| n == name);
    match (allowed(name), at) {
        (true, None) => {
            let stop = Arc::new(AtomicBool::new(false));
            loops.push((name.to_string(), stop.clone()));
            note(name);
            spawn(name.to_string(), Some(stop));
        }
        (false, Some(i)) => loops.remove(i).1.store(true, Relaxed),
        _ => {}
    }
}

pub fn release(name: &str) {
    let Ok(mut loops) = LOOPS.lock() else { return };
    if let Some(i) = loops.iter().position(|(n, _)| n == name) {
        loops.remove(i).1.store(true, Relaxed);
    }
}

/// Play a cue regardless of the mutes, for the settings page's preview button.
pub fn play(name: &str) {
    spawn(name.to_string(), None);
}

fn spawn(cue: String, stop: Option<Arc<AtomicBool>>) {
    std::thread::spawn(move || {
        let (file, speaker, volume) = {
            let Ok(c) = CFG.lock() else { return };
            (file_of(&cue, &c, if cue == "call_ringing" { dice() } else { 0 }), c.speaker.clone(), c.volume)
        };
        let (clip, gain) = match load(&file) {
            Some(c) => (c, (volume * LEVEL).min(1.0)),
            None => (Arc::new(synth(&cue, 48_000)), volume.min(1.0)),
        };
        if clip.pcm.is_empty() {
            return;
        }
        QUEUED.fetch_add(1, Relaxed);
        if std::env::var_os("RUSTCORD_SILENT").is_some() {
            if let Some(stop) = stop {
                loop_silently(&cue, &clip, &stop);
            }
            return;
        }
        output(&cue, &clip, gain, speaker.as_deref(), stop);
    });
}

fn loop_silently(cue: &str, clip: &Clip, stop: &AtomicBool) {
    let every = Duration::from_secs_f32(clip.pcm.len() as f32 / (clip.ch * clip.rate) as f32);
    let mut last = std::time::Instant::now();
    while !stop.load(Relaxed) {
        if last.elapsed() >= every {
            last = std::time::Instant::now();
            note(cue);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn load(file: &str) -> Option<Arc<Clip>> {
    let mut clips = CLIPS.lock().ok()?;
    let clips = clips.get_or_insert_with(HashMap::new);
    if let Some(c) = clips.get(file) {
        return Some(c.clone());
    }
    let p = path(file);
    let bytes = std::fs::read(&p).ok()?;
    let Some(clip) = decode(&bytes) else {
        let _ = std::fs::remove_file(&p);
        return None;
    };
    DECODED.fetch_add(1, Relaxed);
    let clip = Arc::new(clip);
    clips.insert(file.to_string(), clip.clone());
    Some(clip)
}

fn decode(bytes: &[u8]) -> Option<Clip> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::{FormatOptions, TrackType, probe::Hint};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    let src = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes.to_vec())), Default::default());
    let mut fmt = symphonia::default::get_probe().probe(&Hint::new(), src, FormatOptions::default(), MetadataOptions::default()).ok()?;
    let track = fmt.default_track(TrackType::Audio)?;
    let id = track.id;
    let params = track.codec_params.as_ref()?.audio()?;
    let mut dec = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).ok()?;
    let (mut pcm, mut ch, mut rate) = (Vec::new(), 0, 0);
    let mut buf: Vec<f32> = Vec::new();
    while let Ok(Some(pkt)) = fmt.next_packet() {
        if pkt.track_id != id {
            continue;
        }
        let Ok(audio) = dec.decode(&pkt) else { continue };
        ch = audio.spec().channels().count();
        rate = audio.spec().rate() as usize;
        buf.resize(audio.samples_interleaved(), 0.0);
        audio.copy_to_slice_interleaved(&mut buf);
        pcm.extend_from_slice(&buf);
    }
    (ch > 0 && rate > 0 && !pcm.is_empty()).then_some(Clip { pcm, ch, rate })
}

fn output(cue: &str, clip: &Clip, gain: f32, speaker: Option<&str>, stop: Option<Arc<AtomicBool>>) {
    let host = cpal::default_host();
    let named = speaker.and_then(|n| host.output_devices().ok()?.find(|d| d.description().is_ok_and(|d| d.name() == n)));
    let Some(dev) = named.or_else(|| host.default_output_device()) else { return };
    let Ok(cfg) = dev.default_output_config() else { return };
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return;
    }
    let conf = cfg.config();
    let mut pcm = crate::audioplay::resample(&clip.pcm, clip.ch, clip.rate, conf.sample_rate as usize, conf.channels as usize);
    pcm.iter_mut().for_each(|s| *s *= gain);
    let looping = stop.is_some();
    let len = pcm.len().max(1);
    let at = Arc::new(AtomicUsize::new(0));
    let (p, pos) = (Arc::new(pcm), at.clone());
    let stream = dev.build_output_stream::<f32, _, _>(
        conf,
        move |data, _| {
            for s in data.iter_mut() {
                let i = pos.fetch_add(1, Relaxed);
                *s = p.get(if looping { i % len } else { i }).copied().unwrap_or(0.0);
            }
        },
        |_| {},
        None,
    );
    let Ok(stream) = stream else { return };
    if stream.play().is_err() {
        return;
    }
    let stopped = || stop.as_ref().is_some_and(|s| s.load(Relaxed));
    let mut plays = 0;
    while !stopped() && (looping || at.load(Relaxed) < len) {
        let n = at.load(Relaxed) / len;
        if n > plays {
            plays = n;
            note(cue);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(60));
}

/// Downloads whatever of discord's sounds isn't cached yet, in the background.
pub fn fetch(rt: &Runtime) {
    let mut wants: Vec<(String, String)> = CLASSIC.iter().map(|&(n, h)| (n.to_string(), h.to_string())).collect();
    if halloween_season() {
        wants.extend(HALLOWEEN.iter().map(|&(n, h)| (n.to_string(), h.to_string())));
    }
    wants.retain(|(n, _)| !path(n).exists());
    if wants.is_empty() {
        return;
    }
    rt.spawn(async move {
        let http = api::http();
        let base = origin();
        let mut found: Option<HashMap<String, String>> = None;
        let mut jobs = futures_util::stream::iter(wants).map(|(name, seed)| {
            let (http, base) = (http.clone(), base.clone());
            async move { (name, get(&http, &format!("{base}/assets/{seed}.mp3")).await) }
        }).buffer_unordered(6);
        let mut missed = Vec::new();
        while let Some((name, bytes)) = jobs.next().await {
            match bytes {
                Some(b) => save(&name, &b),
                None => missed.push(name),
            }
        }
        for name in missed {
            if found.is_none() {
                found = Some(discover(&http, &base).await.unwrap_or_default());
            }
            if let Some(h) = found.as_ref().and_then(|f| f.get(&name))
                && let Some(b) = get(&http, &format!("{base}/assets/{h}.mp3")).await
            {
                save(&name, &b);
            }
        }
    });
}

fn save(name: &str, bytes: &[u8]) {
    let p = path(name);
    let Some(dir) = p.parent() else { return };
    let tmp = p.with_extension("part");
    if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &p).is_ok() {
        FETCHED.fetch_add(1, Relaxed);
    }
}

async fn get(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let res = http.get(url).send().await.ok()?.error_for_status().ok()?;
    Some(res.bytes().await.ok()?.to_vec())
}

/// The client's sound table lives in the app bundle: a context map from file name to module id,
/// and a module per id exporting the asset. Looked for in /app's scripts, once.
async fn discover(http: &reqwest::Client, base: &str) -> Option<HashMap<String, String>> {
    let page = String::from_utf8(get(http, &format!("{base}/app")).await?).ok()?;
    let scripts: Vec<String> = page
        .split("src=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .filter(|u| u.starts_with("/assets/") && u.ends_with(".js"))
        .map(|u| format!("{base}{u}"))
        .collect();
    let mut found = futures_util::stream::iter(scripts)
        .map(|u| {
            let http = http.clone();
            async move { get(&http, &u).await }
        })
        .buffer_unordered(6);
    while let Some(js) = found.next().await {
        let Some(js) = js.and_then(|b| String::from_utf8(b).ok()) else { continue };
        let table = assets(&js);
        if !table.is_empty() {
            return Some(table);
        }
    }
    None
}

/// Sound name -> asset hash, out of the bundle's `"./name.mp3":"id"` map and the
/// `id(e,t,n){...e.exports=n.p+"hash.mp3"}` modules it points at.
fn assets(js: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(start) = js.find("\"./activity_end.mp3\":\"") else { return out };
    let map = &js[start..];
    let map = &map[..map.find('}').unwrap_or(map.len())];
    for entry in map.split(',') {
        let mut parts = entry.split('"').filter(|p| !p.is_empty() && *p != ":");
        let (Some(file), Some(id)) = (parts.next(), parts.next()) else { continue };
        let Some(name) = file.strip_prefix("./").and_then(|f| f.strip_suffix(".mp3")) else { continue };
        let Some(at) = js.find(&format!(",{id}(")) else { continue };
        let module = &js[at..(at + 200).min(js.len())];
        if let Some(h) = module.split("n.p+\"").nth(1).and_then(|r| r.split(".mp3").next()) {
            out.insert(name.to_string(), h.to_string());
        }
    }
    out
}

fn notes(name: &str) -> &'static [(f32, f32)] {
    match sound_of(name) {
        "message1" => &[(1046.5, 0.07), (1568.0, 0.12)],
        "mute" => &[(880.0, 0.06), (659.3, 0.09)],
        "unmute" => &[(659.3, 0.06), (880.0, 0.09)],
        "deafen" => &[(659.3, 0.07), (440.0, 0.11)],
        "undeafen" => &[(440.0, 0.07), (659.3, 0.11)],
        "disconnect" => &[(523.3, 0.08), (392.0, 0.08), (261.6, 0.14)],
        "user_join" => &[(523.3, 0.06), (659.3, 0.06), (784.0, 0.1)],
        "user_leave" => &[(784.0, 0.06), (659.3, 0.06), (523.3, 0.1)],
        "camera_on" => &[(784.0, 0.05), (1046.5, 0.1)],
        "camera_off" => &[(1046.5, 0.05), (784.0, 0.1)],
        "ptt_start" => &[(1318.5, 0.04)],
        "ptt_stop" => &[(987.8, 0.04)],
        "user_moved" => &[(659.3, 0.05), (880.0, 0.05), (659.3, 0.08)],
        "call_calling" => &[(587.3, 0.2), (0.0, 0.1), (587.3, 0.2), (0.0, 0.4), (587.3, 0.2), (0.0, 0.1), (587.3, 0.2)],
        "stream_started" => &[(523.3, 0.05), (784.0, 0.05), (1046.5, 0.1)],
        "stream_ended" => &[(1046.5, 0.05), (784.0, 0.05), (523.3, 0.1)],
        "stream_user_joined" => &[(698.5, 0.05), (880.0, 0.1)],
        "stream_user_left" => &[(880.0, 0.05), (698.5, 0.1)],
        "discodo" => &[(523.3, 0.07), (659.3, 0.07), (784.0, 0.07), (1046.5, 0.07), (784.0, 0.07), (1046.5, 0.18)],
        "call_ringing" => &[(659.3, 0.18), (0.0, 0.08), (784.0, 0.18), (0.0, 0.3), (659.3, 0.18), (0.0, 0.08), (784.0, 0.18)],
        _ => &[],
    }
}

// soft sine notes with a quick attack and exponential tail, so nothing clicks
fn synth(name: &str, rate: usize) -> Clip {
    let mut pcm = Vec::new();
    for (f, secs) in notes(name) {
        for i in 0..(secs * rate as f32) as usize {
            let t = i as f32 / rate as f32;
            let env = (t / 0.005).min(1.0) * (-t * 18.0).exp();
            pcm.push(if *f > 0.0 { (t * f * std::f32::consts::TAU).sin() * env * 0.22 } else { 0.0 });
        }
    }
    Clip { pcm, ch: 1, rate }
}

#[cfg(test)]
pub fn has_file(cue: &str) -> bool {
    CLASSIC.iter().any(|(n, _)| *n == sound_of(cue))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(discodo: bool, halloween: bool) -> Config {
        Config { on: true, muted: Vec::new(), discodo, halloween, speaker: None, volume: 1.0 }
    }

    #[test]
    fn packs_swap_the_files_discord_swaps() {
        assert_eq!(file_of("message", &cfg(false, false), 0), "message1");
        assert_eq!(file_of("message", &cfg(true, false), 0), "discodo");
        assert_eq!(file_of("message", &cfg(false, true), 0), "halloween_message1");
        assert_eq!(file_of("user_join", &cfg(false, true), 0), "halloween_user_join");
        assert_eq!(file_of("user_moved", &cfg(false, true), 0), "user_moved");
        assert_eq!(file_of("stream_user_joined", &cfg(false, false), 0), "stream_user_joined");
    }

    #[test]
    fn one_ring_in_a_thousand_is_a_secret_tune() {
        let c = cfg(false, false);
        assert_eq!(file_of("call_ringing", &c, 499), "call_ringing_beat");
        assert_eq!(file_of("call_ringing", &c, 1499), "call_ringing_snow_halation");
        assert_eq!(file_of("call_ringing", &c, 498), "call_ringing");
        assert_eq!(file_of("call_ringing", &cfg(false, true), 499), "halloween_call_ringing");
    }

    #[test]
    fn an_mp3_decodes_to_pcm() {
        let out = std::env::temp_dir().join("rustcord-sound-test.mp3");
        let made = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=880:duration=0.25", "-b:a", "32k"])
            .arg(&out)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        let clip = decode(&std::fs::read(&out).unwrap()).unwrap();
        assert!(clip.ch >= 1 && clip.rate >= 8000 && clip.pcm.len() >= clip.rate / 4 * clip.ch);
    }

    #[test]
    fn asset_names_come_out_of_the_bundle() {
        let js = r#"x},696354(e,t,n){var i={"./activity_end.mp3":"440261","./user_join.mp3":"902775"};function r(){}},440261(e,t,n){"use strict";e.exports=n.p+"878f7e7f2b3bf972.mp3"},902775(e,t,n){"use strict";e.exports=n.p+"a71f183dc4de32a3.mp3"}"#;
        let t = assets(js);
        assert_eq!(t.get("activity_end").map(String::as_str), Some("878f7e7f2b3bf972"));
        assert_eq!(t.get("user_join").map(String::as_str), Some("a71f183dc4de32a3"));
    }
}
