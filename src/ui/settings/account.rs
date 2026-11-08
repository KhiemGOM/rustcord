use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::json;

use super::{Look, action, card, link, loading, note, open_web, remote, secondary, section};
use super::modals::{self, Done, Edit, send};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, text};

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Sub {
    #[default]
    None,
    Mfa,
    Devices,
}

fn masked(s: &str) -> String {
    match s.split_once('@') {
        Some((u, d)) => format!("{}@{d}", "*".repeat(u.chars().count())),
        None => format!("{}{}", "*".repeat(s.len().saturating_sub(4)), &s[s.len().saturating_sub(4)..]),
    }
}

/// A settings row: the label on the left, the value and an Edit button on the right.
fn info(ui: &mut Ui, label: &str, value: &str, reveal: Option<&mut bool>, edit: Option<&str>) -> bool {
    super::lead(ui);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, medium(16.0), pal().header_primary);
    let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
    let mut clicked = false;
    if let Some(e) = edit {
        let b = secondary(&mut c, e);
        super::mark(ui, &format!("edit:{label}"), b.rect);
        clicked = b.clicked();
        c.add_space(16.0);
    }
    if let Some(shown) = reveal {
        let l = c.add(egui::Label::new(RichText::new(if *shown { "Hide" } else { "Reveal" }).font(medium(16.0)).color(sat(super::REVEAL))).sense(Sense::click()));
        if l.hovered() {
            c.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if l.clicked() {
            *shown = !*shown;
        }
        c.add_space(4.0);
    }
    text::label(&mut c, value, medium(16.0), pal().text_normal, Sense::hover());
    super::end(ui, 32.0);
    clicked
}

/// A row that leads somewhere: the current value and a chevron.
fn nav(ui: &mut Ui, label: &str, value: &str) -> bool {
    super::lead(ui);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, medium(16.0), pal().header_primary);
    ui.painter().text(pos2(r.right() - 36.0, r.center().y), Align2::RIGHT_CENTER, value, medium(16.0), pal().text_normal);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(24.0, 24.0)), pal().header_primary);
    super::mark(ui, &format!("nav:{label}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    super::end(ui, 32.0);
    resp.clicked()
}

/// A status row: a 48px tile with an icon, a heading and blurb, the value on the right.
fn status(ui: &mut Ui, icon: Icon, tint: Color32, head: &str, desc: &str, value: &str, chevron: bool) -> bool {
    super::lead(ui);
    let w = ui.available_width();
    let side = if chevron { 120.0 } else { 90.0 };
    let g = ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 64.0 - side);
    let h = (24.0 + g.size().y + 4.0).max(48.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), if chevron { Sense::click() } else { Sense::hover() });
    let tile = Rect::from_min_size(r.min, vec2(48.0, 48.0));
    ui.painter().rect_filled(tile, CornerRadius::same(8), Color32::from_black_alpha(31));
    icons::draw(ui.painter(), icon, Rect::from_center_size(tile.center(), vec2(24.0, 24.0)), tint);
    ui.painter().text(pos2(r.left() + 64.0, r.top()), Align2::LEFT_TOP, head, medium(16.0), pal().header_primary);
    ui.painter().galley(pos2(r.left() + 64.0, r.top() + 24.0), g, pal().header_secondary);
    let x = if chevron { r.right() - 36.0 } else { r.right() };
    ui.painter().text(pos2(x, r.top() + 12.0), Align2::RIGHT_CENTER, value, medium(16.0), pal().text_normal);
    if chevron {
        icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 12.0, r.top() + 12.0), vec2(24.0, 24.0)), pal().header_primary);
        super::mark(ui, &format!("nav:{head}"), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    super::end(ui, 24.0);
    chevron && resp.clicked()
}

fn banner(ui: &mut Ui, msg: &str, learn: &str) -> bool {
    let mut dismiss = false;
    Frame::new().fill(Color32::from_rgba_unmultiplied(0, 120, 215, 20)).stroke(Stroke::new(1.0, Color32::from_rgb(0, 120, 215))).corner_radius(8).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 8.0, Color32::from_rgb(0, 120, 215));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "i", semi(12.0), Color32::WHITE);
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 32.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(msg).font(reg(14.0)).color(pal().text_normal));
                    if link(ui, learn).clicked() {
                        let _ = open::that_detached("https://discord.com/blog/updating-discords-age-assurance");
                    }
                });
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                dismiss = crate::ui::icon_button(ui, Icon::Close, 20.0, "Dismiss").clicked();
            });
        });
    });
    super::end(ui, 16.0);
    dismiss
}

pub fn account(app: &mut App, ui: &mut Ui) -> bool {
    match app.sform.sub {
        Sub::Mfa => return mfa(app, ui),
        Sub::Devices => return devices(app, ui),
        Sub::None => {}
    }
    let me = app.store.user(app.store.me).cloned().unwrap_or_default();
    let raw = app.store.me_raw.clone();
    let hide = app.settings.hiding_personal();
    let mut edit = None;

    section(ui, "Account Info");
    if info(ui, "Username", &me.username, None, Some("Edit")) {
        edit = Some(Edit::Username);
    }
    let email = app.store.email.clone().unwrap_or_else(|| "Not added".into());
    let shown = if app.sform.reveal_email && !hide { email } else { masked(&email) };
    let mut rev = app.sform.reveal_email;
    if info(ui, "Email", &shown, (!hide).then_some(&mut rev), Some("Edit")) {
        edit = Some(Edit::Email);
    }
    app.sform.reveal_email = rev;
    let phone = raw["phone"].as_str().map(str::to_string);
    let shown = match &phone {
        Some(p) if app.sform.reveal_phone && !hide => p.clone(),
        Some(p) => masked(p),
        None => "Not added".into(),
    };
    let mut rev = app.sform.reveal_phone;
    if info(ui, "Phone Number", &shown, (phone.is_some() && !hide).then_some(&mut rev), Some(if phone.is_some() { "Edit" } else { "Add" })) {
        edit = Some(Edit::Phone);
    }
    app.sform.reveal_phone = rev;

    section(ui, "Password & Security");
    if info(ui, "Password", "", None, Some("Edit")) {
        edit = Some(Edit::Password);
    }
    if nav(ui, "Multi-Factor Authentication", if raw["mfa_enabled"].as_bool().unwrap_or(false) { "Enabled" } else { "Disabled" }) {
        app.sform.sub = Sub::Mfa;
    }
    let count = match remote(app, "sessions", "/auth/sessions") {
        Some(Ok(v)) => format!("{} devices", v["user_sessions"].as_array().map_or(0, Vec::len)),
        _ => String::new(),
    };
    if nav(ui, "Logged-in Devices", &count) {
        app.sform.sub = Sub::Devices;
    }

    section(ui, "Account Status");
    if !app.sform.age_note_off {
        let teen = raw["nsfw_allowed"].as_bool() == Some(false);
        let msg = if teen {
            "Age groups determine if you can access content and settings meant for adults. You’re in the teen age group, so some content and settings are limited."
        } else {
            "Age groups determine if you can access content and settings meant for adults. You’re in the adult age group (18+), so nothing changes for you."
        };
        if banner(ui, msg, "Learn more on our blog") {
            app.sform.age_note_off = true;
        }
    }
    let teen = raw["nsfw_allowed"].as_bool() == Some(false);
    let (desc, value) = if teen {
        ("You’re in the teen age group (13-17), so age-restricted content, spaces, and settings are not available to you.", "Teen")
    } else {
        ("You’re in the adult age group (18+) and have access to age-restricted content, spaces, and settings.", "Adult")
    };
    status(ui, Icon::UserCircle, pal().header_primary, "Age Group", desc, value, false);
    let state = match remote(app, "safety", "/safety-hub/@me") {
        Some(Ok(v)) => v["account_standing"]["state"].as_u64().unwrap_or(100),
        _ => 100,
    };
    let (label, icon, tint) = standing(state);
    let blurb = "Thank you for upholding Discord's Terms of Service and Community Guidelines. If you have a violation, it will appear here.";
    if status(ui, icon, tint, "Account Standing", blurb, label, true) {
        edit = Some(Edit::Standing);
    }

    section(ui, "Family Center");
    super::lead(ui);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, "Set up Family Center", medium(16.0), pal().header_primary);
    let g = ui.painter().layout(
        "Stay informed about your teen’s experience on Discord. See their activity, manage key safety settings, and start better conversations together.".into(),
        reg(14.0),
        pal().header_secondary,
        r.width() - 64.0,
    );
    ui.painter().galley(r.left_top() + vec2(0.0, 24.0), g, pal().header_secondary);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(24.0, 24.0)), pal().header_primary);
    super::mark(ui, "nav:Set up Family Center", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        open_web("/family-center");
    }
    super::end(ui, 32.0);
    super::divider(ui);

    if action(ui, "Disable your account", "Temporarily disable your account.", "Disable Account", Look::Danger) {
        edit = Some(Edit::Disable);
    }
    if action(ui, "Close your account", "Permanently close your account.", "Delete Account", Look::Filled) {
        edit = Some(Edit::Delete);
    }
    super::activity::hypesquad(app, ui);
    if let Some(k) = edit {
        modals::open(app, k);
    }
    false
}

pub(super) fn standing(state: u64) -> (&'static str, Icon, Color32) {
    match state {
        100 => ("All good", Icon::CircleCheck, pal().green),
        200 => ("Limited", Icon::Flag, pal().yellow),
        300 => ("Very limited", Icon::Flag, Color32::from_rgb(0xf0, 0x80, 0x30)),
        400 => ("At risk", Icon::Flag, pal().red),
        _ => ("Suspended", Icon::Flag, pal().red),
    }
}

/// The page's breadcrumb: "Account" and, inside a sub-page, where you are.
pub fn crumbs(app: &mut App, ui: &mut Ui, head: Rect, title: &str) {
    let sub = match (app.settings_page, app.sform.sub) {
        (Some(super::Page::Account), Sub::Mfa) => "Multi-Factor Authentication",
        (Some(super::Page::Account), Sub::Devices) => "Logged-in Devices",
        _ => "",
    };
    let at = pos2(head.left() + 16.0, head.top() + 24.0);
    if sub.is_empty() {
        ui.painter().text(at, Align2::LEFT_CENTER, title, medium(16.0), pal().text_normal);
        return;
    }
    let first = ui.painter().text(at, Align2::LEFT_CENTER, title, medium(16.0), pal().text_muted);
    let resp = ui.interact(first, egui::Id::new("settings_crumb"), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.sform.sub = Sub::None;
    }
    super::mark(ui, "crumb", first);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(first.right() + 14.0, at.y), vec2(16.0, 16.0)), pal().text_muted);
    ui.painter().text(pos2(first.right() + 28.0, at.y), Align2::LEFT_CENTER, sub, medium(16.0), pal().text_normal);
}

// sub-pages -----------------------------------------------------------------------------

fn mfa(app: &mut App, ui: &mut Ui) -> bool {
    let on = app.store.me_raw["mfa_enabled"].as_bool().unwrap_or(false);
    let mut want = None;
    section(ui, "Multi-Factor Authentication");
    note(ui, "Multi-Factor Authentication adds a second step to logging in, so your account stays yours even if someone learns your password.");
    super::sub(ui, "Authenticator App");
    if on {
        note(ui, "An authenticator app is protecting your account.");
        if action(ui, "Remove Authenticator App", "You'll stop being asked for a code when you log in.", "Remove Authenticator App", Look::Danger) {
            want = Some(Edit::TotpOff);
        }
        super::sub(ui, "Backup Codes");
        if action(ui, "Backup codes", "Get a set of one-time codes for when you can't use your authenticator app.", "View Backup Codes", Look::Secondary) {
            want = Some(Edit::Codes);
        }
    } else {
        note(ui, "Use an app like Google Authenticator or Authy to get a code each time you log in.");
        if action(ui, "Enable Authenticator App", "Protect your account with a second step when you log in.", "Enable Authenticator App", Look::Primary) {
            want = Some(Edit::Totp);
        }
    }
    if on {
        super::sub(ui, "SMS Backup Authentication");
        note(ui, "Receive auth code from SMS");
        if app.store.me_raw["flags"].as_u64().unwrap_or(0) & modals::SMS_FLAG != 0 {
            if action(ui, "SMS Authentication", "You will receive a text message with a verification code.", "Remove SMS Backup", Look::Danger) {
                want = Some(Edit::SmsOff);
            }
        } else if app.store.me_raw["phone"].is_string() {
            if action(ui, "SMS Authentication", "You will receive a text message with a verification code.", "Enable SMS Authentication", Look::Primary) {
                send(app, Done::Sms, "POST", "/users/@me/mfa/sms/enable", None);
            }
        } else {
            note(ui, "Add a phone number to your account to enable SMS authentication.");
        }
    }
    super::sub(ui, "Security Keys and Passkeys");
    note(ui, "Secure your account by adding a Passkey or another supported security key.");
    if let Some(Ok(v)) = remote(app, "keys", "/users/@me/mfa/webauthn/credentials").cloned() {
        let mut rename = None;
        let mut remove = None;
        for k in v.as_array().into_iter().flatten() {
            let id = k["id"].as_str().unwrap_or("").to_string();
            let name = k["name"].as_str().unwrap_or("Security key").to_string();
            let used = k["last_used"].as_str().map(ago).filter(|s| !s.is_empty());
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        text::label(ui, &name, medium(16.0), pal().header_primary, Sense::hover());
                        if let Some(u) = &used {
                            text::label(ui, &format!("Last used {u}"), reg(14.0), pal().header_secondary, Sense::hover());
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::icon_button(ui, Icon::Trash, 24.0, "Remove").clicked() {
                            remove = Some(id.clone());
                        }
                        if crate::ui::icon_button(ui, Icon::Pencil, 24.0, "Rename").clicked() {
                            rename = Some((id.clone(), name.clone()));
                        }
                    });
                });
            });
        }
        if let Some((id, name)) = rename {
            let mut m = modals::new_modal(Edit::Key);
            m.f[0] = name;
            m.key = Some((id, String::new()));
            app.sform.modal = Some(m);
        }
        if let Some(id) = remove {
            send(app, Done::Refresh("keys"), "DELETE", &format!("/users/@me/mfa/webauthn/credentials/{id}"), None);
        }
    }
    if super::primary(ui, "Add a Passkey").clicked() {
        want = Some(Edit::AddKey);
    }
    if let Some(k) = want {
        modals::open(app, k);
    }
    false
}

fn ago(iso: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(iso) else { return String::new() };
    let s = (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds();
    match s {
        ..120 => "just now".into(),
        120..7200 => format!("{} minutes ago", s / 60),
        7200..172_800 => format!("{} hours ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

fn faint_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, 31)
}

fn devices(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Logged-in Devices");
    note(ui, "These are the devices that are currently logged in to your account. Log out of any you don't recognise.");
    let Some(v) = loading(ui, remote(app, "sessions", "/auth/sessions")) else { return false };
    let list = v["user_sessions"].as_array().cloned().unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for s in &list {
        let ci = &s["client_info"];
        let hash = s["id_hash"].as_str().unwrap_or("").to_string();
        super::lead(ui);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        let icon_r = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(40.0, 40.0));
        ui.painter().circle_filled(icon_r.center(), 20.0, faint_fill());
        icons::draw(ui.painter(), Icon::Screen, icon_r.shrink(10.0), pal().header_secondary);
        let os = ci["os"].as_str().unwrap_or("Unknown");
        let client = ci["platform"].as_str().or(ci["client"].as_str()).unwrap_or("");
        ui.painter().text(pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, format!("{} · {}", os.to_uppercase(), client.to_uppercase()), semi(13.0), pal().header_primary);
        let loc = ci["location"].as_str().unwrap_or("");
        let when = s["approx_last_used_time"].as_str().map(ago).unwrap_or_default();
        ui.painter().text(pos2(r.left() + 52.0, r.center().y + 10.0), Align2::LEFT_CENTER, format!("{loc} · {when}"), reg(14.0), pal().header_secondary);
        let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(24.0, 24.0));
        let resp = ui.interact(x, egui::Id::new(("logout_session", hash.as_str())), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x, crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
        super::mark(ui, &format!("logout:{hash}"), x);
        if resp.on_hover_text("Log out").clicked() {
            out.push(hash);
        }
        super::end(ui, 8.0);
    }
    if list.is_empty() {
        note(ui, "No other devices are logged in.");
    }
    super::end(ui, 24.0);
    if action(ui, "Log out of all known devices", "You'll have to log back in on all logged out devices.", "Log Out All Known Devices", Look::Filled) {
        out = list.iter().filter_map(|s| s["id_hash"].as_str().map(str::to_string)).collect();
    }
    if !out.is_empty() {
        send(app, Done::Refresh("sessions"), "POST", "/auth/sessions/logout", Some(json!({ "session_id_hashes": out })));
    }
    false
}
