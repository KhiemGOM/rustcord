use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Ui, pos2, vec2};

use super::{App, avatar, badge, channel_title, icon_button, status_dot};
use super::icons::{self, Icon};
use super::text;
use crate::state::{Channel, Id, Status, Store};
use crate::theme::*;

fn sort_key(c: &Channel) -> (bool, i32, Id) {
    (c.is_voice(), c.position, c.id)
}

/// Visible channels grouped under their categories, in sidebar order. `keep` stays in even when
/// onboarding's opt-in would leave it out, the way discord keeps the open channel.
pub fn tree(store: &Store, gid: Id, keep: Option<Id>) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
    let Some(g) = store.guilds.get(&gid) else { return Vec::new() };
    let all: Vec<&Channel> = g.channels.iter().filter_map(|c| store.channels.get(c)).collect();
    let visible: Vec<&Channel> = all
        .iter()
        .copied()
        .filter(|c| c.kind != 4 && !matches!(c.kind, 10..=12) && store.can_view(c) && (keep == Some(c.id) || store.listed(c)))
        .collect();
    let opt_in = store.opt_in.contains(&gid);
    let mut cats: Vec<&Channel> = all.iter().copied().filter(|c| c.kind == 4).collect();
    cats.sort_by_key(|c| (c.position, c.id));

    let mut out = Vec::new();
    let mut loose: Vec<&Channel> = visible.iter().copied().filter(|c| c.parent == 0 || !cats.iter().any(|k| k.id == c.parent)).collect();
    loose.sort_by_key(|c| sort_key(c));
    if !loose.is_empty() {
        out.push((None, loose));
    }
    for cat in cats {
        let mut kids: Vec<&Channel> = visible.iter().copied().filter(|c| c.parent == cat.id).collect();
        if kids.is_empty() && (opt_in || !store.can_view(cat)) {
            continue;
        }
        kids.sort_by_key(|c| sort_key(c));
        out.push((Some(cat), kids));
    }
    out
}

pub fn first_channel(store: &Store, gid: Id) -> Option<Id> {
    tree(store, gid, None).into_iter().flat_map(|(_, k)| k).find(|c| c.is_text()).map(|c| c.id)
}

/// How much of the bottom the floating account card takes, its 8 margin included. The rail and
/// the sidebar both stop short of it.
pub fn foot(app: &App) -> f32 {
    66.0 + if app.voice.chan.is_some() { 102.0 } else { 0.0 } + if app.game.is_some() { 57.0 } else { 0.0 }
}

/// Discord's account card, floating 8 in from the left of the server rail, the bottom, and the
/// sidebar's right edge. A game we detected gets the top of it, then the voice controls while
/// connected.
pub fn card(app: &mut App, ui: &mut Ui, rail: Rect, side: Rect) {
    let card = Rect::from_min_max(pos2(rail.left() + 8.0, side.bottom() - foot(app)), pos2(side.right() - 8.0, side.bottom() - 8.0));
    ui.painter().rect(card, 8, pal().bg_secondary_alt, egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    let user = Rect::from_min_max(pos2(card.left(), card.bottom() - 58.0), card.max);
    let mut top = card.top();
    if app.game.is_some() {
        let area = Rect::from_min_size(card.min + vec2(1.0, 1.0), vec2(card.width() - 2.0, 56.0));
        game_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)), area);
        top = area.bottom();
    }
    if app.voice.chan.is_some() {
        ui.painter().hline(card.x_range().shrink(1.0), user.top() - 0.5, egui::Stroke::new(1.0, pal().border_muted));
        let area = Rect::from_min_max(pos2(card.left() + 1.0, top + 1.0), pos2(card.right() - 1.0, user.top() - 1.0)).shrink(12.0);
        // children that take no room from the window's layout; the chat panel comes after
        super::call::panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)));
    }
    app.rects.insert("userpanel".into(), user);
    user_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(user.shrink2(vec2(12.0, 0.0)))));
}

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::Panel::bottom("me").exact_size(foot(app)).resizable(false).show_separator_line(false).frame(Frame::NONE).show(ui, |_| {});

    match app.ui.guild {
        Some(gid) => guild(app, ui, gid),
        None => dms(app, ui),
    }
}

fn header(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    header_ruled(ui, true, add);
}

/// The header with or without its hairline; a banner runs under it without one.
fn header_ruled(ui: &mut Ui, rule: bool, add: impl FnOnce(&mut Ui)) {
    egui::Panel::top("side_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(egui::Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            if rule {
                ui.painter().hline(r.x_range().expand(16.0), r.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
            }
            ui.horizontal_centered(add);
        });
}

fn guild(app: &mut App, ui: &mut Ui, gid: Id) {
    // html collapses runs of spaces in names, so discord never shows them
    let name = app.store.guilds.get(&gid).map(|g| g.name.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
    let badge = app.store.guilds.get(&gid).and_then(|g| {
        let has = |f: &str| g.features.iter().any(|x| x == f);
        if has("PARTNERED") {
            Some(Icon::BadgePartner)
        } else if has("COMMUNITY") {
            Some(if g.tier > 0 || g.boosts >= 2 { Icon::BadgeCommunityBoosted } else { Icon::BadgeCommunity })
        } else {
            None
        }
    });
    // anything else that's boosted gets discord's grey flower with the gem in it
    let boosted = app.store.guilds.get(&gid).filter(|g| badge.is_none() && (g.tier > 0 || g.boosts > 0)).map(|g| g.tier);
    let can_invite = app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.guild == gid && app.store.perms(c) & crate::state::CREATE_INVITE != 0);
    let perms = app.store.my_perms(gid);
    let dev = app.settings.dev;
    let owner = app.store.guilds.get(&gid).is_some_and(|g| g.owner == app.store.me);
    let hide = app.store.hide_muted.contains(&gid);
    let community = app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "COMMUNITY"));
    let mut invite = false;
    let mut act = None;
    // a boosted server's banner sits behind the header and scrolls away with the list
    let banner = app.store.guilds.get(&gid).and_then(|g| g.banner_url());
    let scroll_id = egui::Id::new(("side_scroll", gid));
    // and once it has gone behind the header, the header is a plain one again
    let off: f32 = ui.ctx().data(|d| d.get_temp(scroll_id)).unwrap_or(0.0);
    let shown = banner.as_ref().filter(|_| off < 81.0);
    if let Some(url) = shown {
        let side = ui.available_rect_before_wrap();
        let r = Rect::from_min_size(pos2(side.left(), side.top() - off), vec2(side.width(), 130.0));
        let p = ui.painter().with_clip_rect(side);
        p.rect_filled(r, 0, pal().bg_mod_selected);
        // banners are 16:9; cover the 21:9 strip by trimming top and bottom
        let trim = (1.0 - (16.0 / 9.0) / (side.width() / 130.0)).max(0.0) / 2.0;
        let uv = Rect::from_min_max(pos2(0.0, trim), pos2(1.0, 1.0 - trim));
        egui::Image::from_uri(url.clone()).uv(uv).show_loading_spinner(false).paint_at(&ui.new_child(egui::UiBuilder::new().max_rect(side)), r);
        // discord darkens the top behind the name so white text reads on any banner
        let mut m = egui::Mesh::default();
        let shade = Rect::from_min_size(r.min, vec2(r.width(), 72.0));
        let dark = Color32::from_black_alpha(133);
        m.colored_vertex(shade.left_top(), dark);
        m.colored_vertex(shade.right_top(), dark);
        m.colored_vertex(shade.right_bottom(), Color32::TRANSPARENT);
        m.colored_vertex(shade.left_bottom(), Color32::TRANSPARENT);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(m);
    }
    let ink = if shown.is_some() { Color32::WHITE } else { pal().header_primary };
    header_ruled(ui, shown.is_none(), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let head = ui.max_rect().expand2(vec2(16.0, 0.0));
        let id = ui.id().with("guild_menu");
        let open = egui::Popup::is_id_open(ui.ctx(), id.with("popup"));
        let bg = ui.painter().add(egui::Shape::Noop);
        let left = ui.cursor().left();
        if let Some(b) = badge {
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::draw(ui.painter(), b, r, Color32::WHITE);
            ui.add_space(5.0);
        } else if let Some(tier) = boosted {
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::draw(ui.painter(), Icon::FlowerStar, r, pal().text_muted);
            let gem = match tier {
                0 => Color32::from_white_alpha(51),
                3 => Color32::from_rgb(0xff, 0x73, 0xfa),
                _ => Color32::WHITE,
            };
            icons::draw(ui.painter(), Icon::BoostGem, Rect::from_center_size(r.center(), vec2(10.0, 10.0)), gem);
            app.rects.insert("guild_boost_badge".into(), r);
            ui.add_space(5.0);
        }
        text::label(ui, &name, semi(16.0), ink, Sense::hover());
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        icons::draw(ui.painter(), if open { Icon::ChevronUp } else { Icon::ChevronDown }, r, ink);
        let btn = Rect::from_x_y_ranges(left - 8.0..=r.right() + 8.0, head.center().y - 16.0..=head.center().y + 16.0);
        let resp = ui.interact(btn, id, Sense::click());
        if open || resp.hovered() {
            ui.painter().set(bg, egui::Shape::rect_filled(btn, CornerRadius::same(8), MENU_HOVER));
        }
        app.rects.insert("guildmenu".into(), btn);
        egui::Popup::menu(&resp).anchor(head).align(egui::RectAlign::BOTTOM).gap(5.0).close_behavior(super::menu_closing(ui.ctx())).show(|ui| {
            super::menu_body(ui, |ui| act = guild_menu(ui, gid, perms, can_invite, owner, hide, dev, community));
        });
        if can_invite {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                icons::draw(ui.painter(), Icon::PersonAdd, r, if resp.hovered() { pal().interactive_hover } else { pal().header_secondary });
                invite = resp.on_hover_text("Invite to Server").clicked();
            });
        }
    });
    match act {
        Some(GuildAct::Web(url)) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
        Some(GuildAct::Boosts) => app.open_boosts(gid),
        Some(GuildAct::Settings) => app.open_server_settings(gid),
        Some(GuildAct::Create(k)) => app.create = Some(super::create::Form::new(k, gid, None)),
        Some(GuildAct::Threads) => app.active_threads = Some(gid),
        Some(GuildAct::Directory) => app.open_directory(),
        Some(GuildAct::Invite) => invite = true,
        Some(GuildAct::Notifs) => app.guild_notifs = Some(gid),
        Some(GuildAct::HideMuted) => toggle_hide_muted(app, gid),
        Some(GuildAct::Leave) => leave(app, gid),
        Some(GuildAct::CopyId) => ui.ctx().copy_text(gid.to_string()),
        None => {}
    }
    if invite && let Some(c) = app.cur() {
        app.invite_people(c);
    }

    // the server guide's checklist, fetched the first time the server is shown
    let guided = app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_SERVER_GUIDE"));
    if guided && !app.guides.contains_key(&gid) {
        app.guides.insert(gid, Default::default());
        app.rest("GET", format!("/guilds/{gid}/new-member-welcome"), None, crate::net::Tag::Welcome(gid));
        app.rest("GET", format!("/guilds/{gid}/new-member-actions"), None, crate::net::Tag::MemberActions(gid));
    }

    let cur = app.cur();
    let mut open: Option<Id> = None;
    let mut toggle: Option<Id> = None;
    let mut menu: Option<(Id, ChanAct)> = None;
    let mut make_invite: Option<Id> = None;
    let mut set_status: Option<Id> = None;
    let mut boost_page = false;
    let mut events_page = false;
    let mut open_chat: Option<Id> = None;
    let goal = app.boost_info.get(&gid).map(|i| i.goal()).filter(|g| *g > 0).unwrap_or(33);
    let mut home_page = None;
    let open_page = if app.boosts == Some(gid) {
        Some("Server Boosts")
    } else {
        match app.home.as_ref().filter(|h| h.guild == gid).map(|h| h.page) {
            Some(super::community::Page::Members) => Some("Members"),
            Some(super::community::Page::Channels(_)) => Some(if app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_ONBOARDING")) { "Channels & Roles" } else { "Browse Channels" }),
            None => None,
        }
    };
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // under a banner the list starts where the banner ends
            ui.add_space(if banner.is_some() { 81.0 } else { 8.0 });
            let store = &app.store;
            if let Some(g) = store.guilds.get(&gid).filter(|g| g.boost_bar) {
                boost_page |= boost_goal(ui, g.boosts, goal, banner.is_some());
            }
            let mut threads: Vec<&Channel> = store
                .channels
                .values()
                .filter(|c| c.guild == gid && c.is_thread() && c.joined && !c.archived)
                .collect();
            threads.sort_by_key(|t| t.id);
            // discord's "N New" on forums: open posts started since you last looked
            let mut fresh: std::collections::HashMap<Id, usize> = Default::default();
            for t in store.channels.values().filter(|t| t.guild == gid && t.is_thread() && !t.archived) {
                if store.channels.get(&t.parent).is_some_and(|f| f.is_forum()) && store.reads.get(&t.parent).is_some_and(|r| t.id > r.last_acked) {
                    *fresh.entry(t.parent).or_default() += 1;
                }
            }
            let revealed = egui::Id::new("side_revealed");
            if let Some(g) = store.guilds.get(&gid) {
                // discord counts only the steps in channels you can see
                let acts: Vec<Id> = app.guides.get(&gid).map(|(a, _)| a.iter().copied().filter(|c| store.channels.get(c).is_some_and(|c| store.can_view(c))).collect()).unwrap_or_default();
                if g.features.iter().any(|f| f == "COMMUNITY") && !acts.is_empty() && g.new_member() {
                    let done = app.guides.get(&gid).map_or(0, |(_, d)| acts.iter().filter(|c| d.contains(c)).count());
                    get_started(ui, gid, acts.len(), done);
                }
                match home_rows(ui, g, store.my_perms(gid), open_page) {
                    Some("Server Boosts") => boost_page = true,
                    Some("Events") => events_page = true,
                    Some("Members") => home_page = Some(super::community::Page::Members),
                    Some(_) => home_page = Some(super::community::Page::Channels(if g.features.iter().any(|f| f == "GUILD_ONBOARDING") { 0 } else { 1 })),
                    None => {}
                }
            }
            let hide = store.hide_muted.contains(&gid);
            for (cat, kids) in tree(store, gid, cur) {
                // hiding muted channels takes empty categories with them, but never the open channel
                let kids: Vec<&Channel> = kids.into_iter().filter(|c| !hide || cur == Some(c.id) || !store.is_muted(c)).collect();
                if hide && kids.is_empty() && cat.is_some() {
                    continue;
                }
                let collapsed = cat.is_some_and(|c| app.ui.collapsed.contains(&c.id));
                if let Some(cat) = cat {
                    // discord's category is 16 of padding over a 24 row, the first channel 1 under it
                    ui.add_space(15.0);
                    let (r, add) = category(ui, &cat.name, collapsed, store.perms(cat) & crate::state::MANAGE_CHANNELS != 0);
                    if r.clicked() {
                        toggle = Some(cat.id);
                    }
                    if add {
                        menu = Some((cat.id, ChanAct::Create));
                    }
                    let unread = kids.iter().any(|k| k.is_text() && store.unread(k));
                    channel_menu(&r, store, cat, unread, collapsed, dev, &mut menu);
                }
                for c in kids {
                    let selected = cur == Some(c.id);
                    let muted = store.is_muted(c);
                    let new = if muted { 0 } else { fresh.get(&c.id).copied().unwrap_or(0) };
                    let unread = (c.is_text() && store.unread(c)) || new > 0;
                    if collapsed && !selected && !unread {
                        continue;
                    }
                    let vr = c.is_voice().then(|| {
                        let here = app.voice.chan == Some(c.id);
                        let status = store.voice_status.get(&c.id).cloned();
                        let prompt = here && status.is_none() && store.perms(c) & crate::state::SET_VOICE_CHANNEL_STATUS != 0;
                        VoiceRow {
                            here,
                            timer: store.voice_start.get(&c.id).map(|t| elapsed(chrono::Utc::now().timestamp() - t)),
                            sub: status.or_else(|| prompt.then(|| "Set a channel status".to_string())),
                            prompt,
                        }
                    });
                    let (r, emoji, invite, edit, set, chat) = channel_row(ui, store, c, selected, unread, store.mentions(c.id), muted, new, vr.as_ref());
                    if set {
                        set_status = Some(c.id);
                    }
                    if chat {
                        open_chat = Some(c.id);
                    }
                    if let Some(v) = &vr {
                        if v.timer.is_some() {
                            app.rects.insert(format!("chan_timer:{}", c.id), r.rect);
                        }
                        if v.sub.is_some() {
                            app.rects.insert(format!("chan_status:{}", c.id), r.rect);
                        }
                    }
                    // like discord, bring a newly opened channel into view, once
                    if selected && ui.ctx().data(|d| d.get_temp::<Id>(revealed)) != Some(c.id) {
                        ui.scroll_to_rect(r.rect, None);
                        ui.ctx().data_mut(|d| d.insert_temp(revealed, c.id));
                    }
                    if invite {
                        make_invite = Some(c.id);
                    }
                    if edit {
                        menu = Some((c.id, ChanAct::Edit));
                    }
                    app.rects.insert(format!("chan:{}", c.id), r.rect);
                    if let Some(e) = emoji {
                        app.rects.insert(format!("chanemoji:{}", c.id), e);
                    }
                    app.rects.insert(format!("chanicon:{}", c.id), Rect::from_center_size(pos2(r.rect.left() + 26.0, r.rect.center().y), vec2(20.0, 20.0)));
                    if r.clicked() {
                        open = Some(c.id);
                    }
                    channel_menu(&r, store, c, unread, false, dev, &mut menu);
                    let kids: Vec<&&Channel> = threads.iter().filter(|t| t.parent == c.id).collect();
                    let n = kids.len();
                    for (i, t) in kids.into_iter().enumerate() {
                        let r = thread_row(ui, t, cur == Some(t.id), store.unread(t), i + 1 == n);
                        app.rects.insert(format!("chan:{}", t.id), r.rect);
                        if r.clicked() {
                            open = Some(t.id);
                        }
                        channel_menu(&r, store, t, store.unread(t), false, dev, &mut menu);
                    }
                    if c.is_voice() && !collapsed {
                        voice_users(ui, store, &app.voice, gid, c.id);
                    }
                }
            }
            ui.add_space(12.0);
        });
        ui.ctx().data_mut(|d| d.insert_temp(scroll_id, out.state.offset.y));
    });
    match menu {
        Some((c, ChanAct::Read)) => {
            let mut chans: Vec<Id> = app.store.channels.values().filter(|x| x.parent == c).map(|x| x.id).collect();
            chans.push(c);
            app.mark_read(&chans);
        }
        Some((c, ChanAct::Mute(m, secs))) => app.mute_for(c, false, m, secs),
        Some((c, ChanAct::Level(l))) => app.set_level(c, l),
        Some((c, ChanAct::Invite)) => make_invite = Some(c),
        Some((c, ChanAct::Link)) => ui.ctx().copy_text(format!("https://discord.com/channels/{gid}/{c}")),
        Some((c, ChanAct::CopyId)) => ui.ctx().copy_text(c.to_string()),
        Some((c, ChanAct::Edit)) => app.open_chan_settings(c),
        Some((c, ChanAct::Delete)) => delete_channel(app, c),
        Some((c, ChanAct::Duplicate)) => {
            // discord's create dialog, filled in from the channel
            if let Some(x) = app.store.channels.get(&c) {
                let mut f = super::create::Form::new(super::create::Kind::Channel, gid, (x.parent != 0).then_some(x.parent));
                f.chan = if x.is_voice() { 2 } else { x.kind };
                f.name = x.name.clone();
                f.private = x.overwrites.iter().any(|o| o.id == gid && o.deny & crate::state::VIEW_CHANNEL != 0);
                app.create = Some(f);
            }
        }
        Some((c, ChanAct::Create)) => {
            let parent = app.store.channels.get(&c).map(|x| if x.kind == 4 { x.id } else { x.parent }).filter(|p| *p != 0);
            app.create = Some(super::create::Form::new(super::create::Kind::Channel, gid, parent));
        }
        Some((c, ChanAct::Collapse)) => toggle = Some(c),
        Some((_, ChanAct::CollapseAll)) => {
            let cats = app.store.channels.values().filter(|x| x.guild == gid && x.kind == 4).map(|x| x.id);
            app.ui.collapsed.extend(cats);
        }
        None => {}
    }
    if let Some(c) = toggle
        && !app.ui.collapsed.remove(&c)
    {
        app.ui.collapsed.insert(c);
    }
    if boost_page {
        app.open_boosts(gid);
    }
    if events_page {
        app.open_events(gid);
    }
    if let Some(p) = home_page {
        app.open_home(gid, p);
    }
    if let Some(c) = set_status {
        app.status_edit = Some((c, String::new()));
    }
    if let Some(c) = make_invite {
        app.invite_people(c);
    }
    if let Some(c) = open {
        if app.store.channels.get(&c).is_some_and(|c| c.is_voice()) {
            app.join_voice(c, true);
        }
        app.open(c);
    }
    // the voice channel without joining it, its chat open beside the call
    if let Some(c) = open_chat {
        app.open(c);
        app.voice_chat = true;
    }
}

enum ChanAct {
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
}

pub fn toggle_hide_muted(app: &mut App, gid: Id) {
    let hide = !app.store.hide_muted.remove(&gid);
    if hide {
        app.store.hide_muted.insert(gid);
    }
    let body = serde_json::json!({ "hide_muted_channels": hide });
    app.rest("PATCH", format!("/users/@me/guilds/{gid}/settings"), Some(body), crate::net::Tag::Quiet);
}

/// Discord's question before a channel or category goes.
pub fn delete_channel(app: &mut App, ch: Id) {
    let Some(c) = app.store.channels.get(&ch) else { return };
    let title = if c.kind == 4 { "Delete Category" } else { "Delete Channel" };
    let name = if c.kind == 4 || c.is_voice() { c.name.clone() } else { format!("#{}", c.name) };
    let body = format!("Are you sure you want to delete **{name}**? This cannot be undone.");
    app.confirm = Some(super::Confirm::new(title, body, title, move |app: &mut App| {
        app.rest("DELETE", format!("/channels/{ch}"), None, crate::net::Tag::Quiet);
        app.chan_settings = None;
    }));
}

/// Asks first, like discord.
pub fn leave(app: &mut App, gid: Id) {
    let name = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    let body = format!("Are you sure you want to leave {name}? You won't be able to rejoin this server unless you are re-invited.");
    app.confirm = Some(super::Confirm::new(format!("Leave '{name}'"), body, "Leave Server", move |app: &mut App| {
        app.rest("DELETE", format!("/users/@me/guilds/{gid}"), Some(serde_json::json!({ "lurking": false })), crate::net::Tag::Quiet);
        app.select_guild(None);
    }));
}

enum GuildAct {
    Web(String),
    Boosts,
    Settings,
    Create(super::create::Kind),
    Threads,
    Directory,
    Invite,
    Notifs,
    HideMuted,
    Leave,
    CopyId,
}

/// The menu under the server name. What we don't do here ourselves opens discord in the browser.
#[allow(clippy::too_many_arguments)]
fn guild_menu(ui: &mut Ui, gid: Id, perms: u64, can_invite: bool, owner: bool, hide: bool, dev: bool, community: bool) -> Option<GuildAct> {
    use crate::state::{MANAGE_CHANNELS, MANAGE_EVENTS, MANAGE_GUILD, VIEW_GUILD_INSIGHTS};
    use super::{Trail, menu_item, menu_row, menu_sep};
    let web = || Some(GuildAct::Web(format!("https://discord.com/channels/{gid}")));
    let mut act = None;
    if menu_item(ui, Some(Icon::Gem), "Server Boost", false) {
        act = Some(GuildAct::Boosts);
    }
    menu_sep(ui);
    if can_invite && menu_item(ui, Some(Icon::PersonAdd), "Invite to Server", false) {
        act = Some(GuildAct::Invite);
    }
    if perms & MANAGE_GUILD != 0 && menu_item(ui, Some(Icon::Gear), "Server Settings", false) {
        act = Some(GuildAct::Settings);
    }
    // discord keeps insights to community servers
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
    if menu_item(ui, Some(Icon::Bubble), "Active Threads", false) {
        act = Some(GuildAct::Threads);
    }
    if perms & MANAGE_GUILD != 0 && menu_item(ui, Some(Icon::Apps), "App Directory", false) {
        act = Some(GuildAct::Directory);
    }
    menu_sep(ui);
    if menu_item(ui, Some(Icon::Bell), "Notification Settings", false) {
        act = Some(GuildAct::Notifs);
    }
    if menu_item(ui, Some(Icon::Shield), "Privacy Settings", false) {
        act = web();
    }
    menu_sep(ui);
    if menu_item(ui, Some(Icon::Pencil), "Edit Per-server Profile", false) {
        act = web();
    }
    if menu_row(ui, Some(Icon::EyeOff), "Hide Muted Channels", None, false, Trail::Check(hide)).clicked() {
        act = Some(GuildAct::HideMuted);
    }
    if perms & MANAGE_GUILD != 0 || !owner {
        menu_sep(ui);
    }
    if perms & MANAGE_GUILD != 0 {
        if menu_item(ui, Some(Icon::Lock), "Security Actions", true) {
            act = web();
        }
        if menu_item(ui, Some(Icon::Shield), "Report Raid", true) {
            act = web();
        }
    }
    if !owner {
        if menu_item(ui, Some(Icon::Leave), "Leave Server", true) {
            act = Some(GuildAct::Leave);
        }
    }
    if dev {
        menu_sep(ui);
        if menu_item(ui, Some(Icon::Id), "Copy Server ID", false) {
            act = Some(GuildAct::CopyId);
        }
    }
    act
}

/// Discord's right-click menu for a channel, thread or category. Editing them is left to discord.
fn channel_menu(r: &egui::Response, store: &Store, c: &Channel, unread: bool, collapsed: bool, dev: bool, out: &mut Option<(Id, ChanAct)>) {
    use super::{LEVELS, Trail, menu_item, menu_row, menu_sep};
    let cat = c.kind == 4;
    let what = if cat { "Category" } else if c.is_thread() { "Thread" } else { "Channel" };
    let perms = store.perms(c);
    egui::Popup::context_menu(r).close_behavior(super::menu_closing(&r.ctx)).show(|ui| super::menu_body(ui, |ui| {
        let mut act = None;
        if ui.add_enabled_ui(unread, |ui| menu_item(ui, None, "Mark As Read", false)).inner {
            act = Some(ChanAct::Read);
        }
        menu_sep(ui);
        if cat {
            if menu_row(ui, None, "Collapse Category", None, false, Trail::Check(collapsed)).clicked() {
                act = Some(ChanAct::Collapse);
            }
            if menu_item(ui, None, "Collapse All Categories", false) {
                act = Some(ChanAct::CollapseAll);
            }
        } else {
            if !c.is_thread() && perms & crate::state::CREATE_INVITE != 0 && menu_item(ui, None, "Invite to Channel", false) {
                act = Some(ChanAct::Invite);
            }
            if menu_item(ui, None, "Copy Link", false) {
                act = Some(ChanAct::Link);
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
        if perms & crate::state::MANAGE_CHANNELS != 0 && !c.is_thread() {
            menu_sep(ui);
            let edits: &[(&str, bool)] = if cat {
                &[("Edit Category", false), ("Delete Category", true)]
            } else {
                &[("Edit Channel", false), ("Duplicate Channel", false), ("Create Text Channel", false), ("Delete Channel", true)]
            };
            for (label, danger) in edits {
                if menu_item(ui, None, label, *danger) {
                    act = Some(match *label {
                        "Create Text Channel" => ChanAct::Create,
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

fn thread_row(ui: &mut Ui, t: &Channel, selected: bool, unread: bool, last: bool) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let spine = egui::Stroke::new(1.5, pal().bg_accent);
    let x = row.left() + 24.0;
    // the little tree branch the official client draws from the parent channel
    ui.painter().line_segment([pos2(x, row.top() - 2.0), pos2(x, if last { row.center().y - 6.0 } else { row.bottom() + 2.0 })], spine);
    ui.painter().add(egui::epaint::CubicBezierShape::from_points_stroke(
        [pos2(x, row.center().y - 6.0), pos2(x, row.center().y), pos2(x, row.center().y), pos2(x + 8.0, row.center().y)],
        false,
        Color32::TRANSPARENT,
        spine,
    ));
    let inner = Rect::from_min_max(pos2(row.left() + 36.0, row.top()), pos2(row.right() - 8.0, row.bottom()));
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let col = if selected || unread {
        pal().header_primary
    } else if resp.hovered() {
        pal().interactive_hover
    } else {
        pal().channel_default
    };
    let font = if selected || unread { semi(15.0) } else { reg(15.0) };
    text::paint_em(ui.painter(), pos2(inner.left() + 8.0, row.center().y), Align2::LEFT_CENTER, &t.name, font, col, inner.width() - 16.0, 1.0);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Discord's boost goal pill: boosts against the next level, then against everything boosts can
/// unlock (levels 1-3 at 2+5+7, the perks at 3+3+3+5+3+2), or once that's covered a full bar
/// and the count. It opens the server's boost page, in the browser here.
fn boost_goal(ui: &mut Ui, boosts: u32, goal: u32, under_banner: bool) -> bool {
    ui.add_space(if under_banner { 19.0 } else { 4.0 });
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let pill = Rect::from_min_max(pos2(row.left() + 12.0, row.top()), pos2(row.right() - 13.0, row.bottom()));
    let p = ui.painter();
    p.rect_filled(pill, 13, pal().bg_tertiary);
    // the goal is every level and perk together, 33 boosts in all
    let next = (boosts < goal).then_some(goal);
    let fill = next.map_or(1.0, |n| boosts as f32 / n as f32);
    let bar = pill.shrink(2.0);
    let done = Rect::from_min_size(bar.min, vec2(bar.width() * fill, bar.height()));
    if done.width() > 0.0 {
        // discord's fill: the boost pink at .1 rising to .4, round ended, with a soft pink glow
        let rad = done.height() / 2.0;
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: 5, spread: 0, color: Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 51) }.as_shape(done, rad));
        let (a, b) = (Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 26), Color32::from_rgba_unmultiplied(0xff, 0x73, 0xfa, 102));
        let rx = rad.min(done.width() / 2.0);
        // a strip along the pill, its ends rounded off, coloured by how far along it is
        let mut edge: Vec<(f32, f32)> = (0..=8).map(|i| (i as f32 / 8.0) * std::f32::consts::FRAC_PI_2).map(|t| (done.left() + rx - rx * t.cos(), rad * t.sin())).collect();
        edge.extend((0..=8).map(|i| (i as f32 / 8.0) * std::f32::consts::FRAC_PI_2).map(|t| (done.right() - rx + rx * t.sin(), rad * t.cos())));
        let mut m = egui::Mesh::default();
        for (k, (x, h)) in edge.iter().enumerate() {
            let c = a.lerp_to_gamma(b, ((x - done.left()) / done.width().max(1.0)).clamp(0.0, 1.0));
            m.colored_vertex(pos2(*x, done.center().y - h), c);
            m.colored_vertex(pos2(*x, done.center().y + h), c);
            if k > 0 {
                let i = (k * 2) as u32;
                m.add_triangle(i - 2, i - 1, i);
                m.add_triangle(i - 1, i, i + 1);
            }
        }
        p.add(m);
    }
    let left = if next.is_none() { "Boost Goal 🎉".to_string() } else { "Boost Goal".to_string() };
    text::paint(p, pos2(bar.left() + 6.0, bar.center().y), Align2::LEFT_CENTER, &left, semi(12.0), pal().text_normal, 160.0);
    let caret = Rect::from_center_size(pos2(bar.right() - 12.0, bar.center().y), vec2(12.0, 12.0));
    // the count and caret are the text colour at .7
    let dim = pal().text_normal.gamma_multiply(0.7);
    icons::draw(p, Icon::ChevronRight, caret, dim);
    let count = match next {
        Some(n) => format!("{boosts}/{n} Boosts"),
        None => format!("{boosts} Boosts"),
    };
    p.text(pos2(caret.left() - 2.0, bar.center().y), Align2::RIGHT_CENTER, count, semi(12.0), dim);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(16.0), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
    ui.add_space(7.0);
    resp.clicked()
}

/// Discord's "Get Started" bar over a server guide: how many of the new member steps are done,
/// with a bar filling green, then a rule. It opens the guide, which lives in the browser here.
fn get_started(ui: &mut Ui, gid: Id, total: usize, done: usize) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let p = ui.painter();
    let x = r.left() + 16.0;
    let right = r.right() - 9.0;
    p.text(pos2(x, r.top() + 5.0), Align2::LEFT_TOP, "Get Started", bold(14.0), pal().header_primary);
    let arrow = Rect::from_min_size(pos2(right - 16.0, r.top() + 6.0), vec2(16.0, 16.0));
    icons::draw(p, Icon::ChevronRight, arrow, pal().text_normal);
    let n = p.text(pos2(arrow.left(), arrow.center().y), Align2::RIGHT_CENTER, total.to_string(), bold(12.0), pal().text_normal);
    let of = p.text(pos2(n.left() - 4.0, arrow.center().y), Align2::RIGHT_CENTER, "of", medium(12.0), pal().text_muted);
    p.text(pos2(of.left() - 4.0, arrow.center().y), Align2::RIGHT_CENTER, done.to_string(), bold(12.0), pal().text_normal);
    let track = Rect::from_min_max(pos2(x, r.top() + 31.0), pos2(right, r.top() + 35.0));
    p.rect_filled(track, 3, pal().border_subtle);
    let filled = Rect::from_min_size(track.min, vec2(track.width() * done as f32 / total.max(1) as f32, track.height()));
    p.rect_filled(filled, 3, GREEN);
    p.hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(1.0, pal().border_subtle));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{gid}/@home")));
    }
    ui.add_space(8.0);
}

/// The rows over the channels, in discord's order and on its conditions. Server Boosts, Members
/// and Channels & Roles are pages in place of the chat, Events is a window, and what's left opens
/// discord's own page in the browser. Hands back the row clicked and which page is open.
fn home_rows(ui: &mut Ui, g: &crate::state::Guild, perms: u64, open: Option<&str>) -> Option<&'static str> {
    use crate::state::{ADMINISTRATOR, BAN_MEMBERS, CREATE_EVENTS, KICK_MEMBERS, MANAGE_EVENTS, MANAGE_GUILD, MODERATE_MEMBERS};
    let has = |f: &str| g.features.iter().any(|x| x == f);
    let id = g.id;
    let community = has("COMMUNITY");
    let perms = if perms & ADMINISTRATOR != 0 { u64::MAX } else { perms };
    let mut rows: Vec<(Icon, &'static str, String)> = Vec::new();
    if has("GUILD_SERVER_GUIDE") {
        rows.push((Icon::ServerGuide, "Server Guide", format!("https://discord.com/channels/{id}/@home")));
    }
    // events only once there are some or you could make one
    if g.events > 0 || perms & (MANAGE_EVENTS | CREATE_EVENTS) != 0 {
        rows.push((Icon::Events, "Events", String::new()));
    }
    if community {
        rows.push((Icon::ChannelsRoles, if has("GUILD_ONBOARDING") { "Channels & Roles" } else { "Browse Channels" }, String::new()));
    }
    // servers selling role subscriptions get discord's shop row
    if has("ROLE_SUBSCRIPTIONS_AVAILABLE_FOR_PURCHASE") {
        rows.push((Icon::Shop, "Server Shop", format!("https://discord.com/channels/{id}/shop")));
    }
    // the member table is for moderators, in community servers or ones with discord's tools on
    let moderator = perms & (BAN_MEMBERS | KICK_MEMBERS | MODERATE_MEMBERS | MANAGE_GUILD) != 0;
    if (moderator && (community || has("ENABLED_MODERATION_EXPERIENCE_FOR_NON_COMMUNITY"))) || (moderator && has("MEMBER_VERIFICATION_MANUAL_APPROVAL")) {
        rows.push((Icon::People, "Members", String::new()));
    }
    if perms & MANAGE_GUILD != 0 {
        rows.push((Icon::Gem, "Server Boosts", String::new()));
    }
    if rows.is_empty() {
        return None;
    }
    let mut clicked = None;
    for (icon, label, url) in rows {
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let inner = row.shrink2(vec2(8.0, 0.0));
        let selected = open == Some(label);
        super::settings::mark(ui, &format!("home_row:{label}"), row);
        if selected {
            ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let col = if selected { pal().header_primary } else if resp.hovered() { pal().interactive_hover } else { pal().channel_default };
        icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(inner.left() + 18.0, row.center().y), vec2(20.0, 20.0)), col);
        text::paint(ui.painter(), pos2(inner.left() + 34.0, row.center().y), Align2::LEFT_CENTER, label, medium(16.0), col, inner.width() - 40.0);
        if resp.clicked() {
            if url.is_empty() {
                clicked = Some(label);
            } else {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
    }
    ui.add_space(9.0);
    let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    // drawn a row under its slot so the first category's 15 lands 17 below the line
    ui.painter().hline(sep.x_range().shrink(16.0), sep.center().y + 1.0, egui::Stroke::new(1.0, pal().border_subtle));
    clicked
}

/// A category header, and whether its create-channel plus was clicked.
fn category(ui: &mut Ui, name: &str, collapsed: bool, can_add: bool) -> (egui::Response, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 23.0), Sense::click());
    let col = if resp.hovered() { pal().interactive_hover } else { pal().channel_default };
    let chev = if collapsed { Icon::ChevronRight } else { Icon::ChevronDown };
    let y = rect.top() + 12.0;
    // the refresh's type consolidation: sentence case at 14 medium, caret after the name
    let t = text::paint_em(ui.painter(), pos2(rect.left() + 8.0, y), Align2::LEFT_CENTER, name, medium(14.0), col, rect.width() - 52.0, 1.0);
    icons::draw(ui.painter(), chev, Rect::from_center_size(pos2(t.right() + 9.0, y), vec2(12.0, 12.0)), col);
    let mut add = false;
    if can_add {
        let pr = Rect::from_center_size(pos2(rect.right() - 24.0, y), vec2(16.0, 16.0));
        let hit = ui.interact(pr.expand(2.0), ui.id().with(("add_channel", name)), Sense::click());
        icons::draw(ui.painter(), Icon::Plus, pr, if hit.hovered() { pal().interactive_hover } else { pal().channel_default });
        add = hit.on_hover_text("Create Channel").clicked();
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, add)
}

/// The channel's icon with the lock or warning discord adds for private and age-gated ones.
pub fn chan_icon(store: &Store, p: &egui::Painter, c: &Channel, rect: Rect, col: Color32) {
    let icon = if store.guilds.get(&c.guild).is_some_and(|g| g.rules == c.id) { Icon::Rules } else { glyph(c) };
    if store.locked(c) {
        icons::draw_marked(p, icon, icons::Mark::Lock, rect, col);
    } else if c.nsfw && !c.is_thread() {
        icons::draw_marked(p, icon, icons::Mark::Warn, rect, col);
    } else {
        icons::draw(p, icon, rect, col);
    }
}

pub fn glyph(c: &Channel) -> Icon {
    match c.kind {
        2 => Icon::Speaker,
        13 => Icon::Stage,
        5 => Icon::Megaphone,
        15 => Icon::Forum,
        16 => Icon::Media,
        10..=12 => Icon::Thread,
        _ => Icon::Hash,
    }
}

/// A channel in the list. Besides the row, hands back where its name's emoji went and whether
/// the invite or edit buttons were clicked.
/// What a voice channel's row adds: green while we're in it, how long its call has gone on, and a
/// second line with its status or, in our own call, discord's prompt to set one.
struct VoiceRow {
    here: bool,
    timer: Option<String>,
    sub: Option<String>,
    prompt: bool,
}

fn elapsed(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// The bools are clicks on invite, edit, "Set a channel status" and a voice channel's Open Chat.
#[allow(clippy::too_many_arguments)]
fn channel_row(ui: &mut Ui, store: &Store, c: &Channel, selected: bool, unread: bool, mentions: u32, muted: bool, new: usize, voice: Option<&VoiceRow>) -> (egui::Response, Option<Rect>, bool, bool, bool, bool) {
    let sub = voice.and_then(|v| v.sub.as_ref());
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), if sub.is_some() { 48.0 } else { 32.0 }), Sense::click());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let y = row.top() + 16.0;
    let here = voice.is_some_and(|v| v.here);
    // not hovered(): egui unhovers the row while its own button is pressed, which would take the
    // button away mid-click and hand the click to the row
    let hovered = resp.contains_pointer();
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    let col = if selected || here || (unread && !muted) {
        pal().header_primary
    } else if hovered {
        pal().interactive_hover
    } else if muted {
        pal().channel_muted
    } else {
        pal().channel_default
    };
    if unread && !selected && !muted {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(row.left(), y - 4.0), vec2(4.0, 8.0)),
            CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 },
            pal().header_primary,
        );
    }
    let icon_col = if here {
        GREEN
    } else if selected {
        pal().header_primary
    } else if muted {
        pal().channel_muted
    } else {
        pal().channel_default
    };
    let ir = Rect::from_center_size(pos2(inner.left() + 18.0, y), vec2(20.0, 20.0));
    chan_icon(store, ui.painter(), c, ir, icon_col);
    // the invite and edit buttons discord shows on the hovered or selected row; a running call
    // keeps its timer there until the row is hovered
    let timer = voice.and_then(|v| v.timer.as_ref());
    let perms = store.perms(c);
    let lit = (hovered || (selected && timer.is_none())) && !c.is_thread() && mentions == 0;
    let (mut invite, mut edit, mut chat) = (false, false, false);
    let mut right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let mut x = inner.right() - 16.0;
    let voice_room = c.is_voice();
    for (icon, tip, shown, out) in [
        (Icon::Gear, "Edit Channel", perms & crate::state::MANAGE_CHANNELS != 0, &mut edit),
        (Icon::PersonAdd, if voice_room { "Invite to Voice" } else { "Invite to Channel" }, perms & crate::state::CREATE_INVITE != 0, &mut invite),
        // a voice channel's own text chat, open beside the call
        (Icon::Bubble, "Open Chat", voice_room, &mut chat),
    ] {
        if !lit || !shown {
            continue;
        }
        let ir = Rect::from_center_size(pos2(x, y), vec2(16.0, 16.0));
        let hit = ui.interact(ir.expand(2.0), ui.id().with((tip, c.id)), Sense::click());
        super::settings::mark(ui, &format!("row_btn:{tip}:{}", c.id), ir);
        icons::draw(ui.painter(), icon, ir, if hit.hovered() { pal().interactive_hover } else { pal().channel_default });
        *out = hit.on_hover_text(tip).clicked();
        right = ir.left() - 6.0;
        x -= 22.0;
    }
    if new > 0 && mentions == 0 && x == inner.right() - 16.0 {
        let n = ui.painter().text(pos2(inner.right() - 8.0, y), Align2::RIGHT_CENTER, format!("{new} New"), semi(12.0), Color32::from_rgb(0x63, 0x74, 0xf4));
        right = n.left() - 6.0;
    }
    if let Some(t) = timer.filter(|_| x == inner.right() - 16.0 && mentions == 0) {
        let r = ui.painter().text(pos2(inner.right() - 8.0, y), Align2::RIGHT_CENTER, t, mono(13.0), GREEN);
        right = r.left() - 6.0;
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    // channel names are medium; unread ones go semibold
    let font = if unread { semi(16.0) } else { medium(16.0) };
    let left = inner.left() + 34.0;
    let name = text::layout_em(ui.ctx(), &c.name, font, col, right - left, 1.0);
    let at = pos2(left, y - name.size().y / 2.0);
    name.paint(ui.painter(), at);
    let emoji = name.emoji_rects(at).next().map(|(_, r)| r);
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, y), mentions, None);
    }
    let mut set = false;
    if let (Some(sub), Some(v)) = (sub, voice) {
        let sy = row.top() + 36.0;
        let r = text::paint_em(ui.painter(), pos2(left, sy), Align2::LEFT_CENTER, sub, reg(14.0), pal().channel_default, inner.right() - 8.0 - left - if v.prompt { 20.0 } else { 0.0 }, 1.0);
        if v.prompt {
            let pen = Rect::from_center_size(pos2(r.right() + 10.0, sy), vec2(14.0, 14.0));
            let hit = ui.interact(r.union(pen), ui.id().with(("set_status", c.id)), Sense::click());
            icons::draw(ui.painter(), Icon::Pencil, pen, if hit.hovered() { pal().interactive_hover } else { pal().channel_default });
            set = hit.on_hover_text("Set a channel status").clicked();
        }
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, emoji, invite, edit, set, chat)
}

fn voice_users(ui: &mut Ui, store: &Store, voice: &crate::voice::VoiceUi, gid: Id, ch: Id) {
    let Some(g) = store.guilds.get(&gid) else { return };
    let mut users: Vec<Id> = g.voice.iter().filter(|(_, c)| **c == ch).map(|(u, _)| *u).collect();
    // discord lists them by name, 32 to a row, in their own display name font and server tag
    users.sort_by_cached_key(|u| store.name_in(gid, *u).to_lowercase());
    for u in users {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        let av = Rect::from_min_size(pos2(row.left() + 44.0, row.center().y - 12.0), vec2(24.0, 24.0));
        if let Some(user) = store.user(u) {
            avatar(ui, av, &user.avatar_url());
        }
        let speaking = voice.is_speaking(u);
        if speaking {
            super::call::ring(ui, av);
        }
        let used = super::call::badges(ui, voice, u, row);
        // discord's usernameSpeaking: the name goes bright while they talk
        let col = if speaking { pal().interactive_hover } else { pal().channel_default };
        let room = row.right() - av.right() - 16.0 - used;
        let face = name_font(store.user(u).map_or(0, |x| x.font), 14.0);
        let r = text::paint_em(ui.painter(), pos2(av.right() + 8.0, row.center().y), Align2::LEFT_CENTER, &store.name_in(gid, u), face, col, room, 1.0);
        if let Some(user) = store.user(u) {
            let clip = Rect::from_min_max(row.min, pos2(av.right() + 8.0 + room, row.bottom()));
            super::tag_chip(&ui.painter().with_clip_rect(clip), user, pos2(r.right() + 4.0, row.center().y), 1.0);
        }
    }
}

fn dms(app: &mut App, ui: &mut Ui) {
    // discord's is a button into the quick switcher, not a filter
    let mut find = false;
    let mut menu = None;
    header(ui, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { pal().control_hover } else { pal().control });
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "Find or start a conversation", medium(14.0), pal().header_primary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        find = resp.clicked();
    });
    if find {
        app.switcher = Some(String::new());
    }
    let cur = app.cur();
    let mut open = None;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(8.0);
            let pending = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            // friends, then the store pages rustcord leaves to the browser
            let rows = [
                (Icon::Friends, "Friends", None),
                (Icon::Nitro, "Nitro Home", Some("https://discord.com/store")),
                (Icon::Shop, "Shop", Some("https://discord.com/shop")),
                (Icon::Quests, "Quests", Some("https://discord.com/quest-home")),
            ];
            for (icon, label, url) in rows {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                let inner = row.shrink2(vec2(8.0, 0.0));
                let on = url.is_none() && app.friends;
                if on {
                    ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
                }
                let col = if on { pal().header_primary } else if resp.hovered() { pal().interactive_hover } else { pal().channel_default };
                icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(inner.left() + 18.0, row.center().y), vec2(20.0, 20.0)), col);
                ui.painter().text(pos2(inner.left() + 40.0, row.center().y), Align2::LEFT_CENTER, label, medium(16.0), col);
                if url.is_none() {
                    if pending > 0 {
                        badge(ui, pos2(inner.right() - 8.0, row.center().y), pending, None);
                    }
                    app.rects.insert("friends".into(), row);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    match url {
                        Some(u) => ui.ctx().open_url(egui::OpenUrl::new_tab(u)),
                        None => app.friends = true,
                    }
                }
            }
            ui.add_space(14.0);
            let (h, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
            ui.painter().text(pos2(h.left() + 16.0, h.center().y), Align2::LEFT_CENTER, "Direct Messages", medium(14.0), pal().channel_default);
            let pr = Rect::from_center_size(pos2(h.right() - 24.0, h.center().y), vec2(16.0, 16.0));
            let plus = ui.interact(pr.expand(4.0), ui.id().with("new_dm"), Sense::click());
            icons::draw(ui.painter(), Icon::Plus, pr, if plus.hovered() { pal().interactive_hover } else { pal().channel_default });
            if plus.on_hover_text("Create DM").clicked() {
                app.new_dm = Some(Default::default());
            }
            ui.add_space(4.0);
            let store = &app.store;
            for &id in &store.dms {
                let Some(c) = store.channels.get(&id) else { continue };
                let title = channel_title(store, id);
                let r = dm_row(ui, store, c, &title, cur == Some(id) && !app.friends);
                app.rects.insert(format!("chan:{id}"), r.rect);
                if r.clicked() {
                    open = Some(id);
                }
                if r.secondary_clicked() {
                    let who = if c.kind == 1 { c.recipients.first().copied().unwrap_or(0) } else { 0 };
                    menu = Some((who, id, r.interact_pointer_pos().unwrap_or(r.rect.center())));
                }
            }
        });
    });
    if let Some((who, c, at)) = menu {
        app.user_menu = Some((who, 0, Some(c), at));
    }
    if let Some(c) = open {
        app.open(c);
    }
}

fn dm_row(ui: &mut Ui, store: &Store, c: &Channel, title: &str, selected: bool) -> egui::Response {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let inner = row.shrink2(vec2(8.0, 0.0));
    let hovered = resp.hovered();
    if selected {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(inner, CornerRadius::same(4), pal().bg_mod_hover);
    }
    // discord greys a muted dm out: muted text, the avatar at .3, no count and no bold
    let muted = store.dm_muted(c.id);
    let unread = store.unread(c) && !muted;
    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
    let ring = if selected { pal().bg_mod_selected } else if hovered { pal().bg_mod_hover } else { pal().bg_tertiary };
    let person = if c.kind == 1 { c.recipients.first().and_then(|u| store.user(*u)) } else { None };
    if let Some(u) = person {
        avatar(ui, av, &u.avatar_url());
        if let Some(d) = u.decoration_url() {
            text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(38.0, 38.0)));
        }
        status_dot(ui, av, store.status(u.id), ring);
    } else {
        match &c.icon {
            Some(icon) => avatar(ui, av, &format!("{}/channel-icons/{}/{}.png?size=64", crate::api::cdn(), c.id, icon)),
            None => {
                ui.painter().circle_filled(av.center(), 16.0, group_color(c.id));
                icons::draw(ui.painter(), Icon::People, Rect::from_center_size(av.center(), vec2(20.0, 20.0)), Color32::WHITE);
            }
        }
    }
    if muted {
        let under = if selected { pal().bg_mod_selected } else if hovered { pal().bg_mod_hover } else { pal().bg_tertiary };
        ui.painter().circle_filled(av.center(), 17.0, under.gamma_multiply(0.7));
    }
    let col = if selected || unread {
        pal().header_primary
    } else if hovered {
        pal().interactive_hover
    } else if muted {
        pal().text_muted
    } else {
        pal().channel_default
    };
    let mentions = if muted { 0 } else { store.mentions(c.id) };
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    let font = if unread { semi(16.0) } else { medium(16.0) };
    let tw = right - av.right() - 12.0;
    let clip = ui.painter().with_clip_rect(Rect::from_x_y_ranges(av.right()..=right, row.y_range()));
    if c.kind == 3 {
        text::paint_em(ui.painter(), pos2(av.right() + 12.0, row.center().y - 8.0), Align2::LEFT_CENTER, title, font, col, tw, 1.0);
        clip.text(
            pos2(av.right() + 12.0, row.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("{} Members", c.recipients.len() + 1),
            reg(12.0),
            pal().text_muted,
        );
    } else {
        let r = text::paint_em(ui.painter(), pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, title, font, col, tw, 1.0);
        if let Some(u) = person {
            super::tag_chip(&clip, u, pos2(r.right() + 4.0, row.center().y), 1.0);
        }
    }
    if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, row.center().y), mentions, None);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn group_color(id: Id) -> Color32 {
    const C: [Color32; 5] = [
        Color32::from_rgb(0x58, 0x65, 0xf2),
        Color32::from_rgb(0x3b, 0xa5, 0x5c),
        Color32::from_rgb(0xfa, 0xa6, 0x1a),
        Color32::from_rgb(0xed, 0x42, 0x45),
        Color32::from_rgb(0xeb, 0x45, 0x9e),
    ];
    C[(id % 5) as usize]
}

/// The game's panel: its icon and name, "Not Sharing" while activity sharing is off (a click
/// there opens the setting), and the button that streams it.
fn game_panel(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(g) = app.game.clone() else { return };
    ui.painter().rect_filled(area, CornerRadius { nw: 7, ne: 7, ..Default::default() }, pal().bg_secondary);
    ui.painter().hline(area.x_range(), area.bottom() + 0.5, egui::Stroke::new(1.0, pal().border_subtle));
    app.rects.insert("game:panel".into(), area);
    let inner = area.shrink(12.0);
    let icon = Rect::from_min_size(inner.min, vec2(32.0, 32.0));
    match &g.icon {
        Some(h) => {
            egui::Image::from_uri(format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), g.id)).corner_radius(8).show_loading_spinner(false).paint_at(ui, icon);
        }
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(8), pal().bg_mod_selected);
            icons::draw(ui.painter(), Icon::Controller, Rect::from_center_size(icon.center(), vec2(20.0, 20.0)), pal().header_secondary);
        }
    }
    app.rects.insert("game:icon".into(), icon);

    let live = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let btn = Rect::from_center_size(pos2(inner.right() - 14.5, inner.center().y), vec2(32.0, 32.0));
    let resp = ui.interact(btn, ui.id().with("game_live"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(btn, CornerRadius::same(8), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let col = if live { GREEN } else if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), Icon::ScreenArrow, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), col);
    app.rects.insert("game:live".into(), btn);
    let tip = match (app.voice.chan.is_some(), live) {
        (_, true) => "Stop Streaming".to_string(),
        (true, false) => format!("Stream {}", g.name),
        // discord offers its channel picker here; rustcord streams from a call
        (false, false) => "Join a voice channel to stream".to_string(),
    };
    if resp.on_hover_text(tip).clicked() && app.voice.chan.is_some() {
        if live {
            app.voice.end_stream(&app.net);
        } else {
            app.share = Some(super::share::Picker::new(ui.ctx()));
        }
    }

    // discord says nothing under the name while it's being shared
    let x = icon.right() + 8.0;
    let room = btn.left() - 8.0 - x;
    let hidden = app.store.hide_game || app.store.presence.get(&app.store.me) == Some(&Status::Offline);
    let y = if hidden { inner.top() + 8.0 } else { inner.center().y };
    text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, &g.name, medium(16.0), pal().text_normal, room);
    if hidden {
        let galley = ui.painter().layout_no_wrap("Not Sharing".into(), medium(12.0), pal().header_secondary);
        let r = Rect::from_min_size(pos2(x, inner.top() + 16.0), vec2(galley.size().x, 16.0));
        let resp = ui.interact(r, ui.id().with("game_sharing"), Sense::click());
        let col = if resp.hovered() { pal().interactive_hover } else { pal().header_secondary };
        ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, "Not Sharing", medium(12.0), col);
        app.rects.insert("game:not_sharing".into(), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.settings_page = Some(super::settings::Page::Activity);
        }
    }
}

fn user_panel(app: &mut App, ui: &mut Ui) {
    let me = app.store.me;
    let user = app.store.user(me).cloned();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let bg = ui.painter().add(egui::Shape::Noop);
        let (av, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        if let Some(u) = &user {
            avatar(ui, av, &u.avatar_url());
        }
        let status = match app.conn {
            crate::net::Conn::Online => app.store.presence.get(&me).copied().unwrap_or(Status::Online),
            _ => Status::Offline,
        };
        status_dot(ui, av, status, pal().bg_secondary_alt);
        // leave room for the three buttons so long names elide instead of running under them
        let col = (ui.available_width() - 146.0).max(40.0);
        let hot = ui.rect_contains_pointer(ui.max_rect());
        let names = ui.allocate_ui_with_layout(vec2(col, 38.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(col);
            ui.set_max_width(col);
            ui.spacing_mut().item_spacing.y = 2.0;
            let name = user.as_ref().map(|u| u.display().to_string()).unwrap_or_default();
            text::label(ui, &name, medium(16.0), pal().header_primary, Sense::hover());
            let sub = match app.conn {
                crate::net::Conn::Online if app.settings.hiding_personal() => "Streamer Mode".into(),
                // discord shows the custom status, or the status itself, and the username on hover
                crate::net::Conn::Online if hot => user.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
                crate::net::Conn::Online => app.store.custom_status.clone().unwrap_or_else(|| {
                    match status {
                        Status::Online => "Online",
                        Status::Idle => "Idle",
                        Status::Dnd => "Do Not Disturb",
                        Status::Offline => "Invisible",
                    }
                    .into()
                }),
                crate::net::Conn::Connecting => "Connecting…".into(),
                crate::net::Conn::Resuming => "Reconnecting…".into(),
                crate::net::Conn::Offline => "Offline".into(),
            };
            // in a call discord swaps the line for a green speaker and "In voice"
            let in_voice = matches!(app.conn, crate::net::Conn::Online) && app.voice.chan.is_some() && !hot && !app.settings.hiding_personal();
            if in_voice {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    icons::draw(ui.painter(), Icon::Speaker, r, GREEN);
                    ui.add(egui::Label::new(egui::RichText::new("In voice").font(medium(12.0)).color(pal().header_secondary)).selectable(false));
                });
            } else {
                ui.add(egui::Label::new(egui::RichText::new(sub).font(medium(12.0)).color(pal().header_secondary)).truncate().selectable(false));
            }
        });
        let hit = Rect::from_x_y_ranges(av.left() - 4.0..=names.response.rect.right() + 4.0, av.center().y - 20.0..=av.center().y + 20.0);
        let resp = ui.interact(hit, ui.id().with("account"), Sense::click());
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
        if open || resp.hovered() {
            ui.painter().set(bg, egui::Shape::rect_filled(hit, CornerRadius::same(8), MENU_HOVER));
        }
        egui::Popup::menu(&resp)
            .align(egui::RectAlign::TOP_START)
            .gap(12.0)
            .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(ui.style().visuals.popup_shadow))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| super::popups::account(app, ui));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let gear = icon_button(ui, Icon::Gear, 32.0, "User Settings");
            app.rects.insert("gear".into(), gear.rect);
            if gear.clicked() {
                app.settings_page = Some(super::settings::Page::Account);
            }
            super::call::user_buttons(app, ui);
        });
    });
}
