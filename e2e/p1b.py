"""Checks for chatcheck.rs and p1b.rs (steps 4340-4399): things driven by real clicks and keys in
the general channel, the forum's Post Guidelines card, the DM list's Wave button, Listen Along in
a profile popout and the built-in commands as pills."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    posts = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").startswith("/api/v9/channels/") and r["path"].endswith("/messages")]
    waves = [r for r in posts if body(r).get("sticker_ids") == ["749054660769218631"]]
    shrugs = [r for r in posts if body(r).get("content", "").startswith("pill hello")]
    return {
        "chat: the emoji picker's Copy Emoji Image Link put the emoji's cdn url on the clipboard": both("copy_emoji_image_link"),
        "chat: Escape closed the search results and emptied the search box": both("escape_clears_search"),
        "chat: hovering an upload card's spoiler button showed a Spoiler Attachment tooltip, whether or not it was on": both("spoiler_tip_off") and both("spoiler_tip_on"),
        "chat: /gif and /sticker described themselves as Search Animated GIFs on the Web and Search your stickers": both("gif_command_text") and both("sticker_command_text"),
        "forum: the topic showed as a Post Guidelines card and its close button took it away": both("forum_guidelines_card") and both("forum_guidelines_closed"),
        "dms: a friend's new empty dm had a Wave button with a Wave to tooltip, and no other dm did": both("wave_only_new_dm") and both("wave_tooltip"),
        "dms: Wave sent the Wave sticker to that dm and the button went away": both("wave_sent") and len(waves) >= 2 and all(not r["path"].endswith("/3000/messages") for r in waves),
        "profile: a song's popout card had a Listen Along button with a Listen along with tooltip, and a click did something": both("listen_along_tip") and both("listen_along_click"),
        "composer: a built-in command became a pill, took its option and sent what discord sends": both("builtin_pill") and both("builtin_pill_sent") and len(shrugs) >= 2,
        "composer: /gif as a pill opened the gif picker on its query": both("builtin_gif_pill"),
    }
