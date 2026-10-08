use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{card, danger, divider, h5, input, link, loading, note, open_web, primary, radio, refetch, remote, secondary, title, toggle};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::{App, avatar, text};

#[derive(Default)]
pub struct Forms {
    pub search: String,
    pub modal: Option<Modal>,
    pub profile: Option<Profile>,
    pub profile_base: Option<Profile>,
    pub profile_tab: u8,
    pub nick_guild: Option<Id>,
    pub nick: String,
    pub nick_base: String,
    pub saving: bool,
    pub recording: Option<usize>,
    pub rec_keys: Vec<u32>,
    pub reveal_email: bool,
    pub reveal_phone: bool,
}

impl Forms {
    pub fn recording(&self) -> bool {
        self.recording.is_some()
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct Profile {
    name: String,
    pronouns: String,
    bio: String,
    accent: Option<u32>,
    // a data: uri when a new avatar was picked
    avatar: Option<String>,
    remove_avatar: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Edit {
    DisplayName,
    Username,
    Email,
    Password,
    Disable,
    Delete,
}

pub struct Modal {
    pub kind: Edit,
    a: String,
    b: String,
    c: String,
    err: Option<String>,
    busy: bool,
}

fn open_modal(app: &mut App, kind: Edit) {
    let a = match kind {
        Edit::DisplayName => app.store.user(app.store.me).and_then(|u| u.global_name.clone()).unwrap_or_default(),
        Edit::Username => app.store.user(app.store.me).map(|u| u.username.clone()).unwrap_or_default(),
        Edit::Email => app.store.email.clone().unwrap_or_default(),
        _ => String::new(),
    };
    app.sform.modal = Some(Modal { kind, a, b: String::new(), c: String::new(), err: None, busy: false });
}

fn masked(s: &str) -> String {
    match s.split_once('@') {
        Some((u, d)) => format!("{}@{d}", "*".repeat(u.chars().count())),
        None => format!("{}{}", "*".repeat(s.len().saturating_sub(4)), &s[s.len().saturating_sub(4)..]),
    }
}

fn field(ui: &mut Ui, label: &str, value: &str, reveal: Option<&mut bool>, edit: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 90.0);
            ui.label(RichText::new(label).font(bold(12.0)).color(pal().header_secondary));
            ui.horizontal(|ui| {
                text::label(ui, value, reg(16.0), pal().header_primary, Sense::hover());
                if let Some(r) = reveal {
                    ui.add_space(4.0);
                    if link(ui, if *r { "Hide" } else { "Reveal" }).clicked() {
                        *r = !*r;
                    }
                }
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(e) = edit {
                let r = secondary(ui, e);
                super::mark(ui, &format!("edit:{label}"), r.rect);
                clicked = r.clicked();
            }
        });
    });
    ui.add_space(16.0);
    clicked
}

pub fn account(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "My Account");
    let me = app.store.user(app.store.me).cloned().unwrap_or_default();
    let raw = app.store.me_raw.clone();
    let accent = raw["accent_color"].as_u64().map(|c| role_color(c as u32)).unwrap_or(BLURPLE);
    let hide = app.settings.hiding_personal();
    let mut edit = None;
    let mut to_profile = false;
    Frame::new().fill(pal().bg_tertiary).corner_radius(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (banner, _) = ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
        ui.painter().rect_filled(banner, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, accent);
        let av = Rect::from_min_size(pos2(banner.left() + 16.0, banner.bottom() - 28.0), vec2(80.0, 80.0));
        ui.painter().circle_filled(av.center(), 46.0, pal().bg_tertiary);
        avatar(ui, av, &me.avatar_url());
        ui.horizontal(|ui| {
            ui.add_space(112.0);
            text::label(ui, me.display(), bold(20.0), pal().header_primary, Sense::hover());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(16.0);
                to_profile = primary(ui, "Edit User Profile").clicked();
            });
        });
        ui.add_space(12.0);
        Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16).outer_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if field(ui, "DISPLAY NAME", me.global_name.as_deref().unwrap_or("You haven't set one yet."), None, Some("Edit")) {
                edit = Some(Edit::DisplayName);
            }
            if field(ui, "USERNAME", &me.username, None, Some("Edit")) {
                edit = Some(Edit::Username);
            }
            let email = app.store.email.clone().unwrap_or_else(|| "You haven't added an email yet.".into());
            let shown = if app.sform.reveal_email && !hide { email } else { masked(&email) };
            let mut rev = app.sform.reveal_email;
            if field(ui, "EMAIL", &shown, (!hide).then_some(&mut rev), Some("Edit")) {
                edit = Some(Edit::Email);
            }
            app.sform.reveal_email = rev;
            let phone = raw["phone"].as_str().map(str::to_string);
            let shown = match &phone {
                Some(p) if app.sform.reveal_phone && !hide => p.clone(),
                Some(p) => masked(p),
                None => "You haven't added a phone number yet.".into(),
            };
            let mut rev = app.sform.reveal_phone;
            if field(ui, "PHONE NUMBER", &shown, (phone.is_some() && !hide).then_some(&mut rev), Some(if phone.is_some() { "Edit" } else { "Add" })) {
                // phone numbers need an sms code, which only discord's own flow can do
                open_web("/settings/account");
            }
            app.sform.reveal_phone = rev;
        });
    });
    divider(ui);
    ui.label(RichText::new("Password and Authentication").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(8.0);
    if primary(ui, "Change Password").clicked() {
        edit = Some(Edit::Password);
    }
    ui.add_space(12.0);
    let mfa = raw["mfa_enabled"].as_bool().unwrap_or(false);
    h5(ui, "Authenticator App");
    if mfa {
        ui.label(RichText::new("Multi-Factor Authentication enabled").font(semi(16.0)).color(GREEN));
        note(ui, "Your account is protected by an authenticator app.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if secondary(ui, "View Backup Codes").clicked() {
                open_web("/settings/account");
            }
            if danger(ui, "Remove Authenticator App", false).clicked() {
                open_web("/settings/account");
            }
        });
    } else {
        note(ui, "Protect your Discord account with an extra layer of security. Once configured, you'll be required to enter your password and complete one additional step in order to sign in.");
        ui.add_space(4.0);
        if primary(ui, "Enable Authenticator App").clicked() {
            open_web("/settings/account");
        }
    }
    divider(ui);
    h5(ui, "Account Removal");
    note(ui, "Disabling your account means you can recover it at any time after taking this action.");
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if danger(ui, "Disable Account", true).clicked() {
            edit = Some(Edit::Disable);
        }
        ui.add_space(8.0);
        if danger(ui, "Delete Account", false).clicked() {
            edit = Some(Edit::Delete);
        }
    });
    if let Some(k) = edit {
        open_modal(app, k);
    }
    if to_profile {
        app.settings_page = Some(super::Page::Profiles);
    }
    false
}

pub fn modal(app: &mut App, ctx: &egui::Context) {
    let Some(m) = &mut app.sform.modal else { return };
    let (head, sub) = match m.kind {
        Edit::DisplayName => ("Change your display name", ""),
        Edit::Username => ("Change your username", "Enter a new username and your existing password."),
        Edit::Email => ("Enter an email address", "Enter a new email address and your existing password."),
        Edit::Password => ("Update your password", "Enter your current password and a new password."),
        Edit::Disable => ("Disable Account", "Are you sure that you want to disable your account? This will immediately log you out and make your account inaccessible to anyone."),
        Edit::Delete => ("Delete Account", "Are you sure that you want to delete your account? This will immediately log you out of your account and you will not be able to log in again."),
    };
    let mut submit = false;
    let mut close = false;
    let r = egui::Modal::new("settings_modal".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(8))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            Frame::new().inner_margin(16).show(ui, |ui| {
                ui.set_width(408.0);
                let danger_kind = matches!(m.kind, Edit::Disable | Edit::Delete);
                if danger_kind {
                    ui.label(RichText::new(head).font(bold(20.0)).color(pal().header_primary));
                    ui.add_space(8.0);
                    Frame::new().fill(YELLOW).corner_radius(4).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(sub).font(reg(14.0)).color(Color32::BLACK));
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(head).font(bold(24.0)).color(pal().header_primary));
                        if !sub.is_empty() {
                            ui.label(RichText::new(sub).font(reg(16.0)).color(pal().header_secondary));
                        }
                    });
                }
                ui.add_space(12.0);
                let label = |ui: &mut Ui, s: &str| {
                    ui.label(RichText::new(s).font(bold(12.0)).color(pal().header_secondary));
                };
                match m.kind {
                    Edit::DisplayName => {
                        label(ui, "DISPLAY NAME");
                        let r = input(ui, &mut m.a, "", false);
                        super::mark(ui, "modal_input", r.rect);
                    }
                    Edit::Username | Edit::Email => {
                        label(ui, if m.kind == Edit::Username { "USERNAME" } else { "EMAIL" });
                        input(ui, &mut m.a, "", false);
                        ui.add_space(8.0);
                        label(ui, "CURRENT PASSWORD");
                        input(ui, &mut m.b, "", true);
                    }
                    Edit::Password => {
                        label(ui, "CURRENT PASSWORD");
                        input(ui, &mut m.a, "", true);
                        ui.add_space(8.0);
                        label(ui, "NEW PASSWORD");
                        input(ui, &mut m.b, "", true);
                        ui.add_space(8.0);
                        label(ui, "CONFIRM NEW PASSWORD");
                        input(ui, &mut m.c, "", true);
                    }
                    Edit::Disable | Edit::Delete => {
                        label(ui, "PASSWORD");
                        input(ui, &mut m.a, "", true);
                    }
                }
                if let Some(e) = &m.err {
                    ui.add_space(6.0);
                    ui.label(RichText::new(e).font(reg(14.0)).color(RED));
                }
            });
            Frame::new()
                .fill(pal().bg_secondary)
                .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 })
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_width(408.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = match m.kind {
                            Edit::Disable => "Disable Account",
                            Edit::Delete => "Delete Account",
                            _ => "Done",
                        };
                        let fill = if matches!(m.kind, Edit::Disable | Edit::Delete) { RED } else { BLURPLE };
                        let w = ui.painter().layout_no_wrap(label.into(), semi(14.0), Color32::WHITE).size().x + 32.0;
                        let r = crate::ui::button(ui, if m.busy { "…" } else { label }, fill, w);
                        super::mark(ui, "modal_done", r.rect);
                        if r.clicked() && !m.busy {
                            submit = true;
                        }
                        ui.add_space(8.0);
                        if ui.add(egui::Label::new(RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                            close = true;
                        }
                    });
                });
        });
    if close || r.should_close() {
        app.sform.modal = None;
        return;
    }
    if !submit {
        return;
    }
    let (method, path, body) = match m.kind {
        Edit::DisplayName => ("PATCH", "/users/@me", json!({ "global_name": m.a.trim() })),
        Edit::Username => ("PATCH", "/users/@me", json!({ "username": m.a.trim(), "password": m.b })),
        Edit::Email => ("PATCH", "/users/@me", json!({ "email": m.a.trim(), "password": m.b })),
        Edit::Password => {
            if m.b != m.c {
                m.err = Some("Passwords do not match!".into());
                return;
            }
            ("PATCH", "/users/@me", json!({ "password": m.a, "new_password": m.b }))
        }
        Edit::Disable => ("POST", "/users/@me/disable", json!({ "password": m.a })),
        Edit::Delete => ("POST", "/users/@me/delete", json!({ "password": m.a })),
    };
    m.busy = true;
    m.err = None;
    app.rest(method, path.into(), Some(body), Tag::Remote("modal"));
}

pub fn modal_done(app: &mut App, res: Result<Value, String>) {
    let Some(m) = &mut app.sform.modal else { return };
    m.busy = false;
    match res {
        Err(e) => m.err = Some(e),
        Ok(v) => {
            let kind = m.kind;
            app.sform.modal = None;
            match kind {
                Edit::Disable | Edit::Delete => {
                    app.settings_page = None;
                    app.logout();
                }
                _ => {
                    if v["id"].is_string() {
                        app.store.apply("USER_UPDATE", &v);
                        if let Some(e) = v["email"].as_str() {
                            app.store.email = Some(e.to_string());
                        }
                    }
                    // a password change hands back a fresh token; the old one stops working
                    if let Some(t) = v["token"].as_str() {
                        crate::cache::set_token(Some(t));
                        app.net.send(crate::net::Cmd::Start(t.to_string()));
                        app.token = Some(t.to_string());
                    }
                    app.toast("Saved.");
                }
            }
        }
    }
}

fn profile_from(v: &Value, me: &crate::state::User) -> Profile {
    Profile {
        name: me.global_name.clone().unwrap_or_default(),
        pronouns: v["user_profile"]["pronouns"].as_str().unwrap_or("").to_string(),
        bio: v["user_profile"]["bio"].as_str().or(v["user"]["bio"].as_str()).unwrap_or("").to_string(),
        accent: v["user_profile"]["accent_color"].as_u64().or(v["user"]["accent_color"].as_u64()).map(|c| c as u32),
        avatar: None,
        remove_avatar: false,
    }
}

pub fn profiles(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Profiles");
    ui.horizontal(|ui| {
        for (i, t) in ["User Profile", "Per-server Profiles"].iter().enumerate() {
            let sel = app.sform.profile_tab == i as u8;
            let g = ui.painter().layout_no_wrap(t.to_string(), semi(16.0), pal().header_primary);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x, 32.0), Sense::click());
            let col = if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary };
            ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, col);
            if sel {
                ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(2.0, BLURPLE));
            }
            if resp.clicked() {
                app.sform.profile_tab = i as u8;
            }
            ui.add_space(24.0);
        }
    });
    divider(ui);
    if app.sform.profile_tab == 1 {
        return server_profile(app, ui);
    }
    let me = app.store.me;
    let user = app.store.user(me).cloned().unwrap_or_default();
    let path = format!("/users/{me}/profile?with_mutual_guilds=false");
    let loaded = loading(ui, remote(app, "me_profile", &path));
    let Some(v) = loaded else { return false };
    if app.sform.profile_base.is_none() {
        let p = profile_from(&v, &user);
        app.sform.profile_base = Some(p.clone());
        app.sform.profile = Some(p);
    }
    let Some(draft) = &mut app.sform.profile else { return false };
    let mut pick = false;
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.spacing_mut().item_spacing.y = 6.0;
        h5(ui, "Display Name");
        input(ui, &mut draft.name, &user.username, false);
        ui.add_space(12.0);
        h5(ui, "Pronouns");
        let r = input(ui, &mut draft.pronouns, "Add your pronouns", false);
        super::mark(ui, "profile:pronouns", r.rect);
        ui.add_space(12.0);
        h5(ui, "Avatar");
        ui.horizontal(|ui| {
            pick = primary(ui, "Change Avatar").clicked();
            ui.add_space(8.0);
            if link(ui, "Remove Avatar").clicked() {
                draft.avatar = None;
                draft.remove_avatar = true;
            }
        });
        ui.add_space(12.0);
        h5(ui, "Banner Color");
        let c = draft.accent.unwrap_or(0x5865f2);
        let mut rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            draft.accent = Some(((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32);
        }
        ui.add_space(12.0);
        h5(ui, "About Me");
        note(ui, "You can use markdown and links if you'd like.");
        input_frame().corner_radius(3).inner_margin(10).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut draft.bio)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .char_limit(190)
                    .desired_rows(5)
                    .font(reg(15.0))
                    .text_color(pal().text_normal)
                    .desired_width(ui.available_width()),
            );
        });
        ui.label(RichText::new(format!("{}", 190 - draft.bio.chars().count())).font(reg(12.0)).color(pal().text_muted));

        let ui = &mut cols[1];
        h5(ui, "Preview");
        preview(ui, &user, draft);
    });
    if pick && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
        if let Ok(bytes) = std::fs::read(&path) {
            use base64::Engine;
            let mime = match bytes.get(..4) {
                Some([0x89, b'P', ..]) => "image/png",
                Some([b'G', b'I', ..]) => "image/gif",
                Some([b'R', b'I', ..]) => "image/webp",
                _ => "image/jpeg",
            };
            if let Some(d) = &mut app.sform.profile {
                d.avatar = Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
                d.remove_avatar = false;
            }
        }
    }
    false
}

fn preview(ui: &mut Ui, user: &crate::state::User, d: &Profile) {
    Frame::new().fill(pal().bg_tertiary).corner_radius(8).show(ui, |ui| {
        ui.set_width(300.0);
        let (banner, _) = ui.allocate_exact_size(vec2(300.0, 105.0), Sense::hover());
        ui.painter().rect_filled(banner, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, role_color(d.accent.unwrap_or(0x5865f2)));
        let av = Rect::from_min_size(pos2(banner.left() + 16.0, banner.bottom() - 40.0), vec2(80.0, 80.0));
        ui.painter().circle_filled(av.center(), 46.0, pal().bg_tertiary);
        match (&d.avatar, d.remove_avatar) {
            (Some(uri), _) => {
                use base64::Engine;
                let b64 = uri.split_once(',').map(|(_, b)| b).unwrap_or("");
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    egui::Image::from_bytes(format!("bytes://avatar/{}", b64.len()), bytes)
                        .corner_radius(CornerRadius::same(40))
                        .paint_at(ui, av);
                }
            }
            (None, true) => {
                let none = crate::state::User { avatar: None, ..user.clone() };
                avatar(ui, av, &none.avatar_url());
            }
            (None, false) => avatar(ui, av, &user.avatar_url()),
        }
        ui.add_space(48.0);
        Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let name = if d.name.trim().is_empty() { user.username.clone() } else { d.name.clone() };
            text::label(ui, &name, bold(20.0), pal().header_primary, Sense::hover());
            ui.label(RichText::new(&user.username).font(reg(14.0)).color(pal().header_primary));
            if !d.pronouns.is_empty() {
                ui.label(RichText::new(&d.pronouns).font(reg(14.0)).color(pal().header_secondary));
            }
            if !d.bio.is_empty() {
                ui.add_space(8.0);
                Frame::new().fill(pal().bg_secondary_alt).corner_radius(8).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("ABOUT ME").font(bold(12.0)).color(pal().header_primary));
                    ui.label(RichText::new(&d.bio).font(reg(14.0)).color(pal().text_normal));
                });
            }
        });
    });
}

fn server_profile(app: &mut App, ui: &mut Ui) -> bool {
    note(ui, "Show who you are with different profiles for each of your servers.");
    ui.add_space(8.0);
    h5(ui, "Choose a server");
    let guilds: Vec<(Id, String)> = app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g).map(|x| (*g, x.name.clone()))).collect();
    if app.sform.nick_guild.is_none() {
        app.sform.nick_guild = app.ui.guild.or(guilds.first().map(|g| g.0));
    }
    let cur = app.sform.nick_guild;
    let shown = guilds.iter().find(|g| Some(g.0) == cur).map(|g| g.1.clone()).unwrap_or_default();
    let mut pick = cur;
    egui::ComboBox::from_id_salt("nick_guild").width(320.0).selected_text(shown).show_ui(ui, |ui| {
        for (g, n) in &guilds {
            ui.selectable_value(&mut pick, Some(*g), n);
        }
    });
    let me = app.store.me;
    if pick != cur || (app.sform.nick_base.is_empty() && app.sform.nick.is_empty()) {
        app.sform.nick_guild = pick;
        let n = pick.and_then(|g| app.store.members.get(&g)).and_then(|m| m.get(&me)).and_then(|m| m.nick.clone()).unwrap_or_default();
        app.sform.nick_base = n.clone();
        app.sform.nick = n;
    }
    ui.add_space(12.0);
    h5(ui, "Server Nickname");
    let hint = app.store.user(me).map(|u| u.display().to_string()).unwrap_or_default();
    input(ui, &mut app.sform.nick, &hint, false);
    false
}

pub fn save_bar(app: &mut App, ui: &mut Ui) {
    if app.settings_page != Some(super::Page::Profiles) {
        return;
    }
    let dirty_profile = app.sform.profile_tab == 0 && app.sform.profile.is_some() && app.sform.profile != app.sform.profile_base;
    let dirty_nick = app.sform.profile_tab == 1 && app.sform.nick != app.sform.nick_base;
    if !dirty_profile && !dirty_nick {
        return;
    }
    let area = ui.max_rect();
    let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 40.0), vec2(area.width().min(700.0) - 40.0, 52.0));
    ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
    ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful — you have unsaved changes!", semi(16.0), pal().header_primary);
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 7.0))).layout(Layout::right_to_left(Align::Center)));
    let save_r = crate::ui::button(&mut b, if app.sform.saving { "…" } else { "Save Changes" }, GREEN, 120.0);
    super::mark(ui, "save_changes", save_r.rect);
    let save = save_r.clicked();
    b.add_space(12.0);
    let reset = b.add(egui::Label::new(RichText::new("Reset").font(semi(14.0)).color(pal().header_primary)).sense(Sense::click())).clicked();
    if reset {
        app.sform.profile = app.sform.profile_base.clone();
        app.sform.nick = app.sform.nick_base.clone();
    }
    if save && !app.sform.saving {
        app.sform.saving = true;
        if dirty_nick {
            let g = app.sform.nick_guild.unwrap_or(0);
            app.rest("PATCH", format!("/guilds/{g}/members/@me"), Some(json!({ "nick": app.sform.nick.trim() })), Tag::Remote("nick_save"));
        } else if let Some(d) = app.sform.profile.clone() {
            app.rest(
                "PATCH",
                "/users/@me/profile".into(),
                Some(json!({ "bio": d.bio, "pronouns": d.pronouns, "accent_color": d.accent })),
                Tag::Remote("action"),
            );
            let mut body = json!({ "global_name": if d.name.trim().is_empty() { Value::Null } else { json!(d.name.trim()) } });
            if let Some(a) = &d.avatar {
                body["avatar"] = json!(a);
            } else if d.remove_avatar {
                body["avatar"] = Value::Null;
            }
            app.rest("PATCH", "/users/@me".into(), Some(body), Tag::Remote("profile_save"));
        }
    }
}

pub fn profile_saved(app: &mut App, key: &str, res: Result<Value, String>) {
    app.sform.saving = false;
    match res {
        Err(e) => app.toast(e),
        Ok(v) => {
            if key == "nick_save" {
                app.sform.nick_base = app.sform.nick.clone();
                let (g, me) = (app.sform.nick_guild.unwrap_or(0), app.store.me);
                let nick = v["nick"].as_str().map(str::to_string);
                if let Some(m) = app.store.members.get_mut(&g).and_then(|m| m.get_mut(&me)) {
                    m.nick = nick;
                }
            } else {
                if v["id"].is_string() {
                    app.store.apply("USER_UPDATE", &v);
                }
                if let Some(d) = &mut app.sform.profile {
                    d.avatar = None;
                    d.remove_avatar = false;
                }
                app.sform.profile_base = app.sform.profile.clone();
                refetch(app, "me_profile");
                app.profiles.remove(&app.store.me);
            }
            app.toast("Your changes have been saved.");
        }
    }
}

/// The classic JSON settings object. Its fields have names, so writing one can
/// never land on the wrong setting; if discord ever retires it, writes just fail.
fn legacy(app: &mut App, ui: &mut Ui) -> Option<Value> {
    let v = remote(app, "legacy", "/users/@me/settings").cloned();
    match v {
        Some(Ok(v)) => Some(v),
        None => {
            ui.add(egui::Spinner::new().color(pal().text_muted));
            None
        }
        Some(Err(_)) => {
            card(ui, |ui| {
                note(ui, "Discord didn't hand these settings to Fastcord. You can still change them in the browser.");
                ui.add_space(6.0);
                if primary(ui, "Open on discord.com").clicked() {
                    open_web("/settings");
                }
            });
            None
        }
    }
}

fn set_legacy(app: &mut App, patch: Value) {
    if let Some(Some(Ok(cur))) = app.remote.get_mut("legacy")
        && let (Some(c), Some(p)) = (cur.as_object_mut(), patch.as_object())
    {
        for (k, v) in p {
            c.insert(k.clone(), v.clone());
        }
    }
    app.rest("PATCH", "/users/@me/settings".into(), Some(patch), Tag::Remote("action"));
}

pub fn content(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Content & Social");
    let Some(s) = legacy(app, ui) else { return false };
    h5(ui, "Sensitive media");
    note(ui, "Automatically scan and delete direct messages you receive that contain explicit media content.");
    ui.add_space(4.0);
    let mut filter = s["explicit_content_filter"].as_u64().unwrap_or(1);
    let opts = [
        (2u64, "Keep me safe", "Scan direct messages from everyone."),
        (1, "My friends are nice", "Scan direct messages from everyone unless they are a friend."),
        (0, "Do not scan", "Direct messages will not be scanned for explicit content."),
    ];
    if radio(ui, &mut filter, &opts) {
        set_legacy(app, json!({ "explicit_content_filter": filter }));
    }
    divider(ui);
    h5(ui, "Social permissions");
    let mut dms = !s["default_guilds_restricted"].as_bool().unwrap_or(false);
    if toggle(ui, &mut dms, "Allow DMs from other members in servers you join", "This setting is applied when you join a new server. It does not apply retroactively to your existing servers.") {
        set_legacy(app, json!({ "default_guilds_restricted": !dms }));
    }
    h5(ui, "Who can send you a friend request");
    let f = &s["friend_source_flags"];
    let mut all = f["all"].as_bool().unwrap_or(true);
    let mut fof = f["mutual_friends"].as_bool().unwrap_or(true) || all;
    let mut guilds = f["mutual_guilds"].as_bool().unwrap_or(true) || all;
    let mut changed = toggle(ui, &mut all, "Everyone", "");
    changed |= toggle(ui, &mut fof, "Friends of Friends", "");
    changed |= toggle(ui, &mut guilds, "Server Members", "");
    if changed {
        set_legacy(app, json!({ "friend_source_flags": { "all": all, "mutual_friends": fof, "mutual_guilds": guilds } }));
    }
    false
}

pub fn privacy(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Data & Privacy");
    h5(ui, "How we use your data");
    if let Some(c) = loading(ui, remote(app, "consent", "/users/@me/consent")) {
        let mut usage = c["usage_statistics"]["consented"].as_bool().unwrap_or(false);
        let mut pers = c["personalization"]["consented"].as_bool().unwrap_or(false);
        let mut change = None;
        if toggle(ui, &mut usage, "Use data to improve Discord", "This setting allows us to use and process information about how you navigate and use Discord for analytical purposes.") {
            change = Some(("usage_statistics", usage));
        }
        if toggle(ui, &mut pers, "Use data to personalize my Discord experience", "This setting allows us to use information, such as who you talk to and what games you play, to personalize Discord for you.") {
            change = Some(("personalization", pers));
        }
        if let Some((k, on)) = change {
            if let Some(Some(Ok(cur))) = app.remote.get_mut("consent") {
                cur[k]["consented"] = json!(on);
            }
            let body = if on { json!({ "grant": [k], "revoke": [] }) } else { json!({ "grant": [], "revoke": [k] }) };
            app.rest("POST", "/users/@me/consent".into(), Some(body), Tag::Remote("action"));
        }
    }
    if let Some(s) = legacy(app, ui) {
        let mut sr = s["allow_accessibility_detection"].as_bool().unwrap_or(false);
        if toggle(ui, &mut sr, "Allow Discord to track screen reader usage", "Lets Discord know whether you use a screen reader, to improve accessibility features.") {
            set_legacy(app, json!({ "allow_accessibility_detection": sr }));
        }
    }
    h5(ui, "Request all of my data");
    note(ui, "Get a copy of the information Discord holds about you. It can take up to 30 days to arrive in your email.");
    ui.add_space(6.0);
    if primary(ui, "Request Data").clicked() {
        app.rest("POST", "/users/@me/harvest".into(), Some(json!({})), Tag::Remote("action"));
        app.toast("Data request sent. Discord will email you when it's ready.");
    }
    divider(ui);
    ui.horizontal(|ui| {
        if link(ui, "Privacy Policy").clicked() {
            let _ = open::that_detached("https://discord.com/privacy");
        }
        ui.add_space(12.0);
        if link(ui, "Terms of Service").clicked() {
            let _ = open::that_detached("https://discord.com/terms");
        }
    });
    false
}

pub fn family(ui: &mut Ui) -> bool {
    title(ui, "Family Center");
    card(ui, |ui| {
        ui.label(RichText::new("Stay connected with your teen").font(semi(16.0)).color(pal().header_primary));
        note(ui, "Family Center lets parents and guardians see who their teen is talking to on Discord, without seeing the content of their messages. Connecting a family is done with a QR code from the Discord mobile app.");
        ui.add_space(8.0);
        if primary(ui, "Learn More").clicked() {
            let _ = open::that_detached("https://discord.com/safety-family-center");
        }
    });
    false
}

pub fn apps(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Authorized Apps");
    let Some(v) = loading(ui, remote(app, "oauth", "/oauth2/tokens")) else { return false };
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        note(ui, "You have no authorized apps.");
    }
    let mut revoke = None;
    for t in &list {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let a = &t["application"];
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                match a["icon"].as_str() {
                    Some(h) => avatar(ui, r, &format!("{}/app-icons/{}/{h}.png?size=80", crate::api::cdn(), sid(&a["id"]))),
                    None => {
                        ui.painter().circle_filled(r.center(), 20.0, BLURPLE);
                    }
                }
                ui.vertical(|ui| {
                    text::label(ui, a["name"].as_str().unwrap_or("Unknown app"), semi(16.0), pal().header_primary, Sense::hover());
                    if let Some(d) = a["description"].as_str().filter(|d| !d.is_empty()) {
                        note(ui, d);
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if secondary(ui, "Deauthorize").clicked() {
                        revoke = Some(t["id"].as_str().unwrap_or("").to_string());
                    }
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("ABOUT THIS APP").font(bold(12.0)).color(pal().header_secondary));
            for s in t["scopes"].as_array().into_iter().flatten() {
                ui.label(RichText::new(format!("✓ {}", s.as_str().unwrap_or(""))).font(reg(14.0)).color(pal().text_normal));
            }
        });
        ui.add_space(8.0);
    }
    if let Some(id) = revoke {
        app.rest("DELETE", format!("/oauth2/tokens/{id}"), None, Tag::Remote("action"));
        refetch(app, "oauth");
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

pub fn devices(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Devices");
    note(ui, "Here are all the devices that are currently logged in with your Discord account. You can log out of each one individually or all other devices.");
    note(ui, "If you see an entry you don't recognize, log out of that device and change your Discord account password immediately.");
    ui.add_space(8.0);
    let Some(v) = loading(ui, remote(app, "sessions", "/auth/sessions")) else { return false };
    let list = v["user_sessions"].as_array().cloned().unwrap_or_default();
    h5(ui, "Other devices");
    let mut out: Vec<String> = Vec::new();
    for s in &list {
        let ci = &s["client_info"];
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        let icon_r = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(40.0, 40.0));
        ui.painter().circle_filled(icon_r.center(), 20.0, pal().bg_secondary);
        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Gear, icon_r.shrink(10.0), pal().header_secondary);
        let os = ci["os"].as_str().unwrap_or("Unknown");
        let client = ci["platform"].as_str().or(ci["client"].as_str()).unwrap_or("");
        ui.painter().text(pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, format!("{} · {}", os.to_uppercase(), client.to_uppercase()), bold(13.0), pal().header_primary);
        let loc = ci["location"].as_str().unwrap_or("");
        let when = s["approx_last_used_time"].as_str().map(ago).unwrap_or_default();
        ui.painter().text(pos2(r.left() + 52.0, r.center().y + 10.0), Align2::LEFT_CENTER, format!("{loc} · {when}"), reg(14.0), pal().header_secondary);
        let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(24.0, 24.0));
        let resp = ui.interact(x, egui::Id::new(("logout_session", s["id_hash"].as_str().unwrap_or(""))), Sense::click());
        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Close, x, if resp.hovered() { pal().header_primary } else { pal().header_secondary });
        if resp.on_hover_text("Log out").clicked() {
            out.push(s["id_hash"].as_str().unwrap_or("").to_string());
        }
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, pal().bg_mod_hover));
    }
    if list.is_empty() {
        note(ui, "No other devices are logged in.");
    }
    divider(ui);
    h5(ui, "Log out of all known devices");
    note(ui, "You'll have to log back in on all logged out devices.");
    ui.add_space(6.0);
    if danger(ui, "Log Out All Known Devices", true).clicked() {
        out = list.iter().filter_map(|s| s["id_hash"].as_str().map(str::to_string)).collect();
    }
    if !out.is_empty() {
        app.rest("POST", "/auth/sessions/logout".into(), Some(json!({ "session_id_hashes": out })), Tag::Remote("action"));
        refetch(app, "sessions");
    }
    false
}

pub fn connections(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Connections");
    if app.settings.hiding_personal() {
        note(ui, "Hidden while Streamer Mode is on.");
        return false;
    }
    card(ui, |ui| {
        ui.label(RichText::new("ADD ACCOUNTS TO YOUR PROFILE").font(bold(12.0)).color(pal().header_secondary));
        note(ui, "Connecting accounts happens in your browser, since each service has its own sign in.");
        ui.add_space(6.0);
        if primary(ui, "Add a Connection").clicked() {
            open_web("/settings/connections");
        }
    });
    ui.add_space(12.0);
    let Some(v) = loading(ui, remote(app, "connections", "/users/@me/connections")) else { return false };
    let list = v.as_array().cloned().unwrap_or_default();
    let mut patch: Option<(String, Value)> = None;
    let mut remove = None;
    for c in &list {
        let kind = c["type"].as_str().unwrap_or("").to_string();
        let id = c["id"].as_str().unwrap_or("").to_string();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let mut kind_name = kind.clone();
                if let Some(f) = kind_name.get_mut(..1) {
                    f.make_ascii_uppercase();
                }
                ui.vertical(|ui| {
                    text::label(ui, c["name"].as_str().unwrap_or(""), semi(16.0), pal().header_primary, Sense::hover());
                    ui.label(RichText::new(kind_name).font(reg(12.0)).color(pal().header_secondary));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let x = crate::ui::icon_button(ui, crate::ui::icons::Icon::Close, 24.0, "Remove");
                    if x.clicked() {
                        remove = Some(format!("{kind}/{id}"));
                    }
                });
            });
            ui.add_space(8.0);
            let mut vis = c["visibility"].as_u64() == Some(1);
            if toggle(ui, &mut vis, "Display on profile", "") {
                patch = Some((format!("{kind}/{id}"), json!({ "visibility": vis as u8 })));
            }
            let mut act = c["show_activity"].as_bool().unwrap_or(true);
            if toggle(ui, &mut act, "Display activity", "") {
                patch = Some((format!("{kind}/{id}"), json!({ "show_activity": act })));
            }
        });
        ui.add_space(8.0);
    }
    if let Some((p, body)) = patch {
        app.rest("PATCH", format!("/users/@me/connections/{p}"), Some(body), Tag::Remote("action"));
        refetch(app, "connections");
    }
    if let Some(p) = remove {
        app.rest("DELETE", format!("/users/@me/connections/{p}"), None, Tag::Remote("action"));
        refetch(app, "connections");
    }
    false
}

pub fn nitro(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Nitro");
    let tier = match app.store.me_raw["premium_type"].as_u64().unwrap_or(0) {
        1 => Some("Nitro Classic"),
        2 => Some("Nitro"),
        3 => Some("Nitro Basic"),
        _ => None,
    };
    Frame::new().fill(Color32::from_rgb(0x8e, 0x4d, 0xd8)).corner_radius(8).inner_margin(24).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match tier {
            Some(t) => {
                ui.label(RichText::new(format!("You have {t}")).font(bold(24.0)).color(Color32::WHITE));
                ui.label(RichText::new("Thanks for supporting Discord.").font(reg(16.0)).color(Color32::WHITE));
            }
            None => {
                ui.label(RichText::new("Unleash more fun with Nitro").font(bold(24.0)).color(Color32::WHITE));
                ui.label(RichText::new("Bigger uploads, HD streaming, custom emoji anywhere and more.").font(reg(16.0)).color(Color32::WHITE));
            }
        }
        ui.add_space(8.0);
        if crate::ui::button(ui, if tier.is_some() { "Manage" } else { "Subscribe" }, Color32::WHITE.gamma_multiply(0.25), 120.0).clicked() {
            open_web("/settings/premium");
        }
    });
    false
}

fn read_only_list(app: &mut App, ui: &mut Ui, key: &'static str, path: &str, empty: &str, row: impl Fn(&Value) -> (String, String)) {
    let Some(v) = loading(ui, remote(app, key, path)) else { return };
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        note(ui, empty);
    }
    for x in &list {
        let (a, b) = row(x);
        card(ui, |ui| {
            ui.label(RichText::new(a).font(semi(16.0)).color(pal().header_primary));
            if !b.is_empty() {
                note(ui, &b);
            }
        });
        ui.add_space(8.0);
    }
    ui.add_space(8.0);
    if link(ui, "Manage on discord.com").clicked() {
        open_web("/settings/billing");
    }
}

pub fn subscriptions(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Subscriptions");
    read_only_list(app, ui, "subs", "/users/@me/billing/subscriptions", "You don't have any subscriptions.", |s| {
        let ends = s["current_period_end"].as_str().unwrap_or("").get(..10).unwrap_or("").to_string();
        let status = match s["status"].as_u64() {
            Some(1) => "Active",
            Some(2) => "Past due",
            Some(3) => "Canceled",
            _ => "Pending",
        };
        (format!("Subscription {}", s["id"].as_str().unwrap_or("")), format!("{status} · renews {ends}"))
    });
    false
}

pub fn gifts(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Gift Inventory");
    read_only_list(app, ui, "gifts", "/users/@me/entitlements/gifts", "You don't have any gifts. Gifts you buy for others show up here.", |g| {
        (g["sku"]["name"].as_str().unwrap_or("Gift").to_string(), String::new())
    });
    false
}

pub fn billing(app: &mut App, ui: &mut Ui) -> bool {
    title(ui, "Billing");
    h5(ui, "Payment methods");
    read_only_list(app, ui, "sources", "/users/@me/billing/payment-sources", "You don't have any payment methods saved.", |p| {
        let brand = p["brand"].as_str().unwrap_or(if p["type"].as_u64() == Some(2) { "PayPal" } else { "Card" });
        let last4 = p["last_4"].as_str().map(|l| format!(" ending in {l}")).unwrap_or_default();
        let exp = match (p["expires_month"].as_u64(), p["expires_year"].as_u64()) {
            (Some(m), Some(y)) => format!("Expires {m:02}/{y}"),
            _ => String::new(),
        };
        (format!("{brand}{last4}"), exp)
    });
    divider(ui);
    h5(ui, "Transaction history");
    read_only_list(app, ui, "payments", "/users/@me/billing/payments?limit=20", "No transactions yet.", |p| {
        let amount = p["amount"].as_u64().unwrap_or(0) as f64 / 100.0;
        let cur = p["currency"].as_str().unwrap_or("usd").to_uppercase();
        let when = p["created_at"].as_str().unwrap_or("").get(..10).unwrap_or("").to_string();
        (format!("{} — {amount:.2} {cur}", p["description"].as_str().unwrap_or("Payment")), when)
    });
    false
}
