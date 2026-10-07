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

use egui::{Color32, CornerRadius, Frame, Image, Pos2, Rect, Sense, Ui, pos2, vec2};
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
    pub scroll_fix: Option<(Id, f32)>,
    pub revealed: HashSet<Id>,
    pub picker: Option<(Pick, Pos2)>,
    pub menu: Option<(Id, Id, Pos2)>,
    pub confirm_delete: Option<(Id, Id)>,
    pub bar: Option<(Id, Rect)>,
    pub toast: Option<(String, Instant)>,
    pub marker: Option<(Id, Id)>,
    pub dm_filter: String,
    pub focus_composer: bool,
    pub rects: HashMap<String, Rect>,
    pub settings: Settings,
    pub settings_page: Option<settings::Page>,
    pub invite: Option<String>,
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
    // a channel the user just marked unread shouldn't get re-acked on the next frame
    pub hold_ack: Option<Id>,
    pub reply_ping: bool,
    pub gifs: Option<popups::Gifs>,
    title_count: Option<u32>,
    e2e: Option<e2e::Run>,
    win: Option<[f32; 2]>,
    frames: u64,
    save_at: Option<Instant>,
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
        let (net, rx) = Net::new(ctx.clone());
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
            scroll_fix: None,
            revealed: HashSet::new(),
            picker: None,
            menu: None,
            confirm_delete: None,
            bar: None,
            toast: None,
            marker: None,
            dm_filter: String::new(),
            focus_composer: true,
            rects: HashMap::new(),
            e2e: e2e::Run::from_env(t0),
            win: None,
            settings: cache::settings(),
            settings_page: None,
            invite: None,
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
            hold_ack: None,
            reply_ping: true,
            gifs: None,
            title_count: None,
            frames: 0,
            save_at: None,
        };
        ctx.set_zoom_factor(app.settings.zoom);
        crate::markdown::CLOCK24.store(app.settings.clock24, std::sync::atomic::Ordering::Relaxed);
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
        if self.cur() != Some(ch) {
            self.reply = None;
            self.editing = None;
            self.files.clear();
            self.pins = None;
            self.post = None;
            self.hold_ack = None;
        }
        self.friends = false;
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

    /// Open a channel scrolled to one message, fetching the history around it if needed.
    pub fn jump_to(&mut self, ch: Id, id: Id) {
        self.scroll_to = Some((ch, id));
        self.scroll_tries = 0;
        if self.cur() != Some(ch) {
            let g = self.store.channels.get(&ch).map(|c| c.guild).unwrap_or(0);
            self.ui.guild = (g != 0).then_some(g);
            self.open(ch);
        }
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
        let text = self.resolve_aliases(ch, text.trim());
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
        self.net.send(Cmd::Send { channel: ch, content: text, nonce, reply, files });
        self.hold_ack = None;
        self.last_typing = None;
    }

    pub fn react(&mut self, ch: Id, msg: Id, em: ReactEmoji, add: bool) {
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
        if muted {
            self.store.muted.insert(id);
        } else {
            self.store.muted.remove(&id);
        }
        if guild {
            self.rest("PATCH", format!("/users/@me/guilds/{id}/settings"), Some(json!({ "muted": muted })), Tag::Quiet);
        } else {
            let g = self.store.channels.get(&id).map(|c| c.guild).unwrap_or(0);
            let path = if g == 0 { "@me".to_string() } else { g.to_string() };
            let body = json!({ "channel_overrides": { id.to_string(): { "muted": muted } } });
            self.rest("PATCH", format!("/users/@me/guilds/{path}/settings"), Some(body), Tag::Quiet);
        }
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
        ctx.set_zoom_factor(self.settings.zoom);
        crate::markdown::CLOCK24.store(self.settings.clock24, std::sync::atomic::Ordering::Relaxed);
        cache::set_settings(&self.settings);
    }

    fn notify(&mut self, d: &serde_json::Value, ctx: &egui::Context) {
        let m = self.store.parse_message(d);
        if m.author == self.store.me || !self.settings.notify {
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
        if self.title_count != Some(n) {
            self.title_count = Some(n);
            let t = if n > 0 { format!("({n}) Fastcord") } else { "Fastcord".into() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
    }

    fn on_rest(&mut self, tag: Tag, res: Result<serde_json::Value, String>) {
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                if let Tag::Pins(_) = tag {
                    self.pins = None;
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
                self.profiles.insert(u, v);
            }
            Tag::Forum(f) => self.store.load_forum(f, &v),
            Tag::Invite => {
                let name = v["guild"]["name"].as_str().unwrap_or("the server").to_string();
                self.toast(format!("Joined {name}"));
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
        self.net.send(Cmd::Stop);
        cache::set_token(None);
        self.token = None;
        self.store = Store::default();
        self.ui = UiState::default();
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
                    self.store.apply(&t, &d);
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
            }
        }
    }

    fn ack_current(&mut self, ctx: &egui::Context) {
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
                            .fill(BG_FLOATING)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(16, 10))
                            .show(ui, |ui| ui.label(egui::RichText::new(text).color(TEXT_NORMAL).font(reg(14.0))));
                    });
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if let Some((ch, id)) = self.confirm_delete {
            let mut close = false;
            let modal = egui::Modal::new("delete".into())
                .frame(Frame::new().fill(BG_PRIMARY).corner_radius(8).inner_margin(16))
                .show(&ctx, |ui| {
                    ui.set_width(440.0);
                    ui.label(egui::RichText::new("Delete Message").font(bold(20.0)).color(HEADER_PRIMARY));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Are you sure you want to delete this message?").color(TEXT_NORMAL));
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
                            if ui.add(egui::Label::new(egui::RichText::new("Cancel").color(HEADER_PRIMARY)).sense(Sense::click())).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            if close || modal.should_close() {
                self.confirm_delete = None;
            }
        }
        picker::show(self, &ctx);
        chat::context_menu(self, &ctx);
        popups::show(self, &ctx);
        popups::viewer(self, &ctx);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.pump(&ctx);
        if !self.cjk_loaded && crate::theme::CJK_WANTED.load(std::sync::atomic::Ordering::Relaxed) {
            self.cjk_loaded = true;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::theme::fonts(true));
            });
            self.fonts_rx = Some(rx);
        }
        if let Some(f) = self.fonts_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            ctx.set_fonts(f);
            self.fonts_rx = None;
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

        if self.token.is_none() {
            login::show(self, ui);
        } else if self.settings_page.is_some() {
            settings::show(self, ui);
        } else {
            let w = ctx.content_rect().width();
            if (w - self.heights_w).abs() > 0.5 {
                self.heights.clear();
                self.heights_w = w;
            }
            egui::Panel::left("rail")
                .exact_size(72.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(BG_TERTIARY))
                .show(ui, |ui| rail::show(self, ui));
            egui::Panel::left("side")
                .exact_size(240.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(BG_SECONDARY))
                .show(ui, |ui| side::show(self, ui));
            egui::CentralPanel::default()
                .frame(Frame::new().fill(BG_PRIMARY))
                .show(ui, |ui| chat::show(self, ui));
            self.overlays(ui);
            self.ack_current(&ctx);
            self.update_title(&ctx);
            if ctx.input_mut(|i| take_key(i, egui::Modifiers::COMMAND, egui::Key::K)) {
                self.switcher = Some(String::new());
            }
            self.shortcuts(&ctx);
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

        if self.save_at.is_some_and(|t| Instant::now() >= t) {
            self.save_at = None;
            cache::save(&self.store, &self.ui);
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
        BG_TERTIARY.to_normalized_gamma_f32()
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
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, BG_ACCENT);
    Image::from_uri(url.to_string())
        .corner_radius(CornerRadius::same((rect.width() / 2.0) as u8))
        .show_loading_spinner(false)
        .paint_at(ui, rect);
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
    let galley = ui.painter().layout_no_wrap(text, bold(12.0), Color32::WHITE);
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
    let col = if resp.hovered() { INTERACTIVE_HOVER } else { HEADER_SECONDARY };
    if resp.hovered() {
        if size >= 32.0 {
            ui.painter().rect_filled(rect, CornerRadius::same(4), BG_MOD_HOVER);
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let s = if size >= 32.0 { 20.0 } else { (size * 0.86).min(24.0) };
    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(s, s)), col);
    resp.on_hover_text(tip)
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
