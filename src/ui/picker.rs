use egui::{Align2, Color32, CornerRadius, Frame, Image, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{App, Pick};
use crate::markdown::twemoji;
use crate::state::{Emoji, Id, ReactEmoji};
use crate::theme::*;


/// Every name that types an emoji, default skin tone only. Gemoji gives several per emoji
/// (`+1` and `thumbsup`), and discord spells flags `flag_us` where gemoji says `us`.
pub fn common() -> impl Iterator<Item = (&'static str, String)> {
    emojis::iter()
        .filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default))
        .flat_map(|e| {
            let flag = e.group() == emojis::Group::Flags;
            e.shortcodes().flat_map(move |n| {
                let alias = (flag && n.len() == 2).then(|| format!("flag_{n}"));
                [Some(n.to_string()), alias].into_iter().flatten().map(move |n| (e.as_str(), n))
            })
        })
}

pub fn unicode(name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    let name = name.strip_prefix("flag_").filter(|n| n.len() == 2).unwrap_or(&name);
    emojis::get_by_shortcode(name).map(|e| e.as_str())
}

fn emoji_list(group: Option<emojis::Group>) -> impl Iterator<Item = (&'static str, &'static str)> {
    emojis::iter()
        .filter(move |e| group.is_none_or(|g| e.group() == g))
        .filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default))
        .filter_map(|e| Some((e.as_str(), e.shortcode()?)))
}

// the face each category shows in the picker's left column
const GROUP_FACES: [(&str, &str); 8] = [
    ("People", "🙂"),
    ("Nature", "🌿"),
    ("Food", "🍎"),
    ("Activities", "⚽"),
    ("Travel", "🚗"),
    ("Objects", "💡"),
    ("Symbols", "❤️"),
    ("Flags", "🏁"),
];

const GROUPS: [(emojis::Group, &str); 8] = [
    (emojis::Group::SmileysAndEmotion, "People"),
    (emojis::Group::AnimalsAndNature, "Nature"),
    (emojis::Group::FoodAndDrink, "Food"),
    (emojis::Group::Activities, "Activities"),
    (emojis::Group::TravelAndPlaces, "Travel"),
    (emojis::Group::Objects, "Objects"),
    (emojis::Group::Symbols, "Symbols"),
    (emojis::Group::Flags, "Flags"),
];

enum Chosen {
    Custom(Emoji),
    Unicode(&'static str),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Gifs,
    Stickers,
    Emoji,
}

/// Discord's expression picker sits 8 above the composer and 24 in from its right end.
pub fn anchor(app: &App) -> Pos2 {
    app.rects.get("composer").map_or(pos2(800.0, 600.0), |r| pos2(r.right() - 24.0, r.top() - 8.0))
}

/// The composer's buttons and the picker's tabs: open that tab, or close it if it's showing.
pub fn toggle(app: &mut App, tab: Tab) {
    let open = match tab {
        Tab::Gifs => app.gifs.is_some(),
        Tab::Stickers => app.stickers.is_some(),
        Tab::Emoji => matches!(app.picker, Some((Pick::Composer, _))),
    };
    app.gifs = None;
    app.stickers = None;
    app.picker = None;
    if open {
        return;
    }
    let at = anchor(app);
    match tab {
        Tab::Gifs => app.open_gifs(at),
        Tab::Stickers => app.stickers = Some(at),
        Tab::Emoji => app.picker = Some((Pick::Composer, at)),
    }
}

/// The picker's frame: 498 by 486 with GIFs, Stickers and Emoji along the top, or 396 with no
/// tabs when it's only picking a reaction. Hands back the area and any tab clicked.
pub fn shell(ctx: &egui::Context, at: Pos2, pivot: Align2, tab: Option<Tab>, body: impl FnOnce(&mut Ui)) -> (egui::Response, Option<Tab>) {
    let mut to = None;
    let h = if tab.is_some() { 486.0 } else { 396.0 };
    let area = egui::Area::new(egui::Id::new("expression"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            let shadow = ui.style().visuals.popup_shadow;
            Frame::new().fill(pal().bg_secondary).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).shadow(shadow).show(ui, |ui| {
                ui.set_width(496.0);
                ui.set_height(h - 2.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                ui.add_space(16.0);
                if let Some(tab) = tab {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        for (t, label) in [(Tab::Gifs, "GIFs"), (Tab::Stickers, "Stickers"), (Tab::Emoji, "Emoji")] {
                            let on = t == tab;
                            let g = ui.painter().layout_no_wrap(label.into(), semi(14.0), Color32::WHITE);
                            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 30.0), Sense::click());
                            if on || resp.hovered() {
                                ui.painter().rect_filled(r, 8, if on { pal().control_hover } else { MENU_HOVER });
                            }
                            let col = if on { pal().header_primary } else if resp.hovered() { pal().interactive_hover } else { pal().text_normal };
                            ui.painter().galley(r.center() - g.size() / 2.0, g, col);
                            if resp.clicked() && !on {
                                to = Some(t);
                            }
                            ui.add_space(8.0);
                        }
                    });
                    ui.add_space(12.0);
                }
                body(ui);
            });
        });
    (area.response, to)
}

/// The picker's search box: 40 tall, 16 in from each side, the magnifier at its left.
/// `back` swaps the magnifier for an arrow and says whether it was clicked.
pub fn search_box(ui: &mut Ui, q: &mut String, hint: &str, back: Option<&mut bool>) -> egui::Response {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let field = Rect::from_min_max(row.min + vec2(16.0, 0.0), row.max - vec2(16.0, 0.0));
    ui.painter().rect_filled(field, 8, pal().input_bg);
    let inner = Rect::from_min_max(pos2(field.left() + 38.0, field.center().y - 10.0), pos2(field.right() - 12.0, field.center().y + 10.0));
    let resp = ui.put(
        inner,
        egui::TextEdit::singleline(q)
            .hint_text(RichText::new(hint).color(pal().text_muted))
            .font(reg(16.0))
            .frame(Frame::NONE)
            .margin(egui::Margin::ZERO),
    );
    let ring = if resp.has_focus() { BLURPLE } else { pal().border_subtle };
    ui.painter().rect_stroke(field, 8, Stroke::new(1.0, ring), egui::StrokeKind::Inside);
    let ir = Rect::from_center_size(pos2(field.left() + 20.0, field.center().y), vec2(16.0, 16.0));
    match back {
        Some(b) => {
            let hit = ui.interact(ir.expand(6.0), ui.id().with("picker_back"), Sense::click());
            super::icons::draw(ui.painter(), super::icons::Icon::ArrowLeft, ir, if hit.hovered() { pal().interactive_hover } else { pal().header_primary });
            *b = hit.clicked();
        }
        None => super::icons::draw(ui.painter(), super::icons::Icon::Search, ir, pal().header_primary),
    }
    resp
}

/// The hairline under the picker's header.
pub fn header_rule(ui: &mut Ui) {
    ui.add_space(12.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, pal().border_subtle));
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((target, at)) = &app.picker else { return };
    let at = *at;
    let composer = matches!(target, Pick::Composer);
    let guild = app.ui.guild.unwrap_or(0);
    let mut servers: Vec<(Id, String, Option<String>, Vec<(Emoji, bool)>)> = Vec::new();
    for (g, e, ok) in app.store.emojis_for(guild) {
        match servers.last_mut() {
            Some((id, _, _, list)) if *id == g.id => list.push((e.clone(), ok)),
            _ => servers.push((g.id, g.name.clone(), g.icon_url(), vec![(e.clone(), ok)])),
        }
    }
    let query_id = egui::Id::new("picker_q");
    let jump_id = egui::Id::new("picker_jump");
    let tops_id = egui::Id::new("picker_tops");
    let mut q: String = ctx.data_mut(|d| d.get_temp(query_id).unwrap_or_default());
    let jump: Option<f32> = ctx.data_mut(|d| d.remove_temp(jump_id));
    let tops: Vec<f32> = ctx.data(|d| d.get_temp(tops_id)).unwrap_or_default();
    let mut new_tops = Vec::new();
    let mut chosen = None;
    let mut hovered: Option<(String, String, Option<String>)> = None;
    let mut first: Option<(String, String, Option<String>)> = None;
    let mut go = None;

    let (pivot, tab) = if composer { (Align2::RIGHT_BOTTOM, Some(Tab::Emoji)) } else { (Align2::LEFT_TOP, None) };
    let (area, to) = shell(ctx, at, pivot, tab, |ui| {
        search_box(ui, &mut q, "Find the perfect emoji", None).request_focus();
        header_rule(ui);
        let rest = ui.available_rect_before_wrap();
        let foot = Rect::from_min_max(pos2(rest.left(), rest.bottom() - 48.0), rest.max);
        let rail = Rect::from_min_max(rest.min, pos2(rest.left() + 48.0, foot.top()));
        let list = Rect::from_min_max(pos2(rail.right(), rest.top()), pos2(rest.right(), foot.top()));
        ui.painter().rect_filled(rail, CornerRadius::ZERO, pal().bg_tertiary);
        let ql = q.to_lowercase();
        // one icon per section down the left: servers by their icon, then discord's categories
        let mut marks: Vec<(Option<String>, &str, String)> = servers.iter().map(|(_, name, icon, _)| (icon.clone(), "", name.clone())).collect();
        marks.extend(GROUPS.iter().map(|(_, t)| (None, *t, t.to_string())));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail.shrink2(vec2(8.0, 8.0))), |ui| {
            egui::ScrollArea::vertical().id_salt("picker_rail").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, (icon, group, name)) in marks.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r.expand(2.0), 8, MENU_HOVER);
                    }
                    match icon {
                        Some(url) => {
                            Image::from_uri(url.clone()).corner_radius(CornerRadius::same(16)).show_loading_spinner(false).paint_at(ui, r);
                        }
                        None if group.is_empty() => {
                            ui.painter().circle_filled(r.center(), 16.0, pal().bg_accent);
                            let initial: String = name.chars().filter(|c| !c.is_whitespace()).take(2).collect();
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, semi(12.0), pal().text_normal);
                        }
                        None => {
                            let face = GROUP_FACES.iter().find(|(t, _)| t == group).map_or("🙂", |(_, f)| f);
                            Image::from_uri(twemoji(face)).tint(Color32::from_gray(170)).show_loading_spinner(false).paint_at(ui, r.shrink(6.0));
                        }
                    }
                    if resp.on_hover_text(name.as_str()).clicked() {
                        go = Some(i);
                    }
                }
            });
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            let mut sa = egui::ScrollArea::vertical().id_salt("picker_list").auto_shrink([false, false]);
            if let Some(y) = jump {
                sa = sa.vertical_scroll_offset(y);
            }
            sa.show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let top = ui.min_rect().top();
                for (_, name, _, list) in &servers {
                    let customs: Vec<&(Emoji, bool)> = list.iter().filter(|(e, _)| ql.is_empty() || e.name.to_lowercase().contains(&ql)).collect();
                    new_tops.push(ui.cursor().top() - top);
                    if customs.is_empty() {
                        continue;
                    }
                    section(ui, name);
                    grid(ui, customs.len(), |ui, i, r| {
                        let (e, ok) = customs[i];
                        let tint = if *ok { Color32::WHITE } else { Color32::from_white_alpha(90) };
                        Image::from_uri(crate::gif::uri(ui.ctx(), &e.url(), ui.is_rect_visible(r))).tint(tint).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                    }, |i| {
                        if customs[i].1 {
                            chosen = Some(Chosen::Custom(customs[i].0.clone()));
                        }
                    }, |i, hot| {
                        let (e, ok) = customs[i];
                        let it = (e.url(), format!(":{}:", e.name), Some(if *ok { format!("from {name}") } else { "Needs Nitro or a role here".into() }));
                        if first.is_none() {
                            first = Some(it.clone());
                        }
                        if hot {
                            hovered = Some(it);
                        }
                    });
                }
                for (group, title) in GROUPS {
                    let uni: Vec<(&str, &str)> = emoji_list(Some(group))
                        .filter(|(g, n)| ql.is_empty() || n.contains(&ql) || emojis::get(g).is_some_and(|e| e.shortcodes().any(|s| s.contains(&ql))))
                        .collect();
                    new_tops.push(ui.cursor().top() - top);
                    if uni.is_empty() {
                        continue;
                    }
                    section(ui, title);
                    grid(ui, uni.len(), |ui, i, r| {
                        Image::from_uri(twemoji(uni[i].0)).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                    }, |i| {
                        chosen = Some(Chosen::Unicode(uni[i].0));
                    }, |i, hot| {
                        let it = (twemoji(uni[i].0), format!(":{}:", uni[i].1), None);
                        if first.is_none() {
                            first = Some(it.clone());
                        }
                        if hot {
                            hovered = Some(it);
                        }
                    });
                }
                ui.add_space(8.0);
            });
        });
        // discord's inspector: the emoji under the pointer, or the first one, and its name
        let p = ui.painter();
        p.rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 7, se: 7 }, pal().bg_tertiary);
        p.hline(foot.x_range(), foot.top(), Stroke::new(1.0, pal().border_normal));
        if let Some((url, name, sub)) = hovered.or(first) {
            Image::from_uri(crate::gif::uri(ui.ctx(), &url, true)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(foot.left() + 30.0, foot.center().y), vec2(28.0, 28.0)));
            let y = if sub.is_some() { foot.center().y - 8.0 } else { foot.center().y };
            super::text::paint(ui.painter(), pos2(foot.left() + 52.0, y), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, 400.0);
            if let Some(s) = sub {
                ui.painter().text(pos2(foot.left() + 52.0, foot.center().y + 10.0), Align2::LEFT_CENTER, s, reg(12.0), pal().text_muted);
            }
        }
        ui.allocate_rect(rest, Sense::hover());
    });

    ctx.data_mut(|d| {
        d.insert_temp(query_id, q);
        d.insert_temp(tops_id, new_tops);
        if let Some(y) = go.and_then(|i| tops.get(i)) {
            d.insert_temp(jump_id, *y);
        }
    });
    if let Some(t) = to {
        toggle(app, t);
        return;
    }
    let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !area.contains_pointer() && !on_picker_button(app, ctx);
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(c) = chosen {
        match target {
            Pick::Composer => {
                let text = match &c {
                    Chosen::Custom(e) => format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id),
                    Chosen::Unicode(s) => s.to_string(),
                };
                if let Some(ch) = app.cur() {
                    app.drafts.entry(ch).or_default().push_str(&text);
                }
                app.focus_composer = true;
            }
            Pick::React(ch, id) => {
                let em = match c {
                    Chosen::Custom(e) => ReactEmoji { id: e.id, name: e.name, animated: e.animated },
                    Chosen::Unicode(s) => ReactEmoji { id: 0, name: s.to_string(), animated: false },
                };
                let (ch, id) = (*ch, *id);
                let mine = app.store.find_msg(ch, id).is_some_and(|m| m.reactions.iter().any(|r| r.emoji == em && r.me));
                if !mine {
                    app.react(ch, id, em, true);
                }
            }
        }
        app.picker = None;
        ctx.data_mut(|d| d.remove::<String>(query_id));
    } else if clicked_out || esc {
        app.picker = None;
        ctx.data_mut(|d| d.remove::<String>(query_id));
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title).font(medium(14.0)).color(pal().header_secondary));
    });
    ui.add_space(4.0);
}

fn grid(ui: &mut Ui, n: usize, mut paint: impl FnMut(&Ui, usize, Rect), mut pick: impl FnMut(usize), mut hover: impl FnMut(usize, bool)) {
    let cols = 9;
    let cell = 48.0;
    for row in 0..n.div_ceil(cols) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            for c in 0..cols {
                let i = row * cols + c;
                if i >= n {
                    break;
                }
                let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().control_hover);
                }
                if ui.is_rect_visible(r) {
                    paint(ui, i, r);
                }
                hover(i, resp.hovered());
                if resp.clicked() {
                    pick(i);
                }
            }
        });
    }
}

/// Discord's sticker picker: every server's stickers, the current one first; another
/// server's need nitro.
pub fn stickers(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.stickers else { return };
    let Some(ch) = app.cur() else { return };
    let here = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let nitro = app.store.nitro();
    let order = std::iter::once(here).chain(app.store.guild_order.iter().copied().filter(|g| *g != here));
    let groups: Vec<(String, Vec<(Id, String, u8)>, bool)> = order
        .filter_map(|g| app.store.guilds.get(&g))
        .filter(|g| !g.stickers.is_empty())
        .map(|g| (g.name.clone(), g.stickers.clone(), nitro || g.id == here))
        .collect();
    let qid = egui::Id::new("sticker_q");
    let mut q: String = ctx.data(|d| d.get_temp(qid)).unwrap_or_default();
    let ql = q.to_lowercase();
    let mut chosen = None;
    let (area, to) = shell(ctx, at, Align2::RIGHT_BOTTOM, Some(Tab::Stickers), |ui| {
        search_box(ui, &mut q, "Search for stickers", None).request_focus();
        header_rule(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if groups.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("No stickers here yet.").color(pal().text_muted)));
            }
            for (name, list, ok) in &groups {
                let list: Vec<_> = list.iter().filter(|(_, n, _)| ql.is_empty() || n.to_lowercase().contains(&ql)).collect();
                if list.is_empty() {
                    continue;
                }
                section(ui, name);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for (id, sname, fmt) in list {
                        let (r, resp) = ui.allocate_exact_size(vec2(112.0, 112.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(8), pal().control_hover);
                        }
                        // lottie stickers (format 3) have no still image to show
                        if *fmt != 3 {
                            let ext = if *fmt == 4 { "gif" } else { "png" };
                            let url = format!("https://media.discordapp.net/stickers/{id}.{ext}?size=160");
                            let tint = if *ok { Color32::WHITE } else { Color32::from_white_alpha(90) };
                            Image::from_uri(crate::gif::uri(ui.ctx(), &url, ui.is_rect_visible(r))).tint(tint).show_loading_spinner(false).paint_at(ui, r.shrink(8.0));
                        }
                        let resp = resp.on_hover_text(if *ok { sname.clone() } else { format!("{sname} needs Nitro here") });
                        if resp.clicked() && *ok {
                            chosen = Some(*id);
                        }
                    }
                });
            }
            ui.add_space(8.0);
        });
    });
    ctx.data_mut(|d| d.insert_temp(qid, q));
    if let Some(t) = to {
        toggle(app, t);
        return;
    }
    if let Some(id) = chosen {
        let nonce = crate::state::make_nonce();
        app.net.send(crate::net::Cmd::Send { channel: ch, content: String::new(), nonce, reply: app.reply.take().map(|r| (r, app.reply_ping)), files: Vec::new(), stickers: vec![id] });
        app.jump_to_present(ch);
        app.stickers = None;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (ctx.input(|i| i.pointer.any_pressed()) && !area.contains_pointer() && !on_picker_button(app, ctx)) {
        app.stickers = None;
        ctx.data_mut(|d| d.remove::<String>(qid));
    }
}

/// The composer's picker buttons toggle the picker themselves; a press there isn't a click away.
pub fn on_picker_button(app: &App, ctx: &egui::Context) -> bool {
    let at = ctx.pointer_hover_pos();
    ["gif_button", "emoji_button", "sticker_button"].iter().any(|k| app.rects.get(*k).zip(at).is_some_and(|(r, p)| r.contains(p)))
}
