"""Checks for web_steps in src/ui/e2e/web.rs: the checkout window opens on the page asked for,
is signed in before the page's scripts run, moves to a second page and closes."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    seen = [body(r) for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/mock/webwin")]
    signed = lambda page: sum(s.get("page") == page and s.get("token") == '"mock-token"' for s in seen)
    return {
        "web: a purchase opened its page in the checkout window": both("web_open"),
        "web: the page ran signed in, with the token in place before its scripts": signed("/store") == len(runs),
        "web: a second purchase moved the open window to its page, still signed in": both("web_navigate") and signed("/shop") == len(runs),
        "web: the checkout window closed": both("web_closed"),
    }
