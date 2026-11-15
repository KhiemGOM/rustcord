use super::*;
use crate::ui::favorites;
use crate::ui::menus::channel_menu_in;

/// The Favorites server's sidebar: its name, then the favourites as `favorites::sections` orders them.
pub(super) fn list(app: &mut App, ui: &mut Ui) {
    header(ui, |ui| {
        text::label(ui, "Favorites", semi(16.0), pal().header_primary, Sense::hover());
    });
    let cur = app.cur();
    let dev = app.settings.dev;
    let mut open = None;
    let mut fold = None;
    let mut chat = None;
    let mut invite = None;
    let mut edit = None;
    let mut menu: Option<(Id, ChanAct)> = None;
    egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("side_list_favorites").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(13.0);
            let store = &app.store;
            for (cat, kids) in favorites::sections(store) {
                let collapsed = cat.and_then(|k| store.favorites.channels.get(&k)).is_some_and(|f| f.collapsed);
                if let Some(k) = cat {
                    ui.add_space(15.0);
                    let name = store.favorites.channels.get(&k).map(|f| f.nickname.clone()).unwrap_or_default();
                    let (r, _) = category(ui, &name, collapsed, false, false);
                    app.rects.insert(format!("favcat:{k}"), r.rect);
                    if r.clicked() {
                        fold = Some(k);
                    }
                }
                for c in &kids {
                    let selected = cur == Some(c.id);
                    let unread = c.is_text() && store.unread(c);
                    if collapsed && !selected && !unread {
                        continue;
                    }
                    let (r, _, inv, ed, _, ch) = channel_row(ui, store, c, selected, unread, store.mentions(c.id), store.is_muted(c), 0, None);
                    app.rects.insert(format!("favchan:{}", c.id), r.rect);
                    if inv {
                        invite = Some(c.id);
                    }
                    if ed {
                        edit = Some(c.id);
                    }
                    if ch {
                        chat = Some(c.id);
                    }
                    if r.clicked() {
                        open = Some(c.id);
                    }
                    channel_menu_in(&r, store, c, unread, false, dev, app.settings.hide_names.contains(&c.id), false, &mut menu);
                }
            }
            ui.add_space(12.0);
        });
    });
    match menu {
        Some((c, ChanAct::Read)) => app.mark_read(&[c]),
        Some((c, ChanAct::Mute(m, secs))) => app.mute_for(c, false, m, secs),
        Some((c, ChanAct::Level(l))) => app.set_level(c, l),
        Some((c, ChanAct::Invite)) => invite = Some(c),
        Some((c, ChanAct::Link)) => {
            let gid = app.store.channels.get(&c).map_or(0, |x| x.guild);
            ui.ctx().copy_text(format!("https://discord.com/channels/{gid}/{c}"));
        }
        Some((c, ChanAct::CopyId)) => ui.ctx().copy_text(c.to_string()),
        Some((c, ChanAct::List(on))) => app.chan_flag(c, crate::ui::OPTED, on),
        Some((c, ChanAct::Pin)) => {
            let on = app.store.chan_flags.get(&c).is_none_or(|f| f & crate::ui::PINNED == 0);
            app.chan_flag(c, crate::ui::PINNED, on);
        }
        Some((c, ChanAct::HideNames)) => {
            if let Some(i) = app.settings.hide_names.iter().position(|x| *x == c) {
                app.settings.hide_names.remove(i);
            } else {
                app.settings.hide_names.push(c);
            }
            app.save_settings(ui.ctx());
        }
        Some((c, ChanAct::Chat)) => chat = Some(c),
        Some((c, ChanAct::Unfavorite)) => favorites::remove(app, c),
        Some((c, ChanAct::Favorite(p))) => favorites::add(app, c, p),
        Some((c, ChanAct::Edit)) => edit = Some(c),
        _ => {}
    }
    if let Some(k) = fold {
        favorites::toggle_category(app, k);
    }
    if let Some(c) = invite {
        app.invite_people(c, "Guild Channels");
    }
    if let Some(c) = edit {
        app.open_chan_settings(c);
    }
    if let Some(c) = open {
        if app.store.channels.get(&c).is_some_and(|c| c.is_voice()) {
            app.join_voice(c, true);
        }
        favorites::open(app, c);
    }
    if let Some(c) = chat {
        favorites::open(app, c);
        app.voice_chat = true;
    }
}
