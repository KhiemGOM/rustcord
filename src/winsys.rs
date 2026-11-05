//! The few Windows-specific things the app needs: run on startup, hiding the
//! window from screen capture, spotting a running OBS for streamer mode, and
//! the resize border of the frameless window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn set_startup(on: bool) {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
    let key = wide(RUN_KEY);
    let name = wide("Rustcord");
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

#[derive(Clone, Copy)]
pub struct Prefs {
    pub animations: bool,
    pub high_contrast: bool,
}

/// What Windows' own ease-of-access settings say: animations in the client area, and a contrast theme.
pub fn preferences() -> Prefs {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
    let mut anim = 1i32;
    let mut flags = [0u16; 16];
    let mut len = std::mem::size_of_val(&flags) as u32;
    let (key, name) = (wide("Control Panel\\Accessibility\\HighContrast"), wide("Flags"));
    unsafe {
        SystemParametersInfoW(0x1042, 0, (&mut anim as *mut i32).cast(), 0);
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), flags.as_mut_ptr().cast(), &mut len);
    }
    let text = String::from_utf16_lossy(&flags);
    let flags: u32 = text.trim_matches('\0').trim().parse().unwrap_or(0);
    Prefs { animations: anim != 0, high_contrast: flags & 1 != 0 }
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

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

static OLD_PROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static BUTTON_CREATED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

// width of the invisible resize border, in points
const EDGE: f32 = 6.0;

/// winit leaves an undecorated window with no resize border at all, so answer
/// WM_NCHITTEST ourselves; the system then does the cursor, the drag and Aero Snap.
unsafe extern "system" fn chrome_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    if msg == WM_CLOSE && crate::tray::close_hides() {
        crate::tray::send("hide");
        return 0;
    }
    // a new taskbar button (shown again from the tray, or explorer restarted) has no badge yet
    if msg != 0 && msg == BUTTON_CREATED.load(Ordering::Relaxed) {
        crate::tray::send("badge");
    }
    if msg == WM_NCHITTEST && unsafe { IsZoomed(hwnd) } == 0 {
        let (x, y) = (lp as i16 as i32, ((lp >> 16) as i16) as i32);
        let mut r: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetWindowRect(hwnd, &mut r) };
        let e = (EDGE * unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0).round() as i32;
        // corners reach further along the edge, like the system's own frames
        let c = e * 3;
        let left = x < r.left + e;
        let right = x >= r.right - e;
        let top = y < r.top + e;
        let bottom = y >= r.bottom - e;
        let near_l = x < r.left + c;
        let near_r = x >= r.right - c;
        let near_t = y < r.top + c;
        let near_b = y >= r.bottom - c;
        let hit = match () {
            _ if (left && near_t) || (top && near_l) => HTTOPLEFT,
            _ if (right && near_t) || (top && near_r) => HTTOPRIGHT,
            _ if (left && near_b) || (bottom && near_l) => HTBOTTOMLEFT,
            _ if (right && near_b) || (bottom && near_r) => HTBOTTOMRIGHT,
            _ if left => HTLEFT,
            _ if right => HTRIGHT,
            _ if top => HTTOP,
            _ if bottom => HTBOTTOM,
            _ => 0,
        };
        if hit != 0 {
            return hit as isize;
        }
    }
    let old: WNDPROC = unsafe { std::mem::transmute(OLD_PROC.load(Ordering::Relaxed)) };
    unsafe { CallWindowProcW(old, hwnd, msg, wp, lp) }
}

/// Resize borders and Windows 11 rounded corners for the frameless window. Call once.
pub fn frameless(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrW};
    let Ok(h) = frame.window_handle() else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else { return };
    let hwnd = w.hwnd.get() as HWND;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&DWMWCP_ROUND as *const i32).cast(), 4);
        let name: Vec<u16> = "TaskbarButtonCreated".encode_utf16().chain(Some(0)).collect();
        BUTTON_CREATED.store(windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(name.as_ptr()), Ordering::Relaxed);
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, chrome_proc as *const () as isize);
        OLD_PROC.store(old, Ordering::Relaxed);
        // e2e: maximise and fullscreen would otherwise activate the window over someone's game
        if std::env::var_os("RUSTCORD_E2E").is_some() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOACTIVATE};
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, GetWindowLongPtrW(hwnd, GWL_EXSTYLE) | WS_EX_NOACTIVATE as isize);
        }
    }
    WINDOW.store(hwnd as isize, Ordering::Relaxed);
}

static BADGE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// The taskbar badge last put up, for the e2e run.
pub fn badge() -> i32 {
    BADGE.load(Ordering::Relaxed)
}

/// Discord's taskbar badge, with its own icons: a red count from 1 to 9 then 9+, or for -1 the
/// dot it shows when something's unread but nothing mentions you. 0 takes it down.
pub fn set_badge(count: i32) {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    const ICONS: [&[u8]; 11] = [
        include_bytes!("../assets/badges/badge-1.ico"),
        include_bytes!("../assets/badges/badge-2.ico"),
        include_bytes!("../assets/badges/badge-3.ico"),
        include_bytes!("../assets/badges/badge-4.ico"),
        include_bytes!("../assets/badges/badge-5.ico"),
        include_bytes!("../assets/badges/badge-6.ico"),
        include_bytes!("../assets/badges/badge-7.ico"),
        include_bytes!("../assets/badges/badge-8.ico"),
        include_bytes!("../assets/badges/badge-9.ico"),
        include_bytes!("../assets/badges/badge-10.ico"),
        include_bytes!("../assets/badges/badge-11.ico"),
    ];
    let hwnd = WINDOW.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    let (index, what) = match count {
        -1 => (Some(10), "Unread messages".to_string()),
        0 => (None, "No Notifications".to_string()),
        n => (Some(n.clamp(1, 10) as usize - 1), format!("{n} notifications")),
    };
    unsafe {
        let Ok(bar) = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) else { return };
        if bar.HrInit().is_err() {
            return;
        }
        let hicon = index.and_then(|i| ico(ICONS[i], GetSystemMetrics(SM_CXSMICON))).map(HICON).unwrap_or_default();
        let set = bar.SetOverlayIcon(windows::Win32::Foundation::HWND(hwnd as _), hicon, &windows::core::HSTRING::from(what));
        // the taskbar keeps its own copy
        if !hicon.is_invalid() {
            let _ = DestroyIcon(hicon);
        }
        if set.is_ok() {
            BADGE.store(count, Ordering::Relaxed);
        }
    }
}

/// The entry of an .ico nearest `want` pixels (the next size up if there is one): its size and
/// bytes, which are a PNG or a DIB.
pub fn ico_entry(ico: &[u8], want: i32) -> Option<(i32, &[u8])> {
    let n = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let (size, off, len) = (0..n)
        .map(|i| {
            let e = &ico[6 + 16 * i..22 + 16 * i];
            let size = if e[0] == 0 { 256 } else { e[0] as i32 };
            (size, u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize, u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize)
        })
        .min_by_key(|(s, _, _)| if *s >= want { s - want } else { 1000 + want - s })?;
    Some((size, ico.get(off..off + len)?))
}

/// That entry as an icon `want` pixels across, handed to Windows as is.
pub fn ico(ico: &[u8], want: i32) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTCOLOR};
    let (size, bytes) = ico_entry(ico, want)?;
    let px = size.min(want.max(16));
    let h = unsafe { CreateIconFromResourceEx(bytes.as_ptr(), bytes.len() as u32, 1, 0x00030000, px, px, LR_DEFAULTCOLOR) };
    (!h.is_null()).then_some(h)
}

/// Electron's flashFrame: the taskbar button blinks four times, then stays lit until the window
/// is looked at.
pub fn flash() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FLASHW_ALL, FLASHWINFO, FlashWindowEx};
    let fwi = FLASHWINFO { cbSize: size_of::<FLASHWINFO>() as u32, hwnd: WINDOW.load(Ordering::Relaxed) as HWND, dwFlags: FLASHW_ALL, uCount: 4, dwTimeout: 0 };
    unsafe { FlashWindowEx(&fwi) };
}

/// Asks the window to close the way Alt+F4 does, for the e2e run.
pub fn send_close() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe { PostMessageW(WINDOW.load(Ordering::Relaxed) as HWND, WM_CLOSE, 0, 0) };
}

/// Whether the window is on screen at all, rather than put away in the tray.
pub fn shown() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    unsafe { IsWindowVisible(WINDOW.load(Ordering::Relaxed) as HWND) != 0 }
}

/// Minimizes or restores the window without activating anything, for the e2e run.
pub fn show_quietly(minimized: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, ShowWindow};
    let hwnd = WINDOW.load(Ordering::Relaxed) as HWND;
    unsafe { ShowWindow(hwnd, if minimized { SW_SHOWMINNOACTIVE } else { SW_SHOWNOACTIVATE }) };
}

/// What the window answers for a point at each edge and corner plus the middle, for the e2e run.
pub fn probe_edges() -> Vec<u32> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, SendMessageW, WM_NCHITTEST};
    let hwnd = WINDOW.load(Ordering::Relaxed) as HWND;
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(hwnd, &mut r) };
    let (l, t, rt, b) = (r.left + 1, r.top + 1, r.right - 2, r.bottom - 2);
    let (mx, my) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    [(l, my), (rt, my), (mx, t), (mx, b), (l, t), (rt, t), (l, b), (rt, b), (mx, my)]
        .iter()
        .map(|(x, y)| unsafe { SendMessageW(hwnd, WM_NCHITTEST, 0, ((*y as u16 as isize) << 16) | (*x as u16 as isize)) as u32 })
        .collect()
}
