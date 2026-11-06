//! Purchases open here: a small WebView2 window owned by the main window, on discord.com with
//! the account signed in, so a checkout never leaves the app for the browser. One window at a
//! time, on its own thread; it keeps no profile on disk and only reaches Discord and the payment
//! providers Discord uses, anything else goes to the system browser.

pub mod child;
pub mod frame;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc;
use std::time::Duration;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DownloadStartingEventHandler, ExecuteScriptCompletedHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, WebMessageReceivedEventHandler, WindowCloseRequestedEventHandler, take_pwstr,
};
use windows::Win32::Foundation::{E_POINTER, HWND as Hwnd, RECT as WinRect};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW, GetWindowRect,
    ICON_SMALL, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW, SW_SHOW, SW_SHOWNOACTIVATE, SendMessageW, ShowWindow, TranslateMessage,
    IDC_ARROW, WM_APP, WM_CLOSE, WM_DESTROY, WM_GETICON, WM_SETICON, WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CLIPCHILDREN, WS_SYSMENU,
};

const WM_GO: u32 = WM_APP + 1;
const CLIENT: (i32, i32) = (640, 720);
const SURFACE: u32 = 0x0022_1f1e;
const INK: u32 = 0x00f5_f3f2;
const HOSTS: [&str; 7] = ["discord.com", "stripe.com", "adyen.com", "paypal.com", "braintreegateway.com", "venmo.com", "cash.app"];

static OPEN: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static NEXT: Mutex<Option<String>> = Mutex::new(None);
static SHOWN: Mutex<Option<String>> = Mutex::new(None);
static COUNT: AtomicU32 = AtomicU32::new(0);
static LIVE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

thread_local! {
    static VIEW: RefCell<Option<Rc<View>>> = const { RefCell::new(None) };
}

struct View {
    ctl: ICoreWebView2Controller,
    wv: ICoreWebView2,
    base: String,
    token: Option<String>,
    signed: Cell<bool>,
    target: RefCell<Option<String>>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn base() -> String {
    std::env::var("RUSTCORD_WEB").unwrap_or_else(|_| "https://discord.com".into())
}

/// Opens a discord.com page (`/store`, `/shop`, `/channels/<guild>/shop`, ...) in the checkout
/// window; one already open goes to the new page.
pub fn checkout(path: &str) {
    *NEXT.lock().unwrap() = Some(format!("{}{path}", base()));
    if BUSY.swap(true, Relaxed) {
        let w = OPEN.load(Relaxed);
        if w != 0 {
            unsafe { PostMessageW(w as HWND, WM_GO, 0, 0) };
        }
        return;
    }
    let parent = crate::winsys::WINDOW.load(Relaxed);
    let token = crate::cache::token();
    if std::thread::Builder::new().name("webwin".into()).spawn(move || run(parent, token)).is_err() {
        BUSY.store(false, Relaxed);
        if let Some(u) = NEXT.lock().unwrap().take() {
            browser(&u);
        }
    }
}

/// The page the window is showing, once it has loaded.
pub fn shown() -> Option<String> {
    SHOWN.lock().unwrap().clone()
}

pub fn is_open() -> bool {
    BUSY.load(Relaxed)
}

pub fn close() {
    let w = OPEN.load(Relaxed);
    if w != 0 {
        unsafe { PostMessageW(w as HWND, WM_CLOSE, 0, 0) };
    }
}

fn browser(url: &str) {
    if url.starts_with("https://") || url.starts_with("http://") {
        let _ = open::that_detached(url);
    }
}

fn allowed(url: &str, base: &str) -> bool {
    if url == "about:blank" {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    if reqwest::Url::parse(base).is_ok_and(|b| b.origin() == u.origin()) {
        return true;
    }
    u.scheme() == "https" && u.host_str().is_some_and(|h| HOSTS.iter().any(|d| h == *d || h.strip_suffix(d).is_some_and(|p| p.ends_with('.'))))
}

fn sign_in_script(token: &str) -> Option<String> {
    let safe = !token.is_empty() && token.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-+/=".contains(&b));
    safe.then(|| format!("localStorage.setItem('token',JSON.stringify('{token}'))"))
}

fn go(v: &View, url: &str) {
    let on_site = reqwest::Url::parse(url).ok().zip(reqwest::Url::parse(&v.base).ok()).is_some_and(|(u, b)| u.origin() == b.origin());
    if v.token.is_some() && !v.signed.get() && on_site {
        v.signed.set(true);
        *v.target.borrow_mut() = Some(url.to_string());
        navigate(v, &format!("{}/robots.txt", v.base));
    } else {
        navigate(v, url);
    }
}

fn navigate(v: &View, url: &str) {
    let _ = unsafe { v.wv.Navigate(&HSTRING::from(url)) };
}

fn source(wv: &ICoreWebView2) -> String {
    let mut s = PWSTR::null();
    unsafe { wv.Source(&mut s) }.map(|_| take_pwstr(s)).unwrap_or_default()
}

fn view() -> Option<Rc<View>> {
    VIEW.with(|v| v.borrow().clone())
}

fn fit(ctl: &ICoreWebView2Controller, hwnd: HWND) {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(hwnd, &mut r) };
    let _ = unsafe { ctl.SetBounds(WinRect { left: 0, top: 0, right: r.right, bottom: r.bottom }) };
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            if let Some(v) = view() {
                fit(&v.ctl, hwnd);
            }
            0
        }
        WM_GO => {
            if let Some(v) = view()
                && let Some(u) = NEXT.lock().unwrap().take()
            {
                go(&v, &u);
            }
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn create_window(parent: isize) -> HWND {
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide("RustcordCheckout");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = inst;
        wc.lpszClassName = class.as_ptr();
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.hbrBackground = CreateSolidBrush(SURFACE);
        RegisterClassW(&wc);

        let parent = parent as HWND;
        let dpi = if parent.is_null() { 96 } else { GetDpiForWindow(parent) }.max(96) as i32;
        let mut work: MONITORINFO = std::mem::zeroed();
        work.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromWindow(parent, MONITOR_DEFAULTTOPRIMARY), &mut work);
        let area = work.rcWork;
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let mut r = RECT { left: 0, top: 0, right: CLIENT.0 * dpi / 96, bottom: CLIENT.1 * dpi / 96 };
        r.right = r.right.min((area.right - area.left) * 9 / 10);
        r.bottom = r.bottom.min((area.bottom - area.top) * 9 / 10);
        AdjustWindowRectExForDpi(&mut r, style, 0, 0, dpi as u32);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let mut around = area;
        if !parent.is_null() {
            GetWindowRect(parent, &mut around);
        }
        let x = ((around.left + around.right - w) / 2).clamp(area.left, (area.right - w).max(area.left));
        let y = ((around.top + around.bottom - h) / 2).clamp(area.top, (area.bottom - h).max(area.top));
        let title = wide("Discord");
        let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), style, x, y, w, h, parent, std::ptr::null_mut(), inst, std::ptr::null());
        if hwnd.is_null() {
            return hwnd;
        }
        let dark = |attr: i32, v: u32| DwmSetWindowAttribute(hwnd, attr as u32, (&v as *const u32).cast(), 4);
        dark(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
        dark(DWMWA_CAPTION_COLOR, SURFACE);
        dark(DWMWA_BORDER_COLOR, SURFACE);
        dark(DWMWA_TEXT_COLOR, INK);
        if !parent.is_null() {
            let icon = SendMessageW(parent, WM_GETICON, ICON_SMALL as usize, 0);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon);
        }
        hwnd
    }
}

type Done = Rc<RefCell<Option<Box<dyn FnOnce(windows::core::Result<ICoreWebView2Controller>)>>>>;

fn fire(done: &Done, r: windows::core::Result<ICoreWebView2Controller>) {
    if let Some(f) = done.borrow_mut().take() {
        f(r);
    }
}

/// Starts a WebView2 under `parent` with a private profile in `data`; `done` gets the controller
/// from the thread's message loop, nothing here blocks.
pub(crate) fn create(parent: HWND, data: &Path, args: &str, done: impl FnOnce(windows::core::Result<ICoreWebView2Controller>) + 'static) {
    let folder = HSTRING::from(data.as_os_str());
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe { options.set_additional_browser_arguments(args.to_string()) };
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let parent = Hwnd(parent);
    let done: Done = Rc::new(RefCell::new(Some(Box::new(done))));
    let on_env = {
        let done = done.clone();
        CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |hr, env| {
            let started = hr.and_then(|_| env.ok_or_else(|| windows::core::Error::from(E_POINTER))).and_then(|env| {
                let done = done.clone();
                let h = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |hr, ctl| {
                    fire(&done, hr.and_then(|_| ctl.ok_or_else(|| windows::core::Error::from(E_POINTER))));
                    Ok(())
                }));
                unsafe {
                    match env.cast::<ICoreWebView2Environment10>() {
                        Ok(e) => e.CreateCoreWebView2ControllerOptions().and_then(|o| {
                            o.SetIsInPrivateModeEnabled(true)?;
                            e.CreateCoreWebView2ControllerWithOptions(parent, &o, &h)
                        }),
                        Err(_) => env.CreateCoreWebView2Controller(parent, &h),
                    }
                }
            });
            if let Err(e) = started {
                fire(&done, Err(e));
            }
            Ok(())
        }))
    };
    let started = unsafe { CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &folder, &options, &on_env) };
    if let Err(e) = started {
        fire(&done, Err(e));
    }
}

fn setup(hwnd: HWND, base: &str, token: Option<String>, data: &Path) -> windows::core::Result<()> {
    let (tx, rx) = mpsc::channel();
    create(hwnd, data, "", move |r| {
        let _ = tx.send(r);
    });
    let ctl = webview2_com::wait_with_pump(rx).map_err(|_| windows::core::Error::from(E_POINTER))??;

    let wv = unsafe { ctl.CoreWebView2()? };
    unsafe {
        if let Ok(c) = ctl.cast::<ICoreWebView2Controller2>() {
            let _ = c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR { A: 255, R: 0x1e, G: 0x1f, B: 0x22 });
        }
        let s = wv.Settings()?;
        s.SetAreDevToolsEnabled(false)?;
        s.SetIsStatusBarEnabled(false)?;
        ctl.SetIsVisible(true)?;
    }
    fit(&ctl, hwnd);

    let mut t = 0i64;
    let origin = base.to_string();
    unsafe {
        wv.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut u = PWSTR::null();
                    args.Uri(&mut u)?;
                    let url = take_pwstr(u);
                    if !allowed(&url, &origin) {
                        args.SetCancel(true)?;
                        browser(&url);
                    }
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut u = PWSTR::null();
                    args.Uri(&mut u)?;
                    let url = take_pwstr(u);
                    args.SetHandled(true)?;
                    match view() {
                        Some(v) if allowed(&url, &v.base) => go(&v, &url),
                        _ => browser(&url),
                    }
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(|_, args| {
                let (Some(v), Some(args)) = (view(), args) else { return Ok(()) };
                let src = source(&v.wv);
                let Some(target) = v.target.borrow_mut().take() else {
                    *SHOWN.lock().unwrap() = Some(src);
                    return Ok(());
                };
                let mut ok = Default::default();
                args.IsSuccess(&mut ok)?;
                let script = v.token.as_deref().and_then(sign_in_script).filter(|_| ok.as_bool() && src == format!("{}/robots.txt", v.base));
                match script {
                    Some(js) => {
                        let done = ExecuteScriptCompletedHandler::create(Box::new(move |_, _| {
                            if let Some(v) = view() {
                                navigate(&v, &target);
                            }
                            Ok(())
                        }));
                        let _ = v.wv.ExecuteScript(&HSTRING::from(js), &done);
                    }
                    None => navigate(&v, &target),
                }
                Ok(())
            })),
            &mut t,
        )?;
        ctl.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let (mut kind, mut key) = (Default::default(), 0u32);
                    args.KeyEventKind(&mut kind)?;
                    args.VirtualKey(&mut key)?;
                    if kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && key == 0x1b {
                        args.SetHandled(true)?;
                        PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    }
                }
                Ok(())
            })),
            &mut t,
        )?;
    }

    let v = Rc::new(View { ctl, wv, base: base.to_string(), token, signed: Cell::new(false), target: RefCell::new(None) });
    VIEW.with(|c| *c.borrow_mut() = Some(v.clone()));
    if let Some(u) = NEXT.lock().unwrap().take() {
        go(&v, &u);
    }
    Ok(())
}

fn sweep(root: &Path) {
    let mine = format!("{}-", std::process::id());
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if LIVE.lock().unwrap().contains(&e.path()) {
            continue;
        }
        let stale = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a > Duration::from_secs(86400));
        if stale || e.file_name().to_string_lossy().starts_with(&mine) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn run(parent: isize, token: Option<String>) {
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let root = crate::cache::dir().join("webview2");
    sweep(&root);
    let data: PathBuf = root.join(format!("{}-{}", std::process::id(), COUNT.fetch_add(1, Relaxed)));
    LIVE.lock().unwrap().push(data.clone());
    let hwnd = create_window(parent);
    let up = !hwnd.is_null() && setup(hwnd, &base(), token, &data).is_ok();
    if up {
        OPEN.store(hwnd as isize, Relaxed);
        let quiet = std::env::var_os("RUSTCORD_E2E").is_some() || std::env::var_os("RUSTCORD_BENCH").is_some();
        unsafe { ShowWindow(hwnd, if quiet { SW_SHOWNOACTIVATE } else { SW_SHOW }) };
        let mut msg = unsafe { std::mem::zeroed() };
        while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    } else {
        if !hwnd.is_null() {
            unsafe { DestroyWindow(hwnd) };
        }
        if let Some(u) = NEXT.lock().unwrap().take() {
            browser(&u);
        }
    }
    if let Some(v) = VIEW.with(|c| c.borrow_mut().take()) {
        let _ = unsafe { v.ctl.Close() };
    }
    OPEN.store(0, Relaxed);
    *SHOWN.lock().unwrap() = None;
    BUSY.store(false, Relaxed);
    for _ in 0..10 {
        if std::fs::remove_dir_all(&data).is_ok() || !data.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    LIVE.lock().unwrap().retain(|p| *p != data);
    unsafe { CoUninitialize() };
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "https://discord.com";

    #[test]
    fn payment_and_discord_hosts_stay_in_the_window() {
        for u in [
            "https://discord.com/store",
            "https://www.discord.com/shop",
            "https://js.stripe.com/v3/",
            "https://checkoutshopper-live.adyen.com/x",
            "https://www.paypal.com/checkoutnow",
            "about:blank",
        ] {
            assert!(allowed(u, B), "{u}");
        }
    }

    #[test]
    fn everything_else_leaves() {
        for u in [
            "https://evil.com/",
            "https://discord.com.evil.com/",
            "https://evildiscord.com/",
            "https://discord.com@evil.com/",
            "http://discord.com/store",
            "javascript:alert(1)",
            "file:///c:/windows/win.ini",
            "ms-msdt:/id",
            "https://notstripe.com/",
        ] {
            assert!(!allowed(u, B), "{u}");
        }
    }

    #[test]
    fn only_a_plain_token_is_scripted() {
        assert!(sign_in_script("abc.DEF_-123").is_some());
        assert!(sign_in_script("a'); alert(1);//").is_none());
        assert!(sign_in_script("").is_none());
    }
}
