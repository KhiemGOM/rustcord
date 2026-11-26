use egui::{Align, Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::cards::{card_head, shown, theme, theme_palette, theme_wash};
use super::{App, avatar, profile, text};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::icons::Icon;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelfBadge {
    icon: String,
    description: String,
}

enum AccountAct {
    Status(&'static str),
    StatusFor(&'static str, Option<i64>),
    Edit,
    Custom,
    Badges,
    CopyId,
    Switch(Id),
    Manage,
}

/// The two trailing thought-bubble dots beside the self status prompt / speech bubble:
/// small dot toward the avatar rim, larger dot nestled into the bubble's upper-left shoulder.
fn thought_bubble_dots(bubble: Rect) -> [(Pos2, f32); 2] {
    [(pos2(bubble.left() - 3.0, bubble.top() - 7.0), 3.5), (pos2(bubble.left() + 9.0, bubble.top() + 1.0), 6.0)]
}

fn self_bio(p: Option<&Value>) -> String {
    p.filter(|p| !p.is_null())
        .and_then(|p| shown(p, "bio").as_str().or(p["user"]["bio"].as_str()))
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn self_badges(profile: Option<&Value>, user_flags: u64) -> Vec<SelfBadge> {
    if let Some(list) = profile.filter(|p| !p.is_null()).and_then(|p| p["badges"].as_array())
        && !list.is_empty()
    {
        let out: Vec<SelfBadge> = list
            .iter()
            .filter_map(|b| {
                let icon = b["icon"].as_str().filter(|s| !s.is_empty())?.to_string();
                Some(SelfBadge {
                    icon,
                    description: b["description"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();
        if !out.is_empty() {
            return out;
        }
    }
    const FLAG_BADGES: [(u64, &str, &str); 12] = [
        (1 << 0, "5e74e9b61934fc1f67c65515d1f7e60d", "Discord Staff"),
        (1 << 1, "3f9748e53446a137a052f3454e2de41e", "Partnered Server Owner"),
        (1 << 2, "bf01d1073931f921909045f3a39fd264", "HypeSquad Events"),
        (1 << 3, "2717692c7dca7289b35297368a940dd0", "Discord Bug Hunter"),
        (1 << 6, "8a88d63823d8a71cd5e390baa45efa02", "HypeSquad Bravery"),
        (1 << 7, "011940fd013da3f7fb926e4a1cd2e618", "HypeSquad Brilliance"),
        (1 << 8, "3aa41de486fa12454c3761e8e223442e", "HypeSquad Balance"),
        (1 << 9, "7060786766c9c840eb3019e725d2b358", "Early Supporter"),
        (1 << 14, "848f79194d4be5ff5f81505cbd0ce1e6", "Discord Bug Hunter"),
        (1 << 17, "6df5892e0f35b051f8b61eace34f4967", "Early Verified Bot Developer"),
        (1 << 18, "fee1624003e2fee35cb398e125dc479b", "Moderator Programs Alumni"),
        (1 << 22, "6bdc42827a38498929a4920da12695d9", "Active Developer"),
    ];
    FLAG_BADGES
        .into_iter()
        .filter(|(bit, ..)| user_flags & bit != 0)
        .map(|(_, icon, desc)| SelfBadge { icon: icon.into(), description: desc.into() })
        .collect()
}

fn layout_bio(ctx: &egui::Context, bio: &str, max_w: f32, color: Color32) -> (std::sync::Arc<egui::Galley>, Vec<(String, Rect)>) {
    crate::theme::note_text(bio);
    let font = reg(14.0);
    let mut job = egui::text::LayoutJob::default();
    job.wrap = egui::text::TextWrapping {
        max_width: max_w,
        max_rows: 3,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    let fmt = egui::text::TextFormat { font_id: font.clone(), color, line_height: Some(18.0), valign: Align::Center, ..Default::default() };
    let gap = egui::text::TextFormat { color: Color32::TRANSPARENT, ..fmt.clone() };
    let space_w = ctx.fonts_mut(|f| f.glyph_width(&font, '\u{a0}'));
    let em_size = 18.0;
    let mut emojis = Vec::new();
    let (mut chars, mut plain, mut i) = (0usize, 0usize, 0usize);
    while i < bio.len() {
        let rest = &bio[i..];
        if let Some(n) = crate::markdown::take_emoji(rest) {
            let run = &bio[plain..i];
            if !run.is_empty() {
                job.append(run, 0.0, fmt.clone());
                chars += run.chars().count();
            }
            let lead = em_size - space_w + 1.0;
            job.append("\u{a0}", lead, gap.clone());
            emojis.push((chars, crate::markdown::twemoji(&rest[..n]), em_size, lead));
            chars += 1;
            i += n;
            plain = i;
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    if plain < bio.len() {
        job.append(&bio[plain..], 0.0, fmt);
    }
    let galley = ctx.fonts_mut(|f| f.layout_job(job));
    let visible = text::visible_chars(&galley);
    let placed = emojis
        .into_iter()
        .filter(|(idx, ..)| *idx < visible)
        .map(|(idx, url, size, lead)| {
            let c = galley.pos_from_cursor(egui::text::CCursor::new(idx));
            (url, Rect::from_min_size(pos2((c.min.x - lead).max(0.0), c.center().y - size / 2.0), vec2(size, size)))
        })
        .collect();
    (galley, placed)
}

/// Renders the user's own bio under the username, replacing unicode emoji with inline Twemoji.
fn paint_bio(ui: &mut Ui, bio: &str, max_w: f32) {
    let color = pal().header_primary;
    let (galley, emojis) = layout_bio(ui.ctx(), bio, max_w, color);
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    for (url, er) in emojis {
        text::image(ui.painter(), &url, er.translate(rect.min.to_vec2()));
    }
}

/// What opens from the account panel: your card, then discord's grouped rows.
pub(crate) fn account(app: &mut App, ui: &mut Ui) {
    use crate::state::Status;
    let me = app.store.me;
    let Some(user) = app.store.user(me).cloned() else { return };
    if !app.profiles.contains_key(&me) {
        // a placeholder until it lands, so this asks once
        app.profiles.insert(me, Value::Null);
        app.rest("GET", format!("/users/{me}/profile?with_mutual_guilds=false"), None, Tag::Profile(me, 0));
    }
    let status = app.store.presence.get(&me).copied().unwrap_or(Status::Online);
    let dev = app.settings.dev;
    let custom = app.store.custom_status.clone().or_else(|| app.store.custom_emoji.as_ref().map(|_| String::new()));
    let emoji = app.store.custom_emoji.clone();
    let app_accounts = app.accounts.clone();
    let p = app.profiles.get(&me);
    let bio = self_bio(p);
    let flags = user.flags | app.store.me_raw["flags"].as_u64().unwrap_or(0) | app.store.me_raw["public_flags"].as_u64().unwrap_or(0);
    let badges = self_badges(p, flags);
    let mut act = None;
    ui.set_width(300.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    let _theme = crate::theme::scope(theme_palette(p));
    let wash = ui.painter().add(egui::Shape::Noop);
    let top = card_head(ui, &app.store, 0, &user, p, user.display(), status);
    let hint = ui.painter().layout_no_wrap("Favorite collectible?".into(), italic(14.0), pal().header_primary.gamma_multiply(0.76));
    let border = Stroke::new(1.0, pal().border_subtle);
    let bubble = match &custom {
        // the speech bubble discord floats beside your avatar
        Some(c) => {
            let lead = if emoji.is_some() { 24.0 } else { 0.0 };
            let g = ui.painter().layout(c.clone(), reg(14.0), pal().header_primary, 155.0 - lead);
            let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(179.0, (g.size().y + 22.0).max(40.0)));
            let [(c0, r0), (c1, r1)] = thought_bubble_dots(b);
            ui.painter().circle(c0, r0, pal().bg_floating, border);
            ui.painter().circle(c1, r1, pal().bg_floating, border);
            ui.painter().rect(b, 16, pal().bg_floating, border, egui::StrokeKind::Inside);
            ui.painter().circle_filled(c1, r1 - 1.0, pal().bg_floating);
            if let Some(e) = &emoji {
                egui::Image::from_uri(crate::ui::chat::emoji_url(e)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(b.left() + 22.0, b.center().y), vec2(18.0, 18.0)));
            }
            ui.painter().galley(pos2(b.left() + 12.0 + lead, b.center().y - g.size().y / 2.0), g, pal().header_primary);
            b
        }
        // and with nothing set, the prompt to add one
        None => {
            let b = Rect::from_min_size(pos2(top.left() + 105.0, top.bottom() - 7.0), vec2(hint.size().x + 48.0, 36.0));
            let [(c0, r0), (c1, r1)] = thought_bubble_dots(b);
            ui.painter().circle(c0, r0, pal().bg_floating, border);
            ui.painter().circle(c1, r1, pal().bg_floating, border);
            ui.painter().rect(b, 18, pal().bg_floating, border, egui::StrokeKind::Inside);
            ui.painter().circle_filled(c1, r1 - 1.0, pal().bg_floating);
            crate::ui::icons::draw(ui.painter(), Icon::CirclePlus, Rect::from_center_size(pos2(b.left() + 20.0, b.center().y), vec2(16.0, 16.0)), pal().header_primary.gamma_multiply(0.76));
            ui.painter().galley(pos2(b.left() + 36.0, b.center().y - hint.size().y / 2.0), hint, pal().header_primary);
            b
        }
    };
    let bubble_hit = ui.interact(bubble, ui.id().with("custom_status"), Sense::click());
    if bubble_hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if bubble_hit.clicked() {
        act = Some(AccountAct::Custom);
    }
    if !bio.is_empty() {
        Frame::new().inner_margin(Margin { left: 16, right: 16, top: 8, bottom: 0 }).show(ui, |ui| {
            ui.set_width(268.0);
            paint_bio(ui, &bio, 268.0);
        });
    }
    let label = match status {
        Status::Online => "Online",
        Status::Idle => "Idle",
        Status::Dnd => "Do Not Disturb",
        Status::Offline => "Invisible",
    };
    Frame::new().inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 16 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let card = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            Frame::new().fill(PROFILE_CARD).corner_radius(8).show(ui, |ui| {
                ui.set_width(268.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui);
            });
        };
        card(ui, &mut |ui| {
            if account_row(ui, Some(Icon::Pencil), "Edit Profile", false, false).clicked() {
                act = Some(AccountAct::Edit);
            }
            let r = account_row(ui, None, label, true, true);
            let c = pos2(r.rect.left() + 24.0, r.rect.center().y);
            let base = match theme(p) {
                Some(t) => {
                    let end = r.rect.bottom() + 48.0 + 12.0 + if dev { 96.0 } else { 48.0 } + 16.0;
                    let k = ((c.y - top.top()) / (end - top.top()).max(1.0)).clamp(0.0, 1.0);
                    t.top.lerp_to_gamma(t.bottom, k)
                }
                None => pal().bg_secondary,
            };
            let bg = status_card_bg(base, crate::ui::motion::hover(ui.ctx(), &r, 0.05));
            crate::ui::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), status, bg);
            crate::ui::submenu(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (key, label, sub, st) in [
                    ("online", "Online", None, Status::Online),
                    ("idle", "Idle", None, Status::Idle),
                    ("dnd", "Do Not Disturb", Some("You will not receive desktop notifications"), Status::Dnd),
                    ("invisible", "Invisible", Some("You will appear offline"), Status::Offline),
                ] {
                    let timed = key != "online";
                    let (resp, lead) = crate::ui::menu_row_at(ui, true, label, sub, false, if timed { crate::ui::Trail::Caret } else { crate::ui::Trail::None });
                    let c = lead.center();
                    let k = if resp.has_focus() { 1.0 } else { crate::ui::motion::hover(ui.ctx(), &resp, 0.05) };
                    let bg = status_menu_bg(pal().bg_floating, k);
                    crate::ui::status_dot(ui, Rect::from_min_max(c - vec2(32.1, 32.1), c + vec2(5.4, 5.4)), st, bg);
                    if !timed && resp.clicked() {
                        act = Some(AccountAct::Status(key));
                    }
                    if timed {
                        crate::ui::submenu(ui, &resp, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for (name, secs) in crate::ui::cstatus::STATUS_FOR {
                                if crate::ui::menu_item(ui, None, name, false) {
                                    act = Some(AccountAct::StatusFor(key, secs));
                                }
                            }
                        });
                    }
                    if key == "online" {
                        crate::ui::menu_sep(ui);
                    }
                }
            });
            let br = account_row(ui, Some(Icon::Badge), "Badges", false, true);
            paint_badges_trail(ui, br.rect, &badges);
            if br.clicked() {
                act = Some(AccountAct::Badges);
            }
        });
        let others = app_accounts.clone();
        card(ui, &mut |ui| {
            let r = account_row(ui, Some(Icon::Person), "Switch Accounts", true, false);
            crate::ui::submenu(ui, &r, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for a in &others {
                    let (resp, lead) = crate::ui::menu_row_at(ui, true, &a.name, None, false, crate::ui::Trail::Check(a.id == me));
                    let url = match &a.avatar {
                        Some(h) => format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), a.id),
                        None => format!("{}/embed/avatars/{}.png", crate::api::cdn(), (a.id >> 22) % 6),
                    };
                    avatar(ui, lead.expand(2.0), &url);
                    if resp.clicked() && a.id != me {
                        act = Some(AccountAct::Switch(a.id));
                    }
                }
                crate::ui::menu_sep(ui);
                if crate::ui::menu_item(ui, None, "Manage Accounts", false) {
                    act = Some(AccountAct::Manage);
                }
            });
            if dev && account_row(ui, Some(Icon::Id), "Copy User ID", false, true).clicked() {
                act = Some(AccountAct::CopyId);
            }
        });
    });
    theme_wash(ui, wash, app.profiles.get(&me), ui.min_rect());
    match act {
        Some(AccountAct::Status(s)) => app.set_status_for(s, None),
        Some(AccountAct::StatusFor(s, secs)) => app.set_status_for(s, secs),
        Some(AccountAct::Edit) => app.settings_page = Some(crate::ui::settings::Page::Profiles),
        Some(AccountAct::Custom) => app.status_text = Some(crate::ui::cstatus::State::from(&app.store)),
        Some(AccountAct::Badges) => app.open_full(me, 0, profile::Tab::Activity),
        Some(AccountAct::CopyId) => ui.ctx().copy_text(me.to_string()),
        Some(AccountAct::Switch(id)) => app.switch_account(id),
        Some(AccountAct::Manage) => app.user_dialog = Some(crate::ui::userdlg::Dialog::Accounts),
        None => {}
    }
    if act.is_some() {
        ui.close();
    }
}

fn paint_badges_trail(ui: &mut Ui, row: Rect, badges: &[SelfBadge]) {
    if badges.is_empty() {
        return;
    }
    let mut x = row.right() - 16.0;
    let more = badges.len().saturating_sub(4);
    if more > 0 {
        x = ui.painter().text(pos2(x, row.center().y), Align2::RIGHT_CENTER, format!("+{more}"), medium(12.0), pal().header_secondary).left() - 6.0;
    }
    let shown = badges.len().min(4);
    for (i, b) in badges.iter().take(4).enumerate() {
        let ir = Rect::from_min_size(pos2(x - 18.0 - (shown - 1 - i) as f32 * 22.0, row.center().y - 9.0), vec2(18.0, 18.0));
        egui::Image::from_uri(format!("{}/badge-icons/{}.png", crate::api::cdn(), b.icon)).show_loading_spinner(false).paint_at(ui, ir);
        if !b.description.is_empty() {
            ui.interact(ir, ui.id().with(("self_badge", i)), Sense::hover()).on_hover_text(&b.description);
        }
    }
}

/// A 48 tall row in the account card's groups, with a hairline above all but the first.
pub(super) fn account_row(ui: &mut Ui, icon: Option<Icon>, label: &str, caret: bool, rule: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(268.0, 48.0), Sense::click());
    if rule {
        ui.painter().hline(r.x_range().shrink(12.0), r.top(), Stroke::new(1.0, pal().border_subtle));
    }
    crate::ui::motion::wash(ui, &resp, r.shrink(4.0), CornerRadius::same(4), MENU_HOVER, 0.05);
    if let Some(i) = icon {
        crate::ui::icons::draw(ui.painter(), i, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, medium(14.0), pal().header_primary);
    if caret {
        crate::ui::icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    }
    resp
}

/// Alpha-composite a premultiplied foreground wash over an opaque background color.
fn blend_over(fg: Color32, bg: Color32) -> Color32 {
    let bg = solid(bg);
    let inv = 255 - fg.a() as u32;
    let ch = |f: u8, b: u8| (f as u32 + (b as u32 * inv + 127) / 255).min(255) as u8;
    Color32::from_rgb(ch(fg.r(), bg.r()), ch(fg.g(), bg.g()), ch(fg.b(), bg.b()))
}

fn status_card_bg(base: Color32, hover: f32) -> Color32 {
    let card = blend_over(PROFILE_CARD, base);
    if hover > 0.0 { blend_over(MENU_HOVER.gamma_multiply(hover), card) } else { card }
}

fn status_menu_bg(base: Color32, hover: f32) -> Color32 {
    let base = solid(base);
    if hover > 0.0 { blend_over(MENU_HOVER.gamma_multiply(hover), base) } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{MENU_HOVER, PROFILE_CARD, palette};
    use serde_json::json;

    #[test]
    fn status_cutout_surface_is_always_opaque_across_themes() {
        for i in 0..4 {
            let p = palette(i);
            let idle = status_card_bg(p.bg_secondary, 0.0);
            let hot = status_card_bg(p.bg_secondary, 1.0);
            assert_eq!(idle.a(), 255);
            assert_eq!(hot.a(), 255);
            assert_eq!(idle, blend_over(PROFILE_CARD, p.bg_secondary));
            assert_eq!(hot, blend_over(MENU_HOVER, idle));
            assert_eq!(status_menu_bg(p.bg_floating, 0.0).a(), 255);
            assert_eq!(status_menu_bg(p.bg_floating, 1.0), blend_over(MENU_HOVER, p.bg_floating));
        }
    }

    #[test]
    fn thought_bubble_dots_trail_from_avatar_into_bubble() {
        let bubble = Rect::from_min_size(pos2(105.0, 98.0), vec2(179.0, 36.0));
        let [(c0, r0), (c1, r1)] = thought_bubble_dots(bubble);
        assert!(r0 < r1);
        assert!(c0.x < bubble.left());
        assert!(c0.y < bubble.top());
        assert!(c1.x > bubble.left() && c1.x < bubble.right());
    }

    #[test]
    fn self_bio_prefers_profile_and_trims() {
        assert_eq!(self_bio(None), "");
        assert_eq!(self_bio(Some(&Value::Null)), "");
        assert_eq!(self_bio(Some(&json!({ "user": { "bio": "  🥚  " } }))), "🥚");
        assert_eq!(self_bio(Some(&json!({ "user": { "bio": "old" }, "user_profile": { "bio": "🥚 fresh" } }))), "🥚 fresh");
    }

    #[test]
    fn self_badges_use_profile_then_flags_fallback() {
        let p = json!({
            "badges": [
                { "icon": "abc", "description": "Early Supporter", "link": "https://discord.com" }
            ]
        });
        let from_profile = self_badges(Some(&p), 1 << 6);
        assert_eq!(from_profile.len(), 1);
        assert_eq!(from_profile[0].icon, "abc");

        let from_flags = self_badges(Some(&json!({ "badges": [] })), (1 << 6) | (1 << 22));
        assert_eq!(from_flags.len(), 2);
        assert_eq!(from_flags[0].description, "HypeSquad Bravery");
        assert_eq!(from_flags[1].description, "Active Developer");
        assert!(self_badges(None, 0).is_empty());
    }

    #[test]
    fn bio_elision_skips_emojis_past_three_rows() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::theme::fonts(false));
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();

        let (full_g, full_e) = layout_bio(&ctx, "hello 🥚\nline two 🔥", 268.0, Color32::WHITE);
        assert!(!full_g.elided);
        assert_eq!(full_e.len(), 2);

        let (cut_g, cut_e) = layout_bio(&ctx, "hello 🥚\nline two\nline three\nfourth 🔥 line", 268.0, Color32::WHITE);
        assert!(cut_g.elided);
        assert_eq!(cut_e.len(), 1);
        assert!(cut_e[0].0.contains("1f95a"));

        let (overflow_g, overflow_e) = layout_bio(&ctx, "row1\nrow2\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 🔥", 180.0, Color32::WHITE);
        assert!(overflow_g.elided);
        assert!(overflow_e.is_empty());
    }
}
