"""Checks for rv_steps in src/ui/e2e/rv.rs: a tester's reports, each through the real control."""

import json


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    gets = [r.get("path", "") for r in reqs if r.get("method") == "GET"]
    refused = [r.get("cdn_refused", "") for r in reqs if "cdn_refused" in r]
    emojis = []
    for r in reqs:
        if r.get("method") == "POST" and r.get("path", "").endswith("/emojis"):
            try:
                emojis.append(json.loads(r.get("body") or "null") or {})
            except ValueError:
                pass
    return {
        "profiles: clicking a member's name again closes the profile it opened instead of replaying it": both("rv_popout_open") and both("rv_popout_closed_by_its_name"),
        "profiles: your own profile is asked for without mutual friends or servers": any(p.startswith("/api/v9/users/100/profile?with_mutual_guilds=false&with_mutual_friends=false") for p in gets),
        "composer: a long draft wraps before the gift button": both("rv_composer_clear_of_buttons"),
        "dms: a pinned dm carries the pin at the row's end": both("rv_dm_pin"),
        "quests: the banner comes from the path in the quest's assets, and no quest art was asked for at a doubled path": any(p.startswith("/cdn/quests/9001/hero.png") for p in gets)
            and not any("/quests/" in p for p in refused),
        "channels: a voice channel left off a picked list shows under Active Now with someone in it, and Show All opens the rest": both("rv_voice_toggle") and both("rv_active_now_lists_busy") and both("rv_voice_shown_all"),
        "voice: hovering someone live fetches their stream preview and shows it": both("rv_stream_tip")
            and any(p.startswith("/api/v9/streams/guild:") and "/preview?version=" in p for p in gets),
        "emoji studio: Add Emoji in a dm opens the studio, and picking a server uploads it there": both("rv_studio_open_in_dm") and both("rv_studio_uploaded")
            and any(b.get("name") == "studio_pick" and str(b.get("image", "")).startswith("data:image/png;base64,") for b in emojis),
    }
