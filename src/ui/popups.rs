use egui::{Align, Align2, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::json;

use super::{App, avatar, button, channel_title, chat, icon_button, status_dot, text};
use super::icons::Icon;
use crate::net::Tag;
use crate::plugins;
use crate::state::{Id, Message};
use crate::theme::*;

pub struct Search {
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
                _ => words.push(w),
            }
        }
        let content = words.join(" ");
        if !content.is_empty() {
            params.push(format!("content={}", enc(&content)));
        }
        let q = params.join("&");
        let path = if guild {
            format!("/guilds/{scope}/messages/search?{q}")
        } else {
            format!("/channels/{scope}/messages/search?{q}")
        };
        self.rest("GET", path, None, Tag::Search(scope));
        self.search = Some(Search { scope, guild, query, results: None, total: 0 });
    }

    pub fn open_pins(&mut self, ch: Id) {
        self.pins = Some((ch, None));
        self.rest("GET", format!("/channels/{ch}/pins"), None, Tag::Pins(ch));
        // opening them is seeing them, which takes the dot off the pin button
        if self.store.pins_unread(ch) {
            let last = self.store.channels.get(&ch).map_or(0, |c| c.last_pin);
            self.store.reads.entry(ch).or_default().last_pin = last;
            self.rest("POST", format!("/channels/{ch}/pins/ack"), Some(serde_json::json!({})), Tag::Quiet);
        }
    }

    pub fn open_profile(&mut self, user: Id, guild: Id, at: Pos2) {
        self.profile = Some((user, guild, at));
        if !self.profiles.contains_key(&user) {
            let q = if guild != 0 { format!("&guild_id={guild}") } else { String::new() };
            self.rest("GET", format!("/users/{user}/profile?with_mutual_guilds=true&with_mutual_friends=true{q}"), None, Tag::Profile(user));
        }
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

pub struct Gifs {
    pub query: String,
    pub sent: String,
    pub results: Option<Vec<Gif>>,
    pub at: Pos2,
    // discord's opening view: search terms over a preview each, and a trending preview
    pub cats: Option<Vec<(String, String)>>,
    pub trend: Option<String>,
    pub trending: bool,
    // the starred ones, opened from their tile
    pub favorites: bool,
}

/// Discord's gifs come from Klipy now.
const GIF_Q: &str = "provider=klipy&locale=en-US&media_format=gif";

pub struct Gif {
    pub url: String,
    pub src: String,
    pub w: f32,
    pub h: f32,
}

impl App {
    pub fn open_gifs(&mut self, at: Pos2) {
        self.gifs = Some(Gifs { query: String::new(), sent: String::new(), results: None, at, cats: None, trend: None, trending: false, favorites: crate::plugins::on("BetterGifPicker") });
        self.rest("GET", format!("/gifs/trending?{GIF_Q}"), None, Tag::GifCats);
    }

    /// What /gif and the picker's box search for.
    pub fn search_gifs(&mut self, q: String) {
        let Some(g) = &mut self.gifs else { return };
        g.query = q.clone();
        g.sent = q.clone();
        g.results = None;
        g.favorites = false;
        if q.is_empty() {
            g.trending = false;
        } else {
            let enc: String = q.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            self.rest("GET", format!("/gifs/search?q={enc}&{GIF_Q}"), None, Tag::Gifs(q));
        }
    }

    pub fn set_gif_cats(&mut self, v: &serde_json::Value) {
        let Some(g) = &mut self.gifs else { return };
        let cats = v["categories"].as_array().into_iter().flatten();
        g.cats = Some(cats.filter_map(|c| Some((c["name"].as_str()?.to_string(), c["src"].as_str()?.to_string()))).collect());
        g.trend = v["gifs"][0]["src"].as_str().or(v["gifs"][0]["gif_src"].as_str()).map(str::to_string);
    }

    pub fn set_gifs(&mut self, q: String, v: &serde_json::Value) {
        let Some(g) = &mut self.gifs else { return };
        if g.sent != q {
            return; // an older search answered late
        }
        let list = if v.is_array() { v } else { &v["gifs"] };
        g.results = Some(
            list.as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| {
                    Some(Gif {
                        url: x["url"].as_str()?.to_string(),
                        src: x["src"].as_str().or(x["gif_src"].as_str())?.to_string(),
                        w: x["width"].as_f64().unwrap_or(200.0) as f32,
                        h: x["height"].as_f64().unwrap_or(150.0) as f32,
                    })
                })
                .collect(),
        );
    }
}

fn gifs(app: &mut App, ctx: &egui::Context) {
    let autoplay = app.settings.gif_autoplay;
    let favs: Vec<Gif> = app.settings.fav_gifs.iter().map(|(url, src, w, h)| Gif { url: url.clone(), src: src.clone(), w: *w, h: *h }).collect();
    let Some(g) = &mut app.gifs else { return };
    let mut pick = None;
    let mut search = None;
    let mut trending = false;
    let mut back = false;
    let mut star = None;
    let mut open_favs = false;
    let home = g.query.is_empty() && !g.trending && !g.favorites;
    let mut cat_rects = Vec::new();
    let (area, to) = super::picker::shell(ctx, g.at, Align2::RIGHT_BOTTOM, Some(super::picker::Tab::Gifs), |ui| {
        let r = super::picker::search_box(ui, &mut g.query, "Search Klipy", (!home).then_some(&mut back));
        r.request_focus();
        if r.changed() {
            search = Some(g.query.trim().to_string());
        }
        super::picker::header_rule(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 12, right: 12, top: 12, bottom: 12 }).show(ui, |ui| {
                // two 230x110 tiles a row: trending first, then discord's search terms
                if home {
                    let Some(cats) = &g.cats else {
                        ui.add(egui::Spinner::new().color(pal().text_muted));
                        return;
                    };
                    let top = ui.cursor().min;
                    // discord leads with Favorites once something's starred
                    let fav = favs.first().map(|f| ("Favorites".to_string(), Some(f.src.clone())));
                    let tiles = fav.into_iter().chain(std::iter::once(("Trending GIFs".to_string(), g.trend.clone()))).chain(cats.iter().map(|(n, s)| (n.clone(), Some(s.clone()))));
                    let first_cat = if favs.is_empty() { 1 } else { 2 };
                    let mut n = 0;
                    for (i, (name, src)) in tiles.enumerate() {
                        let r = Rect::from_min_size(top + vec2((i % 2) as f32 * 242.0, (i / 2) as f32 * 122.0), vec2(230.0, 110.0));
                        let resp = ui.interact(r, egui::Id::new(("gifcat", i)), Sense::click());
                        if ui.is_rect_visible(r) {
                            ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
                            if let Some(src) = &src {
                                egui::Image::from_uri(crate::gif::uri(ui.ctx(), src, autoplay || resp.hovered())).corner_radius(CornerRadius::same(5)).show_loading_spinner(false).paint_at(ui, r);
                            }
                            let shade = if resp.hovered() { 140 } else { 184 };
                            ui.painter().rect_filled(r, CornerRadius::same(5), egui::Color32::from_black_alpha(shade));
                            let g = ui.painter().layout_no_wrap(name.clone(), semi(16.0), egui::Color32::WHITE);
                            let mut at = r.center() - g.size() / 2.0;
                            if i < first_cat {
                                at.x += 12.0;
                                let icon = if name == "Favorites" { Icon::Star } else { Icon::Insights };
                                super::icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(at.x - 14.0, r.center().y), vec2(20.0, 20.0)), egui::Color32::WHITE);
                            }
                            ui.painter().galley(at, g, egui::Color32::WHITE);
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if name == "Trending GIFs" {
                            cat_rects.push(("gifcat:trending".to_string(), r));
                        }
                        if name == "Favorites" {
                            cat_rects.push(("gifcat:favorites".to_string(), r));
                        }
                        if resp.clicked() {
                            match name.as_str() {
                                "Favorites" if i == 0 && !favs.is_empty() => open_favs = true,
                                "Trending GIFs" if i < first_cat => trending = true,
                                _ => search = Some(name.clone()),
                            }
                        }
                        n = i + 1;
                    }
                    ui.allocate_space(vec2(472.0, n.div_ceil(2) as f32 * 122.0));
                    return;
                }
                let shown = if g.favorites { Some(&favs) } else { g.results.as_ref() };
                match shown {
                    None => {
                        ui.add(egui::Spinner::new().color(pal().text_muted));
                    }
                    Some(list) if list.is_empty() => {
                        ui.label(RichText::new("No GIFs found.").color(pal().header_secondary));
                    }
                    Some(list) => {
                        // two columns, each image keeps its aspect, shortest column gets the next one
                        let col_w = (ui.available_width() - 12.0) / 2.0;
                        let top = ui.cursor().min;
                        let mut ys = [0.0f32; 2];
                        for (i, gif) in list.iter().enumerate() {
                            let c = if ys[0] <= ys[1] { 0 } else { 1 };
                            let h = col_w * gif.h / gif.w.max(1.0);
                            let r = Rect::from_min_size(top + vec2(c as f32 * (col_w + 12.0), ys[c]), vec2(col_w, h));
                            ys[c] += h + 12.0;
                            let resp = ui.interact(r, egui::Id::new(("gif", i)), Sense::click());
                            if ui.is_rect_visible(r) {
                                ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
                                let play = autoplay || resp.hovered();
                                egui::Image::from_uri(crate::gif::uri(ui.ctx(), &gif.src, play)).corner_radius(CornerRadius::same(5)).show_loading_spinner(false).paint_at(ui, r);
                            }
                            // the star that saves it to favourites, in the corner while hovered
                            let starred = favs.iter().any(|f| f.url == gif.url);
                            let sr = Rect::from_min_size(pos2(r.right() - 36.0, r.top() + 8.0), vec2(28.0, 28.0));
                            if resp.hovered() || starred {
                                ui.painter().circle_filled(sr.center(), 14.0, egui::Color32::from_black_alpha(150));
                                let col = if starred { YELLOW } else { egui::Color32::WHITE };
                                super::icons::draw(ui.painter(), Icon::Star, sr.shrink(6.0), col);
                            }
                            if resp.hovered() {
                                ui.painter().rect_stroke(r, CornerRadius::same(5), egui::Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if i == 0 {
                                cat_rects.push(("gif:0".to_string(), r));
                                cat_rects.push(("gif:0:star".to_string(), sr));
                            }
                            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| sr.contains(p)) {
                                star = Some((gif.url.clone(), gif.src.clone(), gif.w, gif.h));
                            } else if resp.clicked() {
                                pick = Some(gif.url.clone());
                            }
                        }
                        ui.allocate_space(vec2(ui.available_width(), ys[0].max(ys[1])));
                    }
                }
            });
        });
    });
    if back {
        g.query.clear();
        g.trending = false;
        g.favorites = false;
        g.results = None;
        search = None;
    }
    if open_favs {
        g.favorites = true;
    }
    if trending {
        g.trending = true;
        g.sent = String::new();
        g.results = None;
        app.rest("GET", format!("/gifs/trending-gifs?{GIF_Q}"), None, Tag::Gifs(String::new()));
    } else if let Some(q) = search {
        app.search_gifs(q);
    }
    if let Some(f) = star {
        let favs = &mut app.settings.fav_gifs;
        match favs.iter().position(|x| x.0 == f.0) {
            Some(i) => {
                favs.remove(i);
            }
            None => favs.insert(0, f),
        }
        crate::cache::set_settings(&app.settings);
    }
    for (k, r) in cat_rects {
        app.rects.insert(k, r);
    }
    if let Some(t) = to {
        super::picker::toggle(app, t);
    } else if let Some(url) = pick {
        app.gifs = None;
        if let Some(ch) = app.cur() {
            // shift+click puts the link in the composer instead of sending it
            if ctx.input(|i| i.modifiers.shift) || crate::plugins::on("GifPaste") {
                let d = app.drafts.entry(ch).or_default();
                if !d.is_empty() && !d.ends_with(' ') {
                    d.push(' ');
                }
                d.push_str(&url);
                app.focus_composer = true;
                app.caret_end = true;
            } else {
                app.drafts.insert(ch, url);
                app.send_message(ch);
            }
        }
    } else if dismissed(ctx, &area) && !super::picker::on_picker_button(app, ctx) {
        app.gifs = None;
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    gifs(app, ctx);
    forward(app, ctx);
    invite(app, ctx);
    made_invite(app, ctx);
    status_edit(app, ctx);
    switcher(app, ctx);
    profile(app, ctx);
    pins(app, ctx);
    new_thread(app, ctx);
    leaving(app, ctx);
    emoji_pop(app, ctx);
    timeout(app, ctx);
    super::vc::raw_modal(app, ctx);
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

fn invite(app: &mut App, ctx: &egui::Context) {
    let Some(text) = &mut app.invite else { return };
    let mut join = None;
    let mut close = false;
    let modal = egui::Modal::new("invite".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Join a Server").font(semi(24.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(RichText::new("Enter an invite below to join an existing server").color(pal().header_secondary));
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Invite Link").font(medium(14.0)).color(pal().header_secondary));
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
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, "Join Server", BLURPLE, 120.0).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    join = Some(text.clone());
                }
                if ui.add(egui::Label::new(RichText::new("Back").color(pal().header_primary)).sense(Sense::click())).clicked() {
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

pub struct MadeInvite {
    pub ch: Id,
    pub code: Option<String>,
    query: String,
    // rows whose Invite went out: a dm's channel, or a friend we had no dm with
    sent: std::collections::HashSet<Id>,
}

#[derive(Clone, Copy)]
enum Pick {
    Chan(Id),
    Friend(Id),
}

impl App {
    /// Invite People: a fresh link to the channel, good for 30 days like the official client's.
    pub fn invite_people(&mut self, ch: Id) {
        self.made_invite = Some(MadeInvite { ch, code: None, query: String::new(), sent: Default::default() });
        self.rest("POST", format!("/channels/{ch}/invites"), Some(json!({ "max_age": 2_592_000, "max_uses": 0, "temporary": false })), Tag::MadeInvite);
    }
}

/// Discord's "Invite friends" dialog: dms and friends to send the link to, most recent first,
/// leaving out people we know are in the server already, then the link to copy. Measured from
/// discord's: 480 wide, 24 in, 48 tall rows.
fn made_invite(app: &mut App, ctx: &egui::Context) {
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
    let modal = egui::Modal::new("made_invite".into())
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
                super::side::chan_icon(store, ui.painter(), chan, icon, pal().header_secondary);
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
                ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, if r.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
                super::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 20.0, field.center().y), vec2(16.0, 16.0)), pal().header_primary);
                // discord's warning circle: yellow with the "!" cut out of it
                if store.locked(chan) {
                    ui.add_space(8.0);
                    let (line, _) = ui.allocate_exact_size(vec2(432.0, 16.0), Sense::hover());
                    let c = pos2(line.left() + 6.0, line.center().y);
                    ui.painter().circle_filled(c, 6.0, YELLOW);
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
                    for &p in &rows {
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
                                    ui.painter().circle_filled(pic.center(), 16.0, BLURPLE);
                                    super::icons::draw(ui.painter(), Icon::People, pic.shrink(8.0), egui::Color32::WHITE);
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
                    if rows.is_empty() {
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
                ui.painter().rect_filled(b, 8, if resp.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE });
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
                app.rest("POST", format!("/channels/{c}/messages"), Some(json!({ "content": l })), Tag::Quiet);
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

/// Setting the status line under a voice channel, which everyone in the server sees.
fn status_edit(app: &mut App, ctx: &egui::Context) {
    let Some((ch, text)) = &mut app.status_edit else { return };
    let (mut save, mut close) = (false, false);
    let modal = egui::Modal::new("status_edit".into())
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
            r.request_focus();
            save = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                save |= button(ui, "Set Status", BLURPLE, 104.0).clicked();
                ui.add_space(8.0);
                close |= button(ui, "Cancel", pal().control, 88.0).clicked();
            });
        });
    if save {
        let c = *ch;
        let status = text.trim().to_string();
        app.rest("PUT", format!("/channels/{c}/voice-status"), Some(json!({ "status": status })), Tag::Quiet);
        close = true;
    }
    if close || modal.should_close() {
        app.status_edit = None;
    }
}

enum Hit {
    Channel(Id),
    Guild(Id),
}

fn switcher(app: &mut App, ctx: &egui::Context) {
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
            .flat_map(|g| super::side::tree(store, g, None).into_iter().flat_map(|(_, k)| k))
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
    let modal = egui::Modal::new("switcher".into())
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
            egui::ScrollArea::vertical().max_height(236.0).min_scrolled_height(236.0).auto_shrink([false, false]).show(ui, |ui| {
                if hits.is_empty() && !q.is_empty() {
                    ui.add_space(90.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Can't find what you're looking for?").font(medium(16.0)).color(pal().text_muted));
                    });
                }
                ui.spacing_mut().item_spacing.y = 0.0;
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
                ui.label(RichText::new("PROTIP:").font(bold(12.0)).color(GREEN));
                ui.label(RichText::new("Start searches with @ # ! * $ to narrow results.").font(reg(14.0)).color(pal().header_secondary));
            });
        });
    // the title and arrows sit on the backdrop around the box
    let b = modal.response.rect;
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("switcher_deco")));
    p.text(pos2(b.center().x, b.top() - 41.0 + 12.0), Align2::CENTER_CENTER, "Search for servers, channels or DMs", medium(18.0), egui::Color32::WHITE);
    let iy = b.top() + 12.0 + 35.0;
    super::icons::draw(&p, super::icons::Icon::ArrowRight, Rect::from_center_size(pos2(b.left() - 40.0, iy), vec2(28.0, 28.0)), egui::Color32::WHITE);
    super::icons::draw(&p, super::icons::Icon::ArrowLeft, Rect::from_center_size(pos2(b.right() + 40.0, iy), vec2(28.0, 28.0)), egui::Color32::WHITE);
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
fn switch_row(ui: &mut Ui, store: &crate::state::Store, hit: &Hit, sel: bool) -> egui::Response {
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
                        super::icons::draw(p, Icon::People, icon.shrink(4.0), pal().header_primary);
                    }
                }
                let n = text::paint_em(p, pos2(x, y), Align2::LEFT_CENTER, &channel_title(store, c.id), medium(16.0), lit, r.width() - 200.0, 1.0);
                if let Some(u) = person {
                    text::paint(p, pos2(n.right() + 4.0, y), Align2::LEFT_CENTER, &u.username, reg(16.0), pal().header_secondary, r.right() - n.right() - 14.0);
                }
            } else {
                super::side::chan_icon(store, p, c, icon.shrink(2.0), pal().header_secondary);
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

fn profile(app: &mut App, ctx: &egui::Context) {
    let Some((uid, gid, at)) = app.profile else { return };
    let Some(user) = app.store.user(uid).cloned() else {
        app.profile = None;
        return;
    };
    let p = app.profiles.get(&uid);
    let bio = p.and_then(|p| p["user_profile"]["bio"].as_str().or(p["user"]["bio"].as_str())).unwrap_or("").to_string();
    let roles: Vec<(String, u32, Id)> = app
        .store
        .members
        .get(&gid)
        .and_then(|m| m.get(&uid))
        .zip(app.store.guilds.get(&gid))
        .map(|(m, g)| {
            let mut r: Vec<_> = m.roles.iter().filter_map(|r| g.roles.get(r)).collect();
            r.sort_by_key(|r| -r.position);
            r.into_iter().map(|r| (r.name.clone(), r.color, r.id)).collect()
        })
        .unwrap_or_default();
    let name = app.store.name_in(gid, uid);
    let status = app.store.status(uid);
    let me = app.store.me;
    let stranger = !user.bot && uid != me && !app.store.relationships.contains_key(&uid);
    let (mut message, mut more, mut add, mut main) = (None, None, false, false);
    let mut head = Rect::NOTHING;
    let mut view = None;
    let mut open_group = None;
    let mut role_act = None;
    let groups = super::card::mutual_groups(&app.store, uid);
    let store = &app.store;

    // discord's 2025 popout: a 105 banner, round buttons on it, then everything at 16 in
    let area = egui::Area::new(egui::Id::new("profile"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(300.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let wash = ui.painter().add(egui::Shape::Noop);
                let top = card_head(ui, store, &user, p, &name, status);
                head = top;
                view = super::card::click(ui, top, &user, p, gid == 0);
                // from a server profile the avatar goes to the main one
                if gid != 0 {
                    let hit = ui.interact(card_avatar(top), ui.id().with("profile_avatar"), Sense::click());
                    if hit.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    main = hit.on_hover_text("View Main Profile").clicked();
                }
                if uid != me {
                    (more, add) = banner_buttons(ui, top, stranger);
                }
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                    ui.set_width(268.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    mutuals(ui, store, p, pal().bg_secondary);
                    super::card::extras(ui, store, uid, &groups, &mut open_group);
                    if !bio.is_empty() {
                        ui.add_space(16.0);
                        ui.label(RichText::new(&bio).font(reg(14.0)).color(pal().header_primary));
                    }
                    if !roles.is_empty() {
                        ui.add_space(14.0);
                        if !bio.is_empty() {
                            ui.label(RichText::new("Roles").font(semi(12.0)).color(pal().header_primary));
                            ui.add_space(10.0);
                        }
                        // discord keeps roles to two rows and counts the rest in a "+N" pill
                        let galleys: Vec<_> = roles.iter().map(|(r, _, _)| ui.painter().layout_no_wrap(r.clone(), reg(12.0), pal().header_primary)).collect();
                        let (mut shown, mut x, mut row) = (0, 0.0, 0);
                        for g in &galleys {
                            let w = 32.0 + g.size().x;
                            if x > 0.0 && x + w > 268.0 {
                                row += 1;
                                x = 0.0;
                            }
                            let reserve = if row == 1 && shown + 1 < galleys.len() { 44.0 } else { 0.0 };
                            if row > 1 || x + w + reserve > 268.0 {
                                break;
                            }
                            x += w + 4.0;
                            shown += 1;
                        }
                        if crate::plugins::on("AlwaysExpandRoles") {
                            shown = roles.len();
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                            for ((_, c, rid), g) in roles.iter().zip(galleys).take(shown) {
                                let clickable = crate::plugins::on("BetterRoleContext") || crate::plugins::flag("BetterRoleDot", "copyRoleColorInProfilePopout");
                                let (pill, resp) = ui.allocate_exact_size(vec2(32.0 + g.size().x, 24.0), if clickable { Sense::click() } else { Sense::hover() });
                                if let Some(role) = store.guilds.get(&gid).and_then(|g| g.roles.get(rid))
                                    && let Some(a) = super::extra::role_menu(&resp, role)
                                {
                                    role_act = Some(a);
                                }
                                let dot = Rect::from_center_size(pos2(pill.left() + 12.0, pill.center().y), vec2(12.0, 12.0));
                                if resp.clicked() && crate::plugins::flag("BetterRoleDot", "copyRoleColorInProfilePopout") && resp.interact_pointer_pos().is_some_and(|p| dot.expand(4.0).contains(p)) {
                                    ui.ctx().copy_text(format!("#{c:06x}"));
                                }
                                ui.painter().rect_stroke(pill, CornerRadius::same(8), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                let col = if *c == 0 { egui::Color32::from_rgb(0xc4, 0xc9, 0xce) } else { role_color(*c) };
                                ui.painter().circle_filled(pos2(pill.left() + 12.0, pill.center().y), 6.0, col);
                                ui.painter().galley(pos2(pill.left() + 22.0, pill.center().y - g.size().y / 2.0), g, pal().header_primary);
                            }
                            if shown < roles.len() {
                                let g = ui.painter().layout_no_wrap(format!("+{}", roles.len() - shown), reg(12.0), pal().header_secondary);
                                let (pill, _) = ui.allocate_exact_size(vec2(16.0 + g.size().x, 24.0), Sense::hover());
                                ui.painter().rect_stroke(pill, CornerRadius::same(8), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                ui.painter().galley(pill.center() - g.size() / 2.0, g, pal().header_secondary);
                            }
                        });
                    }
                    super::card::connections(ui, p);
                    if uid != me {
                        // discord's box: typed into in place, Enter sends it and opens the dm.
                        // It has the focus as the card opens.
                        ui.add_space(16.0);
                        let draft = egui::Id::new(("profile_msg", uid));
                        let mut text: String = ui.data(|d| d.get_temp(draft)).unwrap_or_default();
                        let (r, _) = ui.allocate_exact_size(vec2(268.0, 46.0), Sense::hover());
                        ui.painter().rect(r, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                        super::icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(pos2(r.right() - 29.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary);
                        let field = Rect::from_min_max(pos2(r.left() + 16.0, r.top() + 1.0), pos2(r.right() - 44.0, r.bottom() - 1.0));
                        let edit = ui.put(
                            field,
                            egui::TextEdit::singleline(&mut text)
                                .id(egui::Id::new("profile_msg_box"))
                                .frame(Frame::NONE)
                                .font(reg(14.0))
                                .text_color(pal().text_normal)
                                .vertical_align(egui::Align::Center)
                                .hint_text(RichText::new(format!("Message @{name}")).font(reg(14.0)).color(pal().text_muted))
                                .desired_width(field.width()),
                        );
                        if !ui.ctx().memory(|m| m.areas().visible_last_frame(&ui.layer_id())) {
                            edit.request_focus();
                        }
                        app.rects.insert("profile_msg".into(), r);
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !text.trim().is_empty() {
                            message = Some(std::mem::take(&mut text));
                        }
                        ui.data_mut(|d| d.insert_temp(draft, text));
                    }
                });
                theme_wash(ui, wash, p, ui.min_rect());
            });
        });
    app.rects.insert("profile_head".into(), head);
    if let Some(a) = role_act {
        super::extra::do_role(app, ctx, gid, a);
    }
    if let Some(c) = open_group {
        app.profile = None;
        app.ui.guild = None;
        app.open(c);
    } else if let Some(url) = view {
        super::extra::view(app, url);
    } else if let Some(text) = message {
        app.profile = None;
        app.dm_text(uid, text);
    } else if main {
        app.open_profile(uid, 0, at);
    } else if let Some(at) = more {
        app.user_menu = Some((uid, gid, None, at));
    } else if add {
        app.rest("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), Tag::Friend);
    } else if dismissed(ctx, &area.response) && app.user_menu.is_none() {
        app.profile = None;
    }
}

/// The round buttons on a profile's banner: "..." for the person's menu, and add friend to a
/// stranger. Hands back where the menu should open and whether add was clicked.
fn banner_buttons(ui: &mut Ui, top: Rect, stranger: bool) -> (Option<Pos2>, bool) {
    let mut x = top.right() - 12.0;
    let mut round = |ui: &mut Ui, icon: Icon, tip: &str| {
        let r = Rect::from_min_size(pos2(x - 32.0, top.top() + 8.0), vec2(32.0, 32.0));
        x -= 40.0;
        let resp = ui.interact(r, ui.id().with(("banner_btn", tip)), Sense::click());
        let fill = egui::Color32::from_black_alpha(if resp.hovered() { 184 } else { 143 });
        ui.painter().circle(r.center(), 16.0, fill, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)));
        super::icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), egui::Color32::WHITE);
        resp.on_hover_text(tip)
    };
    let r = round(ui, Icon::More, "More");
    let more = r.clicked().then(|| r.rect.left_bottom() + vec2(0.0, 4.0));
    let add = stranger && round(ui, Icon::PersonAdd, "Add Friend").clicked();
    (more, add)
}

/// "6 Mutual Friends • 7 Mutual Servers" with the first three friends' faces. `card` is what
/// the faces' rings are cut from.
fn mutuals(ui: &mut Ui, store: &crate::state::Store, p: Option<&serde_json::Value>, card: egui::Color32) {
    let friends: Vec<Id> = p.and_then(|p| p["mutual_friends"].as_array()).into_iter().flatten().map(|f| crate::state::sid(&f["id"])).collect();
    let n_friends = p.and_then(|p| p["mutual_friends_count"].as_u64()).unwrap_or(friends.len() as u64);
    let n_guilds = p.and_then(|p| p["mutual_guilds"].as_array()).map_or(0, |g| g.len() as u64);
    if n_friends == 0 && n_guilds == 0 {
        return;
    }
    ui.add_space(19.0);
    let (r, _) = ui.allocate_exact_size(vec2(268.0, 16.0), Sense::hover());
    let mut x = r.left();
    for f in friends.iter().take(3) {
        let a = Rect::from_min_size(pos2(x, r.top()), vec2(16.0, 16.0));
        // each face sits on a ring of the card over the one before
        ui.painter().circle_filled(a.center(), 9.5, card);
        if let Some(u) = store.user(*f) {
            avatar(ui, a, &u.avatar_url());
        }
        x += 13.0;
    }
    if !friends.is_empty() {
        x += 7.0;
    }
    let plural = |n: u64, what: &str| format!("{n} Mutual {what}{}", if n == 1 { "" } else { "s" });
    let parts: Vec<String> = [(n_friends, "Friend"), (n_guilds, "Server")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| plural(n, w)).collect();
    for (i, t) in parts.iter().enumerate() {
        if i > 0 {
            ui.painter().circle_filled(pos2(x + 6.0, r.center().y), 2.0, pal().header_secondary);
            x += 12.0;
        }
        x = ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, t, reg(12.0), pal().header_secondary).right();
    }
}

/// The card discord keeps beside a one-on-one dm: the person's profile, full height.
pub fn dm_profile(app: &mut App, ui: &mut Ui, uid: Id) {
    let Some(user) = app.store.user(uid).cloned() else { return };
    if !app.profiles.contains_key(&uid) {
        app.profiles.insert(uid, serde_json::Value::Null);
        app.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=true&with_mutual_friends=true"), None, Tag::Profile(uid));
    }
    if !app.notes.contains_key(&uid) {
        app.notes.insert(uid, String::new());
        app.rest("GET", format!("/users/@me/notes/{uid}"), None, Tag::Note(uid));
    }
    // what's saved, and what's being typed over it while the box has focus
    let saved = app.notes.get(&uid).cloned().unwrap_or_default();
    let draft = egui::Id::new(("note_draft", uid));
    let mut note: String = ui.data(|d| d.get_temp(draft)).unwrap_or_else(|| saved.clone());
    let mut save_note = false;
    let stranger = !user.bot && !app.store.relationships.contains_key(&uid);
    let status = app.store.status(uid);
    let (mut more, mut add, mut full) = (None, false, false);
    let mut view = None;
    let mut open_group = None;
    let groups = super::card::mutual_groups(&app.store, uid);
    let height = ui.available_height();
    let store = &app.store;
    let p = app.profiles.get(&uid);
    Frame::new().fill(pal().bg_primary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).show(ui, |ui| {
        ui.set_width(300.0);
        ui.set_min_height(height - 2.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let wash = ui.painter().add(egui::Shape::Noop);
        let top = card_head(ui, store, &user, p, user.display(), status);
        view = super::card::click(ui, top, &user, p, true);
        (more, add) = banner_buttons(ui, top, stranger);
        Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(268.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            mutuals(ui, store, p, pal().bg_primary);
            super::card::extras(ui, store, uid, &groups, &mut open_group);
            super::card::connections(ui, p);
            ui.add_space(24.0);
            ui.label(RichText::new("Member Since").font(semi(12.0)).color(pal().header_primary));
            ui.add_space(8.0);
            let since = chrono::DateTime::from_timestamp_millis(crate::state::snowflake_ms(uid)).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
            ui.label(RichText::new(since).font(reg(14.0)).color(pal().header_primary));
            // the note only you see: Enter saves it, Shift+Enter starts a new line, six at most
            ui.add_space(24.0);
            ui.label(RichText::new("Note").font(semi(12.0)).color(pal().header_primary));
            ui.add_space(4.0);
            let nid = egui::Id::new(("note", uid));
            let focused = ui.memory(|m| m.has_focus(nid));
            let enter = focused && ui.input_mut(|i| super::take_key(i, egui::Modifiers::NONE, Key::Enter));
            let before = note.clone();
            let r = ui.add(
                egui::TextEdit::multiline(&mut note)
                    .id(nid)
                    .frame(Frame::NONE)
                    .desired_rows(1)
                    .font(reg(12.0))
                    .hint_text(RichText::new("Click to add a note").font(reg(12.0)).color(pal().text_muted))
                    .desired_width(268.0),
            );
            if note.lines().count() > 6 {
                note = before;
            }
            app.rects.insert("note".into(), r.rect);
            if enter {
                r.surrender_focus();
            }
            save_note = enter || r.lost_focus();
            if !save_note && !r.has_focus() {
                note = saved.clone();
            }
        });
        let b = Rect::from_min_size(pos2(top.left() + 16.0, top.top() + height - 2.0 - 58.0), vec2(268.0, 40.0));
        let resp = ui.interact(b, ui.id().with("full_profile"), Sense::click());
        ui.painter().rect(b, 8, egui::Color32::from_white_alpha(if resp.hovered() { 31 } else { 20 }), egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "View Full Profile", medium(16.0), pal().header_primary);
        full = resp.clicked();
        theme_wash(ui, wash, p, ui.min_rect());
    });
    if let Some(url) = view {
        super::extra::view(app, url);
    }
    if let Some(c) = open_group {
        app.open(c);
    }
    if let Some(at) = more {
        app.user_menu = Some((uid, 0, None, at));
    }
    if add {
        app.rest("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), Tag::Friend);
    }
    if save_note && note != saved {
        app.rest("PUT", format!("/users/@me/notes/{uid}"), Some(json!({ "note": note })), Tag::Quiet);
        app.notes.insert(uid, note.clone());
    }
    ui.data_mut(|d| d.insert_temp(draft, note));
    if full {
        app.open_profile(uid, 0, ui.min_rect().left_top() - vec2(310.0, 0.0));
    }
}

/// A profile's nitro theme as discord shows it: primary and accent, dimmed in dark themes.
fn theme(p: Option<&serde_json::Value>) -> Option<(egui::Color32, egui::Color32)> {
    let mine = p.is_some_and(|p| crate::state::sid(&p["user"]["id"]) == crate::state::ME.load(std::sync::atomic::Ordering::Relaxed));
    if crate::plugins::on("NoProfileThemes") && !mine {
        return None;
    }
    let c = p.and_then(|p| p["user_profile"]["theme_colors"].as_array()).filter(|c| c.len() == 2)?;
    let k = if pal().dark { 0.4 } else { 1.0 };
    let col = |v: &serde_json::Value| {
        let c = v.as_u64().unwrap_or(0) as u32;
        let ch = |s: u32| (((c >> s) & 255) as f32 * k) as u8;
        egui::Color32::from_rgb(ch(16), ch(8), ch(0))
    };
    Some((col(&c[0]), col(&c[1])))
}

fn theme_top(p: Option<&serde_json::Value>) -> Option<egui::Color32> {
    theme(p).map(|(a, _)| a)
}

/// A nitro profile theme: the card washed from its primary colour down to its accent, dimmed the
/// way discord dims them in dark themes. Paints into `slot`, reserved before the card's content.
fn theme_wash(ui: &Ui, slot: egui::layers::ShapeIdx, p: Option<&serde_json::Value>, rect: Rect) {
    let Some((a, b)) = theme(p) else { return };
    let mut mesh = egui::Mesh::default();
    let size = ui.ctx().fonts(|f| f.font_image_size());
    let mut tess = egui::epaint::Tessellator::new(ui.ctx().pixels_per_point(), Default::default(), size, Vec::new());
    tess.tessellate_rect(&egui::epaint::RectShape::filled(rect, 7, egui::Color32::WHITE), &mut mesh);
    for v in &mut mesh.vertices {
        let t = ((v.pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        v.color = egui::Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), v.color.a());
    }
    ui.painter().set(slot, mesh);
}

/// The top of a profile card: banner, avatar with its decoration and status, then the names.
/// Hands back the banner so buttons can go on it.
/// Where a card's avatar sits under its banner.
fn card_avatar(top: Rect) -> Rect {
    Rect::from_min_size(pos2(top.left() + 16.0, top.top() + 61.0), vec2(80.0, 80.0))
}

pub fn card_head(ui: &mut Ui, store: &crate::state::Store, user: &crate::state::User, p: Option<&serde_json::Value>, name: &str, status: crate::state::Status) -> Rect {
    let accent = p
        .and_then(|p| p["user_profile"]["accent_color"].as_u64().or(p["user"]["accent_color"].as_u64()))
        .map(|c| role_color(c as u32))
        // no banner colour set: discord's pick from the avatar, as in call tiles
        .or_else(|| super::palette::avatar(ui.ctx(), &user.avatar_url()))
        .unwrap_or(BLURPLE);
    let nitro = p.and_then(|p| p["user"]["banner"].as_str()).map(|b| format!("{}/banners/{}/{b}.png?size=600", crate::api::cdn(), user.id));
    let usrbg = super::extra::usrbg(user.id);
    let banner = if crate::plugins::flag("USRBG", "nitroFirst") { nitro.or(usrbg) } else { usrbg.or(nitro) };
    let pronouns = p.and_then(|p| p["user_profile"]["pronouns"].as_str()).filter(|s| !s.is_empty());
    let (top, _) = ui.allocate_exact_size(vec2(300.0, 105.0), Sense::hover());
    let cr = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
    ui.painter().rect_filled(top, cr, accent);
    if let Some(b) = &banner {
        crate::gif::paint_still(ui, b, top, cr, ui.rect_contains_pointer(top));
    }
    let av = card_avatar(top);
    // the ring around the avatar is cut from the card, themed or not
    let ring = theme_top(p).unwrap_or(pal().bg_secondary);
    ui.painter().circle_filled(av.center(), 46.0, ring);
    avatar(ui, av, &user.avatar_url().replace("size=80", "size=128"));
    if let Some(d) = user.decoration_url() {
        text::image(ui.painter(), &d, Rect::from_center_size(av.center(), vec2(96.0, 96.0)));
    }
    status_dot(ui, av, status, ring);
    ui.add_space(47.0);
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.set_width(268.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let face = if crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == user.font) { name_font(user.font, 20.0) } else { bold(20.0) };
            let r = text::label(ui, name, face, pal().header_primary, Sense::hover());
            if user.bot {
                super::app_tag(ui.painter(), pos2(r.rect.right() + 4.0, r.rect.center().y), user.flags & (1 << 16) != 0, 1.0);
            }
        });
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut r = text::label(ui, &user.username, reg(14.0), pal().header_primary, Sense::hover()).rect;
            if let Some(pr) = pronouns {
                ui.painter().circle_filled(pos2(r.right() + 9.0, r.center().y), 2.0, pal().header_primary);
                ui.add_space(18.0);
                r = text::label(ui, pr, reg(14.0), pal().header_primary, Sense::hover()).rect;
            }
            let mut x = super::tag_chip(ui.painter(), user, pos2(r.right() + 8.0, r.center().y), 1.0).map_or(r.right() + 8.0, |t| t.right() + 6.0);
            // the profile's badges, 20 apiece, each telling what it's for on hover
            for b in p.and_then(|p| p["badges"].as_array()).into_iter().flatten() {
                let Some(icon) = b["icon"].as_str() else { continue };
                let br = Rect::from_min_size(pos2(x, r.center().y - 10.0), vec2(20.0, 20.0));
                egui::Image::from_uri(format!("{}/badge-icons/{icon}.png", crate::api::cdn())).show_loading_spinner(false).paint_at(ui, br);
                ui.interact(br, ui.id().with(("badge", icon)), Sense::hover()).on_hover_text(b["description"].as_str().unwrap_or(""));
                x += 22.0;
            }
            if crate::plugins::flag("PlatformIndicators", "badges") {
                for (kind, st) in super::platform::of(store, user.id) {
                    let br = Rect::from_min_size(pos2(x, r.center().y - 10.0), vec2(20.0, 20.0));
                    super::platform::draw(ui.painter(), kind, br, st);
                    ui.interact(br, ui.id().with(("platform", kind)), Sense::hover()).on_hover_text(super::platform::NAMES[kind]);
                    x += 22.0;
                }
            }
        });
    });
    top
}

enum AccountAct {
    Status(&'static str),
    Edit,
    CopyId,
}

/// What opens from the account panel: your card, then discord's grouped rows.
pub fn account(app: &mut App, ui: &mut Ui) {
    use crate::state::Status;
    let me = app.store.me;
    let Some(user) = app.store.user(me).cloned() else { return };
    if !app.profiles.contains_key(&me) {
        // a placeholder until it lands, so this asks once
        app.profiles.insert(me, serde_json::Value::Null);
        app.rest("GET", format!("/users/{me}/profile?with_mutual_guilds=false"), None, Tag::Profile(me));
    }
    let status = app.store.presence.get(&me).copied().unwrap_or(Status::Online);
    let dev = app.settings.dev;
    let custom = app.store.custom_status.clone();
    let mut act = None;
    ui.set_width(300.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    let wash = ui.painter().add(egui::Shape::Noop);
    let top = card_head(ui, &app.store, &user, app.profiles.get(&me), user.display(), status);
    let view = super::card::click(ui, top, &user, app.profiles.get(&me), true);
    if let Some(c) = &custom {
        // the speech bubble discord floats beside your avatar
        let g = ui.painter().layout(c.clone(), reg(14.0), pal().header_primary, 155.0);
        let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(179.0, (g.size().y + 22.0).max(40.0)));
        ui.painter().rect(b, 16, pal().bg_floating, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        ui.painter().galley(pos2(b.left() + 12.0, b.center().y - g.size().y / 2.0), g, pal().header_primary);
    }
    let label = match status {
        Status::Online => "Online",
        Status::Idle => "Idle",
        Status::Dnd => "Do Not Disturb",
        Status::Offline => "Invisible",
    };
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 16 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let card = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            Frame::new().fill(PROFILE_CARD).corner_radius(8).show(ui, |ui| {
                ui.set_width(268.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui);
            });
        };
        card(ui, &mut |ui| {
            if account_row(ui, Some(Icon::Pencil), "Edit Profile", false, false).clicked() {
                act = Some(AccountAct::Edit);
            }
            let r = account_row(ui, None, label, true, true);
            let c = pos2(r.rect.left() + 24.0, r.rect.center().y);
            super::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), status, PROFILE_CARD);
            egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (key, label, sub, st) in [
                    ("online", "Online", None, Status::Online),
                    ("idle", "Idle", None, Status::Idle),
                    ("dnd", "Do Not Disturb", Some("You will not receive desktop notifications"), Status::Dnd),
                    ("invisible", "Invisible", Some("You will appear offline"), Status::Offline),
                ] {
                    let (resp, lead) = super::menu_row_at(ui, true, label, sub, false, super::Trail::None);
                    let c = lead.center();
                    super::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), st, pal().bg_floating);
                    if resp.clicked() {
                        act = Some(AccountAct::Status(key));
                    }
                    if key == "online" {
                        super::menu_sep(ui);
                    }
                }
            });
        });
        if dev {
            card(ui, &mut |ui| {
                if account_row(ui, Some(Icon::Id), "Copy User ID", false, false).clicked() {
                    act = Some(AccountAct::CopyId);
                }
            });
        }
    });
    theme_wash(ui, wash, app.profiles.get(&me), ui.min_rect());
    if let Some(url) = view {
        super::extra::view(app, url);
    }
    match act {
        Some(AccountAct::Status(s)) => app.set_status(s),
        Some(AccountAct::Edit) => app.settings_page = Some(super::settings::Page::Profiles),
        Some(AccountAct::CopyId) => ui.ctx().copy_text(me.to_string()),
        None => {}
    }
    if act.is_some() {
        ui.close();
    }
}

/// A 48 tall row in the account card's groups, with a hairline above all but the first.
fn account_row(ui: &mut Ui, icon: Option<Icon>, label: &str, caret: bool, rule: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(268.0, 48.0), Sense::click());
    if rule {
        ui.painter().hline(r.x_range().shrink(12.0), r.top(), egui::Stroke::new(1.0, pal().border_subtle));
    }
    if resp.hovered() {
        ui.painter().rect_filled(r.shrink(4.0), CornerRadius::same(4), MENU_HOVER);
    }
    if let Some(i) = icon {
        super::icons::draw(ui.painter(), i, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary);
    if caret {
        super::icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    resp
}

fn pins(app: &mut App, ctx: &egui::Context) {
    let Some((ch, list)) = &app.pins else { return };
    let ch = *ch;
    // right-aligned under the button, as tall as 80% of the window allows
    let at = app.rects.get("pins_button").map(|r| pos2(r.right() - 420.0, r.bottom() + 9.0)).unwrap_or(pos2(400.0, 60.0));
    let tall = ctx.content_rect().height() * 0.8 - 57.0;
    let mut jump = None;
    let mut unpin = None;
    let manage = app.store.channels.get(&ch).is_some_and(|c| c.guild == 0 || app.store.perms(c) & crate::state::MANAGE_MESSAGES != 0);
    let area = egui::Area::new(egui::Id::new("pins"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            // an area only offers last frame's size; without this the list never outgrows the spinner
            ui.set_max_height(tall + 57.0);
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20))).show(ui, |ui| {
                ui.set_width(420.0);
                ui.set_max_width(420.0);
                Frame::new().inner_margin(Margin { left: 16, right: 16, top: 16, bottom: 17 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        super::icons::draw(ui.painter(), super::icons::Icon::Pin, r, pal().header_secondary);
                        ui.label(RichText::new("Pinned Messages").font(semi(20.0)).color(pal().header_primary));
                    });
                });
                egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| {
                    Frame::new().inner_margin(Margin { left: 8, right: 12, top: 0, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match list {
                            None => {
                                ui.add(egui::Spinner::new().color(pal().text_muted));
                            }
                            Some(l) if l.is_empty() => {
                                ui.add_space(24.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("This channel doesn't have any
pinned messages... yet.").font(medium(16.0)).color(pal().header_primary));
                                });
                                ui.add_space(24.0);
                                let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                                ui.painter().hline(sep.x_range(), sep.center().y, egui::Stroke::new(1.0, pal().border_subtle));
                                ui.add_space(12.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new("PROTIP:").font(bold(12.0)).color(GREEN));
                                    ui.label(
                                        RichText::new("Users with the 'Pin Messages' permission can pin a message from its context menu.")
                                            .font(reg(14.0))
                                            .color(pal().header_secondary),
                                    );
                                });
                            }
                            Some(l) => {
                                for m in l {
                                    let card = chat::preview(ui, &app.store, m).rect;
                                    // discord's Jump and unpin ✕, over the card's corner while it's hovered
                                    let hot = ui.rect_contains_pointer(card);
                                    let mut x = card.right() - 8.0;
                                    let mut corner = |ui: &mut Ui, icon: Icon, tip: &str, key: &str| {
                                        let r = Rect::from_min_size(pos2(x - 28.0, card.top() + 8.0), vec2(28.0, 28.0));
                                        x -= 32.0;
                                        let resp = ui.interact(r, ui.id().with((key, m.id)), Sense::click());
                                        app.rects.insert(format!("pins:{key}:{}", m.id), r);
                                        if hot {
                                            ui.painter().rect(r, 8, if resp.hovered() { pal().control_hover } else { pal().control }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                                            super::icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal().header_primary);
                                        }
                                        hot && resp.on_hover_text(tip).clicked()
                                    };
                                    if manage && corner(ui, Icon::Close, "Unpin Message", "unpin") {
                                        unpin = Some(m.id);
                                    }
                                    if corner(ui, Icon::Jump, "Jump to Message", "jump") {
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
    // the popout stays up after a jump, as discord's does
    if let Some(id) = jump {
        app.jump_to(ch, id);
    } else if let Some(id) = unpin {
        let ask = !ctx.input(|i| i.modifiers.shift);
        app.pin_msg(ch, id, ask);
    } else if app.confirm.is_none()
        && (ctx.input(|i| i.key_pressed(Key::Escape))
            || dismissed(ctx, &area.response) && !app.rects.get("pins_button").is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.contains(p))))
    {
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
    Frame::new().fill(pal().bg_secondary).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(header).font(semi(16.0)).color(pal().header_primary));
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
                    ui.add(egui::Spinner::new().color(pal().text_muted));
                }
                Some(list) if list.is_empty() => {
                    ui.label(RichText::new("We searched far and wide. Unfortunately, no results were found.").color(pal().header_secondary));
                }
                Some(list) => {
                    let mut last = 0;
                    for m in list {
                        if s.guild && m.channel != last {
                            last = m.channel;
                            let name = app.store.channels.get(&m.channel).map(|c| c.name.clone()).unwrap_or_default();
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("# {name}")).font(semi(14.0)).color(pal().header_secondary));
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

/// What drops from the search box while it's focused. Empty, discord's filters and past
/// searches; typed, a row to search and the people and channels the last word could mean.
/// Picking a filter types it; picking a search runs it.
pub fn search_options(app: &mut App, ctx: &egui::Context, field: &egui::Response, q: &mut String, guild: Id) -> Option<String> {
    let id = egui::Id::new("search_options");
    let hovered = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if !(field.has_focus() || hovered) {
        ctx.data_mut(|d| d.insert_temp(id, false));
        return None;
    }
    let typed = q.trim().to_string();
    let word = typed.rsplit(' ').next().unwrap_or("").to_lowercase();
    let filtering = word.contains(':');
    let store = &app.store;
    // who and where the last word might be, a couple of each like discord
    let people: Vec<(Id, String, String)> = if typed.is_empty() || filtering {
        Vec::new()
    } else {
        let pool: Vec<Id> = match store.members.get(&guild) {
            Some(m) => m.keys().copied().collect(),
            None => store.users.keys().copied().collect(),
        };
        pool.into_iter()
            .filter_map(|u| store.user(u))
            .filter(|u| u.username.to_lowercase().contains(&word) || u.display().to_lowercase().contains(&word))
            .take(2)
            .map(|u| (u.id, store.name_in(guild, u.id), u.username.clone()))
            .collect()
    };
    let places: Vec<(Id, String)> = if typed.is_empty() || filtering || guild == 0 {
        Vec::new()
    } else {
        store.channels.values().filter(|c| c.guild == guild && c.is_text() && !c.is_thread() && c.name.to_lowercase().contains(&word)).take(2).map(|c| (c.id, c.name.clone())).collect()
    };
    let mut run = None;
    let mut add: Option<String> = None;
    let mut replace: Option<String> = None;
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(field.rect.right_bottom() + vec2(-4.0, 8.0))
        .show(ctx, |ui| {
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(356.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let head = |ui: &mut Ui, t: &str| {
                    let (r, _) = ui.allocate_exact_size(vec2(356.0, 36.0), Sense::hover());
                    ui.painter().text(pos2(r.left() + 17.0, r.top() + 25.0), Align2::LEFT_CENTER, t, semi(12.0), pal().header_secondary);
                };
                let rule = |ui: &mut Ui| {
                    ui.add_space(8.0);
                    let (r, _) = ui.allocate_exact_size(vec2(356.0, 1.0), Sense::hover());
                    ui.painter().hline(r.x_range(), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
                };
                // a two-line row: icon, a title, and "key: value" under it
                let row = |ui: &mut Ui, icon: Icon, avatar_url: Option<String>, title: &str, key: &str, hint: &str| {
                    let (r, resp) = ui.allocate_exact_size(vec2(356.0, 48.0), Sense::click());
                    let p = ui.painter();
                    if resp.hovered() {
                        p.rect_filled(r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER);
                    }
                    super::icons::draw(p, icon, Rect::from_center_size(pos2(r.left() + 27.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                    let mut x = r.left() + 45.0;
                    if let Some(url) = avatar_url {
                        avatar(ui, Rect::from_center_size(pos2(x + 8.0, r.center().y - 9.0), vec2(16.0, 16.0)), &url);
                        x += 22.0;
                    }
                    let p = ui.painter();
                    text::paint(p, pos2(x, r.center().y - 9.0), Align2::LEFT_CENTER, title, if key.is_empty() { medium(14.0) } else { semi(14.0) }, pal().header_primary, 280.0);
                    let k = p.text(pos2(r.left() + 45.0, r.center().y + 9.0), Align2::LEFT_CENTER, key, semi(14.0), pal().header_secondary);
                    p.text(pos2(k.right() + 4.0, r.center().y + 9.0), Align2::LEFT_CENTER, hint, medium(14.0), pal().text_muted);
                    resp.clicked()
                };
                if typed.is_empty() {
                    head(ui, "Filters");
                    ui.add_space(4.0);
                    for (icon, title, key, hint) in [
                        (Icon::Person, "From a specific user", "from:", "user"),
                        (Icon::Hash, "Sent in a specific channel", "in:", "channel"),
                        (Icon::Attach, "Includes a specific type of data", "has:", "link, embed or file"),
                        (Icon::At, "Mentions a specific user", "mentions:", "user"),
                    ] {
                        if row(ui, icon, None, title, key, hint) {
                            add = Some(key.to_string());
                        }
                    }
                    let history = &app.ui.searches;
                    if !history.is_empty() {
                        rule(ui);
                        head(ui, "History");
                        ui.add_space(4.0);
                        for h in history {
                            let (r, resp) = ui.allocate_exact_size(vec2(356.0, 36.0), Sense::click());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER);
                            }
                            super::icons::draw(p, Icon::Clock, Rect::from_center_size(pos2(r.left() + 27.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                            text::paint(p, pos2(r.left() + 45.0, r.center().y), Align2::LEFT_CENTER, h, semi(14.0), pal().header_primary, 290.0);
                            if resp.clicked() {
                                run = Some(h.clone());
                            }
                        }
                    }
                } else {
                    ui.add_space(8.0);
                    let (r, resp) = ui.allocate_exact_size(vec2(356.0, 36.0), Sense::click());
                    let p = ui.painter();
                    if resp.hovered() {
                        p.rect_filled(r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER);
                    }
                    super::icons::draw(p, Icon::Search, Rect::from_center_size(pos2(r.left() + 27.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                    let s = p.text(pos2(r.left() + 45.0, r.center().y), Align2::LEFT_CENTER, "Search for ", medium(14.0), pal().header_primary);
                    text::paint(p, pos2(s.right(), r.center().y), Align2::LEFT_CENTER, &typed, semi(14.0), pal().header_primary, 290.0 - s.width());
                    if resp.clicked() {
                        run = Some(typed.clone());
                    }
                    // discord's order: who it's from, where, who it mentions
                    if !people.is_empty() {
                        rule(ui);
                        head(ui, "From User");
                        for (uid, name, username) in &people {
                            if row(ui, Icon::Person, store.user(*uid).map(|u| u.avatar_url()), name, "from:", username) {
                                replace = Some(format!("from:{username} "));
                            }
                        }
                    }
                    if !places.is_empty() {
                        rule(ui);
                        head(ui, "In Channel");
                        for (_, name) in &places {
                            let (r, resp) = ui.allocate_exact_size(vec2(356.0, 36.0), Sense::click());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(r.shrink2(vec2(8.0, 2.0)), 4, MENU_HOVER);
                            }
                            super::icons::draw(p, Icon::Hash, Rect::from_center_size(pos2(r.left() + 27.0, r.center().y), vec2(20.0, 20.0)), pal().text_muted);
                            let k = p.text(pos2(r.left() + 45.0, r.center().y), Align2::LEFT_CENTER, "in:", semi(14.0), pal().header_secondary);
                            text::paint(p, pos2(k.right() + 4.0, r.center().y), Align2::LEFT_CENTER, &format!("# {name}"), semi(14.0), pal().header_primary, 280.0);
                            if resp.clicked() {
                                replace = Some(format!("in:{name} "));
                            }
                        }
                    }
                    if !people.is_empty() {
                        rule(ui);
                        head(ui, "Mentions User");
                        for (uid, name, username) in &people {
                            if row(ui, Icon::At, store.user(*uid).map(|u| u.avatar_url()), name, "mentions:", username) {
                                replace = Some(format!("mentions:{username} "));
                            }
                        }
                    }
                }
                ui.add_space(8.0);
            });
        });
    ctx.data_mut(|d| d.insert_temp(id, area.response.contains_pointer()));
    app.rects.insert("search_options".into(), area.response.rect);
    if let Some(k) = add {
        q.push_str(&k);
        field.request_focus();
    }
    if let Some(r) = replace {
        // the suggestion takes the place of the word that called it up
        let keep = q.trim_end().rsplit_once(' ').map(|(head, _)| format!("{head} ")).unwrap_or_default();
        *q = keep + &r;
        field.request_focus();
    }
    run
}

/// Discord's thread browser under the header's threads button, and Create for a new one.
pub fn threads_menu(app: &mut App, button: &egui::Response, ch: Id) {
    let mut open = None;
    let mut create = false;
    let shadow = button.ctx.global_style().visuals.popup_shadow;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow);
    let menu = egui::Popup::menu(button).align(egui::RectAlign::BOTTOM_END).gap(8.0).frame(frame);
    menu.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(597.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let qid = ui.id().with("thread_search");
        let mut q: String = ui.ctx().data(|d| d.get_temp(qid)).unwrap_or_default();
        let (head, _) = ui.allocate_exact_size(vec2(597.0, 49.0), Sense::hover());
        let p = ui.painter();
        p.hline(head.x_range(), head.bottom(), egui::Stroke::new(1.0, pal().border_subtle));
        super::icons::draw(p, Icon::Thread, Rect::from_min_size(pos2(head.left() + 16.0, head.top() + 12.0), vec2(24.0, 24.0)), pal().header_secondary);
        p.text(pos2(head.left() + 48.0, head.center().y), Align2::LEFT_CENTER, "Threads", semi(16.0), pal().header_primary);
        p.vline(head.left() + 120.0, head.center().y - 12.0..=head.center().y + 12.0, egui::Stroke::new(1.0, pal().border_subtle));
        let field = Rect::from_min_size(pos2(head.left() + 137.0, head.top() + 8.0), vec2(279.0, 32.0));
        ui.painter().rect_filled(field, 8, pal().input_bg);
        let inner = Rect::from_min_max(pos2(field.left() + 30.0, field.center().y - 9.0), pos2(field.right() - 8.0, field.center().y + 9.0));
        let edit = ui.put(
            inner,
            egui::TextEdit::singleline(&mut q)
                .hint_text(RichText::new("Search for Thread Name").color(pal().text_muted))
                .font(reg(14.0))
                .frame(Frame::NONE)
                .margin(Margin::ZERO),
        );
        let ring = if edit.has_focus() { BLURPLE } else { pal().border_subtle };
        ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, ring), egui::StrokeKind::Inside);
        super::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(12.0, 12.0)), pal().header_primary);
        ui.ctx().data_mut(|d| d.insert_temp(qid, q.clone()));
        let b = Rect::from_min_size(pos2(head.right() - 80.0, head.top() + 8.0), vec2(64.0, 32.0));
        let br = ui.interact(b, ui.id().with("thread_create"), Sense::click());
        ui.painter().rect_filled(b, 8, if br.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create", medium(14.0), egui::Color32::WHITE);
        create |= br.clicked();

        let needle = q.to_lowercase();
        let mut list: Vec<_> = app
            .store
            .channels
            .values()
            .filter(|c| c.is_thread() && c.parent == ch && !c.archived && c.name.to_lowercase().contains(&needle))
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.last_msg.max(c.id)));
        if list.is_empty() {
            let (body, _) = ui.allocate_exact_size(vec2(597.0, 400.0), Sense::hover());
            let p = ui.painter();
            let c = pos2(body.center().x, body.top() + 110.0);
            p.circle_filled(c, 40.0, pal().bg_tertiary);
            super::icons::draw(p, Icon::Thread, Rect::from_center_size(c, vec2(36.0, 36.0)), pal().header_secondary);
            p.text(pos2(c.x, body.top() + 182.0), Align2::CENTER_CENTER, "There are no threads.", semi(24.0), pal().header_primary);
            p.text(pos2(c.x, body.top() + 215.0), Align2::CENTER_CENTER, "Stay focused on a conversation with a thread - a temporary text channel.", reg(16.0), pal().text_normal);
            let b = Rect::from_center_size(pos2(c.x, body.top() + 259.0), vec2(130.0, 40.0));
            let br = ui.interact(b, ui.id().with("thread_create_big"), Sense::click());
            ui.painter().rect_filled(b, 8, if br.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE });
            ui.painter().text(b.center(), Align2::CENTER_CENTER, "Create Thread", medium(16.0), egui::Color32::WHITE);
            create |= br.clicked();
            if create {
                ui.close();
            }
            return;
        }
        egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, true]).show(ui, |ui| {
            Frame::new().inner_margin(16).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let n = list.len();
                ui.label(RichText::new(format!("{n} Active Thread{}", if n == 1 { "" } else { "s" })).font(medium(14.0)).color(pal().header_secondary));
                for c in list {
                    let (r, resp) = ui.allocate_exact_size(vec2(565.0, 64.0), Sense::click());
                    ui.painter().rect_filled(r, 8, if resp.hovered() { pal().bg_mod_hover } else { pal().bg_floating });
                    text::paint(ui.painter(), pos2(r.left() + 16.0, r.top() + 22.0), Align2::LEFT_CENTER, &c.name, semi(16.0), pal().header_primary, r.width() - 32.0);
                    let when = (c.last_msg != 0).then(|| ago(crate::state::snowflake_ms(c.last_msg))).unwrap_or_default();
                    let sub = format!("{} message{}{}", c.msg_count, if c.msg_count == 1 { "" } else { "s" }, if when.is_empty() { String::new() } else { format!(" • {when}") });
                    ui.painter().text(pos2(r.left() + 16.0, r.top() + 44.0), Align2::LEFT_CENTER, sub, reg(14.0), pal().header_secondary);
                    if resp.clicked() {
                        open = Some(c.id);
                    }
                }
            });
        });
        if open.is_some() || create {
            ui.close();
        }
    });
    if let Some(c) = open {
        app.open(c);
    }
    if create {
        app.new_thread = Some(NewThread::blank(ch));
    }
}

/// "5m ago", "3h ago", "2d ago", the way discord shortens recent times.
fn ago(ms: i64) -> String {
    let s = (chrono::Utc::now().timestamp_millis() - ms).max(0) / 1000;
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
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
            let sized = |size: egui::Vec2| {
                let sep = if img.contains('?') { '&' } else { '?' };
                if w > 0 && !img.contains(".mp4") && !crate::plugins::on("FixImagesQuality") { format!("{img}{sep}width={}&height={}", (size.x * 1.5) as u32, (size.y * 1.5) as u32) } else { img.clone() }
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
            let lens = crate::plugins::on("ImageZoom");
            if lens {
                super::vc::lens(ui, rect, &sized(size * 3.0));
            }
            if ir.hovered() {
                ui.ctx().set_cursor_icon(if lens { egui::CursorIcon::Crosshair } else { egui::CursorIcon::ZoomIn });
            }
            if ir.clicked() && !lens {
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

enum UserAct {
    Read,
    Profile,
    Mention,
    Message,
    Close,
    AddFriend,
    RemoveFriend,
    Block,
    Unblock,
    Mute(bool, Option<i64>),
    CopyUser,
    CopyChannel,
    Timeout,
    Kick,
    Ban,
    Perms,
    CopyUrl,
    View(String),
    Pin(bool),
    Shift(i32),
}

/// Discord's menu for a person, from the member list, a message's author or a dm row.
pub fn user_menu(app: &mut App, ctx: &egui::Context) {
    use super::{Trail, menu_item, menu_row, menu_sep};
    use crate::state::{BLOCKED, FRIEND, INCOMING, OUTGOING};
    let Some((uid, gid, dm, at)) = app.user_menu else { return };
    let me = uid == app.store.me;
    let rel = app.store.relationships.get(&uid).copied();
    let name = app.store.name_in(gid, uid);
    let chan = dm.and_then(|c| app.store.channels.get(&c));
    let group = chan.is_some_and(|c| c.kind == 3);
    let unread = chan.is_some_and(|c| app.store.unread(c));
    let muted = dm.is_some_and(|c| app.store.muted.contains(&c));
    let composing = dm.is_none() && app.cur().is_some();
    let dev = app.settings.dev;
    // a server's moderators get timeout, kick and ban for members other than themselves
    let perms = app.store.guilds.get(&gid).and_then(|g| g.channels.iter().find_map(|c| app.store.channels.get(c))).map_or(0, |c| app.store.perms(c));
    let (can_timeout, can_kick, can_ban) = if gid == 0 || me {
        (false, false, false)
    } else {
        use crate::state::{BAN_MEMBERS, KICK_MEMBERS, MODERATE_MEMBERS};
        (perms & MODERATE_MEMBERS != 0, perms & KICK_MEMBERS != 0, perms & BAN_MEMBERS != 0)
    };
    let mut act = None;
    let avatar_url = app.store.user(uid).map(|u| match &u.avatar {
        Some(h) => super::extra::icon_url("avatars", uid, h),
        None => u.avatar_url(),
    }).unwrap_or_default();
    let group_icon = chan.filter(|c| c.kind == 3 && plugins::on("ViewIcons")).and_then(|c| c.icon.as_ref().map(|h| super::extra::icon_url("channel-icons", c.id, h)));
    let decoration = app.store.user(uid).and_then(|u| u.decoration.as_ref().map(|a| format!("{}/avatar-decoration-presets/{a}.png?size=4096&passthrough=true", crate::api::cdn())));
    let open = super::float_menu(ctx, "user_menu", at, |ui| {
        if dm.is_some() {
            if ui.add_enabled_ui(unread, |ui| menu_item(ui, None, "Mark As Read", false)).inner {
                act = Some(UserAct::Read);
            }
            menu_sep(ui);
        }
        if !group {
            if menu_item(ui, None, "Profile", false) {
                act = Some(UserAct::Profile);
            }
            if composing && menu_item(ui, None, "Mention", false) {
                act = Some(UserAct::Mention);
            }
            if !me && dm.is_none() && menu_item(ui, None, "Message", false) {
                act = Some(UserAct::Message);
            }
        }
        if dm.is_some() && menu_item(ui, None, if group { "Leave Group" } else { "Close DM" }, group) {
            act = Some(UserAct::Close);
        }
        if !me && !group {
            menu_sep(ui);
            match rel {
                Some(FRIEND) => {
                    if menu_item(ui, None, "Remove Friend", false) {
                        act = Some(UserAct::RemoveFriend);
                    }
                }
                Some(BLOCKED) | Some(OUTGOING) => {}
                Some(INCOMING) => {
                    if menu_item(ui, None, "Accept Friend Request", false) {
                        act = Some(UserAct::AddFriend);
                    }
                }
                _ => {
                    if menu_item(ui, None, "Add Friend", false) {
                        act = Some(UserAct::AddFriend);
                    }
                }
            }
            if rel == Some(BLOCKED) {
                if menu_item(ui, None, "Unblock", false) {
                    act = Some(UserAct::Unblock);
                }
            } else if menu_item(ui, None, "Block", true) {
                act = Some(UserAct::Block);
            }
        }
        if can_timeout || can_kick || can_ban {
            menu_sep(ui);
            if can_timeout && menu_item(ui, None, &format!("Timeout {name}"), true) {
                act = Some(UserAct::Timeout);
            }
            if can_kick && menu_item(ui, None, &format!("Kick {name}"), true) {
                act = Some(UserAct::Kick);
            }
            if can_ban && menu_item(ui, None, &format!("Ban {name}"), true) {
                act = Some(UserAct::Ban);
            }
        }
        if dm.is_some() {
            menu_sep(ui);
            let what = if group { "Conversation".to_string() } else { format!("@{name}") };
            if muted {
                if menu_item(ui, None, &format!("Unmute {what}"), false) {
                    act = Some(UserAct::Mute(false, None));
                }
            } else {
                let m = menu_row(ui, None, &format!("Mute {what}"), None, false, Trail::Caret);
                egui::containers::menu::SubMenu::new().show(ui, &m, |ui| {
                    if let Some(secs) = super::mute_menu(ui) {
                        act = Some(UserAct::Mute(true, secs));
                    }
                });
            }
        }
        if let Some(url) = &group_icon {
            menu_sep(ui);
            if menu_item(ui, None, "View Icon", false) {
                act = Some(UserAct::View(url.clone()));
            }
        }
        if !group && (plugins::on("ViewIcons") || (plugins::on("PermissionsViewer") && gid != 0) || plugins::on("CopyUserURLs")) {
            menu_sep(ui);
            if plugins::on("ViewIcons") {
                if menu_item(ui, None, "View Avatar", false) {
                    act = Some(UserAct::View(avatar_url.clone()));
                }
                if let Some(d) = decoration.as_ref() {
                    if menu_item(ui, None, "View Avatar Decoration", false) {
                        act = Some(UserAct::View(d.clone()));
                    }
                }
            }
            if plugins::on("PermissionsViewer") && gid != 0 && menu_item(ui, None, "View Permissions", false) {
                act = Some(UserAct::Perms);
            }
            if plugins::on("CopyUserURLs") && menu_item(ui, None, "Copy User URL", false) {
                act = Some(UserAct::CopyUrl);
            }
        }
        if let Some(c) = dm.filter(|_| plugins::on("PinDMs")) {
            menu_sep(ui);
            let pins = super::extra::pinned(&app.store);
            let at = pins.iter().position(|p| *p == c);
            if menu_item(ui, None, if at.is_some() { "Unpin DM" } else { "Pin DM" }, false) {
                act = Some(UserAct::Pin(at.is_none()));
            }
            if let Some(i) = at.filter(|_| plugins::text("PinDMs", "pinOrder") == "1") {
                if i > 0 && menu_item(ui, None, "Move Up", false) {
                    act = Some(UserAct::Shift(-1));
                }
                if i + 1 < pins.len() && menu_item(ui, None, "Move Down", false) {
                    act = Some(UserAct::Shift(1));
                }
            }
        }
        if dev {
            menu_sep(ui);
            if !group && menu_item(ui, Some(Icon::Id), "Copy User ID", false) {
                act = Some(UserAct::CopyUser);
            }
            if dm.is_some() && menu_item(ui, Some(Icon::Id), "Copy Channel ID", false) {
                act = Some(UserAct::CopyChannel);
            }
        }
    });
    if !open || act.is_some() {
        app.user_menu = None;
    }
    let dm = dm.unwrap_or(0);
    match act {
        Some(UserAct::Read) => app.mark_read(&[dm]),
        Some(UserAct::Profile) => app.open_profile(uid, gid, at),
        Some(UserAct::Mention) => app.mention(uid, gid),
        // ctrl+click times them out for a minute without asking how long
        Some(UserAct::Timeout) if ctx.input(|i| i.modifiers.ctrl) => app.time_out(gid, uid, 60),
        Some(UserAct::Timeout) => app.timeout = Some(Timeout { guild: gid, user: uid, secs: 60, reason: String::new() }),
        Some(UserAct::Kick) => {
            let server = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
            let body = format!("Are you sure you want to kick @{name} from the server? They will be able to rejoin again with a new invite.");
            app.confirm = Some(super::Confirm::new(format!("Kick '{name}' from {server}"), body, "Kick", move |app: &mut App| {
                app.rest("DELETE", format!("/guilds/{gid}/members/{uid}"), None, Tag::Quiet)
            }));
        }
        Some(UserAct::Ban) => {
            let server = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
            let body = format!("Are you sure you want to ban @{name} from {server}?");
            app.confirm = Some(super::Confirm::new(format!("Ban '{name}'"), body, "Ban", move |app: &mut App| {
                app.rest("PUT", format!("/guilds/{gid}/bans/{uid}"), Some(json!({ "delete_message_seconds": 0 })), Tag::Quiet)
            }));
        }
        Some(UserAct::Message) => app.open_dm(uid),
        Some(UserAct::Close) if group => {
            let title = channel_title(&app.store, dm);
            let body = format!("Are you sure you want to leave {title}? You won't be able to rejoin this group unless you are re-invited.");
            app.confirm = Some(super::Confirm::new(format!("Leave '{title}'"), body, "Leave Group", move |app: &mut App| close_dm(app, dm)));
        }
        Some(UserAct::Close) => close_dm(app, dm),
        Some(UserAct::AddFriend) => app.rest("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({})), Tag::Friend),
        Some(UserAct::RemoveFriend) => {
            let body = format!("Are you sure you want to remove {name} from your friends?");
            app.confirm = Some(super::Confirm::new(format!("Remove '{name}'"), body, "Remove Friend", move |app: &mut App| {
                app.rest("DELETE", format!("/users/@me/relationships/{uid}"), None, Tag::Quiet)
            }));
        }
        Some(UserAct::Block) => {
            let body = "They won't be able to message you, and you won't see their messages unless you choose to.";
            app.confirm = Some(super::Confirm::new(format!("Block {name}?"), body, "Block", move |app: &mut App| {
                app.rest("PUT", format!("/users/@me/relationships/{uid}"), Some(json!({ "type": 2 })), Tag::Quiet)
            }));
        }
        Some(UserAct::Unblock) => app.rest("DELETE", format!("/users/@me/relationships/{uid}"), None, Tag::Quiet),
        Some(UserAct::Mute(m, secs)) => app.mute_for(dm, false, m, secs),
        Some(UserAct::Perms) => app.view_user_perms(gid, uid),
        Some(UserAct::CopyUrl) => ctx.copy_text(format!("https://discord.com/users/{uid}")),
        Some(UserAct::View(url)) => super::extra::view(app, url),
        Some(UserAct::Pin(on)) => super::extra::pin(dm, on),
        Some(UserAct::Shift(by)) => super::extra::shift_pin(dm, by),
        Some(UserAct::CopyUser) => ctx.copy_text(uid.to_string()),
        Some(UserAct::CopyChannel) => ctx.copy_text(dm.to_string()),
        None => {}
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
    let modal = egui::Modal::new("timeout".into())
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
                    ui.painter().rect(r, cr, if on { BLURPLE } else if resp.hovered() { pal().control_hover } else { pal().control }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
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
                go = button(ui, "Timeout", BLURPLE, 212.0).clicked();
            });
        });
    if go && let Some(t) = app.timeout.take() {
        app.time_out(t.guild, t.user, t.secs);
    } else if close || modal.should_close() {
        app.timeout = None;
    }
}

fn close_dm(app: &mut App, c: Id) {
    app.rest("DELETE", format!("/channels/{c}"), None, Tag::Quiet);
    app.store.dms.retain(|d| *d != c);
    if app.cur() == Some(c) {
        app.friends = true;
    }
}

pub struct Forward {
    ch: Id,
    msg: Id,
    query: String,
    picked: Vec<Id>,
    note: String,
}

impl Forward {
    pub fn new(ch: Id, msg: Id) -> Self {
        Forward { ch, msg, query: String::new(), picked: Vec::new(), note: String::new() }
    }
}

/// Discord's Forward To: up to five places, then an optional note under the forwarded message.
fn forward(app: &mut App, ctx: &egui::Context) {
    let Some(f) = &mut app.forward else { return };
    let store = &app.store;
    let ql = f.query.to_lowercase();
    // where you've been lately, then dms, then every channel you can post in
    let mut seen = std::collections::HashSet::new();
    let places: Vec<Id> = app
        .ui
        .recent
        .iter()
        .chain(store.dms.iter())
        .chain(store.guild_order.iter().filter_map(|g| store.guilds.get(g)).flat_map(|g| g.channels.iter()))
        .copied()
        .filter(|c| seen.insert(*c))
        .filter(|c| store.channels.get(c).is_some_and(|c| (c.is_text() && !c.is_thread()) && (c.guild == 0 || store.can_view(c))))
        .filter(|c| ql.is_empty() || channel_title(store, *c).to_lowercase().contains(&ql))
        .take(60)
        .collect();
    let preview = store.find_msg(f.ch, f.msg).cloned();
    let (mut send, mut close) = (false, false);
    // measured from discord's: 478 wide, 24 in, 48 tall rows, the forwarded text quoted at the foot
    let modal = egui::Modal::new("forward".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(478.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 24, bottom: 16 }).show(ui, |ui| {
                ui.set_width(430.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Forward To").font(semi(20.0)).color(pal().header_primary));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 24.0, "Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Select where you want to share this message.").font(reg(16.0)).color(pal().header_secondary));
                ui.add_space(16.0);
                let (field, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
                ui.painter().rect_filled(field, 8, pal().input_bg);
                let inner = Rect::from_min_max(pos2(field.left() + 38.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
                let r = ui.put(
                    inner,
                    egui::TextEdit::singleline(&mut f.query)
                        .hint_text(RichText::new("Search").color(pal().text_muted))
                        .font(reg(16.0))
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO),
                );
                r.request_focus();
                ui.painter().rect_stroke(field, 8, egui::Stroke::new(1.0, if r.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
                super::icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 20.0, field.center().y), vec2(16.0, 16.0)), pal().header_primary);
            });
            let room = (ui.ctx().content_rect().height() - 360.0).clamp(144.0, 400.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 0)).show(ui, |ui| {
                    for c in &places {
                        let Some(ch) = store.channels.get(c) else { continue };
                        let (r, resp) = ui.allocate_exact_size(vec2(430.0, 48.0), Sense::click());
                        let on = f.picked.contains(c);
                        if resp.hovered() {
                            ui.painter().rect_filled(r.expand2(vec2(8.0, 0.0)), 8, MENU_HOVER);
                        }
                        let icon = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(32.0, 32.0));
                        match (ch.recipients.first().and_then(|u| store.user(*u)), store.guilds.get(&ch.guild)) {
                            (Some(u), _) if ch.kind == 1 => avatar(ui, icon, &u.avatar_url()),
                            (_, Some(g)) => {
                                // the server's icon with the channel's glyph in a badge at its corner
                                match g.icon_url() {
                                    Some(url) => {
                                        egui::Image::from_uri(url).corner_radius(CornerRadius::same(16)).show_loading_spinner(false).paint_at(ui, icon);
                                    }
                                    None => {
                                        ui.painter().circle_filled(icon.center(), 16.0, pal().bg_accent);
                                        ui.painter().text(icon.center(), Align2::CENTER_CENTER, g.acronym(), semi(11.0), pal().text_normal);
                                    }
                                }
                                let b = pos2(icon.right() - 4.0, icon.bottom() - 4.0);
                                ui.painter().circle_filled(b, 9.0, pal().bg_secondary);
                                super::side::chan_icon(store, ui.painter(), ch, Rect::from_center_size(b, vec2(12.0, 12.0)), pal().header_secondary);
                            }
                            _ => super::icons::draw(ui.painter(), Icon::People, icon.shrink(6.0), pal().header_secondary),
                        }
                        let title = channel_title(store, *c);
                        let sub = store.guilds.get(&ch.guild).map(|g| g.name.clone());
                        let y = if sub.is_some() { r.center().y - 9.0 } else { r.center().y };
                        text::paint(ui.painter(), pos2(r.left() + 44.0, y), Align2::LEFT_CENTER, &title, semi(16.0), pal().header_primary, 340.0);
                        if let Some(sub) = sub {
                            text::paint(ui.painter(), pos2(r.left() + 44.0, r.center().y + 10.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().header_secondary, 340.0);
                        }
                        let b = Rect::from_center_size(pos2(r.right() - 10.0, r.center().y), vec2(20.0, 20.0));
                        if on {
                            ui.painter().rect_filled(b, 6, BLURPLE);
                            super::icons::draw(ui.painter(), Icon::Check, b.shrink(3.0), egui::Color32::WHITE);
                        } else {
                            ui.painter().rect_stroke(b, 6, egui::Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            if on {
                                f.picked.retain(|x| x != c);
                            } else if f.picked.len() < 5 {
                                f.picked.push(*c);
                            }
                        }
                    }
                    if places.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No results found").color(pal().header_secondary)));
                        ui.add_space(24.0);
                    }
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(478.0, 1.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, pal().border_subtle));
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 24 }).show(ui, |ui| {
                ui.set_width(430.0);
                if let Some(m) = &preview {
                    // quoted like discord's: a bar down the left and the message beside it
                    let guild = store.channels.get(&f.ch).map_or(0, |c| c.guild);
                    let body = if m.content.is_empty() { "Attachment".to_string() } else { crate::markdown::plain(&m.content, store, guild) };
                    let (r, _) = ui.allocate_exact_size(vec2(430.0, 24.0), Sense::hover());
                    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(4.0, 24.0)), 2, pal().border_normal);
                    text::paint(ui.painter(), pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &body, reg(16.0), pal().text_normal, 410.0);
                    ui.add_space(16.0);
                }
                {
                    let n = f.picked.len();
                    // one row cut by hand: put() would push a horizontal layout past the field
                    let (row, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
                    let field = Rect::from_min_size(row.min, vec2(320.0, 40.0));
                    ui.painter().rect(field, 8, pal().input_bg, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let inner = Rect::from_min_max(pos2(field.left() + 16.0, field.center().y - 10.0), pos2(field.right() - 44.0, field.center().y + 10.0));
                    ui.put(
                        inner,
                        egui::TextEdit::singleline(&mut f.note)
                            .hint_text(RichText::new("Add an optional message...").color(pal().text_muted))
                            .font(reg(16.0))
                            .frame(Frame::NONE)
                            .margin(Margin::ZERO),
                    );
                    super::icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(pos2(field.right() - 22.0, field.center().y), vec2(20.0, 20.0)), pal().header_secondary);
                    let b = Rect::from_min_size(pos2(field.right() + 10.0, row.top()), vec2(100.0, 40.0));
                    let resp = ui.interact(b, ui.id().with("forward_send"), Sense::click());
                    let fill = if n == 0 { BLURPLE.gamma_multiply(0.5) } else if resp.hovered() { BLURPLE.gamma_multiply(0.85) } else { BLURPLE };
                    ui.painter().rect_filled(b, 8, fill);
                    let label = if n > 1 { format!("Send ({n})") } else { "Send".to_string() };
                    let g = ui.painter().layout_no_wrap(label, medium(16.0), egui::Color32::WHITE);
                    let x = b.center().x - (g.size().x + 26.0) / 2.0;
                    let w = g.size().x;
                    ui.painter().galley(pos2(x, b.center().y - g.size().y / 2.0), g, egui::Color32::WHITE);
                    super::icons::draw(ui.painter(), Icon::Send, Rect::from_center_size(pos2(x + w + 16.0, b.center().y), vec2(20.0, 20.0)), egui::Color32::WHITE);
                    send = resp.clicked() && n > 0;
                }
            });
        });
    if send {
        let f = app.forward.take().expect("checked above");
        let guild = app.store.channels.get(&f.ch).map_or(0, |c| c.guild);
        for to in f.picked {
            let mut reference = json!({ "type": 1, "message_id": f.msg.to_string(), "channel_id": f.ch.to_string() });
            if guild != 0 {
                reference["guild_id"] = json!(guild.to_string());
            }
            app.rest("POST", format!("/channels/{to}/messages"), Some(json!({ "content": "", "message_reference": reference })), Tag::Quiet);
            if !f.note.trim().is_empty() {
                app.rest("POST", format!("/channels/{to}/messages"), Some(json!({ "content": f.note.trim() })), Tag::Quiet);
            }
        }
        app.toast("Message forwarded.");
    } else if close || modal.should_close() {
        app.forward = None;
    }
}

/// Create Thread, from a message's menu or the threads popout: the message it starts from,
/// a name (the message's own words if left empty) and the first thing to say in it.
pub struct NewThread {
    ch: Id,
    msg: Option<Id>,
    name: String,
    first: String,
    hint: String,
}

impl NewThread {
    pub fn from(app: &App, ch: Id, msg: Id) -> Self {
        let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
        let words = app.store.find_msg(ch, msg).map(|m| crate::markdown::plain(&m.content, &app.store, guild)).unwrap_or_default();
        let hint: String = words.chars().take(40).collect();
        NewThread { ch, msg: Some(msg), name: String::new(), first: String::new(), hint: if hint.trim().is_empty() { "New Thread".into() } else { hint } }
    }

    pub fn blank(ch: Id) -> Self {
        NewThread { ch, msg: None, name: String::new(), first: String::new(), hint: "New Thread".into() }
    }
}

fn new_thread(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &mut app.new_thread else { return };
    let msg = t.msg.and_then(|m| app.store.find_msg(t.ch, m).cloned());
    let (mut make, mut close) = (false, false);
    let modal = egui::Modal::new("new_thread".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(egui::Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(432.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                super::icons::draw(ui.painter(), Icon::ThreadPlus, r, pal().header_secondary);
                ui.add_space(8.0);
                ui.label(RichText::new("New Thread").font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            if let Some(m) = &msg {
                ui.add_space(16.0);
                chat::preview(ui, &app.store, m);
            }
            ui.add_space(20.0);
            ui.label(RichText::new("Thread Name").font(semi(12.0)).color(pal().header_secondary));
            ui.add_space(8.0);
            let field = |ui: &mut Ui, text: &mut String, hint: &str, focus: bool| {
                let (r, _) = ui.allocate_exact_size(vec2(432.0, 40.0), Sense::hover());
                ui.painter().rect_filled(r, 8, pal().input_bg);
                let e = ui.put(
                    r.shrink2(vec2(12.0, 10.0)),
                    egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO),
                );
                ui.painter().rect_stroke(r, 8, egui::Stroke::new(1.0, if e.has_focus() { BLURPLE } else { pal().border_subtle }), egui::StrokeKind::Inside);
                if focus {
                    e.request_focus();
                }
                e
            };
            let first_frame = !ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("new_thread_seen")).unwrap_or(false));
            let n = field(ui, &mut t.name, &t.hint, first_frame);
            ui.add_space(16.0);
            let f = field(ui, &mut t.first, "Enter a message to start the conversation!", false);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("new_thread_seen"), true));
            let enter = (n.lost_focus() || f.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                make = button(ui, "Create Thread", BLURPLE, 128.0).clicked() || enter;
                ui.add_space(8.0);
                close |= super::grey_button(ui, "Cancel", 92.0).clicked();
            });
        });
    let ch = t.ch;
    if make {
        let name = if t.name.trim().is_empty() { t.hint.clone() } else { t.name.trim().to_string() };
        let first = std::mem::take(&mut t.first);
        let (path, body) = match t.msg {
            Some(m) => (format!("/channels/{ch}/messages/{m}/threads"), json!({ "name": name, "auto_archive_duration": 4320 })),
            None => (format!("/channels/{ch}/threads"), json!({ "name": name, "type": 11, "auto_archive_duration": 4320 })),
        };
        app.rest("POST", path, Some(body), Tag::Thread(first));
        close = true;
    }
    if close || modal.should_close() {
        app.new_thread = None;
        ctx.data_mut(|d| d.remove::<bool>(egui::Id::new("new_thread_seen")));
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
        let trusted = ours || plugins::flag("AlwaysTrust", "domain") || self.settings.trusted.iter().any(|t| *t == h);
        if (masked || stranger) && !trusted {
            self.leaving = Some((url.to_string(), false));
        } else {
            super::extra::open(ctx, url);
        }
    }
}

/// "Leaving Discord": the address with its site in bold, a box to trust that site from now on,
/// Go Back and Visit Site.
fn leaving(app: &mut App, ctx: &egui::Context) {
    let Some((url, trust)) = &mut app.leaving else { return };
    let h = host(url);
    let (mut go, mut close) = (false, false);
    let modal = egui::Modal::new("leaving".into())
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
                ui.painter().rect_filled(b, 6, BLURPLE);
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
                let v = button(ui, "Visit Site", BLURPLE, 212.0);
                app.rects.insert("leaving:visit".into(), v.rect);
                go = v.clicked();
            });
        });
    if go && let Some((url, trust)) = app.leaving.take() {
        if trust && !app.settings.trusted.contains(&h) {
            app.settings.trusted.push(h);
            crate::cache::set_settings(&app.settings);
        }
        super::extra::open(ctx, &url);
    } else if close || modal.should_close() {
        app.leaving = None;
    }
}

/// A custom emoji's popout: the emoji, its name, and where it's from when that's a server you're
/// in, else that it's from one you can't see.
fn emoji_pop(app: &mut App, ctx: &egui::Context) {
    let Some((id, name, animated, at)) = app.emoji_pop.clone() else { return };
    let mut dismiss = false;
    let from = app.store.guilds.values().find(|g| g.emojis.iter().any(|e| e.id == id)).map(|g| (g.name.clone(), g.icon_url()));
    let area = egui::Area::new(egui::Id::new("emoji_pop")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(16).show(ui, |ui| {
            ui.set_width(268.0);
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                egui::Image::from_uri(crate::gif::uri(ui.ctx(), &crate::markdown::emoji_url(id, animated), true)).show_loading_spinner(false).paint_at(ui, r);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(format!(":{name}:")).font(semi(16.0)).color(pal().header_primary));
                    match &from {
                        Some((g, _)) => {
                            ui.label(RichText::new("This emoji is from").font(reg(14.0)).color(pal().text_normal));
                            ui.label(RichText::new(g).font(semi(14.0)).color(pal().header_primary));
                        }
                        None => {
                            ui.label(RichText::new("This emoji is from a server that is either invite-only or unavailable.").font(reg(14.0)).color(pal().text_normal));
                        }
                    }
                });
            });
            let link = |ui: &mut Ui, label: &str| ui.add(egui::Label::new(RichText::new(label).font(medium(14.0)).color(pal().link)).sense(Sense::click())).clicked();
            if crate::plugins::on("CopyEmojiMarkdown") || crate::plugins::on("ExpressionCloner") {
                ui.add_space(12.0);
            }
            if crate::plugins::on("CopyEmojiMarkdown") && link(ui, "Copy Emoji Markdown") {
                ctx.copy_text(super::expr::emoji_markdown(id, &name, animated));
                dismiss = true;
            }
            if crate::plugins::on("ExpressionCloner") && link(ui, "Clone Emoji") {
                super::expr::ask(ctx, super::expr::Cloning::Emoji(id, name.clone(), animated));
                dismiss = true;
            }
        });
    });
    app.rects.insert("emoji_pop".into(), area.response.rect);
    if dismiss || dismissed(ctx, &area.response) {
        app.emoji_pop = None;
    }
}
