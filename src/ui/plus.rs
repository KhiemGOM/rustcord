use egui::{Align2, Color32, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, avatar, menu_item, text};
use crate::state::{Channel, Id, sid};
use crate::theme::*;

const ATTACH_FILES: u64 = 1 << 15;
const CREATE_THREADS: u64 = 1 << 35;
const SEND_POLLS: u64 = 1 << 49;
const USE_COMMANDS: u64 = 1 << 31;

pub enum Plus {
    Upload,
    Thread,
    Poll,
    Apps,
}

/// The menu over the composer's plus: what you can add to a message here.
pub fn menu(app: &App, resp: &egui::Response, chan: &Channel) -> Option<Plus> {
    let perms = app.store.perms(chan);
    let mut pick = None;
    egui::Popup::menu(resp).align(egui::RectAlign::TOP_START).gap(8.0).show(|ui| {
        ui.set_min_width(176.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        if perms & ATTACH_FILES != 0 && menu_item(ui, Some(Icon::FileUpload), "Upload a File", false) {
            pick = Some(Plus::Upload);
        }
        if chan.guild != 0 && matches!(chan.kind, 0 | 5) && perms & CREATE_THREADS != 0 && menu_item(ui, Some(Icon::Thread), "Create Thread", false) {
            pick = Some(Plus::Thread);
        }
        if !chan.is_thread() && perms & SEND_POLLS != 0 && menu_item(ui, Some(Icon::Poll), "Create Poll", false) {
            pick = Some(Plus::Poll);
        }
        if perms & USE_COMMANDS != 0 && menu_item(ui, Some(Icon::Apps), "Use Apps", false) {
            pick = Some(Plus::Apps);
        }
    });
    pick
}

#[derive(Default)]
pub struct Launcher {
    q: String,
    app: Option<Id>,
    more: bool,
    fresh: bool,
}

impl Launcher {
    pub fn new() -> Self {
        Launcher { fresh: true, ..Default::default() }
    }
}

struct Entry {
    id: Id,
    name: String,
    icon: Option<String>,
    blurb: String,
}

fn icon(ui: &Ui, r: Rect, e: &Entry) {
    match &e.icon {
        Some(h) => avatar(ui, r, &format!("{}/app-icons/{}/{h}.png?size=64", crate::api::cdn(), e.id)),
        None => {
            ui.painter().rect_filled(r, 8, BLURPLE);
            icons::draw(ui.painter(), Icon::Discord, r.shrink(r.width() * 0.25), Color32::WHITE);
        }
    }
}

fn entries(app: &App, scope: Id) -> Vec<Entry> {
    let Some(c) = app.commands.get(&scope) else { return Vec::new() };
    c.apps
        .iter()
        .map(|(id, name, ic)| Entry {
            id: *id,
            name: name.clone(),
            icon: ic.clone(),
            blurb: c.list.iter().find(|x| sid(&x["application_id"]) == *id).and_then(|x| x["description"].as_str()).unwrap_or_default().to_string(),
        })
        .collect()
}

/// Discord's app launcher: search every command, recent apps, and the apps in this server.
pub fn launcher(app: &mut App, ctx: &egui::Context) {
    let Some(mut l) = app.launcher.take() else { return };
    let Some(chan) = app.cur().and_then(|c| app.store.channels.get(&c).cloned()) else { return };
    app.ensure_commands(&chan);
    let scope = if chan.guild != 0 { chan.guild } else { chan.id };
    let all = entries(app, scope);
    let recent: Vec<&Entry> = app.ui.recent_apps.iter().filter_map(|i| all.iter().find(|e| e.id == *i)).take(8).collect();
    let anchor = app.rects.get("composer").copied().unwrap_or(Rect::from_min_size(pos2(400.0, 600.0), vec2(600.0, 56.0)));
    let mut picked: Option<String> = None;
    let mut keep = true;
    let area = egui::Area::new("launcher".into())
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(anchor.right() - 8.0, anchor.top() - 8.0))
        .show(ctx, |ui| {
            Frame::new()
                .fill(pal().bg_floating)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .shadow(ui.style().visuals.popup_shadow)
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(468.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (sr, _) = ui.allocate_exact_size(vec2(468.0, 40.0), Sense::hover());
                    ui.painter().rect_filled(sr, 8, pal().input_bg);
                    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(sr.left() + 22.0, sr.center().y), vec2(16.0, 16.0)), pal().header_primary);
                    let e = ui.put(
                        Rect::from_min_max(pos2(sr.left() + 44.0, sr.center().y - 10.0), pos2(sr.right() - 12.0, sr.center().y + 10.0)),
                        egui::TextEdit::singleline(&mut l.q).hint_text("Search Apps & Commands").font(reg(16.0)).frame(Frame::NONE).margin(Margin::ZERO),
                    );
                    ui.painter().rect_stroke(sr, 8, Stroke::new(1.0, BLURPLE), egui::StrokeKind::Inside);
                    if std::mem::take(&mut l.fresh) {
                        e.request_focus();
                    }
                    ui.add_space(16.0);
                    egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 220.0).clamp(160.0, 480.0)).auto_shrink([false, true]).show(ui, |ui| {
                        let q = l.q.trim().to_lowercase();
                        if !q.is_empty() || l.app.is_some() {
                            let mut shown = 0;
                            for c in app.commands.get(&scope).map(|c| c.list.as_slice()).unwrap_or_default() {
                                let (name, desc, aid) = (c["name"].as_str().unwrap_or(""), c["description"].as_str().unwrap_or(""), sid(&c["application_id"]));
                                let hit = if q.is_empty() { l.app == Some(aid) } else { name.to_lowercase().contains(&q) || desc.to_lowercase().contains(&q) };
                                if !hit || c["type"].as_u64().unwrap_or(1) != 1 {
                                    continue;
                                }
                                shown += 1;
                                let (r, resp) = ui.allocate_exact_size(vec2(468.0, 48.0), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, 8, pal().control_hover);
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if let Some(e) = all.iter().find(|e| e.id == aid) {
                                    icon(ui, Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0)), e);
                                    ui.painter().text(pos2(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, &e.name, semi(13.0), pal().header_secondary);
                                }
                                ui.painter().text(pos2(r.left() + 52.0, r.center().y - 9.0), Align2::LEFT_CENTER, format!("/{name}"), semi(15.0), pal().header_primary);
                                text::paint(ui.painter(), pos2(r.left() + 52.0, r.center().y + 10.0), Align2::LEFT_CENTER, desc, reg(13.0), pal().header_secondary, 300.0);
                                if resp.clicked() {
                                    picked = Some(name.to_string());
                                    if let Some(a) = all.iter().find(|e| e.id == aid) {
                                        app.ui.recent_apps.retain(|i| *i != a.id);
                                        app.ui.recent_apps.insert(0, a.id);
                                    }
                                }
                            }
                            if shown == 0 {
                                ui.add_space(24.0);
                                ui.vertical_centered(|ui| ui.label(egui::RichText::new("No commands found").font(medium(16.0)).color(pal().header_secondary)));
                                ui.add_space(24.0);
                            }
                            return;
                        }
                        let head = |ui: &mut Ui, s: &str, more: Option<&mut bool>| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(s).font(semi(16.0)).color(pal().header_primary));
                                if let Some(m) = more {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let r = ui.add(egui::Label::new(egui::RichText::new(if *m { "View Less" } else { "View More" }).font(medium(14.0)).color(pal().link)).sense(Sense::click()));
                                        if r.clicked() {
                                            *m = !*m;
                                        }
                                    });
                                }
                            });
                            ui.add_space(8.0);
                        };
                        if !recent.is_empty() {
                            head(ui, "Recents", None);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                for e in &recent {
                                    let (r, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                                    icon(ui, r, e);
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.on_hover_text(&e.name).clicked() {
                                        l.app = Some(e.id);
                                    }
                                }
                            });
                            ui.add_space(16.0);
                        }
                        head(ui, "Apps in this Server", Some(&mut l.more));
                        for e in all.iter().take(if l.more { 100 } else { 4 }) {
                            let (r, resp) = ui.allocate_exact_size(vec2(468.0, 64.0), Sense::click());
                            ui.painter().rect_filled(r, 0, if resp.hovered() { pal().control_hover } else { pal().bg_secondary });
                            icon(ui, Rect::from_center_size(pos2(r.left() + 28.0, r.center().y), vec2(40.0, 40.0)), e);
                            ui.painter().text(pos2(r.left() + 60.0, r.center().y - 11.0), Align2::LEFT_CENTER, &e.name, semi(16.0), pal().header_primary);
                            text::paint(ui.painter(), pos2(r.left() + 60.0, r.center().y + 11.0), Align2::LEFT_CENTER, &e.blurb, reg(14.0), pal().header_secondary, 390.0);
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.clicked() {
                                l.app = Some(e.id);
                            }
                        }
                    });
                });
        });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if esc && l.app.is_some() {
        l.app = None;
    } else if esc || super::popups::dismissed(ctx, &area.response) && !app.rects.get("apps_button").is_some_and(|r| ctx.input(|i| i.pointer.interact_pos().is_some_and(|p| r.contains(p)))) {
        keep = false;
    }
    if let Some(name) = picked {
        app.drafts.insert(chan.id, format!("/{name} "));
        app.focus_composer = true;
        app.caret_end = true;
        keep = false;
    }
    if keep {
        app.launcher = Some(l);
    }
}
