//! What activities look like inside the call: the stage the web view sits over, the card for one
//! you haven't joined, and the consent prompt.

use egui::{Align2, Color32, Context, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{App, Id, bridge};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::webwin::child::Status;

pub const STRIP: f32 = 128.0;

pub fn overlaid(app: &App, ctx: &Context) -> bool {
    app.modal_open() || app.layered(ctx) || app.activity.shelf.is_some() || app.activity.consent.is_some() || app.soundboard.is_some() || egui::Popup::is_any_open(ctx) || ctx.memory(|m| m.top_modal_layer().is_some())
}

pub fn popout_hwnd(title: &str) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
    let wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let w = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(w, &mut pid) };
    if w.is_null() || pid != std::process::id() { 0 } else { w as isize }
}

fn button(ui: &mut Ui, r: Rect, id: impl std::hash::Hash + std::fmt::Debug, text: &str, fill: Color32) -> egui::Response {
    let resp = ui.interact(r, egui::Id::new(id), Sense::click());
    ui.painter().rect_filled(r, CornerRadius::same(4), crate::ui::motion::mix(fill, fill.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, semi(13.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The running activity's place in the call: a header with its name and Leave Activity, and
/// below it the web view.
pub fn stage(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(run) = &app.activity.run else { return };
    let (id, failed) = (run.app, run.failed);
    let ctx = ui.ctx().clone();
    ui.painter().rect_filled(area, CornerRadius::same(8), Color32::BLACK);
    let head = Rect::from_min_size(area.min, vec2(area.width(), 40.0));
    let info = app.activity.info(id).cloned().unwrap_or_default();
    let mut x = head.left() + 12.0;
    if let Some(u) = &info.icon {
        let r = Rect::from_center_size(pos2(x + 12.0, head.center().y), vec2(24.0, 24.0));
        egui::Image::from_uri(u.clone()).corner_radius(CornerRadius::same(6)).show_loading_spinner(false).paint_at(ui, r);
        x += 32.0;
    }
    ui.painter().text(pos2(x, head.center().y), Align2::LEFT_CENTER, &info.name, semi(14.0), pal().header_primary);
    let leave = Rect::from_center_size(pos2(head.right() - 12.0 - 52.0, head.center().y), vec2(104.0, 28.0));
    app.rects.insert("activity:leave".into(), leave);
    if button(ui, leave, "activity_leave", "Leave Activity", pal().red).clicked() {
        return super::leave(app);
    }
    let body = Rect::from_min_max(pos2(area.left(), head.bottom()), area.max);
    app.rects.insert("activity:stage".into(), body);
    let (up, started) = match app.activity.run.as_ref().map(|r| r.host.status()) {
        Some(Status::Up) => (true, true),
        Some(Status::Starting) => (false, false),
        _ => (false, true),
    };
    let covered = overlaid(app, &ctx);
    if !up || covered || failed {
        let text = if failed { "This activity couldn't be loaded" } else if started { info.name.as_str() } else { "Starting…" };
        ui.painter().text(body.center(), Align2::CENTER_CENTER, text, semi(16.0), pal().header_secondary);
    }
    let ppp = ctx.pixels_per_point();
    let px = [(body.left() * ppp).round() as i32, (body.top() * ppp).round() as i32, (body.width() * ppp).round() as i32, (body.height() * ppp).round() as i32];
    let parent = if app.activity.surface != 0 { app.activity.surface } else { crate::winsys::WINDOW.load(std::sync::atomic::Ordering::Relaxed) };
    let pressed = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !body.contains(p)));
    if let Some(run) = &app.activity.run {
        run.host.place(parent, px, up && !covered && !failed);
        if pressed {
            run.host.blur();
        }
    }
    app.activity.placed = true;
    app.activity.in_popout = app.activity.surface != 0;
}

/// The card standing for an activity that's running in the call while we're not in it.
pub fn join_tile(app: &mut App, ui: &mut Ui, ch: Id, r: Rect, app_id: Id, users: &[Id]) {
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgb(0x1e, 0x1f, 0x22));
    let info = app.activity.info(app_id).cloned().unwrap_or_default();
    let c = r.center();
    let a = Rect::from_center_size(c - vec2(0.0, 40.0), vec2(56.0, 56.0));
    ui.painter().rect_filled(a, CornerRadius::same(14), pal().bg_mod_selected);
    if let Some(u) = &info.icon {
        egui::Image::from_uri(u.clone()).corner_radius(CornerRadius::same(14)).show_loading_spinner(false).paint_at(ui, a);
    }
    ui.painter().text(c + vec2(0.0, 6.0), Align2::CENTER_CENTER, &info.name, semi(16.0), pal().header_primary);
    let n = users.len();
    ui.painter().text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, format!("{n} {}", if n == 1 { "person is playing" } else { "people are playing" }), reg(12.0), pal().header_secondary);
    let b = Rect::from_center_size(c + vec2(0.0, 58.0), vec2(120.0, 32.0).min(vec2(r.width() - 16.0, 32.0)));
    app.rects.insert(format!("activity:join_tile:{app_id}"), b);
    if app.activity.busy() || app.voice.chan != Some(ch) {
        return;
    }
    if button(ui, b, ("activity_join_tile", app_id), "Join Activity", pal().green).clicked() {
        super::launch(app, app_id);
    }
}

pub fn consent(app: &mut App, ctx: &Context) {
    let Some(c) = &app.activity.consent else { return };
    let (name, icon, scopes) = (c.name.clone(), c.icon.clone(), c.scopes.clone());
    let mut answer = None;
    let frame = Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(12).inner_margin(Margin::same(24));
    let m = crate::ui::motion::Modal::new("activity_consent".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(12), pal().bg_mod_selected);
            if let Some(u) = &icon {
                egui::Image::from_uri(u.clone()).corner_radius(CornerRadius::same(12)).show_loading_spinner(false).paint_at(ui, r);
            }
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(&name).font(semi(20.0)).color(pal().header_primary));
                ui.label(RichText::new("wants to access your Discord account").font(reg(14.0)).color(pal().header_secondary));
            });
        });
        ui.add_space(16.0);
        ui.label(RichText::new(format!("This will allow {name} to:")).font(medium(14.0)).color(pal().header_primary));
        ui.add_space(8.0);
        for s in &scopes {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Check, r, pal().green);
                ui.add_space(8.0);
                ui.add(egui::Label::new(RichText::new(bridge::scope_text(s)).font(reg(14.0)).color(pal().text_normal)).wrap());
            });
            ui.add_space(4.0);
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if crate::ui::grey_button(ui, "Cancel", 192.0).clicked() {
                answer = Some(false);
            }
            let ok = crate::ui::button(ui, "Authorize", pal().blurple, 192.0);
            app.rects.insert("activity:authorize".into(), ok.rect);
            if ok.clicked() {
                answer = Some(true);
            }
        });
    });
    if answer.is_none() && m.should_close() {
        answer = Some(false);
    }
    if let Some(a) = answer {
        super::consent_answer(app, a);
    }
}

/// The activities running in a voice channel, listed under its people with a Join for each.
pub fn list_rows(ui: &mut Ui, store: &crate::state::Store, act: &super::Activity, ch: Id) -> Option<Id> {
    let mut joined = None;
    for i in store.embedded.get(&ch).into_iter().flatten() {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        let info = act.info(i.app).cloned().unwrap_or_default();
        let icon = Rect::from_center_size(pos2(row.left() + 56.0, row.center().y), vec2(20.0, 20.0));
        ui.painter().rect_filled(icon, CornerRadius::same(5), pal().bg_mod_selected);
        if let Some(u) = &info.icon {
            egui::Image::from_uri(u.clone()).corner_radius(CornerRadius::same(5)).show_loading_spinner(false).paint_at(ui, icon);
        }
        let mine = act.runs_in(ch);
        let b = Rect::from_center_size(pos2(row.right() - 40.0, row.center().y), vec2(52.0, 22.0));
        let room = if mine { row.right() - icon.right() - 24.0 } else { b.left() - icon.right() - 16.0 };
        let clip = ui.painter().with_clip_rect(Rect::from_min_max(pos2(icon.right(), row.top()), pos2(icon.right() + 8.0 + room, row.bottom())));
        clip.text(pos2(icon.right() + 8.0, row.center().y), Align2::LEFT_CENTER, &info.name, reg(14.0), pal().channel_default);
        if mine {
            continue;
        }
        let resp = ui.interact(b, egui::Id::new(("activity_join_row", i.app)), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(4), crate::ui::motion::mix(pal().green, pal().green.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Join", semi(12.0), Color32::WHITE);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("activity_join_row_rect", i.app)), b));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text("Join Activity").clicked() {
            joined = Some(i.app);
        }
    }
    joined
}
