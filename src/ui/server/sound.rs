//! Soundboard: the server's sounds, with upload, edit (name, volume, emoji) and delete.

use egui::{Align2, Rect, RichText, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::path::PathBuf;

use super::super::App;
use super::super::create::{footer, label, modal};
use super::super::icons::{self, Icon};
use super::form::{self, Pick};
use super::{faint, row, title};
use crate::net::Tag;
use crate::state::{Id as Snow, sid};
use crate::theme::*;

pub struct Form {
    pub edit: Option<Snow>,
    pub name: String,
    pub volume: f32,
    pub emoji: Pick,
    pub file: Option<PathBuf>,
}

fn slots(tier: u8) -> usize {
    match tier {
        1 => 24,
        2 => 36,
        3 => 48,
        _ => 8,
    }
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let max = slots(g.tier);
    title(ui, "Soundboard", &format!("Upload up to {max} custom sound reactions that anyone in this server can use. Nitro members will be able to access these sounds in any server on Discord."));
    let Some(list) = app.server_data("sounds", format!("/guilds/{gid}/soundboard-sounds")).cloned() else {
        ui.add(egui::Spinner::new().color(pal().text_muted));
        return;
    };
    let items: Vec<Value> = list["items"].as_array().or(list.as_array()).cloned().unwrap_or_default();
    if form::button(ui, "Upload Sound", true).clicked() {
        app.server.as_mut().unwrap().sound = Some(Form { edit: None, name: String::new(), volume: 1.0, emoji: (String::new(), 0, false), file: None });
    }
    form::rule(ui);
    ui.label(RichText::new(format!("Soundboard Slots \u{2014} {} of {max} used", items.len())).font(bold(12.0)).color(pal().header_secondary));
    if g.tier < 3 {
        ui.add_space(4.0);
        let next = g.tier + 1;
        form::small(ui, &format!("Boost your server to Level {next} to unlock {} more soundboard slots!", slots(next) - max));
    }
    ui.add_space(8.0);
    let mut play = None;
    let mut edit = None;
    let mut del = None;
    if items.is_empty() {
        ui.add_space(8.0);
        form::small(ui, "Get the party started by uploading a sound");
    }
    for s in &items {
        let name = s["name"].as_str().unwrap_or_default().to_string();
        let id = sid(&s["sound_id"]);
        let by = s["user"]["global_name"].as_str().or(s["user"]["username"].as_str()).map(str::to_string);
        let emoji: Pick = (s["emoji_name"].as_str().unwrap_or_default().to_string(), sid(&s["emoji_id"]), false);
        let resp = row(ui, 52.0, |ui, r, hot| {
            let c = pos2(r.left() + 24.0, r.center().y);
            ui.painter().circle_filled(c, 14.0, faint(31));
            icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(c, vec2(14.0, 14.0)), pal().header_primary);
            form::paint_emoji(ui, Rect::from_center_size(pos2(r.left() + 62.0, r.center().y), vec2(24.0, 24.0)), &emoji);
            let t = ui.painter().text(pos2(r.left() + 84.0, r.center().y), Align2::LEFT_CENTER, &name, medium(16.0), pal().header_primary);
            if let Some(b) = &by {
                ui.painter().text(pos2(t.right() + 12.0, r.center().y), Align2::LEFT_CENTER, format!("Uploaded by {b}"), reg(14.0), pal().header_secondary);
            }
            if hot {
                icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(pos2(r.right() - 52.0, r.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), RED);
            }
        });
        super::super::settings::mark(ui, &format!("sound:{name}"), resp.rect);
        if resp.clicked() {
            let x = resp.interact_pointer_pos().map_or(0.0, |p| p.x);
            if x > resp.rect.right() - 36.0 {
                del = Some((id, name.clone()));
            } else if x > resp.rect.right() - 68.0 {
                edit = Some(Form { edit: Some(id), name: name.clone(), volume: s["volume"].as_f64().unwrap_or(1.0) as f32, emoji, file: None });
            } else if x < resp.rect.left() + 44.0 {
                play = Some(id);
            } else {
                edit = Some(Form { edit: Some(id), name: name.clone(), volume: s["volume"].as_f64().unwrap_or(1.0) as f32, emoji, file: None });
            }
        }
    }
    if let Some(id) = play {
        app.play_audio(format!("{}/soundboard-sounds/{id}", crate::api::cdn()));
    }
    if let Some(f) = edit {
        app.server.as_mut().unwrap().sound = Some(f);
    }
    if let Some((id, name)) = del {
        app.confirm = Some(super::super::Confirm::new("Delete Soundboard Sound", format!("Are you sure you want to delete **{name}**? This cannot be undone."), "Delete", move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/soundboard-sounds/{id}"), None, Tag::Server("sounds_done"));
        }));
    }
}

pub fn modal_form(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.server.as_ref() else { return };
    let gid = s.guild;
    let Some(mut f) = app.server.as_mut().and_then(|s| s.sound.take()) else { return };
    let gname = app.store.guilds.get(&gid).map(|g| g.name.clone()).unwrap_or_default();
    let mut done = false;
    let close = modal(ctx, "sound_form", Some(if f.edit.is_some() { "Edit Sound" } else { "Upload a Sound" }), |ui| {
        if f.edit.is_none() {
            ui.label(RichText::new(format!("Upload sound for {gname}")).font(reg(14.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            label(ui, "Sound");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if form::button(ui, "Choose File", true).clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Audio", &["mp3", "ogg", "wav"]).pick_file()
                {
                    if f.name.is_empty() {
                        f.name = p.file_stem().map(|s| s.to_string_lossy().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ' ').take(32).collect()).unwrap_or_default();
                    }
                    f.file = Some(p);
                }
                let shown = f.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "No sound selected".into());
                ui.label(RichText::new(shown).font(reg(14.0)).color(pal().header_secondary));
            });
            ui.add_space(16.0);
        }
        label(ui, "Sound Name");
        super::super::create::field(ui, &mut f.name, "Name your sound", None, 1);
        f.name.truncate(32);
        ui.add_space(16.0);
        label(ui, "Emoji");
        ui.horizontal(|ui| {
            if let Some(p) = form::emoji_button(ui, app, gid, &f.emoji) {
                f.emoji = p;
            }
        });
        ui.add_space(16.0);
        label(ui, "Sound Volume");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let mut pct = f.volume * 100.0;
            super::super::settings::slider(ui, 330.0, &mut pct, 0.0, 100.0);
            f.volume = pct / 100.0;
            ui.label(RichText::new(format!("{}%", pct.round() as u32)).font(medium(14.0)).color(pal().text_normal));
        });
        let ready = f.name.trim().len() >= 2 && (f.edit.is_some() || f.file.is_some());
        let (cancel, go) = footer(ui, if f.edit.is_some() { "Save" } else { "Upload" }, ready);
        if cancel {
            done = true;
        }
        if go && ready {
            let mut body = json!({ "name": f.name.trim(), "volume": f.volume });
            match &f.emoji {
                (n, 0, _) if !n.is_empty() => {
                    body["emoji_name"] = json!(n);
                    body["emoji_id"] = Value::Null;
                }
                (n, id, _) if *id != 0 => {
                    body["emoji_id"] = json!(id.to_string());
                    body["emoji_name"] = json!(n);
                }
                _ => {
                    body["emoji_name"] = Value::Null;
                    body["emoji_id"] = Value::Null;
                }
            }
            match (f.edit, &f.file) {
                (Some(id), _) => app.rest("PATCH", format!("/guilds/{gid}/soundboard-sounds/{id}"), Some(body), Tag::Server("sounds_done")),
                (None, Some(p)) => {
                    if let Some(uri) = form::data_uri(p) {
                        if uri.len() > 700_000 {
                            app.toast("That sound is too big. It has to be under 512 KB.");
                        } else {
                            body["sound"] = json!(uri);
                            app.rest("POST", format!("/guilds/{gid}/soundboard-sounds"), Some(body), Tag::Server("sounds_done"));
                        }
                    }
                }
                _ => {}
            }
            done = true;
        }
    });
    if !(close || done) {
        app.server.as_mut().unwrap().sound = Some(f);
    }
}
