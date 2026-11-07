//! Steps 3600 to 3699: gateway events reaching the pages that are open, then user actions the
//! mock refuses, which must say why and put back what the click changed.

use super::*;
use crate::state::Status;

#[derive(Default)]
pub struct State {
    n: u64,
    rows: usize,
    msg: Id,
    flag: u64,
    text: String,
}

const GROUP: Id = 3001;
const BANNED: Id = 104;
const NEWCOMER: Id = 107;

pub(super) fn event(app: &mut App, t: &str, d: serde_json::Value) {
    app.rest("POST", "/mock/event".into(), Some(json!({ "t": t, "d": d })), crate::net::Tag::Quiet);
}

fn refuse(app: &mut App, method: &str, path: &str, status: u16) {
    app.rest("POST", "/mock/fail".into(), Some(json!({ "method": method, "path": path, "status": status })), crate::net::Tag::Quiet);
}

fn person(id: Id) -> serde_json::Value {
    json!({ "id": id.to_string(), "username": format!("user{id}"), "global_name": format!("User {id}"), "avatar": null })
}

fn rows(app: &App, key: &str) -> Option<Vec<Id>> {
    let v = app.server.as_ref()?.data.get(key)?.as_array()?;
    Some(v.iter().map(|b| crate::state::sid(&b["user"]["id"])).collect())
}

fn toast_is(app: &App, text: &str) -> bool {
    app.toast.as_ref().is_some_and(|(t, _)| t == text)
}

fn detail_is(app: &App, text: &str) -> bool {
    app.toast_detail.as_deref() == Some(text)
}

impl Run {
    pub(super) fn gateway_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let (g, ch) = self.target;
        let me = app.store.me;
        match self.step {
            3600 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&ch) {
                    return;
                }
                app.open_server_settings(g);
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Bans;
                }
                self.next(0);
            }
            3601 => {
                if rows(app, "bans").is_none_or(|b| b != [BANNED]) {
                    return;
                }
                event(app, "GUILD_BAN_ADD", json!({ "guild_id": g.to_string(), "user": person(105) }));
                self.next(900);
            }
            3602 => {
                if rows(app, "bans") != Some(vec![105, BANNED]) {
                    return;
                }
                self.mark("live_ban_added");
                self.shot(ctx, "live_bans.png");
                event(app, "GUILD_BAN_REMOVE", json!({ "guild_id": g.to_string(), "user": person(BANNED) }));
                self.next(0);
            }
            3603 => {
                if rows(app, "bans") != Some(vec![105]) {
                    return;
                }
                self.mark("live_ban_removed");
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Integrations;
                }
                self.next(0);
            }
            3604 => {
                if ["integrations", "webhooks"].iter().any(|k| app.server.as_ref().is_none_or(|s| s.data.get(*k).is_none_or(|v| v.is_null()))) {
                    return;
                }
                event(app, "GUILD_INTEGRATIONS_UPDATE", json!({ "guild_id": g.to_string() }));
                event(app, "WEBHOOKS_UPDATE", json!({ "guild_id": g.to_string(), "channel_id": ch.to_string() }));
                self.mark("live_integrations_asked");
                self.next(700);
            }
            3605 => {
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Soundboard;
                }
                self.next(0);
            }
            3606 => {
                let sounds = |app: &App| app.server.as_ref().and_then(|s| s.data.get("sounds")).and_then(|v| v["items"].as_array()).map(|a| a.len());
                let Some(n) = sounds(app) else { return };
                event(app, "GUILD_SOUNDBOARD_SOUND_CREATE", json!({ "guild_id": g.to_string(), "sound_id": "7777", "name": "live horn", "volume": 1.0, "emoji_name": "📯", "emoji_id": null, "available": true, "user_id": "102", "user": person(102) }));
                self.fx.n = n as u64;
                self.next(0);
            }
            3607 => {
                let n = app.server.as_ref().and_then(|s| s.data.get("sounds")).and_then(|v| v["items"].as_array()).map_or(0, |a| a.len());
                if n as u64 != self.fx.n + 1 {
                    return;
                }
                self.mark("live_sound_added");
                self.shot(ctx, "live_soundboard.png");
                app.soundboard = Some((g, Some(vec![crate::state::Sound::parse(&json!({ "sound_id": "7777", "guild_id": g.to_string(), "name": "live horn" }))])));
                event(app, "GUILD_SOUNDBOARD_SOUND_UPDATE", json!({ "guild_id": g.to_string(), "sound_id": "7777", "name": "live horn 2", "volume": 1.0, "available": true }));
                self.next(0);
            }
            3608 => {
                if !app.soundboard.as_ref().is_some_and(|(_, l)| l.as_ref().is_some_and(|l| l[0].name == "live horn 2")) {
                    return;
                }
                self.mark("live_popout_renamed");
                event(app, "GUILD_SOUNDBOARD_SOUND_DELETE", json!({ "guild_id": g.to_string(), "sound_id": "7777" }));
                self.next(0);
            }
            3609 => {
                let gone = app.soundboard.as_ref().is_some_and(|(_, l)| l.as_ref().is_some_and(|l| l.is_empty()));
                if !gone {
                    return;
                }
                self.mark("live_popout_removed");
                app.soundboard = None;
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Members;
                }
                self.next(0);
            }
            3610 => {
                let n = app.server.as_ref().and_then(|s| s.pages.members.list.as_ref()).map_or(0, |l| l.len());
                if n < 4 {
                    return;
                }
                self.fx.n = n as u64;
                self.fx.rows = n;
                event(app, "GUILD_MEMBER_REMOVE", json!({ "guild_id": g.to_string(), "user": person(103) }));
                self.next(0);
            }
            3611 => {
                let n = app.server.as_ref().and_then(|s| s.pages.members.list.as_ref()).map_or(0, |l| l.len());
                if n as u64 != self.fx.n - 1 {
                    return;
                }
                self.mark("live_member_removed");
                self.shot(ctx, "live_members.png");
                let before = app.store.guilds.get(&g).map_or(0, |x| x.member_count);
                self.fx.n = before as u64;
                event(app, "GUILD_MEMBER_ADD", json!({ "guild_id": g.to_string(), "user": person(NEWCOMER), "roles": [], "joined_at": "2026-11-06T00:00:00.000000+00:00" }));
                self.next(0);
            }
            3612 => {
                let count = app.store.guilds.get(&g).map_or(0, |x| x.member_count);
                let back = app.server.as_ref().and_then(|s| s.pages.members.list.as_ref()).is_some_and(|l| l.len() == self.fx.rows);
                if count as u64 != self.fx.n + 1 || app.store.user(NEWCOMER).is_none() || !back {
                    return;
                }
                self.mark("live_member_added");
                app.server = None;
                app.ui.guild = None;
                app.open(GROUP);
                self.next(300);
            }
            3613 => {
                if app.store.channels.get(&GROUP).is_none_or(|c| c.recipients != [102, 103]) {
                    return;
                }
                event(app, "CHANNEL_RECIPIENT_ADD", json!({ "channel_id": GROUP.to_string(), "user": person(BANNED) }));
                self.next(0);
            }
            3614 => {
                if app.store.channels.get(&GROUP).is_none_or(|c| c.recipients != [102, 103, BANNED]) {
                    return;
                }
                self.mark("live_recipient_added");
                event(app, "CHANNEL_RECIPIENT_REMOVE", json!({ "channel_id": GROUP.to_string(), "user": person(102) }));
                self.next(0);
            }
            3615 => {
                if app.store.channels.get(&GROUP).is_none_or(|c| c.recipients != [103, BANNED]) {
                    return;
                }
                self.mark("live_recipient_removed");
                self.shot(ctx, "live_group_dm.png");
                event(app, "SAVED_MESSAGE_CREATE", json!({ "save_data": { "channel_id": ch.to_string(), "message_id": "424242", "due_at": null, "saved_at": "2026-11-06T00:00:00.000000+00:00" } }));
                event(app, "USER_NOTE_UPDATE", json!({ "id": "101", "note": "live note" }));
                event(app, "SESSIONS_REPLACE", json!([{ "session_id": "other", "status": "dnd", "active": true, "activities": [], "client_info": { "client": "mobile", "os": "android" } }]));
                self.next(0);
            }
            3616 => {
                if !app.bookmarks.contains(&424242) || app.notes.get(&101).map(String::as_str) != Some("live note") || app.store.status(me) != Status::Dnd {
                    return;
                }
                self.mark("live_account_events");
                event(app, "SAVED_MESSAGE_DELETE", json!({ "channel_id": ch.to_string(), "message_id": "424242" }));
                self.next(0);
            }
            3617 => {
                if app.bookmarks.contains(&424242) {
                    return;
                }
                self.mark("live_bookmark_removed");
                app.ui.guild = Some(g);
                app.open(ch);
                self.next(400);
                self.step = 3620;
            }
            3620 => {
                let Some(m) = app.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| !m.pending && !m.pinned)) else { return };
                self.fx.msg = m.id;
                self.fx.flag = m.reactions.len() as u64;
                refuse(app, "DELETE", "/messages/", 403);
                self.next(200);
            }
            3621 => {
                app.delete_msg(ch, self.fx.msg, false);
                if app.store.find_msg(ch, self.fx.msg).is_some() {
                    return self.fail("delete kept the message on screen before the answer".into());
                }
                self.next(0);
            }
            3622 => {
                if !toast_is(app, "Failed to delete message") {
                    return;
                }
                if app.store.find_msg(ch, self.fx.msg).is_some() && detail_is(app, "Missing Permissions") {
                    self.mark("fail_delete_rolled_back");
                    self.shot(ctx, "fail_delete.png");
                }
                refuse(app, "PUT", "/pins/", 403);
                self.next(200);
            }
            3623 => {
                app.pin_msg(ch, self.fx.msg, false);
                self.next(0);
            }
            3624 => {
                if !toast_is(app, "Failed to pin message") {
                    return;
                }
                if app.store.find_msg(ch, self.fx.msg).is_some_and(|m| !m.pinned) {
                    self.mark("fail_pin_rolled_back");
                }
                refuse(app, "PATCH", "/messages/", 400);
                self.next(200);
            }
            3625 => {
                let Some(old) = app.store.find_msg(ch, self.fx.msg).map(|m| m.content.clone()) else { return };
                self.fx.text = old;
                app.edit_msg(ch, self.fx.msg, "an edit that discord refuses".into());
                self.next(0);
            }
            3626 => {
                if !toast_is(app, "Failed to update message") {
                    return;
                }
                if app.store.find_msg(ch, self.fx.msg).is_some_and(|m| m.content == self.fx.text) && detail_is(app, "Must be 32 or fewer in length.") {
                    self.mark("fail_edit_rolled_back");
                    self.mark("fail_field_error");
                }
                refuse(app, "PUT", "/reactions/", 403);
                self.next(200);
            }
            3627 => {
                app.react(ch, self.fx.msg, crate::state::ReactEmoji { id: 0, name: "🎉".into(), animated: false }, true);
                self.next(0);
            }
            3628 => {
                if !toast_is(app, "Reaction failed to send") {
                    return;
                }
                if app.store.find_msg(ch, self.fx.msg).is_some_and(|m| m.reactions.len() as u64 == self.fx.flag) {
                    self.mark("fail_react_rolled_back");
                }
                refuse(app, "DELETE", "/members/", 403);
                self.next(200);
            }
            3629 => {
                app.kick(g, 102);
                self.next(300);
            }
            3630 => {
                self.key(Key::Enter);
                self.next(0);
            }
            3631 => {
                let name = app.store.name_in(g, 102);
                if !toast_is(app, &format!("Whoops... {name} could not be kicked.  Please try again!")) {
                    return;
                }
                self.mark("fail_kick_toast");
                if detail_is(app, "Missing Permissions") {
                    self.mark("fail_kick_reason");
                }
                self.next(500);
                self.step = 3650;
            }
            3650 => {
                self.shot(ctx, "fail_kick.png");
                refuse(app, "DELETE", "/messages/", 429);
                self.next(200);
                self.step = 3632;
            }
            3632 => {
                app.delete_msg(ch, self.fx.msg, false);
                self.next(0);
            }
            3633 => {
                if !toast_is(app, "Failed to delete message") || !detail_is(app, "You are being rate limited.") {
                    return;
                }
                self.mark("fail_ratelimit_text");
                refuse(app, "DELETE", "/messages/", 502);
                self.next(200);
            }
            3634 => {
                app.delete_msg(ch, self.fx.msg, false);
                self.next(0);
            }
            3635 => {
                if !toast_is(app, "Failed to delete message") || !app.toast_detail.as_deref().is_some_and(|d| d.starts_with("Oh no, it looks like the Discord servers are having issues")) {
                    return;
                }
                self.mark("fail_5xx_text");
                let Some(other) = app.store.guilds.get(&g).and_then(|x| x.channels.iter().copied().find(|c| *c != ch && !app.store.msgs.contains_key(c) && app.store.channels.get(c).is_some_and(|c| c.kind == 0))) else {
                    return self.fail("no second text channel to fail loading".into());
                };
                self.fx.msg = other;
                app.rest("POST", "/mock/fail".into(), Some(json!({ "method": "GET", "path": format!("/channels/{other}/messages"), "status": 500 })), crate::net::Tag::Quiet);
                self.next(4800);
            }
            3636 => {
                app.open(self.fx.msg);
                self.next(0);
            }
            3637 => {
                if !app.store.msgs.get(&self.fx.msg).is_some_and(|e| e.failed.is_some()) {
                    return;
                }
                self.mark("messages_failed_state");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(700);
            }
            3638 => {
                if !app.rects.contains_key("messages:retry") {
                    return;
                }
                self.shot(ctx, "messages_failed_small.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
                self.next(700);
            }
            3639 => {
                let Some(r) = app.rects.get("messages:retry").copied() else { return };
                self.shot(ctx, "messages_failed.png");
                self.click(r.center());
                self.next(0);
            }
            3640 => {
                let Some(e) = app.store.msgs.get(&self.fx.msg) else { return };
                if e.failed.is_some() || e.loading || !e.fresh {
                    return;
                }
                self.mark("messages_retry_loaded");
                self.step = 3700;
            }
            _ => self.step = 3700,
        }
    }
}
