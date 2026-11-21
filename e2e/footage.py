"""Screen recordings of the Discord comparison with its numbers on the same clock, to cut a video from.

`vs_discord.py run --record` films every launch of a client: the whole screen at 60 fps, encoded on the
GPU's video engine, plus <name>.csv (the client's CPU, memory and GPU every 250 ms) and
<name>.events.json (launch, first window, settled, each phase), both in seconds from the video's
first frame. `python e2e/cut.py <run dir>` cuts them into one video.
"""
import atexit
import csv
import json
import shutil
import subprocess
import threading
import time
from datetime import datetime

FFMPEG = shutil.which("ffmpeg")
CAPTURE = ["-c:v", "hevc_amf", "-quality", "quality", "-rc", "cqp", "-qp_i", "16", "-qp_p", "18", "-tag:v", "hvc1"]
ENCODE = ["-c:v", "h264_amf", "-quality", "quality", "-rc", "cqp", "-qp_i", "16", "-qp_p", "18", "-pix_fmt", "yuv420p"]
GPU_COUNTERS = [r"\GPU Process Memory(*)\Dedicated Usage", r"\GPU Engine(*engtype_3D)\Utilization Percentage"]


class Off:
    def follow(self, c):
        pass

    def stop(self):
        pass


class Recording:
    def __init__(self, folder, name):
        folder.mkdir(parents=True, exist_ok=True)
        self.base = folder / name
        self.clients, self.rows, self.gpu = [], [], {}
        self.t0 = None
        self.done = threading.Event()
        cmd = [FFMPEG, "-hide_banner", "-loglevel", "error", "-progress", "pipe:1", "-stats_period", "0.05",
               "-f", "lavfi", "-i", "ddagrab=output_idx=0:framerate=60:draw_mouse=1", *CAPTURE, "-y", str(self.base.with_suffix(".mp4"))]
        self.ff = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        threading.Thread(target=self.clock, daemon=True).start()
        atexit.register(self.end_video)
        end = time.time() + 10
        while self.t0 is None and time.time() < end:
            time.sleep(0.02)
        if self.t0 is None:
            raise SystemExit(f"ffmpeg didn't start recording {self.base.name}")
        time.sleep(0.5)

    def clock(self):
        for line in self.ff.stdout:
            k, _, v = line.partition("=")
            if k == "out_time_us" and v.strip().isdigit() and int(v) > 0:
                est = time.time() - int(v) / 1e6
                self.t0 = est if self.t0 is None else min(self.t0, est)

    def end_video(self):
        if self.ff.poll() is None:
            self.ff.stdin.write("q")
            self.ff.stdin.flush()
            self.ff.wait(60)

    def follow(self, c):
        self.clients.append(c)
        threading.Thread(target=self.sample, args=(c,), daemon=True).start()

    def sample(self, c):
        a, t = c.cpu(), time.time()
        while not self.done.wait(0.25):
            b, now = c.cpu(), time.time()
            m = c.memory()
            self.rows.append({"t": now, "client": c.name, "cpu_pct_of_one_core": round((b - a) / (now - t) * 100, 1),
                              "task_manager_mb": m["task_manager"], "private_mb": m["private"], "working_set_mb": m["working_set"], "processes": m["processes"]})
            a, t = b, now
            if c.name not in self.gpu and any(e == "main_window" for e, _ in c.events):
                path = self.base.parent / f"{self.base.name}-{c.name.lower()}-gpu.tmp.csv"
                self.gpu[c.name] = (path, subprocess.Popen(["typeperf", *GPU_COUNTERS, "-si", "1", "-o", str(path), "-f", "CSV", "-y"], stdout=subprocess.DEVNULL))

    def stop(self):
        self.done.set()
        time.sleep(0.3)
        self.end_video()
        gpu = {}
        for name, (path, p) in self.gpu.items():
            p.terminate()
            p.wait(10)
            c = next(c for c in self.clients if c.name == name)
            gpu[name] = gpu_rows(path, set(c.handles))
            path.unlink(missing_ok=True)
        rel = lambda t: round(t - self.t0, 3)
        fields = ["t", "client", "cpu_pct_of_one_core", "task_manager_mb", "private_mb", "working_set_mb", "processes", "gpu_mb", "gpu_3d_pct"]
        with open(self.base.with_suffix(".csv"), "w", newline="") as f:
            w = csv.DictWriter(f, fields)
            w.writeheader()
            for r in sorted(self.rows, key=lambda r: r["t"]):
                g = [s for s in gpu.get(r["client"], []) if s[0] <= r["t"]]
                w.writerow({**r, "t": rel(r["t"]), "gpu_mb": g[-1][1] if g else "", "gpu_3d_pct": g[-1][2] if g else ""})
        events = sorted(({"t": rel(t), "client": c.name, "event": e} for c in self.clients for e, t in c.events), key=lambda e: e["t"])
        self.base.with_suffix(".events.json").write_text(json.dumps(events, indent=1))


def gpu_rows(path, pids):
    """(wall time, dedicated MB, 3D %) for each typeperf row, summed over pids."""
    try:
        rows = list(csv.reader(open(path, newline="")))
    except OSError:
        return []
    if len(rows) < 2:
        return []
    tags = tuple(f"pid_{p}_" for p in pids)
    cols = [(i, "Dedicated Usage" in h) for i, h in enumerate(rows[0]) if i and any(t in h for t in tags)]
    out = []
    for r in rows[1:]:
        try:
            t = datetime.strptime(r[0], "%m/%d/%Y %H:%M:%S.%f").timestamp()
        except ValueError:
            continue
        num = lambda i: float(r[i]) if i < len(r) and r[i].strip() else 0.0
        out.append((t, round(sum(num(i) for i, mem in cols if mem) / 2**20), round(sum(num(i) for i, mem in cols if not mem), 1)))
    return out

