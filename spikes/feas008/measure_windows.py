#!/usr/bin/env python3
"""FEAS-008 Windows D-012 measurements: launch to presented content.

Each run spawns a fresh process with FEAS008_LAUNCH_QPC set to
QueryPerformanceCounter just before CreateProcess. The host reports the QPC
time after the DWM present that follows the UI thread's commit of the first
frame containing laid-out document rows (DwmFlush on a worker), which shares
that clock. The WebView2 host uses the first frame after an injected double
requestAnimationFrame instead.

Cold runs empty the system file-cache working set and purge the standby list
(NtSetSystemInformation MemoryPurgeStandbyList), which needs an elevated
process. Without elevation, `--warm-only` skips them; nothing is called cold
that was not purged.
"""
from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import json
import os
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "build/feas008/windows"
ENGINES = {
    "csharp-jit": OUT / "csharp-jit/Feas008WinUI.exe",
    "csharp-aot": OUT / "csharp-aot/Feas008WinUIAot.exe",
    "cpp": OUT / "cpp/Feas008Cpp.exe",
    "webview": OUT / "webview/Feas008WebView.exe",
}
INPUTS = {
    "prose-1mib": ROOT / "build/fixtures/performance/prose-1mib.md",
    "prose-5mib": ROOT / "build/fixtures/performance/prose-5mib.md",
    "long-lines-1mib": ROOT / "build/fixtures/performance/long-lines-1mib.md",
}
WEBVIEW_DATA = ROOT / "build/feas008/webview-udf"

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll")
# Declare handle-sized signatures: the default int return truncates the
# 64-bit pseudo-handle from GetCurrentProcess.
kernel32.GetCurrentProcess.restype = wt.HANDLE
kernel32.CloseHandle.argtypes = [wt.HANDLE]
kernel32.SetSystemFileCacheSize.argtypes = [ctypes.c_size_t, ctypes.c_size_t, wt.DWORD]
kernel32.SetSystemFileCacheSize.restype = wt.BOOL
advapi32.OpenProcessToken.argtypes = [wt.HANDLE, wt.DWORD, ctypes.POINTER(wt.HANDLE)]
advapi32.OpenProcessToken.restype = wt.BOOL
advapi32.LookupPrivilegeValueW.argtypes = [wt.LPCWSTR, wt.LPCWSTR, ctypes.c_void_p]
advapi32.LookupPrivilegeValueW.restype = wt.BOOL
advapi32.AdjustTokenPrivileges.argtypes = [wt.HANDLE, wt.BOOL, ctypes.c_void_p, wt.DWORD, ctypes.c_void_p, ctypes.c_void_p]
advapi32.AdjustTokenPrivileges.restype = wt.BOOL
ntdll.NtSetSystemInformation.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
ntdll.NtSetSystemInformation.restype = ctypes.c_long


def qpc() -> int:
    value = ctypes.c_longlong()
    kernel32.QueryPerformanceCounter(ctypes.byref(value))
    return value.value


class LUID(ctypes.Structure):
    _fields_ = [("LowPart", wt.DWORD), ("HighPart", wt.LONG)]


class TOKEN_PRIVILEGES(ctypes.Structure):
    _fields_ = [("PrivilegeCount", wt.DWORD), ("Luid", LUID), ("Attributes", wt.DWORD)]


def enable(privilege: str) -> None:
    token = wt.HANDLE()
    if not advapi32.OpenProcessToken(kernel32.GetCurrentProcess(), 0x0020 | 0x0008, ctypes.byref(token)):
        raise OSError(ctypes.get_last_error(), "OpenProcessToken")
    luid = LUID()
    if not advapi32.LookupPrivilegeValueW(None, privilege, ctypes.byref(luid)):
        raise OSError(ctypes.get_last_error(), "LookupPrivilegeValue")
    state = TOKEN_PRIVILEGES(1, luid, 0x2)
    advapi32.AdjustTokenPrivileges(token, False, ctypes.byref(state), 0, None, None)
    error = ctypes.get_last_error()
    kernel32.CloseHandle(token)
    if error:  # ERROR_NOT_ALL_ASSIGNED: not elevated
        raise PermissionError(f"{privilege} unavailable (error {error}); run elevated or pass --warm-only")


def purge_caches() -> None:
    """Empty the system file-cache working set, then purge the standby list."""
    enable("SeIncreaseQuotaPrivilege")
    enable("SeProfileSingleProcessPrivilege")
    size = ctypes.c_size_t(-1)
    if not kernel32.SetSystemFileCacheSize(size, size, 0):
        raise OSError(ctypes.get_last_error(), "SetSystemFileCacheSize")
    for command in (3, 4):  # MemoryFlushModifiedList, MemoryPurgeStandbyList
        value = ctypes.c_int(command)
        status = ntdll.NtSetSystemInformation(80, ctypes.byref(value), ctypes.sizeof(value))
        if status != 0:
            raise OSError(f"NtSetSystemInformation({command}) failed: 0x{status & 0xffffffff:08x}")


def kill_webview_orphans() -> None:
    script = ("Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | "
              "Where-Object { $_.CommandLine -match 'Feas008WebView' } | "
              "ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }")
    subprocess.run(["powershell", "-NoProfile", "-Command", script], check=False, capture_output=True)


def one_run(engine: str, binary: Path, document: Path, timeout: float = 150.0) -> dict:
    with tempfile.TemporaryDirectory() as scratch:
        report = Path(scratch) / "report.json"
        env = dict(os.environ, WEBVIEW2_USER_DATA_FOLDER=str(WEBVIEW_DATA))
        env["FEAS008_LAUNCH_QPC"] = str(qpc())
        proc = subprocess.Popen([str(binary), str(document), "--automated", "--report", str(report)],
                                env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            proc.wait(timeout)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        if engine == "webview":
            kill_webview_orphans()
        if not report.exists():
            return {"error": f"no report (exit {proc.returncode})"}
        data = json.loads(report.read_text(encoding="utf-8"))
    run = {k: data.get(k) for k in ("launch_to_presented_ms", "launch_to_paint_ms", "launch_to_main_ms",
                                     "launch_to_window_loaded_ms", "parse_ms", "read_ms",
                                     "presentation_time_available", "launch_from_launcher", "realized_rows_at_launch",
                                     "presented_timeouts", "error")}
    memory = data.get("process_peak_rss_kib", {}).get("total") or data.get("peak_rss_kib")
    run["peak_rss_mib"] = memory / 1024 if memory else None
    run["jumps_ms"] = [s["jump_to_presented_ms"] for s in data.get("samples", [])]
    for key in ("find", "outline_last_jump"):
        if isinstance(data.get(key), dict) and "jump_to_presented_ms" in data[key]:
            run["jumps_ms"].append(data[key]["jump_to_presented_ms"])
    run["jumps_ms"] += [j["jump_to_presented_ms"] for j in data.get("outline_jumps", [])]
    copy = data.get("select_all_copy", {})
    if engine == "webview":
        run["checks_ok"] = bool(data.get("find", {}).get("found")) and not data.get("error")
    else:
        run["checks_ok"] = (not data.get("error") and not data.get("presented_timeouts")
                            and all(s["displayed_equals_model"] and s["copy_contains_at_offset"] for s in data.get("samples", []))
                            and copy.get("markdown_equals_source") and copy.get("local_readback_equals_model")
                            and copy.get("local_markdown_readback_equals_source")
                            and all(j["heading_presented"] and j["row_focused"] for j in data.get("outline_jumps", [])))
    return run


def summary(values: list[float]) -> dict:
    values = [v for v in values if v is not None]
    if not values:
        return {"runs": 0}
    return {
        "runs": len(values),
        "median": round(statistics.median(values), 1),
        "min": round(min(values), 1),
        "max": round(max(values), 1),
        "stdev": round(statistics.stdev(values), 1) if len(values) > 1 else 0.0,
        "values": [round(v, 1) for v in values],
    }


def measure(engine: str, runs: int, warm_only: bool, inputs: list[str]) -> dict:
    binary = ENGINES[engine]
    results = {}
    for name in inputs:
        document = INPUTS[name]
        cold, warm = [], []
        if not warm_only:
            for _ in range(runs):
                purge_caches()
                time.sleep(2.0)
                cold.append(one_run(engine, binary, document))
        one_run(engine, binary, document)  # warm-up, discarded
        for _ in range(runs):
            time.sleep(1.0)
            warm.append(one_run(engine, binary, document))
        entry = {
            "cold": summary([r.get("launch_to_presented_ms") for r in cold]) if cold else None,
            "warm": summary([r.get("launch_to_presented_ms") for r in warm]),
            "warm_paint": summary([r.get("launch_to_paint_ms") for r in warm]),
            "warm_main": summary([r.get("launch_to_main_ms") for r in warm]),
            "warm_window_loaded": summary([r.get("launch_to_window_loaded_ms") for r in warm]),
            "warm_parse_ms": summary([r.get("parse_ms") for r in warm]),
            "warm_peak_rss_mib": summary([r.get("peak_rss_mib") for r in warm]),
            "warm_jump_ms": summary([j for r in warm for j in r.get("jumps_ms", [])]),
            "realized_rows_at_launch": sorted({r.get("realized_rows_at_launch") for r in warm if r.get("realized_rows_at_launch") is not None}),
            "presentation_time_runs": {
                "cold": [bool(r.get("presentation_time_available")) for r in cold],
                "warm": [bool(r.get("presentation_time_available")) for r in warm],
            },
            "launch_from_launcher": all(r.get("launch_from_launcher") for r in cold + warm),
            "checks_ok": all(r.get("checks_ok") for r in cold + warm),
            "errors": [r["error"] for r in cold + warm if r.get("error")],
            "cold_method": None if warm_only else "SetSystemFileCacheSize(-1,-1) + MemoryFlushModifiedList + MemoryPurgeStandbyList before each run",
        }
        results[name] = entry
        cold_text = entry["cold"]["median"] if entry["cold"] else "-"
        print(f"{engine} {name}: cold {cold_text} warm {entry['warm'].get('median')} ms, checks {entry['checks_ok']}", file=sys.stderr, flush=True)
    return results


def main() -> int:
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=list(ENGINES), action="append")
    parser.add_argument("--input", choices=list(INPUTS), action="append")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--warm-only", action="store_true")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if not args.warm_only:
        purge_caches()  # fail early when not elevated
    kill_webview_orphans()
    engines = args.engine or list(ENGINES)
    inputs = args.input or list(INPUTS)
    results = {engine: measure(engine, args.runs, args.warm_only, inputs) for engine in engines}
    text = json.dumps(results, indent=2)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
