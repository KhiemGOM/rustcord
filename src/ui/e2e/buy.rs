//! The purchase dialog, steps 4100 and up, against the mock's strict billing routes. After every
//! action the mock is asked how many charges reached it (`GET /mock/buy`), so "exactly one per
//! click" and "none on Cancel, Escape, a refusal or a read-only run" are counted by the server,
//! not by the client's own marks.

use serde_json::{Value, json};

use super::*;
use crate::net::{Cmd, Tag};
use crate::ui::buy::{NITRO_MONTH, Step, What};
use crate::ui::shop::Page;

const KEYS: [&str; 22] = [
    "buy:purchase", "buy:total", "buy:done", "buy:failed", "buy:sending", "buy:confirming", "buy:blocker", "buy:add_card", "buy:orbs", "buy:gift_toggle", "buy:cancel", "buy:back", "buy:billing", "buy:close",
    "buy:waiver", "buy:has_sub", "buy:plan:511651880837840896", "buy:friend:101", "buy:friend:102", "buy:qty1", "buy:source:9503", "buy:style:5",
];

fn mock_set(app: &App, body: Value) {
    app.rest("POST", "/mock/buy".into(), Some(body), Tag::Quiet);
}

fn mock_read(app: &mut App) -> Option<(u64, u64, u64)> {
    match app.remote.get("mock_buy") {
        None => {
            app.remote.insert("mock_buy", None);
            app.rest("GET", "/mock/buy".into(), None, Tag::Remote("mock_buy"));
            None
        }
        Some(Some(Ok(v))) => {
            let r = (v["attempts"].as_u64().unwrap_or(99), v["accepted"].as_u64().unwrap_or(99), v["orbs"].as_u64().unwrap_or(0));
            app.remote.remove("mock_buy");
            Some(r)
        }
        _ => None,
    }
}

fn product(app: &App, sku: &str) -> Option<Value> {
    let v = match app.remote.get("shop_cats") {
        Some(Some(Ok(v))) => v,
        _ => return None,
    };
    v["categories"].as_array()?.iter().flat_map(|c| c["products"].as_array().cloned().unwrap_or_default()).find(|p| p["sku_id"] == sku)
}

impl Run {
    fn wipe(ctx: &egui::Context) {
        for k in KEYS {
            ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("e2e", k))));
        }
    }

    /// Click a marked control once the dialog has settled, then forget the mark.
    fn tap(&mut self, ctx: &egui::Context, key: &str) -> bool {
        let Some(r) = Self::at(ctx, key) else { return false };
        ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("e2e", key))));
        self.click(r.center());
        true
    }

    /// Like `tap`, but a control under the dialog's pinned footer is scrolled into view first.
    fn reach(&mut self, ctx: &egui::Context, key: &str) -> bool {
        let Some(r) = Self::at(ctx, key) else { return false };
        let screen = ctx.content_rect();
        if r.bottom() > screen.bottom() - 32.0 - 176.0 - 8.0 {
            self.queue.push_back(vec![Event::PointerMoved(screen.center())]);
            self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -150.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
            self.wait = Instant::now() + Duration::from_millis(300);
            return false;
        }
        self.tap(ctx, key)
    }

    /// The server's own count of charges that reached it, against what this step expects.
    fn charges(&mut self, app: &mut App, attempts: u64, accepted: u64, mark: &str) -> bool {
        let Some((a, b, _)) = mock_read(app) else { return false };
        if (a, b) == (attempts, accepted) {
            self.mark(mark);
        } else {
            self.fail(format!("{mark}: the mock saw {a} charge attempts and {b} accepted, wanted {attempts} and {accepted}"));
        }
        true
    }

    fn ready(app: &App, ctx: &egui::Context) -> bool {
        app.buy.as_ref().is_some_and(|b| b.step == Step::Review && b.blocker().is_none()) && Self::at(ctx, "buy:purchase").is_some() && (Self::at(ctx, "buy:total").is_some() || app.buy.as_ref().is_some_and(|b| b.paying_orbs()))
    }

    pub(super) fn buy_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let shown = |s: &str| Self::at(ctx, s).is_some();
        match self.step {
            4100 => {
                if app.conn != Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                mock_set(app, json!({ "reset": true }));
                app.settings_page = None;
                app.select_guild(None);
                app.open_shop(Page::Shop);
                Self::wipe(ctx);
                self.next(900);
            }
            4101 => {
                let Some(r) = Self::at(ctx, "shop:card:5101") else { return };
                if !self.charges(app, 0, 0, "buy_baseline") {
                    return;
                }
                self.click(r.center());
                self.next(300);
            }
            4102 => {
                if !shown("shop:buy") {
                    return;
                }
                self.tap(ctx, "shop:buy");
                self.next(700);
            }
            4103 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.mark("buy_review");
                self.shot(ctx, "buy_review.png");
                self.key(Key::Enter);
                self.next(600);
            }
            4104 => {
                if !self.charges(app, 0, 0, "buy_enter_inert") {
                    return;
                }
                if app.buy.as_ref().is_none_or(|b| b.step != Step::Review) {
                    self.fail("Enter did something in the purchase dialog".into());
                    return;
                }
                self.tap(ctx, "buy:cancel");
                self.next(500);
            }
            4105 => {
                if app.buy.is_some() {
                    return;
                }
                self.mark("buy_cancel");
                Self::wipe(ctx);
                let Some(r) = Self::at(ctx, "shop:buy") else { return };
                self.click(r.center());
                self.next(800);
            }
            4106 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.key(Key::Escape);
                self.next(500);
            }
            4107 => {
                if app.buy.is_some() {
                    return;
                }
                if !self.charges(app, 0, 0, "buy_no_charge_on_cancel_or_escape") {
                    return;
                }
                self.mark("buy_escape");
                Self::wipe(ctx);
                self.click(Self::at(ctx, "shop:buy").map_or(pos2(0.0, 0.0), |r| r.center()));
                self.next(800);
            }
            4108 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                let Some(r) = Self::at(ctx, "buy:purchase") else { return };
                self.click(r.center());
                self.click(r.center());
                self.next(600);
            }
            4109 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 1, 1, "buy_one_charge_per_double_click") {
                    return;
                }
                self.mark("buy_done");
                self.shot(ctx, "buy_done.png");
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4110 => {
                if app.buy.is_some() {
                    return;
                }
                app.shop.as_mut().map(|s| s.product = None);
                Self::wipe(ctx);
                mock_set(app, json!({ "drift": 1000 }));
                if let Some(p) = product(app, "5102") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4111 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4112 => {
                if !shown("buy:failed") {
                    return;
                }
                self.shot(ctx, "buy_price_changed.png");
                if !self.charges(app, 2, 1, "buy_price_change_refused_without_charge") {
                    return;
                }
                if !app.buy.as_ref().is_some_and(|b| matches!(&b.step, Step::Failed { msg, unsure: false } if msg.starts_with("The amount due changed"))) {
                    self.fail("a changed price didn't say so".into());
                    return;
                }
                self.mark("buy_price_message");
                self.tap(ctx, "buy:back");
                Self::wipe(ctx);
                self.next(900);
            }
            4113 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4114 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 3, 2, "buy_new_click_after_refusal") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4115 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                mock_set(app, json!({ "no_sources": true }));
                self.next(400);
            }
            4116 => {
                if let Some(p) = product(app, "5201") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4117 => {
                if !shown("buy:add_card") || !shown("buy:purchase") {
                    return;
                }
                self.mark("buy_no_source_offers_add_card");
                self.shot(ctx, "buy_no_source.png");
                self.tap(ctx, "buy:purchase");
                self.next(500);
            }
            4118 => {
                if !self.charges(app, 3, 2, "buy_no_source_no_charge") {
                    return;
                }
                self.key(Key::Escape);
                mock_set(app, json!({ "no_sources": false, "orbs": 100 }));
                self.next(600);
            }
            4119 => {
                Self::wipe(ctx);
                if let Some(p) = product(app, "5203") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4120 => {
                if !shown("buy:orbs") {
                    return;
                }
                self.tap(ctx, "buy:orbs");
                self.next(500);
            }
            4121 => {
                if app.buy.as_ref().and_then(|b| b.blocker()).as_deref() != Some("Not enough Orbs.") {
                    return;
                }
                self.mark("buy_orbs_short");
                self.shot(ctx, "buy_orbs_short.png");
                self.tap(ctx, "buy:purchase");
                self.next(500);
            }
            4122 => {
                if !self.charges(app, 3, 2, "buy_orbs_short_no_charge") {
                    return;
                }
                self.key(Key::Escape);
                mock_set(app, json!({ "orbs": 6000 }));
                self.next(600);
            }
            4123 => {
                Self::wipe(ctx);
                if let Some(p) = product(app, "5203") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4124 => {
                if !shown("buy:orbs") {
                    return;
                }
                self.tap(ctx, "buy:orbs");
                self.next(500);
            }
            4125 => {
                if app.buy.as_ref().is_none_or(|b| !b.paying_orbs() || b.blocker().is_some()) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4126 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 4, 3, "buy_orbs_done") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4127 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                if let Some(p) = product(app, "5201") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4128 => {
                if !Self::ready(app, ctx) || !shown("buy:gift_toggle") {
                    return;
                }
                self.tap(ctx, "buy:gift_toggle");
                self.next(700);
            }
            4129 => {
                if !self.reach(ctx, "buy:friend:101") {
                    return;
                }
                self.next(300);
            }
            4130 => {
                if !self.reach(ctx, "buy:style:5") {
                    return;
                }
                if let Some(b) = app.buy.as_mut() {
                    b.message = "Happy birthday".into();
                }
                self.next(900);
            }
            4131 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.shot(ctx, "buy_gift_item.png");
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4132 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 5, 4, "buy_gift_item_done") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4133 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                app.open_buy(What::NitroGift);
                self.next(900);
            }
            4134 => {
                if !shown("buy:plan:511651880837840896") {
                    return;
                }
                self.shot(ctx, "buy_gift_plans2.png");
                self.tap(ctx, "buy:plan:511651880837840896");
                self.next(800);
            }
            4135 => {
                if !self.reach(ctx, "buy:friend:102") {
                    return;
                }
                self.next(900);
            }
            4136 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4137 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 6, 5, "buy_nitro_gift_done") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4138 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                mock_set(app, json!({ "no_sub": true }));
                self.next(500);
            }
            4139 => {
                app.open_buy(What::Nitro(NITRO_MONTH.into()));
                self.next(1200);
            }
            4140 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.shot(ctx, "buy_nitro_subscribe.png");
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4141 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 7, 6, "buy_nitro_subscribe_done") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4142 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                app.open_buy(What::Boost(1002));
                self.next(1200);
            }
            4143 => {
                if !Self::ready(app, ctx) || !shown("buy:qty1") {
                    return;
                }
                self.tap(ctx, "buy:qty1");
                self.next(1000);
            }
            4144 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.shot(ctx, "buy_boost.png");
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4145 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 8, 7, "buy_boost_done") {
                    return;
                }
                if app.buy.as_ref().is_some_and(|b| matches!(&b.step, Step::Done(t) if t.contains("applied"))) {
                    self.mark("buy_boost_applied");
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4146 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                app.shop = None;
                app.select_guild(Some(1002));
                self.next(900);
            }
            4147 => {
                let Some(r) = Self::at(ctx, "home_row:Server Shop") else { return };
                self.click(r.center());
                self.next(900);
            }
            4148 => {
                let Some(r) = Self::at(ctx, "server_shop:card:6101") else { return };
                self.mark("server_shop_page");
                self.shot(ctx, "server_shop.png");
                if Self::at(ctx, "server_shop:card:6103").is_some() {
                    self.fail("an unpublished listing was shown".into());
                    return;
                }
                self.click(r.center());
                self.next(900);
            }
            4149 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4150 => {
                if !shown("buy:done") {
                    return;
                }
                if !self.charges(app, 9, 8, "buy_server_shop_done") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(500);
            }
            4151 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                app.server_shop = None;
                app.select_guild(None);
                app.open_shop(Page::Shop);
                mock_set(app, json!({ "confirm": true }));
                self.next(900);
            }
            4152 => {
                if let Some(p) = product(app, "5202") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4153 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4154 => {
                if !shown("buy:confirming") || !crate::webwin::shown().is_some_and(|u| u.ends_with("/billing/mock-confirm/7001")) {
                    return;
                }
                self.mark("buy_confirmation_window");
                self.shot(ctx, "buy_confirming.png");
                self.next(0);
            }
            4155 => {
                if !shown("buy:done") {
                    if self.step_at.elapsed().as_secs() > 25 {
                        self.fail("a confirmed payment never finished".into());
                    }
                    return;
                }
                crate::webwin::press_escape();
                if !self.charges(app, 10, 9, "buy_confirmed_payment_one_charge") {
                    return;
                }
                self.tap(ctx, "buy:done");
                self.next(600);
            }
            4156 => {
                if app.buy.is_some() || crate::webwin::is_open() {
                    return;
                }
                Self::wipe(ctx);
                mock_set(app, json!({ "boom": true }));
                self.next(400);
            }
            4157 => {
                if let Some(p) = product(app, "5103") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4158 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(1500);
            }
            4159 => {
                if !shown("buy:failed") {
                    return;
                }
                self.shot(ctx, "buy_unknown.png");
                if !self.charges(app, 11, 9, "buy_unknown_result_sent_once") {
                    return;
                }
                if shown("buy:billing") && !shown("buy:back") && app.buy.as_ref().is_some_and(|b| matches!(b.step, Step::Failed { unsure: true, .. })) {
                    self.mark("buy_unknown_says_check_billing");
                }
                self.tap(ctx, "buy:close");
                self.next(500);
            }
            4160 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                mock_set(app, json!({ "decline": true }));
                self.next(400);
            }
            4161 => {
                if let Some(p) = product(app, "5104") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4162 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                self.tap(ctx, "buy:purchase");
                self.next(0);
            }
            4163 => {
                if !shown("buy:failed") {
                    return;
                }
                if !self.charges(app, 12, 9, "buy_declined_no_charge") {
                    return;
                }
                self.tap(ctx, "buy:back");
                mock_set(app, json!({ "decline": false }));
                Self::wipe(ctx);
                self.next(900);
            }
            4164 => {
                if !Self::ready(app, ctx) {
                    return;
                }
                crate::net::FORCE_READONLY.store(true, std::sync::atomic::Ordering::Relaxed);
                self.tap(ctx, "buy:purchase");
                self.next(500);
            }
            4165 => {
                let barred = app.buy.as_ref().is_some_and(|b| matches!(&b.step, Step::Failed { msg, unsure: false } if msg.contains("read-only")));
                if !barred {
                    return;
                }
                let probe = Cmd::Charge { method: "POST", path: "/store/skus/5104/purchase".into(), body: json!({ "probe": true }), tag: Tag::Buy("probe", 0) };
                app.net.send(probe);
                self.next(700);
            }
            4166 => {
                let held = self.charges(app, 12, 9, "buy_read_only_refuses_before_sending");
                if !held {
                    return;
                }
                crate::net::FORCE_READONLY.store(false, std::sync::atomic::Ordering::Relaxed);
                self.tap(ctx, "buy:close");
                self.next(500);
            }
            4167 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                mock_set(app, json!({ "eea": true }));
                self.next(500);
            }
            4168 => {
                if let Some(p) = product(app, "5104") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4169 => {
                if !self.reach(ctx, "buy:source:9503") {
                    return;
                }
                self.next(900);
            }
            4170 => {
                if !shown("buy:waiver") || !shown("buy:total") {
                    return;
                }
                if app.buy.as_ref().and_then(|b| b.blocker()).is_none() {
                    self.fail("the withdrawal waiver wasn't required".into());
                    return;
                }
                self.mark("buy_eea_waiver_required");
                self.shot(ctx, "buy_eea.png");
                self.key(Key::Escape);
                self.next(500);
            }
            4171 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(900);
            }
            4172 => {
                if let Some(p) = product(app, "5201") {
                    app.open_buy(What::Shop(p));
                }
                self.next(900);
            }
            4173 => {
                if !Self::ready(app, ctx) || !shown("buy:gift_toggle") {
                    return;
                }
                self.tap(ctx, "buy:gift_toggle");
                self.next(900);
            }
            4174 => {
                let Some(r) = Self::at(ctx, "buy:purchase") else { return };
                self.shot(ctx, "buy_small_gift.png");
                let screen = ctx.content_rect();
                if r.bottom() > screen.bottom() || r.top() < screen.top() {
                    self.fail(format!("at 940x500 the Purchase button is off the window: {r:?} in {screen:?}"));
                    return;
                }
                self.mark("buy_small_window_reaches_purchase");
                self.key(Key::Escape);
                self.next(500);
            }
            4175 => {
                if app.buy.is_some() {
                    return;
                }
                Self::wipe(ctx);
                app.open_buy(What::NitroGift);
                self.next(900);
            }
            4176 => {
                if !shown("buy:plan:511651880837840896") {
                    return;
                }
                self.shot(ctx, "buy_small_plans.png");
                self.key(Key::Escape);
                self.next(500);
            }
            _ => {
                if app.buy.is_some() {
                    return;
                }
                mock_set(app, json!({ "reset": true }));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                app.shop = None;
                self.step = 1600;
            }
        }
    }
}
