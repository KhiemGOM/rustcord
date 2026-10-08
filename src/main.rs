#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod cache;
mod gateway;
mod gif;
mod hotkeys;
mod imgcache;
mod markdown;
mod mp4;
mod net;
mod proto;
mod sound;
mod state;
mod theme;
mod ui;
mod voice;
mod winsys;

fn main() -> eframe::Result {
    let t0 = std::time::Instant::now();
    // parse the snapshot while the GL context spins up
    let snap = std::thread::spawn(cache::load);
    let size = cache::window().unwrap_or([1280.0, 800.0]);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Fastcord")
            .with_app_id("fastcord")
            .with_inner_size(size)
            .with_min_inner_size([940.0, 500.0]),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Fastcord",
        opts,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, t0, snap)))),
    )
}
