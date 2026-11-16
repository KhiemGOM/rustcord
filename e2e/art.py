"""Checks for the art marks in src/ui/e2e.rs: Discord's icons fetched from the mock, which serves
the real seed chunks from discord.com (downloaded once into target/e2e-art), and cached in the
data dir for the warm run."""

import urllib.request

SEEDS = ["web.843cc7edc28c426c", "1a1518aa502a9482"]


def make(dest):
    """The seed chunks for the mock to serve, or None when discord.com can't be reached."""
    dest.mkdir(parents=True, exist_ok=True)
    for seed in SEEDS:
        f = dest / f"{seed}.js"
        if f.exists():
            continue
        try:
            with urllib.request.urlopen(f"https://discord.com/assets/{seed}.js", timeout=60) as r:
                f.write_bytes(r.read())
        except OSError:
            return None
    return dest


def checks(runs, reqs, data, served):
    if not served:
        return {}
    marks = {k: set(r.get("marks_ms", {})) for k, r in runs.items()}
    return {
        "art: the cold run fetched discord's icons from its bundle into the data dir": "art_fetched" in marks["cold"]
            and any(r.get("path") == f"/assets/{SEEDS[0]}.js" for r in reqs) and len(list((data / "art").glob("*.svg"))) >= 199,
        "art: both runs had the icons, the warm run from the cache without fetching again": all("art_ready" in m for m in marks.values())
            and "art_fetched" not in marks["warm"],
    }
