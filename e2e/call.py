"""Checks for stage_steps in src/ui/e2e/stage.rs: the call bar's menus, Invite, Full Screen, Pop Out, a
tile's Options (volume, mute, focus), the Krisp popout, and stage channels."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [body(r) for r in reqs if r.get("method") == method and part in r.get("path", "")]
    hands = sent("PATCH", "/guilds/1003/voice-states/@me")
    return {
        "call: a tile's Options menu muted Alice for us only, and Focus This Person made her the big tile": both("tile_mute") and both("tile_focus"),
        "call: More Microphone Options lists the input device, profile and voice settings": both("call_mic_menu"),
        "call: the Krisp popout opens from the voice panel and its mic test runs": both("krisp_popout") and both("krisp_test"),
        "call: Invite opened the invite dialog": both("call_invite"),
        "call: Full Screen filled the window and Escape gave it back": both("call_full") and both("call_unfull"),
        "call: Pop Out put the call in its own window and Pop In took it back": both("call_pop"),
        "call: joining another voice channel from a call asked first, and Escape stayed put": both("voice_switch_warn") and both("voice_switch_kept"),
        "stage: the welcome modal came up on the first visit": both("stage_welcome"),
        "stage: Start Stage posted the topic, joined, and made us a speaker": both("stage_started")
            and sum(b.get("topic") == "Panel talk" and b.get("privacy_level") == 2 for b in sent("POST", "/stage-instances")) == 2,
        "stage: Invite to Speak from the queue let the raised hand speak": both("stage_invite")
            and sum(b.get("suppress") is False and bool(b.get("request_to_speak_timestamp")) for b in sent("PATCH", "/guilds/1008/voice-states/102")) == 2,
        "stage: End Stage asked, then deleted the stage": both("stage_ended") and len(sent("DELETE", "/stage-instances/2802")) == 2,
        "stage: Request to Speak raised our hand and Lower Hand put it down": both("stage_hand") and both("stage_hand_down")
            and sum(bool(b.get("request_to_speak_timestamp")) for b in hands) == 2 and sum(b.get("request_to_speak_timestamp") is None for b in hands) == 2,
    }
