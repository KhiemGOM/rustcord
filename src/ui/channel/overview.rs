use egui::text::{CCursor, CCursorRange};
use egui::{Align2, Color32, CornerRadius, Frame, Image, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{App, State, faint, heading, modal};
use crate::markdown::twemoji;
use crate::state::Channel;
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::settings;
use crate::ui::{bolded, menu_item};

const SLOWMODE: [(u64, &str); 14] = [
    (0, "Off"),
    (5, "5s"),
    (10, "10s"),
    (15, "15s"),
    (30, "30s"),
    (60, "1m"),
    (120, "2m"),
    (300, "5m"),
    (600, "10m"),
    (900, "15m"),
    (1800, "30m"),
    (3600, "1h"),
    (7200, "2h"),
    (21600, "6h"),
];

const ARCHIVE: [(u64, &str); 4] = [(60, "1 Hour"), (1440, "24 Hours"), (4320, "3 Days"), (10080, "1 Week")];

const SPOILER: u64 = 1 << 21;
const REQUIRE_TAG: u64 = 1 << 4;
const HIDE_MEDIA_DOWNLOAD: u64 = 1 << 15;

#[derive(Default)]
pub struct TagEdit {
    pub index: Option<usize>,
    pub id: Option<String>,
    pub name: String,
    pub emoji: Option<String>,
    pub emoji_id: Option<String>,
    pub moderated: bool,
}

fn gap(ui: &mut Ui) {
    ui.add_space(24.0);
}

fn desc(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
    ui.add_space(8.0);
}

fn small(ui: &mut Ui, s: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(s).font(reg(12.0)).line_height(Some(16.0)).color(pal().header_secondary));
}

fn pick<T: Copy + PartialEq>(ui: &mut Ui, key: &str, cur: T, opts: &[(T, &str)], fallback: String) -> Option<T> {
    let shown = opts.iter().find(|(v, _)| *v == cur).map_or(fallback, |(_, l)| l.to_string());
    let w = ui.available_width();
    let mut out = None;
    let r = settings::dropdown(ui, w, None, &shown, |ui| {
        for (v, l) in opts {
            if menu_item(ui, None, l, false) {
                out = Some(*v);
            }
        }
    });
    settings::mark(ui, key, r.rect);
    out
}

fn edit(ctx: &egui::Context, id: egui::Id, text: &mut String, pre: &str, post: &str) {
    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let end = text.chars().count();
    let (a, b) = st.cursor.char_range().map_or((end, end), |r| {
        let (x, y): (usize, usize) = (r.primary.index.into(), r.secondary.index.into());
        (x.min(y), x.max(y))
    });
    let at = |i: usize| text.char_indices().nth(i).map_or(text.len(), |(b, _)| b);
    let (ba, bb) = (at(a), at(b));
    text.insert_str(bb, post);
    text.insert_str(ba, pre);
    let n = pre.chars().count();
    st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a + n), CCursor::new(b + n))));
    st.store(ctx, id);
}

/// The emoji button's popup: a search box over the unicode emoji, one click to pick.
pub fn emoji_popup(resp: &egui::Response) -> Option<String> {
    let mut chosen = None;
    egui::Popup::menu(resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).width(330.0).show(|ui| {
        let qid = ui.id().with("emoji_q");
        let mut q: String = ui.data(|d| d.get_temp(qid)).unwrap_or_default();
        settings::input(ui, &mut q, "Search emoji", false);
        ui.add_space(8.0);
        let ql = q.to_lowercase();
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            let list: Vec<_> = emojis::iter().filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default)).filter(|e| ql.is_empty() || e.shortcodes().any(|s| s.contains(&ql))).take(480).collect();
            for row in list.chunks(9) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    for e in row {
                        let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        Image::from_uri(twemoji(e.as_str())).show_loading_spinner(false).paint_at(ui, r.shrink(5.0));
                        if resp.clicked() {
                            chosen = Some(e.as_str().to_string());
                        }
                    }
                });
            }
        });
        ui.data_mut(|d| d.insert_temp(qid, q));
    });
    if chosen.is_some() {
        egui::Popup::close_all(&resp.ctx);
    }
    chosen
}

fn smile(ui: &mut Ui, r: Rect, id: &'static str) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(id), Sense::click());
    icons::draw(ui.painter(), Icon::Smile, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A text box with discord's emoji button inside its right edge.
pub fn emoji_field(ui: &mut Ui, text: &mut String, hint: &str, id: egui::Id, max: usize) -> (egui::Response, Option<String>) {
    let mut emoji = None;
    let out = Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(text).id(id).frame(Frame::NONE).margin(Margin::ZERO).font(reg(16.0)).text_color(pal().text_normal).hint_text(RichText::new(hint).color(pal().text_muted)).char_limit(max).desired_width(ui.available_width() - 30.0));
            let (b, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            let resp = smile(ui, b, "emoji_btn");
            emoji = emoji_popup(&resp);
            r
        })
        .inner
    });
    (out.inner, emoji)
}

fn visibility(ui: &mut Ui, value: &mut i32, options: &[(i32, &str, &str)]) -> bool {
    let mut changed = false;
    let w = ui.available_width();
    for (v, label, desc) in options {
        let g = ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, w - 38.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0 + g.size().y), Sense::click());
        let c = pos2(r.left() + 12.0, r.top() + 12.0);
        if *value == *v {
            ui.painter().circle_filled(c, 12.0, pal().blurple);
            ui.painter().circle_filled(c, 5.0, Color32::WHITE);
        } else {
            ui.painter().circle_stroke(c, 11.0, Stroke::new(1.5, pal().header_secondary));
        }
        ui.painter().text(pos2(r.left() + 38.0, c.y), Align2::LEFT_CENTER, *label, reg(16.0), pal().text_normal);
        ui.painter().galley(pos2(r.left() + 38.0, r.top() + 26.0), g, pal().header_secondary);
        settings::mark(ui, &format!("chanset:vis:{label}"), r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && *value != *v {
            *value = *v;
            changed = true;
        }
        ui.add_space(16.0);
    }
    changed
}

/// A slider with its labels over it where their values fall, and tick marks down to the rail.
fn ticked_slider(ui: &mut Ui, value: &mut f32, lo: f32, hi: f32, ticks: &[(f32, String)], key: &str) -> bool {
    let w = ui.available_width();
    let (top, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
    let x_of = |v: f32| top.left() + 8.0 + (v - lo) / (hi - lo) * (w - 16.0);
    for (v, l) in ticks {
        let g = ui.painter().layout_no_wrap(l.clone(), medium(12.0), pal().header_secondary);
        let left = (x_of(*v) - g.size().x / 2.0).clamp(top.left(), top.right() - g.size().x);
        ui.painter().galley(pos2(left, top.top()), g, pal().header_secondary);
    }
    ui.add_space(6.0);
    let r = ui.cursor().min;
    for (v, _) in ticks {
        ui.painter().vline(x_of(*v), r.y - 4.0..=r.y + 20.0, Stroke::new(2.0, settings::TRACK));
    }
    let moved = settings::slider(ui, w, value, lo, hi);
    settings::mark(ui, key, Rect::from_min_size(r, vec2(w, 16.0)));
    if moved {
        *value = value.round();
    }
    moved
}

fn notice(ui: &mut Ui, text: &str) {
    Frame::new().fill(Color32::from_rgba_unmultiplied(0x3b, 0x82, 0xf6, 24)).stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x3b, 0x82, 0xf6, 90))).corner_radius(8).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(text).font(reg(14.0)).color(pal().text_normal));
    });
}

fn topic(ui: &mut Ui, d: &mut Value, forum: bool) {
    let max = if forum { 4096 } else { 1024 };
    settings::h5(ui, if forum { "Post Guidelines" } else { "Channel Topic" });
    let mut text = d["topic"].as_str().unwrap_or_default().to_string();
    let id = egui::Id::new("chan_topic");
    let ctx = ui.ctx().clone();
    let t = Frame::new().fill(Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
        ui.painter().rect_filled(bar, CornerRadius { nw: 8, ne: 8, ..Default::default() }, Color32::from_black_alpha(40));
        let mut x = bar.left() + 6.0;
        let mut act = None;
        for (i, (icon, pre)) in [(Icon::Bold, "**"), (Icon::Italic, "*"), (Icon::Underline, "__"), (Icon::Strike, "~~"), (Icon::Eye, "||")].into_iter().enumerate() {
            if i == 4 {
                ui.painter().vline(x + 2.0, bar.center().y - 10.0..=bar.center().y + 10.0, Stroke::new(1.0, faint(61)));
                x += 8.0;
            }
            let b = Rect::from_min_size(pos2(x, bar.top() + 4.0), vec2(30.0, 30.0));
            let resp = ui.interact(b, id.with(("fmt", i)), Sense::click());
            icons::draw(ui.painter(), icon, Rect::from_center_size(b.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                act = Some(pre);
            }
            x += 30.0;
        }
        let eb = smile(ui, Rect::from_center_size(pos2(bar.right() - 20.0, bar.center().y), vec2(24.0, 24.0)), "topic_emoji");
        if let Some(e) = emoji_popup(&eb) {
            edit(&ctx, id, &mut text, &e, "");
        }
        if let Some(p) = act {
            edit(&ctx, id, &mut text, p, p);
        }
        Frame::new().inner_margin(Margin::symmetric(16, 10)).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .hint_text(RichText::new("Let everyone know how to use this channel!").color(pal().text_muted))
                    .desired_rows(8)
                    .desired_width(ui.available_width()),
            )
        })
        .inner
    });
    if text.chars().count() > max {
        text = text.chars().take(max).collect();
    }
    let left = max as i64 - text.chars().count() as i64;
    ui.painter().text(t.response.rect.right_bottom() - vec2(16.0, 10.0), Align2::RIGHT_BOTTOM, left.to_string(), bold(14.0), pal().text_normal);
    if text != d["topic"].as_str().unwrap_or_default() {
        d["topic"] = json!(text);
    }
}

fn tags(ui: &mut Ui, d: &mut Value, tag: &mut Option<TagEdit>) {
    let mut list = d["available_tags"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        if settings::primary(ui, "Create Tag").clicked() {
            *tag = Some(TagEdit::default());
        }
        return;
    }
    let mut rects = Vec::new();
    let mut dropped = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (i, t) in list.iter().enumerate() {
            let label = format!("{}{}", t["emoji_name"].as_str().map_or(String::new(), |e| format!("{e} ")), t["name"].as_str().unwrap_or_default());
            let g = ui.painter().layout_no_wrap(label, medium(14.0), pal().text_normal);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::click_and_drag());
            ui.painter().rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(faint(31), pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
            ui.painter().galley(pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, pal().text_normal);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            settings::mark(ui, &format!("chanset:tag:{i}"), r);
            if resp.clicked() {
                *tag = Some(TagEdit {
                    index: Some(i),
                    id: t["id"].as_str().map(str::to_string),
                    name: t["name"].as_str().unwrap_or_default().to_string(),
                    emoji: t["emoji_name"].as_str().map(str::to_string),
                    emoji_id: t["emoji_id"].as_str().map(str::to_string),
                    moderated: t["moderated"].as_bool().unwrap_or(false),
                });
            }
            if resp.drag_stopped() {
                dropped = Some(i);
            }
            rects.push(r);
        }
        if list.len() < 20 {
            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::same(8), crate::ui::motion::mix(pal().blurple, pal().blurple.gamma_multiply(0.85), crate::ui::motion::hot(ui.ctx(), &resp)));
            icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), Color32::WHITE);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            settings::mark(ui, "chanset:tag:new", r);
            if resp.clicked() {
                *tag = Some(TagEdit::default());
            }
        }
    });
    if let Some(from) = dropped
        && let Some(p) = ui.ctx().pointer_latest_pos()
        && let Some(to) = rects.iter().position(|r| r.contains(p))
        && to != from
    {
        let t = list.remove(from);
        list.insert(to, t);
        d["available_tags"] = Value::Array(list);
    }
}

fn check(ui: &mut Ui, on: &mut bool, label: &str, key: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let b = Rect::from_min_size(pos2(r.left(), r.center().y - 10.0), vec2(20.0, 20.0));
    if *on {
        ui.painter().rect_filled(b, CornerRadius::same(4), pal().blurple);
        icons::draw(ui.painter(), Icon::Check, b.shrink(3.0), Color32::WHITE);
    } else {
        ui.painter().rect_stroke(b, CornerRadius::same(4), Stroke::new(2.0, pal().header_secondary), egui::StrokeKind::Inside);
    }
    ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, reg(14.0), pal().text_normal);
    settings::mark(ui, key, b);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
    }
    resp.clicked()
}

pub fn tag_modal(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.chan_settings.as_mut() else { return };
    let State { tag, draft, .. } = s;
    let Some(t) = tag.as_mut() else { return };
    let editing = t.index.is_some();
    let (mut save, mut remove, mut cancel) = (false, false, false);
    let id = egui::Id::new("chan_tag_name");
    let close = modal(ctx, "chan_tag", if editing { "Edit Tag" } else { "Create Tag" }, |ui| {
        ui.label(RichText::new("Create a tag to help people organize their posts into subcategories. Maximum 20 characters.").font(reg(14.0)).color(pal().header_secondary));
        ui.add_space(16.0);
        let (r, e) = emoji_field(ui, &mut t.name, "Relationships, Advice, Art, Gear, Health, Recipes", id, 20);
        settings::mark(ui, "chanset:tag_name", r.rect);
        if let Some(e) = e {
            t.emoji = Some(e);
            t.emoji_id = None;
        }
        ui.add_space(16.0);
        check(ui, &mut t.moderated, "Only allow moderators to apply tag", "chanset:tag_mod");
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            if editing && settings::danger(ui, "Remove Tag", false).clicked() {
                remove = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui.add_enabled(!t.name.is_empty(), |ui: &mut Ui| settings::primary(ui, "Save")).clicked();
                ui.add_space(8.0);
                cancel = settings::secondary(ui, "Cancel").clicked();
            });
        });
    });
    if save || remove {
        let mut list = draft["available_tags"].as_array().cloned().unwrap_or_default();
        match (t.index, remove) {
            (Some(i), true) if i < list.len() => {
                list.remove(i);
            }
            (_, false) => {
                let mut v = json!({ "name": t.name, "emoji_name": t.emoji, "emoji_id": t.emoji_id, "moderated": t.moderated });
                if let Some(id) = &t.id {
                    v["id"] = json!(id);
                }
                match t.index {
                    Some(i) if i < list.len() => list[i] = v,
                    _ => list.push(v),
                }
            }
            _ => {}
        }
        draft["available_tags"] = Value::Array(list);
    }
    if close || save || remove || cancel {
        *tag = None;
    }
}

fn reaction(ui: &mut Ui, d: &mut Value) {
    let cur = d["default_reaction_emoji"]["emoji_name"].as_str().map(str::to_string);
    ui.horizontal(|ui| {
        let resp = settings::primary(ui, "Select emoji");
        settings::mark(ui, "chanset:reaction", resp.rect);
        if let Some(e) = emoji_popup(&resp) {
            d["default_reaction_emoji"] = json!({ "emoji_id": null, "emoji_name": e });
        }
        if cur.is_some() && settings::danger(ui, "Remove", false).clicked() {
            d["default_reaction_emoji"] = Value::Null;
        }
        if let Some(e) = cur {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(8), faint(31));
            Image::from_uri(twemoji(&e)).show_loading_spinner(false).paint_at(ui, r.shrink(8.0));
        }
    });
}

pub fn overview(app: &mut App, ui: &mut Ui, c: &Channel) {
    heading(ui, "Overview", "");
    let guild = app.store.guilds.get(&c.guild).cloned();
    let tier = guild.as_ref().map_or(0, |g| g.tier);
    let news = guild.as_ref().is_some_and(|g| g.features.iter().any(|f| f == "NEWS") && g.rules != c.id);
    let regions = if c.kind == 2 { app.chan_data("regions", format!("/guilds/{}/regions", c.guild)) } else { None };
    let Some(s) = app.chan_settings.as_mut() else { return };
    if s.draft.is_null() {
        ui.add_space(40.0);
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    }
    let State { draft: d, tag, .. } = s;
    let voice = c.is_voice();
    let forum = c.is_forum();
    let text = !voice && c.kind != 4;
    settings::h5(ui, if c.kind == 4 { "Category Name" } else { "Channel Name" });
    let mut name = d["name"].as_str().unwrap_or_default().to_string();
    let before = name.clone();
    let id = egui::Id::new("chan_name");
    let (r, e) = emoji_field(ui, &mut name, "", id, 100);
    settings::mark(ui, "chanset:name", r.rect);
    if let Some(e) = e {
        edit(&ui.ctx().clone(), id, &mut name, &e, "");
    }
    if name != before {
        // discord's text channel names are lowercase with dashes for spaces
        if text {
            name = name.to_lowercase().replace(' ', "-");
        }
        d["name"] = json!(name);
    }
    if c.kind == 4 {
        return;
    }
    if text {
        gap(ui);
        topic(ui, d, forum);
    }
    let flags = d["flags"].as_u64().unwrap_or(0);
    if forum {
        ui.add_space(8.0);
        settings::divider(ui);
        settings::h5(ui, "Tags");
        desc(ui, "Help people organize their posts into subcategories by creating a tag.");
        tags(ui, d, tag);
        ui.add_space(16.0);
        let mut req = flags & REQUIRE_TAG != 0;
        if check(ui, &mut req, "Require people to select tags when posting", "chanset:require_tag") {
            d["flags"] = json!(if req { flags | REQUIRE_TAG } else { flags & !REQUIRE_TAG });
        }
        settings::divider(ui);
        settings::h5(ui, "Default Reaction");
        desc(ui, "Pick a default emoji that your members will use to react to a post from this channel.");
        reaction(ui, d);
        settings::divider(ui);
    }
    let flags = d["flags"].as_u64().unwrap_or(0);
    gap(ui);
    settings::h5(ui, "Slowmode");
    if forum {
        let posts = d["rate_limit_per_user"].as_u64().unwrap_or(0);
        desc(ui, "Posts");
        if let Some(v) = pick(ui, "chanset:slowmode", posts, &SLOWMODE, format!("{posts}s")) {
            d["rate_limit_per_user"] = json!(v);
        }
        small(ui, "Members will be restricted to creating one post per this interval, unless they have the Bypass Slowmode permission.");
        ui.add_space(16.0);
        let msgs = d["default_thread_rate_limit_per_user"].as_u64().unwrap_or(0);
        desc(ui, "Messages");
        if let Some(v) = pick(ui, "chanset:thread_slowmode", msgs, &SLOWMODE, format!("{msgs}s")) {
            d["default_thread_rate_limit_per_user"] = json!(v);
        }
        small(ui, "Members will be limited to one message per this interval for any new posts, unless they have the Bypass Slowmode permission.");
    } else {
        let slow = d["rate_limit_per_user"].as_u64().unwrap_or(0);
        if let Some(v) = pick(ui, "chanset:slowmode", slow, &SLOWMODE, format!("{slow}s")) {
            d["rate_limit_per_user"] = json!(v);
        }
        small(ui, "Members will be restricted to sending one message and creating one thread per this interval, unless they have the Bypass Slowmode permission.");
    }
    if c.kind == 15 {
        gap(ui);
        settings::h5(ui, "Default Layout");
        desc(ui, "Set the default layout view to a media-focused gallery or a text-focused list. Members will still be able to toggle between these options.");
        let cur = if d["default_forum_layout"].as_u64() == Some(2) { 2u64 } else { 1 };
        if let Some(v) = pick(ui, "chanset:layout", cur, &[(1, "List View"), (2, "Gallery View")], String::new()) {
            d["default_forum_layout"] = json!(v);
        }
    }
    if forum {
        gap(ui);
        settings::h5(ui, "Sort Order");
        desc(ui, "Set the default sort order for new posts. Members will still be able to toggle between these options.");
        let cur = d["default_sort_order"].as_u64().unwrap_or(0);
        if let Some(v) = pick(ui, "chanset:sort", cur, &[(0, "Recent Activity"), (1, "Creation Time")], String::new()) {
            d["default_sort_order"] = json!(v);
        }
        gap(ui);
        settings::h5(ui, "Tag Matching");
        desc(ui, "Set the default tag matching behaviour. Members will still be able to toggle between these options.");
        let cur = if d["default_tag_setting"].as_str() == Some("match_all") { "match_all" } else { "match_some" };
        if let Some(v) = pick(ui, "chanset:tag_matching", cur, &[("match_some", "Match Some"), ("match_all", "Match All")], String::new()) {
            d["default_tag_setting"] = json!(v);
        }
    }
    ui.add_space(40.0);
    settings::h5(ui, "Content Visibility");
    let mut vis = if d["nsfw"].as_bool() == Some(true) { 2 } else if flags & SPOILER != 0 { 1 } else { 0 };
    let options = [
        (0, "Default", "Channel content is always visible."),
        (1, "Spoiler Channel", "Mark this channel as containing spoilers, so that plot twists, heavy topics, or other sensitive discussions are hidden until users choose to view."),
        (2, "Age-Restricted Channel", "Users will need to confirm they are of over the legal age to view in the content in this channel. Age-restricted channels are exempt from the explicit content filter."),
    ];
    if visibility(ui, &mut vis, &options) {
        d["nsfw"] = json!(vis == 2);
        d["flags"] = json!(if vis == 1 { flags | SPOILER } else { flags & !SPOILER });
    }
    if matches!(c.kind, 0 | 5) && news {
        let mut on = d["type"].as_u64() == Some(5);
        ui.add_space(8.0);
        if settings::toggle(ui, &mut on, "Announcement Channel", "Users can Follow this channel in their own servers. Posts you ‘Publish’ from this channel will appear in those servers.") {
            d["type"] = json!(if on { 5 } else { 0 });
        }
        if settings::link(ui, "Learn more.").clicked() {
            let _ = open::that_detached("https://support.discord.com/hc/en-us/articles/360032008192");
        }
        ui.add_space(12.0);
        notice(ui, "By creating an announcement channel, your server profile will be visible to others outside your server members. You can control Profile privacy in Server Settings > Profile.");
    }
    if text {
        gap(ui);
        settings::h5(ui, "Hide After Inactivity");
        let arch = d["default_auto_archive_duration"].as_u64().unwrap_or(4320);
        if let Some(v) = pick(ui, "chanset:archive", arch, &ARCHIVE, "3 Days".into()) {
            d["default_auto_archive_duration"] = json!(v);
        }
        small(ui, if forum { "New posts will not show in the channel list after being inactive for the specified duration." } else { "New threads will not show in the channel list after being inactive for the specified duration." });
    }
    if c.kind == 16 {
        ui.add_space(24.0);
        let mut on = flags & HIDE_MEDIA_DOWNLOAD == 0;
        if settings::toggle(ui, &mut on, "Show Option to Save and Copy Embedded Media", "Allows members to save, copy, and share media attachments uploaded to this channel. However, files that cannot be embedded can still be downloaded.") {
            d["flags"] = json!(if on { flags & !HIDE_MEDIA_DOWNLOAD } else { flags | HIDE_MEDIA_DOWNLOAD });
        }
    }
    if !voice {
        return;
    }
    settings::divider(ui);
    // the boost level raises the bitrate's ceiling
    let max = [96, 128, 256, 384][tier.min(3) as usize] as f32;
    settings::h5(ui, "Bitrate");
    let mut kbps = (d["bitrate"].as_u64().unwrap_or(64000) / 1000) as f32;
    if ticked_slider(ui, &mut kbps, 8.0, max, &[(8.0, "8kbps".into()), (64.0, "64kbps".into()), (max, format!("{max}kbps"))], "chanset:bitrate") {
        d["bitrate"] = json!(kbps.round() as u64 * 1000);
    }
    if max > 64.0 {
        small(ui, "ALL THE BITS! Going above 64 kbps may adversely affect people on poor connections.");
    }
    ui.add_space(24.0);
    settings::h5(ui, "Video Quality");
    let mut q = d["video_quality_mode"].as_u64().unwrap_or(1);
    if settings::radio(ui, &mut q, &[(1, "Auto", ""), (2, "720p", "")]) {
        d["video_quality_mode"] = json!(q);
    }
    ui.label(bolded("Sets camera video quality for all channel participants. Choose **Auto** for optimal performance.", 12.0, pal().header_secondary, ui.available_width()));
    ui.add_space(24.0);
    settings::h5(ui, "User Limit");
    let stage = c.kind == 13;
    let top = if stage { 10000.0 } else { 99.0 };
    let mut limit = d["user_limit"].as_u64().unwrap_or(0) as f32;
    if ticked_slider(ui, &mut limit, 0.0, top, &[(0.0, "\u{221e}".into()), (top, format!("{top}"))], "chanset:limit") {
        d["user_limit"] = json!(limit.round() as u64);
    }
    ui.add_space(8.0);
    let what = if stage { "Stage" } else { "voice" };
    ui.label(bolded(&format!("Limits the number of users that can connect to this {what} channel. Users with the **Move Members** permission ignore this limit and can move other users into the channel."), 12.0, pal().header_secondary, ui.available_width()));
    if stage {
        return;
    }
    ui.add_space(24.0);
    settings::h5(ui, "Region Override");
    desc(ui, "Anyone in this channel will attempt to connect to the region you set, regardless of where they live. Regions affect voice and video quality. Leave it on Automatic and Discord will figure out what works best.");
    let region = d["rtc_region"].as_str().map(str::to_string);
    let list: Vec<(String, String)> = regions.as_ref().and_then(|v| v.as_array()).into_iter().flatten().filter(|r| r["deprecated"].as_bool() != Some(true)).map(|r| (r["id"].as_str().unwrap_or_default().to_string(), r["name"].as_str().unwrap_or_default().to_string())).collect();
    let shown = region.as_ref().map_or("Automatic".to_string(), |id| list.iter().find(|(i, _)| i == id).map_or(id.clone(), |(_, n)| n.clone()));
    let mut chosen: Option<Option<String>> = None;
    let w = ui.available_width();
    settings::dropdown(ui, w, None, &shown, |ui| {
        if menu_item(ui, None, "Automatic", false) {
            chosen = Some(None);
        }
        for (id, n) in &list {
            if menu_item(ui, None, n, false) {
                chosen = Some(Some(id.clone()));
            }
        }
    });
    if let Some(p) = chosen {
        d["rtc_region"] = p.map_or(Value::Null, |s| json!(s));
    }
}
