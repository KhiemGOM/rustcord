"""Checks for activity_steps in src/ui/e2e/activity.rs: the shelf, launching an activity into a web
view, the Embedded App SDK handshake, consent before a code is issued, the app's own token, the
link prompt, participants, joining from a tile and from the channel list, and leaving."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    n = len(runs)
    path = lambda r: r.get("path", "").split("?")[0]
    posts = [r for r in reqs if r.get("method") == "POST" and path(r).endswith("/mock/activity")]
    said = lambda k, app="9100": [body(r)["v"] for r in posts if body(r).get("k") == k and body(r).get("app") == app]
    index = lambda pred: next((i for i, r in enumerate(reqs) if pred(r)), -1)
    count = lambda pred: sum(1 for r in reqs if pred(r))
    ends = lambda r, tail: path(r).endswith(tail)
    first_get = index(lambda r: r["method"] == "GET" and ends(r, "/oauth2/authorize") and "client_id=9100" in r["path"])
    first_grant = index(lambda r: r["method"] == "POST" and ends(r, "/oauth2/authorize") and "client_id=9100" in r["path"])
    me = [r for r in reqs if r["method"] == "GET" and ends(r, "/oauth2/@me")]
    probes = said("probe")
    ready = said("ready")
    participants = [len(d.get("participants", [])) for d in said("dispatch:ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE")]
    launches = lambda app: count(lambda r: r["method"] == "POST" and ends(r, f"/activities/2801/{app}") and body(r).get("session_id"))
    leaves = lambda app: count(lambda r: r["method"] == "POST" and path(r).startswith(f"/api/v9/applications/{app}/activities/2801/instances/") and path(r).endswith("/leave") and body(r).get("session_id"))
    return {
        "activity: Start An Activity opened the shelf with the server's activities": both("activity_shelf")
        and count(lambda r: r["method"] == "GET" and ends(r, "/activities/shelf") and "guild_id=1008" in r["path"]) >= n,
        "activity: picking one launched it into the call's channel with our session": launches("9100") >= n,
        "activity: the frame got a proxy ticket for its own app": count(lambda r: ends(r, "/applications/9100/proxy-tickets")) >= n,
        "activity: the web view loaded the frame and it shook hands and got READY with the config": both("activity_ready")
        and len(ready) >= n
        and all(d.get("v") == 1 and d.get("config", {}).get("cdn_host") for d in ready),
        "activity: the frame got the SDK's frame params, with no account token in them": both("activity_launched")
        and len(probes) >= n
        and all("frame_id=" in p.get("href", "") and "instance_id=" in p.get("href", "") and "platform=desktop" in p.get("href", "") and "channel_id=2801" in p.get("href", "") for p in probes),
        "activity: consent was asked before any code was issued": both("activity_consent") and 0 <= first_get < first_grant,
        "activity: approving gave the frame its code": any(d.get("data", {}).get("code") == "mock-code" for d in said("authorize")),
        "activity: authenticate went to the server with the app's own bearer and nothing of ours": len(me) >= n and all(r.get("auth") == "Bearer mock-access-token" for r in me),
        "activity: the frame could read no token, cookie or name of ours": len(probes) >= n
        and all(not any((p.get("seen") or {}).values()) and not p.get("cookie") and not p.get("name") and "mock-token" not in json.dumps(p) for p in probes),
        "activity: the frame was authenticated and could ask for its channel": both("activity_authed") and any(d.get("data", {}).get("name") == "Hangout" for d in said("channel")),
        "activity: external links asked 'Leaving Discord' and a dismissal answered opened=false": both("activity_link_prompt")
        and both("activity_link_dismissed")
        and any(d.get("data", {}).get("opened") is False for d in said("link")),
        "activity: the frame heard the participants, then someone joining": both("activity_participants") and participants[:1] == [1] and 2 in participants,
        "activity: Leave Activity told the server and closed the frame": both("activity_left") and leaves("9100") >= n,
        "activity: another person's running activity showed a tile and Join Activity joined it": both("activity_join_tile") and both("activity_joined") and launches("9101") >= n,
        "activity: refusing consent sent the frame an error and issued no code": both("activity_denied")
        and any(d.get("evt") == "ERROR" and d.get("data", {}).get("code") == 5000 for d in said("authorize", "9101"))
        and count(lambda r: r["method"] == "POST" and ends(r, "/oauth2/authorize") and "client_id=9101" in r["path"]) == 0,
        "activity: leaving the call left the activity": both("activity_left_with_call") and leaves("9101") >= n,
        "activity: the channel list's Join entered the call and the activity": both("activity_list_row") and both("activity_list_join") and launches("9100") >= 2 * n,
    }
