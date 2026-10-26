#!/usr/bin/env python3
"""FEAS-008 Linux D-012 measurements: launch to presented content.

Each run spawns a fresh process with FEAS008_LAUNCH_NS set to CLOCK_MONOTONIC
just before spawn. The app reports the compositor presentation time of the
first frame containing document rows (GDK frame timings), which shares that
clock.

Cold runs evict the app's file set from the page cache with
posix_fadvise(DONTNEED): every file the running app maps or holds open, plus
GTK theme, font-cache and GPU shader-cache directories. Pages still mapped by
other processes cannot be evicted this way; residency after eviction is
recorded. `--drop-caches` instead uses `sudo -n` to drop all caches.
"""

import argparse
import glob
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / "spikes/feas008/target/release/feas008-native"
WEBVIEW = ROOT / "spikes/feas008/target/release/feas008-webview"
INPUTS = {
    "prose-1mib": ROOT / "build/fixtures/performance/prose-1mib.md",
    "prose-5mib": ROOT / "build/fixtures/performance/prose-5mib.md",
    "long-lines-1mib": ROOT / "build/fixtures/performance/long-lines-1mib.md",
}
HOME = Path.home()
EXTRA_GLOBS = [
    "/usr/share/themes/*/gtk-4.0/**",
    str(HOME / ".config/gtk-4.0/**"),
    "/usr/share/icons/*/icon-theme.cache",
    "/var/cache/fontconfig/*",
    str(HOME / ".cache/fontconfig/*"),
    str(HOME / ".cache/nvidia/**"),
    str(HOME / ".cache/mesa_shader_cache*/**"),
]


def spawn(binary: Path, document: Path, *args: str) -> subprocess.Popen:
    env = dict(os.environ)
    env["FEAS008_LAUNCH_NS"] = str(time.clock_gettime_ns(time.CLOCK_MONOTONIC))
    return subprocess.Popen(
        [str(binary), str(document), *args],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        env=env,
    )


def read_events(proc: subprocess.Popen, until: str, timeout: float = 120.0) -> dict:
    deadline = time.monotonic() + timeout
    for line in proc.stdout:
        if line.startswith("EVENT "):
            event = json.loads(line[6:])
            if event.get("event") == until:
                return event
        if time.monotonic() > deadline:
            break
    raise RuntimeError(f"no {until} event")


def report(proc: subprocess.Popen, timeout: float = 180.0) -> dict:
    try:
        out, _ = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        raise
    for line in out.splitlines():
        if line.startswith("REPORT "):
            return json.loads(line[7:])
    raise RuntimeError(f"no report (exit {proc.returncode})")


def file_set(binary: Path, document: Path) -> list[str]:
    """Files a running instance maps or holds open, plus resource directories."""
    proc = spawn(binary, document, "--control")
    read_events(proc, "ready")
    time.sleep(0.5)
    files = {str(binary), str(document)}
    for line in Path(f"/proc/{proc.pid}/maps").read_text().splitlines():
        parts = line.split(maxsplit=5)
        if len(parts) == 6 and parts[5].startswith("/"):
            files.add(parts[5])
    for fd in Path(f"/proc/{proc.pid}/fd").iterdir():
        try:
            target = os.readlink(fd)
        except OSError:
            continue
        if target.startswith("/"):
            files.add(target)
    proc.stdin.write("quit\n")
    proc.stdin.flush()
    report(proc)
    for pattern in EXTRA_GLOBS:
        files.update(glob.glob(pattern, recursive=True))
    return sorted(f for f in files if os.path.isfile(f) and not f.startswith(("/dev/", "/proc/", "/sys/", "/memfd:")))


def evict(files: list[str]) -> None:
    os.sync()
    for path in files:
        try:
            fd = os.open(path, os.O_RDONLY)
        except OSError:
            continue
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        finally:
            os.close(fd)


def residency(files: list[str]) -> dict:
    total = resident = 0
    for chunk in range(0, len(files), 200):
        out = subprocess.run(
            ["fincore", "--bytes", "--noheadings", "--raw", "--output", "RES,SIZE", *files[chunk:chunk + 200]],
            capture_output=True, text=True,
        ).stdout
        for line in out.splitlines():
            res, size = line.split()
            resident += int(res)
            total += int(size)
    return {"files": len(files), "bytes": total, "resident_bytes": resident}


def drop_caches() -> None:
    os.sync()
    subprocess.run(["sudo", "-n", "sh", "-c", "echo 3 > /proc/sys/vm/drop_caches"], check=True)


def summary(values: list[float]) -> dict:
    return {
        "runs": len(values),
        "median": round(statistics.median(values), 1),
        "min": round(min(values), 1),
        "max": round(max(values), 1),
        "stdev": round(statistics.stdev(values), 1) if len(values) > 1 else 0.0,
        "values": [round(v, 1) for v in values],
    }


def one_run(binary: Path, document: Path) -> dict:
    data = report(spawn(binary, document, "--automated"))
    keep = ["launch_to_presented_ms", "launch_to_paint_ms", "launch_to_main_ms", "parse_ms", "read_ms",
            "presentation_time_available", "peak_rss_kib"]
    run = {k: data.get(k) for k in keep}
    run["jumps_ms"] = [s["jump_to_presented_ms"] for s in data.get("samples", [])]
    copy = data.get("select_all_copy", {})
    run["checks_ok"] = all(s.get("displayed_equals_model") in (True, None) and s.get("copy_contains_at_offset", True)
                           for s in data.get("samples", [])) \
        and copy.get("markdown_equals_source", copy.get("contains_samples", False))
    return run


def measure(engine: str, binary: Path, runs: int, use_sudo: bool) -> dict:
    results = {}
    for name, document in INPUTS.items():
        files = [] if use_sudo else file_set(binary, document)
        cold, warm, cold_residency = [], [], []
        for _ in range(runs):
            if use_sudo:
                drop_caches()
            else:
                evict(files)
                cold_residency.append(residency(files))
            time.sleep(1.0)
            cold.append(one_run(binary, document))
        one_run(binary, document)  # warm-up, discarded
        for _ in range(runs):
            time.sleep(1.0)
            warm.append(one_run(binary, document))
        entry = {
            "cold": summary([r["launch_to_presented_ms"] for r in cold]),
            "warm": summary([r["launch_to_presented_ms"] for r in warm]),
            "warm_paint": summary([r["launch_to_paint_ms"] for r in warm]),
            "warm_parse_ms": summary([r["parse_ms"] for r in warm if r["parse_ms"] is not None]),
            "warm_peak_rss_mib": summary([r["peak_rss_kib"] / 1024 for r in warm if r["peak_rss_kib"]]),
            "warm_jump_ms": summary([j for r in warm for j in r["jumps_ms"]]) if warm[0]["jumps_ms"] else None,
            "presentation_time_runs": {
                "cold": [bool(r["presentation_time_available"]) for r in cold],
                "warm": [bool(r["presentation_time_available"]) for r in warm],
            },
            "checks_ok": all(r["checks_ok"] for r in cold + warm),
        }
        if cold_residency:
            entry["cold_eviction"] = {
                "method": "posix_fadvise DONTNEED on app file set",
                "files": cold_residency[0]["files"],
                "set_mib": round(cold_residency[0]["bytes"] / 2**20, 1),
                "resident_mib_after_evict": summary([c["resident_bytes"] / 2**20 for c in cold_residency]),
            }
        else:
            entry["cold_eviction"] = {"method": "sudo drop_caches 3"}
        results[name] = entry
        print(f"{engine} {name}: cold {entry['cold']['median']} warm {entry['warm']['median']} ms", file=sys.stderr)
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=["native", "webview"], default="native")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--drop-caches", action="store_true")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    binary = NATIVE if args.engine == "native" else WEBVIEW
    results = measure(args.engine, binary, args.runs, args.drop_caches)
    text = json.dumps({args.engine: results}, indent=2)
    if args.out:
        args.out.write_text(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
