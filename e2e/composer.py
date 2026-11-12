"""Checks for the composer's states, plus menu, polls, launcher, upload cards and the group DM
picker, driven by composer_steps in src/ui/e2e/composer.rs."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    polls = [body(r)["poll"] for r in sent("POST", "/channels/2011/messages") if "poll" in body(r)]
    return {
        "composer: the plus menu lists Upload a File, Create Thread, Create Poll and Use Apps": both("composer_plus_menu"),
        "composer: Create Poll posted its question, answers, duration and multiselect": both("composer_poll") and len(polls) >= 2
            and all(p["question"]["text"] == "which crate?" and [a["poll_media"]["text"] for a in p["answers"]] == ["serde", "tokio"] and p["duration"] == 24 and p["allow_multiselect"] is False for p in polls),
        "composer: slowmode showed its countdown and held the second message back": both("composer_slowmode") and both("composer_slow_blocked")
            and len([r for r in sent("POST", "/channels/2015/messages")]) == 2,
        "composer: a channel you can't post in disabled the box": both("composer_no_perm"),
        "composer: an announcement channel showed Follow, and the modal posted to followers": both("composer_follow_bar") and both("composer_follow_modal")
            and all((r.get("rects", {}).get("composer_follow_bar_h") or 999) < 60 for r in runs.values())
            and sum(body(r).get("webhook_channel_id") == "2100" for r in sent("POST", "/channels/2002/followers")) == 4,
        "composer: a locked thread was disabled and an archived one stayed open": both("composer_locked_thread") and both("composer_archived_thread"),
        "composer: a timeout disabled the box and counted down": both("composer_timeout"),
        "composer: a blocked dm swapped the box for the Unblock bar, and Unblock asked the mock": both("composer_blocked_dm")
            and len(sent("DELETE", "/users/@me/relationships/101")) >= 2,
        "composer: a refused send's notice shows and dismisses": both("composer_notice") and both("composer_notice_dismissed"),
        "composer: an upload card took E for Modify Attachment, saved the alt text, and took D": both("composer_modify") and both("composer_modify_save") and both("composer_card_d"),
        "composer: the Apps button opened the launcher and Escape closed it": both("composer_launcher") and both("composer_launcher_esc"),
        "composer: clicking the topic opened it": both("composer_topic"),
        "composer: a dm's header has Start Video Call and Add Friends to DM": both("composer_dm_header"),
        "composer: the friend picker made a group dm of three": both("composer_group_pick") and both("composer_group_dm")
            and sum(len(body(r).get("recipients", [])) == 3 for r in sent("POST", "/users/@me/channels")) == 2,
    }
