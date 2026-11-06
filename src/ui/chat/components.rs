use egui::{Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::*;
use crate::state::{Comp, Message, Stage};
use crate::ui::interact::Press;
use crate::ui::selectmenu::{self, Opt, Spec};

pub(super) fn components(ui: &mut Ui, d: &mut D, m: &Message) {
    for (i, row) in m.components.iter().enumerate() {
        if i > 0 {
            ui.add_space(8.0);
        }
        if let [c] = row.as_slice()
            && !matches!(c, Comp::Button { .. } | Comp::Select { .. })
        {
            render_comp(ui, d, m, c, ui.available_width().min(520.0));
            continue;
        }
        render_row(ui, d, m, row);
    }
}

/// One components-v2 node, laid out like discord's: text as markdown, galleries as images,
/// containers as a bordered box with an optional accent bar, sections with a side accessory.
pub(super) fn render_comp(ui: &mut Ui, d: &mut D, m: &Message, c: &Comp, width: f32) {
    match c {
        Comp::Row(children) => render_row(ui, d, m, children),
        Comp::Text(s) => {
            let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 16.0, color: pal().text_normal, revealed: true, edited: false, embed: false };
            let blocks = markdown::parse(s, &cx);
            blocks_ui(ui, &blocks, d, m.id);
        }
        Comp::Gallery(items) if items.len() > 1 && items.iter().all(|(_, w, h)| *w > 0 && *h > 0) => {
            let pictures: Vec<crate::state::Attachment> = items
                .iter()
                .map(|(url, w, h)| crate::state::Attachment { url: url.clone(), proxy: url.clone(), filename: "image.png".into(), ctype: "image/png".into(), w: *w, h: *h, ..Default::default() })
                .collect();
            let refs: Vec<&crate::state::Attachment> = pictures.iter().collect();
            mosaic::show(ui, d, m.id, &refs);
        }
        Comp::Gallery(items) => {
            for (url, iw, ih) in items {
                let (w, h) = if *iw > 0 { fit(*iw, *ih, width.min(400.0), 300.0, width) } else { (width.min(300.0), 200.0) };
                let r = media(ui, d, url, url, w, h, None);
                if r.clicked() {
                    d.acts.push(Act::View(url.clone(), url.clone(), *iw, *ih));
                }
                ui.add_space(4.0);
            }
        }
        Comp::Thumbnail(url) => {
            let r = media(ui, d, url, url, 80.0, 80.0, None);
            if r.clicked() {
                d.acts.push(Act::View(url.clone(), url.clone(), 0, 0));
            }
        }
        Comp::FileCard { name, size, url } => {
            if file_card(ui, name, *size).clicked() {
                d.acts.push(Act::Url(url.clone()));
            }
        }
        Comp::Separator { divider } => {
            ui.add_space(4.0);
            if *divider {
                let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().border_muted));
            }
            ui.add_space(4.0);
        }
        Comp::Section { children, accessory } => {
            ui.horizontal_top(|ui| {
                let aw = if accessory.is_some() { 96.0 } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_width((width - aw - 12.0).max(80.0));
                    for ch in children {
                        render_comp(ui, d, m, ch, width - aw - 12.0);
                    }
                });
                if let Some(a) = accessory {
                    render_comp(ui, d, m, a, aw);
                }
            });
        }
        Comp::Container { accent, children } => {
            let col = accent.map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_muted))
                .corner_radius(8)
                .inner_margin(Margin { left: if col.is_some() { 12 } else { 16 }, right: 16, top: 12, bottom: 12 })
                .show(ui, |ui| {
                    ui.set_width(width - 32.0);
                    if let Some(col) = col {
                        let g = ui.min_rect();
                        ui.painter().rect_filled(Rect::from_min_size(pos2(g.left() - 8.0, g.top() - 12.0), vec2(4.0, g.height() + 24.0)), CornerRadius::same(2), col);
                    }
                    for (i, ch) in children.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                        }
                        render_comp(ui, d, m, ch, width - 32.0);
                    }
                });
        }
        Comp::Button { .. } | Comp::Select { .. } => render_row(ui, d, m, std::slice::from_ref(c)),
    }
}


fn render_row(ui: &mut Ui, d: &mut D, m: &Message, row: &[Comp]) {
    let waiting = m.busy.as_ref().filter(|b| !matches!(b.stage, Stage::Failed(..)));
    let failed = m.busy.as_ref().and_then(|b| match &b.stage {
        Stage::Failed(why, _) => Some((b.component.as_str(), why.as_str())),
        _ => None,
    });
    let mut error = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for c in row {
            match c {
                Comp::Button { style, label, emoji, custom_id, url, disabled, sku } => {
                    let loading = waiting.is_some_and(|b| b.component == *custom_id);
                    let locked = *disabled || (waiting.is_some() && url.is_none() && sku.is_none() && !loading);
                    let edge = Color32::from_white_alpha(20);
                    let (fill, hover, ink, stroke) = match style {
                        1 | 6 => (BLURPLE, Color32::from_rgb(0x44, 0x52, 0xbb), Color32::WHITE, edge),
                        3 => (Color32::from_rgb(0x00, 0x85, 0x45), Color32::from_rgb(0x00, 0x6c, 0x37), Color32::WHITE, edge),
                        4 => (Color32::from_rgb(0xd2, 0x2d, 0x39), Color32::from_rgb(0xa9, 0x23, 0x2e), Color32::WHITE, edge),
                        _ => (pal().control, pal().control_hover, pal().header_primary, pal().border_muted),
                    };
                    let label = if *style == 6 && label.is_empty() { "Subscribe" } else { label.as_str() };
                    let g = ui.painter().layout_no_wrap(label.to_string(), medium(14.0), Color32::WHITE);
                    let ew = if emoji.is_some() { 22.0 } else { 0.0 };
                    let lw = if url.is_some() || *style == 6 { 18.0 } else { 0.0 };
                    let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + ew + lw + 24.0).max(60.0), 32.0), Sense::click());
                    let fade = if locked { 0.5 } else { 1.0 };
                    let fill = if resp.hovered() && !locked { hover } else { fill };
                    ui.painter().rect_filled(r, CornerRadius::same(8), fill.gamma_multiply(fade));
                    ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, stroke.gamma_multiply(fade)), egui::StrokeKind::Inside);
                    if ui.clip_rect().contains_rect(r) {
                        d.rects.push((format!("button:{custom_id}"), r));
                        if let Some(u) = url {
                            d.rects.push((format!("button:{u}"), r));
                        }
                        if let Some(s) = sku {
                            d.rects.push((format!("button:sku:{s}"), r));
                        }
                    }
                    let tc = ink.gamma_multiply(fade);
                    if loading {
                        for i in 0..3 {
                            let (a, s) = crate::ui::motion::dots(ui.ctx(), i);
                            ui.painter().circle_filled(pos2(r.center().x + (i as f32 - 1.0) * 8.0, r.center().y), 3.0 * s, tc.gamma_multiply(a));
                        }
                    } else {
                        let mut x = r.center().x - (g.size().x + ew + lw) / 2.0;
                        if *style == 6 {
                            icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(x + 9.0, r.center().y), vec2(16.0, 16.0)), tc);
                            x += lw;
                        }
                        if let Some(e) = emoji {
                            text::image(ui.painter(), &emoji_url(e), Rect::from_min_size(pos2(x, r.center().y - 9.0), vec2(18.0, 18.0)));
                            x += ew;
                        }
                        let gw = g.size().x;
                        ui.painter().galley(pos2(x, (r.center().y - g.size().y / 2.0).floor()), g, tc);
                        if url.is_some() {
                            icons::draw(ui.painter(), Icon::ArrowRight, Rect::from_center_size(pos2(x + gw + 12.0, r.center().y), vec2(14.0, 14.0)), tc);
                        }
                    }
                    if resp.hovered() && !locked {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() && !locked && !loading {
                        match (url, sku) {
                            (Some(u), _) => d.acts.push(Act::Masked(u.clone())),
                            (_, Some(s)) => d.acts.push(Act::Premium(s.clone())),
                            _ => d.acts.push(Act::Component(m.id, custom_id.clone(), Press::Button)),
                        }
                    }
                    if failed.is_some_and(|(id, _)| id == custom_id) {
                        error = failed.map(|f| f.1.to_string());
                    }
                }
                Comp::Select { kind, custom_id, placeholder, options, disabled, min, max, picked: initial, types } => {
                    let loading = waiting.is_some_and(|b| b.component == *custom_id);
                    let locked = *disabled || (waiting.is_some() && !loading);
                    let id = ui.id().with(("select", m.id, custom_id));
                    let opts: Vec<Opt> = options.iter().map(|(l, v, desc)| Opt { label: l.clone(), value: v.clone(), desc: desc.clone(), lead: selectmenu::Lead::None }).collect();
                    let mut picked: Vec<String> = ui.data(|dd| dd.get_temp(id.with("picked"))).unwrap_or_else(|| initial.clone());
                    let (min, max) = (*min as usize, (*max as usize).max(1));
                    let mark = format!("select:{custom_id}");
                    let spec = Spec {
                        kind: *kind,
                        placeholder,
                        min,
                        max,
                        disabled: locked,
                        loading,
                        options: &opts,
                        channel_types: types,
                        guild: d.guild,
                        channel: m.channel,
                        width: ui.available_width().min(400.0),
                        mark: &mark,
                    };
                    let out = selectmenu::show(ui, d.store, id, &spec, &mut picked);
                    if out.changed {
                        let err = out.error.clone();
                        ui.data_mut(|dd| {
                            dd.insert_temp(id.with("picked"), picked.clone());
                            dd.insert_temp(id.with("err"), err.clone());
                        });
                        if err.is_none() {
                            d.acts.push(Act::Component(m.id, custom_id.clone(), Press::Select(*kind, picked.clone())));
                        }
                    }
                    if let Some(e) = ui.data(|dd| dd.get_temp::<Option<String>>(id.with("err"))).flatten() {
                        error = Some(e);
                    }
                    if failed.is_some_and(|(id, _)| id == custom_id) {
                        error = failed.map(|f| f.1.to_string());
                    }
                }
                _ => {}
            }
        }
    });
    if let Some(e) = error {
        ui.add_space(4.0);
        super::interaction::line(ui, d.ctx, 14.0, false, &e);
    }
}
