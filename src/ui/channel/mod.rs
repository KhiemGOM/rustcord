//! Channel Settings, from Edit Channel: discord's overview, permissions, invites and integrations,
//! with its unsaved-changes bar, and Delete Channel at the bottom of the sidebar. Categories get
//! the overview and permissions pages.

use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::state::{Id, Overwrite, sid};
use crate::theme::*;

mod extras;
mod overview;
mod perms;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Overview,
    Permissions,
    Invites,
    Integrations,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sub {
    Cards,
    Webhooks,
    Followed,
}

pub struct Hook {
    pub id: Id,
    pub name: String,
    pub chan: Id,
    pub avatar: Option<String>,
}

pub struct State {
    pub chan: Id,
    pub page: Page,
    // the channel as discord last sent it, and as edited
    pub base: Value,
    pub draft: Value,
    pub perms: Vec<Overwrite>,
    pub saving: bool,
    // whose overwrite the advanced permissions show
    pub who: Id,
    pub advanced: bool,
    pub find: String,
    pub data: HashMap<&'static str, Value>,
    pub tag: Option<overview::TagEdit>,
    pub sub: Sub,
    pub hook: Option<Hook>,
    // the pause invites dialog: invites, dms, hours
    pub pause: Option<(bool, bool, u64)>,
}

// the fields the overview edits, and so the ones a save sends if they changed
const FIELDS: [&str; 17] = [
    "name",
    "type",
    "topic",
    "rate_limit_per_user",
    "default_thread_rate_limit_per_user",
    "nsfw",
    "flags",
    "default_auto_archive_duration",
    "bitrate",
    "user_limit",
    "video_quality_mode",
    "rtc_region",
    "default_reaction_emoji",
    "default_forum_layout",
    "default_sort_order",
    "default_tag_setting",
    "available_tags",
];

impl App {
    pub fn open_chan_settings(&mut self, ch: Id) {
        let Some(c) = self.store.channels.get(&ch) else { return };
        self.chan_settings = Some(State {
            chan: ch,
            page: Page::Overview,
            base: Value::Null,
            draft: Value::Null,
            perms: c.overwrites.clone(),
            saving: false,
            who: c.guild,
            advanced: false,
            find: String::new(),
            data: HashMap::new(),
            tag: None,
            sub: Sub::Cards,
            hook: None,
            pause: None,
        });
        self.rest("GET", format!("/channels/{ch}"), None, Tag::Chan("channel"));
    }

    fn chan_data(&mut self, key: &'static str, path: String) -> Option<Value> {
        let s = self.chan_settings.as_mut()?;
        if !s.data.contains_key(key) {
            s.data.insert(key, Value::Null);
            self.rest("GET", path, None, Tag::Chan(key));
        }
        self.chan_settings.as_ref()?.data.get(key).filter(|v| !v.is_null()).cloned()
    }
}

pub fn loaded(app: &mut App, key: &'static str, v: Value) {
    let Some(s) = &mut app.chan_settings else { return };
    match key {
        "channel" | "saved" => {
            s.perms = parse_overwrites(&v);
            s.base = v.clone();
            s.draft = v;
            s.saving = false;
            if key == "saved" {
                app.store.apply("CHANNEL_UPDATE", &s.base);
            }
        }
        "hook_made" | "hook_gone" | "hook_saved" => {
            s.data.remove("webhooks");
            s.hook = None;
            s.saving = false;
        }
        _ => {
            s.data.insert(key, v);
        }
    }
}

fn parse_overwrites(v: &Value) -> Vec<Overwrite> {
    let num = |v: &Value| v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64()).unwrap_or(0);
    v["permission_overwrites"].as_array().into_iter().flatten().map(|o| Overwrite { id: sid(&o["id"]), kind: o["type"].as_u64().unwrap_or(0) as u8, allow: num(&o["allow"]), deny: num(&o["deny"]) }).collect()
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

fn rule(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
}

fn same_perms(a: &[Overwrite], b: &[Overwrite]) -> bool {
    let key = |v: &[Overwrite]| {
        let mut k: Vec<_> = v.iter().map(|o| (o.id, o.allow, o.deny)).collect();
        k.sort();
        k
    };
    key(a) == key(b)
}

/// Discord's modal: 480 wide, 24 in, a title, an optional blurb, and a close cross on top.
fn modal(ctx: &egui::Context, id: &'static str, title: &str, add: impl FnOnce(&mut Ui)) -> bool {
    let mut close = false;
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51))).inner_margin(Margin::same(24));
    let r = crate::ui::motion::Modal::new(id.into()).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        ui.set_width(432.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (r, _) = ui.allocate_exact_size(vec2(432.0, 24.0), Sense::hover());
        ui.painter().text(pos2(r.left(), r.top() + 2.0), Align2::LEFT_TOP, title, semi(20.0), pal().header_primary);
        let x = Rect::from_center_size(pos2(r.right() + 6.0, r.center().y - 4.0), vec2(40.0, 40.0));
        let resp = ui.interact(x, ui.id().with("close"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(20.0, 20.0)), crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
        close = resp.clicked();
        ui.add_space(16.0);
        add(ui);
    });
    close || r.should_close()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(st) = app.chan_settings.as_ref() else { return };
    let ch = st.chan;
    let page = st.page;
    let Some(c) = app.store.channels.get(&ch).cloned() else {
        app.chan_settings = None;
        return;
    };
    let modals = st.tag.is_some() || st.pause.is_some();
    // like discord, escape leaves even with changes unsaved, and drops them
    if app.confirm.is_none() && !modals && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.chan_settings = None;
        return;
    }
    let screen = ctx.content_rect();
    let area = Rect::from_min_max(pos2(screen.left(), screen.top() + 32.0), screen.max);
    let (mut goto, mut close, mut delete) = (None, false, false);
    crate::ui::motion::layer(ctx, egui::Area::new("chan_settings".into()).order(egui::Order::Foreground).fixed_pos(area.min), area).show(ctx, |ui| {
        ui.set_min_size(area.size());
        ui.painter().rect_filled(area, 0, pal().bg_primary);
        let side_right = area.center().x - 280.0;
        let content_left = area.center().x - 228.0;
        ui.painter().rect_filled(Rect::from_min_max(area.min, pos2(side_right + 6.0, area.bottom())), 0, pal().bg_secondary_alt);
        let mut sc = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(side_right - 239.0, area.top()), pos2(side_right, area.bottom()))));
        sc.spacing_mut().item_spacing.y = 2.0;
        sc.add_space(46.0);
        // "# GENERAL  TEXT CHANNELS": the channel muted, its category bright
        let (r, _) = sc.allocate_exact_size(vec2(sc.available_width(), 28.0), Sense::hover());
        let y = r.center().y + 4.0;
        if c.kind == 4 {
            icons::draw(sc.painter(), Icon::Folder, Rect::from_center_size(pos2(r.left() + 16.0, y), vec2(12.0, 12.0)), pal().header_secondary);
            super::text::paint(sc.painter(), pos2(r.left() + 24.0, y), Align2::LEFT_CENTER, &c.name.to_uppercase(), bold(12.0), pal().header_secondary, r.width() - 30.0);
        } else {
            super::side::chan_icon(&app.store, sc.painter(), &c, Rect::from_center_size(pos2(r.left() + 16.0, y), vec2(12.0, 12.0)), pal().header_secondary);
            let n = sc.painter().text(pos2(r.left() + 24.0, y), Align2::LEFT_CENTER, c.name.to_uppercase(), bold(12.0), pal().header_secondary);
            if let Some(cat) = app.store.channels.get(&c.parent) {
                super::text::paint(sc.painter(), pos2(n.right() + 6.0, y), Align2::LEFT_CENTER, &cat.name.to_uppercase(), bold(12.0), pal().header_primary, r.right() - n.right() - 10.0);
            }
        }
        let pages: &[(Page, &str)] = if c.kind == 4 {
            &[(Page::Overview, "Overview"), (Page::Permissions, "Permissions")]
        } else {
            &[(Page::Overview, "Overview"), (Page::Permissions, "Permissions"), (Page::Invites, "Invites"), (Page::Integrations, "Integrations")]
        };
        for (p, label) in pages {
            let (r, resp) = sc.allocate_exact_size(vec2(sc.available_width(), 32.0), Sense::click());
            let on = *p == page;
            if on {
                sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
            } else if resp.hovered() {
                sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
            }
            sc.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, *label, medium(16.0), if on || resp.hovered() { pal().header_primary } else { pal().header_secondary });
            app.rects.insert(format!("chanset:{label}"), r);
            if resp.hovered() {
                sc.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                goto = Some(*p);
            }
        }
        sc.add_space(8.0);
        let (r, _) = sc.allocate_exact_size(vec2(sc.available_width(), 1.0), Sense::hover());
        sc.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, faint(51)));
        sc.add_space(8.0);
        let (r, resp) = sc.allocate_exact_size(vec2(sc.available_width(), 32.0), Sense::click());
        if resp.hovered() {
            sc.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_hover);
        }
        let what = if c.kind == 4 { "Delete Category" } else { "Delete Channel" };
        sc.painter().text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, what, medium(16.0), pal().critical);
        icons::draw(sc.painter(), Icon::Trash, Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(16.0, 16.0)), pal().critical);
        app.rects.insert("chanset:delete".into(), r);
        delete = resp.clicked();

        let esc = Rect::from_center_size(pos2(area.right() - 56.0, area.top() + 64.0), vec2(36.0, 36.0));
        let er = ui.interact(esc, ui.id().with("chan_esc"), Sense::click());
        let ink = crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &er));
        ui.painter().circle_stroke(esc.center(), 17.0, Stroke::new(2.0, ink));
        icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(esc.center(), vec2(18.0, 18.0)), ink);
        ui.painter().text(pos2(esc.center().x, esc.bottom() + 14.0), Align2::CENTER_CENTER, "ESC", semi(13.0), pal().header_secondary);
        close = er.clicked();

        let body = Rect::from_min_max(pos2(content_left, area.top()), pos2((content_left + 660.0).min(esc.left() - 40.0), area.bottom()));
        let mut bc = ui.new_child(egui::UiBuilder::new().max_rect(body));
        let out = egui::ScrollArea::vertical().id_salt(("chan_page", page as u8)).auto_shrink([false, false]).show(&mut bc, |ui| {
            ui.data_mut(|d| d.remove_temp::<f32>(egui::Id::new("settings_gap")));
            Frame::new().inner_margin(Margin { left: 0, right: 16, top: 46, bottom: 120 }).show(ui, |ui| match page {
                Page::Overview => overview::overview(app, ui, &c),
                Page::Permissions => perms::permissions(app, ui, &c),
                Page::Invites => extras::invites(app, ui, &c),
                Page::Integrations => extras::integrations(app, ui, &c),
            });
        });
        crate::ui::e2e::scrolled(ui.ctx(), &out);
        save_bar(app, ui, body);
    });
    overview::tag_modal(app, ctx);
    extras::pause_modal(app, ctx);
    if close {
        app.chan_settings = None;
    } else if let Some(p) = goto
        && let Some(s) = &mut app.chan_settings
    {
        if p == Page::Integrations {
            s.sub = Sub::Cards;
            s.hook = None;
        }
        s.page = p;
    }
    if delete {
        super::side::delete_channel(app, ch);
    }
}

fn heading(ui: &mut Ui, t: &str, desc: &str) {
    ui.label(RichText::new(t).font(semi(20.0)).line_height(Some(24.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(desc).font(reg(14.0)).line_height(Some(18.0)).color(pal().text_normal));
    }
    ui.add_space(24.0);
}

/// Discord's bar along the bottom while an edit is unsaved: Reset, or Save Changes.
fn save_bar(app: &mut App, ui: &mut Ui, area: Rect) {
    let Some(s) = app.chan_settings.as_mut() else { return };
    if s.base.is_null() {
        return;
    }
    let changed: serde_json::Map<String, Value> = FIELDS.iter().filter(|k| s.draft[**k] != s.base[**k]).map(|k| (k.to_string(), s.draft[*k].clone())).collect();
    let perms_changed = !same_perms(&s.perms, &parse_overwrites(&s.base));
    let hook = s.hook.as_ref().filter(|h| extras::hook_dirty(s.data.get("webhooks"), h));
    if changed.is_empty() && !perms_changed && hook.is_none() {
        return;
    }
    let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 40.0), vec2(area.width() - 20.0, 52.0));
    ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_floating);
    ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Careful — you have unsaved changes!", semi(16.0), pal().header_primary);
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 7.0))).layout(Layout::right_to_left(Align::Center)));
    let save = super::button(&mut bar, if s.saving { "…" } else { "Save Changes" }, GREEN, 120.0);
    app.rects.insert("chanset:save".into(), save.rect);
    bar.add_space(12.0);
    let reset = bar.add(egui::Label::new(RichText::new("Reset").font(semi(14.0)).color(pal().header_primary)).sense(Sense::click()));
    if reset.clicked() {
        s.draft = s.base.clone();
        s.perms = parse_overwrites(&s.base);
        s.hook = None;
        return;
    }
    if save.clicked() && !s.saving {
        s.saving = true;
        let mut reqs = Vec::new();
        if let Some(h) = hook {
            let mut body = json!({ "name": h.name, "channel_id": h.chan.to_string() });
            if let Some(a) = &h.avatar {
                body["avatar"] = json!(a);
            }
            reqs.push((format!("/webhooks/{}", h.id), body, Tag::Chan("hook_saved")));
        }
        if !changed.is_empty() || perms_changed {
            let mut body = Value::Object(changed);
            if perms_changed {
                body["permission_overwrites"] = s.perms.iter().map(|o| json!({ "id": o.id.to_string(), "type": o.kind, "allow": o.allow.to_string(), "deny": o.deny.to_string() })).collect();
            }
            reqs.push((format!("/channels/{}", s.chan), body, Tag::Chan("saved")));
        }
        for (path, body, tag) in reqs {
            app.rest("PATCH", path, Some(body), tag);
        }
    }
}
