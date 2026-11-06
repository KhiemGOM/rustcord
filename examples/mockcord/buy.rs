//! The purchase calls of the mock, as strict as Discord's: a charge is refused for a missing or
//! invalid payment source, a total that is not the one a preview gave, a missing purchase token
//! or a bad recipient, and every charge that goes through is counted in the request log
//! (`{"charge": "accepted"|"refused:<code>"|"confirm"|"boom"}`) so a test can say how many
//! there were. `POST /api/v9/mock/buy` sets how the next ones behave.

use serde_json::{Value, json};
use std::sync::Mutex;

const NITRO: &str = "521842831262875670";
const BASIC: &str = "978380684370378762";
const FRIENDS: [&str; 3] = ["101", "102", "103"];

struct State {
    attempts: u64,
    accepted: u64,
    shift: i64,
    orbs: u64,
    owned: Vec<String>,
    drift: i64,
    decline: bool,
    confirm: bool,
    boom: bool,
    no_sources: bool,
    pay: Vec<(String, u64)>,
    slot: u32,
}

static S: Mutex<Option<State>> = Mutex::new(None);

impl State {
    fn new() -> Self {
        State { attempts: 0, accepted: 0, shift: 0, orbs: 6000, owned: vec![], drift: 0, decline: false, confirm: false, boom: false, no_sources: false, pay: vec![], slot: 0 }
    }
}

fn price(sku: &str, plan: Option<&str>) -> Option<i64> {
    Some(match (sku, plan) {
        ("5101" | "5102" | "5201" | "5202" | "5203", None) => 79000,
        ("5103", None) => 92000,
        ("5104", None) => 189000,
        ("6101", None) => 49000,
        ("6102", None) => 99000,
        (NITRO, Some("511651880837840896")) => 113000,
        (NITRO, Some("511651885459963904")) => 1130000,
        (BASIC, Some("978380692553465866")) => 42000,
        (BASIC, Some("1024422698568122368")) => 420000,
        _ => return None,
    })
}

fn orb_price(sku: &str) -> Option<u64> {
    (sku == "5203").then_some(4100)
}

fn plan_price(plan: &str) -> Option<i64> {
    Some(match plan {
        "511651880837840896" => 113000,
        "511651885459963904" => 1130000,
        "978380692553465866" => 42000,
        "1024422698568122368" => 420000,
        "590665532894740483" => 59000,
        "590665538238152709" => 590000,
        _ => return None,
    })
}

fn tax(sub: i64) -> i64 {
    sub / 10
}

fn bad(code: u64, message: &str) -> Option<(u16, Value)> {
    Some((400, json!({ "message": message, "code": code })))
}

fn param(path: &str, key: &str) -> Option<String> {
    path.split_once('?')?.1.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}="))).map(str::to_string)
}

fn source(id: Option<&str>) -> Result<(), (u16, Value)> {
    let Some(id) = id else { return Err((400, json!({ "message": "Payment source required", "code": 100006 }))) };
    let g = crate::BILL.lock().unwrap();
    let hit = g["sources"].as_array().and_then(|l| l.iter().find(|s| s["id"] == id));
    match hit {
        Some(s) if s["invalid"] != true => Ok(()),
        _ => Err((400, json!({ "message": "Unknown Payment Source", "code": 100002 }))),
    }
}

fn invoice(sub: i64, drift: i64) -> Value {
    json!({ "total": sub + tax(sub) + drift, "amount": sub + tax(sub) + drift, "subtotal": sub, "tax": tax(sub), "currency": "vnd", "tax_inclusive": false,
            "invoice_items": [], "subscription_period_start": "2026-11-06T00:00:00+00:00", "subscription_period_end": "2026-12-06T00:00:00+00:00" })
}

fn items_price(items: &[Value], renewal: bool) -> Option<i64> {
    let mut sum = 0;
    for i in items {
        let q = i["quantity"].as_i64().filter(|q| *q >= 1)?;
        let p = plan_price(i["plan_id"].as_str()?)?;
        let boost = i["plan_id"].as_str()?.starts_with("5906655");
        if renewal || boost {
            sum += p * q;
        }
    }
    Some(sum)
}

fn log(w: &crate::W, what: &str) {
    crate::log(w, json!({ "t": crate::now_ms(), "charge": what }));
}

pub fn route(w: &crate::W, method: &str, parts: &[&str], path: &str, body: &[u8]) -> Option<(u16, Value)> {
    let mut g = S.lock().unwrap();
    let st = g.get_or_insert_with(State::new);
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    match (method, parts) {
        ("GET", ["api", "v9", "mock", "buy"]) => Some((200, json!({ "attempts": st.attempts, "accepted": st.accepted, "orbs": st.orbs }))),
        ("POST", ["api", "v9", "mock", "buy"]) => {
            if b["reset"] == true {
                *st = State::new();
                *crate::BILL.lock().unwrap() = crate::bill_state();
            }
            if let Some(o) = b["orbs"].as_u64() {
                st.orbs = o;
            }
            for (k, f) in [("decline", &mut st.decline), ("confirm", &mut st.confirm), ("boom", &mut st.boom), ("no_sources", &mut st.no_sources)] {
                if let Some(v) = b[k].as_bool() {
                    *f = v;
                }
            }
            if let Some(d) = b["drift"].as_i64() {
                st.drift = d;
            }
            let mut bill = crate::BILL.lock().unwrap();
            if bill.is_null() {
                *bill = crate::bill_state();
            }
            if b["no_sub"] == true {
                bill["subs"] = json!([]);
            }
            if b["eea"] == true {
                bill["sources"][2]["country"] = json!("DE");
                bill["sources"][2]["billing_address"]["country"] = json!("DE");
            }
            Some((200, json!({ "orbs": st.orbs })))
        }
        ("POST", ["api", "v9", "mock", "pay", id, "complete"]) => {
            for p in st.pay.iter_mut().filter(|p| p.0 == *id && p.1 == 0) {
                p.1 = 1;
                st.accepted += 1;
            }
            Some((204, Value::Null))
        }
        ("GET", ["api", "v9", "users", "@me", "billing", "payments", id]) => {
            let status = st.pay.iter().find(|p| p.0 == *id)?.1;
            Some((200, json!({ "id": id, "status": status, "amount": 0, "currency": "vnd" })))
        }
        ("GET", ["api", "v9", "users", "@me", "billing", "payment-sources"]) if st.no_sources => Some((200, json!([]))),
        ("GET", ["api", "v9", "users", "@me", "virtual-currency", "balance"]) => Some((200, json!({ "balance": st.orbs }))),
        ("GET", ["api", "v9", "store", "published-listings", "skus", sku, "subscription-plans"]) => {
            let plans: &[(&str, i64, u8)] = match *sku {
                NITRO => &[("511651880837840896", 113000, 1), ("511651885459963904", 1130000, 2)],
                BASIC => &[("978380692553465866", 42000, 1), ("1024422698568122368", 420000, 2)],
                "590663762298667008" => &[("590665532894740483", 59000, 1), ("590665538238152709", 590000, 2)],
                _ => return None,
            };
            Some((200, plans.iter().map(|(id, p, i)| json!({ "id": id, "interval": i, "interval_count": 1, "price": p, "currency": "vnd", "sku_id": sku })).collect()))
        }
        ("GET", ["api", "v9", "guilds", _, "products", "listings"]) => {
            let at = "2026-10-01T00:00:00+00:00";
            Some((200, json!({ "listings": [
                { "id": "6101", "name": "Wumpus Sticker Pack", "description": "Twelve stickers for the whole server to use.", "price_tier": 49000, "published_at": at },
                { "id": "6102", "name": "Design Club Mug", "description": "Digital mug art, full resolution.", "price_tier": 99000, "published_at": at },
                { "id": "6103", "name": "Unpublished", "description": "", "price_tier": 1000, "published_at": null },
            ] })))
        }
        ("GET", ["api", "v9", "store", "skus", sku, "purchase"]) => {
            if let Err(e) = source(param(path, "payment_source_id").as_deref()) {
                return Some(e);
            }
            if let Some(c) = param(path, "currency").filter(|c| c != "vnd") {
                return bad(100144, &format!("currency {c} not allowed"));
            }
            let plan = param(path, "sku_subscription_plan_id");
            let Some(p) = price(sku, plan.as_deref()) else { return Some((404, json!({ "message": "Unknown SKU", "code": 10067 }))) };
            Some((200, invoice(p, st.shift)))
        }
        ("POST", ["api", "v9", "users", "@me", "billing", "subscriptions", "preview"]) | ("PATCH", ["api", "v9", "users", "@me", "billing", "subscriptions", _, "preview"]) => {
            if let Err(e) = source(b["payment_source_id"].as_str()) {
                return Some(e);
            }
            let items = b["items"].as_array()?;
            let main = items_price(items, false)?;
            if b["renewal"] == true {
                return Some((200, invoice(items_price(items, true)?, 0)));
            }
            Some((200, invoice(main, st.shift)))
        }
        ("POST", ["api", "v9", "virtual-currency", "skus", sku, "redeem"]) => {
            st.attempts += 1;
            if b["checkout_session_id"].as_str().is_none_or(str::is_empty) {
                return bad(50035, "checkout_session_id required");
            }
            let Some(cost) = orb_price(sku) else { return bad(50035, "This item is not redeemable with Orbs.") };
            if st.orbs < cost {
                log(w, "refused:590001");
                return bad(590001, "Insufficient balance");
            }
            if st.owned.iter().any(|o| o == sku) {
                log(w, "refused:100011");
                return bad(100011, "Already purchased");
            }
            st.orbs -= cost;
            st.owned.push(sku.to_string());
            st.accepted += 1;
            log(w, "accepted");
            Some((200, json!([{ "id": "8800", "sku_id": sku, "type": 1 }])))
        }
        ("POST", ["api", "v9", "store", "skus", sku, "purchase"]) => {
            st.attempts += 1;
            if let Err((c, v)) = source(b["payment_source_id"].as_str()) {
                log(w, &format!("refused:{}", v["code"]));
                return Some((c, v));
            }
            if b["purchase_token"].as_str().is_none_or(str::is_empty) {
                log(w, "refused:50035");
                return bad(50035, "purchase_token required");
            }
            let gift = b["gift"] == true;
            let plan = b["sku_subscription_plan_id"].as_str();
            let Some(p) = price(sku, plan) else { return Some((404, json!({ "message": "Unknown SKU", "code": 10067 }))) };
            let want = p + tax(p) + st.drift + st.shift;
            if b["expected_amount"].as_i64() != Some(want) || b["expected_currency"] != "vnd" {
                st.shift += std::mem::take(&mut st.drift);
                log(w, "refused:100111");
                return bad(100111, "The purchase could not be validated");
            }
            if gift && !FRIENDS.contains(&b["gift_info_options"]["recipient_id"].as_str().unwrap_or("")) {
                log(w, "refused:50035");
                return bad(50035, "Unknown recipient");
            }
            if !gift && st.owned.iter().any(|o| o == sku) {
                log(w, "refused:100011");
                return bad(100011, "Already purchased");
            }
            if st.decline {
                log(w, "refused:100054");
                return Some((402, json!({ "message": "Your card was declined", "code": 100054 })));
            }
            if st.confirm {
                st.confirm = false;
                let id = "7001".to_string();
                st.pay.push((id.clone(), 0));
                if !gift {
                    st.owned.push(sku.to_string());
                }
                log(w, "confirm");
                let url = format!("http://127.0.0.1:{}/billing/mock-confirm/{id}", crate::PORT.load(std::sync::atomic::Ordering::Relaxed));
                return Some((402, json!({ "message": "Confirmation required", "code": 100047, "payment_id": id, "adyen_redirect_url": url })));
            }
            if st.boom {
                st.boom = false;
                if !gift {
                    st.owned.push(sku.to_string());
                }
                log(w, "boom");
                return Some((500, json!({ "message": "boom" })));
            }
            if !gift {
                st.owned.push(sku.to_string());
            }
            st.accepted += 1;
            log(w, "accepted");
            Some((200, json!({ "entitlements": [{ "id": "8801", "sku_id": sku }], "gift_code": if gift { json!("mockgiftcode") } else { Value::Null } })))
        }
        ("POST", ["api", "v9", "users", "@me", "billing", "subscriptions"]) | ("PATCH", ["api", "v9", "users", "@me", "billing", "subscriptions", _]) => {
            st.attempts += 1;
            if let Err((c, v)) = source(b["payment_source_id"].as_str()) {
                log(w, &format!("refused:{}", v["code"]));
                return Some((c, v));
            }
            if b["purchase_token"].as_str().is_none_or(str::is_empty) {
                log(w, "refused:50035");
                return bad(50035, "purchase_token required");
            }
            let items = b["items"].as_array()?.clone();
            let main = items_price(&items, false)?;
            let renewal = items_price(&items, true)?;
            let (due, again) = (main + tax(main) + st.drift + st.shift, renewal + tax(renewal));
            let ok = |k: &str, want: i64| b[k]["amount"].as_i64() == Some(want) && b[k]["currency"] == "vnd";
            if !ok("expected_invoice_price", due) || !ok("expected_renewal_price", again) {
                st.shift += std::mem::take(&mut st.drift);
                log(w, "refused:100111");
                return bad(100111, "The purchase could not be validated");
            }
            if st.decline {
                log(w, "refused:100054");
                return Some((402, json!({ "message": "Your card was declined", "code": 100054 })));
            }
            let mut bill = crate::BILL.lock().unwrap();
            if method == "POST" {
                bill["subs"] = json!([{ "id": "9610", "type": 1, "status": 1, "payment_source_id": b["payment_source_id"], "canceled_at": null, "items": items }]);
            } else {
                let boosts: i64 = items.iter().filter(|i| i["plan_id"].as_str().is_some_and(|p| p.starts_with("5906655"))).filter_map(|i| i["quantity"].as_i64()).sum();
                for _ in 0..boosts {
                    st.slot += 1;
                    let id = format!("97{}", 10 + st.slot);
                    bill["slots"].as_array_mut()?.push(json!({ "id": id, "subscription_id": "9601", "canceled": false, "cooldown_ends_at": null, "premium_guild_subscription": null }));
                }
            }
            st.accepted += 1;
            log(w, "accepted");
            Some((200, json!({ "id": "9601", "type": 1, "status": 1, "items": items })))
        }
        _ => None,
    }
}
