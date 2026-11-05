//! The clips gallery: every clip in the storage folder as a card, with search, sort, favorites,
//! multi-select, and the share dialog.

use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, Confirm, channel_title, grey_button, icon_button, text};
use crate::clips::{self, Clip, library};
use crate::state::Id;
use crate::theme::*;

#[derive(Default)]
pub struct Gallery {
    pub query: String,
    pub oldest: bool,
    pub favs: bool,
    pub sel: Vec<String>,
    pub pick: Option<Id>,
    pub share: Option<Share>,
    pub edit: Option<super::clips_trim::Trim>,
}

#[derive(Default)]
pub struct Share {
    ids: Vec<String>,
    query: String,
    to: Option<Id>,
    note: String,
}

pub const SHARE_MAX: usize = 10;

enum Act {
    Play(Clip),
    Edit(Clip),
    Share(Vec<String>),
    Fav(String),
    Delete(Vec<String>),
    Attach,
}

fn ago(ms: i64) -> String {
    let s = (chrono::Utc::now().timestamp_millis() - ms) / 1000;
    match s {
        ..60 => "just now".into(),
        60..3600 => format!("{} minutes ago", s / 60),
        3600..86400 => format!("{} hours ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

fn clock(ms: i64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}

fn card(ui: &mut Ui, c: &Clip, on: bool, acts: &mut Vec<Act>, sel: &mut Vec<String>, picking: bool) {
    let (r, resp) = ui.allocate_exact_size(vec2(256.0, 196.0), Sense::click());
    let thumb = Rect::from_min_size(r.min, vec2(256.0, 144.0));
    let hot = resp.hovered();
    ui.painter().rect_filled(thumb, CornerRadius::same(8), Color32::BLACK);
    if c.has_thumb {
        let uri = format!("file://{}", c.thumb().display().to_string().replace('\\', "/"));
        egui::Image::new(uri).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, thumb);
    } else {
        icons::draw(ui.painter(), Icon::Clips, Rect::from_center_size(thumb.center(), vec2(32.0, 32.0)), pal().channel_muted);
    }
    if on {
        ui.painter().rect_stroke(thumb, CornerRadius::same(8), Stroke::new(2.0, BLURPLE), egui::StrokeKind::Inside);
    }
    let len = clock(c.length_ms);
    let g = ui.painter().layout_no_wrap(len, medium(12.0), Color32::WHITE);
    let pill = Rect::from_min_size(pos2(thumb.right() - g.size().x - 20.0, thumb.bottom() - 28.0), vec2(g.size().x + 12.0, 20.0));
    ui.painter().rect_filled(pill, CornerRadius::same(4), Color32::from_black_alpha(160));
    ui.painter().galley(pill.min + vec2(6.0, 4.0), g, Color32::WHITE);
    text::paint(ui.painter(), pos2(r.left() + 2.0, thumb.bottom() + 14.0), Align2::LEFT_CENTER, &c.name(), medium(14.0), pal().header_primary, 250.0);
    let sub = format!("{} · {}", if c.app.is_empty() { "Clip" } else { &c.app }, ago(c.created));
    text::paint(ui.painter(), pos2(r.left() + 2.0, thumb.bottom() + 34.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().header_secondary, 250.0);
    if c.favorite {
        icons::draw(ui.painter(), Icon::Star, Rect::from_center_size(pos2(thumb.left() + 20.0, thumb.top() + 20.0), vec2(16.0, 16.0)), Color32::from_rgb(0xfe, 0xe7, 0x5c));
    }
    let mut inner = false;
    if hot || on {
        let strip = Rect::from_min_size(pos2(thumb.right() - 4.0 - 4.0 * 28.0, thumb.top() + 4.0), vec2(4.0 * 28.0, 28.0));
        ui.painter().rect_filled(strip, CornerRadius::same(6), Color32::from_black_alpha(170));
        let mut x = strip.left();
        for (icon, tip, act) in [
            (Icon::Star, if c.favorite { "Remove from Favorites" } else { "Add to Favorites" }, 0),
            (Icon::Pencil, "Edit", 1),
            (Icon::Forward, "Share", 2),
            (Icon::Trash, "Delete", 3),
        ] {
            let b = Rect::from_min_size(pos2(x, strip.top()), vec2(28.0, 28.0));
            x += 28.0;
            let rb = ui.interact(b, resp.id.with(act), Sense::click());
            icons::draw(ui.painter(), icon, b.shrink(6.0), if rb.hovered() { Color32::WHITE } else { Color32::from_gray(200) });
            let rb = rb.on_hover_text(tip);
            inner |= rb.hovered();
            if rb.clicked() {
                match act {
                    0 => acts.push(Act::Fav(c.id.clone())),
                    1 => acts.push(Act::Edit(c.clone())),
                    2 => acts.push(Act::Share(vec![c.id.clone()])),
                    _ => acts.push(Act::Delete(vec![c.id.clone()])),
                }
            }
        }
    }
    let cb = Rect::from_min_size(pos2(thumb.left() + 8.0, thumb.bottom() - 28.0), vec2(20.0, 20.0));
    if hot || on || !sel.is_empty() {
        if on {
            ui.painter().rect_filled(cb, 6, BLURPLE);
            icons::draw(ui.painter(), Icon::Check, cb.shrink(3.0), Color32::WHITE);
        } else {
            ui.painter().rect(cb, 6, Color32::from_black_alpha(120), Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
        }
    }
    if resp.clicked() && !inner {
        let over_box = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| cb.expand(4.0).contains(p));
        if over_box || picking || !sel.is_empty() {
            if on {
                sel.retain(|s| *s != c.id);
            } else {
                sel.push(c.id.clone());
            }
        } else {
            acts.push(Act::Play(c.clone()));
        }
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn load(app: &mut App) {
    if app.clips.list.is_none() {
        app.clips.list = Some(library::scan(&clips::folder(&app.settings.clips_dir)));
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut g) = app.clips.gallery.take() else { return };
    load(app);
    let mut close = false;
    let mut acts = Vec::new();
    let list = app.clips.list.clone().unwrap_or_default();
    let ql = g.query.to_lowercase();
    let mut shown: Vec<&Clip> = list.iter().filter(|c| (!g.favs || c.favorite) && (ql.is_empty() || c.name().to_lowercase().contains(&ql) || c.app.to_lowercase().contains(&ql))).collect();
    if g.oldest {
        shown.reverse();
    }
    let screen = ctx.content_rect();
    let (w, h) = ((screen.width() - 160.0).clamp(560.0, 1200.0), (screen.height() - 120.0).clamp(360.0, 900.0));
    let keys = app.clip_keys();
    let picking = g.pick.is_some();
    let modal = egui::Modal::new("clips".into())
        .frame(Frame::new().fill(pal().bg_primary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(0))
        .show(ctx, |ui| {
            ui.set_width(w);
            ui.set_height(h);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
                ui.set_width(w - 48.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if picking { "Share a Clip" } else { "Clips" }).font(semi(20.0)).color(pal().header_primary));
                    ui.add_space(16.0);
                    let (field, _) = ui.allocate_exact_size(vec2(240.0, 32.0), Sense::hover());
                    ui.painter().rect(field, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(field.left() + 16.0, field.center().y), vec2(16.0, 16.0)), pal().header_secondary);
                    let inner = Rect::from_min_max(pos2(field.left() + 32.0, field.center().y - 10.0), pos2(field.right() - 8.0, field.center().y + 10.0));
                    ui.put(inner, egui::TextEdit::singleline(&mut g.query).hint_text(RichText::new("Search clips").color(pal().text_muted)).font(reg(14.0)).frame(Frame::NONE).margin(Margin::ZERO));
                    ui.add_space(12.0);
                    for (label, on) in [("All Clips", !g.favs), ("Favorites", g.favs)] {
                        let (r, resp) = ui.allocate_exact_size(vec2(96.0, 32.0), Sense::click());
                        ui.painter().rect_filled(r, 8, if on { pal().bg_mod_selected } else if resp.hovered() { pal().bg_mod_hover } else { Color32::TRANSPARENT });
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, label, medium(14.0), if on { pal().header_primary } else { pal().header_secondary });
                        if resp.clicked() {
                            g.favs = label == "Favorites";
                        }
                        ui.add_space(4.0);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                        ui.add_space(8.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(88.0, 32.0), Sense::click());
                        ui.painter().rect(r, 8, if resp.hovered() { pal().control_hover } else { pal().control }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, if g.oldest { "Oldest" } else { "Recent" }, medium(14.0), pal().header_primary);
                        g.oldest ^= resp.on_hover_text("Sort Clips").clicked();
                    });
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, pal().border_subtle));
            let foot = if g.sel.is_empty() { 0.0 } else { 64.0 };
            egui::ScrollArea::vertical().max_height(h - 74.0 - foot).auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 24)).show(ui, |ui| {
                    ui.set_width(w - 48.0);
                    if shown.is_empty() {
                        ui.add_space(h * 0.2);
                        ui.vertical_centered(|ui| {
                            icons::draw(ui.painter(), Icon::Clips, Rect::from_center_size(ui.cursor().center_top() + vec2(0.0, 16.0), vec2(32.0, 32.0)), pal().channel_muted);
                            ui.add_space(48.0);
                            ui.label(RichText::new("No clips found").font(semi(20.0)).color(pal().header_primary));
                            ui.add_space(8.0);
                            let line = if !g.query.is_empty() {
                                "Try searching for something else".to_string()
                            } else if let Some(k) = &keys {
                                format!("Press {k} to capture a clip while streaming.")
                            } else {
                                "Setup a shortcut to capture a clip while gaming.".into()
                            };
                            ui.label(RichText::new("Capture and share your best gaming moments, all in one place.").font(reg(16.0)).color(pal().header_secondary));
                            ui.label(RichText::new(line).font(reg(16.0)).color(pal().header_secondary));
                        });
                        return;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 20.0);
                        for c in &shown {
                            let on = g.sel.contains(&c.id);
                            card(ui, c, on, &mut acts, &mut g.sel, picking);
                        }
                    });
                });
            });
            if !g.sel.is_empty() {
                let (bar, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
                ui.painter().rect_filled(bar, CornerRadius { sw: 12, se: 12, nw: 0, ne: 0 }, pal().bg_secondary);
                let n = g.sel.len();
                ui.painter().text(pos2(bar.left() + 24.0, bar.center().y), Align2::LEFT_CENTER, format!("{n} {} selected", if n == 1 { "Clip" } else { "Clips" }), semi(16.0), pal().header_primary);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(24.0, 12.0))).layout(Layout::right_to_left(Align::Center)));
                c.spacing_mut().item_spacing.x = 8.0;
                if picking {
                    if super::button(&mut c, "Add to Message", BLURPLE, 160.0).clicked() {
                        acts.push(Act::Attach);
                    }
                } else {
                    if super::button(&mut c, "Delete", super::DANGER, 96.0).clicked() {
                        acts.push(Act::Delete(g.sel.clone()));
                    }
                    if super::button(&mut c, "Share", BLURPLE, 96.0).clicked() {
                        acts.push(Act::Share(g.sel.clone()));
                    }
                }
                if grey_button(&mut c, "Clear", 80.0).clicked() {
                    g.sel.clear();
                }
            }
        });
    if modal.should_close() {
        close = true;
    }
    let mut again = true;
    for a in acts {
        match a {
            Act::Play(c) => {
                let _ = open::that(&c.path);
            }
            Act::Edit(c) => g.edit = Some(super::clips_trim::Trim::new(&c)),
            Act::Share(ids) if ids.len() > SHARE_MAX => app.toast("You can only share 10 clips at a time."),
            Act::Share(ids) => g.share = Some(Share { ids, ..Default::default() }),
            Act::Fav(id) => {
                if let Some(c) = app.clips.list.as_mut().and_then(|l| l.iter_mut().find(|c| c.id == id)) {
                    c.favorite = !c.favorite;
                    library::write_meta(c);
                }
            }
            Act::Delete(ids) => {
                let found: Vec<Clip> = list.iter().filter(|c| ids.contains(&c.id)).cloned().collect();
                let n = found.len();
                let body = if n == 1 {
                    "Deleting a clip cannot be undone. Would you like to delete this clip and remove the file from your computer?".to_string()
                } else {
                    format!("Deleting clips cannot be undone. Would you like to delete these {n} clips and remove their files from your computer?")
                };
                app.confirm = Some(Confirm::new(if n == 1 { "Delete this clip?" } else { "Delete these clips?" }, body, "Delete", move |app| {
                    for c in &found {
                        library::delete(c);
                    }
                    if let Some(l) = app.clips.list.as_mut() {
                        l.retain(|c| !found.iter().any(|f| f.id == c.id));
                    }
                    if let Some(g) = app.clips.gallery.as_mut() {
                        g.sel.retain(|s| !found.iter().any(|f| f.id == *s));
                    }
                }));
            }
            Act::Attach => {
                if let Some(ch) = g.pick {
                    for id in g.sel.drain(..).take(SHARE_MAX) {
                        if let Some(c) = list.iter().find(|c| c.id == id) {
                            app.files.push(super::composer::Upload::new(c.path.clone()));
                        }
                    }
                    let _ = ch;
                }
                again = false;
            }
        }
    }
    super::clips_trim::show(app, ctx, &mut g);
    share(app, ctx, &mut g);
    if close {
        again = false;
    }
    if again {
        app.clips.gallery = Some(g);
    }
}

/// One message in `ch` carrying the clips, each cut to its trim first.
pub fn send_clips(app: &mut App, ch: Id, note: String, clips: &[Clip]) {
    let files: Vec<_> = clips
        .iter()
        .filter_map(|c| super::clips_trim::export(c).map(|p| (p.clone(), p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), String::new())))
        .collect();
    if files.is_empty() {
        return;
    }
    let nonce = crate::state::make_nonce();
    app.store.add_pending(ch, note.clone(), nonce.clone(), None);
    app.net.send(crate::net::Cmd::Send { channel: ch, content: note, nonce, reply: None, files, stickers: Vec::new(), tts: false });
    app.toast("Clip shared");
}

fn share(app: &mut App, ctx: &egui::Context, g: &mut Gallery) {
    let Some(s) = &mut g.share else { return };
    let store = &app.store;
    let ql = s.query.to_lowercase();
    let places: Vec<Id> = app
        .ui
        .recent
        .iter()
        .chain(store.dms.iter())
        .chain(store.guild_order.iter().filter_map(|g| store.guilds.get(g)).flat_map(|g| g.channels.iter()))
        .copied()
        .filter(|c| store.channels.get(c).is_some_and(|c| c.is_text() && !c.is_thread() && (c.guild == 0 || store.can_view(c))))
        .filter(|c| ql.is_empty() || channel_title(store, *c).to_lowercase().contains(&ql))
        .take(60)
        .collect();
    let (mut send, mut close) = (false, false);
    let n = s.ids.len();
    let modal = egui::Modal::new("clips_share".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(430.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(if n == 1 { "Share Clip" } else { "Share Clips" }).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| close |= icon_button(ui, Icon::Close, 24.0, "Close").clicked());
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Share your clip to a channel or directly with friends!").font(reg(16.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            let (field, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
            ui.painter().rect(field, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let inner = Rect::from_min_max(pos2(field.left() + 12.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
            ui.put(inner, egui::TextEdit::singleline(&mut s.query).hint_text(RichText::new("Search").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                for c in &places {
                    let (r, resp) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::click());
                    let on = s.to == Some(*c);
                    if resp.hovered() || on {
                        ui.painter().rect_filled(r, 8, pal().bg_mod_hover);
                    }
                    text::paint(ui.painter(), pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &channel_title(store, *c), medium(16.0), pal().header_primary, 360.0);
                    if on {
                        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(16.0, 16.0)), BLURPLE);
                    }
                    if resp.clicked() {
                        s.to = Some(*c);
                    }
                }
            });
            ui.add_space(12.0);
            let (row, _) = ui.allocate_exact_size(vec2(430.0, 40.0), Sense::hover());
            let field = Rect::from_min_size(row.min, vec2(320.0, 40.0));
            ui.painter().rect(field, 8, pal().input_bg, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            let inner = Rect::from_min_max(pos2(field.left() + 12.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
            ui.put(inner, egui::TextEdit::singleline(&mut s.note).hint_text(RichText::new("Add an optional message...").color(pal().text_muted)).font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO));
            let b = Rect::from_min_size(pos2(row.right() - 100.0, row.top()), vec2(100.0, 40.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(b));
            c.add_enabled_ui(s.to.is_some(), |ui| send = super::button(ui, "Send", BLURPLE, 100.0).clicked());
        });
    close |= modal.should_close();
    if send {
        let (ids, to, note) = (s.ids.clone(), s.to, s.note.clone());
        if let Some(ch) = to {
            let picked: Vec<Clip> = app.clips.list.iter().flatten().filter(|c| ids.contains(&c.id)).cloned().collect();
            send_clips(app, ch, note, &picked);
        }
        g.share = None;
        g.sel.clear();
    } else if close {
        g.share = None;
    }
}
