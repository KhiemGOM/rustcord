//! Steps 5120 to 5199: controls that were drawn but never clicked: the discovery category dropdown,
//! the server profile's top-games tiles, the AutoMod regex check, the Shop pager and Subscribe to
//! Nitro Basic. `python e2e/from_step.py 5120` runs just these.

use std::time::Duration;

use egui::{Event, ViewportCommand, pos2, vec2};
use serde_json::json;

use super::Run;
use crate::ui::App;
use crate::ui::server::Page as ServerPage;

const OWNED: u64 = 1001;
const TOP_GAME: u64 = 7100;

fn wheel(dy: f32) -> Event {
    Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, dy), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE }
}

fn menu(ctx: &egui::Context, label: &str) -> Option<egui::Rect> {
    Run::at(ctx, &format!("menu_row:{label}"))
}

fn regex_errors(app: &App) -> bool {
    app.server.as_ref().is_some_and(|s| s.errors.iter().any(|e| e.0.starts_with("trigger_metadata.regex_patterns")))
}

impl Run {
    fn in_view(&mut self, ctx: &egui::Context, r: egui::Rect) -> bool {
        let screen = ctx.content_rect();
        if r.top() > screen.top() + 120.0 && r.bottom() < screen.bottom() - 8.0 {
            return true;
        }
        if self.step_at.elapsed() > Duration::from_secs(12) {
            self.shot(ctx, "p5b_not_in_view.png");
            self.fail(format!("step {} could not scroll {r:?} into view of {screen:?}", self.step));
            return false;
        }
        self.queue.push_back(vec![Event::PointerMoved(pos2(screen.center().x, screen.center().y))]);
        self.queue.push_back(vec![wheel(if r.top() <= screen.top() + 120.0 { 240.0 } else { -240.0 })]);
        self.wait = std::time::Instant::now() + Duration::from_millis(500);
        false
    }

    pub(super) fn p5b_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5120 => {
                if let Some(g) = app.store.guilds.get_mut(&OWNED)
                    && !g.features.iter().any(|f| f == "DISCOVERABLE")
                {
                    g.features.push("DISCOVERABLE".into());
                }
                app.server = None;
                app.open_server_settings(OWNED);
                if let Some(s) = &mut app.server {
                    s.page = ServerPage::Discovery;
                }
                self.next(900);
            }
            5121 => {
                let Some(r) = Self::at(ctx, "dropdown:Select a category") else {
                    if stuck {
                        self.fail("the discovery page had no category dropdown".into());
                    }
                    return;
                };
                self.click(r.center());
                self.next(500);
            }
            5122 => {
                let Some(r) = menu(ctx, "Music") else {
                    if stuck {
                        self.fail("the category dropdown listed no Music".into());
                    }
                    return;
                };
                self.shot(ctx, "discovery_category_open.png");
                self.click(r.center());
                self.next(500);
            }
            5123 => {
                if Self::at(ctx, "dropdown:Music").is_none() {
                    if stuck {
                        self.fail("picking Music left the dropdown showing something else".into());
                    }
                    return;
                }
                let Some(save) = Self::at(ctx, "discovery:save") else { return };
                self.mark("discovery_category_picked");
                self.shot(ctx, "discovery_category_picked.png");
                self.click(save.center());
                self.next(900);
            }
            5124 => {
                let saved = app.server.as_ref().and_then(|s| s.data.get("discovery_meta")).is_some_and(|m| m["primary_category_id"] == 2);
                if !saved {
                    if stuck {
                        self.fail("saving the picked category did not come back as Music".into());
                    }
                    return;
                }
                self.mark("discovery_category_saved");
                if let Some(g) = app.store.guilds.get_mut(&OWNED) {
                    g.features.retain(|f| f != "DISCOVERABLE");
                }
                app.server = None;
                self.next(300);
                self.step = 5130;
            }
            5130 => {
                app.open_server_settings(OWNED);
                if let Some(s) = &mut app.server {
                    s.page = ServerPage::Profile;
                }
                self.next(1200);
            }
            5131 => {
                let Some(r) = Self::at(ctx, &format!("game:tile:{TOP_GAME}")) else {
                    if stuck {
                        self.fail("the server profile's Games had no tile for the server's top game".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.shot(ctx, "server_games_before.png");
                self.click(r.center());
                self.next(500);
            }
            5132 => {
                let Some(save) = Self::at(ctx, "bar:Save Changes") else {
                    if stuck {
                        self.shot(ctx, "server_games_stuck.png");
                        self.fail("clicking a selected game's tile did not make the profile saveable".into());
                    }
                    return;
                };
                self.shot(ctx, "server_games_off.png");
                self.click(save.center());
                self.next(900);
            }
            5133 => {
                let games = |app: &App| app.server.as_ref().and_then(|s| s.data.get("profile")).map(|p| p["game_application_ids"].clone());
                if games(app).is_none_or(|g| g != json!([])) {
                    if stuck {
                        self.fail(format!("saving the game off came back as {:?}", games(app)));
                    }
                    return;
                }
                let Some(r) = Self::at(ctx, &format!("game:tile:{TOP_GAME}")) else {
                    if stuck {
                        self.fail("a game taken off the profile left no tile among the top games".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.mark("server_games_tile_off");
                self.click(r.center());
                self.next(500);
            }
            5134 => {
                let Some(save) = Self::at(ctx, "bar:Save Changes") else {
                    if stuck {
                        self.fail("clicking a top game's tile did not make the profile saveable".into());
                    }
                    return;
                };
                self.shot(ctx, "server_games_on.png");
                self.click(save.center());
                self.next(900);
            }
            5135 => {
                if !app.server.as_ref().and_then(|s| s.data.get("profile")).is_some_and(|p| p["game_application_ids"] == json!(["7100"])) {
                    if stuck {
                        self.fail("saving the top game back on did not come back".into());
                    }
                    return;
                }
                self.mark("server_games_tile");
                app.server = None;
                self.next(300);
                self.step = 5140;
            }
            5140 => {
                app.open_server_settings(OWNED);
                if let Some(s) = &mut app.server {
                    s.page = ServerPage::AutoMod;
                }
                self.next(1000);
            }
            5141 => {
                let Some(r) = Self::at(ctx, "button:Set Up").or_else(|| Self::at(ctx, "button:Create Rule")) else {
                    if stuck {
                        self.fail("the AutoMod page had no Set Up or Create Rule button".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.click(r.center());
                self.next(600);
            }
            5142 => {
                let Some(r) = Self::at(ctx, "area:^spam.*$") else {
                    if stuck {
                        self.fail("the new AutoMod rule had no regex patterns field".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("(".into())]);
                self.next(1600);
            }
            5143 => {
                if !regex_errors(app) {
                    if stuck {
                        self.fail("an unclosed group in the regex field drew no error".into());
                    }
                    return;
                }
                self.mark("automod_regex_invalid");
                self.shot(ctx, "automod_regex_invalid.png");
                self.queue.push_back(vec![Event::Text(")".into())]);
                self.next(1600);
            }
            5144 => {
                if regex_errors(app) {
                    if stuck {
                        self.fail("closing the group left the regex error up".into());
                    }
                    return;
                }
                self.mark("automod_regex_valid");
                self.shot(ctx, "automod_regex_valid.png");
                self.next(500);
            }
            5145 => {
                app.server = None;
                self.next(300);
                self.step = 5150;
            }
            5150 => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings_page = None;
                app.select_guild(None);
                app.open_shop(crate::ui::shop::Page::Shop);
                self.next(900);
            }
            5151 => {
                let Some(r) = Self::at(ctx, "shop:tab:Browse") else { return };
                self.click(r.center());
                self.next(700);
            }
            5152 => {
                let Some(r) = Self::at(ctx, "shop:only:Profile Frames") else {
                    if stuck {
                        self.fail("Browse had no Profile Frames filter".into());
                    }
                    return;
                };
                self.click(r.center());
                self.next(900);
            }
            5153 => {
                let Some(r) = Self::at(ctx, "shop:page:next") else {
                    if stuck {
                        self.shot(ctx, "shop_pager_missing.png");
                        self.fail("thirty profile frames drew no pager under the grid".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.shot(ctx, "shop_pager_first.png");
                self.click(r.center());
                self.next(900);
            }
            5154 => {
                if app.shop.as_ref().is_none_or(|s| s.browse.page != 1) {
                    if stuck {
                        self.fail("Next on the shop pager did not go to page 2".into());
                    }
                    return;
                }
                self.mark("shop_pager_next");
                self.shot(ctx, "shop_pager_second.png");
                let Some(r) = Self::at(ctx, "shop:page:1") else { return };
                self.click(r.center());
                self.next(900);
            }
            5155 => {
                if app.shop.as_ref().is_none_or(|s| s.browse.page != 0) {
                    if stuck {
                        self.fail("the pager's page 1 did not go back to the first page".into());
                    }
                    return;
                }
                self.mark("shop_pager_number");
                app.shop = None;
                self.next(300);
                self.step = 5160;
            }
            5160 => {
                app.store.me_raw["premium_type"] = json!(0);
                app.settings_page = Some(crate::ui::settings::Page::Nitro);
                self.next(1800);
            }
            5161 => {
                let Some(r) = Self::at(ctx, "billing:Subscribe to Nitro Basic") else {
                    if stuck {
                        self.fail("the Nitro page had no Subscribe to Nitro Basic button".into());
                    }
                    return;
                };
                if !self.in_view(ctx, r) {
                    return;
                }
                self.shot(ctx, "nitro_basic_button.png");
                self.click(r.center());
                self.next(1200);
            }
            5162 => {
                if app.buy.is_none() {
                    if stuck {
                        self.fail("Subscribe to Nitro Basic opened no checkout".into());
                    }
                    return;
                }
                self.mark("nitro_basic_click");
                self.next(1500);
            }
            5163 => {
                self.shot(ctx, "nitro_basic_checkout.png");
                self.next(600);
            }
            5164 => {
                app.buy = None;
                app.store.me_raw["premium_type"] = json!(2);
                app.settings_page = None;
                self.next(400);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
        }
    }
}
