use super::*;

pub(super) fn popup(app: &mut App, ctx: &egui::Context, chan: &Channel, items: &[Item], composer: Rect, kind: char) -> Option<usize> {
    let mut picked = None;
    let title = match kind {
        '@' => "Members".to_string(),
        '#' => "Text Channels".to_string(),
        ':' => "Emoji Matching".to_string(),
        'T' => "Time".to_string(),
        'G' => "Games".to_string(),
        _ => "Commands".to_string(),
    };
    let row_h = if kind == '/' { 52.0 } else { 34.0 };
    egui::Area::new(egui::Id::new("autocomplete"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(composer.left_top())
        .show(ctx, |ui| {
            Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                ui.set_width(composer.width() - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                ui.painter().text(pos2(hr.left() + 8.0, hr.center().y), Align2::LEFT_CENTER, title, medium(14.0), pal().header_secondary);
                // the area only offers the space below the composer, so the list has to ask for its height
                let list_h = row_h * items.len().min(10) as f32;
                egui::ScrollArea::vertical().min_scrolled_height(list_h).max_height(list_h).show(ui, |ui| {
                    for (i, it) in items.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                            app.ac_sel = i;
                        }
                        if i == app.ac_sel {
                            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                        }
                        row(app, ui, chan, it, r);
                        if matches!(it, Item::Emoji(e) if e.locked) {
                            resp.clone().on_hover_text("Get Nitro, or the role this emoji needs, to use it here");
                        }
                        if resp.clicked() {
                            picked = Some(i);
                        }
                    }
                });
            });
        });
    picked
}

pub(super) fn row(app: &App, ui: &Ui, chan: &Channel, it: &Item, r: Rect) {
    let p = ui.painter();
    let y = r.center().y;
    let x = r.left() + 8.0;
    match it {
        Item::User(u, name, username) => {
            if let Some(user) = app.store.user(*u) {
                avatar(ui, Rect::from_center_size(pos2(x + 12.0, y), vec2(24.0, 24.0)), &user.avatar_url());
            }
            let nr = text::paint(p, pos2(x + 32.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary, r.width() * 0.5);
            p.text(pos2(nr.right() + 8.0, y), Align2::LEFT_CENTER, username, reg(14.0), pal().header_secondary);
        }
        Item::Role(_, name, color) => {
            let col = if *color == 0 { pal().header_primary } else { role_color(*color) };
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{name}"), semi(15.0), col);
        }
        Item::Everyone(e) => {
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, format!("@{e}"), semi(15.0), pal().header_primary);
            let note = if *e == "here" { "Notify everyone online in this channel." } else { "Notify everyone who has permission to view this channel." };
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, note, reg(13.0), pal().header_secondary);
        }
        Item::AtGame => {
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, "@game", semi(15.0), pal().header_primary);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, "Mention a game", reg(13.0), pal().header_secondary);
        }
        Item::Game(id, name, icon) => {
            let ir = Rect::from_center_size(pos2(x + 12.0, y), vec2(24.0, 24.0));
            match icon {
                Some(h) => avatar(ui, ir, &format!("{}/app-icons/{id}/{h}.png?size=64", crate::api::cdn())),
                None => {
                    p.rect_filled(ir, CornerRadius::same(6), pal().bg_tertiary);
                }
            }
            text::paint(p, pos2(x + 32.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary, r.width() - 48.0);
        }
        Item::AtTime => {
            p.text(pos2(x + 4.0, y), Align2::LEFT_CENTER, "@time", semi(15.0), pal().header_primary);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, "Refer to a time dynamically in the viewer's time zone", reg(13.0), pal().header_secondary);
        }
        Item::When(secs, st) => {
            text::paint(p, pos2(x + 4.0, y), Align2::LEFT_CENTER, &crate::markdown::fmt_ts(*secs, *st), semi(15.0), pal().header_primary, r.width() * 0.7);
            let name = crate::ui::when::STYLES.iter().find(|(c, _)| c == st).map_or("", |(_, n)| n);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, name, reg(13.0), pal().header_secondary);
        }
        Item::Channel(id, name) => {
            let ir = Rect::from_center_size(pos2(x + 10.0, y), vec2(18.0, 18.0));
            match app.store.channels.get(id) {
                Some(c) => crate::ui::side::chan_icon(&app.store, p, c, ir, pal().channel_default),
                None => icons::draw(p, Icon::Hash, ir, pal().channel_default),
            }
            text::paint(p, pos2(x + 28.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary, r.width() - 160.0);
            let cat = app
                .store
                .channels
                .get(id)
                .and_then(|c| app.store.channels.get(&c.parent))
                .map(|c| c.name.to_uppercase())
                .unwrap_or_default();
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, cat, semi(12.0), pal().header_secondary);
        }
        Item::Emoji(e) => {
            let er = Rect::from_center_size(pos2(x + 12.0, y), vec2(22.0, 22.0));
            let fade = if e.locked { 0.4 } else { 1.0 };
            match &e.url {
                Some(u) => text::image_tint(p, u, er, Color32::WHITE.gamma_multiply(fade)),
                None => text::image(p, &crate::markdown::twemoji(&e.raw), er),
            }
            p.text(pos2(x + 32.0, y), Align2::LEFT_CENTER, format!(":{}:", e.name), semi(15.0), pal().header_primary.gamma_multiply(fade));
            if let Some(srv) = &e.server {
                text::paint(p, pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, srv, semi(12.0), pal().header_secondary.gamma_multiply(fade), r.width() * 0.4);
            }
        }
        Item::Sticker(id, name, fmt, server) => {
            text::image(p, &crate::api::sticker(*id, *fmt, 64), Rect::from_center_size(pos2(x + 12.0, y), vec2(26.0, 26.0)));
            p.text(pos2(x + 32.0, y), Align2::LEFT_CENTER, name, semi(15.0), pal().header_primary);
            text::paint(p, pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, server, semi(12.0), pal().header_secondary, r.width() * 0.4);
        }
        Item::Command(i, path) => {
            let scope = if chan.guild != 0 { chan.guild } else { chan.id };
            let Some(c) = app.commands.get(&scope).and_then(|c| c.list.get(*i)) else { return };
            let mut leaf = &c["options"];
            let mut desc = c["description"].as_str().unwrap_or("");
            for p in path {
                desc = leaf[*p]["description"].as_str().unwrap_or(desc);
                leaf = &leaf[*p]["options"];
            }
            let app_id = sid(&c["application_id"]);
            let (app_name, icon) = app
                .commands
                .get(&scope)
                .and_then(|cs| cs.apps.iter().find(|a| a.0 == app_id))
                .map(|a| (a.1.clone(), a.2.clone()))
                .unwrap_or_default();
            let ir = Rect::from_center_size(pos2(x + 16.0, y), vec2(32.0, 32.0));
            match icon {
                Some(h) => avatar(ui, ir, &format!("{}/app-icons/{app_id}/{h}.png?size=64", crate::api::cdn())),
                None => {
                    p.circle_filled(ir.center(), 16.0, pal().blurple);
                }
            }
            let name = slash::form_name(c, path);
            let mut args = String::new();
            let mut optional = 0;
            for o in leaf.as_array().into_iter().flatten() {
                if o["required"].as_bool() == Some(true) {
                    args.push_str(&format!(" {}", o["name"].as_str().unwrap_or("")));
                } else {
                    optional += 1;
                }
            }
            if optional > 0 {
                args.push_str(&format!("  +{optional} optional"));
            }
            let nr = p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
            p.text(pos2(nr.right() + 6.0, y - 9.0), Align2::LEFT_CENTER, args, reg(14.0), pal().header_secondary);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, desc, reg(13.0), pal().header_secondary, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, app_name, semi(13.0), pal().header_secondary);
        }
        Item::BuiltIn(i) => {
            let (name, args, desc) = BUILT_IN[*i];
            crate::ui::settings::said(ui.ctx(), &format!("builtin:{name}"), desc);
            let ir = Rect::from_center_size(pos2(x + 16.0, y), vec2(32.0, 32.0));
            p.circle_filled(ir.center(), 16.0, pal().blurple);
            icons::draw(p, Icon::Discord, ir.shrink(8.0), Color32::WHITE);
            let nr = p.text(pos2(x + 44.0, y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
            p.text(pos2(nr.right() + 6.0, y - 9.0), Align2::LEFT_CENTER, args, reg(14.0), pal().header_secondary);
            text::paint(p, pos2(x + 44.0, y + 10.0), Align2::LEFT_CENTER, desc, reg(13.0), pal().header_secondary, r.width() - 200.0);
            p.text(pos2(r.right() - 8.0, y), Align2::RIGHT_CENTER, "Built-In", semi(13.0), pal().header_secondary);
        }
    }
}
