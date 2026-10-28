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

#[derive(Clone)]
enum Chosen {
    Custom(Emoji),
    Unicode(&'static str),
}

impl Chosen {
    fn react(&self) -> ReactEmoji {
        match self {
            Chosen::Custom(e) => ReactEmoji { id: e.id, name: e.name.clone(), animated: e.animated },
            Chosen::Unicode(s) => ReactEmoji { id: 0, name: s.to_string(), animated: false },
        }
    }
}

/// Where the keyboard is in the picker's grid, counted across every section in order, and
/// whether it just moved there.
#[derive(Clone, Copy)]
struct Kbd {
    sel: Option<usize>,
    moved: bool,
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
    search_box_in(ui, q, hint, back, 0.0)
}

/// The search box with `reserve` points kept free at its right end.
pub fn search_box_in(ui: &mut Ui, q: &mut String, hint: &str, back: Option<&mut bool>, reserve: f32) -> egui::Response {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let field = Rect::from_min_max(row.min + vec2(16.0, 0.0), row.max - vec2(16.0 + reserve, 0.0));
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
        let ok = ok || (composer && super::outgoing::bypass_emoji());
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
    // the first usable hit, which Enter picks while searching
    let mut best: Option<Chosen> = None;
    let mut hovered: Option<(String, String, Option<String>)> = None;
    let mut first: Option<(String, String, Option<String>)> = None;
    let mut go = None;

    let tone = app.ui.skin;
    let frequent: Vec<ReactEmoji> = app.ui.frequent.clone();
    let shut_id = egui::Id::new("picker_shut");
    let mut shut: Vec<String> = ctx.data(|d| d.get_temp(shut_id)).unwrap_or_default();
    let tones_id = egui::Id::new("picker_tones");
    let mut tones_open: bool = ctx.data(|d| d.get_temp(tones_id)).unwrap_or(false);
    let mut new_tone = None;
    let mut add_emoji = false;
    // the arrows walk the grid nine to a row, the search box keeping its focus for typing
    let sel_id = egui::Id::new("picker_sel");
    let (last_sel, last_len, last_q): (Option<usize>, usize, String) = ctx.data(|d| d.get_temp(sel_id)).unwrap_or_default();
    let step: i64 = ctx.input_mut(|i| {
        let k = |i: &mut egui::InputState, key| super::take_key(i, egui::Modifiers::NONE, key);
        if k(i, egui::Key::ArrowRight) {
            1
        } else if k(i, egui::Key::ArrowLeft) {
            -1
        } else if k(i, egui::Key::ArrowDown) {
            9
        } else if k(i, egui::Key::ArrowUp) {
            -9
        } else {
            0
        }
    });
    let last_sel = last_sel.filter(|_| last_q == q);
    let sel = match (last_sel, step) {
        (s, 0) => s,
        (None, _) => Some(0),
        (Some(s), d) => Some((s as i64 + d).clamp(0, last_len.saturating_sub(1) as i64) as usize),
    };
    let kbd = Kbd { sel, moved: step != 0 };
    let mut order: Vec<Chosen> = Vec::new();
    let mut menu_at: Option<(ReactEmoji, Pos2)> = None;
    let favs: Vec<ReactEmoji> = app.settings.fav_emoji.clone();
    let (pivot, tab) = if composer { (Align2::RIGHT_BOTTOM, Some(Tab::Emoji)) } else { (Align2::LEFT_TOP, None) };
    let (area, to) = shell(ctx, at, pivot, tab, |ui| {
        let row = ui.cursor().min;
        search_box_in(ui, &mut q, "Find the perfect emoji", None, 156.0).request_focus();
        // discord's skin tone hand and its add emoji button, right of the search box
        let hand = Rect::from_min_size(pos2(ui.max_rect().right() - 16.0 - 108.0 - 8.0 - 40.0, row.y), vec2(40.0, 40.0));
        let hr = ui.interact(hand, ui.id().with("skin"), Sense::click());
        if hr.hovered() {
            ui.painter().rect_filled(hand, 8, MENU_HOVER);
        }
        Image::from_uri(twemoji(tinted("👏", tone))).show_loading_spinner(false).paint_at(ui, hand.shrink(8.0));
        if hr.on_hover_text("Default Skin Tone").clicked() {
            tones_open = !tones_open;
        }
        let add = Rect::from_min_size(pos2(hand.right() + 8.0, row.y), vec2(108.0, 40.0));
        let ar = ui.interact(add, ui.id().with("add_emoji"), Sense::click());
        ui.painter().rect(add, 8, if ar.hovered() { pal().control_hover } else { pal().control }, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
        ui.painter().text(add.center(), Align2::CENTER_CENTER, "Add Emoji", medium(16.0), pal().header_primary);
        add_emoji = ar.clicked();
        header_rule(ui);
        let rest = ui.available_rect_before_wrap();
        let foot = Rect::from_min_max(pos2(rest.left(), rest.bottom() - 48.0), rest.max);
        let rail = Rect::from_min_max(rest.min, pos2(rest.left() + 48.0, foot.top()));
        let list = Rect::from_min_max(pos2(rail.right(), rest.top()), pos2(rest.right(), foot.top()));
        ui.painter().rect_filled(rail, CornerRadius::ZERO, pal().bg_tertiary);
        let ql = q.to_lowercase();
        let recent: Vec<(ReactEmoji, Chosen)> = if ql.is_empty() {
            frequent
                .iter()
                .filter_map(|e| {
                    let c = if e.id == 0 {
                        Chosen::Unicode(emojis::get(&e.name)?.as_str())
                    } else {
                        Chosen::Custom(servers.iter().flat_map(|(_, _, _, l)| l).find(|(x, ok)| x.id == e.id && *ok)?.0.clone())
                    };
                    Some((e.clone(), c))
                })
                .collect()
        } else {
            Vec::new()
        };
        // one mark per section down the left: frequently used, servers by their icon, then discord's categories
        enum Mark<'a> {
            Star,
            Clock,
            Server(Option<&'a str>, &'a str),
            Group(&'a str),
        }
        // the starred ones, which may come from any server you can use them in
        let favorites: Vec<(ReactEmoji, Chosen)> = if ql.is_empty() {
            favs.iter()
                .filter_map(|e| {
                    let c = if e.id == 0 {
                        Chosen::Unicode(emojis::get(&e.name)?.as_str())
                    } else {
                        Chosen::Custom(servers.iter().flat_map(|(_, _, _, l)| l).find(|(x, ok)| x.id == e.id && *ok)?.0.clone())
                    };
                    Some((e.clone(), c))
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut marks: Vec<Mark> = Vec::new();
        if !favorites.is_empty() {
            marks.push(Mark::Star);
        }
        if !recent.is_empty() {
            marks.push(Mark::Clock);
        }
        marks.extend(servers.iter().map(|(_, name, icon, _)| Mark::Server(icon.as_deref(), name.as_str())));
        marks.extend(GROUPS.iter().map(|(_, t)| Mark::Group(t)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail.shrink2(vec2(8.0, 8.0))), |ui| {
            egui::ScrollArea::vertical().id_salt("picker_rail").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, mark) in marks.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r.expand(2.0), 8, MENU_HOVER);
                    }
                    let tip = match mark {
                        Mark::Star => {
                            super::icons::draw(ui.painter(), super::icons::Icon::Star, r.shrink(6.0), pal().header_secondary);
                            "Favorites"
                        }
                        Mark::Clock => {
                            super::icons::draw(ui.painter(), super::icons::Icon::Clock, r.shrink(6.0), pal().header_secondary);
                            "Frequently Used"
                        }
                        Mark::Server(Some(url), name) => {
                            Image::from_uri(url.to_string()).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, r);
                            name
                        }
                        Mark::Server(None, name) => {
                            ui.painter().rect_filled(r, 8, pal().bg_accent);
                            let initial: String = name.chars().filter(|c| !c.is_whitespace()).take(2).collect();
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, semi(12.0), pal().text_normal);
                            name
                        }
                        Mark::Group(group) => {
                            let face = GROUP_FACES.iter().find(|(t, _)| t == group).map_or("🙂", |(_, f)| f);
                            Image::from_uri(twemoji(face)).tint(Color32::from_gray(170)).show_loading_spinner(false).paint_at(ui, r.shrink(6.0));
                            group
                        }
                    };
                    if resp.on_hover_text(tip).clicked() {
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
                for (title, icon, list) in [("Favorites", super::icons::Icon::Star, &favorites), ("Frequently Used", super::icons::Icon::Clock, &recent)] {
                    if list.is_empty() {
                        continue;
                    }
                    new_tops.push(ui.cursor().top() - top);
                    if section(ui, title, Lead::Icon(icon), &mut shut) {
                        let base = order.len();
                        order.extend(list.iter().map(|(_, c)| c.clone()));
                        grid(ui, base, list.len(), kbd, |ui, i, r| {
                            let url = super::chat::emoji_url(&list[i].0);
                            Image::from_uri(crate::gif::uri(ui.ctx(), &url, ui.is_rect_visible(r))).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                        }, |i| {
                            chosen = Some(list[i].1.clone());
                        }, |i, hot| {
                            if hot {
                                let e = &list[i].0;
                                let name = if e.id != 0 { e.name.clone() } else { emojis::get(&e.name).and_then(|x| x.shortcode()).unwrap_or(&e.name).to_string() };
                                hovered = Some((super::chat::emoji_url(e), format!(":{name}:"), None));
                            }
                        }, |i, at| menu_at = Some((list[i].0.clone(), at)));
                    }
                }
                for (_, name, icon, list) in &servers {
                    let customs: Vec<&(Emoji, bool)> = list.iter().filter(|(e, _)| ql.is_empty() || e.name.to_lowercase().contains(&ql)).collect();
                    new_tops.push(ui.cursor().top() - top);
                    if customs.is_empty() || !section(ui, name, icon.as_deref().map_or(Lead::None, Lead::Image), &mut shut) {
                        continue;
                    }
                    let base = order.len();
                    order.extend(customs.iter().map(|(e, _)| Chosen::Custom(e.clone())));
                    grid(ui, base, customs.len(), kbd, |ui, i, r| {
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
                        if best.is_none() && *ok {
                            best = Some(Chosen::Custom(e.clone()));
                        }
                        if hot {
                            hovered = Some(it);
                        }
                    }, |i, at| menu_at = Some((Chosen::Custom(customs[i].0.clone()).react(), at)));
                }
                for (group, title) in GROUPS {
                    let uni: Vec<(&str, &str)> = emoji_list(Some(group))
                        .filter(|(g, n)| ql.is_empty() || n.contains(&ql) || emojis::get(g).is_some_and(|e| e.shortcodes().any(|s| s.contains(&ql))))
                        .map(|(g, n)| (tinted(g, tone), n))
                        .collect();
                    new_tops.push(ui.cursor().top() - top);
                    if uni.is_empty() || !section(ui, title, Lead::None, &mut shut) {
                        continue;
                    }
                    let base = order.len();
                    order.extend(uni.iter().map(|(g, _)| Chosen::Unicode(g)));
                    grid(ui, base, uni.len(), kbd, |ui, i, r| {
                        Image::from_uri(twemoji(uni[i].0)).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                    }, |i| {
                        chosen = Some(Chosen::Unicode(uni[i].0));
                    }, |i, hot| {
                        let it = (twemoji(uni[i].0), format!(":{}:", uni[i].1), None);
                        if first.is_none() {
                            first = Some(it.clone());
                        }
                        if best.is_none() {
                            best = Some(Chosen::Unicode(uni[i].0));
                        }
                        if hot {
                            hovered = Some(it);
                        }
                    }, |i, at| menu_at = Some((Chosen::Unicode(uni[i].0).react(), at)));
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
            ui.painter().text(pos2(foot.right() - 16.0, foot.center().y), Align2::RIGHT_CENTER, "Hold Alt and click to add to favorites", reg(12.0), pal().text_muted);
        }
        ui.allocate_rect(rest, Sense::hover());
        if tones_open {
            // the six hands drop down over the list, so they come last
            let strip = Rect::from_min_size(hand.left_bottom() + vec2(0.0, 4.0), vec2(40.0, 6.0 * 40.0));
            ui.painter().rect(strip, 8, pal().bg_floating, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
            for t in 0..6u8 {
                let r = Rect::from_min_size(strip.min + vec2(0.0, t as f32 * 40.0), vec2(40.0, 40.0));
                let resp = ui.interact(r, ui.id().with(("tone", t)), Sense::click());
                if resp.hovered() || t == tone {
                    ui.painter().rect_filled(r.shrink(2.0), 6, pal().control_hover);
                }
                Image::from_uri(twemoji(tinted("👏", t))).show_loading_spinner(false).paint_at(ui, r.shrink(8.0));
                if resp.clicked() {
                    new_tone = Some(t);
                }
            }
        }
    });
    if let Some(t) = new_tone {
        app.ui.skin = t;
        tones_open = false;
    }
    if add_emoji {
        ctx.open_url(egui::OpenUrl::new_tab(format!("https://discord.com/channels/{guild}")));
    }
    ctx.data_mut(|d| {
        d.insert_temp(shut_id, shut);
        d.insert_temp(tones_id, tones_open);
    });
    let searching = !q.trim().is_empty();
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
    let menu_id = egui::Id::new("picker_menu");
    if let Some(m) = menu_at {
        ctx.data_mut(|d| d.insert_temp(menu_id, m));
    }
    let menu: Option<(ReactEmoji, Pos2)> = ctx.data(|d| d.get_temp(menu_id));
    let clicked_out = super::popups::pressed_outside(ctx, &area) && !on_picker_button(app, ctx) && menu.is_none();
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if chosen.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        chosen = sel.and_then(|i| order.get(i).cloned()).or(if searching { best } else { None });
    }
    let typed: String = ctx.data(|d| d.get_temp(query_id)).unwrap_or_default();
    ctx.data_mut(|d| d.insert_temp(sel_id, (sel, order.len(), typed)));
    // right-click on an emoji: favourite it, or copy a custom one's id
    if let Some((em, at)) = menu {
        let fav = app.settings.fav_emoji.contains(&em);
        let mut act = None;
        let open = super::float_menu(ctx, "picker_menu", at, |ui| {
            if super::menu_item(ui, Some(super::icons::Icon::Star), if fav { "Unfavorite Emoji" } else { "Favorite Emoji" }, false) {
                act = Some(1);
            }
            if em.id != 0 && super::menu_item(ui, Some(super::icons::Icon::Id), "Copy Emoji ID", false) {
                act = Some(2);
            }
            if crate::plugins::on("CopyEmojiMarkdown") && super::menu_item(ui, Some(super::icons::Icon::Copy), "Copy Emoji Markdown", false) {
                act = Some(3);
            }
            if em.id != 0 && crate::plugins::on("ExpressionCloner") && super::menu_item(ui, Some(super::icons::Icon::Copy), "Clone Emoji", false) {
                act = Some(4);
            }
        });
        match act {
            Some(1) => toggle_fav(app, ctx, em),
            Some(2) => ctx.copy_text(em.id.to_string()),
            Some(3) if em.id != 0 => ctx.copy_text(super::expr::emoji_markdown(em.id, &em.name, em.animated)),
            Some(3) if crate::plugins::flag("CopyEmojiMarkdown", "copyUnicode") => ctx.copy_text(em.name.clone()),
            Some(3) => ctx.copy_text(format!(":{}:", emojis::get(&em.name).and_then(|x| x.shortcode()).unwrap_or(&em.name))),
            Some(_) => super::expr::ask(ctx, super::expr::Cloning::Emoji(em.id, em.name.clone(), em.animated)),
            None => {}
        }
        if act.is_some() || !open {
            ctx.data_mut(|d| d.remove::<(ReactEmoji, Pos2)>(menu_id));
        }
        return;
    }
    let (alt, shift) = ctx.input(|i| (i.modifiers.alt, i.modifiers.shift));
    // alt+click stars it instead of picking it
    if let Some(c) = chosen.as_ref().filter(|_| alt) {
        toggle_fav(app, ctx, c.react());
        return;
    }
    if let Some(c) = chosen {
        match target {
            Pick::Composer => {
                let text = match &c {
                    Chosen::Custom(e) => format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id),
                    Chosen::Unicode(s) => s.to_string(),
                };
                app.used(match &c {
                    Chosen::Custom(e) => ReactEmoji { id: e.id, name: e.name.clone(), animated: e.animated },
                    Chosen::Unicode(s) => ReactEmoji { id: 0, name: s.to_string(), animated: false },
                });
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
            Pick::Quick(slot) => {
                let em = match c {
                    Chosen::Custom(e) => ReactEmoji { id: e.id, name: e.name, animated: e.animated },
                    Chosen::Unicode(s) => ReactEmoji { id: 0, name: s.to_string(), animated: false },
                };
                let slot = *slot;
                app.set_quick(slot, em, ctx);
            }
        }
        // shift+click keeps the picker open for another
        if !shift {
            app.picker = None;
            ctx.data_mut(|d| d.remove::<String>(query_id));
        }
    } else if clicked_out || esc {
        app.picker = None;
        ctx.data_mut(|d| d.remove::<String>(query_id));
    }
}

enum Lead<'a> {
    None,
    Icon(super::icons::Icon),
    Image(&'a str),
}

/// A section title in the picker. Clicking folds it; returns whether it's open.
fn section(ui: &mut Ui, title: &str, lead: Lead, shut: &mut Vec<String>) -> bool {
    let open = !shut.iter().any(|s| s == title);
    ui.add_space(12.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let col = if resp.hovered() { pal().header_primary } else { pal().header_secondary };
    let mut x = r.left() + 12.0;
    let lr = Rect::from_min_size(pos2(x, r.center().y - 8.0), vec2(16.0, 16.0));
    match lead {
        Lead::None => {}
        Lead::Icon(i) => {
            super::icons::draw(ui.painter(), i, lr, col);
            x += 22.0;
        }
        Lead::Image(url) => {
            Image::from_uri(url.to_string()).corner_radius(CornerRadius::same(4)).show_loading_spinner(false).paint_at(ui, lr);
            x += 22.0;
        }
    }
    let t = ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, title, semi(14.0), col);
    let chev = if open { super::icons::Icon::ChevronDown } else { super::icons::Icon::ChevronRight };
    super::icons::draw(ui.painter(), chev, Rect::from_center_size(pos2(t.right() + 10.0, r.center().y), vec2(12.0, 12.0)), col);
    if resp.clicked() {
        if open {
            shut.push(title.to_string());
        } else {
            shut.retain(|s| s != title);
        }
    }
    ui.add_space(4.0);
    open
}

/// The emoji in the picked skin tone, where it has one.
fn tinted(e: &'static str, tone: u8) -> &'static str {
    use emojis::SkinTone::*;
    let t = [Default, Light, MediumLight, Medium, MediumDark, Dark][tone.min(5) as usize];
    emojis::get(e).and_then(|x| x.with_skin_tone(t)).map_or(e, |x| x.as_str())
}

/// Rows of nine. `base` is where this section starts in the keyboard's count, so the cell the
/// arrows are on lights up and scrolls into view.
#[allow(clippy::too_many_arguments)]
fn grid(
    ui: &mut Ui,
    base: usize,
    n: usize,
    kbd: Kbd,
    mut paint: impl FnMut(&Ui, usize, Rect),
    mut pick: impl FnMut(usize),
    mut hover: impl FnMut(usize, bool),
    mut menu: impl FnMut(usize, Pos2),
) {
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
                let on = kbd.sel == Some(base + i);
                if base + i == 0 {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("e2e", "emoji0")), r));
                }
                if resp.hovered() || on {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().control_hover);
                }
                if on && kbd.moved {
                    ui.scroll_to_rect(r, None);
                }
                if ui.is_rect_visible(r) {
                    paint(ui, i, r);
                }
                hover(i, resp.hovered() || (on && kbd.moved));
                if resp.clicked() {
                    pick(i);
                }
                if resp.secondary_clicked() {
                    menu(i, resp.interact_pointer_pos().unwrap_or(r.center()));
                }
            }
        });
    }
}

fn toggle_fav(app: &mut App, ctx: &egui::Context, em: ReactEmoji) {
    let favs = &mut app.settings.fav_emoji;
    match favs.iter().position(|f| *f == em) {
        Some(i) => {
            favs.remove(i);
        }
        None => favs.push(em),
    }
    app.save_settings(ctx);
}

/// Discord's sticker picker: every server's stickers, the current one first; another
/// server's need nitro.
pub fn stickers(app: &mut App, ctx: &egui::Context) {
    let Some(at) = app.stickers else { return };
    let Some(ch) = app.cur() else { return };
    let here = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let nitro = app.store.nitro();
    let order = std::iter::once(here).chain(app.store.guild_order.iter().copied().filter(|g| *g != here));
    // (server, its icon, its stickers, whether we may send them here)
    let groups: Vec<(String, Option<String>, Vec<(Id, String, u8)>, bool)> = order
        .filter_map(|g| app.store.guilds.get(&g))
        .filter(|g| !g.stickers.is_empty())
        .map(|g| (g.name.clone(), g.icon_url(), g.stickers.clone(), nitro || g.id == here || super::outgoing::bypass_sticker()))
        .collect();
    let qid = egui::Id::new("sticker_q");
    let jump_id = egui::Id::new("sticker_jump");
    let tops_id = egui::Id::new("sticker_tops");
    let shut_id = egui::Id::new("sticker_shut");
    let mut q: String = ctx.data(|d| d.get_temp(qid)).unwrap_or_default();
    let jump: Option<f32> = ctx.data_mut(|d| d.remove_temp(jump_id));
    let tops: Vec<f32> = ctx.data(|d| d.get_temp(tops_id)).unwrap_or_default();
    let mut shut: Vec<String> = ctx.data(|d| d.get_temp(shut_id)).unwrap_or_default();
    let mut new_tops = Vec::new();
    let ql = q.to_lowercase();
    let find = |id: Id| groups.iter().find_map(|(name, icon, l, ok)| l.iter().find(|s| s.0 == id).map(|s| (s.clone(), name.clone(), icon.clone(), *ok)));
    let recent: Vec<_> = if ql.is_empty() { app.ui.frequent_stickers.iter().filter_map(|id| find(*id)).filter(|s| s.3).collect() } else { Vec::new() };
    let mut chosen = None;
    let mut menu_at: Option<((Id, String, u8), Pos2)> = None;
    let mut go = None;
    let mut hovered: Option<((Id, String, u8), String)> = None;
    let url = |id: &Id, fmt: u8| format!("https://media.discordapp.net/stickers/{id}.{}?size=160", if fmt == 4 { "gif" } else { "png" });
    let (area, to) = shell(ctx, at, Align2::RIGHT_BOTTOM, Some(Tab::Stickers), |ui| {
        search_box(ui, &mut q, "Find the perfect sticker", None).request_focus();
        header_rule(ui);
        let rest = ui.available_rect_before_wrap();
        let foot = Rect::from_min_max(pos2(rest.left(), rest.bottom() - 48.0), rest.max);
        let rail = Rect::from_min_max(rest.min, pos2(rest.left() + 48.0, foot.top()));
        let list = Rect::from_min_max(pos2(rail.right(), rest.top()), pos2(rest.right(), foot.top()));
        ui.painter().rect_filled(rail, CornerRadius::ZERO, pal().bg_tertiary);
        // the left column: frequently used, then each server by its icon
        let mut marks: Vec<(Option<&str>, &str)> = Vec::new();
        if !recent.is_empty() {
            marks.push((None, "Frequently Used"));
        }
        marks.extend(groups.iter().map(|(name, icon, _, _)| (icon.as_deref(), name.as_str())));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail.shrink2(vec2(8.0, 8.0))), |ui| {
            egui::ScrollArea::vertical().id_salt("sticker_rail").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, (icon, name)) in marks.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r.expand(2.0), 8, MENU_HOVER);
                    }
                    match icon {
                        Some(url) => {
                            Image::from_uri(url.to_string()).corner_radius(CornerRadius::same(8)).show_loading_spinner(false).paint_at(ui, r);
                        }
                        None if i == 0 && !recent.is_empty() => super::icons::draw(ui.painter(), super::icons::Icon::Clock, r.shrink(6.0), pal().header_secondary),
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
                if !recent.is_empty() {
                    sections.push(("Frequently Used", Lead::Icon(super::icons::Icon::Clock), recent.iter().map(|(s, g, _, ok)| (s.clone(), g.clone(), *ok)).collect()));
                }
                for (name, icon, l, ok) in &groups {
                    let l = l.iter().filter(|(_, n, _)| ql.is_empty() || n.to_lowercase().contains(&ql)).map(|s| (s.clone(), name.clone(), *ok)).collect();
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
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, CornerRadius::same(8), pal().control_hover);
                                    hovered = Some((st.clone(), format!("from {from}")));
                                }
                                // lottie stickers (format 3) have no still image to show
                                if st.2 != 3 && ui.is_rect_visible(r) {
                                    let tint = if *ok { Color32::WHITE } else { Color32::from_white_alpha(90) };
                                    Image::from_uri(crate::gif::uri(ui.ctx(), &url(&st.0, st.2), true)).tint(tint).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                                }
                                let resp = resp.on_hover_text(if *ok { st.1.clone() } else { format!("{} needs Nitro here", st.1) });
                                if resp.secondary_clicked() && super::expr::has_sticker_menu() {
                                    menu_at = Some((st.clone(), resp.interact_pointer_pos().unwrap_or(r.center())));
                                }
                                if resp.clicked() && *ok {
                                    chosen = Some(st.clone());
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
        p.rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 7, se: 7 }, pal().bg_tertiary);
        p.hline(foot.x_range(), foot.top(), Stroke::new(1.0, pal().border_normal));
        if let Some(((id, name, fmt), from)) = hovered {
            if fmt != 3 {
                Image::from_uri(crate::gif::uri(ui.ctx(), &url(&id, fmt), true)).show_loading_spinner(false).paint_at(ui, Rect::from_center_size(pos2(foot.left() + 30.0, foot.center().y), vec2(28.0, 28.0)));
            }
            super::text::paint(ui.painter(), pos2(foot.left() + 52.0, foot.center().y - 8.0), Align2::LEFT_CENTER, &name, semi(16.0), pal().header_primary, 400.0);
            ui.painter().text(pos2(foot.left() + 52.0, foot.center().y + 10.0), Align2::LEFT_CENTER, from, reg(12.0), pal().text_muted);
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
    let menu_id = egui::Id::new("sticker_menu");
    if let Some(m) = menu_at {
        ctx.data_mut(|d| d.insert_temp(menu_id, m));
    }
    if let Some(((id, name, fmt), at)) = ctx.data(|d| d.get_temp::<((Id, String, u8), Pos2)>(menu_id)) {
        let mut used = false;
        let open = super::float_menu(ctx, "sticker_menu", at, |ui| used = super::expr::sticker_menu(ui, id, &name, fmt));
        if used || !open {
            ctx.data_mut(|d| d.remove::<((Id, String, u8), Pos2)>(menu_id));
        }
        return;
    }
    if let Some((id, name, fmt)) = chosen {
        let real = nitro || app.store.guilds.get(&here).is_some_and(|g| g.stickers.iter().any(|s| s.0 == id));
        let paste = crate::plugins::on("StickerPaste");
        if !real {
            let d = app.drafts.entry(ch).or_default();
            if !d.is_empty() && !d.ends_with(' ') {
                d.push(' ');
            }
            d.push_str(&super::outgoing::sticker_link(id, &name, fmt));
            app.stickers = None;
            if paste {
                app.focus_composer = true;
                app.caret_end = true;
            } else {
                app.send_message(ch);
            }
            return;
        }
        if paste {
            app.plug.sticker = Some((id, name, fmt));
            app.stickers = None;
            app.focus_composer = true;
            return;
        }
        let nonce = crate::state::make_nonce();
        app.net.send(crate::net::Cmd::Send { channel: ch, content: String::new(), nonce, reply: app.reply.take().map(|r| (r, app.reply_ping)), files: Vec::new(), stickers: vec![id], tts: false, silent: false });
        app.jump_to_present(ch);
        app.ui.frequent_stickers.retain(|s| *s != id);
        app.ui.frequent_stickers.insert(0, id);
        app.ui.frequent_stickers.truncate(12);
        app.stickers = None;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (super::popups::pressed_outside(ctx, &area) && !on_picker_button(app, ctx)) {
        app.stickers = None;
        ctx.data_mut(|d| d.remove::<String>(qid));
    }
}

/// The composer's picker buttons toggle the picker themselves; a press there isn't a click away.
pub fn on_picker_button(app: &App, ctx: &egui::Context) -> bool {
    let at = ctx.pointer_hover_pos();
    ["gif_button", "emoji_button", "sticker_button"].iter().any(|k| app.rects.get(*k).zip(at).is_some_and(|(r, p)| r.contains(p)))
}
