//! Discord's local services: rich presence over named pipes for games, the loopback socket
//! websites use for "Open in app", and `discord://` links. They run on one thread of their own;
//! what they ask of the app comes back down a channel as `Msg`.

mod activity;
mod ipc;
pub mod link;
mod web;

use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use tokio::sync::broadcast;

use crate::api::Api;
use crate::state::Id;

pub enum Msg {
    /// A game's rich presence, ready for the gateway; None once it clears it or its pipe closes.
    Activity { conn: u64, app: Id, name: String, icon: Option<String>, activity: Option<Value> },
    Invite(String),
    Template(String),
    Open(link::Link),
    CloseJoinRequest(Id),
    Consent { app: Id, name: String, icon: Option<String>, scopes: Vec<String>, reply: tokio::sync::oneshot::Sender<bool> },
}

#[derive(Clone)]
enum Bus {
    JoinRequest { app: Id, user: Value },
    SignedOut,
}

struct Signed {
    token: String,
    user: Value,
    session: String,
}

static SIGNED: Mutex<Option<Signed>> = Mutex::new(None);
static ACTS: Mutex<std::collections::BTreeMap<u64, Msg>> = Mutex::new(std::collections::BTreeMap::new());
static OUT: OnceLock<(Mutex<SyncSender<Msg>>, egui::Context)> = OnceLock::new();
static BUS: OnceLock<broadcast::Sender<Bus>> = OnceLock::new();
static PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
static PIPE: OnceLock<String> = OnceLock::new();

pub fn is_startup_launch() -> bool {
    for arg in std::env::args().skip(1) {
        if arg == "--" {
            break;
        }
        if arg.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("discord://")) {
            continue;
        }
        if arg == "--startup" {
            return true;
        }
    }
    false
}

pub(super) fn user_sid() -> Option<String> {
    static SID: OnceLock<Option<String>> = OnceLock::new();
    SID.get_or_init(|| unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
        use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
        use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        let mut tok = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return None;
        }
        let mut len = 0u32;
        GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut len);
        if len == 0 {
            CloseHandle(tok);
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) != 0;
        CloseHandle(tok);
        if !ok {
            return None;
        }
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = std::ptr::null_mut();
        if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 || s.is_null() {
            return None;
        }
        let mut n = 0usize;
        while *s.add(n) != 0 {
            n += 1;
        }
        let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
        LocalFree(s.cast());
        Some(out)
    })
    .clone()
}

pub(super) fn create_pipe(opts: &mut tokio::net::windows::named_pipe::ServerOptions, path: &str) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    let sddl = match user_sid() {
        Some(sid) => format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"),
        None => "D:P(A;;GA;;;OW)(A;;GA;;;SY)".into(),
    };
    let wide = crate::winsys::wide(&sddl);
    let mut sd = std::ptr::null_mut();
    let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) };
    if ok == 0 || sd.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let res = unsafe { opts.create_with_security_attributes_raw(path, (&mut sa as *mut SECURITY_ATTRIBUTES).cast()) };
    unsafe { LocalFree(sd) };
    res
}

const BUILTIN_ADMINISTRATORS: &str = "S-1-5-32-544";

pub(super) fn same_owner(file: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
    let Some(mine) = user_sid() else { return false };
    unsafe {
        let mut owner = std::ptr::null_mut();
        let mut sd = std::ptr::null_mut();
        if GetSecurityInfo(file.as_raw_handle(), SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION, &mut owner, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &mut sd) != 0 {
            return false;
        }
        let mut s = std::ptr::null_mut();
        let ok = !owner.is_null() && ConvertSidToStringSidW(owner, &mut s) != 0 && !s.is_null();
        let same = if ok {
            let mut n = 0usize;
            while *s.add(n) != 0 {
                n += 1;
            }
            let got = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            got == mine || got == BUILTIN_ADMINISTRATORS
        } else {
            false
        };
        if !sd.is_null() {
            LocalFree(sd);
        }
        same
    }
}

/// Test runs bind names and ports of their own, so they never answer for a Discord that's
/// really running, or hand a game's presence to the mock.
pub fn testing() -> bool {
    crate::winsys::harness()
}

pub fn scope_text(scope: &str) -> Option<&'static str> {
    ipc::SCOPES.iter().find(|(n, _)| *n == scope).map(|(_, t)| *t)
}

pub fn web_port() -> u16 {
    PORT.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn pipe_name() -> Option<&'static str> {
    PIPE.get().map(String::as_str)
}

pub fn start(ctx: egui::Context) -> Receiver<Msg> {
    let (tx, rx) = std::sync::mpsc::sync_channel(64);
    let _ = OUT.set((Mutex::new(tx), ctx));
    let _ = BUS.set(broadcast::channel(16).0);
    std::thread::Builder::new()
        .name("rpc".into())
        .spawn(|| {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            rt.block_on(async {
                tokio::spawn(web::serve());
                tokio::spawn(link::serve());
                ipc::serve().await;
                std::future::pending::<()>().await;
            });
        })
        .ok();
    rx
}

pub fn signed_in(token: &str, user: &Value, session: &str) {
    if let Ok(mut s) = SIGNED.lock() {
        *s = Some(Signed { token: token.to_string(), user: user.clone(), session: session.to_string() });
    }
}

pub fn signed_out() {
    if let Ok(mut s) = SIGNED.lock() {
        *s = None;
    }
    if let Some(b) = BUS.get() {
        let _ = b.send(Bus::SignedOut);
    }
}

/// Someone asking to join the game we're showing arrives as a dm; games that subscribed to
/// ACTIVITY_JOIN_REQUEST hear about it.
pub fn on_message(d: &Value) {
    let me = SIGNED.lock().ok().and_then(|s| s.as_ref().map(|s| s.user["id"].clone()));
    if d["activity"]["type"] != 5 || me.as_ref() == Some(&d["author"]["id"]) {
        return;
    }
    let app = d["application_id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    if let Some(b) = BUS.get() {
        let _ = b.send(Bus::JoinRequest { app, user: d["author"].clone() });
    }
}

/// Each pipe's latest rich presence since the last call; only the newest counts, so they're kept
/// apart from the queue and a game spamming updates can't crowd out anything else.
pub fn activities() -> Vec<Msg> {
    ACTS.lock().map(|mut a| std::mem::take(&mut *a).into_values().collect()).unwrap_or_default()
}

fn send(m: Msg) {
    if let Some((tx, ctx)) = OUT.get() {
        if let Msg::Activity { conn, .. } = m {
            if let Ok(mut a) = ACTS.lock() {
                a.insert(conn, m);
            }
        } else if let Ok(tx) = tx.lock() {
            let _ = tx.try_send(m);
        }
        ctx.request_repaint();
    }
}

fn bus() -> broadcast::Receiver<Bus> {
    BUS.get().map(|b| b.subscribe()).unwrap_or_else(|| broadcast::channel(1).1)
}

fn session() -> Option<(Value, String)> {
    SIGNED.lock().ok()?.as_ref().map(|s| (s.user.clone(), s.session.clone()))
}

async fn call(method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    let token = SIGNED.lock().ok().and_then(|s| s.as_ref().map(|s| s.token.clone())).ok_or("signed out")?;
    let http = HTTP.get_or_init(crate::api::http).clone();
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    Api::new(http, token).call(m, path, body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_pipe_dacl_matches_current_user() {
        let sid = user_sid().expect("current user sid");
        assert!(sid.starts_with("S-1-"), "{sid}");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let name = format!(r"\\.\pipe\rustcord-dacl-test-{}", std::process::id());
            let mut opts = tokio::net::windows::named_pipe::ServerOptions::new();
            opts.first_pipe_instance(true).reject_remote_clients(true);
            let _server = create_pipe(&mut opts, &name).expect("create private pipe");
            let client = std::fs::OpenOptions::new().write(true).open(&name).expect("open private pipe");
            assert!(same_owner(&client));
        });
    }
}
