//! Steps 5200 to 5299: scheduled messages from the plus menu to the inbox's Scheduled tab.
//! `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5200` runs just this group.

use std::time::{Duration, Instant};

use egui::{Event, Key};

use super::Run;
use crate::theme::Fill;
use crate::ui::titlebar::Inbox;
use crate::ui::{App, themes};

const BOT: u64 = 104;
const HUB_GUILD: u64 = 1001;
const THEMED: u64 = 1002;
const NEWS: u64 = 2002;
const ALICE: u64 = 101;
const ALICE_DM: u64 = 3000;

fn menu(ctx: &egui::Context, label: &str) -> Option<egui::Rect> {
    Run::at(ctx, &format!("menu_row:{label}"))
}

impl Run {
    fn tool(&mut self, ctx: &egui::Context, tip: &str) -> bool {
        match Self::at(ctx, &format!("sched:tool:{tip}")) {
            Some(r) => {
                self.click(r.center());
                true
            }
            None => false,
        }
    }

    pub(super) fn r1_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5200 => {
                app.inbox = None;
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(800);
            }
            5201 => {
                let Some(r) = app.rects.get("plus_button").copied() else { return };
                self.click(r.center());
                self.next(400);
            }
            5202 => {
                let Some(r) = menu(ctx, "Schedule Message") else {
                    if stuck {
                        self.step = 5220;
                    }
                    return;
                };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(500);
            }
            5203 => {
                let Some(r) = menu(ctx, "Custom...") else { return };
                if menu(ctx, "Monday at 9:00am").is_some() && (menu(ctx, "Today at 9:00am").is_some() || menu(ctx, "Tomorrow at 9:00am").is_some()) {
                    self.mark("sched_menu_presets");
                }
                self.click(r.center());
                self.next(700);
            }
            5204 => {
                let Some(r) = Self::at(ctx, "sched:set") else { return };
                if app.sched.picker.is_none() || self.step_at.elapsed() < Duration::from_millis(600) {
                    return;
                }
                self.click(r.center());
                self.next(500);
            }
            5205 => {
                if !app.sched.drafts.contains_key(&general) || Self::at(ctx, "sched:bar").is_none() {
                    if stuck {
                        self.step = 5220;
                    }
                    return;
                }
                self.mark("sched_bar");
                if let Some(r) = app.rects.get("composer").copied() {
                    self.click(r.center());
                }
                self.queue.push_back(vec![Event::Text("see you later".into())]);
                self.key(Key::Enter);
                self.next(600);
            }
            5206 => {
                let said = app.toast.as_ref().is_some_and(|t| t.0.starts_with("Message scheduled for "));
                if !said {
                    if stuck {
                        self.step = 5220;
                    }
                    return;
                }
                if !app.sched.drafts.contains_key(&general) && !app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.content == "see you later")) {
                    self.mark("sched_made");
                }
                app.inbox = Some(Inbox { tab: 4, mentions: None, saved: None, friends: false });
                app.sched.asked = false;
                self.next(800);
            }
            5207 => {
                let Some(l) = &app.sched.list else { return };
                if l.len() == 1 && l[0].msg.content == "see you later" && Self::at(ctx, "sched:tool:Send now").is_some() {
                    self.mark("sched_listed");
                    self.shot(ctx, "scheduled_inbox.png");
                }
                if self.tool(ctx, "Edit message") {
                    self.next(500);
                }
            }
            5208 => {
                if app.sched.edit.is_none() {
                    return;
                }
                self.queue.push_back(vec![Event::Text(" then".into())]);
                self.key(Key::Enter);
                self.next(900);
            }
            5209 => {
                if !app.sched.list.as_ref().is_some_and(|l| l.iter().any(|i| i.msg.content == "see you later then")) {
                    if stuck {
                        self.step = 5220;
                    }
                    return;
                }
                self.mark("sched_edited");
                if self.tool(ctx, "Edit schedule time") {
                    self.next(700);
                }
            }
            5210 => {
                let Some(r) = Self::at(ctx, "sched:set") else { return };
                if self.step_at.elapsed() < Duration::from_millis(600) {
                    return;
                }
                if let Some(p) = &mut app.sched.picker {
                    p.push_days(1);
                }
                self.wait = Instant::now() + Duration::from_millis(200);
                self.click(r.center());
                self.next(900);
            }
            5211 => {
                if app.sched.picker.is_some() {
                    return;
                }
                if app.toast.as_ref().is_some_and(|t| t.0 == "Scheduled message updated") {
                    self.mark("sched_retimed");
                }
                if self.tool(ctx, "Cancel scheduled message") {
                    self.next(900);
                }
            }
            5212 => {
                if !app.sched.list.as_ref().is_some_and(Vec::is_empty) {
                    if stuck {
                        self.step = 5220;
                    }
                    return;
                }
                self.mark("sched_cancelled");
                app.inbox = None;
                self.step = 5220;
            }
            5220 => {
                app.inbox = None;
                app.sched.picker = None;
                app.open_full(BOT, self.target.0, crate::ui::profile::Tab::Activity);
                self.next(900);
            }
            5221 => {
                let Some(r) = Self::at(ctx, &format!("fp_tab:{}", crate::ui::profile::Tab::Data as u8)) else {
                    if stuck {
                        self.step = 5230;
                    }
                    return;
                };
                if Self::at(ctx, &format!("fp_tab:{}", crate::ui::profile::Tab::Bio as u8)).is_some() && Self::at(ctx, &format!("fp_tab:{}", crate::ui::profile::Tab::Activity as u8)).is_none() {
                    self.mark("bot_profile_tabs");
                }
                self.click(r.center());
                self.next(600);
            }
            5222 => {
                if Self::at(ctx, "bot_access:Read Messages").is_some() && Self::at(ctx, "bot_access:Members").is_some() && Self::at(ctx, "bot_access:Presence").is_none() {
                    self.mark("bot_data_access");
                    self.shot(ctx, "bot_data_access.png");
                }
                self.next(300);
            }
            5223 => {
                app.full_profile = None;
                app.select_guild(None);
                app.open_shop(crate::ui::shop::Page::Shop);
                self.next(1200);
            }
            5224 => {
                let Some(r) = Self::at(ctx, "shop:orbs") else {
                    if stuck {
                        self.step = 5290;
                    }
                    return;
                };
                if Self::at(ctx, "orbs:dot").is_some() {
                    self.mark("orb_dot");
                } else if !stuck {
                    return;
                }
                self.click(r.center());
                self.next(900);
            }
            5225 => {
                let Some(claim) = Self::at(ctx, "wallet:claim:first_message") else {
                    if stuck {
                        self.step = 5290;
                    }
                    return;
                };
                if Self::at(ctx, "wallet:Quests").is_some() && Self::at(ctx, "wallet:Shop").is_some() && Self::at(ctx, "wallet:challenge:shop_visit").is_some() && Self::at(ctx, "wallet:card").is_some() {
                    self.mark("wallet_opens");
                    self.shot(ctx, "orb_wallet.png");
                }
                self.click(claim.center());
                self.next(1200);
            }
            5226 => {
                if Self::at(ctx, "wallet:challenge:shop_visit").is_some() && Self::at(ctx, "wallet:challenge:first_message").is_none() {
                    if Self::at(ctx, "orbs:dot").is_none() {
                        self.mark("orb_dot_cleared");
                    }
                    self.mark("wallet_claimed");
                } else if !stuck {
                    return;
                }
                self.key(Key::Escape);
                app.rest("POST", "/mock/wallet-reset".into(), Some(serde_json::json!({})), crate::net::Tag::Quiet);
                self.next(500);
            }
            5227 => {
                let Some(r) = Self::at(ctx, "shop:tab:Game Shops") else {
                    if stuck {
                        app.shop = None;
                        self.step = 5290;
                    }
                    return;
                };
                self.click(r.center());
                self.next(500);
            }
            5228 => {
                let Some(r) = Self::at(ctx, "shop:game:9004") else { return };
                if Self::at(ctx, "shop:game:9001").is_none() {
                    self.mark("game_shops_menu");
                }
                self.click(r.center());
                self.next(900);
            }
            5229 => {
                let Some(r) = Self::at(ctx, "game_shop:page:1") else {
                    if stuck {
                        app.shop = None;
                        self.step = 5290;
                    }
                    return;
                };
                if app.shop.as_ref().is_some_and(|s| s.game.as_deref() == Some("9004") && s.tab == 3) {
                    self.mark("game_shop_page");
                    self.shot(ctx, "game_shop.png");
                }
                self.click(r.center());
                self.next(500);
            }
            5230 => {
                if app.shop.as_ref().is_some_and(|s| s.game_page == 1) {
                    self.mark("game_shop_pages");
                }
                app.shop = None;
                self.step = 5240;
            }
            5240 => {
                app.shop = None;
                app.select_guild(Some(HUB_GUILD));
                self.next(900);
            }
            5241 => {
                let Some(r) = Self::at(ctx, "home_row:Server Hub") else {
                    if stuck {
                        self.step = 5290;
                    }
                    return;
                };
                if Self::at(ctx, "home_row:Events").is_none_or(|e| e.top() > r.top()) {
                    self.mark("hub_row");
                }
                self.click(r.center());
                self.next(900);
            }
            5242 => {
                let Some(r) = Self::at(ctx, "hub:Edit Server Hub") else {
                    if stuck {
                        self.step = 5247;
                    }
                    return;
                };
                if Self::at(ctx, "hub:welcome").is_some() && app.hub.is_some() {
                    self.mark("hub_welcome");
                    self.shot(ctx, "server_hub.png");
                }
                self.click(r.center());
                self.next(500);
            }
            5243 => {
                let Some(r) = Self::at(ctx, "hub:add:0") else {
                    if stuck {
                        self.step = 5247;
                    }
                    return;
                };
                self.click(r.center());
                self.next(500);
            }
            5244 => {
                let Some(r) = menu(ctx, "Whiteboard") else {
                    if stuck {
                        self.step = 5247;
                    }
                    return;
                };
                self.click(r.center());
                self.next(1000);
            }
            5245 => {
                let (Some(_), Some(publish)) = (Self::at(ctx, "hub:remove:0"), Self::at(ctx, "hub:Publish")) else {
                    if stuck {
                        self.step = 5247;
                    }
                    return;
                };
                self.mark("hub_widget_added");
                self.click(publish.center());
                self.next(700);
            }
            5246 => {
                if app.store.guilds.get(&HUB_GUILD).is_some_and(|g| g.hub_published) {
                    self.mark("hub_published");
                } else if !stuck {
                    return;
                }
                self.step = 5247;
            }
            5247 => {
                app.rest("POST", "/mock/hub-reset".into(), Some(serde_json::json!({})), crate::net::Tag::Quiet);
                if let Some(g) = app.store.guilds.get_mut(&HUB_GUILD) {
                    g.hub_published = false;
                    g.hub_enabled = None;
                }
                app.hub = None;
                self.step = 5250;
            }
            5250 => {
                app.toast = None;
                app.open_servers();
                self.next(1200);
            }
            5251 => {
                let Some(n) = app.directory.as_ref().map(|d| d.sv.cats.len() + 1).filter(|n| *n > 1) else {
                    if stuck {
                        self.step = 5265;
                    }
                    return;
                };
                let Some(r) = app.rects.get(&format!("discover:tab:{n}")).copied() else { return };
                self.click(r.center());
                self.next(700);
            }
            5252 => {
                let on = app.directory.as_ref().is_some_and(|d| d.tab == d.sv.cats.len() + 1);
                let Some(r) = app.rects.get("discover:hub_join").copied().filter(|_| on) else {
                    if stuck {
                        self.step = 5265;
                    }
                    return;
                };
                self.mark("hub_tab");
                self.shot(ctx, "student_hubs.png");
                self.click(r.center());
                self.next(700);
            }
            5253 => {
                let (Some(j), Some(r)) = (app.hub_join.as_mut(), Self::at(ctx, "hub_join:go")) else {
                    if stuck {
                        self.step = 5264;
                    }
                    return;
                };
                j.email = "sam@mock.edu".into();
                self.wait = Instant::now() + Duration::from_millis(200);
                self.click(r.center());
                self.next(900);
            }
            5254 => {
                let Some(r) = Self::at(ctx, "hub_join:school:1003") else {
                    if stuck {
                        self.step = 5264;
                    }
                    return;
                };
                if Self::at(ctx, "hub_join:school:1001").is_some() {
                    self.mark("hub_schools");
                    self.shot(ctx, "hub_schools.png");
                }
                self.click(r.center());
                self.next(900);
            }
            5255 => {
                let (Some(j), Some(r)) = (app.hub_join.as_mut().filter(|j| j.step == crate::ui::hubjoin::Step::Code), Self::at(ctx, "hub_join:submit")) else {
                    if stuck {
                        self.step = 5264;
                    }
                    return;
                };
                j.code = "246810".into();
                self.mark("hub_code_asked");
                self.wait = Instant::now() + Duration::from_millis(200);
                self.click(r.center());
                self.next(900);
            }
            5256 => {
                if app.hub_join.is_none() && app.ui.guild == Some(1003) {
                    self.mark("hub_joined");
                } else if !stuck {
                    return;
                }
                app.open_servers();
                app.hub_join = Some(Default::default());
                self.next(700);
            }
            5257 => {
                let (Some(j), Some(r)) = (app.hub_join.as_mut(), Self::at(ctx, "hub_join:go")) else {
                    if stuck {
                        self.step = 5264;
                    }
                    return;
                };
                j.email = "sam@nowhere.example".into();
                self.wait = Instant::now() + Duration::from_millis(200);
                self.click(r.center());
                self.next(900);
            }
            5258 => {
                let (Some(j), Some(r)) = (app.hub_join.as_mut().filter(|j| j.step == crate::ui::hubjoin::Step::Waitlist), Self::at(ctx, "hub_join:waitlist")) else {
                    if stuck {
                        self.step = 5264;
                    }
                    return;
                };
                j.school = "Nowhere College".into();
                self.wait = Instant::now() + Duration::from_millis(200);
                self.click(r.center());
                self.next(900);
            }
            5259 => {
                if app.hub_join.as_ref().is_some_and(|j| j.step == crate::ui::hubjoin::Step::Listed) {
                    self.mark("hub_waitlisted");
                    self.shot(ctx, "hub_waitlisted.png");
                } else if !stuck {
                    return;
                }
                self.step = 5264;
            }
            5264 => {
                app.hub_join = None;
                app.directory = None;
                self.step = 5265;
            }
            5265 => {
                self.text = app.store.me_raw["premium_type"].to_string();
                app.store.me_raw["premium_type"] = serde_json::json!(2);
                app.settings_page = Some(crate::ui::settings::Page::Nitro);
                self.next(1500);
            }
            5266 => {
                let Some(r) = Self::at(ctx, "button:Select Friends") else {
                    if stuck {
                        self.step = 5270;
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.shot(ctx, "share_nitro.png");
                self.click(r.center());
                self.next(900);
            }
            5267 => {
                let (Some(a), Some(_)) = (Self::at(ctx, "referral:user:101"), Self::at(ctx, "referral:user:102")) else {
                    if stuck {
                        self.step = 5270;
                    }
                    return;
                };
                if app.referral.is_some() {
                    self.mark("nitro_friends");
                    self.shot(ctx, "share_nitro_friends.png");
                }
                self.click(a.center());
                self.next(400);
            }
            5268 => {
                let (Some(b), Some(send)) = (Self::at(ctx, "referral:user:102"), Self::at(ctx, "referral:send")) else { return };
                self.click(b.center());
                self.wait = Instant::now() + Duration::from_millis(400);
                self.click(send.center());
                self.next(1200);
            }
            5269 => {
                let sent = match app.remote.get("referrals") {
                    Some(Some(Ok(v))) => v["sent_user_ids"].as_array().map_or(0, Vec::len),
                    _ => 0,
                };
                if app.referral.is_none() && sent == 2 {
                    self.mark("nitro_shared");
                } else if !stuck {
                    return;
                }
                self.step = 5270;
            }
            5270 => {
                app.referral = None;
                app.settings_page = None;
                app.store.me_raw["premium_type"] = serde_json::from_str(&self.text).unwrap_or_default();
                self.text.clear();
                app.rest("POST", "/mock/school-reset".into(), Some(serde_json::json!({})), crate::net::Tag::Quiet);
                crate::ui::settings::refetch(app, "referrals");
                self.step = 5272;
            }
            5272 => {
                super::live::event(app, "GUILD_UPDATE", serde_json::json!({ "id": THEMED.to_string(), "name": "Design Club", "icon": "icon1002", "owner_id": "102", "features": ["GUILD_THEME"], "theme": { "enabled": true, "preset_id": 11, "custom_user_theme_settings": null } }));
                themes::set_server_theme(app, ctx, THEMED, 2);
                app.select_guild(Some(THEMED));
                self.next(900);
            }
            5273 => {
                if themes::shown(app) != Some(Fill::Preset(11)) {
                    if stuck {
                        self.step = 5275;
                    }
                    return;
                }
                let Some(r) = app.rects.get("guildmenu").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            5274 => {
                let Some(r) = menu(ctx, "Use Server Theme") else {
                    if stuck {
                        self.step = 5275;
                    }
                    return;
                };
                self.shot(ctx, "use_server_theme.png");
                self.click(r.center());
                self.next(700);
            }
            5275 => {
                if !themes::uses_server_theme(app, THEMED) && themes::shown(app).is_none() && app.ui.guild == Some(THEMED) {
                    self.mark("server_theme_off_here");
                } else if !stuck {
                    return;
                }
                themes::set_server_theme(app, ctx, THEMED, 0);
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(700);
            }
            5276 => {
                self.text = serde_json::to_string(&app.settings.theme).unwrap_or_default();
                app.settings.theme = crate::theme::Theme::Light;
                app.apply_look(ctx);
                app.settings_page = Some(crate::ui::settings::Page::Appearance);
                self.next(900);
            }
            5277 => {
                let Some(r) = self.scroll_to(ctx, Self::at(ctx, "toggle:Dark sidebar"), false) else {
                    if stuck {
                        self.step = 5279;
                    }
                    return;
                };
                self.click(r.center());
                self.next(700);
            }
            5278 => {
                if !app.settings.dark_sidebar {
                    if stuck {
                        self.step = 5279;
                    }
                    return;
                }
                if app.settings_page.is_some() {
                    app.settings_page = None;
                    self.wait = Instant::now() + Duration::from_millis(600);
                    return;
                }
                let Some(rail) = app.rects.get("guildmenu").copied() else { return };
                if themes::dark_sidebar(app).is_some() {
                    self.mark("dark_sidebar_on");
                }
                let screen = ctx.content_rect();
                let shot = "dark_sidebar.png".to_string();
                self.probes.push((shot.clone(), vec![("rail".into(), super::Probe::At(egui::pos2(10.0, screen.center().y))), ("side".into(), super::Probe::At(egui::pos2(rail.center().x, rail.top() - 5.0))), ("chat".into(), super::Probe::Bright(egui::Rect::from_center_size(egui::pos2(screen.right() - 400.0, screen.center().y), egui::vec2(40.0, 40.0))))]));
                self.shot(ctx, &shot);
                self.next(500);
            }
            5279 => {
                app.settings_page = None;
                if app.settings.dark_sidebar {
                    app.settings.dark_sidebar = false;
                    themes::push(app);
                }
                app.settings.theme = serde_json::from_str(&self.text).unwrap_or(crate::theme::Theme::Dark);
                self.text.clear();
                app.save_settings(ctx);
                app.apply_look(ctx);
                self.next(600);
            }
            5280 => {
                self.text = app.store.me_raw["premium_type"].to_string();
                app.store.me_raw["premium_type"] = serde_json::json!(2);
                themes::open_editor(app, ctx, themes::Back::Settings);
                self.next(900);
            }
            5281 => {
                let screen = ctx.content_rect();
                let Some(r) = Self::at(ctx, "themes:share").filter(|r| r.bottom() < screen.bottom() - 60.0) else {
                    if stuck {
                        self.shot(ctx, "theme_share_missing.png");
                        self.step = 5285;
                        return;
                    }
                    self.queue.push_back(vec![Event::PointerMoved(egui::pos2(screen.right() - 140.0, screen.center().y))]);
                    self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -240.0), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE }]);
                    self.wait = Instant::now() + Duration::from_millis(300);
                    return;
                };
                self.click(r.center());
                self.next(900);
            }
            5282 => {
                let Some(r) = Self::at(ctx, &format!("forward:{general}")).filter(|_| app.forward.is_some()) else {
                    if stuck {
                        self.step = 5285;
                    }
                    return;
                };
                self.shot(ctx, "share_theme.png");
                self.click(r.center());
                self.wait = Instant::now() + Duration::from_millis(500);
                self.key(Key::Enter);
                self.next(900);
            }
            5283 => {
                let card = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.extra["shared_client_theme"]["colors"].is_array()).map(|m| m.id));
                let Some(id) = card.filter(|_| app.forward.is_none()) else {
                    if stuck {
                        self.step = 5285;
                    }
                    return;
                };
                if app.themes.panel.is_some() {
                    themes::close(app, ctx, false);
                    self.wait = Instant::now() + Duration::from_millis(700);
                    return;
                }
                let Some(r) = app.rects.get(&format!("theme_card:{id}")).copied().filter(|r| ctx.content_rect().contains(r.center())) else {
                    if stuck {
                        self.shot(ctx, "theme_card_missing.png");
                        self.step = 5285;
                        return;
                    }
                    app.jump_to(general, id);
                    self.wait = Instant::now() + Duration::from_millis(700);
                    return;
                };
                self.mark("theme_shared");
                self.shot(ctx, "theme_card.png");
                self.click(r.center());
                self.next(900);
            }
            5284 => {
                if app.themes.panel == Some(themes::Panel::Editor) {
                    self.mark("theme_previewed");
                } else if !stuck {
                    return;
                }
                self.step = 5285;
            }
            5285 => {
                app.forward = None;
                themes::close(app, ctx, false);
                if let Some(id) = app.store.msgs.get(&general).and_then(|e| e.list.iter().rev().find(|m| m.extra["shared_client_theme"].is_object()).map(|m| m.id)) {
                    app.rest("DELETE", format!("/channels/{general}/messages/{id}"), None, crate::net::Tag::Quiet);
                }
                app.store.me_raw["premium_type"] = serde_json::from_str(&self.text).unwrap_or_default();
                self.text.clear();
                self.step = 5286;
            }
            5286 => {
                app.ask_publish(NEWS, 2);
                self.next(600);
            }
            5287 => {
                let Some(c) = &app.confirm else {
                    if stuck {
                        self.step = 5290;
                    }
                    return;
                };
                if c.title == "Are you sure?" && c.body.starts_with("This message will be sent to 3 servers following this channel.") {
                    self.mark("publish_asks");
                    self.shot(ctx, "publish_confirm.png");
                }
                self.key(Key::Enter);
                self.next(700);
            }
            5288 => {
                if app.confirm.is_none() {
                    self.mark("publish_confirmed");
                } else if !stuck {
                    return;
                }
                app.confirm = None;
                self.step = 5291;
            }
            5291 => {
                let me = app.store.me;
                if let Some(u) = app.store.users.get_mut(&me) {
                    u.tag = None;
                }
                app.tag_pop = Some((HUB_GUILD, egui::pos2(400.0, 300.0)));
                self.next(900);
            }
            5292 => {
                let Some(r) = Self::at(ctx, &format!("invite:tag:{HUB_GUILD}:join")) else {
                    if stuck {
                        app.tag_pop = None;
                        self.step = 5290;
                    }
                    return;
                };
                self.shot(ctx, "tag_adopt.png");
                self.click(r.center());
                self.next(700);
            }
            5293 => {
                if app.tag_pop.is_none() && app.ui.guild != Some(HUB_GUILD) {
                    self.mark("tag_adopted");
                } else if !stuck {
                    return;
                }
                app.tag_pop = None;
                self.step = 5294;
            }
            5294 => {
                super::live::event(app, "PRESENCE_UPDATE", serde_json::json!({ "user": { "id": ALICE.to_string() }, "status": "dnd", "activities": [{ "name": "Rocket Crab", "type": 0, "id": "e2e" }] }));
                app.rects.remove("header:activity");
                app.select_guild(None);
                app.open(ALICE_DM);
                self.next(900);
            }
            5295 => {
                let chip = app.rects.get("header:activity").copied().filter(|r| r.bottom() < 90.0);
                if chip.is_some() && app.store.status(ALICE) == crate::state::Status::Dnd {
                    self.mark("dm_header_activity");
                    self.shot(ctx, "dm_header_activity.png");
                } else if !stuck {
                    return;
                }
                super::live::event(app, "PRESENCE_UPDATE", serde_json::json!({ "user": { "id": ALICE.to_string() }, "status": "online", "activities": [] }));
                self.step = 5296;
            }
            5296 => {
                let base = ((chrono::Utc::now().timestamp_millis() - 1_420_070_400_000) as u64) << 22;
                self.text = base.to_string();
                let picture = format!("{}/attachments/2011/1/photo-800x450-1.png", crate::api::cdn());
                for (n, name, flags) in [(1u64, "SPOILER_photo.png", 0), (2, "photo.png", 64)] {
                    super::live::event(app, "MESSAGE_CREATE", serde_json::json!({
                        "id": (base + n).to_string(), "channel_id": general.to_string(), "guild_id": self.target.0.to_string(), "type": 0, "content": "",
                        "author": { "id": "101", "username": "alice" }, "timestamp": chrono::Utc::now().to_rfc3339(), "embeds": [], "mentions": [],
                        "attachments": [{ "id": (7000 + n).to_string(), "filename": name, "size": 4096, "url": picture, "proxy_url": picture, "width": 400, "height": 100, "content_type": "image/png", "flags": flags }],
                    }));
                }
                super::live::event(app, "MESSAGE_CREATE", serde_json::json!({
                    "id": (base + 3).to_string(), "channel_id": general.to_string(), "guild_id": self.target.0.to_string(), "type": 0, "content": "",
                    "author": { "id": "101", "username": "alice" }, "timestamp": chrono::Utc::now().to_rfc3339(), "attachments": [], "mentions": [],
                    "embeds": [{ "type": "image", "url": picture, "thumbnail": { "url": picture, "proxy_url": picture, "width": 400, "height": 100 }, "flags": 64 }],
                }));
                app.select_guild(Some(self.target.0));
                app.open(general);
                app.jump_to_present(general);
                self.sub = 0;
                self.next(1200);
            }
            5297 => {
                let base: u64 = self.text.parse().unwrap_or(0);
                let screen = ctx.content_rect();
                let seen = |n: u64| app.rects.get(&format!("obscured:{}:0", base + n)).copied().filter(|r| screen.contains(r.center()));
                let (Some(spoiler), Some(_)) = (seen(1), seen(2)) else {
                    if stuck {
                        self.shot(ctx, "media_cover_missing.png");
                        self.step = 5299;
                        self.sub = 2;
                    }
                    return;
                };
                self.mark("media_covered");
                if app.rects.contains_key(&format!("obscured:{}:100", base + 3)) {
                    self.mark("media_embed_covered");
                }
                self.shot(ctx, "media_covered.png");
                self.click(spoiler.center());
                self.next(600);
            }
            5298 => {
                let base: u64 = self.text.parse().unwrap_or(0);
                if self.sub == 0 {
                    if !app.unveiled.contains(&7001) {
                        if stuck {
                            self.step = 5299;
                            self.sub = 2;
                        }
                        return;
                    }
                    if !app.unveiled.contains(&7002) {
                        self.mark("media_revealed");
                    }
                    app.menu = Some((general, base + 2, ctx.content_rect().center()));
                    self.sub = 1;
                    self.wait = Instant::now() + Duration::from_millis(500);
                    return;
                }
                let Some(r) = menu(ctx, "Mark As Not Sensitive") else {
                    if stuck {
                        app.menu = None;
                        self.step = 5299;
                        self.sub = 2;
                    }
                    return;
                };
                self.click(r.center());
                self.sub = 0;
                self.next(600);
            }
            5299 => {
                let base: u64 = self.text.parse().unwrap_or(0);
                if self.sub == 0 {
                    if app.confirm.as_ref().is_some_and(|c| c.title == "Mark as not sensitive") {
                        self.mark("media_report_asks");
                        self.key(Key::Enter);
                        self.sub = 1;
                        self.wait = Instant::now() + Duration::from_millis(700);
                    } else if stuck {
                        self.sub = 2;
                    }
                    return;
                }
                if self.sub == 1 {
                    if app.confirm.is_some() && !stuck {
                        return;
                    }
                    app.confirm = None;
                    app.unveiled.insert(7002);
                    self.sub = 3;
                    self.step_at = Instant::now();
                    self.wait = Instant::now() + Duration::from_millis(500);
                    return;
                }
                if self.sub == 3 {
                    let screen = ctx.content_rect();
                    match app.rects.get("veil:7002").copied().filter(|r| screen.contains(r.center())) {
                        Some(r) => {
                            self.click(r.center());
                            self.sub = 4;
                            self.step_at = Instant::now();
                            self.wait = Instant::now() + Duration::from_millis(500);
                            return;
                        }
                        None if !stuck => return,
                        None => {}
                    }
                }
                if self.sub == 4 {
                    if !app.unveiled.contains(&7002) {
                        self.mark("media_rehidden");
                    } else if !stuck {
                        return;
                    }
                }
                for n in [1u64, 2, 3] {
                    super::live::event(app, "MESSAGE_DELETE", serde_json::json!({ "id": (base + n).to_string(), "channel_id": general.to_string(), "guild_id": self.target.0.to_string() }));
                    app.unveiled.remove(&(7000 + n));
                }
                app.confirm = None;
                app.menu = None;
                self.text.clear();
                self.sub = 0;
                self.step = 5290;
            }
            _ => {
                app.inbox = None;
                app.sched.picker = None;
                app.full_profile = None;
                app.hub = None;
                app.hub_join = None;
                app.referral = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5300 };
            }
        }
    }
}
