"""Checks for serverx_steps in src/ui/e2e/serverx.rs, and for what the client sends on the writes
whose shape was settled against discord's own client: read acks, channel permission overwrites,
X-Context-Properties, the status in the settings blob and member applications."""

import base64
import json



def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def context(r):
    try:
        return json.loads(base64.b64decode(r.get("context") or ""))
    except ValueError:
        return None


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    acks = [r for r in reqs if r.get("method") == "POST" and "/messages/" in r.get("path", "") and r.get("path", "").endswith("/ack") and "refused" not in r]
    plain = [body(r) for r in acks if not body(r).get("manual")]
    manual = [body(r) for r in acks if body(r).get("manual")]
    refused = [r for r in reqs if r.get("refused") and ("/ack" in r.get("path", "") or "/permissions/" in r.get("path", "") or "/requests/" in r.get("path", ""))]
    located = lambda method, part, loc: any(r.get("method") == method and part in r.get("path", "") and (context(r) or {}).get("location") == loc for r in reqs)
    patches = [body(r) for r in sent("PATCH", "/guilds/1001/requests/")]
    puts = [body(r) for r in sent("PUT", "/channels/2102/permissions/")]
    links = [e for r in sent("PUT", "/guilds/1001/roles/4800/connections/configuration") for g in body(r) for e in g]
    return {
        "serverx: read acks sent last_viewed, and the token the last answer handed out": bool(plain)
            and all(isinstance(b.get("last_viewed"), int) for b in plain) and any(str(b.get("token") or "").startswith("ack-") for b in plain),
        "serverx: Mark Unread sent manual and its mention count, with no token": bool(manual) and all(isinstance(b.get("mention_count"), int) and "token" not in b for b in manual),
        "serverx: the mock refused none of the acks, overwrites or applications the client sent": not refused,
        "serverx: channel permissions went one PUT per overwrite, not a PATCH with the list": bool(puts) and all(b.get("type") in (0, 1) and isinstance(b.get("allow"), str) for b in puts),
        "serverx: Add Friend and Invite People said where they came from": located("POST", "/users/@me/relationships", "Add Friend") and located("POST", "/invites", "Guild Context Menu"),
        "serverx: member applications listed Pending and Rejected, Approve and Reject went out, and a new one arrived over the gateway": both("joins_pending") and both("joins_detail")
            and both("joins_approved") and both("joins_rejected") and both("joins_rejected_with_reason") and both("joins_gateway_create") and both("joins_sort")
            and any(b.get("action") == "APPROVED" for b in patches) and any(b.get("action") == "REJECTED" and b.get("rejection_reason") == "please read the rules first" for b in patches),
        "serverx: View Server as Role replaced the owner's permissions under its bar and Disable put them back": both("viewas_on") and both("viewas_off"),
            "serverx: a linked role's Steam requirement saved with its Games field, operator and value": both("linked_fields")
            and any(e.get("connection_type") == "steam" and "connection_metadata_field" not in e for e in links)
            and any(e.get("connection_metadata_field") == "game_count" and e.get("operator") == 4 and e.get("value") == "0" for e in links),
    }
