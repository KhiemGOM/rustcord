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
        "p4c: waiting in an unstarted stage looped stage_waiting (fetched from the sound assets) and showed Mute Music in the tray and the voice panel": both("stage_music_plays")
            and any(r.get("path", "").endswith("/assets/7458c5c3bc415b01.mp3") for r in reqs),
        "p4c: Mute Music in the tray silenced it (the button reads Unmute Music), Unmute Music in the voice panel brought it back": both("stage_music_muted") and both("stage_music_panel_unmuted"),
        "p4c: a live stage stops the music, Unmute Music there plays it, and leaving ends that": both("stage_music_stops_live") and both("stage_music_forced") and both("stage_music_ends_on_leave"),
        "p4c: the stage tray's hand is Request to Speak with REQUEST_TO_SPEAK and Raise Hand Disabled (and does nothing) without it": both("stage_hand_enabled") and both("stage_hand_disabled"),
        "p4c: the DM header's profile toggle greys out (Show User Profile (Unavailable)) below 1132px": both("dm_profile_unavailable"),
    }
