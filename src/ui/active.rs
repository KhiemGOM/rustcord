//! Discord's "All Active Threads" for a server: every open thread you can see, newest first, as
//! cards that open the thread.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, pos2, vec2};

use super::App;
use super::icons::{self, Icon};
use crate::state::{Id, snowflake_ms};
use crate::theme::*;

fn ago(ms: i64) -> String {
    let s = ((chrono::Utc::now().timestamp_millis() - ms) / 1000).max(0);
    match s {
        s if s < 60 => format!("{s}s ago"),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86400 => format!("{}h ago", s / 3600),
        s if s < 30 * 86400 => format!("{}d ago", s / 86400),
        s => format!("{}mo ago", s / (30 * 86400)),
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(gid) = app.active_threads else { return };
    let s = &app.store;
    let mut threads: Vec<&crate::state::Channel> = s.channels.values().filter(|c| c.guild == gid && c.is_thread() && !c.archived && s.can_view(c)).collect();
    threads.sort_by_key(|t| std::cmp::Reverse(t.last_msg.max(t.id)));
    let rows: Vec<(Id, String, String, String, Option<String>)> = threads
        .iter()
        .map(|t| {
            let parent = s.channels.get(&t.parent).map(|p| format!("#{}", p.name)).unwrap_or_default();
            let last = s.msgs.get(&t.id).and_then(|e| e.list.last()).filter(|m| !m.content.is_empty());
            let line = match last {
                Some(m) => format!("{}: {}", s.name_in(gid, m.author), crate::markdown::plain(&m.content, s, gid)),
                None => "No recent messages".to_string(),
            };
            let line = format!("{line}\u{a0} \u{2022} \u{a0}{}", ago(snowflake_ms(t.last_msg.max(t.id))));
            (t.id, t.name.clone(), parent, line, s.user(t.owner).map(|u| u.avatar_url()))
        })
        .collect();
    let mut open = None;
    let mut close = false;
    let mut cards = Vec::new();
    let screen = ctx.content_rect();
    let h = (screen.height() - 112.0).min(800.0);
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = crate::ui::motion::Modal::new("active_threads".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.set_height(h);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let top = ui.max_rect();
        let p = ui.painter();
        p.text(pos2(top.left() + 24.0, top.top() + 38.0), Align2::LEFT_CENTER, "All Active Threads", semi(20.0), pal().header_primary);
        let x = Rect::from_min_size(pos2(top.right() - 50.0, top.top() + 10.0), vec2(40.0, 40.0));
        let xr = ui.interact(x, ui.id().with("close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &xr)));
        close |= xr.clicked();
        ui.painter().hline(top.x_range(), top.top() + 74.5, Stroke::new(1.0, pal().border_subtle));
        let list = Rect::from_min_max(pos2(top.left(), top.top() + 75.0), pos2(top.right(), top.bottom() - 88.0));
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut body, |ui| {
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 12, bottom: 12 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if rows.is_empty() {
                    ui.label(egui::RichText::new("There are no active threads.").font(reg(14.0)).color(pal().header_secondary));
                }
                for (id, name, parent, line, avatar) in &rows {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(pal().bg_floating, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
                    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    let n = super::text::paint_em(p, pos2(r.left() + 16.0, r.top() + 26.0), Align2::LEFT_CENTER, name, semi(16.0), pal().text_normal, r.width() - 140.0, 1.0);
                    p.text(pos2(n.right() + 8.0, r.top() + 27.0), Align2::LEFT_CENTER, parent, medium(14.0), pal().text_muted);
                    super::text::paint(p, pos2(r.left() + 16.0, r.top() + 46.0), Align2::LEFT_CENTER, line, reg(14.0), pal().text_normal, r.width() - 80.0);
                    if let Some(a) = avatar {
                        super::avatar(ui, Rect::from_center_size(pos2(r.right() - 28.0, r.center().y), vec2(24.0, 24.0)), a);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        open = Some(*id);
                    }
                    cards.push((format!("thread_card:{id}"), r));
                }
            });
        });
        let b = Rect::from_min_size(pos2(top.left() + 24.0, top.bottom() - 64.0), vec2(top.width() - 48.0, 40.0));
        let br = ui.interact(b, ui.id().with("close_button"), Sense::click());
        ui.painter().rect_filled(b, CornerRadius::same(8), crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x44, 0x52, 0xbb), crate::ui::motion::hot(ui.ctx(), &br)));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Close", medium(16.0), Color32::WHITE);
        close |= br.clicked();
    });
    app.rects.extend(cards);
    if m.should_close() || close {
        app.active_threads = None;
    }
    if let Some(t) = open {
        app.active_threads = None;
        let shift = ctx.input(|i| i.modifiers.shift);
        app.open_thread(t, true, shift);
    }
}
