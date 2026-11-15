//! An app's full profile: the Bio and Data Access tabs discord shows in place of a person's
//! activity.

use egui::{Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::Value;

use super::icons::{self, Icon};
use crate::state::Id;
use crate::theme::*;

const PRESENCE: u64 = 1 << 12 | 1 << 13;
const MEMBERS: u64 = 1 << 14 | 1 << 15;
const MESSAGE_CONTENT: u64 = 1 << 18 | 1 << 19;
const BASELINE: &str = "https://support.discord.com/hc/articles/7933951485975";

/// What an app may read, from its application flags: (title, what it means, icon).
pub fn access(flags: u64) -> Vec<(&'static str, &'static str, Icon)> {
    let mut rows = Vec::new();
    if flags & MESSAGE_CONTENT != 0 {
        rows.push(("Read Messages", "Can access messages in this server and read their contents.", Icon::Bubble));
    }
    if flags & PRESENCE != 0 {
        rows.push(("Presence", "Can see online status of server members and what they are playing, watching, or listening to.", Icon::Controller));
    }
    if flags & MEMBERS != 0 {
        rows.push(("Members", "Can see who's in the server and their roles, nicknames, and permission changes.", Icon::People));
    }
    rows
}

fn head(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(medium(12.0)).color(pal().header_secondary));
}

fn link(ui: &mut Ui, icon: Icon, label: &str, url: &str) {
    let g = ui.painter().layout_no_wrap(label.into(), medium(14.0), pal().text_normal);
    let (r, resp) = ui.allocate_exact_size(vec2(28.0 + g.size().x, 24.0), Sense::click());
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(20.0, 20.0)), pal().text_normal);
    ui.painter().galley(pos2(r.left() + 28.0, r.center().y - g.size().y / 2.0), g, pal().text_normal);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

pub fn bio(ui: &mut Ui, p: Option<&Value>, uid: Id) {
    let text = p.and_then(|p| p["user_profile"]["bio"].as_str().or(p["user"]["bio"].as_str())).unwrap_or("");
    if !text.is_empty() {
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(reg(14.0), pal().text_normal));
        job.wrap.max_width = ui.available_width();
        ui.label(ui.painter().layout_job(job));
        ui.add_space(8.0);
    }
    let app = p.map(|p| &p["application"]);
    let terms = app.and_then(|a| a["terms_of_service_url"].as_str());
    let privacy = app.and_then(|a| a["privacy_policy_url"].as_str());
    if terms.is_some() || privacy.is_some() {
        head(ui, "Links");
        if let Some(u) = terms {
            link(ui, Icon::File, "Terms of Service", u);
        }
        if let Some(u) = privacy {
            link(ui, Icon::Lock, "Privacy Policy", u);
        }
        ui.add_space(8.0);
    }
    head(ui, "Created On");
    let made = chrono::DateTime::from_timestamp_millis(crate::state::snowflake_ms(uid)).map(|d| d.format("%b %-d, %Y").to_string()).unwrap_or_default();
    ui.label(RichText::new(made).font(reg(14.0)).color(pal().text_normal));
}

pub fn data_access(ui: &mut Ui, p: Option<&Value>) {
    let flags = p.and_then(|p| p["application"]["flags"].as_u64()).unwrap_or(0);
    for (title, body, icon) in access(flags) {
        let w = ui.available_width();
        let text = ui.painter().layout(body.into(), reg(14.0), pal().text_normal, w - 32.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 20.0 + text.size().y), Sense::hover());
        crate::ui::settings::mark(ui, &format!("bot_access:{title}"), r);
        icons::draw(ui.painter(), icon, Rect::from_min_size(pos2(r.left(), r.top() + 2.0), vec2(20.0, 20.0)), pal().text_normal);
        ui.painter().text(pos2(r.left() + 32.0, r.top()), egui::Align2::LEFT_TOP, title, semi(12.0), pal().text_normal);
        ui.painter().galley(pos2(r.left() + 32.0, r.top() + 18.0), text, pal().text_normal);
        ui.add_space(8.0);
    }
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    job.append("All apps can access a set of baseline data. Please visit the ", 0.0, egui::TextFormat::simple(reg(12.0), pal().text_normal));
    job.append("Help Center", 0.0, egui::TextFormat::simple(reg(12.0), pal().link));
    job.append(" to learn more.", 0.0, egui::TextFormat::simple(reg(12.0), pal().text_normal));
    let r = ui.add(egui::Label::new(job).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(BASELINE));
    }
}

#[cfg(test)]
mod tests {
    use super::access;

    #[test]
    fn data_access_rows_follow_the_apps_intent_flags() {
        assert!(access(0).is_empty());
        let titles = |f: u64| access(f).iter().map(|r| r.0).collect::<Vec<_>>();
        assert_eq!(titles(1 << 19), ["Read Messages"]);
        assert_eq!(titles(1 << 12 | 1 << 15), ["Presence", "Members"]);
        assert_eq!(titles(1 << 18 | 1 << 13 | 1 << 14), ["Read Messages", "Presence", "Members"]);
    }
}
