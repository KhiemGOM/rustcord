#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod art;
#[cfg(test)]
mod fuzz;
mod audioplay;
mod auth;
mod cache;
mod clips;
mod cursor;
mod emoji_font;
mod emoji_names;
mod fonts;
mod gateway;
mod gif;
mod highlight;
mod hotkeys;
mod imgcache;
mod logo;
mod lottie;
mod markdown;
mod media;
mod mp4;
mod net;
mod proto;
mod rpc;
mod sound;
mod spell;
mod speech;
mod state;
mod theme;
mod games;
mod procs;
mod shareable;
mod toast;
mod tray;
mod upload;
mod ui;
mod voice;
mod webauthn;
mod webwin;
mod winsys;

fn main() -> eframe::Result {
    if !rpc::link::first_instance() {
        return Ok(());
    }
    let t0 = std::time::Instant::now();
    let _ = rustls::crypto::ring::default_provider().install_default();
    if let Some(local) = dirs::data_local_dir()
        && std::env::var_os("RUSTCORD_DATA").is_none()
        && !cache::dir().exists()
    {
        let _ = std::fs::rename(local.join("Fastcord"), cache::dir());
    }
    // parse the snapshot while the graphics card's driver loads
    let snap = std::sync::Mutex::new(Some(std::thread::spawn(cache::load)));
    let size = cache::window().unwrap_or([1280.0, 800.0]);
    // the logo on the window and taskbar, as on the exe
    let logo = logo::rgba(256);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Rustcord")
        .with_decorations(false)
        .with_app_id("rustcord")
        .with_inner_size(size)
        .with_min_inner_size([940.0, 500.0]);
    if let Some(l) = logo {
        viewport = viewport.with_icon(l);
    }
    // a harness run (e2e, tour, replay, bench) opens behind everything and never takes focus, so
    // whoever's at the machine isn't interrupted; its input is injected anyway
    winsys::keep_down();
    if winsys::harness() {
        viewport = viewport.with_active(false).with_window_level(egui::WindowLevel::AlwaysOnBottom);
    } else if !std::env::args().any(|a| a == "--startup") {
        winsys::show_early(size, cache::window_color());
    }
    let opts = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        "Rustcord",
        opts,
        Box::new(|cc| {
            let snap = snap.lock().unwrap().take().ok_or("started twice")?;
            Ok(Box::new(ui::App::new(cc, t0, snap)))
        }),
    )
}

