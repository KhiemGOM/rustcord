//! Following an announcement channel: the header's Follow button and Discord's "Add this channel's
//! updates to your server!" dialog.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, channel_title, text};
use crate::net::Tag;
use crate::state::{Channel, Id, MANAGE_WEBHOOKS};
use crate::theme::*;

pub struct Follow {
    pub channel: Id,
    pub guild: Option<Id>,
    pub target: Option<Id>,
    pub busy: bool,
}

impl Follow {
    pub fn new(channel: Id) -> Self {
        Follow { channel, guild: None, target: None, busy: false }
    }
}

/// Announcement channels can be followed into any server you can add webhooks in.
pub fn can_follow(app: &App, chan: &Channel) -> bool {
    chan.kind == 5 && chan.guild != 0 && app.store.guilds.keys().any(|g| app.store.my_perms(*g) & MANAGE_WEBHOOKS != 0)
}

/// The small grey Follow button after an announcement channel's name.
pub fn header_button(app: &mut App, ui: &mut Ui, chan: &Channel) {
    ui.add_space(12.0);
    let (r, resp) = ui.allocate_exact_size(vec2(62.0, 32.0), Sense::click());
    app.rects.insert("follow".into(), r);
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, CornerRadius::same(8), fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, "Follow", medium(14.0), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.follow = Some(Follow::new(chan.id));
    }
}

impl App {
    /// Crossposts an announcement to the servers following its channel.
    pub fn publish_msg(&mut self, ch: Id, id: Id) {
        self.rest("POST", format!("/channels/{ch}/messages/{id}/crosspost"), None, Tag::Remote("action"));
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
            m.flags |= 1;
        }
    }
}

pub fn done(app: &mut App, ok: bool) {
    if ok {
        let name = app.follow.as_ref().and_then(|f| app.store.channels.get(&f.channel)).map(|c| c.name.clone()).unwrap_or_default();
        app.follow = None;
        app.toast(format!("Now following {name}."));
    } else if let Some(f) = &mut app.follow {
        f.busy = false;
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(f) = app.follow.as_ref() else { return };
    let Some(chan) = app.store.channels.get(&f.channel).cloned() else {
        app.follow = None;
        return;
    };
    let (guild, target, busy) = (f.guild, f.target, f.busy);
    let servers: Vec<(Id, String)> = app
        .store
        .guild_order
        .iter()
        .filter_map(|g| app.store.guilds.get(g))
        .filter(|g| app.store.my_perms(g.id) & MANAGE_WEBHOOKS != 0)
        .map(|g| (g.id, g.name.clone()))
        .collect();
    let channels: Vec<(Id, String)> = guild
        .map(|g| {
            let mut v: Vec<&Channel> = app.store.channels.values().filter(|c| c.guild == g && matches!(c.kind, 0 | 5) && app.store.can_view(c)).collect();
            v.sort_by_key(|c| (c.position, c.id));
            v.into_iter().map(|c| (c.id, c.name.clone())).collect()
        })
        .unwrap_or_default();
    let title = channel_title(&app.store, chan.id);
    let icon = app.store.guilds.get(&chan.guild).and_then(|g| g.icon_url());
    let mut pick_guild = None;
    let mut pick_chan = None;
    let mut cancel = false;
    let mut go = false;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = crate::ui::motion::Modal::new("follow".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (band, _) = ui.allocate_exact_size(vec2(520.0, 80.0), Sense::hover());
        ui.painter().rect_filled(band, CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, pal().bg_tertiary);
        let g = ui.painter().layout_no_wrap(title.clone(), medium(16.0), pal().header_primary);
        let pill_w = 52.0 + g.size().x + 16.0;
        let total = 56.0 + 12.0 + pill_w;
        let x = band.center().x - total / 2.0;
        let av = Rect::from_min_size(pos2(x, band.center().y - 28.0), vec2(56.0, 56.0));
        ui.painter().circle_filled(av.center(), 28.0, pal().bg_floating);
        if let Some(u) = &icon {
            super::avatar(ui, av, u);
        }
        let pill = Rect::from_min_size(pos2(av.right() + 12.0, band.center().y - 18.0), vec2(pill_w, 36.0));
        ui.painter().rect_filled(pill, CornerRadius::same(8), Color32::from_black_alpha(90));
        icons::draw(ui.painter(), Icon::Megaphone, Rect::from_center_size(pos2(pill.left() + 20.0, pill.center().y), vec2(20.0, 20.0)), pal().channel_default);
        text::paint(ui.painter(), pos2(pill.left() + 36.0, pill.center().y), Align2::LEFT_CENTER, &title, medium(16.0), pal().header_primary, pill_w - 44.0);
        ui.add_space(0.0);
        Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
            ui.set_width(472.0);
            ui.add_space(8.0);
            let (t, _) = ui.allocate_exact_size(vec2(472.0, 32.0), Sense::hover());
            ui.painter().text(t.center(), Align2::CENTER_CENTER, "Add this channel's updates to your server!", semi(24.0), pal().header_primary);
            ui.add_space(8.0);
            let body = ui.painter().layout("Select where you want these updates to go. You can remove this at any time in Server Settings > Integrations > Channels Followed.".into(), reg(14.0), pal().header_secondary, 440.0);
            let (b, _) = ui.allocate_exact_size(vec2(472.0, body.size().y), Sense::hover());
            ui.painter().galley(pos2(b.center().x - body.size().x / 2.0, b.top()), body, pal().header_secondary);
            ui.add_space(24.0);
            label(ui, "Send to");
            let shown = guild.and_then(|g| servers.iter().find(|s| s.0 == g)).map_or("Select...".to_string(), |s| s.1.clone());
            super::settings::dropdown(ui, 472.0, None, &shown, |ui| {
                for (id, name) in &servers {
                    if super::menu_item(ui, None, name, false) {
                        pick_guild = Some(*id);
                    }
                }
            });
            ui.add_space(20.0);
            label(ui, "Select channel");
            let shown = target.and_then(|t| channels.iter().find(|c| c.0 == t)).map_or("Select...".to_string(), |c| format!("#{}", c.1));
            ui.add_enabled_ui(guild.is_some(), |ui| {
                super::settings::dropdown(ui, 472.0, None, &shown, |ui| {
                    for (id, name) in &channels {
                        if super::menu_item(ui, Some(Icon::Hash), name, false) {
                            pick_chan = Some(*id);
                        }
                    }
                });
            });
            ui.add_space(16.0);
            let note = ui.painter().layout("Only servers and channels you have webhook permissions in are shown.".into(), reg(14.0), pal().header_secondary, 472.0);
            let (n, _) = ui.allocate_exact_size(vec2(472.0, note.size().y), Sense::hover());
            ui.painter().galley(n.min, note, pal().header_secondary);
            ui.add_space(32.0);
            let (row, _) = ui.allocate_exact_size(vec2(472.0, 38.0), Sense::hover());
            let ready = target.is_some() && !busy;
            let follow = Rect::from_min_size(pos2(row.right() - 100.0, row.top()), vec2(100.0, 38.0));
            let can = Rect::from_min_size(pos2(follow.left() - 8.0 - 100.0, row.top()), vec2(100.0, 38.0));
            go = button(ui, follow, "Follow", true, ready);
            cancel = button(ui, can, "Cancel", false, true);
        });
    });
    if let Some(g) = pick_guild
        && let Some(f) = &mut app.follow
    {
        f.guild = Some(g);
        f.target = None;
    }
    if let Some(c) = pick_chan
        && let Some(f) = &mut app.follow
    {
        f.target = Some(c);
    }
    if go
        && let Some(t) = target
    {
        if let Some(f) = &mut app.follow {
            f.busy = true;
        }
        app.rest("POST", format!("/channels/{}/followers", chan.id), Some(json!({ "webhook_channel_id": t.to_string() })), Tag::Follow);
    } else if cancel || m.should_close() {
        app.follow = None;
    }
}

fn label(ui: &mut Ui, s: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, s, semi(16.0), pal().header_primary);
    ui.add_space(8.0);
}

fn button(ui: &Ui, r: Rect, label: &str, primary: bool, on: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(("follow_btn", label)), if on { Sense::click() } else { Sense::hover() });
    super::settings::mark(ui, &format!("follow:{label}"), r);
    let fade = if on { 1.0 } else { 0.5 };
    let fill = if primary {
        if resp.hovered() && on { Color32::from_rgb(0x44, 0x52, 0xbb) } else { BLURPLE }
    } else if resp.hovered() {
        pal().control_hover
    } else {
        pal().control
    };
    ui.painter().rect(r, CornerRadius::same(8), fill.gamma_multiply(fade), Stroke::new(1.0, if primary { Color32::from_white_alpha(20) } else { pal().border_subtle }), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(16.0), if primary { Color32::WHITE } else { pal().header_primary }.gamma_multiply(fade));
    if resp.hovered() && on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
