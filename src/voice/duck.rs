//! Global attenuation: while you or the others in the call are speaking, every other app's audio
//! session on the default output turns down by the chosen percent, and comes back afterwards.
//! What was turned down is written to disk, so a crash can't leave an app quiet for good.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::Interface;

use super::audio::{Shared, now_ms};

pub struct Duck {
    stop: Arc<AtomicBool>,
    th: Option<JoinHandle<()>>,
}

impl Drop for Duck {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.th.take() {
            let _ = t.join();
        }
    }
}

pub fn start(shared: Arc<Shared>) -> Duck {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let th = std::thread::Builder::new().name("voice-duck".into()).spawn(move || run(shared, s)).ok();
    Duck { stop, th }
}

struct Session {
    id: String,
    pid: u32,
    vol: ISimpleAudioVolume,
}

fn sessions() -> Vec<Session> {
    let list = || unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).ok()?;
        let list = mgr.GetSessionEnumerator().ok()?;
        let mut out = Vec::new();
        for i in 0..list.GetCount().ok()? {
            let Ok(ctl) = list.GetSession(i) else { continue };
            let (Ok(c2), Ok(vol)) = (ctl.cast::<IAudioSessionControl2>(), ctl.cast::<ISimpleAudioVolume>()) else { continue };
            let id = c2.GetSessionInstanceIdentifier().ok().and_then(|p| p.to_string().ok()).unwrap_or_default();
            out.push(Session { id, pid: c2.GetProcessId().unwrap_or(0), vol });
        }
        Some(out)
    };
    list().unwrap_or_default()
}

fn set(s: &Session, v: f32) {
    unsafe {
        let _ = s.vol.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null());
    }
}

fn file() -> std::path::PathBuf {
    crate::cache::dir().join("ducked.txt")
}

fn remember(held: &[(String, f32)]) {
    if held.is_empty() {
        let _ = std::fs::remove_file(file());
    } else {
        let text: String = held.iter().map(|(id, v)| format!("{v}\t{id}\n")).collect();
        let _ = std::fs::create_dir_all(crate::cache::dir());
        let _ = std::fs::write(file(), text);
    }
}

fn restore(held: &mut Vec<(String, f32)>) {
    for s in sessions() {
        if let Some((_, v)) = held.iter().find(|(id, _)| *id == s.id) {
            set(&s, *v);
        }
    }
    held.clear();
    remember(held);
}

/// Puts back whatever a previous run was holding down when it died.
pub fn recover() {
    let Ok(text) = std::fs::read_to_string(file()) else { return };
    let mut held: Vec<(String, f32)> = text
        .lines()
        .filter_map(|l| {
            let (v, id) = l.split_once('\t')?;
            Some((id.to_string(), v.parse().ok()?))
        })
        .collect();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        restore(&mut held);
    });
}

fn run(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let me = std::process::id();
    let mut held: Vec<(String, f32)> = Vec::new();
    let mut tick = 0u32;
    while !stop.load(Relaxed) {
        std::thread::sleep(Duration::from_millis(50));
        tick = tick.wrapping_add(1);
        let pct = shared.atten.load(Relaxed);
        let heard = shared.others_ms.load(Relaxed);
        let others = heard > 0 && now_ms().saturating_sub(heard) < 500;
        let want = pct > 0 && ((shared.atten_self.load(Relaxed) && shared.talking.load(Relaxed)) || (shared.atten_others.load(Relaxed) && others));
        if want {
            if !held.is_empty() && tick % 5 != 0 {
                continue;
            }
            let k = 1.0 - pct.min(100) as f32 / 100.0;
            for s in sessions().into_iter().filter(|s| s.pid != me && s.pid != 0) {
                let orig = match held.iter().find(|(id, _)| *id == s.id) {
                    Some((_, v)) => *v,
                    None => {
                        let v = unsafe { s.vol.GetMasterVolume() }.unwrap_or(1.0);
                        held.push((s.id.clone(), v));
                        remember(&held);
                        v
                    }
                };
                set(&s, orig * k);
            }
        } else if !held.is_empty() {
            restore(&mut held);
        }
    }
    restore(&mut held);
}
