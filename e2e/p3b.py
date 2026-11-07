"""Checks for p3b_steps in src/ui/e2e/p3b.rs: the settings rows that do something, billing address editing
and the profile leftovers."""

import base64
import json


def blobs(reqs):
    out = []
    for r in reqs:
        if r.get("method") == "PATCH" and r.get("path", "").endswith("/settings-proto/1"):
            try:
                out.append(base64.b64decode(json.loads(r.get("body") or "{}").get("settings", "")))
            except ValueError:
                pass
    return out


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "p3b: the shop's pointer was the hand over a tab and the arrow over bare shop, and the window answered with Discord's pictures": both("shop_cursor_hand") and both("shop_cursor_hand_applied") and both("shop_cursor_arrow") and both("shop_cursor_arrow_applied"),
        "p3b: turning Enable custom cursor in shop off gave the pointer back": both("shop_cursor_off"),
    }
