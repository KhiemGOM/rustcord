mod accounts;
pub mod applications;
mod actions;
mod attach;
mod active;
mod addapp;
pub mod activity;
mod autoscroll;
mod boosts;
mod botmodal;
pub mod buy;
mod call;
mod callbar;
mod callfx;
mod stagebar;
mod stagemusic;
mod chat;
mod clips;
mod clips_gallery;
mod clips_trim;
mod composer;
mod create;
mod cosmo;
mod cstatus;
mod crop;
mod directory;
mod dmpick;
mod discover;
mod embedplay;
mod eyedrop;
mod e2e;
mod events;
mod favs;
pub mod failed;
mod feedabout;
mod folders;
mod follow;
mod forum;
mod forumbar;
mod gate;
mod guide;
mod guildmodal;
mod friends;
pub mod icons;
mod interact;
mod keys;
mod kickban;
mod prune;
#[cfg(test)]
mod locks;
mod live;
mod lurk;
mod links;
mod login;
mod mdpreview;
mod members;
mod modview;
mod notifs;
mod now;
mod motion;
mod palette;
mod newserver;
mod menus;
mod picker;
mod player;
mod plus;
mod poll;
mod blocked;
mod pollvotes;
mod remind;
mod sourcepop;
pub mod viewas;
mod uploading;
mod chatinvite;
mod invitecard;
mod popups;
mod profed;
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
mod selectmenu;
mod settings;
mod share;
mod soundboard;
mod stage;
mod tile;
mod side;
mod sidebar_view;
mod tagpop;
mod textfile;
mod themes;
mod thread;
mod spelling;
mod splash;
mod when;
pub mod text;
mod titlebar;
mod topic;
mod wheel;
mod userdlg;
mod usermenu;
mod nav;
mod prefs;
mod restmsg;
mod pump;
mod frame;
mod widgets;
pub(crate) use widgets::avatar;
pub(crate) use widgets::status_dot;
pub(crate) use widgets::badge;
pub(crate) use widgets::badge_in;
pub(crate) use widgets::button;
pub(crate) use widgets::icon_button;
pub(crate) use widgets::toggle_button;
pub(crate) use widgets::menu_item;
pub(crate) use widgets::Confirm;
pub(crate) use widgets::grey_button;
mod popmenu;
pub(crate) use popmenu::bolded;
pub(crate) use popmenu::float_menu;
pub(crate) use popmenu::menu_closing;
pub(crate) use popmenu::menu_body;
pub(crate) use popmenu::rows_of;
pub(crate) use popmenu::option;
pub(crate) use popmenu::list_row;
pub(crate) use popmenu::select_keys;
pub(crate) use popmenu::mute_menu;
pub(crate) use popmenu::LEVELS;
pub(crate) use popmenu::level_menu;
pub(crate) use popmenu::Trail;
pub(crate) use popmenu::menu_row;
pub(crate) use popmenu::menu_row_at;
pub(crate) use popmenu::menu_sep;
pub(crate) use popmenu::tag_chip;
pub(crate) use popmenu::app_tag;
pub(crate) use popmenu::channel_title;
mod inputs;
use inputs::*;
pub(crate) use inputs::take_key;
pub(crate) use inputs::enter_submits;
pub(crate) use inputs::take_text;

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
    // the emoji of the status being set
    Status,
    React(Id, Id),
    // a slot of the quick reactions
    Quick(usize),
    // the emoji of an answer in the poll being made
    Poll(usize),
}

const SAVE_FAILED: &str = "Failed to update your settings. Please try again.";
pub const SEND_FAILED: &str = "Your message failed to send!";

pub struct App {
    pub net: Net,
    rx: Receiver<Ev>,
    pub store: Tracked,
    pub ui: UiState,
    favs: favs::Favs,
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
    pub interactions: interact::State,
    pub slash: Map<Id, composer::slash::Slash>,
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
    pub packs: Vec<picker::Pack>,
    pub packs_at: Option<Instant>,
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
    pub guide_raw: Map<Id, serde_json::Value>,
    pub guild_notifs: Option<Id>,
    pub bar: Option<(Id, Rect)>,
    pub toast: Option<(String, Instant)>,
    // an error toast's full text, which shift+click opens
    pub toast_detail: Option<String>,
    pub fail: Option<(Vec<(String, String)>, String)>,
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
    pub chat_invites: chatinvite::Cache,
    pub unblocked: Set<Id>,
    pub votes: Option<pollvotes::Votes>,
    pub remind_custom: Option<remind::Custom>,
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
    pub buy: Option<buy::Buy>,
    pub server_shop: Option<buy::server_shop::ServerShop>,
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
    pub embed: Option<embedplay::Play>,
    pub embed_at: Option<(embedplay::Key, egui::Rect, egui::Rect)>,
    pub events: Option<events::Events>,
    pub joins: applications::Joins,
    pub quests: Option<quests::Quests>,
    pub shop: Option<shop::Shop>,
    pub friends_tab: u8,
    pub add_friend: String,
    pub status_until: Option<i64>,
    pub add_app: Option<addapp::AddApp>,
    pub add_friend_note: Option<(bool, String)>,
    pub post: Option<forum::Post>,
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
    pub themes: themes::State,
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
    pub cosmo: cosmo::Store,
    pub keyed: Map<String, Option<Result<serde_json::Value, String>>>,
    pub profed: Option<profed::Editor>,
    pub new_thread: Option<popups::NewThread>,
    pub poll: Option<poll::Draft>,
    pub launcher: Option<plus::Launcher>,
    pub follow: Option<follow::Follow>,
    pub modify: Option<attach::Modify>,
    pub group_pick: Option<dmpick::Pick>,
    pub topic: Option<Id>,
    pub feed_about: bool,
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
    pub source_pop: Option<sourcepop::Pop>,
    pub sources: Map<(bool, Id), serde_json::Value>,
    pub top_emojis: Map<Id, Vec<Id>>,
    // a link waiting on "Leaving Discord", and whether to trust its site from now on
    pub leaving: Option<(String, bool)>,
    // the Ctrl+/ sheet, the Raging Demon it can set off, and breathing with Wumpus
    pub combos: Option<keys::Combos>,
    pub demon: Option<Instant>,
    pub breathe: Option<keys::Breathe>,
    // Timeout for a member: who, in which server, the length picked and the reason
    pub timeout: Option<popups::Timeout>,
    pub kickban: Option<kickban::Dialog>,
    pub prune: Option<prune::Dialog>,
    // the server left for the dms, which Ctrl+Alt+→ goes back to
    pub prev_guild: Option<Id>,
    // the open channel when it's scrolled well back from the newest messages
    pub far_up: Option<Id>,
    // quick clicks on the home button so far, and when the last was; the ディスコード tooltip
    pub home_clicks: (u32, Option<Instant>),
    pub discodo_at: Option<Instant>,
    pub gifs: Option<popups::Gifs>,
    pub gif_load: Option<String>,
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
    // members we've asked the gateway for this connection; a cached member is as old as the last session
    members_asked: Set<(Id, Id)>,
    // the taskbar badge's count, and when unreads were last looked over for its dot
    badge: Option<i32>,
    // when the activity feed is next fetched
    feed_at: Option<Instant>,
    synced: Option<crate::proto::Account>,
    // images drawn lately, so the rest can be let go now and then
    textures: std::sync::Arc<crate::imgcache::Textures>,
    sweep_at: Instant,
    system: crate::winsys::Prefs,
    system_at: f64,
    // when the window stopped being used (hidden or out of focus), and whether its memory went
    // back since; and whether the pages startup left behind have gone
    quiet: Option<(Instant, bool)>,
    startup_trims: usize,
    ui_priority: Option<bool>,
    born: Instant,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        t0: Instant,
        snap: std::thread::JoinHandle<Option<(Store, UiState)>>,
    ) -> Self {
        crate::winsys::show_early_over();
        let ctx = cc.egui_ctx.clone();
        // egui's own ctrl +/- zoom goes by 10%; ours steps through discord's levels
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            // once an image is on the gpu, its decoded pixels and bytes are only a second copy
            o.reduce_texture_memory = true;
        });
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
            interactions: Default::default(),
            slash: Map::default(),
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
            packs: Vec::new(),
            packs_at: None,
            menu: None,
            confirm: None,
            forward: None,
            reactors: None,
            report: None,
            guides: Map::default(),
            guide_raw: Map::default(),
            user_menu: None,
            menu_from: Default::default(),
            user_dialog: None,
            guild_notifs: None,
            bar: None,
            toast: None,
            toast_detail: None,
            fail: None,
            marker: None,
            marker_bar: false,
            autoscroll: None,
            wheel: wheel::Wheel::new(),
            zoom_acc: 0.0,
            focus_composer: true,
            rects: Map::default(),
            voice: Default::default(),
            e2e: e2e::Run::from_env(t0),
            favs: Default::default(),
            win: None,
            settings: cache::settings(),
            settings_page: None,
            themes: Default::default(),
            invite: None,
            invite_card: Default::default(),
            chat_invites: Default::default(),
            unblocked: Set::default(),
            votes: None,
            remind_custom: None,
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
            embed: None,
            embed_at: None,
            events: None,
            joins: Default::default(),
            quests: None,
            shop: None,
            buy: None,
            server_shop: None,
            friends_tab: 0,
            add_friend: String::new(),
            status_until: None,
            add_app: None,
            add_friend_note: None,
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
            cosmo: Default::default(),
            keyed: Map::default(),
            profed: None,
            new_thread: None,
            poll: None,
            launcher: None,
            follow: None,
            modify: None,
            group_pick: None,
            topic: None,
            feed_about: false,
            cam_on_join: None,
            notice: None,
            crop: None,
            status_text: None,
            sticker_q: None,
            source_pop: None,
            sources: Map::default(),
            top_emojis: Map::default(),
            leaving: None,
            combos: None,
            demon: None,
            breathe: None,
            timeout: None,
            kickban: None,
            prune: None,
            prev_guild: None,
            far_up: None,
            home_clicks: (0, None),
            discodo_at: None,
            gifs: None,
            gif_load: None,
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
            members_asked: Default::default(),
            badge: None,
            feed_at: None,
            synced: None,
            textures,
            sweep_at: Instant::now(),
            system: crate::winsys::preferences(),
            system_at: 0.0,
            quiet: None,
            startup_trims: 0,
            ui_priority: None,
            born: t0,
        };
        app.apply_settings(&ctx);
        let (htx, hrx) = std::sync::mpsc::channel();
        crate::tray::start(ctx.clone(), htx.clone());
        crate::hotkeys::start(ctx.clone(), htx);
        app.hotkey_rx = Some(hrx);
        crate::procs::start(ctx.clone());
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

}

