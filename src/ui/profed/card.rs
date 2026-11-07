//! The profile you're editing, drawn as the card others see: you edit the banner, avatar,
//! name, pronouns, bio and status where they are.

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Shape, TextEdit, Ui, UiBuilder, pos2, vec2};
use serde_json::Value;

use super::draft::Pic;
use super::{Dlg, Editor, Menu, STATUS_PROMPTS, BIO_PROMPTS, faint, round_icon, style};
use crate::state::User;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, avatar, cosmo, status_dot, text};

fn ink() -> Color32 {
    Color32::WHITE.gamma_multiply(0.96)
}

fn dimmer() -> Color32 {
    Color32::WHITE.gamma_multiply(0.76)
}

pub fn paint_data_image(ui: &Ui, uri: &str, r: Rect, cr: CornerRadius) {
    use base64::Engine;
    let b64 = uri.split_once(',').map_or("", |(_, b)| b);
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
        egui::Image::from_bytes(format!("bytes://profed/{}", b64.len() ^ (b64.bytes().take(64).map(usize::from).sum::<usize>() << 20)), bytes).corner_radius(cr).paint_at(ui, r);
    }
}

pub fn paint_avatar(ui: &Ui, av: Rect, d: &super::Draft, current: &str) {
    let cr = CornerRadius::same((av.width() / 2.0) as u8);
    match &d.avatar {
        Pic::New(u) => {
            ui.painter().circle_filled(av.center(), av.width() / 2.0, pal().bg_accent);
            paint_data_image(ui, u, av, cr);
        }
        Pic::Gone => avatar(ui, av, &User { avatar: None, ..crate::state::User::default() }.avatar_url()),
        Pic::Recent(_, url) => avatar(ui, av, url),
        Pic::Keep => avatar(ui, av, if d.avatar_url.is_empty() { current } else { &d.avatar_url }),
    }
    if let Some(dec) = &d.decoration {
        text::image(ui.painter(), &cosmo::deco(&dec.asset, 240), Rect::from_center_size(av.center(), av.size() * 1.2));
    }
}

pub fn change_banner(app: &mut App, ed: &mut Editor) {
    if !super::nitro(app, "Banners") {
        return;
    }
    let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() else { return };
    let Ok(b) = std::fs::read(p) else { return };
    let _ = ed;
    crate::ui::crop::open(app, b, 5.0 / 2.0, |app: &mut App, u: String| {
        if let Some(e) = &mut app.profed {
            e.draft.banner = Pic::New(u);
        }
    });
}

pub fn pick_image(app: &mut App) {
    let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif"]).pick_file() else { return };
    let Ok(b) = std::fs::read(&p) else { return };
    if b.len() > 10 << 20 {
        app.toast("Upload a PNG, JPG, or GIF under 10MB.");
        return;
    }
    crate::ui::crop::open(app, b, 1.0, |app: &mut App, u: String| {
        if let Some(e) = &mut app.profed {
            e.draft.avatar = Pic::New(u);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn field(ui: &mut Ui, r: Rect, id: &str, text: &mut String, hint: &str, font: egui::FontId, limit: usize, multi: bool) -> egui::Response {
    let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::TOP)));
    let hint = RichText::new(hint).font(italic(font.size)).color(dimmer());
    let edit = if multi { TextEdit::multiline(text).desired_rows(1) } else { TextEdit::singleline(text) };
    c.add(edit.id(ui.id().with(id)).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(font).text_color(ink()).hint_text(hint).char_limit(limit).desired_width(r.width()))
}

fn since(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()).unwrap_or_default()
}

pub fn connections(ui: &mut Ui, accounts: &Value, x: f32, y: &mut f32, w: f32, ink: Color32) -> Option<String> {
    let mut open = None;
    for c in accounts.as_array().into_iter().flatten() {
        let (label, col) = crate::ui::settings::apps::provider(c["type"].as_str().unwrap_or(""));
        let r = Rect::from_min_size(pos2(x, *y), vec2(w.min(300.0), 24.0));
        let resp = ui.interact(r, ui.id().with(("conn", c["id"].as_str().unwrap_or(""), c["type"].as_str().unwrap_or(""))), Sense::click());
        crate::ui::settings::apps::badge(ui, Rect::from_min_size(r.min, vec2(24.0, 24.0)), label, col);
        let n = text::paint(ui.painter(), pos2(x + 32.0, r.center().y), Align2::LEFT_CENTER, c["name"].as_str().unwrap_or(""), reg(14.0), ink, w - 60.0);
        icons::draw(ui.painter(), Icon::LinkExternal, Rect::from_center_size(pos2(n.right() + 14.0, r.center().y), vec2(14.0, 14.0)), ink);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            open = Some(format!("https://discord.com/users/{}", c["name"].as_str().unwrap_or("")));
        }
        *y += 40.0;
    }
    open
}

/// Draws the card at `rect.min`, as wide as `rect`; hands back how tall it came out.
pub fn show(app: &mut App, ed: &mut Editor, ui: &mut Ui, rect: Rect, user: &User) -> f32 {
    let me = app.store.me;
    let o = rect.min;
    let w = rect.width();
    let back = ui.painter().add(Shape::Noop);
    let banner = Rect::from_min_size(o, vec2(w, 140.0));
    let cr = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
    let accent = ed.draft.accent.or(ed.draft.theme[0]).map(role_color).or_else(|| crate::ui::palette::avatar(ui.ctx(), &user.avatar_url())).unwrap_or(BLURPLE);
    ui.painter().rect_filled(banner, cr, accent);
    match (&ed.draft.banner, &ed.draft.banner_url) {
        (Pic::New(u), _) => paint_data_image(ui, u, banner, cr),
        (Pic::Keep, Some(url)) => crate::gif::paint_still(ui, url, banner, cr, ui.rect_contains_pointer(banner)),
        _ => {}
    }
    let hover = ui.rect_contains_pointer(banner) || ed.menu == Some(Menu::Banner);
    let pen = Rect::from_min_size(pos2(banner.right() - 40.0, banner.top() + 8.0), vec2(32.0, 32.0));
    if hover {
        let r = round_icon(ui, pen, Icon::Pencil, "Edit banner and effect", true);
        if r.clicked() {
            ed.menu = if ed.menu == Some(Menu::Banner) { None } else { Some(Menu::Banner) };
            ed.anchor = pen;
        }
    }
    crate::ui::settings::mark(ui, "edit_banner", pen);
    let ring = theme_ring(ed);
    let av = Rect::from_min_size(pos2(o.x + 16.0, banner.bottom() - 60.0), vec2(120.0, 120.0));
    ui.painter().circle_filled(av.center(), 69.0, ring);
    paint_avatar(ui, av, &ed.draft, &user.avatar_url());
    status_dot(ui, Rect::from_min_size(av.right_bottom() - vec2(95.0, 95.0), vec2(95.0, 95.0)), app.store.status(me), ring);
    let ar = Rect::from_center_size(av.center(), vec2(32.0, 32.0));
    if ui.rect_contains_pointer(av) {
        ui.painter().circle_filled(av.center(), 60.0, Color32::from_black_alpha(90));
        if round_icon(ui, ar, Icon::Pencil, "Edit avatar and decoration", true).clicked() {
            ed.dlg = Dlg::Avatar;
        }
    }
    crate::ui::settings::mark(ui, "edit_avatar", ar);
    status_bubble(app, ed, ui, Rect::from_min_size(pos2(o.x + 172.0, banner.bottom() - 8.0), vec2((w - 172.0 - 16.0).max(60.0), 0.0)));
    let x = o.x + 32.0;
    let mut y = banner.bottom() + 84.0;
    let name_w = w - 64.0 - 40.0;
    let focused = ui.memory(|m| m.has_focus(ui.id().with("name")));
    let shown = if ed.draft.name.is_empty() { user.display().to_string() } else { ed.draft.name.clone() };
    let nr = Rect::from_min_size(pos2(x, y), vec2(name_w, 30.0));
    if focused {
        field(ui, nr, "name", &mut ed.draft.name, user.display(), semi(24.0), 32, false);
    } else {
        let tr = style::paint_name(ui, pos2(x, y + 15.0), Align2::LEFT_CENTER, &shown, ed.draft.style.as_ref(), 24.0, name_w, true);
        let resp = ui.interact(Rect::from_min_size(nr.min, vec2(tr.width().max(60.0), 30.0)), ui.id().with("name_click"), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if resp.clicked() {
            ui.memory_mut(|m| m.request_focus(ui.id().with("name")));
        }
        let eb = Rect::from_min_size(pos2(tr.right() + 8.0, y - 1.0), vec2(32.0, 32.0));
        if round_icon(ui, eb, Icon::Pencil, "Edit display name style", false).clicked() && super::nitro(app, "Display name styles") {
            ed.dlg = Dlg::Style(style::Edit::from(ed.draft.style.as_ref()));
        }
        crate::ui::settings::mark(ui, "edit_dns", eb);
    }
    y += 42.0;
    let ur = text::paint(ui.painter(), pos2(x, y + 9.0), Align2::LEFT_CENTER, &user.username, reg(14.0), ink(), 160.0);
    ui.painter().circle_filled(pos2(ur.right() + 9.0, ur.center().y), 2.0, ink());
    let pr = Rect::from_min_size(pos2(ur.right() + 18.0, y), vec2(110.0, 18.0));
    field(ui, pr, "pronouns", &mut ed.draft.pronouns, "Add pronouns", reg(14.0), 40, false);
    crate::ui::settings::mark(ui, "profile:pronouns", pr);
    if ed.guild.is_none() {
        let tr = Rect::from_min_size(pos2(pr.right() + 12.0, y - 2.0), vec2(96.0, 22.0));
        tag_button(ui, ed, app, tr);
    }
    y += 32.0;
    let end = crate::ui::profile::badges(ui, Some(&ed.raw), x, y + 10.0, 20.0);
    let _ = end;
    y += 46.0;
    for (i, (t, wd)) in [("Message", 96.0), ("", 32.0), ("", 32.0)].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(x + [0.0, 104.0, 144.0][i], y), vec2(wd, 32.0));
        let resp = ui.interact(r, ui.id().with(("card_btn", i)), Sense::click());
        let fill = if i == 0 { GREEN.gamma_multiply(0.5) } else { Color32::WHITE.gamma_multiply(if resp.hovered() { 0.14 } else { 0.08 }) };
        ui.painter().rect_filled(r, CornerRadius::same(8), fill);
        if i == 0 {
            icons::draw(ui.painter(), Icon::Bubble, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(16.0, 16.0)), Color32::WHITE.gamma_multiply(0.6));
            ui.painter().text(pos2(r.left() + 38.0, r.center().y), Align2::LEFT_CENTER, t, medium(14.0), Color32::WHITE.gamma_multiply(0.6));
        } else {
            icons::draw(ui.painter(), if i == 1 { Icon::Shop } else { Icon::More }, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), Color32::WHITE);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if i == 1 && resp.on_hover_text("Shop").clicked() {
                ed.to_shop = true;
            }
        }
    }
    y += 56.0;
    let head = |ui: &Ui, y: &mut f32, s: &str| {
        ui.painter().text(pos2(x, *y + 8.0), Align2::LEFT_CENTER, s, medium(12.0), Color32::WHITE.gamma_multiply(0.88));
        *y += 20.0;
    };
    head(ui, &mut y, if ed.guild.is_some() { "Server Bio" } else { "Bio" });
    let br = Rect::from_min_size(pos2(x, y), vec2(w - 64.0, 40.0));
    let bio_prompt = BIO_PROMPTS[ed.prompts[0]];
    let before = ed.draft.bio.clone();
    field(ui, br, "bio", &mut ed.draft.bio, bio_prompt, reg(14.0), 190, true);
    if ed.draft.bio.matches('\n').count() > 9 {
        ed.draft.bio = before;
    }
    crate::ui::settings::mark(ui, "profile:bio", br);
    let bg = ui.ctx().fonts_mut(|f| f.layout(ed.draft.bio.clone(), reg(14.0), ink(), br.width()));
    y += bg.size().y.max(20.0) + 28.0;
    head(ui, &mut y, "Member Since");
    ui.painter().text(pos2(x, y + 9.0), Align2::LEFT_CENTER, since(crate::state::snowflake_ms(me)), reg(14.0), ink());
    y += 36.0;
    if let Some(g) = ed.guild {
        let roles = crate::ui::profile::member_roles(&app.store, Some(&ed.raw), g, me);
        if !roles.is_empty() {
            head(ui, &mut y, "Roles Granted");
            y += 8.0;
            let mut cx = x;
            for r in roles.iter().filter_map(|r| app.store.guilds.get(&g).and_then(|g| g.roles.get(r))) {
                let lay = ui.painter().layout_no_wrap(r.name.clone(), reg(12.0), ink());
                let cwid = lay.size().x + 32.0;
                if cx + cwid > o.x + w - 32.0 && cx > x {
                    cx = x;
                    y += 32.0;
                }
                let chip = Rect::from_min_size(pos2(cx, y), vec2(cwid, 24.0));
                ui.painter().rect_filled(chip, CornerRadius::same(6), Color32::WHITE.gamma_multiply(0.12));
                ui.painter().circle_filled(pos2(chip.left() + 13.0, chip.center().y), 6.0, role_color(r.color));
                ui.painter().galley(pos2(chip.left() + 26.0, chip.center().y - lay.size().y / 2.0), lay, ink());
                cx += cwid + 6.0;
            }
            y += 44.0;
        }
    }
    let accounts = ed.raw["connected_accounts"].clone();
    head(ui, &mut y, "Connections");
    y += 8.0;
    if let Some(url) = connections(ui, &accounts, x, &mut y, w - 64.0, ink()) {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    let add = Rect::from_min_size(pos2(x, y), vec2(140.0, 20.0));
    let resp = ui.interact(add, ui.id().with("add_conn"), Sense::click());
    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(x + 9.0, y + 10.0), vec2(16.0, 16.0)), dimmer());
    ui.painter().text(pos2(x + 24.0, y + 10.0), Align2::LEFT_CENTER, "Add Connection", medium(12.0), dimmer());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.settings_page = Some(crate::ui::settings::Page::Apps);
        app.profed = None;
    }
    y += 36.0;
    head(ui, &mut y, "Note (only visible to you)");
    let saved = app.notes.get(&me).cloned().unwrap_or_default();
    let mut nc = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(x, y), vec2(w - 64.0, 24.0))));
    if let Some(n) = crate::ui::profile::note_box(&mut nc, &mut app.rects, "profed_note", me, &saved, w - 64.0, 14.0, false) {
        app.rest("PUT", format!("/users/@me/notes/{me}"), Some(serde_json::json!({ "note": n })), crate::net::Tag::Quiet);
        app.notes.insert(me, n);
    }
    y += 40.0;
    let h = y - o.y;
    let card = Rect::from_min_size(o, vec2(w, h));
    let shape = match super::theme_pair(&ed.draft) {
        Some((a, b)) => super::gradient_shape(ui, card, a, b, 8),
        None => Shape::rect_filled(card, CornerRadius::same(8), pal().bg_secondary),
    };
    ui.painter().set(back, shape);
    let (eff, frame) = (ed.draft.effect.as_ref().map(|e| e.sku.clone()).unwrap_or_default(), ed.draft.frame.as_ref().map(|e| e.sku.clone()).unwrap_or_default());
    cosmo::paint_profile(app, ui, card, &eff, &frame, ed.since);
    ui.allocate_rect(card, Sense::hover());
    h
}

fn theme_ring(ed: &Editor) -> Color32 {
    super::theme_pair(&ed.draft).map_or(pal().bg_secondary, |p| p.0)
}

fn status_bubble(app: &mut App, ed: &mut Editor, ui: &mut Ui, at: Rect) {
    let cur = app.store.custom_status.clone();
    let prompt = STATUS_PROMPTS[ed.prompts[1]];
    let (label, f) = match &cur {
        Some(t) => (t.as_str(), reg(14.0)),
        None => (prompt, italic(14.0)),
    };
    let lead = if cur.is_some() { 12.0 } else { 36.0 };
    let g = ui.painter().layout(label.to_string(), f, ink(), at.width() - lead - 12.0);
    let size = vec2(g.size().x + lead + 12.0, (g.size().y + 20.0).max(36.0));
    let r = Rect::from_min_size(at.min, size);
    let resp = ui.interact(r, ui.id().with("status_bubble"), Sense::click());
    ui.painter().rect(r, CornerRadius::same(16), Color32::from_black_alpha(if resp.hovered() { 150 } else { 120 }), egui::Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
    if cur.is_none() {
        icons::draw(ui.painter(), Icon::CirclePlus, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(16.0, 16.0)), ink());
    }
    ui.painter().galley(pos2(r.left() + lead, r.center().y - g.size().y / 2.0), g, ink());
    crate::ui::settings::mark(ui, "add_status", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.status_text = Some(crate::ui::cstatus::State::from(&app.store));
    }
}

fn tag_button(ui: &mut Ui, ed: &mut Editor, app: &App, r: Rect) {
    let resp = ui.interact(r, ui.id().with("tag_btn"), Sense::click());
    let on = ed.menu == Some(Menu::Tag);
    ui.painter().rect_stroke(r, CornerRadius::same(4), egui::Stroke::new(1.0, faint(if resp.hovered() || on { 120 } else { 61 })), egui::StrokeKind::Inside);
    let label = match ed.draft.tag.and_then(|g| app.guild_profiles.get(&g)).and_then(|p| p["tag"].as_str().map(str::to_string)) {
        Some(t) => t,
        None => "Server Tag".into(),
    };
    ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, label, italic(12.0), dimmer());
    icons::draw(ui.painter(), if on { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(14.0, 14.0)), dimmer());
    crate::ui::settings::mark(ui, "tag_select", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ed.menu = if on { None } else { Some(Menu::Tag) };
        ed.anchor = r;
    }
}
