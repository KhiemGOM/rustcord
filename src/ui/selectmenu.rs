use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{avatar, text};
use crate::state::{Id, Store};
use crate::theme::*;

pub const STRING: u8 = 3;
pub const USER: u8 = 5;
pub const ROLE: u8 = 6;
pub const MENTIONABLE: u8 = 7;
pub const CHANNEL: u8 = 8;

#[derive(Clone)]
pub struct Opt {
    pub label: String,
    pub value: String,
    pub desc: Option<String>,
    pub lead: Lead,
}

#[derive(Clone)]
pub enum Lead {
    None,
    Avatar(String),
    Dot(Color32),
    Channel(Id),
}

pub struct Spec<'a> {
    pub kind: u8,
    pub placeholder: &'a str,
    pub min: usize,
    pub max: usize,
    pub disabled: bool,
    pub loading: bool,
    pub options: &'a [Opt],
    pub channel_types: &'a [u8],
    pub guild: Id,
    pub channel: Id,
    pub width: f32,
    pub mark: &'a str,
}

pub struct Out {
    pub changed: bool,
    pub error: Option<String>,
}

pub fn options_for(store: &Store, guild: Id, channel: Id, kind: u8, types: &[u8]) -> Vec<Opt> {
    let users = |out: &mut Vec<Opt>| {
        let mut ids: Vec<Id> = if guild == 0 {
            store.channels.get(&channel).map(|c| c.recipients.iter().copied().chain([store.me]).collect()).unwrap_or_default()
        } else {
            store.members.get(&guild).map(|m| m.keys().copied().collect()).unwrap_or_default()
        };
        ids.sort();
        ids.dedup();
        for u in ids {
            if let Some(user) = store.user(u) {
                let lead = Lead::Avatar(user.avatar_url());
                out.push(Opt { label: store.name_in(guild, u), value: u.to_string(), desc: Some(user.username.clone()), lead });
            }
        }
    };
    let roles = |out: &mut Vec<Opt>| {
        if let Some(g) = store.guilds.get(&guild) {
            let mut rs: Vec<_> = g.roles.values().filter(|r| r.id != g.id).collect();
            rs.sort_by_key(|r| std::cmp::Reverse(r.position));
            for r in rs {
                let col = if r.color == 0 { pal().header_secondary } else { role_color(r.color) };
                out.push(Opt { label: r.name.clone(), value: r.id.to_string(), desc: None, lead: Lead::Dot(col) });
            }
        }
    };
    let mut out = Vec::new();
    match kind {
        USER => users(&mut out),
        ROLE => roles(&mut out),
        MENTIONABLE => {
            users(&mut out);
            roles(&mut out);
        }
        CHANNEL => {
            if let Some(g) = store.guilds.get(&guild) {
                for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
                    if (!types.is_empty() && !types.contains(&c.kind)) || !store.can_view(c) {
                        continue;
                    }
                    out.push(Opt { label: c.name.clone(), value: c.id.to_string(), desc: None, lead: Lead::Channel(c.id) });
                }
            }
        }
        _ => {}
    }
    out
}

fn lead(ui: &Ui, store: &Store, o: &Opt, at: Rect) {
    match &o.lead {
        Lead::None => {}
        Lead::Avatar(u) => avatar(ui, at, u),
        Lead::Dot(c) => {
            ui.painter().circle_filled(at.center(), 5.0, *c);
        }
        Lead::Channel(id) => match store.channels.get(id) {
            Some(c) => super::side::chan_icon(store, ui.painter(), c, at, pal().channel_default),
            None => icons::draw(ui.painter(), Icon::Hash, at, pal().channel_default),
        },
    }
}

pub fn validate(count: usize, min: usize, max: usize) -> Option<String> {
    if count == 0 {
        return (min > 0).then(|| "This field is required.".to_string());
    }
    if count < min {
        return Some(format!("Select at least {min}"));
    }
    (count > max).then(|| format!("Select at most {max}"))
}

fn summary(spec: &Spec, picked: &[String], all: &[Opt]) -> Option<String> {
    let names: Vec<&str> = picked.iter().filter_map(|v| all.iter().find(|o| &o.value == v).map(|o| o.label.as_str())).collect();
    (!names.is_empty()).then(|| if spec.kind == CHANNEL { names.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(", ") } else { names.join(", ") })
}

/// A select menu, closed or dropped down over the page. `picked` is what's chosen so far.
pub fn show(ui: &mut Ui, store: &Store, id: egui::Id, spec: &Spec, picked: &mut Vec<String>) -> Out {
    let all_owned;
    let all: &[Opt] = if spec.kind == STRING {
        spec.options
    } else {
        all_owned = options_for(store, spec.guild, spec.channel, spec.kind, spec.channel_types);
        &all_owned
    };
    let open_id = id.with("open");
    let q_id = id.with("q");
    let mut open: bool = ui.data(|d| d.get_temp(open_id).unwrap_or(false));
    let (r, resp) = ui.allocate_exact_size(vec2(spec.width, 40.0), Sense::click());
    let fade = if spec.disabled || spec.loading { 0.5 } else { 1.0 };
    ui.painter().rect(r, CornerRadius::same(4), pal().input_bg.gamma_multiply(fade), Stroke::new(1.0, if open { pal().blurple } else { pal().border_normal }), egui::StrokeKind::Inside);
    let shown = summary(spec, picked, all);
    let (label, col) = match &shown {
        Some(s) => (s.as_str(), pal().text_normal),
        None => (spec.placeholder, pal().text_muted),
    };
    text::paint(ui.painter(), pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, label, reg(15.0), col.gamma_multiply(fade), r.width() - 56.0);
    if spec.loading {
        ui.add(egui::Spinner::new().size(16.0));
    }
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(18.0, 18.0)), pal().header_secondary.gamma_multiply(fade));
    if resp.hovered() && !spec.disabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !spec.mark.is_empty() {
        super::settings::mark(ui, spec.mark, r);
    }
    if resp.clicked() && !spec.disabled && !spec.loading {
        open = !open;
        if open {
            ui.data_mut(|d| d.insert_temp(q_id, String::new()));
        }
    }
    let mut changed = false;
    if open {
        let mut q: String = ui.data(|d| d.get_temp(q_id).unwrap_or_default());
        let multi = spec.max > 1;
        let search = spec.kind != STRING || all.len() > 10;
        let area = egui::Area::new(id.with("list")).order(egui::Order::Foreground).fixed_pos(r.left_bottom() + vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
            Frame::new().fill(pal().bg_floating).corner_radius(8).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.set_width(spec.width - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                if search {
                    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
                    ui.painter().rect_filled(sr, 4, pal().input_bg);
                    let e = ui.put(sr.shrink2(vec2(8.0, 6.0)), egui::TextEdit::singleline(&mut q).hint_text("Search Options").font(reg(14.0)).frame(Frame::NONE).margin(Margin::ZERO));
                    ui.advance_cursor_after_rect(sr);
                    if !ui.data(|d| d.get_temp::<bool>(id.with("focused")).unwrap_or(false)) {
                        e.request_focus();
                        ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
                    }
                    ui.add_space(8.0);
                }
                let low = q.to_lowercase();
                let hits: Vec<&Opt> = all.iter().filter(|o| low.is_empty() || o.label.to_lowercase().contains(&low)).collect();
                if hits.is_empty() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No options match your search").font(reg(14.0)).color(pal().text_muted));
                    ui.add_space(8.0);
                }
                egui::ScrollArea::vertical().max_height(260.0).id_salt(id.with("scroll")).show(ui, |ui| {
                    for o in hits {
                        let h = if o.desc.is_some() && spec.kind == STRING { 48.0 } else { 36.0 };
                        let (or, oresp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
                        super::settings::mark(ui, &format!("select_opt:{}", o.value), or);
                        let on = picked.contains(&o.value);
                        if on || oresp.hovered() {
                            ui.painter().rect_filled(or, CornerRadius::same(4), if on { pal().bg_mod_selected } else { pal().bg_mod_hover });
                        }
                        let mut x = or.left() + 8.0;
                        if multi {
                            let cb = Rect::from_center_size(pos2(x + 9.0, or.center().y), vec2(18.0, 18.0));
                            ui.painter().rect(cb, CornerRadius::same(3), if on { pal().blurple } else { Color32::TRANSPARENT }, Stroke::new(1.5, if on { pal().blurple } else { pal().header_secondary }), egui::StrokeKind::Inside);
                            if on {
                                icons::draw(ui.painter(), Icon::Check, cb.shrink(2.0), Color32::WHITE);
                            }
                            x += 28.0;
                        }
                        if !matches!(o.lead, Lead::None) {
                            lead(ui, store, o, Rect::from_center_size(pos2(x + 10.0, or.center().y), vec2(20.0, 20.0)));
                            x += 28.0;
                        }
                        let (ty, dy) = if o.desc.is_some() && spec.kind == STRING { (or.top() + 18.0, 36.0) } else { (or.center().y, 0.0) };
                        let nr = text::paint(ui.painter(), pos2(x, ty), Align2::LEFT_CENTER, &o.label, semi(15.0), pal().header_primary, or.right() - x - 8.0);
                        if let Some(d) = &o.desc {
                            if spec.kind == STRING {
                                text::paint(ui.painter(), pos2(x, or.top() + dy), Align2::LEFT_CENTER, d, reg(13.0), pal().header_secondary, or.right() - x - 8.0);
                            } else {
                                text::paint(ui.painter(), pos2(nr.right() + 8.0, ty), Align2::LEFT_CENTER, d, reg(13.0), pal().header_secondary, or.right() - nr.right() - 16.0);
                            }
                        }
                        if oresp.clicked() {
                            if multi {
                                match picked.iter().position(|p| *p == o.value) {
                                    Some(i) => {
                                        picked.remove(i);
                                    }
                                    None => picked.push(o.value.clone()),
                                }
                            } else {
                                *picked = vec![o.value.clone()];
                            }
                            changed = true;
                        }
                    }
                });
            });
        });
        ui.data_mut(|d| d.insert_temp(q_id, q));
        let outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !r.contains(p)));
        let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if outside || esc || (changed && !multi) {
            open = false;
            ui.data_mut(|d| d.remove::<bool>(id.with("focused")));
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    Out { changed, error: validate(picked.len(), spec.min, spec.max) }
}
