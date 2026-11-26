use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, avatar, icon_button, status_dot, text};
use crate::state::Status;
use crate::theme::*;

/// How much of the bottom the floating account card takes, its 8 margin included. The rail and
/// the sidebar both stop short of it.
pub fn foot(app: &App) -> f32 {
    66.0 + if app.voice.chan.is_some() { 102.0 } else { 0.0 } + if app.game.is_some() { 57.0 } else { 0.0 }
}

/// Discord's account card, floating 8 in from the left of the server rail, the bottom, and the
/// sidebar's right edge. A game we detected gets the top of it, then the voice controls while
/// connected.
pub fn card(app: &mut App, ui: &mut Ui, rail: Rect, side: Rect) {
    let card = Rect::from_min_max(pos2(rail.left() + 8.0, side.bottom() - foot(app)), pos2(side.right() - 9.0, side.bottom() - 8.0));
    ui.painter().rect(card, 8, pal().bg_secondary_alt, egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
    let user = Rect::from_min_max(pos2(card.left(), card.bottom() - 58.0), card.max);
    let mut top = card.top();
    if app.game.is_some() {
        let area = Rect::from_min_size(card.min + vec2(1.0, 1.0), vec2(card.width() - 2.0, 56.0));
        game_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)), area);
        top = area.bottom();
    }
    if app.voice.chan.is_some() {
        ui.painter().hline(card.x_range().shrink(1.0), user.top() - 0.5, egui::Stroke::new(1.0, pal().border_muted));
        let area = Rect::from_min_max(pos2(card.left() + 1.0, top + 1.0), pos2(card.right() - 1.0, user.top() - 1.0)).shrink(12.0);
        // children that take no room from the window's layout; the chat panel comes after
        super::super::call::panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(area)));
    }
    let top_rounded = app.game.is_none() && app.voice.chan.is_none();
    if let Some(asset) = app.store.nameplate_in(0, app.store.me) {
        let plate = dock_nameplate_rect(user.shrink(1.0));
        let cr = dock_nameplate_radius(top_rounded);
        let hot = ui.rect_contains_pointer(user);
        super::super::cosmo::paint_plate(ui, plate, asset, cr, Color32::WHITE, hot, None, Some(pal().bg_secondary_alt));
    }
    app.rects.insert("userpanel".into(), user);
    user_panel(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(user.shrink2(vec2(12.0, 0.0)))));
}

fn dock_nameplate_rect(area: Rect) -> Rect {
    Rect::from_min_max(pos2((area.right() - area.height() * 448.0 / 84.0).max(area.left()), area.top()), area.max)
}

fn dock_nameplate_radius(top_rounded: bool) -> CornerRadius {
    let top = if top_rounded { 7 } else { 0 };
    CornerRadius { nw: top, ne: top, sw: 7, se: 7 }
}

fn dock_subtitle_shows_username(hovered: bool, popout_open: bool) -> bool {
    hovered || popout_open
}

fn dock_shows_activity(available: f32, clips: bool) -> bool {
    // gear(32) + deafen(57) + mute(57) + activity(36) + 8px gap + 40px min name (+ clips 32)
    available >= 146.0 + 36.0 + 8.0 + 40.0 + if clips { 32.0 } else { 0.0 }
}

fn dock_name_width(available: f32, clips: bool, show_activity: bool) -> f32 {
    let buttons = 146.0 + if show_activity { 36.0 } else { 0.0 } + 8.0 + if clips { 32.0 } else { 0.0 };
    (available - buttons).max(40.0)
}

/// The game's panel: its icon and name, "Not Sharing" while activity sharing is off (a click
/// there opens the setting), and the button that streams it.
fn game_panel(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(g) = app.game.clone() else { return };
    ui.painter().rect_filled(area, CornerRadius { nw: 7, ne: 7, ..Default::default() }, pal().bg_secondary);
    ui.painter().hline(area.x_range(), area.bottom() + 0.5, egui::Stroke::new(1.0, pal().border_subtle));
    app.rects.insert("game:panel".into(), area);
    let inner = area.shrink(12.0);
    let icon = Rect::from_min_size(inner.min, vec2(32.0, 32.0));
    match &g.icon {
        Some(h) => {
            egui::Image::from_uri(format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), g.id)).corner_radius(8).show_loading_spinner(false).paint_at(ui, icon);
        }
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(8), pal().bg_mod_selected);
            icons::draw(ui.painter(), Icon::Controller, Rect::from_center_size(icon.center(), vec2(20.0, 20.0)), pal().header_secondary);
        }
    }
    app.rects.insert("game:icon".into(), icon);

    let live = app.voice.stream.as_ref().is_some_and(|l| l.mine);
    let btn = Rect::from_center_size(pos2(inner.right() - 14.5, inner.center().y), vec2(32.0, 32.0));
    let resp = ui.interact(btn, ui.id().with("game_live"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(btn, CornerRadius::same(8), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let col = if live { pal().green } else { crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)) };
    icons::draw(ui.painter(), Icon::ScreenArrow, Rect::from_center_size(btn.center(), vec2(20.0, 20.0)), col);
    app.rects.insert("game:live".into(), btn);
    let tip = match (app.voice.chan.is_some(), live) {
        (_, true) => "Stop Streaming".to_string(),
        (true, false) => format!("Stream {}", g.name),
        // discord offers its channel picker here; rustcord streams from a call
        (false, false) => "Join a voice channel to stream".to_string(),
    };
    if resp.on_hover_text(tip).clicked() && app.voice.chan.is_some() {
        if live {
            app.voice.end_stream(&app.net);
        } else {
            app.share = Some(super::super::share::Picker::new(ui.ctx()));
        }
    }

    // discord says nothing under the name while it's being shared
    let x = icon.right() + 8.0;
    let room = btn.left() - 8.0 - x;
    let hidden = app.store.hide_game || app.store.presence.get(&app.store.me) == Some(&Status::Offline);
    let y = if hidden { inner.top() + 8.0 } else { inner.center().y };
    text::paint(ui.painter(), pos2(x, y), Align2::LEFT_CENTER, &g.name, medium(16.0), pal().text_normal, room);
    if hidden {
        let galley = ui.painter().layout_no_wrap("Not Sharing".into(), medium(12.0), pal().header_secondary);
        let r = Rect::from_min_size(pos2(x, inner.top() + 16.0), vec2(galley.size().x, 16.0));
        let resp = ui.interact(r, ui.id().with("game_sharing"), Sense::click());
        let col = crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp));
        ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, "Not Sharing", medium(12.0), col);
        app.rects.insert("game:not_sharing".into(), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            app.settings_page = Some(super::super::settings::Page::Activity);
        }
    }
}

fn user_panel(app: &mut App, ui: &mut Ui) {
    let me = app.store.me;
    let user = app.store.user(me).cloned();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let bg = ui.painter().add(egui::Shape::Noop);
        let (av, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        if let Some(u) = &user {
            avatar(ui, av, &u.avatar_url());
        }
        let status = match app.conn {
            crate::net::Conn::Online => app.store.presence.get(&me).copied().unwrap_or(Status::Online),
            _ => Status::Offline,
        };
        status_dot(ui, av, status, pal().bg_secondary_alt);
        let avail = ui.available_width();
        let show_activity = dock_shows_activity(avail, app.settings.clips);
        let col = dock_name_width(avail, app.settings.clips, show_activity);
        // only the avatar and names count, as discord's account button; the buttons beside don't
        let hot = ui.rect_contains_pointer(Rect::from_min_max(ui.max_rect().min, pos2(av.right() + 8.0 + col, ui.max_rect().bottom())));
        let account_id = ui.id().with("account");
        let popup_id = account_id.with("popup");
        let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let show_user = dock_subtitle_shows_username(hot, open);
        let names = ui.allocate_ui_with_layout(vec2(col, 40.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(col);
            ui.set_max_width(col);
            ui.spacing_mut().item_spacing.y = -2.0;
            let name = user.as_ref().map(|u| u.display().to_string()).unwrap_or_default();
            text::label(ui, &name, medium(16.0), pal().header_primary, Sense::hover());
            let sub = match app.conn {
                crate::net::Conn::Online if app.settings.hiding_personal() => "Streamer Mode".into(),
                // discord shows the custom status, or the status itself, and the username on hover or open popout
                crate::net::Conn::Online if show_user => user.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
                crate::net::Conn::Online if app.store.custom_emoji.as_ref().is_some_and(|e| e.id == 0) => {
                    format!("{} {}", app.store.custom_emoji.as_ref().map_or("", |e| e.name.as_str()), app.store.custom_status.as_deref().unwrap_or_default()).trim().to_string()
                }
                crate::net::Conn::Online if app.store.custom_emoji.as_ref().is_some_and(|e| e.id != 0) => app.store.custom_status.clone().unwrap_or_default(),
                crate::net::Conn::Online => app.store.custom_status.clone().unwrap_or_else(|| {
                    match status {
                        Status::Online => "Online",
                        Status::Idle => "Idle",
                        Status::Dnd => "Do Not Disturb",
                        Status::Offline => "Invisible",
                    }
                    .into()
                }),
                crate::net::Conn::Connecting => "Connecting…".into(),
                crate::net::Conn::Resuming => "Reconnecting…".into(),
                crate::net::Conn::Offline => "Offline".into(),
            };
            // in a call discord swaps the line for a green speaker and "In voice"
            let in_voice = matches!(app.conn, crate::net::Conn::Online) && app.voice.chan.is_some() && !show_user && !app.settings.hiding_personal();
            if in_voice {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    let stage = app.voice.chan.and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.kind == 13);
                    icons::draw(ui.painter(), if stage { Icon::Stage } else { Icon::Speaker }, r, pal().green);
                    ui.add(egui::Label::new(egui::RichText::new(if stage { "In a stage" } else { "In voice" }).font(medium(12.0)).color(pal().text_muted)).selectable(false));
                });
            } else if let Some(e) = app.store.custom_emoji.clone().filter(|e| e.id != 0 && matches!(app.conn, crate::net::Conn::Online) && !show_user && !app.settings.hiding_personal()) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    egui::Image::from_uri(crate::markdown::emoji_url(e.id, e.animated)).show_loading_spinner(false).paint_at(ui, r);
                    app.rects.insert("side:status_emoji".into(), r);
                    ui.add(egui::Label::new(egui::RichText::new(sub).font(medium(12.0)).color(pal().header_secondary)).truncate().selectable(false));
                });
            } else {
                ui.add(egui::Label::new(egui::RichText::new(sub).font(medium(12.0)).color(pal().header_secondary)).truncate().selectable(false));
            }
        });
        let hit = Rect::from_x_y_ranges(av.left() - 4.0..=names.response.rect.right() + 4.0, av.center().y - 20.0..=av.center().y + 20.0);
        let resp = ui.interact(hit, account_id, Sense::click());
        if open || resp.hovered() {
            ui.painter().set(bg, egui::Shape::rect_filled(hit, CornerRadius::same(8), MENU_HOVER));
        }
        egui::Popup::menu(&resp)
            .id(popup_id)
            .align(egui::RectAlign::TOP_START)
            .gap(12.0)
            .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(egui::Stroke::new(1.0, pal().border_subtle)).shadow(ui.style().visuals.popup_shadow))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| super::super::popups::account(app, ui));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let gear = icon_button(ui, Icon::Gear, 32.0, "User Settings");
            app.rects.insert("gear".into(), gear.rect);
            if gear.clicked() {
                app.settings_page = Some(super::super::settings::Page::Account);
            }
            if app.settings.clips {
                super::super::clips::panel_button(app, ui);
            }
            super::super::call::user_buttons(app, ui);
            if show_activity {
                activity_button(app, ui);
            }
        });
    });
}

fn activity_button(app: &mut App, ui: &mut Ui) {
    ui.add_space(4.0);
    let hidden = app.store.hide_game;
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let k = crate::ui::motion::hover(ui.ctx(), &resp, 0.2);
    let bg = if hidden { pal().danger.gamma_multiply(0.12 + 0.08 * k) } else { pal().control_hover.gamma_multiply(k) };
    ui.painter().rect_filled(r, CornerRadius::same(8), bg);
    let fg = if hidden {
        pal().danger
    } else {
        crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp))
    };
    icons::draw(ui.painter(), if hidden { Icon::ControllerOff } else { Icon::Controller }, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), fg);
    app.rects.insert("side:game_activity".into(), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let tip = if hidden { "Share Your Activity" } else { "Hide Your Activity" };
    if resp.on_hover_text(tip).clicked() {
        super::super::settings::set_share_activity(app, hidden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_subtitle_shows_username_on_hover_or_open_popout() {
        assert!(!dock_subtitle_shows_username(false, false));
        assert!(dock_subtitle_shows_username(true, false));
        assert!(dock_subtitle_shows_username(false, true));
        assert!(dock_subtitle_shows_username(true, true));
    }

    #[test]
    fn dock_name_width_reserves_controls_without_collision() {
        assert!(dock_shows_activity(295.0, false));
        assert!(dock_shows_activity(231.0, false));
        assert!(!dock_shows_activity(191.0, false));
        assert!(dock_shows_activity(295.0, true));
        assert!(!dock_shows_activity(231.0, true));
        assert_eq!(dock_name_width(295.0, false, true), 105.0);
        assert_eq!(dock_name_width(231.0, false, true), 41.0);
        assert_eq!(dock_name_width(191.0, false, false), 40.0);
        assert_eq!(dock_name_width(295.0, true, true), 73.0);
    }

    #[test]
    fn dock_nameplate_rect_and_radius_fit_card() {
        let area = Rect::from_min_size(pos2(9.0, 100.0), vec2(357.0, 56.0));
        let plate = dock_nameplate_rect(area);
        assert_eq!(plate.right(), area.right());
        assert_eq!(plate.top(), area.top());
        assert_eq!(plate.bottom(), area.bottom());
        assert!((plate.width() - 56.0 * 448.0 / 84.0).abs() < 0.01);
        assert_eq!(dock_nameplate_radius(true), CornerRadius { nw: 7, ne: 7, sw: 7, se: 7 });
        assert_eq!(dock_nameplate_radius(false), CornerRadius { nw: 0, ne: 0, sw: 7, se: 7 });
    }
}
