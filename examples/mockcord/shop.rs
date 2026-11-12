//! The Shop and the checkout pages of the mock: a small collectibles catalog in Discord's shape,
//! the wishlist, and the standalone billing pages the checkout window loads. Anything outside
//! those pages answers 404, so a window that wanders into the rest of the app fails visibly.

use serde_json::{Value, json};
use std::sync::Mutex;

const CONFIRM: &str = r#"<title>Confirm payment</title><style>body{background:#1e1f22;color:#f2f3f5;font:16px sans-serif;padding:32px}</style><h2>Confirm your payment</h2><p>Mock bank page.</p><script>setTimeout(()=>fetch("/api/v9/mock/pay/__ID__/complete",{method:"POST"}),1500)</script>"#;

static WISH: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn reset() {
    WISH.lock().unwrap().clear();
}

const POPULAR: [&str; 8] = ["5104", "5101", "5201", "5103", "5102", "5202", "5203", "5204"];

fn tags(sku: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match sku {
        "5101" | "5102" => (&["PURPLE"], &["FANTASY", "DARK_MOODY"]),
        "5103" => (&["GREEN"], &["ANIME"]),
        "5104" => (&["PURPLE", "BLUE"], &["DARK_MOODY", "FANTASY"]),
        "5201" => (&["ORANGE"], &["ANIMALS_PETS", "CUTE_COZY"]),
        "5202" => (&["PINK"], &["CUTE_COZY"]),
        "5203" => (&["BLACK", "WHITE"], &["ANIMALS_PETS"]),
        "5204" => (&["RED"], &["GAMING"]),
        _ => (&[], &[]),
    }
}

fn wishlist() -> Value {
    let list: Vec<Value> = WISH.lock().unwrap().iter().map(|s| json!({ "sku": { "id": s }, "sku_id": s })).collect();
    json!({ "id": "7700", "user_id": "100", "visibility": 1, "wishlist_items": list })
}

fn decode(s: &str) -> String {
    let b = s.replace('+', " ");
    let raw = b.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match b.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(h) if raw[i] == b'%' => {
                out.push(h);
                i += 3;
            }
            _ => {
                out.push(raw[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn list_price(p: &Value) -> i64 {
    p["prices"]["0"]["country_prices"]["prices"][0]["amount"].as_i64().unwrap_or(0)
}

fn bad(field: &str, message: &str) -> Option<(u16, Value)> {
    Some((400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { field: { "_errors": [{ "code": "BASE_TYPE_CHOICES", "message": message }] } } })))
}

fn search(query: &str) -> Option<(u16, Value)> {
    let all = |k: &str| -> Vec<String> { query.split('&').filter_map(|kv| kv.split_once('=')).filter(|(a, _)| *a == k).map(|(_, v)| decode(v)).collect() };
    let one = |k: &str| all(k).into_iter().next();
    let types = all("item_types");
    for t in &types {
        if !["AVATAR_DECORATION", "PROFILE_EFFECT", "NAMEPLATE", "PROFILE_FRAME", "BUNDLE"].contains(&t.as_str()) {
            return bad("item_types", "Value must be one of the item types.");
        }
    }
    let colors = all("colors");
    if colors.iter().any(|c| !c.starts_with("COLLECTIBLES_COLOR_")) {
        return bad("colors", "Value must be one of the colors.");
    }
    let themes = all("themes");
    if themes.iter().any(|c| !c.starts_with("COLLECTIBLES_THEME_")) {
        return bad("themes", "Value must be one of the themes.");
    }
    let kind = one("sort_type").unwrap_or_else(|| "recency".into());
    if !["price", "alphabetical", "recency", "popularity", "relevance"].contains(&kind.as_str()) {
        return bad("sort_type", "Value must be one of price, alphabetical, recency, popularity, relevance.");
    }
    let dir = one("sort_direction").unwrap_or_else(|| "desc".into());
    if !["asc", "desc"].contains(&dir.as_str()) {
        return bad("sort_direction", "Value must be one of asc, desc.");
    }
    let num = |k: &str, d: usize| one(k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(d);
    let (offset, limit) = (num("offset", 0), num("limit", 15));
    if limit == 0 || limit > 100 {
        return bad("limit", "Value must be between 1 and 100.");
    }
    let text = one("search").unwrap_or_default().to_lowercase();
    let orbs = one("orbs_eligible").as_deref() == Some("true");
    let collabs = one("is_first_party").as_deref() == Some("false");
    let mut hits: Vec<Value> = categories()
        .iter()
        .flat_map(|c| c["products"].as_array().cloned().unwrap_or_default())
        .filter(|p| {
            let sku = p["sku_id"].as_str().unwrap_or("");
            let (c, t) = tags(sku);
            let ty = match p["type"].as_u64() {
                Some(0) => "AVATAR_DECORATION",
                Some(1) => "PROFILE_EFFECT",
                Some(2) => "NAMEPLATE",
                Some(3) => "PROFILE_FRAME",
                Some(1000) => "BUNDLE",
                _ => "OTHER",
            };
            (types.is_empty() || types.iter().any(|x| x == ty))
                && (colors.is_empty() || colors.iter().any(|x| c.iter().any(|y| x.ends_with(y))))
                && (themes.is_empty() || themes.iter().any(|x| t.iter().any(|y| x.ends_with(y))))
                && (!orbs || p["prices"]["0"]["country_prices"]["prices"].as_array().is_some_and(|l| l.iter().any(|x| x["currency"] == "discord_orb")))
                && (!collabs || p["is_first_party"] == false)
                && (text.is_empty() || p["name"].as_str().unwrap_or("").to_lowercase().contains(&text))
        })
        .collect();
    let rank = |p: &Value| POPULAR.iter().position(|s| p["sku_id"] == *s).unwrap_or(99);
    match kind.as_str() {
        "price" => hits.sort_by_key(list_price),
        "popularity" | "relevance" => hits.sort_by_key(rank),
        "alphabetical" => hits.sort_by_key(|p| p["name"].as_str().unwrap_or("").to_string()),
        _ => hits.sort_by_key(|p| std::cmp::Reverse(p["sku_id"].as_str().unwrap_or("").to_string())),
    }
    if (kind == "price" || kind == "alphabetical") && dir == "desc" {
        hits.reverse();
    }
    let total = hits.len();
    let skus: Vec<Value> = hits.iter().skip(offset).take(limit).map(|p| p["sku_id"].clone()).collect();
    Some((200, json!({ "skus": skus, "pagination": { "total": total, "has_more": offset + limit < total, "offset": offset, "limit": limit } })))
}

fn cdn() -> String {
    format!("http://127.0.0.1:{}/cdn", crate::PORT.load(std::sync::atomic::Ordering::Relaxed))
}

fn media(name: &str) -> String {
    format!("{}/media/v1/collectibles-shop/{name}", cdn())
}

fn prices(base: i64, member: i64, orbs: Option<i64>) -> Value {
    let list = |amount: i64| {
        let mut v = vec![json!({ "amount": amount, "currency": "vnd", "exponent": 0 })];
        if let Some(o) = orbs {
            v.push(json!({ "amount": o, "currency": "discord_orb", "exponent": 0 }));
        }
        json!({ "country_prices": { "country_code": "VN", "prices": v } })
    };
    json!({ "0": list(base), "4": list(member) })
}

fn styles() -> Value {
    json!({ "background_colors": [1183774, 1319459], "button_colors": [5793266, 5793266], "confetti_colors": [12320512, 16727693] })
}

fn product(sku: &str, name: &str, summary: &str, kind: u64, items: Value, p: Value, cat: &str) -> Value {
    json!({ "sku_id": sku, "name": name, "summary": summary, "store_listing_id": sku, "styles": styles(), "prices": p, "preview_assets": null,
        "items": items, "type": kind, "premium_type": 0, "category_sku_id": cat })
}

fn categories() -> Vec<Value> {
    let hat = product("5101", "Hex's Hat", "Give your avatar a new look.", 0, json!([{ "type": 0, "sku_id": "5101", "asset": "a_mockhat", "label": "A witch hat." }]), prices(79000, 66000, None), "5001");
    let dust = product("5102", "Witch Dust", "Make your name stand out in servers and chats.", 2, json!([{ "type": 2, "sku_id": "5102", "asset": "nameplates/witch_dust/5102/", "label": "Sparkles.", "palette": "cobalt" }]), prices(79000, 66000, None), "5001");
    let fx = product("5103", "Lottie's Cauldron", "Show this effect when others view your profile.", 1,
        json!([{ "type": 1, "sku_id": "5103", "title": "Lottie's Cauldron", "description": "Show this effect when others view your profile.", "thumbnailPreviewSrc": media("effect-5103"), "reducedMotionSrc": media("effect-5103"), "effects": [] }]),
        prices(92000, 79000, None), "5001");
    let mut bundle = product("5104", "Witching Hour Bundle", "{joinedItems}", 1000, json!([]), prices(189000, 146000, None), "5001");
    bundle["preview_assets"] = json!({ "fg_static": media("bundle-fg-static/5104"), "bg_static": media("bundle-bg-static/5104") });
    bundle["bundled_products"] = json!([hat, dust]);
    let fox = product("5201", "Fox Ears", "Give your avatar a new look.", 0, json!([{ "type": 0, "sku_id": "5201", "asset": "a_mockfox", "label": "Fox ears." }]), prices(79000, 66000, None), "5002");
    let bunny = product("5202", "Bunny Babe", "Make your name stand out in servers and chats.", 2, json!([{ "type": 2, "sku_id": "5202", "asset": "nameplates/all_ears/5202/", "label": "Bunny.", "palette": "bubble_gum" }]), prices(79000, 66000, None), "5002");
    let mut cape = product("5204", "Collab Cape", "A cape from another world.", 0, json!([{ "type": 0, "sku_id": "5204", "asset": "a_mockcape", "label": "A cape." }]), prices(99000, 79000, None), "5002");
    cape["is_first_party"] = json!(false);
    let mut orb = product("5203", "Panda Companion", "Give your avatar a new look.", 2000, json!([]), prices(79000, 66000, Some(4100)), "5002");
    orb["variants"] = json!([
        product("5210", "Panda Companion", "Give your avatar a new look.", 0, json!([{ "type":0,"sku_id":"5210","asset":"a_mockpanda" }]), prices(79000,66000,Some(4100)), "5002"),
        product("5211", "Blue Panda Companion", "Give your avatar a new look.", 0, json!([{ "type":0,"sku_id":"5211","asset":"a_mockpanda_blue" }]), prices(79000,66000,Some(4100)), "5002")
    ]);
    let frames: Vec<Value> = (1..=30).map(|n| product(&format!("53{n:02}"), &format!("Orchard Frame {n}"), "Frame your avatar.", 3, json!([]), prices(79000, 66000, None), "5003")).collect();
    vec![
        json!({ "sku_id": "5001", "name": "Cauldron Chaos", "summary": " ", "store_listing_id": "5001", "styles": styles(), "hero_ranking": ["5104", "5101"], "products": [bundle, hat, dust, fx],
            "hero_banner_url": media("hero-5001"), "hero_logo_url": media("logo-5001"), "catalog_banner_url": media("banner-5001"), "logo_url": media("logo-5001") }),
        json!({ "sku_id": "5002", "name": "All Ears", "summary": " ", "store_listing_id": "5002", "styles": styles(), "hero_ranking": ["5201"], "products": [fox, bunny, orb, cape],
            "hero_banner_url": media("hero-5002"), "hero_logo_url": media("logo-5002"), "catalog_banner_url": media("banner-5002"), "logo_url": media("logo-5002") }),
        json!({ "sku_id": "5003", "name": "Orchard", "summary": " ", "store_listing_id": "5003", "styles": styles(), "hero_ranking": [], "products": frames,
            "hero_banner_url": media("hero-5002"), "hero_logo_url": media("logo-5002"), "catalog_banner_url": media("banner-5002"), "logo_url": media("logo-5002") }),
    ]
}

fn home() -> Value {
    json!({
        "shop_blocks": [
            { "type": 0, "category_sku_id": "5001", "name": "Cauldron Chaos", "category_store_listing_id": "5001", "unpublished_at": null, "summary": "Cast a spell, unlock a reward",
              "ranked_sku_ids": ["5104", "5101", "5102", "5103"], "hero_banner_url": media("hero-5001"), "hero_logo_url": media("logo-5001"), "mobile_hero_url": media("hero-5001"), "banner_display_config": {} },
            { "type": 1, "subblocks": [
                { "type": 0, "category_sku_id": "5001", "category_store_listing_id": "5001", "name": "Cauldron Chaos", "unpublished_at": null, "asset_url": media("tile-5001") },
                { "type": 0, "category_sku_id": "5002", "category_store_listing_id": "5002", "name": "All Ears", "unpublished_at": null, "asset_url": media("tile-5002") } ] },
            { "type": 4, "name": "Fresh Drops", "ranked_sku_ids": ["5201", "5202", "5203"], "category_sku_id": "5002", "show_button": true, "button_text": "Shop all" },
            { "type": 2, "ranked_sku_ids": POPULAR, "sorted_sku_ids": { "popular": POPULAR, "recommended": [] } },
        ],
        "categories": categories(),
    })
}

fn orbs_home() -> Value {
    json!({ "shop_blocks": [
        { "type": 3, "title": "Introducing Discord Orbs", "body": "Reward Your Play. Earn through Quests. Spend in the Shop.", "wide_banner_url": media("orbs-banner"),
          "cta_text": "Go to Quest Home", "cta_route": "/quest-home", "category_store_listing_id": "5002" },
        { "type": 2, "ranked_sku_ids": POPULAR, "sorted_sku_ids": { "popular": POPULAR, "recommended": [] } },
    ], "categories": categories() })
}

pub fn route(method: &str, parts: &[&str], query: &str, body: &[u8]) -> Option<(u16, Value)> {
    let missing = |what: &str| Some((400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { what: { "_errors": [{ "code": "BASE_TYPE_REQUIRED", "message": "This field is required" }] } } })));
    match (method, parts) {
        ("GET", ["api", "v9", "collectibles-shop"]) if query.contains("tab=orbs") => Some((200, orbs_home())),
        ("GET", ["api", "v9", "collectibles-shop"]) => Some((200, home())),
        ("GET", ["api", "v9", "shop", "search"]) => search(query),
        ("GET", ["api", "v9", "collectibles-products", sku]) => categories().iter().flat_map(|c| c["products"].as_array().cloned().unwrap_or_default()).find(|p| p["sku_id"] == *sku).map(|p| (200, p)),
        ("GET", ["api", "v9", "wishlists", "7700"]) => Some((200, wishlist())),
        ("GET", ["api", "v9", "wishlist", "gift-recommendations"]) => {
            if !query.contains("user_ids=") {
                return missing("user_ids");
            }
            let skus: Vec<Value> = ["5201", "5202", "5101", "5102", "5103", "5204", "5301", "5302"].iter().map(|s| json!({ "id": s, "product_line": 1 })).collect();
            Some((200, json!({ "skus": skus, "skus_to_user_and_reason": {}, "applications": [], "storefront_pricing": null })))
        }
        ("GET", ["api", "v9", "collectibles-categories", "v2"]) => Some((200, json!({ "categories": categories(), "collections": [] }))),
        ("POST", ["api", "v9", "mock", "wishlist-reset"]) => {
            WISH.lock().unwrap().clear();
            Some((200, json!({})))
        }
        ("POST", ["api", "v9", "users", "@me", "wishlist", "items"]) => {
            let b: Value = serde_json::from_slice(body).unwrap_or_default();
            let Some(sku) = b["sku_id"].as_str() else { return missing("sku_id") };
            if !categories().iter().flat_map(|c| c["products"].as_array().cloned().unwrap_or_default()).any(|p| p["sku_id"] == sku) {
                return Some((404, json!({ "message": "Unknown SKU", "code": 10067 })));
            }
            let mut w = WISH.lock().unwrap();
            if !w.iter().any(|s| s == sku) {
                w.push(sku.to_string());
            }
            drop(w);
            Some((200, wishlist()))
        }
        ("DELETE", ["api", "v9", "users", "@me", "wishlists", "7700", "items", sku]) => {
            WISH.lock().unwrap().retain(|s| s != sku);
            Some((200, wishlist()))
        }
        ("DELETE", ["api", "v9", "users", "@me", "wishlists", ..]) => Some((404, json!({ "message": "Unknown Wishlist", "code": 10000 }))),
        _ => None,
    }
}

const PAGE: &str = r#"<title>Discord</title><style>body{background:#1e1f22;color:#f2f3f5;font:16px sans-serif;padding:32px}</style><h2>__TITLE__</h2><script>fetch("/api/v9/mock/webwin",{method:"POST",body:JSON.stringify({page:location.pathname+location.search,token:localStorage.getItem("token")})})</script>"#;

fn page(title: &str) -> (u16, &'static str, Vec<u8>) {
    (200, "text/html", PAGE.replace("__TITLE__", title).into_bytes())
}

pub fn billing_page(parts: &[&str]) -> Option<(u16, &'static str, Vec<u8>)> {
    match parts {
        ["billing", "premium", "manage"] => Some(page("Manage subscription")),
        ["billing", "payment-sources", "create"] => Some(page("Add a payment method")),
        ["billing", "mock-confirm", id] => Some((200, "text/html", CONFIRM.replace("__ID__", id).into_bytes())),
        ["billing", ..] => Some((404, "text/plain", b"no such billing page".to_vec())),
        _ => None,
    }
}
