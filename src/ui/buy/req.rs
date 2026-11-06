//! The calls a purchase makes, and what comes back. Shapes are the ones Discord's client sends
//! (`STORE_SKU_PURCHASE`, `BILLING_SUBSCRIPTIONS` and its preview, `VIRTUAL_CURRENCY_SKU_REDEEM`);
//! nothing here sends anything.

use serde_json::{Map, Value, json};

pub const BASIC_SKU: &str = "978380684370378762";
pub const BOOST_SKU: &str = "590663762298667008";
pub const NITRO_MONTH: &str = "511651880837840896";
pub const NITRO_YEAR: &str = "511651885459963904";
pub const BASIC_MONTH: &str = "978380692553465866";
pub const BASIC_YEAR: &str = "1024422698568122368";
pub const BOOST_MONTH: &str = "590665532894740483";
pub const BOOST_YEAR: &str = "590665538238152709";

const CONFIRMATION_REQUIRED: u64 = 100047;
const AUTHENTICATION_REQUIRED: u64 = 100029;
const OUTDATED_PARAMETERS: u64 = 100111;
const INSUFFICIENT_ORBS: u64 = 590001;

pub fn nitro_sku(plan: &str) -> &'static str {
    match plan {
        BASIC_MONTH | BASIC_YEAR => BASIC_SKU,
        "511651871736201216" | "511651876987469824" => "521846918637420545",
        _ => crate::ui::settings::billing::NITRO_SKU,
    }
}

pub fn yearly(plan: &str) -> bool {
    matches!(plan, NITRO_YEAR | BASIC_YEAR | BOOST_YEAR | "511651876987469824")
}

pub fn boost_plan(year: bool) -> &'static str {
    if year { BOOST_YEAR } else { BOOST_MONTH }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Sku { sku: String, plan: Option<String> },
    New { plan: String, qty: u32 },
    Add { sub: String, items: Vec<(Option<String>, String, u32)> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Invoice {
    pub total: i64,
    pub subtotal: i64,
    pub tax: i64,
    pub currency: String,
    pub inclusive: bool,
}

pub fn invoice(v: &Value) -> Option<Invoice> {
    let total = v["total"].as_i64().or(v["amount"].as_i64())?;
    Some(Invoice {
        total,
        subtotal: v["subtotal"].as_i64().unwrap_or(total),
        tax: v["tax"].as_i64().unwrap_or(0),
        currency: v["currency"].as_str()?.to_string(),
        inclusive: v["tax_inclusive"].as_bool().unwrap_or(false),
    })
}

fn items(list: &[(Option<String>, String, u32)]) -> Value {
    list.iter()
        .map(|(id, plan, qty)| {
            let mut o = Map::new();
            if let Some(id) = id {
                o.insert("id".into(), json!(id));
            }
            o.insert("plan_id".into(), json!(plan));
            o.insert("quantity".into(), json!(qty));
            Value::Object(o)
        })
        .collect()
}

fn sub_items(line: &Line) -> Option<Value> {
    match line {
        Line::New { plan, qty } => Some(items(&[(None, plan.clone(), *qty)])),
        Line::Add { items: i, .. } => Some(items(i)),
        Line::Sku { .. } => None,
    }
}

pub struct Call {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
}

/// The server's own total for this purchase, before any money moves.
pub fn preview(line: &Line, gift: bool, source: Option<&str>, currency: &str, load: &str, renewal: bool) -> Call {
    match line {
        Line::Sku { sku, plan } => {
            let mut q = vec![format!("gift={gift}")];
            if let Some(s) = source {
                q.push(format!("payment_source_id={s}"));
            }
            if let Some(p) = plan {
                q.push(format!("sku_subscription_plan_id={p}"));
            }
            q.push(format!("currency={currency}"));
            q.push(format!("load_id={load}"));
            Call { method: "GET", path: format!("/store/skus/{sku}/purchase?{}", q.join("&")), body: None }
        }
        Line::New { .. } | Line::Add { .. } => {
            let mut b = Map::new();
            b.insert("items".into(), sub_items(line).unwrap_or_default());
            b.insert("payment_source_id".into(), json!(source));
            b.insert("apply_entitlements".into(), json!(false));
            b.insert("currency".into(), json!(currency));
            b.insert("renewal".into(), json!(renewal));
            b.insert("load_id".into(), json!(load));
            match line {
                Line::Add { sub, .. } => Call { method: "PATCH", path: format!("/users/@me/billing/subscriptions/{sub}/preview"), body: Some(Value::Object(b)) },
                _ => Call { method: "POST", path: "/users/@me/billing/subscriptions/preview".into(), body: Some(Value::Object(b)) },
            }
        }
    }
}

pub struct Gift {
    pub style: u8,
    pub to: String,
    pub message: String,
}

pub struct Pay<'a> {
    pub line: &'a Line,
    pub gift: Option<&'a Gift>,
    pub source: &'a str,
    pub token: Option<String>,
    pub load: &'a str,
    pub purchase_token: &'a str,
    pub expected: &'a Invoice,
    pub renewal: Option<&'a Invoice>,
}

/// The purchase itself. `expected_amount` and `expected_currency` are the figures on screen, so
/// the server refuses if the price has moved.
pub fn charge(p: &Pay) -> Call {
    let money = |i: &Invoice| json!({ "amount": i.total, "currency": i.currency });
    match p.line {
        Line::Sku { sku, plan } => {
            let mut b = Map::new();
            b.insert("gift".into(), json!(p.gift.is_some()));
            if let Some(plan) = plan {
                b.insert("sku_subscription_plan_id".into(), json!(plan));
            }
            b.insert("gateway_checkout_context".into(), Value::Null);
            b.insert("load_id".into(), json!(p.load));
            if let Some(g) = p.gift {
                b.insert("gift_info_options".into(), json!({ "gift_style": g.style, "reward_sku_ids": [], "recipient_id": g.to, "custom_message": g.message }));
            }
            b.insert("payment_source_id".into(), json!(p.source));
            b.insert("payment_source_token".into(), json!(p.token));
            b.insert("expected_amount".into(), json!(p.expected.total));
            b.insert("expected_currency".into(), json!(p.expected.currency));
            b.insert("purchase_token".into(), json!(p.purchase_token));
            b.insert("apply_wallet_balance".into(), json!(false));
            Call { method: "POST", path: format!("/store/skus/{sku}/purchase"), body: Some(Value::Object(b)) }
        }
        Line::New { .. } | Line::Add { .. } => {
            let mut b = Map::new();
            b.insert("items".into(), sub_items(p.line).unwrap_or_default());
            b.insert("payment_source_id".into(), json!(p.source));
            b.insert("payment_source_token".into(), json!(p.token));
            b.insert("currency".into(), json!(p.expected.currency));
            b.insert("gateway_checkout_context".into(), Value::Null);
            b.insert("purchase_token".into(), json!(p.purchase_token));
            b.insert("load_id".into(), json!(p.load));
            b.insert("expected_invoice_price".into(), money(p.expected));
            b.insert("expected_renewal_price".into(), p.renewal.map_or(money(p.expected), money));
            match p.line {
                Line::Add { sub, .. } => Call { method: "PATCH", path: format!("/users/@me/billing/subscriptions/{sub}"), body: Some(Value::Object(b)) },
                _ => Call { method: "POST", path: "/users/@me/billing/subscriptions".into(), body: Some(Value::Object(b)) },
            }
        }
    }
}

pub fn redeem(sku: &str, load: &str) -> Call {
    Call { method: "POST", path: format!("/virtual-currency/skus/{sku}/redeem"), body: Some(json!({ "checkout_session_id": load })) }
}

pub enum Outcome {
    Done(Value),
    Refused(String),
    Confirm { payment: String, url: Option<String> },
    Unknown(String),
}

pub fn explain(code: u64, status: u64, server: &str) -> String {
    let fixed = match code {
        OUTDATED_PARAMETERS => "The amount due changed. Review the new total and try again.",
        100054 => "This payment method was declined by issuer. Please try a different payment method.",
        100107 => "The card was declined due to insufficient funds.",
        100002 => "An unknown payment source was specified. Select another and try again.",
        100042 => "Unable to process change due to pending payment.",
        100096 => "Already purchased the bundle.",
        100097 => "Already own at least one item from the bundle.",
        100144 => "The selected currency is not available for your country. Please try a different payment method or currency.",
        100155 => "Claim your existing purchase in-game to buy this again.",
        100150 | 100151 => "This purchase would put you over your monthly spending cap. Ask your guardian to increase it, or try again next month.",
        INSUFFICIENT_ORBS => "Not enough Orbs.",
        _ if status == 429 => "You are being rate limited. Wait a bit.",
        _ => "",
    };
    if !fixed.is_empty() {
        fixed.to_string()
    } else if !server.is_empty() {
        server.to_string()
    } else {
        "Something went wrong while processing your request, please try again!".to_string()
    }
}

/// What a charge's reply amounts to. Only a reply that says so counts as a refusal, so a dropped
/// connection or a server error is `Unknown`: the money may have moved.
pub fn outcome(res: Result<Value, String>) -> Outcome {
    let Ok(r) = res else { return Outcome::Unknown("Discord never answered.".into()) };
    let status = r["status"].as_u64().unwrap_or(0);
    let body = &r["body"];
    let code = body["code"].as_u64().unwrap_or(0);
    let server = body["message"].as_str().unwrap_or_default();
    match status {
        200..=299 => Outcome::Done(body.clone()),
        0 => Outcome::Refused(server.to_string()),
        _ if code == CONFIRMATION_REQUIRED => match body["payment_id"].as_str() {
            Some(p) => Outcome::Confirm { payment: p.to_string(), url: body["adyen_redirect_url"].as_str().map(str::to_string) },
            None => Outcome::Unknown("The payment needs confirming, but Discord gave no way to do it.".into()),
        },
        _ if code == AUTHENTICATION_REQUIRED => Outcome::Unknown("Your bank asked to confirm this payment, which can't be done in Rustcord.".into()),
        400..=499 if status != 408 => Outcome::Refused(explain(code, status, server)),
        _ => Outcome::Unknown(explain(code, status, server)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv(total: i64) -> Invoice {
        Invoice { total, subtotal: total - 100, tax: 100, currency: "usd".into(), inclusive: false }
    }

    #[test]
    fn a_one_time_purchase_carries_the_price_on_screen_and_the_chosen_source() {
        let line = Line::Sku { sku: "5101".into(), plan: None };
        let e = inv(1099);
        let c = charge(&Pay { line: &line, gift: None, source: "9501", token: None, load: "L", purchase_token: "T", expected: &e, renewal: None });
        let b = c.body.unwrap();
        assert_eq!((c.method, c.path.as_str()), ("POST", "/store/skus/5101/purchase"));
        assert_eq!((b["expected_amount"].as_i64(), b["expected_currency"].as_str(), b["payment_source_id"].as_str(), b["gift"].as_bool()), (Some(1099), Some("usd"), Some("9501"), Some(false)));
        assert!(b["gift_info_options"].is_null() && b["payment_source_token"].is_null() && b["apply_wallet_balance"] == false);
    }

    #[test]
    fn a_gift_names_its_recipient_and_plan() {
        let line = Line::Sku { sku: "521842831262875670".into(), plan: Some(NITRO_MONTH.into()) };
        let e = inv(999);
        let g = Gift { style: 4, to: "42".into(), message: "hi".into() };
        let c = charge(&Pay { line: &line, gift: Some(&g), source: "1", token: None, load: "L", purchase_token: "T", expected: &e, renewal: None });
        let b = c.body.unwrap();
        assert_eq!((b["gift"].as_bool(), b["sku_subscription_plan_id"].as_str()), (Some(true), Some(NITRO_MONTH)));
        assert_eq!((b["gift_info_options"]["recipient_id"].as_str(), b["gift_info_options"]["gift_style"].as_u64(), b["gift_info_options"]["custom_message"].as_str()), (Some("42"), Some(4), Some("hi")));
    }

    #[test]
    fn a_subscription_states_both_prices_and_boosts_on_one_go_through_a_patch() {
        let e = inv(1000);
        let r = Invoice { total: 1200, ..inv(1200) };
        let new = Line::New { plan: NITRO_MONTH.into(), qty: 1 };
        let c = charge(&Pay { line: &new, gift: None, source: "9", token: None, load: "L", purchase_token: "T", expected: &e, renewal: Some(&r) });
        let b = c.body.unwrap();
        assert_eq!((c.method, c.path.as_str()), ("POST", "/users/@me/billing/subscriptions"));
        assert_eq!((b["expected_invoice_price"]["amount"].as_i64(), b["expected_renewal_price"]["amount"].as_i64()), (Some(1000), Some(1200)));
        assert_eq!(b["items"][0]["plan_id"], NITRO_MONTH);
        let add = Line::Add { sub: "9601".into(), items: vec![(Some("77".into()), NITRO_MONTH.into(), 1), (None, BOOST_MONTH.into(), 2)] };
        let c = charge(&Pay { line: &add, gift: None, source: "9", token: None, load: "L", purchase_token: "T", expected: &e, renewal: None });
        assert_eq!((c.method, c.path.as_str()), ("PATCH", "/users/@me/billing/subscriptions/9601"));
        let b = c.body.unwrap();
        assert_eq!((b["items"][0]["id"].as_str(), b["items"][1]["quantity"].as_u64(), b["items"][1]["id"].is_null()), (Some("77"), Some(2), true));
    }

    #[test]
    fn previews_are_reads_of_the_same_purchase() {
        let line = Line::Sku { sku: "5101".into(), plan: None };
        let c = preview(&line, false, Some("9501"), "usd", "L", false);
        assert_eq!((c.method, c.path.as_str()), ("GET", "/store/skus/5101/purchase?gift=false&payment_source_id=9501&currency=usd&load_id=L"));
        let sub = Line::New { plan: BOOST_MONTH.into(), qty: 3 };
        let c = preview(&sub, false, None, "usd", "L", true);
        let b = c.body.unwrap();
        assert_eq!((c.method, b["renewal"].as_bool(), b["items"][0]["quantity"].as_u64(), b["payment_source_id"].is_null()), ("POST", Some(true), Some(3), true));
    }

    #[test]
    fn only_a_reply_that_says_so_is_a_refusal() {
        let reply = |status: u64, code: u64| Ok(json!({ "status": status, "body": { "code": code, "message": "m", "payment_id": "77" } }));
        assert!(matches!(outcome(reply(200, 0)), Outcome::Done(_)));
        assert!(matches!(outcome(reply(400, OUTDATED_PARAMETERS)), Outcome::Refused(m) if m.starts_with("The amount due changed")));
        assert!(matches!(outcome(reply(402, 100054)), Outcome::Refused(_)));
        assert!(matches!(outcome(reply(500, 0)), Outcome::Unknown(_)));
        assert!(matches!(outcome(reply(502, 0)), Outcome::Unknown(_)));
        assert!(matches!(outcome(Err("connection reset".into())), Outcome::Unknown(_)));
        assert!(matches!(outcome(reply(402, CONFIRMATION_REQUIRED)), Outcome::Confirm { .. }));
        assert!(matches!(outcome(reply(402, AUTHENTICATION_REQUIRED)), Outcome::Unknown(_)));
        assert!(matches!(outcome(reply(0, 0)), Outcome::Refused(_)));
    }

    #[test]
    fn an_invoice_reads_either_shape() {
        let a = invoice(&json!({ "total": 5, "subtotal": 4, "tax": 1, "currency": "usd" })).unwrap();
        let b = invoice(&json!({ "amount": 5, "subtotal": 4, "tax": 1, "currency": "usd" })).unwrap();
        assert_eq!(a, b);
        assert!(invoice(&json!({ "currency": "usd" })).is_none());
    }
}
