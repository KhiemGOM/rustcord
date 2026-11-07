"""Checks for p5_steps and p5b_steps in src/ui/e2e/p5.rs and p5b.rs: the controls nobody had clicked and
the leftovers list."""

import json


def sent(reqs, method, path_has):
    for r in reqs:
        if r.get("method") == method and path_has in r.get("path", ""):
            try:
                yield json.loads(r.get("body") or "null")
            except ValueError:
                pass


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    offset_pages = [r for r in reqs if "/shop/search" in r.get("path", "") and "offset=0" not in r.get("path", "")]
    return {
        "p5: Watch Stream on Active Now's stream row joined the streamer's voice channel and started watching": both("active_now_watch_stream"),
        "p5: a pending join request's More Options menu lists Open in Mod View, Ban User and Copy User ID, and Ban User and Open in Mod View open theirs": both("joins_more_menu") and both("joins_more_ban") and both("joins_more_mod_view"),
        "p5: picking Music in the discovery category dropdown and saving PATCHed primary_category_id 2": both("discovery_category_saved")
            and any(b and b.get("primary_category_id") == 2 for b in sent(reqs, "PATCH", "/discovery-metadata")),
        "p5: clicking a selected top-games tile saved the profile without that game, and clicking it again saved it back": both("server_games_tile_off") and both("server_games_tile")
            and any(b and b.get("game_application_ids") == [] for b in sent(reqs, "PATCH", "/guilds/1001/profile"))
            and any(b and b.get("game_application_ids") == ["7100"] for b in sent(reqs, "PATCH", "/guilds/1001/profile")),
        "p5: an unclosed group in the AutoMod regex field was checked against /rules/validate, showed its error, and the error went when closed": both("automod_regex_invalid") and both("automod_regex_valid")
            and any(b and "(" in ((b.get("trigger_metadata") or {}).get("regex_patterns") or []) for b in sent(reqs, "POST", "/auto-moderation/rules/validate")),
        "p5: the Shop pager's Next and numbered pages moved through thirty frames (a /shop/search with a later offset)": both("shop_pager_next") and both("shop_pager_number") and bool(offset_pages),
        "p5: Subscribe to Nitro Basic opened the checkout": both("nitro_basic_click"),
    }
