"""Perf numbers against the mock: startup, idle and busy CPU, frames drawn, memory.

    python e2e/bench.py [--build] [--profile release|fast] [--secs 15] [--rate 20] [--guilds 115] [--history 800] [--target target]

A cold launch (empty data dir) for startup, then a warm one that sits idle for --secs, takes
--rate gateway events a second for --secs (presences, typing, messages elsewhere), repaints
nonstop for --secs to price a frame, scrolls a --history message chat with the wheel for --secs and prices its frames, has its memory read, then opens its recent channels for
3x --secs and has it read again. The mock adds --guilds servers of 115 channels each, about a real account's size
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
PROFILE = sys.argv[sys.argv.index("--profile") + 1] if "--profile" in sys.argv else "release"
TARGET = ROOT / (sys.argv[sys.argv.index("--target") + 1] if "--target" in sys.argv else "target") / PROFILE
EXE = Path(os.environ.get("RUSTCORD_EXE") or TARGET / "rustcord.exe")
k32 = ctypes.WinDLL("kernel32", use_last_error=True)
u32 = ctypes.WinDLL("user32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)


class Mem(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
                                       "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage",
                                       "PrivateWorkingSetSize", "SharedCommitUsage")]


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
    # task manager's "memory" is the private working set
    return {"task_manager": m.PrivateWorkingSetSize >> 20, "working_set": m.WorkingSetSize >> 20, "private": m.PrivateUsage >> 20, "peak_working_set": m.PeakWorkingSetSize >> 20}


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


def free_both():
    """The voice port takes tcp and udp. Windows reserves udp ranges inside the ephemeral
    ports a tcp probe hands out (the mock then dies binding it), so try until one binds both ways."""
    while True:
        p = free_port()
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as u:
            try:
                u.bind(("127.0.0.1", p))
                return p
            except OSError:
                pass


def wait_mock(mock, secs=10):
    """Until the mock prints its ports, which it does once every one of them is bound (http
    binds first, so it can answer and still die on the voice port). A mock that died says so,
    instead of the run carrying on against nothing and failing at its first request. The rest
    of its stdout is drained so the pipe never fills and stalls it."""
    import threading
    up = threading.Event()

    def drain():
        for line in mock.stdout:
            if line.startswith(b"mockcord http="):
                up.set()

    threading.Thread(target=drain, daemon=True).start()
    end = time.time() + secs
    while time.time() < end:
        if up.wait(0.05):
            return
        if mock.poll() is not None:
            raise SystemExit(f"mockcord exited with {mock.returncode} before it was up (its error is above)")
    raise SystemExit(f"mockcord wasn't up within {secs}s")


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


# every client started, so a run that fails midway doesn't leave one behind to load the next
SPAWNED: list = []


def launch(env, out):
    out.mkdir(parents=True, exist_ok=True)
    spawned = time.time() * 1000
    p = subprocess.Popen([str(EXE)], env=dict(env, RUSTCORD_BENCH=str(out)))
    SPAWNED.append(p)
    print(f"{out.name} pid {p.pid}", file=sys.stderr, flush=True)
    ff = wait(lambda: (out / "first_frame_epoch_ms").exists() and (out / "first_frame_epoch_ms").read_text(), 30)
    first = round(float(ff) - spawned) if ff else None
    s = wait(lambda: (s := stats(out)) and "content" in s["marks_ms"] and s, 60) or stats(out) or {}
    return p, {"spawn_to_first_frame_ms": first, "marks_ms": s.get("marks_ms"), "slow_frames": s.get("slow_frames")}


class ThreadEntry(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ThreadID", wintypes.DWORD), ("th32OwnerProcessID", wintypes.DWORD),
                ("tpBasePri", wintypes.LONG), ("tpDeltaPri", wintypes.LONG), ("dwFlags", wintypes.DWORD)]


k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
k32.OpenThread.restype = wintypes.HANDLE
k32.GetThreadDescription.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_wchar_p)]
k32.LocalFree.argtypes = [ctypes.c_void_p]


def thread_cpu(pid):
    """CPU seconds per thread name ("main" for the oldest thread, "?" for unnamed ones), no admin needed."""
    snap = k32.CreateToolhelp32Snapshot(0x4, 0)
    rows = []
    e = ThreadEntry()
    e.dwSize = ctypes.sizeof(e)
    ok = k32.Thread32First(snap, ctypes.byref(e))
    while ok:
        if e.th32OwnerProcessID == pid:
            t = k32.OpenThread(0x0800, False, e.th32ThreadID)
            if t:
                c, x, kt, ut = (wintypes.FILETIME() for _ in range(4))
                k32.GetThreadTimes(t, ctypes.byref(c), ctypes.byref(x), ctypes.byref(kt), ctypes.byref(ut))
                desc = ctypes.c_wchar_p()
                name = "?"
                if k32.GetThreadDescription(t, ctypes.byref(desc)) >= 0 and desc.value:
                    name = desc.value
                    k32.LocalFree(ctypes.cast(desc, ctypes.c_void_p))
                k32.CloseHandle(t)
                born = c.dwHighDateTime << 32 | c.dwLowDateTime
                rows.append((born, name, sum((v.dwHighDateTime << 32 | v.dwLowDateTime) for v in (kt, ut)) / 1e7))
        ok = k32.Thread32Next(snap, ctypes.byref(e))
    k32.CloseHandle(snap)
    out = {}
    for i, (_, name, s) in enumerate(sorted(rows)):
        name = "main" if i == 0 else name
        out[name] = out.get(name, 0) + s
    return out


def window(h, out, secs):
    pid = k32.GetProcessId(h)
    a, c, ta = stats(out) or {}, cpu_s(h), thread_cpu(pid)
    time.sleep(secs)
    b, d, tb = stats(out) or {}, cpu_s(h), thread_cpu(pid)
    causes = {k: n - a.get("causes", {}).get(k, 0) for k, n in b.get("causes", {}).items()}
    # threads that ended inside the window drop out; their time is still in cpu_pct_of_one_core
    threads = {k: round((s - ta.get(k, 0)) / secs * 100, 2) for k, s in tb.items()}
    # the bench's own threads inside the client; a user's client doesn't run them
    harness = sum(v for k, v in threads.items() if k in ("bench-io", "e2e-watchdog"))
    return {"cpu_pct_of_one_core": round((d - c) / secs * 100, 2), "app_cpu_pct": round((d - c) / secs * 100 - harness, 2),
            "frames_per_s": round((b["frames"] - a["frames"]) / secs, 1),
            "ui_ms_per_s": round((b["ui_ms"] - a["ui_ms"]) / secs, 2),
            "cpu_by_thread": dict(sorted(((k, v) for k, v in threads.items() if v >= 0.05), key=lambda kv: -kv[1])),
            "repaints_asked_by": dict(sorted(((k, n) for k, n in causes.items() if n), key=lambda kv: -kv[1])[:8])}


def scroll(h, out, secs):
    """A notch every 90ms, twelve one way then twelve the other, with the pointer over the chat. The window sits
    behind everything, so nothing holds it to the display's rate: what counts is the cost of a frame, not their spacing."""
    (out / "scroll").touch()
    time.sleep(1)
    n, t0 = len((stats(out) or {}).get("costs", [])), (stats(out) or {}).get("travel", 0)
    w = window(h, out, secs)
    costs = sorted((stats(out) or {}).get("costs", [])[n:])
    (out / "scroll").unlink()
    if not costs:
        return w
    at = lambda q: round(costs[min(int(len(costs) * q), len(costs) - 1)], 2)
    w.update({"scrolled_px": round((stats(out) or {}).get("travel", 0) - t0), "frames": len(costs), "ui_ms_mean": round(sum(costs) / len(costs), 2), "ui_ms_p50": at(0.5), "ui_ms_p95": at(0.95),
              "ui_ms_p99": at(0.99), "ui_ms_max": round(costs[-1], 2), "over_8ms": sum(c > 8.3 for c in costs), "over_16ms": sum(c > 16.7 for c in costs)})
    return w


def main():
    secs = float(sys.argv[sys.argv.index("--secs") + 1]) if "--secs" in sys.argv else 15
    rate = int(sys.argv[sys.argv.index("--rate") + 1]) if "--rate" in sys.argv else 20
    guilds = sys.argv[sys.argv.index("--guilds") + 1] if "--guilds" in sys.argv else "115"
    history = sys.argv[sys.argv.index("--history") + 1] if "--history" in sys.argv else "800"
    if "--build" in sys.argv:
        subprocess.run(["cargo", "build", "--profile", PROFILE, "--bin", "rustcord", "--example", "mockcord"], cwd=ROOT, check=True)
    http, ws = free_port(), free_port()
    artifacts = Path(sys.argv[sys.argv.index("--artifacts") + 1]) if "--artifacts" in sys.argv else None
    tmp = artifacts or Path(tempfile.mkdtemp(prefix="rcbench"))
    if artifacts:
        tmp.mkdir(parents=True, exist_ok=False)
    mock = subprocess.Popen([str(TARGET / "examples" / "mockcord.exe"), str(http), str(ws), str(tmp / "mock.jsonl"), str(free_both())],
                            env=dict(os.environ, MOCK_GUILDS=guilds, MOCK_HISTORY=history), stdout=subprocess.PIPE)
    env = dict(os.environ, RUSTCORD_API=f"http://127.0.0.1:{http}/api/v9", RUSTCORD_CDN=f"http://127.0.0.1:{http}/cdn",
               RUSTCORD_EMOJI_CDN=f"http://127.0.0.1:{http}/cdn/emoji", RUSTCORD_FONTS=f"http://127.0.0.1:{http}", RUSTCORD_SOUNDS=f"http://127.0.0.1:{http}",
               RUSTCORD_GATEWAY=f"ws://127.0.0.1:{ws}", RUSTCORD_TOKEN="mock-token", RUSTCORD_DATA=str(tmp / "data"), RUSTCORD_SILENT="1")
    report: dict = {"exe_mb": round(EXE.stat().st_size / 2**20, 1), "guilds": int(guilds)}
    try:
        # building 115 servers' worth of channels comes before the mock binds its ports
        wait_mock(mock, 30)
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
        report["scroll"] = scroll(h, tmp / "w", secs)
        time.sleep(1)
        (tmp / "w" / "spin").touch()
        time.sleep(1)
        spin = window(h, tmp / "w", secs)
        report["frame_cost_ms"] = {"cpu": round(spin["cpu_pct_of_one_core"] * 10 / spin["frames_per_s"], 2), "ui": round(spin["ui_ms_per_s"] / spin["frames_per_s"], 2),
                                   "frames_per_s": spin["frames_per_s"]}
        report["memory_mb"] = mem_mb(h)
        # the recent channels one after another, three seconds each, as someone reading around would
        (tmp / "w" / "spin").unlink()
        (tmp / "w" / "browse").touch()
        time.sleep(secs * 3)
        report["memory_after_browsing_mb"] = mem_mb(h)
        report["warm"]["slow_frames"] = (stats(tmp / "w") or {}).get("slow_frames")
        p.kill()
        p.wait()
    finally:
        for c in SPAWNED:
            if c.poll() is None:
                c.kill()
        mock.kill()
        mock.wait()
        if not artifacts:
            shutil.rmtree(tmp, ignore_errors=True)
    dest = artifacts or ROOT / "e2e" / "artifacts"
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "bench.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
