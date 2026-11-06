"""Checks for live_steps in src/ui/e2e/live.rs: gateway events reaching the pages that are open,
and user actions the mock refuses saying why and putting back what the click changed."""


def checks(runs, reqs, data):
    both = lambda *names: all(n in r.get("marks_ms", {}) for r in runs.values() for n in names)
    n = len(runs)
    path = lambda r: r.get("path", "").split("?")[0]
    count = lambda method, tail: sum(1 for r in reqs if r.get("method") == method and path(r).endswith(tail))
    return {
        "live: a ban made elsewhere appeared on the open Bans page and its revoke took it off": both("live_ban_added", "live_ban_removed"),
        "live: an integrations or webhooks event made the open Integrations page ask again": both("live_integrations_asked")
        and count("GET", "/guilds/1000/integrations") >= 2 * n
        and count("GET", "/guilds/1000/webhooks") >= 2 * n,
        "live: a sound uploaded elsewhere showed on the Soundboard page, and renamed or deleted, in the call's popout": both("live_sound_added", "live_popout_renamed", "live_popout_removed"),
        "live: a member leaving dropped their row from the open Members table, one joining counted and asked for the table again": both("live_member_removed", "live_member_added")
        and count("POST", "/guilds/1000/members-search") >= 2 * n,
        "live: people added to and removed from a group dm changed who it lists": both("live_recipient_added", "live_recipient_removed"),
        "live: a bookmark, a note and a status set on another device landed here": both("live_account_events", "live_bookmark_removed"),
        "failed: a refused delete said why and put the message back": both("fail_delete_rolled_back"),
        "failed: a refused pin was unpinned again and said so in discord's words": both("fail_pin_rolled_back"),
        "failed: a refused edit put the old text back and showed the API's field error": both("fail_edit_rolled_back", "fail_field_error"),
        "failed: a refused reaction was taken back off": both("fail_react_rolled_back"),
        "failed: a refused kick named the member, with the API's message behind a shift click": both("fail_kick_toast", "fail_kick_reason"),
        "failed: a 429 said it was rate limited without waiting out the retry": both("fail_ratelimit_text"),
        "failed: a 502 said the servers are having issues": both("fail_5xx_text"),
        "failed: a channel whose messages wouldn't load said so and Try Again loaded them": both("messages_failed_state", "messages_retry_loaded"),
    }
