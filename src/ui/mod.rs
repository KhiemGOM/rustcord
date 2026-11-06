mod accounts;
mod actions;
mod attach;
mod active;
pub mod activity;
mod autoscroll;
mod boosts;
mod call;
mod callbar;
mod callfx;
mod chat;
mod clips;
mod clips_gallery;
mod clips_trim;
mod composer;
mod create;
mod cstatus;
mod crop;
mod directory;
mod dmpick;
mod discover;
mod e2e;
mod events;
mod folders;
mod follow;
mod forum;
mod gate;
mod guildmodal;
mod friends;
pub mod icons;
mod keys;
mod lurk;
mod links;
mod login;
mod mdpreview;
mod members;
mod modview;
mod now;
mod motion;
mod palette;
mod newserver;
mod menus;
mod picker;
mod player;
mod plus;
mod poll;
mod invitecard;
mod popups;
mod presence;
mod reactions;
mod report;
mod system;
mod profile;
mod quests;
pub mod shop;
mod rail;
mod reorder;
mod channel;
mod community;
mod server;
mod settings;
mod share;
mod soundboard;
mod stage;
mod tile;
mod side;
mod tagpop;
mod textfile;
mod thread;
mod spelling;
mod when;
pub mod text;
mod titlebar;
mod topic;
mod wheel;
mod userdlg;
mod usermenu;

use egui::{Color32, CornerRadius, Frame, Pos2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::auth;
use crate::cache::{self, Settings, UiState};
use crate::net::{Cmd, Conn, Ev, Net, Tag};
use crate::state::{Id, Map, Message, Page, ReactEmoji, Set, Status, Store, Tracked};
use crate::theme::*;

/// Discord's zoom levels, in percent: the accessibility slider's ticks, which ctrl+wheel and
/// ctrl +/- step through.
pub const OPTED: u64 = 4096;
pub const PINNED: u64 = 2048;

pub const ZOOMS: [f32; 11] = [50.0, 67.0, 75.0, 80.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0];

pub enum Pick {
    Composer,
    React(Id, Id),
    // a slot of the quick reactions
    Quick(usize),
    // the emoji of an answer in the poll being made
    Poll(usize),
}

pub struct App {
    pub net: Net,
    rx: Receiver<Ev>,
    pub store: Tracked,
    pub ui: UiState,
    pub token: Option<String>,
    // the other accounts signed in on this machine, for Switch Accounts
    pub accounts: Vec<cache::SavedAccount>,
    pub conn: Conn,
    pub login: login::Form,
    pub drafts: Map<Id, String>,
    pub files: Vec<composer::Upload>,
    pub ac_sel: usize,
    pub ac_open: bool,
    pub ac_dismissed: Option<String>,
    pub aliases: Map<Id, Vec<(String, String)>>,
    pub commands: Map<Id, composer::Commands>,
    v_down: bool,
    fonts_rx: Option<Receiver<egui::FontDefinitions>>,
    cjk_loaded: bool,
    pub reply: Option<Id>,
    pub editing: Option<(Id, String)>,
    pub last_typing: Option<(Id, Instant)>,
    pub heights: Map<(Id, bool), f32>,
    pub heights_w: f32,
    pub offsets: Map<Id, f32>,
    // channels whose message list sat at the bottom last frame
    pub pinned: Set<Id>,
    pub scroll_fix: Option<(Id, f32)>,
    pub revealed: Set<Id>,
    pub picker: Option<(Pick, Pos2)>,
    // the sticker picker, anchored where it opened
    pub stickers: Option<Pos2>,
    pub menu: Option<(Id, Id, Pos2)>,
    // a person's right-click menu: user, server, the dm it came from, where
    pub user_menu: Option<(Id, Id, Option<Id>, Pos2)>,
    // where it opened: the call and friends lists add rows
    pub menu_from: usermenu::Origin,
    // the note, nickname or group dialog from that menu
    pub user_dialog: Option<userdlg::Dialog>,
    pub confirm: Option<Confirm>,
    pub forward: Option<popups::Forward>,
    pub reactors: Option<reactions::Reactors>,
    pub report: Option<report::Report>,
    // the server guide's new member actions, per server: (their channels, the ones done)
    pub guides: Map<Id, (Vec<Id>, Set<Id>)>,
    pub guild_notifs: Option<Id>,
    pub bar: Option<(Id, Rect)>,
    pub toast: Option<(String, Instant)>,
    // an error toast's full text, which shift+click opens
    pub toast_detail: Option<String>,
    pub marker: Option<(Id, Id)>,
    // whether the "new messages since" bar is still up; scrolling down to the newest takes it away
    pub marker_bar: bool,
    pub autoscroll: Option<autoscroll::Auto>,
    pub wheel: wheel::Wheel,
    // ctrl+wheel travel not yet worth a zoom step
    zoom_acc: f32,
    pub focus_composer: bool,
    pub rects: Map<String, Rect>,
    pub settings: Settings,
    pub settings_page: Option<settings::Page>,
    pub invite: Option<String>,
    pub invite_card: invitecard::Card,
    pub folders: folders::State,
    pub wizard: Option<newserver::Wizard>,
    pub lurk_goto: Option<(Id, Id)>,
    pub guild_modal: Option<guildmodal::Modal>,
    // "invite people" from a channel row: the channel, and the code once discord made one
    pub made_invite: Option<popups::MadeInvite>,
    // a voice channel's status being written, from "Set a channel status"
    pub status_edit: Option<(Id, String)>,
    // go live's picker of windows, screens and devices
    pub share: Option<share::Picker>,
    // the soundboard popout in a voice call: which guild's sounds, and them once fetched
    pub soundboard: Option<(Id, Option<Vec<crate::state::Sound>>)>,
    pub switcher: Option<String>,
    pub profile: Option<(Id, Id, Pos2)>,
    pub profiles: Map<Id, serde_json::Value>,
    pub full_profile: Option<profile::Full>,
    pub outbox: Map<Id, Vec<profile::Recent>>,
    // the + button's role list: server, member, where, search
    pub role_add: Option<(Id, Id, Pos2, String)>,
    // a server tag's popout, a role mention's, and what the first one fetched
    pub tag_pop: Option<(Id, Pos2)>,
    pub role_pop: Option<(Id, Id, Pos2)>,
    pub guild_profiles: Map<Id, serde_json::Value>,
    pub mod_view: Option<modview::State>,
    pub pins: Option<(Id, Option<Vec<Message>>)>,
    pub search: Option<popups::Search>,
    pub friends: bool,
    // the server whose boosts page is open, and what each page has loaded
    pub boosts: Option<Id>,
    pub boost_info: Map<Id, boosts::Info>,
    // the server menu's modals and pages: create, active threads, the app directory, settings
    pub create: Option<create::Form>,
    pub active_threads: Option<Id>,
    pub directory: Option<directory::Dir>,
    pub spell: spelling::State,
    pub server: Option<server::State>,
    pub chan_settings: Option<channel::State>,
    // Members or Channels & Roles, open in place of a channel
    pub home: Option<community::Home>,
    // a voice channel's chat open beside the call
    pub voice_chat: bool,
    pub call: callbar::CallUi,
    // a thread open beside its parent channel
    pub side_thread: Option<Id>,
    // a thread whose first messages are being fetched for Jump To Top
    pub jump_top: Option<Id>,
    pub activity: activity::Activity,
    pub events: Option<events::Events>,
    pub quests: Option<quests::Quests>,
    pub shop: Option<shop::Shop>,
    pub friends_tab: u8,
    pub add_friend: String,
    pub post: Option<(String, String)>,
    pub scroll_to: Option<(Id, Id)>,
    pub scroll_tries: u8,
    pub flash: Option<(Id, Instant)>,
    pub viewer: Option<(String, String, u32, u32)>,
    pub media_fs: Option<String>,
    // the open message's pictures, which the viewer's arrows step through
    pub gallery: Vec<(String, String, u32, u32)>,
    pub remote: Map<&'static str, Option<Result<serde_json::Value, String>>>,
    hotkey_rx: Option<Receiver<(String, bool)>>,
    pub clips: clips::State,
    obs_seen: bool,
    capture_hidden: bool,
    pub sform: settings::Forms,
    // a channel the user just marked unread shouldn't get re-acked on the next frame
    pub hold_ack: Option<Id>,
    pub reply_ping: bool,
    pub open_threads: bool,
    pub unsent: Map<String, actions::Unsent>,
    // the message keyboard focus sits on (shift+tab out of the composer), and whether it just
    // moved and wants scrolling into view
    pub focus_msg: Option<Id>,
    pub focus_scroll: bool,
    // put the composer's caret at the end of its text, after something was added to it
    pub caret_end: bool,
    // messages bookmarked for later, by id
    pub bookmarks: Set<Id>,
    // the notes you keep on people, by user, once fetched
    pub notes: Map<Id, String>,
    pub new_thread: Option<popups::NewThread>,
    pub poll: Option<poll::Draft>,
    pub launcher: Option<plus::Launcher>,
    pub follow: Option<follow::Follow>,
    pub modify: Option<attach::Modify>,
    pub group_pick: Option<dmpick::Pick>,
    pub topic: Option<Id>,
    // a DM whose call was started with video: the camera goes on once it's connected
    pub cam_on_join: Option<Id>,
    // what Discord told us alone about a send it refused: the channel and the words
    pub notice: Option<(Id, String)>,
    // the Edit Image dialog over a picture about to be uploaded
    pub crop: Option<crop::Crop>,
    // Set a custom status, over the account card
    pub status_text: Option<cstatus::State>,
    // what /sticker was given, for the sticker picker to start its search with
    pub sticker_q: Option<String>,
    // a custom emoji's popout: id, name, animated, where
    pub emoji_pop: Option<(Id, String, bool, Pos2)>,
    // a link waiting on "Leaving Discord", and whether to trust its site from now on
    pub leaving: Option<(String, bool)>,
    // the Ctrl+/ sheet, the Raging Demon it can set off, and breathing with Wumpus
    pub combos: Option<keys::Combos>,
    pub demon: Option<Instant>,
    pub breathe: Option<keys::Breathe>,
    // Timeout for a member: who, in which server, the length picked and the reason
    pub timeout: Option<popups::Timeout>,
    // the server left for the dms, which Ctrl+Alt+→ goes back to
    pub prev_guild: Option<Id>,
    // the open channel when it's scrolled well back from the newest messages
    pub far_up: Option<Id>,
    // quick clicks on the home button so far, and when the last was; the ディスコード tooltip
    pub home_clicks: (u32, Option<Instant>),
    pub discodo_at: Option<Instant>,
    pub gifs: Option<popups::Gifs>,
    pub nav: titlebar::Nav,
    pub inbox: Option<titlebar::Inbox>,
    pub(crate) title_count: Option<u32>,
    pub voice: crate::voice::VoiceUi,
    e2e: Option<e2e::Run>,
    win: Option<[f32; 2]>,
    frames: u64,
    save_at: Option<Instant>,
    // toasts clicked, as (channel, message); none under e2e, which never shows any
    pings: Option<std::sync::mpsc::Receiver<crate::toast::Click>>,
    games: Option<std::sync::mpsc::Receiver<Option<crate::games::Game>>>,
    // the program we spotted running, and what the account card shows: that or a game's rich presence
    detected: Option<crate::games::Game>,
    pub game: Option<crate::games::Game>,
    links: links::State,
    // the game and the rich presence changes the gateway has from us; identify sends none
    shared: (Option<Id>, u64),
    // servers we've asked for their voice channels' statuses and start times this session
    info_asked: Set<Id>,
    // the taskbar badge's count, and when unreads were last looked over for its dot
    badge: Option<i32>,
    badge_at: Option<Instant>,
    // when the activity feed is next fetched
    feed_at: Option<Instant>,
    synced: Option<crate::proto::Account>,
    // images drawn lately, so the rest can be let go now and then
    textures: std::sync::Arc<crate::imgcache::Textures>,
    sweep_at: Instant,
    system: crate::winsys::Prefs,
    system_at: f64,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        t0: Instant,
        snap: std::thread::JoinHandle<Option<(Store, UiState)>>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        // egui's own ctrl +/- zoom goes by 10%; ours steps through discord's levels
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        crate::theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let textures = std::sync::Arc::new(crate::imgcache::Textures::default());
        ctx.add_texture_loader(textures.clone());
        ctx.add_image_loader(std::sync::Arc::new(crate::gif::GifLoader::default()));
        // added last so it's asked first: no other loader knows its scheme
        ctx.add_image_loader(std::sync::Arc::new(crate::emoji_font::Loader::default()));
        let (net, rx) = Net::new(ctx.clone());
        crate::fonts::fetch(&net.rt, ctx.clone());
        crate::sound::fetch(&net.rt);
        ctx.add_bytes_loader(std::sync::Arc::new(crate::imgcache::DiskLoader::new(
            cache::dir().join("img"),
            net.rt.clone(),
        )));
        let token = cache::token();
        let (store, ui) = match (&token, snap.join().ok().flatten()) {
            (Some(_), Some(s)) => s,
            _ => Default::default(),
        };
        if let Some(t) = &token {
            net.send(Cmd::Start(t.clone()));
        }
        let mut app = App {
            net,
            rx,
            store: store.into(),
            ui,
            token,
            accounts: cache::accounts(),
            conn: Conn::Connecting,
            login: Default::default(),
            drafts: Map::default(),
            files: Vec::new(),
            ac_sel: 0,
            ac_open: false,
            ac_dismissed: None,
            aliases: Map::default(),
            commands: Map::default(),
            v_down: false,
            fonts_rx: None,
            cjk_loaded: false,
            reply: None,
            editing: None,
            last_typing: None,
            heights: Map::default(),
            heights_w: 0.0,
            offsets: Map::default(),
            pinned: Set::default(),
            scroll_fix: None,
            revealed: Set::default(),
            picker: None,
            stickers: None,
            menu: None,
            confirm: None,
            forward: None,
            reactors: None,
            report: None,
            guides: Map::default(),
            user_menu: None,
            menu_from: Default::default(),
            user_dialog: None,
            guild_notifs: None,
            bar: None,
            toast: None,
            toast_detail: None,
            marker: None,
            marker_bar: false,
            autoscroll: None,
            wheel: wheel::Wheel::new(),
            zoom_acc: 0.0,
            focus_composer: true,
            rects: Map::default(),
            voice: Default::default(),
            e2e: e2e::Run::from_env(t0),
            win: None,
            settings: cache::settings(),
            settings_page: None,
            invite: None,
            invite_card: Default::default(),
            folders: Default::default(),
            wizard: None,
            lurk_goto: None,
            guild_modal: None,
            made_invite: None,
            status_edit: None,
            share: None,
            soundboard: None,
            switcher: None,
            profile: None,
            profiles: Map::default(),
            full_profile: None,
            outbox: Map::default(),
            role_add: None,
            tag_pop: None,
            role_pop: None,
            guild_profiles: Map::default(),
            mod_view: None,
            pins: None,
            search: None,
            friends: false,
            boosts: None,
            boost_info: Map::default(),
            create: None,
            active_threads: None,
            directory: None,
            spell: Default::default(),
            server: None,
            chan_settings: None,
            home: None,
            voice_chat: false,
            call: Default::default(),
            side_thread: None,
            jump_top: None,
            activity: Default::default(),
            events: None,
            quests: None,
            shop: None,
            friends_tab: 0,
            add_friend: String::new(),
            post: None,
            scroll_to: None,
            scroll_tries: 0,
            flash: None,
            viewer: None,
            media_fs: None,
            gallery: Vec::new(),
            remote: Map::default(),
            hotkey_rx: None,
            clips: Default::default(),
            obs_seen: false,
            capture_hidden: false,
            sform: Default::default(),
            hold_ack: None,
            reply_ping: true,
            open_threads: false,
            unsent: Default::default(),
            focus_msg: None,
            focus_scroll: false,
            caret_end: false,
            bookmarks: Set::default(),
            notes: Map::default(),
            new_thread: None,
            poll: None,
            launcher: None,
            follow: None,
            modify: None,
            group_pick: None,
            topic: None,
            cam_on_join: None,
            notice: None,
            crop: None,
            status_text: None,
            sticker_q: None,
            emoji_pop: None,
            leaving: None,
            combos: None,
            demon: None,
            breathe: None,
            timeout: None,
            prev_guild: None,
            far_up: None,
            home_clicks: (0, None),
            discodo_at: None,
            gifs: None,
            nav: Default::default(),
            inbox: None,
            title_count: None,
            frames: 0,
            save_at: None,
            pings: None,
            games: None,
            detected: None,
            game: None,
            links: Default::default(),
            shared: (None, 0),
            info_asked: Default::default(),
            badge: None,
            badge_at: None,
            feed_at: None,
            synced: None,
            textures,
            sweep_at: Instant::now(),
            system: crate::winsys::preferences(),
            system_at: 0.0,
        };
        app.apply_settings(&ctx);
        let (htx, hrx) = std::sync::mpsc::channel();
        crate::tray::start(ctx.clone(), htx.clone());
        crate::hotkeys::start(ctx.clone(), htx);
        app.hotkey_rx = Some(hrx);
        crate::winsys::watch_streaming(ctx.clone());
        if app.e2e.is_none() {
            app.pings = Some(crate::toast::start(ctx.clone()));
        }
        app.games = Some(crate::games::start(ctx.clone()));
        app.links.rx = Some(crate::rpc::start(ctx.clone()));
        app.obs_seen = crate::winsys::OBS_RUNNING.load(std::sync::atomic::Ordering::Relaxed);
        if app.settings.start_minimized && std::env::args().any(|a| a == "--startup") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if let Some(ch) = app.cur() {
            app.open(ch);
        }
        app
    }

    pub fn cur(&self) -> Option<Id> {
        let ch = *self.ui.chan.get(&self.ui.guild.unwrap_or(0))?;
        self.store.channels.contains_key(&ch).then_some(ch)
    }

    pub fn select_guild(&mut self, g: Option<Id>) {
        if g.is_none() && self.ui.guild.is_some() {
            self.prev_guild = self.ui.guild;
        }
        self.ui.guild = g;
        self.boosts = None;
        self.home = None;
        self.directory = None;
        let ch = match self.cur() {
            Some(c) => Some(c),
            None => match g {
                Some(g) => side::first_channel(&self.store, g),
                None => self.store.dms.first().copied(),
            },
        };
        if let Some(c) = ch {
            self.open(c);
        }
    }

    pub fn open(&mut self, ch: Id) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        self.boosts = None;
        self.home = None;
        let guild = c.guild;
        let forum = c.is_forum();
        let is_thread = c.is_thread();
        // member list subscriptions are per parent channel, threads borrow it
        let sub = if c.is_thread() { c.parent } else { ch };
        // search results belong to one server (or one dm), don't carry them elsewhere
        if self.search.as_ref().is_some_and(|s| s.scope != if guild != 0 { guild } else { ch }) {
            self.search = None;
        }
        if self.cur() != Some(ch) {
            self.reply = None;
            self.editing = None;
            self.files.clear();
            self.pins = None;
            self.post = None;
            self.hold_ack = None;
        }
        self.friends = false;
        self.quests = None;
        self.shop = None;
        if is_thread {
            self.side_thread = None;
        }
        self.nav.visit(titlebar::Loc::Chan(ch, None));
        self.ui.chan.insert(guild, ch);
        crate::net::OPEN.store(ch, std::sync::atomic::Ordering::Relaxed);
        self.ui.recent.retain(|x| *x != ch);
        self.ui.recent.insert(0, ch);
        self.ui.recent.truncate(15);
        self.focus_composer = true;
        let acked = self.store.reads.get(&ch).map(|r| r.last_acked).unwrap_or(0);
        self.marker = (c.last_msg > acked && acked != 0).then_some((ch, acked));
        self.marker_bar = self.marker.is_some();
        if forum {
            let path = format!("/channels/{ch}/threads/search?sort_by=last_message_time&sort_order=desc&limit=25&offset=0");
            self.rest("GET", path, None, Tag::Forum(ch));
        } else {
            let jumping = self.scroll_to.is_some_and(|(c, _)| c == ch);
            let e = self.store.msgs.entry(ch).or_default();
            if !e.fresh && !e.loading && !jumping {
                e.loading = true;
                self.net.send(Cmd::Messages { channel: ch, page: Page::Latest });
            }
        }
        if guild != 0 {
            let mut chans = serde_json::Map::new();
            chans.insert(sub.to_string(), json!([[0, 99]]));
            self.net.send(Cmd::Gateway(json!({
                "op": 37,
                "d": { "subscriptions": { guild.to_string(): {
                    "typing": true, "activities": true, "threads": true, "channels": chans
                }}}
            })));
            if is_thread {
                self.subscribe_thread(guild, ch);
            }
            if self.info_asked.insert(guild) {
                self.net.send(Cmd::Gateway(json!({ "op": 43, "d": { "guild_id": guild.to_string(), "fields": ["status", "voice_start_time"] } })));
            }
        }
    }

    // history from REST has no member objects; ask the gateway for the authors we haven't seen,
    // and whoever they mention, bots' embeds included, so mentions show nicknames like discord's
    fn request_members(&mut self, ch: Id) {
        let Some(guild) = self.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0) else { return };
        let known = self.store.members.get(&guild);
        let mut ids: Vec<String> = Vec::new();
        let mut want = |id: Id| {
            let s = id.to_string();
            if id != 0 && !known.is_some_and(|k| k.contains_key(&id)) && !ids.contains(&s) {
                ids.push(s);
            }
        };
        for m in self.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default() {
            want(m.author);
            m.mentions.iter().for_each(|u| want(*u));
            let texts = m.embeds.iter().flat_map(|e| e.desc.iter().chain(e.fields.iter().map(|f| &f.1)));
            for t in std::iter::once(&m.content).chain(texts) {
                for part in t.split("<@").skip(1) {
                    if let Some(id) = part.split('>').next().map(|x| x.trim_start_matches('!')).and_then(|x| x.parse().ok()) {
                        want(id);
                    }
                }
            }
        }
        if ids.is_empty() {
            return;
        }
        ids.truncate(100);
        self.net.send(Cmd::Gateway(json!({
            "op": 8,
            "d": { "guild_id": [guild.to_string()], "user_ids": ids, "presences": false }
        })));
    }

    /// The size a panel was dragged to, if it was; only counts while a button is held, so a
    /// shrinking window doesn't overwrite what the user picked, and an untouched default isn't saved.
    pub fn dragged(&mut self, ctx: &egui::Context, now: f32, was: f32) -> Option<f32> {
        if (now - was).abs() < 0.5 || !ctx.input(|i| i.pointer.any_down()) {
            return None;
        }
        self.save_at = Some(Instant::now() + Duration::from_secs(1));
        Some(now)
    }

    /// Open a channel scrolled to one message, fetching the history around it if needed.
    pub fn jump_to(&mut self, ch: Id, id: Id) {
        self.scroll_to = Some((ch, id));
        self.scroll_tries = 0;
        let moved = self.cur() != Some(ch);
        if moved {
            let g = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
            self.ui.guild = (g != 0).then_some(g);
            self.open(ch);
        }
        self.nav.jumped(ch, id, moved);
        let e = self.store.msgs.entry(ch).or_default();
        if !e.list.iter().any(|m| m.id == id) {
            e.loading = true;
            self.net.send(Cmd::Messages { channel: ch, page: Page::Around(id) });
        }
    }

    pub fn jump_to_present(&mut self, ch: Id) {
        let e = self.store.msgs.entry(ch).or_default();
        if e.detached {
            e.loading = true;
            self.net.send(Cmd::Messages { channel: ch, page: Page::Latest });
        }
        self.scroll_fix = Some((ch, f32::MAX));
    }

    pub fn send_message(&mut self, ch: Id) {
        self.post(ch, false);
    }

    /// The draft and uploads out as a message; `tts` reads it aloud to everyone looking.
    pub fn post(&mut self, ch: Id, tts: bool) {
        let text = self.drafts.remove(&ch).unwrap_or_default();
        let mut text = self.resolve_aliases(ch, text.trim());
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        text = self.convert_emoji(guild, &text);
        if self.settings.emoticons {
            text = emoticons(&text);
        }
        self.aliases.remove(&ch);
        let files: Vec<(PathBuf, String, String)> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|u| {
                let name = if u.spoiler { format!("SPOILER_{}", u.name) } else { u.name };
                (u.path, name, u.alt)
            })
            .collect();
        if text.is_empty() && files.is_empty() {
            return;
        }
        let nonce = crate::state::make_nonce();
        let reply = self.reply.take();
        self.store.add_pending(ch, text.clone(), nonce.clone(), reply);
        self.jump_to_present(ch);
        let reply = reply.map(|r| (r, self.reply_ping));
        self.remember_send(&nonce, actions::Unsent { content: text.clone(), reply, files: files.clone(), tts });
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply, files, stickers: Vec::new(), tts });
        self.hold_ack = None;
        self.last_typing = None;
    }

    /// Text that didn't come from the composer, so its draft, uploads and reply stay put.
    pub fn send_text(&mut self, ch: Id, text: String) {
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        let mut text = self.convert_emoji(guild, text.trim());
        if self.settings.emoticons {
            text = emoticons(&text);
        }
        let nonce = crate::state::make_nonce();
        self.store.add_pending(ch, text.clone(), nonce.clone(), None);
        self.jump_to_present(ch);
        self.remember_send(&nonce, actions::Unsent { content: text.clone(), reply: None, files: Vec::new(), tts: false });
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply: None, files: Vec::new(), stickers: Vec::new(), tts: false });
    }

    /// The four one-click reactions on top of the message menu, the toolbar taking the first three:
    /// the ones picked by hand, else the latest used, topped up with discord's defaults.
    pub fn quick_reactions(&self) -> Vec<ReactEmoji> {
        let mut list = if self.settings.quick_reactions.is_empty() { self.ui.quick.clone() } else { self.settings.quick_reactions.clone() };
        for n in ["✅", "❤️", "🔥", "😂"] {
            if list.len() < 4 && !list.iter().any(|q| q.id == 0 && q.name == n) {
                list.push(ReactEmoji { id: 0, name: n.to_string(), animated: false });
            }
        }
        list.truncate(4);
        list
    }

    /// Puts an emoji in one of the quick reaction slots; one already there trades places.
    pub fn set_quick(&mut self, slot: usize, em: ReactEmoji, ctx: &egui::Context) {
        let mut list = self.quick_reactions();
        if slot >= list.len() {
            return;
        }
        match list.iter().position(|q| *q == em) {
            Some(i) => list.swap(slot, i),
            None => list[slot] = em,
        }
        self.settings.quick_reactions = list;
        self.save_settings(ctx);
    }

    /// The next zoom level up or down from where it is now.
    fn step_zoom(&mut self, ctx: &egui::Context, up: bool) {
        let now = ctx.zoom_factor() * 100.0;
        let next = if up { ZOOMS.iter().find(|z| **z > now + 0.5) } else { ZOOMS.iter().rev().find(|z| **z < now - 0.5) };
        if let Some(z) = next {
            ctx.set_zoom_factor(z / 100.0);
            self.toast(format!("Zoom {z}%"));
        }
    }

    pub fn react(&mut self, ch: Id, msg: Id, em: ReactEmoji, add: bool) {
        if add {
            self.ui.quick.retain(|q| *q != em);
            self.ui.quick.insert(0, em.clone());
            self.ui.quick.truncate(3);
            self.used(em.clone());
        }
        self.net.send(Cmd::React { channel: ch, id: msg, emoji: em.api_name(), add });
        self.store.react_local(ch, msg, em, add);
    }

    /// An emoji went out, as a reaction or in a message: it moves up the picker's frequently used.
    pub fn used(&mut self, em: ReactEmoji) {
        self.ui.frequent.retain(|q| *q != em);
        self.ui.frequent.insert(0, em);
        self.ui.frequent.truncate(27);
    }

    pub fn rest(&self, method: &'static str, path: String, body: Option<serde_json::Value>, tag: Tag) {
        self.net.send(Cmd::Rest { method, path, body, tag });
    }

    pub fn mark_read(&mut self, chans: &[Id]) {
        let mut states = Vec::new();
        for ch in chans {
            let Some(last) = self.store.channels.get(ch).map(|c| c.last_msg).filter(|l| *l != 0) else { continue };
            let r = self.store.reads.entry(*ch).or_default();
            if r.last_acked < last || r.mentions > 0 {
                r.last_acked = last;
                r.mentions = 0;
                states.push(json!({ "channel_id": ch.to_string(), "message_id": last.to_string(), "read_state_type": 0 }));
            }
        }
        if !states.is_empty() {
            self.rest("POST", "/read-states/ack-bulk".into(), Some(json!({ "read_states": states })), Tag::Quiet);
        }
    }

    pub fn set_muted(&mut self, id: Id, guild: bool, muted: bool) {
        self.mute_for(id, guild, muted, None);
    }

    /// Muting with discord's mute_config: a window in seconds, or None for until turned back on.
    pub fn mute_for(&mut self, id: Id, guild: bool, muted: bool, secs: Option<i64>) {
        let cfg = match secs {
            Some(s) if muted => json!({ "selected_time_window": s, "end_time": (chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }),
            _ => json!({ "selected_time_window": -1, "end_time": null }),
        };
        if muted {
            self.store.muted.insert(id);
        } else {
            self.store.muted.remove(&id);
        }
        if guild {
            self.rest("PATCH", format!("/users/@me/guilds/{id}/settings"), Some(json!({ "muted": muted, "mute_config": cfg })), Tag::Quiet);
        } else {
            let g = self.store.channels.get(&id).map(|c| c.guild).unwrap_or(0);
            let path = if g == 0 { "@me".to_string() } else { g.to_string() };
            let body = json!({ "channel_overrides": { id.to_string(): { "muted": muted, "mute_config": cfg } } });
            self.rest("PATCH", format!("/users/@me/guilds/{path}/settings"), Some(body), Tag::Quiet);
        }
    }

    /// One bit of a channel override's flags, the rest kept: 4096 on the list of a server that
    /// only lists picked channels, 2048 pinned to the top, 1024 and 512 the unread setting.
    pub fn chan_flag(&mut self, ch: Id, bit: u64, on: bool) {
        let Some(g) = self.store.channels.get(&ch).map(|c| c.guild) else { return };
        let flags = self.store.chan_flags.entry(ch).or_default();
        if on {
            *flags |= bit;
        } else {
            *flags &= !bit;
        }
        let flags = *flags;
        if bit == OPTED {
            if on {
                self.store.opted.insert(ch);
            } else {
                self.store.opted.remove(&ch);
            }
        }
        let body = json!({ "channel_overrides": { ch.to_string(): { "flags": flags } } });
        self.rest("PATCH", format!("/users/@me/guilds/{g}/settings"), Some(body), Tag::Quiet);
    }

    /// Edit Per-server Profile: Settings > Profiles on that server.
    pub fn edit_server_profile(&mut self, gid: Id) {
        self.sform.profile_guild = Some(gid);
        self.sform.profile = None;
        self.sform.profile_base = None;
        self.settings_page = Some(settings::Page::Profiles);
    }

    pub fn opt_in(&mut self, ch: Id) {
        self.chan_flag(ch, OPTED, true);
    }

    /// A channel's notification level, as Discord's bell menu sets it; None goes back to the
    /// category or server default.
    pub fn set_level(&mut self, ch: Id, level: Option<u8>) {
        if self.store.guilds.contains_key(&ch) {
            if let Some(n) = level {
                self.store.levels.insert(ch, n);
            }
            self.rest("PATCH", format!("/users/@me/guilds/{ch}/settings"), Some(json!({ "message_notifications": level.unwrap_or(3) })), Tag::Quiet);
            return;
        }
        match level {
            Some(n) => self.store.levels.insert(ch, n),
            None => self.store.levels.remove(&ch),
        };
        let g = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
        let body = json!({ "channel_overrides": { ch.to_string(): { "message_notifications": level.unwrap_or(3) } } });
        self.rest("PATCH", format!("/users/@me/guilds/{g}/settings"), Some(body), Tag::Quiet);
    }

    pub fn mark_unread(&mut self, ch: Id, id: Id) {
        // ack the message before this one so it shows as the first unread
        let prev = self.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.id < id)).map(|m| m.id).unwrap_or(id - 1);
        self.store.reads.entry(ch).or_default().last_acked = prev;
        self.rest("POST", format!("/channels/{ch}/messages/{prev}/ack"), Some(json!({ "manual": true, "token": null })), Tag::Quiet);
        self.marker = Some((ch, prev));
        self.marker_bar = true;
        self.hold_ack = Some(ch);
    }

    pub fn set_status(&mut self, s: &str) {
        let me = self.store.me;
        self.store.presence.insert(me, Status::parse(s));
        self.send_presence();
    }

    /// Our status, with the game we detected unless activity sharing is off.
    fn send_presence(&mut self) {
        let status = match self.store.presence.get(&self.store.me) {
            Some(Status::Idle) => "idle",
            Some(Status::Dnd) => "dnd",
            Some(Status::Offline) => "invisible",
            _ => "online",
        };
        self.shared = self.presence_key();
        let activities = if self.store.hide_game { json!([]) } else { json!(self.activities()) };
        self.net.send(Cmd::Gateway(json!({ "op": 3, "d": { "status": status, "since": 0, "activities": activities, "afk": false } })));
    }

    pub fn save_settings(&self, ctx: &egui::Context) {
        self.apply_settings(ctx);
        cache::set_settings(&self.settings);
    }

    /// The account's values win at login and whenever one of them changes; a
    /// value the account still agrees on with last time leaves a local edit alone.
    fn sync_account(&mut self, ctx: &egui::Context) {
        let now = self.store.account.clone();
        let changed = now.changed(self.synced.as_ref());
        self.synced = Some(now);
        // sync with computer is a local choice the account doesn't know about
        let theme = self.settings.theme;
        changed.apply(&mut self.settings);
        if theme == crate::theme::Theme::Sync || !self.settings.sync_theme {
            self.settings.theme = theme;
        }
        self.heights.clear();
        self.save_settings(ctx);
    }

    /// Push every setting that lives outside `App` (theme, zoom, sounds, hotkeys...) to where it acts.
    fn apply_settings(&self, ctx: &egui::Context) {
        let s = &self.settings;
        ctx.set_zoom_factor(s.zoom);
        crate::tray::set_close_hides(s.minimize_to_tray && std::env::var_os("RUSTCORD_BENCH").is_none());
        crate::media::set_volume(s.media_volume, s.media_muted);
        crate::markdown::CLOCK24.store(s.clock24(), std::sync::atomic::Ordering::Relaxed);
        crate::markdown::set_date_order(&s.locale);
        crate::theme::set_reduced_motion(self.reduced_motion());
        crate::theme::set_view(crate::theme::View {
            underline_links: s.underline_links,
            animate_emoji: s.animate_emoji,
            hide_invites: s.streamer && s.hide_invites,
            name_styles: s.name_styles,
            own_profiles: s.own_theme_on_profiles,
            switch_icons: s.switch_icons,
            density: s.density,
        });
        self.apply_look(ctx);
        crate::speech::set_speed(s.tts_rate);
        crate::toast::set_long(s.min_toast_secs > 5.0);
        crate::theme::style(ctx);
        crate::sound::configure(s);
        self.sync_hotkeys();
        crate::clips::configure(s.clips, s.clips_secs);
        crate::api::set_locale(&s.locale);
        self.voice.configure(s);
    }

    fn reduced_motion(&self) -> bool {
        if self.settings.sync_motion { !self.system.animations } else { self.settings.reduced_motion }
    }

    fn apply_look(&self, ctx: &egui::Context) {
        let s = &self.settings;
        crate::theme::set_look(
            ctx,
            &crate::theme::Look {
                theme: s.theme,
                gradient: s.custom_theme,
                saturation: s.saturation,
                saturate_custom: s.saturate_custom,
                contrast: s.high_contrast || (s.sync_contrast && self.system.high_contrast),
            },
        );
    }

    fn speak_incoming(&self, m: &crate::state::Message, tts: bool) {
        let here = self.cur() == Some(m.channel);
        let s = &self.settings;
        let wanted = (s.tts_command && tts && here) || match s.tts_notify {
            2 => true,
            1 => here,
            _ => false,
        };
        if !wanted || m.author == self.store.me || self.voice.deaf {
            return;
        }
        let guild = self.store.channels.get(&m.channel).map_or(0, |c| c.guild);
        let text = crate::markdown::plain(&m.content, &self.store, guild);
        if !text.is_empty() {
            crate::speech::say(format!("{} says {text}", self.store.name_in(guild, m.author)));
        }
    }

    fn notify(&mut self, d: &serde_json::Value, ctx: &egui::Context) {
        let m = self.store.parse_message(d);
        self.speak_incoming(&m, d["tts"].as_bool() == Some(true));
        if m.author == self.store.me || !self.settings.notify || (self.settings.streamer && self.settings.streamer_no_notifications) {
            return;
        }
        // do not disturb: no desktop notifications, no sound, no flashing, as discord promises
        if self.store.presence.get(&self.store.me) == Some(&Status::Dnd) {
            return;
        }
        let Some(c) = self.store.channels.get(&m.channel) else { return };
        if self.store.muted.contains(&c.id) || self.store.muted.contains(&c.guild) {
            return;
        }
        let focused = ctx.input(|i| i.raw.focused);
        if focused && self.cur() == Some(c.id) {
            if self.settings.notify_selected {
                crate::sound::cue("message");
            }
            return;
        }
        let dm = c.guild == 0;
        if !(self.store.mentions_me(&m) || dm) {
            return;
        }
        let who = self.store.name_in(c.guild, m.author);
        let title = match self.store.guilds.get(&c.guild) {
            Some(g) => format!("{who} (#{}, {})", c.name, g.name),
            None if c.kind == 3 => format!("{who} ({})", channel_title(&self.store, c.id)),
            None => who,
        };
        // mentions and channels by name, the markdown taken off, as discord's toasts read
        let mut body: String = crate::markdown::plain(&m.content, &self.store, c.guild).chars().take(200).collect();
        if body.is_empty() && !m.attachments.is_empty() {
            body = "Uploaded a file".into();
        }
        let avatar = self.store.user(m.author).map(|u| u.avatar_url()).unwrap_or_default();
        let ping = crate::toast::Ping { title, body, avatar, at: (c.id, m.id), call: false };
        crate::sound::cue("message");
        if let Some(e) = &mut self.e2e {
            e.mark_once("notified");
            e.pinged.push(c.id);
            e.pings.push((crate::toast::xml(&ping, None), ping.at));
        } else {
            crate::toast::send(ping);
        }
        if !focused && self.settings.flash && self.e2e.is_none() {
            crate::winsys::flash();
        }
    }

    /// Settings that react to the outside world every frame: system theme,
    /// ctrl+wheel zoom, global keybinds, OBS starting, capture exclusion.
    fn os_settings(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let now = ctx.input(|i| i.time);
        if now >= self.system_at {
            self.system_at = now + 2.0;
            let sys = crate::winsys::preferences();
            if (sys.animations, sys.high_contrast) != (self.system.animations, self.system.high_contrast) {
                self.system = sys;
                self.apply_settings(ctx);
            }
        }
        if self.settings.theme == crate::theme::Theme::Sync {
            self.apply_look(ctx);
        }
        let z = ctx.zoom_factor();
        if (z - self.settings.zoom).abs() > 0.001 {
            self.settings.zoom = z;
            cache::set_settings(&self.settings);
        }
        let events: Vec<(String, bool)> = self.hotkey_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        crate::hotkeys::pause(self.settings_page == Some(settings::Page::System));
        for (action, pressed) in events {
            if action == "ptt" && self.settings.ptt && self.voice.chan.is_some() && !self.voice.mute {
                crate::sound::cue(if pressed { "ptt_start" } else { "ptt_stop" });
            }
            if action == "soundboard_hold" && !pressed {
                self.soundboard = None;
            }
            if !pressed {
                continue;
            }
            match action.as_str() {
                "voice_mode" => {
                    self.settings.ptt = !self.settings.ptt;
                    self.save_settings(ctx);
                }
                "voice_chat" if self.voice.chan.is_some() => self.voice_chat = !self.voice_chat,
                "camera" if self.voice.chan.is_some() => {
                    let on = !self.voice.camera;
                    self.voice.set_camera(&self.net, self.store.me, on);
                }
                "screenshare" if self.voice.chan.is_some() => {
                    if self.voice.stream.as_ref().is_some_and(|l| l.mine) {
                        self.voice.end_stream(&self.net);
                    } else if self.share.is_none() {
                        self.show_window(ctx);
                        self.share = Some(share::Picker::new(ctx));
                    }
                }
                "back" => self.go(true),
                "forward" => self.go(false),
                "goto_voice" => {
                    if let Some(c) = self.voice.chan {
                        self.show_window(ctx);
                        let g = self.store.channels.get(&c).map_or(0, |c| c.guild);
                        self.ui.guild = (g != 0).then_some(g);
                        self.open(c);
                    }
                }
                "disconnect" => self.voice.leave(&self.net),
                "soundboard" | "soundboard_hold" => {
                    let guild = self.voice.chan.and_then(|c| self.store.channels.get(&c)).map_or(0, |c| c.guild);
                    if self.soundboard.is_some() && action == "soundboard" {
                        self.soundboard = None;
                    } else if guild != 0 {
                        self.rest("GET", format!("/guilds/{guild}/soundboard-sounds"), None, crate::net::Tag::Soundboard(guild));
                        self.soundboard = Some((guild, None));
                    }
                }
                "mute" => {
                    let on = !self.voice.mute;
                    self.voice.set_mute(&self.net, on);
                }
                "deafen" => {
                    let on = !self.voice.deaf;
                    self.voice.set_deaf(&self.net, on);
                }
                "streamer" => {
                    self.settings.streamer = !self.settings.streamer;
                    self.save_settings(ctx);
                }
                "save_clip" => self.save_clip(ctx),
                "open" => self.show_window(ctx),
                "badge" => self.badge = None,
                "hide" => self.hide_to_tray(ctx),
                "voice_settings" => {
                    self.show_window(ctx);
                    self.settings_page = Some(settings::Page::Voice);
                }
                "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                _ => {}
            }
        }
        self.clips_tick(ctx);
        let obs = crate::winsys::OBS_RUNNING.load(std::sync::atomic::Ordering::Relaxed);
        if obs != self.obs_seen {
            self.obs_seen = obs;
            if self.settings.streamer_auto {
                self.settings.streamer = obs;
                self.save_settings(ctx);
            }
        }
        let hide = self.settings.streamer && self.settings.hide_window;
        if hide != self.capture_hidden {
            self.capture_hidden = hide;
            crate::winsys::exclude_from_capture(frame, hide);
        }
    }

    // the purple strip the official client shows across the top in streamer mode
    fn streamer_banner(&mut self, ui: &mut Ui) {
        if !self.settings.streamer {
            return;
        }
        let mut off = false;
        egui::Panel::top("streamer_banner")
            .exact_size(36.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(Color32::from_rgb(0x59, 0x3b, 0xd3)))
            .show(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                let msg = p.layout_no_wrap("Streamer Mode is enabled. Stay safe, friend.".into(), semi(14.0), Color32::WHITE);
                let link = p.layout_no_wrap("Disable".into(), semi(14.0), Color32::WHITE);
                let total = msg.size().x + 10.0 + link.size().x;
                let x = r.center().x - total / 2.0;
                let y = r.center().y - msg.size().y / 2.0;
                let lr = Rect::from_min_size(pos2(x + msg.size().x + 10.0, y), link.size());
                p.galley(pos2(x, y), msg, Color32::WHITE);
                p.galley(lr.min, link, Color32::WHITE);
                p.hline(lr.x_range(), lr.bottom(), egui::Stroke::new(1.0, Color32::WHITE));
                let resp = ui.interact(lr, egui::Id::new("streamer_off"), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                off = resp.clicked();
            });
        if off {
            self.settings.streamer = false;
            self.settings.streamer_auto = false;
            self.save_settings(ui.ctx());
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let n = self.store.mention_count()
            + self.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
        if self.title_count != Some(n) {
            self.title_count = Some(n);
            let t = if n > 0 { format!("({n}) Rustcord") } else { "Rustcord".into() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
        // discord's taskbar badge: the count, or with nothing mentioning you and the unread badge
        // on, a dot while anything's unread. Unreads take a walk over every channel, so once a second.
        let mut badge = n.min(i32::MAX as u32) as i32;
        if badge == 0 && self.settings.badge {
            let wait = self.badge_at.map_or(Duration::ZERO, |t| Duration::from_secs(1).saturating_sub(t.elapsed()));
            badge = match (wait.is_zero(), self.badge) {
                (false, Some(b)) => {
                    ctx.request_repaint_after(wait);
                    b.min(0)
                }
                _ => {
                    self.badge_at = Some(Instant::now());
                    let s = &self.store;
                    let any = s.channels.values().any(|c| c.is_text() && !s.muted.contains(&c.guild) && !s.is_muted(c) && s.unread(c) && s.can_view(c));
                    if any { -1 } else { 0 }
                }
            };
        }
        if self.badge != Some(badge) && self.store.me != 0 {
            crate::winsys::set_badge(badge);
            if crate::winsys::badge() == badge {
                self.badge = Some(badge);
            }
        }
        let v = &self.voice;
        let me = self.store.me;
        let m = v.muted.get(&me).copied().unwrap_or_default();
        crate::tray::set(crate::tray::State {
            unread: self.settings.badge && badge != 0,
            connected: v.state == crate::voice::State::Connected,
            speaking: v.speaking.contains(&me),
            mute: v.mute || v.deaf,
            server_mute: m[1],
            deaf: v.deaf,
            server_deaf: m[3],
        });
    }

    /// Minimized or put away in the tray: eframe stops calling ui() for the first and we skip
    /// drawing for the second, so logic() keeps the badge and the tray icon up to date.
    fn away(&self, ctx: &egui::Context) -> bool {
        self.frames > 0 && (ctx.input(|i| i.viewport().visible() == Some(false)) || !crate::winsys::shown())
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        // winit focuses by faking an alt press, which the e2e mustn't do to whoever's at the machine
        if self.e2e.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    pub(crate) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if !self.settings.tray_hint && self.e2e.is_none() {
            crate::tray::hint();
            self.settings.tray_hint = true;
            cache::set_settings(&self.settings);
        }
    }

    /// A clicked toast: the window back up and the message on screen.
    pub(crate) fn open_ping(&mut self, ctx: &egui::Context, ch: Id, msg: Id) {
        self.show_window(ctx);
        if self.store.channels.contains_key(&ch) {
            self.jump_to(ch, msg);
        }
    }

    fn on_rest(&mut self, tag: Tag, res: Result<serde_json::Value, String>) {
        if let Tag::Link(k) = tag {
            return self.on_link_rest(k, res);
        }
        if let (Tag::Remote(k), Err(e)) = (&tag, &res) {
            return settings::on_remote(self, k, Err(e.clone()));
        }
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                match tag {
                    Tag::Activity(k) => return activity::failed(self, &k, e),
                    Tag::InvitePreview(_) => return,
                    Tag::Pins(_) => self.pins = None,
                    Tag::Report(_) => return self.report_failed(e),
                    Tag::Inbox => self.inbox = None,
                    // the feed is a nicety; when it fails there's just no activity section
                    Tag::Feed | Tag::AppIcons(_) | Tag::Saved(_) | Tag::Note(_) | Tag::Outbox(_) | Tag::GuildProfile(_) | Tag::ModView(_) => return,
                    Tag::Boosts(g, k) => {
                        if let Some(i) = self.boost_info.get_mut(&g) {
                            i.loaded |= 1 << k;
                        }
                    }
                    Tag::Created => {
                        if let Some(f) = &mut self.create {
                            f.busy = false;
                        }
                    }
                    Tag::NewServer => {
                        if let Some(w) = &mut self.wizard {
                            w.busy = false;
                        }
                    }
                    // an empty list rather than a spinner that never stops
                    Tag::Directory(k) => directory::loaded(self, k, serde_json::json!([])),
                    Tag::Server(k) => server::loaded(self, k, serde_json::json!([])),
                    Tag::Chan("saved") => {
                        if let Some(s) = &mut self.chan_settings {
                            s.saving = false;
                        }
                    }
                    Tag::Chan(k) if k != "channel" => channel::loaded(self, k, serde_json::json!([])),
                    Tag::Home("members") => community::loaded(self, "members", serde_json::json!({ "members": [] })),
                    Tag::Events(g, k) => events::loaded(self, g, k, &serde_json::json!([])),
                    Tag::EventUsers(g, e) => events::users_loaded(self, g, e, &serde_json::json!([])),
                    Tag::Quests(k) => quests::loaded(self, k, &serde_json::json!({})),
                    Tag::Follow => follow::done(self, false),
                    _ => {}
                }
                return self.error_toast(e);
            }
        };
        match tag {
            Tag::Link(_) => {}
            Tag::Pins(ch) => {
                let list: Vec<Message> = v.as_array().into_iter().flatten().map(|m| self.store.parse_message(m)).collect();
                if self.pins.as_ref().is_some_and(|(c, _)| *c == ch) {
                    self.pins = Some((ch, Some(list)));
                }
            }
            Tag::Report(submit) => self.report_loaded(submit, v),
            Tag::Reactors(_, msg, key, burst) => self.store.set_reactors((msg, key, burst), &v),
            Tag::Soundboard(g) => {
                let items = if g == 0 { v.as_array() } else { v["items"].as_array() };
                let list: Vec<crate::state::Sound> = items.into_iter().flatten().map(crate::state::Sound::parse).collect();
                if let Some((_, slot)) = self.soundboard.as_mut() {
                    let all = slot.get_or_insert_with(Vec::new);
                    if g == 0 {
                        all.extend(list);
                    } else {
                        all.splice(0..0, list);
                    }
                }
            }
            Tag::Search(_) => {
                let list: Vec<Message> = v["messages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|group| group.as_array().and_then(|g| g.first()))
                    .map(|m| self.store.parse_message(m))
                    .collect();
                if let Some(s) = &mut self.search {
                    s.total = v["total_results"].as_u64().unwrap_or(list.len() as u64);
                    s.results = Some(list);
                }
            }
            Tag::Profile(u) => {
                if v["user"].is_object() {
                    self.store.apply("USER_UPDATE", &v["user"]);
                }
                for f in v["mutual_friends"].as_array().into_iter().flatten() {
                    self.store.apply("USER_UPDATE", f);
                }
                self.profiles.insert(u, v);
            }
            Tag::Outbox(u) => {
                self.outbox.insert(u, profile::recents(&v));
            }
            Tag::GuildProfile(g) => {
                self.guild_profiles.insert(g, v);
            }
            Tag::ModView(u) => modview::loaded(self, u, &v),
            Tag::Inbox => {
                let list: Vec<Message> = v.as_array().into_iter().flatten().map(|m| self.store.parse_message(m)).collect();
                if let Some(i) = &mut self.inbox {
                    i.mentions = Some(list);
                }
            }
            Tag::Forum(f) => self.store.load_forum(f, &v),
            Tag::Boosts(g, k) => boosts::load(self, g, k, &v),
            Tag::Created => create::created(self, &v),
            Tag::NewServer => newserver::created(self, &v),
            Tag::Lurk(g) => {
                self.store.lurking.insert(g);
            }
            Tag::Joined(g) => {
                self.store.lurking.remove(&g);
            }
            Tag::Directory(k) => directory::loaded(self, k, v),
            Tag::Server(k) => server::loaded(self, k, v),
            Tag::Chan(k) => channel::loaded(self, k, v),
            Tag::Home(k) => community::loaded(self, k, v),
            Tag::Events(g, k) => events::loaded(self, g, k, &v),
            Tag::EventUsers(g, e) => events::users_loaded(self, g, e, &v),
            Tag::Quests(k) => quests::loaded(self, k, &v),
            Tag::Follow => follow::done(self, true),
            Tag::Feed => {
                self.store.set_feed(&v);
                let mut want: Vec<Id> = self.store.feed.iter().map(|e| e.app).filter(|a| *a != 0 && !self.store.app_icons.contains_key(a)).collect();
                want.sort_unstable();
                want.dedup();
                want.truncate(50);
                if !want.is_empty() {
                    let q: Vec<String> = want.iter().map(|a| format!("application_ids={a}")).collect();
                    self.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::AppIcons(want));
                }
            }
            Tag::AppIcons(asked) => {
                for a in asked {
                    self.store.app_icons.insert(a, None);
                }
                for a in v.as_array().into_iter().flatten() {
                    self.store.app_icons.insert(crate::state::sid(&a["id"]), a["icon"].as_str().map(str::to_string));
                }
            }
            Tag::InvitePreview(code) => self.invite_card.got(&code, &v),
            Tag::Invite => {
                let name = v["guild"]["name"].as_str().unwrap_or("the server").to_string();
                self.toast(format!("Joined {name}"));
            }
            Tag::MadeInvite => {
                if let (Some(inv), Some(c)) = (&mut self.made_invite, v["code"].as_str()) {
                    inv.code = Some(c.to_string());
                }
            }
            Tag::InviteDm(link) => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.rest("POST", format!("/channels/{id}/messages"), Some(json!({ "content": link })), Tag::Quiet);
            }
            Tag::CallDm => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.ui.guild = None;
                self.open(id);
                self.join_voice(id, true);
            }
            Tag::InviteTo(uid) => {
                if let Some(code) = v["code"].as_str() {
                    self.send_invite(uid, code);
                }
            }
            Tag::OpenDm(text) => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.ui.guild = None;
                self.open(id);
                if !text.is_empty() {
                    self.send_text(id, text);
                }
            }
            Tag::Thread(first) => {
                self.store.apply("THREAD_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.open(id);
                if !first.trim().is_empty() {
                    self.drafts.insert(id, first);
                    self.send_message(id);
                }
            }
            Tag::Saved(k) => titlebar::saved(self, k, &v),
            Tag::Note(u) => {
                if let Some(n) = v["note"].as_str() {
                    self.notes.insert(u, n.to_string());
                }
            }
            Tag::Friend => self.toast("Friend request sent."),
            Tag::Commands(scope) => self.set_commands(scope, &v),
            Tag::Gifs(q) => self.set_gifs(q, &v),
            Tag::GifCats => self.set_gif_cats(&v),
            Tag::Welcome(g) => {
                let on = v["enabled"].as_bool().unwrap_or(false);
                let actions = v["new_member_actions"].as_array().into_iter().flatten().map(|a| crate::state::sid(&a["channel_id"]));
                let e = self.guides.entry(g).or_default();
                e.0 = if on { actions.filter(|c| *c != 0).collect() } else { Vec::new() };
            }
            Tag::MemberActions(g) => {
                let done = v["channel_actions"].as_object().into_iter().flatten().filter(|(_, a)| a["completed"].as_bool() == Some(true));
                self.guides.entry(g).or_default().1 = done.filter_map(|(c, _)| c.parse().ok()).collect();
            }
            Tag::Remote(k) => settings::on_remote(self, k, Ok(v)),
            Tag::Activity(k) => activity::on_rest(self, &k, v),
            Tag::Quiet => {}
        }
    }

    /// Images and copied files on the clipboard become attachments.
    pub fn paste_files(&mut self) {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        if let Ok(list) = cb.get().file_list()
            && !list.is_empty()
        {
            self.files.extend(list.into_iter().map(composer::Upload::new));
            return;
        }
        let Ok(img) = cb.get_image() else { return };
        let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else { return };
        let dir = std::env::temp_dir().join("rustcord");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("image-{}.png", chrono::Utc::now().timestamp_millis()));
        if buf.save(&path).is_ok() {
            let mut u = composer::Upload::new(path);
            u.name = "image.png".into();
            self.files.push(u);
            self.focus_composer = true;
        }
    }

    pub fn sign_in(&mut self, t: String) {
        cache::set_token(Some(&t));
        self.net.send(Cmd::Start(t.clone()));
        self.token = Some(t);
        self.login = Default::default();
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
        self.toast_detail = None;
    }

    pub fn error_toast(&mut self, e: String) {
        self.toast("Something went wrong. Shift Click this notification to see error details.");
        self.toast_detail = Some(e);
    }

    pub fn logout(&mut self) {
        if self.store.me != 0 {
            cache::forget_account(self.store.me);
            self.accounts = cache::accounts();
        }
        self.reset_session();
    }

    /// Drops the signed-in session and its cached data, back to the login screen.
    pub fn reset_session(&mut self) {
        self.voice.leave(&self.net);
        self.net.send(Cmd::Stop);
        crate::rpc::signed_out();
        self.links.clear();
        self.refresh_game();
        cache::set_token(None);
        self.token = None;
        self.store = Tracked::default();
        self.ui = UiState::default();
        self.nav = Default::default();
        self.inbox = None;
    }

    fn pump(&mut self, ctx: &egui::Context) {
        let clicked: Vec<crate::toast::Click> = self.pings.as_ref().map(|r| r.try_iter().collect()).unwrap_or_default();
        for click in clicked {
            match click {
                crate::toast::Click::Open(ch, msg) => self.open_ping(ctx, ch, msg),
                crate::toast::Click::Answer(ch) => {
                    self.show_window(ctx);
                    self.answer(ch);
                }
                crate::toast::Click::Decline(ch) => self.decline(ch),
            }
        }
        self.call_fx(ctx);
        if let Some(g) = self.games.as_ref().and_then(|r| r.try_iter().last()) {
            self.detected = g;
            self.refresh_game();
        }
        self.links_pump(ctx);
        // a game opening or closing, rich presence changing, or sharing switched, goes out once we're connected
        let want = self.presence_key();
        if want != self.shared && self.conn == crate::net::Conn::Online && self.store.me != 0 {
            self.send_presence();
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Dispatch(t, d) => {
                    if t == "MESSAGE_CREATE" {
                        crate::rpc::on_message(&d);
                        self.notify(&d, ctx);
                    }
                    if t == "INTERACTION_FAILURE" {
                        self.toast("The application did not respond.");
                    }
                    if t == "READY" {
                        if let Some(e) = &mut self.e2e {
                            e.mark("ready");
                        }
                        self.shared = (None, 0);
                        if let Some(tk) = &self.token {
                            crate::rpc::signed_in(tk, &d["user"], d["session_id"].as_str().unwrap_or(""));
                        }
                        self.info_asked.clear();
                        self.feed_at = Some(Instant::now());
                        self.save_at = Some(Instant::now() + Duration::from_secs(5));
                        // which messages are bookmarked, for the menus' Remove Bookmark
                        self.rest("GET", "/users/@me/saved-messages?bookmark_ids=true".into(), None, Tag::Saved(0));
                    }
                    if t.starts_with("STREAM_")
                        && let Some(e) = &mut self.e2e
                    {
                        e.heard.push(json!({ "t": t, "d": d }));
                    }
                    self.voice_cues(&t, &d);
                    if t == "VOICE_CHANNEL_EFFECT_SEND" {
                        self.soundboard_effect(&d);
                    }
                    self.voice.dispatch(&self.net, self.store.me, &t, &d);
                    let took = Instant::now();
                    self.store.apply(&t, &d);
                    if t == "READY" || t == "USER_SETTINGS_PROTO_UPDATE" {
                        self.sync_account(ctx);
                    }
                    if t == "READY"
                        && let Some(e) = &mut self.e2e
                    {
                        e.cost("ready_apply", took.elapsed());
                    }
                    if t == "READY" {
                        self.remember_me();
                        // cached channel lists can be stale, refetch whatever is open
                        for m in self.store.msgs.values_mut() {
                            m.fresh = false;
                        }
                        match self.cur() {
                            Some(c) => self.open(c),
                            None => self.select_guild(self.ui.guild.filter(|g| self.store.guilds.contains_key(g))),
                        }
                    }
                }
                Ev::Messages { channel, page, list } => {
                    let n = self.store.set_messages(channel, page, &list);
                    self.request_members(channel);
                    if self.jump_top == Some(channel) {
                        self.jump_top = None;
                        self.scroll_fix = Some((channel, 0.0));
                    }
                    if matches!(page, Page::Before(_)) && n > 0 {
                        let est: f32 = self.store.msgs[&channel].list[..n].iter().map(chat::estimate).sum();
                        let off = self.offsets.get(&channel).copied().unwrap_or(0.0);
                        self.scroll_fix = Some((channel, off + est));
                    }
                }
                Ev::MessagesFailed { channel } => {
                    if let Some(m) = self.store.msgs.get_mut(&channel) {
                        m.loading = false;
                        m.fresh = true;
                    }
                }
                Ev::SendFailed { channel, nonce, err, again } => {
                    // a retry going out stays red without saying so again
                    let first = self.store.msgs.get(&channel).is_some_and(|e| e.list.iter().any(|m| m.pending && m.nonce.as_deref() == Some(nonce.as_str())));
                    self.store.fail_pending(channel, &nonce);
                    let low = err.to_lowercase();
                    let why = if low.contains("cannot send messages to this user") {
                        Some("You cannot message this user")
                    } else if low.contains("slowmode") {
                        Some("You are sending messages too quickly")
                    } else if low.contains("missing permissions") || low.contains("missing access") {
                        Some("You cannot send messages in this channel")
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        self.notice = Some((channel, why.into()));
                    } else if first || !again {
                        self.error_toast(format!("Message failed to send: {err}"));
                    }
                }
                Ev::Conn(c) => self.conn = c,
                Ev::AuthFailed => {
                    self.logout();
                    self.login.expired();
                }
                Ev::Auth(auth::Ev::Token(t)) => self.sign_in(t),
                Ev::Auth(e) => self.login.event(e, &self.net),
                Ev::Error(e) => self.error_toast(e),
                Ev::Toast(e) => self.toast(e),
                Ev::Rest { tag, res } => self.on_rest(tag, res),
                Ev::Voice(e) => {
                    if let Some(err) = self.voice.event(e) {
                        if let Some(e) = &mut self.e2e {
                            e.heard.push(json!({ "t": "voice error", "d": err }));
                        }
                        self.toast(format!("Voice disconnected: {err}"));
                    }
                    if let Some((key, img)) = self.voice.preview_job.take() {
                        self.stream_preview(key, img);
                    }
                    if std::mem::take(&mut self.voice.entrance_due) {
                        self.entrance_sound();
                    }
                    if self.voice.shared.no_audio.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        self.toast("No audio detected from your mic. Check the input device in Voice & Video settings.");
                    }
                    if self.voice.video_call && self.voice.state == crate::voice::State::Connected {
                        self.voice.video_call = false;
                        self.voice.set_camera(&self.net, self.store.me, true);
                    }
                }
            }
        }
    }

    fn ack_current(&mut self, ctx: &egui::Context) {
        if self.e2e.as_ref().is_some_and(|e| e.tour) {
            return;
        }
        if !ctx.input(|i| i.focused) {
            return;
        }
        let Some(ch) = self.cur() else { return };
        let side = self.side_thread.filter(|t| self.store.channels.get(t).is_some_and(|t| t.parent == ch));
        for ch in std::iter::once(ch).chain(side) {
            if Some(ch) == self.cur() {
                if self.hold_ack == Some(ch) {
                    continue;
                }
                self.hold_ack = None;
            }
            let Some(last) = self.store.channels.get(&ch).map(|c| c.last_msg) else { continue };
            let (acked, mentions) = self.store.reads.get(&ch).map_or((0, 0), |r| (r.last_acked, r.mentions));
            if last <= acked && mentions == 0 {
                continue;
            }
            let r = self.store.reads.entry(ch).or_default();
            r.last_acked = last.max(r.last_acked);
            r.mentions = 0;
            if last != 0 {
                self.net.send(Cmd::Ack { channel: ch, id: last });
            }
        }
    }

    fn overlays(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        create::show(self, &ctx);
        lurk::tick(self);
        folders::show(self, &ctx);
        newserver::show(self, &ctx);
        guildmodal::show(self, &ctx);
        active::show(self, &ctx);
        server::show(self, &ctx);
        channel::show(self, &ctx);
        events::show(self, &ctx);
        stage::modals(self, &ctx);
        share::show(self, &ctx);
        let mut show_detail = false;
        clips_gallery::show(self, &ctx);
        if let Some((text, at)) = &self.toast {
            let e = at.elapsed().as_secs_f32();
            if e > 4.0 {
                self.toast = None;
            } else {
                let text = text.clone();
                let k = motion::EASE_OUT.at(e / 0.3) - motion::EASE_IN.at((e - 3.7) / 0.3);
                let slide = if crate::theme::anim(1.0) > 0.0 { 24.0 * (1.0 - k) } else { 0.0 };
                let area = egui::Area::new("toast".into()).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -90.0)).order(egui::Order::Tooltip).fade_in(false);
                ctx.set_transform_layer(area.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, slide)));
                area.show(&ctx, |ui| {
                    ui.multiply_opacity(k);
                    let r = Frame::new()
                        .fill(pal().bg_floating)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(text).color(pal().text_normal).font(reg(14.0))).wrap_mode(egui::TextWrapMode::Extend)));
                    self.rects.insert("toast".into(), r.response.rect);
                    let r = ui.interact(r.response.rect, egui::Id::new("toast_click"), Sense::click());
                    if r.clicked() && ui.input(|i| i.modifiers.shift) {
                        show_detail = true;
                    }
                });
                if e < 0.3 || e >= 3.7 {
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(Duration::from_secs_f32(3.7 - e));
                }
            }
        }
        if show_detail && let Some(e) = self.toast_detail.take() {
            self.toast = None;
            self.confirm = Some(Confirm::new("Error Details", e, "Okay", |_: &mut App| {}).blurple());
        }
        self.confirm_modal(&ctx);
        settings::camera::frame(self, &ctx);
        if let Some(gid) = self.guild_notifs {
            let name = self.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
            let default = self.store.guilds.get(&gid).map(|g| g.default_notif).unwrap_or(0);
            let level = self.store.levels.get(&gid).copied().unwrap_or(default);
            let muted = self.store.muted.contains(&gid);
            let (mut pick, mut mute) = (None, None);
            let modal = crate::ui::motion::Modal::new("guild_notifs".into())
                .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
                .show(&ctx, |ui| {
                    ui.set_width(440.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.label(egui::RichText::new("Notification Settings").font(semi(20.0)).color(pal().header_primary));
                    ui.label(egui::RichText::new(&name).font(reg(14.0)).color(pal().header_secondary));
                    ui.add_space(16.0);
                    if menu_row(ui, None, &format!("Mute {name}"), None, false, Trail::Check(muted)).clicked() {
                        mute = Some(!muted);
                    }
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("Server Notification Settings").font(medium(14.0)).color(pal().header_secondary));
                    ui.add_space(8.0);
                    for (label, v) in [("All Messages", 0), ("Only @mentions", 1), ("Nothing", 2)] {
                        if menu_row(ui, None, label, None, false, Trail::Radio(level == v)).clicked() {
                            pick = Some(v);
                        }
                    }
                    });
                });
            if let Some(m) = mute {
                self.set_muted(gid, true, m);
            }
            if let Some(v) = pick {
                self.set_level(gid, Some(v));
            }
            if modal.should_close() {
                self.guild_notifs = None;
            }
        }
        crop::show(self, &ctx);
        cstatus::show(self, &ctx);
        picker::show(self, &ctx);
        picker::stickers(self, &ctx);
        chat::context_menu(self, &ctx);
        usermenu::user_menu(self, &ctx);
        userdlg::show(self, &ctx);
        popups::show(self, &ctx);
        attach::dragging(self, &ctx);
        links::show(self, &ctx);
        popups::viewer(self, &ctx);
        player::overlay(self, &ctx);
        crate::media::tick(ctx.cumulative_frame_nr());
        call::incoming(self, &ctx);
        activity::overlays(self, &ctx);
        keys::combos(self, &ctx);
        keys::breathe(self, &ctx);
        keys::demon(self, &ctx);
        keys::discodo(self, &ctx);
    }

    /// Discord's confirm dialog: the title with a ✕, the question, the message it's about and a
    /// protip, then Cancel and the action. With a message they split the width; without, they
    /// sit at the right, "Don't ask again" across from them. Enter goes ahead, as in discord.
    fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(c) = &mut self.confirm else { return };
        let (mut close, mut go) = (false, false);
        let pinned = |ch: Id, id: Id| self.pins.as_ref().filter(|(c, _)| *c == ch).and_then(|(_, l)| l.as_ref()?.iter().find(|m| m.id == id).cloned());
        let msg = c.msg.and_then(|(ch, id)| self.store.find_msg(ch, id).cloned().or_else(|| pinned(ch, id)));
        let store = &self.store;
        let modal = crate::ui::motion::Modal::new("confirm".into())
            .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
            .show(ctx, |ui| {
                ui.set_width(432.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&c.title).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close |= icon_button(ui, icons::Icon::Close, 24.0, "Close").clicked();
                    });
                });
                ui.add_space(8.0);
                ui.label(ui.painter().layout_job(bolded(&c.body, 16.0, pal().text_normal, 432.0)));
                if let Some(m) = &msg {
                    ui.add_space(16.0);
                    chat::preview(ui, store, m);
                }
                if let Some([a, b, rest]) = c.tip {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("PROTIP:").font(bold(12.0)).color(GREEN));
                    ui.add_space(2.0);
                    ui.label(ui.painter().layout_job(bolded(&format!("{a}**{b}**{rest}"), 14.0, pal().text_normal, 432.0)));
                }
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if msg.is_some() {
                        close |= grey_button(ui, "Cancel", 212.0).clicked();
                        go = button(ui, c.button, c.fill, 212.0).clicked();
                        return;
                    }
                    if let Some((on, _)) = &mut c.check {
                        let (r, resp) = ui.allocate_exact_size(vec2(160.0, 38.0), Sense::click());
                        let b = Rect::from_center_size(pos2(r.left() + 12.0, r.center().y), vec2(24.0, 24.0));
                        if *on {
                            ui.painter().rect_filled(b, 6, BLURPLE);
                            icons::draw(ui.painter(), icons::Icon::Check, b.shrink(4.0), Color32::WHITE);
                        } else {
                            ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                        }
                        ui.painter().text(pos2(b.right() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, "Don't ask again", reg(16.0), pal().text_normal);
                        if resp.clicked() {
                            *on = !*on;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let w = ui.painter().layout_no_wrap(c.button.to_string(), medium(16.0), Color32::WHITE).size().x + 32.0;
                        go = button(ui, c.button, c.fill, w.max(60.0)).clicked();
                        close |= grey_button(ui, "Cancel", 92.0).clicked();
                    });
                });
            });
        go |= c.shown && enter_submits(ctx, true);
        c.shown = true;
        if go && let Some(c) = self.confirm.take() {
            if let Some((true, then)) = c.check {
                then(self);
            }
            (c.run)(self);
        } else if close || modal.should_close() {
            self.confirm = None;
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.pump(ctx);
        activity::logic(self, ctx);
        self.os_settings(ctx, frame);
        if self.away(ctx) {
            self.update_title(ctx);
            if let Some(mut e) = self.e2e.take() {
                e.hidden(self, ctx);
                self.e2e = Some(e);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        if self.away(&ctx) {
            return;
        }
        let want_cjk = crate::theme::CJK_WANTED.load(std::sync::atomic::Ordering::Relaxed);
        if (want_cjk && !self.cjk_loaded) || crate::fonts::take_fresh() {
            self.cjk_loaded = want_cjk;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::theme::fonts(want_cjk));
            });
            self.fonts_rx = Some(rx);
        }
        if let Some(f) = self.fonts_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            let took = Instant::now();
            ctx.set_fonts(f);
            self.fonts_rx = None;
            if let Some(e) = &mut self.e2e {
                e.cost("cjk_fonts", took.elapsed());
            }
        }
        crate::theme::set_me(self.store.me);
        crate::theme::backdrop(&ctx);
        // rebuilt every frame by whatever gets drawn, so nothing points at last frame's layout
        self.rects.clear();
        if self.frames == 0
            && let Some(e) = &mut self.e2e
        {
            e.mark("first_frame");
        }
        if let Some(e) = &mut self.e2e {
            e.screenshots(&ctx);
        }

        if self.frames == 0 {
            crate::winsys::frameless(frame);
        }
        let (zin, zout, zreset) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                take_key(i, Modifiers::COMMAND, Key::Equals) || take_key(i, Modifiers::COMMAND, Key::Plus),
                take_key(i, Modifiers::COMMAND, Key::Minus),
                take_key(i, Modifiers::COMMAND, Key::Num0),
            )
        });
        if zin || zout {
            self.step_zoom(&ctx, zin);
        }
        if zreset {
            ctx.set_zoom_factor(1.0);
            self.toast("Zoom 100%");
        }
        // F11, as in a browser
        if ctx.input_mut(|i| take_key(i, egui::Modifiers::NONE, egui::Key::F11)) {
            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            titlebar::fullscreen(&ctx, !full);
        }
        // what was open as the frame began owns Escape and most keys this frame
        let layered = self.layered(&ctx);
        titlebar::show(self, ui);
        if self.token.is_none() {
            login::show(self, ui);
        } else {
            self.streamer_banner(ui);
            // a modal over the app; shown first so it takes ESC before the chat does
            let settings_open = self.settings_page.is_some();
            if settings_open {
                settings::show(self, ui);
            }
            let w = ctx.content_rect().width();
            if (w - self.heights_w).abs() > 0.5 {
                self.heights.clear();
                self.heights_w = w;
            }
            let full = self.call.full && self.voice.chan.is_some() && self.cur() == self.voice.chan;
            self.call.full = full;
            let rail = if full {
                Rect::NOTHING
            } else {
                egui::Panel::left("rail")
                    .exact_size(72.0)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(Frame::new().fill(pal().bg_tertiary))
                    .show(ui, |ui| rail::show(self, ui))
                    .response
                    .rect
            };
            // discord: 375 wide with the rail and its 1px frame line, draggable between 264 and 432
            let side_w = if self.ui.side_w > 0.0 { self.ui.side_w } else { 302.0 };
            let cr = if full {
                egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| chat::show(self, ui)).response.rect
            } else if self.directory.is_some() {
                // discord's discover page brings its own sidebar
                let c = egui::CentralPanel::default()
                    .frame(Frame::new().fill(pal().bg_primary).inner_margin(egui::Margin { top: 1, ..egui::Margin::ZERO }))
                    .show(ui, |ui| directory::show(self, ui));
                let side = egui::Rect::from_min_size(c.response.rect.min, vec2(side_w, c.response.rect.height()));
                side_edge(ui.painter(), side);
                side::card(self, ui, rail, side);
                c.response.rect
            } else {
                let r = egui::Panel::left("side")
                    .default_size(side_w)
                    .size_range(191.0..=(w - 72.0 - 420.0).clamp(191.0, 359.0))
                    .resizable(true)
                    .show_separator_line(false)
                    .frame(Frame::new().fill(pal().bg_primary))
                    .show(ui, |ui| side::show(self, ui));
                side_edge(ui.painter(), r.response.rect);
                side::card(self, ui, rail, r.response.rect);
                if let Some(w) = self.dragged(&ctx, r.response.rect.width(), side_w) {
                    self.ui.side_w = w;
                }
                // discord's frame line carries on from the sidebar's top over the chat and members
                let c = egui::CentralPanel::default()
                    .frame(Frame::new().fill(pal().bg_primary).inner_margin(egui::Margin { top: 1, ..egui::Margin::ZERO }))
                    .show(ui, |ui| chat::show(self, ui));
                c.response.rect
            };
            ui.painter().hline(cr.x_range(), cr.top() + 0.5, egui::Stroke::new(1.0, pal().frame_border));
            self.overlays(ui);
            callbar::popout(self, &ctx);
            self.update_title(&ctx);
            if !settings_open {
                self.ack_current(&ctx);
                if !layered && ctx.input_mut(|i| take_key(i, egui::Modifiers::COMMAND, egui::Key::K)) {
                    self.switcher = Some(String::new());
                }
                self.shortcuts(&ctx, layered);
            }
            if self.store.typing.values().flatten().any(|(_, t)| t.elapsed().as_secs() >= 10) {
                self.store.prune_typing();
            }
            if let Some(e) = &mut self.e2e
                && self.frames > 0
                && !self.store.guilds.is_empty()
            {
                e.mark_once("content");
            }
        }

        if self.token.is_some() {
            titlebar::inbox(self, &ctx);
        }

        // discord refreshes its activity feed now and then; ten minutes is close enough
        if self.feed_at.is_some_and(|t| Instant::now() >= t) {
            self.feed_at = Some(Instant::now() + Duration::from_secs(600));
            self.rest("GET", "/content-inventory/users/@me?for_game_profile=false&feature=inbox".into(), None, Tag::Feed);
        }
        if self.save_at.is_some_and(|t| Instant::now() >= t) {
            self.save_at = None;
            let took = Instant::now();
            cache::save(&self.store, &self.ui, false);
            if let Some(e) = &mut self.e2e {
                e.cost("cache_save", took.elapsed());
            }
        }
        if self.sweep_at.elapsed() >= Duration::from_secs(30) {
            self.sweep_at = Instant::now();
            for uri in self.textures.stale(Duration::from_secs(120)) {
                ctx.forget_image(&uri);
            }
        }
        autoscroll::paint(&ctx, &self.autoscroll);
        // eframe opens links only with a feature we don't build, so hand them to the shell; under
        // e2e they're only written down
        let urls: Vec<String> = ctx.output_mut(|o| {
            let mut urls = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) => {
                    urls.push(u.url.clone());
                    false
                }
                _ => true,
            });
            urls
        });
        for u in urls {
            match &mut self.e2e {
                Some(e) => e.opened.push(u),
                None => {
                    let _ = open::that_detached(u);
                }
            }
        }
        if let Some(mut e) = self.e2e.take() {
            e.step(self, &ctx);
            e.frame_time(started.elapsed());
            self.e2e = Some(e);
        }
        motion::tooltips(&ctx);
        self.frames += 1;
        // fullscreen's size isn't the window's; keep the one to come back to
        if !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
            self.win = ctx.input(|i| i.viewport().inner_rect).map(|r| [r.width(), r.height()]);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // egui wakes a delayed repaint this much early, then spins a frame per vsync until it's
        // due: five frames for every blink of the caret
        raw.predicted_dt = 0.0;
        if let Some(e) = &mut self.e2e {
            e.inject(raw);
        }
        autoscroll::filter(&mut self.autoscroll, raw);
        let nav = self.focus_msg.is_none() && !self.modal_open() && !self.layered(ctx);
        self.wheel.filter(ctx, raw, nav);
        // ctrl+wheel: a zoom level a notch
        let mut notches = 0.0;
        raw.events.retain(|e| match e {
            egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.command => {
                notches += match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                    egui::MouseWheelUnit::Page => delta.y * 3.0,
                };
                false
            }
            _ => true,
        });
        self.zoom_acc += notches;
        while self.zoom_acc.abs() >= 1.0 {
            let up = self.zoom_acc > 0.0;
            self.zoom_acc -= if up { 1.0 } else { -1.0 };
            self.step_zoom(ctx, up);
        }
        // text pasted past the message limit goes up as message.txt instead, as in discord
        if ctx.memory(|m| m.has_focus(egui::Id::new("composer"))) {
            let limit = if self.store.nitro() { 4000 } else { 2000 };
            let mut long = Vec::new();
            raw.events.retain(|e| match e {
                egui::Event::Paste(t) if t.chars().count() > limit => {
                    long.push(t.clone());
                    false
                }
                _ => true,
            });
            for t in long {
                self.attach_text(&t);
            }
        }
        // dropping files on the window attaches them to the next message
        let dropped = !raw.dropped_files.is_empty();
        for f in raw.dropped_files.drain(..) {
            if !server::dropped(self, f.path()) {
                self.files.push(composer::Upload::new(f.path().to_path_buf()));
            }
        }
        if dropped && key_down(0x10) && let Some(ch) = self.cur() {
            let draft = self.drafts.remove(&ch);
            self.post(ch, false);
            if let Some(d) = draft {
                self.drafts.insert(ch, d);
            }
        }
        // egui-winit swallows ctrl+v when the clipboard holds no text, so watch the key ourselves;
        // not under e2e, where the real keyboard belongs to whoever's at the machine
        let v = self.e2e.is_none() && raw.focused && key_down(0x11) && key_down(b'V');
        if v && !self.v_down && !raw.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
            self.paste_files();
        }
        self.v_down = v;
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        let c = crate::theme::clear();
        crate::winsys::set_backdrop([c.r(), c.g(), c.b()]);
        c.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        crate::tray::remove();
        if self.token.is_some() {
            cache::save(&self.store, &self.ui, true);
        }
        if let Some(w) = self.win {
            cache::set_window(w);
        }
    }
}

pub fn avatar(ui: &Ui, rect: Rect, url: &str) {
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, pal().bg_accent);
    crate::gif::paint_still(ui, url, rect, CornerRadius::same((rect.width() / 2.0) as u8), ui.rect_contains_pointer(rect));
}

pub fn status_dot(ui: &Ui, avatar: Rect, s: Status, ring: Color32) {
    let r = (avatar.width() * 0.16).max(4.0);
    let c = pos2(avatar.right() - r * 0.9, avatar.bottom() - r * 0.9);
    ui.painter().circle_filled(c, r + 2.5, ring);
    let col = status_color(s);
    match s {
        Status::Offline => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().circle_filled(c, r * 0.45, ring);
        }
        Status::Idle => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().circle_filled(c + vec2(-r * 0.45, -r * 0.45), r * 0.6, ring);
        }
        Status::Dnd => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().rect_filled(
                Rect::from_center_size(c, vec2(r * 1.2, r * 0.4)),
                CornerRadius::same(2),
                ring,
            );
        }
        Status::Online => {
            ui.painter().circle_filled(c, r, col);
        }
    }
}

pub fn badge(ui: &Ui, right_center: Pos2, n: u32, ring: Option<Color32>) {
    badge_scaled(ui, right_center, n, ring, 1.0);
}

/// A badge that grows in when it first shows, on discord's .2s (0, 0, .2, 1.1) curve.
pub fn badge_in(ui: &Ui, id: egui::Id, right_center: Pos2, n: u32, ring: Option<Color32>) {
    badge_scaled(ui, right_center, n, ring, motion::enter(ui.ctx(), id, 0.2, motion::Curve::Bezier(0.0, 0.0, 0.2, 1.1)));
}

fn badge_scaled(ui: &Ui, right_center: Pos2, n: u32, ring: Option<Color32>, k: f32) {
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let galley = ui.painter().layout_no_wrap(text, semi(12.0), Color32::WHITE);
    let w = (galley.size().x + 10.0).max(16.0);
    let rect = Rect::from_min_size(pos2(right_center.x - w, right_center.y - 8.0), vec2(w, 16.0));
    let rect = Rect::from_center_size(rect.center(), rect.size() * k);
    if let Some(ring) = ring {
        ui.painter().rect_filled(rect.expand(3.0 * k), CornerRadius::same(11), ring);
    }
    ui.painter().rect_filled(rect, CornerRadius::same(8), RED);
    // centre the digits' ink, not the line box, which keeps room under them for descenders
    let mut ink = (f32::MAX, f32::MIN);
    for r in &galley.rows {
        for g in &r.glyphs {
            let top = r.pos.y + g.pos.y + g.uv_rect.offset.y;
            ink = (ink.0.min(top), ink.1.max(top + g.uv_rect.size.y));
        }
    }
    let mid = if ink.0 < ink.1 { (ink.0 + ink.1) / 2.0 } else { galley.size().y / 2.0 };
    ui.painter().galley_with_override_text_color(pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - mid), galley, Color32::WHITE.gamma_multiply(k.clamp(0.0, 1.0)));
}

pub fn button(ui: &mut Ui, text: &str, fill: Color32, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = motion::mix(fill, fill.gamma_multiply(0.85), motion::hot(ui.ctx(), &resp));
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, semi(14.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Square icon button. Small ones (header style) only brighten on hover,
/// 32px+ ones get the rounded hover plate like the user panel and message toolbar.
pub fn icon_button(ui: &mut Ui, icon: icons::Icon, size: f32, tip: &str) -> egui::Response {
    toggle_button(ui, icon, size, tip, false)
}

/// An icon button that stays lit while what it opens is showing, like discord's member list toggle.
pub fn toggle_button(ui: &mut Ui, icon: icons::Icon, size: f32, tip: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let k = motion::hot(ui.ctx(), &resp);
    let col = motion::mix(if on { pal().header_primary } else { pal().header_secondary }, pal().interactive_hover, k);
    if size >= 32.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal().control_hover.gamma_multiply(k));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let s = if size >= 32.0 { 20.0 } else { (size * 0.86).min(24.0) };
    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(s, s)), col);
    resp.on_hover_text(tip)
}

/// A context menu row the way discord draws them: icon on the left, 14 medium label,
/// red for destructive ones, a soft wash on hover.
pub fn menu_item(ui: &mut Ui, icon: Option<icons::Icon>, label: &str, danger: bool) -> bool {
    menu_row(ui, icon, label, None, danger, Trail::None).clicked()
}

/// A confirmation like discord's, for leaving servers, blocking, pinning and so on: Cancel
/// beside a red button, or a blurple one for the harmless ones.
pub struct Confirm {
    pub title: String,
    // **bold** stretches as discord bolds names in these
    pub body: String,
    pub button: &'static str,
    pub fill: Color32,
    // the message it's about, shown under the body as the pin and delete dialogs do
    pub msg: Option<(Id, Id)>,
    // discord's protip under that: the words, the bold part, the rest
    pub tip: Option<[&'static str; 3]>,
    // "Don't ask again" beside the buttons, and what ticking it does when the answer is yes
    pub check: Option<(bool, fn(&mut App))>,
    pub run: Box<dyn FnOnce(&mut App)>,
    // Enter only answers once the dialog has been up a frame, not the Enter that opened it
    shown: bool,
}

pub const DANGER: Color32 = Color32::from_rgb(0xd2, 0x2d, 0x39);

impl Confirm {
    pub fn new(title: impl Into<String>, body: impl Into<String>, button: &'static str, run: impl FnOnce(&mut App) + 'static) -> Self {
        Confirm { title: title.into(), body: body.into(), button, fill: DANGER, msg: None, tip: None, check: None, run: Box::new(run), shown: false }
    }

    pub fn blurple(mut self) -> Self {
        self.fill = BLURPLE;
        self
    }

    pub fn about(mut self, ch: Id, id: Id) -> Self {
        self.msg = Some((ch, id));
        self
    }

    pub fn tip(mut self, tip: [&'static str; 3]) -> Self {
        self.tip = Some(tip);
        self
    }

    pub fn dont_ask(mut self, then: fn(&mut App)) -> Self {
        self.check = Some((false, then));
        self
    }
}

/// Discord's quieter button: a grey wash with a hairline, for Cancel and the like.
pub fn grey_button(ui: &mut Ui, text: &str, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = motion::mix(pal().control, pal().control_hover, motion::hot(ui.ctx(), &resp));
    ui.painter().rect(rect, CornerRadius::same(8), fill, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, medium(16.0), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// `**bold**` stretches in a line of dialog text.
pub fn bolded(text: &str, size: f32, ink: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (i, part) in text.split("**").enumerate() {
        let (font, col) = if i % 2 == 1 { (semi(size), pal().header_primary) } else { (reg(size), ink) };
        job.append(part, 0.0, egui::TextFormat::simple(font, col));
    }
    job.wrap.max_width = width;
    job
}

/// A menu that opens at a point, like a right click, with room for submenus. False once closed.
pub fn float_menu(ctx: &egui::Context, id: &str, at: Pos2, content: impl FnOnce(&mut Ui)) -> bool {
    let mut open = true;
    egui::Popup::new(egui::Id::new(id), ctx.clone(), egui::PopupAnchor::Position(at), egui::LayerId::background())
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .frame(Frame::menu(&ctx.global_style()))
        .align(egui::RectAlign::BOTTOM_START)
        .open_bool(&mut open)
        .show(|ui| menu_body(ui, content));
    open
}

/// Shift+click on a menu's checkbox ticks it and leaves the menu open, as in discord.
pub fn menu_closing(ctx: &egui::Context) -> egui::PopupCloseBehavior {
    if ctx.input(|i| i.modifiers.shift) { egui::PopupCloseBehavior::CloseOnClickOutside } else { egui::PopupCloseBehavior::CloseOnClick }
}

/// Long menus scroll rather than run off a small window.
pub fn menu_body(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let h = ui.ctx().content_rect().height() - 32.0;
    egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui)
    });
}

thread_local! {
    static ROWS: std::cell::RefCell<Option<Vec<(String, egui::Id)>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `menu`, returning the label and id of each row it drew, top to bottom.
pub fn rows_of(menu: impl FnOnce()) -> Vec<(String, egui::Id)> {
    ROWS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    menu();
    ROWS.with(|r| r.borrow_mut().take()).unwrap_or_default()
}

/// A select's option, listed with the menu's rows for `select_keys`.
pub fn option(ui: &mut Ui, on: bool, label: &str) -> egui::Response {
    let r = ui.selectable_label(on, label);
    list_row(label, r.id);
    r
}

/// Lists a hand-drawn row with its menu's rows for `select_keys`.
pub fn list_row(label: &str, id: egui::Id) {
    ROWS.with(|rows| {
        if let Some(v) = rows.borrow_mut().as_mut() {
            v.push((label.to_string(), id));
        }
    });
}

/// A select's keys, once its list is open: up and down move by one and wrap, page up and down
/// by ten, and letters jump to the first row starting with what was typed in the last second.
pub fn select_keys(ctx: &egui::Context, rows: &[(String, egui::Id)], popup: egui::Id) {
    if rows.is_empty() {
        return;
    }
    let n = rows.len() as i64;
    let at = rows.iter().position(|(_, id)| ctx.memory(|m| m.has_focus(*id))).map_or(-1, |a| a as i64);
    let mut to = None;
    ctx.input_mut(|i| {
        if take_key(i, egui::Modifiers::NONE, egui::Key::ArrowDown) {
            to = Some(if at + 1 >= n { 0 } else { at + 1 });
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::ArrowUp) {
            to = Some(if at <= 0 { n - 1 } else { at - 1 });
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::PageDown) {
            to = Some((at.max(0) + 10).min(n - 1));
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::PageUp) {
            to = Some((at.max(0) - 10).max(0));
        }
    });
    let typed: String = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect());
    if !typed.is_empty() {
        let key = egui::Id::new("select_typed");
        let (mut prefix, at_t): (String, Option<Instant>) = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
        if at_t.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            prefix.clear();
        }
        prefix.push_str(&typed.to_lowercase());
        to = rows.iter().position(|(l, _)| l.to_lowercase().starts_with(&prefix)).map(|i| i as i64).or(to);
        ctx.data_mut(|d| d.insert_temp(key, (prefix, Some(Instant::now()))));
    }
    if let Some(i) = to {
        ctx.memory_mut(|m| m.request_focus(rows[i as usize].1));
    }
    // Enter on a row presses it, which a keyboard press doesn't close the list for
    let pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
    if pressed && rows.iter().any(|(_, id)| ctx.memory(|m| m.has_focus(*id))) {
        egui::Popup::close_id(ctx, popup);
    }
}

/// Discord's mute durations, in seconds; None is until turned back on.
pub fn mute_menu(ui: &mut Ui) -> Option<Option<i64>> {
    let mut pick = None;
    for (label, secs) in [
        ("For 15 Minutes", Some(900)),
        ("For 1 Hour", Some(3600)),
        ("For 3 Hours", Some(10800)),
        ("For 8 Hours", Some(28800)),
        ("For 24 Hours", Some(86400)),
        ("Until I turn it back on", None),
    ] {
        if menu_item(ui, None, label, false) {
            pick = Some(secs);
        }
    }
    pick
}

pub const LEVELS: [&str; 3] = ["All Messages", "Only @mentions", "Nothing"];

/// The radio rows under a channel's Notification Settings. Some(None) goes back to the default.
pub fn level_menu(ui: &mut Ui, level: Option<u8>, inherited: u8, default_label: &str) -> Option<Option<u8>> {
    let mut pick = None;
    if menu_row(ui, None, default_label, Some(LEVELS[inherited.min(2) as usize]), false, Trail::Radio(level.is_none())).clicked() {
        pick = Some(None);
    }
    for (i, label) in LEVELS.iter().enumerate() {
        if menu_row(ui, None, label, None, false, Trail::Radio(level == Some(i as u8))).clicked() {
            pick = Some(Some(i as u8));
        }
    }
    pick
}

/// What sits at the right end of a menu row.
#[derive(PartialEq)]
pub enum Trail {
    None,
    Caret,
    Check(bool),
    Radio(bool),
    Image(String),
}

/// One row of discord's menus. Rows size the menu to the widest of them, 34 tall, or 36 with
/// an icon at either end, or 52 with a grey line under the label.
pub fn menu_row(ui: &mut Ui, icon: Option<icons::Icon>, label: &str, sub: Option<&str>, danger: bool, trail: Trail) -> egui::Response {
    let (resp, lead) = menu_row_at(ui, icon.is_some(), label, sub, danger, trail);
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, lead, if danger { pal().critical } else { pal().header_primary });
    }
    resp
}

/// A menu row that leaves its leading 20x20 for the caller to paint.
pub fn menu_row_at(ui: &mut Ui, lead: bool, label: &str, sub: Option<&str>, danger: bool, trail: Trail) -> (egui::Response, Rect) {
    let ink = if danger { pal().critical } else { pal().header_primary };
    let text = ui.painter().layout_no_wrap(label.to_string(), medium(14.0), ink);
    let sub = sub.map(|s| ui.painter().layout_no_wrap(s.to_string(), reg(12.0), pal().text_muted));
    let indent = if lead { 36.0 } else { 8.0 };
    let tail = if trail == Trail::None { 8.0 } else { 36.0 };
    let w = indent + text.size().x.max(sub.as_ref().map_or(0.0, |s| s.size().x)) + tail;
    let h = if sub.is_some() { 52.0 } else if lead || trail != Trail::None { 36.0 } else { 34.0 };
    let (r, resp) = ui.allocate_at_least(vec2(w, h), Sense::click());
    list_row(label, resp.id);
    // menus have no app to report to; the e2e driver finds rows here by label
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("menu_row", label)), r));
    let p = ui.painter();
    if resp.has_focus() {
        p.rect_filled(r, CornerRadius::same(4), if danger { crate::theme::CRITICAL_BG } else { crate::theme::MENU_HOVER });
    } else {
        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), if danger { crate::theme::CRITICAL_BG } else { crate::theme::MENU_HOVER }, 0.05);
    }
    let top = if sub.is_some() { r.top() + 8.0 } else { r.center().y - text.size().y / 2.0 };
    if let Some(s) = sub {
        p.galley(pos2(r.left() + indent, r.top() + 28.0), s, pal().text_muted);
    }
    p.galley(pos2(r.left() + indent, top), text, ink);
    let at = Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0));
    match trail {
        Trail::None => {}
        Trail::Caret => icons::draw(p, icons::Icon::ChevronRight, at, pal().text_muted),
        Trail::Check(true) => {
            p.rect_filled(at.shrink(1.0), CornerRadius::same(4), BLURPLE);
            icons::draw(p, icons::Icon::Check, at.shrink(3.0), Color32::WHITE);
        }
        Trail::Check(false) => {
            p.rect_stroke(at.shrink(1.0), CornerRadius::same(4), egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
        }
        Trail::Radio(true) => {
            p.circle_filled(at.center(), 9.0, BLURPLE);
            p.circle_filled(at.center(), 4.0, Color32::WHITE);
        }
        Trail::Image(url) => {
            egui::Image::from_uri(url).show_loading_spinner(false).paint_at(ui, at);
        }
        Trail::Radio(false) => {
            p.circle_stroke(at.center(), 8.5, egui::Stroke::new(1.5, pal().header_secondary));
        }
    }
    (resp, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(20.0, 20.0)))
}

pub fn menu_sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_at_least(vec2(0.0, 17.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(8.0), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
}

/// Discord's server tag chip after a name: badge and tag on a grey wash, 16 tall.
pub fn tag_chip(p: &egui::Painter, u: &crate::state::User, at: Pos2, fade: f32) -> Option<Rect> {
    let (tag, _, _) = u.tag.as_ref()?;
    let ink = pal().header_primary.gamma_multiply(fade);
    let g = p.layout_no_wrap(tag.clone(), semi(12.0), ink);
    let badge = u.tag_badge_url();
    let w = 8.0 + if badge.is_some() { 16.0 } else { 0.0 } + g.size().x;
    let r = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(w, 16.0));
    p.rect_filled(r, CornerRadius::same(4), Color32::from_rgba_unmultiplied(151, 151, 159, 61).gamma_multiply(fade));
    let mut x = r.left() + 4.0;
    if let Some(url) = badge {
        text::image_tint(p, &url, Rect::from_min_size(pos2(x, at.y - 7.0), vec2(14.0, 14.0)), Color32::WHITE.gamma_multiply(fade));
        x += 16.0;
    }
    p.galley(pos2(x, at.y - g.size().y / 2.0), g, ink);
    Some(r)
}

/// The blurple "APP" tag after a bot's name, with the check for verified apps.
pub fn app_tag(p: &egui::Painter, at: Pos2, verified: bool, fade: f32) -> Rect {
    let g = p.layout_no_wrap("APP".into(), semi(12.0), Color32::WHITE);
    let w = 8.0 + if verified { 16.0 } else { 0.0 } + g.size().x;
    let r = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(w, 16.0));
    p.rect_filled(r, CornerRadius::same(4), BLURPLE.gamma_multiply(fade.max(0.5)));
    let mut x = r.left() + 4.0;
    if verified {
        icons::draw(p, icons::Icon::Verified, Rect::from_min_size(pos2(x - 2.0, at.y - 8.0), vec2(16.0, 16.0)), Color32::WHITE);
        x += 14.0;
    }
    p.galley(pos2(x, at.y - g.size().y / 2.0), g, Color32::WHITE);
    r
}

pub fn channel_title(store: &Store, ch: Id) -> String {
    let Some(c) = store.channels.get(&ch) else { return String::new() };
    match c.kind {
        1 => c.recipients.first().map(|u| store.name_in(0, *u)).unwrap_or_else(|| "Unknown".into()),
        3 if c.name.is_empty() => c
            .recipients
            .iter()
            .map(|u| store.name_in(0, *u))
            .collect::<Vec<_>>()
            .join(", "),
        _ => c.name.clone(),
    }
}

fn key_down(vk: u8) -> bool {
    // high bit set means the key is held right now
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// `consume_key` treats extra modifiers as a match, so plain Enter would also
/// swallow Shift+Enter. Shortcuts here want the exact combination.
pub fn take_key(i: &mut egui::InputState, mods: egui::Modifiers, key: egui::Key) -> bool {
    let mut hit = false;
    i.events.retain(|e| {
        let m = matches!(e, egui::Event::Key { key: k, modifiers: em, pressed: true, .. } if *k == key && em.matches_exact(mods));
        hit |= m;
        !m
    });
    hit
}

/// Enter in a dialog: it goes ahead with the main button, unless Tab has put the keyboard on
/// some other widget, which then takes the key itself. A text field only lets it through when
/// `text_ok`, which is not for ones that type new lines.
pub fn enter_submits(ctx: &egui::Context, text_ok: bool) -> bool {
    if !ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift && !i.modifiers.command) {
        return false;
    }
    match ctx.memory(|m| m.focused()) {
        None => true,
        Some(id) => text_ok && egui::TextEdit::load_state(ctx, id).is_some(),
    }
}

/// Takes a typed character out of this frame's input, so nothing else types it.
pub fn take_text(i: &mut egui::InputState, s: &str) -> bool {
    let mut hit = false;
    i.events.retain(|e| {
        let m = matches!(e, egui::Event::Text(t) if t == s);
        hit |= m;
        !m
    });
    hit
}

/// Discord's channel list sits in the app frame: a hairline down its left and along its top
/// with a 12px rounded corner where they meet, the frame colour showing outside the curve, an
/// 8px strip of frame below the account card, and on onyx a line where the chat page starts.
fn side_edge(p: &egui::Painter, r: Rect) {
    let rad = 12.0;
    let c = pos2(r.left() + rad, r.top() + rad);
    let arc: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    let mut corner = egui::Mesh::default();
    corner.colored_vertex(r.left_top(), pal().bg_tertiary);
    for q in &arc {
        corner.colored_vertex(*q, pal().bg_tertiary);
    }
    for i in 1..arc.len() as u32 {
        corner.add_triangle(0, i, i + 1);
    }
    p.add(corner);
    p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 8.0), r.max), CornerRadius::ZERO, pal().bg_tertiary);
    let h = 0.5;
    let mut edge = vec![pos2(r.left() + h, r.bottom())];
    edge.extend(arc.iter().map(|q| *q + vec2(h, h)));
    edge.push(pos2(r.right(), r.top() + h));
    p.add(egui::Shape::line(edge, egui::Stroke::new(1.0, pal().frame_border)));
    if onyx() {
        p.vline(r.right() - h, r.y_range(), egui::Stroke::new(1.0, pal().border_subtle));
    }
}

/// ":)" and friends become emoji, but only as whole words, so urls and
/// code like `a:b` are left alone.
fn emoticons(text: &str) -> String {
    const MAP: [(&str, &str); 14] = [
        (":)", "🙂"), (":-)", "🙂"), (":(", "🙁"), (":-(", "🙁"), (":D", "😃"), (":-D", "😃"), (";)", "😉"),
        (";-)", "😉"), (":P", "😛"), (":p", "😛"), ("<3", "❤️"), (":O", "😮"), (":o", "😮"), (":'(", "😢"),
    ];
    if text.contains("```") {
        return text.to_string();
    }
    text.split(' ')
        .map(|w| MAP.iter().find(|(k, _)| *k == w).map(|(_, e)| e.to_string()).unwrap_or_else(|| w.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}
