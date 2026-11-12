//! The settings window, laid out like the official client's: a modal over the app, a
//! grouped sidebar on the left that unfolds the open page's sections, the page on the
//! right, ESC to close.

mod account;
mod modals;
mod activity;
mod app;
pub use app::backfill;
mod theme;
pub mod apps;
pub mod billing;
pub mod camera;
mod clips;
mod privacy;
pub use privacy::send;
mod keybinds;
mod notify;
mod search;
mod system;
mod testmode;
mod user;
mod voice;
mod wire;

use egui::{Align, Align2, Color32, CornerRadius, Frame, Id, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::Value;

use super::App;
use super::motion;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::theme::*;

pub use system::combo;
pub use testmode::banner as test_mode_banner;

/// A request discord may answer with a second-factor challenge, from outside the settings.
pub fn guarded(app: &mut App, method: &'static str, path: &str, body: Option<serde_json::Value>) {
    modals::send(app, modals::Done::Toast, method, path, body);
}

/// `guarded` for a change shown before the answer: a refusal drops `key` from the open server page so it reads again.
pub fn guarded_or(app: &mut App, key: &'static str, method: &'static str, path: &str, body: Option<serde_json::Value>) {
    modals::send(app, modals::Done::Stale(key), method, path, body);
}

pub fn verify(app: &mut App, ctx: &egui::Context) {
    if app.settings_page.is_none() {
        modals::verify_modal(app, ctx);
    }
}
pub use user::Forms;

/// Read back what a settings page wrote, for the e2e.
pub fn proto_flag(app: &App, g: u64, f: u64) -> Option<bool> {
    wire::flag(&privacy::current(app), g, f)
}

pub fn proto_num(app: &App, g: u64, f: u64) -> Option<u64> {
    wire::num(&privacy::current(app), g, f)
}

pub fn proto_ids(app: &App, g: u64, f: u64) -> usize {
    wire::ids(&privacy::current(app), g, f).len()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Account,
    Profiles,
    Privacy,
    Messaging,
    Notifications,
    Clips,
    Nitro,
    Boost,
    Subscriptions,
    Gifts,
    Billing,
    Voice,
    Appearance,
    Accessibility,
    System,
    Language,
    Activity,
    Apps,
    Developer,
}

enum Item {
    // a rule, then a small caption
    Head(&'static str),
    Sep,
    Page(Page, &'static str, Icon, &'static [&'static str]),
}

const SIDEBAR: &[Item] = &[
    Item::Page(Page::Account, "Account", Icon::SideAccount, &["Account Info", "Password & Security", "Account Status", "Family Center"]),
    Item::Page(Page::Privacy, "Data & Privacy", Icon::SidePrivacy, &["How Discord Uses My Data", "Profile Privacy"]),
    Item::Page(Page::Messaging, "Messaging Permissions", Icon::SideMessaging, &[]),
    Item::Page(Page::Notifications, "Notifications", Icon::SideBell, &["Overview", "Sounds", "Badges", "Email", "Advanced"]),
    Item::Page(Page::Clips, "Clips", Icon::Clips, &["Clipping", "Capture Settings", "Storage"]),
    Item::Head("Billing"),
    Item::Page(Page::Nitro, "Nitro", Icon::SideNitro, &[]),
    Item::Page(Page::Boost, "Server Boost", Icon::SideBoost, &[]),
    Item::Page(Page::Subscriptions, "Subscriptions", Icon::SideSubs, &[]),
    Item::Page(Page::Gifts, "Gift Inventory", Icon::SideGift, &["Redeem Codes", "The Gifting Badge", "Gifts I Purchased"]),
    Item::Page(Page::Billing, "Billing", Icon::SideBilling, &["Payment Methods", "Transaction History"]),
    Item::Head("Experience"),
    Item::Page(Page::Voice, "Voice & Video", Icon::SideVoice, &["Voice", "Camera", "Streaming", "Sounds", "Soundboard", "Advanced"]),
    Item::Page(Page::Appearance, "Appearance", Icon::SideAppearance, &["Theme", "App Icon", "Messages", "Chat Box", "Search", "Streamer Mode"]),
    Item::Page(
        Page::Accessibility,
        "Accessibility",
        Icon::SideAccess,
        &["Text Readability", "Visual Density", "Color & Contrast", "Reduced Motion", "Audio & Screen Reader"],
    ),
    Item::Page(Page::System, "System", Icon::SideSystem, &["General", "Custom Keybinds", "Default Keybinds"]),
    Item::Page(Page::Language, "Language & Time", Icon::SideLanguage, &[]),
    Item::Head("Games & Apps"),
    Item::Page(Page::Activity, "Activity Privacy", Icon::SideActivity, &["What Activity I Share", "Where I Share Activity", "Who Can Join My Games"]),
    Item::Page(Page::Apps, "Connected Apps", Icon::SideApps, &[]),
    Item::Sep,
    Item::Page(Page::Developer, "Developer", Icon::SideDev, &[]),
];

const SIDE: f32 = 253.0;
const LOGOUT: Color32 = Color32::from_rgb(235, 95, 94);
const REVEAL: Color32 = Color32::from_rgb(99, 116, 244);
pub const TRACK: Color32 = Color32::from_rgb(56, 57, 64);

pub fn label(p: Page) -> &'static str {
    SIDEBAR
        .iter()
        .find_map(|i| match i {
            Item::Page(q, l, ..) if *q == p => Some(*l),
            _ => None,
        })
        .unwrap_or("Profiles")
}

fn faint(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(151, 151, 159, a)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut page) = app.settings_page else { return };
    let ctx = ui.ctx().clone();
    if page == Page::Profiles {
        let guild = app.sform.profile_guild.take();
        crate::ui::profed::open(app, guild);
        page = Page::Account;
        app.settings_page = Some(page);
    }
    // reopening starts every list at the top, and so does moving to another page
    let now = ctx.cumulative_frame_nr();
    if ctx.data_mut(|d| d.get_temp::<u64>(Id::new("settings_frame"))).is_none_or(|t| now > t + 1) {
        app.sform.session += 1;
        app.sform.page = None;
    }
    ctx.data_mut(|d| d.insert_temp(Id::new("settings_frame"), now));
    if app.sform.page != Some(page) {
        app.sform.page = Some(page);
        app.sform.visit += 1;
    }
    let screen = ctx.content_rect();
    let size = vec2((screen.width() - 144.0).min(1400.0), screen.height() - 144.0);
    let frame = Frame::new().fill(solid(pal().bg_primary)).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
    let mut changed = false;
    let mut logout = false;
    // sections register themselves as they draw; the sidebar reads last frame's list
    let seen: Vec<(String, f32)> = ctx.data_mut(|d| d.remove_temp(Id::new("settings_secs"))).unwrap_or_default();
    ctx.data_mut(|d| d.insert_temp(Id::new("settings_secs_seen"), seen));
    let modal = crate::ui::motion::Modal::new("settings".into()).layer().backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(&ctx, |ui| {
        ui.set_min_size(size - vec2(2.0, 2.0));
        ui.set_max_size(size - vec2(2.0, 2.0));
        let area = ui.max_rect();
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        logout = sidebar(app, ui, Rect::from_min_size(area.min, vec2(SIDE, area.height())), page);
        changed = body(app, ui, Rect::from_min_max(pos2(area.left() + SIDE, area.top()), area.max), page);
    });
    modals::modals(app, &ctx);
    crate::ui::profed::show(app, &ctx);
    if changed {
        app.save_settings(&ctx);
    }
    if !app.sform.recording() && modal.should_close() {
        app.settings_page = None;
    }
    if logout {
        app.confirm = Some(crate::ui::Confirm::new("Log Out", "Are you sure you want to logout?", "Log Out", |app: &mut App| {
            app.settings_page = None;
            app.logout();
        }));
    }
}

fn sidebar(app: &mut App, ui: &mut Ui, area: Rect, page: Page) -> bool {
    let mut logout = false;
    let ctx = ui.ctx().clone();
    let me = app.store.user(app.store.me).cloned().unwrap_or_default();

    let card = Rect::from_min_size(area.min + vec2(16.0, 16.0), vec2(220.0, 56.0));
    let resp = ui.interact(card, Id::new("settings_card"), Sense::click());
    app.rects.insert("settings:Edit Profiles".into(), card);
    super::avatar(ui, Rect::from_min_size(area.min + vec2(24.0, 24.0), vec2(48.0, 48.0)), &me.avatar_url());
    super::text::paint(ui.painter(), pos2(area.left() + 84.0, area.top() + 38.0), Align2::LEFT_CENTER, me.display(), medium(16.0), pal().header_primary, 150.0);
    let edit = ui.painter().text(pos2(area.left() + 84.0, area.top() + 59.0), Align2::LEFT_CENTER, "Edit Profiles", reg(14.0), pal().text_muted);
    icons::draw(ui.painter(), Icon::Pencil, Rect::from_center_size(pos2(edit.right() + 11.0, edit.center().y), vec2(14.0, 14.0)), pal().text_muted);
    if resp.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.settings_page = Some(Page::Profiles);
    }

    let sr = Rect::from_min_size(area.min + vec2(16.0, 88.0), vec2(220.0, 40.0));
    ui.painter().rect(sr, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(pos2(sr.left() + 21.0, sr.center().y), vec2(20.0, 20.0)), pal().header_secondary);
    let mut field = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(sr.min + vec2(40.0, 0.0), vec2(170.0, 40.0))).layout(Layout::left_to_right(Align::Center)));
    field.add(
        egui::TextEdit::singleline(&mut app.sform.search)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .font(reg(16.0))
            .hint_text(RichText::new("Search").color(pal().text_muted))
            .desired_width(170.0),
    );

    let q = app.sform.search.to_lowercase();
    let active: String = ctx
        .data(|d| d.get_temp::<Vec<(String, f32)>>(Id::new("settings_secs_seen")))
        .and_then(|v| v.iter().rev().find(|(_, y)| *y <= area.top() + 49.0 + 90.0).or(v.first()).map(|(t, _)| t.clone()))
        .unwrap_or_default();
    let mut goto = None;
    let list = Rect::from_min_max(pos2(area.left(), area.top() + 128.0), area.max);
    let mut scroll = ui.new_child(UiBuilder::new().max_rect(list));
    egui::ScrollArea::vertical().id_salt(("settings_side", app.sform.session)).auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let left = area.left() + 16.0;
        let row = |ui: &mut Ui, id: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(area.width(), 36.0), Sense::hover());
            let row = Rect::from_min_size(pos2(left, r.top()), vec2(220.0, 36.0));
            (row, ui.interact(row, Id::new(("settings_row", id)), Sense::click()))
        };
        ui.add_space(12.0);
        for item in SIDEBAR {
            match item {
                Item::Head(h) if q.is_empty() => {
                    sep(ui, left);
                    ui.add_space(12.0);
                    let (r, _) = ui.allocate_exact_size(vec2(area.width(), 18.0), Sense::hover());
                    ui.painter().text(pos2(left + 8.0, r.center().y), Align2::LEFT_CENTER, *h, medium(14.0), pal().text_muted);
                    ui.add_space(8.0);
                }
                Item::Sep if q.is_empty() => {
                    sep(ui, left);
                    ui.add_space(4.0);
                }
                Item::Page(p, name, icon, subs) if q.is_empty() || name.to_lowercase().contains(&q) || subs.iter().any(|s| s.to_lowercase().contains(&q)) || search::hits(*p, &q) => {
                    let (r, resp) = row(ui, name);
                    if ui.clip_rect().contains_rect(r) {
                        app.rects.insert(format!("settings:{name}"), r);
                    }
                    let on = *p == page;
                    if on {
                        ui.painter().rect_filled(r, CornerRadius::same(8), faint(51));
                    } else {
                        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), faint(20), 0.05);
                    }
                    let col = if on { pal().header_primary } else { pal().text_muted };
                    icons::draw(ui.painter(), *icon, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(20.0, 20.0)), col);
                    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, *name, medium(16.0), col);
                    if resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() && !on {
                        app.settings_page = Some(*p);
                        app.sform.sub = account::Sub::None;
                    }
                    if on && !subs.is_empty() {
                        let top = ui.cursor().top();
                        for s in *subs {
                            let (r, resp) = row(ui, s);
                            if ui.clip_rect().contains_rect(r) {
                                app.rects.insert(format!("settings:{s}"), r);
                            }
                            let here = *s == active;
                            ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, *s, reg(16.0), if here { pal().header_primary } else { pal().text_muted });
                            if here {
                                ui.painter().rect_filled(Rect::from_min_size(pos2(r.left() + 17.0, r.top() + 8.0), vec2(2.0, 20.0)), CornerRadius::same(1), pal().header_primary);
                            }
                            if resp.hovered() {
                                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.clicked() {
                                goto = Some(s.to_string());
                            }
                        }
                        let guide = Rect::from_min_max(pos2(left + 17.0, top + 8.0), pos2(left + 19.0, ui.cursor().top() - 8.0));
                        ui.painter().rect_filled(guide, CornerRadius::same(1), faint(31));
                        // the active marker sits on top of its guide
                        if let Some(i) = subs.iter().position(|s| *s == active) {
                            let y = top + i as f32 * 36.0 + 8.0;
                            ui.painter().rect_filled(Rect::from_min_size(pos2(left + 17.0, y), vec2(2.0, 20.0)), CornerRadius::same(1), pal().header_primary);
                        }
                    }
                    ui.add_space(4.0);
                }
                _ => {}
            }
        }
        if q.is_empty() {
            let (r, resp) = row(ui, "Log Out");
            if ui.clip_rect().contains_rect(r) {
                app.rects.insert("settings:Log Out".into(), r);
            }
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(8), faint(20));
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            icons::draw(ui.painter(), Icon::SideLogout, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(20.0, 20.0)), sat(LOGOUT));
            ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, "Log Out", medium(16.0), sat(LOGOUT));
            logout = resp.clicked();
            ui.add_space(12.0);
            let version = format!("Rustcord {}", env!("CARGO_PKG_VERSION"));
            let (vr, vresp) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::click());
            ui.painter().text(pos2(vr.left() + 10.0, vr.center().y), Align2::LEFT_CENTER, &version, reg(12.0), pal().text_muted);
            if vresp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if vresp.on_hover_text("Click to copy version").clicked() {
                ctx.copy_text(version);
            }
            ui.add_space(20.0);
        }
    });
    if let Some(g) = goto {
        ctx.data_mut(|d| d.insert_temp(Id::new("settings_goto"), g));
    }
    logout
}

fn sep(ui: &mut Ui, left: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(left..=left + 220.0, r.center().y, Stroke::new(1.0, faint(51)));
}

fn body(app: &mut App, ui: &mut Ui, area: Rect, page: Page) -> bool {
    let head = Rect::from_min_size(area.min, vec2(area.width(), 49.0));
    account::crumbs(app, ui, head, label(page));
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, faint(51)));
    let x = Rect::from_min_size(pos2(head.right() - 40.0, head.top() + 8.0), vec2(32.0, 32.0));
    let resp = ui.interact(x, Id::new("settings_close"), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(x, CornerRadius::same(8), faint(20));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(24.0, 24.0)), pal().header_secondary);
    if resp.clicked() {
        app.settings_page = None;
    }

    let view = Rect::from_min_max(pos2(area.left(), head.bottom()), area.max);
    let cols = super::sidebar_view::column(area.left() - SIDE, area.width() + SIDE, SIDE, super::sidebar_view::Kind::Wide, false);
    ui.ctx().data_mut(|d| d.insert_temp(Id::new("notice"), cols.notice));
    let mut changed = false;
    let mut scroll = ui.new_child(UiBuilder::new().max_rect(view));
    let out = egui::ScrollArea::vertical().id_salt(("settings_page", app.sform.visit)).auto_shrink([false, false]).show(&mut scroll, |ui| {
        ui.add_space(64.0);
        let col = Rect::from_x_y_ranges(cols.left..=cols.right, ui.cursor().top()..=f32::INFINITY);
        ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            // dropdowns the way discord draws them in settings
            ui.spacing_mut().interact_size.y = 40.0;
            let v = ui.visuals_mut();
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.weak_bg_fill = Color32::from_black_alpha(31);
                w.bg_fill = Color32::from_black_alpha(31);
                w.bg_stroke = Stroke::new(1.0, faint(61));
                w.corner_radius = 8.into();
            }
            v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
            ui.data_mut(|d| d.remove_temp::<f32>(Id::new("settings_gap")));
            changed = match page {
                Page::Account => account::account(app, ui),
                Page::Profiles => account::account(app, ui),
                Page::Privacy => privacy::data(app, ui),
                Page::Messaging => privacy::messaging(app, ui),
                Page::Notifications => notify::page(app, ui),
                Page::Clips => clips::page(app, ui),
                Page::Nitro => billing::nitro(app, ui),
                Page::Boost => billing::boost(app, ui),
                Page::Subscriptions => billing::subscriptions(app, ui),
                Page::Gifts => billing::gifts(app, ui),
                Page::Billing => billing::billing(app, ui),
                Page::Voice => voice::page(app, ui),
                Page::Appearance => theme::page(app, ui) | app::appearance(app, ui),
                Page::Accessibility => app::accessibility(app, ui),
                Page::System => system::system(app, ui),
                Page::Language => system::language(app, ui),
                Page::Activity => activity::privacy(app, ui),
                Page::Apps => apps::page(app, ui),
                Page::Developer => system::developer(app, ui),
            };
        });
        ui.add_space(96.0);
    });
    crate::ui::e2e::scrolled(ui.ctx(), &out);
    changed
}

/// Settings pages ask for data by key; the first ask fetches it.
pub fn remote<'a>(app: &'a mut App, key: &'static str, path: &str) -> Option<&'a Result<Value, String>> {
    if !app.remote.contains_key(key) {
        app.remote.insert(key, None);
        app.rest("GET", path.to_string(), None, Tag::Remote(key));
    }
    app.remote.get(key).and_then(|v| v.as_ref())
}

pub fn refetch(app: &mut App, key: &'static str) {
    app.remote.remove(key);
}

pub fn on_remote(app: &mut App, key: &str, res: Result<Value, String>) {
    match key {
        "guard" => modals::guard_done(app, res),
        "mfa_finish" => modals::finish_done(app, res),
        "profile_save" => crate::ui::profed::saved(app, res),
        "widgets_save" => crate::ui::profed::widgets_saved(app, res),
        "proto_set" => privacy::proto_done(app, res),
        "conn_auth" => apps::authorize_done(app, res),
        "action" => match res {
            Ok(_) => {}
            Err(e) => app.toast(e),
        },
        _ => {
            if let Some(k) = app.remote.keys().find(|k| **k == key).copied() {
                app.remote.insert(k, Some(res));
            }
        }
    }
}

/// Remember where a control is, for the scripted e2e run to click.
pub fn mark(ui: &Ui, key: &str, r: Rect) {
    mark_in(ui.ctx(), key, r);
}

pub fn mark_in(ctx: &egui::Context, key: &str, r: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(Id::new(("e2e", key)), (r, pass)));
    crate::ui::e2e::stress::record(ctx, key, r);
}

pub fn said(ctx: &egui::Context, key: &str, text: &str) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(Id::new(("e2e_note", key)), (text.to_string(), pass)));
}

// widgets every page is built from. Each one starts with whatever gap the one above left
// (`lead`) and says how much room it wants after itself (`end`), so a page reads as a list.

fn lead(ui: &mut Ui) {
    let g = ui.data_mut(|d| d.remove_temp::<f32>(Id::new("settings_gap"))).unwrap_or(0.0);
    ui.add_space(g);
}

fn end(ui: &Ui, g: f32) {
    ui.data_mut(|d| d.insert_temp(Id::new("settings_gap"), g));
}

/// A page section: the 24px heading, with a rule above it unless it's the first. Its name
/// is what the sidebar's sub-rows scroll to.
pub fn section(ui: &mut Ui, title: &str) {
    section_with(ui, title, None);
}

/// `section` with a secondary button at the right end of the heading row; true when it's clicked.
pub fn section_with(ui: &mut Ui, title: &str, button: Option<&str>) -> bool {
    let id = Id::new("settings_secs");
    let first = ui.data(|d| d.get_temp::<Vec<(String, f32)>>(id)).is_none_or(|v| v.is_empty());
    if !first {
        ui.add_space(52.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0, faint(51));
        ui.add_space(40.0);
    }
    ui.data_mut(|d| d.remove_temp::<f32>(Id::new("settings_gap")));
    let r = ui.add(egui::Label::new(RichText::new(title).font(reg(24.0)).color(pal().header_primary)).selectable(false)).rect;
    ui.data_mut(|d| {
        let mut v = d.get_temp::<Vec<(String, f32)>>(id).unwrap_or_default();
        v.push((title.to_string(), r.top()));
        d.insert_temp(id, v);
    });
    if ui.data_mut(|d| d.get_temp::<String>(Id::new("settings_goto"))).as_deref() == Some(title) {
        ui.data_mut(|d| d.remove_temp::<String>(Id::new("settings_goto")));
        ui.scroll_to_rect(r.translate(vec2(0.0, -64.0)), Some(Align::TOP));
    }
    let mut clicked = false;
    if let Some(text) = button {
        let row = Rect::from_min_max(pos2(r.left(), r.center().y - 20.0), pos2(ui.max_rect().right(), r.center().y + 20.0));
        let mut c = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)));
        clicked = secondary(&mut c, text).clicked();
    }
    end(ui, 33.0);
    clicked
}

/// A rule inside a section.
pub fn divider(ui: &mut Ui) {
    ui.add_space(48.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    end(ui, 49.0);
}

/// The 16px/600 heading above a group of controls.
pub fn sub(ui: &mut Ui, s: &str) {
    lead(ui);
    ui.add(egui::Label::new(RichText::new(s).font(semi(16.0)).color(pal().text_normal)).selectable(false));
    end(ui, 24.0);
}

/// Label and one muted line, ahead of a slider.
pub fn caption(ui: &mut Ui, label: &str, desc: &str) {
    lead(ui);
    ui.add(egui::Label::new(RichText::new(label).font(medium(16.0)).color(pal().header_primary)).selectable(false));
    if !desc.is_empty() {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary)).selectable(false));
    }
    end(ui, 8.0);
}

/// A bold caption above a form field.
pub fn h5(ui: &mut Ui, s: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(s).font(medium(16.0)).color(pal().header_primary));
    ui.add_space(8.0);
}

pub fn note(ui: &mut Ui, s: &str) {
    lead(ui);
    ui.add(egui::Label::new(RichText::new(s).font(reg(14.0)).color(pal().header_secondary)).selectable(false));
    end(ui, 16.0);
}

pub fn switch(ui: &mut Ui, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(48.0, 24.0), Sense::click());
    paint_switch(ui, r, &resp, on);
    resp
}

pub(super) fn paint_switch(ui: &Ui, r: Rect, resp: &egui::Response, on: bool) {
    let t = motion::tween(ui.ctx(), resp.id, on, 0.25, motion::SWITCH);
    let k = t.clamp(0.0, 1.0);
    let edge = Stroke::new(1.0, faint(51).gamma_multiply(1.0 - k));
    ui.painter().rect(r, CornerRadius::same(12), motion::mix(Color32::from_black_alpha(31), pal().blurple, k), edge, egui::StrokeKind::Inside);
    let x = egui::lerp(r.left() + 13.0..=r.right() - 13.0, t);
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    if view().switch_icons {
        let glyph = Rect::from_center_size(pos2(x, r.center().y), vec2(12.0, 12.0));
        icons::draw(ui.painter(), if on { Icon::Check } else { Icon::Close }, glyph, if on { pal().blurple } else { pal().grey });
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// Label and description on the left, switch on the right.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str, desc: &str) -> bool {
    lead(ui);
    let w = ui.available_width();
    let body = (!desc.is_empty()).then(|| ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 124.0));
    let h = 20.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h.max(24.0)), Sense::click());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, label, medium(16.0), pal().header_primary);
    if let Some(g) = body {
        ui.painter().galley(r.left_top() + vec2(0.0, 24.0), g, pal().header_secondary);
    }
    let sw = Rect::from_min_size(pos2(r.right() - 48.0, r.top()), vec2(48.0, 24.0));
    paint_switch(ui, sw, &resp, *on);
    mark(ui, &format!("toggle:{label}"), sw);
    end(ui, if desc.is_empty() { 32.0 } else { 24.0 });
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// Discord's radio list: a ring, the label, a muted line under it.
pub fn radio<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str, &str)]) -> bool {
    let mut changed = false;
    for (v, label, desc) in options {
        lead(ui);
        let on = *value == *v;
        let text = (!desc.is_empty()).then(|| ui.painter().layout(desc.to_string(), reg(14.0), pal().header_secondary, (ui.available_width() - 32.0).max(0.0)));
        let h = text.as_ref().map_or(24.0, |g| (25.0 + g.size().y).max(43.0));
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
        mark(ui, &format!("radio:{label}"), r);
        let c = pos2(r.left() + 11.0, r.top() + 10.5);
        let fill_curve = motion::Curve::Bezier(0.33, 0.0, 0.67, 1.0);
        let (fu, du) = (motion::flip(ui.ctx(), resp.id.with("fill"), on, 0.225, 0.16), motion::flip(ui.ctx(), resp.id.with("dot"), on, 0.333, 0.25));
        let (fill, dot) = if on {
            (
                motion::keys_eased(&[(0.0, 0.0), (0.35, 0.33), (0.4, 1.15), (1.0, 1.0)], fu, fill_curve),
                motion::keys_eased(&[(0.0, 0.0), (0.4, 1.2), (0.75, 1.0), (1.0, 1.0)], du, motion::Curve::Bezier(0.26, 0.21, 0.67, 1.0)),
            )
        } else {
            (
                if fu < 0.99 { motion::keys_eased(&[(0.0, 1.0), (1.0, 0.0)], fu, fill_curve) } else { 0.0 },
                motion::keys_eased(&[(0.0, 1.0), (0.4, 1.0), (1.0, 0.0)], du, motion::Curve::Bezier(0.33, 0.0, 0.25, 1.0)),
            )
        };
        ui.painter().circle_stroke(c, 10.0, Stroke::new(2.0, pal().header_secondary));
        if fill > 0.0 {
            ui.painter().circle_filled(c, 11.0 * fill, pal().blurple);
        }
        if dot > 0.0 {
            ui.painter().circle_filled(c, 4.5 * dot, Color32::WHITE);
        }
        ui.painter().text(pos2(r.left() + 32.0, r.top()), Align2::LEFT_TOP, *label, reg(16.0), pal().text_normal);
        if let Some(g) = text {
            ui.painter().galley(pos2(r.left() + 32.0, r.top() + 25.0), g, pal().header_secondary);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        end(ui, if desc.is_empty() { 12.0 } else { 15.0 });
        if resp.clicked() && !on {
            *value = *v;
            changed = true;
        }
    }
    end(ui, 24.0);
    changed
}

fn knob(ui: &Ui, c: egui::Pos2) {
    ui.painter().circle(c, 8.0, Color32::WHITE, Stroke::new(1.0, faint(61)));
}

/// A plain slider: 4px rail, 16px knob. True when the value moved.
pub fn slider(ui: &mut Ui, w: f32, value: &mut f32, lo: f32, hi: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 16.0), Sense::click_and_drag());
    let rail = Rect::from_center_size(r.center(), vec2(w, 4.0));
    let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let x = r.left() + 8.0 + t * (w - 16.0);
    ui.painter().rect_filled(rail, CornerRadius::same(2), TRACK);
    ui.painter().rect_filled(Rect::from_min_max(rail.min, pos2(x, rail.bottom())), CornerRadius::same(2), pal().blurple);
    knob(ui, pos2(x, r.center().y));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let v = lo + ((p.x - r.left() - 8.0) / (w - 16.0)).clamp(0.0, 1.0) * (hi - lo);
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            return true;
        }
    }
    false
}

/// The slider with labelled tick marks used for font size, spacing and zoom. Ticks sit
/// where their values fall on the rail, so uneven stops show up uneven.
pub fn markers(ui: &mut Ui, value: &mut f32, stops: &[f32], label: impl Fn(f32) -> String, default: f32) -> bool {
    lead(ui);
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 48.0), Sense::click_and_drag());
    let (lo, hi) = (stops[0], stops[stops.len() - 1]);
    let x_of = |v: f32| r.left() + 5.0 + (v - lo) / (hi - lo) * (w - 10.0);
    let at = stops.iter().position(|s| (*s - *value).abs() < 0.001).unwrap_or(0);
    let rail = Rect::from_min_size(pos2(r.left(), r.top() + 34.0), vec2(w, 4.0));
    let p = ui.painter();
    p.rect_filled(rail, CornerRadius::same(2), TRACK);
    p.rect_filled(Rect::from_min_max(rail.min, pos2(x_of(stops[at]), rail.bottom())), CornerRadius::same(2), pal().blurple);
    for s in stops {
        let x = x_of(*s);
        let g = p.layout_no_wrap(label(*s), medium(12.0), if (*s - default).abs() < 0.001 { Color32::from_rgb(65, 160, 99) } else { pal().header_secondary });
        let left = (x - g.size().x / 2.0).clamp(r.left(), r.right() - g.size().x);
        p.galley(pos2(left, r.top()), g, pal().header_secondary);
        p.rect_filled(Rect::from_min_size(pos2(x - 1.0, r.top() + 24.0), vec2(2.0, 24.0)), CornerRadius::same(1), TRACK);
    }
    knob(ui, pos2(x_of(stops[at]), rail.center().y));
    end(ui, 24.0);
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let v = lo + ((pos.x - r.left() - 5.0) / (w - 10.0)).clamp(0.0, 1.0) * (hi - lo);
        let near = stops.iter().copied().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs())).unwrap_or(lo);
        if (near - *value).abs() > 0.001 {
            *value = near;
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    Primary,
    Secondary,
    Danger,
    Filled,
}

pub fn button(ui: &mut Ui, text: &str, look: Look) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.into(), medium(16.0), Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(vec2((g.size().x + 32.0).max(100.0), 40.0), Sense::click());
    let (fill, ink, line) = match look {
        Look::Primary => (pal().blurple, Color32::WHITE, Color32::from_white_alpha(20)),
        Look::Filled => (pal().red, Color32::WHITE, Color32::from_white_alpha(20)),
        Look::Secondary => (faint(31), pal().header_primary, faint(61)),
        Look::Danger => (faint(31), sat(LOGOUT), faint(61)),
    };
    let over = if matches!(look, Look::Secondary | Look::Danger) { faint(51) } else { fill.gamma_multiply(0.85) };
    let fill = motion::mix(fill, over, motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, CornerRadius::same(8), fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, medium(16.0), ink);
    mark(ui, &format!("button:{text}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Look::Primary)
}

pub fn secondary(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Look::Secondary)
}

pub fn danger(ui: &mut Ui, text: &str, filled: bool) -> egui::Response {
    button(ui, text, if filled { Look::Filled } else { Look::Danger })
}

/// A heading and blurb with one button to the right, vertically centred on them.
pub fn action(ui: &mut Ui, head: &str, desc: &str, text: &str, look: Look) -> bool {
    lead(ui);
    let w = ui.available_width();
    let body = (!desc.is_empty()).then(|| ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 160.0));
    let h = (20.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y)).max(40.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let ty = r.center().y - (20.0 + body.as_ref().map_or(0.0, |g| 4.0 + g.size().y)) / 2.0;
    ui.painter().text(pos2(r.left(), ty), Align2::LEFT_TOP, head, medium(16.0), pal().header_primary);
    if let Some(g) = body {
        ui.painter().galley(pos2(r.left(), ty + 24.0), g, pal().header_secondary);
    }
    let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
    let resp = button(&mut c, text, look);
    mark(ui, &format!("action:{head}"), resp.rect);
    end(ui, 32.0);
    resp.clicked()
}

pub fn link(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().link)).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// The dark text box discord uses in forms.
pub fn input(ui: &mut Ui, value: &mut String, hint: &str, password: bool) -> egui::Response {
    Frame::new()
        .fill(Color32::from_black_alpha(31))
        .stroke(Stroke::new(1.0, faint(61)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .password(password)
                    .font(reg(16.0))
                    .text_color(pal().text_normal)
                    .hint_text(RichText::new(hint).color(pal().text_muted))
                    .desired_width(ui.available_width()),
            )
        })
        .inner
}

/// A closed 40px box that opens a menu: the device and keybind pickers.
pub fn dropdown(ui: &mut Ui, w: f32, icon: Option<Icon>, shown: &str, menu: impl FnOnce(&mut Ui)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    ui.painter().rect(r, CornerRadius::same(8), Color32::from_black_alpha(31), Stroke::new(1.0, faint(61)), egui::StrokeKind::Inside);
    let mut x = r.left() + 12.0;
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, Rect::from_center_size(pos2(x + 10.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
        x += 28.0;
    }
    super::text::paint(ui.painter(), pos2(x + 1.0, r.center().y), Align2::LEFT_CENTER, shown, medium(16.0), pal().text_normal, r.right() - x - 40.0);
    icons::draw(ui.painter(), Icon::ChevronDown, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0)), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let id = egui::Popup::default_response_id(&resp);
    if resp.has_focus() && !egui::Popup::is_id_open(ui.ctx(), id) && ui.input_mut(|i| super::take_key(i, egui::Modifiers::ALT, Key::ArrowDown)) {
        egui::Popup::open_id(ui.ctx(), id);
    }
    let mut rows = Vec::new();
    egui::Popup::menu(&resp).width(w).show(|ui| rows = super::rows_of(|| menu(ui)));
    super::select_keys(ui.ctx(), &rows, id);
    mark(ui, &format!("dropdown:{shown}"), r);
    resp
}

/// A card for lists: devices, apps, connections.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    lead(ui);
    let out = Frame::new()
        .fill(pal().bg_primary)
        .stroke(Stroke::new(1.0, faint(61)))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    end(ui, 16.0);
    out
}

pub fn loading(ui: &mut Ui, r: Option<&Result<Value, String>>) -> Option<Value> {
    match r {
        None => {
            crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
            None
        }
        Some(Err(e)) => {
            ui.label(RichText::new(format!("Couldn't load this from Discord: {e}")).font(reg(14.0)).color(pal().red));
            None
        }
        Some(Ok(v)) => Some(v.clone()),
    }
}

pub fn open_web(path: &str) {
    let _ = open::that_detached(format!("https://discord.com{path}"));
}
