"""Where a running client's threads spend their time: a sampling profiler that needs no admin.

    python tools/sample.py <pid> [--secs 10] [--thread main] [--top 40] [--symbols target/fast] [--callers RtlAllocateHeap]

Suspends the thread every few ms, walks its stack and names the frames from the exe's pdb, so
build with symbols (the `fast` profile keeps line tables; release strips them). --thread takes a
thread's name, `main` for the oldest, or `all`. Prints the functions on top of the stack (self)
and anywhere in it (total), as a share of the samples that weren't waiting. The thread runs
slower while sampled, so read shares, not times. --callers adds, for a function called from
everywhere, which of our own functions it runs under. `e2e/bench.py` prints the pids it starts.
"""
import ctypes
import sys
import time
from collections import Counter
from ctypes import wintypes
from pathlib import Path

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
dbg = ctypes.WinDLL("dbghelp", use_last_error=True)
k32.OpenProcess.restype = k32.OpenThread.restype = k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
k32.GetThreadDescription.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_wchar_p)]
k32.GetThreadContext.argtypes = [wintypes.HANDLE, ctypes.c_void_p]
k32.SuspendThread.argtypes = k32.ResumeThread.argtypes = [wintypes.HANDLE]
k32.SuspendThread.restype = wintypes.DWORD
dbg.SymInitializeW.argtypes = [wintypes.HANDLE, wintypes.LPCWSTR, wintypes.BOOL]
dbg.SymFromAddrW.argtypes = [wintypes.HANDLE, ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint64), ctypes.c_void_p]
dbg.SymFunctionTableAccess64.restype = ctypes.c_void_p
dbg.SymFunctionTableAccess64.argtypes = [wintypes.HANDLE, ctypes.c_uint64]
dbg.SymGetModuleBase64.restype = ctypes.c_uint64
dbg.SymGetModuleBase64.argtypes = [wintypes.HANDLE, ctypes.c_uint64]
dbg.StackWalk64.argtypes = [wintypes.DWORD, wintypes.HANDLE, wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]


class ThreadEntry(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ThreadID", wintypes.DWORD), ("th32OwnerProcessID", wintypes.DWORD),
                ("tpBasePri", wintypes.LONG), ("tpDeltaPri", wintypes.LONG), ("dwFlags", wintypes.DWORD)]


def arg(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def threads(pid):
    """(thread id, name) oldest first, the oldest named main."""
    snap = k32.CreateToolhelp32Snapshot(0x4, 0)
    e = ThreadEntry()
    e.dwSize = ctypes.sizeof(e)
    rows = []
    ok = k32.Thread32First(snap, ctypes.byref(e))
    while ok:
        if e.th32OwnerProcessID == pid:
            t = k32.OpenThread(0x0800, False, e.th32ThreadID)
            if t:
                c, x, kt, ut = (wintypes.FILETIME() for _ in range(4))
                k32.GetThreadTimes(t, ctypes.byref(c), ctypes.byref(x), ctypes.byref(kt), ctypes.byref(ut))
                desc = ctypes.c_wchar_p()
                name = desc.value if k32.GetThreadDescription(t, ctypes.byref(desc)) >= 0 and desc.value else "?"
                rows.append((c.dwHighDateTime << 32 | c.dwLowDateTime, e.th32ThreadID, name))
                k32.CloseHandle(t)
        ok = k32.Thread32Next(snap, ctypes.byref(e))
    k32.CloseHandle(snap)
    rows.sort()
    return [(tid, "main" if i == 0 else name) for i, (_, tid, name) in enumerate(rows)]


def main():
    pid = int(sys.argv[1])
    secs, want, top = float(arg("--secs", 10)), arg("--thread", "main"), int(arg("--top", 40))
    symbols = str(Path(arg("--symbols", Path(__file__).resolve().parent.parent / "target" / "fast")).resolve())
    proc = k32.OpenProcess(0x0400 | 0x0010, False, pid)
    if not proc:
        raise SystemExit(f"can't open {pid}: {ctypes.get_last_error()}")
    dbg.SymSetOptions(0x2 | 0x4)
    if not dbg.SymInitializeW(proc, symbols, True):
        raise SystemExit(f"SymInitialize: {ctypes.get_last_error()}")
    picked = [(tid, name) for tid, name in threads(pid) if want in ("all", name)]
    handles = [(k32.OpenThread(0x0002 | 0x0008 | 0x0040, False, tid), name) for tid, name in picked]
    # CONTEXT is 1232 bytes and must sit on a 16 byte boundary; STACKFRAME64 is 264
    raw = ctypes.create_string_buffer(1232 + 16)
    ctx = ctypes.addressof(raw) + (-ctypes.addressof(raw) % 16)
    frame = ctypes.create_string_buffer(512)
    u64 = lambda at: ctypes.c_uint64.from_address(at)
    stacks = []
    end = time.time() + secs
    while time.time() < end:
        for h, _ in handles:
            if k32.SuspendThread(h) == 0xFFFFFFFF:
                continue
            pcs = []
            # an interrupt here must not leave the client's thread suspended
            try:
                ctypes.memset(ctx, 0, 1232)
                ctypes.c_uint32.from_address(ctx + 0x30).value = 0x100003
                if k32.GetThreadContext(h, ctx):
                    ctypes.memset(frame, 0, 512)
                    base = ctypes.addressof(frame)
                    for off, reg in ((0, 0xF8), (32, 0xA0), (48, 0x98)):
                        u64(base + off).value = u64(ctx + reg).value
                        ctypes.c_uint32.from_address(base + off + 12).value = 3
                    while len(pcs) < 96 and dbg.StackWalk64(0x8664, proc, h, frame, ctx, None, dbg.SymFunctionTableAccess64, dbg.SymGetModuleBase64, None):
                        pc = u64(base).value
                        if not pc:
                            break
                        pcs.append(pc)
            finally:
                k32.ResumeThread(h)
            if pcs:
                stacks.append(pcs)
        time.sleep(0.002)

    info = ctypes.create_string_buffer(88 + 2 * 512)
    names = {}

    def name(pc):
        if pc not in names:
            ctypes.memset(info, 0, len(info))
            ctypes.c_uint32.from_address(ctypes.addressof(info)).value = 88
            ctypes.c_uint32.from_address(ctypes.addressof(info) + 80).value = 512
            moved = ctypes.c_uint64()
            if dbg.SymFromAddrW(proc, pc, ctypes.byref(moved), info):
                names[pc] = ctypes.wstring_at(ctypes.addressof(info) + 84)
            else:
                mod = dbg.SymGetModuleBase64(proc, pc)
                names[pc] = f"?{mod:#x}"
        return names[pc]

    waits = ("NtWait", "ZwWait", "NtUserMsgWait", "NtUserGetMessage", "NtDelayExecution", "NtRemoveIoCompletion", "NtUserWaitMessage", "NtYieldExecution")
    busy = [[name(pc) for pc in s] for s in stacks]
    busy = [s for s in busy if not s[0].startswith(waits)]
    print(f"{len(stacks)} samples of {', '.join(sorted({n for _, n in picked}))}, {len(busy)} not waiting")
    if not busy:
        return
    own, total = Counter(s[0] for s in busy), Counter(f for s in busy for f in set(s))
    tables = [("self", own), ("total", total)]
    if "--callers" in sys.argv:
        # for something called from everywhere (the allocator): which of our functions it's under
        of = arg("--callers", "")
        ours = Counter(next((f for f in s[i:] if f.startswith("rustcord::")), "?") for s in busy for i in [next((i for i, f in enumerate(s) if f.startswith(of)), None)] if i is not None)
        tables.append((f"nearest rustcord:: caller of {of}", ours))
    for title, counts in tables:
        print(f"\n{title}")
        for f, n in counts.most_common(top):
            print(f"{n / len(busy) * 100:6.1f}%  {f[:150]}")


if __name__ == "__main__":
    main()
