//! The programs running right now, from one snapshot every few seconds that the OBS watcher and
//! game detection share. Each used to take its own, and a snapshot costs tens of ms of kernel
//! time on a machine with a few hundred processes.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    // the executable's file name, lowercased
    pub name: String,
}

static LATEST: Mutex<Option<Arc<Vec<Proc>>>> = Mutex::new(None);
static FRESH: Condvar = Condvar::new();

/// An executable name out of a PROCESSENTRY32W, up to its NUL, lowercased.
pub fn exe_name(raw: &[u16]) -> String {
    let len = raw.iter().position(|c| *c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..len]).to_lowercase()
}

/// Whether a streaming app is among `procs`.
pub fn streaming_in(procs: &[Proc]) -> bool {
    const NAMES: [&str; 5] = ["obs64.exe", "obs32.exe", "obs.exe", "streamlabs obs.exe", "xsplit.core.exe"];
    procs.iter().any(|p| NAMES.contains(&p.name.as_str()))
}

/// The processes whose executable is one of `names`.
pub fn wanted<'a, V>(procs: &'a [Proc], names: &std::collections::HashMap<String, V>) -> Vec<&'a Proc> {
    procs.iter().filter(|p| names.contains_key(&p.name)).collect()
}

/// Every running process, or none if Windows won't say.
pub fn snapshot() -> Vec<Proc> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            out.push(Proc { pid: e.th32ProcessID, name: exe_name(&e.szExeFile) });
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// The newest snapshot, waiting up to `wait` for the first one.
pub fn latest(wait: Duration) -> Option<Arc<Vec<Proc>>> {
    let g = LATEST.lock().unwrap_or_else(|e| e.into_inner());
    let (g, _) = FRESH.wait_timeout_while(g, wait, |s| s.is_none()).unwrap_or_else(|e| e.into_inner());
    g.clone()
}

/// Starts the snapshot thread; it keeps `winsys::OBS_RUNNING` current and wakes the window when it flips.
pub fn start(ctx: egui::Context) {
    std::thread::Builder::new()
        .name("procs".into())
        .spawn(move || {
            loop {
                let snap = Arc::new(snapshot());
                let obs = streaming_in(&snap);
                *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(snap);
                FRESH.notify_all();
                if crate::winsys::OBS_RUNNING.swap(obs, Ordering::Relaxed) != obs {
                    ctx.request_repaint();
                }
                std::thread::sleep(EVERY);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn p(pid: u32, name: &str) -> Proc {
        Proc { pid, name: name.into() }
    }
    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn exe_names_stop_at_nul_and_are_lowercased() {
        let mut raw = w("OBS64.EXE\0junk after the nul");
        raw.resize(260, 0);
        assert_eq!(exe_name(&raw), "obs64.exe");
        assert_eq!(exe_name(&w("Game.exe")), "game.exe");
        assert_eq!(exe_name(&[]), "");
    }

    #[test]
    fn every_streaming_app_is_seen() {
        for n in ["obs64.exe", "obs32.exe", "obs.exe", "streamlabs obs.exe", "xsplit.core.exe"] {
            assert!(streaming_in(&[p(1, "explorer.exe"), p(2, n)]), "{n}");
        }
    }

    #[test]
    fn near_misses_are_not_streaming_apps() {
        assert!(!streaming_in(&[]));
        assert!(!streaming_in(&[p(1, "obs64.exe.old"), p(2, "notobs.exe"), p(3, "obs64")]));
    }

    #[test]
    fn wanted_keeps_every_copy_of_a_listed_game() {
        let names: HashMap<String, Vec<usize>> = [("game.exe".to_string(), vec![0])].into();
        let procs = [p(1, "game.exe"), p(2, "other.exe"), p(3, "game.exe")];
        let pids: Vec<u32> = wanted(&procs, &names).iter().map(|p| p.pid).collect();
        assert_eq!(pids, [1, 3]);
        assert!(wanted(&[p(4, "other.exe")], &names).is_empty());
    }

    #[test]
    fn a_real_snapshot_includes_this_process() {
        let snap = snapshot();
        let me = std::process::id();
        let mine = snap.iter().find(|p| p.pid == me).expect("own process in the snapshot");
        assert!(mine.name.ends_with(".exe"), "{}", mine.name);
        assert_eq!(mine.name, mine.name.to_lowercase());
    }
}
