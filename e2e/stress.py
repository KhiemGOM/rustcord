"""Seeded UI stress against mockcord: python e2e/stress.py [seed ...]."""

import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import run


def main():
    art = Path(tempfile.mkdtemp(prefix="rustcord_stress_"))
    run.ART = art
    mock = subprocess.Popen([str(run.ROOT / "target/release/examples/mockcord.exe"), str(run.HTTP), str(run.WS),
                             str(art / "mock_requests.jsonl"), str(run.VOICE), str(run.REMOTE)])
    ok = True
    try:
        time.sleep(0.5)
        for seed in sys.argv[1:] or ["1", "42", "8675309"]:
            result = run.run_client(f"seed_{seed}", art / f"data_{seed}", {
                "RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1",
                "RUSTCORD_E2E_FROM": "9000", "RUSTCORD_STRESS_SEED": str(int(seed)),
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
