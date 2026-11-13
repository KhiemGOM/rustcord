use egui::{Align, Align2, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, button, channel_title, chat, icon_button, presence, profile, status_dot, text};
use super::icons::Icon;
use crate::net::Tag;
use crate::state::{Id, Message};
use crate::theme::*;
mod gifs;
use gifs::*;
pub(crate) use gifs::Gifs;
mod invite;
use invite::*;
pub(crate) use invite::MadeInvite;
mod switcher;
use switcher::*;
mod cards;
use cards::*;
pub(crate) use cards::dm_profile;
pub(super) use cards::{banner, shown, theme_top};
pub(super) use cards::theme_wash;
pub(crate) use cards::account;
mod lookup;
use lookup::*;
pub(crate) use lookup::search_panel;
pub(crate) use lookup::search_options;
pub(crate) use lookup::threads_menu;
mod share;
use share::*;
pub(crate) use share::Forward;
pub(crate) use share::NewThread;

pub struct Search {
    pub ch: Id,
    // 0 newest, 1 oldest, 2 most relevant
    pub sort: u8,
    pub scope: Id,
    pub guild: bool,
    pub query: String,
    pub results: Option<Vec<Message>>,
    pub total: u64,
}

impl App {
    pub fn start_search(&mut self, ch: Id, query: String) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        let (scope, guild) = if c.guild != 0 { (c.guild, true) } else { (ch, false) };
        let gid = c.guild;
        self.ui.searches.retain(|s| *s != query);
        self.ui.searches.insert(0, query.clone());
        self.ui.searches.truncate(5);
        let enc = |s: &str| -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect() };
        let user = |name: &str| {
            let name = name.trim_start_matches('@').to_lowercase();
            self.store.users.values().find(|u| u.username.to_lowercase() == name || u.display().to_lowercase() == name).map(|u| u.id)
        };
        let mut params = Vec::new();
        let mut words = Vec::new();
        for w in query.split_whitespace() {
            match w.split_once(':') {
                Some(("from", v)) if let Some(u) = user(v) => params.push(format!("author_id={u}")),
                Some(("mentions", v)) if let Some(u) = user(v) => params.push(format!("mentions={u}")),
                Some(("in", v)) => {
                    let v = v.trim_start_matches('#').to_lowercase();
                    let found = self.store.channels.values().find(|c| c.guild == gid && c.name.to_lowercase() == v);
                    match found {
                        Some(c) => params.push(format!("channel_id={}", c.id)),
                        None => words.push(w),
                    }
                }
                Some(("has", v)) if !v.is_empty() => params.push(format!("has={}", enc(v))),
                Some(("pinned", v @ ("true" | "false"))) => params.push(format!("pinned={v}")),
                Some(("author_type", v @ ("user" | "bot" | "webhook"))) => params.push(format!("author_type={v}")),
                Some((k @ ("before" | "after" | "during"), v)) if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") => {
                    let id = |d: chrono::NaiveDate| ((d.and_hms_opt(0, 0, 0).map_or(0, |t| t.and_utc().timestamp_millis()) - 1_420_070_400_000).max(0) as u64) << 22;
                    match k {
                        "before" => params.push(format!("max_id={}", id(d))),
                        "after" => params.push(format!("min_id={}", id(d.succ_opt().unwrap_or(d)))),
                        _ => params.push(format!("min_id={}&max_id={}", id(d), id(d.succ_opt().unwrap_or(d)))),
                    }
                }
                _ => words.push(w),
            }
        }
        let content = words.join(" ");
        if !content.is_empty() {
            params.push(format!("content={}", enc(&content)));
        }
        let sort = self.search.as_ref().map_or(0, |s| s.sort);
        params.push(format!("sort_by={}&sort_order={}", if sort == 2 { "relevance" } else { "timestamp" }, if sort == 1 { "asc" } else { "desc" }));
        let q = params.join("&");
        let path = if guild {
            format!("/guilds/{scope}/messages/search?{q}")
        } else if self.settings.search_all_dms {
            format!("/users/@me/messages/search?{q}")
        } else {
            format!("/channels/{scope}/messages/search?{q}")
        };
        self.rest("GET", path, None, Tag::Search(scope));
        self.search = Some(Search { ch, sort, scope, guild, query, results: None, total: 0 });
    }

    pub fn open_pins(&mut self, ch: Id) {
        self.pins = Some((ch, None));
        self.rest("GET", format!("/channels/{ch}/messages/pins?limit=50"), None, Tag::Pins(ch));
        // opening them is seeing them, which takes the dot off the pin button
        if self.store.pins_unread(ch) {
            let last = self.store.channels.get(&ch).map_or(0, |c| c.last_pin);
            self.store.reads.entry(ch).or_default().last_pin = last;
            self.rest("POST", format!("/channels/{ch}/pins/ack"), Some(serde_json::json!({})), Tag::Quiet);
        }
    }

    pub fn open_profile(&mut self, user: Id, guild: Id, at: Pos2) {
        self.profile = Some((user, guild, at));
        self.want_profile(user, guild, "with_mutual_guilds=true&with_mutual_friends=true");
    }

    pub fn open_timeout(&mut self, gid: Id, uid: Id) {
        self.timeout = Some(Timeout { guild: gid, user: uid, secs: 60, reason: String::new() });
    }

    pub fn kick(&mut self, gid: Id, uid: Id) {
        self.kickban = Some(super::kickban::Dialog::new(gid, uid, false));
    }

    pub fn ban(&mut self, gid: Id, uid: Id) {
        self.kickban = Some(super::kickban::Dialog::new(gid, uid, true));
    }

    pub fn open_dm(&mut self, user: Id) {
        self.dm_text(user, String::new());
    }

    /// Opens the dm with them, made first if there isn't one, and sends `text` there if it
    /// isn't empty: the profile card's message box.
    pub fn dm_text(&mut self, user: Id, text: String) {
        let existing = self.store.dms.iter().copied().find(|c| {
            self.store.channels.get(c).is_some_and(|c| c.kind == 1 && c.recipients == [user])
        });
        match existing {
            Some(c) => {
                self.ui.guild = None;
                self.open(c);
                if !text.is_empty() {
                    self.send_text(c, text);
                }
            }
            None => self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [user.to_string()] })), Tag::OpenDm(text)),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    gifs(app, ctx);
    super::profed::gif_arrived(app, ctx);
    forward(app, ctx);
    super::reactions::show(app, ctx);
    super::pollvotes::show(app, ctx);
    super::remind::show(app, ctx);
    super::report::show(app, ctx);
    invite(app, ctx);
    made_invite(app, ctx);
    status_edit(app, ctx);
    switcher(app, ctx);
    profile(app, ctx);
    super::profile::show(app, ctx);
    super::profile::role_picker(app, ctx);
    super::tagpop::show(app, ctx);
    super::modview::show(app, ctx);
    pins(app, ctx);
    new_thread(app, ctx);
    leaving(app, ctx);
    super::sourcepop::show(app, ctx);
    timeout(app, ctx);
    super::kickban::show(app, ctx);
    super::prune::show(app, ctx);
    super::poll::show(app, ctx);
    super::attach::modify(app, ctx);
    super::follow::show(app, ctx);
    super::dmpick::show(app, ctx);
    super::topic::show(app, ctx);
    super::feedabout::show(app, ctx);
    super::plus::launcher(app, ctx);
}

pub(super) fn dismissed(ctx: &egui::Context, area: &egui::Response) -> bool {
    ctx.input(|i| i.key_pressed(Key::Escape)) || pressed_outside(ctx, area)
}

/// A press outside where an area ended up. Not contains_pointer: egui hit-tests a constrained
/// area where it was asked to go, before it's pushed back on screen, so a popout opened low
/// down took every click in its top half for a click outside.
pub(super) fn pressed_outside(ctx: &egui::Context, area: &egui::Response) -> bool {
    ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.rect.contains(p)))
}

/// Setting the status line under a voice channel, which everyone in the server sees.
fn status_edit(app: &mut App, ctx: &egui::Context) {
    let Some((ch, text)) = &mut app.status_edit else { return };
    let (mut save, mut close) = (false, false);
    let modal = crate::ui::motion::Modal::new("status_edit".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(392.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Set a channel status").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(16.0);
            let (field, _) = ui.allocate_exact_size(vec2(392.0, 40.0), Sense::hover());
            ui.painter().rect(field, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let r = ui.put(
                field.shrink2(vec2(12.0, 10.0)),
                egui::TextEdit::singleline(text)
                    .hint_text(RichText::new("What's going on?").color(pal().text_muted))
                    .font(reg(16.0))
                    .char_limit(500)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO),
            );
            save = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            r.request_focus();
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                save |= button(ui, "Set Status", pal().blurple, 104.0).clicked();
                ui.add_space(8.0);
                close |= button(ui, "Cancel", pal().control, 88.0).clicked();
            });
        });
    if save {
        let c = *ch;
        let status = text.trim().to_string();
        app.rest("PUT", format!("/channels/{c}/voice-status"), Some(json!({ "status": status })), Tag::reason());
        close = true;
    }
    if close || modal.should_close() {
        app.status_edit = None;
    }
}

/// Full-window image viewer, like clicking an image in the official client.
pub fn viewer(app: &mut App, ctx: &egui::Context) {
    let Some((img, open, w, h)) = app.viewer.clone() else { return };
    // below the title bar, so the window buttons stay reachable
    let mut screen = ctx.content_rect();
    screen.min.y += super::titlebar::HEIGHT;
    let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
    let mut browse = false;
    // ←/→ step through the message's other pictures
    let step = ctx.input_mut(|i| if super::take_key(i, egui::Modifiers::NONE, Key::ArrowRight) { 1 } else if super::take_key(i, egui::Modifiers::NONE, Key::ArrowLeft) { -1 } else { 0 });
    let at = app.gallery.iter().position(|g| g.0 == img);
    if step != 0
        && let Some(i) = at
        && let Some(next) = app.gallery.get((i as i64 + step).rem_euclid(app.gallery.len() as i64) as usize).cloned()
    {
        app.viewer = Some(next);
        return;
    }
    let count = at.map(|i| (i + 1, app.gallery.len())).filter(|(_, n)| *n > 1);
    // a click on the picture zooms it, to its own size or twice the fitted one, and the
    // pointer pans it; another click anywhere zooms back out
    let zid = egui::Id::new(("viewer_zoom", &img));
    let mut zoomed = ctx.data(|d| d.get_temp::<bool>(zid)).unwrap_or(false);
    let pop = super::motion::enter(ctx, egui::Id::new("viewer"), 0.2, super::motion::POP);
    egui::Area::new(egui::Id::new("viewer"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .fade_in(false)
        .show(ctx, |ui| {
            let (bg, bg_resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(bg, CornerRadius::ZERO, egui::Color32::from_black_alpha(215).gamma_multiply(pop.clamp(0.0, 1.0)));
            ui.multiply_opacity(pop.clamp(0.0, 1.0));
            let (iw, ih) = (w.max(1) as f32, h.max(1) as f32);
            let s = (screen.width() * 0.8 / iw).min(screen.height() * 0.75 / ih).min(if w == 0 { 1.0 } else { 2.0 });
            let size = if w == 0 { vec2(screen.width() * 0.6, screen.height() * 0.6) } else { vec2(iw * s, ih * s) };
            let rect = Rect::from_center_size(screen.center() - vec2(0.0, 16.0), size * (0.9 + 0.1 * pop));
            let sized = |size: egui::Vec2| {
                let sep = if img.contains('?') { '&' } else { '?' };
                if w > 0 && !img.contains(".mp4") { format!("{img}{sep}width={}&height={}", (size.x * 1.5) as u32, (size.y * 1.5) as u32) } else { img.clone() }
            };
            let url = sized(size);
            let ir = ui.interact(rect, egui::Id::new("viewer_img"), Sense::click());
            app.rects.insert("viewer_img".into(), rect);
            if zoomed {
                let k = if w == 0 { 2.0 } else { (iw / ctx.pixels_per_point() / size.x).max(2.0) };
                let zs = size * k;
                let p = ui.input(|i| i.pointer.hover_pos()).unwrap_or(bg.center());
                let f = ((p - bg.min) / bg.size()).clamp(vec2(0.0, 0.0), vec2(1.0, 1.0));
                let pan = |lo: f32, len: f32, z: f32, f: f32| if z > len { lo - f * (z - len) } else { lo + (len - z) / 2.0 };
                let zr = Rect::from_min_size(pos2(pan(bg.left(), bg.width(), zs.x, f.x), pan(bg.top(), bg.height(), zs.y, f.y)), zs);
                let mut clip = ui.new_child(egui::UiBuilder::new().max_rect(bg));
                clip.set_clip_rect(bg);
                // the fitted copy stretched underneath while the big one loads
                egui::Image::from_uri(crate::gif::uri(ctx, &url, true)).show_loading_spinner(false).paint_at(&clip, zr);
                egui::Image::from_uri(crate::gif::uri(ctx, &sized(zs), true)).show_loading_spinner(false).paint_at(&clip, zr);
                if ui.rect_contains_pointer(bg) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomOut);
                }
                app.rects.insert("viewer_zoomed".into(), zr);
                if bg_resp.clicked() || ir.clicked() {
                    zoomed = false;
                }
                return;
            }
            egui::Image::from_uri(crate::gif::uri(ctx, &url, true)).show_loading_spinner(true).paint_at(ui, rect);
            if ir.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
            }
            if ir.clicked() {
                zoomed = true;
            }
            let link = pos2(rect.left(), rect.bottom() + 12.0);
            let lr = ui.painter().text(link, Align2::LEFT_TOP, "Open in Browser", semi(14.0), pal().header_secondary);
            let lresp = ui.interact(lr, egui::Id::new("viewer_link"), Sense::click());
            if lresp.hovered() {
                ui.painter().hline(lr.x_range(), lr.bottom(), egui::Stroke::new(1.0, pal().header_secondary));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            browse = lresp.clicked();
            if let Some((i, n)) = count {
                ui.painter().text(pos2(rect.right(), rect.bottom() + 12.0), Align2::RIGHT_TOP, format!("{i} / {n}"), semi(14.0), pal().header_secondary);
            }
            if bg_resp.clicked() && !ir.hovered() && !lresp.hovered() {
                close = true;
            }
        });
    ctx.data_mut(|d| d.insert_temp(zid, zoomed));
    if browse {
        let _ = open::that_detached(open);
    }
    if close || browse {
        app.viewer = None;
    }
}

/// Timeout for a member: who, where, how long and why.
pub struct Timeout {
    guild: Id,
    user: Id,
    secs: i64,
    reason: String,
}

/// Discord's Timeout dialog: what timeout means, the six lengths, a reason for the audit log.
pub fn timeout(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &mut app.timeout else { return };
    let name = app.store.name_in(t.guild, t.user);
    let (mut go, mut close) = (false, false);
    let modal = crate::ui::motion::Modal::new("timeout".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Timeout {name}")).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(8.0);
            let about = "Members who are in timeout are temporarily not allowed to chat or react in text channels. They are also not allowed to connect to voice or Stage channels.";
            ui.label(RichText::new(about).font(reg(16.0)).color(pal().text_normal));
            ui.add_space(20.0);
            ui.label(RichText::new("Duration").font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let lengths = [("60 secs", 60), ("5 mins", 300), ("10 mins", 600), ("1 hour", 3600), ("1 day", 86400), ("1 week", 604800)];
                for (i, (label, secs)) in lengths.into_iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(72.0, 36.0), Sense::click());
                    let cr = CornerRadius { nw: if i == 0 { 8 } else { 0 }, sw: if i == 0 { 8 } else { 0 }, ne: if i == 5 { 8 } else { 0 }, se: if i == 5 { 8 } else { 0 } };
                    let on = t.secs == secs;
                    ui.painter().rect(r, cr, if on { pal().blurple } else { crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp)) }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, semi(14.0), if on { egui::Color32::WHITE } else { pal().header_primary });
                    if resp.clicked() {
                        t.secs = secs;
                    }
                }
            });
            ui.add_space(20.0);
            ui.label(RichText::new("Reason").font(semi(14.0)).color(pal().header_primary));
            ui.add_space(8.0);
            input_frame().corner_radius(8).inner_margin(12).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut t.reason)
                        .frame(Frame::NONE)
                        .desired_rows(3)
                        .hint_text("Enter a reason. This will only be visible in the Audit Log and will not be shown to the member.")
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= super::grey_button(ui, "Cancel", 212.0).clicked();
                let t = button(ui, "Timeout", pal().blurple, 212.0);
                super::settings::mark(ui, "timeout:go", t.rect);
                go = t.clicked();
            });
        });
    go |= super::enter_submits(ctx, false);
    if go && let Some(t) = app.timeout.take() {
        app.time_out(t.guild, t.user, t.secs, &t.reason);
    } else if close || modal.should_close() {
        app.timeout = None;
    }
}

/// Closes a dm; a group asks first, since leaving it can't be undone.
pub fn leave_dm(app: &mut App, c: Id) {
    if app.store.channels.get(&c).is_none_or(|x| x.kind != 3) {
        return close_dm(app, c);
    }
    let title = channel_title(&app.store, c);
    let body = format!("Are you sure you want to leave {title}? You won't be able to rejoin this group unless you are re-invited.");
    app.confirm = Some(super::Confirm::new(format!("Leave '{title}'"), body, "Leave Group", move |app: &mut App| close_dm(app, c)));
}

pub fn close_dm(app: &mut App, c: Id) {
    let at = app.store.dms.iter().position(|d| *d == c);
    app.rest("DELETE", format!("/channels/{c}"), None, Tag::undo("Failed to close direct message", move |app: &mut App| {
        if let Some(at) = at {
            let n = app.store.dms.len();
            app.store.dms.insert(at.min(n), c);
        }
    }));
    app.store.dms.retain(|d| *d != c);
    if app.cur() == Some(c) {
        app.friends = true;
    }
}

/// Discord's own addresses, which never warn.
const DISCORD_HOSTS: [&str; 9] = [
    "discord.com", "discord.gg", "discordapp.com", "discordapp.net", "discord.media", "discord.new", "discord.gift", "discord.co", "discordstatus.com",
];

fn host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase()
}

impl App {
    /// A link clicked in chat. Discord's own message links open here unless a modifier sends
    /// them to the browser. Others go straight out when their text is their address and this
    /// isn't a dm with someone who isn't a friend; the rest ask "Leaving Discord" first,
    /// unless their site was trusted from that dialog before.
    pub fn open_link(&mut self, ctx: &egui::Context, url: &str, masked: bool) {
        let h = host(url);
        let ours = DISCORD_HOSTS.iter().any(|d| h == *d || h.ends_with(&format!(".{d}")));
        let modded = ctx.input(|i| i.modifiers.ctrl || i.modifiers.shift || i.modifiers.alt);
        if ours && !modded {
            let path: Vec<&str> = url.split("/channels/").nth(1).unwrap_or("").split(['?', '#']).next().unwrap_or("").split('/').collect();
            let ch = path.get(1).and_then(|c| c.parse::<Id>().ok()).filter(|c| self.store.channels.contains_key(c));
            if ch.is_none()
                && let (Some(g), Some(c)) = (path.first().and_then(|g| g.parse::<Id>().ok()), path.get(1).and_then(|c| c.parse::<Id>().ok()))
                && !self.store.guilds.contains_key(&g)
            {
                super::lurk::start(self, g, c);
                return;
            }
            if let Some(c) = ch {
                match path.get(2).and_then(|m| m.parse::<Id>().ok()) {
                    Some(m) => self.jump_to(c, m),
                    None => {
                        let g = self.store.channels.get(&c).map_or(0, |c| c.guild);
                        self.ui.guild = (g != 0).then_some(g);
                        self.open(c);
                    }
                }
                return;
            }
        }
        let stranger = self.cur().and_then(|c| self.store.channels.get(&c)).is_some_and(|c| {
            c.kind == 1 && c.recipients.first().is_some_and(|u| self.store.relationships.get(u) != Some(&crate::state::FRIEND))
        });
        let trusted = ours || self.settings.trusted.iter().any(|t| *t == h);
        if (masked || stranger) && !trusted {
            self.leaving = Some((url.to_string(), false));
        } else {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
    }
}

/// "Leaving Discord": the address with its site in bold, a box to trust that site from now on,
/// Go Back and Visit Site.
fn leaving(app: &mut App, ctx: &egui::Context) {
    let Some((url, trust)) = &mut app.leaving else { return };
    let h = host(url);
    let (mut go, mut close) = (false, false);
    let modal = crate::ui::motion::Modal::new("leaving".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Leaving Discord").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("This link is taking you to the following website").font(reg(16.0)).color(pal().text_normal));
            ui.add_space(16.0);
            // the site picked out in white, the rest of the address greyed round it
            let at = url.find(&h).unwrap_or(0);
            let shown = format!("{}**{}**{}", &url[..at], &url[at..at + h.len().min(url.len() - at)], &url[(at + h.len()).min(url.len())..]);
            Frame::new().fill(pal().bg_tertiary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(400.0);
                let mut job = super::bolded(&shown, 16.0, pal().text_normal, 400.0);
                job.wrap.break_anywhere = true;
                ui.label(ui.painter().layout_job(job));
            });
            ui.add_space(16.0);
            let (r, resp) = ui.allocate_exact_size(vec2(432.0, 24.0), Sense::click());
            let b = Rect::from_min_size(r.left_top(), vec2(24.0, 24.0));
            if *trust {
                ui.painter().rect_filled(b, 6, pal().blurple);
                super::icons::draw(ui.painter(), Icon::Check, b.shrink(4.0), egui::Color32::WHITE);
            } else {
                ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
            }
            let line = super::bolded(&format!("Trust **{h}** links from now on"), 16.0, pal().text_normal, 400.0);
            let g = ui.painter().layout_job(line);
            ui.painter().galley(pos2(b.right() + 12.0, r.center().y - g.size().y / 2.0), g, pal().text_normal);
            if resp.clicked() {
                *trust = !*trust;
            }
            app.rects.insert("leaving:trust".into(), b);
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close |= super::grey_button(ui, "Go Back", 212.0).clicked();
                let v = button(ui, "Visit Site", pal().blurple, 212.0);
                app.rects.insert("leaving:visit".into(), v.rect);
                go = v.clicked();
            });
        });
    go |= super::enter_submits(ctx, true);
    if go && let Some((url, trust)) = app.leaving.take() {
        if trust && !app.settings.trusted.contains(&h) {
            app.settings.trusted.push(h);
            crate::cache::set_settings(&app.settings);
        }
        ctx.open_url(egui::OpenUrl::new_tab(url));
        super::activity::link_done(app, true);
    } else if close || modal.should_close() {
        app.leaving = None;
        super::activity::link_done(app, false);
    }
}

