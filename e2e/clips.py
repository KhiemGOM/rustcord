"""Checks for clips_steps in src/ui/e2e/clips.rs: the clips page, a real Go Live into the buffer, a
saved clip, the gallery, the trimmer's keys and export, sharing as an attachment, the voice flags
and the consent warning, and everything freed once the stream ends. They need the fake screen, so
only the cold run counts."""


def checks(runs, reqs, data):
    cold = runs["cold"].get("marks_ms", {})
    has = lambda *names: all(n in cold for n in names)
    flags = [(r.get("d") or {}).get("flags", 0) for r in reqs if r.get("gateway_op") == 4]
    sent_on_voice = [r for r in reqs if r.get("voice_op") == 18]
    shared = [r for r in reqs if r.get("method") == "POST" and "/channels/2801/messages" in r.get("path", "") and "is_clip" in (r.get("body") or "")]
    return {
        "clips: the Clips page has Enable Clipping and View Clips, and ALT + C is the default keybind": has("clips_page", "clips_default_keybind"),
        "clips: a Go Live with clips on buffers video and audio, and polls the keybind only then": has("clips_buffering", "clips_hotkey_on", "clips_hotkey_off"),
        "clips: the panel button says Save Clip (ALT + C), and clicking it wrote an mp4 of about the clip length with an audio track, metadata and a thumbnail":
            has("clips_tooltip", "clip_saved", "clip_length_and_audio", "clip_metadata"),
        "clips: saving toasted 'You recently captured a clip!'": has("clip_toast"),
        "clips: the gallery lists the saved clip": has("clips_gallery"),
        "clips: in the trimmer arrow keys moved a handle 0.1 s and Shift+arrow 1 s, and the trim was cut out on export": has("clip_trim_keys", "clip_trim_export"),
        "clips: sharing posted the clip with is_clip metadata": bool(shared),
        "clips: the client's voice state carries its flags (clips enabled and voice allowed), nothing goes out on voice op 18, and it read the other member's":
            any(f & 1 and f & 2 for f in flags) and not sent_on_voice and has("clips_consent_flags"),
        "clips: Save Clip is one row in the keybind list": has("clips_keybind_once"),
        "clips: a clip plays in the inline player": has("clip_plays_inline", "clip_player_closed"),
        "clips: a call member with clips on raised the warning toast once, and the call shows the Enabled badge": has("clips_call_warning", "clips_badge"),
        "clips: the privacy page keeps its 'Allow my voice to be recorded in Clips' switch": has("clips_privacy_switch"),
        "clips: ending the stream freed the buffer and showed the reminder": has("clips_freed", "clips_reminder"),
    }
