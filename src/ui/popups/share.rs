use super::*;

pub(crate) struct Forward {
    pub(super) ch: Id,
    pub(super) msg: Id,
    pub(super) query: String,
    pub(super) picked: Vec<Id>,
    pub(super) note: String,
}

impl Forward {
    pub fn new(ch: Id, msg: Id) -> Self {
        Forward { ch, msg, query: String::new(), picked: Vec::new(), note: String::new() }
    }
}

/// Discord's Forward To: up to five places, then an optional note under the forwarded message.
pub(super) fn forward(app: &mut App, ctx: &egui::Context) {
    let Some(f) = &mut app.forward else { return };
    let store = &app.store;
    let ql = f.query.to_lowercase();
    // where you've been lately, then dms, then every channel you can post in
    let mut seen = std::collections::HashSet::new();
    let places: Vec<Id> = app
        .ui
        .recent
        .iter()
        .chain(store.dms.iter())
        .chain(store.guild_order.iter().filter_map(|g| store.guilds.get(g)).flat_map(|g| g.channels.iter()))
        .copied()
        .filter(|c| seen.insert(*c))
        .filter(|c| store.channels.get(c).is_some_and(|c| (c.is_text() && !c.is_thread()) && (c.guild == 0 || store.can_view(c))))
        .filter(|c| ql.is_empty() || channel_title(store, *c).to_lowercase().contains(&ql))
        .take(60)
        .collect();
    let preview = store.find_msg(f.ch, f.msg).cloned();
    let (mut send, mut close) = (false, false);
    // measured from discord's: 478 wide, 24 in, 48 tall rows, the forwarded text quoted at the foot
    let modal = crate::ui::motion::Modal::new("forward".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(478.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 24, bottom: 16 }).show(ui, |ui| {
                ui.set_width(430.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Forward To").font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Select where you want to share this message.").font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(16.0);
                let (field, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
                ui.painter().rect_filled(field, 8, pal().input_bg);
                let inner = Rect::from_min_max(pos2(field.left() + 38.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
                // put: the rest of the field is painted by rect
                let r = ui.put(
                    inner,
                    egui::TextEdit::singleline(&mut f.query)
                        .hint_text(RichText::new("Search").color(pal().text_muted))
                        .font(reg(16.0))
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO),
                );
                r.request_focus();
                ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, if r.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
                crate::ui::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 20.0, field.center().y), vec2(16.0, 16.0)), pal().header_primary);
            });
            let room = (ui.ctx().content_rect().height() - 360.0).clamp(144.0, 400.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 0)).show(ui, |ui| {
                    for c in &places {
                        let Some(ch) = store.channels.get(c) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(430.0, 48.0), Sense::click());
                        let on = f.picked.contains(c);
                        crate::ui::motion::wash(ui, &resp, r.expand2(vec2(8.0, 0.0)), 8, MENU_HOVER, 0.05);
                        let icon = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(32.0, 32.0));
                        match (ch.recipients.first().and_then(|u| store.user(*u)), store.guilds.get(&ch.guild)) {
                            (Some(u), _) if ch.kind == 1 => avatar(ui, icon, &u.avatar_url()),
                            (_, Some(g)) => {
                                // the server's icon with the channel's glyph in a badge at its corner
                                match g.icon_url() {
                                    Some(url) => {
                                        egui::Image::from_uri(url).corner_radius(CornerRadius::same(16)).show_loading_spinner(false).paint_at(ui, icon);
                                    }
                                    None => {
                                        ui.painter().circle_filled(icon.center(), 16.0, pal().bg_accent);
                                        ui.painter().text(icon.center(), Align2::CENTER_CENTER, g.acronym(), semi(11.0), pal().text_normal);
                                    }
                                }
                                let b = pos2(icon.right() - 4.0, icon.bottom() - 4.0);
                                ui.painter().circle_filled(b, 9.0, pal().bg_secondary);
                                crate::ui::side::chan_icon(store, ui.painter(), ch, Rect::from_center_size(b, vec2(12.0, 12.0)), pal().header_secondary);
                            }
                            _ => crate::ui::icons::draw(ui.painter(), Icon::People, icon.shrink(6.0), pal().header_secondary),
                        }
                        let title = channel_title(store, *c);
                        let sub = store.guilds.get(&ch.guild).map(|g| g.name.clone());
                        let y = if sub.is_some() { r.center().y - 9.0 } else { r.center().y };
                        text::paint(ui.painter(), pos2(r.left() + 44.0, y), Align2::LEFT_CENTER, &title, semi(16.0), pal().header_primary, 340.0);
                        if let Some(sub) = sub {
                            text::paint(ui.painter(), pos2(r.left() + 44.0, r.center().y + 10.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().header_secondary, 340.0);
                        }
                        let b = Rect::from_center_size(pos2(r.right() - 10.0, r.center().y), vec2(20.0, 20.0));
                        if on {
                            ui.painter().rect_filled(b, 6, pal().blurple);
                            crate::ui::icons::draw(ui.painter(), Icon::Check, b.shrink(3.0), egui::Color32::WHITE);
                        } else {
                            ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            if on {
                                f.picked.retain(|x| x != c);
                            } else if f.picked.len() < 5 {
                                f.picked.push(*c);
                            }
                        }
                    }
                    if places.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No results found").color(pal().header_secondary)));
                        ui.add_space(24.0);
                    }
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(478.0, 1.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, pal().border_subtle));
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 24 }).show(ui, |ui| {
                ui.set_width(430.0);
                if let Some(m) = &preview {
                    // quoted like discord's: a bar down the left and the message beside it
                    let guild = store.channels.get(&f.ch).map_or(0, |c| c.guild);
                    let body = if m.content.is_empty() { "Attachment".to_string() } else { crate::markdown::plain(&m.content, store, guild) };
                    let (r, _) = ui.allocate_exact_size(vec2(430.0, 24.0), Sense::hover());
                    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(4.0, 24.0)), 2, pal().border_normal);
                    text::paint(ui.painter(), pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &body, reg(16.0), pal().text_normal, 410.0);
                    ui.add_space(16.0);
                }
                {
                    let n = f.picked.len();
                    // one row cut by hand: put() would push a horizontal layout past the field
                    let (row, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
                    let field = Rect::from_min_size(row.min, vec2(320.0, 40.0));
                    ui.painter().rect(field, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let inner = Rect::from_min_max(pos2(field.left() + 16.0, field.center().y - 10.0), pos2(field.right() - 44.0, field.center().y + 10.0));
                    // put: the rest of the row is placed by rect
                    ui.put(
                        inner,
                        egui::TextEdit::singleline(&mut f.note)
                            .hint_text(RichText::new("Add an optional message...").color(pal().text_muted))
                            .font(reg(16.0))
                            .frame(Frame::NONE)
                            .margin(Margin::ZERO),
                    );
                    crate::ui::icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(pos2(field.right() - 22.0, field.center().y), vec2(20.0, 20.0)), pal().header_secondary);
                    let b = Rect::from_min_size(pos2(field.right() + 10.0, row.top()), vec2(100.0, 40.0));
                    let resp = ui.interact(b, ui.id().with("forward_send"), Sense::click());
                    let fill = if n == 0 { pal().blurple.gamma_multiply(0.5) } else { crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)) };
                    ui.painter().rect_filled(b, 8, fill);
                    let label = if n > 1 { format!("Send ({n})") } else { "Send".to_string() };
                    let g = ui.painter().layout_no_wrap(label, medium(16.0), egui::Color32::WHITE);
                    let x = b.center().x - (g.size().x + 26.0) / 2.0;
                    let w = g.size().x;
                    ui.painter().galley(pos2(x, b.center().y - g.size().y / 2.0), g, egui::Color32::WHITE);
                    crate::ui::icons::draw(ui.painter(), Icon::Send, Rect::from_center_size(pos2(x + w + 16.0, b.center().y), vec2(20.0, 20.0)), egui::Color32::WHITE);
                    send = resp.clicked() && n > 0;
                }
            });
        });
    send |= !f.picked.is_empty() && crate::ui::enter_submits(ctx, true);
    if send {
        let f = app.forward.take().expect("checked above");
        let guild = app.store.channels.get(&f.ch).map_or(0, |c| c.guild);
        for to in f.picked {
            let mut reference = json!({ "type": 1, "message_id": f.msg.to_string(), "channel_id": f.ch.to_string() });
            if guild != 0 {
                reference["guild_id"] = json!(guild.to_string());
            }
            app.rest("POST", format!("/channels/{to}/messages"), Some(json!({ "content": "", "message_reference": reference })), Tag::fail(crate::ui::SEND_FAILED));
            if !f.note.trim().is_empty() {
                app.rest("POST", format!("/channels/{to}/messages"), Some(json!({ "content": f.note.trim() })), Tag::fail(crate::ui::SEND_FAILED));
            }
        }
        app.toast("Message forwarded.");
    } else if close || modal.should_close() {
        app.forward = None;
    }
}

/// Create Thread, from a message's menu or the threads popout: the message it starts from,
/// a name (the message's own words if left empty) and the first thing to say in it.
pub(crate) struct NewThread {
    pub(super) ch: Id,
    pub(super) msg: Option<Id>,
    pub(super) name: String,
    pub(super) first: String,
    pub(super) hint: String,
    pub(super) private: bool,
}

impl NewThread {
    pub fn from(app: &App, ch: Id, msg: Id) -> Self {
        let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
        let words = app.store.find_msg(ch, msg).map(|m| crate::markdown::plain(&m.content, &app.store, guild)).unwrap_or_default();
        let hint: String = words.chars().take(40).collect();
        NewThread { ch, msg: Some(msg), name: String::new(), first: String::new(), hint: if hint.trim().is_empty() { "New Thread".into() } else { hint }, private: false }
    }

    pub fn blank(ch: Id) -> Self {
        NewThread { ch, msg: None, name: String::new(), first: String::new(), hint: "New Thread".into(), private: false }
    }
}

pub(super) fn new_thread(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &mut app.new_thread else { return };
    let msg = t.msg.and_then(|m| app.store.find_msg(t.ch, m).cloned());
    let can_private = t.msg.is_none() && app.store.channels.get(&t.ch).is_some_and(|c| c.kind == 0 && app.store.perms(c) & (1 << 36) != 0);
    let (mut make, mut close) = (false, false);
    let modal = crate::ui::motion::Modal::new("new_thread".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                crate::ui::icons::draw(ui.painter(), Icon::ThreadPlus, r, pal().header_secondary);
                ui.add_space(8.0);
                ui.label(RichText::new("New Thread").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            if let Some(m) = &msg {
                ui.add_space(16.0);
                chat::preview(ui, &app.store, m);
            }
            ui.add_space(20.0);
            ui.label(RichText::new("Thread Name").font(semi(12.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let field = |ui: &mut Ui, text: &mut String, hint: &str, focus: bool| {
                let (r, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect_filled(r, 8, pal().input_bg);
                let e = ui.put(
                    r.shrink2(vec2(12.0, 10.0)),
                    egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO),
                );
                ui.painter().rect_stroke(r, 8, egui::Stroke::new(1.0, if e.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
                ui.advance_cursor_after_rect(r);
                if focus {
                    e.request_focus();
                }
                e
            };
            let first_frame = !ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("new_thread_seen")).unwrap_or(false));
            let n = field(ui, &mut t.name, &t.hint, first_frame);
            if can_private {
                ui.add_space(16.0);
                ui.label(RichText::new("Private Thread").font(semi(12.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                let (r, resp) = ui.allocate_exact_size(vec2(432.0, 24.0), Sense::click());
                crate::ui::settings::mark(ui, "new_thread:private", r);
                let b = Rect::from_min_size(r.min, vec2(24.0, 24.0));
                if t.private {
                    ui.painter().rect_filled(b, 6, pal().blurple);
                    crate::ui::icons::draw(ui.painter(), Icon::Check, b.shrink(4.0), egui::Color32::WHITE);
                } else {
                    ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                }
                ui.painter().text(pos2(b.right() + 12.0, r.center().y), Align2::LEFT_CENTER, "Only people you invite and moderators can see", reg(14.0), pal().text_normal);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    t.private = !t.private;
                }
                ui.add_space(16.0);
            } else {
                ui.add_space(16.0);
            }
            let f = field(ui, &mut t.first, "Enter a message to start the conversation!", false);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("new_thread_seen"), true));
            let enter = (n.lost_focus() || f.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let create = button(ui, "Create Thread", pal().blurple, 128.0);
                crate::ui::settings::mark(ui, "new_thread:create", create.rect);
                make = create.clicked() || enter;
                ui.add_space(8.0);
                close |= crate::ui::grey_button(ui, "Cancel", 92.0).clicked();
            });
        });
    let ch = t.ch;
    if make {
        let name = if t.name.trim().is_empty() { t.hint.clone() } else { t.name.trim().to_string() };
        let first = std::mem::take(&mut t.first);
        let (path, body) = match t.msg {
            Some(m) => (format!("/channels/{ch}/messages/{m}/threads"), json!({ "name": name, "auto_archive_duration": 4320 })),
            None if t.private => (format!("/channels/{ch}/threads"), json!({ "name": name, "type": 12, "invitable": false, "auto_archive_duration": 4320 })),
            None => (format!("/channels/{ch}/threads"), json!({ "name": name, "type": 11, "auto_archive_duration": 4320 })),
        };
        app.rest("POST", path, Some(body), Tag::Thread(first));
        close = true;
    }
    if close || modal.should_close() {
        app.new_thread = None;
        ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("new_thread_seen")));
    }
}
