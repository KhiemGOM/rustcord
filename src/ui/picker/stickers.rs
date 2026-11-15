//! Discord's sticker picker and its own sticker packs.

use egui::{Align2, Color32, CornerRadius, Image, Rect, Sense, Stroke, pos2, vec2};

use super::super::App;
use super::*;
use crate::net::Tag;
use crate::state::Id;
use std::time::{Duration, Instant};

/// One of discord's own sticker packs, as `GET /sticker-packs` lists it.
pub struct Pack {
    pub name: String,
    pub cover: Id,
    pub stickers: Vec<(Id, String, u8)>,
    pub tags: Vec<String>,
}

impl App {
    pub fn load_packs(&mut self) {
        let keep = if self.packs.is_empty() { 10 } else { 3600 };
        if self.packs_at.is_some_and(|t| t.elapsed() < Duration::from_secs(keep)) {
            return;
        }
        self.packs_at = Some(Instant::now());
        self.rest("GET", "/sticker-packs?locale=en-US".into(), None, Tag::StickerPacks);
    }

    pub fn set_packs(&mut self, v: &serde_json::Value) {
        self.packs = v["sticker_packs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let list = p["stickers"].as_array().map(Vec::as_slice).unwrap_or_default();
                Pack {
                    name: p["name"].as_str().unwrap_or_default().to_string(),
                    cover: crate::state::sid(&p["cover_sticker_id"]),
                    stickers: list.iter().map(|s| (crate::state::sid(&s["id"]), s["name"].as_str().unwrap_or_default().to_string(), s["format_type"].as_u64().unwrap_or(1) as u8)).collect(),
                    tags: list.iter().map(|s| s["tags"].as_str().unwrap_or_default().to_lowercase()).collect(),
                }
            })
            .filter(|p| !p.stickers.is_empty())
            .collect();
    }
}

/// Discord's sticker picker: every server's stickers, the current one first; another
/// server's need nitro. Discord's own packs follow, every one of them sendable.
pub fn stickers(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.stickers else { return ctx.data_mut(|d| d.remove::<String>(egui::Id::new("sticker_q"))) };
    let Some(ch) = app.cur() else { return };
    app.load_packs();
    let here = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let nitro = app.store.nitro();
    let order = std::iter::once(here).chain(app.store.guild_order.iter().copied().filter(|g| *g != here));
    // (server, its icon, its stickers, whether we may send them here)
    let mut groups: Vec<(String, Option<String>, Vec<(Id, String, u8)>, bool)> = order
        .filter_map(|g| app.store.guilds.get(&g))
        .filter(|g| !g.stickers.is_empty())
        .map(|g| (g.name.clone(), g.icon_url(), g.stickers.clone(), nitro || g.id == here))
        .collect();
    let url = |id: &Id, fmt: u8| crate::api::sticker(*id, fmt, 160);
    let mut tags: std::collections::HashMap<Id, &str> = std::collections::HashMap::new();
    for p in &app.packs {
        let cover = p.stickers.iter().find(|s| s.0 == p.cover).or(p.stickers.first()).map(|s| url(&s.0, s.2));
        groups.push((p.name.clone(), cover, p.stickers.clone(), true));
        tags.extend(p.stickers.iter().zip(&p.tags).map(|(s, t)| (s.0, t.as_str())));
    }
    let qid = egui::Id::new("sticker_q");
    let jump_id = egui::Id::new("sticker_jump");
    let tops_id = egui::Id::new("sticker_tops");
    let shut_id = egui::Id::new("sticker_shut");
    if let Some(first) = app.sticker_q.take() {
        ctx.data_mut(|d| d.insert_temp(qid, first));
    }
    let mut q: String = ctx.data(|d| d.get_temp(qid)).unwrap_or_default();
    let jump: Option<f32> = ctx.data_mut(|d| d.remove_temp(jump_id));
    let tops: Vec<f32> = ctx.data(|d| d.get_temp(tops_id)).unwrap_or_default();
    let mut shut: Vec<String> = ctx.data(|d| d.get_temp(shut_id)).unwrap_or_default();
    let mut new_tops = Vec::new();
    let ql = q.to_lowercase();
    let find = |id: Id| groups.iter().find_map(|(name, icon, l, ok)| l.iter().find(|s| s.0 == id).map(|s| (s.clone(), name.clone(), icon.clone(), *ok)));
    let recent: Vec<_> = if ql.is_empty() { app.ui.frequent_stickers.iter().filter_map(|id| find(*id)).filter(|s| s.3).collect() } else { Vec::new() };
    let favs: Vec<_> = if ql.is_empty() { app.ui.fav_stickers.iter().filter_map(|id| find(*id)).collect() } else { Vec::new() };
    let mut fav_toggle = None;
    let mut chosen = None;
    let mut go = None;
    let mut hovered: Option<((Id, String, u8), String)> = None;
    let (area, to) = shell(ctx, at, Align2::RIGHT_BOTTOM, Some(Tab::Stickers), |ui| {
        search_box(ui, &mut q, "Find the perfect sticker", None).request_focus();
        header_rule(ui);
        let rest = ui.available_rect_before_wrap();
        let foot = Rect::from_min_max(pos2(rest.left() + 48.0, rest.bottom() - 48.0), rest.max);
        let rail = Rect::from_min_max(rest.min, pos2(rest.left() + 48.0, rest.bottom()));
        let list = Rect::from_min_max(pos2(rail.right(), rest.top()), pos2(rest.right(), foot.top()));
        ui.painter().rect_filled(rail, CornerRadius { sw: 7, ..CornerRadius::ZERO }, pal().bg_tertiary);
        // the left column: frequently used, then each server by its icon
        let mut marks: Vec<(Option<&str>, &str)> = Vec::new();
        if !favs.is_empty() {
            marks.push((None, "Favorites"));
        }
        if !recent.is_empty() {
            marks.push((None, "Frequently Used"));
        }
        marks.extend(groups.iter().map(|(name, icon, _, _)| (icon.as_deref(), name.as_str())));
        if go.is_none() && !tops.is_empty() && let Some(n) = ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("sticker_goto"))) {
            go = marks.iter().position(|(_, m)| *m == n);
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail.with_max_y(foot.top()).shrink2(vec2(8.0, 8.0))), |ui| {
            egui::ScrollArea::vertical().id_salt("sticker_rail").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, (icon, name)) in marks.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    crate::ui::motion::wash(ui, &resp, r.expand(2.0), 8, MENU_HOVER, 0.05);
                    match icon {
                        Some(url) => {
                            Image::from_uri(url.to_string()).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, r);
                        }
                        None if *name == "Favorites" => crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Star, r.shrink(4.0), pal().header_secondary),
                        None if *name == "Frequently Used" => crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Clock, r.shrink(4.0), pal().header_secondary),
                        None => {
                            ui.painter().rect_filled(r, 8, pal().bg_accent);
                            let initial: String = name.chars().filter(|c| !c.is_whitespace()).take(2).collect();
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, semi(12.0), pal().text_normal);
                        }
                    }
                    if resp.on_hover_text(*name).clicked() {
                        go = Some(i);
                    }
                }
            });
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            let mut sa = egui::ScrollArea::vertical().id_salt("sticker_list").auto_shrink([false, false]);
            if let Some(y) = jump {
                sa = sa.vertical_scroll_offset(y);
            }
            sa.show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let top = ui.min_rect().top();
                if groups.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("No stickers here yet.").color(pal().text_muted)));
                }
                let mut sections: Vec<(&str, Lead, Vec<((Id, String, u8), String, bool)>)> = Vec::new();
                if !favs.is_empty() {
                    sections.push(("Favorites", Lead::Icon(crate::ui::icons::Icon::Star), favs.iter().map(|(s, g, _, ok)| (s.clone(), g.clone(), *ok)).collect()));
                }
                if !recent.is_empty() {
                    sections.push(("Frequently Used", Lead::Icon(crate::ui::icons::Icon::Clock), recent.iter().map(|(s, g, _, ok)| (s.clone(), g.clone(), *ok)).collect()));
                }
                for (name, icon, l, ok) in &groups {
                    let l = l
                        .iter()
                        .filter(|(id, n, _)| ql.is_empty() || n.to_lowercase().contains(&ql) || name.to_lowercase().contains(&ql) || tags.get(id).is_some_and(|t| t.contains(&ql)))
                        .map(|s| (s.clone(), name.clone(), *ok))
                        .collect();
                    sections.push((name, icon.as_deref().map_or(Lead::None, Lead::Image), l));
                }
                for (title, lead, l) in sections {
                    new_tops.push(ui.cursor().top() - top);
                    if l.is_empty() || !section(ui, title, lead, &mut shut) {
                        continue;
                    }
                    // four to a row, like discord's
                    for row in l.chunks(4) {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (st, from, ok) in row {
                                let (r, resp) = ui.allocate_exact_size(vec2(104.0, 104.0), Sense::click());
                                crate::ui::settings::mark(ui, &format!("sticker:{}", st.0), r);
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, CornerRadius::same(8), pal().control_hover);
                                    hovered = Some((st.clone(), format!("from {from}")));
                                }
                                if ui.is_rect_visible(r) {
                                    let tint = if *ok { Color32::WHITE } else { Color32::from_white_alpha(90) };
                                    let play = match app.settings.stickers {
                                        crate::cache::Animate::Always => true,
                                        crate::cache::Animate::OnInteraction => resp.hovered(),
                                        crate::cache::Animate::Never => false,
                                    };
                                    Image::from_uri(crate::gif::uri(ui.ctx(), &url(&st.0, st.2), play)).tint(tint).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                                }
                                let resp = resp.on_hover_text(if *ok { st.1.clone() } else { format!("{} needs Nitro here", st.1) });
                                let fav = app.ui.fav_stickers.contains(&st.0);
                                resp.context_menu(|ui| {
                                    if crate::ui::menu_item(ui, Some(crate::ui::icons::Icon::Star), if fav { "Unfavorite Sticker" } else { "Favorite Sticker" }, false) {
                                        fav_toggle = Some(st.0);
                                    }
                                });
                                if resp.clicked() && ui.input(|i| i.modifiers.alt) {
                                    fav_toggle = Some(st.0);
                                } else if resp.clicked() && *ok {
                                    chosen = Some(st.0);
                                }
                            }
                        });
                        ui.add_space(4.0);
                    }
                }
                ui.add_space(8.0);
            });
        });
        let p = ui.painter();
        p.rect_filled(foot, CornerRadius { se: 7, ..CornerRadius::ZERO }, pal().bg_tertiary);
        p.hline(foot.x_range(), foot.top(), Stroke::new(1.0, pal().border_normal));
        if let Some(((id, name, fmt), from)) = hovered {
            if fmt != 3 {
                Image::from_uri(crate::gif::uri(ui.ctx(), &url(&id, fmt), true)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(foot.left() + 30.0, foot.center().y), vec2(28.0, 28.0)));
            }
            crate::ui::text::paint(ui.painter(), pos2(foot.left() + 52.0, foot.center().y - 8.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, 400.0);
            from_line(ui, pos2(foot.left() + 52.0, foot.center().y + 10.0), &from);
        }
        ui.allocate_rect(rest, Sense::hover());
    });
    ctx.data_mut(|d| {
        d.insert_temp(qid, q);
        d.insert_temp(tops_id, new_tops);
        d.insert_temp(shut_id, shut);
        if let Some(y) = go.and_then(|i| tops.get(i)) {
            d.insert_temp(jump_id, *y);
        }
    });
    if let Some(t) = to {
        toggle(app, t);
        return;
    }
    if let Some(id) = fav_toggle {
        if app.ui.fav_stickers.contains(&id) {
            app.ui.fav_stickers.retain(|s| *s != id);
        } else {
            app.ui.fav_stickers.insert(0, id);
        }
        return;
    }
    if let Some(id) = chosen {
        let nonce = crate::state::make_nonce();
        app.net.send(crate::net::Cmd::Send { channel: ch, content: String::new(), nonce, reply: app.reply.take().map(|r| (r, app.reply_ping)), files: Vec::new(), stickers: vec![id], tts: false });
        app.jump_to_present(ch);
        crate::ui::favs::used_sticker(app, id);
        app.stickers = None;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (crate::ui::popups::pressed_outside(ctx, &area) && !on_picker_button(app, ctx)) {
        app.stickers = None;
        ctx.data_mut(|d| d.remove::<String>(qid));
    }
}
