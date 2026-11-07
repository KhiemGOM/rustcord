//! Steps 4650 to 4699: the Shop's leftovers. Hide Filters, the Browse menu, Clear all filters, the
//! colour and theme filters and the sort, a collection from its banner and from a product, a
//! bundle's Previous and Next, the wishlist coming back from the server and its empty page, the
//! Orbs Exclusives hero, and the Featured feed's sort, Shuffle and scroll arrow.
//! `python e2e/from_step.py 4650` runs just this group.

use egui::{Event, ViewportCommand, vec2};

use super::Run;
use crate::ui::App;
use crate::ui::shop::Page;

fn path_of(app: &App, part: &str) -> Option<(String, Option<usize>)> {
    app.keyed.iter().find(|(k, _)| k.starts_with("/shop/search") && k.contains(part)).map(|(k, v)| {
        let total = v.as_ref().and_then(|r| r.as_ref().ok()).map(|v| v["pagination"]["total"].as_u64().unwrap_or(0) as usize);
        (k.clone(), total)
    })
}

impl Run {
    pub(super) fn shopx_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |k: &str| Self::at(ctx, k);
        let forget = |keys: &[&str]| {
            for k in keys {
                ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::new(("e2e", *k))));
            }
        };
        let menu = |label: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))));
        let stuck = self.step_at.elapsed() > std::time::Duration::from_secs(12);
        let tab = app.shop.as_ref().map_or(9, |s| s.tab);
        match self.step {
            4650 => {
                if app.conn != crate::ui::Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings_page = None;
                app.profed = None;
                app.select_guild(None);
                app.open_shop(Page::Shop);
                self.next(900);
            }
            4651 => {
                let Some(r) = at("shop:tab:Browse") else { return };
                self.click(r.center());
                self.next(500);
            }
            4652 => {
                if tab != 1 || at("shop:filters_toggle").is_none() || at("shop:only:Nameplates").is_none() {
                    if stuck {
                        self.fail("Browse showed no sort row or filter bar".into());
                    }
                    return;
                }
                self.mark("shop_filter_bar");
                self.shot(ctx, "shop_filters_open.png");
                forget(&["shop:only:Nameplates"]);
                let Some(r) = at("shop:filters_toggle") else { return };
                self.click(r.center());
                self.next(500);
            }
            4653 => {
                if app.shop.as_ref().is_some_and(|s| s.browse.open) || at("shop:only:Nameplates").is_some() {
                    if stuck {
                        self.fail("Hide Filters left the filter bar up".into());
                    }
                    return;
                }
                self.mark("shop_filters_hide");
                let Some(r) = at("shop:filters_toggle") else { return };
                self.click(r.center());
                self.next(500);
            }
            4654 => {
                if at("shop:only:Nameplates").is_none() {
                    if stuck {
                        self.fail("Show Filters did not bring the filter bar back".into());
                    }
                    return;
                }
                self.mark("shop_filters_show");
                let Some(r) = at("shop:browse_menu") else { return };
                self.click(r.center());
                self.next(400);
            }
            4655 => {
                let Some(r) = menu("Profile Effects") else { return };
                self.shot(ctx, "shop_browse_menu.png");
                self.click(r.center());
                self.next(900);
            }
            4656 => {
                let on = app.shop.as_ref().is_some_and(|s| s.tab == 1 && s.browse.types == [false, false, true, false, false]);
                if !on || path_of(app, "item_types=PROFILE_EFFECT").and_then(|p| p.1) != Some(1) {
                    if stuck {
                        self.fail("the Browse menu did not filter to Profile Effects".into());
                    }
                    return;
                }
                self.mark("shop_category_menu");
                self.shot(ctx, "shop_effects.png");
                if app.shop.as_ref().is_some_and(|s| !s.browse.open) {
                    let Some(r) = at("shop:filters_toggle") else { return };
                    self.click(r.center());
                    self.next(500);
                    return;
                }
                let Some(r) = at("shop:clear") else { return };
                self.click(r.center());
                self.next(500);
            }
            4657 => {
                if app.shop.as_ref().is_some_and(|s| s.browse.any()) {
                    return;
                }
                self.mark("shop_clear_all");
                let Some(r) = at("shop:color:Pink") else { return };
                self.click(r.center());
                self.next(900);
            }
            4658 => {
                if path_of(app, "colors=COLLECTIBLES_COLOR_PINK").and_then(|p| p.1) != Some(1) {
                    if stuck {
                        self.fail("the Pink swatch did not narrow the results to one item".into());
                    }
                    return;
                }
                self.mark("shop_color_filter");
                let Some(r) = at("shop:theme:Anime") else { return };
                self.click(r.center());
                self.next(900);
            }
            4659 => {
                if path_of(app, "themes=COLLECTIBLES_THEME_ANIME").and_then(|p| p.1) != Some(0) {
                    if stuck {
                        self.fail("Pink and Anime together did not come back empty".into());
                    }
                    return;
                }
                self.mark("shop_theme_filter");
                self.shot(ctx, "shop_no_results.png");
                let Some(r) = at("shop:clear") else { return };
                self.click(r.center());
                self.next(500);
            }
            4660 => {
                let Some(r) = at("shop:sort") else { return };
                self.click(r.center());
                self.next(400);
            }
            4661 => {
                let Some(r) = menu("Price: Low to High") else { return };
                self.click(r.center());
                self.next(900);
            }
            4662 => {
                let sorted = app.shop.as_ref().and_then(|s| s.browse.picked).is_some_and(|s| s.label() == "Price: Low to High");
                if !sorted || path_of(app, "sort_type=price&sort_direction=asc").and_then(|p| p.1) != Some(8) {
                    if stuck {
                        self.fail("Price: Low to High did not search by price".into());
                    }
                    return;
                }
                self.mark("shop_sort_price");
                self.shot(ctx, "shop_sorted.png");
                if app.shop.as_ref().is_some_and(|s| !s.browse.open) {
                    let Some(r) = at("shop:filters_toggle") else { return };
                    self.click(r.center());
                    self.next(500);
                    return;
                }
                let Some(r) = at("shop:clear") else { return };
                self.click(r.center());
                self.next(500);
            }
            4663 => {
                if app.shop.as_ref().is_some_and(|s| s.browse.picked.is_some()) {
                    return;
                }
                let Some(r) = at("shop:banner:5001") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(400);
            }
            4664 => {
                let Some(r) = at("shop:banner:5001") else { return };
                self.click(r.center());
                self.next(700);
            }
            4665 => {
                if app.shop.as_ref().and_then(|s| s.collection.as_deref()) != Some("5001") || at("shop:crumb_all").is_none() {
                    if stuck {
                        self.fail("a banner did not open its collection".into());
                    }
                    return;
                }
                self.mark("shop_collection");
                self.shot(ctx, "shop_collection.png");
                let Some(r) = at("shop:crumb_all") else { return };
                forget(&["shop:crumb_all"]);
                self.click(r.center());
                self.next(500);
            }
            4666 => {
                if app.shop.as_ref().is_some_and(|s| s.collection.is_some()) {
                    return;
                }
                self.mark("shop_collection_back");
                forget(&["shop:card:5101"]);
                self.next(400);
                self.step = 4667;
            }
            4667 => {
                let Some(r) = at("shop:card:5101") else { return };
                self.click(r.center());
                self.next(500);
            }
            4668 => {
                let Some(r) = at("shop:go_collection") else { return };
                self.shot(ctx, "shop_detail_collection.png");
                self.click(r.center());
                self.next(700);
            }
            4669 => {
                let s = app.shop.as_ref();
                if s.and_then(|s| s.collection.as_deref()) != Some("5001") || s.is_some_and(|s| s.product.is_some()) {
                    if stuck {
                        self.fail("Go to collection did not open the product's collection".into());
                    }
                    return;
                }
                self.mark("shop_go_collection");
                forget(&["shop:card:5104"]);
                self.next(400);
                self.step = 4670;
            }
            4670 => {
                let Some(r) = at("shop:card:5104") else { return };
                self.click(r.center());
                self.next(600);
            }
            4671 => {
                let Some(r) = at("shop:slide_next") else { return };
                self.shot(ctx, "shop_bundle.png");
                self.click(r.center());
                self.next(400);
            }
            4672 => {
                if app.shop.as_ref().map(|s| s.slide) != Some(1) {
                    return;
                }
                self.mark("shop_bundle_next");
                let Some(r) = at("shop:slide_prev") else { return };
                self.click(r.center());
                self.next(400);
            }
            4673 => {
                if app.shop.as_ref().map(|s| s.slide) != Some(0) {
                    return;
                }
                self.mark("shop_bundle_prev");
                let Some(r) = at("shop:slide:1") else { return };
                self.click(r.center());
                self.next(400);
            }
            4674 => {
                if app.shop.as_ref().map(|s| s.slide) != Some(1) {
                    return;
                }
                self.mark("shop_bundle_chip");
                self.key(egui::Key::Escape);
                self.next(500);
            }
            4675 => {
                if app.shop.as_ref().is_some_and(|s| s.product.is_some()) {
                    return;
                }
                app.open_shop(Page::Shop);
                self.next(1200);
            }
            4676 => {
                if app.shop.as_ref().is_none_or(|s| s.wish_load < 2) {
                    if stuck {
                        self.fail("the wishlist never came back from the server".into());
                    }
                    return;
                }
                if app.shop.as_ref().is_some_and(|s| s.wish.iter().any(|w| w == "5101")) {
                    self.mark("shop_wishlist_persisted");
                }
                forget(&["shop:card:5101", "shop:heart:5101"]);
                let Some(r) = at("shop:tab:Browse") else { return };
                self.click(r.center());
                self.next(700);
            }
            4677 => {
                let Some(r) = at("shop:card:5101") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(300);
            }
            4678 => {
                let Some(r) = at("shop:heart:5101") else { return };
                self.click(r.center());
                self.next(800);
            }
            4679 => {
                if app.shop.as_ref().is_some_and(|s| s.wish.iter().any(|w| w == "5101")) {
                    return;
                }
                self.mark("shop_unwish");
                let Some(r) = at("shop:wishlist") else { return };
                forget(&["wish:explore"]);
                self.click(r.center());
                self.next(900);
            }
            4680 => {
                let Some(r) = at("wish:explore") else {
                    if stuck {
                        self.fail("an empty wishlist showed no Explore the Shop".into());
                    }
                    return;
                };
                self.mark("shop_wishlist_empty");
                self.shot(ctx, "shop_wishlist_empty.png");
                self.click(r.center());
                self.next(700);
            }
            4681 => {
                if app.profed.is_some() || app.shop.is_none() {
                    return;
                }
                self.mark("shop_wishlist_explore");
                let Some(r) = at("shop:tab:Orbs Exclusives") else { return };
                self.click(r.center());
                self.next(900);
            }
            4682 => {
                if tab != 2 || at("shop:quest_home").is_none() {
                    if stuck {
                        self.fail("Orbs Exclusives had no hero".into());
                    }
                    return;
                }
                self.mark("shop_orbs_hero");
                self.shot(ctx, "shop_orbs.png");
                let Some(r) = at("shop:quest_home") else { return };
                self.click(r.center());
                self.next(700);
            }
            4683 => {
                if app.quests.is_none() {
                    return;
                }
                self.mark("shop_quest_home");
                app.open_shop(Page::Shop);
                self.next(900);
            }
            4684 => {
                let Some(r) = at("shop:shuffle") else { return };
                self.shot(ctx, "shop_feed.png");
                self.click(r.center());
                self.next(400);
            }
            4685 => {
                if app.shop.as_ref().map(|s| s.feed_sort) != Some(3) {
                    return;
                }
                self.mark("shop_shuffle");
                let Some(r) = at("shop:feed_sort") else { return };
                self.click(r.center());
                self.next(400);
            }
            4686 => {
                let Some(r) = menu("Recently Added") else { return };
                self.click(r.center());
                self.next(400);
            }
            4687 => {
                if app.shop.as_ref().map(|s| s.feed_sort) != Some(1) {
                    return;
                }
                self.mark("shop_feed_sort");
                let Some(r) = at("shop:scroll_right:hero") else { return };
                forget(&["shop:scroll_right:hero"]);
                self.click(r.center());
                self.next(900);
            }
            4688 => {
                if at("shop:scroll_right:hero").is_some() {
                    return;
                }
                self.mark("shop_scroll_right");
                self.next(300);
            }
            _ => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1600.0, 900.0)));
                app.shop = None;
                self.step = 3110;
            }
        }
    }
}
