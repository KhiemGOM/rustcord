"""Checks for the menu and dialog steps (3800 on in src/ui/e2e/menus.rs): what each did on screen
is a mark, what it sent is read from the mock's request log."""

import json

import protowire


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    body = lambda r: json.loads(r.get("body") or "{}")
    prefs = [body(r)["guilds"]["1000"] for r in sent("PATCH", "/users/@me/guilds/settings") if "1000" in body(r).get("guilds", {})]
    has = lambda key, val: any(p.get(key) == val for p in prefs)
    searches = [r.get("path", "") for r in sent("GET", "/guilds/1000/messages/search")]
    installs = [body(r) for r in sent("POST", "/oauth2/authorize") if "client_id=9101" in r.get("path", "")]
    threads = [body(r) for r in sent("POST", "/channels/2100/threads")]
    statuses = protowire.status_writes(reqs)
    return {
        "menus: Log Out asked first and Escape kept the session": both("logout_asks_first") and both("logout_cancel_stays_in"),
        "menus: the Ctrl+/ sheet sat inside the window, at the default size and a short one": both("combos_fits") and both("combos_fits_short"),
        "menus: a user menu opened near the bottom edge still showed Block": both("tall_menu_shows_block"),
        "menus: Notification Settings sent suppress everyone, roles, highlights, events, mobile push and mute": both("notifs_done_closes")
            and has("suppress_everyone", True) and has("suppress_roles", True) and has("notify_highlights", 2)
            and has("mute_scheduled_events", True) and has("mobile_push", False) and has("muted", True),
        "menus: Add Friend took focus, said no one was found, then that the request was sent": both("add_friend_autofocus") and both("add_friend_error_line") and both("add_friend_success_line"),
        "menus: the server guide page opened and its step opened the channel": both("server_guide_page") and both("server_guide_opens_channel"),
        "menus: Discovery is a page with its requirements": both("discovery_page"),
        "menus: Add App picked a server and authorized it in the app": both("add_app_in_app")
            and any(b.get("guild_id") == "1000" and b.get("authorize") is True for b in installs),
        "menus: a private thread went out as type 12": any(t.get("type") == 12 for t in threads),
        "menus: the rail tooltip sat beside its icon": both("rail_tip_beside_icon"),
        "menus: the title bar said Discover and Quests": both("title_says_discover") and both("title_says_quests"),
        "menus: streamer mode kept the note out of the profile popout": both("streamer_hides_note"),
        "menus: a search with pinned, before and has went out as ids and params": any("pinned=true" in p and "max_id=" in p and "has=image" in p for p in searches),
        "menus: a timed status sent its expiry": any(s.get("status") == "idle" and s.get("expires") and s.get("created") for s in statuses),
        "menus: Create Your Server and Notification Settings fit a short window": both("new_server_fits_short") and both("notifs_fits_short"),
    }
