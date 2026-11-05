//! The activity shelf behind Start An Activity: a search box, the activities launched last, and the
//! cards for what the server offers in this call.

use egui::{Align2, Color32, Context, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{App, Id, launch};
use crate::theme::*;

#[derive(Default)]
pub struct Shelf {
    pub query: String,
    pub failed: bool,
}

const WIDTH: f32 = 468.0;
const CARD: egui::Vec2 = vec2(224.0, 180.0);

fn icon(ui: &Ui, r: Rect, url: Option<&str>, fill: Color32, radius: u8) {
    ui.painter().rect_filled(r, CornerRadius::same(radius), fill);
    if let Some(u) = url {
        egui::Image::from_uri(u.to_string()).corner_radius(CornerRadius::same(radius)).show_loading_spinner(false).paint_at(ui, r);
    }
}

fn tile(ui: &mut Ui, id: Id, at: Rect) -> egui::Response {
    let resp = ui.interact(at, egui::Id::new(("activity_card", id)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(at, CornerRadius::same(12), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn badge(ui: &Ui, card: Rect, text: &str) {
    let g = ui.painter().layout_no_wrap(text.to_uppercase(), semi(12.0), Color32::from_rgb(0x2f, 0x30, 0x35));
    let r = Rect::from_min_size(pos2(card.right() - 8.0 - g.size().x - 16.0, card.top() + 8.0), vec2(g.size().x + 16.0, 16.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::WHITE);
    ui.painter().galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
}

fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(medium(16.0)).color(pal().header_primary));
}

pub fn show(app: &mut App, ctx: &Context) {
    if app.activity.shelf.is_none() {
        return;
    }
    if app.voice.chan.is_none() {
        app.activity.shelf = None;
        return;
    }
    let g = super::guild_of(app);
    let mut pick = None;
    let frame = Frame::new()
        .fill(pal().bg_floating)
        .stroke(Stroke::new(1.0, pal().border_subtle))
        .corner_radius(12)
        .inner_margin(Margin::same(16))
        .shadow(egui::epaint::Shadow { offset: [0, 12], blur: 24, spread: 0, color: Color32::from_black_alpha(61) });
    let modal = egui::Modal::new("activity_shelf".into()).backdrop_color(Color32::TRANSPARENT).frame(frame).show(ctx, |ui| {
        ui.set_width(WIDTH);
        ui.set_height(648.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let Some(sh) = &mut app.activity.shelf else { return };
        let (bar, _) = ui.allocate_exact_size(vec2(WIDTH, 40.0), Sense::hover());
        ui.painter().rect(bar, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Search, Rect::from_center_size(pos2(bar.left() + 20.0, bar.center().y), vec2(16.0, 16.0)), pal().header_secondary);
        let field = Rect::from_min_max(pos2(bar.left() + 40.0, bar.top() + 4.0), pos2(bar.right() - 8.0, bar.bottom() - 4.0));
        let r = ui.put(field, egui::TextEdit::singleline(&mut sh.query).frame(Frame::NONE).hint_text("Search").font(reg(16.0)).desired_width(field.width()));
        r.request_focus();
        app.rects.insert("activity:search".into(), bar);
        ui.add_space(20.0);
        let q = sh.query.trim().to_lowercase();
        let failed = sh.failed;
        let list: Vec<(Id, &'static str)> = app
            .activity
            .entries
            .get(&g)
            .into_iter()
            .flatten()
            .filter(|e| app.activity.apps.get(&e.app).is_some_and(|i| q.is_empty() || i.name.to_lowercase().contains(&q) || i.desc.to_lowercase().contains(&q)))
            .map(|e| (e.app, e.label))
            .collect();
        let recents: Vec<Id> = app.settings.recent_activities.iter().copied().filter(|a| q.is_empty() && app.activity.apps.contains_key(a)).take(5).collect();
        let loaded = app.activity.entries.contains_key(&g);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(WIDTH);
            if !recents.is_empty() {
                label(ui, "Recents");
                ui.add_space(8.0);
                let (row, _) = ui.allocate_exact_size(vec2(WIDTH, 48.0), Sense::hover());
                for (i, a) in recents.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(row.left() + i as f32 * 60.0, row.top()), vec2(48.0, 48.0));
                    let info = &app.activity.apps[a];
                    icon(ui, r, info.icon.as_deref(), pal().bg_mod_selected, 12);
                    let resp = tile(ui, *a, r);
                    app.rects.insert(format!("activity:recent:{a}"), r);
                    if resp.on_hover_text(&info.name).clicked() {
                        pick = Some(*a);
                    }
                }
                ui.add_space(36.0);
            }
            label(ui, "Activities");
            ui.add_space(8.0);
            if failed || (loaded && list.is_empty()) {
                ui.add_space(24.0);
                let text = if failed { "Failed to fetch activities" } else { "No activities match your search" };
                ui.vertical_centered(|ui| ui.label(RichText::new(text).font(reg(14.0)).color(pal().text_muted)));
                return;
            }
            for pair in list.chunks(2) {
                let (row, _) = ui.allocate_exact_size(vec2(WIDTH, CARD.y), Sense::hover());
                for (i, (a, tag)) in pair.iter().enumerate() {
                    let card = Rect::from_min_size(pos2(row.left() + i as f32 * 236.0, row.top()), CARD);
                    let info = app.activity.apps[a].clone();
                    let banner = Rect::from_min_size(card.min, vec2(CARD.x, 126.0));
                    ui.painter().rect_filled(banner, CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, pal().bg_mod_selected);
                    if let Some(b) = info.banner.as_deref().or(info.icon.as_deref()) {
                        egui::Image::from_uri(b.to_string()).corner_radius(CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }).show_loading_spinner(false).paint_at(ui, banner);
                    }
                    let text = if info.promoted { "Promoted" } else { tag };
                    if !text.is_empty() {
                        badge(ui, card, text);
                    }
                    icon(ui, Rect::from_min_size(card.min + vec2(8.0, 134.0), vec2(38.0, 38.0)), info.icon.as_deref(), pal().bg_mod_selected, 12);
                    let clip = ui.painter().with_clip_rect(Rect::from_min_max(card.min + vec2(58.0, 130.0), card.max - vec2(8.0, 0.0)));
                    clip.text(card.min + vec2(58.0, 134.0), Align2::LEFT_TOP, &info.name, semi(16.0), pal().header_primary);
                    clip.text(card.min + vec2(58.0, 154.0), Align2::LEFT_TOP, &info.desc, reg(14.0), pal().text_muted);
                    let resp = tile(ui, *a, card);
                    app.rects.insert(format!("activity:card:{a}"), card);
                    if resp.clicked() {
                        pick = Some(*a);
                    }
                }
                ui.add_space(12.0);
            }
        });
    });
    if let Some(a) = pick {
        app.settings.recent_activities.retain(|r| *r != a);
        app.settings.recent_activities.insert(0, a);
        app.settings.recent_activities.truncate(5);
        crate::cache::set_settings(&app.settings);
        launch(app, a);
    } else if modal.should_close() {
        app.activity.shelf = None;
    }
}
