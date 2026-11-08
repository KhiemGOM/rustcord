"""Checks for the inline video, audio and text-file players, driven by player_steps in
src/ui/e2e/players.rs against the mock's clip.mp4, tone.mp3 and hello.rs in #media."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    got = lambda part: sum(r.get("method") == "GET" and part in r.get("path", "") and "width=" not in r.get("path", "") for r in reqs)
    return {
        "players: a video attachment drew its poster until it was played": both("player_poster"),
        "players: clicking the video played it and the mock served the file": both("player_video_plays") and got("/cdn/media/clip.mp4") >= 1,
        "players: the picture changed on screen while it played, a frame at a time": both("player_video_advances"),
        "players: pausing stopped the frame uploads and the clock": both("player_video_pauses") and both("player_pause_idle"),
        "players: a click on the seek bar jumped to that point": both("player_seek"),
        "players: K played, M muted and the volume was saved": both("player_keys"),
        "players: F opened the full window view and Escape closed it": both("player_fullscreen") and both("player_fullscreen_exit"),
        "players: scrolling the video out of view paused it": both("player_offscreen_pause"),
        "players: the mp3 was read in ranged pieces, never as one whole download": got("/cdn/media/tone.mp3") >= 1
            and all(r.get("range") for r in reqs if r.get("method") == "GET" and "/cdn/media/tone.mp3" in r.get("path", "")),
        "players: an mp3 played, seeked, and took over from the video": both("player_audio_plays") and both("player_audio_seek") and both("player_one_at_a_time")
            and got("/cdn/media/tone.mp3") >= 1,
        "players: a .rs attachment showed as code, expanded, and ctrl+a selected just its text": both("player_text_loads") and both("player_text_expands")
            and both("player_text_selects") and got("/cdn/media/hello.rs") >= 1,
    }
