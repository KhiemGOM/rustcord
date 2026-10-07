use egui::{Align, Align2, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, button, channel_title, chat, icon_button, status_dot, text};
use super::icons::Icon;
use crate::net::Tag;
use crate::state::{Id, Message, snowflake_ms};
use crate::theme::*;

pub struct Search {
    pub guild: bool,
    pub query: String,
    pub results: Option<Vec<Message>>,
    pub total: u64,
}

impl App {
    pub fn start_search(&mut self, ch: Id, query: String) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        let (scope, guild) = if c.guild != 0 { (c.guild, true) } else { (ch, false) };
        let q: String = query
            .bytes()
            .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect();
        let path = if guild {
            format!("/guilds/{scope}/messages/search?content={q}")
        } else {
            format!("/channels/{scope}/messages/search?content={q}")
        };
        self.rest("GET", path, None, Tag::Search(scope));
        self.search = Some(Search { guild, query, results: None, total: 0 });
    }

    pub fn open_pins(&mut self, ch: Id) {
        self.pins = Some((ch, None));
        self.rest("GET", format!("/channels/{ch}/pins"), None, Tag::Pins(ch));
    }

    pub fn open_profile(&mut self, user: Id, guild: Id, at: Pos2) {
        self.profile = Some((user, guild, at));
        if !self.profiles.contains_key(&user) {
            let q = if guild != 0 { format!("&guild_id={guild}") } else { String::new() };
            self.rest("GET", format!("/users/{user}/profile?with_mutual_guilds=false{q}"), None, Tag::Profile(user));
        }
    }

    pub fn open_dm(&mut self, user: Id) {
        let existing = self.store.dms.iter().copied().find(|c| {
            self.store.channels.get(c).is_some_and(|c| c.kind == 1 && c.recipients == [user])
        });
        match existing {
            Some(c) => {
                self.ui.guild = None;
                self.open(c);
            }
            None => self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [user.to_string()] })), Tag::OpenDm),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    invite(app, ctx);
    switcher(app, ctx);
    profile(app, ctx);
    pins(app, ctx);
}

fn dismissed(ctx: &egui::Context, area: &egui::Response) -> bool {
    ctx.input(|i| i.key_pressed(Key::Escape)) || (ctx.input(|i| i.pointer.any_pressed()) && !area.contains_pointer())
}

fn invite(app: &mut App, ctx: &egui::Context) {
    let Some(text) = &mut app.invite else { return };
    let mut join = None;
    let mut close = false;
    let modal = egui::Modal::new("invite".into())
        .frame(Frame::new().fill(BG_PRIMARY).corner_radius(8).inner_margin(16))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Join a Server").font(bold(24.0)).color(HEADER_PRIMARY));
                ui.add_space(8.0);
                ui.label(RichText::new("Enter an invite below to join an existing server").color(HEADER_SECONDARY));
            });
            ui.add_space(16.0);
            ui.label(RichText::new("INVITE LINK").font(bold(12.0)).color(HEADER_SECONDARY));
            ui.add_space(8.0);
            Frame::new().fill(BG_TERTIARY).corner_radius(3).inner_margin(10).show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(text)
                        .frame(Frame::NONE)
                        .hint_text("https://discord.gg/hTKzmak")
                        .desired_width(ui.available_width()),
                );
                r.request_focus();
            });
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, "Join Server", BLURPLE, 120.0).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    join = Some(text.clone());
                }
                if ui.add(egui::Label::new(RichText::new("Back").color(HEADER_PRIMARY)).sense(Sense::click())).clicked() {
                    close = true;
                }
            });
        });
    if let Some(t) = join {
        let code = t.trim().trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
        if !code.is_empty() {
            app.rest("POST", format!("/invites/{code}"), Some(json!({})), Tag::Invite);
        }
        close = true;
    }
    if close || modal.should_close() {
        app.invite = None;
    }
}

enum Hit {
    Channel(Id),
    Guild(Id),
}

fn switcher(app: &mut App, ctx: &egui::Context) {
    let Some(q) = &mut app.switcher else { return };
    let sel_id = egui::Id::new("switcher_sel");
    let mut sel: usize = ctx.data(|d| d.get_temp(sel_id).unwrap_or(0));
    let ql = q.to_lowercase();
    let store = &app.store;

    let mut hits: Vec<(Hit, String, String)> = Vec::new();
    for &c in &store.dms {
        let t = channel_title(store, c);
        if t.to_lowercase().contains(&ql) {
            hits.push((Hit::Channel(c), format!("@{t}"), String::new()));
        }
    }
    for gid in &store.guild_order {
        let Some(g) = store.guilds.get(gid) else { continue };
        if !ql.is_empty() && g.name.to_lowercase().contains(&ql) {
            hits.push((Hit::Guild(*gid), g.name.clone(), String::new()));
        }
        for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
            if (c.is_text() || c.is_forum()) && c.name.to_lowercase().contains(&ql) && store.can_view(c) && !c.is_thread() {
                hits.push((Hit::Channel(c.id), format!("# {}", c.name), g.name.to_uppercase()));
            }
        }
    }
    // recent channels first when nothing is typed
    if ql.is_empty() {
        let recent = &app.ui.recent;
        hits.retain(|(h, ..)| matches!(h, Hit::Channel(c) if recent.contains(c)));
        hits.sort_by_key(|(h, ..)| match h {
            Hit::Channel(c) => recent.iter().position(|r| r == c).unwrap_or(99),
            Hit::Guild(_) => 99,
        });
    }
    hits.truncate(10);
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        sel = (sel + 1).min(hits.len().saturating_sub(1));
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        sel = sel.saturating_sub(1);
    }
    sel = sel.min(hits.len().saturating_sub(1));
    let mut pick = ctx.input(|i| i.key_pressed(Key::Enter)).then_some(sel);

    let modal = egui::Modal::new("switcher".into())
        .frame(Frame::new().fill(BG_SECONDARY).corner_radius(8).inner_margin(20))
        .show(ctx, |ui| {
            ui.set_width(530.0);
            Frame::new().fill(BG_TERTIARY).corner_radius(4).inner_margin(Margin::symmetric(12, 12)).show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(q)
                        .frame(Frame::NONE)
                        .font(reg(20.0))
                        .hint_text(RichText::new("Where would you like to go?").color(TEXT_MUTED))
                        .desired_width(ui.available_width()),
                );
                r.request_focus();
            });
            ui.add_space(12.0);
            if hits.is_empty() {
                ui.label(RichText::new("Can't find what you're looking for?").color(TEXT_MUTED));
            }
            for (i, (_, label, sub)) in hits.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                if i == sel || resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_SELECTED);
                }
                text::paint(ui.painter(), pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, semi(16.0), HEADER_PRIMARY, r.width() - 160.0);
                ui.painter().text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, sub, semi(12.0), TEXT_MUTED);
                if resp.clicked() {
                    pick = Some(i);
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("PROTIP: Ctrl+K opens this from anywhere").font(semi(12.0)).color(GREEN));
        });
    ctx.data_mut(|d| d.insert_temp(sel_id, sel));
    if let Some(i) = pick.filter(|i| *i < hits.len()) {
        match hits[i].0 {
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

fn profile(app: &mut App, ctx: &egui::Context) {
    let Some((uid, gid, at)) = app.profile else { return };
    let Some(user) = app.store.user(uid).cloned() else {
        app.profile = None;
        return;
    };
    let p = app.profiles.get(&uid);
    let accent = p
        .and_then(|p| p["user_profile"]["accent_color"].as_u64().or(p["user"]["accent_color"].as_u64()))
        .map(|c| role_color(c as u32))
        .unwrap_or(BLURPLE);
    let bio = p.and_then(|p| p["user_profile"]["bio"].as_str().or(p["user"]["bio"].as_str())).unwrap_or("").to_string();
    let banner = p.and_then(|p| p["user"]["banner"].as_str()).map(|b| format!("{}/banners/{uid}/{b}.png?size=600", crate::api::cdn()));
    let roles: Vec<(String, u32)> = app
        .store
        .members
        .get(&gid)
        .and_then(|m| m.get(&uid))
        .zip(app.store.guilds.get(&gid))
        .map(|(m, g)| {
            let mut r: Vec<_> = m.roles.iter().filter_map(|r| g.roles.get(r)).collect();
            r.sort_by_key(|r| -r.position);
            r.into_iter().map(|r| (r.name.clone(), r.color)).collect()
        })
        .unwrap_or_default();
    let name = app.store.name_in(gid, uid);
    let status = app.store.status(uid);
    let me = app.store.me;
    let mut message = false;

    let area = egui::Area::new(egui::Id::new("profile"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(BG_TERTIARY).corner_radius(8).show(ui, |ui| {
                ui.set_width(300.0);
                let (top, _) = ui.allocate_exact_size(vec2(300.0, 60.0), Sense::hover());
                ui.painter().rect_filled(top, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, accent);
                if let Some(b) = &banner {
                    egui::Image::from_uri(b.clone())
                        .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
                        .show_loading_spinner(false)
                        .paint_at(ui, top);
                }
                let av = Rect::from_min_size(pos2(top.left() + 16.0, top.bottom() - 40.0), vec2(80.0, 80.0));
                ui.painter().circle_filled(av.center(), 46.0, BG_TERTIARY);
                avatar(ui, av, &user.avatar_url());
                status_dot(ui, av, status, BG_TERTIARY);
                ui.add_space(48.0);
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    text::label(ui, &name, bold(20.0), HEADER_PRIMARY, Sense::hover());
                    ui.label(RichText::new(&user.username).font(reg(14.0)).color(HEADER_PRIMARY));
                    ui.add_space(8.0);
                    Frame::new().fill(BG_SECONDARY_ALT).corner_radius(8).inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 4.0;
                        if !bio.is_empty() {
                            ui.label(RichText::new("ABOUT ME").font(bold(12.0)).color(HEADER_PRIMARY));
                            ui.label(RichText::new(&bio).font(reg(14.0)).color(TEXT_NORMAL));
                            ui.add_space(8.0);
                        }
                        ui.label(RichText::new("DISCORD MEMBER SINCE").font(bold(12.0)).color(HEADER_PRIMARY));
                        let since = chrono::DateTime::from_timestamp_millis(snowflake_ms(uid)).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
                        ui.label(RichText::new(since).font(reg(14.0)).color(TEXT_NORMAL));
                        if !roles.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new(if roles.len() == 1 { "ROLE" } else { "ROLES" }).font(bold(12.0)).color(HEADER_PRIMARY));
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                for (r, c) in &roles {
                                    Frame::new().fill(BG_PRIMARY).corner_radius(4).inner_margin(Margin::symmetric(6, 3)).show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            let (d, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                            let col = if *c == 0 { GREY } else { role_color(*c) };
                                            ui.painter().circle_filled(d.center(), 6.0, col);
                                            ui.label(RichText::new(r).font(reg(12.0)).color(TEXT_NORMAL));
                                        });
                                    });
                                }
                            });
                        }
                    });
                    if uid != me {
                        ui.add_space(10.0);
                        if button(ui, &format!("Message @{name}"), BLURPLE, ui.available_width()).clicked() {
                            message = true;
                        }
                    }
                });
            });
        });
    if message {
        app.profile = None;
        app.open_dm(uid);
    } else if dismissed(ctx, &area.response) {
        app.profile = None;
    }
}

fn pins(app: &mut App, ctx: &egui::Context) {
    let Some((ch, list)) = &app.pins else { return };
    let ch = *ch;
    let at = app.rects.get("pins_button").map(|r| r.left_bottom() + vec2(-380.0, 8.0)).unwrap_or(pos2(400.0, 60.0));
    let mut jump = None;
    let area = egui::Area::new(egui::Id::new("pins"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(BG_SECONDARY).corner_radius(8).show(ui, |ui| {
                ui.set_width(420.0);
                Frame::new().fill(BG_TERTIARY).corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }).inner_margin(16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Pinned Messages").font(bold(16.0)).color(HEADER_PRIMARY));
                });
                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    Frame::new().inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match list {
                            None => {
                                ui.add(egui::Spinner::new().color(TEXT_MUTED));
                            }
                            Some(l) if l.is_empty() => {
                                ui.add_space(24.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("This channel doesn't have any pinned messages... yet.").color(HEADER_SECONDARY));
                                });
                                ui.add_space(24.0);
                            }
                            Some(l) => {
                                for m in l {
                                    if chat::preview(ui, &app.store, m).clicked() {
                                        jump = Some(m.id);
                                    }
                                    ui.add_space(8.0);
                                }
                            }
                        }
                    });
                });
            });
        });
    if let Some(id) = jump {
        app.pins = None;
        app.jump_to(ch, id);
    } else if dismissed(ctx, &area.response) && !app.rects.get("pins_button").is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.contains(p))) {
        app.pins = None;
    }
}

pub fn search_panel(app: &mut App, ui: &mut Ui) {
    let Some(s) = &app.search else { return };
    let mut close = false;
    let mut jump = None;
    let header = match &s.results {
        None => "Searching…".to_string(),
        Some(_) => format!("{} Results for “{}”", s.total, s.query),
    };
    Frame::new().fill(BG_SECONDARY).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(header).font(semi(16.0)).color(HEADER_PRIMARY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                    close = true;
                }
            });
        });
    });
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &s.results {
                None => {
                    ui.add(egui::Spinner::new().color(TEXT_MUTED));
                }
                Some(list) if list.is_empty() => {
                    ui.label(RichText::new("We searched far and wide. Unfortunately, no results were found.").color(HEADER_SECONDARY));
                }
                Some(list) => {
                    let mut last = 0;
                    for m in list {
                        if s.guild && m.channel != last {
                            last = m.channel;
                            let name = app.store.channels.get(&m.channel).map(|c| c.name.clone()).unwrap_or_default();
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("# {name}")).font(semi(14.0)).color(HEADER_SECONDARY));
                            ui.add_space(4.0);
                        }
                        if chat::preview(ui, &app.store, m).clicked() {
                            jump = Some((m.channel, m.id));
                        }
                        ui.add_space(8.0);
                    }
                }
            }
        });
    });
    if close {
        app.search = None;
    }
    if let Some((c, id)) = jump {
        app.jump_to(c, id);
    }
}

pub fn threads_menu(app: &mut App, button: &egui::Response, ch: Id) {
    let mut open = None;
    egui::Popup::menu(button).show(|ui| {
        ui.set_width(320.0);
        ui.label(RichText::new("Threads").font(bold(16.0)).color(HEADER_PRIMARY));
        ui.add_space(8.0);
        let mut list: Vec<_> = app
            .store
            .channels
            .values()
            .filter(|c| c.is_thread() && c.parent == ch && !c.archived)
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.last_msg.max(c.id)));
        if list.is_empty() {
            ui.label(RichText::new("There are no active threads.").color(HEADER_SECONDARY));
        }
        for c in list {
            let (r, resp) = ui.allocate_exact_size(vec2(320.0, 44.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_HOVER);
            }
            text::paint(ui.painter(), pos2(r.left() + 8.0, r.top() + 13.0), Align2::LEFT_CENTER, &c.name, semi(15.0), HEADER_PRIMARY, r.width() - 16.0);
            let sub = format!("{} messages{}", c.msg_count, if c.joined { " · joined" } else { "" });
            ui.painter().text(pos2(r.left() + 8.0, r.top() + 32.0), Align2::LEFT_CENTER, sub, reg(12.0), TEXT_MUTED);
            if resp.clicked() {
                open = Some(c.id);
            }
        }
    });
    if let Some(c) = open {
        app.open(c);
    }
}

/// Full-window image viewer, like clicking an image in the official client.
pub fn viewer(app: &mut App, ctx: &egui::Context) {
    let Some((img, open, w, h)) = app.viewer.clone() else { return };
    let screen = ctx.content_rect();
    let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
    let mut browse = false;
    egui::Area::new(egui::Id::new("viewer"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (bg, bg_resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(bg, CornerRadius::ZERO, egui::Color32::from_black_alpha(215));
            let (iw, ih) = (w.max(1) as f32, h.max(1) as f32);
            let s = (screen.width() * 0.8 / iw).min(screen.height() * 0.75 / ih).min(if w == 0 { 1.0 } else { 2.0 });
            let size = if w == 0 { vec2(screen.width() * 0.6, screen.height() * 0.6) } else { vec2(iw * s, ih * s) };
            let rect = Rect::from_center_size(screen.center() - vec2(0.0, 16.0), size);
            let sep = if img.contains('?') { '&' } else { '?' };
            let url = if w > 0 { format!("{img}{sep}width={}&height={}", (size.x * 1.5) as u32, (size.y * 1.5) as u32) } else { img.clone() };
            let ir = ui.interact(rect, egui::Id::new("viewer_img"), Sense::click());
            egui::Image::from_uri(url).show_loading_spinner(true).paint_at(ui, rect);
            let link = pos2(rect.left(), rect.bottom() + 12.0);
            let lr = ui.painter().text(link, Align2::LEFT_TOP, "Open in Browser", semi(14.0), HEADER_SECONDARY);
            let lresp = ui.interact(lr, egui::Id::new("viewer_link"), Sense::click());
            if lresp.hovered() {
                ui.painter().hline(lr.x_range(), lr.bottom(), egui::Stroke::new(1.0, HEADER_SECONDARY));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            browse = lresp.clicked();
            if bg_resp.clicked() && !ir.hovered() && !lresp.hovered() {
                close = true;
            }
        });
    if browse {
        let _ = open::that_detached(open);
    }
    if close || browse {
        app.viewer = None;
    }
}
