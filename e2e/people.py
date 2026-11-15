"""Checks for the profile family, driven by the 5000 steps in src/ui/e2e.rs: Active Now's cards,
the popout's Mod View, role add and remove, the full profile, the server tag and role popouts."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    return {
        "people: Active Now drew alice's song, game and voice rows": both("active_now_cards"),
        "people: Mod View opened from the popout and asked for their messages": both("modview") and len(sent("GET", "/guilds/1000/messages/search?author_id=101")) >= 2,
        "people: the + in Mod View added a role and its x took it away": both("role_added") and both("role_removed")
            and len(sent("PUT", "/guilds/1000/members/101/roles/1102")) == 2 and len(sent("DELETE", "/guilds/1000/members/101/roles/1102")) == 2,
        "people: the full profile opened on Activity with its recent entries and switched to Mutual Friends": both("full_profile") and both("full_profile_tabs")
            and len(sent("GET", "/content-inventory/users/101/outbox")) >= 2,
        "people: a server tag's popout fetched the server's profile": both("tag_popout") and len(sent("GET", "/guilds/1001/profile")) >= 2,
        "people: a role mention's popout opened": both("role_popout"),
    }
