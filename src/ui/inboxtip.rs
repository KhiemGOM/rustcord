//! What an empty Bookmarks, Reminders or Scheduled tab of the inbox shows.

use egui::{Align2, Image, Rect, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use crate::theme::*;

/// Discord's picture, a heading, how to fill the tab, and a small mock-up of the menu row that
/// does. Offsets are from the popout's corner.
pub(super) fn show(ui: &mut Ui, art: &str, head: &str, how: &str, action: &str, tail: &str) {
    let scheduled = art == super::webart::NOTHING_SCHEDULED;
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 447.6), Sense::hover());
    let o = pos2(area.left() - 16.0, area.top() - 118.0);
    let down = if scheduled { 4.5 } else { 0.0 };
    let p = ui.painter();
    super::webart::draw(p, art, Rect::from_min_size(o + vec2(210.0, 170.9 + down), vec2(140.0, 93.3)));
    p.text(o + vec2(280.0, 304.3 + down), Align2::CENTER_CENTER, head, semi(24.0), pal().text_normal);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |font| egui::TextFormat { font_id: font, color: pal().header_secondary, ..Default::default() };
    job.append(how, 0.0, fmt(medium(14.0)));
    job.append(action, 0.0, fmt(semi(14.0)));
    job.append(tail, 0.0, fmt(medium(14.0)));
    job.halign = egui::Align::Center;
    job.wrap.max_width = 520.0;
    p.galley(o + vec2(280.0, 328.3 + down), p.layout_job(job), pal().header_secondary);
    let row = |at: egui::Vec2, icon: Icon, label: &str, on: bool| {
        if on {
            p.rect_filled(Rect::from_min_size(o + at + vec2(5.0, 5.0), vec2(if scheduled { 151.4 } else { 157.0 }, 30.0)), 4, MENU_HOVER);
        }
        icons::draw(p, icon, Rect::from_min_size(o + at + vec2(13.0, 12.0), vec2(16.0, 16.0)), if on { pal().header_primary } else { pal().header_secondary });
        p.text(o + at + vec2(37.0, 20.0), Align2::LEFT_CENTER, label, medium(14.0), pal().text_normal);
    };
    let menu = |at: egui::Vec2, w: f32| p.rect(Rect::from_min_size(o + at, vec2(w, 70.0)), 8, pal().bg_floating, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    if scheduled {
        let bar = Rect::from_min_size(o + vec2(16.0, 465.8), vec2(528.0, 42.0));
        p.rect(bar, 8, pal().bg_primary, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        let plus = Rect::from_min_size(o + vec2(29.0, 474.8), vec2(24.0, 24.0));
        p.rect_filled(plus, 8, pal().border_normal);
        icons::draw(p, Icon::Plus, plus.shrink(4.0), pal().header_primary);
        p.text(o + vec2(61.0, 486.8), Align2::LEFT_CENTER, "Message #general", reg(14.0), pal().text_muted);
        menu(vec2(28.0, 400.8), 161.4);
        row(vec2(28.0, 400.8), Icon::FileUpload, "Upload a File", false);
        row(vec2(28.0, 430.8), Icon::CalendarPlus, "Schedule Message", true);
        return;
    }
    let card = Rect::from_min_size(o + vec2(16.0, 378.3), vec2(528.0, 118.0));
    p.rect(card, 12, pal().bg_primary, egui::Stroke::new(1.0, pal().border_normal), egui::StrokeKind::Inside);
    let face = Rect::from_min_size(o + vec2(33.0, 391.3), vec2(40.0, 40.0));
    Image::from_uri(crate::gif::uri(ui.ctx(), &super::webart::url(super::webart::WUMPUS), true)).corner_radius(20).show_loading_spinner(false).paint_at(ui, face);
    p.text(o + vec2(89.0, 399.8), Align2::LEFT_CENTER, "Wumpus", medium(16.0), pal().header_primary);
    p.text(o + vec2(156.5, 401.3), Align2::LEFT_CENTER, "1:37 AM", medium(12.0), pal().chat_muted);
    for (i, line) in ["Concert tickets go on sale Friday at 10am!", "wait, did you see this", "the setlist for tonight looks unreal"].into_iter().enumerate() {
        p.text(o + vec2(89.0, 421.8 + 26.0 * i as f32), Align2::LEFT_CENTER, line, reg(16.0), pal().text_normal);
    }
    let reminders = art == super::webart::NO_REMINDERS;
    menu(vec2(317.0, 428.3), 167.0);
    row(vec2(317.0, 428.3), Icon::Bookmark, "Bookmark Message", !reminders);
    row(vec2(317.0, 458.3), Icon::Clock, "Create Reminder", reminders);
}
