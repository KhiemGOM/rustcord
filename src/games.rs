//! Game detection the desktop client's way: Discord's list of detectable games matched against
//! the paths of running programs every few seconds. It feeds the "Playing" activity and the
//! panel over the account card.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::Value;

use crate::state::Id;

#[derive(Clone, PartialEq, Debug)]
pub struct Game {
    pub id: Id,
    pub name: String,
    pub icon: Option<String>,
    // unix ms, when we first saw it running
    pub start: i64,
}

// (executable the way discord lists it, application id, name, icon hash)
type Entry = (String, Id, String, Option<String>);

/// Windows executables out of discord's list, cached a day; the list itself is 13MB of json.
fn list(rt: &tokio::runtime::Runtime) -> Vec<Entry> {
    let path = crate::cache::dir().join("detectable.json");
    let read = || std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Vec<Entry>>(&b).ok());
    let fresh = std::fs::metadata(&path).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e < Duration::from_secs(86_400)));
    if fresh && let Some(l) = read() {
        return l;
    }
    let http = crate::api::http();
    let url = format!("{}/applications/detectable", crate::api::base());
    let Some(v) = rt.block_on(async { http.get(url).send().await.ok()?.error_for_status().ok()?.json::<Value>().await.ok() }) else {
        // offline: last week's list beats none
        return read().unwrap_or_default();
    };
    let mut out = Vec::new();
    for a in v.as_array().into_iter().flatten() {
        let id = a["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
        for e in a["executables"].as_array().into_iter().flatten() {
            // launchers aren't games, and the few that need a command line to tell apart stay out
            if e["os"] != "win32" || e["is_launcher"] == true || e["arguments"].is_string() {
                continue;
            }
            let Some(exe) = e["name"].as_str() else { continue };
            out.push((exe.trim_start_matches('>').to_lowercase(), id, a["name"].as_str().unwrap_or("").to_string(), a["icon_hash"].as_str().map(str::to_string)));
        }
    }
    let _ = std::fs::write(&path, serde_json::to_vec(&out).unwrap_or_default());
    out
}

/// Full paths, lowercase with forward slashes, of running programs whose file name is one we
/// know. A program we can't open (run as admin, say) still gives its bare name.
fn running(names: &HashMap<String, Vec<usize>>) -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
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
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            if names.contains_key(&name) {
                let mut path = name;
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                if !h.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut n = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut n) != 0 {
                        path = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase().replace('\\', "/");
                    }
                    CloseHandle(h);
                }
                out.push(path);
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// Starts the watcher; it sends the game whenever it changes, None once it's closed.
pub fn start(ctx: egui::Context) -> Receiver<Option<Game>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("games".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            let list = list(&rt);
            let mut names: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, (exe, ..)) in list.iter().enumerate() {
                names.entry(exe.rsplit('/').next().unwrap_or(exe).to_string()).or_default().push(i);
            }
            let mut cur: Option<Game> = None;
            loop {
                let mut hits = Vec::new();
                for path in running(&names) {
                    let file = path.rsplit('/').next().unwrap_or(&path);
                    for &i in names.get(file).into_iter().flatten() {
                        let exe = &list[i].0;
                        if path == *exe || path.ends_with(&format!("/{exe}")) {
                            hits.push(i);
                        }
                    }
                }
                // with two games open the one we already show stays put
                let pick = hits.iter().find(|i| cur.as_ref().is_some_and(|g| g.id == list[**i].1)).or(hits.first());
                let next = pick.map(|&i| match &cur {
                    Some(g) if g.id == list[i].1 => g.clone(),
                    _ => Game { id: list[i].1, name: list[i].2.clone(), icon: list[i].3.clone(), start: chrono::Utc::now().timestamp_millis() },
                });
                if next != cur {
                    cur = next.clone();
                    if tx.send(next).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .ok();
    rx
}
