use super::*;

pub(super) enum Hit {
    Channel(Id),
    Guild(Id),
}

pub(super) fn switcher(app: &mut App, ctx: &egui::Context) {
    let cur = app.cur();
    let here = app.ui.guild;
    let Some(q) = &mut app.switcher else { return };
    let sel_id = egui::Id::new("switcher_sel");
    let mut sel: usize = ctx.data(|d| d.get_temp(sel_id).unwrap_or(0));
    let ql = q.to_lowercase();
    let store = &app.store;

    // with nothing typed discord lists where you were and what's unread, each under a header
    let mut hits: Vec<Hit> = Vec::new();
    let mut heads: Vec<(usize, &str)> = Vec::new();
    let mode = match ql.chars().next() {
        Some('@') => Some("Searching All Users"),
        Some('#') => Some("Searching Text Channels"),
        Some('!') => Some("Searching Voice Channels"),
        Some('*') => Some("Searching Servers"),
        _ => None,
    };
    if let Some(m) = mode {
        heads.push((0, m));
    }
    if ql.is_empty() {
        let prev: Vec<Hit> = app.ui.recent.iter().filter(|c| Some(**c) != cur && store.channels.contains_key(c)).take(3).map(|c| Hit::Channel(*c)).collect();
        if !prev.is_empty() {
            heads.push((0, "Previous Channels"));
            hits.extend(prev);
        }
        // this server's first, then the rest down the rail, each in sidebar order
        let order = here.into_iter().chain(store.guild_order.iter().copied().filter(|g| Some(*g) != here));
        let unread: Vec<Id> = order
            .filter(|g| !store.muted.contains(g))
            .flat_map(|g| crate::ui::side::tree(store, g).into_iter().flat_map(|(_, k)| k))
            .filter(|c| c.is_text() && Some(c.id) != cur && store.unread(c))
            .map(|c| c.id)
            .take(20)
            .collect();
        if !unread.is_empty() {
            heads.push((hits.len(), "Unread Channels"));
            hits.extend(unread.into_iter().map(Hit::Channel));
        }
    } else {
        // discord's prefixes narrow it: @ people, # text channels, ! voice channels, * servers
        let (only, ql) = match ql.chars().next() {
            Some(c @ ('@' | '#' | '!' | '*')) => (Some(c), ql[1..].trim_start().to_string()),
            _ => (None, ql.clone()),
        };
        if only.is_none() || only == Some('@') {
            for &c in &store.dms {
                if channel_title(store, c).to_lowercase().contains(&ql) {
                    hits.push(Hit::Channel(c));
                }
            }
        }
        for gid in &store.guild_order {
            let Some(g) = store.guilds.get(gid) else { continue };
            if matches!(only, None | Some('*')) && g.name.to_lowercase().contains(&ql) {
                hits.push(Hit::Guild(*gid));
            }
            if matches!(only, Some('@' | '*')) {
                continue;
            }
            for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
                let kind = if only == Some('!') { c.is_voice() } else { c.is_text() || c.is_forum() };
                if kind && c.name.to_lowercase().contains(&ql) && store.can_view(c) && !c.is_thread() {
                    hits.push(Hit::Channel(c.id));
                }
            }
        }
        hits.truncate(10);
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        sel = (sel + 1).min(hits.len().saturating_sub(1));
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        sel = sel.saturating_sub(1);
    }
    sel = sel.min(hits.len().saturating_sub(1));
    let mut pick = ctx.input(|i| i.key_pressed(Key::Enter)).then_some(sel);

    // discord's: a 638 wide, 70 tall input, a fixed results area, the protip under a rule
    let rows_h = (ctx.content_rect().height() - 2.0 * (crate::ui::titlebar::HEIGHT + 41.0 + 4.0) - 155.0).clamp(80.0, 236.0);
    let typed = !q.is_empty();
    let modal = crate::ui::motion::Modal::new("switcher".into())
        .frame(
            Frame::new()
                .fill(pal().bg_secondary)
                .corner_radius(8)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)))
                .inner_margin(Margin { left: 20, right: 20, top: 12, bottom: 12 }),
        )
        .show(ctx, |ui| {
            ui.set_width(638.0);
            input_frame().corner_radius(8).inner_margin(Margin::symmetric(12, 22)).show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(q)
                        .frame(Frame::NONE)
                        .font(reg(22.0))
                        .hint_text(RichText::new("Where would you like to go?").font(reg(22.0)).color(pal().text_muted))
                        .desired_width(ui.available_width()),
                );
                r.request_focus();
            });
            ui.add_space(16.0);
            egui::ScrollArea::vertical().max_height(rows_h).min_scrolled_height(rows_h).auto_shrink([false, false]).show(ui, |ui| {
                if hits.is_empty() && !q.is_empty() && mode.is_none() {
                    ui.add_space(90.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Can't find what you're looking for?").font(medium(16.0)).color(pal().text_muted));
                    });
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                if hits.is_empty() && let Some(m) = mode {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                    ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, m.to_uppercase(), semi(12.0), pal().header_secondary);
                }
                for (i, hit) in hits.iter().enumerate() {
                    if let Some((_, h)) = heads.iter().find(|(at, _)| *at == i) {
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                        ui.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, h.to_uppercase(), semi(12.0), pal().header_secondary);
                    }
                    if switch_row(ui, store, hit, i == sel).clicked() {
                        pick = Some(i);
                    }
                }
            });
            ui.add_space(10.0);
            let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(rule.x_range(), rule.center().y, egui::Stroke::new(1.0, pal().border_subtle));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("PROTIP:").font(bold(12.0)).color(pal().green));
                ui.label(RichText::new("Start searches with @ # ! * $ to narrow results.").font(reg(14.0)).color(pal().header_secondary));
                let learn = ui.add(egui::Label::new(RichText::new("Learn more").font(reg(14.0)).color(pal().link)).sense(Sense::click()));
                if learn.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if learn.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/115000070311"));
                }
            });
        });
    // the title and arrows sit on the backdrop around the box
    let b = modal.response.rect;
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("switcher_deco")));
    p.text(pos2(b.center().x, b.top() - 41.0 + 12.0), Align2::CENTER_CENTER, if typed { "Select a result and press ENTER to jump to it" } else { "Search for servers, channels or DMs" }, medium(18.0), egui::Color32::WHITE);
    let iy = b.top() + 12.0 + 35.0;
    crate::ui::icons::draw(&p, crate::ui::icons::Icon::ArrowRight, Rect::from_center_size(pos2(b.left() - 40.0, iy), vec2(28.0, 28.0)), egui::Color32::WHITE);
    crate::ui::icons::draw(&p, crate::ui::icons::Icon::ArrowLeft, Rect::from_center_size(pos2(b.right() + 40.0, iy), vec2(28.0, 28.0)), egui::Color32::WHITE);
    ctx.data_mut(|d| d.insert_temp(sel_id, sel));
    if let Some(i) = pick.filter(|i| *i < hits.len()) {
        match hits[i] {
            Hit::Channel(c) => {
                let g = app.store.channels.get(&c).map(|c| c.guild).unwrap_or(0);
                app.ui.guild = if g == 0 { None } else { Some(g) };
                app.open(c);
            }
            Hit::Guild(g) => app.select_guild(Some(g)),
        }
        app.switcher = None;
    } else if modal.should_close() {
        app.switcher = None;
    }
}

/// One result: a channel's icon, name and category with its server on the right, a person's
/// avatar, name and username, or a server.
pub(super) fn switch_row(ui: &mut Ui, store: &crate::state::Store, hit: &Hit, sel: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    if sel || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
    }
    let p = ui.painter();
    let y = r.center().y;
    let x = r.left() + 35.0;
    let lit = if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary };
    let icon = Rect::from_center_size(pos2(r.left() + 20.0, y), vec2(20.0, 20.0));
    match *hit {
        Hit::Guild(g) => {
            let Some(g) = store.guilds.get(&g) else { return resp };
            match g.icon_url() {
                Some(u) => avatar(ui, icon, &u),
                None => {
                    p.rect_filled(icon, CornerRadius::same(6), pal().bg_accent);
                    p.text(icon.center(), Align2::CENTER_CENTER, g.acronym(), medium(9.0), pal().header_primary);
                }
            }
            text::paint_em(p, pos2(x, y), Align2::LEFT_CENTER, &g.name, medium(16.0), lit, r.width() - 50.0, 1.0);
        }
        Hit::Channel(c) => {
            let Some(c) = store.channels.get(&c) else { return resp };
            if c.guild == 0 {
                let person = if c.kind == 1 { c.recipients.first().and_then(|u| store.user(*u)) } else { None };
                match person {
                    Some(u) => avatar(ui, icon, &u.avatar_url()),
                    None => {
                        p.circle_filled(icon.center(), 10.0, pal().bg_accent);
                        crate::ui::icons::draw(p, Icon::People, icon.shrink(4.0), pal().header_primary);
                    }
                }
                let n = text::paint_em(p, pos2(x, y), Align2::LEFT_CENTER, &channel_title(store, c.id), medium(16.0), lit, r.width() - 200.0, 1.0);
                if let Some(u) = person {
                    text::paint(p, pos2(n.right() + 4.0, y), Align2::LEFT_CENTER, &u.username, reg(16.0), pal().header_secondary, r.right() - n.right() - 14.0);
                }
            } else {
                crate::ui::side::chan_icon(store, p, c, icon.shrink(2.0), pal().header_secondary);
                let col = if store.unread(c) { pal().header_primary } else { lit };
                let guild = store.guilds.get(&c.guild).map(|g| g.name.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
                let g = p.text(pos2(r.right() - 10.0, y), Align2::RIGHT_CENTER, guild, medium(14.0), pal().header_secondary);
                let n = text::paint_em(p, pos2(x, y), Align2::LEFT_CENTER, &c.name, medium(16.0), col, g.left() - x - 120.0, 1.0);
                if let Some(cat) = store.channels.get(&c.parent).filter(|k| k.kind == 4) {
                    text::paint(p, pos2(n.right() + 4.0, y + 1.0), Align2::LEFT_CENTER, &cat.name.to_uppercase(), semi(10.0), pal().text_muted, g.left() - n.right() - 16.0);
                }
            }
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
