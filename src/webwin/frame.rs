//! A video embed's web view. The host from `child.rs` sits inside a window of its own that is
//! only ever the part of the embed the chat shows, so Windows clips the page to the chat's
//! viewport and it never draws over the header, the composer or the panels around it.

use serde_json::Value;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowRect, IsWindowVisible, SW_HIDE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos, ShowWindow,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
};

use super::child::{Cfg, Host, Status};

pub struct Frame {
    host: Option<Host>,
    win: HWND,
    main: HWND,
}

fn meet(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let (l, t) = (a[0].max(b[0]), a[1].max(b[1]));
    let (r, bt) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    (r > l && bt > t).then_some([l, t, r - l, bt - t])
}

impl Frame {
    pub fn open(ctx: &egui::Context, main: isize, src: String) -> Frame {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let win = unsafe {
            CreateWindowExW(0, class.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, 0, 0, main as HWND, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null())
        };
        let host = Host::open(ctx, win as isize, Cfg { src, hello: Value::Null, embed: true });
        Frame { host: Some(host), win, main: main as HWND }
    }

    pub fn status(&self) -> Status {
        self.host.as_ref().map_or(Status::Failed, Host::status)
    }

    pub fn place(&self, full: [i32; 4], clip: [i32; 4], show: bool) {
        let Some(host) = &self.host else { return };
        let seen = meet(full, clip).filter(|_| show && matches!(host.status(), Status::Up));
        match seen {
            Some(v) => {
                unsafe { SetWindowPos(self.win, std::ptr::null_mut(), v[0], v[1], v[2], v[3], SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW) };
                host.place(self.win as isize, [full[0] - v[0], full[1] - v[1], full[2], full[3]], true);
            }
            None => {
                unsafe { ShowWindow(self.win, SW_HIDE) };
                host.place(self.win as isize, [0, 0, full[2], full[3]], false);
            }
        }
    }

    pub fn blur(&self) {
        unsafe { SetFocus(self.main) };
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
