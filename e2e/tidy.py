"""Checks for tidy_steps in src/ui/e2e/tidy.rs: a video embed's page going fullscreen through a real
click in it (examples/fixtures/embed.html has the button) and giving the window back, and the message
toolbar showing over the embed's native window."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    path = lambda r: r.get("path", "").split("?")[0]
    posts = [body(r) for r in reqs if r.get("method") == "POST" and path(r).endswith("/mock/embed")]
    full = [p.get("v") for p in posts if p.get("k") == "fullscreen"]
    return {
        "embed fullscreen: the page's own button went fullscreen and its window covered the whole app": both("embed_fullscreen_covers_window")
        and any(isinstance(v, dict) and v.get("on") and v.get("w", 0) > 700 for v in full),
        "embed fullscreen: leaving it put the window back over its embed": both("embed_fullscreen_exit_returns"),
        "embeds at 150% zoom: the window sat over its embed in physical pixels, covered the app in fullscreen, and came back": both("embed_frame_follows_150")
        and both("embed_fullscreen_covers_window_150")
        and both("embed_fullscreen_exit_returns_150"),
        "member list: at 940 wide the list stayed beside the chat": both("member_list_at_940"),
    }
