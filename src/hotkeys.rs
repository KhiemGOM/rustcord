//! Global keybinds. Windows will tell anyone the state of any key via
//! GetAsyncKeyState, so a small polling thread is enough, no hooks needed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

static BINDS: Mutex<Vec<(String, Vec<u32>)>> = Mutex::new(Vec::new());
// the polling thread, woken when binds arrive: with none set it sleeps instead of polling
static POLLER: std::sync::OnceLock<std::thread::Thread> = std::sync::OnceLock::new();

/// Held while any push-to-talk bind is down. The mic thread reads it directly
/// so talking doesn't wait for a UI frame.
pub static PTT_HELD: AtomicBool = AtomicBool::new(false);

fn down(vk: u32) -> bool {
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Whether anything is being polled for.
pub fn watching() -> bool {
    BINDS.lock().is_ok_and(|b| !b.is_empty())
}

pub fn set(binds: &[(String, Vec<u32>)]) {
    if let Ok(mut b) = BINDS.lock() {
        *b = binds.iter().filter(|(_, k)| !k.is_empty()).cloned().collect();
    }
    if let Some(t) = POLLER.get() {
        t.unpark();
    }
}

/// Polls every 10ms while any bind is set and reports (action, pressed) edges.
pub fn start(ctx: egui::Context, tx: Sender<(String, bool)>) {
    std::thread::Builder::new()
        .name("hotkeys".into())
        .spawn(move || {
            let _ = POLLER.set(std::thread::current());
            let mut held: Vec<bool> = Vec::new();
            loop {
                if BINDS.lock().is_ok_and(|b| b.is_empty()) {
                    PTT_HELD.store(false, Ordering::Relaxed);
                    held.clear();
                    std::thread::park();
                    continue;
                }
                std::thread::sleep(Duration::from_millis(10));
                let Ok(binds) = BINDS.lock() else { continue };
                held.resize(binds.len(), false);
                let mut ptt = false;
                for (i, (action, keys)) in binds.iter().enumerate() {
                    let now = keys.iter().all(|k| down(*k));
                    if action == "ptt" {
                        ptt |= now;
                    }
                    if now != held[i] {
                        held[i] = now;
                        let _ = tx.send((action.clone(), now));
                        ctx.request_repaint();
                    }
                }
                PTT_HELD.store(ptt, Ordering::Relaxed);
            }
        })
        .ok();
}

/// Keybind recording: collect every key pressed until all are let go again.
/// Generic shift/ctrl/alt codes stand in for their left/right twins, and plain
/// left/right clicks are ignored so the user can still click the button.
pub fn record(keys: &mut Vec<u32>) -> Option<Vec<u32>> {
    let mut any = false;
    for vk in 0x03..=0xFEu32 {
        if matches!(vk, 0xA0..=0xA5) || !down(vk) {
            continue;
        }
        any = true;
        if !keys.contains(&vk) {
            keys.push(vk);
        }
    }
    if any || keys.is_empty() {
        return None;
    }
    // modifiers first, the way people read combos
    keys.sort_by_key(|k| (!matches!(k, 0x10..=0x12 | 0x5B | 0x5C), *k));
    Some(std::mem::take(keys))
}
