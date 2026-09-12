"""Scan a live GeForce NOW process for type-10 / mouse-settings patterns."""

from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import sys

PROCESS_QUERY_INFORMATION = 0x0400
PROCESS_VM_READ = 0x0010
MEM_COMMIT = 0x1000
MEM_PRIVATE = 0x20000
PAGE_NOACCESS = 0x01
PAGE_GUARD = 0x100

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)


class MEMORY_BASIC_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("BaseAddress", ctypes.c_void_p),
        ("AllocationBase", ctypes.c_void_p),
        ("AllocationProtect", wt.DWORD),
        ("PartitionId", wt.WORD),
        ("RegionSize", ctypes.c_size_t),
        ("State", wt.DWORD),
        ("Protect", wt.DWORD),
        ("Type", wt.DWORD),
    ]


kernel32.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
kernel32.OpenProcess.restype = wt.HANDLE
kernel32.ReadProcessMemory.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
]
kernel32.ReadProcessMemory.restype = wt.BOOL
kernel32.VirtualQueryEx.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    ctypes.POINTER(MEMORY_BASIC_INFORMATION),
    ctypes.c_size_t,
]
kernel32.VirtualQueryEx.restype = ctypes.c_size_t
kernel32.CloseHandle.argtypes = [wt.HANDLE]
kernel32.CloseHandle.restype = wt.BOOL

PATTERNS = {
    "wire_0323_speed10_accel0": bytes.fromhex("230308000a00000000000000"),
    "wire_0323_accel0_speed10": bytes.fromhex("23030800000000000a000000"),
    "wire_0323_speed10_accel1": bytes.fromhex("230308000a00000001000000"),
    "wire_0323_hdr_only": bytes.fromhex("23030800"),
    "nvb_client_from_log": bytes.fromhex("70c5dc1029020000"),
    "nvb_type10_accel0_speed10": bytes.fromhex("0a000000000000000a000000"),
}


def open_process(pid: int) -> wt.HANDLE:
    handle = kernel32.OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, False, pid)
    if not handle:
        raise OSError(f"OpenProcess({pid}) failed: {ctypes.get_last_error()}")
    return handle


def iter_committed(handle: wt.HANDLE):
    address = 0
    mbi = MEMORY_BASIC_INFORMATION()
    while True:
        got = kernel32.VirtualQueryEx(handle, ctypes.c_void_p(address), ctypes.byref(mbi), ctypes.sizeof(mbi))
        if not got:
            break
        base = mbi.BaseAddress or 0
        size = mbi.RegionSize
        readable = (
            mbi.State == MEM_COMMIT
            and mbi.Type == MEM_PRIVATE
            and not (mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD))
        )
        if readable and size and size < 64 * 1024 * 1024:
            yield base, size
        nxt = base + size
        if nxt <= address:
            break
        address = nxt


def read_region(handle: wt.HANDLE, base: int, size: int) -> bytes | None:
    buf = (ctypes.c_ubyte * size)()
    n = ctypes.c_size_t()
    ok = kernel32.ReadProcessMemory(handle, ctypes.c_void_p(base), buf, size, ctypes.byref(n))
    if not ok or n.value == 0:
        return None
    return bytes(buf[: n.value])


def find_all(hay: bytes, needle: bytes) -> list[int]:
    out = []
    start = 0
    while True:
        i = hay.find(needle, start)
        if i < 0:
            break
        out.append(i)
        start = i + 1
        if len(out) >= 64:
            break
    return out


def hexdump(data: bytes) -> str:
    return " ".join(f"{b:02x}" for b in data)


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: _scan_gfn_type10.py PID [PID...]")
        return 2
    for pid_s in sys.argv[1:]:
        pid = int(pid_s)
        print(f"===== PID {pid} =====", flush=True)
        try:
            handle = open_process(pid)
        except OSError as exc:
            print(exc)
            continue
        counts = {name: 0 for name in PATTERNS}
        samples = {name: [] for name in PATTERNS}
        regions = 0
        scanned = 0
        try:
            for base, size in iter_committed(handle):
                regions += 1
                blob = read_region(handle, base, size)
                if not blob:
                    continue
                scanned += len(blob)
                for name, pat in PATTERNS.items():
                    for off in find_all(blob, pat):
                        counts[name] += 1
                        if len(samples[name]) < 4:
                            lo = max(0, off - 16)
                            hi = min(len(blob), off + len(pat) + 16)
                            samples[name].append((base + off, blob[lo:hi], off - lo))
        finally:
            kernel32.CloseHandle(handle)
        print(f"regions={regions} scanned={scanned}")
        for name, count in counts.items():
            print(f"  {name}: {count}")
            for addr, window, mark in samples[name]:
                print(f"    @{addr:016x}  {hexdump(window)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
