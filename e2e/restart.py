"""Changes account settings through their controls, restarts the client on the same data folder against
the same mock, and checks they stuck (a login must not put back what the account was never told).

    python e2e/restart.py     # needs target/release built; exits 1 if a setting came back changed
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import run


def main():
    art = Path(tempfile.mkdtemp(prefix="rustcord_restart_"))
    run.ART = art
    log = art / "mock_requests.jsonl"
    mock = subprocess.Popen([str(run.ROOT / "target" / "release" / "examples" / "mockcord.exe"), str(run.HTTP), str(run.WS), str(log), str(run.VOICE), str(run.REMOTE)],
                            env=dict(os.environ, MOCK_SETTINGS_LIVE="1"))
    time.sleep(0.5)
    fake = {"RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1"}
    try:
        first = run.run_client("change", art / "data", {**fake, "RUSTCORD_E2E_FROM": "5000", "RUSTCORD_P4B": "keep"})
        second = run.run_client("verify", art / "data", {**fake, "RUSTCORD_E2E_FROM": "5040", "RUSTCORD_P4B": "verify"})
    finally:
        mock.kill()
    ok = "p4b_changed" in first.get("marks_ms", {}) and "p4b_stuck" in second.get("marks_ms", {})
    print(json.dumps({"changed": first.get("error") or "p4b_changed" in first.get("marks_ms", {}), "stuck": second.get("error") or "p4b_stuck" in second.get("marks_ms", {})}))
    print("artifacts:", art)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
