"""Compiles vendor/eframe/src/native/d3d11.hlsl to the bytecode the D3D11 painter embeds. Rerun after editing the shader.

    python tools/hlsl.py
"""
import ctypes
from ctypes import wintypes
from pathlib import Path

NATIVE = Path(__file__).resolve().parent.parent / "vendor" / "eframe" / "src" / "native"
fxc = ctypes.WinDLL("d3dcompiler_47")


def call(blob, slot, restype):
    vtbl = ctypes.cast(blob, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    return ctypes.WINFUNCTYPE(restype, ctypes.c_void_p)(vtbl[slot])(blob)


def compile(src, entry, target):
    code, errors = ctypes.c_void_p(), ctypes.c_void_p()
    hr = fxc.D3DCompile(src, len(src), b"d3d11.hlsl", None, None, entry, target, 1 << 15, 0, ctypes.byref(code), ctypes.byref(errors))
    if hr != 0:
        msg = ctypes.string_at(call(errors, 3, ctypes.c_void_p), call(errors, 4, ctypes.c_size_t)) if errors else b""
        raise SystemExit(f"{entry.decode()}: {hr & 0xffffffff:#x} {msg.decode(errors='replace')}")
    data = ctypes.string_at(call(code, 3, ctypes.c_void_p), call(code, 4, ctypes.c_size_t))
    call(code, 2, wintypes.ULONG)
    return data


src = (NATIVE / "d3d11.hlsl").read_bytes()
for entry, target, out in [(b"vs_main", b"vs_4_0", "d3d11_vs.cso"), (b"ps_main", b"ps_4_0", "d3d11_ps.cso")]:
    (NATIVE / out).write_bytes(compile(src, entry, target))
    print(out, (NATIVE / out).stat().st_size, "bytes")
