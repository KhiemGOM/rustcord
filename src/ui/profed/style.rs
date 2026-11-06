//! Display name styles: painting a name in its font and effect, and the "Change Display Name
//! Style" dialog.

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::draft::Style;
use super::{Dlg, Editor, Swatch, faint, rgb_of, round_icon, tile};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, motion, text};

pub const FONTS: [u8; 12] = [11, 3, 4, 6, 7, 8, 10, 12, 13, 14, 15, 16];
pub const EFFECTS: [(u8, &str); 7] = [(1, "Solid"), (2, "Gradient"), (3, "Neon"), (4, "Toon"), (5, "Pop"), (8, "Gummy"), (7, "Prism")];
pub const PRESETS: [u32; 7] = [0xd9dbe1, 0x23d160, 0x1d9bf0, 0xb91dfc, 0xe3074d, 0xff3b1f, 0xc9a50f];

pub struct Edit {
    pub font: u8,
    pub effect: u8,
    pub colors: Vec<u32>,
}

impl Edit {
    pub fn from(s: Option<&Style>) -> Edit {
        match s {
            Some(s) => Edit { font: s.font, effect: s.effect, colors: s.colors.clone() },
            None => Edit { font: 11, effect: 1, colors: vec![PRESETS[0]] },
        }
    }

    pub fn style(&self) -> Style {
        Style { font: self.font, effect: self.effect, colors: self.colors.clone() }
    }

    fn wants(&self) -> usize {
        match self.effect {
            2 | 7 | 8 => 2,
            _ => 1,
        }
    }

    pub fn surprise(&mut self) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as usize);
        self.font = FONTS[t % FONTS.len()];
        self.effect = EFFECTS[(t / 13) % EFFECTS.len()].0;
        self.colors = (0..self.wants()).map(|i| PRESETS[(t / (29 + i * 7) + i) % PRESETS.len()]).collect();
    }
}

fn face(font: u8, size: f32) -> FontId {
    if crate::fonts::NAME_FONTS.iter().any(|(id, ..)| *id == font) {
        FontId::new(size, FontFamily::Name(format!("name{font}").into()))
    } else {
        semi(size)
    }
}

/// A name the way discord paints it in a profile: its font, and its effect over its colours.
#[allow(clippy::too_many_arguments)]
pub fn paint_name(ui: &Ui, at: Pos2, align: Align2, name: &str, style: Option<&Style>, size: f32, max_w: f32, animate: bool) -> Rect {
    let ink = pal().header_primary;
    let Some(s) = style else {
        return text::paint(ui.painter(), at, align, name, semi(size), ink, max_w);
    };
    let font = face(s.font, size);
    let cols: Vec<Color32> = if s.colors.is_empty() { vec![ink] } else { s.colors.iter().map(|c| role_color(*c)).collect() };
    let dark = Color32::from_rgb(0x2d, 0x2d, 0x33);
    let base = text::layout_em(ui.ctx(), name, font.clone(), Color32::WHITE, max_w, 1.0);
    let rect = align.anchor_size(at, base.size());
    let phase = if animate { (ui.ctx().input(|i| i.time) as f32 / 1.5).fract() } else { 0.0 };
    if animate && s.effect != 1 {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    let w = base.size().x.max(1.0);
    let flow = |stops: &[Color32]| {
        let mut st = stops.to_vec();
        if st.len() < 2 {
            st.push(st[0]);
        }
        st.push(st[0]);
        let n = st.len();
        move |x: f32| ramp(&st[..n], ((x / w) + phase).rem_euclid(1.0))
    };
    let painter = ui.painter();
    let tinted = |c: Color32| text::layout_em(ui.ctx(), name, font.clone(), c, max_w, 1.0);
    let ring = |c: Color32, r: f32, n: usize| {
        let t = tinted(c);
        for k in 0..n {
            let a = k as f32 * std::f32::consts::TAU / n as f32;
            t.paint(painter, rect.min + vec2(a.cos(), a.sin()) * r);
        }
    };
    match s.effect {
        2 | 7 => base.tint(flow(&cols)).paint(painter, rect.min),
        3 => {
            for (r, a) in [(6.0, 0.05), (4.0, 0.08), (2.5, 0.14)] {
                ring(cols[0].gamma_multiply(a * 4.0), r * size / 24.0, 12);
            }
            ring(cols[0], 1.0 + size * 0.04, 8);
            tinted(Color32::WHITE).paint(painter, rect.min);
        }
        4 => {
            ring(dark, 1.5, 8);
            tinted(cols[0]).paint(painter, rect.min);
        }
        5 => {
            ring(dark, 2.0, 12);
            tinted(Color32::WHITE).paint(painter, rect.min);
        }
        6 => {
            for (r, a) in [(1.5, 0.03), (3.0, 0.025), (4.5, 0.018), (6.0, 0.012)] {
                ring(cols[0].gamma_multiply(a * 10.0), r, 12);
            }
            base.tint(flow(&cols)).paint(painter, rect.min);
        }
        8 => {
            ring(dark, 1.5, 8);
            base.tint(flow(&cols)).paint(painter, rect.min);
        }
        _ => tinted(cols[0]).paint(painter, rect.min),
    }
    rect
}

fn chip(ui: &mut Ui, r: Rect, id: &str, on: bool) -> egui::Response {
    let resp = tile(ui, r, id, on);
    if on {
        ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
    }
    resp
}

pub fn dialog(app: &mut App, ed: &mut Editor, ctx: &egui::Context, user: &crate::state::User) {
    let Dlg::Style(e) = &mut ed.dlg else { return };
    let (mut apply, mut cancel, mut custom) = (false, false, None);
    let name = if ed.draft.name.is_empty() { user.display().to_string() } else { ed.draft.name.clone() };
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
    let screen = ctx.content_rect();
    let w = 980.0f32.min(screen.width() - 48.0);
    let two = w >= 760.0;
    let m = motion::Modal::new("profed_style".into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let area = ui.max_rect();
        let left_w = if two { w * 0.5 } else { w };
        let mut y = area.top() + 24.0;
        let x = area.left() + 32.0;
        ui.painter().text(pos2(x, y + 12.0), Align2::LEFT_CENTER, "Change Display Name Style", semi(20.0), pal().header_primary);
        let c = Rect::from_min_size(pos2(area.right() - 56.0, area.top() + 16.0), vec2(32.0, 32.0));
        if round_icon(ui, c, Icon::Close, "Close", false).clicked() {
            cancel = true;
        }
        y += 48.0;
        let cols = if two { 4 } else { 4 };
        let gap = 8.0;
        let tw = ((left_w - 64.0 - gap * (cols as f32 - 1.0)) / cols as f32).min(104.0);
        let section = |ui: &Ui, y: &mut f32, s: &str| {
            let r = ui.painter().text(pos2(x, *y + 10.0), Align2::LEFT_CENTER, s, semi(16.0), pal().header_primary);
            icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(r.right() + 14.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
            *y += 32.0;
        };
        section(ui, &mut y, "Choose Font");
        for (i, f) in FONTS.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * (tw * 0.75 + gap)), vec2(tw, tw * 0.75));
            let resp = chip(ui, r, &format!("font{f}"), e.font == *f);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Gg", face(*f, 24.0), pal().header_primary);
            crate::ui::settings::mark(ui, &format!("style:font:{f}"), r);
            if resp.clicked() {
                e.font = *f;
            }
        }
        y += (FONTS.len() / cols) as f32 * (tw * 0.75 + gap) + 16.0;
        section(ui, &mut y, "Choose Effect");
        for (i, (id, label)) in EFFECTS.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * (tw * 0.5 + gap)), vec2(tw, tw * 0.5));
            let resp = chip(ui, r, &format!("fx{id}"), e.effect == *id);
            let one = Style { font: 11, effect: *id, colors: e.colors.clone() };
            paint_name(ui, r.center(), Align2::CENTER_CENTER, label, Some(&one), 16.0, tw - 8.0, false);
            crate::ui::settings::mark(ui, &format!("style:effect:{id}"), r);
            if resp.clicked() {
                e.effect = *id;
                let want = e.wants();
                while e.colors.len() < want {
                    e.colors.push(PRESETS[e.colors.len() % PRESETS.len()]);
                }
                e.colors.truncate(want);
            }
        }
        y += (EFFECTS.len().div_ceil(cols)) as f32 * (tw * 0.5 + gap) + 16.0;
        for slot in 0..e.wants() {
            section(ui, &mut y, if slot == 0 { "Choose Color" } else { "Choose Second Color" });
            let sw = 40.0;
            let pen = Rect::from_min_size(pos2(x, y), vec2(sw, sw));
            let resp = chip(ui, pen, &format!("pen{slot}"), false);
            icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(pen.center(), vec2(16.0, 16.0)), pal().header_primary);
            crate::ui::settings::mark(ui, &format!("style:custom:{slot}"), pen);
            if resp.on_hover_text("Pick a color from the page").clicked() {
                custom = Some(slot);
            }
            for (i, c) in PRESETS.iter().enumerate() {
                let r = Rect::from_min_size(pos2(x + (i + 1) as f32 * (sw + 6.0), y), vec2(sw, sw));
                let resp = ui.interact(r, ui.id().with(("preset", slot, i)), Sense::click());
                ui.painter().rect_filled(r, CornerRadius::same(8), role_color(*c));
                if e.colors.get(slot) == Some(c) {
                    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
                    icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), Color32::from_gray(30));
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                crate::ui::settings::mark(ui, &format!("style:preset:{slot}:{i}"), r);
                if resp.clicked() {
                    if e.colors.len() <= slot {
                        e.colors.resize(slot + 1, *c);
                    }
                    e.colors[slot] = *c;
                }
            }
            y += sw + 20.0;
        }
        let side = if two { Rect::from_min_max(pos2(area.left() + left_w, area.top() + 72.0), pos2(area.right() - 24.0, y)) } else { Rect::NOTHING };
        if two {
            ui.painter().rect_filled(Rect::from_min_max(pos2(area.left() + left_w, area.top()), pos2(area.right(), y + 76.0)), CornerRadius { nw: 0, ne: 12, sw: 0, se: 0 }, pal().bg_primary);
            preview(ui, side, &name, e, user);
        }
        let foot = y + 8.0;
        ui.painter().hline(area.x_range(), foot, Stroke::new(1.0, faint(31)));
        let by = foot + 16.0;
        let ap = Rect::from_min_size(pos2(area.right() - 24.0 - 162.0, by), vec2(162.0, 44.0));
        let sr = Rect::from_min_size(pos2(ap.left() - 12.0 - 164.0, by), vec2(164.0, 44.0));
        let resp = ui.interact(sr, ui.id().with("surprise"), Sense::click());
        ui.painter().rect(sr, CornerRadius::same(8), faint(if resp.hovered() { 51 } else { 31 }), Stroke::new(1.0, faint(41)), egui::StrokeKind::Inside);
        ui.painter().text(sr.center(), Align2::CENTER_CENTER, "Surprise Me", medium(16.0), pal().header_primary);
        crate::ui::settings::mark(ui, "style:surprise", sr);
        if resp.clicked() {
            e.surprise();
        }
        let resp = ui.interact(ap, ui.id().with("apply"), Sense::click());
        ui.painter().rect_filled(ap, CornerRadius::same(8), motion::mix(BLURPLE, BLURPLE.gamma_multiply(0.85), motion::hot(ui.ctx(), &resp)));
        ui.painter().text(ap.center(), Align2::CENTER_CENTER, "Apply", medium(16.0), Color32::WHITE);
        crate::ui::settings::mark(ui, "style:apply", ap);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        apply = resp.clicked();
        ui.allocate_rect(Rect::from_min_max(area.min, pos2(area.right(), by + 44.0 + 24.0)), Sense::hover());
    });
    let _ = app;
    if apply {
        ed.draft.style = Some(e.style());
        ed.dlg = Dlg::None;
    } else if let Some(slot) = custom {
        let c = e.colors.get(slot).copied().unwrap_or(PRESETS[0]);
        let back = std::mem::replace(&mut ed.dlg, Dlg::None);
        if let Dlg::Style(st) = back {
            ed.style_back = Some(st);
        }
        ed.dlg = Dlg::Colour(Swatch::Name(slot), rgb_of(c));
    } else if cancel || m.should_close() {
        ed.dlg = Dlg::None;
    }
}

fn preview(ui: &mut Ui, r: Rect, name: &str, e: &Edit, user: &crate::state::User) {
    let st = e.style();
    let card = Rect::from_min_size(r.min + vec2(24.0, 8.0), vec2((r.width() - 48.0).min(360.0), 220.0));
    let ring = Rect::from_min_size(card.min, card.size());
    ui.painter().rect_filled(ring, CornerRadius::same(8), pal().bg_secondary);
    ui.painter().rect_filled(Rect::from_min_size(card.min, vec2(card.width(), 100.0)), CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Color32::from_rgb(0x2f, 0x6b, 0x5a));
    crate::ui::avatar(ui, Rect::from_min_size(card.min + vec2(16.0, 56.0), vec2(88.0, 88.0)), &user.avatar_url());
    paint_name(ui, card.min + vec2(16.0, 168.0), Align2::LEFT_CENTER, name, Some(&st), 22.0, card.width() - 32.0, true);
    ui.painter().text(card.min + vec2(16.0, 196.0), Align2::LEFT_CENTER, &user.username, reg(14.0), pal().header_primary);
    let chat = Rect::from_min_size(card.left_bottom() + vec2(24.0, -16.0), vec2(card.width() - 24.0, 64.0));
    ui.painter().rect_filled(chat, CornerRadius::same(8), pal().bg_primary);
    crate::ui::avatar(ui, Rect::from_min_size(chat.min + vec2(12.0, 12.0), vec2(40.0, 40.0)), &user.avatar_url());
    let n = paint_name(ui, chat.min + vec2(64.0, 20.0), Align2::LEFT_CENTER, name, Some(&st), 16.0, chat.width() - 160.0, false);
    ui.painter().text(pos2(n.right() + 8.0, n.center().y), Align2::LEFT_CENTER, "11:39 AM", reg(12.0), pal().text_muted);
    ui.painter().text(chat.min + vec2(64.0, 44.0), Align2::LEFT_CENTER, "does anyone read this?", reg(16.0), pal().text_normal);
    let row = Rect::from_min_size(chat.left_bottom() - vec2(24.0, -8.0), vec2(card.width() * 0.8, 40.0));
    ui.painter().rect_filled(row, CornerRadius::same(8), pal().bg_primary);
    crate::ui::avatar(ui, Rect::from_min_size(row.min + vec2(8.0, 4.0), vec2(32.0, 32.0)), &user.avatar_url());
    paint_name(ui, row.min + vec2(52.0, 20.0), Align2::LEFT_CENTER, name, Some(&st), 16.0, row.width() - 64.0, false);
    ui.painter().text(pos2(r.left() + 24.0, r.bottom() - 4.0), Align2::LEFT_BOTTOM, "Note: Display name colors and effects will not appear in servers.", reg(12.0), pal().text_muted);
}
