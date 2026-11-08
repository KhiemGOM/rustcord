use super::*;

/// Puts `mark` either side of the selection, or takes it off when it's already there, the words
/// staying selected.
pub(super) fn wrap(app: &mut App, ctx: &egui::Context, ch: Id, tid: egui::Id, r: CCursorRange, mark: &str) {
    let Some(draft) = app.drafts.get_mut(&ch) else { return };
    let [a, b] = r.sorted_cursors();
    let (a, b) = (a.index.0, b.index.0);
    let byte = |d: &str, c: usize| d.char_indices().nth(c).map_or(d.len(), |(i, _)| i);
    let (ba, bb) = (byte(draft, a), byte(draft, b));
    let n = mark.chars().count();
    let (na, nb) = if draft[..ba].ends_with(mark) && draft[bb..].starts_with(mark) {
        draft.replace_range(bb..bb + mark.len(), "");
        draft.replace_range(ba - mark.len()..ba, "");
        (a - n, b - n)
    } else {
        draft.insert_str(bb, mark);
        draft.insert_str(ba, mark);
        (a + n, b + n)
    };
    if let Some(mut st) = egui::TextEdit::load_state(ctx, tid) {
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(na), CCursor::new(nb))));
        st.store(ctx, tid);
    }
    ctx.memory_mut(|m| m.request_focus(tid));
}

/// Discord's formatting bar over selected text: bold, italic, underline, strikethrough, then
/// spoiler past a rule. A press, not a click, so the composer keeps its selection.
pub(super) fn format_bar(ctx: &egui::Context, at: Pos2) -> Option<&'static str> {
    let mut pick = None;
    egui::Area::new(egui::Id::new("format_bar")).order(egui::Order::Foreground).fixed_pos(at - vec2(4.0, 48.0)).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(4).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (icon, tip, mark) in [
                    (Icon::Bold, "Bold", "**"),
                    (Icon::Italic, "Italic", "*"),
                    (Icon::Underline, "Underline", "__"),
                    (Icon::Strike, "Strikethrough", "~~"),
                    (Icon::Eye, "Spoiler", "||"),
                ] {
                    if mark == "||" {
                        let (r, _) = ui.allocate_exact_size(vec2(13.0, 32.0), Sense::hover());
                        ui.painter().vline(r.center().x, r.y_range().shrink(6.0), Stroke::new(1.0, pal().border_subtle));
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r, 4, pal().control_hover, 0.05);
                    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                    if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                        pick = Some(mark);
                    }
                    resp.on_hover_text(tip);
                }
            });
        });
    });
    pick
}

/// Replace the token with the chosen completion, returning the new cursor (in chars).
/// The query as discord sends it: trimmed, lowercase, underscores as spaces, 100 characters at most.
pub(super) fn game_query(q: &str) -> String {
    q.trim().to_lowercase().replace('_', " ").chars().take(100).collect()
}

/// Discord asks for games a typed query at a time, no faster than every 200ms.
pub(super) fn game_search(app: &mut App, ctx: &egui::Context, q: &str) {
    let key = format!("games:{q}");
    if q.is_empty() || app.keyed.contains_key(&key) {
        return;
    }
    let id = egui::Id::new("game_query");
    let now = ctx.input(|i| i.time);
    let (last, since) = ctx.data(|d| d.get_temp::<(String, f64)>(id)).unwrap_or_default();
    if last != q {
        ctx.data_mut(|d| d.insert_temp(id, (q.to_string(), now)));
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    } else if now - since < 0.2 {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    } else {
        app.keyed.insert(key.clone(), None);
        app.rest("GET", format!("/games/autocomplete?q={}&filter_group=default", crate::ui::directory::urlencode(q)), None, Tag::Keyed(key));
    }
}

pub(super) fn complete(app: &mut App, chan: &Channel, it: &Item, start: usize, cursor: usize) -> Option<usize> {
    let ch = chan.id;
    let draft = app.drafts.entry(ch).or_default();
    let cur_b = draft.char_indices().nth(cursor).map(|(b, _)| b).unwrap_or(draft.len());
    let (shown, raw) = match it {
        Item::User(u, name, _) => (format!("@{name}"), format!("<@{u}>")),
        Item::Role(id, name, _) => (format!("@{name}"), format!("<@&{id}>")),
        Item::Everyone(e) => (format!("@{e}"), format!("@{e}")),
        Item::AtTime => ("@time".into(), "@time".into()),
        Item::AtGame => ("@game".into(), "@game".into()),
        Item::Game(id, name, _) => {
            app.store.game_names.insert(*id, name.clone());
            (format!("@{name}"), format!("<@${id}>"))
        }
        Item::When(secs, st) => {
            let raw = format!("<t:{secs}:{st}>");
            (raw.clone(), raw)
        }
        Item::Channel(id, name) => (format!("#{name}"), format!("<#{id}>")),
        Item::Emoji(e) if e.locked => (format!(":{}:", e.name), String::new()),
        Item::Emoji(e) if e.url.is_some() => (format!(":{}:", e.name), e.raw.clone()),
        Item::Emoji(e) => (e.raw.clone(), e.raw.clone()),
        Item::Command(i, path) => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            if let Some(cmd) = app.commands.get(&scope).and_then(|c| c.list.get(*i)).cloned() {
                app.start_slash(chan, &cmd, path, "");
            }
            return Some(0);
        }
        Item::BuiltIn(i) => {
            app.start_slash(chan, &built_in_command(*i), &[], "");
            if let Some(sl) = app.slash.get_mut(&chan.id).and_then(|s| s.slots.first_mut()) {
                sl.shown = true;
            }
            return Some(0);
        }
        Item::Sticker(id, ..) => {
            draft.replace_range(start..cur_b, "");
            let new_cursor = draft[..start].chars().count();
            let nonce = crate::state::make_nonce();
            app.net.send(crate::net::Cmd::Send { channel: ch, content: String::new(), nonce, reply: app.reply.take().map(|r| (r, app.reply_ping)), files: Vec::new(), stickers: vec![*id], tts: false });
            app.jump_to_present(ch);
            app.ac_sel = 0;
            return Some(new_cursor);
        }
    };
    let insert = format!("{shown} ");
    draft.replace_range(start..cur_b, &insert);
    if shown != raw && !raw.is_empty() {
        app.aliases.entry(ch).or_default().push((shown.clone(), raw));
    }
    let new_cursor = draft[..start + insert.len()].chars().count();
    app.ac_sel = 0;
    Some(new_cursor)
}

/// The `:name:` that ends `before`, as (byte offset of the opening colon, name).
pub(super) fn typed_name(before: &str) -> Option<(usize, &str)> {
    let body = before.strip_suffix(':')?;
    let start = body.rfind(':')?;
    let name = &body[start + 1..];
    let fine = name.len() >= 2 && name.chars().all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c));
    (fine && !body[..start].chars().last().is_some_and(char::is_alphanumeric)).then_some((start, name))
}

/// Typing the closing colon of a unicode name swaps in the glyph, like discord. Custom names
/// stay as typed and turn into the emoji when the message goes out.
pub(super) fn close_colon(app: &mut App, chan: &Channel, cursor: usize) -> Option<usize> {
    let draft = app.drafts.get(&chan.id)?;
    let end = draft.char_indices().nth(cursor).map(|(b, _)| b).unwrap_or(draft.len());
    let (start, name) = typed_name(&draft[..end])?;
    if draft[..start].matches('`').count() % 2 == 1 {
        return None;
    }
    let glyph = match app.emoji_named(chan.guild, name)? {
        (_, true) => return None,
        (glyph, false) => glyph,
    };
    let draft = app.drafts.get_mut(&chan.id)?;
    draft.replace_range(start..end, &glyph);
    Some(draft[..start + glyph.len()].chars().count())
}

/// Runs `f` over the stretches of `text` that aren't code, so nothing gets rewritten inside
/// `inline` spans or fenced blocks.
pub(super) fn outside_code(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        out.push_str(&f(&rest[..i]));
        let n = rest[i..].chars().take_while(|c| *c == '`').count();
        let after = &rest[i + n..];
        // a span closes on the next run of the same length; with no closer the ticks are plain text
        let mut at = 0;
        let close = loop {
            let Some(j) = after[at..].find('`') else { break None };
            let run = after[at + j..].chars().take_while(|c| *c == '`').count();
            if run == n {
                break Some(at + j + n);
            }
            at += j + run;
        };
        let end = close.unwrap_or(0);
        out.push_str(&rest[i..i + n + end]);
        rest = &after[end..];
    }
    out.push_str(&f(rest));
    out
}

/// What a reaction is made of: a ready-made `<:name:id>`, or a unicode emoji.
pub(super) fn react_emoji(raw: &str) -> Option<crate::state::ReactEmoji> {
    if custom_len(raw) == Some(raw.len()) {
        let animated = raw.starts_with("<a:");
        let inner = raw.trim_start_matches("<a:").trim_start_matches("<:").trim_end_matches('>');
        let (name, id) = inner.split_once(':')?;
        return Some(crate::state::ReactEmoji { id: id.parse().ok()?, name: name.to_string(), animated });
    }
    emojis::get(raw).map(|_| crate::state::ReactEmoji { id: 0, name: raw.to_string(), animated: false })
}

/// Length of a ready-made `<:name:id>` / `<a:name:id>` at the start of `s`.
pub(super) fn custom_len(s: &str) -> Option<usize> {
    let body = s.strip_prefix('<')?;
    let end = body.find('>')?;
    let inner = body[..end].strip_prefix('a').unwrap_or(&body[..end]);
    let mut parts = inner.strip_prefix(':')?.split(':');
    let (name, id) = (parts.next()?, parts.next()?);
    let ok = parts.next().is_none() && !name.is_empty() && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    ok.then_some(end + 2)
}

/// Discord's sits in the 24 above the composer, over the bottom of the messages, which fade out
/// under it. Below the composer there's only discord's 8.
pub(super) fn typing_line(app: &App, ui: &mut Ui, chan: &Channel, composer: Rect) {
    ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    let users: Vec<Id> = app.store.typing.get(&chan.id).map(|v| v.iter().map(|(u, _)| *u).collect()).unwrap_or_default();
    if users.is_empty() {
        return;
    }
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("typing")));
    let wide = ui.max_rect().x_range().expand(8.0);
    let fade = Rect::from_x_y_ranges(wide, composer.top() - 16.0..=composer.top());
    let mut m = egui::Mesh::default();
    let (clear, bg) = (pal().bg_primary.gamma_multiply(0.0), pal().bg_primary);
    m.colored_vertex(fade.left_top(), clear);
    m.colored_vertex(fade.right_top(), clear);
    m.colored_vertex(fade.right_bottom(), bg);
    m.colored_vertex(fade.left_bottom(), bg);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(m);

    let y = composer.top() - 12.0;
    for i in 0..3 {
        let (a, s) = crate::ui::motion::dots(ui.ctx(), i);
        p.circle_filled(pos2(composer.left() + 19.5 + i as f32 * 8.75, y), 3.5 * s, pal().text_normal.gamma_multiply(a));
    }
    let names: Vec<String> = users.iter().map(|u| app.store.name_in(chan.guild, *u)).collect();
    let parts: Vec<(&str, bool)> = match names.as_slice() {
        [a] => vec![(a, true), (" is typing...", false)],
        [a, b] => vec![(a, true), (" and ", false), (b, true), (" are typing...", false)],
        [a, b, c] => vec![(a, true), (", ", false), (b, true), (", and ", false), (c, true), (" are typing...", false)],
        _ => vec![("Several people are typing...", false)],
    };
    let mut job = egui::text::LayoutJob::default();
    for (s, name) in parts {
        let (font, color) = if name { (semi(12.0), pal().text_normal) } else { (medium(12.0), pal().header_secondary) };
        job.append(s, 0.0, egui::TextFormat::simple(font, color));
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(composer.width() - 80.0);
    let g = ui.painter().layout_job(job);
    p.galley(pos2(composer.left() + 64.5, y - g.size().y / 2.0), g, pal().text_normal);
}
