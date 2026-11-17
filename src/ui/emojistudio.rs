use std::path::PathBuf;

use egui::{Align2, Color32, CornerRadius, Frame, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{App, text};
use crate::net::Tag;
use crate::state::{CREATE_GUILD_EXPRESSIONS, Id, MANAGE_GUILD_EXPRESSIONS};
use crate::theme::*;

const KEY: &str = "emoji_studio";
const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

pub struct Studio {
    path: PathBuf,
    image: String,
    name: String,
    guild: Option<Id>,
    busy: bool,
    error: Option<String>,
}

pub fn name_of(stem: &str) -> String {
    let mut s: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(32).collect();
    while s.len() < 2 {
        s.push('_');
    }
    s
}

fn can_add(app: &App, gid: Id) -> bool {
    app.store.my_perms(gid) & (CREATE_GUILD_EXPRESSIONS | MANAGE_GUILD_EXPRESSIONS) != 0
}

pub fn open(app: &mut App, guild: Id) {
    let Some(path) = rfd::FileDialog::new().add_filter("Images", &IMAGES).pick_file() else { return };
    let Some(image) = super::server::form::data_uri(&path) else { return };
    let name = name_of(&path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    let guild = Some(guild).filter(|g| *g != 0 && can_add(app, *g));
    app.keyed.remove(KEY);
    app.emoji_studio = Some(Studio { path, image, name, guild, busy: false, error: None });
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.emoji_studio.as_ref() else { return };
    if s.busy
        && let Some(Some(res)) = app.keyed.remove(KEY)
    {
        match res {
            Ok(v) => {
                let gname = s.guild.and_then(|g| app.store.guilds.get(&g)).map(|g| g.name.clone()).unwrap_or_default();
                let said = format!(":{}: added to {gname}", v["name"].as_str().unwrap_or(&s.name));
                app.emoji_studio = None;
                app.toast(said);
                return;
            }
            Err(e) => {
                let s = app.emoji_studio.as_mut().unwrap();
                s.busy = false;
                s.error = Some(e);
            }
        }
    }
    let servers: Vec<(Id, String)> = app
        .store
        .guild_order
        .iter()
        .filter_map(|g| app.store.guilds.get(g))
        .filter(|g| can_add(app, g.id))
        .map(|g| (g.id, g.name.clone()))
        .collect();
    let Some(s) = app.emoji_studio.as_mut() else { return };
    let uri = format!("file:///{}", s.path.display().to_string().replace('\\', "/"));
    let (mut close, mut finish, mut pick) = (false, false, None);
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 51)));
    let m = super::motion::Modal::new("emoji_studio".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (all, _) = ui.allocate_exact_size(vec2(336.0 + 312.0, 412.0), Sense::hover());
        let editor = Rect::from_min_size(all.min, vec2(336.0, all.height()));
        let aside = Rect::from_min_max(pos2(editor.right(), all.top()), all.max);
        ui.painter().rect_filled(editor, CornerRadius { nw: 12, sw: 12, ne: 0, se: 0 }, pal().bg_tertiary);
        ui.painter().text(editor.left_top() + vec2(24.0, 36.0), Align2::LEFT_CENTER, "Add Emoji", medium(20.0), pal().header_primary);
        let board = Rect::from_min_size(pos2(editor.left() + 24.0, editor.top() + 76.0), vec2(288.0, 288.0));
        checkerboard(ui, board);
        egui::Image::from_uri(uri.clone()).maintain_aspect_ratio(true).fit_to_exact_size(board.size()).paint_at(ui, fit(ui, &uri, board));
        let x = Rect::from_center_size(pos2(all.right() - 28.0, all.top() + 36.0), vec2(24.0, 24.0));
        let xr = ui.interact(x, ui.id().with("studio_close"), Sense::click());
        super::icons::draw(ui.painter(), super::icons::Icon::Close, x, super::motion::mix(pal().header_secondary, pal().interactive_hover, super::motion::hot(ui.ctx(), &xr)));
        close = xr.clicked();

        let mut col = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(aside.min + vec2(24.0, 76.0), aside.max - vec2(24.0, 24.0))));
        let ui = &mut col;
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let w = ui.available_width();
        label(ui, "Preview");
        let (row, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
        let pill = Rect::from_min_size(pos2(row.left(), row.center().y - 15.0), vec2(56.0, 30.0));
        ui.painter().rect(pill, CornerRadius::same(8), pal().bg_mod_selected, Stroke::new(1.0, pal().blurple), egui::StrokeKind::Inside);
        egui::Image::from_uri(uri.clone()).paint_at(ui, Rect::from_center_size(pos2(pill.left() + 18.0, pill.center().y), vec2(20.0, 20.0)));
        ui.painter().text(pos2(pill.left() + 34.0, pill.center().y), Align2::LEFT_CENTER, "6", semi(14.0), pal().header_primary);
        egui::Image::from_uri(uri).paint_at(ui, Rect::from_min_size(pos2(pill.right() + 16.0, row.center().y - 24.0), vec2(48.0, 48.0)));
        ui.add_space(20.0);
        label(ui, "Emoji name");
        super::settings::input(ui, &mut s.name, "", false);
        ui.add_space(20.0);
        label(ui, "Upload to");
        let shown = s.guild.and_then(|g| servers.iter().find(|x| x.0 == g)).map_or("Select a server", |x| x.1.as_str()).to_string();
        super::settings::dropdown(ui, w, None, &shown, |ui| {
            for (id, name) in &servers {
                if super::menu_item(ui, None, name, false) {
                    pick = Some(*id);
                }
            }
        });
        ui.add_space(20.0);
        if let Some(e) = &s.error {
            let g = ui.painter().layout(e.clone(), reg(14.0), pal().red, w);
            let (r, _) = ui.allocate_exact_size(vec2(w, g.size().y), Sense::hover());
            ui.painter().galley(r.min, g, pal().red);
            ui.add_space(8.0);
        }
        let ready = s.guild.is_some() && s.name.chars().count() >= 2 && !s.busy;
        let (b, resp) = ui.allocate_exact_size(vec2(w, 40.0), if ready { Sense::click() } else { Sense::hover() });
        let fade = if ready { 1.0 } else { 0.5 };
        let fill = if resp.hovered() && ready { Color32::from_rgb(0x44, 0x52, 0xbb) } else { pal().blurple };
        ui.painter().rect_filled(b, CornerRadius::same(8), fill.gamma_multiply(fade));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Finish", medium(16.0), Color32::WHITE.gamma_multiply(fade));
        super::settings::mark(ui, "studio:finish", b);
        finish = resp.clicked();
    });
    if let Some(g) = pick {
        s.guild = Some(g);
        s.error = None;
    }
    if finish && let Some(g) = s.guild {
        s.name = name_of(&s.name);
        s.busy = true;
        s.error = None;
        let body = json!({ "name": s.name, "image": s.image });
        app.keyed.insert(KEY.into(), None);
        app.rest("POST", format!("/guilds/{g}/emojis"), Some(body), Tag::Keyed(KEY.into()));
    } else if close || m.should_close() {
        app.emoji_studio = None;
    }
}

fn label(ui: &mut Ui, s: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    text::paint(ui.painter(), r.left_center(), Align2::LEFT_CENTER, s, medium(14.0), pal().header_secondary, r.width());
    ui.add_space(8.0);
}

fn checkerboard(ui: &Ui, r: Rect) {
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_secondary);
    let n = (r.width() / 16.0) as usize;
    for i in 0..n {
        for j in 0..n {
            if (i + j) % 2 == 0 {
                ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(i as f32 * 16.0, j as f32 * 16.0), vec2(16.0, 16.0)), 0, pal().bg_mod_hover);
            }
        }
    }
}

fn fit(ui: &Ui, uri: &str, board: Rect) -> Rect {
    let size = egui::Image::from_uri(uri.to_string()).load_and_calc_size(ui, board.size()).unwrap_or(board.size());
    Rect::from_center_size(board.center(), size)
}

#[cfg(test)]
mod tests {
    use super::name_of;

    #[test]
    fn emoji_names_keep_word_characters_and_reach_two() {
        assert_eq!(name_of("my cool-emoji!"), "mycoolemoji");
        assert_eq!(name_of("a"), "a_");
        assert_eq!(name_of(""), "__");
        assert_eq!(name_of(&"x".repeat(40)).len(), 32);
    }
}
