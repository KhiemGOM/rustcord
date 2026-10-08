use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as smpsc;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::api::{self, Api};
use crate::gateway;
use crate::state::{Id, Page};
use crate::voice;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Conn {
    Offline,
    Connecting,
    Resuming,
    Online,
}

pub enum Ev {
    Dispatch(String, Value),
    Conn(Conn),
    AuthFailed,
    Messages { channel: Id, page: Page, list: Vec<Value> },
    MessagesFailed { channel: Id },
    SendFailed { channel: Id, nonce: String, err: String },
    LoggedIn(String),
    NeedMfa(String),
    LoginError(String),
    Error(String),
    Toast(String),
    Rest { tag: Tag, res: Result<Value, String> },
    Voice(voice::Ev),
}

/// What a generic REST reply is for, so the UI knows where to put it.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Pins(Id),
    Search(Id),
    Profile(Id),
    Forum(Id),
    Invite,
    MadeInvite,
    OpenDm,
    Friend,
    Commands(Id),
    Gifs(String),
    GifCats,
    Inbox,
    /// settings pages park their data under a key, see `App::remote`
    Remote(&'static str),
    Quiet,
}

pub enum Cmd {
    Start(String),
    Stop,
    Login { login: String, password: String },
    Mfa { ticket: String, code: String },
    Messages { channel: Id, page: Page },
    Send { channel: Id, content: String, nonce: String, reply: Option<(Id, bool)>, files: Vec<(PathBuf, String)>, stickers: Vec<Id> },
    Download { url: String, path: PathBuf },
    Interact(Value),
    Edit { channel: Id, id: Id, content: String },
    Delete { channel: Id, id: Id },
    React { channel: Id, id: Id, emoji: String, add: bool },
    Ack { channel: Id, id: Id },
    Typing { channel: Id },
    Gateway(Value),
    Rest { method: &'static str, path: String, body: Option<Value>, tag: Tag },
    Voice(voice::Cmd),
}

/// With FASTCORD_READONLY set, nothing goes out that would change the account or
/// that other people could see: no messages, reactions, acks, typing, voice,
/// presence, or non-GET REST calls. Used when touring a real account.
pub static BLOCKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn allowed(c: &Cmd) -> bool {
    match c {
        Cmd::Send { .. } | Cmd::Interact(_) | Cmd::Edit { .. } | Cmd::Delete { .. } | Cmd::React { .. } => false,
        Cmd::Ack { .. } | Cmd::Typing { .. } | Cmd::Voice(_) => false,
        Cmd::Rest { method, .. } => *method == "GET",
        // member requests and lazy member-list subscriptions only read
        Cmd::Gateway(v) => matches!(v["op"].as_u64(), Some(8) | Some(14) | Some(37)),
        _ => true,
    }
}

#[derive(Clone)]
pub struct Tx {
    tx: smpsc::Sender<Ev>,
    ctx: egui::Context,
}

impl Tx {
    pub fn send(&self, e: Ev) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }
}

pub struct Net {
    pub rt: Arc<Runtime>,
    cmd: mpsc::UnboundedSender<Cmd>,
}

impl Net {
    pub fn new(ctx: egui::Context) -> (Net, smpsc::Receiver<Ev>) {
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("net")
                .build()
                .expect("tokio"),
        );
        let (etx, erx) = smpsc::channel();
        let (ctx_tx, crx) = mpsc::unbounded_channel();
        let tx = Tx { tx: etx, ctx };
        rt.spawn(dispatch(crx, tx));
        (Net { rt, cmd: ctx_tx }, erx)
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }
}

async fn dispatch(mut rx: mpsc::UnboundedReceiver<Cmd>, tx: Tx) {
    let http = api::http();
    let mut api = Arc::new(Api::new(http.clone(), String::new()));
    let mut gw: Option<mpsc::UnboundedSender<Value>> = None;
    let mut voice = voice::Link::default();

    let readonly = std::env::var_os("FASTCORD_READONLY").is_some();
    while let Some(cmd) = rx.recv().await {
        if readonly && !allowed(&cmd) {
            BLOCKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        }
        let a = api.clone();
        let tx = tx.clone();
        match cmd {
            Cmd::Start(token) => {
                api = Arc::new(Api::new(http.clone(), token.clone()));
                let (gtx, grx) = mpsc::unbounded_channel();
                gw = Some(gtx);
                tokio::spawn(gateway::run(token, tx, grx));
            }
            Cmd::Stop => {
                gw = None;
                api = Arc::new(Api::new(http.clone(), String::new()));
            }
            Cmd::Gateway(v) => {
                if let Some(g) = &gw {
                    let _ = g.send(v);
                }
            }
            Cmd::Login { login, password } => {
                tokio::spawn(async move { login_result(&tx, a.login(&login, &password).await) });
            }
            Cmd::Mfa { ticket, code } => {
                tokio::spawn(async move { login_result(&tx, a.mfa(&ticket, &code).await) });
            }
            Cmd::Messages { channel, page } => {
                tokio::spawn(async move {
                    match a.messages(channel, page).await {
                        Ok(Value::Array(list)) => tx.send(Ev::Messages { channel, page, list }),
                        _ => tx.send(Ev::MessagesFailed { channel }),
                    }
                });
            }
            Cmd::Send { channel, content, nonce, reply, files, stickers } => {
                tokio::spawn(async move {
                    if let Err(err) = a.send(channel, &content, &nonce, reply, &files, &stickers).await {
                        tx.send(Ev::SendFailed { channel, nonce, err });
                    }
                });
            }
            Cmd::Edit { channel, id, content } => {
                tokio::spawn(async move {
                    if let Err(e) = a.edit(channel, id, &content).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::Delete { channel, id } => {
                tokio::spawn(async move {
                    if let Err(e) = a.delete(channel, id).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::React { channel, id, emoji, add } => {
                tokio::spawn(async move {
                    if let Err(e) = a.react(channel, id, &emoji, add).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::Ack { channel, id } => {
                tokio::spawn(async move {
                    let _ = a.ack(channel, id).await;
                });
            }
            Cmd::Rest { method, path, body, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
                    let res = a.call(m, &path, body).await;
                    match (&tag, &res) {
                        (Tag::Quiet, Ok(_)) => {}
                        (Tag::Quiet, Err(e)) => tx.send(Ev::Error(e.clone())),
                        _ => tx.send(Ev::Rest { tag, res }),
                    }
                });
            }
            Cmd::Download { url, path } => {
                let http = http.clone();
                tokio::spawn(async move {
                    let res = async {
                        let b = http.get(&url).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
                        tokio::fs::write(&path, &b).await.map_err(|e| e.to_string())
                    };
                    match res.await {
                        Ok(()) => tx.send(Ev::Toast(format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))),
                        Err(e) => tx.send(Ev::Error(e)),
                    }
                });
            }
            Cmd::Interact(payload) => {
                tokio::spawn(async move {
                    if let Err(e) = a.interact(payload).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::Typing { channel } => {
                tokio::spawn(async move {
                    let _ = a.typing(channel).await;
                });
            }
            Cmd::Voice(c) => voice.cmd(c, &tx),
        }
    }
}

fn login_result(tx: &Tx, r: Result<Value, String>) {
    match r {
        Ok(v) if v["token"].is_string() => tx.send(Ev::LoggedIn(v["token"].as_str().unwrap().to_string())),
        Ok(v) if v["mfa"].as_bool() == Some(true) => {
            tx.send(Ev::NeedMfa(v["ticket"].as_str().unwrap_or("").to_string()))
        }
        Ok(v) if v.get("captcha_key").is_some() => tx.send(Ev::LoginError(
            "Discord wants a captcha for this login. Use a token instead.".into(),
        )),
        Ok(v) => {
            let msg = v["errors"]["login"]["_errors"][0]["message"]
                .as_str()
                .or_else(|| v["errors"]["password"]["_errors"][0]["message"].as_str())
                .or_else(|| v["message"].as_str())
                .unwrap_or("Login failed.");
            tx.send(Ev::LoginError(msg.to_string()))
        }
        Err(e) => tx.send(Ev::LoginError(e)),
    }
}
