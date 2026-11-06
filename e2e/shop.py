"""Checks for shop_steps in src/ui/e2e/shop.rs: Nitro Home and the Shop are pages of the client,
fetched from the calls Discord's own pages make, and only a Nitro purchase opens the checkout window."""

import json

import web


def gets(reqs, suffix):
    return any(r.get("method") == "GET" and r.get("path", "").endswith(suffix) for r in reqs)


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    paths = [r.get("path", "") for r in reqs]
    has = lambda part: any(part in p for p in paths)
    gift = sum(s.get("page", "").endswith("&gift=true") and s.get("token") == '"mock-token"' for s in web.pages(reqs))
    wished = any(r.get("method") == "POST" and r.get("path", "").endswith("/users/@me/wishlist/items") and web.body(r).get("sku_id") == "5101" for r in reqs)
    return {
        "nitro: the Nitro Home row opened the page, with the plans, the subscription and the gift buttons": both("nitro_page") and has("/subscription-plans") and has("/billing/subscriptions"),
        "nitro: Gift Nitro opened the checkout window on the gift page, signed in": both("nitro_gift_checkout") and gift >= len(runs),
        "shop: the Shop row opened Featured from the catalog calls": both("shop_featured") and has("/collectibles-shop?") and has("/collectibles-categories/v2"),
        "shop: Featured drew the hero's items and the category tiles": both("shop_featured_content"),
        "shop: Browse listed the catalog and Show only filtered it": both("shop_browse") and both("shop_filter"),
        "shop: a card opened the product, and it closed on Escape and on the backdrop": both("shop_detail") and both("shop_detail_escape") and both("shop_detail_backdrop"),
        "shop: the heart added the item to the wishlist on discord and the wishlist tab lists it": both("shop_wish") and wished and both("shop_wishlist"),
        "shop: search finds items by name": both("shop_search"),
        "shop: the Orbs balance shows and Orbs Exclusives lists the Orbs-priced item": both("shop_orbs") and both("shop_orbs_tab") and has("/virtual-currency/balance"),
        "shop: Buy on a collectible does not open the checkout window": both("shop_buy_no_window"),
    }
