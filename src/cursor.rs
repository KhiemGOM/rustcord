//! The shop's own pointer: Discord's two cursor pictures, made into system cursors and handed to
//! the window whenever the shop asks for them.

use std::sync::atomic::{AtomicIsize, AtomicU8, AtomicU32, Ordering::Relaxed};

use resvg::{tiny_skia, usvg};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, ScreenToClient};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetClientRect, GetCursorPos, HCURSOR, ICONINFO, IDC_ARROW, LoadCursorW, SetCursor, WindowFromPoint,
};

pub const ARROW: usize = 0;
pub const POINTER: usize = 1;

static CURSORS: [AtomicIsize; 2] = [AtomicIsize::new(0), AtomicIsize::new(0)];
static SCALE: AtomicU32 = AtomicU32::new(0);
static MODE: AtomicU8 = AtomicU8::new(0);

pub fn ready(scale: f32) -> bool {
    SCALE.load(Relaxed) == scale.to_bits() && CURSORS.iter().all(|c| c.load(Relaxed) != 0)
}

pub fn handle(kind: usize) -> isize {
    CURSORS[kind].load(Relaxed)
}

pub fn mode() -> u8 {
    MODE.load(Relaxed)
}

pub fn build(kind: usize, svg: &[u8], scale: f32) -> bool {
    let Ok(tree) = usvg::Tree::from_data(svg, &usvg::Options::default()) else { return false };
    let size = tree.size();
    let scale = scale.min(128.0 / size.width().max(size.height()));
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let Some(mut pix) = tiny_skia::Pixmap::new(w, h) else { return false };
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pix.as_mut());
    let mut bgra = Vec::with_capacity(pix.pixels().len() * 4);
    let mut mask = vec![0u8; (w as usize).div_ceil(16) * 2 * h as usize];
    for (i, p) in pix.pixels().iter().enumerate() {
        let c = p.demultiply();
        bgra.extend([c.blue(), c.green(), c.red(), c.alpha()]);
        if c.alpha() == 0 {
            let (x, y) = (i % w as usize, i / w as usize);
            mask[y * (w as usize).div_ceil(16) * 2 + x / 8] |= 0x80 >> (x % 8);
        }
    }
    let cursor = unsafe {
        let color = CreateBitmap(w as i32, h as i32, 1, 32, bgra.as_ptr().cast());
        let hole = CreateBitmap(w as i32, h as i32, 1, 1, mask.as_ptr().cast());
        let info = ICONINFO { fIcon: 0, xHotspot: 0, yHotspot: 0, hbmMask: hole, hbmColor: color };
        let c = CreateIconIndirect(&info);
        DeleteObject(color as _);
        DeleteObject(hole as _);
        c
    };
    if cursor.is_null() {
        return false;
    }
    let old = CURSORS[kind].swap(cursor as isize, Relaxed);
    if old != 0 {
        unsafe { DestroyIcon(old as HCURSOR) };
    }
    SCALE.store(scale.to_bits(), Relaxed);
    true
}

fn pointer_in_window() -> bool {
    let hwnd = crate::winsys::WINDOW.load(Relaxed) as HWND;
    let mut at = POINT { x: 0, y: 0 };
    let mut client: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        if GetCursorPos(&mut at) == 0 || WindowFromPoint(at) != hwnd {
            return false;
        }
        ScreenToClient(hwnd, &mut at);
        GetClientRect(hwnd, &mut client);
    }
    at.x >= client.left && at.x < client.right && at.y >= client.top && at.y < client.bottom
}

/// 0 leaves the pointer to whoever sets it, 1 is the arrow and 2 the hand.
pub fn show(mode: u8) {
    let before = MODE.swap(mode, Relaxed);
    if mode == before || !pointer_in_window() {
        return;
    }
    match mode {
        0 => unsafe {
            SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
        },
        m => unsafe {
            SetCursor(handle(m as usize - 1) as HCURSOR);
        },
    }
}

/// The window asked which pointer to show: ours, while the shop wants one.
pub fn apply() -> bool {
    match MODE.load(Relaxed) {
        0 => false,
        m => {
            unsafe { SetCursor(handle(m as usize - 1) as HCURSOR) };
            true
        }
    }
}

/// What the system shows once the window has been asked: the e2e's proof that the pointer is ours.
pub fn probe() -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursor, HTCLIENT, SendMessageW, WM_SETCURSOR};
    let hwnd = crate::winsys::WINDOW.load(Relaxed) as HWND;
    unsafe {
        SendMessageW(hwnd, WM_SETCURSOR, hwnd as usize, HTCLIENT as isize);
        GetCursor() as isize
    }
}
