//! Discord's icon in the notification area: idle, unread, and its four voice faces, the
//! right-click menu its desktop app builds, a click that brings the window back, and the
//! one-time balloon saying the app keeps running there. It lives on its own thread with a
//! hidden window, so the menu's modal loop never runs inside winit's. What the user picks
//! goes to the app down the global keybinds' channel.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HBITMAP};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NINF_KEY,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct State {
    pub unread: bool,
    pub connected: bool,
    pub speaking: bool,
    pub mute: bool,
    pub server_mute: bool,
    pub deaf: bool,
    pub server_deaf: bool,
}

const IDLE: &[u8] = include_bytes!("../assets/tray/tray.ico");

impl State {
    fn face(&self) -> &'static [u8] {
        match self {
            s if s.connected && (s.deaf || s.server_deaf) => include_bytes!("../assets/tray/tray-deafened.ico"),
            s if s.connected && (s.mute || s.server_mute) => include_bytes!("../assets/tray/tray-muted.ico"),
            s if s.connected && s.speaking => include_bytes!("../assets/tray/tray-speaking.ico"),
            s if s.connected => include_bytes!("../assets/tray/tray-connected.ico"),
            s if s.unread => include_bytes!("../assets/tray/tray-unread.ico"),
            _ => IDLE,
        }
    }
}

const KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const WM_TRAY: u32 = WM_APP + 1;
const WM_STATE: u32 = WM_APP + 2;
const WM_HINT: u32 = WM_APP + 3;
const ID_MUTE: usize = 1;
const ID_DEAFEN: usize = 2;
const ID_VOICE: usize = 3;
const ID_QUIT: usize = 4;
const HINT: &str = "Hi! Rustcord will run in the background to keep you in touch with your friends. You can right-click here to quit.";

static STATE: Mutex<State> =
    Mutex::new(State { unread: false, connected: false, speaking: false, mute: false, server_mute: false, deaf: false, server_deaf: false });
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static CLOSE_HIDES: AtomicBool = AtomicBool::new(false);
static OUT: OnceLock<(Mutex<Sender<(String, bool)>>, egui::Context)> = OnceLock::new();

thread_local! {
    static ICON: Cell<HICON> = const { Cell::new(std::ptr::null_mut()) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn copy(dst: &mut [u16], s: &str) {
    let room = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(room)) {
        *d = c;
    }
}

pub fn start(ctx: egui::Context, tx: Sender<(String, bool)>) {
    let _ = OUT.set((Mutex::new(tx), ctx));
    let _ = std::thread::Builder::new().name("tray".into()).spawn(run);
}

/// Hands the app one of its actions (open, hide, mute, deafen, voice_settings, quit).
pub fn send(action: &str) {
    if let Some((tx, ctx)) = OUT.get() {
        let _ = tx.lock().unwrap().send((action.into(), true));
        ctx.request_repaint();
    }
}

/// Discord's "Minimize to Tray": with it on, closing the window only puts it away.
pub fn set_close_hides(on: bool) {
    CLOSE_HIDES.store(on, Relaxed);
}

pub fn close_hides() -> bool {
    CLOSE_HIDES.load(Relaxed) && WINDOW.load(Relaxed) != 0
}

pub fn current() -> State {
    *STATE.lock().unwrap()
}

pub fn set(s: State) {
    let mut cur = STATE.lock().unwrap();
    if *cur != s {
        *cur = s;
        post(WM_STATE);
    }
}

/// The balloon discord shows the first time its window goes to the tray.
pub fn hint() {
    post(WM_HINT);
}

/// Takes the icon down on the way out, so no dead one is left behind.
pub fn remove() {
    let w = WINDOW.swap(0, Relaxed);
    if w != 0 {
        let d = data(w as HWND);
        unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
    }
}

fn post(msg: u32) {
    let w = WINDOW.load(Relaxed);
    if w != 0 {
        unsafe { PostMessageW(w as HWND, msg, 0, 0) };
    }
}

fn run() {
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide("RustcordTray");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = inst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        // a hidden top-level window rather than a message-only one: those miss TaskbarCreated
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
        if hwnd.is_null() {
            return;
        }
        dark_menus(hwnd);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Relaxed);
        WINDOW.store(hwnd as isize, Relaxed);
        add(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Lets the menu follow Windows' dark mode, as Discord's does. The two exports are uxtheme's
/// unnamed SetPreferredAppMode (135) and FlushMenuThemes (136), plus AllowDarkModeForWindow (133).
unsafe fn dark_menus(hwnd: HWND) {
    unsafe {
        let ux = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if ux.is_null() {
            return;
        }
        if let Some(f) = GetProcAddress(ux, 133 as _) {
            std::mem::transmute::<_, unsafe extern "system" fn(HWND, i32) -> i32>(f)(hwnd, 1);
        }
        if let Some(f) = GetProcAddress(ux, 135 as _) {
            std::mem::transmute::<_, unsafe extern "system" fn(i32) -> i32>(f)(1);
        }
        if let Some(f) = GetProcAddress(ux, 136 as _) {
            std::mem::transmute::<_, unsafe extern "system" fn()>(f)();
        }
    }
}

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = 1;
    d
}

/// The face for the current state at the notification area's size; the shell keeps its own copy.
fn icon() -> HICON {
    let face = current().face();
    let h = crate::winsys::ico(face, unsafe { GetSystemMetrics(SM_CXSMICON) }).unwrap_or(std::ptr::null_mut());
    let old = ICON.with(|c| c.replace(h));
    if !old.is_null() {
        unsafe { DestroyIcon(old) };
    }
    h
}

fn add(hwnd: HWND) {
    let mut d = data(hwnd);
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = icon();
    copy(&mut d.szTip, "Rustcord");
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &d);
        d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &d);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => match (lp & 0xffff) as u32 {
            KEYSELECT | NIN_SELECT => send("open"),
            WM_CONTEXTMENU => unsafe { menu(hwnd, wp as i16 as i32, (wp >> 16) as i16 as i32) },
            _ => {}
        },
        WM_STATE => {
            let mut d = data(hwnd);
            d.uFlags = NIF_ICON;
            d.hIcon = icon();
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
        }
        WM_HINT => {
            let mut d = data(hwnd);
            d.uFlags = NIF_INFO;
            copy(&mut d.szInfoTitle, "Rustcord");
            copy(&mut d.szInfo, HINT);
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Relaxed) => add(hwnd),
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    0
}

/// Discord's tray menu on Windows: its name greyed out with the icon beside it, the voice
/// controls while in a call, and Quit.
unsafe fn menu(hwnd: HWND, x: i32, y: i32) {
    let s = current();
    unsafe {
        let m = CreatePopupMenu();
        let name = wide("Rustcord");
        let bmp = bitmap();
        let mut item: MENUITEMINFOW = std::mem::zeroed();
        item.cbSize = size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_STRING | MIIM_STATE | MIIM_BITMAP;
        item.fState = MFS_DISABLED;
        item.dwTypeData = name.as_ptr() as *mut u16;
        item.hbmpItem = bmp;
        InsertMenuItemW(m, 0, 1, &item);
        let add = |flags: MENU_ITEM_FLAGS, id: usize, label: &str| {
            let w = wide(label);
            AppendMenuW(m, flags, id, w.as_ptr());
        };
        let checked = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        if s.connected {
            add(MF_SEPARATOR, 0, "");
            match s.server_mute {
                true => add(MF_STRING | MF_GRAYED | MF_CHECKED, ID_MUTE, "Server Muted"),
                false => add(MF_STRING | checked(s.mute), ID_MUTE, "Mute"),
            }
            match s.server_deaf {
                true => add(MF_STRING | MF_GRAYED | MF_CHECKED, ID_DEAFEN, "Server Deafened"),
                false => add(MF_STRING | checked(s.deaf), ID_DEAFEN, "Deafen"),
            }
            add(MF_STRING, ID_VOICE, "Open Voice & Video Settings");
        }
        add(MF_SEPARATOR, 0, "");
        add(MF_STRING, ID_QUIT, "Quit Rustcord");
        // without this the menu stays up when the user clicks elsewhere
        SetForegroundWindow(hwnd);
        let picked = TrackPopupMenuEx(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, x, y, hwnd, std::ptr::null()) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(m);
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        match picked {
            ID_MUTE => send("mute"),
            ID_DEAFEN => send("deafen"),
            ID_VOICE => send("voice_settings"),
            ID_QUIT => send("quit"),
            _ => {}
        }
    }
}

/// The idle face as a 32-bit bitmap with premultiplied alpha, which is what a menu item takes.
fn bitmap() -> HBITMAP {
    let px = unsafe { GetSystemMetrics(SM_CXSMICON) };
    let Some(img) = crate::winsys::ico_entry(IDLE, px).and_then(|(_, png)| image::load_from_memory(png).ok()) else {
        return std::ptr::null_mut();
    };
    let img = image::imageops::resize(&img.to_rgba8(), px as u32, px as u32, image::imageops::FilterType::Triangle);
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: px,
        biHeight: -px,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { std::mem::zeroed() }
    };
    let mut bits = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(std::ptr::null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0) };
    if bmp.is_null() || bits.is_null() {
        return std::ptr::null_mut();
    }
    let out = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, (px * px * 4) as usize) };
    for (o, p) in out.chunks_exact_mut(4).zip(img.pixels()) {
        let [r, g, b, a] = p.0;
        let pre = |c: u8| (c as u32 * a as u32 / 255) as u8;
        o.copy_from_slice(&[pre(b), pre(g), pre(r), a]);
    }
    bmp
}
