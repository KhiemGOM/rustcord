"""Checks for p4b_steps in src/ui/e2e/p4b.rs: account settings written on change, the chat header and
composer at narrow widths, and @game."""

import base64
import json

import protowire


def written(reqs):
    out = set()
    for r in reqs:
        if r.get("method") == "PATCH" and r.get("path", "").endswith("/settings-proto/1"):
            try:
                blob = base64.b64decode(json.loads(r.get("body") or "{}").get("settings", ""))
            except ValueError:
                continue
            for group, body in protowire.fields(blob):
                out.update((group, f) for f, _ in protowire.fields(body))
    return out


SETTINGS = {
    (6, 7): "image descriptions",
    (6, 15): "sticker animation",
    (13, 12): "UI density",
    (6, 36): "search across all DMs",
    (5, 5): "always preview video",
    (5, 1): "video background blur",
    (13, 9): "hour cycle",
    (7, 12): "friends come online",
}


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    rect = lambda r, k: r.get("rects", {}).get(k)

    def clear(r, tag):
        name, search = rect(r, f"p4b_{tag}_header:name"), rect(r, f"p4b_{tag}_search")
        return bool(name and search) and name[0] + name[2] <= search[0] + 0.5

    def room(r, tag):
        box, strip = rect(r, f"p4b_{tag}_composer"), rect(r, f"p4b_{tag}_composer:buttons")
        return bool(box and strip) and strip[0] >= box[0] + 55

    sent = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/messages") and "<@$7100> is the best" in r.get("body", "")]
    asked = [r for r in reqs if "/games/autocomplete" in r.get("path", "")]
    tags = ("plain", "themes", "search", "wide", "wide_search")
    return {
        "p4b: every setting a click changed was PATCHed to the settings blob in Discord's field: " + ", ".join(SETTINGS.values()): both("p4b_changed") and set(SETTINGS) <= written(reqs),
        "p4b: clicking each one back restored it": both("p4b_restored"),
        "p4b: the header's name never ran under the search box at 940x500@150 or wide, with and without a theme or search panel": both("p4b_chat_column")
            and all(clear(r, t) for r in runs.values() for t in tags if rect(r, f"p4b_{t}_search")),
        "p4b: the composer's buttons never ran into its text at 940x500@150, and folded the gif, sticker and emoji buttons into one": all(
            room(r, t) and (rect(r, f"p4b_{t}_composer:buttons") or [0, 0, 0])[2] == (200 if t.startswith("wide") else 120) for r in runs.values() for t in tags),
        "p4b: @game listed Mention a game, asked /games/autocomplete once for the query, and sent <@$id>": both("at_game_row") and both("at_game_results") and len(sent) == 2
            and len(asked) == 2 and all("q=mock&filter_group=default" in r["path"] for r in asked),
        "p4b: with Show games in autocomplete results off, @gam offered nothing": both("at_game_gated"),
    }
