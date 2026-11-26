use egui::{Align2, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, TipExt, avatar, status_dot};
use crate::state::{FeedEntry, Id, ListItem, Status, Store, snowflake_ms};
use crate::theme::*;

pub fn show(app: &mut App, ui: &mut Ui, gid: Id, thread: Option<Id>) {
    let mut clicked = None;
    // shift+click puts an @mention of them in the composer
    let mut mention = None;
    let mut menu = None;
    let store = &app.store;
    let mut rects = Vec::new();
    let mut boosts = Vec::new();
    // anything else worth a name in app.rects
    let mut named: Vec<(String, Rect)> = Vec::new();
    let Some(g) = store.guilds.get(&gid) else { return };

    // until the gateway sends a member list, fall back to people we've seen talk
    let fallback: Vec<ListItem>;
    let items: &[ListItem] = if let Some(t) = thread {
        let mut seen: Vec<Id> = store.thread_members.get(&t).cloned().unwrap_or_default();
        seen.sort_by_key(|u| store.name_in(gid, *u).to_lowercase());
        let (on, off): (Vec<Id>, Vec<Id>) = seen.into_iter().partition(|u| store.status(*u) != Status::Offline);
        let mut v = Vec::new();
        for (id, list) in [("online", on), ("offline", off)] {
            if !list.is_empty() {
                v.push(ListItem::Group { id: id.into(), count: list.len() as u32 });
                v.extend(list.into_iter().map(ListItem::Member));
            }
        }
        fallback = v;
        &fallback
    } else if g.list.is_empty() {
        let mut seen: Vec<Id> = store.members.get(&gid).map(|m| m.keys().copied().collect()).unwrap_or_default();
        seen.sort_by_key(|u| store.name_in(gid, *u).to_lowercase());
        let (on, off): (Vec<Id>, Vec<Id>) = seen.into_iter().partition(|u| store.status(*u) != Status::Offline);
        let mut v = Vec::new();
        if !on.is_empty() {
            v.push(ListItem::Group { id: "online".into(), count: on.len() as u32 });
            v.extend(on.into_iter().map(ListItem::Member));
        }
        if !off.is_empty() {
            v.push(ListItem::Group { id: "offline".into(), count: off.len() as u32 });
            v.extend(off.into_iter().map(ListItem::Member));
        }
        fallback = v;
        &fallback
    } else {
        &g.list
    };

    let mut feed = None;
    let hidden = app.settings.hide_activity_cards;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // discord's rows sit flush; a group header is 20 above, 18 tall, 4 over its first row
        ui.spacing_mut().item_spacing.y = 0.0;
        if thread.is_none()
            && let Some(a) = activity(ui, store, gid, hidden, &mut named)
        {
            match a {
                Pick::Hide => feed = Some(false),
                Pick::About => feed = Some(true),
                Pick::Card(u, r) => clicked = Some((u, r.left_top() - vec2(310.0, 0.0))),
            }
        }
        let mut offline = false;
        for it in items {
            match it {
                ListItem::Group { id, count } => {
                    offline = id == "offline";
                    let role = id.parse::<Id>().ok().and_then(|r| g.roles.get(&r));
                    let name = match id.as_str() {
                        "online" => "Online".to_string(),
                        "offline" => "Offline".to_string(),
                        _ => role.map(|r| r.name.clone()).unwrap_or_default(),
                    };
                    ui.add_space(20.0);
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
                    ui.add_space(4.0);
                    // discord puts the role's icon in front of its group
                    let icon = role.and_then(|r| r.icon_url(32));
                    let mut x = r.left() + 16.0;
                    if let Some(url) = icon {
                        super::text::image(ui.painter(), &url, Rect::from_min_size(pos2(x, r.center().y - 8.0), vec2(16.0, 16.0)));
                        x += 20.0;
                    }
                    ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, format!("{} — {}", name, count), medium(14.0), pal().channel_default);
                }
                ListItem::Member(uid) => {
                    // with an activity the row is 44, the name rides up and the line sits under it
                    let line = if offline { None } else { store.activity_line(*uid) };
                    let (row, resp, hl) = super::side::dense_row(ui, if line.is_some() { 44.0 } else { 42.0 });
                    rects.push((*uid, row));
                    if !ui.is_rect_visible(row) {
                        continue;
                    }
                    if resp.clicked() && ui.input(|i| i.modifiers.shift) {
                        mention = Some(*uid);
                    } else if resp.clicked() {
                        clicked = Some((*uid, row.left_top() - vec2(310.0, 0.0)));
                    }
                    if resp.secondary_clicked() {
                        menu = Some((*uid, resp.interact_pointer_pos().unwrap_or(row.center())));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let inner = row.shrink2(vec2(8.0, 0.0));
                    let hovered = resp.hovered();
                    if hovered {
                        ui.painter().rect_filled(hl, CornerRadius::same(8), pal().bg_mod_hover);
                    }
                    // a nameplate sits behind the row, flush right at the row's height
                    if let Some(asset) = store.nameplate_in(gid, *uid) {
                        let plate = Rect::from_min_max(pos2((inner.right() - inner.height() * 448.0 / 84.0).max(inner.left()), inner.top()), inner.max);
                        let tint = if offline && !hovered { egui::Color32::from_white_alpha(77) } else { egui::Color32::WHITE };
                        let scrim = if hovered { pal().bg_mod_hover } else { pal().bg_primary };
                        super::cosmo::paint_plate(ui, plate, asset, CornerRadius::same(8), tint, hovered, None, Some(scrim));
                    }
                    // discord fades the whole offline row to .3, except your own
                    let fade = if offline && !hovered && *uid != store.me { 0.3 } else { 1.0 };
                    let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
                    if let Some(url) = store.avatar_url_in(gid, *uid) {
                        avatar(ui, av, &url);
                        if let Some(d) = store.decoration_url_in(gid, *uid) {
                            super::cosmo::paint_deco(ui, Rect::from_center_size(av.center(), av.size() * 1.2), &d, hovered);
                        }
                        if fade < 1.0 {
                            ui.painter().circle_filled(av.center(), 19.5, pal().bg_primary.gamma_multiply(0.7));
                        }
                    }
                    let st = store.status(*uid);
                    if st != Status::Offline {
                        status_dot(ui, av, st, if hovered { pal().bg_mod_hover } else { pal().bg_primary });
                    }
                    let color = store.role_color(gid, *uid).map(role_color).unwrap_or(pal().header_secondary);
                    let name = store.name_in(gid, *uid);
                    let user = store.user(*uid);
                    // discord leaves the crown off while the owner sits in the offline group
                    let owner = !offline && store.guilds.get(&gid).is_some_and(|g| g.owner == *uid);
                    let bot = user.is_some_and(|u| u.bot);
                    let app_badge = user.filter(|_| bot).map(|u| super::app_tag_layout(ui.painter(), u.flags & (1 << 16) != 0, fade, false));
                    let chip = user.filter(|_| !bot).and_then(|u| super::tag_chip_layout(ui.painter(), u, fade));
                    // boosters get discord's pink gem after the name and tag; role icons stay in chat
                    let since = store.members.get(&gid).and_then(|m| m.get(uid)).map_or(0, |m| m.boost);
                    let badges_w = row_badges_width(owner, app_badge.as_ref().map(|b| b.width()).or_else(|| chip.as_ref().map(|c| c.width())), since > 0);
                    let tw = (inner.right() - 8.0 - (av.right() + 12.0) - badges_w).max(0.0);
                    let ny = if line.is_some() { row.top() + 14.5 } else { row.center().y };
                    let face = name_font(store.name_font_in(gid, *uid), 16.0);
                    let nr = name_in(ui, store, gid, *uid, &name, face, pos2(av.right() + 12.0, ny), color.gamma_multiply(fade), fade, tw, hovered);
                    if store.role_gradient(gid, *uid).is_some() {
                        named.push((format!("member_grad:{uid}"), nr));
                    }
                    if let Some(line) = line {
                        status_line(ui.painter(), line, av.right() + 12.0, row.top() + 31.0, inner.right() - 8.0, pal().text_muted, 12.0, "");
                    }
                    let mut tx = nr.right() + 4.0;
                    if owner {
                        let cr = Rect::from_min_size(pos2(tx, ny - 7.0), vec2(14.0, 14.0));
                        super::icons::draw(ui.painter(), super::icons::Icon::Crown, cr, egui::Color32::from_rgb(0xf0, 0xb2, 0x32).gamma_multiply(fade));
                        ui.interact(cr, ui.id().with(("owner", *uid)), Sense::hover()).tip("Server Owner");
                        tx = cr.right() + 4.0;
                    }
                    if let Some(badge) = app_badge {
                        tx = badge.paint(ui.painter(), pos2(tx, ny)).right() + 4.0;
                    } else if let Some(chip) = chip {
                        tx = chip.paint(ui.painter(), pos2(tx, ny)).right() + 4.0;
                    }
                    if since > 0 {
                        let ir = Rect::from_min_size(pos2(tx - 1.0, ny - 7.0), vec2(14.0, 14.0));
                        super::icons::draw(ui.painter(), super::icons::Icon::Gem, ir, egui::Color32::from_rgb(0xff, 0x73, 0xfa).gamma_multiply(fade));
                        let r = ui.interact(ir, ui.id().with(("boosting", *uid)), Sense::hover());
                        if (r.hovered() || r.is_tooltip_open()) && !ui.input(|i| i.is_scrolling()) {
                            let date = chrono::DateTime::from_timestamp(since, 0).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default();
                            r.on_hover_text(format!("Server boosting since {date}"));
                        }
                        boosts.push((*uid, ir));
                    }
                }
            }
        }
        ui.add_space(16.0);
    });
    for (u, r) in rects {
        app.rects.insert(format!("member:{u}"), r);
    }
    for (u, r) in boosts {
        app.rects.insert(format!("boost:{u}"), r);
    }
    app.rects.extend(named);
    match feed {
        Some(true) => app.feed_about = true,
        Some(false) => {
            app.settings.hide_activity_cards = !app.settings.hide_activity_cards;
            app.save_settings(ui.ctx());
        }
        None => {}
    }
    if let Some((u, at)) = clicked {
        app.open_profile(u, gid, at);
    }
    if let Some(u) = mention {
        app.mention(u, gid);
    }
    if let Some((u, at)) = menu {
        app.user_menu = Some((u, gid, None, at));
    }
}

/// A member's name as the list draws it: a gradient role's colours run along it, anything else
/// is the flat colour given. Anchored left-centre at `at`.
#[allow(clippy::too_many_arguments)]
/// The line under a name in a list: the custom status with its emoji, or what they're doing.
pub(super) fn status_line(p: &egui::Painter, (emoji, plain, bold, also): (Option<String>, String, String, Option<u8>), mut x: f32, y: f32, right: f32, col: egui::Color32, size: f32, doing: &str) {
    let pic = if size > 12.0 { 16.0 } else { 14.0 };
    // doing something as well: its icon, its name if asked for, and a dot lead the status
    if let Some(kind) = also {
        let icon = if kind == 2 { super::icons::Icon::Music } else { super::icons::Icon::Controller };
        super::icons::draw(p, icon, Rect::from_center_size(pos2(x + 7.0, y), vec2(14.0, 14.0)), pal().positive);
        x += 16.0;
        if !doing.is_empty() {
            x = super::text::paint(p, pos2(x + 2.0, y), egui::Align2::LEFT_CENTER, doing, medium(size), col, (right - x) * 0.5).right();
        }
        x = p.text(pos2(x + if doing.is_empty() { 0.0 } else { 4.0 }, y), egui::Align2::LEFT_CENTER, "•", reg(size), col).right() + 4.0;
    }
    if let Some(url) = emoji {
        super::text::image(p, &url, Rect::from_center_size(pos2(x + pic / 2.0, y), vec2(pic, pic)));
        x += pic + 4.0;
    } else if plain == "Playing " {
        // discord: a green controller and the game, no verb
        super::icons::draw(p, super::icons::Icon::Controller, Rect::from_center_size(pos2(x + 7.0, y), vec2(14.0, 14.0)), pal().positive);
        x += 18.0;
    }
    // watching and playing show just the name
    let (plain, bold) = if plain == "Playing " || plain == "Watching " { (bold, String::new()) } else { (plain, bold) };
    if bold.is_empty() {
        super::text::paint_em(p, pos2(x, y), egui::Align2::LEFT_CENTER, &plain, medium(size), col, right - x, 1.0);
        return;
    }
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = right - x;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.append(&plain, 0.0, egui::TextFormat { font_id: medium(size), color: col, ..Default::default() });
    job.append(&bold, 0.0, egui::TextFormat { font_id: semi(size), color: col, ..Default::default() });
    let g = p.layout_job(job);
    p.galley(pos2(x, y - g.size().y / 2.0), g, col);
}

fn name_in(ui: &Ui, store: &Store, gid: Id, uid: Id, name: &str, face: egui::FontId, at: egui::Pos2, flat: Color32, fade: f32, max_w: f32, hot: bool) -> Rect {
    match store.role_gradient(gid, uid) {
        Some(r) => {
            let mut ink = super::chat::grad_ink(r);
            for s in &mut ink.stops {
                *s = s.gamma_multiply(fade);
            }
            let rich = ink.layout_em(ui.ctx(), name, face, max_w, hot, 1.0);
            let rect = Align2::LEFT_CENTER.anchor_size(at, rich.size());
            ink.paint(ui.painter(), &rich, rect.min, hot);
            rect
        }
        None => super::text::paint_em(ui.painter(), at, Align2::LEFT_CENTER, name, face, flat, max_w, 1.0),
    }
}

enum Pick {
    Hide,
    About,
    Card(Id, Rect),
}

/// Discord's "Activity" section over the member list: what people in this server have been
/// playing and listening to, from the activity feed. Three cards, or all of them opened up.
fn activity(ui: &mut Ui, store: &Store, gid: Id, hidden: bool, named: &mut Vec<(String, Rect)>) -> Option<Pick> {
    let members = store.members.get(&gid)?;
    let cards: Vec<&FeedEntry> = store.feed.iter().filter(|e| e.author != store.me && members.contains_key(&e.author)).collect();
    if cards.is_empty() {
        return None;
    }
    let open_id = egui::Id::new(("activity_open", gid));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let mut pick = None;

    ui.add_space(20.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    ui.add_space(4.0);
    let head = Rect::from_min_max(pos2(r.left() + 16.0, r.top() - 3.0), pos2(r.right() - 12.0, r.bottom() + 3.0));
    let resp = ui.interact(head, ui.id().with("activity_head"), Sense::click());
    let p = ui.painter();
    let t = p.text(pos2(head.left(), r.center().y), Align2::LEFT_CENTER, format!("Activity — {}", cards.len()), medium(14.0), pal().channel_default);
    named.push((format!("activity:{}", cards.len()), head));
    let gr = Rect::from_min_size(pos2(t.right(), r.center().y - 12.0), vec2(24.0, 24.0));
    let g = ui.interact(gr, ui.id().with("activity_gear"), Sense::click());
    named.push(("activity:gear".into(), gr));
    let ink = crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &g));
    icons::draw(ui.painter(), Icon::Gear, Rect::from_center_size(gr.center(), vec2(12.0, 12.0)), ink);
    let hot = crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    let foldable = hidden || cards.len() > 3;
    let chev = Rect::from_center_size(pos2(head.right() - 12.0, r.center().y), vec2(12.0, 12.0));
    if hidden {
        icons::draw(ui.painter(), Icon::EyeOff, chev, hot);
    } else if foldable {
        super::motion::chevron(ui, resp.id, open, chev, hot);
    }
    if foldable && resp.clicked() && !g.hovered() {
        if hidden {
            pick = Some(Pick::Hide);
        } else {
            open = !open;
            ui.data_mut(|d| d.insert_temp(open_id, open));
        }
    }
    if foldable && (resp.hovered() || g.hovered()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let g = g.tip("Member List Recent Activity Settings");
    egui::Popup::menu(&g).align(egui::RectAlign::BOTTOM_END).show(|ui| feed_menu(ui, hidden, &mut pick));
    egui::Popup::context_menu(&resp).show(|ui| feed_menu(ui, hidden, &mut pick));
    if hidden {
        return pick;
    }

    ui.add_space(4.0);
    let now = chrono::Utc::now().timestamp_millis();
    let mut ticking = false;
    for e in cards.iter().take(if open { usize::MAX } else { 3 }) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::hover());
        ui.add_space(8.0);
        let card = row.shrink2(vec2(8.0, 0.0));
        named.push((format!("activity:card:{}", e.author), card));
        if !ui.is_rect_visible(card) {
            continue;
        }
        let resp = ui.interact(card, ui.id().with(("activity_card", e.author, e.id, &e.title)), Sense::click());
        let p = ui.painter();
        // the card's 1px border sits on its own fill, as css draws it
        p.rect_filled(card, CornerRadius::same(12), crate::ui::motion::mix(pal().bg_secondary, pal().bg_floating, crate::ui::motion::hot(ui.ctx(), &resp)));
        p.rect_stroke(card, CornerRadius::same(12), egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
        let x = card.left() + 9.0;
        let info = card.width() - 18.0 - 56.0;
        if let Some(url) = store.avatar_url_in(gid, e.author) {
            avatar(ui, Rect::from_min_size(pos2(x, card.top() + 10.0), vec2(16.0, 16.0)), &url);
            let color = store.role_color(gid, e.author).map(role_color).unwrap_or(pal().header_secondary);
            let name = store.name_in(gid, e.author);
            name_in(ui, store, gid, e.author, &name, name_font(store.name_font_in(gid, e.author), 14.0), pos2(x + 22.0, card.top() + 18.0), color, 1.0, info - 22.0, resp.hovered());
        }
        super::text::paint_em(p, pos2(x, card.top() + 38.0), Align2::LEFT_CENTER, &e.title, medium(14.0), pal().text_normal, info, 1.0);
        let clip = p.with_clip_rect(Rect::from_min_size(pos2(x, card.top() + 47.0), vec2(info, 16.0)));
        let mut bx = x;
        for (icon, text, col) in badges(e, now) {
            if col == pal().positive {
                ticking = true;
            }
            icons::draw(&clip, icon, Rect::from_min_size(pos2(bx, card.top() + 49.0), vec2(12.0, 12.0)), col);
            let g = clip.layout_no_wrap(text, reg(12.0), col);
            let w = g.size().x;
            clip.galley(pos2(bx + 16.0, card.top() + 55.0 - g.size().y / 2.0), g, col);
            bx += 16.0 + w + 8.0;
        }
        let img = match e.kind {
            6 => e.image.clone(),
            _ => store.app_icons.get(&e.app).cloned().flatten().map(|h| format!("{}/app-icons/{}/{h}.png?size=96", crate::api::cdn(), e.app)),
        };
        if let Some(url) = img {
            let ir = Rect::from_min_size(pos2(card.right() - 57.0, card.top() + 11.0), vec2(48.0, 48.0));
            egui::Image::from_uri(url).corner_radius(CornerRadius::same(4)).show_loading_spinner(false).paint_at(ui, ir);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            pick = Some(Pick::Card(e.author, card));
        }
    }
    if ticking && !ui.ctx().has_requested_repaint() {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    pick
}

fn feed_menu(ui: &mut Ui, hidden: bool, pick: &mut Option<Pick>) {
    super::menu_body(ui, |ui| {
        if super::menu_row(ui, Some(Icon::Help), "About Recent Activity", None, false, super::Trail::None).clicked() {
            *pick = Some(Pick::About);
            ui.close();
        }
        if super::menu_row(ui, Some(Icon::EyeOff), "Hide Activity Cards", None, false, super::Trail::Check(hidden)).clicked() {
            *pick = Some(Pick::Hide);
            ui.close();
        }
    });
}

/// A card's line of badges, in discord's order: how long ago (or a live timer), then whatever
/// else the entry earned.
fn badges(e: &FeedEntry, now: i64) -> Vec<(Icon, String, Color32)> {
    let col = pal().header_secondary;
    let at = snowflake_ms(e.id);
    let mut out = Vec::new();
    match e.kind {
        6 if e.count > 0 => out.push((Icon::Trophy, format!("{} tracks this week", e.count), col)),
        3 if e.secs > 0 => out.push((Icon::Trophy, format!("Most played — {}h this week", (e.secs as f32 / 3600.0).round()), col)),
        1 | 8 => {
            let icon = if e.kind == 8 { Icon::Launched } else { Icon::Controller };
            if e.live {
                // a running game counts up in green
                let s = ((now - at).max(0) / 1000) as u64;
                let t = if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) };
                out.push((icon, t, pal().positive));
            } else {
                out.push((icon, ago(now - at), col));
            }
            // an app made in the last week is a new release
            if e.app != 0 && now - snowflake_ms(e.app) <= 7 * 86_400_000 {
                out.push((Icon::NewRelease, "New Release".into(), col));
            }
            if e.kind == 1 && e.first {
                out.push((Icon::NewPlayer, "New Player".into(), col));
            }
            // streaks show from three days, and only while the entry is under two days old
            if e.kind == 1 && e.streak >= 3 && now - at <= 48 * 3_600_000 {
                out.push((Icon::Streak, format!("{}x Streak", e.streak), col));
            }
            if e.trending {
                out.push((Icon::Fire, "Trending".into(), col));
            }
            if e.kind == 1 && e.returning {
                out.push((Icon::Retry, "Returning".into(), col));
            }
            let h = (e.secs as f32 / 3600.0).round();
            if e.marathon && h > 0.0 {
                out.push((Icon::Marathon, format!("{h}h Marathon"), col));
            }
        }
        _ => {}
    }
    out
}

/// Discord's short relative time: 45s, 12m, 3h, 2d, 3w, then months.
fn ago(ms: i64) -> String {
    let s = (ms / 1000).max(0) as f64;
    match s {
        s if s < 60.0 => format!("{}s ago", s as i64),
        s if s < 3600.0 => format!("{}m ago", (s / 60.0).round()),
        s if s < 12.0 * 3600.0 => format!("{}h ago", (s / 3600.0).round()),
        s if s < 9.0 * 86400.0 => format!("{}d ago", (s / 86400.0).round()),
        s if s < 28.0 * 86400.0 => format!("{}w ago", (s / 604800.0).round()),
        s => format!("{}mo ago", (s / 2_592_000.0).round()),
    }
}

/// A group dm's people, the way discord lists them beside the chat: everyone under one heading,
/// the owner crowned.
pub fn group(app: &mut App, ui: &mut Ui, chan: &crate::state::Channel) {
    let store = &app.store;
    let mut people: Vec<Id> = chan.recipients.iter().copied().chain(std::iter::once(store.me)).collect();
    people.sort_by_key(|u| store.user(*u).map(|u| u.username.to_lowercase()).unwrap_or_default());
    let mut clicked = None;
    // shift+click puts an @mention of them in the composer
    let mut mention = None;
    let mut menu = None;
    let mut invite = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(19.5);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, format!("Members—{}", people.len()), medium(14.0), pal().channel_default);
        ui.add_space(2.0);
        for uid in people {
            let Some(u) = store.user(uid) else { continue };
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
            let inner = row.shrink2(vec2(8.0, 0.0));
            if resp.hovered() {
                ui.painter().rect_filled(inner, CornerRadius::same(8), pal().bg_mod_hover);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let st = store.status(uid);
            let fade = if st == Status::Offline && !resp.hovered() && uid != store.me { 0.3 } else { 1.0 };
            if let Some(asset) = u.nameplate.as_deref() {
                let plate = Rect::from_min_max(pos2((inner.right() - inner.height() * 448.0 / 84.0).max(inner.left()), inner.top()), inner.max);
                let tint = if fade < 1.0 { egui::Color32::from_white_alpha(77) } else { egui::Color32::WHITE };
                let scrim = if resp.hovered() { pal().bg_mod_hover } else { pal().bg_primary };
                super::cosmo::paint_plate(ui, plate, asset, CornerRadius::same(8), tint, resp.hovered(), None, Some(scrim));
                super::settings::mark(ui, &format!("group:plate:{uid}"), plate);
            }
            let av = Rect::from_min_size(pos2(inner.left() + 8.0, row.center().y - 16.0), vec2(32.0, 32.0));
            avatar(ui, av, &u.avatar_url());
            if let Some(d) = u.decoration_url() {
                let at = Rect::from_center_size(av.center(), av.size() * 1.2);
                super::cosmo::paint_deco(ui, at, &d, resp.hovered());
                super::settings::mark(ui, &format!("group:decoration:{uid}"), at);
            }
            if fade < 1.0 {
                ui.painter().circle_filled(av.center(), 19.5, pal().bg_primary.gamma_multiply(0.7));
            } else {
                status_dot(ui, av, st, crate::ui::motion::mix(pal().bg_primary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            }
            let owner = chan.owner() == uid;
            let chip = super::tag_chip_layout(ui.painter(), u, fade);
            let badges_w = row_badges_width(owner, chip.as_ref().map(|c| c.width()), false);
            let tw = (inner.right() - 8.0 - (av.right() + 12.0) - badges_w).max(0.0);
            let nr = super::text::paint(ui.painter(), pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, u.display(), name_font(u.font, 16.0), crate::ui::motion::mix(pal().channel_default, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)).gamma_multiply(fade), tw);
            let mut tx = nr.right() + 4.0;
            if owner {
                let cr = Rect::from_min_size(pos2(tx, row.center().y - 7.0), vec2(14.0, 14.0));
                super::icons::draw(ui.painter(), super::icons::Icon::Crown, cr, egui::Color32::from_rgb(0xf0, 0xb2, 0x32).gamma_multiply(fade));
                ui.interact(cr, ui.id().with(("owner", uid)), Sense::hover()).on_hover_text("Group Owner");
                tx = cr.right() + 4.0;
            }
            if let Some(chip) = chip {
                chip.paint(ui.painter(), pos2(tx, row.center().y));
            }
            if resp.clicked() && ui.input(|i| i.modifiers.shift) {
                mention = Some(uid);
            } else if resp.clicked() {
                clicked = Some((uid, row.left_top() - vec2(310.0, 0.0)));
            }
            if resp.secondary_clicked() {
                menu = Some((uid, resp.interact_pointer_pos().unwrap_or(row.center())));
            }
        }
        ui.add_space(16.0);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 16.0, 40.0), Sense::click());
        let r = r.translate(vec2(8.0, 0.0));
        super::settings::mark(ui, "group:invite", r);
        ui.painter().rect(r, 8, if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        super::icons::draw(ui.painter(), super::icons::Icon::PersonAdd, Rect::from_center_size(pos2(r.center().x - 62.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
        ui.painter().text(pos2(r.center().x - 46.0, r.center().y), Align2::LEFT_CENTER, "Invite to Group DM", medium(16.0), pal().header_primary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        invite = resp.clicked();
        ui.add_space(16.0);
    });
    if invite {
        app.group_pick = Some(super::dmpick::Pick::new(Some(chan.id)));
    }
    if let Some((u, at)) = clicked {
        app.open_profile(u, 0, at);
    }
    if let Some(u) = mention {
        app.mention(u, 0);
    }
    if let Some((u, at)) = menu {
        app.user_menu = Some((u, 0, None, at));
    }
}

/// Width past `nr.right()` required for every badge drawn after a member's name
/// (owner crown, bot APP tag or server tag chip, and booster gem, with gaps).
fn row_badges_width(owner: bool, chip_w: Option<f32>, boost: bool) -> f32 {
    let mut tx = 4.0;
    let mut end = 0.0;
    if owner {
        end = tx + 14.0;
        tx = end + 4.0;
    }
    if let Some(w) = chip_w {
        end = tx + w;
        tx = end + 4.0;
    }
    if boost {
        end = tx - 1.0 + 14.0;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_row_reserves_exact_width_for_all_badges() {
        assert_eq!(row_badges_width(false, None, false), 0.0);
        assert_eq!(row_badges_width(true, None, false), 18.0);
        assert_eq!(row_badges_width(false, Some(52.0), false), 56.0);
        assert_eq!(row_badges_width(false, None, true), 17.0);
        assert_eq!(row_badges_width(false, Some(52.0), true), 73.0);
        assert_eq!(row_badges_width(true, Some(52.0), true), 91.0);
    }

    #[test]
    fn shared_tag_and_app_layouts_match_their_painted_widths() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::theme::fonts(false));
        let mut user = crate::state::User::default();
        user.tag = Some(("BUDU".into(), 1, "badge_hash".into()));
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let chip = crate::ui::tag_chip_layout(ui.painter(), &user, 1.0).unwrap();
            let measured = chip.width();
            let painted = chip.paint(ui.painter(), pos2(10.0, 20.0));
            assert_eq!(measured, painted.width());

            for verified in [false, true] {
                let app = crate::ui::app_tag_layout(ui.painter(), verified, 1.0, false);
                let measured = app.width();
                let painted = app.paint(ui.painter(), pos2(10.0, 40.0));
                assert_eq!(measured, painted.width());
            }
        });
        out.textures_delta.clear();
    }
}
