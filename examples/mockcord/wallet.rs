//! The Orb Wallet's reads: the challenges that pay Orbs, claiming one, and the lifetime total.

use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

static CLAIMED: AtomicBool = AtomicBool::new(false);
static SEEN: AtomicBool = AtomicBool::new(false);

pub const EXPERIMENT: u32 = 2185542098;

pub fn route(method: &str, parts: &[&str]) -> Option<(u16, Value)> {
    Some(match (method, parts) {
        ("GET", ["api", "v9", "users", "@me", "achievements", "list"]) => (200, json!({
            "achievements": [
                { "achievement_identifier": "first_message", "achievement_definition_id": "9900000000000000001", "title": "Say hello in a server", "reward_amount": 50, "status": if CLAIMED.load(Relaxed) { 3 } else { 2 } },
                { "achievement_identifier": "shop_visit", "achievement_definition_id": "1531422538125676707", "title": "Visit the Shop", "reward_amount": 100, "status": 0 },
            ],
            "orb_multiplier": 1.0, "nitro_gated_orb_multiplier": 1.2, "has_unclaimed_achievements": !CLAIMED.load(Relaxed),
        })),
        ("GET", ["api", "v9", "users", "@me", "virtual-currency", "total-redeemed"]) => (200, json!({ "total_redeemed": 15600 })),
        ("GET", ["api", "v9", "users", "@me", "achievements", "unread-state"]) => (200, json!({ "has_unread_new_achievement": !SEEN.load(Relaxed), "has_unclaimed_achievements": !CLAIMED.load(Relaxed) })),
        ("POST", ["api", "v9", "users", "@me", "achievements", "unread-ack"]) => {
            SEEN.store(true, Relaxed);
            (200, json!({ "has_unread_new_achievement": false, "has_unclaimed_achievements": !CLAIMED.load(Relaxed) }))
        }
        ("POST", ["api", "v9", "users", "@me", "achievements", id, "claim"]) => {
            if *id != "first_message" || CLAIMED.swap(true, Relaxed) {
                (400, json!({ "message": "Achievement cannot be claimed", "code": 50035 }))
            } else {
                (200, json!({}))
            }
        }
        ("POST", ["api", "v9", "mock", "wallet-reset"]) => {
            CLAIMED.store(false, Relaxed);
            SEEN.store(false, Relaxed);
            (200, json!({}))
        }
        _ => return None,
    })
}
