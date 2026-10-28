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
    super::rpc::tick(app);
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
