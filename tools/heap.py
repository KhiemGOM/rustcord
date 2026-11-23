"""Names the call stacks a `--features heapprof` build wrote, while that client is still running.

    python tools/heap.py <pid> <file> [--top 40] [--symbols target/fast]

<file> is what the client writes every five seconds under RUSTCORD_HEAPPROF=<file> (src/heapprof.rs):
live bytes per call stack. Prints the bytes held under each of our own functions (the nearest
`rustcord::` frame of each stack, then every frame), and the heaviest whole stacks.
"""
import ctypes
import sys
from collections import Counter
from ctypes import wintypes
from pathlib import Path

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
dbg = ctypes.WinDLL("dbghelp", use_last_error=True)
k32.OpenProcess.restype = wintypes.HANDLE
dbg.SymInitializeW.argtypes = [wintypes.HANDLE, wintypes.LPCWSTR, wintypes.BOOL]
dbg.SymFromAddrW.argtypes = [wintypes.HANDLE, ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64), ctypes.c_void_p]


def arg(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def main():
    pid, path, top = int(sys.argv[1]), Path(sys.argv[2]), int(arg("--top", 40))
    symbols = str(Path(arg("--symbols", Path(__file__).resolve().parent.parent / "target" / "fast")).resolve())
    proc = k32.OpenProcess(0x0400 | 0x0010, False, pid)
    if not proc:
        raise SystemExit(f"can't open {pid}: {ctypes.get_last_error()}")
    dbg.SymSetOptions(0x2 | 0x4)
    if not dbg.SymInitializeW(proc, symbols, True):
        raise SystemExit(f"SymInitialize: {ctypes.get_last_error()}")
    info = ctypes.create_string_buffer(88 + 2 * 512)
    names = {}

    def name(pc):
        if pc not in names:
            ctypes.memset(info, 0, len(info))
            ctypes.c_uint32.from_address(ctypes.addressof(info)).value = 88
            ctypes.c_uint32.from_address(ctypes.addressof(info) + 80).value = 512
            moved = ctypes.c_uint64()
            names[pc] = ctypes.wstring_at(ctypes.addressof(info) + 84) if dbg.SymFromAddrW(proc, pc, ctypes.byref(moved), info) else "?"
        return names[pc]

    rows = []
    for line in path.read_text().splitlines():
        # a .peak file starts with the commit it was taken at
        if len(line.split()) == 1:
            print(f"taken at {line} MB of commit")
            continue
        size, count, *stack = line.split()
        rows.append((int(size), int(count), [name(int(a, 16)) for a in stack if a != "0"]))
    total = sum(r[0] for r in rows)
    mb = lambda b: f"{b / 2**20:7.2f} MB"
    print(f"{mb(total)} followed in {sum(r[1] for r in rows)} allocations, {len(rows)} stacks")
    nearest, anywhere = Counter(), Counter()
    for size, _, stack in rows:
        nearest[next((f for f in stack if f.startswith("rustcord::") and "heapprof" not in f), stack[-1] if stack else "?")] += size
        for f in set(stack):
            anywhere[f] += size
    for title, counts in (("nearest rustcord:: frame", nearest), ("anywhere in the stack", anywhere)):
        print(f"\n{title}")
        for f, n in counts.most_common(top):
            print(f"{mb(n)}  {f[:140]}")
    print("\nheaviest stacks")
    for size, count, stack in rows[:12]:
        print(f"{mb(size)} in {count}")
        for f in stack[:12]:
            print(f"      {f[:130]}")


if __name__ == "__main__":
    main()
