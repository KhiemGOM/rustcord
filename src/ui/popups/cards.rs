use super::*;

pub(super) fn profile(app: &mut App, ctx: &egui::Context) {
    let key = egui::Id::new("popout_fx_uid");
    let Some((uid, gid, at)) = app.profile else {
        if let Some(old) = ctx.data_mut(|d| d.remove_temp::<Id>(key)) {
            crate::ui::cosmo::forget(ctx, ("popout", old));
        }
        return;
    };
    ctx.data_mut(|d| d.insert_temp(key, uid));
    let worn = app.profile_of(uid, gid).cloned();
    let layers = crate::ui::cosmo::layers_of(app, worn.as_ref());
    let since = crate::ui::cosmo::since(ctx, ("popout", uid));
    let widgets = worn.as_ref().map(crate::ui::profed::widgets_of).unwrap_or_default();
    crate::ui::profed::board::want_game_icons(app, &widgets);
    let Some(user) = app.store.user(uid).cloned() else {
        app.profile = None;
        return;
    };
    if !app.notes.contains_key(&uid) {
        app.notes.insert(uid, String::new());
        app.rest("GET", format!("/users/@me/notes/{uid}"), None, Tag::Note(uid));
    }
    let p = app.member_profiles.get(&(uid, gid)).or(app.profiles.get(&uid)).filter(|p| !p.is_null());
    let bio = p.and_then(|p| shown(p, "bio").as_str().or(p["user"]["bio"].as_str())).unwrap_or("").to_string();
    let roles = if gid != 0 { profile::member_roles(&app.store, p, gid, uid) } else { Vec::new() };
    let name = app.store.name_in(gid, uid);
    let status = app.store.status(uid);
    let me = app.store.me;
    let rel = app.store.relationships.get(&uid).copied();
    let stranger = !user.bot && uid != me && rel.is_none();
    let live: Vec<crate::state::Activity> = app.store.activities.get(&uid).map(|l| presence::shown(l).cloned().collect()).unwrap_or_default();
    let spotify = app.profiles.get(&me).is_some_and(|m| m["connected_accounts"].as_array().is_some_and(|a| a.iter().any(|c| c["type"] == "spotify")));
    let app_icons = app.store.app_icons.clone();
    let saved = app.notes.get(&uid).cloned().unwrap_or_default();
    let note_open_id = egui::Id::new(("note_open", uid));
    let react_id = egui::Id::new(("react_open", uid));
    let note_open: bool = !saved.is_empty() || ctx.data(|d| d.get_temp(note_open_id)).unwrap_or(false);
    let mod_view = uid != me && gid != 0 && app.store.members.get(&gid).is_some_and(|m| m.contains_key(&uid)) && crate::ui::modview::allowed(&app.store, gid);
    let (mut message, mut banner, mut main, mut full, mut tab) = (None, BannerAct::default(), false, false, None);
    let mut view_avatar = false;
    let (mut role_act, mut note_save, mut open_url): (Option<profile::RoleAct>, Option<String>, Option<String>) = (None, None, None);
    let mut head = Rect::NOTHING;
    let store = &app.store;
    let notes = &mut app.rects;

    // discord's 2025 popout: a 105 banner, round buttons on it, then everything at 16 in
    let area = crate::ui::motion::popout(ctx, egui::Area::new(egui::Id::new("profile"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true), vec2(0.0, 1.0))
        .show(ctx, |ui| {
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(300.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let wash = ui.painter().add(egui::Shape::Noop);
                let top = card_head(ui, &user, p, &name, status);
                head = top;
                full = clicked_flag(ui.ctx(), "card_name_clicked") == Some(uid);
                // from a server profile the avatar goes to the main one
                if gid != 0 {
                    let hit = ui.interact(card_avatar(top), ui.id().with("profile_avatar"), Sense::click());
                    if hit.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    main = hit.on_hover_text("View Main Profile").clicked();
                } else if user.avatar.is_some() {
                    let hit = ui.interact(card_avatar(top), ui.id().with("profile_avatar"), Sense::click());
                    if hit.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    view_avatar = hit.on_hover_text("View Profile Picture").clicked();
                }
                if uid != me {
                    banner = banner_buttons(ui, top, stranger, rel == Some(crate::state::FRIEND), mod_view);
                    let nb = Rect::from_min_size(pos2(top.right() - 40.0, top.bottom() + 44.0), vec2(24.0, 24.0));
                    let resp = ui.interact(nb, ui.id().with("add_note"), Sense::click());
                    crate::ui::icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(nb.center(), vec2(16.0, 16.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
                    if resp.on_hover_text("Add Note (only visible to you)").clicked() {
                        ui.data_mut(|d| d.insert_temp(note_open_id, true));
                    }
                }
                let st = profile::status_bubble(ui, top, store, uid, uid != me);
                if matches!(st, Some(profile::StatusAct::React)) {
                    let open: bool = ui.data(|d| d.get_temp(react_id)).unwrap_or(false);
                    ui.data_mut(|d| d.insert_temp(react_id, !open));
                }
                if matches!(st, Some(profile::StatusAct::Reply)) {
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(("reply_status", uid)), true));
                    ui.memory_mut(|m| m.request_focus(egui::Id::new("profile_msg_box")));
                }
                if ui.data(|d| d.get_temp(react_id)).unwrap_or(false) {
                    let bar = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() + 36.0), vec2(6.0 * 32.0 + 8.0, 40.0));
                    ui.painter().rect(bar, 12, pal().bg_floating, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    for (i, e) in ["\u{1f44d}", "\u{2764}\u{fe0f}", "\u{1f602}", "\u{1f62e}", "\u{1f622}", "\u{1f525}"].into_iter().enumerate() {
                        let c = Rect::from_center_size(pos2(bar.left() + 20.0 + i as f32 * 32.0, bar.center().y), vec2(28.0, 28.0));
                        let resp = ui.interact(c, ui.id().with(("react", i)), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(c, 6, pal().bg_mod_hover);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        text::image(ui.painter(), &crate::markdown::twemoji(e), Rect::from_center_size(c.center(), vec2(20.0, 20.0)));
                        if resp.clicked() {
                            message = Some(e.to_string());
                        }
                    }
                }
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                    ui.set_width(268.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tab = mutuals(ui, store, p, pal().bg_secondary, 19.0);
                    if !bio.is_empty() {
                        ui.add_space(16.0);
                        let mut job = egui::text::LayoutJob::single_section(bio.clone(), egui::TextFormat::simple(reg(14.0), pal().header_primary));
                        job.wrap.max_width = 268.0;
                        job.wrap.max_rows = 3;
                        job.wrap.break_anywhere = false;
                        job.wrap.overflow_character = Some('…');
                        let g = ui.painter().layout_job(job);
                        let cut = g.elided;
                        ui.label(g);
                        if cut {
                            ui.add_space(4.0);
                            let (r, resp) = ui.allocate_exact_size(vec2(268.0, 16.0), Sense::click());
                            let t = ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "View Full Bio", medium(12.0), pal().header_primary);
                            if resp.hovered() {
                                ui.painter().hline(t.x_range(), t.bottom(), egui::Stroke::new(1.0, pal().header_primary));
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            notes.insert("view_full_bio".into(), r);
                            if resp.clicked() {
                                full = true;
                            }
                        }
                    }
                    if !widgets.is_empty() {
                        ui.add_space(12.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(268.0, 40.0), Sense::click());
                        ui.painter().rect_filled(r, 8, egui::Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 }));
                        ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "Game Collection", medium(12.0), pal().header_primary);
                        let ids = crate::ui::profed::board::game_ids(&widgets);
                        let n = ids.len().min(4);
                        for (i, id) in ids.iter().take(4).enumerate() {
                            let ir = Rect::from_min_size(pos2(r.right() - 12.0 - (n - i) as f32 * 28.0 + 4.0, r.center().y - 12.0), vec2(24.0, 24.0));
                            ui.painter().rect_filled(ir, 4, egui::Color32::from_white_alpha(20));
                            if let Some((a, h)) = id.parse::<u64>().ok().and_then(|a| app_icons.get(&a).cloned().flatten().map(|h| (a, h))) {
                                text::image(ui.painter(), &format!("{}/app-icons/{a}/{h}.png?size=48", crate::api::cdn()), ir);
                            }
                        }
                        notes.insert("game_collection".into(), r);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            tab = Some(profile::Tab::Board);
                        }
                    }
                    for a in &live {
                        ui.add_space(12.0);
                        let icon = app_icons.get(&a.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=96", crate::api::cdn(), a.app));
                        match presence::card(ui, a, 268.0, &icon, spotify, &name, false) {
                            Some(presence::Act::Play(u)) => open_url = Some(u),
                            Some(presence::Act::Connect) => open_url = Some("https://discord.com/settings/connections".into()),
                            None => {}
                        }
                    }
                    if !live.is_empty() {
                        ui.add_space(8.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(268.0, 16.0), Sense::click());
                        let t = ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "View All Activity", medium(12.0), pal().header_secondary);
                        if resp.hovered() {
                            ui.painter().hline(t.x_range(), t.bottom(), egui::Stroke::new(1.0, pal().header_secondary));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        notes.insert("view_all_activity".into(), r);
                        if resp.clicked() {
                            tab = Some(profile::Tab::Activity);
                        }
                    }
                    if !roles.is_empty() || (gid != 0 && profile::can_manage_any(store, gid)) {
                        ui.add_space(14.0);
                        if !bio.is_empty() {
                            ui.label(RichText::new("Roles").font(semi(12.0)).color(pal().header_primary));
                            ui.add_space(10.0);
                        }
                        role_act = profile::roles_row(ui, store, gid, uid, &roles, 268.0, 2);
                    }
                    if uid != me && note_open && !crate::theme::view().hide_personal {
                        ui.add_space(16.0);
                        ui.label(RichText::new("Note").font(semi(12.0)).color(pal().header_primary));
                        ui.add_space(4.0);
                        note_save = profile::note_box(ui, notes, "popout_note", uid, &saved, 268.0, 12.0, true);
                    }
                    if uid != me {
                        // discord's box: typed into in place, Enter sends it and opens the dm.
                        // It has the focus as the card opens.
                        ui.add_space(16.0);
                        let draft = egui::Id::new(("profile_msg", uid));
                        let replying = ui.data(|d| d.get_temp::<bool>(egui::Id::new(("reply_status", uid)))).unwrap_or(false);
                        let mut text: String = ui.data(|d| d.get_temp(draft)).unwrap_or_default();
                        let (r, _) = ui.allocate_exact_size(vec2(268.0, 46.0), Sense::hover());
                        ui.painter().rect(r, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                        crate::ui::icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(pos2(r.right() - 29.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary);
                        let field = Rect::from_min_max(pos2(r.left() + 16.0, r.top() + 1.0), pos2(r.right() - 44.0, r.bottom() - 1.0));
                        let hint = if replying { format!("Reply to {name}'s status") } else { format!("Message @{name}") };
                        let edit = ui.put(
                            field,
                            egui::TextEdit::singleline(&mut text)
                                .id(egui::Id::new("profile_msg_box"))
                                .frame(Frame::NONE)
                                .font(reg(14.0))
                                .text_color(pal().text_normal)
                                .vertical_align(egui::Align::Center)
                                .hint_text(RichText::new(hint).font(reg(14.0)).color(pal().text_muted))
                                .desired_width(field.width()),
                        );
                        if !ui.ctx().memory(|m| m.areas().visible_last_frame(&ui.layer_id())) {
                            edit.request_focus();
                        }
                        notes.insert("profile_msg".into(), r);
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !text.trim().is_empty() {
                            message = Some(std::mem::take(&mut text));
                        }
                        ui.data_mut(|d| d.insert_temp(draft, text));
                    }
                });
                theme_wash(ui, wash, p, ui.min_rect());
                crate::ui::cosmo::paint_layers(ui, ui.min_rect(), &layers, since);
            });
        });
    app.rects.insert("profile_head".into(), head);
    let tag = clicked_flag(ctx, "card_tag_clicked");
    if let Some(r) = role_act {
        profile::do_role(app, gid, uid, r);
    }
    if let Some(n) = note_save {
        app.save_note(uid, n);
    }
    if let Some(u) = open_url {
        ctx.open_url(egui::OpenUrl::new_tab(u));
    }
    if view_avatar {
        let big = user.avatar_url().replace("size=80", "size=1024");
        app.viewer = Some((big.clone(), big, 1024, 1024));
    }
    if let Some(text) = message {
        app.profile = None;
        app.dm_text(uid, text);
    } else if let Some(g) = tag {
        app.tag_pop = Some((g, at + vec2(0.0, 60.0)));
    } else if let Some(t) = tab {
        app.open_full(uid, gid, t);
    } else if full {
        app.open_full(uid, gid, profile::Tab::Activity);
    } else if main {
        app.open_profile(uid, 0, at);
    } else if let Some(at) = banner.more {
        app.user_menu = Some((uid, gid, None, at));
    } else if let Some(at) = banner.friend {
        app.user_menu = Some((uid, gid, None, at));
    } else if banner.add {
        app.rest_in("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), "User Profile", Tag::Friend);
    } else if banner.mod_view {
        app.open_mod_view(gid, uid);
    } else if dismissed(ctx, &area.response) && app.user_menu.is_none() && app.role_add.is_none() && app.tag_pop.is_none() {
        app.profile = None;
    }
}

/// Something on a card was clicked in this pass: `card_head` leaves the click here for whichever
/// card called it to pick up.
pub(super) fn click_flag(ctx: &egui::Context, key: &str, v: Id) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(key), (v, pass)));
}

pub(super) fn clicked_flag(ctx: &egui::Context, key: &str) -> Option<Id> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(Id, u64)>(egui::Id::new(key))).filter(|(_, p)| *p == pass).map(|(v, _)| v)
}

#[derive(Default)]
pub(super) struct BannerAct {
    pub(super) more: Option<Pos2>,
    pub(super) friend: Option<Pos2>,
    pub(super) add: bool,
    pub(super) mod_view: bool,
}

/// The round buttons on a profile's banner, right to left: "..." for the person's menu, add
/// friend for a stranger or the friend button for a friend, and Mod View for moderators.
pub(super) fn banner_buttons(ui: &mut Ui, top: Rect, stranger: bool, friend: bool, mod_view: bool) -> BannerAct {
    let mut x = top.right() - 12.0;
    let mut round = |ui: &mut Ui, icon: Icon, tip: &str| {
        let r = Rect::from_min_size(pos2(x - 32.0, top.top() + 8.0), vec2(32.0, 32.0));
        x -= 40.0;
        let resp = ui.interact(r, ui.id().with(("banner_btn", tip)), Sense::click());
        crate::ui::settings::mark(ui, &format!("banner:{tip}"), r);
        let fill = egui::Color32::from_black_alpha(if resp.hovered() { 184 } else { 143 });
        ui.painter().circle(r.center(), 16.0, fill, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)));
        crate::ui::icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), egui::Color32::WHITE);
        let at = r.left_bottom() + vec2(0.0, 4.0);
        (resp.on_hover_text(tip), at)
    };
    let (r, at) = round(ui, Icon::More, "More");
    let mut out = BannerAct { more: r.clicked().then_some(at), ..Default::default() };
    if stranger {
        out.add = round(ui, Icon::PersonAdd, "Add Friend").0.clicked();
    } else if friend {
        let (r, at) = round(ui, Icon::Friends, "Friend");
        out.friend = r.clicked().then_some(at);
    }
    if mod_view {
        out.mod_view = round(ui, Icon::Shield, "Open in Mod View").0.clicked();
    }
    out
}

/// "6 Mutual Friends • 7 Mutual Servers" with the first three friends' faces; each count opens
/// the full profile on its tab. `card` is what the faces' rings are cut from.
pub(super) fn mutuals(ui: &mut Ui, store: &crate::state::Store, p: Option<&serde_json::Value>, card: egui::Color32, gap: f32) -> Option<profile::Tab> {
    let friends: Vec<Id> = p.and_then(|p| p["mutual_friends"].as_array()).into_iter().flatten().map(|f| crate::state::sid(&f["id"])).collect();
    let n_friends = p.and_then(|p| p["mutual_friends_count"].as_u64()).unwrap_or(friends.len() as u64);
    let n_guilds = p.and_then(|p| p["mutual_guilds"].as_array()).map_or(0, |g| g.len() as u64);
    if n_friends == 0 && n_guilds == 0 {
        return None;
    }
    ui.add_space(gap);
    let (r, _) = ui.allocate_exact_size(vec2(268.0, 16.0), Sense::hover());
    let mut x = r.left();
    for f in friends.iter().take(3) {
        let a = Rect::from_min_size(pos2(x, r.top()), vec2(16.0, 16.0));
        // each face sits on a ring of the card over the one before
        ui.painter().circle_filled(a.center(), 9.5, card);
        if let Some(u) = store.user(*f) {
            avatar(ui, a, &u.avatar_url());
        }
        x += 13.0;
    }
    if !friends.is_empty() {
        x += 7.0;
    }
    let plural = |n: u64, what: &str| format!("{n} Mutual {what}{}", if n == 1 { "" } else { "s" });
    let parts: Vec<(String, profile::Tab)> = [(n_friends, "Friend", profile::Tab::Friends), (n_guilds, "Server", profile::Tab::Servers)].into_iter().filter(|(n, ..)| *n > 0).map(|(n, w, t)| (plural(n, w), t)).collect();
    let mut hit = None;
    for (i, (t, tab)) in parts.iter().enumerate() {
        if i > 0 {
            ui.painter().circle_filled(pos2(x + 8.0, r.center().y), 2.0, pal().header_secondary);
            x += 16.0;
        }
        let g = ui.painter().layout_no_wrap(t.clone(), reg(12.0), pal().header_secondary);
        let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x, 16.0));
        let resp = ui.interact(tr, ui.id().with(("mutual", t)), Sense::click());
        let ink = crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            ui.painter().hline(tr.x_range(), tr.bottom(), egui::Stroke::new(1.0, ink));
        }
        ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, ink);
        x = tr.right();
        if resp.clicked() {
            hit = Some(*tab);
        }
    }
    hit
}

/// The card discord keeps beside a one-on-one dm: the person's profile, full height.
pub(crate) fn dm_profile(app: &mut App, ui: &mut Ui, uid: Id) {
    let Some(user) = app.store.user(uid).cloned() else { return };
    if !app.profiles.contains_key(&uid) {
        app.profiles.insert(uid, serde_json::Value::Null);
        app.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=true&with_mutual_friends=true"), None, Tag::Profile(uid, 0));
    }
    if !app.notes.contains_key(&uid) {
        app.notes.insert(uid, String::new());
        app.rest("GET", format!("/users/@me/notes/{uid}"), None, Tag::Note(uid));
    }
    let stranger = !user.bot && !app.store.relationships.contains_key(&uid);
    let status = app.store.status(uid);
    let worn = app.profiles.get(&uid).filter(|p| !p.is_null()).cloned();
    let layers = crate::ui::cosmo::layers_of(app, worn.as_ref());
    let since = crate::ui::cosmo::since(ui.ctx(), ("side", uid));
    let (mut banner, mut full, mut tab) = (BannerAct::default(), false, None);
    let height = ui.available_height();
    let store = &app.store;
    let p = app.profiles.get(&uid);
    let bio = p.and_then(|p| shown(p, "bio").as_str().or(p["user"]["bio"].as_str())).unwrap_or("").to_string();
    Frame::new().fill(pal().bg_input).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).show(ui, |ui| {
        ui.set_width(300.0);
        ui.set_min_height(height - 2.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let wash = ui.painter().add(egui::Shape::Noop);
        let top = card_head(ui, &user, p, user.display(), status);
        banner = banner_buttons(ui, top, stranger, !stranger && !user.bot, false);
        full = clicked_flag(ui.ctx(), "card_name_clicked") == Some(uid);
        Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(268.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            tab = mutuals(ui, store, p, pal().bg_input, 14.4);
            ui.add_space(24.0);
            if !bio.is_empty() {
                let mut job = egui::text::LayoutJob::single_section(bio, egui::TextFormat::simple(reg(14.0), pal().header_primary));
                job.wrap.max_width = 268.0;
                ui.label(ui.painter().layout_job(job));
                ui.add_space(19.0);
            }
            ui.label(RichText::new("Member Since").font(semi(12.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let since = chrono::DateTime::from_timestamp_millis(crate::state::snowflake_ms(uid)).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
            ui.label(RichText::new(since).font(reg(14.0)).color(pal().text_normal));
        });
        let b = Rect::from_min_size(pos2(top.left() + 16.0, top.top() + height - 2.0 - 58.0), vec2(268.0, 40.0));
        let resp = ui.interact(b, ui.id().with("full_profile"), Sense::click());
        ui.painter().rect(b, 8, egui::Color32::from_white_alpha(if resp.hovered() { 31 } else { 20 }), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "View Full Profile", medium(16.0), pal().header_primary);
        full |= resp.clicked();
        theme_wash(ui, wash, p, ui.min_rect());
        crate::ui::cosmo::paint_layers(ui, ui.min_rect(), &layers, since);
    });
    if let Some(at) = banner.more.or(banner.friend) {
        app.user_menu = Some((uid, 0, None, at));
    }
    if banner.add {
        app.rest_in("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), "User Profile", Tag::Friend);
    }
    if let Some(t) = tab {
        app.open_full(uid, 0, t);
    } else if full {
        app.open_full(uid, 0, profile::Tab::Activity);
    }
}

/// A profile's field the way Discord shows it in a server: what the person set for that server,
/// else what their account has.
pub(crate) fn shown<'a>(p: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    let here = &p["guild_member_profile"][key];
    let unset = here.is_null() || here.as_str().is_some_and(str::is_empty) || here.as_array().is_some_and(|a| a.is_empty());
    if unset { &p["user_profile"][key] } else { here }
}

pub(crate) fn banner(p: &serde_json::Value, uid: Id) -> Option<String> {
    let cdn = crate::api::cdn();
    let here = &p["guild_member_profile"];
    match here["banner"].as_str() {
        Some(b) => Some(format!("{cdn}/guilds/{}/users/{uid}/banners/{b}.png?size=600", here["guild_id"].as_str()?)),
        None => p["user_profile"]["banner"].as_str().or(p["user"]["banner"].as_str()).map(|b| format!("{cdn}/banners/{uid}/{b}.png?size=600")),
    }
}

/// A profile's nitro theme as discord shows it: primary and accent, dimmed in dark themes.
pub(super) fn theme(p: Option<&serde_json::Value>) -> Option<(egui::Color32, egui::Color32)> {
    let c = p.and_then(|p| shown(p, "theme_colors").as_array()).filter(|c| c.len() == 2)?;
    let mine = p.and_then(|p| p["user"]["id"].as_str()).and_then(|id| id.parse::<u64>().ok()) == Some(crate::theme::me());
    if crate::theme::view().own_profiles && !mine {
        return None;
    }
    let k = if pal().dark { 0.4 } else { 1.0 };
    let col = |v: &serde_json::Value| {
        let c = v.as_u64().unwrap_or(0) as u32;
        let ch = |s: u32| (((c >> s) & 255) as f32 * k) as u8;
        egui::Color32::from_rgb(ch(16), ch(8), ch(0))
    };
    Some((col(&c[0]), col(&c[1])))
}

pub(crate) fn theme_top(p: Option<&serde_json::Value>) -> Option<egui::Color32> {
    theme(p).map(|(a, _)| a)
}

/// A nitro profile theme: the card washed from its primary colour down to its accent, dimmed the
/// way discord dims them in dark themes. Paints into `slot`, reserved before the card's content.
pub(crate) fn theme_wash(ui: &Ui, slot: egui::layers::ShapeIdx, p: Option<&serde_json::Value>, rect: Rect) {
    let Some((a, b)) = theme(p) else { return };
    let mut mesh = egui::Mesh::default();
    let size = ui.ctx().fonts(|f| f.font_image_size());
    let mut tess = egui::epaint::Tessellator::new(ui.ctx().pixels_per_point(), Default::default(), size, Vec::new());
    tess.tessellate_rect(&egui::epaint::RectShape::filled(rect, 7, egui::Color32::WHITE), &mut mesh);
    for v in &mut mesh.vertices {
        let t = ((v.pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        v.color = egui::Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), v.color.a());
    }
    ui.painter().set(slot, mesh);
}

/// The top of a profile card: banner, avatar with its decoration and status, then the names.
/// Hands back the banner so buttons can go on it.
/// Where a card's avatar sits under its banner.
pub(super) fn card_avatar(top: Rect) -> Rect {
    Rect::from_min_size(pos2(top.left() + 16.0, top.top() + 61.0), vec2(80.0, 80.0))
}

pub(crate) fn card_head(ui: &mut Ui, user: &crate::state::User, p: Option<&serde_json::Value>, name: &str, status: crate::state::Status) -> Rect {
    let accent = p
        .and_then(|p| shown(p, "accent_color").as_u64().or(p["user"]["accent_color"].as_u64()))
        .map(|c| role_color(c as u32))
        // no banner colour set: discord's pick from the avatar, as in call tiles
        .or_else(|| crate::ui::palette::avatar(ui.ctx(), &user.avatar_url()))
        .unwrap_or(pal().blurple);
    let banner = p.and_then(|p| banner(p, user.id));
    let pronouns = p.and_then(|p| shown(p, "pronouns").as_str()).filter(|s| !s.is_empty());
    let (top, _) = ui.allocate_exact_size(vec2(300.0, 105.0), Sense::hover());
    let cr = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
    ui.painter().rect_filled(top, cr, accent);
    if let Some(b) = &banner {
        crate::gif::paint_still(ui, b, top, cr, ui.rect_contains_pointer(top));
    }
    let av = card_avatar(top);
    // the ring around the avatar is cut from the card, themed or not
    let ring = theme_top(p).unwrap_or(pal().bg_secondary);
    ui.painter().circle_filled(av.center(), 46.0, ring);
    avatar(ui, av, &user.avatar_url().replace("size=80", "size=128"));
    if let Some(d) = user.decoration_url() {
        text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(96.0, 96.0)));
    }
    status_dot(ui, av, status, ring);
    ui.add_space(47.0);
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.set_width(268.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let face = if crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == user.font) { name_font(user.font, 20.0) } else { semi(20.0) };
            let r = text::label(ui, name, face, profile_name_ink(user), Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                click_flag(ui.ctx(), "card_name_clicked", user.id);
            }
            if user.bot {
                crate::ui::app_tag(ui.painter(), pos2(r.rect.right() + 4.0, r.rect.center().y), user.flags & (1 << 16) != 0, 1.0, false);
            }
        });
        ui.add_space(1.3);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let copy = text::label(ui, &user.username, reg(14.0), pal().header_primary, Sense::click());
            if copy.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if copy.clicked() {
                ui.ctx().copy_text(user.username.clone());
            }
            let mut r = copy.on_hover_text("Copy Username").rect;
            if let Some(pr) = pronouns {
                ui.painter().circle_filled(pos2(r.right() + 9.0, r.center().y), 2.0, pal().header_primary);
                ui.add_space(18.0);
                r = text::label(ui, pr, reg(14.0), pal().header_primary, Sense::hover()).rect;
            }
            let chip = crate::ui::profile_tag(ui.painter(), user, pos2(r.right() + 8.0, r.center().y));
            if let Some(t) = chip
                && crate::ui::tagpop::hit(ui, t, "card_tag")
                && let Some((_, g, _)) = &user.tag
            {
                click_flag(ui.ctx(), "card_tag_clicked", *g);
            }
            profile::badges(ui, p, chip.map_or(r.right() + 8.0, |t| t.right() + 6.0), r.center().y, 20.0);
        });
    });
    top
}

pub(super) enum AccountAct {
    Status(&'static str),
    StatusFor(&'static str, Option<i64>),
    Edit,
    Custom,
    CopyId,
    Switch(Id),
    Manage,
}

/// What opens from the account panel: your card, then discord's grouped rows.
pub(crate) fn account(app: &mut App, ui: &mut Ui) {
    use crate::state::Status;
    let me = app.store.me;
    let Some(user) = app.store.user(me).cloned() else { return };
    if !app.profiles.contains_key(&me) {
        // a placeholder until it lands, so this asks once
        app.profiles.insert(me, serde_json::Value::Null);
        app.rest("GET", format!("/users/{me}/profile?with_mutual_guilds=false"), None, Tag::Profile(me, 0));
    }
    let status = app.store.presence.get(&me).copied().unwrap_or(Status::Online);
    let dev = app.settings.dev;
    let custom = app.store.custom_status.clone().or_else(|| app.store.custom_emoji.as_ref().map(|_| String::new()));
    let emoji = app.store.custom_emoji.clone();
    let app_accounts = app.accounts.clone();
    let mut act = None;
    ui.set_width(300.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    let wash = ui.painter().add(egui::Shape::Noop);
    let top = card_head(ui, &user, app.profiles.get(&me), user.display(), status);
    let hint = ui.painter().layout_no_wrap("Favorite collectible?".into(), italic(14.0), pal().header_primary.gamma_multiply(0.76));
    let bubble = match &custom {
        // the speech bubble discord floats beside your avatar
        Some(c) => {
            let lead = if emoji.is_some() { 24.0 } else { 0.0 };
            let g = ui.painter().layout(c.clone(), reg(14.0), pal().header_primary, 155.0 - lead);
            let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(179.0, (g.size().y + 22.0).max(40.0)));
            ui.painter().rect(b, 16, pal().bg_floating, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            if let Some(e) = &emoji {
                egui::Image::from_uri(crate::ui::chat::emoji_url(e)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(b.left() + 22.0, b.center().y), vec2(18.0, 18.0)));
            }
            ui.painter().galley(pos2(b.left() + 12.0 + lead, b.center().y - g.size().y / 2.0), g, pal().header_primary);
            b
        }
        // and with nothing set, the prompt to add one
        None => {
            let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(hint.size().x + 48.0, 36.0));
            ui.painter().rect(b, 18, pal().bg_floating, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            crate::ui::icons::draw(ui.painter(), Icon::CirclePlus, Rect::from_center_size(pos2(b.left() + 20.0, b.center().y), vec2(16.0, 16.0)), pal().header_primary.gamma_multiply(0.76));
            ui.painter().galley(pos2(b.left() + 36.0, b.center().y - hint.size().y / 2.0), hint, pal().header_primary);
            b
        }
    };
    let bubble_hit = ui.interact(bubble, ui.id().with("custom_status"), Sense::click());
    if bubble_hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if bubble_hit.clicked() {
        act = Some(AccountAct::Custom);
    }
    let label = match status {
        Status::Online => "Online",
        Status::Idle => "Idle",
        Status::Dnd => "Do Not Disturb",
        Status::Offline => "Invisible",
    };
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 16 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let card = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            Frame::new().fill(PROFILE_CARD).corner_radius(8).show(ui, |ui| {
                ui.set_width(268.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui);
            });
        };
        card(ui, &mut |ui| {
            if account_row(ui, Some(Icon::Pencil), "Edit Profile", false, false).clicked() {
                act = Some(AccountAct::Edit);
            }
            if account_row(ui, Some(Icon::Smile), if custom.is_some() { "Edit Custom Status" } else { "Set a custom status" }, false, true).clicked() {
                act = Some(AccountAct::Custom);
            }
            let r = account_row(ui, None, label, true, true);
            let c = pos2(r.rect.left() + 24.0, r.rect.center().y);
            crate::ui::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), status, PROFILE_CARD);
            egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (key, label, sub, st) in [
                    ("online", "Online", None, Status::Online),
                    ("idle", "Idle", None, Status::Idle),
                    ("dnd", "Do Not Disturb", Some("You will not receive desktop notifications"), Status::Dnd),
                    ("invisible", "Invisible", Some("You will appear offline"), Status::Offline),
                ] {
                    let timed = key != "online";
                    let (resp, lead) = crate::ui::menu_row_at(ui, true, label, sub, false, if timed { crate::ui::Trail::Caret } else { crate::ui::Trail::None });
                    let c = lead.center();
                    crate::ui::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), st, pal().bg_floating);
                    if !timed && resp.clicked() {
                        act = Some(AccountAct::Status(key));
                    }
                    if timed {
                        egui::containers::menu::SubMenu::new().show(ui, &resp, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for (name, secs) in crate::ui::cstatus::STATUS_FOR {
                                if crate::ui::menu_item(ui, None, name, false) {
                                    act = Some(AccountAct::StatusFor(key, secs));
                                }
                            }
                        });
                    }
                    if key == "online" {
                        crate::ui::menu_sep(ui);
                    }
                }
            });
        });
        let others = app_accounts.clone();
        card(ui, &mut |ui| {
            let r = account_row(ui, Some(Icon::Person), "Switch Accounts", true, false);
            egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for a in &others {
                    let (resp, lead) = crate::ui::menu_row_at(ui, true, &a.name, None, false, crate::ui::Trail::Check(a.id == me));
                    let url = match &a.avatar {
                        Some(h) => format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), a.id),
                        None => format!("{}/embed/avatars/{}.png", crate::api::cdn(), (a.id >> 22) % 6),
                    };
                    avatar(ui, lead.expand(2.0), &url);
                    if resp.clicked() && a.id != me {
                        act = Some(AccountAct::Switch(a.id));
                    }
                }
                crate::ui::menu_sep(ui);
                if crate::ui::menu_item(ui, None, "Manage Accounts", false) {
                    act = Some(AccountAct::Manage);
                }
            });
            if dev && account_row(ui, Some(Icon::Id), "Copy User ID", false, true).clicked() {
                act = Some(AccountAct::CopyId);
            }
        });
    });
    theme_wash(ui, wash, app.profiles.get(&me), ui.min_rect());
    match act {
        Some(AccountAct::Status(s)) => app.set_status_for(s, None),
        Some(AccountAct::StatusFor(s, secs)) => app.set_status_for(s, secs),
        Some(AccountAct::Edit) => app.settings_page = Some(crate::ui::settings::Page::Profiles),
        Some(AccountAct::Custom) => app.status_text = Some(crate::ui::cstatus::State::from(&app.store)),
        Some(AccountAct::CopyId) => ui.ctx().copy_text(me.to_string()),
        Some(AccountAct::Switch(id)) => app.switch_account(id),
        Some(AccountAct::Manage) => app.user_dialog = Some(crate::ui::userdlg::Dialog::Accounts),
        None => {}
    }
    if act.is_some() {
        ui.close();
    }
}

/// A 48 tall row in the account card's groups, with a hairline above all but the first.
pub(super) fn account_row(ui: &mut Ui, icon: Option<Icon>, label: &str, caret: bool, rule: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(268.0, 48.0), Sense::click());
    if rule {
        ui.painter().hline(r.x_range().shrink(12.0), r.top(), egui::Stroke::new(1.0, pal().border_subtle));
    }
    crate::ui::motion::wash(ui, &resp, r.shrink(4.0), CornerRadius::same(4), MENU_HOVER, 0.05);
    if let Some(i) = icon {
        crate::ui::icons::draw(ui.painter(), i, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary);
    if caret {
        crate::ui::icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::{banner, shown};
    use serde_json::json;

    #[test]
    fn a_server_profile_overrides_the_account_only_where_it_set_something() {
        let p = json!({
            "user": { "banner": "aa" },
            "user_profile": { "bio": "account", "pronouns": "they", "theme_colors": [1, 2] },
            "guild_member_profile": { "guild_id": "7", "bio": "", "pronouns": "she", "theme_colors": [3, 4], "banner": "bb" },
        });
        assert_eq!(shown(&p, "bio"), "account");
        assert_eq!(shown(&p, "pronouns"), "she");
        assert_eq!(shown(&p, "theme_colors"), &json!([3, 4]));
        assert!(banner(&p, 5).unwrap().ends_with("/guilds/7/users/5/banners/bb.png?size=600"));
        assert!(banner(&json!({ "user": { "banner": "aa" } }), 5).unwrap().ends_with("/banners/5/aa.png?size=600"));
    }
}
