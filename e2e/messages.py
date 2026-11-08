"""Checks for messages_steps in src/ui/e2e.rs: the system messages discord draws (their wording), who
reacted, the message menu per kind, a failed send that goes out again under its nonce, Report Message,
the search sort menu and the inbox's filter and friend requests."""

import json

TEXTS = {
    "7:102": "Bob 🔨",
    "7:104": "Dyno was added to the server.",
    "6:103": "Carol ✿ pinned a message to this channel. See all pinned messages.",
    "8:102": "Bob 🔨 just Boosted the server!",
    "9:103": "Carol ✿ just Boosted the server 2 times! Rust Lounge has achieved Level 1!",
    "18:101": "Alice started a thread: release plan. See all threads.",
    "24:103": "AutoMod has blocked a message in #general",
    "25:105": "Eve renewed Gold and has been a subscriber of Rust Lounge for 3 months!",
    "27:101": "Alice started Rust talk",
    "28:101": "Alice ended Rust talk",
    "29:102": "Bob 🔨 is now a speaker.",
    "30:103": "Carol ✿ requested to speak.",
    "31:101": "Alice changed the Stage topic: Q&A",
    "36:101": "Alice enabled security actions until",
    "37:101": "Alice disabled security actions.",
    "38:101": "Alice reported a raid in Rust Lounge.",
    "39:101": "Alice resolved an Activity Alert.",
    "44:102": "Bob 🔨 has purchased Rust Mug!",
    "46:103": "Carol ✿’s poll Worse leg has closed.",
    "14:101": "has been removed from Server Discovery because it no longer passes all the requirements.",
    "15:101": "This server is eligible for Server Discovery again and has been automatically relisted!",
    "16:101": "has failed Discovery activity requirements for 1 week.",
    "17:101": "has failed Discovery activity requirements for 3 weeks in a row.",
    "3:101": "You missed a call from Alice that lasted 2 minutes.",
    "4:102": "Bob 🔨 changed the channel name: new-name",
}


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    rects = lambda r: r.get("rects", {})
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    menu = lambda k: [set(rects(r).get(k, [])) for r in runs.values()]
    shows = lambda k, *labels: all(set(labels) <= m for m in menu(k))
    hides = lambda k, *labels: all(not (set(labels) & m) for m in menu(k))
    posts = [r for r in sent("POST", "/channels/2070/messages") if body(r).get("content") == "e2e-fail-once"]
    nonces = {}
    for r in posts:
        nonces.setdefault(body(r).get("nonce"), []).append(r)
    reports = [body(r) for r in sent("POST", "/reporting/message")]
    out = {
        "messages: every system message kind says what discord's wording says": both("msg_system_texts")
        and all(want in rects(r).get("system_texts", {}).get(k, "") for r in runs.values() for k, want in TEXTS.items()),
        "messages: a reaction's tooltip names who reacted": both("msg_reaction_tooltip")
        and all(rects(r).get("reaction_tooltips") == [":fire: reacted by Alice, Bob 🔨, and Carol ✿", ":thumbsup: reacted by Alice and Bob 🔨"] for r in runs.values()),
        "messages: who reacted is fetched per emoji": len([r for r in sent("GET", "/messages/") if "/reactions/" in r.get("path", "") and "limit=100" in r.get("path", "")]) >= 4,
        "messages: View Reactions opens its window": both("msg_reactors_modal"),
        "messages: the menu on a reacted message has View Reactions and Report Message": shows("menu:0", "Add Reaction", "View Reactions", "Reply", "Forward", "Copy Text", "Report Message") and hides("menu:0", "Edit Message", "Resend message"),
        "messages: a join message has no Forward, Copy Text or Pin": shows("menu:1", "Reply", "Report Message") and hides("menu:1", "Forward", "Copy Text", "Pin Message"),
        "messages: an app's reply has View Interaction Info": shows("menu:2", "View Interaction Info"),
        "messages: a voice message has Save File and no Copy Text": shows("menu:3", "Save File") and hides("menu:3", "Copy Text"),
        "messages: a pin notice can't be replied to or pinned": shows("menu:4", "Report Message", "Bookmark Message") and hides("menu:4", "Reply", "Pin Message", "Forward"),
        "messages: a send that failed turns red and its menu offers Resend message": both("msg_failed") and all({"Add Reaction", "Resend message", "Delete Message"} <= set(rects(r).get("menu:failed", [])) and not {"Reply", "Forward", "Edit Message", "Pin Message", "Report Message"} & set(rects(r).get("menu:failed", [])) for r in runs.values()),
        "messages: Resend message goes out again under the same nonce": both("msg_resent") and len(nonces) == 2 and all(len(v) == 2 for v in nonces.values()),
        "messages: Report Message walks discord's menu and submits the path": both("msg_report_sent") and both("msg_report_closed") and len(sent("GET", "/reporting/menu/message")) == 2
        and len(reports) == 2 and all(b.get("breadcrumbs") == ["root", "spam"] and b.get("elements") == {"reasons": ["links"]} and b.get("name") == "message" for b in reports),
        "messages: the search sort menu re-runs the search oldest first": both("msg_search_sort") and sum("sort_order=asc" in r.get("path", "") and "sort_by=timestamp" in r.get("path", "") for r in sent("GET", "/messages/search")) == 2,
        "messages: the inbox Filter button drops role mentions from the mentions": both("msg_inbox_filter") and sum("roles=false" in r.get("path", "") for r in sent("GET", "/users/@me/mentions")) == 2,
        "messages: the inbox's View Friend Requests shows the requests": both("msg_inbox_friends"),
        "messages: the inbox's Unreads tab showed Stay up to date with latest messages! once, and Got it! dismissed it for good": both("inbox_tip_dismissed"),
        "messages: Configure on a join message opens Engagement settings": both("msg_configure"),
    }
    return out
