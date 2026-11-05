//! Server folders: the account's guild_folders setting, reordered from the rail by drag or
//! Ctrl+D and edited from the folder's menu.

use base64::Engine;
use egui::{Align2, Rect, Sense, pos2, vec2};
use serde_json::json;

use super::App;
use super::create::{field, label, modal, plate};
use crate::net::Tag;
use crate::state::{Folder, Id};
use crate::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Src {
    Guild(Id),
    Folder(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drop {
    Before(usize),
    Combine(usize),
    In(usize, usize),
}

#[derive(Default)]
pub struct State {
    pub edit: Option<Edit>,
    pub pick: Option<(Id, Vec<Folder>)>,
}

pub struct Edit {
    id: Id,
    name: String,
    color: Option<u32>,
}

const COLORS: [u32; 20] = [
    0x1abc9c, 0x2ecc71, 0x3498db, 0x9b59b6, 0xe91e63, 0xf1c40f, 0xe67e22, 0xe74c3c, 0x95a5a6, 0x607d8b, 0x11806a, 0x1f8b4c, 0x206694, 0x71368a, 0xad1457, 0xc27c0e, 0xa84300,
    0x992d22, 0x979c9f, 0x546e7a,
];

fn locate(f: &[Folder], g: Id) -> Option<(usize, usize)> {
    f.iter().enumerate().find_map(|(i, x)| x.guilds.iter().position(|y| *y == g).map(|p| (i, p)))
}

fn fresh_id() -> Id {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() as u64 ^ d.as_secs());
    (t & 0x7fff_ffff) as Id | 1
}

pub fn apply(f: &mut Vec<Folder>, src: Src, d: Drop) -> bool {
    match src {
        Src::Folder(s) => {
            let Drop::Before(i) = d else { return false };
            if s >= f.len() || i == s || i == s + 1 {
                return false;
            }
            let item = f.remove(s);
            f.insert(if i > s { i - 1 } else { i }, item);
            true
        }
        Src::Guild(g) => {
            let Some((s, p)) = locate(f, g) else { return false };
            if matches!(d, Drop::Combine(i) if i == s) {
                return false;
            }
            let left = f[s].guilds.len() == 1;
            if left && matches!(d, Drop::In(i, _) if i == s) {
                return false;
            }
            if left && f[s].id == 0 && matches!(d, Drop::Before(i) if i == s || i == s + 1) {
                return false;
            }
            f[s].guilds.remove(p);
            let gone = f[s].guilds.is_empty();
            if gone {
                f.remove(s);
            }
            let fix = |i: usize| if gone && i > s { i - 1 } else { i };
            match d {
                Drop::Before(i) => f.insert(fix(i).min(f.len()), Folder { guilds: vec![g], ..Default::default() }),
                Drop::Combine(i) => {
                    let t = &mut f[fix(i)];
                    if t.id == 0 && t.guilds.len() == 1 {
                        t.id = fresh_id();
                    }
                    t.guilds.push(g);
                }
                Drop::In(i, pos) => {
                    let i = fix(i);
                    let pos = if i == s && pos > p { pos - 1 } else { pos };
                    let t = &mut f[i];
                    t.guilds.insert(pos.min(t.guilds.len()), g);
                }
            }
            true
        }
    }
}

pub fn current(app: &App) -> Vec<Folder> {
    if app.store.folders.is_empty() {
        app.store.guild_order.iter().map(|g| Folder { guilds: vec![*g], ..Default::default() }).collect()
    } else {
        app.store.folders.clone()
    }
}

pub fn save(app: &mut App, folders: Vec<Folder>) {
    let raw = crate::proto::folders_update(&folders);
    let seen = app.store.guild_order.clone();
    app.store.arrange(folders, &seen);
    let body = json!({ "settings": base64::engine::general_purpose::STANDARD.encode(raw) });
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(body), Tag::Quiet);
}

pub fn move_folder(app: &mut App, id: Id, last: bool) {
    let mut f = current(app);
    let Some(i) = f.iter().position(|x| x.id == id) else { return };
    let to = if last { f.len() } else { 0 };
    if apply(&mut f, Src::Folder(i), Drop::Before(to)) {
        save(app, f);
    }
}

pub fn settings(app: &mut App, id: Id) {
    let Some(f) = app.store.folders.iter().find(|f| f.id == id) else { return };
    app.folders.edit = Some(Edit { id, name: f.name.clone().unwrap_or_default(), color: f.color });
}

pub fn drop(app: &mut App, src: Src, d: Drop) {
    let mut f = current(app);
    if apply(&mut f, src, d) {
        save(app, f);
    }
}

/// Ctrl+D picks up the open server, the arrows walk it through the rail, Enter sets it down and
/// Escape puts it back.
pub fn keys(app: &mut App, ctx: &egui::Context) {
    use egui::Key;
    let Some((g, backup)) = app.folders.pick.clone() else {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::D))
            && let Some(g) = app.ui.guild
        {
            app.folders.pick = Some((g, current(app)));
        }
        return;
    };
    let mut f = current(app);
    let step = ctx.input(|i| i.key_pressed(Key::ArrowDown) as i32 - i.key_pressed(Key::ArrowUp) as i32);
    if step != 0
        && let Some((i, p)) = locate(&f, g)
    {
        let d = if f[i].guilds.len() > 1 || f[i].id != 0 {
            let to = p as i32 + step;
            (0..f[i].guilds.len() as i32).contains(&to).then_some(Drop::In(i, if step > 0 { p + 2 } else { p - 1 }))
        } else if step > 0 {
            (i + 1 < f.len()).then_some(Drop::Before(i + 2))
        } else {
            (i > 0).then_some(Drop::Before(i - 1))
        };
        if let Some(d) = d
            && apply(&mut f, Src::Guild(g), d)
        {
            let seen = app.store.guild_order.clone();
            app.store.arrange(f.clone(), &seen);
        }
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter) || i.consume_key(egui::Modifiers::NONE, Key::Space)) {
        app.folders.pick = None;
        save(app, f);
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        app.folders.pick = None;
        let seen = app.store.guild_order.clone();
        app.store.arrange(backup, &seen);
    }
}

/// Folder Settings: the name and the colour, saved with Done.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(e) = app.folders.edit.as_mut() else { return };
    let mut done = false;
    let close = modal(ctx, "folder_settings", Some("Folder Settings"), |ui| {
        label(ui, "Folder Name");
        field(ui, &mut e.name, "Server Folder", None, 1);
        e.name = e.name.chars().take(32).collect();
        ui.add_space(20.0);
        label(ui, "Folder Color");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for c in std::iter::once(None).chain(COLORS.iter().map(|c| Some(*c))) {
                let big = c.is_none();
                let (r, resp) = ui.allocate_exact_size(vec2(if big { 68.0 } else { 40.0 }, if big { 56.0 } else { 26.0 }), Sense::click());
                let fill = c.map_or(BLURPLE, role_color);
                ui.painter().rect_filled(r, 4, fill);
                if e.color == c {
                    ui.painter().rect_stroke(r.expand(2.0), 6, egui::Stroke::new(2.0, pal().header_primary), egui::StrokeKind::Outside);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "✓", semi(16.0), egui::Color32::WHITE);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    e.color = c;
                }
            }
        });
        ui.add_space(24.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        done = plate(ui, Rect::from_min_size(pos2(r.right() - 96.0, r.top()), vec2(96.0, 40.0)), "Done", true, true);
    });
    if done || close {
        let Some(e) = app.folders.edit.take() else { return };
        if done {
            let mut f = current(app);
            if let Some(x) = f.iter_mut().find(|x| x.id == e.id) {
                x.name = Some(e.name).filter(|n| !n.is_empty());
                x.color = e.color;
            }
            save(app, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lone(g: Id) -> Folder {
        Folder { guilds: vec![g], ..Default::default() }
    }

    fn rail(f: &[Folder]) -> Vec<Vec<Id>> {
        f.iter().map(|x| x.guilds.clone()).collect()
    }

    #[test]
    fn reorder_guilds() {
        let mut f = vec![lone(1), lone(2), lone(3)];
        assert!(apply(&mut f, Src::Guild(1), Drop::Before(3)));
        assert_eq!(rail(&f), [vec![2], vec![3], vec![1]]);
        assert!(!apply(&mut f, Src::Guild(1), Drop::Before(3)));
        assert!(apply(&mut f, Src::Guild(1), Drop::Before(0)));
        assert_eq!(rail(&f), [vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn combine_makes_a_folder() {
        let mut f = vec![lone(1), lone(2), lone(3)];
        assert!(apply(&mut f, Src::Guild(1), Drop::Combine(2)));
        assert_eq!(rail(&f), [vec![2], vec![3, 1]]);
        assert!(f[1].id != 0);
        assert!(apply(&mut f, Src::Guild(2), Drop::Combine(1)));
        assert_eq!(rail(&f), [vec![3, 1, 2]]);
    }

    #[test]
    fn drag_out_of_a_folder() {
        let mut f = vec![lone(1), Folder { id: 7, guilds: vec![2, 3], ..Default::default() }];
        assert!(apply(&mut f, Src::Guild(3), Drop::Before(0)));
        assert_eq!(rail(&f), [vec![3], vec![1], vec![2]]);
        assert_eq!(f[2].id, 7);
    }

    #[test]
    fn folder_empties_out() {
        let mut f = vec![Folder { id: 7, guilds: vec![2], ..Default::default() }, lone(1)];
        assert!(apply(&mut f, Src::Guild(2), Drop::Combine(1)));
        assert_eq!(rail(&f), [vec![1, 2]]);
    }

    #[test]
    fn reorder_inside_a_folder() {
        let mut f = vec![Folder { id: 7, guilds: vec![1, 2, 3], ..Default::default() }];
        assert!(apply(&mut f, Src::Guild(1), Drop::In(0, 3)));
        assert_eq!(f[0].guilds, [2, 3, 1]);
        assert!(apply(&mut f, Src::Guild(1), Drop::In(0, 0)));
        assert_eq!(f[0].guilds, [1, 2, 3]);
    }

    #[test]
    fn move_a_folder() {
        let mut f = vec![lone(1), Folder { id: 7, guilds: vec![2, 3], ..Default::default() }, lone(4)];
        assert!(apply(&mut f, Src::Folder(1), Drop::Before(0)));
        assert_eq!(rail(&f), [vec![2, 3], vec![1], vec![4]]);
        assert!(apply(&mut f, Src::Folder(0), Drop::Before(3)));
        assert_eq!(rail(&f), [vec![1], vec![4], vec![2, 3]]);
    }
}
