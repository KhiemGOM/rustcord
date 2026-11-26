//! A voice channel's call as Discord lays it out: every person and every stream is a tile in one
//! grid, or the picked one is big over a strip of all of them; the header and the controls float
//! over the tiles and fade while the mouse rests.

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use super::layout::{WIDE, focus, grid};
use super::{awake, frame, gradient, live_badge, preview, suggest, tile_color};
use crate::state::{Channel, Id};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, avatar};

const PREVIEW: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x22);
const INK: Color32 = Color32::from_rgb(0xdc, 0xdc, 0xdf);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tile {
    Stream(Id),
    User(Id),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Size {
    Grid,
    Big,
    Strip,
}

/// Discord's sort key: streams first (a streamer with a camera on ahead), then people with a camera
/// on, then people streaming, then everyone else, each by name compared as plain text.
fn key(stream: bool, name: &str, cam: bool, live: bool) -> String {
    match stream {
        true => format!("{}{name}\u{3}", if cam { '\u{2}' } else { '\u{3}' }),
        false => format!("{}{name}", if cam { '\u{3}' } else if live { '\u{4}' } else { '\u{5}' }),
    }
}

fn order(app: &App, guild: Id, members: &[Id]) -> Vec<Tile> {
    let v = &app.voice;
    let mut keyed = Vec::new();
    for &u in members {
        let (name, cam, live) = (app.store.name_in(guild, u), v.cams.contains(&u), v.live.contains(&u));
        keyed.push((key(false, &name, cam, live), Tile::User(u)));
        if live {
            keyed.push((key(true, &name, cam, live), Tile::Stream(u)));
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, t)| t).collect()
}

/// The strip's own order in Discord: streams, then whoever is speaking, has spoken in the last 10
/// seconds or has a camera on, the latest speaker first (and anyone who never spoke ahead of them,
/// as Discord's key comes out), then by name.
fn strip_key(stream: bool, name: &str, speaking: bool, spoke: Option<i64>, now: i64, cam: bool) -> String {
    if stream {
        return format!("\u{2}{name}");
    }
    let class = if speaking { '\u{3}' } else if spoke.is_some_and(|t| now - t < 10_000) { '\u{4}' } else if cam { '\u{5}' } else { '\u{6}' };
    let when = spoke.map_or_else(|| "0000000000000NaN".to_string(), |t| format!("{:016}", 8_640_000_000_000_000 - t));
    format!("{class}{when}{name}")
}

/// What the strip shows, in its order; our own camera, if on, is always last and always shown.
fn strip(app: &App, guild: Id, list: &[Tile], fit: usize) -> Vec<Tile> {
    let (v, me, now) = (&app.voice, app.store.me, chrono::Utc::now().timestamp_millis());
    let mut out = list.to_vec();
    out.sort_by_cached_key(|t| match *t {
        Tile::Stream(u) => strip_key(true, &app.store.name_in(guild, u), false, None, now, false),
        Tile::User(u) => strip_key(false, &app.store.name_in(guild, u), v.is_speaking(u), v.spoke.get(&u).copied(), now, v.cams.contains(&u)),
    });
    if let Some(i) = out.iter().position(|t| *t == Tile::User(me) && v.cams.contains(&me)) {
        let mine = out.remove(i);
        out.truncate(fit.saturating_sub(1));
        out.push(mine);
    }
    out.truncate(fit);
    out
}

fn aspect(app: &App, t: Tile) -> f32 {
    let img = match t {
        Tile::Stream(_) => app.voice.stream.as_ref().and_then(|l| l.frame.clone()),
        Tile::User(u) => app.voice.video.get(&u).cloned(),
    };
    img.map_or(WIDE, |i| i.size()[0] as f32 / i.size()[1].max(1) as f32)
}

/// Voice channel view.
pub fn room(app: &mut App, ui: &mut Ui, chan: &Channel) {
    if chan.kind == 13 {
        return crate::ui::stage::room(app, ui, chan);
    }
    let inside = app.voice.chan == Some(chan.id);
    let members = app.voice.members(&app.store, chan.id);
    let shown = if !inside || members.is_empty() { 1.0 } else { awake(ui, ui.max_rect()) };
    egui::CentralPanel::default().frame(Frame::new().fill(Color32::BLACK)).show(ui, |ui| {
        let r = ui.max_rect();
        let body = Rect::from_min_max(r.min + vec2(0.0, 48.0), r.max);
        // an empty channel is its name, a line and the way in, stacked about the middle
        let empty = members.is_empty();
        let activity = app.activity.runs_in(chan.id);
        let outside = !inside && !activity;
        let drop = if empty { 0.0 } else { 49.0 };
        if outside {
            preview::backdrop(ui, body);
        }
        if empty || outside {
            if !empty {
                preview::cards(app, ui, chan, &members, body.center().y - 147.5, body.center().x);
            }
            crate::ui::text::paint_em(ui.painter(), body.center() - vec2(0.0, 55.0 - drop), Align2::CENTER_CENTER, &chan.name, semi(32.0), pal().header_primary, body.width() - 32.0, 1.0);
            ui.painter().text(body.center() - vec2(0.0, 17.5 - drop), Align2::CENTER_CENTER, preview::who(app, chan.guild, &members), reg(14.0), pal().text_normal);
        }
        let mut list = if outside { Vec::new() } else { order(app, chan.guild, &members) };
        let cam = |app: &App, u: &Id| app.voice.video.contains_key(u) || app.voice.cams.contains(u);
        if app.settings.hide_idle_tiles && list.iter().any(|t| matches!(t, Tile::Stream(_)) || matches!(t, Tile::User(u) if cam(app, u))) {
            list.retain(|t| matches!(t, Tile::Stream(_)) || matches!(t, Tile::User(u) if cam(app, u)));
        }
        let streaming = app.voice.stream.as_ref().filter(|_| inside).map(|l| (l.user, l.mine));
        if let Some((u, _)) = streaming.filter(|(u, _)| !list.contains(&Tile::Stream(*u))) {
            list.insert(0, Tile::Stream(u));
        }
        let watching = streaming.filter(|(_, mine)| !mine).map(|(u, _)| u);
        if app.call.focus.is_some_and(|f| !list.contains(&Tile::User(f))) {
            app.call.focus = None;
        }
        let picked = if activity { None } else { watching.map(Tile::Stream).or(app.call.focus.map(Tile::User)) };
        let idle: Vec<(Id, Vec<Id>)> = match activity || picked.is_some() {
            true => Vec::new(),
            false => app.store.embedded.get(&chan.id).into_iter().flatten().map(|i| (i.app, i.users.clone())).collect(),
        };
        if activity {
            let (big, rects) = focus(r, r.width() / (r.height() - 280.0).max(1.0), list.len());
            crate::ui::activity::view::stage(app, ui, big);
            for (t, s) in strip(app, chan.guild, &list, rects.len()).into_iter().zip(rects) {
                draw(app, ui, chan, t, s, shown, Size::Strip, false);
            }
        } else if let Some(p) = picked {
            let (big, rects) = focus(r, aspect(app, p), list.len());
            draw(app, ui, chan, p, big, shown, Size::Big, true);
            for (t, s) in strip(app, chan.guild, &list, rects.len()).into_iter().zip(rects) {
                draw(app, ui, chan, t, s, shown, Size::Strip, t == p);
            }
        } else {
            let extra = !outside && suggest::wanted(app, ui.ctx(), chan, members.len());
            let mut tiles = grid(r, idle.len() + list.len() + extra as usize).into_iter();
            for ((a, users), tile) in idle.iter().zip(tiles.by_ref()) {
                crate::ui::activity::view::join_tile(app, ui, chan.id, tile, *a, users);
            }
            for (t, s) in list.iter().zip(tiles.by_ref()) {
                draw(app, ui, chan, *t, s, shown, Size::Grid, false);
            }
            if let Some(s) = tiles.next().filter(|_| extra) {
                suggest::tile(app, ui, chan, s, members.len());
            }
        }
        if shown > 0.0 && !outside {
            if !activity {
                gradient(ui, r, true, shown);
            }
            gradient(ui, r, false, shown);
        }
        ui.scope(|ui| {
            ui.multiply_opacity(shown);
            head(app, ui, chan, Rect::from_min_size(r.min, vec2(r.width(), 48.0)), watching.filter(|u| *u != app.store.me), Icon::Speaker);
        });
        // the join button stays put; once in, the controls fade with the rest
        if inside {
            ui.multiply_opacity(shown);
        }
        if (empty || outside) && !inside {
            let label = ui.painter().layout_no_wrap("Join Voice".into(), medium(16.0), Color32::BLACK);
            let b = Rect::from_center_size(body.center() + vec2(0.0, 35.5 + drop), vec2(label.size().x + 32.0, 40.0));
            let hit = ui.interact(b, egui::Id::new(("join", chan.id)), Sense::click());
            ui.painter().rect_filled(b, CornerRadius::same(8), crate::ui::motion::mix(Color32::WHITE, Color32::from_gray(0xdf), crate::ui::motion::hot(ui.ctx(), &hit)));
            ui.painter().galley(b.center() - label.size() / 2.0, label, Color32::BLACK);
            app.rects.insert("voice:join".into(), b);
            if hit.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hit.clicked() {
                app.join_voice(chan.id, true);
            }
        } else {
            crate::ui::callbar::controls(app, ui, chan.id, Rect::from_min_max(pos2(r.left(), r.bottom() - 72.0), r.max), "Join Voice");
        }
    });
}

/// The call's header: the channel, and while watching someone, a dot, their face and whose screen
/// it is, with the LIVE pill beside Show Chat.
pub fn head(app: &mut App, ui: &mut Ui, chan: &Channel, bar: Rect, watching: Option<Id>, icon: Icon) {
    let cy = bar.center().y;
    let p = ui.painter().clone();
    icons::draw(&p, icon, Rect::from_min_size(pos2(bar.left() + 16.0, cy - 10.0), vec2(20.0, 20.0)), pal().channel_default);
    let mut x = bar.left() + 44.0;
    let mut right = bar.right() - 16.0;
    if !app.voice_chat {
        let b = Rect::from_min_size(pos2(right - 24.0, cy - 12.0), vec2(24.0, 24.0));
        let resp = ui.interact(b, egui::Id::new("voice:show_chat"), Sense::click());
        icons::draw(&p, Icon::Bubble, b, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
        app.rects.insert("voice:show_chat".into(), b);
        if resp.on_hover_text("Show Chat").clicked() {
            app.voice_chat = true;
        }
        right = b.left() - 16.0;
    }
    if watching.is_some() {
        live_badge(ui, pos2(right, cy - 8.0));
        right -= 58.0;
    }
    let t = crate::ui::text::layout_em(ui.ctx(), &chan.name, medium(16.0), pal().header_primary, (right - x).max(0.0), 1.0);
    t.paint(&p, pos2(x, cy - t.size().y / 2.0));
    x += t.size().x;
    let Some(u) = watching else { return };
    p.circle_filled(pos2(x + 14.0, cy), 2.0, pal().header_secondary);
    if let Some(url) = app.store.avatar_url_in(chan.guild, u) {
        avatar(ui, Rect::from_min_size(pos2(x + 28.0, cy - 12.0), vec2(24.0, 24.0)), &url);
    }
    let whose = format!("{}\u{2019}s Screen", app.store.name_in(chan.guild, u));
    crate::ui::text::paint(&p, pos2(x + 60.0, cy), Align2::LEFT_CENTER, &whose, medium(16.0), pal().header_primary, (right - x - 60.0).max(0.0));
}

fn draw(app: &mut App, ui: &mut Ui, chan: &Channel, t: Tile, r: Rect, shown: f32, size: Size, picked: bool) {
    match t {
        Tile::User(u) => person(app, ui, chan, u, r, shown, size, picked),
        Tile::Stream(u) => stream(app, ui, chan, u, r, shown, size, picked),
    }
    if picked && size == Size::Strip {
        ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(153));
    }
}

fn person(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, tile: Rect, shown: f32, size: Size, picked: bool) {
    let twin = size == Size::Strip && picked;
    let fill = tile_color(app, ui.ctx(), u, chan.guild);
    ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
    if !twin {
        app.rects.insert(format!("voice:tile:{u}"), tile);
    }
    match app.voice.video.get(&u).cloned() {
        Some(img) => {
            if !twin {
                crate::voice::shown(u, tile.size() * ui.ctx().pixels_per_point());
            }
            frame(app, ui, format!("cam:{u}"), &img, tile)
        }
        None => {
            let av = if tile.width() >= 195.0 { 80.0 } else { 40.0 };
            if let Some(url) = app.store.avatar_url_in(chan.guild, u) {
                avatar(ui, Rect::from_center_size(tile.center(), vec2(av, av)), &url.replace("size=80", "size=128"));
            }
        }
    }
    // a server's own mute or deafen is red
    let state = app.voice.muted.get(&u).copied().filter(|[mute, _, deaf, _]| *mute || *deaf).map(|[_, server_mute, deaf, server_deaf]| {
        let (icon, server) = if deaf { (Icon::HeadphonesOff, server_deaf) } else { (Icon::MicOff, server_mute) };
        (icon, if server { pal().critical } else { Color32::WHITE })
    });
    let name = app.store.name_in(chan.guild, u);
    tag(app, ui, tile, size, shown, state, Some(&name), (!twin).then(|| format!("voice:tile_state:{u}")));
    if app.settings.dev && app.settings.stream_info && !twin && ui.rect_contains_pointer(tile) {
        crate::ui::tile::stream_info(ui, app, u, tile);
    }
    let k = crate::ui::motion::tween(ui.ctx(), egui::Id::new(("speaking", u, "tile", size == Size::Strip)), app.voice.is_speaking(u), 0.15, crate::ui::motion::EASE);
    if k > 0.0 {
        ui.painter().rect_stroke(tile, CornerRadius::same(8), Stroke::new(3.0, pal().green.gamma_multiply(k)), egui::StrokeKind::Inside);
    }
    if !twin {
        crate::ui::tile::options(app, ui, chan, u, tile, shown);
    }
}

/// Someone's stream: what we're watching of it, or Discord's dimmed preview with Watch Stream.
pub(super) fn stream(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, tile: Rect, shown: f32, size: Size, picked: bool) {
    let mine = u == app.store.me;
    let twin = size == Size::Strip && picked;
    if !twin {
        app.rects.insert(format!("voice:stream:{u}"), tile);
    }
    ui.painter().rect_filled(tile, CornerRadius::same(8), if size == Size::Big { Color32::BLACK } else { PREVIEW });
    match app.voice.stream.as_ref().filter(|l| l.user == u).map(|l| l.frame.clone()) {
        Some(Some(img)) => {
            if !twin {
                crate::voice::shown(u, tile.size() * ui.ctx().pixels_per_point());
            }
            frame(app, ui, "stream".into(), &img, tile)
        }
        Some(None) if size == Size::Big => {
            ui.painter().text(tile.center(), Align2::CENTER_CENTER, "Connecting to stream…", reg(14.0), pal().header_secondary);
        }
        _ => {
            if let Some(url) = crate::ui::streamtip::preview_url(app, ui.ctx(), chan.guild, chan.id, u) {
                egui::Image::from_uri(url).corner_radius(8).tint(Color32::from_white_alpha(38)).show_loading_spinner(false).paint_at(ui, tile);
            }
            if !mine && app.voice.chan == Some(chan.id) {
                let resp = ui.interact(tile, egui::Id::new(("watch", u, size == Size::Strip)), Sense::click());
                app.rects.insert(format!("voice:watch:{u}"), tile);
                let cta = size == Size::Grid && button(ui, ("watch_cta", u), tile.center(), None, "Watch Stream").clicked();
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let menu = stream_options(app, ui, u, tile, &resp);
                if resp.clicked() || cta || menu == Some(false) {
                    app.voice.watch(&app.net, u);
                }
                if menu == Some(true) {
                    app.voice.watch(&app.net, u);
                    app.call.pop = true;
                    app.call.full = false;
                }
            }
        }
    }
    if size == Size::Strip || mine && size == Size::Grid {
        live_badge(ui, tile.right_top() + vec2(-8.0, 8.0));
    }
    let name = app.store.name_in(chan.guild, u);
    tag(app, ui, tile, size, shown, Some((Icon::Screen, Color32::WHITE)), Some(&name), None);
}

/// The tile's "…" while it's hovered: Watch Stream, or Pop Out Stream (true) to watch it in the
/// call's own window.
fn stream_options(app: &mut App, ui: &Ui, u: Id, tile: Rect, resp: &egui::Response) -> Option<bool> {
    let btn = Rect::from_min_size(pos2(tile.right() - 44.0, tile.bottom() - 44.0), vec2(32.0, 32.0));
    let b = ui.interact(btn, egui::Id::new(("stream_opts", u)), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&b));
    if resp.hovered() || b.hovered() || open {
        ui.painter().rect_filled(btn, CornerRadius::same(8), Color32::from_black_alpha(143));
        icons::draw(ui.painter(), Icon::More, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), Color32::WHITE);
        app.rects.insert(format!("voice:stream_opts:{u}"), btn);
    }
    let b = b.on_hover_text("Options");
    let mut picked = None;
    egui::Popup::menu(&b).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(220.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        if crate::ui::menu_item(ui, None, "Watch Stream", false) {
            picked = Some(false);
            ui.close();
        }
        crate::ui::menu_sep(ui);
        if crate::ui::menu_item(ui, Some(Icon::PopOut), "Pop Out Stream", false) {
            picked = Some(true);
            ui.close();
        }
    });
    picked
}

/// Discord's secondary button: a faint wash, a hairline and an icon before the words.
pub(super) fn button(ui: &Ui, id: impl std::hash::Hash + std::fmt::Debug, center: egui::Pos2, icon: Option<Icon>, text: &str) -> Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(16.0), INK);
    let lead = if icon.is_some() { 24.0 } else { 0.0 };
    let r = Rect::from_center_size(center, vec2(32.0 + lead + g.size().x, 40.0));
    let resp = ui.interact(r, egui::Id::new(id), Sense::click());
    let wash = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
    ui.painter().rect_filled(r, CornerRadius::same(8), wash);
    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, Rect::from_min_size(pos2(r.left() + 16.0, r.center().y - 10.0), vec2(20.0, 20.0)), INK);
    }
    ui.painter().galley(pos2(r.left() + 16.0 + lead, r.center().y - g.size().y / 2.0), g, INK);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Discord's overlay title: 12 in from the corner (4 in the strip), 32 tall, an icon and the name
/// on a dark wash. While the call is idle only the icon stays.
fn tag(app: &mut App, ui: &Ui, tile: Rect, size: Size, shown: f32, icon: Option<(Icon, Color32)>, name: Option<&str>, mark: Option<String>) {
    let m = if size == Size::Strip { 4.0 } else { 12.0 };
    let font = if size == Size::Strip { reg(14.0) } else { reg(16.0) };
    let g = name.filter(|_| shown > 0.0 && !app.settings.hide_call_names).map(|n| ui.painter().layout_no_wrap(n.to_string(), font, Color32::WHITE.gamma_multiply(shown)));
    if icon.is_none() && g.is_none() {
        return;
    }
    let lead = if icon.is_some() { 20.0 } else { 0.0 };
    let full = match &g {
        Some(g) => 16.0 + lead + g.size().x,
        None => 32.0,
    };
    let r = Rect::from_min_size(pos2(tile.left() + m, tile.bottom() - m - 32.0), vec2(full.min(tile.width() - 2.0 * m - 40.0).max(32.0), 32.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(143));
    if let Some((i, c)) = icon {
        icons::draw(ui.painter(), i, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 8.0), vec2(16.0, 16.0)), c);
        if let Some(k) = mark {
            app.rects.insert(k, r);
        }
    }
    if let Some(g) = g {
        let x = r.left() + 8.0 + lead;
        ui.painter().with_clip_rect(Rect::from_min_max(pos2(x, r.top()), pos2(r.right() - 8.0, r.bottom()))).galley(pos2(x, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_sort_the_way_discord_sorts_them() {
        let mut keys = [
            key(false, "Ann", false, false),
            key(false, "zed", false, true),
            key(true, "zed", false, true),
            key(false, "Bob", true, false),
            key(true, "Cam", true, true),
        ];
        keys.sort();
        assert_eq!(keys, [key(true, "Cam", true, true), key(false, "Bob", true, false), key(true, "zed", false, true), key(false, "zed", false, true), key(false, "Ann", false, false)]);
    }

    #[test]
    fn the_strip_sorts_the_way_discords_strip_does() {
        let now = 1_800_000_000_000;
        let mut tiles = [
            (strip_key(false, "Ann", false, None, now, false), "Ann"),
            (strip_key(false, "Bob", false, Some(now - 60_000), now, false), "Bob"),
            (strip_key(false, "Cy", false, Some(now - 120_000), now, false), "Cy"),
            (strip_key(false, "Dee", false, Some(now - 2_000), now, false), "Dee"),
            (strip_key(false, "Eve", true, Some(now), now, false), "Eve"),
            (strip_key(false, "Fay", false, None, now, true), "Fay"),
            (strip_key(true, "zed", false, None, now, false), "zed"),
        ];
        tiles.sort();
        assert_eq!(tiles.map(|t| t.1), ["zed", "Eve", "Dee", "Fay", "Ann", "Bob", "Cy"]);
    }
}
