//! Who holds the heap. Build with `--features heapprof` (on the `fast` profile, for its pdb) and
//! run with `RUSTCORD_HEAPPROF=<file>`: every five seconds the live bytes by call stack are
//! written there, and to `<file>.peak` whenever the process's commit reaches a new high;
//! `python tools/heap.py <pid> <file>` names the stacks while the client runs.
//! Allocations of 2 KB and up are all followed, smaller ones one in 32 and counted 32 times.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

const DEPTH: usize = 14;
type Stack = [usize; DEPTH];

pub struct Prof;

static ON: AtomicBool = AtomicBool::new(false);
static LIVE: Mutex<Option<HashMap<usize, (Stack, usize)>>> = Mutex::new(None);

thread_local! {
    // set while the profiler itself allocates, and counts the small allocations it skips
    static BUSY: Cell<bool> = const { Cell::new(false) };
    static SKIPPED: Cell<u32> = const { Cell::new(0) };
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn RtlCaptureStackBackTrace(skip: u32, frames: u32, out: *mut usize, hash: *mut u32) -> u16;
}

fn follow(ptr: *mut u8, size: usize) {
    if ptr.is_null() || !ON.load(Relaxed) {
        return;
    }
    let weight = if size >= 2048 {
        1
    } else {
        let n = SKIPPED.with(|s| s.replace((s.get() + 1) % 32));
        if n != 0 {
            return;
        }
        32
    };
    if BUSY.with(|b| b.replace(true)) {
        return;
    }
    let mut stack = [0usize; DEPTH];
    unsafe { RtlCaptureStackBackTrace(2, DEPTH as u32, stack.as_mut_ptr(), std::ptr::null_mut()) };
    if let Some(live) = LIVE.lock().unwrap().as_mut() {
        live.insert(ptr as usize, (stack, size * weight));
    }
    BUSY.with(|b| b.set(false));
}

fn forget(ptr: *mut u8) {
    if !ON.load(Relaxed) || BUSY.with(|b| b.replace(true)) {
        return;
    }
    if let Some(live) = LIVE.lock().unwrap().as_mut() {
        live.remove(&(ptr as usize));
    }
    BUSY.with(|b| b.set(false));
}

unsafe impl GlobalAlloc for Prof {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        follow(p, layout.size());
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        follow(p, layout.size());
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        forget(ptr);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        forget(ptr);
        let p = unsafe { System.realloc(ptr, layout, size) };
        follow(p, size);
        p
    }
}

#[repr(C)]
#[derive(Default)]
struct Counters {
    cb: u32,
    faults: u32,
    sizes: [usize; 8],
    private: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
}

fn commit() -> usize {
    let mut c = Counters { cb: size_of::<Counters>() as u32, ..Default::default() };
    // -1 is this process
    unsafe { K32GetProcessMemoryInfo(-1, &mut c, c.cb) };
    c.private
}

/// The live bytes by call stack, heaviest first, one stack a line: bytes, allocations, frames.
fn table() -> String {
    let mut by: HashMap<Stack, (usize, usize)> = HashMap::new();
    for (stack, bytes) in LIVE.lock().unwrap().iter().flat_map(|l| l.values()) {
        let e = by.entry(*stack).or_default();
        *e = (e.0 + bytes, e.1 + 1);
    }
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_unstable_by_key(|r| std::cmp::Reverse(r.1.0));
    rows.iter().map(|(stack, (bytes, n))| format!("{bytes} {n} {}\n", stack.iter().map(|a| format!("{a:x}")).collect::<Vec<_>>().join(" "))).collect()
}

pub fn start() {
    let Some(path) = std::env::var_os("RUSTCORD_HEAPPROF") else { return };
    *LIVE.lock().unwrap() = Some(HashMap::new());
    ON.store(true, Relaxed);
    std::thread::spawn(move || {
        BUSY.with(|b| b.set(true));
        // the same table whenever the process's commit reaches a new high, in <file>.peak with
        // that commit in MB on a line of its own first: what a spike is made of is gone in seconds
        let mut peak = std::path::PathBuf::from(&path).into_os_string();
        peak.push(".peak");
        let (mut high, mut ticks) = (0, 0u32);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let now = commit();
            if now > high + (4 << 20) {
                high = now;
                let _ = std::fs::write(&peak, format!("{}\n{}", now >> 20, table()));
            }
            ticks += 1;
            if ticks % 100 == 0 {
                let _ = std::fs::write(&path, table());
            }
        }
    });
}
