"""Runs the client against the mock from one e2e step on, to try a range of steps without the
whole run. The steps before it that open the first channel still run.

    python e2e/from_step.py 1900     # needs target/release built; prints the marks it reached
"""

import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import run


def main():
    art = Path(tempfile.mkdtemp(prefix="rustcord_from_step_"))
    run.ART = art
    log = art / "mock_requests.jsonl"
    mock = subprocess.Popen([str(run.ROOT / "target" / "release" / "examples" / "mockcord.exe"), str(run.HTTP), str(run.WS), str(log), str(run.VOICE)])
    time.sleep(0.5)
    try:
        env = {"RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1", "RUSTCORD_E2E_FROM": sys.argv[1]}
        res = run.run_client("run", art / "data", env)
    finally:
        mock.kill()
    print(json.dumps({"ok": res.get("ok"), "error": res.get("error"), "marks": sorted(res.get("marks_ms", {}))}, indent=1))
    print("artifacts:", art)


if __name__ == "__main__":
    main()
