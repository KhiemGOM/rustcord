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
    MessagesFailed { channel: Id, page: Page, err: String },
    // `again` when it'll be retried by itself
    SendFailed { channel: Id, nonce: String, err: String, again: bool },
    // an upload stopped from its card: the message that was waiting on it goes away
    SendCancelled { channel: Id, nonce: String },
    Auth(auth::Ev),
    Error(String),
    // the field errors and raw answer of the refusal that comes next
    Fail { req: String, fields: Vec<(String, String)>, raw: String },
    Toast(String),
    InteractFailed { nonce: String, status: u16, message: String },
    Rest { tag: Tag, res: Result<Value, String> },
    Voice(voice::Ev),
}

/// What a generic REST reply is for, so the UI knows where to put it.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    // the server behind the link in the Join a Server dialog
    InvitePreview(String),
    // an invite link in a message: what the card under it shows, and joining from it
    ChatInvite(String),
    ChatJoin(String),
    // the people who picked a poll answer: channel, message, answer
    PollVotes(Id, Id, u8),
    // where an emoji or a sticker (true) comes from
    Source(bool, Id),
    // a server's most used emoji, for the picker
    TopEmojis(Id),
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
    AddFriend(String),
    Commands(Id),
    Gifs(String),
    GifCats,
    // discord's own sticker packs
    StickerPacks,
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
    // a server's member applications: its list for a status, one person's earlier ones, an approve or reject
    Joins(Id, u8),
    JoinsFor(Id, Id),
    JoinsAct(Id),
    // the quests hub: 0 on offer, 1 claimed, 2 the Orbs balance, 3 an accept or claim went through
    Quests(u8),
    // a follow of an announcement channel, from its dialog
    Follow,
    /// settings pages park their data under a key, see `App::remote`
    Remote(&'static str),
    // a shop product, by sku, for the cosmetic someone wears
    Product(String),
    // a read parked under a key, see `App::keyed`
    Keyed(String),
    // the people who reacted to a message with an emoji: channel, message, emoji, whether super
    Reactors(Id, Id, String, bool),
    // report message: false the menu, true the submission
    Report(bool),
    Quiet,
    // the account's favourites and frecency lists, read or written
    Favs,
    Act(crate::ui::failed::Act),
    // a guild's soundboard sounds, for the call popout
    Soundboard(Id),
    // the activity shelf, launching one and the oauth steps its page asks for, by step
    Activity(String),
    AddApp(bool),
    // the purchase dialog's calls, and the server a listing or shop page is for
    Buy(&'static str, Id),
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
    Ack { channel: Id, id: Id, last_viewed: u64, flags: Option<u8> },
    Typing { channel: Id },
    Gateway(Value),
    Rest { method: &'static str, path: String, body: Option<Value>, tag: Tag },
    /// Channel settings the way discord saves them: the changed fields in a PATCH, then one PUT
    /// per overwrite that changed and one DELETE per overwrite that went, then the channel again.
    Channel { channel: Id, patch: Option<Value>, put: Vec<Value>, clear: Vec<Id>, tag: Tag },
    /// A moderation write whose reason rides in X-Audit-Log-Reason.
    Audit { method: &'static str, path: String, body: Option<Value>, reason: String, tag: Tag },
    /// A purchase: sent once, never retried, and only when `charge_barred` says nothing stands in the way.
    Charge { method: &'static str, path: String, body: Value, tag: Tag },
    Sticker { guild: Id, name: String, tags: String, description: String, file: PathBuf },
    /// A forum post with files on its first message.
    ForumPost { channel: Id, body: Value, files: Vec<(PathBuf, String, String)>, tag: Tag },
    Sudo { method: &'static str, path: String, body: Option<Value>, mfa: Option<String>, tag: Tag },
    /// A write that says where in the app it came from, in X-Context-Properties.
    Located { method: &'static str, path: String, body: Option<Value>, context: Value, tag: Tag },
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
        Cmd::Ack { .. } | Cmd::Typing { .. } | Cmd::Voice(_) | Cmd::Sudo { .. } | Cmd::Located { .. } => false,
        Cmd::Rest { method, path, .. } => *method == "GET" || path.ends_with("/members-search"),
        Cmd::Sticker { .. } | Cmd::ForumPost { .. } | Cmd::Audit { .. } | Cmd::Charge { .. } | Cmd::Channel { .. } => false,
        // member requests and lazy member-list subscriptions only read
        Cmd::Gateway(v) => matches!(v["op"].as_u64(), Some(8) | Some(14) | Some(37)),
        _ => true,
    }
}

/// Set by the e2e to prove a read-only run refuses a purchase before anything is sent.
pub static FORCE_READONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn loopback(base: &str) -> bool {
    reqwest::Url::parse(base).ok().and_then(|u| u.host_str().map(|h| matches!(h, "127.0.0.1" | "localhost" | "[::1]"))).unwrap_or(false)
}

fn barred(readonly: bool, harness: bool, base: &str) -> Option<&'static str> {
    if readonly {
        return Some("This run is read-only, so nothing was bought.");
    }
    if harness && !loopback(base) {
        return Some("A test run only buys from the mock, so nothing was bought.");
    }
    None
}

/// Why a purchase must not go out right now, if it must not: a read-only run, or a test or tour
/// run that isn't pointed at a server on this machine.
pub fn charge_barred() -> Option<&'static str> {
    let readonly = FORCE_READONLY.load(std::sync::atomic::Ordering::Relaxed) || std::env::var_os("RUSTCORD_READONLY").is_some();
    let harness = ["RUSTCORD_E2E", "RUSTCORD_TOUR", "RUSTCORD_BENCH"].iter().any(|k| std::env::var_os(k).is_some());
    barred(readonly, harness, api::base())
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

    /// For a video frame: one frame drawn for it. `request_repaint` draws two, the second with
    /// nothing new in it, which at a stream's rate is half of all the work on the main thread.
    pub fn send_frame(&self, e: Ev) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint_after(std::time::Duration::from_millis(1));
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
            if err == "cancelled" {
                tx.send(Ev::SendCancelled { channel: o.channel, nonce: o.nonce.clone() });
                break;
            }
            let again = api::offline(&err) && wait <= 32;
            tx.send(Ev::SendFailed { channel: o.channel, nonce: o.nonce.clone(), err, again });
            if !again {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            wait *= 2;
        }
        crate::upload::finish(&o.nonce);
    }
}

fn fail(tx: &Tx, m: &reqwest::Method, path: &str, f: crate::api::Failure) -> String {
    if !f.raw.is_empty() {
        tx.send(Ev::Fail { req: format!("{m} {path}"), fields: f.fields, raw: f.raw });
    }
    f.message
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
        if let Cmd::Charge { tag, .. } = &cmd
            && let Some(why) = charge_barred()
        {
            BLOCKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tx.send(Ev::Rest { tag: tag.clone(), res: Ok(serde_json::json!({ "status": 0, "body": { "message": why } })) });
            continue;
        }
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
                        Err(err) => tx.send(Ev::MessagesFailed { channel, page, err }),
                        Ok(_) => tx.send(Ev::MessagesFailed { channel, page, err: String::new() }),
                    }
                });
            }
            Cmd::Send { channel, content, nonce, reply, files, stickers, tts } => {
                if !files.is_empty() {
                    crate::upload::start(&nonce, &files);
                }
                let _ = outbox.send((a, Outgoing { channel, content, nonce, reply, files, stickers, tts }));
            }
            Cmd::Ack { channel, id, last_viewed, flags } => {
                tokio::spawn(async move {
                    let _ = a.ack(channel, id, last_viewed, flags).await;
                });
            }
            Cmd::Rest { method, path, body, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
                    let res = a.try_call(m.clone(), &path, body, None, None, None).await.map_err(|f| fail(&tx, &m, &path, f));
                    match (&tag, &res) {
                        (Tag::Quiet, Ok(_)) => {}
                        (Tag::Quiet, Err(_)) => {}
                        _ => tx.send(Ev::Rest { tag, res }),
                    }
                });
            }
            Cmd::Channel { channel, patch, put, clear, tag } => {
                tokio::spawn(async move {
                    use reqwest::Method;
                    let chan = format!("/channels/{channel}");
                    let mut writes: Vec<(Method, String, Option<Value>)> = Vec::new();
                    writes.extend(patch.map(|b| (Method::PATCH, chan.clone(), Some(b))));
                    writes.extend(put.into_iter().map(|o| (Method::PUT, format!("{chan}/permissions/{}", o["id"].as_str().unwrap_or_default()), Some(o))));
                    writes.extend(clear.into_iter().map(|id| (Method::DELETE, format!("{chan}/permissions/{id}"), None)));
                    let mut failed = None;
                    for (m, path, body) in writes {
                        if let Err(f) = a.try_call(m.clone(), &path, body, None, None, None).await {
                            failed.get_or_insert(fail(&tx, &m, &path, f));
                        }
                    }
                    let res = match failed {
                        Some(e) => Err(e),
                        None => a.try_call(Method::GET, &chan, None, None, None, None).await.map_err(|f| fail(&tx, &Method::GET, &chan, f)),
                    };
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Audit { method, path, body, reason, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
                    let res = a.try_call(m.clone(), &path, body, None, Some(&reason), None).await.map_err(|f| fail(&tx, &m, &path, f));
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Charge { method, path, body, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::POST);
                    let res = a.charge(m, &path, &body).await;
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Sticker { guild, name, tags, description, file } => {
                tokio::spawn(async move {
                    if let Err(e) = a.sticker(guild, [("name", name), ("tags", tags), ("description", description)], file).await {
                        tx.send(Ev::Error(e));
                    }
                });
            }
            Cmd::ForumPost { channel, body, files, tag } => {
                tokio::spawn(async move {
                    let res = a.forum_post(channel, body, &files).await;
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Sudo { method, path, body, mfa, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
                    let res = a.try_call(m.clone(), &path, body, mfa.as_deref(), None, None).await.map_err(|f| fail(&tx, &m, &path, f));
                    tx.send(Ev::Rest { tag, res });
                });
            }
            Cmd::Located { method, path, body, context, tag } => {
                tokio::spawn(async move {
                    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::POST);
                    let res = a.try_call(m.clone(), &path, body, None, None, Some(&context)).await.map_err(|f| fail(&tx, &m, &path, f));
                    match (&tag, &res) {
                        (Tag::Quiet, _) => {}
                        _ => tx.send(Ev::Rest { tag, res }),
                    }
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
                    let nonce = payload["nonce"].as_str().unwrap_or_default().to_string();
                    if let Err((status, message)) = a.interact(payload).await {
                        tx.send(Ev::InteractFailed { nonce, status, message });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_purchase_is_barred_when_read_only_or_when_a_test_run_isnt_on_this_machine() {
        assert!(barred(true, false, "http://127.0.0.1:1/api/v9").is_some());
        assert!(barred(true, true, "http://127.0.0.1:1/api/v9").is_some());
        assert!(barred(false, true, "https://discord.com/api/v9").is_some());
        assert!(barred(false, true, "http://discord.com.evil.test/api/v9").is_some());
        assert!(barred(false, true, "http://127.0.0.1:9/api/v9").is_none());
        assert!(barred(false, false, "https://discord.com/api/v9").is_none());
    }

    #[test]
    fn a_read_only_run_drops_a_charge_at_the_gate() {
        let c = Cmd::Charge { method: "POST", path: "/store/skus/1/purchase".into(), body: Value::Null, tag: Tag::Quiet };
        assert!(!allowed(&c));
    }
}
