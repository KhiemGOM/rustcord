//! The few Windows-specific things settings need: run on startup, hiding the
//! window from screen capture, and spotting a running OBS for streamer mode.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn set_startup(on: bool) {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
    let key = wide(RUN_KEY);
    let name = wide("Fastcord");
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let cmd = wide(&format!("\"{}\" --startup", exe.display()));
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, cmd.as_ptr().cast(), (cmd.len() * 2) as u32);
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
        }
    }
}

pub fn exclude_from_capture(frame: &eframe::Frame, on: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
    let Ok(h) = frame.window_handle() else { return };
    if let RawWindowHandle::Win32(w) = h.as_raw() {
        unsafe {
            SetWindowDisplayAffinity(w.hwnd.get() as _, if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }
}

pub static OBS_RUNNING: AtomicBool = AtomicBool::new(false);

fn streaming_app_running() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    const NAMES: [&str; 5] = ["obs64.exe", "obs32.exe", "obs.exe", "streamlabs obs.exe", "xsplit.core.exe"];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok && !found {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            found = NAMES.contains(&name.as_str());
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        found
    }
}

/// Checks for OBS/XSplit every few seconds; cheap enough to leave running.
pub fn watch_streaming(ctx: egui::Context) {
    std::thread::Builder::new()
        .name("obs-watch".into())
        .spawn(move || {
            loop {
                let now = streaming_app_running();
                if OBS_RUNNING.swap(now, Ordering::Relaxed) != now {
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .ok();
}
