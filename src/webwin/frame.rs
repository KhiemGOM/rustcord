//! A video embed's web view. The host from `child.rs` sits inside a window of its own that is
//! only ever the part of the embed the chat shows, so Windows clips the page to the chat's
//! viewport and it never draws over the header, the composer or the panels around it.

use serde_json::Value;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{BLACK_BRUSH, ClientToScreen, CombineRgn, CreateRectRgn, DeleteObject, GetStockObject, RGN_DIFF, SetWindowRgn};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetWindowRect, IsWindowVisible, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
};

use super::child::{Cfg, Host, Status};

pub struct Frame {
    host: Option<Host>,
    win: HWND,
    main: HWND,
    hushed: std::cell::Cell<bool>,
    hole: std::cell::Cell<(Option<[i32; 4]>, [i32; 2])>,
}

fn meet(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let (l, t) = (a[0].max(b[0]), a[1].max(b[1]));
    let (r, bt) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    (r > l && bt > t).then_some([l, t, r - l, bt - t])
}

impl Frame {
    pub fn open(ctx: &egui::Context, main: isize, src: String) -> Frame {
        let class: Vec<u16> = "RustcordEmbed\0".encode_utf16().collect();
        let win = unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.hInstance = inst;
            wc.lpszClassName = class.as_ptr();
            wc.hbrBackground = GetStockObject(BLACK_BRUSH) as _;
            RegisterClassW(&wc);
            CreateWindowExW(0, class.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, 0, 0, main as HWND, std::ptr::null_mut(), inst, std::ptr::null())
        };
        let host = Host::open(ctx, win as isize, Cfg { src, hello: Value::Null, embed: true });
        Frame { host: Some(host), win, main: main as HWND, hushed: Default::default(), hole: Default::default() }
    }

    pub fn status(&self) -> Status {
        self.host.as_ref().map_or(Status::Failed, Host::status)
    }

    /// `over` is something egui draws over the embed, which the window leaves a hole for.
    pub fn place(&self, full: [i32; 4], clip: [i32; 4], show: bool, over: Option<[i32; 4]>) {
        let Some(host) = &self.host else { return };
        let seen = meet(full, clip).filter(|_| show);
        match seen {
            Some(v) => {
                unsafe { SetWindowPos(self.win, std::ptr::null_mut(), v[0], v[1], v[2], v[3], SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW) };
                self.carve(over.and_then(|h| meet(h, v)).map(|h| [h[0] - v[0], h[1] - v[1], h[2], h[3]]), [v[2], v[3]]);
                host.place(self.win as isize, [full[0] - v[0], full[1] - v[1], full[2], full[3]], true);
            }
            None => {
                unsafe { ShowWindow(self.win, SW_HIDE) };
                host.place(self.win as isize, [0, 0, full[2], full[3]], false);
            }
        }
    }

    fn carve(&self, hole: Option<[i32; 4]>, size: [i32; 2]) {
        if self.hole.replace((hole, size)) == (hole, size) {
            return;
        }
        unsafe {
            let rgn = match hole {
                Some([x, y, w, h]) => {
                    let (all, cut) = (CreateRectRgn(0, 0, size[0], size[1]), CreateRectRgn(x, y, x + w, y + h));
                    CombineRgn(all, all, cut, RGN_DIFF);
                    DeleteObject(cut);
                    all
                }
                None => std::ptr::null_mut(),
            };
            SetWindowRgn(self.win, rgn, 1);
        }
    }

    /// The hole the window has now, in its own pixels.
    pub fn hole(&self) -> Option<[i32; 4]> {
        self.hole.get().0
    }

    pub fn fullscreen(&self) -> bool {
        self.host.as_ref().is_some_and(Host::fullscreen)
    }

    pub fn press(&self, x: i32, y: i32) {
        if let Some(host) = &self.host {
            host.press(x, y);
        }
    }

    pub fn hush(&self, on: bool) {
        if self.hushed.replace(on) != on
            && let Some(host) = &self.host
        {
            host.hush(on);
        }
    }

    /// Only while we're in front: SetFocus activates the window, over whatever the user has open.
    pub fn blur(&self) {
        unsafe {
            if GetForegroundWindow() == self.main {
                SetFocus(self.main);
            }
        }
    }

    /// Where the window is in the main window's pixels, whether it is shown, and the page's own
    /// bounds inside it.
    pub fn probe(&self) -> (bool, [i32; 4], [i32; 4]) {
        let mut r: RECT = unsafe { std::mem::zeroed() };
        let mut o = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            GetWindowRect(self.win, &mut r);
            ClientToScreen(self.main, &mut o);
        }
        let shown = unsafe { IsWindowVisible(self.win) } != 0 && self.host.as_ref().is_some_and(|h| h.placed().1);
        let bounds = self.host.as_ref().map_or([0; 4], |h| h.placed().0);
        (shown, [r.left - o.x, r.top - o.y, r.right - r.left, r.bottom - r.top], bounds)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.host.take();
        unsafe { DestroyWindow(self.win) };
    }
}

#[cfg(test)]
mod tests {
    use super::meet;

    #[test]
    fn only_the_part_in_the_viewport_shows() {
        let view = [100, 50, 400, 300];
        assert_eq!(meet([150, 60, 200, 100], view), Some([150, 60, 200, 100]));
        assert_eq!(meet([150, 300, 200, 100], view), Some([150, 300, 200, 50]));
        assert_eq!(meet([50, 20, 100, 100], view), Some([100, 50, 50, 70]));
        assert_eq!(meet([150, 400, 200, 100], view), None);
        assert_eq!(meet([150, 0, 200, 50], view), None);
    }
}
