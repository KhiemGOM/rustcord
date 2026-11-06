//! Discord's local services: rich presence over named pipes for games, the loopback socket
//! websites use for "Open in app", and `discord://` links. They run on one thread of their own;
//! what they ask of the app comes back down a channel as `Msg`.

mod activity;
mod ipc;
pub mod link;
mod web;

use std::sync::mpsc::{Receiver, Sender};
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
static OUT: OnceLock<(Mutex<Sender<Msg>>, egui::Context)> = OnceLock::new();
static BUS: OnceLock<broadcast::Sender<Bus>> = OnceLock::new();
static PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
static PIPE: OnceLock<String> = OnceLock::new();

/// Test runs bind names and ports of their own, so they never answer for a Discord that's
/// really running, or hand a game's presence to the mock.
pub fn testing() -> bool {
    ["RUSTCORD_E2E", "RUSTCORD_TOUR", "RUSTCORD_BENCH"].iter().any(|k| std::env::var_os(k).is_some())
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
    let (tx, rx) = std::sync::mpsc::channel();
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

fn send(m: Msg) {
    if let Some((tx, ctx)) = OUT.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(m);
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
