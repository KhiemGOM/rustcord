use super::*;

pub(super) fn small_button(ui: &mut Ui, text: &str) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), medium(14.0), pal().header_primary);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 32.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(super) fn system_line(ui: &mut Ui, d: &mut D, m: &Message, sys: system::Sys) {
    let left = ui.max_rect().left();
    let y = ui.cursor().top() + 11.0;
    icons::draw(ui.painter(), sys.icon, Rect::from_center_size(pos2(left - 36.0, y), vec2(16.0, 16.0)), sys.tint);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in &sys.spans {
            let (text, color, go) = match s {
                system::Span::Text(t) => (t, pal().channel_default, None),
                system::Span::Bold(t) => (t, pal().header_primary, None),
                system::Span::Name(t, id) => (t, pal().header_primary, Some(system::Go::Profile(*id))),
                system::Span::Link(t, go) => (t, pal().link, Some(go.clone())),
            };
            let font = if matches!(s, system::Span::Text(_)) { reg(16.0) } else { medium(16.0) };
            let r = ui.add(Label::new(RichText::new(text).font(font).color(color)).selectable(false).sense(if go.is_some() { Sense::click() } else { Sense::hover() }));
            if let Some(go) = go {
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.clicked() {
                    d.acts.push(Act::System(go, r.rect.left_bottom()));
                }
            }
        }
        if let Some((label, go)) = &sys.button {
            ui.add_space(8.0);
            let b = small_button(ui, label);
            if matches!(go, system::Go::JoinCall(_)) {
                d.rects.push((format!("call_msg_join:{}", d.ch), b.rect));
            }
            if b.clicked() {
                d.acts.push(Act::System(go.clone(), Pos2::ZERO));
            }
        }
        ui.add_space(4.0);
        ui.add(Label::new(RichText::new(stamp(m.ts())).font(medium(12.0)).color(pal().chat_muted)).selectable(false));
    });
    match sys.card {
        system::Card::None => {}
        system::Card::Poll { winner, poll } => {
            ui.add_space(4.0);
            Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(432.0));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&winner.0).font(semi(16.0)).color(pal().header_primary));
                            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                            icons::draw(ui.painter(), Icon::CircleCheck, r, pal().green);
                        });
                        ui.label(RichText::new(format!("Winning answer \u{2022} {}%", winner.1)).font(reg(12.0)).color(pal().header_secondary));
                    });
                    if let Some((c, id)) = poll {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_button(ui, "View Poll").clicked() {
                                d.acts.push(Act::System(system::Go::Message(c, id), Pos2::ZERO));
                            }
                        });
                    }
                });
            });
        }
        system::Card::AutoMod { content, keyword, rule } => {
            ui.add_space(4.0);
            Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(520.0));
                if !content.is_empty() {
                    ui.label(RichText::new(content).font(reg(14.0)).color(pal().text_normal));
                }
                for (k, v) in [("Keyword", keyword), ("Rule", rule)] {
                    if let Some(v) = v {
                        ui.label(RichText::new(format!("{k}: {v}")).font(reg(12.0)).color(pal().header_secondary));
                    }
                }
            });
        }
    }
}

/// The line from a reply's (or a command's) header down to the message under it.
pub(super) fn spine(p: &egui::Painter, row_left: f32, r: Rect) {
    let stroke = Stroke::new(2.0, pal().spine);
    let x = row_left + 36.0;
    let sy = r.center().y + 1.0;
    p.line_segment([pos2(x, r.bottom()), pos2(x, sy + 6.0)], stroke);
    p.add(egui::epaint::CubicBezierShape::from_points_stroke([pos2(x, sy + 6.0), pos2(x, sy), pos2(x, sy), pos2(x + 6.0, sy)], false, Color32::TRANSPARENT, stroke));
    p.line_segment([pos2(x + 6.0, sy), pos2(r.left() - 4.0, sy)], stroke);
}

pub(super) fn reply_line(ui: &mut Ui, d: &mut D, rid: Id, row_left: f32) {
    // discord's reply is 18 tall with 4 under it; the spine's 2px border starts at its middle and
    // runs down to its bottom
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
    ui.add_space(4.0);
    spine(ui.painter(), row_left, r);
    let y = r.center().y;
    let p = ui.painter();
    let Some(rm) = d.store.find_msg(d.ch, rid) else {
        // discord: a small reply arrow, then the notice in italics
        icons::draw(p, Icon::Reply, Rect::from_center_size(pos2(r.left() + 6.0, y), vec2(12.0, 12.0)), pal().text_muted);
        let mut job = egui::text::LayoutJob::default();
        let lost = if d.popout { "Message could not be loaded" } else { "Original message was deleted" };
        job.append(lost, 0.0, egui::TextFormat { font_id: italic(14.0), color: pal().text_muted, ..Default::default() });
        let g = ui.painter().layout_job(job);
        p.galley(pos2(r.left() + 16.0, y - g.size().y / 2.0), g, pal().text_muted);
        return;
    };
    let mut cx = r.left();
    if let Some(u) = d.store.user(rm.author) {
        avatar(ui, Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0)), &u.avatar_url());
        cx += 20.0;
    }
    let name = format!("@{}", d.store.name_in(d.guild, rm.author));
    let (ink, _) = name_ink(d, rm.author);
    let rich = ink.layout(ui.ctx(), &name, medium(14.0), 200.0, false);
    let nr = Align2::LEFT_CENTER.anchor_size(pos2(cx, y), rich.size());
    ink.paint(p, &rich, nr.min, false);
    cx = nr.right() + 4.0;
    if let Some(r) = d.store.user(rm.author).filter(|u| !u.bot).and_then(|u| crate::ui::tag_chip(p, u, pos2(cx, y), 1.0)) {
        cx = r.right() + 4.0;
    }
    let excerpt = markdown::plain(&rm.content, d.store, d.guild);
    if excerpt.is_empty() {
        let ir = Rect::from_center_size(pos2(cx + 8.0, y), vec2(16.0, 16.0));
        let (icon, note) = if rm.interaction.is_some() {
            (Icon::Apps, "Click to see command")
        } else if !rm.stickers.is_empty() {
            (Icon::Sticker, "Click to see sticker")
        } else if rm.flags & (1 << 13) != 0 {
            (Icon::Mic, "Click to see voice message")
        } else {
            (Icon::Image, "Click to see attachment")
        };
        icons::draw(p, icon, ir, pal().header_secondary);
        cx += 20.0;
        p.text(pos2(cx, y), Align2::LEFT_CENTER, note, reg(14.0), pal().header_secondary);
    } else {
        let t = text::paint(p, pos2(cx, y), Align2::LEFT_CENTER, &excerpt, reg(14.0), pal().text_normal, r.right() - cx - 50.0);
        cx = t.right() + 4.0;
    }
    if rm.edited {
        p.text(pos2(cx, y + 1.0), Align2::LEFT_CENTER, "(edited)", reg(10.0), pal().text_muted);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.clip_rect().contains_rect(r) {
        d.rects.push(("reply".into(), r));
    }
    if resp.clicked() {
        d.acts.push(Act::Jump(rid));
    }
}

/// The stretch of each row that a run of characters covers: the row's rect, then where the run
/// starts and ends on it.
fn spans(galley: &egui::Galley, r: &std::ops::Range<usize>) -> Vec<(Rect, f32, f32)> {
    let a = galley.pos_from_cursor(CCursor { prefer_next_row: true, ..CCursor::new(r.start) });
    let b = galley.pos_from_cursor(CCursor::new(r.end));
    galley
        .rows
        .iter()
        .filter(|row| row.min_y() >= a.min.y && row.min_y() <= b.min.y)
        .map(|row| {
            let all = row.rect();
            (all, if row.min_y() == a.min.y { a.min.x } else { all.min.x }, if row.min_y() == b.min.y { b.min.x } else { all.max.x })
        })
        .collect()
}

/// A list item's marker, outside the item as chromium places it: a number ends where the item
/// starts, with its dot and a space, and a bullet is a dot two thirds of the ascent wide that
/// starts that far plus 7 before the item.
fn bullet(ui: &Ui, marker: Marker, at: Pos2, size: f32, line: f32) {
    let font = reg(size);
    if let Marker::Number(n) = marker {
        let space = ui.fonts_mut(|f| f.glyph_width(&font, ' '));
        ui.painter().text(at + vec2(-space, line / 2.0), Align2::RIGHT_CENTER, format!("{n}."), font, pal().text_normal);
        return;
    }
    // gg sans: ascent 1.018em, descent 0.316em
    let (ascent, descent) = ((1.018 * size).round(), (0.316 * size).round());
    let offset = (ascent * 2.0 / 3.0).floor();
    let wide = ((offset + 1.0) / 2.0).floor();
    let dot = Rect::from_min_size(at + vec2(-offset - 7.0, (line - ascent - descent) / 2.0 + (3.0 * (ascent - offset) / 2.0).floor()), vec2(wide, wide));
    if marker == Marker::Disc {
        ui.painter().circle_filled(dot.center(), wide / 2.0, pal().text_normal);
    } else {
        ui.painter().circle_stroke(dot.center(), wide / 2.0 - 0.5, Stroke::new(1.0, pal().text_normal));
    }
}

/// A message's text. Discord keeps its code blocks and quotes to 90% of the text's width.
pub(super) fn blocks_ui(ui: &mut Ui, blocks: &[Block], d: &mut D, msg: Id) {
    blocks_in(ui, blocks, d, msg, 0.9);
}

/// An embed's text, and a quote's: there they may be as wide as the text.
pub(super) fn blocks_wide(ui: &mut Ui, blocks: &[Block], d: &mut D, msg: Id) {
    blocks_in(ui, blocks, d, msg, 1.0);
}

fn blocks_in(ui: &mut Ui, blocks: &[Block], d: &mut D, msg: Id, cap: f32) {
    for b in blocks {
        match b {
            Block::Text(t) => {
                ui.add_space(t.lead);
                let mut job = t.job.clone();
                job.wrap.max_width = ui.available_width();
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                let under = ui.painter().add(egui::Shape::Noop);
                let resp = ui.add(Label::new(galley.clone()).sense(Sense::click()));
                let origin = resp.rect.min;
                let mut fills = Vec::new();
                let mut lit = Vec::new();
                let pointer = ui.ctx().pointer_hover_pos().filter(|_| ui.rect_contains_pointer(resp.rect));
                for (r, fill, line, h) in &t.pills {
                    let a = galley.pos_from_cursor(CCursor { prefer_next_row: true, ..CCursor::new(r.start) });
                    let b = galley.pos_from_cursor(CCursor::new(r.end));
                    let piece = |l: f32, r: f32, row: Rect| {
                        let top = origin.y + row.center().y - line / 2.0;
                        Rect::from_min_max(pos2(origin.x + l, top), pos2(origin.x + r, top + h))
                    };
                    let mut pieces = Vec::new();
                    if a.min.y == b.min.y {
                        pieces.push(piece(a.min.x - 2.0, b.min.x + 2.0, a));
                    } else if let (Some(ra), Some(rb)) = (
                        galley.rows.iter().find(|row| row.min_y() == a.min.y).map(|row| row.rect()),
                        galley.rows.iter().find(|row| row.min_y() == b.min.y).map(|row| row.rect()),
                    ) {
                        // wrapped onto the next row: padded only at its two ends
                        pieces.push(piece(a.min.x - 2.0, ra.max.x, a));
                        pieces.push(piece(rb.min.x, b.min.x + 2.0, b));
                    }
                    let hot = *fill == pal().mention_bg && pointer.is_some_and(|p| pieces.iter().any(|q| q.contains(p)));
                    for q in pieces {
                        fills.push(egui::Shape::rect_filled(q, CornerRadius::same(3), if hot { pal().blurple } else { *fill }));
                        if hot {
                            lit.push(q);
                        }
                    }
                }
                let mut spoilers = 0;
                for (r, kind, line, h) in &t.boxes {
                    let (grow, top) = match kind {
                        Boxed::Code(pad) => (*pad, (line - h) / 2.0),
                        Boxed::Spoiler(_) => (0.0, 0.0),
                    };
                    let rows = spans(&galley, r);
                    let last = rows.len().saturating_sub(1);
                    let pieces: Vec<Rect> = rows
                        .into_iter()
                        .enumerate()
                        .map(|(i, (row, left, right))| {
                            let left = if i == 0 { left - grow } else { left };
                            let right = if i == last { right + grow } else { right };
                            let top = origin.y + row.center().y - line / 2.0 + top;
                            Rect::from_min_max(pos2(origin.x + left, top), pos2(origin.x + right, top + h))
                        })
                        .filter(|q| q.width() > grow)
                        .collect();
                    // for the e2e driver: the last one drawn of each kind
                    let name = match kind {
                        Boxed::Code(_) => "code".to_string(),
                        Boxed::Spoiler(key) => {
                            spoilers += 1;
                            format!("{}{spoilers}", if key.is_some() { "covered" } else { "open" })
                        }
                    };
                    if let Some(q) = pieces.first() {
                        crate::ui::settings::mark(ui, &format!("box:{name}"), *q);
                    }
                    let (fill, edge) = match kind {
                        Boxed::Code(_) => (pal().bg_code, Stroke::new(1.0, pal().border_normal)),
                        Boxed::Spoiler(None) => (pal().spoiler_revealed, Stroke::NONE),
                        Boxed::Spoiler(Some(key)) => {
                            let over = pointer.is_some_and(|p| pieces.iter().any(|q| q.contains(p)));
                            let t = crate::ui::motion::tween(ui.ctx(), egui::Id::new(("spoiler", key)), over, 0.2, crate::ui::motion::EASE);
                            (crate::ui::motion::mix(pal().spoiler, pal().spoiler_hover, t), Stroke::NONE)
                        }
                    };
                    fills.extend(pieces.into_iter().map(|q| egui::epaint::RectShape::new(q, CornerRadius::same(4), fill, edge, egui::StrokeKind::Inside).into()));
                }
                ui.painter().set(under, egui::Shape::Vec(fills));
                for q in lit {
                    ui.painter().with_clip_rect(q.intersect(ui.clip_rect())).galley_with_override_text_color(origin, galley.clone(), egui::Color32::WHITE);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                // where links sit, for the e2e driver to click
                for (r, l) in &t.links {
                    if matches!(l, Link::Url(_) | Link::Masked(_)) {
                        let a = galley.pos_from_cursor(CCursor::new(r.start));
                        let at = Rect::from_min_size(origin + a.min.to_vec2(), vec2(8.0, a.height()));
                        if ui.clip_rect().contains_rect(at) {
                            d.rects.push(("link".into(), at));
                        }
                    }
                }
                let mut over_emoji = None;
                for (idx, url, size, lead) in &t.emojis {
                    let c = galley.pos_from_cursor(CCursor::new(*idx));
                    let x = origin.x + (c.min.x - lead).max(0.0);
                    let rect = Rect::from_min_size(pos2(x, origin.y + c.center().y - size / 2.0), vec2(*size, *size));
                    if resp.hover_pos().is_some_and(|p| rect.contains(p)) {
                        over_emoji = Some(*idx);
                    }
                    Image::from_uri(gif::uri(ui.ctx(), url, ui.is_rect_visible(rect))).show_loading_spinner(false).paint_at(ui, rect);
                    if let Some((_, id)) = url.split('?').next().and_then(|u| u.rsplit_once("/emojis/"))
                        && ui.clip_rect().contains_rect(rect)
                    {
                        d.rects.push((format!("custom_emoji:{}", id.split('.').next().unwrap_or("")), rect));
                    }
                }
                if !t.links.is_empty()
                    && let Some(p) = resp.hover_pos()
                {
                    // cursor_from_pos snaps to the nearest gap; we want the glyph under the pointer
                    let mut idx = galley.cursor_from_pos(p - origin).index.0;
                    if idx > 0 && origin.x + galley.pos_from_cursor(CCursor::new(idx)).min.x > p.x {
                        idx -= 1;
                    }
                    if let Some(i) = over_emoji {
                        idx = i;
                    }
                    if let Some((r, l)) = t.links.iter().find(|(r, _)| r.contains(&idx)) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if matches!(l, Link::Url(_) | Link::Masked(_)) && !crate::theme::view().underline_links {
                            for (row, left, right) in spans(&galley, r) {
                                ui.painter().hline(origin.x + left..=origin.x + right, origin.y + row.max.y, Stroke::new(1.0, pal().link));
                            }
                        }
                        if let Link::Emoji(_, name, _) = l {
                            let tip = egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), ui.id().with(("emoji_tip", msg)), egui::PopupAnchor::Pointer).show(|ui| {
                                ui.label(RichText::new(format!(":{name}:")).font(semi(14.0)).color(pal().header_primary));
                                ui.label(RichText::new("Click to learn more").font(reg(12.0)).color(pal().header_secondary));
                            });
                            if let Some(t) = tip {
                                d.rects.push(("emoji_tip".into(), t.response.rect));
                            }
                        }
                        if resp.middle_clicked() {
                            match l {
                                Link::Url(u) | Link::Masked(u) => d.acts.push(Act::Browser(u.clone())),
                                Link::Channel(c) => {
                                    let g = d.store.channels.get(c).map_or(0, |c| c.guild);
                                    d.acts.push(Act::Browser(format!("https://discord.com/channels/{}/{c}", if g == 0 { "@me".into() } else { g.to_string() })));
                                }
                                _ => {}
                            }
                        }
                        if resp.clicked() {
                            match l {
                                Link::Url(u) => d.acts.push(Act::Url(u.clone())),
                                Link::Masked(u) => d.acts.push(Act::Masked(u.clone())),
                                Link::Channel(c) => d.acts.push(Act::Channel(*c)),
                                Link::Role(r) => d.acts.push(Act::Role(*r, p + vec2(8.0, 8.0))),
                                Link::Spoiler(key) => d.acts.push(Act::Reveal(*key)),
                                Link::User(u) if ui.input(|i| i.modifiers.shift) => d.acts.push(Act::Mention(*u)),
                                Link::User(u) => {
                                    let at = p + vec2(8.0, 0.0);
                                    d.acts.push(Act::Profile(*u, at));
                                }
                                Link::Emoji(id, name, animated) => d.acts.push(Act::Emoji(*id, name.clone(), *animated, p + vec2(8.0, 8.0))),
                            }
                        }
                    }
                }
            }
            Block::Code(lang, code) => {
                ui.add_space(6.0);
                let job = ui.ctx().memory_mut(|m| m.caches.cache::<CodeJobs>().get((msg, lang.as_str(), code.as_str(), pal().text_normal, pal().text_muted)).clone());
                // .markup pre is at most 90% wide, and the .codeContainer in it at most 50vw
                let wide = (ui.available_width() * cap).min(ui.ctx().content_rect().width() / 2.0);
                let r = Frame::new()
                    .fill(pal().bg_code)
                    .stroke(Stroke::new(1.0, pal().border_normal))
                    .corner_radius(4)
                    .inner_margin(7)
                    .show(ui, |ui| {
                        ui.set_width(wide - 16.0);
                        ui.add(Label::new(job.clone()).wrap());
                    });
                if ui.clip_rect().contains_rect(r.response.rect) {
                    d.rects.push(("codeblock".into(), r.response.rect));
                    if job.sections.len() > 1 {
                        d.rects.push(("codeblock_tinted".into(), r.response.rect));
                    }
                }
                ui.add_space(4.0);
            }
            Block::List(list) => {
                // .markup ol, ul: 4 above the list, and li: 4 under each item
                ui.add_space(4.0);
                for (marker, inner) in &list.items {
                    let at = ui.cursor().min + vec2(list.indent, 0.0);
                    crate::ui::settings::mark(ui, &format!("li:{marker:?}"), Rect::from_min_max(ui.cursor().min, at + vec2(0.0, list.line)));
                    ui.scope_builder(UiBuilder::new().max_rect(Rect::from_min_size(at, vec2(ui.available_width() - list.indent, 100_000.0))), |ui| blocks_in(ui, inner, d, msg, cap));
                    bullet(ui, *marker, at, list.size, list.line);
                    // the margin under a list that ends the item collapses into the item's own
                    if !matches!(inner.last(), Some(Block::List(_))) {
                        ui.add_space(4.0);
                    }
                }
            }
            Block::Quote(inner) => {
                ui.add_space(4.0);
                let top = ui.cursor().top();
                let x = ui.cursor().left();
                // a bar of 4, then the blockquote: 12 and 8 of padding inside its 90%
                let wide = (ui.available_width() * cap).min(ui.available_width() - 4.0) - 20.0;
                let r = ui
                    .scope_builder(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x + 16.0, top), vec2(wide, 100_000.0))), |ui| blocks_wide(ui, inner, d, msg))
                    .response
                    .rect;
                ui.painter().rect_filled(Rect::from_min_max(pos2(x, top), pos2(x + 4.0, r.bottom())), CornerRadius::same(4), pal().spine);
                crate::ui::settings::mark(ui, "quote", Rect::from_min_max(pos2(x, top), pos2(x + 16.0 + wide, r.bottom())));
                crate::ui::settings::mark(ui, "quote_room", Rect::from_min_size(pos2(x, top), vec2(ui.available_width(), 1.0)));
                ui.add_space(4.0);
            }
        }
    }
}
