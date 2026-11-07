"""Checks for the forum post form, driven by forum_steps in src/ui/e2e/forum.rs: the tag picker
and what the post's request body carried."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    made = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").startswith("/api/v9/channels/2030/threads")]
    sent = [body(r) for r in made]
    tagged = [b for b in sent if b.get("name") == "Tagged post"]
    return {
        "forum: a REQUIRE_TAG forum showed \"A tag is required to create a post in this channel\" and Post sent nothing": both("forum_required_error")
            and both("forum_required_blocks") and all(b.get("applied_tags") for b in sent),
        "forum: picking a tag in the post form cleared the error and the post went out with that tag in applied_tags, nested fields on": both("forum_tag_picked")
            and len(tagged) >= 2 and all(b["applied_tags"] == ["2"] and b["message"] == {"content": "Needs a tag first"} for b in tagged)
            and all("use_nested_fields=true" in r["path"] for r in made),
        "forum: the new post came back with its tag": both("forum_post_created_tagged"),
        "forum: a post's toolbar showed Follow, Copy Link and React to Post; Follow joined the post, Copy Link said Copied, React to Post opened the picker": both("forum_bar_drawn")
            and both("forum_bar_following") and both("forum_bar_copied") and both("forum_bar_react_picker")
            and len([r for r in reqs if r.get("method") == "PUT" and r.get("path", "").endswith("/channels/2031/thread-members/@me")]) >= 2,
        "forum: a forum with more tags than fit shows See more tags, and a tag picked from it joined the post": both("forum_see_more_tags") and both("forum_more_tag_picked"),
    }
