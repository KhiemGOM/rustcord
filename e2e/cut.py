"""Cuts a recorded comparison into one video: each interaction with both clients side by side and their memory, CPU and GPU under them.

    python e2e/cut.py <run dir> [--secs 10]

Needs a `vs_discord.py run --record` run. It opens on the first launch and a later one, then each
interaction. The clients were filmed one after the other; each clip
starts both at the same moment of the interaction (launch, first scroll, ...), and the numbers come
from the CSV recorded with each video. Writes <run dir>/comparison.mp4, 2560x1440 at 60 fps.
"""
import csv
import json
import subprocess
import sys
from bisect import bisect_right
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from footage import ENCODE, FFMPEG

W, H, FPS = 2560, 1440, 60
VW, VH, VY = 1248, 780, 236
VX = (24, 24 + VW + 16)
CHART = (24, 1196, W - 24, 1416)
BG, PANEL, TEXT, DIM, GRID = (17, 18, 20), (30, 31, 34), (242, 243, 245), (148, 155, 164), (52, 54, 59)
NAMES = ("Rustcord", "Discord")
COLOR = {"Rustcord": (232, 116, 59), "Discord": (88, 101, 242)}
DISCLAIMER = [
    "Measured on a throwaway account made for this test, on one PC, one run each; your own numbers will vary.",
    "Discord is the stock client with hardware acceleration on (its default on this PC was off, which draws on the CPU).",
    "Memory is what Task Manager shows; committed memory is higher and is not under 100 MB on a full account.",
]
PHASES = [("scroll", "Scrolling the chat"), ("switcher", "The quick switcher"), ("emoji", "The emoji picker"),
          ("gifs", "GIFs in chat"), ("video", "Playing a video"), ("golive", "Sharing your screen"), ("stream", "Watching a stream"),
          ("channels", "Switching channels"), ("servers", "Switching servers"), ("idle", "Sitting idle"), ("minimized", "Minimized")]


def font(size, bold=False):
    return ImageFont.truetype(f"C:/Windows/Fonts/{'segoeuib' if bold else 'segoeui'}.ttf", size)


class Take:
    """One client's video with its events and samples."""

    def __init__(self, base, client):
        self.video = base.with_suffix(".mp4")
        self.ev = {}
        for e in json.loads(base.with_suffix(".events.json").read_text()):
            if e["client"] == client:
                self.ev.setdefault(e["event"], e["t"])
        self.rows = [r for r in csv.DictReader(open(base.with_suffix(".csv"), newline="")) if r["client"] == client]
        self.ts = [float(r["t"]) for r in self.rows]

    def at(self, t):
        i = bisect_right(self.ts, t) - 1
        return self.rows[i] if i >= 0 else {}

    def cpu(self, t, over=1.0):
        rows = self.rows[bisect_right(self.ts, t - over):bisect_right(self.ts, t)]
        return sum(float(r["cpu_pct_of_one_core"]) for r in rows) / len(rows) if rows else None

    def mem(self, t0, t1):
        return [(float(r["t"]) - t0, int(r["task_manager_mb"])) for r in self.rows if t0 <= float(r["t"]) <= t1]


def num(v):
    return int(float(v)) if v not in (None, "") else None


def backdrop(title, sub):
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    d.text((24, 28), title, font=font(72, True), fill=TEXT)
    d.text((26, 118), sub, font=font(30), fill=DIM)
    for x, name in zip(VX, NAMES):
        d.rectangle((x, VY, x + VW - 1, VY + VH - 1), fill=PANEL)
        d.text((x, VY - 12), name, font=font(44, True), fill=COLOR[name], anchor="ld")
    d.rectangle(CHART, fill=PANEL)
    d.text((CHART[0] + 20, CHART[1] + 14), "Memory over this clip (MB, as Task Manager shows it)", font=font(26), fill=DIM)
    return img


def segment(out, title, sub, takes, starts, secs, rect, launch=False):
    """Renders the panel frame by frame into ffmpeg, which lays both clips over it."""
    n = round(secs * FPS)
    starts = [max(0.0, s) for s in starts]
    x, y, w, h = rect
    args = [FFMPEG, "-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-"]
    # a launch is closed once it's live; past that its crop is the sheet behind it, so its last
    # picture and numbers are held instead
    ends = [tk.ev["launch_end"] - 0.2 if launch and "launch_end" in tk.ev else s + secs for tk, s in zip(takes, starts)]
    for tk, s, end in zip(takes, starts, ends):
        args += ["-ss", f"{s:.3f}", "-t", f"{max(0.1, end - s):.3f}", "-i", str(tk.video)]
    # before the app's window appears, its crop is the desktop behind it; black that out so a cold
    # launch never shows what was on screen first.
    appear = lambda tk, s: max(0.0, next((tk.ev[e] for e in ("first_window", "main_window", "settled") if e in tk.ev), s) - s)
    blk = [appear(tk, s) if launch else 0.0 for tk, s in zip(takes, starts)]
    box = lambda i: f",drawbox=x=0:y=0:w=iw:h=ih:color=black:t=fill:enable='lt(t\\,{blk[i]:.3f})'" if blk[i] > 0 else ""
    clip = lambda i: f"[{i + 1}:v]crop={w}:{h}:{x}:{y},scale={VW}:{VH}{box(i)},fps={FPS},tpad=stop_mode=clone:stop_duration={secs + 1}[v{i}]"
    graph = f"{clip(0)};{clip(1)};[0:v][v0]overlay={VX[0]}:{VY}[a];[a][v1]overlay={VX[1]}:{VY}"
    ff = subprocess.Popen(args + ["-filter_complex", graph, "-frames:v", str(n), *ENCODE, "-r", str(FPS), "-y", str(out)], stdin=subprocess.PIPE)

    base = backdrop(title, sub)
    mems = [tk.mem(s, min(s + secs, end)) for tk, s, end in zip(takes, starts, ends)]
    top = max([m for series in mems for _, m in series] + [1]) * 1.15
    cx0, cy0, cx1, cy1 = CHART[0] + 90, CHART[1] + 60, CHART[2] - 20, CHART[3] - 20
    px = lambda t, m: (cx0 + (cx1 - cx0) * t / secs, cy1 - (cy1 - cy0) * m / top)
    d = ImageDraw.Draw(base)
    for frac in (0, 0.5, 1):
        yy = cy1 - (cy1 - cy0) * frac
        d.line((cx0, yy, cx1, yy), fill=GRID, width=2)
        d.text((cx0 - 12, yy), f"{round(top * frac)}", font=font(24), fill=DIM, anchor="rm")
    big, small, label = font(84, True), font(32), font(26)
    for k in range(n):
        tau = k / FPS
        img = base.copy()
        d = ImageDraw.Draw(img)
        for i, (name, tk, s, series) in enumerate(zip(NAMES, takes, starts, mems)):
            now = min(s + tau, ends[i])
            row, x0 = tk.at(now), VX[i]
            mem, gpu, procs, cpu = num(row.get("task_manager_mb")), num(row.get("gpu_mb")), num(row.get("processes")), tk.cpu(now)
            yb = VY + VH + 14
            d.text((x0, yb), f"{mem} MB" if mem is not None else "-", font=big, fill=COLOR[name])
            d.text((x0 + 4, yb + 100), "memory", font=label, fill=DIM)
            lines = [f"CPU {cpu:.1f}% of a core" if cpu is not None else "CPU -", f"GPU memory {gpu} MB" if gpu is not None else "GPU memory -",
                     f"{procs} process{'es' if procs != 1 else ''}" if procs else ""]
            for j, line in enumerate(lines):
                d.text((x0 + VW, yb + 6 + j * 42), line, font=small, fill=TEXT, anchor="ra")
            if launch:
                t = max(0.0, tau - (tk.ev["launch"] - s))
                first, seen, live = (tk.ev.get(e, 1e9) - tk.ev["launch"] for e in ("first_window", "settled", "live"))
                live = max(live, seen) if live < 1e8 else live
                if t >= live:
                    text = f"On screen and live in {live:.2f} s" if live == seen else f"On screen in {seen:.2f} s, live in {live:.2f} s"
                elif t >= seen:
                    text = f"On screen in {seen:.2f} s, connecting"
                elif t >= first:
                    text = f"Window in {first:.2f} s"
                else:
                    text = f"{t:.2f} s"
                d.rounded_rectangle((x0 + VW - 700, VY - 70, x0 + VW, VY - 10), 14, fill=COLOR[name] if t >= live else PANEL)
                d.text((x0 + VW - 350, VY - 40), text, font=font(34, True), fill=TEXT, anchor="mm")
            pts = [px(t, m) for t, m in series if t <= tau]
            if len(pts) > 1:
                d.line(pts, fill=COLOR[name], width=5, joint="curve")
        ff.stdin.write(img.tobytes())
    ff.stdin.close()
    if ff.wait():
        raise SystemExit(f"ffmpeg failed on {out.name}")


def summary(out, r, secs=6.0):
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    d.text((W // 2, 150), "Rustcord vs Discord", font=font(96, True), fill=TEXT, anchor="mm")
    s = r["session"]
    get = lambda n, *path: _dig(s.get(n, {}), *path)
    took = lambda ms: f"{ms / 1000:.2f} s" if ms else "-"
    live = lambda t: max(t.get("live_ms") or 0, t.get("settled_ms") or 0)
    shown_c = {n: r["cold"].get(n, {}).get("settled_ms") for n in NAMES}
    shown_w = {n: r.get("warm_median", {}).get(n, {}).get("settled_ms") for n in NAMES}
    # "live" (gateway connected) only earns its own row when it lands clearly after the window shows
    sep = any((live(r["cold"].get(n, {})) or 0) - (shown_c[n] or 0) > 300 for n in NAMES)
    rows = [("First window, first launch", lambda n: took(r["cold"].get(n, {}).get("first_window_ms"))),
            ("On screen, first launch", lambda n: took(shown_c[n]))]
    if sep:
        rows.append(("Live, first launch", lambda n: took(live(r["cold"].get(n, {})))))
    rows.append(("On screen, opening it again", lambda n: took(shown_w[n])))
    if sep:
        rows.append(("Live, opening it again", lambda n: took(live(r.get("warm_median", {}).get(n, {})))))
    rows += [
            ("Memory, idle (Task Manager)", lambda n: f"{get(n, 'idle', 'memory_mb', 'task_manager')} MB"),
            ("Memory committed, idle", lambda n: f"{get(n, 'idle', 'memory_mb', 'private')} MB"),
            ("GPU memory, idle", lambda n: f"{get(n, 'idle', 'gpu_mb')} MB"),
            ("CPU while scrolling", lambda n: f"{get(n, 'scroll', 'cpu_pct_of_one_core')}% of a core"),
            ("CPU idle", lambda n: f"{get(n, 'idle', 'cpu_pct_of_one_core')}% of a core"),
            ("Processes", lambda n: f"{get(n, 'idle', 'memory_mb', 'processes')}"),
            ("Size on disk", lambda n: f"{r['install_mb'][n]} MB")]
    cols = (500, 1500, 2150)
    d.text((cols[1], 300), "Rustcord", font=font(56, True), fill=COLOR["Rustcord"], anchor="mm")
    d.text((cols[2], 300), "Discord", font=font(56, True), fill=COLOR["Discord"], anchor="mm")
    for i, (label, f) in enumerate(rows):
        yy = 384 + i * 78
        d.line((240, yy - 50, W - 240, yy - 50), fill=GRID, width=2)
        d.text((240, yy), label, font=font(44), fill=DIM, anchor="lm")
        d.text((cols[1], yy), f("Rustcord"), font=font(54, True), fill=TEXT, anchor="mm")
        d.text((cols[2], yy), f("Discord"), font=font(54, True), fill=TEXT, anchor="mm")
    for i, line in enumerate(DISCLAIMER):
        d.text((240, 1330 + i * 34), line, font=font(24), fill=DIM, anchor="lm")
    png = out.with_suffix(".png")
    img.save(png)
    subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-loop", "1", "-framerate", str(FPS), "-i", str(png), "-frames:v", str(round(secs * FPS)),
                    *ENCODE, "-r", str(FPS), "-y", str(out)], check=True)
    png.unlink()


def _dig(d, *path):
    for k in path:
        d = d.get(k, {}) if isinstance(d, dict) else {}
    return d if d != {} else "-"


def cut(run, secs=10.0):
    run = Path(run)
    r = json.loads((run / "report.json").read_text())
    folder, parts = run / "footage", run / "footage" / "cut"
    parts.mkdir(exist_ok=True)
    take = lambda what, name: Take(next(folder.glob(f"*-{what}-{name.lower()}.mp4")).with_suffix(""), name)
    sub = f"Rustcord and Discord {r['discord_version'].removeprefix('app-')} on the same account, filmed one after the other on the same PC"
    clips = []
    for what, title in (("cold", "Opening the app for the first time"), ("warm1", "Opening it again")):
        if not all(list(folder.glob(f"*-{what}-{n.lower()}.mp4")) for n in NAMES):
            continue
        takes = [take(what, n) for n in NAMES]
        ready = max(max(tk.ev.get(e, 0) for e in ("settled", "live", "main_window")) - tk.ev["launch"] for tk in takes)
        clips.append(parts / f"00-{what}.mp4")
        segment(clips[-1], title, sub, takes, [tk.ev["launch"] - 0.1 for tk in takes], min(ready + 3, 25), r["rect"], launch=True)
    session = [take("session", n) for n in NAMES]
    for i, (phase, title) in enumerate(PHASES, 1):
        if all(phase in tk.ev for tk in session):
            clips.append(parts / f"{i:02}-{phase}.mp4")
            segment(clips[-1], title, sub, session, [tk.ev[phase] for tk in session], secs, r["rect"])
    clips.append(parts / "99-summary.mp4")
    summary(clips[-1], r)
    listing = parts / "list.txt"
    listing.write_text("".join(f"file '{c.name}'\n" for c in clips))
    out = run / "comparison.mp4"
    subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing), "-c", "copy", "-movflags", "+faststart", "-y", str(out)], check=True)
    return out


if __name__ == "__main__":
    a = sys.argv[1:]
    print(cut(a[0], float(a[a.index("--secs") + 1]) if "--secs" in a else 10.0))
