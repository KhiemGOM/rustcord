use super::*;

pub(super) fn pins(app: &mut App, ctx: &egui::Context) {
    let Some((ch, list)) = &app.pins else { return };
    let ch = *ch;
    // right-aligned under the button, as tall as 80% of the window allows
    let at = app.rects.get("pins_button").map(|r| pos2(r.right() - 420.0, r.bottom() + 9.0)).unwrap_or(pos2(400.0, 60.0));
    let tall = ctx.content_rect().height() * 0.8 - 57.0;
    let mut jump = None;
    let mut unpin = None;
    let manage = app.store.channels.get(&ch).is_some_and(|c| c.guild == 0 || app.store.perms(c) & crate::state::PIN_MESSAGES != 0);
    let area = crate::ui::motion::popout(ctx, egui::Area::new(egui::Id::new("pins"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true), vec2(0.0, 1.0))
        .show(ctx, |ui| {
            // an area only offers last frame's size; without this the list never outgrows the spinner
            ui.set_max_height(tall + 57.0);
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20))).show(ui, |ui| {
                ui.set_width(420.0);
                ui.set_max_width(420.0);
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 17 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Pin, r, pal().header_secondary);
                        ui.label(RichText::new("Pinned Messages").font(semi(20.0)).color(pal().header_primary));
                    });
                });
                egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| {
                    Frame::new().inner_margin(Margin { left: 8, right: 12, top: 0, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match list {
                            None => {
                                crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                            }
                            Some(l) if l.is_empty() => {
                                ui.add_space(24.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("This channel doesn't have any
pinned messages... yet.").font(medium(16.0)).color(pal().header_primary));
                                });
                                ui.add_space(24.0);
                                let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                                ui.painter().hline(sep.x_range(), sep.center().y, egui::Stroke::new(1.0, pal().border_subtle));
                                ui.add_space(12.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("PROTIP:").font(bold(12.0)).color(pal().green));
                                    ui.label(
                                        RichText::new("Users with the 'Pin Messages' permission can pin a message from its context menu.")
                                            .font(reg(14.0))
                                            .color(pal().header_secondary),
                                    );
                                });
                            }
                            Some(l) => {
                                for m in l {
                                    let card = chat::preview(ui, &app.store, m).rect;
                                    // discord's Jump and unpin ✕, over the card's corner while it's hovered
                                    let hot = ui.rect_contains_pointer(card);
                                    let mut x = card.right() - 8.0;
                                    let mut corner = |ui: &mut Ui, icon: Icon, tip: &str, key: &str| {
                                        let r = Rect::from_min_size(pos2(x - 28.0, card.top() + 8.0), vec2(28.0, 28.0));
                                        x -= 32.0;
                                        let resp = ui.interact(r, ui.id().with((key, m.id)), Sense::click());
                                        app.rects.insert(format!("pins:{key}:{}", m.id), r);
                                        if hot {
                                            ui.painter().rect(r, 8, crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                            crate::ui::icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
                                        }
                                        hot && resp.on_hover_text(tip).clicked()
                                    };
                                    if manage && corner(ui, Icon::Close, "Unpin Message", "unpin") {
                                        unpin = Some(m.id);
                                    }
                                    if corner(ui, Icon::Jump, "Jump to Message", "jump") {
                                        jump = Some(m.id);
                                    }
                                    ui.add_space(8.0);
                                }
                            }
                        }
                    });
                });
            });
        });
    // the popout stays up after a jump, as discord's does
    if let Some(id) = jump {
        app.jump_to(ch, id);
    } else if let Some(id) = unpin {
        let ask = !ctx.input(|i| i.modifiers.shift);
        app.pin_msg(ch, id, ask);
    } else if app.confirm.is_none()
        && (ctx.input(|i| i.key_pressed(Key::Escape))
            || dismissed(ctx, &area.response) && !app.rects.get("pins_button").is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.contains(p))))
    {
        app.pins = None;
    }
}

pub(super) fn sort_button(ui: &mut Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(78.0, 32.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(rect, 8, fill, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    crate::ui::icons::draw(ui.painter(), Icon::Sort, Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(16.0, 16.0)), pal().header_primary);
    ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, "Sort", medium(14.0), pal().header_primary);
    resp
}

pub(crate) fn search_panel(app: &mut App, ui: &mut Ui) {
    let Some(s) = &app.search else { return };
    let mut close = false;
    let mut jump = None;
    let header = match &s.results {
        None => "Searching…".to_string(),
        Some(_) if s.total == 1 => "1 Result".to_string(),
        Some(_) => format!("{} Results", s.total),
    };
    let mut sort = None;
    Frame::new().fill(pal().bg_secondary).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(header).font(semi(16.0)).color(pal().header_primary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                    close = true;
                }
                let label = ["Newest", "Oldest", "Most Relevant"][s.sort as usize % 3];
                let b = sort_button(ui);
                crate::ui::settings::mark(ui, "search_sort", b.rect);
                let frame = Frame::menu(&ui.ctx().global_style());
                egui::Popup::menu(&b).align(egui::RectAlign::BOTTOM_END).gap(4.0).frame(frame).show(|ui| {
                    ui.set_width(148.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, name) in ["Newest", "Oldest", "Most Relevant"].into_iter().enumerate() {
                        if crate::ui::menu_row(ui, None, name, None, false, crate::ui::Trail::Radio(name == label)).clicked() {
                            sort = Some(i as u8);
                        }
                    }
                });
            });
        });
    });
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &s.results {
                None => {
                    crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                }
                Some(list) if list.is_empty() => {
                    ui.label(RichText::new("We searched far and wide. Unfortunately, no results were found.").color(pal().header_secondary));
                }
                Some(list) => {
                    let mut last = 0;
                    for m in list {
                        if s.guild && m.channel != last {
                            last = m.channel;
                            let name = app.store.channels.get(&m.channel).map(|c| c.name.clone()).unwrap_or_default();
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("# {name}")).font(semi(14.0)).color(pal().header_secondary));
                            ui.add_space(4.0);
                        }
                        if chat::preview(ui, &app.store, m).clicked() {
                            jump = Some((m.channel, m.id));
                        }
                        ui.add_space(8.0);
                    }
                }
            }
        });
    });
    if close {
        app.search = None;
    }
    if let Some(n) = sort
        && let Some(s) = &mut app.search
    {
        let (ch, q) = (s.ch, s.query.clone());
        s.sort = n;
        app.start_search(ch, q);
    }
    if let Some((c, id)) = jump {
        app.jump_to(c, id);
    }
}

/// What drops from the search box while it's focused. Empty, discord's filters and past
/// searches; typed, a row to search and the people and channels the last word could mean.
/// Picking a filter types it; picking a search runs it.
pub(crate) fn search_options(app: &mut App, ctx: &egui::Context, field: &egui::Response, anchor: Rect, q: &mut String, guild: Id) -> Option<String> {
    let id = egui::Id::new("search_options");
    let hovered = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if !(field.has_focus() || hovered) {
        ctx.data_mut(|d| {
            d.insert_temp(id, false);
            d.insert_temp(id.with("more"), false);
        });
        return None;
    }
    let typed = q.trim().to_string();
    let word = typed.rsplit(' ').next().unwrap_or("").to_lowercase();
    let filtering = word.contains(':');
    let store = &app.store;
    // who and where the last word might be, a couple of each like discord
    let people: Vec<(Id, String, String)> = if typed.is_empty() || filtering {
        Vec::new()
    } else {
        let pool: Vec<Id> = match store.members.get(&guild) {
            Some(m) => m.keys().copied().collect(),
            None => store.users.keys().copied().collect(),
        };
        pool.into_iter()
            .filter_map(|u| store.user(u))
            .filter(|u| u.username.to_lowercase().contains(&word) || u.display().to_lowercase().contains(&word))
            .take(2)
            .map(|u| (u.id, store.name_in(guild, u.id), u.username.clone()))
            .collect()
    };
    let places: Vec<(Id, String)> = if typed.is_empty() || filtering || guild == 0 {
        Vec::new()
    } else {
        store.channels.values().filter(|c| c.guild == guild && c.is_text() && !c.is_thread() && c.name.to_lowercase().contains(&word)).take(2).map(|c| (c.id, c.name.clone())).collect()
    };
    let mut run = None;
    let mut add: Option<String> = None;
    let mut replace: Option<String> = None;
    let area = crate::ui::motion::popout(ctx, egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(anchor.right_bottom() + vec2(-6.0, 8.0)), vec2(0.0, 1.0))
        .show(ctx, |ui| {
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(352.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let head = |ui: &mut Ui, t: &str| {
                    let (r, _) = ui.allocate_exact_size(vec2(352.0, 36.0), Sense::hover());
                    ui.painter().text(pos2(r.left() + 15.0, r.top() + 25.0), Align2::LEFT_CENTER, t, semi(12.0), pal().header_secondary);
                };
                let rule = |ui: &mut Ui| {
                    ui.add_space(8.0);
                    let (r, _) = ui.allocate_exact_size(vec2(352.0, 1.0), Sense::hover());
                    ui.painter().hline(r.x_range(), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
                };
                // a two-line row: icon, a title, and "key: value" under it
                let row = |ui: &mut Ui, icon: Icon, avatar_url: Option<String>, title: &str, key: &str, hint: &str| {
                    let (r, resp) = ui.allocate_exact_size(vec2(352.0, 48.0), Sense::click());
                    let p = ui.painter();
                    crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                    crate::ui::icons::draw(p, icon, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                    let mut x = r.left() + 43.0;
                    if let Some(url) = avatar_url {
                        avatar(ui, Rect::from_center_size(pos2(x + 8.0, r.center().y - 9.0), vec2(16.0, 16.0)), &url);
                        x += 22.0;
                    }
                    let p = ui.painter();
                    text::paint(p, pos2(x, r.center().y - 9.0), Align2::LEFT_CENTER, title, medium(14.0), pal().header_primary, 280.0);
                    let k = p.text(pos2(r.left() + 43.0, r.center().y + 9.0), Align2::LEFT_CENTER, key, semi(14.0), pal().header_secondary);
                    p.text(pos2(k.right() + 4.0, r.center().y + 9.0), Align2::LEFT_CENTER, hint, medium(14.0), pal().text_muted);
                    resp.clicked()
                };
                let more_id = id.with("more");
                let more = ui.ctx().data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
                let has_word = word.strip_prefix("has:");
                if let Some(have) = has_word {
                    head(ui, "Message Contains");
                    ui.add_space(4.0);
                    for (icon, kind) in [(Icon::Image, "image"), (Icon::Video, "video"), (Icon::Link, "link"), (Icon::File, "file"), (Icon::Play, "embed"), (Icon::Soundboard, "sound"), (Icon::Poll, "poll"), (Icon::Sticker, "sticker"), (Icon::Forward, "forward")] {
                        if !kind.starts_with(have) {
                            continue;
                        }
                        let (r, resp) = ui.allocate_exact_size(vec2(352.0, 48.0), Sense::click());
                        let p = ui.painter();
                        crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                        crate::ui::icons::draw(p, icon, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(24.0, 24.0)), pal().header_secondary);
                        p.text(pos2(r.left() + 50.0, r.center().y), Align2::LEFT_CENTER, kind, medium(16.0), pal().header_primary);
                        if resp.clicked() {
                            replace = Some(format!("has:{kind} "));
                        }
                    }
                } else if more {
                    head(ui, "More filters");
                    ui.add_space(4.0);
                    for (icon, title, key, hint) in [
                        (Icon::Clock, "Before a date", "before:", "specific date"),
                        (Icon::Clock, "After a date", "after:", "specific date"),
                        (Icon::Clock, "Sent on a date", "during:", "specific date"),
                        (Icon::Pin, "Pinned", "pinned:", "true or false"),
                        (Icon::Person, "By author type", "author_type:", "user, bot or webhook"),
                    ] {
                        if row(ui, icon, None, title, key, hint) {
                            add = Some(key.to_string());
                            ui.ctx().data_mut(|d| d.insert_temp(more_id, false));
                        }
                    }
                } else if typed.is_empty() {
                    head(ui, "Filters");
                    ui.add_space(4.0);
                    for (icon, title, key, hint) in [
                        (Icon::Person, "From a specific user", "from:", "user"),
                        (Icon::Hash, "Sent in a specific channel", "in:", "channel"),
                        (Icon::Attach, "Includes a specific type of data", "has:", "link, embed or file"),
                        (Icon::At, "Mentions a specific user", "mentions:", "user"),
                    ] {
                        if row(ui, icon, None, title, key, hint) {
                            add = Some(key.to_string());
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(352.0, 48.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                    crate::ui::icons::draw(ui.painter(), Icon::Filter, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                    ui.painter().text(pos2(r.left() + 43.0, r.center().y - 9.0), Align2::LEFT_CENTER, "More filters", medium(14.0), pal().header_primary);
                    ui.painter().text(pos2(r.left() + 43.0, r.center().y + 9.0), Align2::LEFT_CENTER, "dates, author type, and more", medium(14.0), pal().text_muted);
                    app.rects.insert("search_more".into(), r);
                    if resp.clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(more_id, true));
                    }
                    let history = &app.ui.searches;
                    if !history.is_empty() {
                        rule(ui);
                        head(ui, "History");
                        ui.add_space(4.0);
                        for h in history {
                            let (r, resp) = ui.allocate_exact_size(vec2(352.0, 36.0), Sense::click());
                            let p = ui.painter();
                            crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                            crate::ui::icons::draw(p, Icon::Clock, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                            text::paint(p, pos2(r.left() + 43.0, r.center().y), Align2::LEFT_CENTER, h, semi(14.0), pal().header_primary, 290.0);
                            if resp.clicked() {
                                run = Some(h.clone());
                            }
                        }
                    }
                } else {
                    ui.add_space(8.0);
                    let (r, resp) = ui.allocate_exact_size(vec2(352.0, 36.0), Sense::click());
                    let p = ui.painter();
                    crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                    crate::ui::icons::draw(p, Icon::Search, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                    let s = p.text(pos2(r.left() + 43.0, r.center().y), Align2::LEFT_CENTER, "Search for ", medium(14.0), pal().header_primary);
                    text::paint(p, pos2(s.right(), r.center().y), Align2::LEFT_CENTER, &typed, semi(14.0), pal().header_primary, 290.0 - s.width());
                    if resp.clicked() {
                        run = Some(typed.clone());
                    }
                    // discord's order: who it's from, where, who it mentions
                    if !people.is_empty() {
                        rule(ui);
                        head(ui, "From User");
                        for (uid, name, username) in &people {
                            if row(ui, Icon::Person, store.user(*uid).map(|u| u.avatar_url()), name, "from:", username) {
                                replace = Some(format!("from:{username} "));
                            }
                        }
                    }
                    if !places.is_empty() {
                        rule(ui);
                        head(ui, "In Channel");
                        for (_, name) in &places {
                            let (r, resp) = ui.allocate_exact_size(vec2(352.0, 36.0), Sense::click());
                            let p = ui.painter();
                            crate::ui::motion::wash(ui, &resp, r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER, 0.05);
                            crate::ui::icons::draw(p, Icon::Hash, Rect::from_center_size(pos2(r.left() + 25.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                            let k = p.text(pos2(r.left() + 43.0, r.center().y), Align2::LEFT_CENTER, "in:", semi(14.0), pal().header_secondary);
                            text::paint(p, pos2(k.right() + 4.0, r.center().y), Align2::LEFT_CENTER, &format!("# {name}"), semi(14.0), pal().header_primary, 280.0);
                            if resp.clicked() {
                                replace = Some(format!("in:{name} "));
                            }
                        }
                    }
                    if !people.is_empty() {
                        rule(ui);
                        head(ui, "Mentions User");
                        for (uid, name, username) in &people {
                            if row(ui, Icon::At, store.user(*uid).map(|u| u.avatar_url()), name, "mentions:", username) {
                                replace = Some(format!("mentions:{username} "));
                            }
                        }
                    }
                }
                ui.add_space(8.0);
            });
        });
    ctx.data_mut(|d| d.insert_temp(id, area.response.contains_pointer()));
    app.rects.insert("search_options".into(), area.response.rect);
    if let Some(k) = add {
        q.push_str(&k);
        field.request_focus();
    }
    if let Some(r) = replace {
        // the suggestion takes the place of the word that called it up
        let keep = q.trim_end().rsplit_once(' ').map(|(head, _)| format!("{head} ")).unwrap_or_default();
        *q = keep + &r;
        field.request_focus();
    }
    run
}

/// Discord's thread browser under the header's threads button, and Create for a new one.
pub(crate) fn threads_menu(app: &mut App, button: &egui::Response, ch: Id) {
    let mut open = None;
    let mut create = false;
    let shadow = button.ctx.global_style().visuals.popup_shadow;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow);
    let menu = egui::Popup::menu(button).align(egui::RectAlign::BOTTOM_END).gap(8.0).frame(frame);
    menu.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(597.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let qid = ui.id().with("thread_search");
        let mut q: String = ui.ctx().data(|d| d.get_temp(qid)).unwrap_or_default();
        let (head, _) = ui.allocate_exact_size(vec2(597.0, 49.0), Sense::hover());
        let p = ui.painter();
        p.hline(head.x_range(), head.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
        crate::ui::icons::draw(p, Icon::Thread, Rect::from_min_size(pos2(head.left() + 16.0, head.top() + 12.0), vec2(24.0, 24.0)), pal().header_secondary);
        p.text(pos2(head.left() + 48.0, head.center().y), Align2::LEFT_CENTER, "Threads", semi(16.0), pal().header_primary);
        p.vline(head.left() + 120.0, head.center().y - 12.0..=head.center().y + 12.0, egui::Stroke::new(1.0, pal().border_subtle));
        let field = Rect::from_min_size(pos2(head.left() + 137.0, head.top() + 8.0), vec2(279.0, 32.0));
        ui.painter().rect_filled(field, 8, pal().input_bg);
        let inner = Rect::from_min_max(pos2(field.left() + 30.0, field.center().y - 9.0), pos2(field.right() - 8.0, field.center().y + 9.0));
        let edit = ui.put(
            inner,
            egui::TextEdit::singleline(&mut q)
                .hint_text(RichText::new("Search for Thread Name").color(pal().text_muted))
                .font(reg(14.0))
                .frame(Frame::NONE)
                .margin(Margin::ZERO),
        );
        let ring = if edit.has_focus() { pal().blurple } else { pal().border_subtle };
        ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, ring), egui::StrokeKind::Inside);
        crate::ui::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(12.0, 12.0)), pal().header_primary);
        ui.ctx().data_mut(|d| d.insert_temp(qid, q.clone()));
        let b = Rect::from_min_size(pos2(head.right() - 80.0, head.top() + 8.0), vec2(64.0, 32.0));
        let br = ui.interact(b, ui.id().with("thread_create"), Sense::click());
        ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &br)));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create", medium(14.0), egui::Color32::WHITE);
        create |= br.clicked();

        let needle = q.to_lowercase();
        let mut list: Vec<_> = app
            .store
            .channels
            .values()
            .filter(|c| c.is_thread() && c.parent == ch && !c.archived && c.name.to_lowercase().contains(&needle))
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.last_msg.max(c.id)));
        if list.is_empty() {
            let (body, _) = ui.allocate_exact_size(vec2(597.0, 400.0), Sense::hover());
            let p = ui.painter();
            let c = pos2(body.center().x, body.top() + 110.0);
            p.circle_filled(c, 40.0, pal().bg_tertiary);
            crate::ui::icons::draw(p, Icon::Thread, Rect::from_center_size(c, vec2(36.0, 36.0)), pal().header_secondary);
            p.text(pos2(c.x, body.top() + 182.0), Align2::CENTER_CENTER, "There are no threads.", semi(24.0), pal().header_primary);
            p.text(pos2(c.x, body.top() + 215.0), Align2::CENTER_CENTER, "Stay focused on a conversation with a thread - a temporary text channel.", reg(16.0), pal().text_normal);
            let b = Rect::from_center_size(pos2(c.x, body.top() + 259.0), vec2(130.0, 40.0));
            let br = ui.interact(b, ui.id().with("thread_create_big"), Sense::click());
            ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &br)));
            ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create Thread", medium(16.0), egui::Color32::WHITE);
            create |= br.clicked();
            if create {
                ui.close();
            }
            return;
        }
        egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, true]).show(ui, |ui| {
            Frame::new().inner_margin(16).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let n = list.len();
                ui.label(RichText::new(format!("{n} Active Thread{}", if n == 1 { "" } else { "s" })).font(medium(14.0)).color(pal().header_secondary));
                for c in list {
                    let (r, resp) = ui.allocate_exact_size(vec2(565.0, 64.0), Sense::click());
                    ui.painter().rect_filled(r, 8, crate::ui::motion::mix(pal().bg_floating, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                    text::paint(ui.painter(), pos2(r.left() + 16.0, r.top() + 22.0), Align2::LEFT_CENTER, &c.name, semi(16.0), pal().header_primary, r.width() - 32.0);
                    let when = (c.last_msg != 0).then(|| ago(crate::state::snowflake_ms(c.last_msg))).unwrap_or_default();
                    let sub = format!("{} message{}{}", c.msg_count, if c.msg_count == 1 { "" } else { "s" }, if when.is_empty() { String::new() } else { format!(" • {when}") });
                    ui.painter().text(pos2(r.left() + 16.0, r.top() + 44.0), Align2::LEFT_CENTER, sub, reg(14.0), pal().header_secondary);
                    if resp.clicked() {
                        open = Some(c.id);
                    }
                }
            });
        });
        if open.is_some() || create {
            ui.close();
        }
    });
    if let Some(c) = open {
        let shift = button.ctx.input(|i| i.modifiers.shift);
        app.open_thread(c, true, shift);
    }
    if create {
        app.new_thread = Some(NewThread::blank(ch));
    }
}

/// "5m ago", "3h ago", "2d ago", the way discord shortens recent times.
pub(super) fn ago(ms: i64) -> String {
    let s = (chrono::Utc::now().timestamp_millis() - ms).max(0) / 1000;
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}
