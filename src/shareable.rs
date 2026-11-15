//! What Go Live can share, listed the way discord's picker does: open windows with a thumbnail
//! and their app's icon, each monitor, and capture devices. A thread keeps the list fresh while
//! the picker is up.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, ColorImage};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Window(isize),
    Screen(isize),
    Device(usize),
}

pub struct Item {
    pub kind: Kind,
    pub title: String,
    pub icon: Option<Arc<ColorImage>>,
    pub thumb: Option<Arc<ColorImage>>,
}

pub struct Shareable {
    pub items: Arc<Mutex<Vec<Item>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Shareable {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
    }
}

// thumbnails come out no bigger than the picker's tiles
const THUMB: (usize, usize) = (440, 248);

pub fn start(ctx: egui::Context) -> Shareable {
    let items = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (out, quit) = (items.clone(), stop.clone());
    std::thread::Builder::new()
        .name("share-list".into())
        .spawn(move || {
            let mut icons = HashMap::new();
            let cams = crate::voice::cameras();
            let devices = || cams.iter().enumerate().map(|(i, n)| Item { kind: Kind::Device(i), title: n.clone(), icon: None, thumb: None });
            // screens take no time, a picture of every window can take seconds
            let first: Vec<Item> = unsafe { screens() }.into_iter().chain(devices()).collect();
            *out.lock().unwrap_or_else(|e| e.into_inner()) = first;
            ctx.request_repaint();
            while !quit.load(Relaxed) {
                let mut v = unsafe { windows(&mut icons) };
                v.extend(unsafe { screens() });
                v.extend(devices());
                *out.lock().unwrap_or_else(|e| e.into_inner()) = v;
                ctx.request_repaint();
                // discord refreshes its thumbnails every couple of seconds
                for _ in 0..20 {
                    if quit.load(Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .ok();
    Shareable { items, stop }
}

fn wide_str(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())])
}

unsafe extern "system" fn each_window(h: HWND, l: LPARAM) -> i32 {
    unsafe { (*(l as *mut Vec<HWND>)).push(h) };
    1
}

/// Top level windows someone would want to share, front to back: visible, titled, not a tool
/// window or someone's popup, not hidden by the shell, and not ours.
unsafe fn windows(icons: &mut HashMap<String, Option<Arc<ColorImage>>>) -> Vec<Item> {
    let mut list: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(each_window), &mut list as *mut _ as LPARAM);
        let me = windows_sys::Win32::System::Threading::GetCurrentProcessId();
        let mut out = Vec::new();
        for h in list {
            if IsWindowVisible(h) == 0 || !GetWindow(h, GW_OWNER).is_null() || GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
                continue;
            }
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as *mut _, 4);
            if cloaked != 0 {
                continue;
            }
            let mut buf = [0u16; 256];
            GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32);
            if matches!(wide_str(&buf).as_str(), "Progman" | "Shell_TrayWnd" | "WorkerW") {
                continue;
            }
            let mut buf = [0u16; 512];
            GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
            let title = wide_str(&buf);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if title.trim().is_empty() || pid == me {
                continue;
            }
            let exe = exe_path(pid);
            let icon = match exe.as_ref().map(|p| icons.entry(p.clone()).or_insert_with(|| exe_icon(p).map(Arc::new)).clone()) {
                Some(Some(i)) => Some(i),
                _ => class_icon(h).map(Arc::new),
            };
            let thumb = if IsIconic(h) != 0 { None } else { print(h).map(Arc::new) };
            out.push(Item { kind: Kind::Window(h as isize), title, icon, thumb });
        }
        out
    }
}

unsafe fn exe_path(pid: u32) -> Option<String> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if p.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut n) != 0;
        CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

unsafe fn exe_icon(path: &str) -> Option<ColorImage> {
    let w: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut icon: HICON = std::ptr::null_mut();
    unsafe {
        if PrivateExtractIconsW(w.as_ptr(), 0, 64, 64, &mut icon, std::ptr::null_mut(), 1, 0) == 0 || icon.is_null() {
            return None;
        }
        let img = icon_image(icon, 64);
        DestroyIcon(icon);
        img
    }
}

unsafe fn class_icon(h: HWND) -> Option<ColorImage> {
    unsafe {
        let icon = GetClassLongPtrW(h, GCLP_HICON) as HICON;
        if icon.is_null() { None } else { icon_image(icon, 32) }
    }
}

/// A DIB section to draw into, top-down 32 bit, with its pixels.
struct Canvas {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: usize,
    h: usize,
}

impl Canvas {
    unsafe fn new(w: usize, h: usize) -> Option<Canvas> {
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w as i32;
            bmi.bmiHeader.biHeight = -(h as i32);
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bmp);
            Some(Canvas { dc, bmp, old, bits: bits as *mut u8, w, h })
        }
    }

    fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, self.w * self.h * 4) }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp);
            DeleteDC(self.dc);
        }
    }
}

unsafe fn icon_image(icon: HICON, size: usize) -> Option<ColorImage> {
    unsafe {
        let c = Canvas::new(size, size)?;
        DrawIconEx(c.dc, 0, 0, icon, size as i32, size as i32, 0, std::ptr::null_mut(), DI_NORMAL);
        let px = c.pixels();
        // old icons have no alpha at all; those are drawn opaque
        let any_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
        let pixels = px.chunks_exact(4).map(|p| if any_alpha { Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3]) } else { Color32::from_rgb(p[2], p[1], p[0]) }).collect();
        Some(ColorImage::new([size, size], pixels))
    }
}

/// The window as it looks right now, without its invisible resize border, shrunk to a tile.
unsafe fn print(h: HWND) -> Option<ColorImage> {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        let (w, ht) = ((r.right - r.left).clamp(1, 7680) as usize, (r.bottom - r.top).clamp(1, 4320) as usize);
        let mut f: RECT = std::mem::zeroed();
        let frame = DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut f as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32) == 0;
        let c = Canvas::new(w, ht)?;
        // 2 = PW_RENDERFULLCONTENT, which gets GPU-drawn windows too
        if windows_sys::Win32::Storage::Xps::PrintWindow(h, c.dc, 2) == 0 {
            return None;
        }
        let crop = if frame {
            ((f.left - r.left).max(0) as usize, (f.top - r.top).max(0) as usize, ((f.right - f.left).max(1) as usize).min(w), ((f.bottom - f.top).max(1) as usize).min(ht))
        } else {
            (0, 0, w, ht)
        };
        Some(shrink(c.pixels(), w, crop))
    }
}

const PW_CLIENT_FULL_CONTENT: u32 = 3;

/// A window's client area as it draws it, child windows included, even when other windows cover it.
pub fn client(h: isize) -> Option<ColorImage> {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h as HWND, &mut r);
        let (w, ht) = (r.right.max(1) as usize, r.bottom.max(1) as usize);
        let c = Canvas::new(w, ht)?;
        let ok = windows_sys::Win32::Storage::Xps::PrintWindow(h as HWND, c.dc, PW_CLIENT_FULL_CONTENT) != 0;
        ok.then(|| ColorImage::new([w, ht], c.pixels().chunks_exact(4).map(|p| Color32::from_rgb(p[2], p[1], p[0])).collect()))
    }
}

unsafe extern "system" fn each_monitor(m: HMONITOR, _: HDC, r: *mut RECT, l: LPARAM) -> i32 {
    unsafe { (*(l as *mut Vec<(HMONITOR, RECT)>)).push((m, *r)) };
    1
}

unsafe fn screens() -> Vec<Item> {
    let mut list: Vec<(HMONITOR, RECT)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each_monitor), &mut list as *mut _ as LPARAM);
        list.into_iter()
            .enumerate()
            .map(|(i, (m, r))| {
                let (w, h) = ((r.right - r.left).max(1) as usize, (r.bottom - r.top).max(1) as usize);
                let thumb = Canvas::new(w, h).and_then(|c| {
                    let screen = GetDC(std::ptr::null_mut());
                    let ok = BitBlt(c.dc, 0, 0, w as i32, h as i32, screen, r.left, r.top, SRCCOPY | CAPTUREBLT) != 0;
                    ReleaseDC(std::ptr::null_mut(), screen);
                    ok.then(|| shrink(c.pixels(), w, (0, 0, w, h)))
                });
                Item { kind: Kind::Screen(m as isize), title: format!("Screen {}", i + 1), icon: None, thumb: thumb.map(Arc::new) }
            })
            .collect()
    }
}

/// Box-filters the (x, y, w, h) part of a BGRA image down to fit a tile, keeping its shape.
fn shrink(px: &[u8], stride_w: usize, (x0, y0, w, h): (usize, usize, usize, usize)) -> ColorImage {
    let k = (THUMB.0 as f32 / w as f32).min(THUMB.1 as f32 / h as f32).min(1.0);
    let (tw, th) = (((w as f32 * k) as usize).max(1), ((h as f32 * k) as usize).max(1));
    let mut out = Vec::with_capacity(tw * th);
    for ty in 0..th {
        let (sy0, sy1) = (y0 + ty * h / th, (y0 + (ty + 1) * h / th).max(y0 + ty * h / th + 1));
        for tx in 0..tw {
            let (sx0, sx1) = (x0 + tx * w / tw, (x0 + (tx + 1) * w / tw).max(x0 + tx * w / tw + 1));
            // a few samples per output pixel is plenty for a thumbnail
            let (step_y, step_x) = (((sy1 - sy0) / 3).max(1), ((sx1 - sx0) / 3).max(1));
            let (mut s, mut n) = ([0u32; 3], 0u32);
            for y in (sy0..sy1).step_by(step_y) {
                for x in (sx0..sx1).step_by(step_x) {
                    let p = &px[(y * stride_w + x) * 4..][..3];
                    s[0] += p[2] as u32;
                    s[1] += p[1] as u32;
                    s[2] += p[0] as u32;
                    n += 1;
                }
            }
            out.push(Color32::from_rgb((s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8));
        }
    }
    ColorImage::new([tw, th], out)
}
