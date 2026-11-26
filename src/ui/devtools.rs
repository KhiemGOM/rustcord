//! Lightweight on-demand Performance, Memory, and Network HUD (toggled with Ctrl+Shift+I
//! or Settings > Developer). Allocates nothing while closed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, CornerRadius, Frame, Margin, RichText, Sense, Stroke, pos2, vec2};

use super::App;
use crate::theme::*;

const CAP: usize = 120;

#[derive(Clone, Copy, Default)]
pub struct MemInfo {
    pub working_set_bytes: usize,
    pub private_bytes: usize,
}

#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCountersEx {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_usage: usize,
}

unsafe extern "system" {
    fn K32GetProcessMemoryInfo(process: *mut std::ffi::c_void, counters: *mut ProcessMemoryCountersEx, cb: u32) -> i32;
}

pub fn query_mem() -> MemInfo {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c = ProcessMemoryCountersEx { cb: std::mem::size_of::<ProcessMemoryCountersEx>() as u32, ..Default::default() };
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok != 0 {
        MemInfo { working_set_bytes: c.working_set_size, private_bytes: c.private_usage }
    } else {
        MemInfo::default()
    }
}

pub struct State {
    frames_ms: VecDeque<f32>,
    mem: MemInfo,
    mem_at: Instant,
}

impl Default for State {
    fn default() -> Self {
        Self {
            frames_ms: VecDeque::with_capacity(CAP),
            mem: query_mem(),
            mem_at: Instant::now(),
        }
    }
}

pub fn stats_ms(samples: &VecDeque<f32>) -> (f32, f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted: Vec<f32> = samples.iter().copied().collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
    let p95 = sorted[((sorted.len() as f32 * 0.95) as usize).min(sorted.len() - 1)];
    let max = *sorted.last().unwrap_or(&0.0);
    (avg, p95, max)
}

pub fn toggle(app: &mut App) {
    if app.devtools.is_some() {
        app.devtools = None;
    } else {
        app.devtools = Some(State::default());
    }
}

pub fn record(app: &mut App, elapsed: Duration) {
    let Some(st) = &mut app.devtools else { return };
    if st.frames_ms.len() >= CAP {
        st.frames_ms.pop_front();
    }
    st.frames_ms.push_back(elapsed.as_secs_f32() * 1000.0);
}

pub fn show(app: &mut App, ctx: &Context) {
    if app.devtools.is_none() {
        return;
    }
    if let Some(st) = &mut app.devtools
        && st.mem_at.elapsed() >= Duration::from_millis(500)
    {
        st.mem = query_mem();
        st.mem_at = Instant::now();
    }
    let (avg, p95, max_ms, samples, mem) = {
        let st = app.devtools.as_ref().unwrap();
        let (a, p, m) = stats_ms(&st.frames_ms);
        (a, p, m, st.frames_ms.clone(), st.mem)
    };
    let tex_mb = egui::load::TextureLoader::byte_size(&*app.textures) as f32 / (1024.0 * 1024.0);
    let guilds = app.store.guilds.len();
    let chans = app.store.channels.len();
    let users = app.store.users.len();
    let msgs: usize = app.store.msgs.values().map(|m| m.list.len()).sum();
    let conn = format!("{:?}", app.conn);
    let voice_ping = app.voice.pings.back().map(|p| p.1);
    let voice_loss = app.voice.loss;

    let mut open = true;
    let mut do_trim = false;
    egui::Window::new("DevTools · Perf & Memory")
        .open(&mut open)
        .resizable(false)
        .default_width(300.0)
        .frame(Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::same(12)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(RichText::new(format!("Frame: avg {avg:.2} ms · p95 {p95:.2} ms · max {max_ms:.2} ms")).font(mono(12.0)).color(pal().header_primary));
            let (r, _) = ui.allocate_exact_size(vec2(276.0, 44.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_tertiary);
            let step = r.width() / CAP as f32;
            for (i, &ms) in samples.iter().enumerate() {
                let frac = (ms / 33.3).clamp(0.03, 1.0);
                let h = r.height() * frac;
                let col = if ms < 8.0 { pal().green } else if ms < 16.7 { pal().yellow } else { pal().red };
                let x = r.left() + i as f32 * step;
                let bar = egui::Rect::from_min_max(pos2(x, r.bottom() - h), pos2((x + step - 0.5).max(x + 1.0), r.bottom()));
                ui.painter().rect_filled(bar, 0, col);
            }
            ui.painter().hline(r.x_range(), r.bottom() - r.height() * 0.5, Stroke::new(1.0, Color32::from_white_alpha(40)));
            ui.painter().text(pos2(r.right() - 4.0, r.top() + 2.0), Align2::RIGHT_TOP, "33ms", mono(10.0), pal().text_muted);

            let ws_mb = mem.working_set_bytes as f32 / (1024.0 * 1024.0);
            let priv_mb = mem.private_bytes as f32 / (1024.0 * 1024.0);
            ui.separator();
            ui.label(RichText::new(format!("RAM: {ws_mb:.1} MB WS · {priv_mb:.1} MB Private · {tex_mb:.1} MB Tex")).font(mono(12.0)).color(pal().text_normal));
            ui.label(RichText::new(format!("Store: {guilds} guilds · {chans} chans · {users} users · {msgs} msgs")).font(mono(12.0)).color(pal().text_muted));
            let vline = match voice_ping {
                Some(p) => format!("Gateway: {conn} · Voice: {p} ms ({:.1}% loss)", voice_loss.unwrap_or(0.0)),
                None => format!("Gateway: {conn} · Voice: idle"),
            };
            ui.label(RichText::new(vline).font(mono(12.0)).color(pal().text_muted));
            if super::grey_button(ui, "Trim Working Set", 136.0).clicked() {
                do_trim = true;
            }
        });
    if do_trim {
        crate::winsys::trim();
        if let Some(st) = &mut app.devtools {
            st.mem = query_mem();
        }
    }
    if !open {
        app.devtools = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_frame_time_summary_and_queries_process_memory() {
        let mut q = VecDeque::new();
        for v in [1.0, 2.0, 3.0, 4.0, 10.0] {
            q.push_back(v);
        }
        let (avg, p95, max) = stats_ms(&q);
        assert!((avg - 4.0).abs() < 0.01);
        assert!((p95 - 10.0).abs() < 0.01);
        assert!((max - 10.0).abs() < 0.01);
        let mem = query_mem();
        assert!(mem.working_set_bytes > 0);
    }
}
