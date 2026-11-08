use super::*;

/// A rule across the chat at `y`, takes no room: the date in the middle if there is one, and
/// the red NEW tab on the end for the first unread message.
pub(super) fn divider(ui: &Ui, y: f32, text: &str, unread: bool) {
    let r = ui.max_rect();
    let left = r.left() + 16.0;
    let right = r.right() - 14.0;
    let y = y + 0.5;
    let color = if unread { pal().red } else { pal().bg_accent };
    if text.is_empty() {
        ui.painter().hline(left..=right, y, Stroke::new(1.0, color));
    } else {
        let g = ui.painter().layout_no_wrap(text.to_string(), semi(12.0), Color32::WHITE);
        let gw = g.size().x + 8.0;
        let mid = r.center().x;
        ui.painter().hline(left..=mid - gw / 2.0, y, Stroke::new(1.0, color));
        ui.painter().hline(mid + gw / 2.0..=right, y, Stroke::new(1.0, color));
        let ink = if unread { pal().red } else { pal().text_muted };
        ui.painter().galley(pos2(mid - g.size().x / 2.0, y - g.size().y / 2.0), g, ink);
    }
    if unread {
        let g = ui.painter().layout_no_wrap("NEW".into(), bold(10.0), Color32::WHITE);
        let pr = Rect::from_min_size(pos2(right - g.size().x - 5.0, y - 7.0), vec2(g.size().x + 5.0, 13.0));
        ui.painter().rect_filled(pr, CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, pal().red);
        // the tab's pointed end
        let tip = vec![pos2(pr.left() - 7.0, pr.center().y), pos2(pr.left() + 0.5, pr.top()), pos2(pr.left() + 0.5, pr.bottom())];
        ui.painter().add(egui::Shape::convex_polygon(tip, pal().red, Stroke::NONE));
        ui.painter().galley(pos2(pr.left() + 1.0, pr.center().y - g.size().y / 2.0), g, Color32::WHITE);
    }
}

pub(super) fn message(ui: &mut Ui, d: &mut D, m: &Message, prev: Option<&Message>, start: bool) {
    d.image_menu = false;
    if m.kind == 21 && let Some(r) = m.reply.and_then(|r| d.store.refs.get(&r)) {
        return message(ui, d, r, None, true);
    }
    let sys = if normal(m.kind) {
        None
    } else {
        let Some(sys) = system::describe(d.store, d.guild, m) else { return };
        Some(sys)
    };
    let compact = d.s.compact;
    let font = d.s.font_size;
    // discord's group spacing N is N+1 above a group
    let gs = d.s.group_space;
    let mut gap = gs + 1.0;
    let unread = d.marker.is_some_and(|after| prev.is_some_and(|p| p.id <= after) && m.id > after);
    if let Some(p) = prev
        && day(p.ts()) != day(m.ts())
    {
        // the date sits 24 under the last message, then 24-N or the group's gap, whichever's more
        ui.add_space(24.0);
        let label = markdown::local(m.ts()).format("%B %-d, %Y").to_string();
        divider(ui, ui.cursor().top(), &label, unread);
        ui.add_space(1.0);
        gap = gap.max(24.0 - gs);
    } else if unread && start {
        // over the gap above a group, 8 under the last message
        divider(ui, ui.cursor().top() + 8.0, "", true);
    } else if unread {
        ui.add_space(4.0);
        divider(ui, ui.cursor().top(), "", true);
        ui.add_space(5.0);
    }
    if start {
        ui.add_space(gap);
    }

    let left = ui.max_rect().left();
    let w = ui.available_width();
    let top = ui.cursor().top();
    let bg = ui.painter().add(egui::Shape::Noop);
    let content = Rect::from_min_size(pos2(left + 72.0, top + 2.0), vec2((w - 72.0 - 48.0).max(120.0), 100_000.0));
    let mut cui = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    cui.spacing_mut().item_spacing = vec2(4.0, 0.0);

    let me = d.store.me;
    // last frame's row decides whether the pointer is over this message yet
    let row_id = egui::Id::new(("row", m.id));
    let hot = ui.data(|s| s.get_temp::<Rect>(row_id)).is_some_and(|r| ui.rect_contains_pointer(r));
    let mut head_top = top + 2.0;
    let mut tail = 0.0;
    if let Some(sys) = sys {
        system_line(&mut cui, d, m, sys);
    } else {
        if let Some(r) = m.reply {
            reply_line(&mut cui, d, r, left);
            head_top = cui.cursor().top();
        } else if let Some(i) = m.interaction.as_ref().filter(|i| !i.name.is_empty()) {
            interaction::executed(&mut cui, d, i, left);
            head_top = cui.cursor().top();
        }
        let name = d.store.name_in(d.guild, m.author);
        let (ink, dot) = name_ink(d, m.author);
        let color = ink.stops[0];
        if start && !compact {
            author_line(&mut cui, d, m, &name, &ink, dot, font, hot);
        }
        let fwd = m.forward.map(|_| forward::begin(&mut cui));
        let editing = d.editing.as_ref().is_some_and(|(id, _)| *id == m.id);
        if editing {
            edit_box(&mut cui, d, m.id);
        } else if !m.content.is_empty() && !(d.embeds && d.s.inline_links && link_only(m)) {
            let color = if m.failed {
                pal().red
            } else {
                crate::ui::motion::mix(pal().text_muted, pal().text_normal, crate::ui::motion::tween(d.ctx, egui::Id::new(("sent", m.id)), !m.pending, 0.2, crate::ui::motion::EASE))
            };
            let revealed = d.revealed.contains(&m.id)
                || d.s.spoilers == crate::cache::Spoilers::Always
                || (d.s.spoilers == crate::cache::Spoilers::Moderated && d.moderator);
            // with no text the marker goes under the embeds and buttons instead
            let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: font, color, revealed, edited: m.edited && !m.content.trim().is_empty(), embed: false };
            let mut blocks = markdown::parse(&m.content, &cx);
            if compact {
                compact_prefix(d, m, &mut blocks, &name, color, font);
                if !matches!(blocks.first(), Some(Block::Text(_))) {
                    compact_name_line(&mut cui, d, m, &name, color, font);
                }
            }
            blocks_ui(&mut cui, &blocks, d, m.id);
        } else if compact {
            compact_name_line(&mut cui, d, m, &name, color, font);
        }
        if let Some(s) = interaction::status(d, m) {
            interaction::status_line(&mut cui, d, font, &s);
        }
        // discord's accessories: 2 under the text, 4 apart, 2 under the last
        let mut acc = 2.0;
        // a components-v2 message shows its files through its own gallery, file and thumbnail nodes
        let files: &[crate::state::Attachment] = if m.flags & (1 << 15) != 0 { &[] } else { &m.attachments };
        let tiled: Vec<&crate::state::Attachment> = files.iter().filter(|a| (a.is_image() && d.s.inline_uploads) || a.is_video()).collect();
        for a in files {
            if tiled.len() > 1 && tiled.iter().any(|t| std::ptr::eq(*t, a)) {
                if std::ptr::eq(tiled[0], a) {
                    cui.add_space(acc);
                    acc = 4.0;
                    mosaic::show(&mut cui, d, m.id, &tiled);
                }
                continue;
            }
            cui.add_space(acc);
            acc = 4.0;
            if a.is_image() && d.s.inline_uploads {
                let sep = if a.proxy.contains('?') { '&' } else { '?' };
                let (w, h) = fit(a.w, a.h, 550.0, 350.0, cui.available_width());
                let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                let r = media(&mut cui, d, &url, &a.url, w, h, None);
                if d.s.image_descriptions && !a.description.is_empty() {
                    alt_badge(&cui, &r, &a.description);
                }
                if cui.clip_rect().contains(r.rect.center()) {
                    d.rects.push(("image".into(), r.rect.intersect(cui.clip_rect())));
                }
                if r.clicked() {
                    d.acts.push(Act::View(a.proxy.clone(), a.url.clone(), a.w, a.h));
                }
            } else if a.is_video() {
                let (w, h) = fit(a.w, a.h, 550.0, 350.0, cui.available_width());
                let sep = if a.proxy.contains('?') { '&' } else { '?' };
                let poster = format!("{}{sep}format=webp&width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                let mut outs = Vec::new();
                let r = crate::ui::player::video(&mut cui, a, vec2(w, h), &poster, &mut outs);
                d.card("player_video", &cui, r);
                d.player(outs);
            } else if a.is_audio() && (m.flags & (1 << 13) != 0 || !a.waveform.is_empty()) {
                audio_card(&mut cui, d, a);
            } else if a.is_audio() {
                let mut outs = Vec::new();
                let r = crate::ui::player::audio(&mut cui, a, &mut outs);
                d.card("player_audio", &cui, r);
                d.player(outs);
            } else if a.is_text() {
                let mut outs = Vec::new();
                let r = crate::ui::textfile::card(&mut cui, a, &mut outs);
                d.card("player_text", &cui, r);
                d.player(outs);
            } else if file_card(&mut cui, &a.filename, a.size).clicked() {
                d.acts.push(Act::Url(a.url.clone()));
            }
        }
        if m.pending
            && let Some(job) = m.nonce.as_deref().and_then(crate::upload::get)
        {
            cui.add_space(acc);
            acc = 4.0;
            let c = crate::ui::uploading::card(&mut cui, &job);
            d.card("upload_card", &cui, c.rect);
            d.card("upload_bar", &cui, c.bar);
            d.card("upload_cancel", &cui, c.x);
            if c.cancel {
                d.acts.push(Act::CancelUpload(m.nonce.clone().unwrap_or_default()));
            }
        }
        for (id, name, fmt) in &m.stickers {
            cui.add_space(acc);
            acc = 4.0;
            let url = crate::api::sticker(*id, *fmt, 160);
            let r = media(&mut cui, d, &url, &url, 160.0, 160.0, None).on_hover_ui(|ui| {
                ui.label(RichText::new(name).font(semi(14.0)).color(pal().header_primary));
                ui.label(RichText::new("Click to learn more").font(reg(12.0)).color(pal().header_secondary));
            });
            d.card("sticker", &cui, r.rect);
            d.card(&format!("sticker:{id}"), &cui, r.rect);
            if r.clicked() {
                d.acts.push(Act::Sticker(*id, name.clone(), *fmt, r.rect.right_top() + vec2(8.0, 0.0)));
            }
        }
        for code in crate::ui::chatinvite::codes(&m.content) {
            cui.add_space(acc);
            acc = 4.0;
            if let Some(go) = crate::ui::chatinvite::card(&mut cui, d.store, d.invites, &code, m.author == me) {
                d.acts.push(Act::Invite(go));
            }
        }
        let embeds = if d.embeds { m.embeds.as_slice() } else { &[] };
        for (i, e) in embeds.iter().enumerate() {
            cui.add_space(acc);
            acc = 4.0;
            let r = embed(&mut cui, d, e, m.id, i);
            // discord's ✕ beside the first of your own embeds, to take them all off for everyone
            if let Some(r) = r.filter(|_| i == 0 && hot && m.author == me && !d.popout) {
                let x = Rect::from_min_size(pos2(r.right() + 4.0, r.top()), vec2(16.0, 16.0));
                let resp = cui.interact(x, cui.id().with(("clear_embeds", m.id)), Sense::click());
                icons::draw(cui.painter(), Icon::Close, x, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                d.rects.push(("clear_embeds".into(), x));
                if resp.on_hover_text("Remove all embeds").clicked() {
                    d.acts.push(Act::ClearEmbeds(m.id));
                }
            }
        }
        if m.poll.is_some() {
            cui.add_space(acc);
            acc = 4.0;
            poll(&mut cui, d, m);
        }
        if let Some(t) = m.thread.and_then(|t| d.store.channels.get(&t)) {
            cui.add_space(acc);
            acc = 4.0;
            let line = thread_line(d.store, t, d.guild);
            let r = Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(4)
                .inner_margin(8)
                .show(&mut cui, |ui| {
                    ui.set_max_width(ui.available_width().min(400.0));
                    ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
                    ui.horizontal(|ui| {
                        ui.add(Label::new(RichText::new(&t.name).font(semi(14.0)).color(pal().header_primary)).selectable(false));
                        ui.add(Label::new(RichText::new(format!("{} Messages ›", t.msg_count)).font(semi(14.0)).color(pal().link)).selectable(false));
                    });
                    ui.add(Label::new(RichText::new(line).font(reg(14.0)).color(pal().text_normal)).truncate().selectable(false));
                })
                .response
                .interact(Sense::click());
            crate::ui::settings::mark(&cui, &format!("thread_card:{}", t.id), r.rect);
            if r.hovered() {
                cui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                d.acts.push(Act::Channel(t.id));
            }
        }
        if !m.components.is_empty() {
            cui.add_space(acc);
            acc = 4.0;
            components::components(&mut cui, d, m);
        }
        if let Some(open) = fwd {
            forward::end(&mut cui, d, m, open);
        }
        if m.edited && m.content.trim().is_empty() {
            cui.add_space(2.0);
            cui.label(RichText::new("(edited)").font(reg(10.0)).color(pal().text_muted));
        }
        if !m.reactions.is_empty() && d.s.reactions {
            cui.add_space(acc);
            acc = 4.0;
            reactions(&mut cui, d, m);
        }
        if acc > 2.0 {
            tail = 2.0;
        }
    }

    let bottom = cui.min_rect().bottom().max(top + 22.0) + 2.0 + tail;
    let row = Rect::from_min_max(pos2(left, top), pos2(left + w, bottom));
    ui.advance_cursor_after_rect(row);
    ui.data_mut(|s| s.insert_temp(row_id, row));
    // on screen top to bottom; a panel dragged to a fractional width can leave the row a hair
    // wider than the clip
    let clip = ui.clip_rect();
    if row.top() >= clip.top() && row.bottom() <= clip.bottom() && normal(m.kind) && !m.pending {
        d.rects.push(("msg:last".into(), row));
        if m.author != me {
            d.rects.push(("msg:other".into(), row));
        }
    }

    let hovered = match d.hover_lock {
        Some(id) => id == m.id,
        None => ui.rect_contains_pointer(row),
    };
    let fill = if d.store.mentions_me(m) && m.author != me {
        ui.painter().rect_filled(Rect::from_min_size(row.min, vec2(2.0, row.height())), CornerRadius::ZERO, pal().mention_bar);
        // discord's hover over a mention is the lighter of the two
        if hovered { pal().highlight_hover } else { pal().highlight_bg }
    } else if let Some((_, t)) = d.flash.filter(|(id, _)| *id == m.id) {
        // jumped-to message glows and fades out over two seconds
        let a = (1.0 - t.elapsed().as_secs_f32() / 2.0).clamp(0.0, 1.0);
        wash(pal().blurple, (40.0 * a) as u8)
    } else if hovered || d.editing.as_ref().is_some_and(|(id, _)| *id == m.id) {
        pal().bg_msg_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().set(bg, egui::epaint::RectShape::filled(row, CornerRadius::ZERO, fill));
    if d.focus == Some(m.id) {
        ui.painter().rect_stroke(row.shrink(1.0), CornerRadius::same(4), Stroke::new(2.0, pal().focus), egui::StrokeKind::Inside);
        d.rects.push(("msg:focus".into(), row));
    }

    if compact && normal(m.kind) {
        // compact mode keeps the time in the gutter on every line
        let t = markdown::local(m.ts()).format(markdown::hm()).to_string();
        ui.painter().text(pos2(left + 64.0, head_top + font * 0.69), Align2::RIGHT_CENTER, t, medium((11.0 * font / 16.0).max(10.0)), pal().text_muted);
    } else if start && normal(m.kind) {
        if let Some(u) = d.store.user(m.author) {
            let ar = Rect::from_min_size(pos2(left + 16.0, head_top + 2.0), vec2(40.0, 40.0));
            avatar(ui, ar, &u.avatar_url());
            if ui.clip_rect().contains_rect(ar) {
                d.rects.push((format!("avatar:{}", m.author), ar));
            }
            let r = ui.interact(ar, egui::Id::new(("avatar", m.id)), Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, ar.right_top() + vec2(8.0, 0.0)) });
            }
            if r.secondary_clicked() {
                d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(ar.center())));
            }
        }
    } else if hovered && normal(m.kind) {
        let t = markdown::local(m.ts()).format(markdown::hm()).to_string();
        ui.painter().text(pos2(left + 56.0, top + 13.0), Align2::RIGHT_CENTER, t, medium(11.0), pal().text_muted);
    }

    if hovered && !m.pending {
        if m.failed {
            failed_bar(d, m, row);
        } else {
            // alt+click marks unread from this message on, a long-standing discord shortcut
            if ui.input(|i| i.pointer.primary_clicked() && i.modifiers.alt) {
                d.acts.push(Act::MarkUnread(m.id));
            }
            action_bar(d, m, row);
        }
        if ui.input(|i| i.pointer.secondary_clicked())
            && !d.acts.iter().any(|a| matches!(a, Act::UserMenu(..)))
            && !std::mem::take(&mut d.image_menu)
            && let Some(p) = ui.ctx().pointer_interact_pos()
        {
            d.acts.push(Act::Menu(m.id, p));
        }
    }
}
