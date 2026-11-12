use super::*;

pub(super) fn guild(app: &mut App, ui: &mut Ui, gid: Id) {
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
    let dev = app.settings.dev;
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
        egui::Popup::menu(&resp).anchor(head).align(egui::RectAlign::BOTTOM).gap(5.0).close_behavior(crate::ui::menu_closing(ui.ctx())).show(|ui| {
            crate::ui::menu_body(ui, |ui| act = guild_menu(ui, &app.store, gid, can_invite, dev));
        });
        if can_invite {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                icons::draw(ui.painter(), Icon::PersonAdd, r, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                invite = resp.on_hover_text("Invite to Server").clicked();
            });
        }
    });
    match act {
        Some(GuildAct::Web(url)) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
        Some(GuildAct::Boosts) => app.open_boosts(gid),
        Some(GuildAct::Settings) => app.open_server_settings(gid),
        Some(GuildAct::Create(k)) => app.create = Some(crate::ui::create::Form::new(k, gid, None)),
        Some(GuildAct::Threads) => app.active_threads = Some(gid),
        Some(GuildAct::Directory) => app.open_directory(),
        Some(GuildAct::Invite) => invite = true,
        Some(GuildAct::Notifs) => app.guild_notifs = Some(gid),
        Some(GuildAct::HideMuted) => toggle_hide_muted(app, gid),
        Some(GuildAct::ShowAll) => show_all_channels(app, gid),
        Some(GuildAct::Tag) => app.guild_modal = Some(crate::ui::guildmodal::Modal::Tag(gid)),
        Some(GuildAct::Privacy) => app.guild_modal = Some(crate::ui::guildmodal::Modal::Privacy(gid)),
        Some(GuildAct::Profile) => app.edit_server_profile(gid),
        Some(GuildAct::Security) => app.guild_modal = Some(crate::ui::guildmodal::Modal::Security { gid, invites: false, dms: false }),
        Some(GuildAct::Raid) => app.guild_modal = Some(crate::ui::guildmodal::Modal::Raid(gid)),
        Some(GuildAct::Leave) => leave(app, gid),
        Some(GuildAct::CopyId) => ui.ctx().copy_text(gid.to_string()),
        None => {}
    }
    if invite && let Some(c) = app.cur() {
        app.invite_people(c, "Guild Header");
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
    let mut person: Option<(Id, Id, bool, egui::Pos2)> = None;
    let mut make_invite: Option<(Id, &str)> = None;
    let mut set_status: Option<Id> = None;
    let mut boost_page = false;
    let mut shop_page = false;
    let mut events_page = false;
    let mut events_act: Option<EventsAct> = None;
    let mut open_chat: Option<Id> = None;
    let mut join_act: Option<(Id, Id)> = None;
    let goal = app.boost_info.get(&gid).map(|i| i.goal()).filter(|g| *g > 0).unwrap_or(33);
    let mut home_page = None;
    let open_page = if app.boosts == Some(gid) {
        Some("Server Boosts")
    } else if app.server_shop.as_ref().is_some_and(|s| s.guild == gid) {
        Some("Server Shop")
    } else {
        match app.home.as_ref().filter(|h| h.guild == gid).map(|h| h.page) {
            Some(crate::ui::community::Page::Members) => Some("Members"),
            Some(crate::ui::community::Page::Guide) => Some("Server Guide"),
            Some(crate::ui::community::Page::Channels(_)) => Some(if app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_ONBOARDING")) { "Channels & Roles" } else { "Browse Channels" }),
            None => None,
        }
    };
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        let out = egui::ScrollArea::vertical().id_salt(("side_list", gid)).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // under a banner the list starts where the banner ends
            ui.add_space(if banner.is_some() { 81.0 } else { 13.0 });
            let store = &app.store;
            if let Some(g) = store.guilds.get(&gid).filter(|g| g.boost_bar) {
                boost_page |= boost_goal(ui, g.boosts, goal, banner.is_some());
            }
            let threads: Vec<&Channel> = store.threads_in(gid).filter(|c| c.joined && !c.archived).collect();
            // discord's "N New" on forums: open posts started since you last looked
            let fresh = fresh_posts(store, gid);
            let revealed = egui::Id::new("side_revealed");
            if let Some(g) = store.guilds.get(&gid) {
                // discord counts only the steps in channels you can see
                let acts: Vec<Id> = app.guides.get(&gid).map(|(a, _)| a.iter().copied().filter(|c| store.channels.get(c).is_some_and(|c| store.can_view(c))).collect()).unwrap_or_default();
                if g.features.iter().any(|f| f == "COMMUNITY") && !acts.is_empty() && g.new_member() {
                    let done = app.guides.get(&gid).map_or(0, |(_, d)| acts.iter().filter(|c| d.contains(c)).count());
                    if get_started(ui, acts.len(), done) {
                        home_page = Some(crate::ui::community::Page::Guide);
                    }
                }
                let muted = store.events_muted.contains(&gid);
                let read = store.event_reads.get(&gid);
                let mentions = if muted { 0 } else { read.map_or(0, |r| r.mentions) };
                let unread = !muted && g.newest_event > read.map_or(0, |r| r.last_acked);
                match home_rows(ui, g, store.my_perms(gid), open_page, EventsRow { muted, unread, mentions }, &mut events_act) {
                    Some("Server Boosts") => boost_page = true,
                    Some("Server Shop") => shop_page = true,
                    Some("Events") => events_page = true,
                    Some("Members") => home_page = Some(crate::ui::community::Page::Members),
                    Some("Server Guide") => home_page = Some(crate::ui::community::Page::Guide),
                    Some(_) => home_page = Some(crate::ui::community::Page::Channels(if g.features.iter().any(|f| f == "GUILD_ONBOARDING") { 0 } else { 1 })),
                    None => {}
                }
            }
            let hide = store.hide_muted.contains(&gid);
            let off_id = egui::Id::new(("suggested_off", gid));
            let off: Vec<Id> = ui.ctx().data(|d| d.get_temp(off_id)).unwrap_or_default();
            let mut suggested = suggested(store, gid, cur);
            suggested.retain(|c| !off.contains(&c.id));
            let mut groups = tree(store, gid);
            let mut pinned: Vec<&Channel> = Vec::new();
            for (_, kids) in &mut groups {
                kids.retain(|c| {
                    let on = store.chan_flags.get(&c.id).is_some_and(|f| f & crate::ui::PINNED != 0);
                    if on {
                        pinned.push(c);
                    }
                    !on
                });
            }
            let sections = Some((Some("Suggested"), None, suggested)).filter(|s| !s.2.is_empty()).into_iter().chain(Some((Some("Pinned Channels"), None, pinned)).filter(|s| !s.2.is_empty())).chain(groups.into_iter().map(|(c, k)| (None, c, k)));
            for (virt, cat, kids) in sections {
                let (suggest_sec, pin_sec) = (virt == Some("Suggested"), virt == Some("Pinned Channels"));
                // hiding muted channels takes empty categories with them, but never the open channel
                let kids: Vec<&Channel> = kids.into_iter().filter(|c| !hide || cur == Some(c.id) || !store.is_muted(c)).collect();
                if hide && kids.is_empty() && (cat.is_some() || pin_sec) {
                    continue;
                }
                let collapsed = if pin_sec { app.ui.collapsed.contains(&gid) } else { cat.is_some_and(|c| app.store.collapsed.contains(&c.id)) };
                if suggest_sec {
                    ui.add_space(15.0);
                    if suggested_head(ui) {
                        let ids: Vec<Id> = kids.iter().map(|c| c.id).collect();
                        ui.ctx().data_mut(|d| d.insert_temp(off_id, ids));
                    }
                }
                if pin_sec {
                    ui.add_space(15.0);
                    let (head, _) = category(ui, "Pinned Channels", collapsed, false);
                    app.rects.insert("side:pinned".into(), head.rect);
                    if head.clicked() {
                        toggle = Some(gid);
                    }
                }
                if let Some(cat) = cat {
                    // discord's category is 16 of padding over a 24 row, the first channel 1 under it
                    ui.add_space(15.0);
                    let (r, add) = category(ui, &cat.name, collapsed, store.perms(cat) & crate::state::MANAGE_CHANNELS != 0);
                    if r.clicked() {
                        toggle = Some(cat.id);
                    }
                    app.rects.insert(format!("cat:{}", cat.id), r.rect);
                    if store.perms(cat) & crate::state::MANAGE_CHANNELS != 0 {
                        crate::ui::reorder::grab(ui.ctx(), &r, cat.id);
                    }
                    if add {
                        menu = Some((cat.id, ChanAct::Create));
                    }
                    let unread = kids.iter().any(|k| k.is_text() && store.unread(k));
                    channel_menu(&r, store, cat, unread, collapsed, dev, false, &mut menu);
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
                        let status = store.voice_status.get(&c.id).cloned().or_else(|| app.voice.stage.live.get(&c.id).map(|l| l.topic.clone()).filter(|t| !t.is_empty()));
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
                        make_invite = Some((c.id, "Guild Channels"));
                    }
                    if edit {
                        menu = Some((c.id, ChanAct::Edit));
                    }
                    app.rects.insert(format!("chan:{}", c.id), r.rect);
                    if store.perms(c) & crate::state::MANAGE_CHANNELS != 0 {
                        crate::ui::reorder::grab(ui.ctx(), &r, c.id);
                    }
                    if let Some(e) = emoji {
                        app.rects.insert(format!("chanemoji:{}", c.id), e);
                    }
                    app.rects.insert(format!("chanicon:{}", c.id), Rect::from_center_size(pos2(r.rect.left() + 26.0, r.rect.center().y), vec2(20.0, 20.0)));
                    if r.clicked() {
                        open = Some(c.id);
                    }
                    channel_menu(&r, store, c, unread, false, dev, app.settings.hide_names.contains(&c.id), &mut menu);
                    let kids: Vec<&&Channel> = threads.iter().filter(|t| t.parent == c.id).collect();
                    let n = kids.len();
                    for (i, t) in kids.into_iter().enumerate() {
                        let r = thread_row(ui, t, cur == Some(t.id), store.unread(t), i + 1 == n);
                        app.rects.insert(format!("chan:{}", t.id), r.rect);
                        if r.clicked() {
                            open = Some(t.id);
                        }
                        channel_menu(&r, store, t, store.unread(t), false, dev, false, &mut menu);
                    }
                    if c.is_voice() && !collapsed {
                        if let Some((u, secondary, at)) = voice_users(ui, store, &app.voice, gid, c.id, app.settings.hide_names.contains(&c.id)) {
                            person = Some((u, c.id, secondary, at));
                        }
                        if let Some(a) = crate::ui::activity::view::list_rows(ui, store, &app.activity, c.id) {
                            join_act = Some((c.id, a));
                        }
                    }
                }
                if suggest_sec {
                    ui.add_space(9.0);
                    let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(sep.x_range().shrink(16.0), sep.center().y + 1.0, egui::Stroke::new(1.0, pal().border_subtle));
                }
            }
            ui.add_space(12.0);
        });
        ui.ctx().data_mut(|d| d.insert_temp(scroll_id, out.state.offset.y));
    });
    crate::ui::reorder::drag(app, ui.ctx());
    if let Some((u, c, secondary, at)) = person {
        if secondary {
            app.user_menu = Some((u, gid, None, at));
            app.menu_from = crate::ui::usermenu::Origin::Voice(c);
        } else {
            app.open_profile(u, gid, at);
        }
    }
    match menu {
        Some((c, ChanAct::Read)) => {
            let mut chans: Vec<Id> = app.store.channels.values().filter(|x| x.parent == c).map(|x| x.id).collect();
            chans.push(c);
            app.mark_read(&chans);
        }
        Some((c, ChanAct::Mute(m, secs))) => app.mute_for(c, false, m, secs),
        Some((c, ChanAct::Level(l))) => app.set_level(c, l),
        Some((c, ChanAct::Invite)) => make_invite = Some((c, "Context Menu")),
        Some((c, ChanAct::Link)) => ui.ctx().copy_text(format!("https://discord.com/channels/{gid}/{c}")),
        Some((c, ChanAct::CopyId)) => ui.ctx().copy_text(c.to_string()),
        Some((c, ChanAct::Edit)) => app.open_chan_settings(c),
        Some((c, ChanAct::Delete)) => delete_channel(app, c),
        Some((c, ChanAct::Duplicate)) => {
            // discord's create dialog, filled in from the channel
            if let Some(x) = app.store.channels.get(&c) {
                let mut f = crate::ui::create::Form::new(crate::ui::create::Kind::Channel, gid, (x.parent != 0).then_some(x.parent));
                f.chan = if x.is_voice() { 2 } else { x.kind };
                f.name = x.name.clone();
                f.private = x.overwrites.iter().any(|o| o.id == gid && o.deny & crate::state::VIEW_CHANNEL != 0);
                app.create = Some(f);
            }
        }
        Some((c, ChanAct::Create)) => {
            let x = app.store.channels.get(&c);
            let parent = x.map(|x| if x.kind == 4 { x.id } else { x.parent }).filter(|p| *p != 0);
            let voice = x.is_some_and(|x| x.is_voice());
            let mut f = crate::ui::create::Form::new(crate::ui::create::Kind::Channel, gid, parent);
            if voice {
                f.chan = 2;
            }
            app.create = Some(f);
        }
        Some((c, ChanAct::List(on))) => app.chan_flag(c, crate::ui::OPTED, on),
        Some((c, ChanAct::Pin)) => {
            let on = app.store.chan_flags.get(&c).is_none_or(|f| f & crate::ui::PINNED == 0);
            app.chan_flag(c, crate::ui::PINNED, on);
        }
        Some((c, ChanAct::HideNames)) => {
            if let Some(i) = app.settings.hide_names.iter().position(|x| *x == c) {
                app.settings.hide_names.remove(i);
            } else {
                app.settings.hide_names.push(c);
            }
            app.save_settings(ui.ctx());
        }
        Some((c, ChanAct::Chat)) => open_chat = Some(c),
        Some((c, ChanAct::Move(m))) => {
            let at = app.store.channels.get(&c).map(|x| x.parent).unwrap_or(0);
            match m {
                Move::Category(p) => crate::ui::reorder::move_to(app, c, p, None),
                Move::First => crate::ui::reorder::move_to(app, c, at, Some(0)),
                Move::Last => crate::ui::reorder::move_to(app, c, at, None),
            }
        }
        Some((c, ChanAct::Collapse)) => toggle = Some(c),
        Some((_, ChanAct::CollapseAll)) => {
            let cats: Vec<Id> = app.store.channels.values().filter(|x| x.guild == gid && x.kind == 4).map(|x| x.id).collect();
            app.set_collapsed(&cats, true);
        }
        None => {}
    }
    // the pinned section's fold is the server's own id, and only this machine's
    if toggle == Some(gid) {
        if !app.ui.collapsed.remove(&gid) {
            app.ui.collapsed.insert(gid);
        }
    } else if let Some(c) = toggle {
        app.set_collapsed(&[c], !app.store.collapsed.contains(&c));
    }
    if boost_page {
        app.open_boosts(gid);
    }
    if shop_page {
        app.open_server_shop(gid);
    }
    if events_page {
        app.open_events(gid);
    }
    match events_act {
        Some(EventsAct::Mute(on)) => {
            if on {
                app.store.events_muted.insert(gid);
            } else {
                app.store.events_muted.remove(&gid);
            }
            app.guild_settings(gid, serde_json::json!({ "mute_scheduled_events": on }), crate::net::Tag::undo("Failed to update your settings. Please try again.", move |app: &mut App| {
                if on {
                    app.store.events_muted.remove(&gid);
                } else {
                    app.store.events_muted.insert(gid);
                }
            }));
        }
        Some(EventsAct::Read) => app.read_events(gid),
        None => {}
    }
    if let Some(p) = home_page {
        app.open_home(gid, p);
    }
    if let Some(c) = set_status {
        app.status_edit = Some((c, String::new()));
    }
    if let Some((c, source)) = make_invite {
        app.invite_people(c, source);
    }
    if let Some((c, a)) = join_act {
        crate::ui::activity::join_from_list(app, c, a);
    }
    if let Some(c) = open {
        if app.store.channels.get(&c).is_some_and(|c| c.is_voice()) {
            app.join_voice(c, true);
        }
        let shift = ui.input(|i| i.modifiers.shift);
        app.open_thread(c, true, shift);
    }
    // the voice channel without joining it, its chat open beside the call
    if let Some(c) = open_chat {
        app.open(c);
        app.voice_chat = true;
    }
}
