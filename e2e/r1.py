"""Checks for r1_steps in src/ui/e2e/r1.rs: scheduled messages."""

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
    made = [b for b in sent(reqs, "POST", "/users/@me/scheduled-messages") if b]
    patches = [b for b in sent(reqs, "PATCH", "/users/@me/scheduled-messages/") if b]
    return {
        "scheduled: the plus menu's Schedule Message lists its presets and Custom..., and Set Schedule put the bar over the composer": both("sched_menu_presets") and both("sched_bar"),
        "scheduled: Enter with a time set POSTed a scheduled message instead of sending it": both("sched_made")
            and any(b.get("content") == "see you later" and b.get("scheduled_timestamp") and b.get("attachments") == [] for b in made),
        "scheduled: the inbox's Scheduled tab listed it, and its buttons edited the text, moved the time and cancelled it": both("sched_listed") and both("sched_edited") and both("sched_retimed") and both("sched_cancelled")
            and any(b.get("content") == "see you later then" for b in patches) and any(b.get("scheduled_timestamp") and "content" not in b for b in patches)
            and any(r.get("method") == "DELETE" and "/users/@me/scheduled-messages/" in r.get("path", "") for r in reqs),
        "shop: the balance chip opened the Orb Wallet with its card, challenges, Quests and Shop, and Claim POSTed the challenge and took it off the list": both("wallet_opens") and both("wallet_claimed")
            and any(r.get("method") == "POST" and r.get("path", "").endswith("/users/@me/achievements/first_message/claim") for r in reqs),
        "shop: Game Shops lists the games the storefront config allows, and picking one opened its shop with pages": both("game_shops_menu") and both("game_shop_page") and both("game_shop_pages")
            and any(r.get("method") == "GET" and r.get("path", "").endswith("/partner-sdk/applications/9004/application-storefront") for r in reqs),
        "apps: a bot's full profile has Bio, Mutual Servers and Data Access tabs, and Data Access lists what its intents allow": both("bot_profile_tabs") and both("bot_data_access"),
    }
