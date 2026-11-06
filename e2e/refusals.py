"""Checks for refusal_steps in src/ui/e2e/refusals.rs: what the client sends for the writes the
mock now validates, and what it shows when discord refuses one."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        refused = [r for r in reqs if r.get("refused") and not (r.get("path", "").endswith("/guilds/1001/profile") and body(r).get("tag") == "A")]
    return {
        "refusals: the mock refused no write the client sent, bar the one-letter tag": not refused,}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    tags = [body(r) for r in sent("PATCH", "/guilds/1001/profile")]
    pauses = [body(r) for r in sent("PUT", "/guilds/1001/incident-actions")]
    return {
        "refusals: a one-letter Server Tag was refused, the toast and the bar said why, and Reset cleared it": both("refused_tag") and both("refused_reset")
            and any(b.get("tag") == "A" and set(b) == {"tag", "badge", "badge_color_primary", "badge_color_secondary"} for b in tags),
        "refusals: Pause Invites PUT the lockdown to incident-actions and Resume cleared it": both("invites_paused") and both("invites_resumed")
            and sum(bool(b.get("invites_disabled_until")) and b.get("lockdown_duration_hours") == 24 for b in pauses) >= 2
            and sum(b.get("invites_disabled_until") is None for b in pauses) >= 2,
    }
