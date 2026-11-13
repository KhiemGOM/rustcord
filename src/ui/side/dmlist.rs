use super::*;

pub(super) fn dms(app: &mut App, ui: &mut Ui) {
    // discord's is a button into the quick switcher, not a filter
    let mut find = false;
    let mut menu = None;
    let mut cleanup = false;
    let mut leave = None;
    let mut wave = None;
    header(ui, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        // the button runs 8 from the sidebar's edges, where the server's name sits 16 in
        let r = r.expand2(vec2(8.0, 0.0));
        ui.painter().rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
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
            ui.add_space(10.0);
            let pending = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
            let rows = [(Icon::Friends, "Friends"), (Icon::Nitro, "Nitro Home"), (Icon::Shop, "Shop"), (Icon::Quests, "Quests")];
            let shop = app.shop.as_ref().map(|s| s.page);
            for (icon, label) in rows {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                let inner = row.shrink2(vec2(8.0, 0.0));
                let on = match label {
                    "Quests" => app.quests.is_some(),
                    "Nitro Home" => shop == Some(crate::ui::shop::Page::Nitro),
                    "Shop" => shop == Some(crate::ui::shop::Page::Shop),
                    _ => app.friends,
                };
                app.rects.insert(format!("home:{label}"), row);
                if on {
                    ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
                }
                let col = if on { pal().text_normal } else { crate::ui::motion::mix(pal().text_muted, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
                icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(inner.left() + 18.0, row.center().y), vec2(20.0, 20.0)), col);
                ui.painter().text(pos2(inner.left() + 40.0, row.center().y), Align2::LEFT_CENTER, label, medium(16.0), col);
                if label == "Friends" {
                    if pending > 0 {
                        badge(ui, pos2(inner.right() - 8.0, row.center().y), pending, None);
                    }
                    app.rects.insert("friends".into(), row);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    match label {
                        "Quests" => app.open_quests(),
                        "Nitro Home" => app.open_shop(crate::ui::shop::Page::Nitro),
                        "Shop" => app.open_shop(crate::ui::shop::Page::Shop),
                        _ => {
                            app.friends = true;
                            app.quests = None;
                            app.shop = None;
                        }
                    }
                }
            }
            // discord: 12 either side of a hairline, then a 24 header whose last 4 are padding
            ui.add_space(11.0);
            let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(rule.x_range().shrink(8.0), rule.center().y, egui::Stroke::new(1.0, pal().frame_border));
            ui.add_space(10.0);
            let (h, _) = ui.allocate_exact_size(vec2(ui.available_width(), 23.0), Sense::hover());
            let h = Rect::from_min_size(h.min, vec2(h.width(), 20.0));
            ui.painter().text(pos2(h.left() + 16.0, h.center().y), Align2::LEFT_CENTER, "Direct Messages", medium(14.0), pal().channel_default);
            let pr = Rect::from_center_size(pos2(h.right() - 24.0, h.center().y), vec2(16.0, 16.0));
            let plus = ui.interact(pr.expand(4.0), ui.id().with("new_dm"), Sense::click());
            icons::draw(ui.painter(), Icon::Plus, pr, crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &plus)));
            if plus.on_hover_text("Create DM").clicked() {
                app.group_pick = Some(crate::ui::dmpick::Pick::new(None));
            }
            let mr = Rect::from_center_size(pos2(h.right() - 48.0, h.center().y), vec2(16.0, 16.0));
            let more = ui.interact(mr.expand(4.0), ui.id().with("dm_more"), Sense::click());
            let shown = ui.rect_contains_pointer(h) || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&more));
            if shown {
                icons::draw(ui.painter(), Icon::More, mr, if more.hovered() { pal().interactive_hover } else { pal().channel_default });
                ui.ctx().set_cursor_icon(if more.hovered() { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default });
            }
            egui::Popup::menu(&more.on_hover_text("More")).show(|ui| {
                if crate::ui::menu_item(ui, None, "Clean up Group DMs…", false) {
                    cleanup = true;
                }
            });
            let store = &app.store;
            let mut ids = store.dms.clone();
            ids.sort_by_key(|id| !store.pinned.contains(id));
            for id in ids {
                let Some(c) = store.channels.get(&id) else { continue };
                let title = channel_title(store, id);
                let (r, act) = dm_row(ui, store, &app.voice, c, &title, cur == Some(id) && !app.friends && app.quests.is_none() && app.shop.is_none());
                app.rects.insert(format!("chan:{id}"), r.rect);
                match act {
                    Some(DmAct::Close) => leave = Some(id),
                    Some(DmAct::Wave) => wave = Some(id),
                    None if r.clicked() => open = Some(id),
                    None => {}
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
    if let Some(c) = leave {
        crate::ui::popups::leave_dm(app, c);
    }
    if let Some(c) = wave {
        app.open(c);
        app.net.send(Cmd::Send { channel: c, content: String::new(), nonce: crate::state::make_nonce(), reply: None, files: Vec::new(), stickers: vec![WAVE_STICKER], tts: false });
    }
    if cleanup {
        app.user_dialog = Some(crate::ui::userdlg::Dialog::cleanup());
    }
}

pub(super) const WAVE_STICKER: Id = 749054660769218631;

pub(super) fn wave_here(store: &Store, c: &Channel) -> bool {
    let [u] = c.recipients[..] else { return false };
    c.kind == 1
        && c.last_msg == 0
        && store.relationships.get(&u) == Some(&FRIEND)
        && !store.ignored.contains(&u)
        && store.user(u).is_some_and(|u| !u.bot)
        && chrono::Utc::now().timestamp_millis() - snowflake_ms(c.id) < 21 * 86_400_000
}

pub(super) enum DmAct {
    Close,
    Wave,
}

pub(super) fn dm_row(ui: &mut Ui, store: &Store, voice: &crate::voice::VoiceUi, c: &Channel, title: &str, selected: bool) -> (egui::Response, Option<DmAct>) {
    let (row, resp, hl) = dense_row(ui, 42.0);
    let inner = row.shrink2(vec2(8.0, 0.0));
    let hovered = resp.contains_pointer();
    if selected {
        ui.painter().rect_filled(hl, CornerRadius::same(8), pal().bg_mod_selected);
    } else if hovered {
        ui.painter().rect_filled(hl, CornerRadius::same(8), pal().bg_mod_hover);
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
    if voice.ringing == Some(c.id) {
        let k = (ui.input(|i| i.time) as f32 * 1.5).fract();
        ui.painter().circle_stroke(av.center(), 18.0 + 4.0 * k, egui::Stroke::new(2.0, pal().green.gamma_multiply(1.0 - k)));
        ui.ctx().request_repaint_after_secs(0.033);
    }
    if muted {
        let under = if selected { pal().bg_mod_selected } else if hovered { pal().bg_mod_hover } else { pal().bg_tertiary };
        ui.painter().circle_filled(av.center(), 17.0, under.gamma_multiply(0.7));
    }
    let col = if selected || unread {
        pal().header_primary
    } else if hovered {
        pal().interactive_hover
    } else {
        pal().text_muted
    };
    let mentions = if muted { 0 } else { store.mentions(c.id) };
    let right = if mentions > 0 { inner.right() - 30.0 } else { inner.right() - 8.0 };
    // discord keeps a styled name's font when it's unread; a plain one goes semibold
    let style = name_font(person.map_or(0, |u| u.font), 16.0);
    let font = if unread && style == medium(16.0) { semi(16.0) } else { style };
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
        // with a status or an activity the name rides up and the line sits under it
        let line = person.filter(|u| store.status(u.id) != Status::Offline).and_then(|u| store.activity_line(u.id));
        let y = if line.is_some() { row.top() + 13.5 } else { row.center().y };
        let r = text::paint_em(ui.painter(), pos2(av.right() + 12.0, y), Align2::LEFT_CENTER, title, font, col, tw, 1.0);
        if let Some(u) = person {
            // discord's plain name keeps 3.5 more before the tag than one in a display font
            crate::ui::tag_chip(&clip, u, pos2(r.right() + if u.font == 0 { 7.5 } else { 4.0 }, y), 1.0);
        }
        if let Some(line) = line {
            crate::ui::members::status_line(&clip, line, av.right() + 12.0, row.top() + 30.0, right, col, 12.0, "");
        }
    }
    let mut act = None;
    if wave_here(store, c) {
        let w = text::layout_em(ui.ctx(), "👋 Wave", medium(14.0), pal().text_normal, 400.0, 1.0).size().x;
        let b = Rect::from_min_size(pos2(row.right() - 13.0 - w - 24.0, row.center().y - 16.0), vec2(w + 24.0, 32.0));
        let hit = ui.interact(b, ui.id().with(("dm_wave", c.id)), Sense::click());
        crate::ui::settings::mark(ui, &format!("dm_wave:{}", c.id), b);
        let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &hit));
        ui.painter().rect(b, CornerRadius::same(8), fill, egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
        text::paint_em(ui.painter(), pos2(b.left() + 12.0, b.center().y), Align2::LEFT_CENTER, "👋 Wave", medium(14.0), pal().text_normal, w, 1.0);
        if hit.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if hit.on_hover_text(format!("Wave to {title}")).clicked() {
            act = Some(DmAct::Wave);
        }
    } else if hovered {
        let xr = Rect::from_center_size(pos2(inner.right() - 16.0, row.center().y), vec2(16.0, 16.0));
        let hit = ui.interact(xr.expand(4.0), ui.id().with(("dm_close", c.id)), Sense::click());
        crate::ui::settings::mark(ui, &format!("dm_close:{}", c.id), xr);
        icons::draw(ui.painter(), Icon::Close, xr, if hit.hovered() { pal().interactive_hover } else { pal().channel_default });
        if hit.on_hover_text("Close DM").clicked() {
            act = Some(DmAct::Close);
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if mentions > 0 {
        badge(ui, pos2(inner.right() - 8.0, row.center().y), mentions, None);
    }
    (resp, act)
}

pub(super) fn group_color(id: Id) -> Color32 {
    let c = [
        pal().blurple,
        Color32::from_rgb(0x3b, 0xa5, 0x5c),
        Color32::from_rgb(0xfa, 0xa6, 0x1a),
        Color32::from_rgb(0xed, 0x42, 0x45),
        Color32::from_rgb(0xeb, 0x45, 0x9e),
    ];
    c[(id % 5) as usize]
}
