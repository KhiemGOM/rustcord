//! Steps 3100 and up, the layout sweep at the end of the tour: every user settings page, every
//! server settings page, the role editor and every channel settings page, each shot at the top
//! and then scrolled to the bottom a screen at a time. RUSTCORD_TOUR_SIZE=WxH (what the ui sees,
//! in points) and RUSTCORD_TOUR_ZOOM=<percent> pick the window, so a sweep can be rerun at any size.

use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, vec2};

use super::Run;
use crate::ui::App;
use crate::ui::channel::Page as ChanPage;
use crate::ui::server::{Page as ServerPage, Probe};
use crate::ui::settings::Page as SettingsPage;
use crate::ui::side;

const USER: [SettingsPage; 19] = [
    SettingsPage::Account,
    SettingsPage::Profiles,
    SettingsPage::Privacy,
    SettingsPage::Messaging,
    SettingsPage::Notifications,
    SettingsPage::Clips,
    SettingsPage::Nitro,
    SettingsPage::Boost,
    SettingsPage::Subscriptions,
    SettingsPage::Gifts,
    SettingsPage::Billing,
    SettingsPage::Voice,
    SettingsPage::Appearance,
    SettingsPage::Accessibility,
    SettingsPage::System,
    SettingsPage::Language,
    SettingsPage::Activity,
    SettingsPage::Apps,
    SettingsPage::Developer,
];

const SERVER: [(ServerPage, &str); 18] = [
    (ServerPage::Profile, "profile"),
    (ServerPage::Tag, "tag"),
    (ServerPage::Engagement, "engagement"),
    (ServerPage::Perks, "perks"),
    (ServerPage::Emoji, "emoji"),
    (ServerPage::Stickers, "stickers"),
    (ServerPage::Soundboard, "soundboard"),
    (ServerPage::Members, "members"),
    (ServerPage::Roles, "roles"),
    (ServerPage::Invites, "invites"),
    (ServerPage::Access, "access"),
    (ServerPage::Integrations, "integrations"),
    (ServerPage::Safety, "safety"),
    (ServerPage::Audit, "audit"),
    (ServerPage::Bans, "bans"),
    (ServerPage::AutoMod, "automod"),
    (ServerPage::Community, "community"),
    (ServerPage::Template, "template"),
];

const CHAN: [(ChanPage, &str); 4] = [(ChanPage::Overview, "overview"), (ChanPage::Permissions, "permissions"), (ChanPage::Invites, "invites"), (ChanPage::Integrations, "integrations")];

#[derive(Clone, Copy)]
enum Target {
    User(SettingsPage),
    Server(ServerPage, &'static str),
    Role(u8),
    // the unsaved-changes bar over a page, with an error toast up as well
    Bar(&'static str),
    Chan(ChanPage, &'static str),
}

impl Target {
    fn name(&self, i: usize) -> String {
        let slug = |s: &str| -> String { s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect() };
        match self {
            Target::User(p) => format!("settings_{i:02}_{}", slug(crate::ui::settings::label(*p))),
            Target::Server(_, s) => format!("server_{i:02}_{s}"),
            Target::Bar(k) => format!("{k}_{i:02}_unsaved_bar"),
            Target::Role(t) => format!("server_{i:02}_role_{}", if *t == 0 { "display" } else { "permissions" }),
            Target::Chan(_, s) => format!("chan_{i:02}_{s}"),
        }
    }
}

#[derive(Default)]
pub struct State {
    targets: Vec<Target>,
    i: usize,
    part: usize,
    offset: f32,
    ready: bool,
    stage: u8,
    skipped: Vec<String>,
}

/// Where a scroll area is, for the sweep to page through it: written every frame by the settings
/// layers' pages and read back by the next step.
pub fn scrolled<R>(ctx: &egui::Context, out: &egui::scroll_area::ScrollAreaOutput<R>) {
    let now = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("tour_scroll"), (now, out.state.offset.y, out.content_size.y, out.inner_rect)));
}

fn read(ctx: &egui::Context) -> Option<(f32, f32, egui::Rect)> {
    let (at, off, content, view) = ctx.data(|d| d.get_temp::<(u64, f32, f32, egui::Rect)>(egui::Id::new("tour_scroll")))?;
    (ctx.cumulative_frame_nr() <= at + 2).then_some((off, content, view))
}

impl Run {
    pub(super) fn tour_setup(&mut self, app: &mut App, ctx: &egui::Context) {
        let size = std::env::var("RUSTCORD_TOUR_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
        });
        let zoom = std::env::var("RUSTCORD_TOUR_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).map_or(1.0, |z| z / 100.0);
        app.settings.zoom = zoom;
        app.apply_settings(ctx);
        if let Some((w, h)) = size {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w * zoom, h * zoom)));
            self.wait = Instant::now() + Duration::from_millis(2000);
        }
    }

    pub(super) fn sweep_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        match self.step {
            3100 => {
                let screen = ctx.content_rect();
                self.rects.insert("screen_points".into(), serde_json::json!([screen.width(), screen.height()]));
                self.rects.insert("pixels_per_point".into(), serde_json::json!(ctx.pixels_per_point()));
                let mut t: Vec<Target> = USER.iter().map(|p| Target::User(*p)).collect();
                t.push(Target::Bar("settings"));
                if std::env::var_os("RUSTCORD_TOUR_SERVER").is_some() {
                    t.extend(SERVER.iter().map(|(p, s)| Target::Server(*p, s)));
                    t.extend([Target::Role(0), Target::Role(1), Target::Bar("server")]);
                    t.extend(CHAN.iter().map(|(p, s)| Target::Chan(*p, s)));
                }
                self.sweep.targets = t;
                self.next(0);
            }
            3101 => {
                let Some(t) = self.sweep.targets.get(self.sweep.i).copied() else {
                    self.rects.insert("sweep_skipped".into(), serde_json::json!(self.sweep.skipped));
                    self.mark("sweep");
                    self.step = 99;
                    return;
                };
                if self.step_at.elapsed() > Duration::from_secs(15) {
                    let why = format!("{} settings={:?} server={} chan={} ready={}", t.name(self.sweep.i), app.settings_page, app.server.is_some(), app.chan_settings.is_some(), self.sweep.ready);
                    let _ = std::fs::write(self.dir.join("progress"), &why);
                    self.sweep.skipped.push(why);
                    self.sweep.i += 1;
                    self.sweep.ready = false;
                    self.step_at = Instant::now();
                    return;
                }
                if self.go(app, ctx, t) {
                    self.sweep.part = 0;
                    self.sweep.offset = -1.0;
                    self.next(1200);
                }
            }
            3102 => {
                let t = self.sweep.targets[self.sweep.i];
                let name = format!("{}_{}.png", t.name(self.sweep.i), self.sweep.part);
                self.shot(ctx, &name);
                self.sweep.part += 1;
                self.next(300);
            }
            3103 => {
                let (off, content, view) = read(ctx).unwrap_or((0.0, 0.0, egui::Rect::NOTHING));
                let end = off + view.height() >= content - 2.0 || self.sweep.part >= 14 || (off - self.sweep.offset).abs() < 0.5;
                if end || view == egui::Rect::NOTHING {
                    self.sweep.i += 1;
                    self.sweep.ready = false;
                    self.sweep.stage = 0;
                    self.step = 3101;
                    self.step_at = Instant::now();
                    self.wait = Instant::now() + Duration::from_millis(300);
                    return;
                }
                self.sweep.offset = off;
                self.queue.push_back(vec![Event::PointerMoved(view.center())]);
                self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -(view.height() - 120.0)), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
                self.step = 3102;
                self.wait = Instant::now() + Duration::from_millis(900);
            }
            _ => self.step = 99,
        }
    }

    fn editor_up(&mut self, app: &mut App) -> bool {
        if app.profed.as_ref().is_some_and(|e| e.ready) {
            return true;
        }
        if app.profed.is_none() {
            app.settings_page = Some(SettingsPage::Profiles);
            self.wait = Instant::now() + Duration::from_millis(1500);
        }
        false
    }

    fn go(&mut self, app: &mut App, ctx: &egui::Context, t: Target) -> bool {
        let guild = self.target.0;
        if !matches!(t, Target::User(SettingsPage::Profiles) | Target::Bar("settings")) {
            app.profed = None;
        }
        match t {
            Target::User(page) => {
                if app.server.is_some() || app.chan_settings.is_some() {
                    app.server = None;
                    app.chan_settings = None;
                }
                if page == SettingsPage::Profiles {
                    return self.editor_up(app);
                }
                if app.settings_page.is_none() {
                    if let Some(r) = app.rects.get("gear").copied() {
                        self.click(r.center());
                        self.wait = Instant::now() + Duration::from_millis(800);
                    }
                    return false;
                }
                if app.settings_page != Some(page) {
                    app.settings_page = Some(page);
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    if page == SettingsPage::Voice {
                        for key in ["advanced", "sounds", "diagnostics"] {
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new(("voice_open", key)), true));
                        }
                    }
                    return false;
                }
                if !self.sweep.ready {
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(1500);
                    return false;
                }
                true
            }
            Target::Server(page, _) => {
                if app.settings_page.is_some() {
                    self.key(Key::Escape);
                    self.wait = Instant::now() + Duration::from_millis(600);
                    return false;
                }
                if app.server.is_none() {
                    app.open_server_settings(guild);
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                if let Some(s) = &mut app.server
                    && s.page != page
                {
                    s.page = page;
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                self.sweep.ready
            }
            Target::Bar(kind) => {
                let user = kind == "settings";
                if user && !self.editor_up(app) {
                    return false;
                }
                if !user && app.settings_page.is_some() {
                    self.key(Key::Escape);
                    self.wait = Instant::now() + Duration::from_millis(600);
                    return false;
                }
                if !user && app.server.is_none() {
                    app.open_server_settings(guild);
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                if let Some(s) = &mut app.server
                    && s.page != ServerPage::Profile
                {
                    s.page = ServerPage::Profile;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                match self.sweep.stage {
                    0 => {
                        let key = if user { "profile:pronouns" } else { "server_profile:name" };
                        let Some(r) = Self::at(ctx, key) else { return false };
                        self.click(r.center());
                        self.queue.push_back(vec![Event::Text("x".into())]);
                        self.sweep.stage = 1;
                        self.wait = Instant::now() + Duration::from_millis(800);
                        false
                    }
                    _ => {
                        app.toast("Something went wrong. Shift Click this notification to see error details.");
                        self.wait = Instant::now() + Duration::from_millis(700);
                        self.sweep.stage = 0;
                        true
                    }
                }
            }
            Target::Role(tab) => {
                self.sweep.ready = false;
                matches!(crate::ui::server::probe(app, tab), Probe::Shot(_))
            }
            Target::Chan(page, _) => {
                app.server = None;
                if app.chan_settings.is_none() {
                    let chan = side::first_channel(&app.store, guild).unwrap_or(0);
                    app.open_chan_settings(chan);
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                if let Some(s) = &mut app.chan_settings
                    && s.page != page
                {
                    s.page = page;
                    self.sweep.ready = true;
                    self.wait = Instant::now() + Duration::from_millis(2500);
                    return false;
                }
                self.sweep.ready
            }
        }
    }
}
