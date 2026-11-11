"""Seeded UI stress against mockcord: python e2e/stress.py [seed ...]."""

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("seeds", nargs="*", type=int, default=[1, 42, 8675309])
    parser.add_argument("--actions", type=int, default=300)
    args = parser.parse_args()
    if not 1 <= args.actions <= 800 or any(seed < 1 or seed >= 2**64 for seed in args.seeds):
        parser.error("actions must be 1..800 and seeds must be 1..2^64-1")
    art = Path(tempfile.mkdtemp(prefix="rustcord_stress_"))
    run.ART = art
    mock = subprocess.Popen([str(run.ROOT / "target/release/examples/mockcord.exe"), str(run.HTTP), str(run.WS),
                             str(art / "mock_requests.jsonl"), str(run.VOICE), str(run.REMOTE)])
    ok = True
    try:
        time.sleep(0.5)
        for seed in args.seeds:
            result = run.run_client(f"seed_{seed}", art / f"data_{seed}", {
                "RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1",
                "RUSTCORD_E2E_FROM": "9000", "RUSTCORD_STRESS_SEED": str(seed), "RUSTCORD_STRESS_ACTIONS": str(args.actions),
            })
            passed = result.get("ok") and result.get("exit_code") == 0 and "stress_completed" in result.get("marks_ms", {})
            ok = ok and passed
            print(json.dumps({"seed": seed, "ok": bool(passed), "error": result.get("error")}), flush=True)
    finally:
        mock.kill()
        mock.wait()
    print("artifacts:", art, flush=True)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
