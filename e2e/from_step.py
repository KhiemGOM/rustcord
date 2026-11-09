"""Runs the client against the mock from one e2e step on, to try a range of steps without the
whole run. The steps before it that open the first channel still run.

    python e2e/from_step.py 1900     # needs target/release built; prints the marks it reached
    python e2e/from_step.py login    # the login run on its own, scored by auth.checks
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import auth
import run


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("step")
    parser.add_argument("--data", type=Path)
    args = parser.parse_args()
    art = Path(tempfile.mkdtemp(prefix="rustcord_from_step_"))
    if args.data:
        shutil.copytree(args.data, art / "data")
    run.ART = art
    log = art / "mock_requests.jsonl"
    mock_exe = os.environ.get("RUSTCORD_MOCK_EXE") or run.ROOT / "target" / "release" / "examples" / "mockcord.exe"
    mock = subprocess.Popen([str(mock_exe), str(run.HTTP), str(run.WS), str(log), str(run.VOICE), str(run.REMOTE)])
    time.sleep(0.5)
    try:
        if args.step == "login":
            res = run.run_client("login", art / "data", {"RUSTCORD_TOKEN": None, "RUSTCORD_E2E_LOGIN": "1", "RUSTCORD_FAKE_PASSKEY": "mock-assertion", "RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1"})
            time.sleep(0.5)
            reqs = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines() if l.strip()]
            print(json.dumps({k: v for k, v in auth.checks(res, reqs).items()}, indent=1))
            print(json.dumps({"ok": res.get("ok"), "error": res.get("error"), "marks": sorted(res.get("marks_ms", {}))}))
            print("artifacts:", art)
            return
        env = {"RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_CAM": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1", "RUSTCORD_E2E_FROM": args.step, "RUSTCORD_E2E_MEMBERS": "1"}
        res = run.run_client("run", art / "data", env)
    finally:
        mock.kill()
    print(json.dumps({"ok": res.get("ok"), "error": res.get("error"), "marks": sorted(res.get("marks_ms", {}))}, indent=1))
    print("artifacts:", art)
    sys.exit(0 if res.get("ok") and res.get("exit_code") == 0 else 1)


if __name__ == "__main__":
    main()
