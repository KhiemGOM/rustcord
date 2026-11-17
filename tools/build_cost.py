"""Times a command and the peak memory of its process tree (cargo, rustc, the linker), to compare
what a rebuild costs under each cargo profile.

    python tools/build_cost.py cargo build --profile iter --bin rustcord --example mockcord
"""

import sys
import time

import psutil

p = psutil.Popen(sys.argv[1:])
start, peak = time.time(), 0
while p.poll() is None:
    try:
        peak = max(peak, sum(c.memory_info().rss for c in [p, *p.children(recursive=True)]))
    except psutil.Error:
        pass
    time.sleep(0.5)
print(f"{time.time() - start:.0f} s, peak {peak / 2**30:.1f} GB, exit {p.returncode}")
sys.exit(p.returncode)
