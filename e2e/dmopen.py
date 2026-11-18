"""Checks for dmopen_steps in src/ui/e2e/dmopen.rs: the start of a DM."""


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    blocked = [r for r in reqs if r.get("method") == "PUT" and "/users/@me/relationships/101" in r.get("path", "") and '"type":2' in (r.get("body") or "").replace(" ", "")]
    return {
        "dm start: the servers you share, then the friend button and Block 7 apart, on one row of 32px buttons": both("dm_start_row"),
        "dm start: Block asks before it blocks, and backing out sends nothing": both("dm_start_block_asks") and not blocked,
        "dm header: the person's name is in their display name font": both("dm_header_name_font"),
        "group dm: a member's nameplate and avatar decoration show in the member list": both("group_member_cosmetics"),
    }
