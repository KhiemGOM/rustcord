"""Checks for fixes_steps in src/ui/e2e/fixes.rs: what a tester found on a real account that the
mock checks passed over."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    settings = [body(r)["guilds"]["1001"] for r in sent("PATCH", "/users/@me/guilds/settings") if "1001" in body(r).get("guilds", {})]
    fresh = [
        b for b in settings
        if b.get("flags", 0) & (1 << 14) and len(b.get("channel_overrides", {})) >= 2 and "2101" not in b["channel_overrides"]
        and all(o.get("flags", 0) & 4096 for o in b["channel_overrides"].values())
    ]
    stickers = [body(r) for r in sent("POST", "/channels/") if "sticker_ids" in body(r)]
    return {
        "fixes: the close DM X stayed drawn for 20 frames under a steady pointer, and a click on it closed the DM": both("fixes_dm_x_stable")
            and both("fixes_dm_x_click") and len(sent("DELETE", "/channels/3002")) == 2,
        "fixes: Browse Channels on a server that lists everything switched it to opt-in with every channel but the unticked one": both("fixes_browse_fresh")
            and len(fresh) == 2,
        "fixes: the Browse Channels button opened the channel without toggling it": both("fixes_browse_open"),
        "fixes: the upload card's eye, pencil and trash worked from their top edge": both("fixes_card_eye") and both("fixes_card_pencil") and both("fixes_card_trash"),
        "fixes: discord's sticker packs listed in the picker, and a pack sticker sent": both("fixes_sticker_packs") and both("fixes_sticker_closed")
            and sum(b.get("sticker_ids") == ["8102"] for b in stickers) == 2,
        "fixes: the member list's activity gear offers About and Hide, hiding drops the cards and the header brings them back": both("fixes_feed_menu")
            and both("fixes_feed_hidden") and both("fixes_feed_shown"),
        "fixes: About Recent Activity opened and closed": both("fixes_feed_about") and both("fixes_feed_about_closed"),
    }
