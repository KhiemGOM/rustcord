use egui::load::BytesPoll;
use egui::{CursorIcon, Rect, Ui};

use crate::cursor;

const PICTURES: [&str; 2] = ["a5a35112cf47613450476b245c5ded25838b11a09c30494ad05dd153120f0e33", "99d372f82ccdb87439920e8033c22642f7cd74ecb8520c0554003b4fb4c88e49"];

/// The collectibles shop draws its pointer from two pictures on discord's CDN: the arrow everywhere and
/// the hand over anything that takes a click (the search box included).
pub fn shop(ui: &Ui, area: Rect, on: bool) {
    let ctx = ui.ctx();
    if !on {
        return cursor::show(0);
    }
    let scale = ctx.pixels_per_point();
    if !cursor::ready(scale) {
        for (kind, name) in PICTURES.iter().enumerate() {
            match ctx.try_load_bytes(&format!("{}/assets/content/{name}.svg", crate::api::cdn())) {
                Ok(BytesPoll::Ready { bytes, .. }) => {
                    cursor::build(kind, &bytes, scale);
                }
                Ok(BytesPoll::Pending { .. }) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                Err(_) => {}
            }
        }
    }
    let inside = ctx.pointer_hover_pos().is_some_and(|p| area.contains(p));
    let mode = match ctx.output(|o| o.cursor_icon) {
        _ if !inside || !cursor::ready(scale) => 0,
        CursorIcon::PointingHand | CursorIcon::Text => 2,
        CursorIcon::Default => 1,
        _ => 0,
    };
    if mode != 0 {
        ctx.set_cursor_icon(CursorIcon::Default);
    }
    cursor::show(mode);
}
