"""Checks for buy_steps in src/ui/e2e/buy.rs: the purchase dialog sends nothing until Purchase is
clicked, sends exactly one charge per click, carries the price on screen, refuses a changed price,
a missing payment source and a read-only run, and says so when it can't tell what happened.
The mock logs every charge it decides on (`charge`: accepted, refused:<code>, confirm, boom)."""


import web

NITRO_SKU = "521842831262875670"


def is_charge(r):
    p = r.get("path", "").split("?")[0]
    m = r.get("method")
    return (m == "POST" and (p.endswith("/purchase") or p.endswith("/redeem") or p.endswith("/billing/subscriptions"))) or (
        m == "PATCH" and "/billing/subscriptions/" in p and not p.endswith("/preview"))


def of_the_buy_group(r):
    b = r.get("body") or ""
    return '"load_id"' in b or '"checkout_session_id"' in b or "/mock/buy" in r.get("path", "")


def checks(runs, reqs, data):
    n = len(runs)
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    charges = [r for r in reqs if is_charge(r) and r.get("method") and "/mock/" not in r.get("path", "")]
    posts = lambda suffix: [r for r in charges if r.get("path", "").split("?")[0].endswith(suffix)]
    logged = lambda what: sum(r.get("charge") == what for r in reqs)
    refused = sum(str(r.get("charge", "")).startswith("refused") for r in reqs)
    sku = lambda s: posts(f"/store/skus/{s}/purchase")
    gift_to = lambda s, to: [r for r in sku(s) if web.body(r).get("gift") is True and web.body(r).get("gift_info_options", {}).get("recipient_id") == to]
    money = [web.body(r) for r in charges if "expected_amount" in web.body(r)]
    subs = [web.body(r) for r in posts("/billing/subscriptions")]
    boost = [r for r in charges if r.get("method") == "PATCH"]
    boost_body = web.body(boost[0]) if boost else {}
    card_keys = ("number", "cvc", "card_number", "security_code", "exp_month")
    return {
        "buy: Enter, Cancel, Escape, a disabled Purchase and a short Orbs balance each reached the server as no charge": all(
            both(m) for m in ("buy_baseline", "buy_enter_inert", "buy_cancel", "buy_escape", "buy_no_charge_on_cancel_or_escape", "buy_no_source_no_charge", "buy_orbs_short_no_charge")),
        "buy: a double click on Purchase made one charge, and the dialog showed Done": both("buy_one_charge_per_double_click") and both("buy_done") and len(sku("5101")) == n,
        "buy: every card charge sent the total and currency from its preview, the chosen source and a purchase token": bool(money) and all(
            m["expected_amount"] > 0 and m["expected_currency"] == "vnd" and m.get("payment_source_id") and len(m.get("purchase_token", "")) >= 16 and m.get("load_id") for m in money),
        "buy: nothing in a purchase body is card data or the account token": not any(k in r.get("body", "") for r in charges for k in card_keys + ('"mock-token"',)),
        "buy: a price that moved was refused by the server, said so, and the next charge needed a new click": both("buy_price_change_refused_without_charge") and both("buy_price_message") and both("buy_new_click_after_refusal") and len(sku("5102")) == 2 * n,
        "buy: with no saved payment method the dialog offered Add Payment Method and did not charge": both("buy_no_source_offers_add_card"),
        "buy: Orbs: a short balance blocked the button; with enough, one redeem went out with the checkout session": both("buy_orbs_short") and both("buy_orbs_done") and len(posts("/virtual-currency/skus/5203/redeem")) == n,
        "buy: a gift of a shop item named the friend, the style and the message": both("buy_gift_item_done") and len(gift_to("5201", "101")) == n and all(
            web.body(r)["gift_info_options"].get("gift_style") == 5 and web.body(r)["gift_info_options"].get("custom_message") == "Happy birthday" for r in gift_to("5201", "101")),
        "buy: gifting Nitro named the plan and the friend": both("buy_nitro_gift_done") and len(gift_to(NITRO_SKU, "102")) == n and all(
            web.body(r).get("sku_subscription_plan_id") == "511651880837840896" for r in gift_to(NITRO_SKU, "102")),
        "buy: subscribing to Nitro sent the invoice and renewal prices from the previews": both("buy_nitro_subscribe_done") and len(subs) == n and all(
            s.get("expected_invoice_price", {}).get("amount") == 124300 and s.get("expected_renewal_price", {}).get("amount") == 124300 and s["items"][0]["plan_id"] == "511651880837840896" for s in subs),
        "buy: Boosts went through the subscription and were then applied to the server": both("buy_boost_done") and both("buy_boost_applied") and len(boost) == n and boost_body.get("items", [{}])[-1].get("quantity") == 2 and any(
            r.get("method") == "PUT" and r.get("path", "").endswith("/guilds/1002/premium/subscriptions") for r in reqs),
        "buy: the Server Shop row opened a page of the published products, and a card bought one": both("server_shop_page") and both("buy_server_shop_done") and len(sku("6101")) == n and not sku("6103"),
        "buy: a payment that needed confirming opened the provider's page in the window, polled it, and was charged once": both("buy_confirmation_window") and both("buy_confirmed_payment_one_charge") and logged("confirm") == n and any(
            r.get("method") == "GET" and r.get("path", "").endswith("/billing/payments/7001") for r in reqs) and len(sku("5202")) == n,
        "buy: a 500 was not retried and the dialog said to check Billing": both("buy_unknown_result_sent_once") and both("buy_unknown_says_check_billing") and logged("boom") == n and len(sku("5103")) == n,
        "buy: a declined card ended with nothing charged and the way back to Review": both("buy_declined_no_charge"),
        "buy: a read-only run refused before sending, and a charge sent past the dialog was dropped by the net layer": both("buy_read_only_refuses_before_sending") and not any("probe" in r.get("body", "") for r in charges),
        "buy: in the EEA the withdrawal waiver was needed before Purchase": both("buy_eea_waiver_required"),
        "buy: at 940x500 the Purchase button stayed inside the window": both("buy_small_window_reaches_purchase"),
        "buy: the mock accepted exactly the purchases that were clicked: 8 charges and 1 confirmed payment per run": logged("accepted") == 8 * n and refused == 2 * n,
    }
