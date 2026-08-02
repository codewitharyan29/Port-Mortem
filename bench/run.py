#!/usr/bin/env python3
"""Benchmark: Rust natsort port vs Python natsort, following the Astral-track
north stars — documented speedup, honest p99 latency, and RSS (not just
hot-loop throughput).

Workload: N filename-like strings ("file{i}-v{i%37}.{i%5}.log"), matching a
realistic "sort a directory listing" use case rather than a synthetic
best-case string.

Usage: python3 bench.py [N] [runs]
"""
import gc
import os
import statistics
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # repo root (this file is one level deep)
_EXE = ".exe" if sys.platform == "win32" else ""
BIN = os.path.join(REPO, "target", "release", "natsort_port" + _EXE)
VENDOR = os.path.join(REPO, "vendor", "natsort-8.4.0")
if os.path.isdir(VENDOR):
    sys.path.insert(0, VENDOR)
from natsort import natsorted  # noqa: E402


def make_workload(n):
    return [f"file{i}-v{i % 37}.{i % 5}.log" for i in range(n)]


def percentile(values, p):
    s = sorted(values)
    k = (len(s) - 1) * p
    f, c = int(k), min(int(k) + 1, len(s) - 1)
    if f == c:
        return s[f]
    return s[f] + (s[c] - s[f]) * (k - f)


def bench_python(items, runs):
    times = []
    for _ in range(runs):
        gc.collect()
        t0 = time.perf_counter()
        natsorted(items)
        times.append((time.perf_counter() - t0) * 1e6)  # microseconds
    return times


def bench_rust(n, runs):
    times = []
    for _ in range(runs):
        out = subprocess.run([BIN, "bench", str(n)], capture_output=True, text=True, encoding="utf-8")
        times.append(int(out.stdout.strip()))  # already microseconds
    return times


def _win_process_memory_counters():
    """Shared ctypes plumbing for GetProcessMemoryInfo, with explicit
    argtypes/restype set. Without these, ctypes' default argument marshaling
    can silently mis-pass the HANDLE on 64-bit Windows, making the API call
    fail quietly and return all-zero counters instead of raising -- which is
    exactly what produced a bogus "0.0 MB" reading instead of an error.
    """
    import ctypes
    from ctypes import wintypes

    class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    psapi = ctypes.WinDLL("psapi.dll", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32.dll", use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE, ctypes.POINTER(PROCESS_MEMORY_COUNTERS), wintypes.DWORD,
    ]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
    kernel32.GetCurrentProcess.restype = wintypes.HANDLE
    return psapi, kernel32, PROCESS_MEMORY_COUNTERS


def peak_rss_mb(pid_self=True):
    """Peak resident set size in MB for the current process (Python side).

    `resource` is Unix-only (Linux/macOS); Windows has no equivalent module,
    so this queries the Win32 API directly via ctypes instead of requiring an
    extra pip install.
    """
    if sys.platform == "win32":
        import ctypes
        psapi, kernel32, PROCESS_MEMORY_COUNTERS = _win_process_memory_counters()
        counters = PROCESS_MEMORY_COUNTERS()
        counters.cb = ctypes.sizeof(PROCESS_MEMORY_COUNTERS)
        handle = kernel32.GetCurrentProcess()
        ok = psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb)
        if not ok:
            err = ctypes.get_last_error()
            print(f"  (warning: GetProcessMemoryInfo failed, error {err})", file=sys.stderr)
            return None
        return counters.PeakWorkingSetSize / (1024 * 1024)
    import resource
    kb = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    # Linux reports KB; macOS reports bytes.
    return kb / 1024 if sys.platform != "darwin" else kb / (1024 * 1024)


def _rust_peak_rss_windows(n):
    """Peak working-set size of the Rust binary on Windows, via ctypes.

    Launches the process and polls GetProcessMemoryInfo while it runs (no
    /usr/bin/time equivalent exists on Windows). Uses explicit argtypes/restype
    on every Win32 call -- without them, ctypes' default argument marshaling
    can silently mis-pass 64-bit HANDLE values (OpenProcess's return value in
    particular), making calls fail quietly rather than raising.
    """
    import ctypes
    from ctypes import wintypes

    psapi, kernel32, PROCESS_MEMORY_COUNTERS = _win_process_memory_counters()
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL

    PROCESS_QUERY_INFORMATION = 0x0400
    PROCESS_VM_READ = 0x0010
    proc = subprocess.Popen([BIN, "bench", str(n)], stdout=subprocess.PIPE)
    handle = kernel32.OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, False, proc.pid)
    if not handle:
        proc.wait()
        print(f"  (warning: OpenProcess failed, error {ctypes.get_last_error()})", file=sys.stderr)
        return None

    peak = 0
    counters = PROCESS_MEMORY_COUNTERS()
    counters.cb = ctypes.sizeof(PROCESS_MEMORY_COUNTERS)
    while proc.poll() is None:
        if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            peak = max(peak, counters.PeakWorkingSetSize)
    # one final read after exit in case the loop missed the true peak
    if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
        peak = max(peak, counters.PeakWorkingSetSize)
    kernel32.CloseHandle(handle)
    proc.wait()
    return peak / (1024 * 1024) if peak else None


def rust_peak_rss_mb(n):
    """Peak RSS of the Rust binary for one bench run."""
    if sys.platform == "win32":
        try:
            return _rust_peak_rss_windows(n)
        except Exception:
            return None
    try:
        r = subprocess.run(
            ["/usr/bin/time", "-v", BIN, "bench", str(n)],
            capture_output=True, text=True,
        )
        for line in r.stderr.splitlines():
            if "Maximum resident set size" in line:
                return int(line.split(":")[1].strip()) / 1024  # KB -> MB
    except FileNotFoundError:
        pass
    return None


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 50_000
    runs = int(sys.argv[2]) if len(sys.argv) > 2 else 15

    print(f"# Benchmark: natsort-rust vs Python natsort 8.4.0")
    print(f"# workload: {n} filename-like strings | {runs} runs (+2 warmup, discarded)")
    print()

    items = make_workload(n)

    # warmup (not counted)
    bench_python(items, 2)
    bench_rust(n, 2)

    py_times = bench_python(items, runs)
    rs_times = bench_rust(n, runs)

    def report(name, times):
        print(f"{name:8} mean={statistics.mean(times):9.1f}us  "
              f"p50={percentile(times,0.50):9.1f}us  "
              f"p95={percentile(times,0.95):9.1f}us  "
              f"p99={percentile(times,0.99):9.1f}us")

    report("Python", py_times)
    report("Rust", rs_times)

    speedup = statistics.mean(py_times) / statistics.mean(rs_times)
    speedup_p99 = percentile(py_times, 0.99) / percentile(rs_times, 0.99)
    print(f"\nSpeedup (mean):  {speedup:.2f}x")
    print(f"Speedup (p99):   {speedup_p99:.2f}x")

    rss = rust_peak_rss_mb(n)
    py_rss = peak_rss_mb()
    print(f"\nRust peak RSS:   {rss:.1f} MB" if rss else "\nRust peak RSS:   (unavailable -- /usr/bin/time not found)")
    print(f"Python peak RSS: {py_rss:.1f} MB  (interpreter + this process, not isolated)")

    print(f"\nNote: this benchmarks natsort's SORTING step only (both sides call")
    print(f"natsorted-equivalent logic on identical input); it does not include")
    print(f"process startup, which favors Python here since Rust pays a fresh")
    print(f"process-spawn cost per run via subprocess. A single long-running")
    print(f"Rust process (as a library) would not pay this cost repeatedly.")


if __name__ == "__main__":
    main()
