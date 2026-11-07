//! Where a custom emoji or a sticker comes from: the server behind it, with a way in.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, Frame, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, popups, text};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;

pub struct Pop {
    pub id: Id,
    pub name: String,
    pub sticker: bool,
    pub animated: bool,
    pub format: u8,
    pub at: egui::Pos2,
}

struct Server {
    id: Id,
    name: String,
    icon: Option<String>,
    online: u64,
    open: bool,
}

fn server(v: &Value) -> Option<Server> {
    let g = if v["guild"].is_object() { &v["guild"] } else { v };
    let id = sid(&g["id"]);
    let name = g["name"].as_str()?.to_string();
    let icon = g["icon"].as_str().or(g["icon_hash"].as_str()).map(|h| format!("{}/icons/{id}/{h}.png?size=96", crate::api::cdn()));
    let online = g["approximate_presence_count"].as_u64().or(g["online_count"].as_u64()).unwrap_or(0);
    let open = g["features"].as_array().is_some_and(|f| f.iter().any(|x| x == "DISCOVERABLE"));
    Some(Server { id, name, icon, online, open })
}

impl App {
    pub fn open_source(&mut self, pop: Pop) {
        let key = (pop.sticker, pop.id);
        let mine = if pop.sticker { self.store.guilds.values().any(|g| g.stickers.iter().any(|s| s.0 == pop.id)) } else { self.store.guilds.values().any(|g| g.emojis.iter().any(|e| e.id == pop.id)) };
        if pop.sticker {
            self.load_packs();
        }
        if !mine && !self.sources.contains_key(&key) {
            self.sources.insert(key, Value::Null);
            let path = if pop.sticker { format!("/stickers/{}/guild", pop.id) } else { format!("/emojis/{}/source", pop.id) };
            self.rest("GET", path, None, Tag::Source(pop.sticker, pop.id));
        }
        self.source_pop = Some(pop);
    }
}

fn row(ui: &mut Ui, s: &Server, here: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(256.0, 40.0), Sense::hover());
    let ic = Rect::from_min_size(r.min + vec2(0.0, 4.0), vec2(32.0, 32.0));
    ui.painter().circle_filled(ic.center(), 16.0, pal().bg_accent);
    match &s.icon {
        Some(u) => crate::gif::paint_still(ui, u, ic, CornerRadius::same(16), false),
        None => {
            let c: String = s.name.chars().take(1).collect();
            ui.painter().text(ic.center(), Align2::CENTER_CENTER, c, semi(14.0), pal().header_primary);
        }
    }
    let x = ic.right() + 8.0;
    text::paint(ui.painter(), pos2(x, r.top() + 10.0), Align2::LEFT_CENTER, &s.name, semi(16.0), pal().header_primary, r.right() - x);
    let sub = match (here, s.online, s.open) {
        (true, ..) => "You're in this server".to_string(),
        (_, n, true) if n > 0 => format!("{n} Online  \u{2022}  Discoverable Server"),
        (_, n, false) if n > 0 => format!("{n} Online"),
        (_, _, true) => "Discoverable Server".to_string(),
        _ => String::new(),
    };
    text::paint(ui.painter(), pos2(x, r.top() + 29.0), Align2::LEFT_CENTER, &sub, reg(12.0), pal().text_muted, r.right() - x);
}

/// A default pack's sticker: whose it is, three more from the pack, and the way into the rest.
fn pack_pop(app: &mut App, ctx: &egui::Context, at: Pos2, name: &str, pack: &str, others: &[(Id, String, u8)]) {
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("source_pop"));
    super::motion::slide(ctx, layer, vec2(0.0, 1.0));
    let mut more = false;
    let play = app.settings.stickers;
    let area = egui::Area::new(egui::Id::new("source_pop")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).inner_margin(16).show(ui, |ui| {
            ui.set_width(288.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(name).font(semi(16.0)).color(pal().header_primary));
            let mut job = LayoutJob::default();
            let plain = |font| TextFormat { font_id: font, color: pal().text_normal, ..Default::default() };
            job.append("This is a ", 0.0, plain(reg(14.0)));
            job.append(pack, 0.0, plain(bold(14.0)));
            job.append(" sticker.", 0.0, plain(reg(14.0)));
            ui.label(job);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                for (sid, sname, fmt) in others {
                    let (r, resp) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
                    let go = match play {
                        crate::cache::Animate::Always => true,
                        crate::cache::Animate::OnInteraction => resp.hovered(),
                        crate::cache::Animate::Never => false,
                    };
                    egui::Image::from_uri(crate::gif::uri(ui.ctx(), &crate::api::sticker(*sid, *fmt, 80), go && ui.is_rect_visible(r))).show_loading_spinner(false).paint_at(ui, r);
                    resp.on_hover_text(sname);
                }
            });
            ui.add_space(8.0);
            let label = ui.painter().layout_no_wrap("View More Stickers".into(), medium(14.0), pal().text_normal);
            let (b, resp) = ui.allocate_exact_size(vec2(label.size().x + 24.0, 32.0), Sense::click());
            super::settings::mark(ui, "source_pop:more", b);
            let fill = Color32::from_rgba_unmultiplied(151, 151, 159, if resp.hovered() { 51 } else { 31 });
            ui.painter().rect(b, 8, fill, Stroke::new(1.0, Color32::from_rgba_unmultiplied(151, 151, 159, 41)), egui::StrokeKind::Inside);
            ui.painter().galley(b.center() - label.size() / 2.0, label, pal().text_normal);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            more = resp.clicked();
        });
    });
    app.rects.insert("source_pop".into(), area.response.rect);
    if more {
        app.source_pop = None;
        app.stickers = Some(at);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("sticker_goto"), pack.to_string()));
    } else if popups::dismissed(ctx, &area.response) {
        app.source_pop = None;
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(p) = &app.source_pop else { return };
    let (id, sticker, animated, format, at, name) = (p.id, p.sticker, p.animated, p.format, p.at, p.name.clone());
    let pack = app.packs.iter().find(|p| sticker && p.stickers.iter().any(|s| s.0 == id)).map(|p| (p.name.clone(), p.stickers.iter().filter(|s| s.0 != id).take(3).cloned().collect::<Vec<_>>()));
    if let Some((pack, others)) = pack {
        return pack_pop(app, ctx, at, &name, &pack, &others);
    }
    let kind = if sticker { "sticker" } else { "emoji" };
    let home = if sticker {
        app.store.guilds.values().find(|g| g.stickers.iter().any(|s| s.0 == id))
    } else {
        app.store.guilds.values().find(|g| g.emojis.iter().any(|e| e.id == id))
    };
    let mine = home.map(|g| Server { id: g.id, name: g.name.clone(), icon: g.icon_url(), online: 0, open: false });
    let found = app.sources.get(&(sticker, id)).filter(|v| !v.is_null()).map(|v| server(v));
    let loading = mine.is_none() && !app.sources.contains_key(&(sticker, id)) || app.sources.get(&(sticker, id)).is_some_and(Value::is_null);
    let src = mine.is_some();
    let srv = mine.or(found.flatten());
    let (mut join, mut more) = (false, false);
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("source_pop"));
    super::motion::slide(ctx, layer, vec2(0.0, 1.0));
    let area = egui::Area::new(egui::Id::new("source_pop")).order(egui::Order::Foreground).fixed_pos(at).constrain(true).show(ctx, |ui| {
        let shadow = ui.style().visuals.popup_shadow;
        Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
            ui.set_width(288.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            Frame::new().inner_margin(16).show(ui, |ui| {
                ui.set_width(256.0);
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    let url = if sticker { crate::api::sticker(id, format, 96) } else { crate::markdown::emoji_url(id, animated) };
                    egui::Image::from_uri(crate::gif::uri(ui.ctx(), &url, true)).show_loading_spinner(false).paint_at(ui, r);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width(196.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let title = if sticker { name.clone() } else { format!(":{name}:") };
                        ui.label(egui::RichText::new(title).font(semi(16.0)).color(pal().header_primary));
                        let line = match (&srv, src, loading) {
                            (_, true, _) => format!("This {kind} is from your server."),
                            (Some(_), ..) => format!("Want to use this {kind} everywhere? Join the server."),
                            (None, _, true) => String::new(),
                            (None, ..) => format!("This {kind} is from a server that is either invite-only or unavailable."),
                        };
                        ui.label(egui::RichText::new(line).font(reg(14.0)).color(pal().text_normal));
                    });
                });
                if loading {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 20.0, pal().text_muted));
                }
                if let Some(s) = srv.as_ref().filter(|_| !src) {
                    ui.add_space(16.0);
                    let (b, resp) = ui.allocate_exact_size(vec2(256.0, 32.0), Sense::click());
                    super::settings::mark(ui, "source_pop:join", b);
                    ui.painter().rect_filled(b, CornerRadius::same(8), crate::ui::motion::mix(BLURPLE, Color32::from_rgb(0x44, 0x52, 0xbb), crate::ui::motion::hot(ui.ctx(), &resp)));
                    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Join Server", medium(14.0), Color32::WHITE);
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    join = resp.clicked() && s.id != 0;
                }
            });
            if let Some(s) = &srv {
                let (rule, _) = ui.allocate_exact_size(vec2(288.0, 1.0), Sense::hover());
                ui.painter().hline(rule.x_range(), rule.center().y, Stroke::new(1.0, pal().border_subtle));
                Frame::new().fill(pal().bg_secondary).corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }).inner_margin(16).show(ui, |ui| {
                    ui.set_width(256.0);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let head = if src { format!("This {kind} is from") } else { format!("Get this {kind} from") };
                    ui.label(egui::RichText::new(head).font(reg(14.0)).color(pal().text_muted));
                    row(ui, s, src);
                    if sticker {
                        let (r, resp) = ui.allocate_exact_size(vec2(256.0, 18.0), Sense::click());
                        super::settings::mark(ui, "source_pop:more", r);
                        let col = if resp.hovered() { pal().header_primary } else { pal().text_muted };
                        let g = ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "Show More Stickers", reg(12.0), col);
                        super::icons::draw(ui.painter(), super::icons::Icon::ChevronRight, Rect::from_center_size(pos2(g.right() + 10.0, r.center().y), vec2(12.0, 12.0)), col);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        more = resp.clicked();
                    }
                });
            }
        });
    });
    app.rects.insert("source_pop".into(), area.response.rect);
    if join && let Some(s) = &srv {
        app.rest("PUT", format!("/guilds/{}/members/@me", s.id), Some(json!({})), Tag::Quiet);
        app.source_pop = None;
    } else if more {
        app.source_pop = None;
        app.stickers = Some(at);
    } else if popups::dismissed(ctx, &area.response) {
        app.source_pop = None;
    }
}
