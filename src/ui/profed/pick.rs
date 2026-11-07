//! The dialogs and menus the editor opens: the image chooser with recent avatars, the shop's
//! decorations, effects, frames and nameplates, colours, and the banner and server tag menus.

use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::draft::Pic;
use super::{Dlg, Editor, Menu, Swatch, card, faint, hex, pack, rgb_of, round_icon, style, tile};
use crate::net::Tag;
use crate::state::{Id, User};
use crate::theme::*;
use crate::ui::cosmo::{self, Item, Kind};
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{mark, remote};
use crate::ui::{App, Confirm, DANGER, avatar, motion, text};

const EYEDROP: &str = "profed_eyedrop";

fn eyedrop(ed: &mut Editor, ctx: &egui::Context) {
    if let Some(c) = crate::ui::eyedrop::pick(ctx, EYEDROP, &mut ed.eyedrop)
        && let Dlg::Colour(_, rgb) = &mut ed.dlg
    {
        *rgb = [c.r(), c.g(), c.b()];
    }
}

pub fn dialogs(app: &mut App, ed: &mut Editor, ctx: &egui::Context, user: &User) {
    menus(app, ed, ctx);
    eyedrop(ed, ctx);
    match &ed.dlg {
        Dlg::None => {}
        Dlg::Avatar => avatar_dialog(app, ed, ctx),
        Dlg::Pick(..) => collectible(app, ed, ctx, user),
        Dlg::Style(_) => style::dialog(app, ed, ctx, user),
        Dlg::Colour(..) => colour(ed, ctx),
        Dlg::Widget(_) => super::board::dialog(app, ed, ctx),
        Dlg::Browse => super::board::browse(app, ed, ctx),
    }
}

fn frame() -> Frame {
    Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)))
}

fn heading(ui: &Ui, x: f32, y: f32, s: &str, size: f32) {
    ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, s, semi(size), pal().header_primary);
}

fn footer_button(ui: &mut Ui, r: Rect, label: &str, primary: bool, on: bool, key: &str) -> bool {
    let resp = ui.interact(r, ui.id().with(("footer", label)), Sense::click());
    let hot = motion::hot(ui.ctx(), &resp);
    let fill = if primary { motion::mix(BLURPLE, BLURPLE.gamma_multiply(0.85), hot) } else { faint(if resp.hovered() { 51 } else { 31 }) };
    ui.painter().rect_filled(r, CornerRadius::same(8), if on || !primary { fill } else { fill.gamma_multiply(0.5) });
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(16.0), if on || !primary { Color32::WHITE } else { Color32::WHITE.gamma_multiply(0.5) });
    mark(ui, key, r);
    if resp.hovered() && on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked() && on
}

fn avatars_url(uid: Id, h: &str) -> String {
    format!("{}/avatars/{uid}/{h}.png?size=128", crate::api::cdn())
}

fn avatar_dialog(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    let me = app.store.me;
    let list = remote(app, "avatars", "/users/@me/avatars").cloned();
    let recent: Vec<(String, String)> = match &list {
        Some(Ok(v)) => v.as_array().into_iter().flatten().map(|a| (a["id"].as_str().map_or_else(|| a["id"].to_string(), str::to_string), a["hash"].as_str().or(a["avatar"].as_str()).unwrap_or("").to_string())).collect(),
        _ => Vec::new(),
    };
    let nitro = app.store.nitro();
    let shift = ctx.input(|i| i.modifiers.shift);
    let (mut upload, mut gif, mut close, mut pick, mut del) = (false, false, false, None, None);
    let screen = ctx.content_rect();
    let w = 560.0f32.min(screen.width() - 48.0);
    let m = motion::Modal::new("profed_avatar".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame()).show(ctx, |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect();
        let x = area.left() + 24.0;
        heading(ui, x, area.top() + 36.0, "Select an Image", 24.0);
        let c = Rect::from_min_size(pos2(area.right() - 56.0, area.top() + 20.0), vec2(32.0, 32.0));
        close |= round_icon(ui, c, Icon::Close, "Close", false).clicked();
        let top = area.top() + 76.0;
        let half = (w - 48.0 - 8.0) / 2.0;
        let up = Rect::from_min_size(pos2(x, top), vec2(half, half.min(220.0)));
        let g = Rect::from_min_size(pos2(x + half + 8.0, top), vec2(half, half.min(220.0)));
        let resp = tile(ui, up, "upload", false);
        icons::draw(ui.painter(), Icon::Upload, Rect::from_center_size(up.center() - vec2(0.0, 14.0), vec2(28.0, 28.0)), pal().header_primary);
        ui.painter().text(up.center() + vec2(0.0, 24.0), Align2::CENTER_CENTER, "Upload Image", medium(14.0), pal().header_primary);
        mark(ui, "avatar:upload", up);
        upload = resp.on_hover_text("Upload a PNG, JPG, or GIF under 10MB").clicked();
        let resp = tile(ui, g, "gif", false);
        ui.painter().rect_filled(g.shrink(1.0), CornerRadius::same(8), Color32::from_rgb(0x1c, 0x22, 0x2a));
        ui.painter().text(g.center() + vec2(0.0, 0.0), Align2::CENTER_CENTER, "Choose GIF", medium(14.0), Color32::WHITE);
        icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(g.center().x - 58.0, g.center().y), vec2(16.0, 16.0)), Color32::WHITE);
        mark(ui, "avatar:gif", g);
        gif = resp.clicked();
        let mut y = top + up.height() + 28.0;
        heading(ui, x, y + 10.0, "Recent Avatars", 16.0);
        y += 28.0;
        ui.painter().text(pos2(x, y + 9.0), Align2::LEFT_CENTER, "Access your 6 most recent avatar uploads.", reg(14.0), pal().header_secondary);
        y += 36.0;
        let step = ((w - 48.0 - 66.0) / 5.0).clamp(40.0, 80.0);
        let d = (step - 8.0).min(66.0);
        for i in 0..6 {
            let r = Rect::from_min_size(pos2(x + i as f32 * step, y), vec2(d, d));
            match recent.get(i) {
                Some((id, h)) => {
                    let resp = ui.interact(r, ui.id().with(("recent", i)), Sense::click());
                    avatar(ui, r, &avatars_url(me, h));
                    mark(ui, &format!("avatar:recent:{i}"), r);
                    if ui.rect_contains_pointer(r) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        let x_r = Rect::from_min_size(pos2(r.right() - 20.0, r.top()), vec2(22.0, 22.0));
                        let xr = ui.interact(x_r, ui.id().with(("recent_x", i)), Sense::click());
                        ui.painter().circle_filled(x_r.center(), 11.0, Color32::from_rgb(0xd2, 0x2d, 0x39));
                        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x_r.center(), vec2(12.0, 12.0)), Color32::WHITE);
                        mark(ui, &format!("avatar:recent_x:{i}"), x_r);
                        if xr.on_hover_text(if shift { "Delete without asking" } else { "Delete avatar" }).clicked() {
                            del = Some(id.clone());
                        } else if resp.clicked() {
                            pick = Some((id.clone(), avatars_url(me, h)));
                        }
                    } else if resp.clicked() {
                        pick = Some((id.clone(), avatars_url(me, h)));
                    }
                }
                None => {
                    ui.painter().circle_filled(r.center(), d / 2.0, faint(41));
                    icons::draw(ui.painter(), Icon::Person, Rect::from_center_size(r.center(), vec2(d * 0.5, d * 0.5)), pal().header_secondary);
                }
            }
        }
        y += d + 12.0;
        let room = 6usize.saturating_sub(recent.len());
        ui.painter().text(pos2(x, y + 9.0), Align2::LEFT_CENTER, format!("You have room for {room} more avatar{}.", if room == 1 { "" } else { "s" }), reg(14.0), pal().header_secondary);
        ui.allocate_rect(Rect::from_min_max(area.min, pos2(area.right(), y + 40.0)), Sense::hover());
    });
    if let Some(id) = del {
        let go = move |app: &mut App| {
            app.rest("DELETE", format!("/users/@me/avatars/{id}"), None, Tag::Remote("action"));
            crate::ui::settings::refetch(app, "avatars");
        };
        if shift {
            go(app);
        } else {
            app.confirm = Some(Confirm { fill: DANGER, ..Confirm::new("Delete Avatar", "Are you sure you want to delete this avatar from your recent avatars?", "Delete", go) });
        }
    } else if let Some((id, url)) = pick {
        ed.draft.avatar = Pic::Recent(id, url);
        ed.dlg = Dlg::None;
    } else if upload {
        ed.dlg = Dlg::None;
        card::pick_image(app);
    } else if gif {
        if nitro {
            ed.dlg = Dlg::None;
            let c = ctx.content_rect().center();
            app.open_gifs_for_profile(pos2(c.x + 248.0, c.y + 198.0));
        } else {
            app.toast("Animated avatars are exclusive to Nitro.");
        }
    } else if close || m.should_close() {
        ed.dlg = Dlg::None;
    }
}

/// A GIF picked for the avatar: once its picture has arrived it goes to the cropper.
pub fn gif_arrived(app: &mut App, ctx: &egui::Context) {
    let Some(url) = app.gif_load.clone() else { return };
    match ctx.try_load_bytes(&url) {
        Ok(egui::load::BytesPoll::Ready { bytes, .. }) => {
            app.gif_load = None;
            crate::ui::crop::open(app, bytes.to_vec(), 1.0, |app: &mut App, u: String| {
                if let Some(e) = &mut app.profed {
                    e.draft.avatar = Pic::New(u);
                }
            });
        }
        Ok(_) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
        Err(_) => {
            app.gif_load = None;
            app.toast("Couldn't load that GIF.");
        }
    }
}

fn load(app: &mut App) {
    if app.cosmo.owned.is_none()
        && let Some(Ok(v)) = remote(app, "owned", "/users/@me/collectibles-purchases?variants_return_style=2")
    {
        let list = cosmo::items(v);
        app.cosmo.absorb(list.clone());
        app.cosmo.owned = Some(list);
    }
    if app.cosmo.shop.is_none()
        && let Some(Ok(v)) = remote(app, "shop", "/collectibles-categories/v2?include_bundles=true&variants_return_style=2")
    {
        let list = cosmo::items(v);
        app.cosmo.absorb(list.clone());
        app.cosmo.shop = Some(list);
    }
}

fn dims(kind: Kind) -> (f32, f32, usize) {
    match kind {
        Kind::Decoration => (75.0, 80.0, 3),
        Kind::Effect => (83.0, 94.0, 3),
        Kind::Frame => (83.0, 94.0, 3),
        Kind::Nameplate => (263.0, 49.0, 1),
    }
}

fn tile_art(ui: &Ui, r: Rect, it: &Item, locked: bool) {
    let p = ui.painter().with_clip_rect(r);
    match it.kind {
        Kind::Decoration => text::image(&p, &cosmo::deco(&it.asset, 128), Rect::from_center_size(r.center(), vec2(64.0, 64.0))),
        Kind::Nameplate => text::image(&p, &cosmo::plate(&it.asset), r),
        _ => text::image(&p, &it.thumb, r),
    }
    if locked {
        let b = Rect::from_min_size(pos2(r.right() - 22.0, r.top() + 4.0), vec2(18.0, 18.0));
        ui.painter().circle_filled(b.center(), 9.0, Color32::WHITE);
        icons::draw(ui.painter(), Icon::Lock, Rect::from_center_size(b.center(), vec2(11.0, 11.0)), Color32::from_gray(20));
    }
}

#[allow(clippy::too_many_lines)]
fn collectible(app: &mut App, ed: &mut Editor, ctx: &egui::Context, user: &User) {
    let Dlg::Pick(kind, sel) = &ed.dlg else { return };
    let kind = *kind;
    let mut chosen: Option<Option<Item>> = None;
    load(app);
    let owned = app.cosmo.owned.clone().unwrap_or_default();
    let shop = app.cosmo.shop.clone().unwrap_or_default();
    let nitro = app.store.nitro();
    let (title, mine, sku_cur) = match kind {
        Kind::Decoration => ("Change Avatar Decoration", "Your Decorations", ed.base.decoration.clone()),
        Kind::Effect => ("Change Profile Effect", "Your Effects", ed.base.effect.clone()),
        Kind::Frame => ("Change Profile Frame", "Your Frames", ed.base.frame.clone()),
        Kind::Nameplate => ("Change Nameplate", "Your Nameplates", ed.base.nameplate.clone()),
    };
    let _ = sku_cur;
    let mine_items: Vec<Item> = owned.iter().filter(|i| i.kind == kind).cloned().collect();
    let owned_skus: Vec<&str> = mine_items.iter().map(|i| i.sku.as_str()).collect();
    let free: Vec<Item> = shop.iter().filter(|i| i.kind == kind && i.nitro && !owned_skus.contains(&i.sku.as_str())).cloned().collect();
    let buy: Vec<Item> = shop.iter().filter(|i| i.kind == kind && !i.nitro && !owned_skus.contains(&i.sku.as_str())).cloned().collect();
    let selected = sel.clone();
    let can_use = |i: &Item| owned_skus.contains(&i.sku.as_str()) || (i.nitro && nitro);
    let (mut apply, mut cancel, mut shop_open) = (false, false, false);
    let screen = ctx.content_rect();
    let (tw, th, cols) = dims(kind);
    let gap = 12.0;
    let grid_w = tw * cols as f32 + gap * (cols as f32 - 1.0);
    let wide = !super::one_column(ctx);
    let w = (grid_w + 48.0 + if wide { 24.0 + 320.0 } else { 0.0 }).min(screen.width() - 48.0);
    let max_h = (screen.height() - 120.0).clamp(280.0, 560.0);
    let m = motion::Modal::new("profed_pick".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame()).show(ctx, |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect();
        let x = area.left() + 24.0;
        heading(ui, x, area.top() + 36.0, title, 20.0);
        let c = Rect::from_min_size(pos2(area.right() - 52.0, area.top() + 20.0), vec2(32.0, 32.0));
        cancel |= round_icon(ui, c, Icon::Close, "Close", false).clicked();
        let list = Rect::from_min_size(pos2(x, area.top() + 64.0), vec2(grid_w + 8.0, max_h - 64.0 - 76.0));
        let mut scroll = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical().id_salt(("profed_pick", kind as u8)).auto_shrink([false, false]).show(&mut scroll, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let left = list.left();
            let mut y = ui.cursor().top();
            let section = |ui: &Ui, y: &mut f32, s: &str| {
                ui.painter().text(pos2(left, *y + 10.0), Align2::LEFT_CENTER, s, semi(16.0), pal().header_primary);
                *y += 28.0;
            };
            let place = |i: usize, y: f32| Rect::from_min_size(pos2(left + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * (th + 12.0)), vec2(tw, th));
            let rows = |n: usize| n.div_ceil(cols) as f32 * (th + 12.0) + 16.0;
            section(ui, &mut y, mine);
            let none = place(0, y);
            let resp = tile(ui, none, "none", selected.is_none());
            icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(none.center() - vec2(0.0, 8.0), vec2(24.0, 24.0)), pal().header_primary);
            ui.painter().text(none.center() + vec2(0.0, 16.0), Align2::CENTER_CENTER, "None", reg(12.0), pal().header_primary);
            mark(ui, "pick:none", none);
            if resp.clicked() {
                chosen = Some(None);
            }
            let sh = place(1, y);
            let resp = tile(ui, sh, "shop", false);
            icons::draw(ui.painter(), Icon::Shop, Rect::from_center_size(sh.center() - vec2(0.0, 8.0), vec2(24.0, 24.0)), pal().header_primary);
            ui.painter().text(sh.center() + vec2(0.0, 16.0), Align2::CENTER_CENTER, "Shop", reg(12.0), pal().header_primary);
            mark(ui, "pick:shop", sh);
            shop_open |= resp.clicked();
            for (i, it) in mine_items.iter().enumerate() {
                let r = place(i + 2, y);
                let resp = tile(ui, r, &it.sku, selected.as_ref() == Some(it));
                tile_art(ui, r, it, false);
                mark(ui, &format!("collectible:{}", it.sku), r);
                if resp.on_hover_text(if it.label.is_empty() { &it.name } else { &it.label }).clicked() {
                    chosen = Some(Some(it.clone()));
                }
            }
            y += rows(mine_items.len() + 2);
            for (s, items, lock) in [("Exclusive to Nitro", &free, !nitro), ("See What's in Shop", &buy, true)] {
                if items.is_empty() {
                    continue;
                }
                section(ui, &mut y, s);
                for (i, it) in items.iter().enumerate() {
                    let r = place(i, y);
                    let resp = tile(ui, r, &it.sku, selected.as_ref() == Some(it));
                    tile_art(ui, r, it, lock);
                    mark(ui, &format!("collectible:{}", it.sku), r);
                    if resp.on_hover_text(if it.label.is_empty() { &it.name } else { &it.label }).clicked() {
                        chosen = Some(Some(it.clone()));
                    }
                }
                y += rows(items.len());
            }
            if app.cosmo.owned.is_none() || app.cosmo.shop.is_none() {
                motion::spinner(&mut ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(left + 8.0, y), vec2(24.0, 24.0)))), 24.0, pal().text_muted);
                y += 32.0;
            }
            ui.allocate_rect(Rect::from_min_max(pos2(left, ui.cursor().top()), pos2(left + grid_w, y)), Sense::hover());
        });
        if wide {
            let pr = Rect::from_min_max(pos2(list.right() + 16.0, area.top() + 64.0), pos2(area.right() - 24.0, area.top() + max_h - 76.0));
            preview(app, ed_ref(&selected), kind, ui, pr, user, ed_draft_hint(ed));
        }
        let by = area.top() + max_h - 60.0;
        let ap = Rect::from_min_size(pos2(area.right() - 24.0 - 120.0, by), vec2(120.0, 44.0));
        let cn = Rect::from_min_size(pos2(ap.left() - 12.0 - 120.0, by), vec2(120.0, 44.0));
        cancel |= footer_button(ui, cn, "Cancel", false, true, "pick:cancel");
        let ok = selected.as_ref().is_none_or(&can_use);
        apply = footer_button(ui, ap, if ok { "Apply" } else { "Get in Shop" }, true, true, "pick:apply");
        ui.allocate_rect(Rect::from_min_size(area.min, vec2(w, max_h)), Sense::hover());
    });
    if let Dlg::Pick(_, s) = &mut ed.dlg
        && let Some(c) = chosen
    {
        *s = c;
    }
    ed.to_shop |= shop_open;
    if apply {
        let it = selected.clone();
        if it.as_ref().is_none_or(&can_use) {
            match kind {
                Kind::Decoration => ed.draft.decoration = it,
                Kind::Effect => ed.draft.effect = it,
                Kind::Frame => ed.draft.frame = it,
                Kind::Nameplate => ed.draft.nameplate = it,
            }
            ed.since = ctx.input(|i| i.time);
            ed.dlg = Dlg::None;
        } else {
            ed.to_shop = true;
        }
    } else if cancel || m.should_close() {
        ed.dlg = Dlg::None;
    }
}

fn ed_ref(sel: &Option<Item>) -> Option<&Item> {
    sel.as_ref()
}

fn ed_draft_hint(ed: &Editor) -> (String, f64) {
    (if ed.draft.name.is_empty() { String::new() } else { ed.draft.name.clone() }, ed.since)
}

fn preview(app: &mut App, it: Option<&Item>, kind: Kind, ui: &mut Ui, r: Rect, user: &User, (name, since): (String, f64)) {
    let name = if name.is_empty() { user.display().to_string() } else { name };
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::BLACK);
    match kind {
        Kind::Decoration => {
            let d = r.width().min(r.height()) * 0.55;
            let av = Rect::from_center_size(r.center(), vec2(d, d));
            avatar(ui, av, &user.avatar_url());
            if let Some(it) = it {
                text::image(ui.painter(), &cosmo::deco(&it.asset, 256), Rect::from_center_size(av.center(), av.size() * 1.2));
            }
        }
        Kind::Nameplate => {
            let rows = 5;
            let rh = 49.0;
            let top = r.center().y - rows as f32 * rh / 2.0;
            for i in 0..rows {
                let rr = Rect::from_min_size(pos2(r.left() + 8.0, top + i as f32 * rh), vec2(r.width() - 16.0, rh - 4.0));
                if i == rows / 2 {
                    if let Some(it) = it {
                        text::image(&ui.painter().with_clip_rect(rr), &cosmo::plate(&it.asset), rr);
                    }
                    avatar(ui, Rect::from_center_size(pos2(rr.left() + 24.0, rr.center().y), vec2(32.0, 32.0)), &user.avatar_url());
                    ui.painter().text(pos2(rr.left() + 48.0, rr.center().y), Align2::LEFT_CENTER, &name, medium(16.0), Color32::WHITE);
                } else {
                    ui.painter().circle_filled(pos2(rr.left() + 24.0, rr.center().y), 16.0, faint(31));
                    ui.painter().rect_filled(Rect::from_center_size(pos2(rr.left() + 130.0, rr.center().y), vec2(150.0, 12.0)), CornerRadius::same(6), faint(31));
                }
            }
        }
        Kind::Effect | Kind::Frame => {
            let w = (r.width() - 24.0).min(300.0);
            let card = Rect::from_min_size(pos2(r.center().x - w / 2.0, r.top() + 12.0), vec2(w, 300.0f32.min(r.height() - 24.0)));
            ui.painter().rect_filled(card, CornerRadius::same(8), Color32::from_rgb(0x0f, 0x4a, 0x42));
            ui.painter().rect_filled(Rect::from_min_size(card.min, vec2(w, 100.0)), CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Color32::from_rgb(0x2f, 0x6b, 0x5a));
            let av = Rect::from_min_size(card.min + vec2(14.0, 52.0), vec2(80.0, 80.0));
            avatar(ui, av, &user.avatar_url());
            ui.painter().text(card.min + vec2(16.0, 154.0), Align2::LEFT_CENTER, &name, semi(20.0), Color32::WHITE);
            ui.painter().text(card.min + vec2(16.0, 178.0), Align2::LEFT_CENTER, &user.username, reg(14.0), Color32::WHITE);
            let btn = Rect::from_min_size(card.min + vec2(14.0, 204.0), vec2(w - 28.0, 32.0));
            ui.painter().rect_filled(btn, CornerRadius::same(4), Color32::from_rgb(0x0f, 0x6b, 0x4a));
            ui.painter().text(btn.center(), Align2::CENTER_CENTER, "Example Button", medium(14.0), Color32::WHITE);
            if let Some(it) = it {
                let layers = app.product(&it.sku).map_or_else(|| it.layers.clone(), |p| p.layers);
                let started = cosmo::since(ui.ctx(), ("profed_prev", it.sku.clone()));
                let _ = since;
                cosmo::paint_layers(ui, card, &layers, started);
            }
        }
    }
}

fn colour(ed: &mut Editor, ctx: &egui::Context) {
    let Dlg::Colour(which, rgb) = &mut ed.dlg else { return };
    let which = *which;
    let (mut ok, mut cancel, mut pipette) = (false, false, false);
    let mut c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let mut text_hex = hex(pack(*rgb));
    let m = motion::Modal::new("profed_colour".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame().inner_margin(24)).show(ctx, |ui| {
        ui.set_width(300.0);
        ui.label(RichText::new(match which { Swatch::Primary => "Primary Color", Swatch::Accent => "Accent Color", Swatch::Name(_) => "Pick a Color" }).font(semi(20.0)).color(pal().header_primary));
        ui.add_space(12.0);
        egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque);
        ui.add_space(8.0);
        let eb = ui.add(egui::Label::new(RichText::new("Pick a color from the page").font(medium(14.0)).color(pal().link)).sense(Sense::click()));
        mark(ui, "colour:eyedrop", eb.rect);
        if eb.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        pipette |= eb.clicked();
        ui.add_space(8.0);
        let r = ui.add(egui::TextEdit::singleline(&mut text_hex).desired_width(100.0).font(reg(14.0)));
        mark(ui, "colour:hex", r.rect);
        if r.changed()
            && let Ok(v) = u32::from_str_radix(text_hex.trim_start_matches('#'), 16)
        {
            c = Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        }
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = crate::ui::button(ui, "Done", BLURPLE, 100.0);
            mark(ui, "colour:done", r.rect);
            ok = r.clicked();
            cancel = crate::ui::grey_button(ui, "Cancel", 100.0).clicked();
        });
    });
    *rgb = [c.r(), c.g(), c.b()];
    let value = pack(*rgb);
    if pipette {
        crate::ui::eyedrop::start(ctx, EYEDROP);
    }
    if ok {
        match which {
            Swatch::Primary => ed.draft.theme[0] = Some(value),
            Swatch::Accent => ed.draft.theme[1] = Some(value),
            Swatch::Name(i) => {
                if let Some(mut s) = ed.style_back.take() {
                    if s.colors.len() <= i {
                        s.colors.resize(i + 1, value);
                    }
                    s.colors[i] = value;
                    ed.dlg = Dlg::Style(s);
                    return;
                }
            }
        }
        ed.dlg = Dlg::None;
    } else if cancel || m.should_close() {
        ed.dlg = match (which, ed.style_back.take()) {
            (Swatch::Name(_), Some(s)) => Dlg::Style(s),
            _ => Dlg::None,
        };
    }
    let _ = rgb_of;
}

fn menus(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    match ed.menu {
        Some(Menu::Banner) => banner_menu(app, ed, ctx),
        Some(Menu::Tag) => tag_menu(app, ed, ctx),
        _ => {}
    }
}

fn banner_menu(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    let mut act = None;
    let has = !matches!(ed.draft.banner, Pic::Gone) && (matches!(ed.draft.banner, Pic::New(_)) || ed.draft.banner_url.is_some());
    let area = egui::Area::new("profed_banner_menu".into()).order(egui::Order::Tooltip).fixed_pos(ed.anchor.right_top() + vec2(8.0, 0.0)).show(ctx, |ui| {
        Frame::popup(ui.style()).fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, faint(41))).show(ui, |ui| {
            ui.set_width(190.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, label) in ["Change Banner", "Change Profile Effect", "Change Profile Frame", "Remove Banner"].into_iter().enumerate() {
                if i == 3 && !has {
                    continue;
                }
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), faint(31));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, label, medium(14.0), if i == 3 { Color32::from_rgb(0xf2, 0x3f, 0x43) } else { pal().header_primary });
                mark(ui, &format!("banner_menu:{i}"), r);
                if resp.clicked() {
                    act = Some(i);
                }
            }
        });
    });
    if let Some(i) = act {
        ed.menu = None;
        match i {
            0 => card::change_banner(app, ed),
            1 => ed.dlg = Dlg::Pick(Kind::Effect, ed.draft.effect.clone()),
            2 => ed.dlg = Dlg::Pick(Kind::Frame, ed.draft.frame.clone()),
            _ => ed.draft.banner = Pic::Gone,
        }
    } else if area.response.clicked_elsewhere() && !ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| ed.anchor.contains(p)) {
        ed.menu = None;
    }
}

fn tag_menu(app: &mut App, ed: &mut Editor, ctx: &egui::Context) {
    let ids: Vec<Id> = app.store.guild_order.iter().copied().filter(|g| app.store.guilds.get(g).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_TAGS"))).collect();
    for g in &ids {
        if !app.guild_profiles.contains_key(g) {
            app.guild_profiles.insert(*g, Value::Null);
            app.rest("GET", format!("/guild-profile/{g}"), None, Tag::GuildProfile(*g));
        }
    }
    let q = ed.tag_q.to_lowercase();
    let rows: Vec<(Id, String, String, Option<String>)> = ids
        .iter()
        .filter_map(|g| {
            let p = app.guild_profiles.get(g).filter(|p| !p.is_null())?;
            let tag = p["tag"].as_str().filter(|t| !t.is_empty())?.to_string();
            let badge = p["badge_hash"].as_str().or(p["badge"].as_str()).filter(|b| !b.is_empty()).map(|b| format!("{}/guild-tag-badges/{g}/{b}.png?size=32", crate::api::cdn()));
            Some((*g, app.store.guilds.get(g)?.name.clone(), tag, badge))
        })
        .filter(|(_, n, t, _)| q.is_empty() || n.to_lowercase().contains(&q) || t.to_lowercase().contains(&q))
        .collect();
    let mut pick: Option<Option<Id>> = None;
    let area = egui::Area::new("profed_tag_menu".into()).order(egui::Order::Tooltip).fixed_pos(ed.anchor.left_bottom() + vec2(-4.0, 6.0)).show(ctx, |ui| {
        Frame::popup(ui.style()).fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, faint(41))).show(ui, |ui| {
            ui.set_width(320.0);
            let r = ui.add(egui::TextEdit::singleline(&mut ed.tag_q).hint_text("Search").desired_width(f32::INFINITY).font(reg(16.0)));
            mark(ui, "tag:search", r.rect);
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), faint(31));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, "No Server Tag", italic(16.0), pal().header_secondary);
                mark(ui, "tag:none", r);
                if resp.clicked() {
                    pick = Some(None);
                }
                for (g, name, tag, badge) in &rows {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), faint(31));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let icon = app.store.guilds.get(g).and_then(|g| g.icon_url());
                    let ir = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0));
                    ui.painter().circle_filled(ir.center(), 12.0, pal().bg_accent);
                    if let Some(u) = icon {
                        text::image(ui.painter(), &u, ir);
                    }
                    text::paint(ui.painter(), pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, name, reg(16.0), pal().header_primary, 190.0);
                    let chip = Rect::from_min_size(pos2(r.right() - 76.0, r.center().y - 10.0), vec2(68.0, 20.0));
                    ui.painter().rect_filled(chip, CornerRadius::same(4), faint(41));
                    if let Some(b) = badge {
                        text::image(ui.painter(), b, Rect::from_min_size(chip.min + vec2(4.0, 3.0), vec2(14.0, 14.0)));
                    }
                    ui.painter().text(pos2(chip.left() + 22.0, chip.center().y), Align2::LEFT_CENTER, tag, semi(12.0), pal().header_primary);
                    mark(ui, &format!("tag:{g}"), r);
                    if resp.clicked() {
                        pick = Some(Some(*g));
                    }
                }
            });
        });
    });
    if let Some(p) = pick {
        ed.draft.tag = p;
        ed.menu = None;
        ed.tag_q.clear();
    } else if area.response.clicked_elsewhere() && !ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| ed.anchor.contains(p)) {
        ed.menu = None;
    }
}
