mod call;
mod chat;
mod composer;
mod e2e;
mod forum;
mod friends;
pub mod icons;
mod login;
mod members;
mod picker;
mod popups;
mod rail;
mod settings;
mod side;
pub mod text;
mod titlebar;

use egui::{Color32, CornerRadius, Frame, Pos2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::cache::{self, Settings, UiState};
use crate::net::{Cmd, Conn, Ev, Net, Tag};
use crate::state::{Id, Message, Page, ReactEmoji, Status, Store};
use crate::theme::*;

pub enum Pick {
    Composer,
    React(Id, Id),
}

pub struct App {
    pub net: Net,
    rx: Receiver<Ev>,
    pub store: Store,
    pub ui: UiState,
    pub token: Option<String>,
    pub conn: Conn,
    pub login: login::Form,
    pub drafts: HashMap<Id, String>,
    pub files: Vec<composer::Upload>,
    pub ac_sel: usize,
    pub ac_open: bool,
    pub ac_dismissed: Option<String>,
    pub aliases: HashMap<Id, Vec<(String, String)>>,
    pub commands: HashMap<Id, composer::Commands>,
    v_down: bool,
    fonts_rx: Option<Receiver<egui::FontDefinitions>>,
    cjk_loaded: bool,
    pub reply: Option<Id>,
    pub editing: Option<(Id, String)>,
    pub last_typing: Option<(Id, Instant)>,
    pub heights: HashMap<(Id, bool), f32>,
    pub heights_w: f32,
    pub offsets: HashMap<Id, f32>,
    // channels whose message list sat at the bottom last frame
    pub pinned: HashSet<Id>,
    pub scroll_fix: Option<(Id, f32)>,
    pub revealed: HashSet<Id>,
    pub picker: Option<(Pick, Pos2)>,
    // the sticker picker, anchored where it opened
    pub stickers: Option<Pos2>,
    pub menu: Option<(Id, Id, Pos2)>,
    // a person's right-click menu: user, server, the dm it came from, where
    pub user_menu: Option<(Id, Id, Option<Id>, Pos2)>,
    pub confirm_delete: Option<(Id, Id)>,
    pub confirm: Option<Confirm>,
    pub forward: Option<popups::Forward>,
    pub guild_notifs: Option<Id>,
    pub bar: Option<(Id, Rect)>,
    pub toast: Option<(String, Instant)>,
    pub marker: Option<(Id, Id)>,
    pub focus_composer: bool,
    pub rects: HashMap<String, Rect>,
    pub settings: Settings,
    pub settings_page: Option<settings::Page>,
    pub invite: Option<String>,
    // "invite people" from a channel row: the channel, and the code once discord made one
    pub made_invite: Option<(Id, Option<String>)>,
    pub switcher: Option<String>,
    pub profile: Option<(Id, Id, Pos2)>,
    pub profiles: HashMap<Id, serde_json::Value>,
    pub pins: Option<(Id, Option<Vec<Message>>)>,
    pub search: Option<popups::Search>,
    pub friends: bool,
    pub friends_tab: u8,
    pub add_friend: String,
    pub post: Option<(String, String)>,
    pub scroll_to: Option<(Id, Id)>,
    pub scroll_tries: u8,
    pub flash: Option<(Id, Instant)>,
    pub viewer: Option<(String, String, u32, u32)>,
    pub remote: HashMap<&'static str, Option<Result<serde_json::Value, String>>>,
    hotkey_rx: Option<Receiver<(String, bool)>>,
    obs_seen: bool,
    capture_hidden: bool,
    pub sform: settings::Forms,
    // a channel the user just marked unread shouldn't get re-acked on the next frame
    pub hold_ack: Option<Id>,
    pub reply_ping: bool,
    pub gifs: Option<popups::Gifs>,
    pub nav: titlebar::Nav,
    pub inbox: Option<titlebar::Inbox>,
    title_count: Option<u32>,
    pub voice: crate::voice::VoiceUi,
    e2e: Option<e2e::Run>,
    win: Option<[f32; 2]>,
    frames: u64,
    save_at: Option<Instant>,
    synced: Option<crate::proto::Account>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        t0: Instant,
        snap: std::thread::JoinHandle<Option<(Store, UiState)>>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        crate::theme::install(&ctx);
        egui_extras::install_image_loaders(&ctx);
        ctx.add_image_loader(std::sync::Arc::new(crate::gif::GifLoader::default()));
        let (net, rx) = Net::new(ctx.clone());
        crate::fonts::fetch(&net.rt, ctx.clone());
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
            store,
            ui,
            token,
            conn: Conn::Connecting,
            login: Default::default(),
            drafts: HashMap::new(),
            files: Vec::new(),
            ac_sel: 0,
            ac_open: false,
            ac_dismissed: None,
            aliases: HashMap::new(),
            commands: HashMap::new(),
            v_down: false,
            fonts_rx: None,
            cjk_loaded: false,
            reply: None,
            editing: None,
            last_typing: None,
            heights: HashMap::new(),
            heights_w: 0.0,
            offsets: HashMap::new(),
            pinned: HashSet::new(),
            scroll_fix: None,
            revealed: HashSet::new(),
            picker: None,
            stickers: None,
            menu: None,
            confirm_delete: None,
            confirm: None,
            forward: None,
            user_menu: None,
            guild_notifs: None,
            bar: None,
            toast: None,
            marker: None,
            focus_composer: true,
            rects: HashMap::new(),
            voice: Default::default(),
            e2e: e2e::Run::from_env(t0),
            win: None,
            settings: cache::settings(),
            settings_page: None,
            invite: None,
            made_invite: None,
            switcher: None,
            profile: None,
            profiles: HashMap::new(),
            pins: None,
            search: None,
            friends: false,
            friends_tab: 0,
            add_friend: String::new(),
            post: None,
            scroll_to: None,
            scroll_tries: 0,
            flash: None,
            viewer: None,
            remote: HashMap::new(),
            hotkey_rx: None,
            obs_seen: false,
            capture_hidden: false,
            sform: Default::default(),
            hold_ack: None,
            reply_ping: true,
            gifs: None,
            nav: Default::default(),
            inbox: None,
            title_count: None,
            frames: 0,
            save_at: None,
            synced: None,
        };
        app.apply_settings(&ctx);
        let (htx, hrx) = std::sync::mpsc::channel();
        crate::hotkeys::start(ctx.clone(), htx);
        app.hotkey_rx = Some(hrx);
        crate::winsys::watch_streaming(ctx.clone());
        app.obs_seen = crate::winsys::OBS_RUNNING.load(std::sync::atomic::Ordering::Relaxed);
        if app.settings.start_minimized && std::env::args().any(|a| a == "--startup") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
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
        self.ui.guild = g;
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
        let guild = c.guild;
        let forum = c.is_forum();
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
        self.nav.visit(titlebar::Loc::Chan(ch, None));
        self.ui.chan.insert(guild, ch);
        self.ui.recent.retain(|x| *x != ch);
        self.ui.recent.insert(0, ch);
        self.ui.recent.truncate(15);
        self.focus_composer = true;
        let acked = self.store.reads.get(&ch).map(|r| r.last_acked).unwrap_or(0);
        self.marker = (c.last_msg > acked && acked != 0).then_some((ch, acked));
        if forum {
            let path = format!("/channels/{ch}/threads/search?archived=true&sort_by=last_message_time&sort_order=desc&limit=25&offset=0");
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
        }
    }

    // history from REST has no member objects; ask the gateway for the authors we haven't seen
    fn request_members(&mut self, ch: Id) {
        let Some(guild) = self.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0) else { return };
        let known = self.store.members.get(&guild);
        let mut ids: Vec<String> = Vec::new();
        for m in self.store.msgs.get(&ch).map(|e| e.list.as_slice()).unwrap_or_default() {
            let id = m.author.to_string();
            if !known.is_some_and(|k| k.contains_key(&m.author)) && !ids.contains(&id) {
                ids.push(id);
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
        let text = self.drafts.remove(&ch).unwrap_or_default();
        let mut text = self.resolve_aliases(ch, text.trim());
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        text = self.convert_emoji(guild, &text);
        if self.settings.emoticons {
            text = emoticons(&text);
        }
        self.aliases.remove(&ch);
        let files: Vec<(PathBuf, String)> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|u| {
                let name = if u.spoiler { format!("SPOILER_{}", u.name) } else { u.name };
                (u.path, name)
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
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply, files, stickers: Vec::new() });
        self.hold_ack = None;
        self.last_typing = None;
    }

    pub fn react(&mut self, ch: Id, msg: Id, em: ReactEmoji, add: bool) {
        if add {
            self.ui.quick.retain(|q| *q != em);
            self.ui.quick.insert(0, em.clone());
            self.ui.quick.truncate(3);
        }
        self.net.send(Cmd::React { channel: ch, id: msg, emoji: em.api_name(), add });
        self.store.react_local(ch, msg, em, add);
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

    /// Alt+↑/↓ walks channels, Alt+Shift+↑/↓ jumps between unread ones,
    /// Ctrl+E opens emoji, Ctrl+Shift+U uploads.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let (up, down, uup, udown, emoji, upload) = ctx.input_mut(|i| {
            (
                take_key(i, Modifiers::ALT, Key::ArrowUp),
                take_key(i, Modifiers::ALT, Key::ArrowDown),
                take_key(i, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowUp),
                take_key(i, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowDown),
                take_key(i, Modifiers::COMMAND, Key::E),
                take_key(i, Modifiers::COMMAND | Modifiers::SHIFT, Key::U),
            )
        });
        if up || down || uup || udown {
            let list: Vec<Id> = match self.ui.guild {
                Some(g) => side::tree(&self.store, g)
                    .into_iter()
                    .flat_map(|(_, k)| k)
                    .filter(|c| c.is_text() || c.is_forum())
                    .map(|c| c.id)
                    .collect(),
                None => self.store.dms.clone(),
            };
            let at = self.cur().and_then(|c| list.iter().position(|x| *x == c));
            let n = list.len();
            let target = if uup || udown {
                let order: Vec<usize> = if udown { (1..=n).collect() } else { (1..=n).rev().collect() };
                order
                    .into_iter()
                    .map(|k| (at.unwrap_or(0) + k) % n.max(1))
                    .find(|i| self.store.channels.get(&list[*i]).is_some_and(|c| self.store.unread(c)))
            } else if n > 0 {
                Some(match (at, down) {
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                    (None, _) => 0,
                })
            } else {
                None
            };
            if let Some(i) = target {
                self.open(list[i]);
            }
        }
        if emoji && let Some(r) = self.rects.get("composer").copied() {
            self.picker = Some((Pick::Composer, r.right_top() - vec2(420.0, 440.0)));
        }
        if upload && let Some(files) = rfd::FileDialog::new().pick_files() {
            self.files.extend(files.into_iter().map(composer::Upload::new));
        }
    }

    // join/leave chimes for people entering or leaving the call you're in, and the ring
    fn voice_cues(&self, t: &str, d: &serde_json::Value) {
        let me = self.store.me;
        match t {
            "VOICE_STATE_UPDATE" => {
                let Some(mine) = self.voice.chan else { return };
                let uid = crate::state::sid(&d["user_id"]);
                if uid == me {
                    return;
                }
                let gid = crate::state::sid(&d["guild_id"]);
                let was = self.store.guilds.get(&gid).and_then(|g| g.voice.get(&uid).copied()).unwrap_or(0);
                let now = crate::state::sid(&d["channel_id"]);
                if now == mine && was != mine {
                    crate::sound::cue("join");
                } else if was == mine && now != mine {
                    crate::sound::cue("leave");
                }
            }
            "CALL_CREATE" | "CALL_UPDATE" => {
                let ringing = d["ringing"].as_array().is_some_and(|r| r.iter().any(|u| crate::state::sid(u) == me));
                if ringing && self.voice.chan.is_none() {
                    crate::sound::cue("ring");
                }
            }
            _ => {}
        }
    }

    pub fn mark_unread(&mut self, ch: Id, id: Id) {
        // ack the message before this one so it shows as the first unread
        let prev = self.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.id < id)).map(|m| m.id).unwrap_or(id - 1);
        self.store.reads.entry(ch).or_default().last_acked = prev;
        self.rest("POST", format!("/channels/{ch}/messages/{prev}/ack"), Some(json!({ "manual": true, "token": null })), Tag::Quiet);
        self.marker = Some((ch, prev));
        self.hold_ack = Some(ch);
    }

    pub fn set_status(&mut self, s: &str) {
        self.store.presence.insert(self.store.me, Status::parse(s));
        self.net.send(Cmd::Gateway(json!({ "op": 3, "d": { "status": s, "since": 0, "activities": [], "afk": false } })));
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
        if theme == crate::theme::Theme::Sync {
            self.settings.theme = theme;
        }
        self.heights.clear();
        self.save_settings(ctx);
    }

    /// Push every setting that lives outside `App` (theme, zoom, sounds, hotkeys...) to where it acts.
    fn apply_settings(&self, ctx: &egui::Context) {
        let s = &self.settings;
        ctx.set_zoom_factor(s.zoom);
        crate::markdown::CLOCK24.store(s.clock24, std::sync::atomic::Ordering::Relaxed);
        crate::theme::set_reduced_motion(s.reduced_motion);
        crate::theme::set_view(crate::theme::View {
            underline_links: s.underline_links,
            animate_emoji: s.animate_emoji,
            hide_invites: s.streamer && s.hide_invites,
        });
        crate::theme::set_theme(ctx, s.theme);
        crate::theme::style(ctx);
        crate::sound::configure(s.sounds && !(s.streamer && s.streamer_no_sounds), &s.muted_sounds);
        crate::hotkeys::set(&s.keybinds);
        crate::api::set_locale(&s.locale);
        self.voice.configure(s);
    }

    fn notify(&mut self, d: &serde_json::Value, ctx: &egui::Context) {
        let m = self.store.parse_message(d);
        if m.author == self.store.me || !self.settings.notify || (self.settings.streamer && self.settings.streamer_no_notifications) {
            return;
        }
        let Some(c) = self.store.channels.get(&m.channel) else { return };
        if self.store.muted.contains(&c.id) || self.store.muted.contains(&c.guild) {
            return;
        }
        let dm = c.guild == 0;
        if !(self.store.mentions_me(&m) || (dm && self.settings.notify_all_dms)) {
            return;
        }
        let focused = ctx.input(|i| i.focused);
        if focused && self.cur() == Some(c.id) {
            return;
        }
        let who = self.store.name_in(c.guild, m.author);
        let title = match self.store.guilds.get(&c.guild) {
            Some(g) => format!("{who} (#{}, {})", c.name, g.name),
            None if c.kind == 3 => format!("{who} ({})", channel_title(&self.store, c.id)),
            None => who,
        };
        let mut body: String = m.content.chars().take(200).collect();
        if body.is_empty() && !m.attachments.is_empty() {
            body = "Uploaded a file".into();
        }
        crate::sound::cue("message");
        if let Some(e) = &mut self.e2e {
            e.mark_once("notified");
        } else {
            std::thread::spawn(move || {
                let _ = notify_rust::Notification::new().appname("Fastcord").summary(&title).body(&body).show();
            });
        }
        if !focused && self.settings.flash {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
    }

    /// Settings that react to the outside world every frame: system theme,
    /// ctrl+wheel zoom, global keybinds, OBS starting, capture exclusion.
    fn os_settings(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.settings.theme == crate::theme::Theme::Sync {
            crate::theme::set_theme(ctx, crate::theme::Theme::Sync);
        }
        let z = ctx.zoom_factor();
        if (z - self.settings.zoom).abs() > 0.001 {
            self.settings.zoom = z;
            cache::set_settings(&self.settings);
        }
        let events: Vec<(String, bool)> = self.hotkey_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for (action, pressed) in events {
            if !pressed {
                continue;
            }
            match action.as_str() {
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
                _ => {}
            }
        }
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
        let mut n: u32 = self
            .store
            .reads
            .iter()
            .filter(|(ch, _)| {
                self.store.channels.get(ch).is_some_and(|c| !self.store.muted.contains(&c.id) && !self.store.muted.contains(&c.guild))
            })
            .map(|(_, r)| r.mentions)
            .sum();
        n += self.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
        if !self.settings.badge {
            n = 0;
        }
        if self.title_count != Some(n) {
            self.title_count = Some(n);
            let t = if n > 0 { format!("({n}) Fastcord") } else { "Fastcord".into() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
    }

    fn on_rest(&mut self, tag: Tag, res: Result<serde_json::Value, String>) {
        if let (Tag::Remote(k), Err(e)) = (&tag, &res) {
            return settings::on_remote(self, k, Err(e.clone()));
        }
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                match tag {
                    Tag::Pins(_) => self.pins = None,
                    Tag::Inbox => self.inbox = None,
                    _ => {}
                }
                return self.toast(e);
            }
        };
        match tag {
            Tag::Pins(ch) => {
                let list: Vec<Message> = v.as_array().into_iter().flatten().map(|m| self.store.parse_message(m)).collect();
                if self.pins.as_ref().is_some_and(|(c, _)| *c == ch) {
                    self.pins = Some((ch, Some(list)));
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
            Tag::Inbox => {
                let list: Vec<Message> = v.as_array().into_iter().flatten().map(|m| self.store.parse_message(m)).collect();
                if let Some(i) = &mut self.inbox {
                    i.mentions = Some(list);
                }
            }
            Tag::Forum(f) => self.store.load_forum(f, &v),
            Tag::Invite => {
                let name = v["guild"]["name"].as_str().unwrap_or("the server").to_string();
                self.toast(format!("Joined {name}"));
            }
            Tag::MadeInvite => {
                if let (Some((_, code)), Some(c)) = (&mut self.made_invite, v["code"].as_str()) {
                    *code = Some(c.to_string());
                }
            }
            Tag::OpenDm => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.ui.guild = None;
                self.open(id);
            }
            Tag::Friend => self.toast("Friend request sent."),
            Tag::Commands(scope) => self.set_commands(scope, &v),
            Tag::Gifs(q) => self.set_gifs(q, &v),
            Tag::GifCats => self.set_gif_cats(&v),
            Tag::Remote(k) => settings::on_remote(self, k, Ok(v)),
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
        let dir = std::env::temp_dir().join("fastcord");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("image-{}.png", chrono::Utc::now().timestamp_millis()));
        if buf.save(&path).is_ok() {
            let mut u = composer::Upload::new(path);
            u.name = "image.png".into();
            self.files.push(u);
            self.focus_composer = true;
        }
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    pub fn logout(&mut self) {
        self.voice.leave(&self.net);
        self.net.send(Cmd::Stop);
        cache::set_token(None);
        self.token = None;
        self.store = Store::default();
        self.ui = UiState::default();
        self.nav = Default::default();
        self.inbox = None;
    }

    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Dispatch(t, d) => {
                    if t == "MESSAGE_CREATE" {
                        self.notify(&d, ctx);
                    }
                    if t == "INTERACTION_FAILURE" {
                        self.toast("The application did not respond.");
                    }
                    if t == "READY" {
                        if let Some(e) = &mut self.e2e {
                            e.mark("ready");
                        }
                        self.save_at = Some(Instant::now() + Duration::from_secs(5));
                    }
                    self.voice_cues(&t, &d);
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
                Ev::SendFailed { channel, nonce, err } => {
                    self.store.fail_pending(channel, &nonce);
                    self.toast(format!("Message failed to send: {err}"));
                }
                Ev::Conn(c) => self.conn = c,
                Ev::AuthFailed => {
                    self.logout();
                    self.login.err = Some("Your session expired. Log in again.".into());
                }
                Ev::LoggedIn(t) => {
                    cache::set_token(Some(&t));
                    self.net.send(Cmd::Start(t.clone()));
                    self.token = Some(t);
                    self.login = Default::default();
                }
                Ev::NeedMfa(ticket) => {
                    self.login.busy = false;
                    self.login.mfa = Some(ticket);
                }
                Ev::LoginError(e) => {
                    self.login.busy = false;
                    self.login.err = Some(e);
                }
                Ev::Error(e) | Ev::Toast(e) => self.toast(e),
                Ev::Rest { tag, res } => self.on_rest(tag, res),
                Ev::Voice(e) => {
                    if let Some(err) = self.voice.event(e) {
                        self.toast(format!("Voice disconnected: {err}"));
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
        if self.hold_ack == Some(ch) {
            return;
        }
        self.hold_ack = None;
        let Some(last) = self.store.channels.get(&ch).map(|c| c.last_msg) else { return };
        let r = self.store.reads.entry(ch).or_default();
        if last > r.last_acked || r.mentions > 0 {
            r.last_acked = last.max(r.last_acked);
            r.mentions = 0;
            if last != 0 {
                self.net.send(Cmd::Ack { channel: ch, id: last });
            }
        }
    }

    fn overlays(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some((text, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                let text = text.clone();
                egui::Area::new("toast".into())
                    .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -90.0))
                    .order(egui::Order::Tooltip)
                    .show(&ctx, |ui| {
                        Frame::new()
                            .fill(pal().bg_floating)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(16, 10))
                            .show(ui, |ui| ui.label(egui::RichText::new(text).color(pal().text_normal).font(reg(14.0))));
                    });
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if let Some((ch, id)) = self.confirm_delete {
            let mut close = false;
            let modal = egui::Modal::new("delete".into())
                .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
                .show(&ctx, |ui| {
                    ui.set_width(440.0);
                    ui.label(egui::RichText::new("Delete Message").font(semi(20.0)).color(pal().header_primary));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Are you sure you want to delete this message?").color(pal().text_normal));
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if button(ui, "Delete", RED, 96.0).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                self.net.send(Cmd::Delete { channel: ch, id });
                                if let Some(m) = self.store.msgs.get_mut(&ch) {
                                    m.list.retain(|m| m.id != id);
                                }
                                close = true;
                            }
                            if ui.add(egui::Label::new(egui::RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            if close || modal.should_close() {
                self.confirm_delete = None;
            }
        }
        if let Some(c) = &self.confirm {
            let (mut close, mut go) = (false, false);
            let modal = egui::Modal::new("confirm".into())
                .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
                .show(&ctx, |ui| {
                    ui.set_width(440.0);
                    ui.label(egui::RichText::new(&c.title).font(semi(20.0)).color(pal().header_primary));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(&c.body).color(pal().text_normal));
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let w = ui.painter().layout_no_wrap(c.button.to_string(), medium(14.0), Color32::WHITE).size().x + 32.0;
                            go = button(ui, c.button, RED, w.max(96.0)).clicked();
                            if ui.add(egui::Label::new(egui::RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            if go && let Some(c) = self.confirm.take() {
                (c.run)(self);
            } else if close || modal.should_close() {
                self.confirm = None;
            }
        }
        if let Some(gid) = self.guild_notifs {
            let name = self.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
            let default = self.store.guilds.get(&gid).map(|g| g.default_notif).unwrap_or(0);
            let level = self.store.levels.get(&gid).copied().unwrap_or(default);
            let muted = self.store.muted.contains(&gid);
            let (mut pick, mut mute) = (None, None);
            let modal = egui::Modal::new("guild_notifs".into())
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
        picker::show(self, &ctx);
        picker::stickers(self, &ctx);
        chat::context_menu(self, &ctx);
        popups::user_menu(self, &ctx);
        popups::show(self, &ctx);
        popups::viewer(self, &ctx);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.pump(&ctx);
        self.os_settings(&ctx, frame);
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
            egui::Panel::left("rail")
                .exact_size(72.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(pal().bg_tertiary))
                .show(ui, |ui| rail::show(self, ui));
            // discord: 375 wide with the rail and its 1px frame line, draggable between 264 and 432
            let side_w = if self.ui.side_w > 0.0 { self.ui.side_w } else { 302.0 };
            let r = egui::Panel::left("side")
                .default_size(side_w)
                .size_range(191.0..=(w - 72.0 - 420.0).clamp(191.0, 359.0))
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::new().fill(pal().bg_primary))
                .show(ui, |ui| side::show(self, ui));
            side_edge(ui.painter(), r.response.rect);
            if let Some(w) = self.dragged(&ctx, r.response.rect.width(), side_w) {
                self.ui.side_w = w;
            }
            egui::CentralPanel::default()
                .frame(Frame::new().fill(pal().bg_primary))
                .show(ui, |ui| chat::show(self, ui));
            self.overlays(ui);
            self.update_title(&ctx);
            if !settings_open {
                self.ack_current(&ctx);
                if ctx.input_mut(|i| take_key(i, egui::Modifiers::COMMAND, egui::Key::K)) {
                    self.switcher = Some(String::new());
                }
                self.shortcuts(&ctx);
            }
            if self.store.prune_typing() {
                ctx.request_repaint_after(Duration::from_millis(50));
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

        if self.save_at.is_some_and(|t| Instant::now() >= t) {
            self.save_at = None;
            let took = Instant::now();
            cache::save(&self.store, &self.ui);
            if let Some(e) = &mut self.e2e {
                e.cost("cache_save", took.elapsed());
            }
        }
        if let Some(mut e) = self.e2e.take() {
            e.step(self, &ctx);
            e.frame_time(started.elapsed());
            self.e2e = Some(e);
        }
        self.frames += 1;
        self.win = ctx.input(|i| i.viewport().inner_rect).map(|r| [r.width(), r.height()]);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(e) = &mut self.e2e {
            e.inject(raw);
        }
        // dropping files on the window attaches them to the next message
        for f in raw.dropped_files.drain(..) {
            self.files.push(composer::Upload::new(f.path().to_path_buf()));
        }
        // egui-winit swallows ctrl+v when the clipboard holds no text, so watch the key ourselves
        let v = raw.focused && key_down(0x11) && key_down(b'V');
        if v && !self.v_down && !raw.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
            self.paste_files();
        }
        self.v_down = v;
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        pal().bg_tertiary.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.token.is_some() {
            cache::save(&self.store, &self.ui);
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
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let galley = ui.painter().layout_no_wrap(text, semi(12.0), Color32::WHITE);
    let w = (galley.size().x + 10.0).max(16.0);
    let rect = Rect::from_min_size(pos2(right_center.x - w, right_center.y - 8.0), vec2(w, 16.0));
    if let Some(ring) = ring {
        ui.painter().rect_filled(rect.expand(3.0), CornerRadius::same(11), ring);
    }
    ui.painter().rect_filled(rect, CornerRadius::same(8), RED);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

pub fn button(ui: &mut Ui, text: &str, fill: Color32, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = if resp.hovered() { fill.gamma_multiply(0.85) } else { fill };
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
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let col = if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    if resp.hovered() {
        if size >= 32.0 {
            ui.painter().rect_filled(rect, CornerRadius::same(8), pal().control_hover);
        }
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

/// A red-button confirmation like discord's, for leaving servers, blocking and so on.
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub button: &'static str,
    pub run: Box<dyn FnOnce(&mut App)>,
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

/// Long menus scroll rather than run off a small window.
pub fn menu_body(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let h = ui.ctx().content_rect().height() - 32.0;
    egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui)
    });
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
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, CornerRadius::same(4), if danger { crate::theme::CRITICAL_BG } else { crate::theme::MENU_HOVER });
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
