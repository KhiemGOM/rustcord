//! Nitro Home and the Shop, steps 3200 and up, from the Home sidebar rows: each page is read
//! from what the mock serves, and the purchase buttons open the checkout window on a billing page.

use super::*;
use crate::ui::buy::{Step, What};
use crate::ui::shop::Page;

impl Run {
    pub(super) fn shop_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |k: &str| Self::at(ctx, k);
        let forget = |keys: &[&str]| {
            for k in keys {
                ctx.data_mut(|d| d.remove::<(egui::Rect, u64)>(egui::Id::new(("e2e", *k))));
            }
        };
        let page = app.shop.as_ref().map(|s| s.page);
        let tab = app.shop.as_ref().map_or(9, |s| s.tab);
        let loaded = |app: &App, k: &str| app.remote.get(k).is_some_and(|v| matches!(v, Some(Ok(_))));
        match self.step {
            3200 => {
                if app.conn != Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings_page = None;
                app.select_guild(None);
                self.next(600);
            }
            3201 => {
                let Some(r) = app.rects.get("home:Nitro Home").copied() else { return };
                self.click(r.center());
                self.next(0);
            }
            3202 => {
                if page != Some(Page::Nitro) || !loaded(app, "nitro_plans") || at("shop:gift").is_none() || at("shop:gift_top").is_none() {
                    return;
                }
                self.mark("nitro_page");
                self.shot(ctx, "nitro_home.png");
                self.next(400);
            }
            3203 => {
                let Some(r) = at("shop:gift_top") else { return };
                self.click(r.center());
                self.next(0);
            }
            3204 => {
                if app.buy.as_ref().is_some_and(|b| b.step == Step::Plans) && at("buy:plan:511651880837840896").is_some() {
                    self.mark("nitro_gift_checkout");
                    self.shot(ctx, "buy_gift_plans.png");
                    self.key(Key::Escape);
                    self.next(0);
                }
            }
            3205 => {
                if app.buy.is_some() {
                    return;
                }
                self.next(0);
                self.step = 3234;
            }
            3234 => {
                let Some(r) = at("shop:nitro:Nitro") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(300);
            }
            3235 => {
                let Some(r) = at("shop:card:Nitro") else { return };
                self.click(r.center());
                self.next(0);
            }
            3236 => {
                if app.buy.as_ref().is_some_and(|b| matches!(b.what, What::Nitro(_))) {
                    self.mark("nitro_subscribe_checkout");
                    self.key(Key::Escape);
                    self.next(0);
                }
            }
            3237 => {
                if app.buy.is_some() {
                    return;
                }
                let Some(r) = app.rects.get("home:Shop").copied() else { return };
                self.click(r.center());
                self.next(0);
                self.step = 3206;
            }
            3206 => {
                if page != Some(Page::Shop) || !loaded(app, "shop_home") || !loaded(app, "shop_cats") || at("shop:collection").is_none() {
                    return;
                }
                self.mark("shop_featured");
                if at("shop:tile:All Ears").is_some() && at("shop:card:5101").is_some() {
                    self.mark("shop_featured_content");
                }
                self.shot(ctx, "shop_featured.png");
                self.next(300);
            }
            3207 => {
                let Some(r) = at("shop:tab:Browse") else { return };
                forget(&["shop:card:5101", "shop:card:5102"]);
                self.click(r.center());
                self.next(300);
            }
            3208 => {
                if tab != 1 || at("shop:card:5101").is_none() {
                    return;
                }
                self.mark("shop_browse");
                self.shot(ctx, "shop_browse.png");
                self.next(0);
            }
            3209 => {
                let Some(r) = at("shop:only:Nameplates") else { return };
                forget(&["shop:card:5101", "shop:card:5102"]);
                self.click(r.center());
                self.next(400);
            }
            3210 => {
                if !app.shop.as_ref().is_some_and(|s| s.browse.types[1]) {
                    return;
                }
                forget(&["shop:card:5101", "shop:card:5102"]);
                self.next(300);
                self.step = 3230;
            }
            3230 => {
                if at("shop:card:5102").is_none() {
                    return;
                }
                if at("shop:card:5101").is_none() {
                    self.mark("shop_filter");
                }
                let Some(r) = at("shop:only:Nameplates") else { return };
                self.click(r.center());
                self.next(400);
                self.step = 3211;
            }
            3211 => {
                let Some(r) = at("shop:card:5101") else { return };
                self.click(r.center());
                self.next(0);
            }
            3212 => {
                if app.shop.as_ref().and_then(|s| s.product.as_deref()) != Some("5101") || at("shop:buy").is_none() {
                    return;
                }
                self.mark("shop_detail");
                self.shot(ctx, "shop_detail.png");
                self.next(300);
            }
            3213 => {
                let Some(r) = at("shop:detail_heart") else { return };
                self.click(r.center());
                self.next(400);
            }
            3214 => {
                if app.shop.as_ref().is_some_and(|s| s.wish.iter().any(|w| w == "5101")) {
                    self.mark("shop_wish");
                    self.next(0);
                }
            }
            3215 => {
                let Some(r) = at("shop:buy") else { return };
                self.click(r.center());
                self.next(400);
            }
            3216 => {
                if app.buy.as_ref().is_some_and(|b| matches!(&b.what, What::Shop(p) if p["sku_id"] == "5101")) {
                    self.mark("shop_buy_opens_checkout");
                    if crate::webwin::is_open() {
                        self.fail("a collectible purchase opened the checkout window".into());
                    }
                    self.key(Key::Escape);
                    self.next(300);
                    self.key(Key::Escape);
                }
            }
            3217 => {
                if app.buy.is_some() || app.shop.as_ref().is_some_and(|s| s.product.is_some()) {
                    return;
                }
                self.mark("shop_detail_escape");
                let Some(r) = at("shop:card:5101") else { return };
                self.click(r.center());
                self.next(500);
            }
            3218 => {
                if app.shop.as_ref().and_then(|s| s.product.as_deref()) != Some("5101") {
                    return;
                }
                self.click(pos2(6.0, 6.0));
                self.next(0);
            }
            3219 => {
                if app.shop.as_ref().is_some_and(|s| s.product.is_some()) {
                    return;
                }
                self.mark("shop_detail_backdrop");
                let Some(r) = at("shop:search") else { return };
                forget(&["shop:card:5101", "shop:card:5102"]);
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("witch dust".into())]);
                self.next(500);
            }
            3220 => {
                if app.shop.as_ref().is_none_or(|s| s.search != "witch dust") {
                    return;
                }
                forget(&["shop:card:5101", "shop:card:5102"]);
                self.next(300);
                self.step = 3231;
            }
            3231 => {
                if at("shop:card:5102").is_some() && at("shop:card:5101").is_none() {
                    self.mark("shop_search");
                    self.shot(ctx, "shop_search.png");
                    self.next(0);
                    self.step = 3221;
                }
            }
            3221 => {
                let Some(r) = at("shop:wishlist") else { return };
                self.click(r.center());
                self.next(400);
            }
            3222 => {
                if app.profed.as_ref().is_none_or(|e| e.tab != crate::ui::profed::Tab::Wishlist) {
                    return;
                }
                forget(&["wish:0"]);
                self.next(600);
                self.step = 3232;
            }
            3232 => {
                if at("wish:0").is_some() {
                    self.mark("shop_wishlist");
                    self.shot(ctx, "shop_wishlist.png");
                    app.profed = None;
                    self.next(300);
                    self.step = 3223;
                }
            }
            3223 => {
                if loaded(app, "shop_orbs") && at("shop:orbs").is_some() {
                    self.mark("shop_orbs");
                }
                let Some(r) = at("shop:tab:Orbs Exclusives") else { return };
                self.click(r.center());
                self.next(400);
            }
            3224 => {
                if tab != 2 {
                    return;
                }
                forget(&["shop:card:5101", "shop:card:5203"]);
                self.next(300);
                self.step = 3233;
            }
            3233 => {
                if at("shop:card:5203").is_some() && at("shop:card:5101").is_none() {
                    self.mark("shop_orbs_tab");
                    self.next(0);
                    self.step = 3225;
                }
            }
            3225 => {
                let Some(r) = app.rects.get("home:Nitro Home").copied() else { return };
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.click(r.center());
                self.next(900);
            }
            3226 => {
                self.shot(ctx, "nitro_home_small.png");
                let Some(r) = app.rects.get("home:Shop").copied() else { return };
                self.click(r.center());
                self.next(900);
            }
            3227 => {
                self.shot(ctx, "shop_featured_small.png");
                self.next(300);
            }
            _ => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                app.shop = None;
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 4200 };
            }
        }
    }
}
