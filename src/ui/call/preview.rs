//! A voice channel you're not in: Discord's glow, a card per person, the channel's name, who's
//! there and the way in.

use egui::{Color32, CornerRadius, Mesh, Rect, Ui, pos2, vec2};

use crate::state::{Channel, Id};
use crate::ui::App;

pub(super) fn backdrop(ui: &Ui, r: Rect) {
    let (dark, glow) = ([15.0, 16.0, 25.0], [78.0, 88.0, 199.0]);
    let focus = pos2(r.center().x, r.bottom());
    let reach = (r.width() / 2.0).hypot(r.height());
    let (cols, rows) = (24u32, 16u32);
    let mut m = Mesh::default();
    for j in 0..=rows {
        for i in 0..=cols {
            let p = pos2(r.left() + r.width() * i as f32 / cols as f32, r.top() + r.height() * j as f32 / rows as f32);
            let t = (1.0 - p.distance(focus) / reach).clamp(0.0, 1.0).powf(1.3);
            let c = |k: usize| (dark[k] + (glow[k] - dark[k]) * t) as u8;
            m.colored_vertex(p, Color32::from_rgb(c(0), c(1), c(2)));
        }
    }
    for j in 0..rows {
        for i in 0..cols {
            let a = j * (cols + 1) + i;
            let c = a + cols + 1;
            m.add_triangle(a, a + 1, c + 1);
            m.add_triangle(a, c + 1, c);
        }
    }
    ui.painter().add(m);
}

pub(super) fn who(app: &App, guild: Id, members: &[Id]) -> String {
    let name = |i: usize| app.store.name_in(guild, members[i]);
    match members.len() {
        0 => "No one is currently in voice".into(),
        1 => format!("{} is currently in voice", name(0)),
        2 => format!("{} and {} are currently in voice", name(0), name(1)),
        n => format!("{}, {}, and {} other{} are currently in voice", name(0), name(1), n - 2, if n == 3 { "" } else { "s" }),
    }
}

pub(super) fn cards(app: &mut App, ui: &Ui, chan: &Channel, members: &[Id], top: f32, cx: f32) {
    let n = members.len().min(6);
    let x0 = cx - (n as f32 * 168.0 - 8.0) / 2.0;
    for (k, u) in members.iter().take(n).enumerate() {
        let tile = Rect::from_min_size(pos2(x0 + k as f32 * 168.0, top), vec2(160.0, 90.0));
        let fill = super::tile_color(app, ui.ctx(), *u, chan.guild);
        ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
        app.rects.insert(format!("voice:preview:{u}"), tile);
        if let Some(user) = app.store.user(*u) {
            crate::ui::avatar(ui, Rect::from_center_size(tile.center(), vec2(40.0, 40.0)), &user.avatar_url());
        }
    }
}
