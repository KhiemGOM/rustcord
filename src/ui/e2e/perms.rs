//! Server Settings and pinning under partial permissions, on the mock's Design Club (the account owns
//! nothing there): which sidebar rows draw, a real click on one, and who may pin. Steps 4400 and up.

use super::live::event;
use super::*;
use crate::state::{BAN_MEMBERS, MANAGE_GUILD_EXPRESSIONS, MANAGE_MESSAGES, MANAGE_ROLES, PIN_MESSAGES, READ_MESSAGE_HISTORY, Role};

const GUILD: Id = 1002;
const OWNED: Id = 1001;
const CHANNEL: Id = 2200;
const ROLE: Id = 4400;

fn menu_row(ctx: &egui::Context, label: &str) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))))
}

const ALWAYS: [&str; 4] = ["Server Profile", "Engagement", "Boost Perks", "App Directory"];
const OWNER_ONLY: [&str; 2] = ["Server Template", "Delete Server"];

impl Run {
    pub(super) fn perms_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::ui::server::Page;
        let drawn = |ctx: &egui::Context, l: &str| Self::at(ctx, &format!("side:{l}")).is_some();
        let forget = |ctx: &egui::Context| {
            for l in ["Server Tag", "Engagement", "Boost Perks", "Emoji", "Stickers", "Soundboard", "Members", "Roles", "Invites", "Access", "Integrations", "App Directory", "Safety Setup", "Audit Log", "Bans", "AutoMod", "Server Profile", "Server Template", "Delete Server", "Enable Community"] {
                ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("e2e", format!("side:{l}").as_str()))));
            }
        };
        let grant = |app: &mut App, perms: Option<u64>| {
            let Some(g) = app.store.guilds.get_mut(&GUILD) else { return };
            g.my_roles.retain(|r| *r != ROLE);
            g.roles.remove(&ROLE);
            if let Some(perms) = perms {
                g.roles.insert(ROLE, Role { id: ROLE, name: "e2e".into(), position: 1, perms, ..Default::default() });
                g.my_roles.push(ROLE);
            }
        };
        match self.step {
            4400 => {
                grant(app, None);
                forget(ctx);
                app.open_server_settings(GUILD);
                self.next(700);
            }
            4401 => {
                if !ALWAYS.iter().all(|l| drawn(ctx, l)) {
                    return;
                }
                let hidden = ["Server Tag", "Emoji", "Members", "Roles", "Invites", "Access", "Integrations", "Safety Setup", "Audit Log", "Bans", "AutoMod", "Server Template", "Delete Server"];
                if let Some(l) = hidden.iter().find(|l| drawn(ctx, l)) {
                    return self.fail(format!("{l} drew with no permissions"));
                }
                self.shot(ctx, "perms_none.png");
                self.mark("perms_none");
                grant(app, Some(MANAGE_GUILD_EXPRESSIONS | BAN_MEMBERS));
                forget(ctx);
                self.next(700);
            }
            4402 => {
                let shown = ["Emoji", "Stickers", "Soundboard", "Members", "Bans"];
                if !shown.iter().chain(ALWAYS.iter()).all(|l| drawn(ctx, l)) {
                    return;
                }
                let hidden = ["Server Tag", "Roles", "Invites", "Access", "Integrations", "Safety Setup", "Audit Log", "AutoMod", "Server Template", "Delete Server"];
                if let Some(l) = hidden.iter().find(|l| drawn(ctx, l)) {
                    return self.fail(format!("{l} drew with only expressions and bans"));
                }
                self.shot(ctx, "perms_partial.png");
                self.mark("perms_partial");
                let Some(r) = Self::at(ctx, "side:Bans") else { return };
                self.click(r.center());
                self.next(500);
            }
            4403 => {
                if !app.server.as_ref().is_some_and(|s| s.page == crate::ui::server::Page::Bans) {
                    return;
                }
                self.mark("perms_clicked");
                let me = app.store.me;
                if let Some(g) = app.store.guilds.get_mut(&GUILD) {
                    g.owner = me;
                }
                forget(ctx);
                self.next(700);
            }
            4404 => {
                if !OWNER_ONLY.iter().chain(["Roles", "Invites", "Audit Log"].iter()).all(|l| drawn(ctx, l)) {
                    return;
                }
                self.shot(ctx, "perms_owner.png");
                self.mark("perms_owner");
                if let Some(g) = app.store.guilds.get_mut(&GUILD) {
                    g.owner = 102;
                }
                grant(app, None);
                app.server = None;
                self.next(300);
            }
            4405 => {
                let Some(c) = app.store.channels.get(&CHANNEL).cloned() else { return self.fail("no Design Club channel".into()) };
                let mut ok = !app.store.can_pin(&c);
                grant(app, Some(MANAGE_MESSAGES | READ_MESSAGE_HISTORY));
                ok &= !app.store.can_pin(&c);
                grant(app, Some(PIN_MESSAGES));
                ok &= !app.store.can_pin(&c);
                grant(app, Some(PIN_MESSAGES | READ_MESSAGE_HISTORY));
                ok &= app.store.can_pin(&c);
                grant(app, None);
                if ok {
                    self.mark("perms_pin");
                } else {
                    self.fail("pinning asked for the wrong permission".into());
                }
                self.next(200);
            }
            4406 => {
                for path in ["/e2e/unauthorized", "/channels/2201/messages"] {
                    app.rest("POST", "/mock/fail".into(), Some(json!({ "method": "GET", "path": path, "status": 401 })), crate::net::Tag::Quiet);
                }
                self.next(300);
            }
            4407 => {
                app.rest("GET", "/e2e/unauthorized".into(), None, crate::net::Tag::Quiet);
                app.net.send(crate::net::Cmd::Messages { channel: 2201, page: crate::state::Page::Latest });
                self.next(1500);
            }
            4408 => {
                if app.token.is_none() || app.store.me == 0 {
                    return self.fail("a 401 from one request signed the account out".into());
                }
                self.mark("unauthorized_kept");
                self.next(200);
            }
            4409 => {
                app.open_server_settings(OWNED);
                if let Some(s) = &mut app.server {
                    s.page = Page::Invites;
                }
                self.next(600);
            }
            4410 => {
                let Some(s) = app.server.as_mut().filter(|s| s.data.get("invites").is_some_and(|v| !v.is_null())) else { return };
                s.data.insert("invites", json!([{ "code": "stale" }]));
                event(app, "INVITE_CREATE", json!({ "guild_id": OWNED.to_string(), "code": "abc" }));
                self.next(0);
            }
            4411 => {
                if app.server.as_ref().is_none_or(|s| s.data.get("invites").is_some_and(|v| v[0]["code"] == "stale")) {
                    return;
                }
                self.mark("live_invites_reloaded");
                if let Some(s) = &mut app.server {
                    s.page = Page::Audit;
                }
                self.next(600);
            }
            4412 => {
                let Some(s) = app.server.as_mut().filter(|s| s.data.get("audit").is_some_and(|v| !v.is_null())) else { return };
                s.data.insert("audit", json!({ "audit_log_entries": [{ "id": "stale" }], "users": [] }));
                event(app, "GUILD_AUDIT_LOG_ENTRY_CREATE", json!({ "guild_id": OWNED.to_string(), "id": "1", "action_type": 1 }));
                self.next(0);
            }
            4413 => {
                if app.server.as_ref().is_none_or(|s| s.data.get("audit").is_some_and(|v| v["audit_log_entries"][0]["id"] == "stale")) {
                    return;
                }
                self.mark("live_audit_reloaded");
                app.server = None;
                event(app, "USER_SETTINGS_UPDATE", json!({ "status": "dnd", "custom_status": { "text": "from another device" } }));
                self.next(500);
            }
            4414 => {
                if app.store.status(app.store.me) != crate::state::Status::Dnd || app.store.custom_status.as_deref() != Some("from another device") {
                    return;
                }
                self.mark("live_settings_update");
                event(app, "USER_SETTINGS_UPDATE", json!({ "status": "online", "custom_status": null }));
                self.next(500);
            }
            4415 => {
                app.ban(GUILD, 160);
                self.next(600);
            }
            4416 => {
                let Some(r) = Self::at(ctx, "kickban:go") else { return };
                self.click(r.center());
                self.next(400);
            }
            4417 => {
                if app.kickban.is_none() {
                    return self.fail("a ban went out with no reason".into());
                }
                self.mark("ban_needs_reason");
                let Some(r) = Self::at(ctx, "dropdown:Select a reason") else { return };
                self.click(r.center());
                self.next(500);
            }
            4418 => {
                let Some(r) = menu_row(ctx, "Other") else { return };
                self.click(r.center());
                self.next(500);
            }
            4419 => {
                let Some(r) = Self::at(ctx, "kickban:reason") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("spam wave".into())]);
                self.next(400);
            }
            4420 => {
                let Some(r) = Self::at(ctx, "dropdown:Previous Hour") else { return };
                self.shot(ctx, "ban_dialog.png");
                self.click(r.center());
                self.next(500);
            }
            4421 => {
                let Some(r) = menu_row(ctx, "Previous 24 Hours") else { return };
                self.click(r.center());
                self.next(400);
            }
            4422 => {
                let Some(r) = Self::at(ctx, "kickban:go") else { return };
                self.click(r.center());
                self.next(800);
            }
            4423 => {
                if app.kickban.is_some() {
                    return;
                }
                self.mark("ban_sent");
                app.kick(GUILD, 160);
                self.next(600);
            }
            4424 => {
                let Some(r) = Self::at(ctx, "kickban:reason") else { return };
                self.shot(ctx, "kick_dialog.png");
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("rude".into())]);
                self.next(400);
            }
            4425 => {
                let Some(r) = Self::at(ctx, "kickban:go") else { return };
                self.click(r.center());
                self.next(800);
            }
            4426 => {
                if app.kickban.is_some() {
                    return;
                }
                self.mark("kick_sent");
                app.toast = None;
                self.next(200);
            }
            4427 => {
                grant(app, Some(MANAGE_ROLES | BAN_MEMBERS));
                if let Some(g) = app.store.guilds.get_mut(&GUILD) {
                    g.roles.insert(4401, Role { id: 4401, name: "SkyHelper".into(), position: 0, managed: true, ..Default::default() });
                    g.roles.insert(4402, Role { id: 4402, name: "Boss".into(), position: 5, ..Default::default() });
                }
                app.open_server_settings(GUILD);
                if let Some(s) = &mut app.server {
                    s.page = Page::Roles;
                }
                self.next(800);
            }
            4428 => {
                for k in ["role:managed", "role:locked"] {
                    ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("e2e", k))));
                }
                crate::ui::server::open_role(app, GUILD, 4401, 3);
                self.next(1000);
            }
            4429 => {
                if Self::at(ctx, "role:managed").is_none() || Self::at(ctx, "role:locked").is_some() {
                    return;
                }
                self.shot(ctx, "role_managed.png");
                self.mark("role_managed_banner");
                self.next(500);
            }
            4430 => {
                crate::ui::server::open_role(app, GUILD, 4402, 1);
                self.next(1000);
            }
            4431 => {
                if Self::at(ctx, "role:locked").is_none() {
                    return;
                }
                self.shot(ctx, "role_locked.png");
                self.mark("role_locked_banner");
                self.next(500);
            }
            4432 => {
                grant(app, Some(BAN_MEMBERS));
                if let Some(r) = app.store.guilds.get_mut(&GUILD).and_then(|g| g.roles.get_mut(&ROLE)) {
                    r.position = 0;
                }
                if let Some(s) = &mut app.server {
                    s.pages.roles.edit = None;
                    s.page = Page::Members;
                }
                self.next(1500);
            }
            4433 => {
                let Some(r) = app.rects.get("members:pick_all").copied() else { return };
                if app.server.as_ref().is_none_or(|s| s.pages.members.list.is_none()) {
                    return;
                }
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(1200);
            }
            4434 => {
                self.shot(ctx, "members_unactionable.png");
                self.mark("members_unactionable");
                self.next(500);
            }
            4435 => {
                if let Some(g) = app.store.guilds.get_mut(&GUILD) {
                    g.roles.remove(&4401);
                    g.roles.remove(&4402);
                }
                grant(app, None);
                app.server = None;
                self.next(300);
            }
            _ => self.step = 3110,
        }
    }
}
