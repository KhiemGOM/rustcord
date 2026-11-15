//! Game Shops: which games sell through Discord, and one game's storefront.

use serde_json::{Value, json};

const GAME: &str = "9004";

fn listing(id: &str, name: &str, amount: i64) -> Value {
    json!({ "id": format!("l{id}"), "summary": name, "sku": {
        "id": id, "type": 5, "application_id": GAME, "name": name, "description": format!("{name} for your account."),
        "tenant_metadata": { "social_layer": { "label": "new", "card_image_asset_id": null, "card_background_image_asset_id": null } },
        "price": { "amount": amount, "currency": "usd", "currency_exponent": 2 } } })
}

pub fn route(method: &str, parts: &[&str]) -> Option<(u16, Value)> {
    Some(match (method, parts) {
        ("GET", ["api", "v9", "partner-sdk", "storefront-config"]) => (200, json!({
            "promotional_sku_ids": [], "promotion_end_datetime": null,
            "storefronts": [
                { "guild_id": "1001", "application_id": GAME, "game_id": GAME, "collectibles_shop_navigation_enabled": true, "excluded_platforms": [], "disable_mobile_account_linking": false },
                { "guild_id": "1002", "application_id": "9001", "game_id": "9001", "collectibles_shop_navigation_enabled": false, "excluded_platforms": [], "disable_mobile_account_linking": false },
            ],
        })),
        ("GET", ["api", "v9", "partner-sdk", "applications", app, "application-storefront"]) if *app == GAME => (200, json!({
            "id": "77001", "application_id": GAME, "application": { "id": GAME, "name": "Genshin Impact" }, "title": "Genshin Impact Shop", "logo_asset_id": null,
            "pages": [
                { "title": "Featured", "sku_ids": ["88001"], "sections": [{ "title": "Bundles", "sku_ids": ["88002", "88003"] }] },
                { "title": "Items", "sku_ids": ["88003"], "sections": [] },
            ],
            "store_listings": [listing("88001", "Welkin Moon", 499), listing("88002", "Starter Bundle", 999), listing("88003", "Crystal Pack", 1499)],
            "assets": [],
        })),
        ("GET", ["api", "v9", "partner-sdk", "applications", _, "application-storefront"]) => (404, json!({ "message": "Unknown Storefront", "code": 10000 })),
        _ => return None,
    })
}
