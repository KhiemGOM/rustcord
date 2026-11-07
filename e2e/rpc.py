"""Checks for local_steps in src/ui/e2e/local.rs: a game's rich presence on the pipe, a website on
the 6463 socket, and a second launch handing a discord:// link to the running client."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    presence = [(r.get("t", 0), r.get("d", {})) for r in reqs if r.get("gateway_op") == 3]
    arena = lambda p: [a for a in p.get("activities", []) if a.get("application_id") == "7200"]
    first = [(t, arena(p)[0]) for t, p in presence if arena(p) and arena(p)[0].get("details") == "Exploring the arena"]
    second = [(t, arena(p)[0]) for t, p in presence if arena(p) and arena(p)[0].get("details") == "Round 2"]
    shaped = lambda a: (
        a.get("name") == "Mock Arena" and a.get("state") == "In a group" and a.get("type") == 0
        and a.get("timestamps", {}).get("start") == 1_700_000_000_000
        and a.get("assets", {}).get("large_image") == "9001"
        and a.get("assets", {}).get("small_image", "").startswith("mp:external/mock/example.com/badge.png")
        and a.get("party") == {"id": "party-e2e", "size": [1, 4]}
        and a.get("secrets") == {"join": "join-e2e-secret"} and a.get("flags") == 3 and a.get("session_id")
    )
    cleared = [t for t, p in presence if not arena(p) and second and t > second[0][0]]
    oauth = lambda method: sum("client_id=7200" in r.get("path", "") for r in sent(method, "/oauth2/authorize"))
    templates = [body(r) for r in sent("POST", "/guilds/templates/e2eTpl1")]
    return {
        "rpc: a game got in on the pipe and was told who we are; a bad client id and a bad version were closed": both("rpc_handshake"),
        "rpc: OAuth-only commands, unknown ones and AUTHORIZE are refused with Discord's codes": both("rpc_refusals"),
        "rpc: SET_ACTIVITY is validated and answered, and its images resolved to asset ids and mp: paths": both("rpc_set_activity") and both("rpc_activity_shape"),
        "rpc: the game's activity went out as our presence with details, party, secrets and images": bool(first) and all(shaped(a) for _, a in first + second),
        "rpc: a second update inside the gateway's pace waited and went out on its own": bool(first) and bool(second) and second[0][0] - first[0][0] >= 3000,
        "rpc: the game's panel showed on the account card, and closing the pipe cleared the presence": both("rpc_game_panel") and both("rpc_activity_cleared") and bool(cleared),
        "rpc: no presence update lists one application twice": all(
            len([a["application_id"] for a in p.get("activities", [])]) == len({a["application_id"] for a in p.get("activities", [])}) for _, p in presence
        ),
        "rpc: other origins, a lookalike, no origin and a rebound host were turned away; a website can't set an activity": both("rpc_web_origin"),
        "rpc: AUTHORIZE refused scopes Rustcord has no text for, a redirect uri, another app and the token grant without asking": both("rpc_authorize_refusals"),
        "rpc: AUTHORIZE and a token-less AUTHENTICATE asked in a window first, then returned a code and a token that AUTHENTICATE accepted": both("rpc_authorize_consented")
            and oauth("POST") == 4 and oauth("GET") == 6,
        "rpc: cancelling the prompt denied that connection for good, and a token for another app was refused": both("rpc_authorize_denied"),
        "rpc: the join dialog showed the invite's server, from a preview with counts": both("rpc_invite_preview") and len(sent("GET", "/invites/e2eRpc1?with_counts=true")) >= 2,
        "rpc: INVITE_BROWSER from discord.com opened the join dialog with the invite and Join Server joined it": both("rpc_web_invite_reply") and both("rpc_invite_modal") and both("rpc_invite_accept")
            and len(sent("POST", "/invites/e2eRpc1")) == 2,
        "rpc: GUILD_TEMPLATE_BROWSER over the websocket showed the template and Create made the server": both("rpc_web_socket") and both("rpc_template_modal") and both("rpc_template_create")
            and len(templates) == 2 and all(t.get("name", "").endswith("'s server") for t in templates),
        "rpc: a second launch with a discord:// link handed it to the running client and exited": both("rpc_second_instance") and both("rpc_link_channel"),
        "rpc: discord://-/invite/ opened the join dialog, and Escape closed it": both("rpc_link_invite") and both("rpc_link_closed"),
    }
