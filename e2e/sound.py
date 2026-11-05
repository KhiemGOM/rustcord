"""Checks for sound_steps in src/ui/e2e.rs: discord's sounds fetched from the mock, cached in the
data dir, decoded and queued for playback while RUSTCORD_SILENT keeps the speakers closed."""

import shutil
import subprocess


def make(dest):
    """Three short mp3s for the mock to serve, or None when ffmpeg isn't here to make them."""
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        return None
    dest.mkdir(parents=True, exist_ok=True)
    for i, hz in enumerate((660, 880, 440)):
        subprocess.run([ffmpeg, "-y", "-loglevel", "error", "-f", "lavfi", "-i", f"sine=frequency={hz}:duration=0.25",
                        "-b:a", "32k", str(dest / f"tone{i}.mp3")], check=True)
    return dest


def checks(runs, reqs, data, served):
    if not served:
        return {}
    fetched = {r["path"] for r in reqs if r.get("path", "").startswith("/assets/") and r["path"].endswith(".mp3")}
    marks = {k: set(r.get("marks_ms", {})) for k, r in runs.items()}
    return {
        "sounds: the cold run fetched discord's sounds into the data dir": "sound_fetched" in marks["cold"]
            and len(fetched) >= 23 and len(list((data / "sounds").glob("*.mp3"))) >= 23,
        "sounds: both runs decoded a sound and queued it for playback": all({"sound_decoded", "sound_queued"} <= m for m in marks.values()),
        "sounds: the warm run played from the cache without fetching again": "sound_fetched" not in marks["warm"],
    }
