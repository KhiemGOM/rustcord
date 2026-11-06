//! What the settings forms share: a draft held next to the loaded data, the "unsaved changes"
//! bar that floats over the bottom of the page, and a few controls discord uses on several pages.

use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Map, Value};

use super::super::App;
use super::super::icons::{self, Icon};
use super::faint;
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

pub fn take(app: &mut App, key: &'static str, orig: &Value) -> Value {
    let Some(s) = app.server.as_mut() else { return orig.clone() };
    s.draft.entry(key).or_insert_with(|| orig.clone()).clone()
}

pub fn put(app: &mut App, key: &'static str, d: Value) {
    if let Some(s) = &mut app.server {
        s.draft.insert(key, d);
    }
}

fn diff(d: &Value, orig: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in d.as_object().into_iter().flatten() {
        if orig.get(k).unwrap_or(&Value::Null) != v {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Whether the settings may leave this page: not while there are unsaved changes, which
/// shake the bar instead.
pub fn guard(app: &mut App, ctx: &egui::Context) -> bool {
    let Some(s) = app.server.as_mut() else { return true };
    if s.dirty {
        s.shake = ctx.input(|i| i.time);
        ctx.request_repaint();
    }
    !s.dirty
}

pub fn fresh(ui: &Ui) {
    ui.data_mut(|d| d.remove_temp::<Vec<(String, f32)>>(Id::new("settings_secs")));
    ui.data_mut(|d| d.remove_temp::<f32>(Id::new("settings_gap")));
}

/// Draws the bar when the draft differs from what's saved. Reset drops the draft; Save sends
/// what changed (as `shape` turns it into a body) and the page's data comes back from the answer.
pub fn bar(app: &mut App, ctx: &egui::Context, key: &'static str, data: &'static str, orig: &Value, path: String, shape: impl FnOnce(Map<String, Value>) -> Value) {
    bar_with(app, ctx, key, orig, move |app, c| app.rest("PATCH", path, Some(shape(c)), Tag::Server(data)));
}

/// The bar for a form whose save isn't one PATCH: `send` gets what changed.
pub fn bar_with(app: &mut App, ctx: &egui::Context, key: &'static str, orig: &Value, send: impl FnOnce(&mut App, Map<String, Value>)) {
    let Some(s) = app.server.as_mut() else { return };
    let changes = s.draft.get(key).map(|d| diff(d, orig)).unwrap_or_default();
    s.dirty = !changes.is_empty();
    if !s.dirty {
        return;
    }
    let area = ctx.data(|d| d.get_temp::<Rect>(Id::new("server_area"))).unwrap_or(ctx.content_rect());
    let t = ctx.input(|i| i.time) - s.shake;
    let off = if t < 0.4 { ((t * 60.0).sin() * 8.0 * (1.0 - t / 0.4)) as f32 } else { 0.0 };
    if t < 0.4 {
        ctx.request_repaint();
    }
    let saving = s.saving;
    let size = vec2(700.0, 53.0);
    let at = pos2(area.center().x - 228.0 - 20.0 + off, area.bottom() - 20.0 - size.y);
    let (mut reset, mut save) = (false, false);
    let shown = egui::Area::new(Id::new(("server_bar", key))).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect(r, CornerRadius::same(5), Color32::from_rgb(0x11, 0x12, 0x14), Stroke::new(1.0, faint(31)), egui::StrokeKind::Inside);
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful \u{2014} you have unsaved changes!", medium(16.0), Color32::WHITE);
        let g = ui.painter().layout_no_wrap("Save Changes".into(), medium(14.0), Color32::WHITE);
        let b = Rect::from_center_size(pos2(r.right() - 12.0 - (g.size().x + 32.0) / 2.0, r.center().y), vec2(g.size().x + 32.0, 32.0));
        let br = ui.interact(b, ui.id().with("save"), if saving { Sense::hover() } else { Sense::click() });
        super::super::settings::mark(ui, "bar:Save Changes", b);
        let fill = crate::ui::motion::mix(Color32::from_rgb(0x24, 0x80, 0x46), Color32::from_rgb(0x1a, 0x6b, 0x38), crate::ui::motion::hot(ui.ctx(), &br));
        ui.painter().rect_filled(b, CornerRadius::same(3), fill.gamma_multiply(if saving { 0.6 } else { 1.0 }));
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "Save Changes", medium(14.0), Color32::WHITE);
        let rg = ui.painter().layout_no_wrap("Reset".into(), medium(14.0), pal().link);
        let rr = Rect::from_center_size(pos2(b.left() - 16.0 - rg.size().x / 2.0, r.center().y), vec2(rg.size().x + 8.0, 32.0));
        let rresp = ui.interact(rr, ui.id().with("reset"), Sense::click());
        super::super::settings::mark(ui, "bar:Reset", rr);
        ui.painter().text(rr.center(), Align2::CENTER_CENTER, "Reset", medium(14.0), pal().link);
        if rresp.hovered() || br.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        reset = rresp.clicked();
        save = br.clicked();
    });
    ctx.move_to_top(shown.response.layer_id);
    if reset {
        s.draft.remove(key);
        s.dirty = false;
    } else if save {
        s.saving = true;
        send(app, changes);
    }
}

/// A text channel (or voice, with `voice`) chosen from a dropdown: its name, and the category
/// it sits under in small capitals as discord shows it. `none` adds an entry for no channel.
pub fn channel(ui: &mut Ui, app: &App, gid: Snow, w: f32, cur: Option<Snow>, voice: bool, none: Option<&str>) -> Option<Option<Snow>> {
    let store = &app.store;
    let mut list: Vec<_> = store.guilds.get(&gid).into_iter().flat_map(|g| g.channels.iter()).filter_map(|c| store.channels.get(c)).filter(|c| if voice { c.kind == 2 } else { matches!(c.kind, 0 | 5) }).collect();
    list.sort_by_key(|c| (c.position, c.id));
    let cat = |c: &crate::state::Channel| store.channels.get(&c.parent).map(|p| p.name.to_uppercase()).unwrap_or_else(|| if voice { "VOICE CHANNELS".into() } else { "TEXT CHANNELS".into() });
    let shown = cur.and_then(|id| store.channels.get(&id)).map(|c| c.name.clone());
    let mut pick = None;
    let resp = dropdown(ui, w, shown.as_deref().or(none).unwrap_or("Select a channel"), shown.is_some().then_some(if voice { Icon::Speaker } else { Icon::Hash }), shown.as_ref().and_then(|_| cur).and_then(|id| store.channels.get(&id)).map(|c| cat(c)), |ui| {
        ui.set_min_width(w - 16.0);
        if let Some(n) = none && ui.selectable_label(cur.is_none(), RichText::new(n).font(medium(16.0)).color(pal().text_normal)).clicked() {
            pick = Some(None);
        }
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for c in &list {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                if resp.hovered() || Some(c.id) == cur {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                }
                icons::draw(ui.painter(), if voice { Icon::Speaker } else { Icon::Hash }, Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary);
                let t = ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, &c.name, medium(16.0), pal().text_normal);
                ui.painter().text(pos2(t.right() + 8.0, r.center().y), Align2::LEFT_CENTER, cat(c), bold(11.0), pal().header_secondary);
                if resp.clicked() {
                    pick = Some(Some(c.id));
                    ui.close();
                }
            }
        });
    });
    super::super::settings::mark(ui, &format!("channel:{}", none.unwrap_or("pick")), resp.rect);
    pick
}

/// The dark closed box with a chevron, with an optional icon and a small-caps tail like the
/// category in front of a channel.
pub fn dropdown(ui: &mut Ui, w: f32, shown: &str, icon: Option<Icon>, tail: Option<String>, menu: impl FnOnce(&mut Ui)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
    let mut x = r.left() + 12.0;
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, Rect::from_center_size(pos2(x + 9.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary);
        x += 24.0;
    }
    let t = super::super::text::paint(ui.painter(), pos2(x, r.center().y), Align2::LEFT_CENTER, shown, medium(16.0), pal().text_normal, r.right() - x - 40.0);
    if let Some(tail) = tail {
        ui.painter().text(pos2(t.right() + 8.0, r.center().y), Align2::LEFT_CENTER, tail, bold(11.0), pal().header_secondary);
    }
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    egui::Popup::menu(&resp).width(w).show(|ui| menu(ui));
    resp
}

/// A page's heading block: the 24px name, a rule above it unless it opens the page, and the
/// blurb under it in the body colour.
pub fn head(ui: &mut Ui, first: bool, name: &str, desc: &str) {
    if !first {
        line(ui);
    }
    ui.label(RichText::new(name).font(reg(24.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(desc).font(reg(14.0)).color(pal().text_normal));
    }
    ui.add_space(16.0);
}

/// The rule between a page's sections.
pub fn line(ui: &mut Ui) {
    ui.add_space(24.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    ui.add_space(32.0);
}

/// Muted fine print.
pub fn small(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
}

/// A label above a field, 12px caps in the muted ink as discord's form labels are.
pub fn caps(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s.to_uppercase()).font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(8.0);
}

pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, faint(51))).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}


/// A 32px button, the small size discord's forms use.
pub fn button(ui: &mut Ui, text: &str, primary: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(14.0), Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + 32.0).max(60.0), 32.0), Sense::click());
    let (fill, ink) = if primary { (crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x47, 0x52, 0xc4), crate::ui::motion::hot(ui.ctx(), &resp)), Color32::WHITE) } else { (crate::ui::motion::mix(faint(31), faint(51), crate::ui::motion::hot(ui.ctx(), &resp)), pal().header_primary) };
    ui.painter().rect_filled(r, CornerRadius::same(3), fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(14.0), ink);
    super::super::settings::mark(ui, &format!("button:{text}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A page's section rule: 40 above and below, as in the profile form.
pub fn rule(ui: &mut Ui) {
    ui.add_space(40.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    ui.add_space(40.0);
}

/// An emoji: its name (the character itself for a unicode one), custom id and animation.
pub type Pick = (String, Snow, bool);

/// A 24px button that shows the emoji, or the smile glyph, and opens a grid of the server's
/// own and the common ones. Returns what was picked; an empty name clears it.
pub fn emoji_button(ui: &mut Ui, app: &App, gid: Snow, cur: &Pick) -> Option<Pick> {
    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    paint_emoji(ui, r.shrink(2.0), cur);
    if cur.0.is_empty() {
        icons::draw(ui.painter(), Icon::Smile, r.shrink(2.0), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut out = None;
    let custom: Vec<_> = app.store.guilds.get(&gid).map(|g| g.emojis.iter().map(|e| (e.id, e.name.clone(), e.animated)).collect()).unwrap_or_default();
    egui::Popup::menu(&resp).width(320.0).show(|ui| {
        let q = ui.data_mut(|d| d.get_temp_mut_or_default::<String>(Id::new("emoji_q")).clone());
        let mut q = q;
        super::super::settings::input(ui, &mut q, "Search emoji", false);
        ui.data_mut(|d| d.insert_temp(Id::new("emoji_q"), q.clone()));
        let q = q.trim().to_lowercase();
        ui.add_space(4.0);
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            let mut items: Vec<Pick> = custom.iter().filter(|(_, n, _)| q.is_empty() || n.to_lowercase().contains(&q)).map(|(id, n, a)| (n.clone(), *id, *a)).collect();
            items.extend(emojis::iter().filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default) && (q.is_empty() || e.shortcodes().any(|c| c.contains(&q)))).take(400).map(|e| (e.as_str().to_string(), 0, false)));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for it in &items {
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), pal().bg_mod_hover, 0.05);
                    paint_emoji(ui, r.shrink(4.0), it);
                    if resp.clicked() {
                        out = Some(it.clone());
                        ui.close();
                    }
                }
            });
        });
        if !cur.0.is_empty() && super::super::settings::link(ui, "Remove emoji").clicked() {
            out = Some((String::new(), 0, false));
            ui.close();
        }
    });
    out
}

pub fn paint_emoji(ui: &Ui, r: Rect, e: &Pick) {
    if e.0.is_empty() {
        return;
    }
    let url = if e.1 != 0 { crate::markdown::emoji_url(e.1, e.2) } else { crate::markdown::twemoji(&e.0) };
    egui::Image::from_uri(url).show_loading_spinner(false).paint_at(ui, r);
}

pub fn data_uri(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    let mime = match bytes.get(..4) {
        Some([0x89, b'P', ..]) => "image/png",
        Some([b'G', b'I', ..]) => "image/gif",
        Some([b'R', b'I', ..]) => "image/webp",
        Some([b'O', b'g', ..]) => "audio/ogg",
        Some([b'I', b'D', b'3', ..]) | Some([0xff, 0xfb, ..]) | Some([0xff, 0xf3, ..]) => "audio/mpeg",
        _ => "image/jpeg",
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Keeps a modal over the settings layer, which comes back to the top whenever it's clicked.
pub fn raise(ctx: &egui::Context, id: &'static str) {
    ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, Id::new(id)));
}
