"""Layout sweep: the tour against the mock at a given window size and zoom, read-only.

Every user settings page, server settings page, the role editor and every channel settings page
is shot at the top and scrolled to the bottom. A read-only run must put nothing but GETs on the
wire, so the mock's request log is checked for that too.

    python e2e/tour.py                       # 1280x720, 940x500 and 1707x913 at zoom 100
    python e2e/tour.py 1280x720@150          # WxH in points, @zoom percent
    python e2e/tour.py --sheets 1280x720     # also contact sheets of each run's shots

Needs target/release built; RUSTCORD_EXE and RUSTCORD_MOCK point at other builds. Shots land in a temp dir printed at the end, one folder per size.
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import run

DEFAULT = ["1280x720", "940x500", "1707x913"]
MOCK = Path(os.environ.get("RUSTCORD_MOCK") or run.ROOT / "target" / "release" / "examples" / "mockcord.exe")


def sweep(art, spec):
    size, _, zoom = spec.partition("@")
    name = "tour_" + spec.replace("@", "_z")
    out = art / name
    extra = {"RUSTCORD_TOUR": str(out), "RUSTCORD_TOUR_SIZE": size, "RUSTCORD_TOUR_ZOOM": zoom or "100", "RUSTCORD_READONLY": None if "--writable" in sys.argv else "1", "RUSTCORD_TOUR_SERVER": "1",
             "RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_SILENT": "1"}
    log = art / f"{name}.jsonl"
    mock = subprocess.Popen([str(MOCK), str(run.HTTP), str(run.WS), str(log), str(run.VOICE)])
    time.sleep(0.5)
    try:
        res = run.run_client(name, art / f"{name}_data", extra, timeout=3600)
    finally:
        mock.kill()
    sent = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines() if l.strip()] if log.exists() else []
    writes = [f'{r["method"]} {r["path"]}' for r in sent if r.get("method") not in (None, "GET") and not r["path"].endswith("/members-search")]
    shots = res.get("screenshots", [])
    skipped = res.get("rects", {}).get("sweep_skipped", [])
    checks = {
        "sweep finished": bool(res.get("marks_ms", {}).get("sweep")),
        "read-only run sent no write": not writes,
        "every page captured": not skipped,
    }
    print(f"{spec}: {len(shots)} shots, screen {res.get('rects', {}).get('screen_points')}, error {res.get('error')}")
    for k, v in checks.items():
        print(f"  {'ok  ' if v else 'FAIL'} {k}")
    if writes:
        print("  writes:", writes[:10])
    if skipped:
        print("  skipped:", skipped)
    return all(checks.values()), art / name


def sheets(folder):
    from PIL import Image
    shots = sorted(folder.glob("*_[0-9]*.png"))
    for kind in ("settings", "server", "chan"):
        group = [s for s in shots if s.name.startswith(kind + "_")]
        for n in range(0, len(group), 6):
            tiles = [Image.open(s).convert("RGB") for s in group[n:n + 6]]
            w, h = tiles[0].size
            scale = 1000 / w
            tw, th = int(w * scale), int(h * scale)
            sheet = Image.new("RGB", (tw * 2, th * 3))
            for i, t in enumerate(tiles):
                sheet.paste(t.resize((tw, th)), ((i % 2) * tw, (i // 2) * th))
            sheet.save(folder / f"sheet_{kind}_{n // 6:02}.png")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    art = Path(tempfile.mkdtemp(prefix="rustcord_tour_"))
    run.ART = art
    ok = True
    for spec in args or DEFAULT:
        good, folder = sweep(art, spec)
        ok = ok and good
        if "--sheets" in sys.argv:
            sheets(folder)
    print("artifacts:", art)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
