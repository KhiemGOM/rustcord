use super::*;

pub(super) fn invite(app: &mut App, ctx: &egui::Context) {
    crate::ui::invitecard::tick(app, ctx);
    let Some(text) = &mut app.invite else { return };
    let card = &app.invite_card;
    let mut join = None;
    let mut close = false;
    let mut discover = false;
    let mut fill = None;
    let modal = crate::ui::motion::Modal::new("invite".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Join a Server").font(semi(24.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(RichText::new("Enter an invite below to join an existing server").color(pal().header_secondary));
            });
            ui.add_space(16.0);
            card.draw(ui);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("Invite link").font(semi(14.0)).color(pal().header_secondary));
                ui.label(RichText::new("*").font(semi(14.0)).color(pal().critical));
            });
            ui.add_space(8.0);
            input_frame().corner_radius(3).inner_margin(10).show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(text)
                        .frame(Frame::NONE)
                        .hint_text("https://discord.gg/hTKzmak")
                        .desired_width(ui.available_width()),
                );
                r.request_focus();
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Invites should look like").font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for sample in ["hTKzmak", "https://discord.gg/hTKzmak", "https://discord.gg/wumpus-friends"] {
                    let g = ui.painter().layout_no_wrap(sample.into(), reg(14.0), pal().header_primary);
                    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(16.0, 12.0), Sense::click());
                    ui.painter().rect(r, 4, if resp.hovered() { pal().bg_mod_hover } else { pal().input_bg }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    ui.painter().galley(r.min + vec2(8.0, 6.0), g, pal().header_primary);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        fill = Some(sample);
                    }
                }
            });
            ui.add_space(16.0);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 68.0), Sense::click());
            crate::ui::settings::mark(ui, "invite:discover", r);
            ui.painter().rect(r, 8, if resp.hovered() { pal().bg_mod_hover } else { pal().input_bg }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let c = pos2(r.left() + 36.0, r.center().y);
            ui.painter().circle_filled(c, 18.0, pal().green);
            crate::ui::icons::draw(ui.painter(), Icon::Discover, Rect::from_center_size(c, vec2(20.0, 20.0)), egui::Color32::WHITE);
            ui.painter().text(pos2(r.left() + 68.0, r.center().y - 11.0), Align2::LEFT_CENTER, "Don't have an invite?", semi(16.0), pal().header_primary);
            let g = ui.painter().layout("Check out Discoverable communities in Server Discovery.".into(), reg(14.0), pal().text_normal, r.width() - 140.0);
            ui.painter().galley(pos2(r.left() + 68.0, r.center().y - 2.0), g, pal().text_normal);
            crate::ui::icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(20.0, 20.0)), pal().header_secondary);
            discover = resp.clicked();
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, "Join Server", pal().blurple, 120.0).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    join = Some(text.clone());
                }
                if ui.add(egui::Label::new(RichText::new("Back").color(pal().header_primary)).sense(Sense::click())).clicked() {
                    close = true;
                }
            });
        });
    if let Some(s) = fill {
        *text = s.to_string();
    }
    if discover {
        app.invite = None;
        app.open_servers();
        return;
    }
    if let Some(t) = join {
        let code = crate::ui::invitecard::code_of(&t);
        if !code.is_empty() {
            let session = app.store.session.clone();
            app.rest_in("POST", format!("/invites/{code}"), Some(json!({ "session_id": session })), "Join Guild", Tag::Invite);
        }
        close = true;
    }
    if close || modal.should_close() {
        app.invite = None;
    }
}

pub(crate) struct MadeInvite {
    pub ch: Id,
    pub code: Option<String>,
    pub(super) query: String,
    // rows whose Invite went out: a dm's channel, or a friend we had no dm with
    pub(super) sent: std::collections::HashSet<Id>,
}

#[derive(Clone, Copy)]
pub(super) enum Pick {
    Chan(Id),
    Friend(Id),
}

impl App {
    /// Invite People: a fresh link to the channel, good for 30 days like the official client's.
    pub fn invite_people(&mut self, ch: Id, source: &str) {
        // without the permission the dialog hands out the server's own link
        let chan = self.store.channels.get(&ch);
        let own = chan.filter(|c| self.store.perms(c) & crate::state::CREATE_INVITE == 0).and_then(|c| self.store.guilds.get(&c.guild)).and_then(|g| g.vanity.clone());
        self.made_invite = Some(MadeInvite { ch, code: own.clone(), query: String::new(), sent: Default::default() });
        if own.is_some() {
            return;
        }
        let body = Some(json!({ "max_age": 2_592_000, "max_uses": 0, "temporary": false }));
        if source.is_empty() {
            self.rest("POST", format!("/channels/{ch}/invites"), body, Tag::MadeInvite);
        } else {
            self.rest_in("POST", format!("/channels/{ch}/invites"), body, source, Tag::MadeInvite);
        }
    }
}

/// Discord's "Invite friends" dialog: dms and friends to send the link to, most recent first,
/// leaving out people we know are in the server already, then the link to copy. Measured from
/// discord's: 480 wide, 24 in, 48 tall rows.
pub(super) fn made_invite(app: &mut App, ctx: &egui::Context) {
    let Some(inv) = &mut app.made_invite else { return };
    let store = &app.store;
    let Some(chan) = store.channels.get(&inv.ch) else { return };
    let server = store.guilds.get(&chan.guild).map(|g| g.name.clone()).unwrap_or_default();
    let link = inv.code.as_ref().map(|c| format!("https://discord.gg/{c}"));
    let members = store.members.get(&chan.guild);
    let here = |u: Id| members.is_some_and(|m| m.contains_key(&u));
    let ql = inv.query.to_lowercase();
    let hit = |s: &str| ql.is_empty() || s.to_lowercase().contains(&ql);
    let mut rows = Vec::new();
    let mut have_dm = std::collections::HashSet::new();
    for &c in &store.dms {
        let Some(ch) = store.channels.get(&c) else { continue };
        if ch.kind == 1 {
            let Some(u) = ch.recipients.first().and_then(|u| store.user(*u)) else { continue };
            have_dm.insert(u.id);
            if !u.bot && !here(u.id) && (hit(u.display()) || hit(&u.username)) {
                rows.push(Pick::Chan(c));
            }
        } else if ch.kind == 3 && hit(&channel_title(store, c)) {
            rows.push(Pick::Chan(c));
        }
    }
    let mut friends: Vec<&crate::state::User> = store
        .relationships
        .iter()
        .filter(|(u, t)| **t == crate::state::FRIEND && !have_dm.contains(*u) && !here(**u))
        .filter_map(|(u, _)| store.user(*u))
        .filter(|u| hit(u.display()) || hit(&u.username))
        .collect();
    friends.sort_by_key(|u| u.display().to_lowercase());
    rows.extend(friends.into_iter().map(|u| Pick::Friend(u.id)));

    let (mut copy, mut close, mut send) = (false, false, None);
    let modal = crate::ui::motion::Modal::new("made_invite".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 24, bottom: 0 }).show(ui, |ui| {
                ui.set_width(432.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Invite friends to {server}")).font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                let (line, _) = ui.allocate_exact_size(vec2(432.0, 24.0), Sense::hover());
                let r = text::paint(ui.painter(), pos2(line.left(), line.center().y), Align2::LEFT_CENTER, "Recipients will land in", reg(16.0), pal().header_secondary, 300.0);
                let icon = Rect::from_center_size(pos2(r.right() + 13.0, line.center().y), vec2(16.0, 16.0));
                crate::ui::side::chan_icon(store, ui.painter(), chan, icon, pal().header_secondary);
                text::paint(ui.painter(), pos2(icon.right() + 2.0, line.center().y), Align2::LEFT_CENTER, &chan.name, reg(16.0), pal().header_secondary, 400.0 - r.width());
                ui.add_space(23.0);
                let (field, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect_filled(field, 8, pal().input_bg);
                let inner = Rect::from_min_max(pos2(field.left() + 38.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
                let r = ui.put(
                    inner,
                    egui::TextEdit::singleline(&mut inv.query)
                        .hint_text(RichText::new("Search for friends").color(pal().text_muted))
                        .font(reg(16.0))
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO),
                );
                ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, if r.has_focus() { pal().blurple } else { pal().border_subtle }), egui::StrokeKind::Inside);
                crate::ui::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 20.0, field.center().y), vec2(16.0, 16.0)), pal().header_primary);
                ui.advance_cursor_after_rect(field);
                // discord's warning circle: yellow with the "!" cut out of it
                if store.locked(chan) {
                    ui.add_space(8.0);
                    let (line, _) = ui.allocate_exact_size(vec2(432.0, 16.0), Sense::hover());
                    let c = pos2(line.left() + 6.0, line.center().y);
                    ui.painter().circle_filled(c, 6.0, pal().yellow);
                    ui.painter().rect_filled(Rect::from_center_size(c - vec2(0.0, 1.25), vec2(1.6, 4.5)), 1, pal().bg_secondary);
                    ui.painter().circle_filled(c + vec2(0.0, 2.75), 0.9, pal().bg_secondary);
                    let note = "This channel is private, only select members and roles can view this channel.";
                    text::paint(ui.painter(), pos2(line.left() + 20.0, line.center().y), Align2::LEFT_CENTER, note, reg(12.0), pal().text_normal, 412.0);
                }
                ui.add_space(20.0);
            });
            let room = (ui.ctx().content_rect().height() - 330.0).clamp(144.0, 480.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 0)).show(ui, |ui| {
                    if link.is_none() {
                        ui.add_space(24.0);
                        ui.label(RichText::new("Fetching your friends list...").font(semi(16.0)).color(pal().header_primary));
                        ui.add_space(24.0);
                    }
                    for &p in rows.iter().filter(|_| link.is_some()) {
                        let (key, ch) = match p {
                            Pick::Chan(c) => (c, store.channels.get(&c)),
                            Pick::Friend(u) => (u, None),
                        };
                        let user = match (p, ch) {
                            (Pick::Friend(u), _) => store.user(u),
                            (_, Some(c)) if c.kind == 1 => c.recipients.first().and_then(|u| store.user(*u)),
                            _ => None,
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(432.0, 48.0), Sense::hover());
                        let pic = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(32.0, 32.0));
                        match (user, ch) {
                            (Some(u), _) => avatar(ui, pic, &u.avatar_url()),
                            (None, Some(c)) => match &c.icon {
                                Some(i) => {
                                    let url = format!("{}/channel-icons/{}/{i}.png?size=64", crate::api::cdn(), c.id);
                                    egui::Image::from_uri(url).corner_radius(CornerRadius::same(16)).show_loading_spinner(false).paint_at(ui, pic);
                                }
                                None => {
                                    ui.painter().circle_filled(pic.center(), 16.0, pal().blurple);
                                    crate::ui::icons::draw(ui.painter(), Icon::People, pic.shrink(8.0), egui::Color32::WHITE);
                                }
                            },
                            _ => {}
                        }
                        let (name, sub) = match user {
                            Some(u) => (u.display().to_string(), Some(u.username.clone())),
                            None => (channel_title(store, key), None),
                        };
                        let y = if sub.is_some() { r.center().y - 9.0 } else { r.center().y };
                        text::paint(ui.painter(), pos2(pic.right() + 10.0, y), Align2::LEFT_CENTER, &name, semi(16.0), pal().text_normal, 300.0);
                        if let Some(sub) = sub {
                            text::paint(ui.painter(), pos2(pic.right() + 10.0, r.center().y + 9.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().text_muted, 300.0);
                        }
                        let b = Rect::from_min_size(pos2(r.right() - 62.0, r.center().y - 16.0), vec2(60.0, 32.0));
                        let done = inv.sent.contains(&key);
                        let resp = ui.interact(b, ui.id().with(("invite_send", key)), if done || link.is_none() { Sense::hover() } else { Sense::click() });
                        if !done {
                            let fill = if resp.hovered() && link.is_some() { pal().control_hover } else { pal().control };
                            ui.painter().rect(b, 8, fill, egui::Stroke::new(1.0, pal().border_normal), egui::StrokeKind::Inside);
                        }
                        let label = if done { "Sent" } else { "Invite" };
                        ui.painter().text(b.center(), Align2::CENTER_CENTER, label, medium(14.0), if done { pal().text_muted } else { pal().header_primary });
                        let tag = match p {
                            Pick::Chan(c) => format!("invite:chan:{c}"),
                            Pick::Friend(u) => format!("invite:friend:{u}"),
                        };
                        app.rects.insert(tag, b);
                        if resp.clicked() {
                            send = Some(p);
                        }
                    }
                    if rows.is_empty() && link.is_some() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No results found").color(pal().header_secondary)));
                        ui.add_space(24.0);
                    }
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(480.0, 1.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, pal().border_subtle));
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 24 }).show(ui, |ui| {
                ui.set_width(432.0);
                ui.label(RichText::new("Or, send a server invite link to a friend").font(semi(16.0)).color(pal().header_primary));
                ui.add_space(12.0);
                let (field, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect(field, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                let b = Rect::from_min_size(pos2(field.right() - 78.0, field.center().y - 16.0), vec2(72.0, 32.0));
                let shown = link.clone().unwrap_or_else(|| "Generating link…".into());
                text::paint(ui.painter(), pos2(field.left() + 10.0, field.center().y), Align2::LEFT_CENTER, &shown, reg(16.0), pal().text_normal, b.left() - field.left() - 18.0);
                let resp = ui.interact(b, ui.id().with("invite_copy"), Sense::click());
                ui.painter().rect_filled(b, 8, crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
                ui.painter().text(b.center(), Align2::CENTER_CENTER, "Copy", medium(14.0), egui::Color32::WHITE);
                copy = link.is_some() && resp.clicked();
                ui.add_space(14.0);
                ui.label(RichText::new("Your invite link expires in 30 days.").font(reg(12.0)).color(pal().text_muted));
            });
        });
    if copy && let Some(l) = &link {
        ctx.copy_text(l.clone());
        app.toast("Invite link copied.");
    }
    // the link goes out as a dm, made first for a friend there's no dm with yet
    if let (Some(p), Some(l)) = (send, link) {
        let key = match p {
            Pick::Chan(c) => {
                app.rest("POST", format!("/channels/{c}/messages"), Some(json!({ "content": l })), Tag::fail(crate::ui::SEND_FAILED));
                c
            }
            Pick::Friend(u) => {
                app.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [u.to_string()] })), Tag::InviteDm(l));
                u
            }
        };
        if let Some(inv) = &mut app.made_invite {
            inv.sent.insert(key);
        }
    }
    if close || modal.should_close() {
        app.made_invite = None;
    }
}
