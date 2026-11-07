use egui::{Pos2, Rect, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, Trail, menu_row, menu_sep};
use super::icons::Icon;
use crate::net::Tag;
use crate::state::{Id, Store};
use crate::theme::*;

/// Where the person's menu was opened from; the call and friends menus carry more rows.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Origin {
    #[default]
    Plain,
    Voice(Id),
    Friends,
}

enum Act {
    Read,
    Pin(bool),
    EditGroup,
    Profile,
    Mention,
    Message,
    Call,
    Note,
    FriendNick,
    Nick,
    Invite(Id),
    Command(Value),
    ManageApp,
    EditProfile,
    Close,
    Leave,
    AddFriend,
    RemoveFriend,
    Ignore(bool),
    Block,
    Unblock,
    Mute(bool, Option<i64>),
    PersonMute(bool),
    Soundboard(bool),
    Focus,
    Role(Id, bool),
    Move(Id),
    ServerMute(bool),
    ServerDeaf(bool),
    Disconnect,
    ModView,
    Timeout,
    Kick,
    Ban,
    CopyUser,
    CopyChannel,
}

pub fn menu_heading(ui: &mut Ui, text: &str) {
    let (r, _) = ui.allocate_at_least(vec2(0.0, 24.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, text, semi(12.0), pal().text_muted);
}

/// A row with a label over a slider, like discord's User Volume; `at` is the thumb's place from
/// 0 to 1. Gives the new place while it's dragged or clicked, and whether the pointer let go.
pub fn menu_slider(ui: &mut Ui, label: &str, at: f32, value: Option<&str>) -> Option<(f32, bool)> {
    let (r, resp) = ui.allocate_at_least(vec2(160.0, 58.0), Sense::click_and_drag());
    super::settings::mark(ui, &format!("menu_row:{label}"), r);
    let track = Rect::from_min_size(pos2(r.left() + 8.0, r.top() + 40.0), vec2(r.width() - 16.0, 4.0));
    let p = ui.painter();
    p.text(pos2(r.left() + 8.0, r.top() + 17.0), egui::Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary);
    if let Some(v) = value {
        p.text(pos2(r.right() - 8.0, r.top() + 17.0), egui::Align2::RIGHT_CENTER, v, reg(14.0), pal().text_muted);
    }
    p.rect_filled(track, 2, pal().bg_mod_selected);
    let x = track.left() + track.width() * at.clamp(0.0, 1.0);
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), 2, BLURPLE);
    p.circle(pos2(x, track.center().y), 7.0, egui::Color32::WHITE, egui::Stroke::new(1.0, egui::Color32::from_black_alpha(40)));
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let held = resp.dragged() || resp.clicked();
    let done = resp.drag_stopped() || resp.clicked();
    let t = resp.interact_pointer_pos().map(|p| ((p.x - track.left()) / track.width()).clamp(0.0, 1.0));
    t.filter(|_| held || done).map(|t| (t, done))
}

/// Discord's slider is on a perceptual scale so 100% sits mid-track: 0 to 200% of amplitude.
pub fn vol_pos(gain: f32) -> f32 {
    let n = gain.max(0.0);
    (if n < 1.0 { n.powf(0.357_142_85) } else { 20.0 * n.log10() / 6.0 + 1.0 }) / 2.0
}

pub fn vol_gain(pos: f32) -> f32 {
    let n = pos.clamp(0.0, 1.0) * 2.0;
    if n < 1.0 { n.powf(2.8) } else { 10f32.powf((n - 1.0) * 6.0 / 20.0) }
}

/// A server's rank for what the menu may do to a member: the owner outranks everyone, else the
/// highest role position wins.
pub(super) fn rank(store: &Store, gid: Id, uid: Id) -> i64 {
    let Some(g) = store.guilds.get(&gid) else { return 0 };
    if g.owner == uid {
        return i64::MAX;
    }
    let roles = if uid == store.me { Some(&g.my_roles) } else { store.members.get(&gid).and_then(|m| m.get(&uid)).map(|m| &m.roles) };
    roles.into_iter().flatten().filter_map(|r| g.roles.get(r)).map(|r| r.position as i64).max().unwrap_or(0)
}

pub fn user_menu(app: &mut App, ctx: &egui::Context) {
    use crate::state::*;
    let Some((uid, gid, dm, at)) = app.user_menu else { return };
    let origin = app.menu_from;
    let store = &app.store;
    let me = uid == store.me;
    let user = store.user(uid).cloned();
    let bot = user.as_ref().is_some_and(|u| u.bot);
    let rel = store.relationships.get(&uid).copied();
    let name = store.name_in(gid, uid);
    let chan = dm.and_then(|c| store.channels.get(&c));
    let group = chan.is_some_and(|c| c.kind == 3);
    let unread = chan.is_some_and(|c| store.unread(c));
    let muted = dm.is_some_and(|c| store.muted.contains(&c));
    let pinned = dm.is_some_and(|c| store.pinned.contains(&c));
    let vch = if let Origin::Voice(c) = origin { Some(c) } else { None };
    let ctx_ch = dm.or(vch).or(if origin == Origin::Friends { None } else { app.cur() });
    let composing = dm.is_none() && origin != Origin::Friends && app.cur().is_some();
    let dev = app.settings.dev;
    let ignored = store.ignored.contains(&uid);
    let perms = if gid == 0 { 0 } else { store.my_perms(gid) };
    let outranks = gid != 0 && !me && rank(store, gid, store.me) > rank(store, gid, uid);
    let (can_timeout, can_kick, can_ban) = (outranks && perms & MODERATE_MEMBERS != 0, outranks && perms & KICK_MEMBERS != 0, outranks && perms & BAN_MEMBERS != 0);
    let can_nick = gid != 0 && !me && outranks && perms & MANAGE_NICKNAMES != 0;
    let can_roles = gid != 0 && perms & MANAGE_ROLES != 0 && (me || outranks);
    let can_mod = gid != 0 && perms & (KICK_MEMBERS | BAN_MEMBERS | MODERATE_MEMBERS | MANAGE_ROLES | MANAGE_GUILD) != 0;
    let in_voice = vch.is_some();
    let vstate = app.voice.muted.get(&uid).copied().unwrap_or_default();
    let person = app.peer(uid);
    let has_video = app.voice.cams.contains(&uid) || app.voice.live.contains(&uid);
    let can_move = in_voice && !me && perms & MOVE_MEMBERS != 0;
    let can_server_mute = in_voice && !me && perms & MUTE_MEMBERS != 0;
    let can_server_deaf = in_voice && !me && perms & DEAFEN_MEMBERS != 0;
    let note = app.notes.get(&uid).map(|n| !n.is_empty());
    let friend_nick = store.nicks.contains_key(&uid);
    let integration = bot && gid != 0 && perms & MANAGE_GUILD != 0;
    let nick_label = if friend_nick { "Change Friend Nickname" } else { "Add Friend Nickname" };

    let invites: Vec<(Id, String, Id)> = if me || bot || group {
        Vec::new()
    } else {
        store
            .guild_order
            .iter()
            .filter_map(|g| store.guilds.get(g))
            .filter(|g| store.members.get(&g.id).is_none_or(|m| !m.contains_key(&uid)) && store.my_perms(g.id) & CREATE_INVITE != 0)
            .filter_map(|g| {
                let c = g.channels.iter().filter_map(|c| store.channels.get(c)).find(|c| matches!(c.kind, 0 | 2 | 5 | 13) && store.perms(c) & CREATE_INVITE != 0)?;
                Some((g.id, g.name.clone(), c.id))
            })
            .collect()
    };
    let roles: Vec<(Id, String, bool, bool)> = if !can_roles {
        Vec::new()
    } else {
        let g = &store.guilds[&gid];
        let held = store.members.get(&gid).and_then(|m| m.get(&uid)).map(|m| m.roles.clone()).unwrap_or_else(|| if me { g.my_roles.clone() } else { Vec::new() });
        let mine = rank(store, gid, store.me);
        let mut list: Vec<_> = g.roles.values().filter(|r| r.id != gid).collect();
        list.sort_by_key(|r| std::cmp::Reverse(r.position));
        list.into_iter().map(|r| (r.id, r.name.clone(), held.contains(&r.id), r.managed || (g.owner != store.me && r.position as i64 >= mine))).collect()
    };
    let moves: Vec<(Id, String)> = if !can_move {
        Vec::new()
    } else {
        let here = store.guilds.get(&gid).and_then(|g| g.voice.get(&uid).copied());
        store
            .guilds
            .get(&gid)
            .into_iter()
            .flat_map(|g| g.channels.iter())
            .filter_map(|c| store.channels.get(c))
            .filter(|c| c.is_voice() && Some(c.id) != here)
            .map(|c| (c.id, c.name.clone()))
            .collect()
    };
    let apps = commands_for(app, ctx_ch, gid, uid, group);

    let mut act = None;
    let mut slider = None;
    let open = open(ctx, at, |ui| {
        if group {
            if ui.add_enabled_ui(unread, |ui| menu_row(ui, None, "Mark As Read", None, false, Trail::None)).inner.clicked() {
                act = Some(Act::Read);
            }
            menu_sep(ui);
            if menu_row(ui, None, if pinned { "Unpin" } else { "Pin" }, None, false, Trail::None).clicked() {
                act = Some(Act::Pin(!pinned));
            }
            menu_sep(ui);
            if menu_row(ui, None, "Edit Group", None, false, Trail::None).clicked() {
                act = Some(Act::EditGroup);
            }
            menu_sep(ui);
            mute_rows(ui, "Conversation", muted, &mut act);
            menu_sep(ui);
            if menu_row(ui, None, "Leave Group", None, true, Trail::None).clicked() {
                act = Some(Act::Leave);
            }
            if dev {
                menu_sep(ui);
                if menu_row(ui, Some(Icon::Id), "Copy Channel ID", None, false, Trail::None).clicked() {
                    act = Some(Act::CopyChannel);
                }
            }
            return;
        }
        if dm.is_some() {
            if ui.add_enabled_ui(unread, |ui| menu_row(ui, None, "Mark As Read", None, false, Trail::None)).inner.clicked() {
                act = Some(Act::Read);
            }
            menu_sep(ui);
            if menu_row(ui, None, if pinned { "Unpin" } else { "Pin" }, None, false, Trail::None).clicked() {
                act = Some(Act::Pin(!pinned));
            }
            menu_sep(ui);
        }
        let row = |ui: &mut Ui, label: &str, sub: Option<&str>, danger: bool, trail: Trail| menu_row(ui, None, label, sub, danger, trail).clicked();
        if row(ui, "Profile", None, false, Trail::None) {
            act = Some(Act::Profile);
        }
        if composing && row(ui, "Mention", None, false, Trail::None) {
            act = Some(Act::Mention);
        }
        if !me && dm.is_none() && row(ui, "Message", None, false, Trail::None) {
            act = Some(Act::Message);
        }
        if !me && dm.is_none() && !bot && row(ui, "Start a Call", None, false, Trail::None) {
            act = Some(Act::Call);
        }
        if !me && !crate::theme::view().hide_personal {
            let (label, sub) = match note {
                None => ("Loading Note", Some("Only visible to you")),
                Some(true) => ("Edit Note", Some("Only visible to you")),
                Some(false) => ("Add Note", Some("Only visible to you")),
            };
            if row(ui, label, sub, false, Trail::None) {
                act = Some(Act::Note);
            }
        }
        if rel == Some(FRIEND) && !bot && row(ui, nick_label, None, false, Trail::None) {
            act = Some(Act::FriendNick);
        }
        if dm.is_some() && row(ui, "Close DM", None, false, Trail::None) {
            act = Some(Act::Close);
        }
        if me && gid != 0 {
            menu_sep(ui);
            if row(ui, "Edit Per-server Profile", None, false, Trail::None) {
                act = Some(Act::EditProfile);
            }
        } else if !in_voice && !me {
            menu_sep(ui);
        }
        if in_voice && !me {
            menu_sep(ui);
            if let Some((t, done)) = menu_slider(ui, "User Volume", vol_pos(person.volume / 100.0), None) {
                slider = Some(((vol_gain(t) * 100.0).round(), done));
            }
            menu_sep(ui);
            if row(ui, "Mute", None, false, Trail::Check(person.muted)) {
                act = Some(Act::PersonMute(!person.muted));
            }
            if row(ui, "Mute Soundboard", None, false, Trail::Check(person.soundboard_muted)) {
                act = Some(Act::Soundboard(!person.soundboard_muted));
            }
            if has_video && row(ui, "Focus This Person", None, false, Trail::None) {
                act = Some(Act::Focus);
            }
        }
        if can_nick && row(ui, "Change Nickname", None, false, Trail::None) {
            act = Some(Act::Nick);
        }
        if ctx_ch.is_some() {
            let a = menu_row(ui, None, "Apps", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &a, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if apps.is_empty() {
                    ui.add_enabled_ui(false, |ui| menu_row(ui, None, "No Apps", None, false, Trail::None));
                } else {
                    menu_heading(ui, "Apps");
                }
                for (app_name, cmds) in &apps {
                    let m = menu_row(ui, None, app_name, None, false, Trail::Caret);
                    egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for c in cmds {
                            if menu_row(ui, None, c["name"].as_str().unwrap_or(""), None, false, Trail::None).clicked() {
                                act = Some(Act::Command(c.clone()));
                            }
                        }
                    });
                }
                if integration {
                    menu_sep(ui);
                    if menu_row(ui, None, "Manage Server Integration", None, false, Trail::None).clicked() {
                        act = Some(Act::ManageApp);
                    }
                }
            });
        }
        if !invites.is_empty() {
            let m = menu_row(ui, None, "Invite to Server", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (_, n, c) in &invites {
                    if menu_row(ui, None, n, None, false, Trail::None).clicked() {
                        act = Some(Act::Invite(*c));
                    }
                }
            });
        }
        if !me && !bot {
            match rel {
                Some(FRIEND) => {
                    if row(ui, "Remove Friend", None, false, Trail::None) {
                        act = Some(Act::RemoveFriend);
                    }
                }
                Some(BLOCKED) | Some(OUTGOING) => {}
                Some(INCOMING) => {
                    if row(ui, "Accept Friend Request", None, false, Trail::None) {
                        act = Some(Act::AddFriend);
                    }
                }
                _ => {
                    if row(ui, "Add Friend", None, false, Trail::None) {
                        act = Some(Act::AddFriend);
                    }
                }
            }
        }
        if !me {
            if rel != Some(BLOCKED) && row(ui, if ignored { "Unignore" } else { "Ignore" }, None, false, Trail::None) {
                act = Some(Act::Ignore(!ignored));
            }
            if rel == Some(BLOCKED) {
                if row(ui, "Unblock", None, false, Trail::None) {
                    act = Some(Act::Unblock);
                }
            } else if row(ui, "Block", None, true, Trail::None) {
                act = Some(Act::Block);
            }
        }
        if !roles.is_empty() || !moves.is_empty() {
            menu_sep(ui);
        }
        if !roles.is_empty() {
            let m = menu_row(ui, None, "Roles", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (id, n, on, off) in &roles {
                    if ui.add_enabled_ui(!off, |ui| menu_row(ui, None, n, None, false, Trail::Check(*on))).inner.clicked() {
                        act = Some(Act::Role(*id, !on));
                    }
                }
            });
        }
        if !moves.is_empty() {
            let m = menu_row(ui, None, "Move to", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (id, n) in &moves {
                    if menu_row(ui, None, n, None, false, Trail::None).clicked() {
                        act = Some(Act::Move(*id));
                    }
                }
            });
        }
        let moderating = can_mod || can_server_mute || can_server_deaf || in_voice && can_move || can_timeout || can_kick || can_ban;
        if moderating {
            menu_sep(ui);
        }
        if can_mod && row(ui, "Open in Mod View", None, false, Trail::None) {
            act = Some(Act::ModView);
        }
        if can_server_mute && row(ui, "Server Mute", None, true, Trail::Check(vstate[1])) {
            act = Some(Act::ServerMute(!vstate[1]));
        }
        if can_server_deaf && row(ui, "Server Deafen", None, true, Trail::Check(vstate[3])) {
            act = Some(Act::ServerDeaf(!vstate[3]));
        }
        if can_move && row(ui, "Disconnect", None, true, Trail::None) {
            act = Some(Act::Disconnect);
        }
        if can_timeout && row(ui, &format!("Timeout {name}"), None, true, Trail::None) {
            act = Some(Act::Timeout);
        }
        if can_kick && row(ui, &format!("Kick {name}"), None, true, Trail::None) {
            act = Some(Act::Kick);
        }
        if can_ban && row(ui, &format!("Ban {name}"), None, true, Trail::None) {
            act = Some(Act::Ban);
        }
        if dm.is_some() {
            menu_sep(ui);
            mute_rows(ui, &format!("@{name}"), muted, &mut act);
        }
        if dev {
            menu_sep(ui);
            if row(ui, "Copy User ID", None, false, Trail::None) {
                act = Some(Act::CopyUser);
            }
            if dm.is_some() && row(ui, "Copy Channel ID", None, false, Trail::None) {
                act = Some(Act::CopyChannel);
            }
        }
    });
    if let Some((gain, done)) = slider {
        let mut p = app.peer(uid);
        p.volume = gain;
        app.set_peer(uid, p, done);
    }
    if !open || act.is_some() {
        app.user_menu = None;
        app.menu_from = Origin::Plain;
    }
    run(app, ctx, act, uid, gid, dm, at, &name, vch);
}

fn mute_rows(ui: &mut Ui, what: &str, muted: bool, act: &mut Option<Act>) {
    if muted {
        if menu_row(ui, None, &format!("Unmute {what}"), None, false, Trail::None).clicked() {
            *act = Some(Act::Mute(false, None));
        }
        return;
    }
    let m = menu_row(ui, None, &if what == "Conversation" { "Mute Conversation".to_string() } else { format!("Mute {what}") }, None, false, Trail::Caret);
    egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if let Some(secs) = super::mute_menu(ui) {
            *act = Some(Act::Mute(true, secs));
        }
    });
}

/// The menu popup, held open through clicks inside it so sliders and ticks don't close it.
fn open(ctx: &egui::Context, at: Pos2, content: impl FnOnce(&mut Ui)) -> bool {
    let mut open = true;
    // as tall as its rows need, held inside the window: a menu opened low on the screen moves up instead of being cut off
    let screen = ctx.content_rect();
    let h = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("menu_h"))).unwrap_or(0.0).min(screen.height() - 32.0);
    let at = pos2(at.x, at.y.min(screen.bottom() - h - 16.0).max(screen.top() + 16.0));
    egui::Popup::new(egui::Id::new("user_menu"), ctx.clone(), egui::PopupAnchor::Position(at), egui::LayerId::background())
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .frame(egui::Frame::menu(&ctx.global_style()))
        .align(egui::RectAlign::BOTTOM_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .open_bool(&mut open)
        .show(|ui| super::menu_body(ui, content));
    open
}

/// The apps with a user command, from the index of the server (or dm) and of your own apps.
fn commands_for(app: &mut App, ch: Option<Id>, gid: Id, uid: Id, group: bool) -> Vec<(String, Vec<Value>)> {
    let Some(ch) = ch.filter(|_| !group) else { return Vec::new() };
    if let Some(c) = app.store.channels.get(&ch).cloned() {
        app.ensure_commands(&c);
    }
    if !app.commands.contains_key(&0) {
        app.commands.insert(0, Default::default());
        app.rest("GET", "/users/@me/application-command-index".into(), None, Tag::Commands(0));
    }
    let scope = app.store.channels.get(&ch).map_or(gid, |c| if c.guild != 0 { c.guild } else { c.id });
    let mut out: Vec<(String, Vec<Value>)> = Vec::new();
    for s in [scope, 0] {
        let Some(c) = app.commands.get(&s) else { continue };
        for cmd in c.list.iter().filter(|c| c["type"].as_u64() == Some(2)) {
            let aid = crate::state::sid(&cmd["application_id"]);
            let n = c.apps.iter().find(|a| a.0 == aid).map_or_else(|| "App".to_string(), |a| a.1.clone());
            match out.iter_mut().find(|(k, _)| *k == n) {
                Some((_, l)) if !l.iter().any(|x| x["id"] == cmd["id"]) => l.push(cmd.clone()),
                Some(_) => {}
                None => out.push((n, vec![cmd.clone()])),
            }
        }
    }
    let _ = uid;
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out
}

#[allow(clippy::too_many_arguments)]
fn run(app: &mut App, ctx: &egui::Context, act: Option<Act>, uid: Id, gid: Id, dm: Option<Id>, at: Pos2, name: &str, vch: Option<Id>) {
    let Some(act) = act else { return };
    let chan = dm.unwrap_or(0);
    match act {
        Act::Read => app.mark_read(&[chan]),
        Act::Pin(on) => app.pin_dm(chan, on),
        Act::EditGroup => app.user_dialog = Some(super::userdlg::Dialog::group(&app.store, chan)),
        Act::Profile => app.open_profile(uid, gid, at),
        Act::Mention => app.mention(uid, gid),
        Act::Message => app.open_dm(uid),
        Act::Call => app.call_user(uid, false),
        Act::Note => app.user_dialog = Some(super::userdlg::Dialog::note(app, uid)),
        Act::FriendNick => app.user_dialog = Some(super::userdlg::Dialog::nick(&app.store, 0, uid)),
        Act::Nick => app.user_dialog = Some(super::userdlg::Dialog::nick(&app.store, gid, uid)),
        Act::Invite(c) => app.rest_in("POST", format!("/channels/{c}/invites"), Some(json!({ "max_uses": 1, "unique": true })), "User Invite Context Menu", Tag::InviteTo(uid)),
        Act::Command(cmd) => app.user_command(dm.or(vch), gid, &cmd, uid),
        Act::ManageApp => {
            app.open_server_settings(gid);
            if let Some(s) = &mut app.server {
                s.page = super::server::Page::Integrations;
            }
        }
        Act::EditProfile => app.settings_page = Some(super::settings::Page::Profiles),
        Act::Close | Act::Leave => super::popups::leave_dm(app, chan),
        Act::AddFriend => app.rest_in("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), "ContextMenu", Tag::Friend),
        Act::RemoveFriend => {
            let body = format!("Are you sure you want to remove {name} from your friends?");
            app.confirm = Some(super::Confirm::new(format!("Remove '{name}'"), body, "Remove Friend", move |app: &mut App| {
                app.rest_in("DELETE", format!("/users/@me/relationships/{uid}"), None, "ContextMenu", Tag::reason())
            }));
        }
        Act::Ignore(true) => {
            let body = "You won't see their messages unless you choose to, and they won't know you ignored them.";
            app.confirm = Some(super::Confirm::new(format!("Ignore {name}?"), body, "Ignore", move |app: &mut App| {
                app.store.ignored.insert(uid);
                app.rest_in("PUT", format!("/users/@me/relationships/{uid}/ignore"), None, "ContextMenu", Tag::rollback(move |app: &mut App| { app.store.ignored.remove(&uid); }))
            }));
        }
        Act::Ignore(false) => {
            app.store.ignored.remove(&uid);
            app.rest_in("DELETE", format!("/users/@me/relationships/{uid}/ignore"), None, "ContextMenu", Tag::rollback(move |app: &mut App| { app.store.ignored.insert(uid); }));
        }
        Act::Block => {
            let body = "They won't be able to message you, and you won't see their messages unless you choose to.";
            app.confirm = Some(super::Confirm::new(format!("Block {name}?"), body, "Block", move |app: &mut App| {
                app.rest_in("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({ "type": 2 })), "ContextMenu", Tag::reason())
            }));
        }
        Act::Unblock => app.rest_in("DELETE", format!("/users/@me/relationships/{uid}"), None, "ContextMenu", Tag::reason()),
        Act::Mute(m, secs) => app.mute_for(chan, false, m, secs),
        Act::PersonMute(on) => person(app, uid, |p| p.muted = on),
        Act::Soundboard(on) => person(app, uid, |p| p.soundboard_muted = on),
        Act::Focus => app.call.focus = if app.call.focus == Some(uid) { None } else { Some(uid) },
        Act::Role(r, on) => app.rest(if on { "PUT" } else { "DELETE" }, format!("/guilds/{gid}/members/{uid}/roles/{r}"), on.then(|| json!({})), Tag::reason()),
        Act::Move(c) => app.rest("PATCH", format!("/guilds/{gid}/members/{uid}"), Some(json!({ "channel_id": c.to_string() })), Tag::reason()),
        Act::ServerMute(on) => app.rest("PATCH", format!("/guilds/{gid}/members/{uid}"), Some(json!({ "mute": on })), Tag::reason()),
        Act::ServerDeaf(on) => app.rest("PATCH", format!("/guilds/{gid}/members/{uid}"), Some(json!({ "deaf": on })), Tag::reason()),
        Act::Disconnect => app.rest("PATCH", format!("/guilds/{gid}/members/{uid}"), Some(json!({ "channel_id": null })), Tag::reason()),
        Act::ModView => app.open_mod_view(gid, uid),
        Act::Timeout if ctx.input(|i| i.modifiers.ctrl) => app.time_out(gid, uid, 60, ""),
        Act::Timeout => app.open_timeout(gid, uid),
        Act::Kick => app.kick(gid, uid),
        Act::Ban => app.ban(gid, uid),
        Act::CopyUser => ctx.copy_text(uid.to_string()),
        Act::CopyChannel => ctx.copy_text(chan.to_string()),
    }
}

fn person(app: &mut App, uid: Id, f: impl FnOnce(&mut crate::cache::Peer)) {
    let mut p = app.peer(uid);
    f(&mut p);
    app.set_peer(uid, p, true);
}

impl App {
    /// A dm's pin to the top of the list, kept in the channel's override flags as discord does.
    pub fn pin_dm(&mut self, ch: Id, on: bool) {
        if on {
            self.store.pinned.insert(ch);
        } else {
            self.store.pinned.remove(&ch);
        }
        let unreads = match self.store.unreads.get(&ch) {
            Some(true) => 1024,
            Some(false) => 512,
            None => 0,
        };
        let flags = unreads | if on { 2048 } else { 0 };
        let body = json!({ "channel_overrides": { ch.to_string(): { "flags": flags } } });
        self.guild_settings(0, body, Tag::fail(super::SAVE_FAILED));
    }

    /// Rings them: in the dm you have with them, made first when there isn't one.
    pub fn call_user(&mut self, uid: Id, video: bool) {
        self.voice.video_call = video;
        let existing = self.store.dms.iter().copied().find(|c| self.store.channels.get(c).is_some_and(|c| c.kind == 1 && c.recipients == [uid]));
        match existing {
            Some(c) => {
                self.ui.guild = None;
                self.open(c);
                self.join_voice(c, true);
            }
            None => self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [uid.to_string()] })), Tag::CallDm),
        }
    }

    /// An app's user command, run on someone from their menu.
    pub fn user_command(&mut self, ch: Option<Id>, gid: Id, cmd: &Value, target: Id) {
        let Some(ch) = ch.or_else(|| self.cur()) else { return };
        let Some(chan) = self.store.channels.get(&ch).cloned() else { return };
        let _ = gid;
        self.send_command(&chan, cmd, 2, Vec::new(), Vec::new(), Some(target), "user_context_menu");
    }

    /// An invite link into a dm with them, made first when there isn't one.
    pub fn send_invite(&mut self, uid: Id, code: &str) {
        let link = format!("https://discord.gg/{code}");
        let existing = self.store.dms.iter().copied().find(|c| self.store.channels.get(c).is_some_and(|c| c.kind == 1 && c.recipients == [uid]));
        match existing {
            Some(c) => self.rest("POST", format!("/channels/{c}/messages"), Some(json!({ "content": link })), Tag::fail(super::SEND_FAILED)),
            None => self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [uid.to_string()] })), Tag::InviteDm(link)),
        }
        self.toast("Invite sent.");
    }
}
