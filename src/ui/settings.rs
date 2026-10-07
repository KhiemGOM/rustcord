use egui::{Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, RichText, Sense, Ui, pos2, vec2};

use super::{App, avatar, button};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Account,
    Appearance,
    Notifications,
    Advanced,
}

const PAGES: [(&str, Option<(Page, &str)>); 6] = [
    ("USER SETTINGS", None),
    ("", Some((Page::Account, "My Account"))),
    ("APP SETTINGS", None),
    ("", Some((Page::Appearance, "Appearance"))),
    ("", Some((Page::Notifications, "Notifications"))),
    ("", Some((Page::Advanced, "Advanced"))),
];

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(page) = app.settings_page else { return };
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        app.settings_page = None;
        return;
    }
    let side_w = (ui.available_width() * 0.36).max(218.0);
    let mut logout = false;
    egui::Panel::left("settings_side")
        .exact_size(side_w)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BG_SECONDARY))
        .show(ui, |ui| {
            ui.add_space(60.0);
            let x = ui.max_rect().right() - 218.0;
            for (header, item) in PAGES {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), if item.is_some() { 34.0 } else { 28.0 }), Sense::click());
                match item {
                    None => {
                        ui.painter().text(pos2(x + 10.0, r.center().y + 4.0), Align2::LEFT_CENTER, header, bold(12.0), CHANNEL_DEFAULT);
                    }
                    Some((p, label)) => {
                        let row = egui::Rect::from_min_size(pos2(x, r.top() + 1.0), vec2(192.0, 32.0));
                        let selected = p == page;
                        if selected {
                            ui.painter().rect_filled(row, CornerRadius::same(4), BG_MOD_SELECTED);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(row, CornerRadius::same(4), BG_MOD_HOVER);
                        }
                        let col = if selected { HEADER_PRIMARY } else { HEADER_SECONDARY };
                        ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, label, semi(16.0), col);
                        if resp.clicked() {
                            app.settings_page = Some(p);
                        }
                    }
                }
            }
            let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
            ui.painter().hline(x + 10.0..=x + 182.0, sep.center().y, egui::Stroke::new(1.0, BG_MOD_HOVER));
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let row = egui::Rect::from_min_size(pos2(x, r.top() + 1.0), vec2(192.0, 32.0));
            if resp.hovered() {
                ui.painter().rect_filled(row, CornerRadius::same(4), BG_MOD_HOVER);
            }
            ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, "Log Out", semi(16.0), RED);
            if resp.clicked() {
                logout = true;
            }
        });

    let mut changed = false;
    egui::CentralPanel::default().frame(Frame::new().fill(BG_PRIMARY)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 40, right: 40, top: 60, bottom: 80 }).show(ui, |ui| {
                ui.set_max_width(740.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(660.0);
                        ui.spacing_mut().item_spacing.y = 8.0;
                        match page {
                            Page::Account => account(app, ui, &mut logout),
                            Page::Appearance => changed |= appearance(app, ui),
                            Page::Notifications => changed |= notifications(app, ui),
                            Page::Advanced => changed |= advanced(app, ui),
                        }
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                        let col = if resp.hovered() { HEADER_PRIMARY } else { HEADER_SECONDARY };
                        ui.painter().circle_stroke(r.center(), 17.0, egui::Stroke::new(2.0, col));
                        super::icons::draw(ui.painter(), super::icons::Icon::Close, egui::Rect::from_center_size(r.center(), vec2(18.0, 18.0)), col);
                        ui.painter().text(pos2(r.center().x, r.bottom() + 10.0), Align2::CENTER_CENTER, "ESC", semi(13.0), col);
                        if resp.clicked() {
                            app.settings_page = None;
                        }
                    });
                });
            });
        });
    });
    if changed {
        app.save_settings(ui.ctx());
    }
    if logout {
        app.settings_page = None;
        app.logout();
    }
}

fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(bold(20.0)).color(HEADER_PRIMARY));
    ui.add_space(12.0);
}

fn toggle(ui: &mut Ui, on: &mut bool, label: &str, note: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width() - 60.0);
            ui.label(RichText::new(label).font(semi(16.0)).color(HEADER_PRIMARY));
            if !note.is_empty() {
                ui.label(RichText::new(note).font(reg(14.0)).color(HEADER_SECONDARY));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let (r, resp) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
            let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
            let bg = if *on { GREEN } else { Color32::from_rgb(0x80, 0x84, 0x8e) };
            ui.painter().rect_filled(r, CornerRadius::same(12), bg);
            let x = egui::lerp(r.left() + 12.0..=r.right() - 12.0, t);
            ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
            if resp.clicked() {
                *on = !*on;
                changed = true;
            }
        });
    });
    let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 21.0), Sense::hover());
    ui.painter().hline(sep.x_range(), sep.center().y, egui::Stroke::new(1.0, BG_MOD_HOVER));
    changed
}

fn account(app: &mut App, ui: &mut Ui, logout: &mut bool) {
    title(ui, "My Account");
    let me = app.store.user(app.store.me).cloned().unwrap_or_default();
    Frame::new().fill(BG_TERTIARY).corner_radius(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (banner, _) = ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
        ui.painter().rect_filled(banner, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, BLURPLE);
        let av = egui::Rect::from_min_size(pos2(banner.left() + 16.0, banner.bottom() - 28.0), vec2(80.0, 80.0));
        ui.painter().circle_filled(av.center(), 46.0, BG_TERTIARY);
        avatar(ui, av, &me.avatar_url());
        ui.horizontal(|ui| {
            ui.add_space(112.0);
            ui.label(RichText::new(me.display()).font(bold(20.0)).color(HEADER_PRIMARY));
        });
        ui.add_space(20.0);
        Frame::new().fill(BG_SECONDARY).corner_radius(8).inner_margin(16).outer_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            let field = |ui: &mut Ui, k: &str, v: &str| {
                ui.label(RichText::new(k).font(bold(12.0)).color(HEADER_SECONDARY));
                ui.label(RichText::new(v).font(reg(16.0)).color(HEADER_PRIMARY));
                ui.add_space(16.0);
            };
            field(ui, "DISPLAY NAME", me.display());
            field(ui, "USERNAME", &me.username);
            let reveal_id = egui::Id::new("reveal_email");
            let reveal: bool = ui.data(|d| d.get_temp(reveal_id).unwrap_or(false));
            let email = app.store.email.clone().unwrap_or_else(|| "You haven't added an email yet.".into());
            let shown = if reveal {
                email
            } else {
                match email.split_once('@') {
                    Some((user, dom)) => format!("{}@{dom}", "*".repeat(user.chars().count())),
                    None => email,
                }
            };
            ui.label(RichText::new("EMAIL").font(bold(12.0)).color(HEADER_SECONDARY));
            ui.horizontal(|ui| {
                ui.label(RichText::new(shown).font(reg(16.0)).color(HEADER_PRIMARY));
                let label = if reveal { "Hide" } else { "Reveal" };
                if ui.add(egui::Label::new(RichText::new(label).font(reg(14.0)).color(LINK)).sense(Sense::click())).clicked() {
                    ui.data_mut(|d| d.insert_temp(reveal_id, !reveal));
                }
            });
        });
    });
    ui.add_space(24.0);
    if button(ui, "Log Out", RED, 96.0).clicked() {
        *logout = true;
    }
}

fn appearance(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Appearance");
    let s = &mut app.settings;
    let mut changed = false;
    ui.label(RichText::new("ZOOM LEVEL").font(bold(12.0)).color(HEADER_SECONDARY));
    let mut pct = (s.zoom * 100.0).round();
    let r = ui.add(egui::Slider::new(&mut pct, 50.0..=200.0).step_by(10.0).suffix("%"));
    if r.drag_stopped() || (r.changed() && !r.dragged()) {
        s.zoom = pct / 100.0;
        changed = true;
    }
    ui.label(RichText::new("Ctrl + and Ctrl − work too.").font(reg(14.0)).color(HEADER_SECONDARY));
    ui.add_space(16.0);
    changed |= toggle(ui, &mut s.clock24, "24-hour clock", "Show timestamps like 16:20 instead of 4:20 PM.");
    changed |= toggle(ui, &mut s.embeds, "Show link previews", "Display embeds for links posted in chat.");
    changed
}

fn notifications(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Notifications");
    let s = &mut app.settings;
    let mut changed = toggle(ui, &mut s.notify, "Enable Desktop Notifications", "Mentions, replies and DMs show up as Windows notifications.");
    changed |= toggle(ui, &mut s.notify_all_dms, "Notify for every DM", "Otherwise only DMs that mention you notify.");
    changed |= toggle(ui, &mut s.flash, "Flash taskbar", "Flash the taskbar button when something needs your attention.");
    changed
}

fn advanced(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Advanced");
    toggle(
        ui,
        &mut app.settings.dev,
        "Developer Mode",
        "Adds Copy ID to the context menus of servers, channels, messages and users.",
    )
}
