use super::*;

pub(super) enum MenuPick {
    Publish,
    React,
    Edit,
    Reply,
    Forward,
    Thread,
    Copy,
    Pin,
    Bookmark,
    Remind(chrono::DateTime<chrono::Utc>),
    RemindCustom,
    App(serde_json::Value),
    Unread,
    Link,
    Speak,
    ClearReactions,
    ClearEmoji(ReactEmoji),
    ViewReactions,
    Resend,
    SaveFile,
    Configure,
    Profile(Id),
    Delete,
    Report,
    CopyId,
}

pub(crate) fn context_menu(app: &mut App, ctx: &egui::Context) {
    let Some((ch, id, at)) = app.menu else { return };
    let Some(m) = app.store.find_msg(ch, id) else {
        app.menu = None;
        return;
    };
    let own = m.author == app.store.me;
    let content = m.content.clone();
    let pinned = m.pinned;
    let reacted = !m.reactions.is_empty();
    let reaction_list: Vec<crate::state::Reaction> = m.reactions.iter().filter(|r| r.count > r.burst || r.burst == 0).cloned().collect();
    let has_thread = m.thread.is_some();
    let kind = m.kind;
    let failed = m.failed;
    let sent = !m.failed && !m.pending;
    let interaction = m.interaction.clone();
    let author_name = app.store.user(m.author).map(|u| u.display().to_string()).unwrap_or_default();
    let voice = (m.flags & (1 << 13) != 0).then(|| m.attachments.first().map(|a| (a.url.clone(), a.filename.clone()))).flatten();
    let editable = matches!(kind, 0 | 19 | 20 | 23);
    let pinnable = editable;
    let forwardable = matches!(kind, 0 | 19 | 20 | 23 | 35 | 69) && m.poll.is_none();
    let replyable = matches!(kind, 0 | 7 | 19 | 20 | 23 | 24 | 25 | 32..=42 | 45 | 46 | 69);
    let published = m.flags & 1 != 0;
    let dev = app.settings.dev;
    let mut frequent: Vec<ReactEmoji> = app.ui.quick.clone();
    for n in ["✅", "❤️", "🔥", "😂", "👍", "😭", "💀", "👀", "🙏", "😊"] {
        if !frequent.iter().any(|q| q.id == 0 && q.name == n) {
            frequent.push(ReactEmoji { id: 0, name: n.to_string(), animated: false });
        }
    }
    frequent.truncate(10);
    let quick = app.quick_reactions();
    let Some(chan) = app.store.channels.get(&ch).cloned() else { return };
    let manage = chan.guild == 0 || app.store.perms(&chan) & crate::state::MANAGE_MESSAGES != 0;
    let pin = app.store.can_pin(&chan);
    let perms = app.store.perms(&chan);
    let can_react = chan.guild == 0 || perms & crate::state::ADD_REACTIONS != 0;
    let replyable = replyable && app.store.can_send(&chan);
    let threads = chan.guild != 0 && matches!(chan.kind, 0 | 5) && !has_thread && editable && perms & crate::state::CREATE_PUBLIC_THREADS != 0;
    let configure = chan.guild != 0 && kind == 7 && app.store.perms(&chan) & crate::state::MANAGE_GUILD != 0;
    let publishable = chan.kind == 5 && (own || manage);
    let saved = app.bookmarks.contains(&id);
    // the apps' message commands, for the Apps submenu
    app.ensure_commands(&chan);
    let scope = if chan.guild != 0 { chan.guild } else { ch };
    let apps: Vec<serde_json::Value> = app.commands.get(&scope).map(|c| c.list.iter().filter(|c| c["type"].as_u64() == Some(3)).cloned().collect()).unwrap_or_default();
    let app_store = &app.store;
    let mut pick = None;
    let mut react = None;
    let mut swap = None;
    let mut tiles = Vec::new();
    // discord's message menu: one-click reactions on top, then its groups
    let open = crate::ui::float_menu(ctx, &format!("msg_menu{id}"), at, |ui| {
        use crate::ui::{Trail, menu_item, menu_row, menu_sep};
        ui.set_min_width(204.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, e) in quick.iter().enumerate().filter(|_| can_react) {
                let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                tiles.push(r);
                ui.painter().rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                let er = Rect::from_center_size(r.center(), vec2(24.0, 24.0));
                Image::from_uri(gif::uri(ui.ctx(), &emoji_url(e), true)).show_loading_spinner(false).paint_at(ui, er);
                // not discord's: right-click a slot to put another emoji there
                if resp.secondary_clicked() {
                    swap = Some((i, r.left_bottom()));
                }
                if resp.on_hover_text("Right-click to change").clicked() {
                    react = Some(e.clone());
                }
            }
        });
        if can_react {
            ui.add_space(8.0);
        }
        let mut started = false;
        let mut group = |ui: &mut Ui, on: bool| {
            if on && started {
                menu_sep(ui);
            }
            started |= on;
            on
        };
        if group(ui, can_react || reacted) {
            if can_react {
                let add = menu_row(ui, Some(Icon::AddReaction), "Add Reaction", None, false, Trail::Caret);
                if add.clicked() {
                    pick = Some(MenuPick::React);
                }
                egui::containers::menu::SubMenu::new().show(ui, &add, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for e in &frequent {
                        let name = if e.id != 0 { e.name.clone() } else { crate::ui::reactions::unicode_name(&e.name).to_string() };
                        let url = gif::uri(ui.ctx(), &emoji_url(e), true);
                        if menu_row(ui, None, &format!(":{name}:"), None, false, Trail::Image(url)).clicked() {
                            react = Some(e.clone());
                        }
                    }
                    menu_sep(ui);
                    if menu_item(ui, None, "View More", false) {
                        pick = Some(MenuPick::React);
                    }
                });
            }
            if reacted && menu_item(ui, Some(Icon::Smile), "View Reactions", false) {
                pick = Some(MenuPick::ViewReactions);
            }
        }
        if group(ui, sent && (own && editable || replyable || forwardable || publishable && !published) || threads) {
            if sent && own && editable && menu_item(ui, Some(Icon::Pencil), "Edit Message", false) {
                pick = Some(MenuPick::Edit);
            }
            if sent && replyable && menu_item(ui, Some(Icon::Reply), "Reply", false) {
                pick = Some(MenuPick::Reply);
            }
            if sent && forwardable && menu_item(ui, Some(Icon::Forward), "Forward", false) {
                pick = Some(MenuPick::Forward);
            }
            if threads && menu_item(ui, Some(Icon::ThreadPlus), "Create Thread", false) {
                pick = Some(MenuPick::Thread);
            }
            if sent && publishable && !published && menu_item(ui, Some(Icon::Megaphone), "Publish", false) {
                pick = Some(MenuPick::Publish);
            }
        }
        group(ui, true);
        if !content.is_empty() && menu_item(ui, Some(Icon::Copy), "Copy Text", false) {
            pick = Some(MenuPick::Copy);
        }
        if sent && pin && pinnable && menu_item(ui, Some(Icon::Pin), if pinned { "Unpin Message" } else { "Pin Message" }, false) {
            pick = Some(MenuPick::Pin);
        }
        let mark = if saved { (Icon::Bookmarked, "Remove Bookmark") } else { (Icon::Bookmark, "Bookmark Message") };
        if menu_item(ui, Some(mark.0), mark.1, false) {
            pick = Some(MenuPick::Bookmark);
        }
        let rem = menu_row(ui, Some(Icon::Clock), "Create Reminder", None, false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &rem, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (r, _) = ui.allocate_exact_size(vec2(170.0, 36.0), Sense::hover());
            ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, "Remind Me", medium(14.0), pal().chat_muted);
            for (label, due) in App::reminder_times() {
                if menu_item(ui, None, &label, false) {
                    pick = Some(MenuPick::Remind(due));
                }
            }
            if menu_item(ui, None, "Custom...", false) {
                pick = Some(MenuPick::RemindCustom);
            }
        });
        let a = menu_row(ui, Some(Icon::Launched), "Apps", None, false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &a, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if apps.is_empty() {
                ui.add_enabled_ui(false, |ui| menu_item(ui, None, "No Apps", false));
            }
            for c in &apps {
                if menu_item(ui, None, c["name"].as_str().unwrap_or(""), false) {
                    pick = Some(MenuPick::App(c.clone()));
                }
            }
        });
        if failed && menu_item(ui, Some(Icon::Refresh), "Resend message", false) {
            pick = Some(MenuPick::Resend);
        }
        if menu_item(ui, Some(Icon::MarkUnread), "Mark Unread", false) {
            pick = Some(MenuPick::Unread);
        }
        if sent && configure && menu_item(ui, Some(Icon::CircleCheck), "Configure", false) {
            pick = Some(MenuPick::Configure);
        }
        if menu_item(ui, Some(Icon::Link), "Copy Message Link", false) {
            pick = Some(MenuPick::Link);
        }
        if !content.is_empty() && menu_item(ui, Some(Icon::Volume), "Speak Message", false) {
            pick = Some(MenuPick::Speak);
        }
        if let Some(i) = &interaction {
            let info = menu_row(ui, Some(Icon::Help), "View Interaction Info", None, false, Trail::Caret);
            egui::containers::menu::SubMenu::new().show(ui, &info, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (owner, url) = if i.in_guild {
                    let g = app_store.guilds.get(&i.owner);
                    (g.map(|g| g.name.clone()).unwrap_or_default(), g.and_then(|g| g.icon_url()))
                } else {
                    let u = app_store.user(i.owner);
                    (u.map(|u| u.display().to_string()).unwrap_or_default(), u.map(|u| u.avatar_url()))
                };
                if i.owner != 0 {
                    let (r, lead) = crate::ui::menu_row_at(ui, true, &owner, Some(&format!("Installed {author_name}")), false, Trail::None);
                    if let Some(url) = url {
                        Image::from_uri(url).corner_radius(CornerRadius::same(10)).show_loading_spinner(false).paint_at(ui, lead);
                    }
                    let _ = r;
                }
                if let Some(u) = app_store.user(i.user) {
                    let (r, lead) = crate::ui::menu_row_at(ui, true, u.display(), Some("Triggered this message"), false, Trail::None);
                    avatar(ui, lead, &u.avatar_url());
                    if r.clicked() {
                        pick = Some(MenuPick::Profile(i.user));
                    }
                }
            });
        }
        let can_delete = (own || manage) && !matches!(kind, 1..=5 | 21 | 35 | 56 | 57 | 64 | 68);
        let can_report = !own && sent && kind != 67;
        if group(ui, sent && manage && chan.guild != 0 && reacted || can_delete || can_report) {
            if sent && manage && chan.guild != 0 && reacted {
                let rm = menu_row(ui, Some(Icon::Close), "Remove Reactions", None, true, Trail::Caret);
                egui::containers::menu::SubMenu::new().show(ui, &rm, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for r in &reaction_list {
                        let url = gif::uri(ui.ctx(), &emoji_url(&r.emoji), true);
                        if menu_row(ui, None, &crate::ui::reactions::colon_name(&r.emoji), None, false, Trail::Image(url)).clicked() {
                            pick = Some(MenuPick::ClearEmoji(r.emoji.clone()));
                        }
                    }
                });
                if menu_item(ui, Some(Icon::Trash), "Remove All Reactions", true) {
                    pick = Some(MenuPick::ClearReactions);
                }
            }
            if can_delete && menu_item(ui, Some(Icon::Trash), "Delete Message", true) {
                pick = Some(MenuPick::Delete);
            }
            if can_report && menu_item(ui, Some(Icon::Flag), "Report Message", true) {
                pick = Some(MenuPick::Report);
            }
        }
        if group(ui, voice.is_some()) && menu_item(ui, Some(Icon::Download), "Save File", false) {
            pick = Some(MenuPick::SaveFile);
        }
        if dev {
            menu_sep(ui);
            if menu_item(ui, Some(Icon::Id), "Copy Message ID", false) {
                pick = Some(MenuPick::CopyId);
            }
        }
    });
    if let Some(e) = react {
        app.react(ch, id, e, true);
        app.menu = None;
        return;
    }
    for (i, r) in tiles.into_iter().enumerate() {
        app.rects.insert(format!("quick:{i}"), r);
    }
    if let Some((slot, under)) = swap {
        app.menu = None;
        app.picker = Some((Pick::Quick(slot), under));
        return;
    }
    let shift = ctx.input(|i| i.modifiers.shift);
    let picked = pick.is_some();
    match pick {
        Some(MenuPick::React) => app.picker = Some((Pick::React(ch, id), at)),
        Some(MenuPick::Edit) => app.editing = Some((id, content)),
        Some(MenuPick::Reply) => {
            app.reply = Some(id);
            app.reply_ping = !shift && !own;
            app.focus_composer = true;
        }
        Some(MenuPick::Forward) => app.forward = Some(crate::ui::popups::Forward::new(ch, id)),
        Some(MenuPick::Thread) => app.new_thread = Some(crate::ui::popups::NewThread::from(app, ch, id)),
        Some(MenuPick::Publish) => app.publish_msg(ch, id),
        Some(MenuPick::Copy) => ctx.copy_text(content),
        Some(MenuPick::Pin) => app.pin_msg(ch, id, !shift),
        Some(MenuPick::Bookmark) => app.bookmark(ch, id),
        Some(MenuPick::Remind(due)) => app.remind(ch, id, due),
        Some(MenuPick::RemindCustom) => app.remind_custom = Some(crate::ui::remind::Custom::new(ch, id)),
        Some(MenuPick::App(c)) => app.message_command(&chan, &c, id),
        Some(MenuPick::Unread) => app.mark_unread(ch, id),
        Some(MenuPick::Link) => ctx.copy_text(app.message_link(ch, id)),
        Some(MenuPick::Speak) => app.speak(ch, id),
        Some(MenuPick::ClearReactions) => app.clear_reactions(ch, id, !shift),
        Some(MenuPick::ClearEmoji(e)) => app.clear_emoji(ch, id, e, !shift),
        Some(MenuPick::ViewReactions) => app.reactors = Some(crate::ui::reactions::Reactors::new(ch, id)),
        Some(MenuPick::Resend) => app.resend(ch, id),
        Some(MenuPick::SaveFile) => {
            if let Some((url, name)) = voice
                && let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file()
            {
                app.net.send(Cmd::Download { url, path });
            }
        }
        Some(MenuPick::Configure) => app.configure_joins(chan.guild),
        Some(MenuPick::Profile(u)) => app.open_profile(u, chan.guild, at),
        Some(MenuPick::Delete) => app.delete_msg(ch, id, !shift),
        Some(MenuPick::Report) => app.open_report(ch, id),
        Some(MenuPick::CopyId) => ctx.copy_text(id.to_string()),
        None => {}
    }
    if picked || !open {
        app.menu = None;
    }
}

/// Compact read-only rendering used by pins and search results.
pub(crate) fn preview(ui: &mut Ui, store: &Store, m: &Message) -> egui::Response {
    let ctx = ui.ctx().clone();
    let guild = store.channels.get(&m.channel).map(|c| c.guild).unwrap_or(0);
    let revealed = Set::default();
    let mut editing = None;
    let settings = crate::cache::Settings::default();
    let none = Set::default();
    let mut invites = Default::default();
    let mut d = D {
        store,
        ctx: &ctx,
        guild,
        ch: m.channel,
        revealed: &revealed,
        editing: &mut editing,
        marker: None,
        hover_lock: None,
        bar: None,
        acts: Vec::new(),
        embeds: false,
        flash: None,
        rects: Vec::new(),
        s: &settings,
        quick: &[],
        moderator: false,
        popout: true,
        focus: None,
        bookmarks: &none,
        invites: &mut invites,
        unblocked: &none,
        image_menu: false,
        playing: None,
        frame_at: None,
    };
    // discord's cards: a raised fill and a hairline, the reply it answers above the author
    let r = Frame::new().fill(pal().bg_floating).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin { left: 12, right: 16, top: 12, bottom: 16 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if let Some(rid) = m.reply {
            let left = ui.max_rect().left();
            ui.horizontal(|ui| {
                ui.add_space(56.0);
                ui.vertical(|ui| reply_line(ui, &mut d, rid, left - 16.0));
            });
        }
        ui.horizontal_top(|ui| {
            let (av, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            if let Some(u) = store.user(m.author) {
                avatar(ui, av, &u.avatar_url());
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let color = store.role_color(guild, m.author).map(role_color).unwrap_or(pal().header_primary);
                    ui.label(RichText::new(store.name_in(guild, m.author)).font(medium(16.0)).color(color));
                    ui.label(RichText::new(stamp(m.ts())).font(medium(12.0)).color(pal().chat_muted));
                });
                let cx = Cx { store, ctx: &ctx, guild, size: 15.0, color: pal().text_normal, revealed: false, edited: m.edited, embed: false };
                let blocks = markdown::parse(&m.content, &cx);
                blocks_ui(ui, &blocks, &mut d, m.id);
                for a in &m.attachments {
                    ui.add_space(4.0);
                    if a.is_image() {
                        let sep = if a.proxy.contains('?') { '&' } else { '?' };
                        let (w, h) = fit(a.w, a.h, 550.0, 350.0, ui.available_width());
                        let url = format!("{}{sep}width={}&height={}", a.proxy, (w * 2.0) as u32, (h * 2.0) as u32);
                        media(ui, &mut d, &url, &a.url, w, h, None);
                    } else {
                        file_card(ui, &a.filename, a.size);
                    }
                }
            });
        });
    });
    for a in d.acts {
        if let Act::Url(u) = a {
            let _ = open::that_detached(u);
        }
    }
    let resp = r.response.interact(Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
