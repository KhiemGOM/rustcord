"""Perf numbers against the mock: startup, idle and busy CPU, frames drawn, memory.

    python e2e/bench.py [--build] [--secs 15] [--rate 20] [--guilds 115] [--target target]

A cold launch (empty data dir) for startup, then a warm one that sits idle for --secs, takes
--rate gateway events a second for --secs (presences, typing, messages elsewhere), repaints
nonstop for --secs to price a frame, and has its memory read. The mock adds --guilds servers of 115 channels each, about a real account's size
(0 for the plain mock). The window opens behind everything and ignores the real mouse and
keyboard. Writes e2e/artifacts/bench.json.
"""
import ctypes
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from ctypes import wintypes
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / (sys.argv[sys.argv.index("--target") + 1] if "--target" in sys.argv else "target") / "release"
EXE = TARGET / "rustcord.exe"
k32 = ctypes.WinDLL("kernel32", use_last_error=True)
u32 = ctypes.WinDLL("user32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)


class Mem(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
                                       "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]


def handle(pid):
    return k32.OpenProcess(0x1000 | 0x0400 | 0x0010, False, pid)


def cpu_s(h):
    c, e, kt, ut = (wintypes.FILETIME() for _ in range(4))
    k32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(kt), ctypes.byref(ut))
    return sum((t.dwHighDateTime << 32 | t.dwLowDateTime) for t in (kt, ut)) / 1e7


def mem_mb(h):
    m = Mem()
    m.cb = ctypes.sizeof(m)
    psapi.GetProcessMemoryInfo(h, ctypes.byref(m), m.cb)
    return {"working_set": m.WorkingSetSize >> 20, "private": m.PrivateUsage >> 20, "peak_working_set": m.PeakWorkingSetSize >> 20}


def close(pid):
    proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _):
        owner = wintypes.DWORD()
        u32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            u32.PostMessageW(hwnd, 0x0010, 0, 0)
        return True

    u32.EnumWindows(proc(each), 0)


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait(pred, secs):
    end = time.time() + secs
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.05)
    return None


def stats(out):
    try:
        return json.loads((out / "bench.json").read_text())
    except (OSError, ValueError):
        return None


def launch(env, out):
    out.mkdir(parents=True, exist_ok=True)
    spawned = time.time() * 1000
    p = subprocess.Popen([str(EXE)], env=dict(env, RUSTCORD_BENCH=str(out)))
    print(f"{out.name} pid {p.pid}", file=sys.stderr, flush=True)
    ff = wait(lambda: (out / "first_frame_epoch_ms").exists() and (out / "first_frame_epoch_ms").read_text(), 30)
    first = round(float(ff) - spawned) if ff else None
    s = wait(lambda: (s := stats(out)) and "content" in s["marks_ms"] and s, 60) or stats(out) or {}
    return p, {"spawn_to_first_frame_ms": first, "marks_ms": s.get("marks_ms"), "slow_frames": s.get("slow_frames")}


def window(h, out, secs):
    a, c = stats(out) or {}, cpu_s(h)
    time.sleep(secs)
    b, d = stats(out) or {}, cpu_s(h)
    causes = {k: n - a.get("causes", {}).get(k, 0) for k, n in b.get("causes", {}).items()}
    return {"cpu_pct_of_one_core": round((d - c) / secs * 100, 2), "frames_per_s": round((b["frames"] - a["frames"]) / secs, 1),
            "ui_ms_per_s": round((b["ui_ms"] - a["ui_ms"]) / secs, 2),
            "repaints_asked_by": dict(sorted(((k, n) for k, n in causes.items() if n), key=lambda kv: -kv[1])[:8])}


def main():
    secs = float(sys.argv[sys.argv.index("--secs") + 1]) if "--secs" in sys.argv else 15
    rate = int(sys.argv[sys.argv.index("--rate") + 1]) if "--rate" in sys.argv else 20
    guilds = sys.argv[sys.argv.index("--guilds") + 1] if "--guilds" in sys.argv else "115"
    if "--build" in sys.argv:
        subprocess.run(["cargo", "build", "--release", "--bin", "rustcord", "--example", "mockcord"], cwd=ROOT, check=True)
    http, ws = free_port(), free_port()
    tmp = Path(tempfile.mkdtemp(prefix="rcbench"))
    mock = subprocess.Popen([str(TARGET / "examples" / "mockcord.exe"), str(http), str(ws), str(tmp / "mock.jsonl"), str(free_port())],
                            env=dict(os.environ, MOCK_GUILDS=guilds), stdout=subprocess.DEVNULL)
    env = dict(os.environ, RUSTCORD_API=f"http://127.0.0.1:{http}/api/v9", RUSTCORD_CDN=f"http://127.0.0.1:{http}/cdn",
               RUSTCORD_EMOJI_CDN=f"http://127.0.0.1:{http}/cdn/emoji", RUSTCORD_FONTS=f"http://127.0.0.1:{http}",
               RUSTCORD_GATEWAY=f"ws://127.0.0.1:{ws}", RUSTCORD_TOKEN="mock-token", RUSTCORD_DATA=str(tmp / "data"), RUSTCORD_SILENT="1")
    report: dict = {"exe_mb": round(EXE.stat().st_size / 2**20, 1), "guilds": int(guilds)}
    try:
        time.sleep(0.3)
        # the first run of a fresh exe pays for Defender's scan; keep that out of the cold start
        p, _ = launch(env, tmp / "prime")
        p.kill()
        p.wait()
        env_cold = dict(env, RUSTCORD_DATA=str(tmp / "cold"))
        p, report["cold"] = launch(env_cold, tmp / "c")
        time.sleep(2)
        close(p.pid)
        p.wait(20)
        env["RUSTCORD_DATA"] = env_cold["RUSTCORD_DATA"]
        p, report["warm"] = launch(env, tmp / "w")
        h = handle(p.pid)
        # past the snapshot save five seconds after READY
        time.sleep(8)
        report["idle"] = window(h, tmp / "w", secs)
        req = urllib.request.Request(f"http://127.0.0.1:{http}/api/v9/mock/busy", json.dumps({"rate": rate, "secs": secs + 2}).encode(), method="POST")
        urllib.request.urlopen(req).read()
        time.sleep(1)
        report["busy"] = window(h, tmp / "w", secs)
        report["busy"]["rate"] = rate
        time.sleep(3)
        (tmp / "w" / "spin").touch()
        time.sleep(1)
        spin = window(h, tmp / "w", secs)
        report["frame_cost_ms"] = {"cpu": round(spin["cpu_pct_of_one_core"] * 10 / spin["frames_per_s"], 2), "ui": round(spin["ui_ms_per_s"] / spin["frames_per_s"], 2),
                                   "frames_per_s": spin["frames_per_s"]}
        report["memory_mb"] = mem_mb(h)
        report["warm"]["slow_frames"] = (stats(tmp / "w") or {}).get("slow_frames")
        p.kill()
        p.wait()
    finally:
        mock.kill()
        mock.wait()
        shutil.rmtree(tmp, ignore_errors=True)
    (ROOT / "e2e" / "artifacts").mkdir(parents=True, exist_ok=True)
    (ROOT / "e2e" / "artifacts" / "bench.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
