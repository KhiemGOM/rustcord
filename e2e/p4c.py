"""Checks for p4c_steps in src/ui/e2e/p4c.rs: what each person in a call is played at, synced with the
account's audio_context_settings."""

import base64
import json

import protowire


def volumes(reqs):
    out = []
    for r in reqs:
        if r.get("method") != "PATCH" or not r.get("path", "").endswith("/settings-proto/1"):
            continue
        try:
            blob = base64.b64decode(json.loads(r.get("body") or "{}").get("settings", ""))
        except ValueError:
            continue
        for n, group in protowire.fields(blob):
            if n != 16:
                continue
            users = {}
            for f, entry in protowire.fields(group):
                if f != 1:
                    continue
                key = protowire.first(entry, 1)
                setting = protowire.first(entry, 2, b"")
                users[key] = (protowire.first(setting, 2, 0.0), bool(protowire.first(setting, 1, 0)), protowire.first(setting, 3, 0))
            out.append(users)
    return out


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = volumes(reqs)
    return {
        "p4c: a click on the User Volume slider PATCHed audio_context_settings with that person at the clicked volume and a modified time": both("peer_volume_clicked")
            and any(101 in u and 140 < u[101][0] < 142 and u[101][2] > 0 for u in sent),
        "p4c: the slider back at 100% sent the map without them (an entry at the defaults when it would be empty)": both("peer_volume_reset")
            and any(u.get(101, (0, 0, 0))[0] == 100.0 and not u[101][1] for u in sent),
        "p4c: another device's volumes reached the mixer, and its reset cleared them": both("peer_remote_applied") and both("peer_remote_cleared"),
    }
