"""Checks for r1_steps in src/ui/e2e/r1.rs: scheduled messages."""

import json


def sent(reqs, method, path_has):
    for r in reqs:
        if r.get("method") == method and path_has in r.get("path", ""):
            try:
                yield json.loads(r.get("body") or "null")
            except ValueError:
                pass


def dark(run, spot):
    h = run.get("rects", {}).get(f"look:dark_sidebar:{spot}", "#ffffff").lstrip("#")
    return sum(int(h[i:i + 2], 16) for i in (0, 2, 4)) < 240


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
        "hub: a managed server in the experiment shows Server Hub over Events, its page welcomes you, and Edit added a widget and published it": both("hub_row") and both("hub_welcome") and both("hub_widget_added") and both("hub_published")
            and any(b and b.get("header") == {} and [w.get("type") for w in b.get("widgets", [])] == ["whiteboard"] and b["widgets"][0].get("position") == {"column": 0, "order": 0} for b in sent(reqs, "PUT", "/guilds/1001/space"))
            and any(b == {"publish_status": 1} for b in sent(reqs, "PATCH", "/guilds/1001/space/settings")),
        "student hubs: Discover's Student Hubs tab opened the join dialog, a school email listed its hubs, and the mailed code joined the one picked": both("hub_tab") and both("hub_schools") and both("hub_code_asked") and both("hub_joined")
            and any(b == {"email": "sam@mock.edu", "guild_id": None, "allow_multiple_guilds": True, "use_verification_code": True} for b in sent(reqs, "POST", "/guilds/automations/email-domain-lookup"))
            and any(b == {"code": "246810", "guild_id": "1003", "email": "sam@mock.edu"} for b in sent(reqs, "POST", "/guilds/automations/email-domain-lookup/verify-code")),
        "student hubs: an email with no hub led to the waitlist, and Join Waitlist signed the school up": both("hub_waitlisted")
            and any(b == {"email": "sam@nowhere.example", "school": "Nowhere College"} for b in sent(reqs, "POST", "/hub-waitlist/signup")),
        "nitro: Select Friends listed who can take a trial, and Share Nitro sent one to each friend picked": both("nitro_friends") and both("nitro_shared")
            and any(b == {"index": 0, "limit": 10, "search_query": ""} for b in sent(reqs, "POST", "/users/@me/referrals/eligible-users"))
            and all(any(r.get("method") == "POST" and r.get("path", "").endswith(f"/users/@me/referrals/{u}") for r in reqs) for u in (101, 102)),
        "themes: Use Server Theme in a themed server's menu switched its theme off there": both("server_theme_off_here"),
        "themes: Dark sidebar in the light theme darkened the rail and channel list and left the chat light": both("dark_sidebar_on")
            and all(dark(r, "rail") and dark(r, "side") and not dark(r, "chat") for r in runs.values()),
        "themes: Share your theme sent the custom theme to a channel, and its card's Preview opened it in the editor": both("theme_shared") and both("theme_previewed")
            and any(b and len((b.get("shared_client_theme") or {}).get("colors", [])) >= 1 and "base_theme" in b["shared_client_theme"] for b in sent(reqs, "POST", "/messages")),
        "announcements: Publish asks first, saying how many servers follow the channel, and Confirm crossposts": both("publish_asks") and both("publish_confirmed")
            and any(r.get("method") == "GET" and r.get("path", "").endswith("/channels/2002/follower-stats") for r in reqs)
            and any(r.get("method") == "POST" and r.get("path", "").endswith("/channels/2002/messages/2/crosspost") for r in reqs),
        "tags: the tag popout of a server you're in offers Adopt Tag, which puts the tag on": both("tag_adopted")
            and any(b == {"identity_guild_id": "1001", "identity_enabled": True} for b in sent(reqs, "PUT", "/users/@me/clan")),
        "dms: a DM's header shows the other person's status on their avatar and what they are playing": both("dm_header_activity"),
        "apps: a bot's full profile has Bio, Mutual Servers and Data Access tabs, and Data Access lists what its intents allow": both("bot_profile_tabs") and both("bot_data_access"),
    }
