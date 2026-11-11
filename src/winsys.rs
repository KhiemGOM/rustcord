//! The few Windows-specific things the app needs: run on startup, hiding the
//! window from screen capture, spotting a running OBS for streamer mode, and
//! the resize border of the frameless window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// the hook below while it's watching, and what it's watching for: width in points, and colour
static EARLY: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static EARLY_FOR: std::sync::Mutex<([f32; 2], [u8; 3])> = std::sync::Mutex::new(([0.0; 2], [0; 3]));

/// The window exists within a few ms, but eframe keeps it hidden until the first frame, which the
/// graphics driver holds up for 200 ms more. So it's shown as soon as it has its size, filled with
/// the theme's background, and the first frame lands on that. Showing it takes the main thread,
/// which only a hook on its messages can run on before the driver starts loading.
pub fn show_early(size: [f32; 2], color: [u8; 3]) {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_CALLWNDPROCRET};
    *EARLY_FOR.lock().unwrap() = (size, color);
    let hook = unsafe { SetWindowsHookExW(WH_CALLWNDPROCRET, Some(early_hook), std::ptr::null_mut(), GetCurrentThreadId()) };
    EARLY.store(hook as isize, Ordering::Relaxed);
}

/// Stops watching, for a window that never turned out the size it was expected to be.
pub fn show_early_over() {
    use windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;
    let hook = EARLY.swap(0, Ordering::Relaxed);
    if hook != 0 {
        unsafe { UnhookWindowsHookEx(hook as _) };
    }
}

unsafe extern "system" fn early_hook(code: i32, wp: usize, lp: isize) -> isize {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, GetDC, ReleaseDC};
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CWPRETSTRUCT, CallNextHookEx, GetClassNameW, SW_SHOW, ShowWindow, WM_SIZE};
    unsafe {
        let m = &*(lp as *const CWPRETSTRUCT);
        if code >= 0 && m.message == WM_SIZE {
            let mut class = [0u16; 32];
            let n = GetClassNameW(m.hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            let (w, h) = ((m.lParam & 0xffff) as i32, (m.lParam >> 16 & 0xffff) as i32);
            let ([points, _], [r, g, b]) = *EARLY_FOR.lock().unwrap();
            if String::from_utf16_lossy(&class[..n]) == "Window Class" && (w - (points * GetDpiForWindow(m.hwnd) as f32 / 96.0).round() as i32).abs() <= 2 {
                show_early_over();
                ShowWindow(m.hwnd, SW_SHOW);
                let dc = GetDC(m.hwnd);
                let brush = CreateSolidBrush(r as u32 | (g as u32) << 8 | (b as u32) << 16);
                FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, brush);
                DeleteObject(brush);
                ReleaseDC(m.hwnd, dc);
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wp, lp)
    }
}

/// The thread drawing the window, while the window's in use, goes ahead of whatever else wants the
/// cpu, or a build in the background shows up as hitches in scrolling. It only wants a millisecond
/// or so a frame. Out of focus it goes back to normal, to stay out of a game's way.
pub fn set_ui_priority(focused: bool) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_NORMAL};
    unsafe { SetThreadPriority(GetCurrentThread(), if focused { THREAD_PRIORITY_ABOVE_NORMAL } else { THREAD_PRIORITY_NORMAL }) };
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn set_startup(on: bool) {
    if std::env::var_os("RUSTCORD_STRESS_SEED").is_some() {
        return;
    }
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

/// Whether windows' own clock is set to 24 hours (iTime 1).
pub fn system_24h() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    let key = wide("Control Panel\\International");
    let name = wide("iTime");
    let mut buf = [0u16; 4];
    let mut len = size_of_val(&buf) as u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
    ok == 0 && buf[0] == b'1' as u16
}

/// Hands back every page nothing is touching, as Windows itself used to for a minimized window:
/// hidden, a client only needs what the gateway and the tray use. The rest comes back as needed.
pub fn trim() {
    use windows_sys::Win32::System::Memory::SetProcessWorkingSetSizeEx;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe { SetProcessWorkingSetSizeEx(GetCurrentProcess(), usize::MAX, usize::MAX, 0) };
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
static FULLSCREEN: AtomicBool = AtomicBool::new(false);
static PASTE: AtomicBool = AtomicBool::new(false);

pub fn take_paste() -> bool {
    PASTE.swap(false, Ordering::Relaxed)
}

pub fn set_fullscreen(on: bool) {
    FULLSCREEN.store(on, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    #[test]
    fn paste_press_survives_key_release_until_the_frame() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, SetKeyboardState};
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GetWindowLongPtrW, GWLP_WNDPROC, WM_KEYDOWN, WS_POPUP};
        unsafe {
            let class = super::wide("STATIC");
            let window = CreateWindowExW(0, class.as_ptr(), std::ptr::null(), WS_POPUP, 0, 0, 8, 8, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
            assert!(!window.is_null());
            let mut keys = [0; 256];
            assert_ne!(GetKeyboardState(keys.as_mut_ptr()), 0);
            let before = keys;
            let old = super::OLD_PROC.swap(GetWindowLongPtrW(window, GWLP_WNDPROC), super::Ordering::Relaxed);
            keys[0x11] = 0x80;
            keys[b'V' as usize] = 0x80;
            let down = SetKeyboardState(keys.as_ptr());
            super::chrome_proc(window, WM_KEYDOWN, b'V' as usize, 0);
            keys[0x11] = 0;
            keys[b'V' as usize] = 0;
            let up = SetKeyboardState(keys.as_ptr());
            let paste = super::take_paste();
            let twice = super::take_paste();
            SetKeyboardState(before.as_ptr());
            super::OLD_PROC.store(old, super::Ordering::Relaxed);
            DestroyWindow(window);
            assert_ne!(down, 0);
            assert_ne!(up, 0);
            assert!(paste, "paste was lost when the keys were released before the frame");
            assert!(!twice);
        }
    }

    #[test]
    fn background_erase_keeps_the_rendered_pixels() {
        use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetPixel, SelectObject, SetPixel};
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WM_ERASEBKGND, WS_POPUP};
        unsafe {
            let class = super::wide("STATIC");
            let window = CreateWindowExW(0, class.as_ptr(), std::ptr::null(), WS_POPUP, 0, 0, 8, 8, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let bitmap = CreateBitmap(8, 8, 1, 32, std::ptr::null());
            assert!(!window.is_null() && !dc.is_null() && !bitmap.is_null());
            let previous = SelectObject(dc, bitmap);
            let color = 0x00372917;
            SetPixel(dc, 0, 0, color);
            let before = GetPixel(dc, 0, 0);
            let handled = super::chrome_proc(window, WM_ERASEBKGND, dc as usize, 0);
            let after = GetPixel(dc, 0, 0);
            SelectObject(dc, previous);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(window);
            assert_eq!(before, color);
            assert_eq!(handled, 1);
            assert_eq!(after, before, "background erasure blanked the rendered frame");
        }
    }
}
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
    if msg == WM_PASTE || msg == WM_KEYDOWN && wp == b'V' as usize && lp & (1 << 30) == 0
        && unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) } < 0
    {
        PASTE.store(true, Ordering::Relaxed);
        unsafe { windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, std::ptr::null(), 0) };
        if msg == WM_PASTE {
            return 0;
        }
    }
    if msg == WM_CLOSE && crate::tray::close_hides() {
        crate::tray::send("hide");
        return 0;
    }
    // a new taskbar button (shown again from the tray, or explorer restarted) has no badge yet
    if msg != 0 && msg == BUTTON_CREATED.load(Ordering::Relaxed) {
        crate::tray::send("badge");
    }
    if msg == WM_SETCURSOR && lp as u16 as u32 == HTCLIENT && crate::cursor::apply() {
        return 1;
    }
    if msg == WM_ERASEBKGND {
        return 1;
    }
    if msg == WM_NCHITTEST && !FULLSCREEN.load(Ordering::Relaxed) && unsafe { IsZoomed(hwnd) } == 0 {
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
