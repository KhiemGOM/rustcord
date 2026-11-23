"""Rustcord against the desktop Discord on the real account, measured the same way from outside.

    python e2e/vs_discord.py prep                         before restarting the PC
    python e2e/vs_discord.py run --channel <guild>/<channel> [--warm 5] [--secs 60] [--exe <rustcord.exe>] [--record]
                                 [--gifs <guild>/<channel>/<message>] [--video <guild>/<channel>/<message>]
                                 [--stream <guild>/<voice channel>]
    python e2e/vs_discord.py restore                      only if a run died before putting things back

prep checks Rustcord's saved login, copies its data dir (the run is read-only and leaves the real one
alone), and takes Discord off Windows startup so its first launch after the restart is as cold as
Rustcord's. run puts it back. Discord runs stock: Vencord's 200-byte app.asar is set aside for the run
and hardware acceleration is switched on if it was off (off, Discord draws on the CPU and holds no
video memory, which is not what it ships as); both are put back after it, along with the startup entry. Run it straight after the restart with nothing else open: it takes
real mouse and keyboard input and puts each client in front at the same size, because Chromium stops
drawing a covered window.

Each client opens the same channel through a discord:// link, over a grey sheet that hides the
desktop, and is timed from launch to its first window, its main window, the moment it fills its rect
(shown), its picture settling after that (under 1% of it changing for 1.5 s) and the status line
under your name last changing (live: Rustcord draws from its cache before the gateway answers). Then one
session each: Esc first (Discord opens on its What's New after an update, and the wheel would scroll
that), then idle, scrolling the chat with the wheel, the quick switcher and the emoji picker (both
while still in the channel the link opened, where posting is allowed: Discord opens no picker where
it isn't), the message --gifs names (GIFs play on their own), the one --video names with a click
on it to play, the --stream voice channel joined, your whole screen shared there (720p 30 fps, all
Discord allows without Nitro) and then the stream someone else is running in it watched
(Rustcord runs with RUSTCORD_READONLY=voice then, read-only but for the call), Alt+Down through channels, Ctrl+Alt+Down through servers, minimized. CPU and
memory are summed over every process of the client; GPU memory and 3D engine time come from
Windows' GPU counters. Writes e2e/artifacts/vs-discord/<time>/report.json and report.md.
--record also films every launch into footage/ next to them (see footage.py); the encoder works on the
GPU while the numbers are taken, the same for both, and the report says it was on.
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

import cut
import footage
from bench import cpu_s, handle, k32, u32

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "e2e" / "artifacts" / "vs-discord"
DATA = OUT / "rustcord-data"
SAVED_RUN = OUT / "discord-run-key.json"
SAVED_HW = OUT / "discord-hardware-off"
SETTINGS = Path(os.environ["APPDATA"]) / "discord" / "settings.json"
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
        self.before = grab()
        self.t0 = time.time()
        self.launcher = subprocess.Popen(cmd, env=env)
        self.handles = {}
        self.events = [("launch", self.t0)]

    def mark(self, event):
        self.events.append((event, time.time()))

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
                # a client can start minimised (Discord remembers it was); count a minimised titled
                # window as its full window so main_window finds it and front() restores it
                if u32.IsIconic(hwnd) and u32.GetWindowTextLengthW(hwnd) > 0:
                    found.append((hwnd, 9000, 9000))
                    return True
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
        return sum(cpu_s(h) for h in list(self.handles.values()))

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


def rustcord(link, exe, readonly="1"):
    env = dict(os.environ, RUSTCORD_DATA=str(DATA), RUSTCORD_READONLY=readonly)
    c = Client("Rustcord", Path(exe).name.lower(), [exe, link], env)
    c.open = lambda to: subprocess.Popen([exe, to], env=env)
    return c


def discord(link):
    cmd = lambda to: [str(DISCORD / "Update.exe"), "--processStart", "Discord.exe", "--process-start-args", f"--url -- {to}"]
    c = Client("Discord", "discord.exe", cmd(link))
    c.open = lambda to: subprocess.Popen(cmd(to))
    return c


u32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.UINT]


def front(hwnd):
    x, y, w, h = RECT
    u32.ShowWindow(hwnd, 9)
    # HWND_TOPMOST (-1): the client joins the topmost band above the sheet; SetForegroundWindow then
    # puts it at the very top, over the sheet and any other topmost window (e.g. Task Manager)
    u32.SetWindowPos(hwnd, -1, x, y, w, h, 0x0040)
    u32.keybd_event(0x12, 0, 0, 0)
    u32.keybd_event(0x12, 0, 2, 0)
    u32.SetForegroundWindow(hwnd)


def grab(w=160, h=100, box=None):
    """The client's rect on screen (or box, in screen pixels), shrunk to w x h grey levels."""
    x, y, cw, ch = box or RECT
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


# a black sheet topmost over everything (even an elevated, always-on-top Task Manager); the client is
# fronted topmost and brought to the foreground, which lifts it to the very top, above the sheet.
CURTAIN = "import tkinter as t;r=t.Tk();r.overrideredirect(1);r.geometry(f'{r.winfo_screenwidth()}x{r.winfo_screenheight()}+0+0');r.configure(bg='black');r.attributes('-topmost',1);r.mainloop()"


def curtain():
    """A black sheet over the desktop, under the clients: the recording shows nothing else of the
    screen, and a client is on screen once its rect carries real detail, not the sheet."""
    # kill any sheet a crashed earlier run left behind, or it would cover this run's clients
    dead = []
    def each(hwnd, _):
        t = ctypes.create_unicode_buffer(8)
        u32.GetWindowTextW(hwnd, t, 8)
        if u32.IsWindowVisible(hwnd) and t.value == "tk":
            o = wintypes.DWORD(); u32.GetWindowThreadProcessId(hwnd, ctypes.byref(o)); dead.append(o.value)
        return True
    u32.EnumWindows(ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(each), 0)
    for pid in set(dead):
        if h := k32.OpenProcess(0x0001, False, pid):
            k32.TerminateProcess(h, 0); k32.CloseHandle(h)
    p = subprocess.Popen([sys.executable, "-c", CURTAIN])
    time.sleep(2)
    return p


def start(c, settle_cap=60):
    """Launch to first window, main window, the client filling its rect, and its picture settling, in ms.
    Settling only counts once it fills the rect: Discord's splash sits still for a second or two first."""
    t = {"first_window_ms": None, "main_window_ms": None, "shown_ms": None, "settled_ms": None, "live_ms": None}
    end = c.t0 + settle_cap
    hwnd = None
    while time.time() < end and not hwnd:
        if t["first_window_ms"] is None and c.windows():
            t["first_window_ms"] = round((time.time() - c.t0) * 1000)
            c.mark("first_window")
        if hwnd := c.main_window():
            t["main_window_ms"] = round((time.time() - c.t0) * 1000)
            if t["first_window_ms"] is None:
                t["first_window_ms"] = t["main_window_ms"]
                c.mark("first_window")
            c.mark("main_window")
            front(hwnd)
        time.sleep(0.02)
    if not hwnd:
        return t, None
    last, still_since, refront = grab(), None, 0.0
    while time.time() < end:
        time.sleep(0.05)
        # a client can swap its window as it loads (Discord's splash, then the app): keep the real
        # one on top of the sheet, or the capture would settle on black behind it
        if time.time() - refront > 0.4:
            refront = time.time()
            if h2 := c.main_window():
                hwnd = h2
                front(h2)
        now = grab()
        # "on screen" means the real UI, not just the sheet or a blank frame: the rect has to carry
        # real detail (the black sheet is ~1-3 grey levels, a loaded client 45+), however much of it
        # the window fills (a sparse view like Discord's channel list is still "on screen")
        if t["shown_ms"] is None and len(set(now)) >= 45:
            t["shown_ms"] = round((time.time() - c.t0) * 1000)
            c.mark("shown")
        if t["shown_ms"] is not None and changed(last, now) < 0.01:
            still_since = still_since or time.time()
            if time.time() - still_since >= 1.5:
                t["settled_ms"] = round((still_since - c.t0) * 1000)
                c.events.append(("settled", still_since))
                break
        else:
            still_since = None
        last = now
    if t["settled_ms"] is not None:
        # the line under your name: Rustcord draws from its cache and says Connecting there until
        # the gateway answers, Discord only shows once it has. Live is when that line last changes.
        box = (RECT[0] + 60, RECT[1] + RECT[3] - 40, 200, 34)
        was, t["live_ms"] = grab(200, 34, box), t["shown_ms"]
        while time.time() < c.t0 + t["shown_ms"] / 1000 + 12:
            time.sleep(0.1)
            if changed(was, grab(200, 34, box)) > 0.01:
                t["live_ms"] = round((time.time() - c.t0) * 1000)
                break
        c.events.append(("live", c.t0 + t["live_ms"] / 1000))
    c.mark("launch_end")
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
        vals = [max(0.0, float(r[i])) for r in samples if i < len(r) and r[i].strip() not in ("", " ")]
        if not vals:
            continue
        if "Dedicated Usage" in name:
            mem += vals[-1]
        else:
            engine += statistics.mean(vals)
    return {"gpu_mb": round(mem / 2**20), "gpu_3d_pct": round(engine, 2)}


def measure(c, label, secs, during=None):
    """CPU (% of one core) of the whole client over secs, with GPU counters read alongside; during() drives input meanwhile."""
    import threading
    if label != "minimized" and (hw := c.main_window()):
        front(hw)  # keep the client over the sheet (and any Task Manager that popped up) for this clip
    pids = c.tree()
    g = {}
    reader = threading.Thread(target=lambda: g.update(gpu(pids, secs)))
    a = c.cpu()
    t = time.time()
    c.mark(label)
    reader.start()
    if during:
        during(t + secs)
    else:
        time.sleep(secs)
    reader.join()
    b = c.cpu()
    c.mark(f"{label}_end")
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


def press(*vks):
    """A chord: each key down in order, then up in reverse. The arrows are extended keys."""
    ext = lambda vk: 1 if vk in (0x26, 0x28) else 0
    for vk in vks:
        u32.keybd_event(vk, 0, ext(vk), 0)
    for vk in reversed(vks):
        u32.keybd_event(vk, 0, ext(vk) | 2, 0)


def click(hwnd, fx, fy):
    r = wintypes.RECT()
    u32.GetWindowRect(hwnd, ctypes.byref(r))
    u32.SetCursorPos(r.left + int((r.right - r.left) * fx), r.top + int((r.bottom - r.top) * fy))
    u32.mouse_event(0x0002, 0, 0, 0, 0)
    u32.mouse_event(0x0004, 0, 0, 0, 0)


def protocol(value=None):
    """Who opens discord:// links. Each client claims it when it starts; the run hands it back."""
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, r"Software\Classes\discord\shell\open\command") as k:
        if value is not None:
            return winreg.SetValueEx(k, "", 0, winreg.REG_SZ, value)
        try:
            return winreg.QueryValueEx(k, "")[0]
        except FileNotFoundError:
            return None


def channels(until):
    while time.time() < until:
        press(0x12, 0x28)
        time.sleep(1.5)


def servers(until):
    while time.time() < until:
        press(0x11, 0x12, 0x28)
        time.sleep(2)


def switcher(until):
    """Ctrl+K and arrows through the results, never a letter: typing where the switcher didn't open would tell the channel you're typing."""
    while time.time() < until:
        press(0x11, 0x4B)
        time.sleep(1.2)
        for _ in range(3):
            press(0x28)
            time.sleep(0.4)
        press(0x1B)
        time.sleep(1)


def emoji(until):
    while time.time() < until:
        press(0x11, 0x45)
        time.sleep(2.5)
        press(0x1B)
        time.sleep(1)


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


def hardware(on):
    s = json.loads(SETTINGS.read_text(encoding="utf-8"))
    s["enableHardwareAcceleration"] = on
    SETTINGS.write_text(json.dumps(s, indent=2), encoding="utf-8")


def stock():
    vencord_off()
    if json.loads(SETTINGS.read_text(encoding="utf-8")).get("enableHardwareAcceleration") is False:
        SAVED_HW.write_text("")
        hardware(True)
        print("Discord's hardware acceleration on for the run")


def restore():
    if SAVED_HW.exists():
        hardware(False)
        SAVED_HW.unlink()
        print("Discord's hardware acceleration off again")
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
    record = "--record" in sys.argv
    if record and not footage.FFMPEG:
        raise SystemExit("--record needs ffmpeg on PATH")
    shots = iter(range(1, 100))
    film = (lambda what, name: footage.Recording(out / "footage", f"{next(shots):02}-{what}-{name.lower()}")) if record else (lambda *_: footage.Off())
    report = {"when": stamp, "recorded": record, "uptime_min_at_start": round(up_min, 1), "machine_quiet": quiet(),
              "rustcord_exe": exe, "discord_version": sorted(p.name for p in DISCORD.glob("app-*"))[-1],
              "install_mb": {"Rustcord": size_mb(exe), "Discord": round(size_mb(sorted(DISCORD.glob("app-*"))[-1]) + size_mb(Path(os.environ["APPDATA"]) / "discord" / sorted(DISCORD.glob("app-*"))[-1].name[4:] / "modules"), 1)},
              "rect": RECT, "cold": {}, "warm": {"Rustcord": [], "Discord": []}, "session": {}}
    make = {"Rustcord": lambda: rustcord(link, exe, "voice" if arg("--stream") else "1"), "Discord": lambda: discord(link)}
    sheet, handler = curtain(), protocol()
    try:
        stock()
        report["hardware_acceleration_turned_on"] = SAVED_HW.exists()
        report["vencord"] = (resources() / "app.asar").stat().st_size < 4096
        for name in ("Rustcord", "Discord"):
            rec = film("cold", name)
            c = make[name]()
            rec.follow(c)
            report["cold"][name], _ = start(c)
            print(name, "cold", report["cold"][name], flush=True)
            c.kill()
            rec.stop()
            quiet(30)
        for i in range(warm):
            for name in (("Rustcord", "Discord") if i % 2 == 0 else ("Discord", "Rustcord")):
                rec = film(f"warm{i + 1}", name)
                c = make[name]()
                rec.follow(c)
                t, _ = start(c)
                report["warm"][name].append(t)
                print(name, "warm", t, flush=True)
                c.kill()
                rec.stop()
                quiet(30)
        for name in ("Rustcord", "Discord"):
            rec = film("session", name)
            c = make[name]()
            rec.follow(c)
            _, hwnd = start(c)
            s = report["session"][name] = {}
            if hwnd:
                time.sleep(30)
                front(hwnd)
                press(0x1B)
                time.sleep(3)
                s["idle"] = measure(c, "idle", secs)
                s["scroll"] = measure(c, "scroll", 20, lambda until: wheel(hwnd, until))
                s["switcher"] = measure(c, "switcher", 15, switcher)
                s["emoji"] = measure(c, "emoji", 15, emoji)
                if arg("--gifs"):
                    c.open(f"discord://-/channels/{arg('--gifs')}")
                    time.sleep(6)
                    front(hwnd)
                    s["gifs"] = measure(c, "gifs", 15)
                if arg("--video"):
                    c.open(f"discord://-/channels/{arg('--video')}")
                    time.sleep(6)
                    front(hwnd)
                    # the embed sits in a different place in each client (Rustcord anchors the lone
                    # message high, Discord renders it lower in the chat); click the play button at the
                    # middle of the embed so the click plays the video instead of opening a profile
                    click(hwnd, *((0.42, 0.56) if c.name == "Rustcord" else (0.61, 0.67)))
                    time.sleep(0.6)
                    s["video"] = measure(c, "video", 20)
                if arg("--stream"):
                    c.open(f"discord://-/channels/{arg('--stream')}")
                    time.sleep(6)
                    front(hwnd)
                    # Join Voice sits mid-channel and the stream's own tile leads the grid in both; the
                    # hang-up button in the panel over your name differs by a few pixels
                    click(hwnd, 0.647, 0.653)
                    time.sleep(6)
                    # going live: the panel's share button, Entire Screen (the picker opens on
                    # Applications, whose thumbnails are other windows), then the one screen. Discord
                    # gives an account without Nitro 720p at 30 fps, so Rustcord is set to the same
                    rc = c.name == "Rustcord"
                    click(hwnd, *((0.113, 0.881) if rc else (0.119, 0.872)))
                    time.sleep(3)
                    click(hwnd, *((0.509, 0.159) if rc else (0.5, 0.126)))
                    time.sleep(1.5)
                    if rc:
                        for fx, fy in ((0.848, 0.540), (0.853, 0.740)):
                            click(hwnd, 0.84, 0.844)
                            time.sleep(1.2)
                            click(hwnd, fx, fy)
                            time.sleep(1.2)
                    click(hwnd, *((0.316, 0.366) if rc else (0.316, 0.335)))
                    time.sleep(4)
                    s["golive"] = measure(c, "golive", 20)
                    click(hwnd, *((0.113, 0.881) if rc else (0.27, 0.745)))
                    time.sleep(4)
                    click(hwnd, 0.473, 0.357)
                    time.sleep(4)
                    s["stream"] = measure(c, "stream", 20)
                    click(hwnd, *((0.264, 0.825) if c.name == "Rustcord" else (0.27, 0.818)))
                    time.sleep(2)
                    c.open(f"discord://-/channels/{arg('--video') or arg('--gifs') or arg('--channel')}")
                    time.sleep(4)
                    front(hwnd)
                s["channels"] = measure(c, "channels", 30, channels)
                s["servers"] = measure(c, "servers", 20, servers)
                time.sleep(5)
                s["after_use"] = measure(c, "after_use", 15)
                u32.ShowWindow(hwnd, 6)
                c.mark("minimize")
                time.sleep(10)
                s["minimized"] = measure(c, "minimized", secs)
                u32.ShowWindow(hwnd, 9)  # restore before killing, so it doesn't start minimised next time
            print(name, "session", json.dumps(s), flush=True)
            c.kill()
            rec.stop()
            quiet(30)
    finally:
        sheet.kill()
        if handler:
            protocol(handler)
        restore()
    report["warm_median"] = {name: {k: median([r[k] for r in runs]) for k in ("first_window_ms", "main_window_ms", "shown_ms", "settled_ms", "live_ms")} for name, runs in report["warm"].items()}
    (out / "report.json").write_text(json.dumps(report, indent=2))
    (out / "report.md").write_text(markdown(report))
    print(markdown(report))
    if record:
        print(cut.cut(out))
    print(f"\n{out}")


def median(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def markdown(r):
    rows = [("Install size (MB)", lambda n: r["install_mb"][n])]
    for when in ("cold", "warm_median"):
        for k in ("first_window_ms", "main_window_ms", "shown_ms", "settled_ms", "live_ms"):
            rows.append((f"{when.split('_')[0]} start: {k[:-3].replace('_', ' ')} (ms)", lambda n, w=when, k=k: r.get(w, {}).get(n, {}).get(k)))
    for phase in ("idle", "scroll", "switcher", "emoji", "gifs", "video", "golive", "stream", "channels", "servers", "after_use", "minimized"):
        rows.append((f"{phase}: CPU % of a core", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("cpu_pct_of_one_core")))
        rows.append((f"{phase}: GPU 3D %", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("gpu_3d_pct")))
        rows.append((f"{phase}: Task Manager memory (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("memory_mb", {}).get("task_manager")))
        rows.append((f"{phase}: private commit (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("memory_mb", {}).get("private")))
        rows.append((f"{phase}: GPU memory (MB)", lambda n, p=phase: r["session"].get(n, {}).get(p, {}).get("gpu_mb")))
    rows.append(("processes", lambda n: r["session"].get(n, {}).get("idle", {}).get("memory_mb", {}).get("processes")))
    lines = [f"Rustcord vs Discord ({r['discord_version']}{', with Vencord' if r['vencord'] else ''}), {r['when']}, {r['uptime_min_at_start']} min after boot, machine quiet: {r['machine_quiet']}{', screen recorded during the run' if r.get('recorded') else ''}{", Discord's hardware acceleration turned on for it" if r.get('hardware_acceleration_turned_on') else ''}", "",
             "| | Rustcord | Discord |", "|---|---|---|"]
    lines += [f"| {label} | {f('Rustcord')} | {f('Discord')} |" for label, f in rows]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    main()
