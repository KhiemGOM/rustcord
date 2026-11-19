"""Rustcord against the desktop Discord on the real account, measured the same way from outside.

    python e2e/vs_discord.py prep                         before restarting the PC
    python e2e/vs_discord.py run --channel <guild>/<channel> [--warm 5] [--secs 60] [--exe <rustcord.exe>]
    python e2e/vs_discord.py restore                      only if a run died before putting things back

prep checks Rustcord's saved login, copies its data dir (the run is read-only and leaves the real one
alone), and takes Discord off Windows startup so its first launch after the restart is as cold as
Rustcord's. run puts it back. Discord runs stock: Vencord's 200-byte app.asar is set aside for the run
and put back after it, along with the startup entry. Run it straight after the restart with nothing else open: it takes
real mouse and keyboard input and puts each client in front at the same size, because Chromium stops
drawing a covered window.

Each client opens the same channel through a discord:// link and is timed from launch to its first
window, its main window, and its picture settling (under 1% of it changing for 1.5 s). Then one
session each: idle, scrolling the chat with the wheel, Alt+Down through channels, minimized. CPU and
memory are summed over every process of the client; GPU memory and 3D engine time come from
Windows' GPU counters. Writes e2e/artifacts/vs-discord/<time>/report.json and report.md.
"""
import ctypes
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
import winreg
from ctypes import wintypes
from pathlib import Path

from bench import cpu_s, handle, k32, u32

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "e2e" / "artifacts" / "vs-discord"
DATA = OUT / "rustcord-data"
SAVED_RUN = OUT / "discord-run-key.json"
LOCAL = Path(os.environ["LOCALAPPDATA"])
DISCORD = LOCAL / "Discord"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
RECT = (80, 60, 1280, 800)
gdi = ctypes.WinDLL("gdi32")
psapi = ctypes.WinDLL("psapi")
u32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
u32.GetDC.restype = wintypes.HDC
gdi.CreateCompatibleDC.restype = wintypes.HDC
gdi.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
gdi.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
gdi.SetStretchBltMode.argtypes = [wintypes.HDC, ctypes.c_int]
gdi.StretchBlt.argtypes = [wintypes.HDC] + [ctypes.c_int] * 4 + [wintypes.HDC] + [ctypes.c_int] * 4 + [wintypes.DWORD]
gdi.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT, ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT]
gdi.DeleteObject.argtypes = [wintypes.HGDIOBJ]
gdi.DeleteDC.argtypes = [wintypes.HDC]
u32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE


def arg(name, default=None):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


class ProcEntry(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ProcessID", wintypes.DWORD), ("th32DefaultHeapID", ctypes.c_size_t),
                ("th32ModuleID", wintypes.DWORD), ("cntThreads", wintypes.DWORD), ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", wintypes.LONG), ("dwFlags", wintypes.DWORD), ("szExeFile", ctypes.c_wchar * 260)]


def processes():
    snap = k32.CreateToolhelp32Snapshot(0x2, 0)
    e = ProcEntry()
    e.dwSize = ctypes.sizeof(e)
    rows = []
    ok = k32.Process32FirstW(snap, ctypes.byref(e))
    while ok:
        rows.append((e.th32ProcessID, e.th32ParentProcessID, e.szExeFile.lower()))
        ok = k32.Process32NextW(snap, ctypes.byref(e))
    k32.CloseHandle(snap)
    return rows


def born(pid):
    h = handle(pid)
    if not h:
        return 0
    c, e, kt, ut = (wintypes.FILETIME() for _ in range(4))
    k32.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(kt), ctypes.byref(ut))
    k32.CloseHandle(h)
    return ((c.dwHighDateTime << 32 | c.dwLowDateTime) - 116444736000000000) / 1e7


class Client:
    """One launch of a client: the processes it made, its windows, and readings over them."""

    def __init__(self, name, exe_name, cmd, env=None):
        self.name, self.exe_name = name, exe_name
        self.t0 = time.time()
        self.launcher = subprocess.Popen(cmd, env=env)
        self.handles = {}

    def tree(self):
        rows = processes()
        mine = {pid for pid, _, exe in rows if exe == self.exe_name and born(pid) >= self.t0 - 0.5} | {self.launcher.pid}
        grew = True
        while grew:
            more = {pid for pid, parent, _ in rows if parent in mine and pid not in mine and born(pid) >= self.t0 - 0.5}
            grew = bool(more)
            mine |= more
        for pid in mine:
            if pid not in self.handles and (h := handle(pid)):
                self.handles[pid] = h
        return [pid for pid, _, _ in rows if pid in mine]

    def windows(self):
        pids = set(self.tree())
        found = []
        proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        def each(hwnd, _):
            owner = wintypes.DWORD()
            u32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value in pids and u32.IsWindowVisible(hwnd) and not u32.GetWindow(hwnd, 4):
                r = wintypes.RECT()
                u32.GetWindowRect(hwnd, ctypes.byref(r))
                if r.right - r.left > 100 and r.bottom - r.top > 100:
                    found.append((hwnd, r.right - r.left, r.bottom - r.top))
            return True

        u32.EnumWindows(proc(each), 0)
        return found

    def main_window(self):
        big = [w for w in self.windows() if w[1] >= 640 and w[2] >= 480]
        return max(big, key=lambda w: w[1] * w[2])[0] if big else None

    def cpu(self):
        self.tree()
        return sum(cpu_s(h) for h in self.handles.values())

    def memory(self):
        from bench import Mem
        tot = {"task_manager": 0, "working_set": 0, "private": 0, "processes": 0}
        for pid in self.tree():
            h = self.handles.get(pid)
            m = Mem()
            m.cb = ctypes.sizeof(m)
            if h and psapi.GetProcessMemoryInfo(h, ctypes.byref(m), m.cb):
                tot["task_manager"] += m.PrivateWorkingSetSize
                tot["working_set"] += m.WorkingSetSize
                tot["private"] += m.PrivateUsage
                tot["processes"] += 1
        return {k: v if k == "processes" else round(v / 2**20) for k, v in tot.items()}

    def kill(self):
        for pid in self.tree():
            if pid != self.launcher.pid and (h := k32.OpenProcess(0x0001, False, pid)):
                k32.TerminateProcess(h, 0)
                k32.CloseHandle(h)
        if self.launcher.poll() is None:
            self.launcher.kill()
        time.sleep(2)


def rustcord(link, exe):
    env = dict(os.environ, RUSTCORD_DATA=str(DATA), RUSTCORD_READONLY="1")
    return Client("Rustcord", Path(exe).name.lower(), [exe, link], env)


def discord(link):
    return Client("Discord", "discord.exe", [str(DISCORD / "Update.exe"), "--processStart", "Discord.exe", "--process-start-args", f"--url -- {link}"])


def front(hwnd):
    x, y, w, h = RECT
    u32.ShowWindow(hwnd, 9)
    u32.SetWindowPos(hwnd, 0, x, y, w, h, 0x0040)
    u32.keybd_event(0x12, 0, 0, 0)
    u32.keybd_event(0x12, 0, 2, 0)
    u32.SetForegroundWindow(hwnd)


def grab(w=160, h=100):
    """The client's rect on screen, shrunk to w x h grey levels."""
    x, y, cw, ch = RECT
    screen = u32.GetDC(None)
    dc = gdi.CreateCompatibleDC(screen)
    bmp = gdi.CreateCompatibleBitmap(screen, w, h)
    gdi.SelectObject(dc, bmp)
    gdi.SetStretchBltMode(dc, 4)
    gdi.StretchBlt(dc, 0, 0, w, h, screen, x, y, cw, ch, 0x00CC0020)
    info = (ctypes.c_uint32 * 10)(40, w, ctypes.c_uint32(-h).value, 32 << 16 | 1, 0, 0, 0, 0, 0, 0)
    buf = (ctypes.c_uint8 * (w * h * 4))()
    gdi.GetDIBits(dc, bmp, 0, h, buf, info, 0)
    gdi.DeleteObject(bmp)
    gdi.DeleteDC(dc)
    u32.ReleaseDC(None, screen)
    return bytes(buf[i + 1] for i in range(0, len(buf), 4))


def changed(a, b):
    return sum(abs(p - q) > 12 for p, q in zip(a, b)) / len(a)


def start(c, settle_cap=60):
    """Launch to first window, main window, and a settled picture, in ms."""
    t = {"first_window_ms": None, "main_window_ms": None, "settled_ms": None}
    end = c.t0 + settle_cap
    hwnd = None
    while time.time() < end and not hwnd:
        if t["first_window_ms"] is None and c.windows():
            t["first_window_ms"] = round((time.time() - c.t0) * 1000)
        if hwnd := c.main_window():
            t["main_window_ms"] = round((time.time() - c.t0) * 1000)
            t["first_window_ms"] = t["first_window_ms"] or t["main_window_ms"]
            front(hwnd)
        time.sleep(0.02)
    if not hwnd:
        return t, None
    last, still_since = grab(), None
    while time.time() < end:
        time.sleep(0.05)
        now = grab()
        if changed(last, now) < 0.01:
            still_since = still_since or time.time()
            if time.time() - still_since >= 1.5:
                t["settled_ms"] = round((still_since - c.t0) * 1000)
                break
        else:
            still_since = None
        last = now
    return t, hwnd


def gpu(pids, secs):
    """Dedicated GPU memory (MB) and mean 3D engine use (% of the GPU) over secs, summed over pids."""
    tags = {f"pid_{p}_" for p in pids}
    cmd = ["typeperf", r"\GPU Process Memory(*)\Dedicated Usage", r"\GPU Engine(*engtype_3D)\Utilization Percentage", "-si", "1", "-sc", str(max(1, int(secs)))]
    out = subprocess.run(cmd, capture_output=True, text=True).stdout.splitlines()
    rows = [line.strip().strip('"').split('","') for line in out if line.startswith('"')]
    if len(rows) < 2:
        return {}
    head, samples = rows[0], rows[1:]
    mem = engine = 0.0
    for i, name in enumerate(head):
        if not any(t in name for t in tags):
            continue
        vals = [float(r[i]) for r in samples if i < len(r) and r[i].strip() not in ("", " ")]
        if not vals:
            continue
        if "Dedicated Usage" in name:
            mem += vals[-1]
        else:
            engine += statistics.mean(vals)
    return {"gpu_mb": round(mem / 2**20), "gpu_3d_pct": round(engine, 2)}


def measure(c, secs, during=None):
    """CPU (% of one core) of the whole client over secs, with GPU counters read alongside; during() drives input meanwhile."""
    import threading
    pids = c.tree()
    g = {}
    reader = threading.Thread(target=lambda: g.update(gpu(pids, secs)))
    a = c.cpu()
    t = time.time()
    reader.start()
    if during:
        during(t + secs)
    else:
        time.sleep(secs)
    reader.join()
    b = c.cpu()
    return {"cpu_pct_of_one_core": round((b - a) / (time.time() - t) * 100, 2), **g, "memory_mb": c.memory()}


def wheel(hwnd, until):
    r = wintypes.RECT()
    u32.GetWindowRect(hwnd, ctypes.byref(r))
    u32.SetCursorPos(r.left + (r.right - r.left) * 3 // 5, r.top + (r.bottom - r.top) // 2)
    n = 0
    while time.time() < until:
        u32.mouse_event(0x0800, 0, 0, 120 if (n // 12) % 2 == 0 else -120, 0)
        n += 1
        time.sleep(0.09)


def channels(until):
    while time.time() < until:
        u32.keybd_event(0x12, 0, 0, 0)
        u32.keybd_event(0x28, 0, 1, 0)
        u32.keybd_event(0x28, 0, 3, 0)
        u32.keybd_event(0x12, 0, 2, 0)
        time.sleep(1.5)


def quiet(limit=600, below=8.0, secs=20):
    """Waits until the whole machine stays under `below`% CPU for secs; hands back whether it did."""
    idle, kern, user = (wintypes.FILETIME() for _ in range(3))
    v = lambda f: f.dwHighDateTime << 32 | f.dwLowDateTime
    end, calm = time.time() + limit, 0
    while time.time() < end:
        k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kern), ctypes.byref(user))
        i0, t0 = v(idle), v(kern) + v(user)
        time.sleep(1)
        k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kern), ctypes.byref(user))
        busy = 100 * (1 - (v(idle) - i0) / max(1, v(kern) + v(user) - t0))
        calm = calm + 1 if busy < below else 0
        if calm >= secs:
            return True
    return False


def running(exe_names):
    return sorted({exe for _, _, exe in processes() if exe in exe_names})


def size_mb(p):
    p = Path(p)
    return round((p.stat().st_size if p.is_file() else sum(f.stat().st_size for f in p.rglob("*") if f.is_file())) / 2**20, 1)


def discord_run_key(value=None, remove=False):
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_READ | winreg.KEY_SET_VALUE) as k:
        if remove:
            winreg.DeleteValue(k, "Discord")
        elif value is not None:
            winreg.SetValueEx(k, "Discord", 0, winreg.REG_SZ, value)
        else:
            try:
                return winreg.QueryValueEx(k, "Discord")[0]
            except FileNotFoundError:
                return None


def resources():
    return sorted(DISCORD.glob("app-*"))[-1] / "resources"


def vencord_off():
    r = resources()
    if (r / "_app.asar").exists() and (r / "app.asar").stat().st_size < 4096:
        (r / "app.asar").rename(r / "app.asar.vencord")
        (r / "_app.asar").rename(r / "app.asar")
        print("Vencord off for the run")


def restore():
    r = resources()
    if (r / "app.asar.vencord").exists():
        (r / "app.asar").rename(r / "_app.asar")
        (r / "app.asar.vencord").rename(r / "app.asar")
        print("Vencord back on")
    if SAVED_RUN.exists():
        discord_run_key(json.loads(SAVED_RUN.read_text())["value"])
        SAVED_RUN.unlink()
        print("Discord starts with Windows again")


def prep(exe):
    OUT.mkdir(parents=True, exist_ok=True)
    src = LOCAL / "Rustcord"
    if DATA.exists():
        shutil.rmtree(DATA)
    shutil.copytree(src, DATA, ignore=shutil.ignore_patterns("*.log", "*.bak*"))
    probe = OUT / "login-probe"
    shutil.rmtree(probe, ignore_errors=True)
    env = dict(os.environ, RUSTCORD_DATA=str(DATA), RUSTCORD_READONLY="1", RUSTCORD_BENCH=str(probe))
    p = subprocess.Popen([exe], env=env)
    ok = False
    end = time.time() + 60
    while time.time() < end and not ok:
        try:
            ok = "ready" in json.loads((probe / "bench.json").read_text()).get("marks_ms", {})
        except (OSError, ValueError):
            pass
        time.sleep(0.5)
    p.kill()
    shutil.rmtree(probe, ignore_errors=True)
    if not ok:
        raise SystemExit("Rustcord didn't reach READY with its saved login: log in to Rustcord, then run prep again")
    if not SAVED_RUN.exists() and (v := discord_run_key()):
        SAVED_RUN.write_text(json.dumps({"value": v}))
        discord_run_key(remove=True)
        print("Discord taken off Windows startup until the run (or `restore`)")
    print(f"login ok, data copied to {DATA}. Restart the PC, open nothing, then run.")


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    exe = str(Path(arg("--exe", ROOT / "target" / "release" / "rustcord.exe")).resolve())
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
    if cmd == "prep":
        return prep(exe)
    if cmd == "restore":
        return restore()
    if cmd != "run" or not arg("--channel"):
        raise SystemExit(__doc__)
    if not DATA.exists():
        raise SystemExit("run prep first")
    if busy := running({"discord.exe", Path(exe).name.lower()}):
        raise SystemExit(f"close these first (Discord sits in the tray): {busy}")
    link = f"discord://-/channels/{arg('--channel')}"
    warm, secs = int(arg("--warm", 5)), float(arg("--secs", 60))
    up_min = k32.GetTickCount64() / 60000
    stamp = time.strftime("%Y%m%d-%H%M")
    out = OUT / stamp
    out.mkdir(parents=True)
    report = {"when": stamp, "uptime_min_at_start": round(up_min, 1), "machine_quiet": quiet(),
              "rustcord_exe": exe, "discord_version": sorted(p.name for p in DISCORD.glob("app-*"))[-1],
              "install_mb": {"Rustcord": size_mb(exe), "Discord": round(size_mb(sorted(DISCORD.glob("app-*"))[-1]) + size_mb(Path(os.environ["APPDATA"]) / "discord" / sorted(DISCORD.glob("app-*"))[-1].name[4:] / "modules"), 1)},
              "rect": RECT, "cold": {}, "warm": {"Rustcord": [], "Discord": []}, "session": {}}
    make = {"Rustcord": lambda: rustcord(link, exe), "Discord": lambda: discord(link)}
    try:
        vencord_off()
        report["vencord"] = (resources() / "app.asar").stat().st_size < 4096
        for name in ("Rustcord", "Discord"):
            c = make[name]()
            report["cold"][name], _ = start(c)
            print(name, "cold", report["cold"][name], flush=True)
            c.kill()
            quiet(120)
        for i in range(warm):
            for name in (("Rustcord", "Discord") if i % 2 == 0 else ("Discord", "Rustcord")):
                c = make[name]()
                t, _ = start(c)
                report["warm"][name].append(t)
                print(name, "warm", t, flush=True)
                c.kill()
                quiet(120)
        for name in ("Rustcord", "Discord"):
            c = make[name]()
            _, hwnd = start(c)
            s = report["session"][name] = {}
            if hwnd:
                time.sleep(30)
                front(hwnd)
                s["idle"] = measure(c, secs)
                s["scroll"] = measure(c, 20, lambda until: wheel(hwnd, until))
                s["channels"] = measure(c, 30, channels)
                time.sleep(5)
                s["after_channels"] = measure(c, 15)
                u32.ShowWindow(hwnd, 6)
                time.sleep(10)
                s["minimized"] = measure(c, secs)
            print(name, "session", json.dumps(s), flush=True)
            c.kill()
            quiet(120)
    finally:
        restore()
    report["warm_median"] = {name: {k: median([r[k] for r in runs]) for k in ("first_window_ms", "main_window_ms", "settled_ms")} for name, runs in report["warm"].items()}
    (out / "report.json").write_text(json.dumps(report, indent=2))
    (out / "report.md").write_text(markdown(report))
    print(markdown(report))
    print(f"\n{out}")


def median(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def markdown(r):
    rows = [("Install size (MB)", lambda n: r["install_mb"][n])]
    for when in ("cold", "warm_median"):
        for k in ("first_window_ms", "main_window_ms", "settled_ms"):
            rows.append((f"{when.split('_')[0]} start: {k[:-3].replace('_', ' ')} (ms)", lambda n, w=when, k=k: r.get(w, {}).get(n, {}).get(k)))
    for phase in ("idle", "scroll", "channels", "after_channels", "minimized"):
        rows.append((f"{phase}: CPU % of a core", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("cpu_pct_of_one_core")))
        rows.append((f"{phase}: GPU 3D %", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("gpu_3d_pct")))
        rows.append((f"{phase}: Task Manager memory (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("memory_mb", {}).get("task_manager")))
        rows.append((f"{phase}: private commit (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("memory_mb", {}).get("private")))
        rows.append((f"{phase}: GPU memory (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("gpu_mb")))
    rows.append(("processes", lambda n: r["session"].get(n, {}).get("idle", {}).get("memory_mb", {}).get("processes")))
    lines = [f"Rustcord vs Discord ({r['discord_version']}{', with Vencord' if r['vencord'] else ''}), {r['when']}, {r['uptime_min_at_start']} min after boot, machine quiet: {r['machine_quiet']}", "",
             "| | Rustcord | Discord |", "|---|---|---|"]
    lines += [f"| {label} | {f('Rustcord')} | {f('Discord')} |" for label, f in rows]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    main()
