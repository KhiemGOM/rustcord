//! Discord's preview of a text or code attachment: the first lines of the file in colour over a
//! footer with expand, language, view-whole-file and more. Only the first 50 KB is fetched.

use super::icons::{self, Icon};
use super::player::Do;
use super::{App, menu_item, menu_row, menu_sep, Trail};
use crate::state::Attachment;
use crate::theme::*;
use egui::{Align2, Color32, Context, CornerRadius, Event, Id, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

const CAP: usize = 50_000;
const LINE: f32 = 16.0;
const COLLAPSED: usize = 6;

enum Load {
    Loading,
    Ready(Arc<str>),
    Failed,
}

static TEXTS: LazyLock<Mutex<HashMap<String, Load>>> = LazyLock::new(Default::default);

pub fn forget() {
    TEXTS.lock().unwrap().clear();
}

pub fn fetch(app: &mut App, ctx: &Context, url: String) {
    let (ctx, http) = (ctx.clone(), crate::api::http());
    app.net.rt.spawn(async move {
        let body = async {
            let mut r = http.get(&url).header("Range", format!("bytes=0-{}", CAP - 1)).send().await.ok()?.error_for_status().ok()?;
            let mut buf = Vec::new();
            while let Some(c) = r.chunk().await.ok()? {
                buf.extend_from_slice(&c);
                if buf.len() >= CAP {
                    break;
                }
            }
            buf.truncate(CAP);
            Some(buf)
        }
        .await;
        let load = match body {
            Some(b) => Load::Ready(String::from_utf8_lossy(&b).trim_end_matches('\u{fffd}').replace('\r', "").into()),
            None => Load::Failed,
        };
        TEXTS.lock().unwrap().insert(url, load);
        ctx.request_repaint();
    });
}

fn size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} bytes"),
        1024..1_048_576 => format!("{:.0} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

#[derive(Clone, Default)]
struct View {
    open: bool,
    wrap: bool,
    lang: Option<&'static str>,
    selected: bool,
    whole: bool,
    scroll: f32,
}

fn galley(ctx: &Context, a: &Attachment, v: &View, text: &str, lines: usize, width: f32) -> Arc<egui::Galley> {
    let left = a.size.saturating_sub(CAP as u64);
    let cut = if v.open { text.len() } else { text.split_inclusive('\n').take(lines).map(str::len).sum() };
    let mut shown = text[..cut].trim_end_matches('\n').to_string();
    if left > 0 && cut == text.len() {
        shown += &format!("... ({} left)", size(left));
    }
    let wrap = if v.wrap { width } else { f32::INFINITY };
    let job = crate::highlight::job(&a.filename, v.lang, &shown, mono(14.0), pal().text_normal, pal().text_muted, wrap);
    ctx.layer_painter(egui::LayerId::background()).layout_job(job)
}

fn button(ui: &mut Ui, id: Id, rect: Rect, icon: Icon, tip: &str) -> egui::Response {
    let r = ui.interact(rect, id, Sense::CLICK);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = if r.hovered() { pal().interactive_hover } else { pal().header_secondary };
    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), c);
    r.on_hover_text(tip)
}

pub fn card(ui: &mut Ui, a: &Attachment, out: &mut Vec<Do>) -> Rect {
    let ctx = ui.ctx().clone();
    let id = Id::new(("textfile", &a.url));
    let mut v = ctx.data(|d| d.get_temp::<View>(id)).unwrap_or_default();
    let loaded = {
        let mut t = TEXTS.lock().unwrap();
        match t.get(&a.url) {
            Some(Load::Ready(text)) => Some(text.clone()),
            Some(_) => None,
            None => {
                t.insert(a.url.clone(), Load::Loading);
                out.push(Do::Fetch(a.url.clone()));
                None
            }
        }
    };
    let w = ui.available_width().min(ctx.content_rect().width() * 0.5).max(302.0);
    let lines = loaded.as_ref().map_or(0, |t| t.lines().count());
    let more = a.size > CAP as u64;
    let height = if v.open { (lines.max(1) as f32 * LINE + 26.0).clamp(118.0, 500.0) } else { (lines.clamp(1, COLLAPSED) as f32 * LINE).max(100.0) + 26.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height + 66.0), Sense::click());
    let body = Rect::from_min_size(rect.min, vec2(w, height));
    let foot = Rect::from_min_max(pos2(rect.left(), body.bottom()), rect.right_bottom());
    let p = ui.painter().clone();
    p.rect(body, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, pal().bg_primary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    p.rect(foot, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    p.hline(foot.x_range().shrink(1.0), foot.top(), Stroke::new(1.0, pal().bg_secondary));
    let inner = Rect::from_min_max(body.min + vec2(17.0, 17.0), body.max - vec2(9.0, 9.0));
    match &loaded {
        None => {
            let failed = matches!(TEXTS.lock().unwrap().get(&a.url), Some(Load::Failed));
            let msg = if failed { "Couldn't load this file" } else { "Loading…" };
            p.text(inner.left_top(), Align2::LEFT_TOP, msg, mono(14.0), pal().text_muted);
        }
        Some(text) => {
            let g = galley(&ctx, a, &v, text, COLLAPSED, inner.width());
            let clip = p.with_clip_rect(inner.intersect(ui.clip_rect()));
            let most = if v.open { (g.size().y - inner.height()).max(0.0) } else { 0.0 };
            let dy = if resp.hovered() { ctx.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
            let moved = (v.scroll - dy).clamp(0.0, most);
            if moved != v.scroll {
                ctx.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
            }
            v.scroll = moved.min(most);
            let at = inner.min - vec2(0.0, v.scroll);
            if v.selected {
                clip.rect_filled(Rect::from_min_size(at, g.size()), 0.0, pal().mention_bg);
            }
            clip.galley(at, g, pal().text_normal);
        }
    }
    if resp.hovered() && ctx.input(|i| i.pointer.any_pressed() || i.pointer.any_click()) {
        ctx.memory_mut(|m| m.request_focus(resp.id));
    }
    if resp.has_focus() {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            v.selected = true;
        }
        if v.selected
            && let Some(text) = &loaded
            && ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Copy)))
        {
            ctx.copy_text(text.to_string());
        }
    } else if resp.clicked_elsewhere() {
        v.selected = false;
    }
    if resp.clicked() {
        v.selected = false;
    }

    let chevron = Rect::from_min_size(foot.min + vec2(13.0, 13.0), vec2(40.0, 40.0));
    let expandable = lines > COLLAPSED;
    let cr = ui.interact(chevron, id.with("chev"), if expandable { Sense::CLICK } else { Sense::HOVER });
    p.rect(chevron, CornerRadius::same(8), if cr.hovered() && expandable { Color32::from_rgba_unmultiplied(151, 151, 159, 51) } else { Color32::from_rgba_unmultiplied(151, 151, 159, 31) }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(&p, if v.open { Icon::ChevronUp } else { Icon::ChevronDown }, Rect::from_center_size(chevron.center(), vec2(24.0, 24.0)), if expandable { pal().header_primary } else { pal().channel_muted });
    let count = if more { format!("More than {lines} lines") } else { format!("{lines} lines") };
    if expandable {
        let cr = cr.on_hover_text(if v.open { "Collapse".to_string() } else { format!("Expand ({count})") });
        if cr.clicked() {
            v.open = !v.open;
            v.scroll = 0.0;
        }
    }
    let label_x = chevron.right() + 8.0;
    let avail = foot.right() - 12.0 - 3.0 * 18.0 - 2.0 * 16.0 - 16.0 - label_x;
    super::text::paint(&p, pos2(label_x, foot.top() + 22.0), Align2::LEFT_CENTER, &a.filename, reg(14.0), pal().text_normal, avail);
    p.text(pos2(label_x, foot.top() + 42.0), Align2::LEFT_CENTER, size(a.size), reg(12.0), pal().header_secondary);

    let cy = foot.center().y;
    let slot = |n: f32| Rect::from_center_size(pos2(foot.right() - 12.0 - 9.0 - n * 34.0, cy), vec2(24.0, 24.0));
    let more_r = button(ui, id.with("more"), slot(0.0), Icon::More, "More");
    let whole_r = button(ui, id.with("whole"), slot(1.0), Icon::Expand, "View whole file");
    let lang_r = button(ui, id.with("lang"), slot(2.0), Icon::Code, "Change language");
    if whole_r.clicked() {
        v.whole = true;
    }
    egui::Popup::menu(&lang_r).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let cur = v.lang.unwrap_or_else(|| crate::highlight::language(&a.filename));
        for n in crate::highlight::names() {
            if menu_row(ui, None, n, None, false, Trail::Radio(n == cur)).clicked() {
                v.lang = Some(n);
                egui::Popup::close_all(ui.ctx());
            }
        }
    });
    egui::Popup::menu(&more_r).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if menu_row(ui, None, "Wrap Text", None, false, Trail::Check(v.wrap)).clicked() {
            v.wrap = !v.wrap;
            egui::Popup::close_all(ui.ctx());
        }
        if let Some(text) = &loaded
            && menu_item(ui, None, "Copy Text", false)
        {
            ui.ctx().copy_text(text.to_string());
        }
        menu_sep(ui);
        if menu_item(ui, None, "Download", false) {
            out.push(Do::Save(a.url.clone()));
        }
    });
    if v.whole
        && let Some(text) = &loaded
    {
        v.whole = whole(&ctx, a, &v, text, out);
    }
    ctx.data_mut(|d| d.insert_temp(id, v));
    rect
}

pub fn loaded(url: &str) -> bool {
    matches!(TEXTS.lock().unwrap().get(url), Some(Load::Ready(_)))
}

pub fn selected(ctx: &Context, url: &str) -> bool {
    ctx.data(|d| d.get_temp::<View>(Id::new(("textfile", url)))).is_some_and(|v| v.selected)
}

fn whole(ctx: &Context, a: &Attachment, v: &View, text: &str, out: &mut Vec<Do>) -> bool {
    let mut open = true;
    let screen = ctx.content_rect();
    let modal = crate::ui::motion::Modal::new(Id::new(("textfile_whole", &a.url)))
        .frame(egui::Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            let w = (screen.width() * 0.8).min(1100.0);
            ui.set_width(w);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let g = galley(ctx, a, &View { open: true, ..v.clone() }, text, usize::MAX, w - 34.0);
            egui::ScrollArea::both().max_height(screen.height() * 0.7).auto_shrink([false, true]).show(ui, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(g.size().x.max(w - 34.0) + 34.0, g.size().y + 34.0), Sense::hover());
                ui.painter().galley(r.min + vec2(17.0, 17.0), g, pal().text_normal);
            });
            ui.add_space(0.0);
            let (f, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
            ui.painter().hline(f.x_range(), f.top(), Stroke::new(1.0, pal().border_subtle));
            super::text::paint(ui.painter(), pos2(f.left() + 16.0, f.center().y), Align2::LEFT_CENTER, &a.filename, reg(14.0), pal().text_normal, w - 160.0);
            let dl = Rect::from_center_size(pos2(f.right() - 28.0, f.center().y), vec2(24.0, 24.0));
            if button(ui, Id::new("textfile_whole_dl"), dl, Icon::Download, "Download").clicked() {
                out.push(Do::Save(a.url.clone()));
            }
        });
    if modal.should_close() {
        open = false;
    }
    open
}
