use egui::{Align2, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

use super::{App, faint, row, rule, title};
use crate::net::{Cmd, Tag};
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;

#[derive(Default)]
pub struct Expression {
    names: HashMap<Id, String>,
    pub dropped: Vec<PathBuf>,
    synced: usize,
    form: Option<StickerForm>,
}

impl Expression {
    pub fn editing(&self) -> bool {
        self.form.is_some()
    }
}

struct StickerForm {
    id: Id,
    name: String,
    tags: String,
    description: String,
    file: Option<PathBuf>,
}

const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

fn upload(app: &mut App, gid: Id, files: Vec<PathBuf>) {
    for path in files {
        let name: String = path.file_stem().map(|s| s.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()).unwrap_or_else(|| "emoji".into());
        if let Some(image) = super::form::data_uri(&path) {
            app.rest("POST", format!("/guilds/{gid}/emojis"), Some(json!({ "name": name, "image": image })), Tag::reason());
        }
    }
}

pub fn emoji(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let slots: usize = match g.tier {
        1 => 100,
        2 => 150,
        3 => 250,
        _ => 50,
    };
    title(ui, "Emoji", &format!("Add up to {slots} custom emoji that anyone can use in this server. Animated GIF emoji may be used by members with Discord Nitro."));
    let mut files: Vec<PathBuf> = Vec::new();
    if settings::primary(ui, "Upload Emoji").clicked()
        && let Some(f) = rfd::FileDialog::new().add_filter("Images", &IMAGES).pick_files()
    {
        files = f;
    }
    ui.add_space(8.0);
    ui.label(RichText::new("If you want to upload multiple emojis or skip the editor, drag and drop the file(s) onto this page. The emojis will be named using the file name.").font(reg(14.0)).color(pal().header_secondary));
    if let Some(s) = &mut app.server {
        files.append(&mut s.pages.emoji.dropped);
    }
    if !files.is_empty() {
        upload(app, gid, files);
    }
    rule(ui);
    let owners: HashMap<Id, (String, String)> = app
        .server_data("emojis", format!("/guilds/{gid}/emojis"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|e| (sid(&e["id"]), (e["user"]["global_name"].as_str().or(e["user"]["username"].as_str()).unwrap_or_default().to_string(), e["user"]["avatar"].as_str().map(|h| format!("{}/avatars/{}/{h}.png?size=64", crate::api::cdn(), e["user"]["id"].as_str().unwrap_or_default())).unwrap_or_default()))).collect())
        .unwrap_or_default();
    if g.emojis.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No Emoji").font(semi(20.0)).color(pal().header_primary));
            ui.label(RichText::new("Get the party started by uploading an emoji").font(reg(14.0)).color(pal().header_secondary));
        });
        return;
    }
    let (stat, anim): (Vec<_>, Vec<_>) = g.emojis.iter().partition(|e| !e.animated);
    let mut delete = None;
    let mut rename = None;
    for (head, list) in [("Emoji", stat), ("Animated Emoji", anim)] {
        if list.is_empty() && head != "Emoji" {
            continue;
        }
        ui.label(RichText::new(format!("{head} \u{2014} {} slots available", slots.saturating_sub(list.len()))).font(bold(12.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        for (x, h) in [(8.0, "IMAGE"), (0.18, "NAME"), (0.58, "UPLOADED BY")] {
            let px = if x > 1.0 { r.left() + x } else { r.left() + r.width() * x };
            ui.painter().text(pos2(px, r.center().y), Align2::LEFT_CENTER, h, bold(12.0), pal().header_secondary);
        }
        for e in list {
            let id = e.id;
            let edited = app.server.as_ref().and_then(|s| s.pages.emoji.names.get(&id)).cloned();
            let resp = row(ui, 56.0, |ui, r, hot| {
                let ir = Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 16.0), vec2(32.0, 32.0));
                egui::Image::from_uri(crate::markdown::emoji_url(id, e.animated)).show_loading_spinner(false).paint_at(ui, ir);
                if let Some((n, a)) = owners.get(&id) {
                    let x = r.left() + r.width() * 0.58;
                    if !a.is_empty() {
                        crate::ui::avatar(ui, Rect::from_min_size(pos2(x, r.center().y - 12.0), vec2(24.0, 24.0)), a);
                    }
                    ui.painter().text(pos2(x + 32.0, r.center().y), Align2::LEFT_CENTER, n, medium(14.0), pal().text_normal);
                }
                if hot {
                    icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), RED);
                }
            });
            let field = Rect::from_min_size(pos2(resp.rect.left() + resp.rect.width() * 0.18, resp.rect.center().y - 16.0), vec2(resp.rect.width() * 0.3, 32.0));
            let mut name = edited.clone().unwrap_or_else(|| e.name.clone());
            let te = ui.put(field, egui::TextEdit::singleline(&mut name).font(medium(16.0)).text_color(pal().text_normal).margin(egui::Margin::symmetric(8, 6)));
            if name != e.name {
                if let Some(s) = &mut app.server {
                    s.pages.emoji.names.insert(id, name.clone());
                }
            }
            if te.lost_focus() && name != e.name {
                let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if clean.len() >= 2 {
                    rename = Some((id, clean));
                }
                if let Some(s) = &mut app.server {
                    s.pages.emoji.names.remove(&id);
                }
            }
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x > resp.rect.right() - 40.0) {
                delete = Some((id, e.name.clone()));
            }
        }
        ui.add_space(24.0);
    }
    if let Some((id, name)) = rename {
        let was = app.store.guilds.get(&gid).and_then(|g| g.emojis.iter().find(|e| e.id == id)).map(|e| e.name.clone()).unwrap_or_default();
        app.rest("PATCH", format!("/guilds/{gid}/emojis/{id}"), Some(json!({ "name": name })), Tag::rollback(move |app: &mut App| {
            if let Some(e) = app.store.guilds.get_mut(&gid).and_then(|g| g.emojis.iter_mut().find(|e| e.id == id)) {
                e.name = was.clone();
            }
        }));
        if let Some(e) = app.store.guilds.get_mut(&gid).and_then(|g| g.emojis.iter_mut().find(|e| e.id == id)) {
            e.name = name;
        }
    }
    if let Some((id, name)) = delete {
        app.confirm = Some(crate::ui::Confirm::new("Delete Emoji", format!("Are you sure you want to delete :{name}:?"), "Delete", move |app: &mut App| {
            let kept = app.store.guilds.get(&gid).map(|g| g.emojis.clone()).unwrap_or_default();
            app.rest("DELETE", format!("/guilds/{gid}/emojis/{id}"), None, Tag::rollback(move |app: &mut App| {
                if let Some(g) = app.store.guilds.get_mut(&gid) {
                    g.emojis = kept.clone();
                }
            }));
            if let Some(g) = app.store.guilds.get_mut(&gid) {
                g.emojis.retain(|e| e.id != id);
            }
        }));
    }
}

const TIERS: [(u32, u32, u32); 3] = [(1, 2, 15), (2, 7, 30), (3, 14, 60)];

pub fn stickers(app: &mut App, ui: &mut Ui, gid: Id) {
    title(ui, "Stickers", "");
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    if let Some(s) = app.server.as_mut().filter(|s| s.pages.emoji.synced != g.stickers.len()) {
        s.pages.emoji.synced = g.stickers.len();
        s.data.remove("stickers");
    }
    let list: Vec<Value> = app.server_data("stickers", format!("/guilds/{gid}/stickers")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let used = list.len().max(g.stickers.len());
    let mut boost = false;
    let mut buy_level = false;
    if g.tier < 3 {
        let (go, learn) = super::stickline::banner(ui, TIERS[g.tier as usize].0);
        boost |= go;
        if learn {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360028038352"));
        }
        ui.add_space(24.0);
        rule(ui);
        ui.add_space(24.0);
    }
    let slot = ui.painter().add(egui::Shape::Noop);
    let rail_x = ui.cursor().left() + 12.0;
    let mut nodes = Vec::new();
    let free_y = std::cell::Cell::new(0.0f32);
    let mut act: Option<(&str, Value)> = None;
    egui::Frame::new().inner_margin(egui::Margin { left: 36, ..Default::default() }).show(ui, |ui| {
        if g.tier == 0 {
            nodes.push(super::stickline::Node { y: ui.cursor().top() + 10.0, reached: true });
            ui.label(RichText::new("No Server Boost").font(medium(16.0)).color(pal().header_primary));
            ui.label(RichText::new("No one has bestowed Boosts to this server yet. See if any members would kindly bless your server for server-wide Boost Perks!").font(reg(14.0)).color(pal().text_normal));
            ui.add_space(16.0);
        }
        let free = 5usize;
        let card_top = ui.cursor().top();
        egui::Frame::new().fill(pal().bg_primary).stroke(Stroke::new(1.0, faint(41))).corner_radius(8).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Free Slots").font(semi(16.0)).color(pal().header_primary));
                    ui.label(RichText::new(format!("{} of {free} slots available", free.saturating_sub(used))).font(reg(12.0)).color(pal().header_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if settings::primary(ui, "Upload Sticker").clicked()
                        && let Some(s) = &mut app.server
                    {
                        s.pages.emoji.form = Some(StickerForm { id: 0, name: String::new(), tags: String::new(), description: String::new(), file: None });
                    }
                });
            });
            ui.add_space(16.0);
            let w = ui.available_width();
            let per = ((w + 16.0) / 152.0).floor().max(1.0) as usize;
            let cells: Vec<Option<Value>> = list.iter().cloned().map(Some).chain(std::iter::repeat(None)).take(list.len().max(free)).collect();
            for chunk in cells.chunks(per) {
                let (r, _) = ui.allocate_exact_size(vec2(w, 176.0), Sense::hover());
                for (i, st) in chunk.iter().enumerate() {
                    let t = Rect::from_min_size(pos2(r.left() + i as f32 * 152.0, r.top()), vec2(136.0, 164.0));
                    let Some(st) = st else {
                        ui.painter().rect_stroke(t, CornerRadius::same(8), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
                        icons::draw(ui.painter(), Icon::Sticker, Rect::from_center_size(t.center(), vec2(48.0, 48.0)), faint(61));
                        continue;
                    };
            let resp = ui.interact(t, ui.id().with(("sticker", i, sid(&st["id"]))), Sense::hover());
            ui.painter().rect(t, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
            let (id, fmt) = (sid(&st["id"]), st["format_type"].as_u64().unwrap_or(1));
            if fmt != 3 {
                let ext = if fmt == 4 { "gif" } else { "png" };
                egui::Image::from_uri(format!("{}/stickers/{id}.{ext}?size=160", crate::api::media())).show_loading_spinner(false).paint_at(ui, Rect::from_min_size(t.min + vec2(24.0, 16.0), vec2(88.0, 88.0)));
            }
            crate::ui::text::paint(ui.painter(), pos2(t.center().x, t.bottom() - 40.0), Align2::CENTER_CENTER, st["name"].as_str().unwrap_or_default(), semi(14.0), pal().header_primary, 120.0);
            crate::ui::text::paint(ui.painter(), pos2(t.center().x, t.bottom() - 20.0), Align2::CENTER_CENTER, st["tags"].as_str().map(|t| format!(":{t}:")).unwrap_or_default().as_str(), reg(12.0), pal().header_secondary, 120.0);
            if resp.hovered() || ui.rect_contains_pointer(t) {
                for (k, (icon, what)) in [(Icon::Pencil, "edit"), (Icon::Trash, "delete")].into_iter().enumerate() {
                    let b = Rect::from_center_size(pos2(t.right() - 20.0 - k as f32 * 32.0, t.top() + 20.0), vec2(28.0, 28.0));
                    let br = ui.interact(b, ui.id().with(("sticker_btn", id, k)), Sense::click());
                    ui.painter().circle_filled(b.center(), 14.0, crate::ui::motion::mix(pal().bg_floating, faint(81), crate::ui::motion::hot(ui.ctx(), &br)));
                    icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(16.0, 16.0)), if what == "delete" { RED } else { pal().header_primary });
                    if br.clicked() {
                        act = Some((what, st.clone()));
                    }
                }
            }
        }
    }
        });
        free_y.set(card_top + 33.0);
        nodes.push(super::stickline::Node { y: card_top + 33.0, reached: true });
        for (lvl, _, total) in TIERS {
            ui.add_space(20.0);
            let before = if lvl == 1 { free as u32 } else { TIERS[lvl as usize - 2].2 };
            let adds = if lvl == 1 { format!("+{} Sticker Slots", total - before) } else { format!("+{} Sticker Slots ({total} total)", total - before) };
            let (y, buy) = super::stickline::level(ui, lvl, TIERS[lvl as usize - 1].1, adds, g.tier >= lvl as u8);
            nodes.push(super::stickline::Node { y, reached: g.tier >= lvl as u8 });
            buy_level |= buy;
        }
    });
    super::stickline::rail(ui, slot, rail_x, &nodes);
    super::stickline::check(ui, pos2(rail_x, free_y.get()));
    if buy_level {
        app.open_buy(crate::ui::buy::What::Boost(gid));
    }
    if boost {
        app.server = None;
        app.ui.guild = Some(gid);
        app.open_boosts(gid);
        return;
    }
    match act {
        Some(("edit", st)) => {
            let f = StickerForm { id: sid(&st["id"]), name: st["name"].as_str().unwrap_or_default().into(), tags: st["tags"].as_str().unwrap_or_default().into(), description: st["description"].as_str().unwrap_or_default().into(), file: None };
            if let Some(s) = &mut app.server {
                s.pages.emoji.form = Some(f);
            }
        }
        Some((_, st)) => {
            let (id, name) = (sid(&st["id"]), st["name"].as_str().unwrap_or_default().to_string());
            app.confirm = Some(crate::ui::Confirm::new("Delete Sticker", format!("Are you sure you want to delete **{name}**? This cannot be undone."), "Delete", move |app: &mut App| {
                app.rest("DELETE", format!("/guilds/{gid}/stickers/{id}"), None, Tag::rollback(crate::ui::failed::stale("stickers")));
                if let Some(s) = &mut app.server {
                    s.data.remove("stickers");
                }
            }));
        }
        None => {}
    }
    sticker_form(app, ui, gid);
}

fn sticker_form(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(mut f) = app.server.as_mut().and_then(|s| s.pages.emoji.form.take()) else { return };
    let (mut done, mut save) = (false, false);
    let m = crate::ui::motion::Modal::new(egui::Id::new("sticker_form")).show(ui.ctx(), |ui| {
        ui.set_width(440.0);
        ui.label(RichText::new(if f.id == 0 { "Upload Sticker" } else { "Edit Sticker" }).font(bold(20.0)).color(pal().header_primary));
        ui.add_space(16.0);
        if f.id == 0 {
            settings::h5(ui, "File");
            ui.horizontal(|ui| {
                if settings::secondary(ui, "Choose File").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Stickers", &["png", "apng", "gif", "json"]).pick_file()
                {
                    if f.name.is_empty() {
                        f.name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    }
                    f.file = Some(p);
                }
                ui.label(RichText::new(f.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "PNG, APNG, GIF or Lottie JSON, up to 512 KB".into())).font(reg(14.0)).color(pal().header_secondary));
            });
            ui.add_space(12.0);
        }
        settings::h5(ui, "Sticker Name");
        settings::input(ui, &mut f.name, "Sticker name", false);
        ui.add_space(12.0);
        settings::h5(ui, "Related Emoji");
        settings::input(ui, &mut f.tags, "Type a related emoji or word", false);
        ui.add_space(12.0);
        settings::h5(ui, "Description (optional)");
        settings::input(ui, &mut f.description, "Describe the sticker", false);
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            let ok = f.name.trim().len() >= 2 && !f.tags.trim().is_empty() && (f.id != 0 || f.file.is_some());
            if ui.add_enabled_ui(ok, |ui| settings::primary(ui, if f.id == 0 { "Upload" } else { "Save" })).inner.clicked() {
                save = true;
                done = true;
            }
            if settings::secondary(ui, "Cancel").clicked() {
                done = true;
            }
        });
    });
    if m.should_close() || ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {

        done = true;
    }
    if save {
        if f.id == 0 {
            if let Some(file) = f.file.take() {
                app.net.send(Cmd::Sticker { guild: gid, name: f.name.trim().into(), tags: f.tags.trim().into(), description: f.description.trim().into(), file });
            }
        } else {
            let body = json!({ "name": f.name.trim(), "tags": f.tags.trim(), "description": f.description.trim() });
            app.rest("PATCH", format!("/guilds/{gid}/stickers/{}", f.id), Some(body.clone()), Tag::rollback(crate::ui::failed::stale("stickers")));
            if let Some(Value::Array(a)) = app.server.as_mut().and_then(|s| s.data.get_mut("stickers")) {
                for st in a.iter_mut().filter(|st| sid(&st["id"]) == f.id) {
                    for k in ["name", "tags", "description"] {
                        st[k] = body[k].clone();
                    }
                }
            }
        }
    }
    if !done && let Some(s) = &mut app.server {
        s.pages.emoji.form = Some(f);
    }
}
