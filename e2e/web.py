"""Checks for web_steps in src/ui/e2e/web.rs: the checkout window opens on a standalone billing page
signed in, moves to a second one, closes on Escape and takes its temporary profile with it; and
nothing in a run ever asked the mock for the web app's own pages."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def pages(reqs):
    return [body(r) for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/mock/webwin")]


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    seen = pages(reqs)
    signed = lambda prefix: sum(s.get("page", "").startswith(prefix) and s.get("token") == '"mock-token"' for s in seen)
    app_pages = [r.get("path") for r in reqs if r.get("method") == "GET" and r.get("path", "").split("?")[0].startswith(("/store", "/shop", "/channels", "/settings"))]
    return {
        "web: a purchase opened its billing page in the checkout window": both("web_open"),
        "web: the page ran signed in, with the token in place before its scripts": signed("/billing/premium/subscribe?plan_id=511651880837840896&gift=false") == len(runs),
        "web: a second purchase moved the open window to its page, still signed in": both("web_navigate") and signed("/billing/guild-subscriptions/purchase?guild_id=1000") == len(runs),
        "web: Escape closed the checkout window": both("web_closed") and both("web_escape"),
        "web: the window's temporary profile folder existed while open and was deleted on close": both("web_profile_made") and both("web_profile_gone"),
        "web: no run asked for the web app's own pages (/store, /shop, /channels, /settings)": not app_pages,
    }
