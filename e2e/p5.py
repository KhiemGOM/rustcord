"""Checks for p5_steps in src/ui/e2e/p5.rs: the Active Now stream row and the join requests menu."""


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "p5: Watch Stream on Active Now's stream row joined the streamer's voice channel and started watching": both("active_now_watch_stream"),
        "p5: a pending join request's More Options menu lists Open in Mod View, Ban User and Copy User ID, and Ban User and Open in Mod View open theirs": both("joins_more_menu") and both("joins_more_ban") and both("joins_more_mod_view"),
    }
