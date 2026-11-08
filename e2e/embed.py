"""Checks for embed_steps in src/ui/e2e/embed.rs: video embeds in the mock's #embeds, a provider's page
framed over its poster once clicked, the web view following and clipped to the chat, and the proxied
file playing in the inline player. What the framed page reports comes from examples/fixtures/embed.html."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    n = len(runs)
    path = lambda r: r.get("path", "").split("?")[0]
    posts = [body(r) for r in reqs if r.get("method") == "POST" and path(r).endswith("/mock/embed")]
    said = lambda k: [p.get("v") for p in posts if p.get("k") == k]
    index = lambda pred: next((i for i, r in enumerate(reqs) if pred(r)), -1)
    poster = index(lambda r: r.get("method") == "GET" and path(r).endswith("/media/still.png"))
    page = index(lambda r: r.get("method") == "GET" and path(r).endswith("/embed/player"))
    loaded = said("loaded")
    return {
        "embeds: a video embed drew its poster and the provider's page wasn't fetched until a click": both("embed_poster") and 0 <= poster < page,
        "embeds: clicking the poster played the provider's page in the chat, with sound and no click inside it": both("embed_frame_starts")
        and len(said("playing")) >= n
        and all(v.get("muted") is False for v in said("playing"))
        and len(said("progress")) >= n,
        "embeds: the page was embedded on discord.com and could read no token or cookie of ours": len(loaded) >= n
        and all(v.get("referrer", "").startswith("https://discord.com") and not v.get("token") and not v.get("cookie") for v in loaded)
        and "mock-token" not in json.dumps(posts),
        "embeds: the page couldn't navigate the app away": len(loaded) >= n and all(v.get("top") == "SecurityError" for v in loaded),
        "embeds: the page's request for the microphone was denied": len(said("mic")) >= n and all(v == "NotAllowedError" for v in said("mic")),
        "embeds: the web view sat exactly over its embed in the chat": both("embed_frame_follows"),
        "embeds: scrolled half under the composer it was clipped to the chat's viewport": both("embed_frame_clipped_bottom"),
        "embeds: scrolled half under the header it was clipped to the chat's viewport": both("embed_frame_clipped_top"),
        "embeds: scrolled just out of view it was hidden, and far out of view it was gone": both("embed_frame_hidden_off_screen") and both("embed_frame_gone_scrolled"),
        "embeds: out of view it paused, and came back playing when scrolled to": both("embed_frame_back_in_view") and len(said("paused")) >= n and len(said("resumed")) >= n,
        "embeds: playing another one replaced the first, only one at a time": both("embed_one_at_a_time"),
        "embeds: a dialog hid it and closing the dialog showed it again": both("embed_frame_hides_for_dialog") and both("embed_frame_returns_after_dialog"),
        "embeds: switching channel took it away": both("embed_frame_gone_channel"),
        "embeds: a file Discord proxies drew in the inline player and played, taking over from the framed one": both("embed_direct_player")
        and both("embed_direct_plays")
        and both("embed_inline_stops_frame"),
    }
