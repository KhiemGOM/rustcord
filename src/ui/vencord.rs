//! Where the app meets the Vencord host: it starts once logged in, hears every gateway event,
//! gets a say in outgoing messages, and does what plugins ask (requests, toasts, sends...).

use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use super::App;
use crate::cache;
use crate::net::{Cmd, Tag};
use crate::state::{Id, sid};
use crate::vencord::{Host, View};

/// A message on hold while plugins look at it.
pub struct Outgoing {
    pub ch: Id,
    pub text: String,
    pub reply: Option<(Id, bool)>,
    pub files: Vec<(PathBuf, String)>,
    pub tts: bool,
}

#[derive(Default)]
pub struct Vc {
    pub host: Option<Host>,
    tried: bool,
    token: u32,
    sends: HashMap<u32, Outgoing>,
    edits: HashMap<u32, (Id, Id)>,
    fetched: Option<(Sender<(u32, Value)>, Receiver<(u32, Value)>)>,
    tick_at: Option<Instant>,
    selected: Option<Id>,
    // what plugins added, by what it was asked for: a menu, a hovered message, a channel's bar
    pub menu: Option<(String, Value)>,
    pub popover: Option<(Id, Instant, Value)>,
    pub bar: Option<(Id, Instant, Value)>,
    pub commands: Option<Value>,
    pub list: Option<Value>,
}

fn settings_path() -> PathBuf {
    cache::dir().join("vencord.json")
}

fn data_path() -> PathBuf {
    cache::dir().join("vencord_data.json")
}

fn read(p: PathBuf) -> Option<Value> {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// Vencord's folder name for a plugin, folded the way its settings name it.
fn folded(path: &str) -> String {
    path.split('/').nth(2).unwrap_or("").split('.').next().unwrap_or("").to_lowercase()
}

impl App {
    fn view(&self) -> View<'_> {
        View { store: &self.store, channel: self.cur().unwrap_or(0), guild: self.ui.guild.unwrap_or(0), voice: self.voice.chan.unwrap_or(0) }
    }

    /// Calls the host, then does whatever the plugins asked for during it.
    pub fn vc_call(&mut self, name: &str, arg: Value) -> Value {
        let Some(h) = &self.vc.host else { return Value::Null };
        let out = h.call(&self.view(), name, arg);
        self.vc_drain();
        out
    }

    /// Starts the host the first time there's an account to run plugins against. Settings come
    /// from Fastcord's own copy, or Vencord's on the first run, so its plugins stay as they were.
    pub(super) fn vc_start(&mut self) {
        if self.vc.tried || self.store.me == 0 {
            return;
        }
        self.vc.tried = true;
        let settings = read(settings_path())
            .or_else(|| read(crate::vencord::vencord_dir().join("settings").join("settings.json")))
            .filter(|v| v.is_object())
            .unwrap_or_else(|| json!({ "plugins": {} }));
        let data = read(data_path()).unwrap_or_else(|| json!({}));
        let extra = cache::dir().join("userplugins");
        self.vc.host = Host::new(&extra, &settings, &data);
        let Some(h) = &self.vc.host else { return };
        let on: Vec<String> = settings["plugins"].as_object().into_iter().flatten().filter(|(_, v)| v["enabled"] == true).map(|(k, _)| k.to_lowercase()).collect();
        let paths: Vec<String> = h.plugins().into_iter().filter(|p| p.starts_with("src/userplugins/") || on.contains(&folded(p))).collect();
        self.vc_call("start", json!({ "paths": paths }));
        self.vc.tick_at = Some(Instant::now());
    }

    /// A gateway event, before the store takes it in, so `x` can say what was there before.
    pub(super) fn vc_dispatch(&mut self, t: &str, d: &Value) {
        if self.vc.host.is_none() {
            return;
        }
        let x = match t {
            "VOICE_STATE_UPDATE" => {
                let u = sid(&d["user_id"]);
                let old = self.store.guilds.get(&sid(&d["guild_id"])).and_then(|g| g.voice.get(&u).copied()).or(if u == self.store.me { self.voice.chan } else { None });
                json!({ "oldChannel": old.map(|c| c.to_string()) })
            }
            "RELATIONSHIP_REMOVE" => json!({ "kind": self.store.relationships.get(&sid(&d["id"])) }),
            _ => json!({}),
        };
        self.vc_call("dispatch", json!({ "t": t, "d": d, "x": x }));
        if matches!(t, "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_DELETE") {
            self.vc.popover = None;
        }
    }

    /// Every frame: timers due, a channel switch to tell plugins about, fetches that came back.
    pub(super) fn vc_tick(&mut self, ctx: &egui::Context) {
        self.vc_start();
        if self.vc.host.is_none() {
            return;
        }
        if self.vc.commands.is_none() {
            self.vc.commands = Some(self.vc_call("commands", json!({})));
        }
        let cur = self.cur();
        if cur != self.vc.selected {
            self.vc.selected = cur;
            let guild = cur.and_then(|c| self.store.channels.get(&c)).map(|c| c.guild).filter(|g| *g != 0);
            self.vc_call("dispatch", json!({ "t": "CHANNEL_SELECT", "d": { "channelId": cur.map(|c| c.to_string()), "guildId": guild.map(|g| g.to_string()) } }));
        }
        let back: Vec<(u32, Value)> = self.vc.fetched.as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default();
        for (id, v) in back {
            self.vc_call("settle", json!({ "id": id, "ok": true, "value": v }));
        }
        if self.vc.tick_at.is_some_and(|t| Instant::now() >= t) {
            let next = self.vc_call("tick", json!({})).as_i64().unwrap_or(-1);
            self.vc.tick_at = (next >= 0).then(|| Instant::now() + Duration::from_millis(next as u64));
        }
        if let Some(t) = self.vc.tick_at {
            ctx.request_repaint_after(t.saturating_duration_since(Instant::now()));
        }
    }

    /// The composer's message on its way out: past the plugins' send listeners when there are any.
    pub fn vc_send(&mut self, o: Outgoing) {
        let listening = self.vc.host.is_some() && self.vc_call("hasSendListeners", json!({})) == true;
        if !listening {
            return self.send_now(o);
        }
        self.vc.token += 1;
        let token = self.vc.token;
        let arg = json!({ "token": token, "channel": o.ch.to_string(), "content": o.text, "reply": o.reply.map(|r| r.0.to_string()), "files": !o.files.is_empty() });
        self.vc.sends.insert(token, o);
        self.vc_call("preSend", arg);
    }

    pub fn send_now(&mut self, o: Outgoing) {
        if o.text.is_empty() && o.files.is_empty() {
            return;
        }
        let nonce = crate::state::make_nonce();
        self.store.add_pending(o.ch, o.text.clone(), nonce.clone(), o.reply.map(|r| r.0));
        self.jump_to_present(o.ch);
        self.net.send(Cmd::Send { channel: o.ch, content: o.text, nonce, reply: o.reply, files: o.files, stickers: Vec::new(), tts: o.tts });
    }

    /// An edit on its way out, past the plugins' edit listeners.
    pub fn vc_edit(&mut self, ch: Id, id: Id, text: String) {
        let listening = self.vc.host.is_some() && self.vc_call("hasEditListeners", json!({})) == true;
        if !listening {
            return self.edit_now(ch, id, text);
        }
        self.vc.token += 1;
        let token = self.vc.token;
        self.vc.edits.insert(token, (ch, id));
        self.vc_call("preEdit", json!({ "token": token, "channel": ch.to_string(), "message": id.to_string(), "content": text }));
    }

    fn edit_now(&mut self, ch: Id, id: Id, text: String) {
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id))
            && m.content != text
        {
            m.content = text.clone();
            m.edited = true;
            self.net.send(Cmd::Edit { channel: ch, id, content: text });
        }
    }

    pub(super) fn vc_rest_done(&mut self, id: u32, res: Result<Value, String>) {
        let v = match res {
            Ok(body) => json!({ "status": 200, "body": body }),
            Err(e) => json!({ "status": 400, "body": { "message": e } }),
        };
        self.vc_call("settle", json!({ "id": id, "ok": true, "value": v }));
    }

    /// Plugin commands for the slash menu: (name, description, plugin). Refreshed by `vc_tick`.
    pub fn vc_command_list(&self) -> Vec<(String, String, String)> {
        self.vc.commands.iter().flat_map(|v| v.as_array().into_iter().flatten()).map(|c| (c["name"].as_str().unwrap_or("").to_string(), c["description"].as_str().unwrap_or("").to_string(), c["plugin"].as_str().unwrap_or("").to_string())).collect()
    }

    /// The draft as a plugin's `/command`, run if it is one. The words after it fill the
    /// command's options in order, the last taking the rest.
    pub fn vc_command(&mut self, chan: &crate::state::Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some(rest) = draft.trim().strip_prefix('/') else { return false };
        let (name, rest) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(n, a)| (n, a.trim()));
        let Some(cmd) = self.vc.commands.iter().flat_map(|v| v.as_array().into_iter().flatten()).find(|c| c["name"] == name).cloned() else { return false };
        let opts = cmd["options"].as_array().cloned().unwrap_or_default();
        let mut words = rest.splitn(opts.len().max(1), char::is_whitespace);
        let args: Vec<Value> = opts
            .iter()
            .filter_map(|o| {
                let w = words.next().filter(|w| !w.is_empty())?;
                let value = match o["type"].as_u64() {
                    Some(4) => json!(w.parse::<i64>().ok()?),
                    Some(10) => json!(w.parse::<f64>().ok()?),
                    Some(5) => json!(w == "true"),
                    Some(6..=9) => json!(w.trim_start_matches("<@").trim_start_matches(['!', '&', '#']).trim_end_matches('>')),
                    _ => json!(w),
                };
                Some(json!({ "name": o["name"], "value": value, "type": o["type"] }))
            })
            .collect();
        self.drafts.remove(&ch);
        let guild = (chan.guild != 0).then(|| chan.guild.to_string());
        self.vc_call("runCommand", json!({ "name": name, "args": args, "channel": ch.to_string(), "guild": guild }));
        true
    }

    /// What the plugins asked for during the last call.
    fn vc_drain(&mut self) {
        let Some(h) = &self.vc.host else { return };
        for op in h.take() {
            let s = |k: &str| op[k].as_str().unwrap_or("").to_string();
            let id = |k: &str| op[k].as_str().and_then(|v| v.parse::<Id>().ok()).unwrap_or(0);
            match op["op"].as_str().unwrap_or("") {
                "toast" => self.toast(s("text")),
                "notify" => {
                    let (title, body) = (s("title"), s("body"));
                    self.toast(if body.is_empty() { title.clone() } else { format!("{title}: {body}") });
                    if self.e2e.is_none() {
                        crate::toast::send(crate::toast::Ping { title, body, avatar: String::new(), at: (0, 0) });
                    }
                }
                "copy" => {
                    if let Ok(mut c) = arboard::Clipboard::new() {
                        let _ = c.set_text(s("text"));
                    }
                }
                "speak" => crate::speech::say(s("text")),
                "insert" => {
                    if let Some(ch) = self.cur() {
                        self.drafts.entry(ch).or_default().push_str(&s("text"));
                        self.caret_end = true;
                        self.focus_composer = true;
                    }
                }
                "send" => {
                    let reply = op["reply"].as_str().and_then(|r| r.parse().ok()).map(|r| (r, true));
                    self.send_now(Outgoing { ch: id("channel"), text: s("content"), reply, files: Vec::new(), tts: false });
                }
                "bot" => self.store.local_message(id("channel"), s("content")),
                "sent" => {
                    let token = op["token"].as_u64().unwrap_or(0) as u32;
                    if let Some(mut o) = self.vc.sends.remove(&token)
                        && let Some(text) = op["content"].as_str()
                    {
                        o.text = text.to_string();
                        self.send_now(o);
                    }
                }
                "edited" => {
                    let token = op["token"].as_u64().unwrap_or(0) as u32;
                    if let Some((ch, mid)) = self.vc.edits.remove(&token)
                        && let Some(text) = op["content"].as_str()
                    {
                        self.edit_now(ch, mid, text.to_string());
                    }
                }
                "open" => self.open(id("channel")),
                "jump" => self.jump_to(id("channel"), id("message")),
                "guild" => self.select_guild(Some(id("guild"))),
                "dm" => self.open_dm(id("user")),
                "profile" => self.profile = Some((id("user"), self.ui.guild.unwrap_or(0), egui::pos2(400.0, 200.0))),
                "image" => self.viewer = Some((s("url"), s("url"), 0, 0)),
                "url" => {
                    let _ = open::that_detached(s("url"));
                }
                "openSettings" => self.settings_page = Some(super::settings::Page::Plugins),
                "rest" => {
                    let method = match s("method").as_str() {
                        "POST" => "POST",
                        "PUT" => "PUT",
                        "PATCH" => "PATCH",
                        "DELETE" => "DELETE",
                        _ => "GET",
                    };
                    let path = s("path");
                    let path = path.strip_prefix("https://discord.com/api/v9").or(path.strip_prefix("/api/v9")).unwrap_or(&path).to_string();
                    let body = Some(op["body"].clone()).filter(|b| !b.is_null());
                    self.rest(method, path, body, Tag::Vencord(op["id"].as_u64().unwrap_or(0) as u32));
                }
                "fetch" => self.vc_fetch(&op),
                _ => {}
            }
            match op["op"].as_str() {
                Some("settings") => {
                    if let Ok(b) = serde_json::to_vec_pretty(&op["value"]) {
                        let _ = std::fs::write(settings_path(), b);
                    }
                    self.vc.list = None;
                    self.vc.commands = None;
                    self.vc.bar = None;
                    self.vc.popover = None;
                }
                Some("data") => {
                    if let Ok(b) = serde_json::to_vec(&op["value"]) {
                        let _ = std::fs::write(data_path(), b);
                    }
                }
                _ => {}
            }
        }
    }

    fn vc_fetch(&mut self, op: &Value) {
        let (tx, _) = self.vc.fetched.get_or_insert_with(channel);
        let tx = tx.clone();
        let id = op["id"].as_u64().unwrap_or(0) as u32;
        let url = op["url"].as_str().unwrap_or("").to_string();
        let method = reqwest::Method::from_bytes(op["method"].as_str().unwrap_or("GET").as_bytes()).unwrap_or(reqwest::Method::GET);
        let headers: Vec<(String, String)> = op["headers"].as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect();
        let body = op["body"].as_str().map(str::to_string);
        self.net.rt.spawn(async move {
            let mut req = crate::api::http().request(method, &url);
            for (k, v) in headers {
                req = req.header(k, v);
            }
            if let Some(b) = body {
                req = req.body(b);
            }
            let v = match req.send().await {
                Ok(r) => {
                    let status = r.status().as_u16();
                    let headers: serde_json::Map<String, Value> = r.headers().iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").into())).collect();
                    let bytes = r.bytes().await.unwrap_or_default();
                    use base64::Engine;
                    json!({ "status": status, "headers": headers, "url": url, "body": base64::engine::general_purpose::STANDARD.encode(&bytes) })
                }
                Err(e) => json!({ "status": 0, "headers": {}, "url": url, "body": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, e.to_string()) }),
            };
            let _ = tx.send((id, v));
        });
    }
}

/// A plugin's svg icon, tinted like Fastcord's own.
pub fn svg(ui: &egui::Ui, svg: &str, rect: egui::Rect, tint: egui::Color32) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    svg.hash(&mut h);
    egui::Image::from_bytes(format!("bytes://vencord/{}.svg", h.finish()), svg.as_bytes().to_vec()).tint(tint).paint_at(ui, rect);
}

/// A 32px toolbar button with a plugin's icon, or its label's first letter if it drew none.
pub fn icon_button(ui: &mut egui::Ui, icon: Option<&str>, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    let col = if resp.hovered() { crate::theme::pal().interactive_hover } else { crate::theme::pal().header_secondary };
    if resp.hovered() {
        ui.painter().rect_filled(r, egui::CornerRadius::same(8), crate::theme::pal().control_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    match icon {
        Some(s) => svg(ui, s, egui::Rect::from_center_size(r.center(), egui::vec2(20.0, 20.0)), col),
        None => {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, tip.chars().next().unwrap_or('?'), crate::theme::semi(14.0), col);
        }
    }
    resp.on_hover_text(tip)
}

/// Menu items from plugins, drawn like the menu they're in. Returns the action picked.
pub fn menu_items(ui: &mut egui::Ui, items: &Value) -> Option<String> {
    use super::{Trail, menu_row, menu_sep};
    let mut picked = None;
    let mut last_sep = true;
    for it in items.as_array().into_iter().flatten() {
        if it["kind"] == "sep" {
            if !last_sep {
                menu_sep(ui);
                last_sep = true;
            }
            continue;
        }
        last_sep = false;
        let label = it["label"].as_str().unwrap_or("");
        let label = match it["check"].as_bool() {
            Some(true) => format!("✓ {label}"),
            _ => label.to_string(),
        };
        let kids = it["children"].as_array().is_some_and(|c| !c.is_empty());
        let r = ui.add_enabled_ui(it["disabled"] != true, |ui| menu_row(ui, None, &label, None, it["danger"] == true, if kids { Trail::Caret } else { Trail::None })).inner;
        if kids {
            egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(a) = menu_items(ui, &it["children"]) {
                    picked = Some(a);
                }
            });
        }
        if r.clicked()
            && let Some(a) = it["action"].as_str()
        {
            picked = Some(a.to_string());
        }
    }
    picked
}

impl App {
    /// What plugins add to a menu, asked for once per menu opened.
    pub fn vc_menu(&mut self, key: String, arg: Value) -> Value {
        if self.vc.host.is_none() {
            return Value::Null;
        }
        if self.vc.menu.as_ref().is_none_or(|(k, _)| *k != key) {
            let items = self.vc_call("menu", arg);
            self.vc.menu = Some((key, items));
        }
        self.vc.menu.as_ref().map(|(_, v)| v.clone()).unwrap_or_default()
    }

    /// The buttons plugins put on the hovered message's bar, refreshed each second.
    pub fn vc_popover(&mut self, msg: Option<Id>) -> (Id, Value) {
        let (Some(id), Some(_)) = (msg, &self.vc.host) else { return (0, Value::Null) };
        let fresh = self.vc.popover.as_ref().is_some_and(|(m, t, _)| *m == id && t.elapsed() < Duration::from_secs(1));
        if !fresh {
            let Some(m) = self.cur().and_then(|c| self.store.find_msg(c, id)).map(|m| crate::vencord::message(&self.store, m)) else { return (0, Value::Null) };
            let v = self.vc_call("popover", json!({ "message": m }));
            self.vc.popover = Some((id, Instant::now(), v));
        }
        self.vc.popover.as_ref().map(|(m, _, v)| (*m, v.clone())).unwrap_or_default()
    }

    /// The buttons plugins put beside the composer, refreshed each second.
    pub fn vc_chatbar(&mut self, ch: Id) -> Value {
        if self.vc.host.is_none() {
            return Value::Null;
        }
        let fresh = self.vc.bar.as_ref().is_some_and(|(c, t, _)| *c == ch && t.elapsed() < Duration::from_secs(1));
        if !fresh {
            let empty = self.drafts.get(&ch).is_none_or(|d| d.is_empty());
            let v = self.vc_call("chatBar", json!({ "channel": ch.to_string(), "empty": empty }));
            self.vc.bar = Some((ch, Instant::now(), v));
        }
        self.vc.bar.as_ref().map(|(_, _, v)| v.clone()).unwrap_or_default()
    }

    pub fn vc_click(&mut self, ch: Id, id: Id, double: bool, mods: egui::Modifiers) {
        if self.vc.host.is_none() {
            return;
        }
        let Some(m) = self.store.find_msg(ch, id).map(|m| crate::vencord::message(&self.store, m)) else { return };
        self.vc_call("messageClick", json!({ "message": m, "channel": ch.to_string(), "detail": if double { 2 } else { 1 }, "shift": mods.shift, "ctrl": mods.ctrl, "alt": mods.alt }));
    }

    pub fn vc_act(&mut self, id: &str, shift: bool) {
        self.vc_call("act", json!({ "id": id, "shift": shift }));
        self.vc.bar = None;
        self.vc.popover = None;
    }
}
