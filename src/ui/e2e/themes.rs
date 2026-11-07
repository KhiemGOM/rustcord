//! Steps 4500 to 4599: Appearance. The theme preview panel a free account gets, the custom theme
//! editor, a server's own theme and the default for servers, and the app icon, each driven with
//! real clicks on the settings page and the panel, and each save checked against what the mock
//! received. `python e2e/from_step.py 4500` runs just this group.

use std::time::Duration;

use egui::{Event, Key, Modifiers, Rect, vec2};
use serde_json::json;

use super::live::event;
use super::Run;
use crate::state::Id;
use crate::theme::Fill;
use crate::ui::App;
use crate::ui::settings::Page;
use crate::ui::themes::{self, Panel, icon};

const SERVER: Id = 1002;
const FREE: u64 = 0;
const NITRO: u64 = 2;

fn translucent() -> bool {
    crate::theme::pal().bg_primary.a() < 255
}

impl Run {
    fn scroll_to(&mut self, ctx: &egui::Context, r: Option<Rect>, panel: bool) -> Option<Rect> {
        let screen = ctx.content_rect();
        if let Some(r) = r
            && r.center().y > 150.0
            && r.center().y < screen.bottom() - 110.0
        {
            return Some(r);
        }
        let dir = if r.is_some_and(|r| r.center().y < 150.0) { 240.0 } else { -240.0 };
        let over = if panel { egui::pos2(screen.right() - 140.0, screen.center().y) } else { screen.center() };
        self.queue.push_back(vec![Event::PointerMoved(over)]);
        self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, dir), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
        self.wait = std::time::Instant::now() + Duration::from_millis(250);
        None
    }

    fn retype(&mut self, text: &str) {
        self.queue.push_back(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
        self.queue.push_back(vec![Event::Text(text.into())]);
    }

    pub(super) fn themes_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |k: &str| Self::at(ctx, k);
        let settings_on = |app: &App| app.settings_page == Some(Page::Appearance);
        match self.step {
            4500 => {
                app.store.me_raw["premium_type"] = json!(FREE);
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            4501 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(900);
            }
            4502 => {
                if settings_on(app) && at("button:Preview Theme").is_some() && app.rects.contains_key("theme:Custom Theme") {
                    self.mark("themes_settings_rows");
                    self.shot(ctx, "themes_settings.png");
                    self.next(400);
                }
            }
            4503 => {
                let Some(r) = self.scroll_to(ctx, app.rects.get("theme:Aurora").copied(), false) else { return };
                self.click(r.center());
                self.next(800);
            }
            4504 => {
                if app.themes.panel != Some(Panel::Themes) {
                    return;
                }
                if app.settings_page.is_none() && app.themes.preview == Some(Fill::Preset(19)) {
                    self.mark("themes_preview_opens");
                }
                if translucent() {
                    self.mark("themes_preview_applies");
                }
                if app.settings.custom_theme.is_none() {
                    self.mark("themes_preview_unsaved");
                }
                self.shot(ctx, "themes_panel.png");
                self.next(400);
            }
            4505 => {
                let Some(r) = at("themes:swatch:Hanami") else { return };
                self.click(r.center());
                self.next(500);
            }
            4506 => {
                if app.themes.preview != Some(Fill::Preset(3)) {
                    return;
                }
                self.mark("themes_swatch_previews");
                self.shot(ctx, "themes_panel_hanami.png");
                let Some(r) = at("themes:Unlock with Nitro") else { return };
                self.click(r.center());
                self.next(700);
            }
            4507 => {
                if app.settings_page == Some(Page::Nitro) && app.themes.panel.is_none() {
                    self.mark("themes_unlock_to_nitro");
                    if !translucent() {
                        self.mark("themes_preview_reverts");
                    }
                    if self.open_page(app, "Appearance") {
                        self.next(800);
                    }
                }
            }
            4508 => {
                if !settings_on(app) {
                    return;
                }
                let Some(r) = at("button:Preview Theme") else { return };
                self.click(r.center());
                self.next(600);
            }
            4509 => {
                if app.themes.panel != Some(Panel::Themes) || app.themes.preview.is_some() {
                    return;
                }
                self.shot(ctx, "themes_panel_empty.png");
                let Some(r) = at("themes:Exit Preview") else { return };
                self.click(r.center());
                self.next(600);
            }
            4510 => {
                if !(settings_on(app) && app.themes.panel.is_none()) {
                    return;
                }
                self.mark("themes_exit_returns");
                let Some(r) = self.scroll_to(ctx, at("button:Try it out"), false) else { return };
                self.click(r.center());
                self.next(800);
            }
            4511 => {
                if app.themes.panel != Some(Panel::Editor) {
                    return;
                }
                if app.themes.draft().colors == vec![0x5865f2] && translucent() {
                    self.mark("themes_editor_opens");
                }
                self.shot(ctx, "themes_editor.png");
                let Some(r) = at("themes:Add Color") else { return };
                self.click(r.center());
                self.next(500);
            }
            4512 => {
                if app.themes.draft().colors.len() != 2 {
                    return;
                }
                self.mark("themes_editor_add_color");
                let Some(r) = at("themes:Light Mode") else { return };
                self.click(r.center());
                self.next(500);
            }
            4513 => {
                if !app.themes.draft().light {
                    return;
                }
                self.mark("themes_editor_light");
                self.shot(ctx, "themes_editor_light.png");
                let Some(r) = at("themes:Dark Mode") else { return };
                self.click(r.center());
                self.next(400);
            }
            4514 => {
                if app.themes.draft().light {
                    return;
                }
                let Some(r) = at("themes:hex") else { return };
                self.click(r.center());
                self.next(200);
            }
            4515 => {
                self.retype("#ff0066");
                self.next(500);
            }
            4516 => {
                if !app.themes.draft().colors.contains(&0xff0066) {
                    return;
                }
                self.mark("themes_editor_hex");
                let Some(r) = self.scroll_to(ctx, at("themes:Gradient Direction:input"), true) else { return };
                self.click(r.center());
                self.next(200);
            }
            4517 => {
                self.retype("90");
                self.next(500);
            }
            4518 => {
                if app.themes.draft().angle != 90 {
                    return;
                }
                self.mark("themes_editor_angle");
                let Some(r) = self.scroll_to(ctx, at("themes:Color Intensity:input"), true) else { return };
                self.click(r.center());
                self.next(200);
            }
            4519 => {
                self.retype("40");
                self.next(500);
            }
            4520 => {
                if app.themes.draft().mix != 40 {
                    return;
                }
                self.mark("themes_editor_intensity");
                self.shot(ctx, "themes_editor_edited.png");
                let Some(r) = self.scroll_to(ctx, at("themes:Surprise Me!"), true) else { return };
                self.click(r.center());
                self.next(500);
            }
            4521 => {
                let d = app.themes.draft();
                if d.colors == vec![0x5865f2, 0xff0066] && d.angle == 90 && d.mix == 40 {
                    return;
                }
                self.mark("themes_editor_surprise");
                let Some(r) = self.scroll_to(ctx, at("themes:Reset"), true) else { return };
                self.click(r.center());
                self.next(500);
            }
            4522 => {
                let d = app.themes.draft();
                if d.colors != vec![0x5865f2] || d.mix != 74 || d.angle != 0 {
                    return;
                }
                self.mark("themes_editor_reset");
                if at("themes:Apply").is_none() {
                    self.mark("themes_free_has_no_apply");
                }
                let Some(r) = at("themes:Back to Settings") else { return };
                self.click(r.center());
                self.next(700);
            }
            4523 => {
                if !(settings_on(app) && app.themes.panel.is_none()) {
                    return;
                }
                if app.settings.own_theme.is_none() {
                    self.mark("themes_free_cannot_apply");
                }
                self.key(Key::Escape);
                event(app, "GUILD_UPDATE", json!({ "id": SERVER.to_string(), "name": "Design Club", "icon": "icon1002", "owner_id": "102", "features": ["GUILD_THEME"], "theme": { "enabled": true, "preset_id": 11, "custom_user_theme_settings": null } }));
                self.next(900);
            }
            4524 => {
                if app.store.guilds.get(&SERVER).is_none_or(|g| g.theme != Some(Fill::Preset(11))) {
                    return;
                }
                self.mark("server_theme_parsed");
                app.select_guild(Some(SERVER));
                self.next(900);
            }
            4525 => {
                if app.ui.guild != Some(SERVER) {
                    return;
                }
                if themes::shown(app) == Some(Fill::Preset(11)) && translucent() {
                    self.mark("server_theme_shown");
                }
                self.shot(ctx, "server_theme.png");
                app.select_guild(Some(1000));
                self.next(900);
            }
            4526 => {
                if app.ui.guild != Some(1000) {
                    return;
                }
                if themes::shown(app).is_none() && !translucent() {
                    self.mark("server_theme_left");
                }
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            4527 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(800);
            }
            4528 => {
                let Some(r) = self.scroll_to(ctx, at("dropdown:Use the server's theme"), false) else { return };
                self.click(r.center());
                self.next(500);
            }
            4529 => {
                let Some(r) = at("option:Use my theme") else { return };
                self.shot(ctx, "themes_default_in_servers.png");
                self.click(r.center());
                self.next(600);
            }
            4530 => {
                if app.settings.server_themes {
                    return;
                }
                self.mark("server_theme_default_mine");
                self.key(Key::Escape);
                self.next(600);
            }
            4531 => {
                app.select_guild(Some(SERVER));
                self.next(900);
            }
            4532 => {
                if app.ui.guild != Some(SERVER) {
                    return;
                }
                if themes::shown(app).is_none() && !translucent() {
                    self.mark("server_theme_skipped_when_mine");
                }
                app.select_guild(Some(1000));
                self.next(700);
            }
            4533 => {
                app.store.me_raw["premium_type"] = json!(NITRO);
                let Some(r) = app.rects.get("gear").copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            4534 => {
                if !self.open_page(app, "Appearance") {
                    return;
                }
                self.next(800);
            }
            4535 => {
                if !settings_on(app) {
                    return;
                }
                let Some(r) = self.scroll_to(ctx, app.rects.get("theme:Aurora").copied(), false) else { return };
                self.click(r.center());
                self.next(800);
            }
            4536 => {
                if app.settings.custom_theme == Some(19) && settings_on(app) && app.themes.panel.is_none() && translucent() {
                    self.mark("themes_nitro_applies");
                    self.shot(ctx, "themes_nitro_aurora.png");
                    let Some(r) = self.scroll_to(ctx, at("button:Try it out"), false) else { return };
                    self.click(r.center());
                    self.next(800);
                }
            }
            4537 => {
                if app.themes.panel != Some(Panel::Editor) {
                    return;
                }
                self.shot(ctx, "themes_editor_nitro.png");
                let Some(r) = at("themes:Add Color") else { return };
                self.click(r.center());
                self.next(500);
            }
            4538 => {
                if app.themes.draft().colors.len() != 2 {
                    return;
                }
                let Some(r) = at("themes:hex") else { return };
                self.click(r.center());
                self.next(200);
            }
            4539 => {
                self.retype("#ff0066");
                self.next(500);
            }
            4540 => {
                let Some(r) = self.scroll_to(ctx, at("themes:Gradient Direction:input"), true) else { return };
                self.click(r.center());
                self.next(200);
            }
            4541 => {
                self.retype("90");
                self.next(500);
            }
            4542 => {
                let Some(r) = self.scroll_to(ctx, at("themes:Color Intensity:input"), true) else { return };
                self.click(r.center());
                self.next(200);
            }
            4543 => {
                self.retype("40");
                self.next(500);
            }
            4544 => {
                let d = app.themes.draft();
                if d.colors.len() != 2 || !d.colors.contains(&0xff0066) || d.angle != 90 || d.mix != 40 {
                    return;
                }
                self.shot(ctx, "themes_editor_nitro_edited.png");
                let Some(r) = at("themes:Apply") else { return };
                self.click(r.center());
                self.next(800);
            }
            4545 => {
                if settings_on(app) && app.themes.panel.is_none() && app.settings.custom_theme.is_none() && app.settings.own_theme.as_ref().is_some_and(|c| c.colors.contains(&0xff0066) && c.angle == 90 && c.mix == 40) {
                    self.mark("themes_custom_applied");
                    self.shot(ctx, "themes_custom_applied.png");
                    self.next(300);
                }
            }
            4546 => {
                let Some(r) = self.scroll_to(ctx, at("icon:Goth"), false) else { return };
                self.click(r.center());
                self.next(800);
            }
            4547 => {
                if app.settings.app_icon != "BrandDarkIcon" {
                    return;
                }
                self.mark("app_icon_chosen");
                if icon::applied() == 1 {
                    self.mark("app_icon_on_window");
                    self.shot(ctx, "app_icon_chosen.png");
                    self.next(300);
                } else if self.step_at.elapsed() > Duration::from_secs(10) {
                    self.shot(ctx, "app_icon_chosen.png");
                    self.next(300);
                }
            }
            4548 => {
                app.store.me_raw["premium_type"] = json!(FREE);
                let Some(r) = self.scroll_to(ctx, at("button:Preview Icon"), false) else { return };
                self.click(r.center());
                self.next(800);
            }
            4549 => {
                if app.themes.panel == Some(Panel::Icons) {
                    self.next(500);
                }
            }
            4550 => {
                let Some(r) = at("icon:Charcoal") else { return };
                self.click(r.center());
                self.next(500);
            }
            4551 => {
                if app.themes.icon == Some(3) && app.settings.app_icon == "BrandDarkIcon" {
                    self.mark("app_icon_previews");
                    self.shot(ctx, "app_icon_panel.png");
                    let Some(r) = at("themes:Exit Preview") else { return };
                    self.click(r.center());
                    self.next(700);
                }
            }
            4552 => {
                if settings_on(app) && app.themes.panel.is_none() {
                    self.mark("app_icon_exit");
                    app.settings.custom_theme = None;
                    app.settings.own_theme = None;
                    app.settings.app_icon = "AppIcon".into();
                    app.settings.server_themes = true;
                    app.save_settings(ctx);
                    self.next(300);
                }
            }
            4553 => {
                if app.settings_page.is_none() {
                    let Some(r) = app.rects.get("gear").copied() else { return };
                    self.click(r.center());
                    self.wait = std::time::Instant::now() + Duration::from_millis(800);
                    return;
                }
                if !self.open_page(app, "Accessibility") {
                    return;
                }
                self.next(800);
            }
            4554 => {
                let Some(r) = self.scroll_to(ctx, at("toggle:Enable custom cursor in shop"), false) else { return };
                if !app.settings.shop_cursor {
                    return;
                }
                self.click(r.center());
                self.next(400);
            }
            4555 => {
                if app.settings.shop_cursor {
                    return;
                }
                self.mark("a11y_custom_cursor_row");
                let Some(r) = self.scroll_to(ctx, at("dropdown:Default"), false) else { return };
                self.click(r.center());
                self.next(400);
            }
            4556 => {
                let Some(r) = at("option:No styling") else { return };
                self.click(r.center());
                self.next(400);
            }
            4557 => {
                if app.settings.official_style != 3 {
                    return;
                }
                self.mark("a11y_official_messages_row");
                let Some(r) = self.scroll_to(ctx, at("toggle:Use the legacy chat input"), false) else { return };
                self.click(r.center());
                self.next(400);
            }
            4558 => {
                if !app.settings.legacy_input {
                    return;
                }
                self.mark("a11y_legacy_input_row");
                self.shot(ctx, "a11y_rows.png");
                app.settings.shop_cursor = true;
                app.settings.official_style = 0;
                app.settings.legacy_input = false;
                app.settings.dev = true;
                self.next(200);
            }
            4559 => {
                if !self.open_page(app, "Developer") {
                    return;
                }
                self.next(800);
            }
            4560 => {
                let Some(r) = at("toggle:Application Test Mode") else { return };
                self.click(r.center());
                self.next(500);
            }
            4561 => {
                if app.sform.test_mode.is_none() {
                    return;
                }
                self.shot(ctx, "test_mode_dialog.png");
                let Some(r) = at("test_mode:id") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("123456789012345678".into())]);
                self.next(500);
            }
            4562 => {
                let Some(r) = at("verify_done") else { return };
                self.click(r.center());
                self.next(500);
            }
            4563 => {
                if app.settings.test_app.as_ref().is_some_and(|t| t.id == "123456789012345678" && !t.proxy && t.port == "8080") && app.sform.test_mode.is_none() {
                    self.mark("developer_test_mode_on");
                    self.shot(ctx, "developer_test_mode.png");
                    let Some(r) = at("toggle:Application Test Mode") else { return };
                    self.click(r.center());
                    self.next(500);
                }
            }
            4564 => {
                if app.settings.test_app.is_none() {
                    self.mark("developer_test_mode_off");
                    app.settings.dev = false;
                    self.next(200);
                }
            }
            4565 => {
                if !self.open_page(app, "Connected Apps") {
                    return;
                }
                self.next(1000);
            }
            4566 => {
                let Some(r) = self.scroll_to(ctx, at("about:Dyno"), false) else { return };
                self.click(r.center());
                self.next(500);
            }
            4567 => {
                if app.sform.app_open.iter().any(|k| k.starts_with("about:")) {
                    self.mark("apps_about_and_policies");
                    self.shot(ctx, "apps_about.png");
                    self.key(Key::Escape);
                    self.next(500);
                }
            }
            _ => self.step = 3110,
        }
    }
}
