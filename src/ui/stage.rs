use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::icons::{self, Icon};
use super::{App, avatar, menu_item, menu_sep};
use crate::net::Tag;
use crate::state::{Channel, Id, MANAGE_CHANNELS};
use crate::theme::*;

const MUTE_MEMBERS: u64 = 1 << 22;
const MOVE_MEMBERS: u64 = 1 << 24;

pub struct Form {
    pub chan: Id,
    pub topic: String,
    pub notify: bool,
    pub edit: bool,
}

pub fn is_mod(app: &App, chan: &Channel) -> bool {
    let need = MANAGE_CHANNELS | MUTE_MEMBERS | MOVE_MEMBERS;
    app.store.perms(chan) & need == need
}

impl App {
    fn stage_state(&mut self, chan: &Channel, who: &str, body: Value) {
        self.rest("PATCH", format!("/guilds/{}/voice-states/{who}", chan.guild), Some(body), Tag::reason());
    }

    pub fn request_to_speak(&mut self, chan: &Channel, on: bool) {
        let at = on.then(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        self.call.asked = on;
        self.stage_state(chan, "@me", json!({ "request_to_speak_timestamp": at, "channel_id": chan.id.to_string() }));
    }

    pub fn set_speaker(&mut self, chan: &Channel, who: Id, speaker: bool) {
        let target = if who == self.store.me { "@me".to_string() } else { who.to_string() };
        self.call.asked = false;
        self.stage_state(chan, &target, json!({ "suppress": !speaker, "request_to_speak_timestamp": null, "channel_id": chan.id.to_string() }));
    }

    pub fn invite_to_speak(&mut self, chan: &Channel, who: Id) {
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.stage_state(chan, &who.to_string(), json!({ "suppress": false, "request_to_speak_timestamp": at, "channel_id": chan.id.to_string() }));
    }

    pub(super) fn end_stage(&mut self, ch: Id) {
        self.rest("DELETE", format!("/stage-instances/{ch}"), None, Tag::reason());
    }
}

fn pill(ui: &mut Ui, r: Rect, text: &str, fill: Color32, ink: Color32, font: egui::FontId) -> egui::Response {
    let resp = ui.interact(r, egui::Id::new(("stage_pill", text)), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { fill.gamma_multiply(0.88) } else { fill });
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, font, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn backdrop(ui: &Ui, r: Rect) {
    const TOP: [u8; 3] = [0x0b, 0x0c, 0x1f];
    const BOTTOM: [u8; 3] = [0x3a, 0x40, 0xc0];
    let mut mesh = egui::Mesh::default();
    let at = |t: f32| Color32::from_rgb(TOP[0] + ((BOTTOM[0] - TOP[0]) as f32 * t) as u8, TOP[1] + ((BOTTOM[1] - TOP[1]) as f32 * t) as u8, TOP[2] + ((BOTTOM[2] - TOP[2]) as f32 * t) as u8);
    for (i, t) in [0.0f32, 0.55, 1.0].into_iter().enumerate() {
        let y = r.top() + r.height() * t;
        let c = at(t * t);
        mesh.colored_vertex(pos2(r.left(), y), c);
        mesh.colored_vertex(pos2(r.right(), y), c);
        if i > 0 {
            let j = (i * 2) as u32;
            mesh.add_triangle(j - 2, j - 1, j);
            mesh.add_triangle(j - 1, j, j + 1);
        }
    }
    ui.painter().add(mesh);
}

pub fn room(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let inside = app.voice.chan == Some(chan.id);
    let members = app.voice.members(&app.store, chan.id);
    let (speakers, audience) = app.voice.stage.split(&members);
    let live = app.voice.stage.live.get(&chan.id).cloned();
    egui::Panel::top("room_head")
        .exact_size(48.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(if inside { Color32::BLACK } else { Color32::from_rgb(0x0b, 0x0c, 0x1f) }).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| super::call::head(app, ui, chan, 1.0, Icon::Stage));
    egui::CentralPanel::default().frame(Frame::new().fill(Color32::BLACK)).show(ui, |ui| {
        let r = ui.max_rect();
        if !inside {
            backdrop(ui, r);
            landing(app, ui, chan, r, &speakers, live.as_ref().map(|l| l.topic.as_str()));
            return;
        }
        let mods = is_mod(app, chan);
        let me = app.store.me;
        if app.call.started == Some(chan.id) && app.voice.stage.seats.get(&me).is_some_and(|s| s.audience) {
            app.call.started = None;
            app.set_speaker(chan, me, true);
        }
        let mut y = r.top() + 16.0;
        let title = live.as_ref().map_or(chan.name.as_str(), |l| if l.topic.is_empty() { chan.name.as_str() } else { l.topic.as_str() }).to_string();
        let heading = ui.painter().text(pos2(r.left() + 24.0, y + 14.0), Align2::LEFT_CENTER, &title, semi(20.0), Color32::WHITE);
        ui.painter().text(pos2(heading.left(), y + 38.0), Align2::LEFT_CENTER, format!("{} speakers, {} listeners", speakers.len(), audience.len()), reg(14.0), pal().header_secondary);
        if mods && let Some(l) = &live {
            let b = Rect::from_min_size(pos2(r.right() - 24.0 - 104.0, y + 2.0), vec2(104.0, 32.0));
            if pill(ui, b, "Edit Stage", pal().control, pal().header_primary, medium(14.0)).clicked() {
                app.call.start = Some(Form { chan: chan.id, topic: l.topic.clone(), notify: false, edit: true });
            }
            app.rects.insert("stage:edit".into(), b);
        }
        y += 64.0;
        let area = Rect::from_min_max(pos2(r.left() + 16.0, y), pos2(r.right() - 16.0, r.top() + 16.0 + (r.height() - 64.0 - 80.0) * 0.5 + 48.0));
        if speakers.is_empty() {
            ui.painter().text(area.center(), Align2::CENTER_CENTER, "No one is currently speaking", reg(16.0), pal().header_secondary);
        }
        let n = speakers.len().max(1);
        let cols = n.min(((area.width() + 8.0) / 248.0).floor().max(1.0) as usize);
        let rows = n.div_ceil(cols);
        let th = ((area.height() - 8.0 * (rows - 1) as f32) / rows as f32).clamp(96.0, 168.0);
        for (i, u) in speakers.iter().enumerate() {
            let (row, col) = (i / cols, i % cols);
            let in_row = (speakers.len() - row * cols).min(cols);
            let x0 = area.center().x - (in_row as f32 * 240.0 + (in_row as f32 - 1.0) * 8.0) / 2.0;
            let tile = Rect::from_min_size(pos2(x0 + col as f32 * 248.0, area.top() + row as f32 * (th + 8.0)), vec2(240.0, th));
            speaker_tile(app, ui, chan, *u, tile);
        }
        y = area.bottom() + 16.0;
        let hands: Vec<Id> = audience.iter().copied().filter(|u| app.voice.stage.seats.get(u).is_some_and(|s| s.hand.is_some())).collect();
        if mods && !hands.is_empty() {
            section(ui, r, y, &format!("Requests to Speak — {}", hands.len()), Icon::Hand, None);
            y += 40.0;
            for u in &hands {
                let row = Rect::from_min_size(pos2(r.left() + 24.0, y), vec2(r.width() - 48.0, 40.0));
                avatar_name(app, ui, chan, *u, row);
                let b = Rect::from_min_size(pos2(row.right() - 140.0, row.top() + 4.0), vec2(140.0, 32.0));
                if pill(ui, b, "Invite to Speak", BLURPLE, Color32::WHITE, medium(14.0)).clicked() {
                    app.invite_to_speak(chan, *u);
                }
                app.rects.insert(format!("stage:invite_speak:{u}"), b);
                y += 44.0;
            }
            y += 12.0;
        }
        let open = !app.call.audience_shut;
        if let Some(now) = section(ui, r, y, &format!("Audience — {}", audience.len()), Icon::People, Some(open)) {
            app.call.audience_shut = !now;
        }
        y += 48.0;
        if open {
            let mut x = r.left() + 24.0;
            for u in &audience {
                if x + 96.0 > r.right() - 16.0 {
                    x = r.left() + 24.0;
                    y += 104.0;
                }
                audience_cell(app, ui, chan, *u, Rect::from_min_size(pos2(x, y), vec2(88.0, 92.0)));
                x += 96.0;
            }
        }
        let bar = Rect::from_min_max(pos2(r.left(), r.bottom() - 72.0), r.max);
        super::stagebar::controls(app, ui, chan, bar);
        invited(app, ui, chan, r, me);
    });
}

fn landing(app: &mut App, ui: &mut Ui, chan: &Channel, r: Rect, speakers: &[Id], topic: Option<&str>) {
    let c = r.center();
    let title = topic.filter(|t| !t.is_empty()).unwrap_or(chan.name.as_str());
    ui.painter().text(c + vec2(0.0, -48.0), Align2::CENTER_CENTER, title, reg(36.0), Color32::WHITE);
    let names: Vec<String> = speakers.iter().take(2).map(|u| app.store.name_in(chan.guild, *u)).collect();
    let line = match names.as_slice() {
        [] => "No one is currently speaking".to_string(),
        [a] => format!("{a} is currently speaking"),
        [a, b] => format!("{a} and {b} are currently speaking"),
        _ => String::new(),
    };
    ui.painter().text(c + vec2(0.0, -8.0), Align2::CENTER_CENTER, line, reg(20.0), Color32::WHITE);
    let live = topic.is_some();
    let mods = is_mod(app, chan);
    let (main, w) = if mods && !live { ("Start Stage", 124.0) } else { ("Join Stage", 104.0) };
    let a = Rect::from_min_size(c + vec2(-(w + 100.0 + 12.0) / 2.0, 18.0), vec2(w, 40.0));
    let b = Rect::from_min_size(pos2(a.right() + 12.0, a.top()), vec2(100.0, 40.0));
    app.rects.insert("stage:join".into(), a);
    if pill(ui, a, main, Color32::WHITE, Color32::BLACK, medium(16.0)).clicked() {
        if mods && !live {
            app.call.start = Some(Form { chan: chan.id, topic: String::new(), notify: false, edit: false });
        } else {
            app.join_voice(chan.id, true);
        }
    }
    if pill(ui, b, "Invite", Color32::from_rgba_unmultiplied(151, 151, 159, 31), Color32::WHITE, medium(16.0)).clicked() {
        app.invite_people(chan.id, "Stage Channel");
    }
}

fn section(ui: &mut Ui, r: Rect, y: f32, label: &str, icon: Icon, caret: Option<bool>) -> Option<bool> {
    let row = Rect::from_min_size(pos2(r.left() + 24.0, y), vec2(r.width() - 48.0, 32.0));
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(row.left() + 10.0, row.center().y), vec2(20.0, 20.0)), Color32::WHITE);
    ui.painter().text(pos2(row.left() + 36.0, row.center().y), Align2::LEFT_CENTER, label, semi(16.0), Color32::WHITE);
    ui.painter().hline(row.x_range(), row.bottom() + 6.0, Stroke::new(1.0, pal().border_subtle));
    let open = caret?;
    let resp = ui.interact(row, egui::Id::new(("stage_section", label.len(), y as i32)), Sense::click());
    icons::draw(ui.painter(), if open { Icon::ChevronDown } else { Icon::ChevronUp }, Rect::from_center_size(pos2(row.right() - 10.0, row.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    resp.clicked().then_some(!open)
}

fn avatar_name(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, row: Rect) {
    let av = Rect::from_center_size(pos2(row.left() + 20.0, row.center().y), vec2(32.0, 32.0));
    if let Some(user) = app.store.user(u) {
        avatar(ui, av, &user.avatar_url());
    }
    ui.painter().text(pos2(av.right() + 12.0, row.center().y), Align2::LEFT_CENTER, app.store.name_in(chan.guild, u), medium(16.0), Color32::WHITE);
}

fn speaker_tile(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, tile: Rect) {
    let fill = super::call::tile_color(app, ui.ctx(), u, chan.guild);
    ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
    app.rects.insert(format!("stage:speaker:{u}"), tile);
    let av = Rect::from_center_size(tile.center() - vec2(0.0, 6.0), vec2(64.0, 64.0));
    if let Some(user) = app.store.user(u) {
        avatar(ui, av, &user.avatar_url().replace("size=80", "size=128"));
    }
    let name = app.store.name_in(chan.guild, u);
    let g = ui.painter().layout_no_wrap(name, medium(14.0), Color32::WHITE);
    let tag = Rect::from_min_size(pos2(tile.left() + 12.0, tile.bottom() - 42.0), vec2((g.size().x + 24.0).min(tile.width() - 56.0), 30.0));
    ui.painter().rect_filled(tag, CornerRadius::same(8), Color32::from_black_alpha(143));
    ui.painter().with_clip_rect(tag.shrink2(vec2(12.0, 0.0))).galley(pos2(tag.left() + 12.0, tag.center().y - g.size().y / 2.0), g, Color32::WHITE);
    if app.voice.muted.get(&u).is_some_and(|m| m[0] || m[2]) {
        icons::draw(ui.painter(), Icon::MicOff, Rect::from_center_size(pos2(tile.right() - 24.0, tile.top() + 24.0), vec2(20.0, 20.0)), Color32::WHITE);
    }
    if app.voice.is_speaking(u) {
        ui.painter().rect_stroke(tile, CornerRadius::same(8), Stroke::new(3.0, GREEN), egui::StrokeKind::Inside);
    }
    super::tile::options(app, ui, chan, u, tile, 1.0);
}

fn audience_cell(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id, cell: Rect) {
    let av = Rect::from_center_size(pos2(cell.center().x, cell.top() + 28.0), vec2(56.0, 56.0));
    if let Some(user) = app.store.user(u) {
        avatar(ui, av, &user.avatar_url());
    }
    if app.voice.stage.seats.get(&u).is_some_and(|s| s.hand.is_some()) {
        let b = pos2(av.right() - 6.0, av.bottom() - 6.0);
        ui.painter().circle_filled(b, 11.0, BLURPLE);
        icons::draw(ui.painter(), Icon::Hand, Rect::from_center_size(b, vec2(14.0, 14.0)), Color32::WHITE);
    }
    let name = app.store.name_in(chan.guild, u);
    ui.painter().with_clip_rect(cell).text(pos2(cell.center().x, cell.top() + 74.0), Align2::CENTER_CENTER, name, reg(14.0), Color32::WHITE);
    app.rects.insert(format!("stage:audience:{u}"), cell);
    let resp = ui.interact(cell, egui::Id::new(("audience", u)), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.open_profile(u, chan.guild, cell.center());
    }
    let (me, c) = (app.store.me, chan.clone());
    egui::Popup::context_menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClick).show(|ui| {
        ui.set_min_width(200.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        if u != me && menu_item(ui, None, "Message", false) {
            app.open_dm(u);
        }
        tile_items(app, ui, &c, u);
    });
}

pub fn tile_items(app: &mut App, ui: &mut Ui, chan: &Channel, u: Id) {
    if chan.kind != 13 || app.voice.chan != Some(chan.id) {
        return;
    }
    let me = app.store.me;
    let audience = app.voice.stage.seats.get(&u).is_some_and(|s| s.audience);
    let mods = is_mod(app, chan);
    if !mods && u != me {
        return;
    }
    menu_sep(ui);
    if audience {
        if mods && menu_item(ui, None, "Invite to Speak", false) {
            if u == me { app.set_speaker(chan, u, true) } else { app.invite_to_speak(chan, u) }
            ui.close();
        }
    } else if menu_item(ui, None, "Move to Audience", false) {
        app.set_speaker(chan, u, false);
        ui.close();
    }
}

fn invited(app: &mut App, ui: &mut Ui, chan: &Channel, r: Rect, me: Id) {
    let seat = app.voice.stage.seats.get(&me).copied().unwrap_or_default();
    if !seat.audience || seat.hand.is_none() || app.call.asked {
        return;
    }
    let card = Rect::from_center_size(pos2(r.center().x, r.bottom() - 150.0), vec2(360.0, 96.0));
    ui.painter().rect(card, CornerRadius::same(8), pal().bg_floating, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(pos2(card.center().x, card.top() + 24.0), Align2::CENTER_CENTER, "You have been invited to speak.", semi(16.0), Color32::WHITE);
    let a = Rect::from_min_size(pos2(card.left() + 16.0, card.bottom() - 48.0), vec2(160.0, 32.0));
    let b = Rect::from_min_size(pos2(a.right() + 8.0, a.top()), vec2(160.0, 32.0));
    app.rects.insert("stage:invited".into(), a);
    if pill(ui, a, "Speak on Stage", BLURPLE, Color32::WHITE, medium(14.0)).clicked() {
        app.set_speaker(chan, me, true);
    }
    if pill(ui, b, "Stay in Audience", pal().control, pal().header_primary, medium(14.0)).clicked() {
        app.request_to_speak(chan, false);
    }
}

pub fn modals(app: &mut App, ctx: &egui::Context) {
    welcome(app, ctx);
    start(app, ctx);
}

fn welcome(app: &mut App, ctx: &egui::Context) {
    if app.settings.stage_welcome || app.settings_page.is_some() {
        return;
    }
    if !app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.kind == 13) {
        return;
    }
    let mut done = false;
    let frame = Frame::new().fill(Color32::from_rgb(0x1a, 0x1b, 0x2e)).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(Margin::symmetric(32, 0));
    let m = crate::ui::motion::Modal::new("stage_welcome".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(416.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let top = ui.cursor().top() - 0.0;
        let panel = ui.max_rect();
        let wash = Rect::from_min_max(pos2(panel.left() - 32.0, top), pos2(panel.right() + 32.0, top + 340.0));
        let (top_c, bottom_c) = (Color32::from_rgb(0x2a, 0x1c, 0x52), Color32::from_rgb(0x1a, 0x1b, 0x2e));
        let mut fade = egui::Mesh::default();
        for (y, c) in [(wash.top(), top_c), (wash.bottom(), bottom_c)] {
            fade.colored_vertex(pos2(wash.left(), y), c);
            fade.colored_vertex(pos2(wash.right(), y), c);
        }
        fade.add_triangle(0, 1, 2);
        fade.add_triangle(1, 2, 3);
        ui.painter().add(fade);
        let close = Rect::from_center_size(pos2(panel.right() + 8.0, top + 32.0), vec2(24.0, 24.0));
        let x = ui.interact(close, egui::Id::new("stage_welcome_x"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, close, crate::ui::motion::mix(pal().header_secondary, Color32::WHITE, crate::ui::motion::hot(ui.ctx(), &x)));
        done |= x.clicked();
        ui.add_space(40.0);
        let (art, _) = ui.allocate_exact_size(vec2(416.0, 160.0), Sense::hover());
        ui.painter().circle_filled(art.center(), 70.0, Color32::from_rgb(0x4a, 0x2f, 0x93));
        icons::draw(ui.painter(), Icon::Mic, Rect::from_center_size(art.center(), vec2(96.0, 96.0)), Color32::from_rgb(0xb7, 0x8c, 0xff));
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Welcome to Stages!").font(semi(24.0)).color(Color32::WHITE)));
        ui.add_space(24.0);
        for (icon, tint, text) in [
            (Icon::MicOff, RED, "Your mic will be muted unless you are invited to speak."),
            (Icon::HangUp, Color32::WHITE, "You can leave and rejoin anytime without making noise."),
            (Icon::Hand, Color32::WHITE, "Request to speak to join the conversation!"),
        ] {
            let (row, _) = ui.allocate_exact_size(vec2(416.0, 64.0), Sense::hover());
            let c = pos2(row.left() + 24.0, row.center().y);
            ui.painter().circle_filled(c, 24.0, Color32::from_rgb(0x0b, 0x0b, 0x10));
            icons::draw(ui.painter(), icon, Rect::from_center_size(c, vec2(20.0, 20.0)), tint);
            let g = ui.painter().layout(text.into(), reg(16.0), Color32::WHITE, 352.0 - 24.0);
            ui.painter().galley(pos2(row.left() + 64.0 + 12.0, row.center().y - g.size().y / 2.0), g, Color32::WHITE);
        }
        ui.add_space(40.0);
        let (b, resp) = ui.allocate_exact_size(vec2(416.0, 48.0), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(8), crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x47, 0x52, 0xc4), crate::ui::motion::hot(ui.ctx(), &resp)));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Got it!", medium(16.0), Color32::WHITE);
        app.rects.insert("stage:welcome_ok".into(), b);
        done |= resp.clicked();
        ui.add_space(32.0);
    });
    if done || m.should_close() {
        app.settings.stage_welcome = true;
        crate::cache::set_settings(&app.settings);
    }
}

fn start(app: &mut App, ctx: &egui::Context) {
    let Some(form) = &mut app.call.start else { return };
    let (mut go, mut close) = (false, false);
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(24);
    let m = crate::ui::motion::Modal::new("stage_start".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.label(RichText::new(if form.edit { "Edit Stage" } else { "Start Stage" }).font(semi(24.0)).color(pal().header_primary));
        ui.add_space(16.0);
        ui.label(RichText::new("STAGE TOPIC").font(semi(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        Frame::new().fill(pal().bg_tertiary).corner_radius(4).inner_margin(10).show(ui, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut form.topic).frame(Frame::NONE).char_limit(120).hint_text("What's your Stage about?").desired_width(ui.available_width()));
            r.request_focus();
        });
        if !form.edit {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let (b, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                ui.painter().rect(b, CornerRadius::same(4), if form.notify { BLURPLE } else { Color32::TRANSPARENT }, Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                if form.notify {
                    icons::draw(ui.painter(), Icon::Check, b.shrink(3.0), Color32::WHITE);
                }
                if resp.clicked() {
                    form.notify = !form.notify;
                }
                ui.label(RichText::new("Mention @everyone when a Stage starts").font(reg(14.0)).color(pal().text_normal));
            });
        }
        ui.add_space(24.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (b, resp) = ui.allocate_exact_size(vec2(if form.edit { 120.0 } else { 120.0 }, 38.0), Sense::click());
            let ready = !form.topic.trim().is_empty();
            ui.painter().rect_filled(b, CornerRadius::same(8), if !ready { BLURPLE.gamma_multiply(0.5) } else { crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x47, 0x52, 0xc4), crate::ui::motion::hot(ui.ctx(), &resp)) });
            ui.painter().text(b.center(), Align2::CENTER_CENTER, if form.edit { "Update Stage" } else { "Start Stage" }, medium(16.0), Color32::WHITE);
            app.rects.insert("stage:start_ok".into(), b);
            go = ready && (resp.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if super::grey_button(ui, "Cancel", 88.0).clicked() {
                close = true;
            }
        });
    });
    if go {
        let f = app.call.start.take().unwrap();
        let ch = f.chan;
        if f.edit {
            app.rest("PATCH", format!("/stage-instances/{ch}"), Some(json!({ "topic": f.topic.trim() })), Tag::reason());
        } else {
            app.rest("POST", "/stage-instances".into(), Some(json!({ "channel_id": ch.to_string(), "topic": f.topic.trim(), "privacy_level": 2, "send_start_notification": f.notify })), Tag::reason());
            app.call.started = Some(ch);
            app.join_voice(ch, true);
        }
    } else if close || m.should_close() {
        app.call.start = None;
    }
}
