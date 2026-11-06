use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as smpsc;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::api::{self, Api};
use crate::auth;
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
    // `again` when it'll be retried by itself
    SendFailed { channel: Id, nonce: String, err: String, again: bool },
    Auth(auth::Ev),
    Error(String),
    Toast(String),
    Rest { tag: Tag, res: Result<Value, String> },
    Voice(voice::Ev),
}

/// What a generic REST reply is for, so the UI knows where to put it.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    // the server behind the link in the Join a Server dialog
    InvitePreview(String),
    Pins(Id),
    Search(Id),
    Profile(Id),
    // the recent activity on someone's full profile
    Outbox(Id),
    // a server's tag popout data
    GuildProfile(Id),
    // a member's messages for Mod View, and their moderation info
    ModView(Id),
    Forum(Id),
    Invite,
    // an invite or template a website or link opened: its details, and the template once made
    Link(&'static str),
    MadeInvite,
    // a friend's new dm, for the invite link that goes in it
    InviteDm(String),
    // a dm opened from someone's profile, with what was typed in its message box
    OpenDm(String),
    // a dm made to ring someone in
    CallDm,
    // an invite made for this person, to go in their dm
    InviteTo(Id),
    // a thread made from its dialog, with the first message to post in it
    Thread(String),
    // saved messages: 0 the bookmark ids and reminders, 1 the bookmarks themselves
    Saved(u8),
    // the note you keep on someone
    Note(Id),
    Friend,
    Commands(Id),
    Gifs(String),
    GifCats,
    Welcome(Id),
    MemberActions(Id),
    Inbox,
    // the activity feed, and the icons of the apps in it
    Feed,
    AppIcons(Vec<Id>),
    // the server boosts page: 0 the powerups catalogue, 1 what's applied, 2 the boosts
    Boosts(Id, u8),
    // a channel, category or event made from its modal
    Created,
    NewServer,
    Lurk(Id),
    Joined(Id),
    // the app directory's lists, by what's showing
    Directory(String),
    // a server settings page's data
    Server(&'static str),
    // channel settings: the channel itself, its invites, webhooks or regions, or a save
    Chan(&'static str),
    // the members table, onboarding or a prune count, for the page over a server's channels
    Home(&'static str),
    // the events window: 0 the server's events, 1 the ones you're interested in
    Events(Id, u8),
    // who is interested in an event: its server and the event
    EventUsers(Id, Id),
    // the quests hub: 0 on offer, 1 claimed, 2 the Orbs balance, 3 an accept or claim went through
    Quests(u8),
    // a follow of an announcement channel, from its dialog
    Follow,
    /// settings pages park their data under a key, see `App::remote`
    Remote(&'static str),
    // the people who reacted to a message with an emoji: channel, message, emoji, whether super
    Reactors(Id, Id, String, bool),
    // report message: false the menu, true the submission
    Report(bool),
    // nothing is said of this one, whether it works or not: acks, previews, settings synced behind the scenes
    Quiet,
    // something the user did that can fail: what to say, and how to undo what the click showed
    Act(crate::ui::failed::Act),
    // a guild's soundboard sounds, for the call popout
    Soundboard(Id),
    // the activity shelf, launching one and the oauth steps its page asks for, by step
    Activity(String),
}

pub enum Cmd {
    Start(String),
    Stop,
    Auth(auth::Cmd),
    QrLogin(bool),
    Messages { channel: Id, page: Page },
    Send { channel: Id, content: String, nonce: String, reply: Option<(Id, bool)>, files: Vec<(PathBuf, String, String)>, stickers: Vec<Id>, tts: bool },
    Download { url: String, path: PathBuf },
    Interact(Value),
    Ack { channel: Id, id: Id },
    Typing { channel: Id },
    Gateway(Value),
    Rest { method: &'static str, path: String, body: Option<Value>, tag: Tag },
    Sticker { guild: Id, name: String, tags: String, description: String, file: PathBuf },
    Sudo { method: &'static str, path: String, body: Option<Value>, mfa: Option<String>, tag: Tag },
    /// A GET with an app's own access token instead of ours.
    Bearer { path: String, token: String, tag: Tag },
    Voice(voice::Cmd),
}

/// With RUSTCORD_READONLY set, nothing goes out that would change the account or
/// that other people could see: no messages, reactions, acks, typing, voice,
/// presence, or non-GET REST calls. Used when touring a real account.
pub static BLOCKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The channel on screen, so the gateway can tell which events can wait for the next frame.
pub static OPEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn allowed(c: &Cmd) -> bool {
    match c {
        Cmd::Send { .. } | Cmd::Interact(_) => false,
        Cmd::Ack { .. } | Cmd::Typing { .. } | Cmd::Voice(_) | Cmd::Sudo { .. } => false,
        Cmd::Rest { method, .. } => *method == "GET",
        Cmd::Sticker { .. } => false,
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

    /// For what's rarely on screen (presences, typing and messages elsewhere): taken in by the
    /// next frame, which comes a quarter second later at most.
    pub fn send_soon(&self, e: Ev) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint_after(std::time::Duration::from_millis(250));
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

struct Outgoing {
    channel: Id,
    content: String,
    nonce: String,
    reply: Option<(Id, bool)>,
    files: Vec<(PathBuf, String, String)>,
    stickers: Vec<Id>,
    tts: bool,
}

/// Messages go out one at a time, in order, like discord's queue. One that can't reach discord
/// turns red and is tried again a little later, the ones after it waiting their turn; one
/// discord refuses fails for good.
async fn sender(mut rx: mpsc::UnboundedReceiver<(Arc<Api>, Outgoing)>, tx: Tx) {
    while let Some((a, o)) = rx.recv().await {
        let mut wait = 2;
        loop {
            let Err(err) = a.send(o.channel, &o.content, &o.nonce, o.reply, &o.files, &o.stickers, o.tts).await else { break };
            let again = api::offline(&err) && wait <= 32;
            tx.send(Ev::SendFailed { channel: o.channel, nonce: o.nonce.clone(), err, again });
            if !again {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            wait *= 2;
        }
    }
}

async fn dispatch(mut rx: mpsc::UnboundedReceiver<Cmd>, tx: Tx) {
    let http = api::http();
    let mut api = Arc::new(Api::new(http.clone(), String::new()));
    let mut gw: Option<mpsc::UnboundedSender<Value>> = None;
    let mut voice = voice::Link::default();
    let (outbox, queue) = mpsc::unbounded_channel();
    tokio::spawn(sender(queue, tx.clone()));

    let mut qr: Option<tokio::task::JoinHandle<()>> = None;
    let readonly = std::env::var_os("RUSTCORD_READONLY").is_some();
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
            Cmd::Auth(c) => {
                tokio::spawn(auth::run(c, a, tx));
            }
            Cmd::QrLogin(on) => {
                if let Some(h) = qr.take() {
                    h.abort();
                }
                if on {
                    qr = Some(tokio::spawn(auth::qr_login(a, tx)));
                }
            }
            Cmd::Messages { channel, page } => {
                tokio::spawn(async move {
                    match a.messages(channel, page).await {
                        Ok(Value::Array(list)) => tx.send(Ev::Messages { channel, page, list }),
                        _ => tx.send(Ev::MessagesFailed { channel }),
                    }
                });
            }
            Cmd::Send { channel, content, nonce, reply, files, stickers, tts } => {
                let _ = outbox.send((a, Outgoing { channel, content, nonce, reply, files, stickers, tts }));
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
                        (Tag::Quiet, Err(_)) => {}
                        _ => tx.send(Ev::Rest { tag, res }),
                    }
                });
            }
            Cmd::Sticker { guild, name, tags, description, file } => {
                tokio::spawn(async move {
                    if let Err(e) = a.sticker(guild, [("name", name), ("tags", tags), ("description", description)], file).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::Sudo { method, path, body, mfa, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
                    let res = a.call_mfa(m, &path, body, mfa.as_deref()).await;
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Bearer { path, token, tag } => {
                tokio::spawn(async move {
                    let res = a.bearer(&path, &token).await;
                    tx.send(Ev::Rest { tag, res });
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
