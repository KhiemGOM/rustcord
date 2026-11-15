//! Steps 5200 to 5299: scheduled messages from the plus menu to the inbox's Scheduled tab.
//! `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5200` runs just this group.

use std::time::{Duration, Instant};

use egui::{Event, Key};

use super::Run;
use crate::ui::App;
use crate::ui::titlebar::Inbox;

const BOT: u64 = 104;

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
                self.step = 5290;
            }
            _ => {
                app.inbox = None;
                app.sched.picker = None;
                app.full_profile = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 };
            }
        }
    }
}
