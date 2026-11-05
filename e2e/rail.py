"""Checks for the rail and channel list steps in src/ui/e2e.rs: pinned channels and the server
preview bar."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    pins = [body(r)["channel_overrides"]["2302"]["flags"] for r in sent("PATCH", "/users/@me/guilds/1003/settings") if "2302" in body(r).get("channel_overrides", {})]
    return {
        "rail: Pin Channel to Top moved a channel into Pinned Channels, and unpinning put it back": both("side_pinned") and both("side_unpinned")
            and pins.count(2048) >= 2 and pins.count(0) >= 2,
        "rail: a link into a server you're not in previewed it with a Join Server bar, and Join Server joined": both("lurk_preview") and both("lurk_join")
            and len(sent("PUT", "/guilds/9000/members/@me?lurker=true")) >= 2 and len(sent("PUT", "/guilds/9000/members/@me?lurker=false")) >= 2,
    }
