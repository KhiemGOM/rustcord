"""Checks for home_steps in src/ui/e2e.rs: the rows over a server's channels and the pages behind
them, a voice channel's chat and settings, voice mute icons, perk cards, the profile card's box
and the viewer's zoom."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    searches = [body(r) for r in sent("POST", "/guilds/1001/members-search")]
    hangout = [body(r) for r in sent("PATCH", "/channels/2102")]
    browse = [body(r) for r in sent("PATCH", "/users/@me/guilds/1001/settings")]
    return {
        "home: voice rows show muted, deafened and server muted": both("home_voice_states"),
        "home: a hovered perk card shows its button": both("home_perk_hover"),
        "home: a community server we own shows Events, Channels & Roles, Members and Server Boosts": both("home_rows"),
        "home: Members lists the server, and typing in its box searches by username": both("home_members_search")
            and any(s.get("and_query", {}).get("usernames", {}).get("or_query") == ["ali"] for s in searches)
            and any(not s.get("and_query") for s in searches),
        "home: picking an onboarding answer saved it": both("home_onboarding")
            and any("7102" in body(r).get("onboarding_responses", []) for r in sent("PUT", "/guilds/1001/onboarding-responses")),
        "home: Browse Channels put a channel on the list": both("home_browse")
            # two from the unlisted channel's Add to Channel List earlier, two from here
            and sum(bool(b.get("channel_overrides", {}).get("2101", {}).get("flags", 0) & 4096) for b in browse) >= 4,
        "home: Open Chat opened the voice channel's chat beside the call, and a message went there": both("home_voice_chat")
            and sum(body(r).get("content") == "hello vc" for r in sent("POST", "/channels/2102/messages")) == 2,
        "home: Edit Channel saved a new name, then View Channel denied to everyone": both("home_chanset_save") and both("home_chanset_perms")
            and sum(b.get("name") == "Hangout 2" for b in hangout) == 2
            and sum(any(o.get("id") == "1001" and int(o.get("deny", "0")) & (1 << 10) for o in b.get("permission_overwrites", [])) for b in hangout) == 2,
        "home: Delete Channel asked, then deleted it": both("home_chanset_delete") and len(sent("DELETE", "/channels/2102")) == 2,
        "home: a profile card pushed up to fit kept a click in it, and its box sent a dm": both("home_profile_stays") and both("home_profile_dm")
            and sum(body(r).get("content") == "hey from profile" for r in sent("POST", "/channels/3000/messages")) == 2,
        "home: a click zoomed the viewer's picture and another zoomed it back": both("home_zoom") and both("home_unzoom"),
    }
