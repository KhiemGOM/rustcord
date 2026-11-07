"""Checks for perms_steps in src/ui/e2e/perms.rs: Server Settings' sidebar under partial permissions,
who may pin, and that a 401 on one request leaves the account signed in."""

import json
from urllib.parse import unquote


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    return {
        "perms: a member with no permissions sees Profile, Engagement, Boost Perks and App Directory only": both("perms_none"),
        "perms: Manage Expressions and Ban Members add Emoji, Stickers, Soundboard, Members and Bans, nothing else": both("perms_partial"),
        "perms: a real click on the Bans row opened the page": both("perms_clicked") and bool(sent("GET", "/guilds/1002/bans")),
        "perms: the owner sees Roles, Invites, Audit Log, Server Template and Delete Server": both("perms_owner"),
        "perms: pinning wants Pin Messages and Read Message History, not Manage Messages": both("perms_pin"),
        "perms: a 401 from a rest call and from a message load left the account signed in": both("unauthorized_kept")
            and bool(sent("GET", "/e2e/unauthorized")) and bool(sent("GET", "/channels/2201/messages")),
        "perms: live invite and audit log events reloaded their open pages": both("live_invites_reloaded") and both("live_audit_reloaded"),
        "perms: USER_SETTINGS_UPDATE moved the status and custom status": both("live_settings_update"),
        "perms: the Ban dialog refused no reason, took Other with typed text and Previous 24 Hours, and sent them": both("ban_needs_reason") and both("ban_sent")
            and any(unquote(r.get("reason") or "") == "spam wave" and body(r).get("delete_message_seconds") == 86400 for r in sent("PUT", "/guilds/1002/bans/160")),
        "perms: the Kick dialog sent its reason as the query the client uses": both("kick_sent")
            and any(r.get("path", "").endswith("/guilds/1002/members/160?reason=rude") for r in sent("DELETE", "/guilds/1002/members/160")),
        "perms: a managed role's editor says it is managed by an integration, and a role above yours says it is locked": both("role_managed_banner") and both("role_locked_banner"),
        "perms: the Members table drew its checkboxes as unactionable under the member's own rank": both("members_unactionable"),
    }
