//! Adding a card, managing a subscription and confirming a payment open here: a small WebView2 window owned by the main window, on discord.com with
//! the account signed in, so a checkout never leaves the app for the browser. One window at a
//! time, on its own thread; it keeps no profile on disk. It loads only Discord's standalone
//! billing pages and the payment providers they use. The account's token is put in place by a
//! script that runs before the page's own, so the target is the first and only page loaded and
//! the window is dark until it paints. Browsing is never done here: any other page is refused.

pub mod child;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc;
use std::time::Duration;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, DownloadStartingEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, WebMessageReceivedEventHandler, WindowCloseRequestedEventHandler, take_pwstr,
};
use windows::Win32::Foundation::{E_POINTER, HWND as Hwnd, RECT as WinRect};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WAIT_TIMEOUT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW, GetWindowRect,
    ICON_SMALL, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW, SW_SHOW, SW_SHOWNOACTIVATE, SendMessageW, ShowWindow, TranslateMessage,
    IDC_ARROW, WM_APP, WM_CLOSE, WM_DESTROY, WM_GETICON, WM_KEYDOWN, WM_SETICON, WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CLIPCHILDREN, WS_SYSMENU,
};

const WM_GO: u32 = WM_APP + 1;
const PAYMENT_STEP_WIDTH: i32 = 480;
const PAYMENT_STEP_MIN_HEIGHT: i32 = 750;
const SPACE_24: i32 = 24;
const BROWSER_EXIT_MS: u32 = 15_000;
const CLIENT: (i32, i32) = (PAYMENT_STEP_WIDTH + 2 * SPACE_24, PAYMENT_STEP_MIN_HEIGHT);
const SURFACE: u32 = 0x0022_1f1e;
const INK: u32 = 0x00f5_f3f2;
const SMALL: [&str; 2] = ["/billing/premium/manage", "/billing/payment-sources/create"];
const HOSTS: [&str; 7] = ["discord.com", "stripe.com", "adyen.com", "paypal.com", "braintreegateway.com", "venmo.com", "cash.app"];

static OPEN: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static NEXT: Mutex<Option<String>> = Mutex::new(None);
static SHOWN: Mutex<Option<String>> = Mutex::new(None);
static COUNT: AtomicU32 = AtomicU32::new(0);
static BROWSER: AtomicU32 = AtomicU32::new(0);
static LIVE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

thread_local! {
    static VIEW: RefCell<Option<Rc<View>>> = const { RefCell::new(None) };
}

struct View {
    ctl: ICoreWebView2Controller,
    wv: ICoreWebView2,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn base() -> String {
    std::env::var("RUSTCORD_WEB").unwrap_or_else(|_| "https://discord.com".into())
}

pub fn manage() {
    open("/billing/premium/manage");
}

pub fn add_card() {
    open("/billing/payment-sources/create");
}

fn small(path: &str) -> bool {
    SMALL.contains(&path.split('?').next().unwrap_or_default())
}

fn open(path: &str) {
    if small(path) {
        go(format!("{}{path}", base()));
    }
}

/// A payment provider's page for a purchase that needs confirming, if it's one the window is
/// allowed to load. Only Discord's own pages ever see the account's token.
pub fn pay_page(url: &str) -> bool {
    let ok = url != "about:blank" && allowed(url, &base());
    if ok {
        go(url.to_string());
    }
    ok
}

fn go(url: String) {
    *NEXT.lock().unwrap() = Some(url);
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

pub fn press_escape() {
    let w = OPEN.load(Relaxed);
    if w != 0 {
        unsafe { PostMessageW(w as HWND, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
    }
}

pub fn profiles_left() -> usize {
    let mine = format!("{}-", std::process::id());
    std::fs::read_dir(crate::cache::dir().join("webview2")).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&mine)).count()
}

fn browser(url: &str) {
    if url.starts_with("https://") || url.starts_with("http://") {
        let _ = open::that_detached(url);
    }
}

fn host_in(u: &reqwest::Url, d: &str) -> bool {
    u.host_str().is_some_and(|h| h == d || h.strip_suffix(d).is_some_and(|p| p.ends_with('.')))
}

fn on_discord(url: &str, base: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    reqwest::Url::parse(base).is_ok_and(|b| b.origin() == u.origin()) || (u.scheme() == "https" && host_in(&u, "discord.com"))
}

fn allowed(url: &str, base: &str) -> bool {
    if url == "about:blank" {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    if on_discord(url, base) {
        let p = u.path();
        return p.starts_with("/billing/") || p.starts_with("/login");
    }
    u.scheme() == "https" && HOSTS.iter().any(|d| host_in(&u, d))
}

fn sign_in_script(token: &str, base: &str) -> Option<String> {
    let safe = |s: &str, extra: &[u8]| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || extra.contains(&b));
    let origin = reqwest::Url::parse(base).ok()?.origin().ascii_serialization();
    (safe(token, b"._-+/=") && safe(&origin, b":/.-")).then(|| format!("if(location.origin==='{origin}')localStorage.setItem('token',JSON.stringify('{token}'))"))
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
                navigate(&v, &u);
            }
            0
        }
        WM_KEYDOWN if wp == VK_ESCAPE as usize => {
            unsafe { DestroyWindow(hwnd) };
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
        AdjustWindowRectExForDpi(&mut r, style, 0, 0, dpi as u32);
        let (w, h) = ((r.right - r.left).min(area.right - area.left), (r.bottom - r.top).min(area.bottom - area.top));
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
pub(crate) fn create(parent: HWND, data: &Path, done: impl FnOnce(windows::core::Result<ICoreWebView2Controller>) + 'static) {
    let folder = HSTRING::from(data.as_os_str());
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
    let started = unsafe { CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &folder, None::<&ICoreWebView2EnvironmentOptions>, &on_env) };
    if let Err(e) = started {
        fire(&done, Err(e));
    }
}

fn setup(hwnd: HWND, base: &str, token: Option<String>, data: &Path) -> windows::core::Result<()> {
    let (tx, rx) = mpsc::channel();
    create(hwnd, data, move |r| {
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

    let mut pid = 0u32;
    let _ = unsafe { wv.BrowserProcessId(&mut pid) };
    BROWSER.store(pid, Relaxed);

    if let Some(js) = token.as_deref().and_then(|t| sign_in_script(t, base)) {
        let (tx, rx) = mpsc::channel();
        let added = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |_, _| {
            let _ = tx.send(());
            Ok(())
        }));
        unsafe { wv.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(js), &added)? };
        let _ = webview2_com::wait_with_pump(rx);
    }

    let mut t = 0i64;
    let origin = base.to_string();
    let opens = origin.clone();
    unsafe {
        wv.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut u = PWSTR::null();
                    args.Uri(&mut u)?;
                    let url = take_pwstr(u);
                    if !allowed(&url, &origin) {
                        args.SetCancel(true)?;
                        if on_discord(&url, &origin) {
                            PostMessageW(hwnd, WM_CLOSE, 0, 0);
                        } else {
                            browser(&url);
                        }
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
                        Some(v) if allowed(&url, &opens) => navigate(&v, &url),
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
            &NavigationCompletedEventHandler::create(Box::new(|_, _| {
                if let Some(v) = view() {
                    *SHOWN.lock().unwrap() = Some(source(&v.wv));
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

    let v = Rc::new(View { ctl, wv });
    VIEW.with(|c| *c.borrow_mut() = Some(v.clone()));
    if let Some(u) = NEXT.lock().unwrap().take() {
        navigate(&v, &u);
    }
    Ok(())
}

fn running(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        alive
    }
}

fn wait_exit(pid: u32) {
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, BROWSER_EXIT_MS);
            CloseHandle(h);
        }
    }
}

fn sweep(root: &Path) {
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if LIVE.lock().unwrap().contains(&e.path()) {
            continue;
        }
        let owner = e.file_name().to_string_lossy().split('-').next().and_then(|p| p.parse::<u32>().ok());
        if owner.is_none_or(|p| p == std::process::id() || !running(p)) {
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
    let browser = BROWSER.swap(0, Relaxed);
    if browser != 0 {
        wait_exit(browser);
    }
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
    fn billing_pages_and_payment_hosts_stay_in_the_window() {
        for u in [
            "https://discord.com/billing/premium/subscribe?plan_id=1&gift=false",
            "https://www.discord.com/billing/payment-sources/create",
            "https://discord.com/billing/popup-bridge/abc",
            "https://js.stripe.com/v3/",
            "https://checkoutshopper-live.adyen.com/x",
            "https://www.paypal.com/checkoutnow",
            "about:blank",
        ] {
            assert!(allowed(u, B), "{u}");
        }
    }

    #[test]
    fn the_rest_of_discord_is_not_shown() {
        for u in ["https://discord.com/store", "https://discord.com/shop", "https://discord.com/channels/1/2", "https://discord.com/channels/@me", "https://discord.com/billing", "https://discord.com/settings/premium"] {
            assert!(!allowed(u, B), "{u}");
            assert!(on_discord(u, B), "{u}");
        }
    }

    #[test]
    fn everything_else_leaves() {
        for u in [
            "https://evil.com/",
            "https://discord.com.evil.com/billing/premium/subscribe",
            "https://evildiscord.com/",
            "https://discord.com@evil.com/",
            "http://discord.com/billing/premium/subscribe",
            "javascript:alert(1)",
            "file:///c:/windows/win.ini",
            "ms-msdt:/id",
            "https://notstripe.com/",
        ] {
            assert!(!allowed(u, B), "{u}");
            assert!(!on_discord(u, B), "{u}");
        }
    }

    #[test]
    fn only_standalone_billing_pages_open_the_window() {
        for p in ["/billing/premium/manage", "/billing/payment-sources/create"] {
            assert!(small(p), "{p}");
            assert!(allowed(&format!("{B}{p}"), B), "{p}");
        }
        for p in ["/store", "/shop", "/channels/1/shop", "/channels/1/boosts", "/settings/premium", "/billing/premium", "/billing/premium/subscribe?plan_id=1&gift=true", "/billing/guild-subscriptions/purchase?guild_id=5"] {
            assert!(!small(p), "{p}");
        }
    }

    #[test]
    fn only_a_plain_token_is_scripted() {
        assert!(sign_in_script("abc.DEF_-123", B).is_some());
        assert!(sign_in_script("a'); alert(1);//", B).is_none());
        assert!(sign_in_script("", B).is_none());
        assert!(sign_in_script("abc", "not a url").is_none());
    }
}
