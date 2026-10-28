use serde_json::Value;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::{LazyLock, RwLock};

use super::App;
use crate::plugins;
use crate::state::Id;

#[derive(Default)]
pub struct State {
    pub rpc: super::rpc::Rpc,
    pub perms: Option<super::info::Perms>,
    pub info: Option<Id>,
    call: Option<(Id, std::time::Instant)>,
    usrbg_rx: Option<Receiver<Option<Value>>>,
    usrbg_asked: bool,
}

struct Usrbg {
    base: String,
    users: HashMap<Id, String>,
}

static USRBG: LazyLock<RwLock<Usrbg>> = LazyLock::new(|| RwLock::new(Usrbg { base: String::new(), users: HashMap::new() }));

/// USRBG's banner for someone, if the list has one.
pub fn usrbg(uid: Id) -> Option<String> {
    if !plugins::on("USRBG") {
        return None;
    }
    let u = USRBG.read().unwrap();
    u.users.get(&uid).map(|tag| format!("{}/{uid}?{tag}", u.base))
}

/// Per-frame upkeep for plugins that fetch things: USRBG's list, once.
pub fn tick(app: &mut App, ctx: &egui::Context) {
    if plugins::on("USRBG") && !app.ext.usrbg_asked {
        app.ext.usrbg_asked = true;
        let (tx, rx) = std::sync::mpsc::channel();
        app.ext.usrbg_rx = Some(rx);
        let ctx = ctx.clone();
        app.net.rt.spawn(async move {
            let v = async { crate::api::http().get("https://usrbg.is-hardly.online/users").send().await.ok()?.json::<Value>().await.ok() }.await;
            let _ = tx.send(v);
            ctx.request_repaint();
        });
    }
    if let Some(v) = app.ext.usrbg_rx.as_ref().and_then(|r| r.try_recv().ok()) {
        app.ext.usrbg_rx = None;
        if let Some(v) = v {
            let mut u = USRBG.write().unwrap();
            u.base = v["endpoint"].as_str().unwrap_or_default().to_string();
            u.users = v["users"].as_object().into_iter().flatten().filter_map(|(k, t)| Some((k.parse().ok()?, t.as_str()?.to_string()))).collect();
        }
    }
    if app.voice.chan.is_none() {
        app.ext.call = None;
    }
    super::rpc::tick(app);
}

/// CallTimer: how long we've been in the call, in the format picked.
pub fn call_timer(app: &mut App, ch: Id) -> Option<String> {
    if !plugins::on("CallTimer") {
        return None;
    }
    let since = match app.ext.call {
        Some((c, t)) if c == ch => t,
        _ => app.ext.call.insert((ch, std::time::Instant::now())).1,
    };
    let s = since.elapsed().as_secs();
    let (d, h, m, sec) = (s / 86400, s / 3600 % 24, s / 60 % 60, s % 60);
    Some(if plugins::text("CallTimer", "format") == "human" {
        let mut out = String::new();
        if d > 0 {
            out += &format!("{d}d ");
        }
        if d > 0 || h > 0 {
            out += &format!("{h:02}h ");
        }
        out + &format!("{m:02}m {sec:02}s")
    } else {
        let mut out = String::new();
        if d > 0 {
            out += &format!("{d}d ");
        }
        if d > 0 || h > 0 {
            out += &format!("{h:02}:");
        }
        out + &format!("{m:02}:{sec:02}")
    })
}

/// MemberCount: online, total and in voice, as Vencord words them.
pub fn member_counts(store: &crate::state::Store, gid: Id) -> Option<(Option<u32>, u32, usize)> {
    let g = store.guilds.get(&gid)?;
    let online: u32 = g.list.iter().map(|i| match i {
        crate::state::ListItem::Group { id, count } if id != "offline" => *count,
        _ => 0,
    }).sum();
    Some(((!g.list.is_empty()).then_some(online), g.member_count, g.voice.len()))
}

/// The row of counts that heads the member list.
pub fn count_row(ui: &mut egui::Ui, store: &crate::state::Store, gid: Id) {
    use crate::theme::*;
    let Some((online, total, voice)) = member_counts(store, gid) else { return };
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let mut x = r.left() + 16.0;
    let mut part = |ui: &egui::Ui, col: egui::Color32, text: String| {
        ui.painter().circle_filled(egui::pos2(x + 4.0, r.center().y), 4.0, col);
        x = ui.painter().text(egui::pos2(x + 12.0, r.center().y), egui::Align2::LEFT_CENTER, text, medium(12.0), pal().header_secondary).right() + 10.0;
    };
    if let Some(n) = online {
        part(ui, GREEN, format!("{n} Online"));
    }
    part(ui, GREY, format!("{total} Members"));
    if plugins::flag("MemberCount", "voiceActivity") && voice > 0 {
        part(ui, GREEN, format!("{voice} In Voice"));
    }
}

/// The server tooltip's count line.
pub fn count_line(store: &crate::state::Store, gid: Id) -> Option<String> {
    if !plugins::flag("MemberCount", "toolTip") {
        return None;
    }
    let (online, total, voice) = member_counts(store, gid)?;
    let mut s = online.map_or(String::new(), |n| format!("{n} Online \u{b7} "));
    s += &format!("{total} Members");
    if plugins::flag("MemberCount", "voiceActivity") && voice > 0 {
        s += &format!(" \u{b7} {voice} In Voice");
    }
    Some(s)
}

pub enum RoleAct {
    Color(u32),
    Icon(String),
    Edit,
}

/// BetterRoleContext: the menu a role's pill or member-list heading opens.
pub fn role_menu(r: &egui::Response, role: &crate::state::Role) -> Option<RoleAct> {
    use super::{menu_body, menu_closing, menu_item};
    if !plugins::on("BetterRoleContext") {
        return None;
    }
    let mut act = None;
    egui::Popup::context_menu(r).close_behavior(menu_closing(&r.ctx)).show(|ui| {
        menu_body(ui, |ui| {
            if menu_item(ui, None, "Copy Role Color", false) {
                act = Some(RoleAct::Color(role.color));
            }
            if let Some(url) = role_icon_url(role)
                && menu_item(ui, None, "View Role Icon", false)
            {
                act = Some(RoleAct::Icon(url));
            }
            if menu_item(ui, None, "Edit Role", false) {
                act = Some(RoleAct::Edit);
            }
        })
    });
    act
}

pub fn role_icon_url(role: &crate::state::Role) -> Option<String> {
    match &role.icon {
        Some(h) => Some(format!("{}/role-icons/{}/{h}.{}?size=512", crate::api::cdn(), role.id, plugins::text("BetterRoleContext", "roleIconFileFormat"))),
        None => role.icon_url(512),
    }
}

pub fn do_role(app: &mut App, ctx: &egui::Context, gid: Id, act: RoleAct) {
    match act {
        RoleAct::Color(c) => ctx.copy_text(format!("#{c:06x}")),
        RoleAct::Icon(url) => app.viewer = Some((url.clone(), url, 0, 0)),
        RoleAct::Edit => {
            app.open_server_settings(gid);
            if let Some(s) = &mut app.server {
                s.page = super::server::Page::Roles;
            }
        }
    }
}

/// ViewIcons: an image of an avatar, banner or icon at the size and format picked.
pub fn icon_url(kind: &str, id: Id, hash: &str) -> String {
    let ext = if hash.starts_with("a_") { "webp".to_string() } else { plugins::text("ViewIcons", "format") };
    format!("{}/{kind}/{id}/{hash}.{ext}?size={}", crate::api::cdn(), plugins::text("ViewIcons", "imgSize"))
}

pub fn view(app: &mut App, url: String) {
    app.viewer = Some((url.clone(), url, 0, 0));
}

/// ViewIcons on a server: its icon and banner, if it has them.
pub fn guild_icons(store: &crate::state::Store, gid: Id) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(g) = store.guilds.get(&gid).filter(|_| plugins::on("ViewIcons")) {
        if let Some(h) = &g.icon {
            out.push(("View Icon", icon_url("icons", gid, h)));
        }
        if let Some(h) = &g.banner {
            out.push(("View Banner", icon_url("banners", gid, h)));
        }
    }
    out
}

/// PinDMs: the pinned DMs that still exist, in the order the plugin is set to show them.
pub fn pinned(store: &crate::state::Store) -> Vec<Id> {
    if !plugins::on("PinDMs") {
        return Vec::new();
    }
    let mut pins: Vec<Id> = pin_list().into_iter().filter(|c| store.dms.contains(c)).collect();
    if plugins::text("PinDMs", "pinOrder") != "1" {
        pins.sort_by_key(|c| store.dms.iter().position(|d| d == c));
    }
    pins
}

fn pin_list() -> Vec<Id> {
    plugins::get("PinDMs", "pinnedDMs").as_array().into_iter().flatten().filter_map(|v| v.as_str()?.parse().ok()).collect()
}

fn save_pins(list: Vec<Id>) {
    plugins::set("PinDMs", "pinnedDMs", list.iter().map(|c| c.to_string()).collect::<Vec<_>>().into());
}

pub fn pin(ch: Id, on: bool) {
    let mut list = pin_list();
    list.retain(|c| *c != ch);
    if on {
        list.push(ch);
    }
    save_pins(list);
}

pub fn shift_pin(ch: Id, by: i32) {
    let mut list = pin_list();
    if let Some(i) = list.iter().position(|c| *c == ch) {
        let j = (i as i32 + by).clamp(0, list.len() as i32 - 1) as usize;
        list.swap(i, j);
        save_pins(list);
    }
}

fn app_uri(url: &str) -> Option<String> {
    const P: &str = "OpenInApp";
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path_only = path.split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path_only.split('/').filter(|s| !s.is_empty()).collect();
    match host {
        "open.spotify.com" if plugins::flag(P, "spotify") => {
            let segs = if segs.first().is_some_and(|s| s.starts_with("intl-")) { &segs[1..] } else { &segs[..] };
            let kind = segs.first().filter(|k| ["track", "album", "artist", "playlist", "user", "episode", "prerelease"].contains(k))?;
            (segs.len() > 1).then(|| format!("spotify://{kind}/{}", segs[1..].join("/")))
        }
        "steamcommunity.com" | "help.steampowered.com" | "store.steampowered.com" if plugins::flag(P, "steam") => Some(format!("steam://openurl/{url}")),
        "store.epicgames.com" if plugins::flag(P, "epic") && !path.is_empty() => Some(format!("com.epicgames.launcher://store/{path}")),
        "tidal.com" | "listen.tidal.com" if plugins::flag(P, "tidal") => {
            let segs = if segs.first() == Some(&"browse") { &segs[1..] } else { &segs[..] };
            let kind = segs.first().filter(|k| ["track", "album", "artist", "playlist", "user", "video", "mix"].contains(k))?;
            let id = segs.get(1).filter(|i| i.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))?;
            Some(format!("tidal://{kind}/{id}"))
        }
        "music.apple.com" | "geo.music.apple.com" if plugins::flag(P, "itunes") => {
            let segs = if segs.first().is_some_and(|s| s.len() == 2) { &segs[1..] } else { &segs[..] };
            let kind = segs.first().filter(|k| ["album", "artist", "playlist", "song", "curator"].contains(k))?;
            let name = segs.get(1)?;
            Some(match segs.get(2) {
                Some(id) => format!("itunes://music.apple.com/us/{kind}/{name}/{id}"),
                None => format!("itunes://music.apple.com/us/{kind}/{name}"),
            })
        }
        _ => None,
    }
}

/// Opens a link in the browser, or in its app when OpenInApp has one for it.
pub fn open(ctx: &egui::Context, url: &str) {
    match app_uri(url) {
        Some(uri) => {
            let _ = open::that_detached(uri);
        }
        None => ctx.open_url(egui::OpenUrl::new_tab(url)),
    }
}
