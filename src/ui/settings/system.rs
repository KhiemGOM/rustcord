use egui::Ui;

use super::privacy::write;
use super::wire::Patch;
use super::{Look, action, dropdown, note, radio, section, sub, toggle};
use crate::ui::App;

pub use super::keybinds::combo;

const LOCALES: [(&str, &str, &str); 32] = [
    ("id", "Bahasa Indonesia", "Indonesian"),
    ("da", "Dansk", "Danish"),
    ("de", "Deutsch", "German"),
    ("en-GB", "English, UK", "English, UK"),
    ("en-US", "English, US", "English, US"),
    ("es-ES", "Español", "Spanish"),
    ("es-419", "Español, LATAM", "Spanish, LATAM"),
    ("fr", "Français", "French"),
    ("hr", "Hrvatski", "Croatian"),
    ("it", "Italiano", "Italian"),
    ("lt", "Lietuviškai", "Lithuanian"),
    ("hu", "Magyar", "Hungarian"),
    ("nl", "Nederlands", "Dutch"),
    ("no", "Norsk", "Norwegian"),
    ("pl", "Polski", "Polish"),
    ("pt-BR", "Português do Brasil", "Portuguese, Brazilian"),
    ("ro", "Română", "Romanian"),
    ("fi", "Suomi", "Finnish"),
    ("sv-SE", "Svenska", "Swedish"),
    ("vi", "Tiếng Việt", "Vietnamese"),
    ("tr", "Türkçe", "Turkish"),
    ("cs", "Čeština", "Czech"),
    ("el", "Ελληνικά", "Greek"),
    ("bg", "български", "Bulgarian"),
    ("ru", "Pусский", "Russian"),
    ("uk", "Українська", "Ukrainian"),
    ("hi", "हिन्दी", "Hindi"),
    ("th", "ไทย", "Thai"),
    ("zh-CN", "中文", "Chinese, China"),
    ("ja", "日本語", "Japanese"),
    ("zh-TW", "繁體中文", "Chinese, Taiwan"),
    ("ko", "한국어", "Korean"),
];

pub fn system(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "General");
    let s = &mut app.settings;
    let mut changed = false;
    if toggle(ui, &mut s.open_on_startup, "Automatically open Rustcord when your computer starts up", "") {
        crate::winsys::set_startup(s.open_on_startup);
        changed = true;
    }
    ui.add_enabled_ui(s.open_on_startup, |ui| {
        let mut on = s.start_minimized && s.open_on_startup;
        if toggle(ui, &mut on, "Start Rustcord minimized", "Start Rustcord in the background, out of your way.") {
            s.start_minimized = on;
            changed = true;
        }
    });
    changed |= toggle(ui, &mut s.minimize_to_tray, "Minimize Rustcord to System Tray", "Clicking X minimizes Rustcord instead of closing it completely.");
    changed |= toggle(ui, &mut s.hw_accel, "Enable Hardware Acceleration", "Uses your GPU to make Rustcord run more smoothly. Turn this off if you’re experiencing visual glitches like frame drops in games or performance problems.");
    if !s.hw_accel {
        note(ui, "Hardware acceleration is disabled, which degrades system performance and visual quality.");
    }
    changed |= super::keybinds::custom(app, ui);
    super::keybinds::defaults(ui);
    changed
}

pub fn language(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Language & Time");
    super::caption(
        ui,
        "Select a Language",
        "Rustcord's own interface is English for now. This tells Discord which language to use for text it sends, like system messages and bot commands.",
    );
    super::lead(ui);
    let mut changed = false;
    let cur = LOCALES.iter().find(|l| l.0 == app.settings.locale).map_or("English, US", |l| l.1);
    let w = ui.available_width();
    let head = dropdown(ui, w, None, cur, |ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (code, native, english) in LOCALES {
                let r = ui.selectable_label(app.settings.locale == code, format!("{native}  ·  {english}"));
                super::mark(ui, &format!("lang:{code}"), r.rect);
                if r.clicked() {
                    app.settings.locale = code.to_string();
                    write(app, Patch::new().text(12, 1, code));
                    changed = true;
                }
            }
        });
    });
    super::mark(ui, "lang", head.rect);
    super::end(ui, 32.0);
    sub(ui, "Time format");
    let mut cycle = app.settings.hour_cycle;
    if radio(ui, &mut cycle, &[(0, "Auto", ""), (1, "12-hour", ""), (2, "24-hour", "")]) {
        app.settings.hour_cycle = cycle;
        write(app, Patch::new().num(13, 9, cycle as u64));
        changed = true;
    }
    changed
}

pub fn developer(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Developer Mode");
    let mut changed = false;
    if toggle(
        ui,
        &mut app.settings.dev,
        "Developer Mode",
        "Developer Mode exposes context menu items helpful for people writing apps using the Discord API.",
    ) {
        write(app, Patch::new().num(13, 2, app.settings.dev as u64));
        changed = true;
    }
    section(ui, "Cache");
    if action(
        ui,
        "Image Cache",
        "Avatars, emoji and images are cached on disk so they appear instantly. Clearing it frees space; everything downloads again as needed.",
        "Clear",
        Look::Danger,
    ) {
        let _ = std::fs::remove_dir_all(crate::cache::dir().join("img"));
        app.toast("Image cache cleared.");
        changed = true;
    }
    changed
}
