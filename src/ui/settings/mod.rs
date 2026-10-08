//! The settings screen, laid out like the official client's: a grouped sidebar on
//! the left, the page on the right, ESC to close.

mod activity;
mod app;
mod user;

use egui::{Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::App;
use crate::net::Tag;
use crate::theme::*;

pub use app::combo;
pub use user::Forms;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Account,
    Profiles,
    Content,
    Privacy,
    Family,
    Apps,
    Devices,
    Connections,
    Nitro,
    Subscriptions,
    Gifts,
    Billing,
    Appearance,
    Accessibility,
    Voice,
    Chat,
    Notifications,
    Keybinds,
    Language,
    Windows,
    Streamer,
    Advanced,
    Activity,
    HypeSquad,
}

enum Item {
    Header(&'static str),
    Page(Page, &'static str),
    Gap,
}

const SIDEBAR: &[Item] = &[
    Item::Header("USER SETTINGS"),
    Item::Page(Page::Account, "My Account"),
    Item::Page(Page::Profiles, "Profiles"),
    Item::Page(Page::Content, "Content & Social"),
    Item::Page(Page::Privacy, "Data & Privacy"),
    Item::Page(Page::Family, "Family Center"),
    Item::Page(Page::Apps, "Authorized Apps"),
    Item::Page(Page::Devices, "Devices"),
    Item::Page(Page::Connections, "Connections"),
    Item::Gap,
    Item::Header("BILLING SETTINGS"),
    Item::Page(Page::Nitro, "Nitro"),
    Item::Page(Page::Subscriptions, "Subscriptions"),
    Item::Page(Page::Gifts, "Gift Inventory"),
    Item::Page(Page::Billing, "Billing"),
    Item::Gap,
    Item::Header("APP SETTINGS"),
    Item::Page(Page::Appearance, "Appearance"),
    Item::Page(Page::Accessibility, "Accessibility"),
    Item::Page(Page::Voice, "Voice & Video"),
    Item::Page(Page::Chat, "Chat"),
    Item::Page(Page::Notifications, "Notifications"),
    Item::Page(Page::Keybinds, "Keybinds"),
    Item::Page(Page::Language, "Language"),
    Item::Page(Page::Windows, "Windows Settings"),
    Item::Page(Page::Streamer, "Streamer Mode"),
    Item::Page(Page::Advanced, "Advanced"),
    Item::Gap,
    Item::Header("ACTIVITY SETTINGS"),
    Item::Page(Page::Activity, "Activity Privacy"),
    Item::Gap,
    Item::Page(Page::HypeSquad, "HypeSquad"),
];

pub fn label(p: Page) -> &'static str {
    SIDEBAR
        .iter()
        .find_map(|i| match i {
            Item::Page(q, l) if *q == p => Some(*l),
            _ => None,
        })
        .unwrap_or("")
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(page) = app.settings_page else { return };
    if app.sform.modal.is_none() && !app.sform.recording() && ui.input(|i| i.key_pressed(Key::Escape)) {
        app.settings_page = None;
        return;
    }
    let side_w = (ui.available_width() * 0.36).max(218.0);
    let mut logout = false;
    egui::Panel::left("settings_side")
        .exact_size(side_w)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_secondary))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(60.0);
                let x = ui.max_rect().right() - 218.0;
                // the search box discord grew in 2024
                let sr = Rect::from_min_size(pos2(x + 6.0, ui.cursor().top()), vec2(180.0, 28.0));
                ui.painter().rect_filled(sr, CornerRadius::same(4), pal().bg_tertiary);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sr.shrink2(vec2(6.0, 4.0))));
                child.add(
                    egui::TextEdit::singleline(&mut app.sform.search)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(reg(14.0))
                        .hint_text(RichText::new("Search").color(pal().text_muted))
                        .desired_width(150.0),
                );
                super::icons::draw(ui.painter(), super::icons::Icon::Search, Rect::from_center_size(pos2(sr.right() - 14.0, sr.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());

                let q = app.sform.search.to_lowercase();
                for item in SIDEBAR {
                    match item {
                        Item::Header(h) if q.is_empty() => {
                            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                            ui.painter().text(pos2(x + 10.0, r.center().y + 4.0), Align2::LEFT_CENTER, *h, bold(12.0), pal().channel_default);
                        }
                        Item::Gap if q.is_empty() => {
                            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
                            ui.painter().hline(x + 10.0..=x + 182.0, r.center().y, Stroke::new(1.0, pal().bg_mod_hover));
                        }
                        Item::Page(p, label) if q.is_empty() || label.to_lowercase().contains(&q) => {
                            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                            let row = Rect::from_min_size(pos2(x, r.top()), vec2(192.0, 32.0));
                            if ui.clip_rect().contains_rect(row) {
                                app.rects.insert(format!("settings:{label}"), row);
                            }
                            let selected = *p == page;
                            if selected {
                                ui.painter().rect_filled(row, CornerRadius::same(4), pal().bg_mod_selected);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(row, CornerRadius::same(4), pal().bg_mod_hover);
                            }
                            let col = if selected { pal().header_primary } else { pal().header_secondary };
                            ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, *label, semi(16.0), col);
                            if resp.clicked() {
                                app.settings_page = Some(*p);
                            }
                        }
                        _ => {}
                    }
                }
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
                ui.painter().hline(x + 10.0..=x + 182.0, r.center().y, Stroke::new(1.0, pal().bg_mod_hover));
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                let row = Rect::from_min_size(pos2(x, r.top()), vec2(192.0, 32.0));
                if resp.hovered() {
                    ui.painter().rect_filled(row, CornerRadius::same(4), pal().bg_mod_hover);
                }
                ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, "Log Out", semi(16.0), RED);
                if resp.clicked() {
                    logout = true;
                }
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
                ui.painter().hline(x + 10.0..=x + 182.0, r.center().y, Stroke::new(1.0, pal().bg_mod_hover));
                for line in [format!("Fastcord {}", env!("CARGO_PKG_VERSION")), "Windows x64".to_string()] {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
                    ui.painter().text(pos2(x + 10.0, r.center().y), Align2::LEFT_CENTER, line, reg(12.0), pal().text_muted);
                }
                ui.add_space(40.0);
            });
        });

    let mut changed = false;
    egui::CentralPanel::default().frame(Frame::new().fill(pal().bg_primary)).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(("settings_page", page as u8)).auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 40, right: 40, top: 60, bottom: 120 }).show(ui, |ui| {
                ui.set_max_width(740.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width().min(740.0) - 72.0);
                        ui.spacing_mut().item_spacing.y = 8.0;
                        // dropdowns and sliders the way discord draws them in settings
                        ui.spacing_mut().interact_size.y = 36.0;
                        ui.spacing_mut().slider_rail_height = 8.0;
                        let v = ui.visuals_mut();
                        v.slider_trailing_fill = true;
                        v.selection.bg_fill = BLURPLE;
                        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                            w.weak_bg_fill = pal().bg_tertiary;
                            w.bg_fill = pal().bg_tertiary;
                            w.corner_radius = 4.into();
                        }
                        v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal().bg_accent);
                        v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
                        changed |= match page {
                            Page::Account => user::account(app, ui),
                            Page::Profiles => user::profiles(app, ui),
                            Page::Content => user::content(app, ui),
                            Page::Privacy => user::privacy(app, ui),
                            Page::Family => user::family(ui),
                            Page::Apps => user::apps(app, ui),
                            Page::Devices => user::devices(app, ui),
                            Page::Connections => user::connections(app, ui),
                            Page::Nitro => user::nitro(app, ui),
                            Page::Subscriptions => user::subscriptions(app, ui),
                            Page::Gifts => user::gifts(app, ui),
                            Page::Billing => user::billing(app, ui),
                            Page::Appearance => app::appearance(app, ui),
                            Page::Accessibility => app::accessibility(app, ui),
                            Page::Voice => super::call::settings(app, ui),
                            Page::Chat => app::chat(app, ui),
                            Page::Notifications => app::notifications(app, ui),
                            Page::Keybinds => app::keybinds(app, ui),
                            Page::Language => app::language(app, ui),
                            Page::Windows => app::windows(app, ui),
                            Page::Streamer => app::streamer(app, ui),
                            Page::Advanced => app::advanced(app, ui),
                            Page::Activity => activity::privacy(app, ui),
                            Page::HypeSquad => activity::hypesquad(app, ui),
                        };
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                        let col = if resp.hovered() { pal().header_primary } else { pal().header_secondary };
                        ui.painter().circle_stroke(r.center(), 17.0, Stroke::new(2.0, col));
                        super::icons::draw(ui.painter(), super::icons::Icon::Close, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), col);
                        ui.painter().text(pos2(r.center().x, r.bottom() + 10.0), Align2::CENTER_CENTER, "ESC", semi(13.0), col);
                        if resp.clicked() {
                            app.settings_page = None;
                        }
                    });
                });
            });
        });
        user::save_bar(app, ui);
    });
    user::modal(app, ui.ctx());
    if changed {
        app.save_settings(ui.ctx());
    }
    if logout {
        app.settings_page = None;
        app.logout();
    }
}

/// Settings pages ask for data by key; the first ask fetches it.
pub fn remote<'a>(app: &'a mut App, key: &'static str, path: &str) -> Option<&'a Result<Value, String>> {
    if !app.remote.contains_key(key) {
        app.remote.insert(key, None);
        app.rest("GET", path.to_string(), None, Tag::Remote(key));
    }
    app.remote.get(key).and_then(|v| v.as_ref())
}

pub fn refetch(app: &mut App, key: &'static str) {
    app.remote.remove(key);
}

pub fn on_remote(app: &mut App, key: &str, res: Result<Value, String>) {
    match key {
        "modal" => user::modal_done(app, res),
        "profile_save" | "nick_save" => user::profile_saved(app, key, res),
        "action" => match res {
            Ok(_) => {}
            Err(e) => app.toast(e),
        },
        _ => {
            if let Some(k) = app.remote.keys().find(|k| **k == key).copied() {
                app.remote.insert(k, Some(res));
            }
        }
    }
}

/// Remember where a control is, for the scripted e2e run to click.
pub fn mark(ui: &Ui, key: &str, r: Rect) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(("e2e", key)), r));
}

// widgets every page is built from

pub fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(bold(20.0)).color(pal().header_primary));
    ui.add_space(12.0);
}

pub fn h5(ui: &mut Ui, s: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(s.to_uppercase()).font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(4.0);
}

pub fn note(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
}

pub fn divider(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 33.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().bg_mod_hover));
}

pub fn switch(ui: &mut Ui, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.12);
    let bg = if on { GREEN } else { Color32::from_rgb(0x80, 0x84, 0x8e) };
    ui.painter().rect_filled(r, CornerRadius::same(12), bg);
    let x = egui::lerp(r.left() + 12.0..=r.right() - 12.0, t);
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Label + description on the left, switch on the right, divider under it.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str, desc: &str) -> bool {
    let mut changed = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 56.0);
            ui.label(RichText::new(label).font(semi(16.0)).color(pal().header_primary));
            if !desc.is_empty() {
                ui.label(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let r = switch(ui, *on);
            mark(ui, &format!("toggle:{label}"), r.rect);
            if r.clicked() {
                *on = !*on;
                changed = true;
            }
        });
    });
    divider(ui);
    changed
}

/// Discord's radio cards: a filled row per option with the dot on the left.
pub fn radio<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str, &str)]) -> bool {
    let mut changed = false;
    for (v, label, desc) in options {
        let selected = *value == *v;
        let h = if desc.is_empty() { 44.0 } else { 62.0 };
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
        mark(ui, &format!("radio:{label}"), r);
        let fill = if selected {
            pal().bg_mod_selected
        } else if resp.hovered() {
            pal().bg_mod_hover
        } else {
            pal().bg_secondary
        };
        ui.painter().rect_filled(r, CornerRadius::same(4), fill);
        let c = pos2(r.left() + 22.0, r.top() + 22.0);
        if selected {
            ui.painter().circle_filled(c, 10.0, Color32::WHITE);
            ui.painter().circle_filled(c, 5.5, BLURPLE);
        } else {
            ui.painter().circle_stroke(c, 9.0, Stroke::new(2.0, pal().interactive_hover));
        }
        ui.painter().text(pos2(r.left() + 46.0, r.top() + 22.0), Align2::LEFT_CENTER, *label, semi(16.0), pal().header_primary);
        if !desc.is_empty() {
            ui.painter().text(pos2(r.left() + 46.0, r.top() + 42.0), Align2::LEFT_CENTER, *desc, reg(14.0), pal().header_secondary);
        }
        if resp.clicked() && !selected {
            *value = *v;
            changed = true;
        }
        ui.add_space(4.0);
    }
    changed
}

/// The slider with labelled tick marks used for font size, spacing and zoom.
pub fn markers(ui: &mut Ui, value: &mut f32, stops: &[f32], label: impl Fn(f32) -> String, default: f32) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::click_and_drag());
    let track = Rect::from_min_size(pos2(r.left() + 10.0, r.top() + 30.0), vec2(w - 20.0, 8.0));
    let n = stops.len().max(2) as f32 - 1.0;
    let x_of = |i: usize| track.left() + track.width() * i as f32 / n;
    let at = stops.iter().position(|s| (*s - *value).abs() < 0.001).unwrap_or(0);
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(4), pal().bg_tertiary);
    p.rect_filled(Rect::from_min_max(track.min, pos2(x_of(at), track.bottom())), CornerRadius::same(4), BLURPLE);
    for (i, s) in stops.iter().enumerate() {
        let x = x_of(i);
        let is_def = (*s - default).abs() < 0.001;
        let col = if is_def { GREEN } else { pal().header_secondary };
        p.text(pos2(x, r.top() + 8.0), Align2::CENTER_CENTER, label(*s), if is_def { semi(12.0) } else { reg(12.0) }, col);
        p.vline(x, (track.top() - 6.0)..=(track.bottom() + 6.0), Stroke::new(2.0, pal().bg_accent));
    }
    let grab = Rect::from_center_size(pos2(x_of(at), track.center().y), vec2(10.0, 24.0));
    p.rect(grab, CornerRadius::same(3), Color32::WHITE, Stroke::new(1.0, pal().bg_accent), egui::StrokeKind::Inside);
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let i = (((pos.x - track.left()) / track.width()) * n).round().clamp(0.0, n) as usize;
        if (stops[i] - *value).abs() > 0.001 {
            *value = stops[i];
            return true;
        }
    }
    false
}

pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    super::button(ui, text, BLURPLE, ui.painter().layout_no_wrap(text.into(), semi(14.0), Color32::WHITE).size().x + 32.0)
}

pub fn secondary(ui: &mut Ui, text: &str) -> egui::Response {
    super::button(ui, text, Color32::from_rgb(0x4e, 0x50, 0x58), ui.painter().layout_no_wrap(text.into(), semi(14.0), Color32::WHITE).size().x + 32.0)
}

pub fn danger(ui: &mut Ui, text: &str, filled: bool) -> egui::Response {
    let w = ui.painter().layout_no_wrap(text.into(), semi(14.0), Color32::WHITE).size().x + 32.0;
    if filled {
        return super::button(ui, text, RED, w);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(w, 38.0), Sense::click());
    let fill = if resp.hovered() { RED } else { Color32::TRANSPARENT };
    ui.painter().rect(r, CornerRadius::same(4), fill, Stroke::new(1.0, RED), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, semi(14.0), if resp.hovered() { Color32::WHITE } else { RED });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn link(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().link)).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// The dark text box discord uses in forms.
pub fn input(ui: &mut Ui, value: &mut String, hint: &str, password: bool) -> egui::Response {
    Frame::new()
        .fill(pal().bg_tertiary)
        .corner_radius(3)
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .password(password)
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .hint_text(RichText::new(hint).color(pal().text_muted))
                    .desired_width(ui.available_width()),
            )
        })
        .inner
}

/// A grey card listing rows, used for devices, apps, connections.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(pal().bg_secondary)
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn loading(ui: &mut Ui, r: Option<&Result<Value, String>>) -> Option<Value> {
    match r {
        None => {
            ui.add(egui::Spinner::new().color(pal().text_muted));
            None
        }
        Some(Err(e)) => {
            ui.label(RichText::new(format!("Couldn't load this from Discord: {e}")).font(reg(14.0)).color(RED));
            None
        }
        Some(Ok(v)) => Some(v.clone()),
    }
}

pub fn open_web(path: &str) {
    let _ = open::that_detached(format!("https://discord.com{path}"));
}
