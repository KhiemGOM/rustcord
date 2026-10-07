"""End-to-end run: real fastcord binary against the mock Discord in examples/mockcord.rs.

Does a cold start (empty data dir) and a warm start (cache from the cold run),
drives the UI (click guild, click channel, type + send, scroll history, open DMs),
then checks timings and what the mock actually received.

    python e2e/run.py            # builds release first
    python e2e/run.py --no-build

Artifacts land in e2e/artifacts/: report.json, report.md, screenshots, request log.
"""

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "e2e" / "artifacts"
HTTP, WS = 7701, 7702
BUDGET_MS = 1000


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "fastcord", "--example", "mockcord"], cwd=ROOT, check=True)


def run_client(name, data_dir):
    out = ART / name
    out.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update({
        "FASTCORD_API": f"http://127.0.0.1:{HTTP}/api/v9",
        "FASTCORD_CDN": f"http://127.0.0.1:{HTTP}/cdn",
        "FASTCORD_EMOJI_CDN": f"http://127.0.0.1:{HTTP}/cdn/emoji",
        "FASTCORD_GATEWAY": f"ws://127.0.0.1:{WS}",
        "FASTCORD_TOKEN": "mock-token",
        "FASTCORD_DATA": str(data_dir),
        "FASTCORD_E2E": str(out),
    })
    exe = ROOT / "target" / "release" / "fastcord.exe"
    spawned = time.time() * 1000
    p = subprocess.Popen([str(exe)], env=env)
    try:
        code = p.wait(timeout=90)
    except subprocess.TimeoutExpired:
        p.kill()
        code = "timeout"
    res = json.loads((out / "results.json").read_text()) if (out / "results.json").exists() else {"ok": False, "error": "no results.json"}
    ff = out / "first_frame_epoch_ms"
    res["spawn_to_first_frame_ms"] = round(float(ff.read_text()) - spawned, 1) if ff.exists() else None
    res["exit_code"] = code
    return res


def main():
    if "--no-build" not in sys.argv:
        build()
    if ART.exists():
        shutil.rmtree(ART)
    ART.mkdir(parents=True)
    data = ART / "data"
    log = ART / "mock_requests.jsonl"

    mock = subprocess.Popen([str(ROOT / "target" / "release" / "examples" / "mockcord.exe"), str(HTTP), str(WS), str(log)])
    time.sleep(0.5)
    try:
        cold = run_client("cold", data)
        warm = run_client("warm", data)
    finally:
        mock.kill()

    reqs = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines() if l.strip()]
    sends = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/channels/2011/messages")]
    sent_bodies = [json.loads(r["body"]).get("content", "") for r in sends]
    acks = [r for r in reqs if "/ack" in r.get("path", "")]
    history = [r for r in reqs if "before=" in r.get("path", "")]
    subs = [r for r in reqs if r.get("gateway_op") == 37]

    checks = {
        "cold run finished its script": cold.get("ok") is True,
        "warm run finished its script": warm.get("ok") is True,
        f"cold: spawn -> first frame < {BUDGET_MS}ms": (cold.get("spawn_to_first_frame_ms") or 1e9) < BUDGET_MS,
        f"warm: spawn -> first frame < {BUDGET_MS}ms": (warm.get("spawn_to_first_frame_ms") or 1e9) < BUDGET_MS,
        f"warm: cached servers on screen < {BUDGET_MS}ms": warm.get("marks_ms", {}).get("content", 1e9) < BUDGET_MS,
        "two messages typed through the composer reached the API": len(sent_bodies) == 2 and all("hello from the e2e run" in b for b in sent_bodies),
        "older history was paged in via ?before=": len(history) >= 2,
        "read state was acked": len(acks) >= 1,
        "member list subscription (op 37) was sent": len(subs) >= 1,
        "message authors were requested via op 8": any(r.get("gateway_op") == 8 for r in reqs),
        "frame cpu p99 < 8ms (cold)": cold.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
        "frame cpu p99 < 8ms (warm)": warm.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
    }
    report = {"checks": checks, "cold": cold, "warm": warm, "requests_seen": len(reqs), "sent_bodies": sent_bodies}
    (ART / "report.json").write_text(json.dumps(report, indent=2))

    md = ["# fastcord e2e report", ""]
    md += [f"- [{'x' if ok else ' '}] {name}" for name, ok in checks.items()]
    for name, r in (("cold", cold), ("warm", warm)):
        md += ["", f"## {name} start", "", f"- spawn -> first frame: {r.get('spawn_to_first_frame_ms')} ms"]
        md += [f"- {k}: {v} ms" for k, v in r.get("marks_ms", {}).items()]
        f = r.get("frame_cpu_ms", {})
        md += [f"- frame cpu: p50 {f.get('p50', 0):.2f} / p95 {f.get('p95', 0):.2f} / p99 {f.get('p99', 0):.2f} / max {f.get('max', 0):.2f} ms over {f.get('count')} frames"]
        md += [f"- screenshot: {name}/{s}" for s in r.get("screenshots", [])]
        if r.get("error"):
            md += [f"- error: {r['error']}"]
    (ART / "report.md").write_text("\n".join(md) + "\n", encoding="utf-8")
    print("\n".join(md))
    sys.exit(0 if all(checks.values()) else 1)


if __name__ == "__main__":
    main()
