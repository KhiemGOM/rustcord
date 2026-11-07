"""Checks for shopx_steps in src/ui/e2e/shopx.rs: the Shop's filters and sort go out as the
/shop/search query Discord's client builds, a collection opens from its banner and from a
product, a bundle steps through its items, and the wishlist is read from and written to the
account's own wishlist."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    searches = [r.get("path", "") for r in reqs if r.get("method") == "GET" and "/shop/search" in r.get("path", "")]
    has = lambda part: any(part in p for p in searches)
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    return {
        "shopx: Browse shows the sort row and the filter bar, and Hide Filters and Show Filters toggle it": both("shop_filter_bar") and both("shop_filters_hide") and both("shop_filters_show"),
        "shopx: the Browse menu's Profile Effects searches by item type": both("shop_category_menu") and has("item_types=PROFILE_EFFECT"),
        "shopx: Clear all filters empties every filter": both("shop_clear_all"),
        "shopx: the colour and theme filters go out as colors= and themes=": both("shop_color_filter") and both("shop_theme_filter") and has("colors=COLLECTIBLES_COLOR_PINK") and has("themes=COLLECTIBLES_THEME_ANIME"),
        "shopx: Price: Low to High searches by price ascending": both("shop_sort_price") and has("sort_type=price&sort_direction=asc"),
        "shopx: a category banner opens its collection and Shop All comes back": both("shop_collection") and both("shop_collection_back"),
        "shopx: Go to collection on a product opens that product's collection": both("shop_go_collection"),
        "shopx: a bundle steps through its items with Next, Previous and a chip": both("shop_bundle_next") and both("shop_bundle_prev") and both("shop_bundle_chip"),
        "shopx: the wishlist is read from the account's own wishlist on opening the Shop": both("shop_wishlist_persisted") and bool(sent("GET", "/wishlists/7700")),
        "shopx: unhearting removes the item on discord": both("shop_unwish") and bool(sent("DELETE", "/users/@me/wishlists/7700/items/5101")),
        "shopx: an empty wishlist shows Explore the Shop and it goes back to the Shop": both("shop_wishlist_empty") and both("shop_wishlist_explore"),
        "shopx: Orbs Exclusives has its hero and Go to Quest Home opens Quests": both("shop_orbs_hero") and both("shop_quest_home"),
        "shopx: the feed's Shuffle and sort change the order": both("shop_shuffle") and both("shop_feed_sort"),
        "shopx: the hero strip's scroll arrow scrolls it": both("shop_scroll_right"),
    }
