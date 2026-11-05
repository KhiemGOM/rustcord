use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{dropdown, h5, input, link, loading, note, primary, refetch, remote, secondary};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::{App, avatar, text};

#[derive(Clone, PartialEq, Default)]
pub struct Item {
    sku: String,
    asset: String,
    label: String,
    thumb: String,
}

#[derive(Clone, PartialEq, Default)]
pub struct Profile {
    name: String,
    pronouns: String,
    bio: String,
    accent: Option<u32>,
    theme: [Option<u32>; 2],
    // data: uris when a new image was picked
    avatar: Option<String>,
    remove_avatar: bool,
    banner: Option<String>,
    remove_banner: bool,
    decoration: Option<Item>,
    nameplate: Option<Item>,
    effect: Option<Item>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Decoration,
    Nameplate,
    Effect,
}

fn item_of(v: &Value, key: &str) -> Option<Item> {
    let d = &v[key];
    if !d.is_object() {
        return None;
    }
    Some(Item {
        sku: d["sku_id"].as_str().or(d["id"].as_str()).unwrap_or("").to_string(),
        asset: d["asset"].as_str().unwrap_or("").to_string(),
        label: d["label"].as_str().unwrap_or("").to_string(),
        thumb: String::new(),
    })
}

fn profile_from(v: &Value, me: &crate::state::User, guild: Option<Id>) -> Profile {
    let (p, m) = if guild.is_some() { (&v["guild_member_profile"], &v["guild_member"]) } else { (&v["user_profile"], &v["user"]) };
    let theme = |i: usize| p["theme_colors"][i].as_u64().map(|c| c as u32);
    Profile {
        name: if guild.is_some() { m["nick"].as_str().unwrap_or("").to_string() } else { me.global_name.clone().unwrap_or_default() },
        pronouns: p["pronouns"].as_str().unwrap_or("").to_string(),
        bio: p["bio"].as_str().or(v["user"]["bio"].as_str()).unwrap_or("").to_string(),
        accent: p["accent_color"].as_u64().or(v["user"]["accent_color"].as_u64()).map(|c| c as u32),
        theme: [theme(0), theme(1)],
        avatar: None,
        remove_avatar: false,
        banner: None,
        remove_banner: false,
        decoration: item_of(if guild.is_some() && m["avatar_decoration_data"].is_object() { m } else { &v["user"] }, "avatar_decoration_data"),
        nameplate: item_of(&v["user"]["collectibles"], "nameplate"),
        effect: item_of(p, "profile_effect"),
    }
}

const BANNER_ASPECT: f32 = 5.0 / 2.0;

fn pick_image() -> Option<Vec<u8>> {
    let p = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file()?;
    std::fs::read(p).ok()
}

fn tabs(app: &mut App, ui: &mut Ui) {
    let guilds: Vec<(Id, String)> = app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g).map(|x| (*g, x.name.clone()))).collect();
    let cur = app.sform.profile_guild;
    let shown = cur.and_then(|g| guilds.iter().find(|x| x.0 == g)).map_or("Main Profile".to_string(), |g| g.1.clone());
    let mut pick = None;
    super::lead(ui);
    let r = dropdown(ui, 320.0, None, &shown, |ui| {
        if ui.selectable_label(cur.is_none(), "Main Profile").clicked() {
            pick = Some(None);
        }
        for (g, n) in &guilds {
            if ui.selectable_label(cur == Some(*g), n).clicked() {
                pick = Some(Some(*g));
            }
        }
    });
    super::mark(ui, "profile_select", r.rect);
    super::end(ui, 24.0);
    if let Some(p) = pick {
        app.sform.profile_guild = p;
        app.sform.profile = None;
        app.sform.profile_base = None;
        refetch(app, "me_profile");
    }
}

pub fn page(app: &mut App, ui: &mut Ui) -> bool {
    super::section(ui, "Profiles");
    tabs(app, ui);
    let me = app.store.me;
    let user = app.store.user(me).cloned().unwrap_or_default();
    let guild = app.sform.profile_guild;
    let path = match guild {
        Some(g) => format!("/users/{me}/profile?with_mutual_guilds=false&guild_id={g}"),
        None => format!("/users/{me}/profile?with_mutual_guilds=false"),
    };
    let loaded = loading(ui, remote(app, "me_profile", &path));
    let Some(v) = loaded else { return false };
    if app.sform.profile_base.is_none() {
        let p = profile_from(&v, &user, guild);
        app.sform.profile_base = Some(p.clone());
        app.sform.profile = Some(p);
    }
    let owned = remote(app, "owned", "/users/@me/collectibles-purchases").cloned();
    let Some(draft) = &mut app.sform.profile else { return false };
    let mut act = Act::None;
    ui.spacing_mut().item_spacing.x = 24.0;
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.spacing_mut().item_spacing.y = 6.0;
        h5(ui, if guild.is_some() { "Server Nickname" } else { "Display Name" });
        let hint = if guild.is_some() { user.display().to_string() } else { user.username.clone() };
        input(ui, &mut draft.name, &hint, false);
        ui.add_space(12.0);
        h5(ui, "Pronouns");
        let r = input(ui, &mut draft.pronouns, "Add your pronouns", false);
        super::mark(ui, "profile:pronouns", r.rect);
        ui.add_space(12.0);
        h5(ui, "Avatar & Decoration");
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "Change Avatar").clicked() {
                act = Act::Avatar;
            }
            let b = secondary(ui, "Change Decoration");
            super::mark(ui, "pick:decoration", b.rect);
            if b.clicked() {
                act = Act::Pick(Kind::Decoration);
            }
            if link(ui, "Remove Avatar").clicked() {
                draft.avatar = None;
                draft.remove_avatar = true;
            }
        });
        ui.add_space(12.0);
        h5(ui, "Theme & Banner");
        ui.horizontal_wrapped(|ui| {
            if secondary(ui, "Change Banner").clicked() {
                act = Act::Banner;
            }
            if link(ui, "Remove Banner").clicked() {
                draft.banner = None;
                draft.remove_banner = true;
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (i, l) in ["Banner Color", "Primary", "Accent"].iter().enumerate() {
                ui.vertical(|ui| {
                    ui.label(RichText::new(*l).font(medium(12.0)).color(pal().header_secondary));
                    let slot = match i {
                        0 => &mut draft.accent,
                        n => &mut draft.theme[n - 1],
                    };
                    let c = slot.unwrap_or(0x5865f2);
                    let mut rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        *slot = Some(((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32);
                    }
                });
                ui.add_space(12.0);
            }
        });
        ui.add_space(12.0);
        h5(ui, "Nameplate");
        ui.horizontal_wrapped(|ui| {
            match &draft.nameplate {
                Some(n) => {
                    let (r, _) = ui.allocate_exact_size(vec2(224.0, 42.0), Sense::hover());
                    text::image(ui.painter(), &format!("{}/assets/collectibles/{}static.png", crate::api::cdn(), n.asset), r);
                }
                None => {
                    ui.label(RichText::new("Not set").font(reg(14.0)).color(pal().header_secondary));
                }
            }
            if secondary(ui, "Change Nameplate").clicked() {
                act = Act::Pick(Kind::Nameplate);
            }
        });
        if guild.is_none() {
            ui.add_space(12.0);
            h5(ui, "Profile Effect");
            ui.horizontal(|ui| {
                match &draft.effect {
                    Some(e) if !e.thumb.is_empty() => {
                        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                        text::image(ui.painter(), &e.thumb, r);
                    }
                    Some(e) => {
                        ui.label(RichText::new(if e.label.is_empty() { "Equipped" } else { &e.label }).font(reg(14.0)).color(pal().header_primary));
                    }
                    None => {
                        ui.label(RichText::new("Not set").font(reg(14.0)).color(pal().header_secondary));
                    }
                }
                if secondary(ui, "Change Effect").clicked() {
                    act = Act::Pick(Kind::Effect);
                }
            });
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
    match act {
        Act::Avatar => {
            if let Some(b) = pick_image() {
                crate::ui::crop::open(app, b, 1.0, |app: &mut App, u: String| {
                    if let Some(d) = &mut app.sform.profile {
                        d.avatar = Some(u);
                        d.remove_avatar = false;
                    }
                });
            }
        }
        Act::Banner => {
            if let Some(b) = pick_image() {
                crate::ui::crop::open(app, b, BANNER_ASPECT, |app: &mut App, u: String| {
                    if let Some(d) = &mut app.sform.profile {
                        d.banner = Some(u);
                        d.remove_banner = false;
                    }
                });
            }
        }
        Act::Pick(k) => app.sform.picker = Some(k),
        Act::None => {}
    }
    picker(app, ui.ctx(), owned);
    false
}

enum Act {
    None,
    Avatar,
    Banner,
    Pick(Kind),
}

fn picker(app: &mut App, ctx: &egui::Context, owned: Option<Result<Value, String>>) {
    let Some(kind) = app.sform.picker else { return };
    let (title, ty) = match kind {
        Kind::Decoration => ("Avatar Decoration", 0),
        Kind::Nameplate => ("Nameplate", 2),
        Kind::Effect => ("Profile Effect", 1),
    };
    let mut items: Vec<Item> = Vec::new();
    if let Some(Ok(v)) = &owned {
        for p in v.as_array().into_iter().flatten() {
            for i in p["items"].as_array().into_iter().flatten().filter(|i| i["type"].as_u64() == Some(ty)) {
                items.push(Item {
                    sku: i["sku_id"].as_str().or(p["sku_id"].as_str()).or(i["id"].as_str()).unwrap_or("").to_string(),
                    asset: i["asset"].as_str().unwrap_or("").to_string(),
                    label: i["label"].as_str().or(i["title"].as_str()).or(p["name"].as_str()).unwrap_or("").to_string(),
                    thumb: i["thumbnailPreviewSrc"].as_str().unwrap_or("").to_string(),
                });
            }
        }
    }
    let cur = app.sform.profile.as_ref().and_then(|d| match kind {
        Kind::Decoration => d.decoration.clone(),
        Kind::Nameplate => d.nameplate.clone(),
        Kind::Effect => d.effect.clone(),
    });
    let mut chosen: Option<Option<Item>> = None;
    let mut close = false;
    let r = egui::Modal::new("profile_picker".into()).frame(Frame::new().fill(pal().bg_secondary).corner_radius(8)).show(ctx, |ui| {
        ui.set_width(520.0);
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(488.0);
            ui.label(RichText::new(title).font(semi(24.0)).color(pal().header_primary));
            ui.add_space(12.0);
            if owned.is_none() {
                ui.add(egui::Spinner::new().color(pal().text_muted));
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    let tile = |ui: &mut Ui, on: bool, id: &str| {
                        let (r, resp) = ui.allocate_exact_size(vec2(104.0, 104.0), Sense::click());
                        ui.painter().rect(r, 8, Color32::from_black_alpha(31), Stroke::new(if on { 2.0 } else { 1.0 }, if on { BLURPLE } else { Color32::from_rgba_unmultiplied(151, 151, 159, 61) }), egui::StrokeKind::Inside);
                        super::mark(ui, &format!("collectible:{id}"), r);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        (r, resp.clicked())
                    };
                    let (r, hit) = tile(ui, cur.is_none(), "none");
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "None", medium(14.0), pal().header_primary);
                    if hit {
                        chosen = Some(None);
                    }
                    for it in &items {
                        let (r, hit) = tile(ui, cur.as_ref().is_some_and(|c| c.sku == it.sku), &it.sku);
                        let url = match kind {
                            Kind::Decoration => format!("{}/avatar-decoration-presets/{}.png?size=96&passthrough=false", crate::api::cdn(), it.asset),
                            Kind::Nameplate => format!("{}/assets/collectibles/{}static.png", crate::api::cdn(), it.asset),
                            Kind::Effect => it.thumb.clone(),
                        };
                        text::image(ui.painter(), &url, r.shrink(8.0));
                        if hit {
                            chosen = Some(Some(it.clone()));
                        }
                    }
                });
            });
            if items.is_empty() && owned.is_some() {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("You don't own any {} yet.", title.to_lowercase())).font(reg(14.0)).color(pal().header_secondary));
                if link(ui, "Browse the Shop").clicked() {
                    super::open_web("/shop");
                }
            }
        });
        Frame::new().inner_margin(16).show(ui, |ui| {
            ui.set_width(488.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = crate::ui::button(ui, "Done", BLURPLE, 100.0);
                super::mark(ui, "picker_done", b.rect);
                if b.clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(c) = chosen
        && let Some(d) = &mut app.sform.profile
    {
        match kind {
            Kind::Decoration => d.decoration = c,
            Kind::Nameplate => d.nameplate = c,
            Kind::Effect => d.effect = c,
        }
    }
    if close || r.should_close() {
        app.sform.picker = None;
    }
}

fn preview(ui: &mut Ui, user: &crate::state::User, d: &Profile) {
    Frame::new().fill(pal().bg_tertiary).corner_radius(8).show(ui, |ui| {
        ui.set_width(300.0);
        let (banner, _) = ui.allocate_exact_size(vec2(300.0, 105.0), Sense::hover());
        let cr = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
        ui.painter().rect_filled(banner, cr, role_color(d.accent.or(d.theme[0]).unwrap_or(0x5865f2)));
        if let Some(uri) = d.banner.as_ref().filter(|_| !d.remove_banner) {
            use base64::Engine;
            let b64 = uri.split_once(',').map(|(_, b)| b).unwrap_or("");
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                egui::Image::from_bytes(format!("bytes://banner/{}", b64.len()), bytes).corner_radius(cr).paint_at(ui, banner);
            }
        }
        let av = Rect::from_min_size(pos2(banner.left() + 16.0, banner.bottom() - 40.0), vec2(80.0, 80.0));
        ui.painter().circle_filled(av.center(), 46.0, pal().bg_tertiary);
        match (&d.avatar, d.remove_avatar) {
            (Some(uri), _) => {
                use base64::Engine;
                let b64 = uri.split_once(',').map(|(_, b)| b).unwrap_or("");
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    egui::Image::from_bytes(format!("bytes://avatar/{}", b64.len()), bytes).corner_radius(CornerRadius::same(40)).paint_at(ui, av);
                }
            }
            (None, true) => {
                let none = crate::state::User { avatar: None, ..user.clone() };
                avatar(ui, av, &none.avatar_url());
            }
            (None, false) => avatar(ui, av, &user.avatar_url()),
        }
        if let Some(dec) = &d.decoration {
            text::image(ui.painter(), &format!("{}/avatar-decoration-presets/{}.png?size=96&passthrough=false", crate::api::cdn(), dec.asset), Rect::from_center_size(av.center(), vec2(96.0, 96.0)));
        }
        ui.add_space(48.0);
        Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let name = if d.name.trim().is_empty() { user.display().to_string() } else { d.name.clone() };
            text::label(ui, &name, semi(20.0), pal().header_primary, Sense::hover());
            ui.label(RichText::new(&user.username).font(reg(14.0)).color(pal().header_primary));
            if !d.pronouns.is_empty() {
                ui.label(RichText::new(&d.pronouns).font(reg(14.0)).color(pal().header_secondary));
            }
            if let Some(n) = &d.nameplate {
                let (r, _) = ui.allocate_exact_size(vec2(268.0, 50.0), Sense::hover());
                text::image(ui.painter(), &format!("{}/assets/collectibles/{}static.png", crate::api::cdn(), n.asset), r);
            }
            if !d.bio.is_empty() {
                ui.add_space(8.0);
                Frame::new().fill(pal().bg_secondary_alt).corner_radius(8).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("About Me").font(medium(14.0)).color(pal().header_primary));
                    ui.label(RichText::new(&d.bio).font(reg(14.0)).color(pal().text_normal));
                });
            }
        });
    });
}

pub fn save_bar(app: &mut App, ui: &mut Ui, area: Rect) {
    if app.settings_page != Some(super::Page::Profiles) {
        return;
    }
    let dirty = app.sform.profile.is_some() && app.sform.profile != app.sform.profile_base;
    if !dirty {
        return;
    }
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
    }
    if save && !app.sform.saving {
        app.sform.saving = true;
        save_profile(app);
    }
}

fn changed<T: PartialEq>(a: &T, b: &Option<Profile>, get: impl Fn(&Profile) -> &T) -> bool {
    b.as_ref().is_none_or(|b| get(b) != a)
}

fn save_profile(app: &mut App) {
    let (Some(d), base) = (app.sform.profile.clone(), app.sform.profile_base.clone()) else { return };
    let sku = |i: &Option<Item>| i.as_ref().map_or(Value::Null, |i| json!(i.sku));
    let mut prof = json!({ "bio": d.bio, "pronouns": d.pronouns, "accent_color": d.accent });
    if d.theme != [None, None] {
        prof["theme_colors"] = json!([d.theme[0], d.theme[1]]);
    }
    if let Some(b) = &d.banner {
        prof["banner"] = json!(b);
    } else if d.remove_banner {
        prof["banner"] = Value::Null;
    }
    let mut acct = json!({});
    if let Some(a) = &d.avatar {
        acct["avatar"] = json!(a);
    } else if d.remove_avatar {
        acct["avatar"] = Value::Null;
    }
    if changed(&d.decoration, &base, |b| &b.decoration) {
        acct["avatar_decoration_sku_id"] = sku(&d.decoration);
    }
    match app.sform.profile_guild {
        Some(g) => {
            acct["nick"] = if d.name.trim().is_empty() { Value::Null } else { json!(d.name.trim()) };
            if changed(&d.nameplate, &base, |b| &b.nameplate) {
                acct["collectibles"] = json!({ "nameplate": d.nameplate.as_ref().map(|n| json!({ "sku_id": n.sku })) });
            }
            app.rest("PATCH", format!("/guilds/{g}/profile/@me"), Some(prof), Tag::Remote("action"));
            app.rest("PATCH", format!("/guilds/{g}/members/@me"), Some(acct), Tag::Remote("profile_save"));
        }
        None => {
            if changed(&d.effect, &base, |b| &b.effect) {
                prof["profile_effect_id"] = d.effect.as_ref().map_or(Value::Null, |e| json!(e.sku));
            }
            acct["global_name"] = if d.name.trim().is_empty() { Value::Null } else { json!(d.name.trim()) };
            if changed(&d.nameplate, &base, |b| &b.nameplate) {
                acct["nameplate_sku_id"] = sku(&d.nameplate);
            }
            app.rest("PATCH", "/users/@me/profile".into(), Some(prof), Tag::Remote("action"));
            app.rest("PATCH", "/users/@me".into(), Some(acct), Tag::Remote("profile_save"));
        }
    }
}

pub fn saved(app: &mut App, res: Result<Value, String>) {
    app.sform.saving = false;
    match res {
        Err(e) => app.toast(e),
        Ok(v) => {
            if v["id"].is_string() && app.sform.profile_guild.is_none() {
                app.store.apply("USER_UPDATE", &v);
            }
            if let Some(d) = &mut app.sform.profile {
                d.avatar = None;
                d.remove_avatar = false;
                d.banner = None;
                d.remove_banner = false;
            }
            app.sform.profile_base = app.sform.profile.clone();
            refetch(app, "me_profile");
            app.profiles.remove(&app.store.me);
            app.toast("Your changes have been saved.");
        }
    }
}
