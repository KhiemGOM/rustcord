#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod audioplay;
mod auth;
mod cache;
mod emoji_font;
mod fonts;
mod gateway;
mod gif;
mod hotkeys;
mod imgcache;
mod markdown;
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
mod shareable;
mod toast;
mod tray;
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
    // parse the snapshot while the GL context spins up
    let snap = std::thread::spawn(cache::load);
    let size = cache::window().unwrap_or([1280.0, 800.0]);
    // discord's own logo on the window and taskbar, as on the exe
    let logo = image::load_from_memory(include_bytes!("../assets/discord.png")).map(|i| i.into_rgba8()).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Rustcord")
        .with_decorations(false)
        .with_app_id("rustcord")
        .with_inner_size(size)
        .with_min_inner_size([940.0, 500.0]);
    if let Some(l) = logo {
        viewport = viewport.with_icon(egui::IconData { width: l.width(), height: l.height(), rgba: l.into_raw() });
    }
    // the e2e run opens behind everything and never takes focus, so whoever's at the machine
    // isn't interrupted; its input is injected anyway
    if std::env::var_os("RUSTCORD_E2E").is_some() || std::env::var_os("RUSTCORD_BENCH").is_some() {
        viewport = viewport.with_active(false).with_window_level(egui::WindowLevel::AlwaysOnBottom);
    }
    let opts = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Rustcord",
        opts,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, t0, snap)))),
    )
}
