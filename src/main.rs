#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod audioplay;
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
mod sound;
mod speech;
mod state;
mod theme;
mod games;
mod shareable;
mod toast;
mod ui;
mod vencord;
mod voice;
mod winsys;

fn main() -> eframe::Result {
    let t0 = std::time::Instant::now();
    // parse the snapshot while the GL context spins up
    let snap = std::thread::spawn(cache::load);
    let size = cache::window().unwrap_or([1280.0, 800.0]);
    // discord's own logo on the window and taskbar, as on the exe
    let logo = image::load_from_memory(include_bytes!("../assets/discord.png")).map(|i| i.into_rgba8()).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Fastcord")
        .with_decorations(false)
        .with_app_id("fastcord")
        .with_inner_size(size);
    // Vencord's "Disable minimum window size"
    let vencord: Option<serde_json::Value> = std::fs::read(cache::dir().join("vencord.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if !vencord.is_some_and(|v| v["disableMinSize"] == true) {
        viewport = viewport.with_min_inner_size([940.0, 500.0]);
    }
    if let Some(l) = logo {
        viewport = viewport.with_icon(egui::IconData { width: l.width(), height: l.height(), rgba: l.into_raw() });
    }
    // the e2e run opens behind everything and never takes focus, so whoever's at the machine
    // isn't interrupted; its input is injected anyway
    if std::env::var_os("FASTCORD_E2E").is_some() {
        viewport = viewport.with_active(false).with_window_level(egui::WindowLevel::AlwaysOnBottom);
    }
    let opts = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Fastcord",
        opts,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, t0, snap)))),
    )
}
