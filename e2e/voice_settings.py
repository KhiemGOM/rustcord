"""Checks for voice_steps in src/ui/e2e.rs: Voice & Video's mic test, input profiles, settings
reaching the voice engine, and what the page sends to the account."""

import base64
import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    protos = [base64.b64decode(body(r).get("settings", "")) for r in reqs
              if r.get("method") == "PATCH" and r.get("path", "").endswith("/users/@me/settings-proto/1")]
    return {
        "voice: Mic Test started and stopped": both("voice_test_on") and both("voice_test_off"),
        "voice: Studio opened the mic with no processing, Custom brought the controls back": both("voice_studio") and both("voice_custom"),
        "voice: attenuation, gain control, echo cancellation and noise suppression reach the engine": both("voice_engine_flags"),
        "voice: choosing another microphone reopens it live": both("voice_device_live"),
        "voice: Reset puts the voice settings back": both("voice_reset"),
        "voice: Show Stream Previews and a sound's switch can be turned off": both("voice_previews_off") and both("voice_sound_off"),
        "voice: turning off stream previews updated the account's voice_and_video": any(p[:1] == b"*" for p in protos),
    }
