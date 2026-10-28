//! Short notification tones, synthesised so there are no audio files to ship.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn notes(name: &str) -> &'static [(f32, f32)] {
    match name {
        "message" => &[(1046.5, 0.07), (1568.0, 0.12)],
        "mute" => &[(880.0, 0.06), (659.3, 0.09)],
        "unmute" => &[(659.3, 0.06), (880.0, 0.09)],
        "deafen" => &[(659.3, 0.07), (440.0, 0.11)],
        "undeafen" => &[(440.0, 0.07), (659.3, 0.11)],
        "disconnect" => &[(523.3, 0.08), (392.0, 0.08), (261.6, 0.14)],
        "join" => &[(523.3, 0.06), (659.3, 0.06), (784.0, 0.1)],
        "leave" => &[(784.0, 0.06), (659.3, 0.06), (523.3, 0.1)],
        "discodo" => &[(523.3, 0.07), (659.3, 0.07), (784.0, 0.07), (1046.5, 0.07), (784.0, 0.07), (1046.5, 0.18)],
        "ring" => &[(659.3, 0.18), (0.0, 0.08), (784.0, 0.18), (0.0, 0.3), (659.3, 0.18), (0.0, 0.08), (784.0, 0.18)],
        "ring_secret" => &[(784.0, 0.12), (880.0, 0.12), (987.8, 0.12), (1174.7, 0.24), (0.0, 0.1), (987.8, 0.12), (1174.7, 0.36)],
        _ => &[],
    }
}

// soft sine notes with a quick attack and exponential tail, so nothing clicks
fn render(name: &str, rate: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (f, secs) in notes(name) {
        let n = (secs * rate) as usize;
        for i in 0..n {
            let t = i as f32 / rate;
            let env = (t / 0.005).min(1.0) * (-t * 18.0).exp();
            out.push(if *f > 0.0 { (t * f * std::f32::consts::TAU).sin() * env * 0.22 } else { 0.0 });
        }
    }
    out
}

static ON: Mutex<(bool, Vec<String>)> = Mutex::new((true, Vec::new()));

/// What the settings say about sounds: master switch and the muted list.
pub fn configure(on: bool, muted: &[String]) {
    if let Ok(mut s) = ON.lock() {
        *s = (on, muted.to_vec());
    }
}

/// Play a cue unless the user turned it (or all sounds) off.
pub fn cue(name: &str) {
    let allowed = ON.lock().map(|s| s.0 && !s.1.iter().any(|m| m == name)).unwrap_or(true);
    if allowed && std::env::var_os("FASTCORD_SILENT").is_none() {
        play(if name == "ring" && crate::plugins::on("SecretRingToneEnabler") { "ring_secret" } else { name });
    }
}

pub fn play(name: &str) {
    let name = name.to_string();
    std::thread::spawn(move || {
        let Some(dev) = cpal::default_host().default_output_device() else { return };
        let Ok(cfg) = dev.default_output_config() else { return };
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            return;
        }
        let conf = cfg.config();
        let ch = conf.channels as usize;
        let mut pcm = render(&name, conf.sample_rate as f32);
        if crate::plugins::on("NotificationVolume") {
            let k = crate::plugins::num("NotificationVolume", "notificationVolume") as f32 / 100.0;
            pcm.iter_mut().for_each(|s| *s *= k);
        }
        let pcm = Arc::new(pcm);
        let len = pcm.len();
        let pos = Arc::new(AtomicUsize::new(0));
        let (p, at) = (pcm.clone(), pos.clone());
        let stream = dev.build_output_stream::<f32, _, _>(
            conf,
            move |data, _| {
                for frame in data.chunks_mut(ch) {
                    let i = at.fetch_add(1, Ordering::Relaxed);
                    let s = p.get(i).copied().unwrap_or(0.0);
                    frame.fill(s);
                }
            },
            |_| {},
            None,
        );
        let Ok(stream) = stream else { return };
        if stream.play().is_ok() {
            while pos.load(Ordering::Relaxed) < len {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(60));
        }
    });
}
