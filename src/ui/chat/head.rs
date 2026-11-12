use super::*;

/// A voice channel's own text chat beside the call.
pub(super) fn voice_chat(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let mut close = false;
    side_card(app, ui, chan, "voice_chat", |app, ui, h| {
        let y = h.top() + 25.0;
        icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(pos2(h.left() + 26.0, y), vec2(20.0, 20.0)), pal().channel_default);
        text::paint_em(ui.painter(), pos2(h.left() + 45.0, y), Align2::LEFT_CENTER, &chan.name, medium(16.0), pal().header_primary, h.width() - 100.0, 1.0);
        let x = Rect::from_center_size(pos2(h.right() - 25.0, y), vec2(20.0, 20.0));
        let r = ui.interact(x.expand(4.0), ui.id().with("voice_chat_close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &r)));
        app.rects.insert("voice_chat:close".into(), x);
        close = r.on_hover_text("Close").clicked();
    });
    if close {
        app.voice_chat = false;
    }
}

/// A channel's chat in discord's side card, rounded at the top left with a hairline down its
/// left and along its top: the header `head` draws into, then the channel as any other.
pub(crate) fn side_card(app: &mut App, ui: &mut Ui, chan: &Channel, key: &'static str, head: impl FnOnce(&mut App, &mut Ui, Rect)) {
    let card = ui.max_rect();
    let round = CornerRadius { nw: 12, ..Default::default() };
    ui.painter().rect_filled(card, round, pal().bg_primary);
    let edge = Rect::from_min_max(card.min, card.max + vec2(4.0, 4.0));
    ui.painter().with_clip_rect(card).rect_stroke(edge, round, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    egui::Panel::top(egui::Id::new((key, "head")))
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let h = ui.max_rect();
            ui.painter().hline(h.left() + 1.0..=h.right(), h.bottom() - 0.5, Stroke::new(1.0, pal().border_subtle));
            head(app, ui, h);
        });
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::Panel::bottom(egui::Id::new((key, "composer")))
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 0 }))
            .show(ui, |ui| crate::ui::composer::show(app, ui, chan));
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| messages(app, ui, chan));
    });
    app.rects.insert(key.into(), card);
}

pub(super) fn header(app: &mut App, ui: &mut Ui, chan: &Channel) {
    egui::Panel::top("chat_head")
        .exact_size(49.0)
        .resizable(false)
        .show_separator_line(false)
        // discord's search box sits 8 from the window's edge
        // and everything centres in the 48 above the border
        .frame(Frame::new().fill(pal().bg_primary).inner_margin(Margin { left: 16, right: 8, top: 0, bottom: 1 }))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.left() - 16.0..=r.right() + 8.0, r.bottom() + 0.5, Stroke::new(1.0, pal().border_subtle));
            let panel = ui.max_rect();
            ui.horizontal_centered(|ui| {
                let title = channel_title(&app.store, chan.id);
                let tools = tools(chan);
                // discord's toolbar is its icons 8 apart and then the search box, which shrinks first;
                // the name and topic get what is left, clipped with a fade
                let bar = 8.0 + tools.iter().map(|t| t.width() + 8.0).sum::<f32>() + 4.0;
                let room = ui.available_width();
                let name_w = (room - bar - SEARCH_W).max(0.0);
                let search_w = (SEARCH_W - (bar + SEARCH_W - room).max(0.0)).max(0.0);
                let (name, _) = ui.allocate_exact_size(vec2(name_w, 32.0), Sense::hover());
                app.rects.insert("header:name".into(), name);
                let mut group = ui.new_child(UiBuilder::new().max_rect(name).layout(Layout::left_to_right(Align::Center)));
                group.set_clip_rect(name.intersect(ui.clip_rect()));
                {
                    let ui = &mut group;
                    // discord's header icons are 20, the name 9 past them
                    let glyph = |ui: &mut Ui, icon: Icon| {
                        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                        icons::draw(ui.painter(), icon, r, pal().channel_default);
                        ui.add_space(4.0);
                    };
                    let chan_glyph = |ui: &mut Ui, store: &crate::state::Store, c: &Channel| {
                        let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                        side::chan_icon(store, ui.painter(), c, r, pal().channel_default);
                        ui.add_space(4.0);
                        r
                    };
                    match chan.kind {
                        1 => {
                            if let Some(u) = chan.recipients.first().and_then(|u| app.store.user(*u)) {
                                let (av, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                                avatar(ui, av, &u.avatar_url());
                            }
                        }
                        3 => glyph(ui, Icon::People),
                        10..=12 => {
                            let parent = app.store.channels.get(&chan.parent).cloned();
                            if let Some(p) = parent {
                                chan_glyph(ui, &app.store, &p);
                                let r = text::label(ui, &p.name, medium(16.0), pal().header_secondary, Sense::click());
                                if r.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if r.clicked() {
                                    app.open(p.id);
                                }
                                let (r, _) = ui.allocate_exact_size(vec2(16.0, 24.0), Sense::hover());
                                icons::draw(ui.painter(), Icon::ChevronRight, r, pal().channel_default);
                            }
                            glyph(ui, Icon::Thread);
                        }
                        _ => {
                            let r = chan_glyph(ui, &app.store, chan);
                            app.rects.insert("header:icon".into(), r);
                        }
                    }
                    // the name keeps its width and is clipped by the group, as discord's overflow does
                    let t = text::layout_em(ui.ctx(), &title, medium(16.0), pal().header_primary, f32::INFINITY, 1.0);
                    let (r, resp) = ui.allocate_exact_size(t.size(), if chan.kind == 3 { Sense::click() } else { Sense::hover() });
                    t.paint(ui.painter(), pos2(r.min.x, r.min.y.floor()));
                    if chan.kind == 3 {
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            app.user_dialog = Some(crate::ui::userdlg::Dialog::group(&app.store, chan.id));
                        }
                    }
                    if crate::ui::follow::can_follow(app, chan) {
                        crate::ui::follow::header_button(app, ui, chan);
                    }
                    let topic_fits = |ui: &Ui, topic: &str| {
                        let first: String = topic.chars().take(1).chain(std::iter::once('…')).collect();
                        let need = text::layout(ui.ctx(), &first, reg(14.0), pal().header_secondary, f32::INFINITY).size().x;
                        ui.available_width() - 17.0 >= need
                    };
                    if let Some(topic) = chan.topic.as_ref().filter(|t| !t.is_empty() && topic_fits(ui, t)) {
                        let (sep, _) = ui.allocate_exact_size(vec2(17.0, 24.0), Sense::hover());
                        ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, pal().bg_accent));
                        let w = ui.available_width();
                        ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                            let r = text::label(ui, &topic.replace('\n', " "), reg(14.0), pal().header_secondary, Sense::click());
                            app.rects.insert("header:topic".into(), r.rect);
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                app.topic = Some(chan.id);
                            }
                        });
                    }
                }
                if name_w >= 8.0 {
                    let (x0, x1) = (name.right() - 8.0, name.right());
                    let (clear, bg) = (Color32::TRANSPARENT, pal().bg_primary);
                    let mut fade = egui::Mesh::default();
                    for (x, y, c) in [(x0, panel.top(), clear), (x1, panel.top(), bg), (x1, panel.bottom(), bg), (x0, panel.bottom(), clear)] {
                        fade.colored_vertex(pos2(x, y), c);
                    }
                    fade.add_triangle(0, 1, 2);
                    fade.add_triangle(0, 2, 3);
                    ui.painter().add(fade);
                }
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(8.0);
                for tool in &tools {
                    match tool {
                        Tool::Call => crate::ui::call::call_button(app, ui, chan.id),
                        Tool::Video => crate::ui::call::video_button(app, ui, chan.id),
                        Tool::Threads => {
                            let t = icon_button(ui, Icon::Thread, 32.0, "Threads");
                            app.rects.insert("threads_button".into(), t.rect);
                            if std::mem::take(&mut app.open_threads) {
                                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&t));
                            }
                            crate::ui::popups::threads_menu(app, &t, if chan.is_thread() { chan.parent } else { chan.id });
                        }
                        Tool::Bell => bell(app, ui, chan),
                        Tool::Pins => {
                            let pins = crate::ui::toggle_button(ui, Icon::Pin, 32.0, "Pinned Messages", app.pins.is_some());
                            if app.store.pins_unread(chan.id) {
                                // discord's new-pins dot sits on the icon's lower right, cut out of it
                                let c = pins.rect.center() + vec2(4.5, 4.5);
                                ui.painter().circle_filled(c, 6.0, pal().bg_primary);
                                if pins.hovered() {
                                    ui.painter().circle_filled(c, 6.0, pal().control_hover);
                                }
                                ui.painter().circle_filled(c, 4.0, pal().blurple);
                                app.rects.insert("pins_dot".into(), Rect::from_center_size(c, vec2(8.0, 8.0)));
                            }
                            app.rects.insert("pins_button".into(), pins.rect);
                            if pins.clicked() {
                                match app.pins {
                                    Some(_) => app.pins = None,
                                    None => app.open_pins(chan.id),
                                }
                            }
                        }
                        Tool::AddFriends => {
                            let add = icon_button(ui, Icon::PersonAdd, 32.0, "Add Friends to DM");
                            app.rects.insert("dm:add".into(), add.rect);
                            if add.clicked() {
                                app.group_pick = Some(crate::ui::dmpick::Pick::new(Some(chan.id)));
                            }
                        }
                        Tool::Members => {
                            let tip = if app.ui.hide_members { "Show Member List" } else { "Hide Member List" };
                            if crate::ui::toggle_button(ui, Icon::People, 32.0, tip, !app.ui.hide_members).clicked() {
                                app.ui.hide_members = !app.ui.hide_members;
                                app.search = None;
                            }
                        }
                        Tool::Profile if dm_profile_unavailable(app, ui.ctx(), chan) => {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::UserProfile, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_secondary.gamma_multiply(0.9));
                            app.rects.insert("dm:profile_off".into(), r);
                            resp.on_hover_text("Show User Profile (Unavailable)");
                        }
                        Tool::Profile => {
                            let tip = if app.ui.hide_dm_profile { "Show User Profile" } else { "Hide User Profile" };
                            if crate::ui::toggle_button(ui, Icon::UserProfile, 32.0, tip, !app.ui.hide_dm_profile).clicked() {
                                app.ui.hide_dm_profile = !app.ui.hide_dm_profile;
                                app.search = None;
                            }
                        }
                        Tool::More => crate::ui::thread::more(app, ui, chan, false),
                    }
                    ui.add_space(8.0);
                }
                ui.add_space(4.0);
                if search_w < 2.0 {
                    return;
                }
                let qid = egui::Id::new("search_q");
                let mut q: String = ui.data(|d| d.get_temp(qid).unwrap_or_default());
                // placeholder naming the server
                let hint = match app.store.guilds.get(&chan.guild) {
                    Some(g) => format!("Search {}", g.name.split_whitespace().collect::<Vec<_>>().join(" ")),
                    None => "Search".to_string(),
                };
                let mut clear = false;
                let (slot, _) = ui.allocate_exact_size(vec2(search_w, 32.0), Sense::hover());
                let mut field = ui.new_child(UiBuilder::new().max_rect(slot).layout(Layout::left_to_right(Align::Center)));
                field.set_clip_rect(slot.intersect(ui.clip_rect()));
                let r = {
                    let ui = &mut field;
                    ui.spacing_mut().item_spacing.x = 8.0;
                    Frame::new()
                        .fill(pal().input_bg)
                        .stroke(Stroke::new(1.0, pal().border_subtle))
                        .corner_radius(8)
                        .inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 5 })
                        .show(ui, |ui| {
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut q)
                                    .id(egui::Id::new("search_box"))
                                    .frame(Frame::NONE)
                                    .font(medium(14.0))
                                    .hint_text(RichText::new(hint).font(medium(14.0)).color(pal().text_muted))
                                    .desired_width((search_w - 42.0).max(0.0)),
                            );
                            // the magnifier turns into a clear button once there's a query
                            let (ir, x) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                            if q.is_empty() {
                                icons::draw(ui.painter(), Icon::Search, ir, pal().header_secondary);
                            } else {
                                icons::draw(ui.painter(), Icon::Close, ir, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &x)));
                                clear = x.clicked();
                            }
                            r
                        })
                        .inner
                };
                app.rects.insert("search".into(), r.rect);
                if clear {
                    q.clear();
                    app.search = None;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !q.trim().is_empty() {
                    app.start_search(chan.id, q.trim().to_string());
                }
                if let Some(past) = crate::ui::popups::search_options(app, ui.ctx(), &r, &mut q, chan.guild) {
                    app.start_search(chan.id, past.clone());
                    q = past;
                }
                ui.data_mut(|d| d.insert_temp(qid, q));
            });
        });
}

pub(super) const SEARCH_W: f32 = 244.0;

#[derive(Clone, Copy)]
pub(super) enum Tool {
    Call,
    Video,
    Threads,
    Bell,
    Pins,
    AddFriends,
    Members,
    Profile,
    More,
}

impl Tool {
    pub(super) fn width(self) -> f32 {
        if matches!(self, Tool::Call | Tool::Video) { 28.0 } else { 32.0 }
    }
}

/// The header's buttons left to right, as discord lists them for this kind of channel.
pub(super) fn tools(chan: &Channel) -> Vec<Tool> {
    let mut out = Vec::new();
    if chan.guild == 0 {
        out.extend([Tool::Call, Tool::Video]);
    }
    if chan.guild != 0 && (matches!(chan.kind, 0 | 5) || chan.is_thread()) {
        out.push(Tool::Threads);
    }
    if chan.guild != 0 {
        out.push(Tool::Bell);
    }
    if !chan.is_forum() {
        out.push(Tool::Pins);
    }
    if chan.guild == 0 {
        out.push(Tool::AddFriends);
    }
    if chan.guild != 0 || chan.kind == 3 {
        out.push(Tool::Members);
    }
    if chan.kind == 1 {
        out.push(Tool::Profile);
    }
    if chan.is_thread() {
        out.push(Tool::More);
    }
    out
}

/// The header's notification bell: Discord's menu to mute the channel or pick its level.
pub(crate) fn bell(app: &mut App, ui: &mut Ui, chan: &Channel) {
    use crate::ui::{Trail, menu_item, menu_row, menu_sep};
    let muted = app.store.muted.contains(&chan.id);
    let level = app.store.levels.get(&chan.id).copied();
    let inherited = app.store.inherited_level(chan);
    // discord slashes the bell whenever not every message notifies
    let quiet = app.store.is_muted(chan) || level.unwrap_or(inherited) != 0;
    let b = icon_button(ui, if quiet { Icon::BellOff } else { Icon::Bell }, 32.0, "Notification Settings");
    app.rects.insert("bell".into(), b.rect);
    let mut pick: Option<Option<u8>> = None;
    let mut mute = None;
    // shift+click mutes or unmutes straight away, no menu
    if b.clicked() && ui.input(|i| i.modifiers.shift) {
        egui::Popup::close_all(ui.ctx());
        app.mute_for(chan.id, false, !muted, None);
        return;
    }
    egui::Popup::menu(&b).gap(4.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if muted {
            if menu_item(ui, None, "Unmute Channel", false) {
                mute = Some((false, None));
            }
        } else {
            let m = menu_row(ui, None, "Mute Channel", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                if let Some(secs) = crate::ui::mute_menu(ui) {
                    mute = Some((true, secs));
                }
            });
        }
        menu_sep(ui);
        let default = if chan.parent != 0 && !chan.is_thread() { "Use Category Default" } else { "Use Server Default" };
        pick = crate::ui::level_menu(ui, level, inherited, default);
    });
    if let Some((m, secs)) = mute {
        app.mute_for(chan.id, false, m, secs);
    }
    if let Some(v) = pick {
        app.set_level(chan.id, v);
    }
}
