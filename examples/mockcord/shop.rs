//! The Shop and the checkout pages of the mock: a small collectibles catalog in Discord's shape,
//! the wishlist, and the standalone billing pages the checkout window loads. Anything outside
//! those pages answers 404, so a window that wanders into the rest of the app fails visibly.

use serde_json::{Value, json};
use std::sync::Mutex;

const CONFIRM: &str = r#"<title>Confirm payment</title><style>body{background:#1e1f22;color:#f2f3f5;font:16px sans-serif;padding:32px}</style><h2>Confirm your payment</h2><p>Mock bank page.</p><script>setTimeout(()=>fetch("/api/v9/mock/pay/__ID__/complete",{method:"POST"}),1500)</script>"#;

static WISH: Mutex<Vec<String>> = Mutex::new(Vec::new());

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
    let orb = product("5203", "Panda Companion", "Give your avatar a new look.", 2000, json!([]), prices(79000, 66000, Some(4100)), "5002");
    vec![
        json!({ "sku_id": "5001", "name": "Cauldron Chaos", "summary": " ", "store_listing_id": "5001", "styles": styles(), "hero_ranking": ["5104", "5101"], "products": [bundle, hat, dust, fx],
            "hero_banner_url": media("hero-5001"), "hero_logo_url": media("logo-5001"), "catalog_banner_url": media("banner-5001"), "logo_url": media("logo-5001") }),
        json!({ "sku_id": "5002", "name": "All Ears", "summary": " ", "store_listing_id": "5002", "styles": styles(), "hero_ranking": ["5201"], "products": [fox, bunny, orb],
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
        ],
        "categories": categories(),
    })
}

pub fn route(method: &str, parts: &[&str], body: &[u8]) -> Option<(u16, Value)> {
    let missing = |what: &str| Some((400, json!({ "message": "Invalid Form Body", "code": 50035, "errors": { what: { "_errors": [{ "code": "BASE_TYPE_REQUIRED", "message": "This field is required" }] } } })));
    match (method, parts) {
        ("GET", ["api", "v9", "collectibles-shop"]) => Some((200, home())),
        ("GET", ["api", "v9", "collectibles-categories", "v2"]) => Some((200, json!({ "categories": categories(), "collections": [] }))),
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
            Some((200, json!({ "id": "7700", "items": w.iter().map(|s| json!({ "sku_id": s })).collect::<Vec<_>>() })))
        }
        ("DELETE", ["api", "v9", "users", "@me", "wishlists", "7700", "items", sku]) => {
            WISH.lock().unwrap().retain(|s| s != sku);
            Some((204, Value::Null))
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
