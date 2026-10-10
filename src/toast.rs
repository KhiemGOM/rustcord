//! Pings as Windows toasts, looking like discord's: the app's name and logo along the top (an
//! AUMID registered under HKCU, so Windows stops filing them under PowerShell), the sender's
//! avatar beside the text, and no sound of their own since the app plays its ping. Clicking one
//! hands the message back to the app.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use crate::state::Id;

const AUMID: &str = "Fastcord";

pub struct Ping {
    pub title: String,
    pub body: String,
    pub avatar: String,
    // channel and message a click opens
    pub at: (Id, Id),
}

static QUEUE: OnceLock<Mutex<Sender<Ping>>> = OnceLock::new();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Puts the app's name and icon where Windows looks them up for toasts.
fn register(dir: &Path) {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let icon = dir.join("fastcord.png");
    let logo = include_bytes!("../assets/discord.png");
    if std::fs::read(&icon).ok().as_deref() != Some(logo.as_slice()) {
        let _ = std::fs::write(&icon, logo);
    }
    let key = wide(&format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
    for (name, value) in [("DisplayName", "Fastcord".to_string()), ("IconUri", icon.display().to_string())] {
        let (name, value) = (wide(name), wide(&value));
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32);
        }
    }
}

/// Starts the toast thread; what comes back is every toast clicked, as (channel, message).
pub fn start(ctx: egui::Context) -> Receiver<(Id, Id)> {
    let (tx, rx) = std::sync::mpsc::channel::<Ping>();
    let (clicks, clicked) = std::sync::mpsc::channel();
    let _ = QUEUE.set(Mutex::new(tx));
    std::thread::Builder::new()
        .name("toasts".into())
        .spawn(move || {
            use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let dir = crate::cache::dir();
            register(&dir);
            let pics = dir.join("toast");
            let _ = std::fs::create_dir_all(&pics);
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok();
            let http = crate::api::http();
            // a toast has to outlive its click handler, so the last few stay alive here
            let mut alive = std::collections::VecDeque::new();
            for p in rx {
                let pic = rt.as_ref().and_then(|rt| avatar(rt, &http, &p.avatar, &pics));
                if let Ok(t) = show(&p, pic.as_deref(), clicks.clone(), ctx.clone()) {
                    alive.push_back(t);
                    if alive.len() > 20 {
                        alive.pop_front();
                    }
                }
            }
        })
        .ok();
    clicked
}

pub fn send(p: Ping) {
    if let Some(q) = QUEUE.get() {
        let _ = q.lock().unwrap_or_else(|e| e.into_inner()).send(p);
    }
}

/// The sender's avatar as a png on disk, where a toast can point at it.
fn avatar(rt: &tokio::runtime::Runtime, http: &reqwest::Client, url: &str, dir: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let path = dir.join(format!("{:016x}.png", h.finish()));
    if path.exists() {
        return Some(path);
    }
    let bytes = rt.block_on(async { http.get(url).send().await.ok()?.error_for_status().ok()?.bytes().await.ok() })?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Electron's toast, which is what discord sends: title, body, the avatar as the square app
/// logo override, and silent.
pub fn xml(p: &Ping, pic: Option<&Path>) -> String {
    let image = pic
        .map(|f| format!(r#"<image placement="appLogoOverride" hint-crop="none" src="file:///{}"/>"#, escape(&f.display().to_string().replace('\\', "/"))))
        .unwrap_or_default();
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text>{image}</binding></visual><audio silent="true"/></toast>"#,
        escape(&p.title),
        escape(&p.body)
    )
}

fn show(p: &Ping, pic: Option<&Path>, clicks: Sender<(Id, Id)>, ctx: egui::Context) -> windows::core::Result<windows::UI::Notifications::ToastNotification> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml(p, pic)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let at = p.at;
    toast.Activated(&TypedEventHandler::new(move |_, _| {
        let _ = clicks.send(at);
        ctx.request_repaint();
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)?;
    Ok(toast)
}
