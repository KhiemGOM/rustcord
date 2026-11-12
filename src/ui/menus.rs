//! The right-click menus on a server's name, its channels and its categories.

use egui::{Sense, Ui, pos2, vec2};

use super::icons::Icon;
use super::{LEVELS, Trail, menu_item, menu_row, menu_sep};
use crate::state::{Channel, Id, Store};
use crate::theme::*;

#[derive(Clone, Copy)]
pub(super) enum Move {
    Category(Id),
    First,
    Last,
}

pub(super) enum ChanAct {
    Read,
    Mute(bool, Option<i64>),
    Level(Option<u8>),
    Invite,
    Link,
    CopyId,
    Edit,
    Delete,
    Duplicate,
    // a category's plus, or Create Text Channel on a channel: the create modal in that category
    Create,
    Collapse,
    CollapseAll,
    Pin,
    // onto the channel list of a server that only lists what was picked, or off it
    List(bool),
    HideNames,
    Chat,
    Move(Move),
}

pub(super) enum GuildAct {
    Web(String),
    Boosts,
    Settings,
    Create(super::create::Kind),
    Threads,
    Directory,
    Invite,
    Notifs,
    HideMuted,
    ShowAll,
    Tag,
    Privacy,
    Profile,
    Security,
    Raid,
    Leave,
    CopyId,
}

fn tag_row(ui: &mut Ui, wearer: &crate::state::User) -> bool {
    let chip = ui.painter().layout_no_wrap(wearer.tag.as_ref().map(|t| t.0.clone()).unwrap_or_default(), semi(12.0), pal().header_primary);
    let w = 8.0 + 8.0 + if wearer.tag_badge_url().is_some() { 16.0 } else { 0.0 } + chip.size().x + 10.0 + 70.0;
    let (r, resp) = ui.allocate_at_least(vec2(w, 34.0), Sense::click());
    crate::ui::motion::wash(ui, &resp, r, 4, MENU_HOVER, 0.05);
    let end = super::tag_chip(ui.painter(), wearer, pos2(r.left() + 8.0, r.center().y), 1.0).map_or(r.left() + 8.0, |c| c.right());
    ui.painter().text(pos2(end + 10.0, r.center().y), egui::Align2::LEFT_CENTER, "Server Tag", medium(14.0), pal().header_primary);
    resp.clicked()
}

/// The menu under the server name.
pub(super) fn guild_menu(ui: &mut Ui, store: &Store, gid: Id, can_invite: bool, dev: bool) -> Option<GuildAct> {
    use crate::state::{MANAGE_CHANNELS, MANAGE_EVENTS, MANAGE_GUILD, SETTINGS_ACCESS, VIEW_GUILD_INSIGHTS};
    let g = store.guilds.get(&gid)?;
    let perms = store.my_perms(gid);
    let owner = g.owner == store.me;
    let hide = store.hide_muted.contains(&gid);
    let has = |f: &str| g.features.iter().any(|x| x == f);
    let community = has("COMMUNITY");
    let web = || Some(GuildAct::Web(format!("https://discord.com/developers/servers/{gid}")));
    let mut act = None;
    if menu_item(ui, Some(Icon::Gem), "Server Boost", false) {
        act = Some(GuildAct::Boosts);
    }
    if let Some(w) = store.users.values().find(|u| u.tag.as_ref().is_some_and(|t| t.1 == gid))
        && tag_row(ui, w)
    {
        act = Some(GuildAct::Tag);
    }
    menu_sep(ui);
    if can_invite && menu_item(ui, Some(Icon::PersonAdd), "Invite to Server", false) {
        act = Some(GuildAct::Invite);
    }
    if perms & SETTINGS_ACCESS != 0 && menu_item(ui, Some(Icon::Gear), "Server Settings", false) {
        act = Some(GuildAct::Settings);
    }
    if community && perms & VIEW_GUILD_INSIGHTS != 0 && menu_item(ui, Some(Icon::Insights), "Server Insights", false) {
        act = web();
    }
    if perms & MANAGE_CHANNELS != 0 {
        if menu_item(ui, Some(Icon::CirclePlus), "Create Channel", false) {
            act = Some(GuildAct::Create(super::create::Kind::Channel));
        }
        if menu_item(ui, Some(Icon::FolderPlus), "Create Category", false) {
            act = Some(GuildAct::Create(super::create::Kind::Category));
        }
    }
    if perms & MANAGE_EVENTS != 0 && menu_item(ui, Some(Icon::CalendarPlus), "Create Event", false) {
        act = Some(GuildAct::Create(super::create::Kind::Event));
    }
    if store.channels.values().any(|c| c.guild == gid && c.is_thread()) && menu_item(ui, Some(Icon::Bubble), "Active Threads", false) {
        act = Some(GuildAct::Threads);
    }
    if menu_item(ui, Some(Icon::Apps), "App Directory", false) {
        act = Some(GuildAct::Directory);
    }
    menu_sep(ui);
    if community && menu_row(ui, Some(Icon::Eye), "Show All Channels", None, false, Trail::Check(!store.opt_in.contains(&gid))).clicked() {
        act = Some(GuildAct::ShowAll);
    }
    if menu_item(ui, Some(Icon::Bell), "Notification Settings", false) {
        act = Some(GuildAct::Notifs);
    }
    if menu_item(ui, Some(Icon::Shield), "Privacy Settings", false) {
        act = Some(GuildAct::Privacy);
    }
    menu_sep(ui);
    if menu_item(ui, Some(Icon::Pencil), "Edit Per-server Profile", false) {
        act = Some(GuildAct::Profile);
    }
    if menu_row(ui, Some(Icon::EyeOff), "Hide Muted Channels", None, false, Trail::Check(hide)).clicked() {
        act = Some(GuildAct::HideMuted);
    }
    let secure = community && perms & MANAGE_GUILD != 0;
    if secure || !owner {
        menu_sep(ui);
    }
    if secure {
        if menu_item(ui, Some(Icon::Lock), "Security Actions", true) {
            act = Some(GuildAct::Security);
        }
        if menu_item(ui, Some(Icon::Shield), "Report Raid", true) {
            act = Some(GuildAct::Raid);
        }
    }
    if !owner && menu_item(ui, Some(Icon::Leave), "Leave Server", true) {
        act = Some(GuildAct::Leave);
    }
    if dev {
        menu_sep(ui);
        if menu_item(ui, Some(Icon::Id), "Copy Server ID", false) {
            act = Some(GuildAct::CopyId);
        }
    }
    act
}

fn move_menu(ui: &mut Ui, store: &Store, c: &Channel) -> Option<Move> {
    let mut mv = None;
    let m = menu_row(ui, None, "Move to...", None, false, Trail::Caret);
    egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
        let peers = super::reorder::peers(store, c);
        let at = peers.iter().position(|x| *x == c.id);
        let cat = c.kind == 4;
        if !cat {
            let sub = menu_row(ui, None, "Category", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &sub, |ui| {
                if ui.add_enabled_ui(c.parent != 0, |ui| menu_item(ui, None, "Uncategorized", false)).inner {
                    mv = Some(Move::Category(0));
                }
                menu_sep(ui);
                for k in super::reorder::categories(store, c.guild) {
                    if ui.add_enabled_ui(c.parent != k.id, |ui| menu_item(ui, None, &k.name, false)).inner {
                        mv = Some(Move::Category(k.id));
                    }
                }
            });
            menu_sep(ui);
        }
        let (first, last) = if cat { ("First in List", "Last in List") } else { ("First in Category", "Last in Category") };
        if ui.add_enabled_ui(at != Some(0), |ui| menu_item(ui, None, first, false)).inner {
            mv = Some(Move::First);
        }
        if ui.add_enabled_ui(at != peers.len().checked_sub(1), |ui| menu_item(ui, None, last, false)).inner {
            mv = Some(Move::Last);
        }
    });
    mv
}

/// Discord's right-click menu for a channel, thread or category.
#[allow(clippy::too_many_arguments)]
pub(super) fn channel_menu(r: &egui::Response, store: &Store, c: &Channel, unread: bool, collapsed: bool, dev: bool, hide_names: bool, out: &mut Option<(Id, ChanAct)>) {
    let cat = c.kind == 4;
    let voice = c.is_voice();
    let what = if cat { "Category" } else if c.is_thread() { "Thread" } else { "Channel" };
    let perms = store.perms(c);
    let pinned = store.chan_flags.get(&c.id).is_some_and(|f| f & super::PINNED != 0);
    egui::Popup::context_menu(r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| super::menu_body(ui, |ui| {
        let mut act = None;
        if ui.add_enabled_ui(unread, |ui| menu_item(ui, None, "Mark As Read", false)).inner {
            act = Some(ChanAct::Read);
        }
        menu_sep(ui);
        let listed = store.opted.contains(&c.id);
        let list = if listed { "Remove from Channel List" } else { "Add to Channel List" };
        let picks = store.opt_in.contains(&c.guild) && !c.is_thread();
        if cat {
            if picks && menu_item(ui, None, list, false) {
                act = Some(ChanAct::List(!listed));
            }
            if menu_row(ui, None, "Collapse Category", None, false, Trail::Check(collapsed)).clicked() {
                act = Some(ChanAct::Collapse);
            }
            if menu_item(ui, None, "Collapse All Categories", false) {
                act = Some(ChanAct::CollapseAll);
            }
        } else {
            if !c.is_thread() && store.can_invite(c) && menu_item(ui, None, if voice { "Invite to Voice" } else { "Invite to Channel" }, false) {
                act = Some(ChanAct::Invite);
            }
            if !c.is_thread() && menu_item(ui, None, if pinned { "Unpin Channel" } else { "Pin Channel to Top" }, false) {
                act = Some(ChanAct::Pin);
            }
            if picks && menu_item(ui, None, list, false) {
                act = Some(ChanAct::List(!listed));
            }
            if menu_item(ui, None, "Copy Link", false) {
                act = Some(ChanAct::Link);
            }
            if voice {
                menu_sep(ui);
                if menu_item(ui, None, "Open Chat", false) {
                    act = Some(ChanAct::Chat);
                }
                if menu_row(ui, None, "Hide Names", None, false, Trail::Check(hide_names)).clicked() {
                    act = Some(ChanAct::HideNames);
                }
            }
        }
        menu_sep(ui);
        if store.muted.contains(&c.id) {
            if menu_item(ui, None, &format!("Unmute {what}"), false) {
                act = Some(ChanAct::Mute(false, None));
            }
        } else {
            let m = menu_row(ui, None, &format!("Mute {what}"), None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                if let Some(secs) = super::mute_menu(ui) {
                    act = Some(ChanAct::Mute(true, secs));
                }
            });
        }
        if !voice {
            let level = store.levels.get(&c.id).copied();
            let inherited = store.inherited_level(c);
            let n = menu_row(ui, None, "Notification Settings", Some(LEVELS[level.unwrap_or(inherited).min(2) as usize]), false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &n, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let default = if c.parent != 0 && !c.is_thread() { "Use Category Default" } else { "Use Server Default" };
                if let Some(l) = super::level_menu(ui, level, inherited, default) {
                    act = Some(ChanAct::Level(l));
                }
            });
        }
        if perms & crate::state::MANAGE_CHANNELS != 0 && !c.is_thread() {
            menu_sep(ui);
            if let Some(m) = move_menu(ui, store, c) {
                act = Some(ChanAct::Move(m));
            }
            menu_sep(ui);
            let create = if voice { "Create Voice Channel" } else { "Create Text Channel" };
            let edits: Vec<(&str, bool)> = if cat {
                vec![("Edit Category", false), ("Delete Category", true)]
            } else {
                vec![("Edit Channel", false), ("Duplicate Channel", false), (create, false), ("Delete Channel", true)]
            };
            for (label, danger) in edits {
                if menu_item(ui, None, label, danger) {
                    act = Some(match label {
                        l if l.starts_with("Create") => ChanAct::Create,
                        "Duplicate Channel" => ChanAct::Duplicate,
                        "Delete Channel" | "Delete Category" => ChanAct::Delete,
                        _ => ChanAct::Edit,
                    });
                }
            }
        }
        if dev {
            menu_sep(ui);
            if menu_item(ui, Some(Icon::Id), &format!("Copy {what} ID"), false) {
                act = Some(ChanAct::CopyId);
            }
        }
        if let Some(a) = act {
            *out = Some((c.id, a));
        }
    }));
}
