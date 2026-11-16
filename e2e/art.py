"""Checks for the art marks in src/ui/e2e.rs: Discord's icons fetched from the mock, which serves
the real seed chunks from discord.com (downloaded once into target/e2e-art), and cached in the
data dir for the warm run."""

import urllib.request

SEEDS = ["web.843cc7edc28c426c", "1a1518aa502a9482"]
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36"


def make(dest):
    """The seed chunks for the mock to serve, or None when discord.com can't be reached."""
    dest.mkdir(parents=True, exist_ok=True)
    for seed in SEEDS:
        f = dest / f"{seed}.js"
        if f.exists():
            continue
        try:
            req = urllib.request.Request(f"https://discord.com/assets/{seed}.js", headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as r:
                f.write_bytes(r.read())
        except OSError:
            return None
    return dest


def checks(runs, reqs, data, served):
    if not served:
        return {"art: discord's seed chunks were downloaded for the mock to serve": False}
    marks = {k: set(r.get("marks_ms", {})) for k, r in runs.items()}
    return {
        "art: the cold run fetched discord's icons from its bundle into the data dir": "art_fetched" in marks["cold"]
            and any(r.get("path") == f"/assets/{SEEDS[0]}.js" for r in reqs) and len(list((data / "art").glob("*.svg"))) >= 199,
        "art: both runs had the icons, the warm run from the cache without fetching again": all("art_ready" in m for m in marks.values())
            and "art_fetched" not in marks["warm"],
    }
