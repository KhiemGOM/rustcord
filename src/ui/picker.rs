use egui::{CornerRadius, Frame, Image, Margin, Rect, RichText, Sense, Ui, vec2};

use super::{App, Pick};
use crate::markdown::twemoji;
use crate::state::{Emoji, ReactEmoji};
use crate::theme::*;


/// Every emoji that has a shortcode, default skin tone only.
pub fn common() -> impl Iterator<Item = (&'static str, &'static str)> {
    emoji_list(None)
}

fn emoji_list(group: Option<emojis::Group>) -> impl Iterator<Item = (&'static str, &'static str)> {
    emojis::iter()
        .filter(move |e| group.is_none_or(|g| e.group() == g))
        .filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default))
        .filter_map(|e| Some((e.as_str(), e.shortcode()?)))
}

const GROUPS: [(emojis::Group, &str); 8] = [
    (emojis::Group::SmileysAndEmotion, "PEOPLE"),
    (emojis::Group::AnimalsAndNature, "NATURE"),
    (emojis::Group::FoodAndDrink, "FOOD"),
    (emojis::Group::Activities, "ACTIVITIES"),
    (emojis::Group::TravelAndPlaces, "TRAVEL"),
    (emojis::Group::Objects, "OBJECTS"),
    (emojis::Group::Symbols, "SYMBOLS"),
    (emojis::Group::Flags, "FLAGS"),
];

enum Chosen {
    Custom(Emoji),
    Unicode(&'static str),
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((target, at)) = &app.picker else { return };
    let at = *at;
    let guild = app.ui.guild.unwrap_or(0);
    let customs: Vec<Emoji> = app.store.guilds.get(&guild).map(|g| g.emojis.clone()).unwrap_or_default();
    let query_id = egui::Id::new("picker_q");
    let mut q: String = ctx.data_mut(|d| d.get_temp(query_id).unwrap_or_default());
    let mut chosen = None;

    let area = egui::Area::new(egui::Id::new("picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(0).show(ui, |ui| {
                ui.set_width(418.0);
                Frame::new().inner_margin(Margin::symmetric(12, 12)).show(ui, |ui| {
                    Frame::new().fill(pal().bg_tertiary).corner_radius(4).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut q)
                                .frame(Frame::NONE)
                                .hint_text(RichText::new("Find the perfect emoji").color(pal().text_muted))
                                .font(reg(15.0))
                                .desired_width(ui.available_width()),
                        );
                        r.request_focus();
                    });
                });
                let ql = q.to_lowercase();
                egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let customs: Vec<&Emoji> = customs.iter().filter(|e| ql.is_empty() || e.name.to_lowercase().contains(&ql)).collect();
                    if !customs.is_empty() {
                        section(ui, "SERVER EMOJI");
                        grid(ui, customs.len(), |ui, i, r| {
                            Image::from_uri(crate::gif::uri(ui.ctx(), &customs[i].url(), ui.is_rect_visible(r))).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                        }, |i| {
                            chosen = Some(Chosen::Custom(customs[i].clone()));
                        }, |i| format!(":{}:", customs[i].name));
                    }
                    for (group, title) in GROUPS {
                        let uni: Vec<(&str, &str)> = emoji_list(Some(group)).filter(|(_, n)| ql.is_empty() || n.contains(&ql)).collect();
                        if uni.is_empty() {
                            continue;
                        }
                        section(ui, title);
                        grid(ui, uni.len(), |ui, i, r| {
                            Image::from_uri(twemoji(uni[i].0)).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                        }, |i| {
                            chosen = Some(Chosen::Unicode(uni[i].0));
                        }, |i| format!(":{}:", uni[i].1));
                    }
                    ui.add_space(8.0);
                });
            });
        });

    ctx.data_mut(|d| d.insert_temp(query_id, q));
    let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
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
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title).font(semi(12.0)).color(pal().header_secondary));
    });
    ui.add_space(6.0);
}

fn grid(
    ui: &mut Ui,
    n: usize,
    mut paint: impl FnMut(&Ui, usize, Rect),
    mut pick: impl FnMut(usize),
    tip: impl Fn(usize) -> String,
) {
    let cols = 9;
    let cell = 44.0;
    for row in 0..n.div_ceil(cols) {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            for c in 0..cols {
                let i = row * cols + c;
                if i >= n {
                    break;
                }
                let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                }
                if ui.is_rect_visible(r) {
                    paint(ui, i, r);
                }
                if resp.on_hover_text(tip(i)).clicked() {
                    pick(i);
                }
            }
        });
    }
}
