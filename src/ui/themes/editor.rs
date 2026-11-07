//! The custom theme editor: up to five colours, a direction and an intensity, shown live on the
//! window while it's open and saved with Apply.

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::sync::Arc;

use super::{Back, BODY, Look, child, close, header, open_themes, unlocked, wide};
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{mark, slider};
use crate::ui::{eyedrop, motion};

const PIPETTE: &str = "themes_eyedrop";
const MAX: usize = 5;

#[derive(Default)]
pub struct Draft {
    pub now: Custom,
    reset: Custom,
    at: usize,
    hex: String,
    angle: String,
    mix: String,
    pipette: Option<Arc<egui::ColorImage>>,
    asked: bool,
    hsv: [f32; 3],
    synced: u32,
    synced_at: usize,
}

impl Draft {
    pub fn open(saved: Option<Custom>, light: bool) -> Draft {
        let now = saved.unwrap_or(Custom { light, ..Custom::default() });
        Draft { reset: now.clone(), now, ..Draft::default() }
    }
}

fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn pack(c: Color32) -> u32 {
    (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32
}

fn parse(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6).then(|| u32::from_str_radix(s, 16).ok()).flatten()
}

/// The saturation and brightness square over the hue, and the hue bar under it.
fn picker(ui: &mut Ui, d: &mut Draft) -> bool {
    let cur = d.now.colors[d.at];
    if d.synced != cur || d.at != d.synced_at {
        let h = egui::ecolor::Hsva::from_srgba_unmultiplied([(cur >> 16) as u8, (cur >> 8) as u8, cur as u8, 255]);
        d.hsv = [h.h, h.s, h.v];
        d.synced = cur;
        d.synced_at = d.at;
    }
    let [mut hue, mut sat, mut val] = d.hsv;
    let (sq, resp) = ui.allocate_exact_size(vec2(BODY, 180.0), Sense::click_and_drag());
    let base: Color32 = egui::ecolor::Hsva::new(hue, 1.0, 1.0, 1.0).into();
    let mut across = egui::Mesh::default();
    for (p, c) in [(sq.left_top(), Color32::WHITE), (sq.right_top(), base), (sq.right_bottom(), base), (sq.left_bottom(), Color32::WHITE)] {
        across.colored_vertex(p, c);
    }
    across.add_triangle(0, 1, 2);
    across.add_triangle(0, 2, 3);
    let mut down = egui::Mesh::default();
    for (p, c) in [(sq.left_top(), Color32::TRANSPARENT), (sq.right_top(), Color32::TRANSPARENT), (sq.right_bottom(), Color32::BLACK), (sq.left_bottom(), Color32::BLACK)] {
        down.colored_vertex(p, c);
    }
    down.add_triangle(0, 1, 2);
    down.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(across));
    ui.painter().add(egui::Shape::mesh(down));
    let mut moved = false;
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        sat = ((p.x - sq.left()) / sq.width()).clamp(0.0, 1.0);
        val = 1.0 - ((p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
        moved = true;
    }
    let knob = pos2(sq.left() + sat * sq.width(), sq.top() + (1.0 - val) * sq.height());
    let now: Color32 = egui::ecolor::Hsva::new(hue, sat, val, 1.0).into();
    ui.painter().circle(knob, 9.0, now, Stroke::new(3.0, Color32::WHITE));
    mark(ui, "themes:square", sq);
    ui.add_space(12.0);
    let (bar, resp) = ui.allocate_exact_size(vec2(BODY, 12.0), Sense::click_and_drag());
    let mut rainbow = egui::Mesh::default();
    for i in 0..=6 {
        let c: Color32 = egui::ecolor::Hsva::new(i as f32 / 6.0, 1.0, 1.0, 1.0).into();
        let x = bar.left() + bar.width() * i as f32 / 6.0;
        rainbow.colored_vertex(pos2(x, bar.top()), c);
        rainbow.colored_vertex(pos2(x, bar.bottom()), c);
    }
    for i in 0..6u32 {
        let a = i * 2;
        rainbow.add_triangle(a, a + 1, a + 2);
        rainbow.add_triangle(a + 1, a + 3, a + 2);
    }
    ui.painter().add(egui::Shape::mesh(rainbow));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        hue = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        moved = true;
    }
    let hue_c: Color32 = egui::ecolor::Hsva::new(hue, 1.0, 1.0, 1.0).into();
    ui.painter().circle(pos2(bar.left() + hue * bar.width(), bar.center().y), 9.0, hue_c, Stroke::new(3.0, Color32::WHITE));
    mark(ui, "themes:hue", bar);
    if moved {
        let [r, g, b, _] = egui::ecolor::Hsva::new(hue, sat, val, 1.0).to_srgba_unmultiplied();
        d.hsv = [hue, sat, val];
        d.now.colors[d.at] = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        d.synced = d.now.colors[d.at];
    }
    moved
}

fn label(ui: &mut Ui, s: &str) {
    ui.add(egui::Label::new(RichText::new(s).font(semi(14.0)).color(pal().text_muted)).selectable(false));
    ui.add_space(8.0);
}

fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let fill = motion::mix(Color32::TRANSPARENT, Color32::from_rgba_unmultiplied(151, 151, 159, 31), motion::hot(ui.ctx(), &resp));
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    icons::draw(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("themes:{tip}"), r);
    resp.on_hover_text(tip)
}

fn mode(ui: &mut Ui, light: &mut bool) -> bool {
    let (r, _) = ui.allocate_exact_size(vec2(BODY, 40.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(31));
    let mut changed = false;
    for (i, (icon, tip, is_light)) in [(Icon::Moon, "Dark Mode", false), (Icon::Sun, "Light Mode", true)].into_iter().enumerate() {
        let cell = Rect::from_min_size(r.min + vec2(i as f32 * BODY / 2.0, 0.0), vec2(BODY / 2.0, 40.0)).shrink(4.0);
        let resp = ui.interact(cell, egui::Id::new(("themes_mode", tip)), Sense::click());
        let on = *light == is_light;
        if on {
            ui.painter().rect_filled(cell, CornerRadius::same(6), Color32::from_rgba_unmultiplied(151, 151, 159, 51));
        }
        icons::draw(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(20.0, 20.0)), if on { pal().header_primary } else { pal().header_secondary });
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        mark(ui, &format!("themes:{tip}"), cell);
        if resp.on_hover_text(tip).clicked() && !on {
            *light = is_light;
            changed = true;
        }
    }
    changed
}

/// The colours as a gradient bar with a handle for each, which picks the one to edit.
fn strip(ui: &mut Ui, colors: &[u32], at: &mut usize) {
    let (r, _) = ui.allocate_exact_size(vec2(BODY, 32.0), Sense::hover());
    let bar = Rect::from_min_size(r.min, vec2(BODY, 24.0));
    let mut mesh = egui::Mesh::default();
    let n = colors.len();
    for i in 0..=20 {
        let x = bar.left() + BODY * i as f32 / 20.0;
        let t = (i as f32 / 20.0 * 100.0 - 10.0).clamp(0.0, 80.0) / 80.0;
        let k = t * (n - 1) as f32;
        let (lo, f) = ((k.floor() as usize).min(n - 1), k.fract());
        let c = rgb(colors[lo]).lerp_to_gamma(rgb(colors[(lo + 1).min(n - 1)]), f);
        mesh.colored_vertex(pos2(x, bar.top()), c);
        mesh.colored_vertex(pos2(x, bar.bottom()), c);
    }
    for i in 0..20u32 {
        let a = i * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    for (i, c) in colors.iter().enumerate() {
        let x = bar.left() + BODY * (i as f32 / (n - 1) as f32 * 80.0 + 10.0) / 100.0;
        let h = Rect::from_center_size(pos2(x, bar.center().y), vec2(20.0, 20.0));
        let resp = ui.interact(h, egui::Id::new(("themes_stop", i)), Sense::click());
        ui.painter().circle(h.center(), 10.0, rgb(*c), Stroke::new(if i == *at { 3.0 } else { 2.0 }, Color32::WHITE));
        if i == *at {
            ui.painter().add(egui::Shape::convex_polygon(vec![pos2(x - 5.0, r.bottom()), pos2(x + 5.0, r.bottom()), pos2(x, r.bottom() - 6.0)], Color32::WHITE, Stroke::NONE));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        mark(ui, &format!("themes:stop:{i}"), h);
        if resp.clicked() {
            *at = i;
        }
    }
}

fn value(ui: &mut Ui, title: &str, text: &mut String, shown: String, unit: &str, v: &mut f32, hi: f32, snap: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(title).font(medium(16.0)).color(pal().header_primary)).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let focus = ui.memory(|m| m.focused()).is_some_and(|f| f == egui::Id::new(("themes_value", title)));
            if !focus {
                *text = shown;
            }
            let r = egui::Frame::new()
                .fill(Color32::from_black_alpha(31))
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| ui.add(egui::TextEdit::singleline(text).id(egui::Id::new(("themes_value", title))).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(reg(14.0)).text_color(pal().text_normal).horizontal_align(egui::Align::Max).desired_width(40.0)))
                .inner;
            mark(ui, &format!("themes:{title}:input"), r.rect);
            if r.changed()
                && let Ok(n) = text.trim_end_matches(unit).trim().parse::<f32>()
            {
                *v = n.clamp(0.0, hi);
                changed = true;
            }
        });
    });
    ui.add_space(8.0);
    let before = *v;
    if slider(ui, BODY, v, 0.0, hi) {
        if snap && let Some(s) = [90.0, 180.0, 270.0].into_iter().find(|s| (*v - s).abs() <= 10.0) {
            *v = s;
        }
        changed |= (*v - before).abs() > f32::EPSILON;
    }
    changed
}

fn surprise(d: &mut Draft) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |t| t.as_nanos() as u64);
    let mut x = t | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 10_000) as f32 / 10_000.0
    };
    let n = 2 + (next() * 2.0) as usize;
    let base = next();
    d.now.colors = (0..n)
        .map(|i| {
            let h = egui::ecolor::Hsva::new((base + i as f32 * (0.08 + next() * 0.25)).fract(), 0.55 + next() * 0.4, 0.6 + next() * 0.4, 1.0);
            let [r, g, b, _] = h.to_srgba_unmultiplied();
            (r as u32) << 16 | (g as u32) << 8 | b as u32
        })
        .collect();
    d.now.angle = (next() * 360.0) as i32;
    d.now.mix = (50.0 + next() * 40.0) as i32;
    d.at = 0;
}

pub fn show(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let unlocked = unlocked(app);
    let mut d = std::mem::take(&mut app.themes.edit);
    let head = Rect::from_min_size(r.min, vec2(BODY, 32.0));
    let back = app.themes.back;
    header(app, ui, head, "Customize your theme", "editor", |app| close(app, &ctx, true));
    let footer_h = if unlocked { 40.0 } else { 92.0 };
    let footer = Rect::from_min_size(pos2(r.left(), r.bottom() - footer_h), vec2(BODY, footer_h));
    let body = Rect::from_min_max(pos2(r.left(), head.bottom() + 16.0), pos2(r.right(), footer.top() - 16.0));
    if !d.asked
        && app.settings.own_theme.is_none()
        && let Some(Ok(v)) = crate::ui::settings::remote(app, "custom_themes", "/users/@me/custom-themes").cloned()
    {
        d.asked = true;
        if d.now == d.reset
            && let Some(c) = super::saved(&v)
        {
            d.now = c.clone();
            d.reset = c;
        }
    }
    let before = d.now.clone();
    let mut scroll = child(ui, body);
    egui::ScrollArea::vertical().id_salt("themes_editor").auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        label(ui, "Appearance");
        mode(ui, &mut d.now.light);
        ui.add_space(24.0);
        label(ui, "Colors");
        d.at = d.at.min(d.now.colors.len() - 1);
        if d.now.colors.len() > 1 {
            strip(ui, &d.now.colors, &mut d.at);
            ui.add_space(12.0);
        }
        picker(ui, &mut d);
        ui.add_space(12.0);
        let focused = ui.memory(|m| m.focused()).is_some_and(|f| f == egui::Id::new("themes_hex"));
        if !focused {
            d.hex = format!("#{:06X}", d.now.colors[d.at]);
        }
        let mut remove = false;
        let mut pick = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (sw, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
            ui.painter().rect(sw, CornerRadius::same(6), rgb(d.now.colors[d.at]), Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)), egui::StrokeKind::Inside);
            let w = BODY - 32.0 - 8.0 - 36.0 - 8.0 - if d.now.colors.len() > 1 { 36.0 + 8.0 } else { 0.0 };
            let resp = egui::Frame::new()
                .fill(Color32::from_black_alpha(31))
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 61)))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| ui.add(egui::TextEdit::singleline(&mut d.hex).id(egui::Id::new("themes_hex")).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(reg(14.0)).text_color(pal().text_normal).char_limit(7).desired_width(w - 22.0)))
                .inner;
            mark(ui, "themes:hex", resp.rect);
            if resp.changed()
                && let Some(v) = parse(&d.hex)
            {
                d.now.colors[d.at] = v;
            }
            pick = icon_button(ui, Icon::Eyedropper, "Pick a color from the screen", 36.0).clicked();
            if d.now.colors.len() > 1 {
                remove = icon_button(ui, Icon::Minus, "Remove", 36.0).clicked();
            }
        });
        if pick {
            eyedrop::start(&ctx, PIPETTE);
        }
        if remove {
            d.now.colors.remove(d.at);
            d.at = d.at.saturating_sub(1).min(d.now.colors.len() - 1);
        }
        if d.now.colors.len() < MAX {
            ui.add_space(12.0);
            let (rr, resp) = ui.allocate_exact_size(vec2(BODY, 40.0), Sense::click());
            let faint = |a: u8| Color32::from_rgba_unmultiplied(151, 151, 159, a);
            ui.painter().rect(rr, CornerRadius::same(8), motion::mix(faint(31), faint(51), motion::hot(&ctx, &resp)), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
            let g = ui.painter().layout_no_wrap("Add Color".into(), medium(16.0), pal().header_primary);
            let x = rr.center().x - (20.0 + 6.0 + g.size().x) / 2.0;
            icons::draw(ui.painter(), Icon::Plus, Rect::from_min_size(pos2(x, rr.center().y - 10.0), vec2(20.0, 20.0)), pal().header_primary);
            ui.painter().text(pos2(x + 26.0, rr.center().y), Align2::LEFT_CENTER, "Add Color", medium(16.0), pal().header_primary);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            mark(ui, "themes:Add Color", rr);
            if resp.clicked() {
                let last = rgb(*d.now.colors.last().unwrap_or(&DEFAULT_COLOR));
                let mut h = egui::ecolor::Hsva::from_srgba_unmultiplied([last.r(), last.g(), last.b(), 255]);
                h.h = (h.h + 15.0 / 360.0).fract();
                let [r, g, b, _] = h.to_srgba_unmultiplied();
                d.now.colors.push((r as u32) << 16 | (g as u32) << 8 | b as u32);
                d.at = d.now.colors.len() - 1;
            }
        }
        ui.add_space(24.0);
        label(ui, "Controls");
        if d.now.colors.len() > 1 {
            let mut a = d.now.angle as f32;
            if value(ui, "Gradient Direction", &mut d.angle, format!("{}°", d.now.angle), "°", &mut a, 360.0, true) {
                d.now.angle = a.round() as i32;
            }
            ui.add_space(16.0);
        }
        let mut m = d.now.mix as f32;
        if value(ui, "Color Intensity", &mut d.mix, format!("{}%", d.now.mix), "%", &mut m, 100.0, false) {
            d.now.mix = m.round() as i32;
        }
        ui.add_space(24.0);
        let (rr, resp) = ui.allocate_exact_size(vec2(BODY, 40.0), Sense::click());
        let faint = |a: u8| Color32::from_rgba_unmultiplied(151, 151, 159, a);
        ui.painter().rect(rr, CornerRadius::same(8), motion::mix(faint(31), faint(51), motion::hot(&ctx, &resp)), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
        let g = ui.painter().layout_no_wrap("Surprise Me!".into(), medium(16.0), pal().header_primary);
        let x = rr.center().x - (20.0 + 6.0 + g.size().x) / 2.0;
        icons::draw(ui.painter(), Icon::Dice, Rect::from_min_size(pos2(x, rr.center().y - 10.0), vec2(20.0, 20.0)), pal().header_primary);
        ui.painter().text(pos2(x + 26.0, rr.center().y), Align2::LEFT_CENTER, "Surprise Me!", medium(16.0), pal().header_primary);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        mark(ui, "themes:Surprise Me!", rr);
        if resp.clicked() {
            surprise(&mut d);
        }
        ui.add_space(12.0);
        let off = d.now == d.reset;
        if wide(ui, "Reset", Look::Secondary, !off).clicked() && !off {
            d.now = d.reset.clone();
            d.at = 0;
        }
    });
    if let Some(c) = eyedrop::pick(&ctx, PIPETTE, &mut d.pipette) {
        d.now.colors[d.at] = pack(c);
    }
    let mut foot = child(ui, footer);
    foot.spacing_mut().item_spacing = vec2(0.0, 12.0);
    let mut leave = false;
    let mut apply = false;
    let mut nitro = false;
    let back_text = if unlocked || back == Back::Themes { "Back" } else { "Back to Settings" };
    if unlocked {
        foot.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let half = (BODY - 8.0) / 2.0;
            ui.allocate_ui(vec2(half, 40.0), |ui| leave = wide(ui, back_text, Look::Secondary, true).clicked());
            ui.allocate_ui(vec2(half, 40.0), |ui| apply = wide(ui, "Apply", Look::Primary, true).clicked());
        });
    } else {
        nitro = wide(&mut foot, "Unlock with Nitro", Look::Premium, true).clicked();
        leave = wide(&mut foot, back_text, Look::Secondary, true).clicked();
    }
    if d.now != before {
        app.themes.preview = Some(Fill::Custom(d.now.clone()));
        app.apply_look(&ctx);
    }
    app.themes.edit = d;
    if apply {
        let fill = Fill::Custom(app.themes.edit.now.clone());
        super::apply(app, &ctx, Some(fill));
        close(app, &ctx, true);
    } else if nitro {
        close(app, &ctx, false);
        app.settings_page = Some(crate::ui::settings::Page::Nitro);
    } else if leave {
        match back {
            Back::Themes => open_themes(app, &ctx, None),
            Back::Settings => close(app, &ctx, true),
        }
    }
}

